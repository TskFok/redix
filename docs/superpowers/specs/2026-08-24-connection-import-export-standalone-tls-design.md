# 连接配置导入导出与 Standalone TLS/证书设计

日期：2026-08-24  
状态：已获用户确认，待编写实现计划

## 1. 目标与范围

参照 `/Users/ushopal/workspace/myself/RedisInsight` 的连接导入导出、证书录入和 Standalone TLS 能力，在当前 Redix 的 Rust + Tauri + React 架构中补齐：

- Standalone Redis TCP/TLS 连接配置；
- CA 证书、客户端证书和客户端私钥的录入、校验、安全保存及连接使用；
- 连接配置的版本化 JSON 导入和导出；
- 导入结果的成功、部分成功和失败反馈。

本批次明确不实现 SSH 隧道、Sentinel、Cluster、Redis Cloud、证书文件路径读取、独立证书管理器或 SQL 功能。

## 2. 已确认的安全决策

普通连接导出是无 secrets 导出：

- 不导出密码；
- 不导出 CA PEM 正文；
- 不导出客户端证书正文；
- 不导出客户端私钥；
- 可以导出非敏感的证书名称提示，帮助用户知道导入后需要重新录入哪些材料。

导入同样不把文件中的密码、证书正文或私钥写入本地。即使输入文件来自 RedisInsight 并携带这些字段，也只统计为被忽略的敏感字段，导入后的连接需要在本机重新录入。

## 3. 方案选择

### 3.1 推荐方案：扩展现有 profile + 钥匙串 secrets

连接 profile JSON 只保存连接元数据、TLS 开关和证书名称；系统钥匙串保存密码、CA PEM、客户端证书 PEM 和私钥。TLS client 在 Redis service 内集中构造，所有连接路径共用该 helper。

前端使用原生文件 input 读取 JSON 和 Blob 下载导出文件，不新增任意路径读取权限。证书通过表单粘贴 PEM，避免证书路径在另一台机器上失效，也避免扩大 Tauri 文件权限。

### 3.2 未采用：把证书正文写进 profile JSON 或应用私有文件

该方案会让备份、同步和误上传更容易泄露私钥；应用私有文件还需要额外权限、加密和迁移协议，超出当前 Redix 的安全存储边界。

### 3.3 未采用：完整复制 RedisInsight 的证书实体/独立 SNI 管理

RedisInsight 的证书实体、证书 ID 引用和独立 `tlsServername` 需要更复杂的证书仓储及底层 TLS 连接定制。当前 `redis` crate 的 Rustls client 直接以连接 host 作为 ServerName/SNI，没有稳定的独立 SNI 调用入口。本批次使用 host 作为 SNI，并在 UI 和文档中明确限制；后续若升级客户端能力，再单独设计独立 SNI。

## 4. 领域模型与安全存储

### 4.1 ConnectionProfile

在现有字段基础上增加：

```text
tls: bool
verify_server_cert: bool
ca_certificate_name: string | null
client_certificate_name: string | null
has_ca_certificate: bool
has_client_certificate: bool
```

`has_ca_certificate` 和 `has_client_certificate` 表示当前设备钥匙串中是否存在有效材料，不表示导出文件中携带了材料。新增字段使用 serde 默认值，兼容已有 profile JSON。

`has_client_certificate` 只有客户端证书和客户端私钥同时存在且通过 PEM 校验时才为 true。

### 4.2 ConnectionSecrets

钥匙串中的单条 credential 从纯密码迁移为版本化 JSON：

```text
{
  "password": "...",
  "ca_certificate": "-----BEGIN CERTIFICATE-----...",
  "client_certificate": "-----BEGIN CERTIFICATE-----...",
  "client_key": "-----BEGIN PRIVATE KEY-----..."
}
```

字段均为可选。读取时兼容历史上直接保存的纯密码字符串，按 `password` 字段解释；下一次保存时升级为结构化格式。所有空字段不落 profile JSON，也不出现在 IPC 返回值中。

### 4.3 保存与编辑语义

`SaveConnectionInput` 增加可选证书正文和显式清除标记：

- `null` 的密码/证书字段表示编辑时保留已有材料；新增连接则表示没有材料；
- `clear_ca_certificate` 清除 CA 及其名称；
- `clear_client_certificate` 同时清除客户端证书、私钥及其名称；
- 新录入客户端材料时证书和私钥必须成对提供；只输入一项返回 `INVALID_INPUT`；
- 新录入证书正文时必须有对应名称；
- 关闭 TLS 不自动删除已保存的材料，重新开启 TLS 可以继续使用。

profile 与钥匙串仍采用现有保存事务：先计算新的完整 secrets，再更新钥匙串和 profile；任一步失败都恢复旧 profile 和旧 secrets。

### 4.4 PEM 校验

前端提供即时提示，Rust 在保存和测试前执行最终校验：

- CA 和客户端证书必须包含 `BEGIN CERTIFICATE` / `END CERTIFICATE` 块；
- 客户端私钥接受 PKCS#8、RSA PKCS#1 和 EC 私钥常见 PEM 标记；
- PEM 只做格式边界校验，真正的证书链、域名和私钥匹配由 Rustls/Redis 连接测试确认；
- `verify_server_cert=false` 明确表示关闭 TLS 证书校验，仍保留为显式配置，不因导入或默认值隐式关闭。

## 5. TLS 连接构造

启用 `redis` crate 的 `tokio-rustls-comp` 及其不安全校验开关，在 `RedisService` 中增加唯一的 client builder：

- `tls=false` 使用现有 `redis://` 路径；
- `tls=true` 使用 `rediss://` 和 `TlsCertificates`；
- CA PEM 作为自定义 root certificate，没有 CA 时使用 Rustls/system roots；
- 客户端证书和私钥成对转换为 `ClientTlsConfig`；
- `verify_server_cert=false` 使用 crate 支持的显式 insecure URL 标记，且只作用于该连接；
- SNI 使用 `profile.host`，本批次不暴露独立 `tls_servername` 字段。

以下路径必须调用同一 builder，不能保留独立的 `Client::open` 分支：

- 测试连接；
- 打开连接；
- 切换数据库；
- 普通命令、键操作、Database Analysis、Instance Details；
- Pub/Sub 和 Profiler 派生连接。

这样可以确保 mTLS 和自定义 CA 不会只在主连接路径生效、在辅助功能中失效。

## 6. 导入导出协议

### 6.1 Redix v1 导出格式

```json
{
  "version": 1,
  "connections": [
    {
      "name": "本地 Redis",
      "host": "127.0.0.1",
      "port": 6379,
      "username": null,
      "database": 0,
      "tls": false,
      "verify_server_cert": true,
      "ca_certificate_name": null,
      "client_certificate_name": null
    }
  ]
}
```

导出不包含 profile id、`has_password`、`has_ca_certificate`、`has_client_certificate` 和任何 secrets。导出全部当前 profile；本批次不增加选择器或 secrets 开关。

### 6.2 导入兼容与归一化

Rust 导入 command 接收原始 JSON 字符串，支持：

- Redix v1 的 `{ version, connections }`；
- `{ profiles: [...] }`；
- RedisInsight 常见的连接数组及其 `name/connectionName`、`db/database`、`tls/ssl`、`verifyServerCert`、`username/authUser` 等字段别名。

只接受 Standalone 连接。明确标记为 SSH、Sentinel、Cluster、Cloud 或带拓扑节点的条目逐条失败，并不会被降级成普通 Standalone 连接。

导入采用追加模式：每条有效记录生成新的 UUID，忽略外部 id，避免覆盖当前连接和错配钥匙串 secrets。名称可重复，但每条记录仍独立报告结果。

每条记录先独立校验，所有有效记录在一次 profile repository save 中追加；若本地持久化失败，则整批有效记录都不落盘。返回：

```text
imported: ConnectionProfile[]
failed: [{ index, name, code, message }]
ignored_secret_fields: number
```

JSON 解析失败或顶层格式不支持返回 `INVALID_INPUT`；部分条目无效时保留有效条目并由 UI 显示部分成功。

### 6.3 前端交互

连接管理页增加：

- “导入连接”：选择 JSON 文件，限制 10 MB，调用 typed IPC，展示导入数量、失败条目和被忽略的敏感字段提示；
- “导出连接”：调用 typed IPC 下载 `redix-connections.json`；
- 连接卡片展示 TLS 状态，以及证书已配置/需重新录入的状态。

连接表单增加 TLS 区块：TLS 开关、验证服务端证书、CA 名称/PEM、客户端证书名称/PEM、客户端私钥 PEM，以及清除已有材料的操作。证书文本只存在于表单状态和 IPC 入参，不回填钥匙串正文。

## 7. IPC 与错误契约

新增 typed command：

- `export_connections -> ConnectionExportDocument`；
- `import_connections(input: ImportConnectionsInput) -> ImportConnectionsResult`。

扩展现有 `save_connection`/`test_connection` 输入，但不改变现有错误序列化格式。新增 PEM、导入格式和不支持拓扑都映射为已有固定错误码 `INVALID_INPUT` 或 `INVALID_CONNECTION`，不把底层证书/钥匙串文本直接返回给前端。

## 8. 测试与验收

### Rust

- profile 新旧 JSON 兼容、TLS 字段校验和 PEM marker 校验；
- structured secrets 的读写、历史纯密码迁移、清除和空值行为；
- save/delete 失败时 profile + secrets 回滚；
- 导入格式归一化、别名、拓扑拒绝、重复 ID、部分成功、敏感字段忽略和持久化失败回滚；
- 导出 JSON 不出现 password、CA/client PEM 或私钥正文；
- TLS URL/client builder 覆盖普通 TLS、自定义 CA、mTLS、关闭校验和无 TLS；
- 现有连接管理和 Redis service 回归测试。

### 前端

- TLS 表单默认值、PEM 输入校验、编辑时留空保留、清除材料和保存入参；
- 连接页导入/导出按钮、10 MB 限制、下载内容、安全字段不出现在下载 JSON；
- 成功/部分成功/失败反馈和刷新 profile 列表；
- 现有连接新增、编辑、测试、打开、删除回归。

### 集成

保留真实 TLS Redis 流程为环境变量 gated 的 ignored 测试；未配置证书/Redis TLS 环境时不宣称真实 TLS 已验证。完成前运行 Rust、前端、构建、非 Cloud 范围、格式和 diff 检查。

## 9. 完成标准

用户可以在 Standalone 连接表单中配置并使用 TLS、CA 和 mTLS；可以导出当前连接并在另一台设备导入；导出文件不含任何密码或证书私密正文；导入不会覆盖已有连接或写入外部 secrets；现有普通 Redis、Browser、Workbench、分析、监控、Pub/Sub 和 Profiler 流程不回归。
