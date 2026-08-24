# 连接导入导出与 Standalone TLS 最终验证报告

日期：2026-08-24

## 交付范围

- 连接 profile 增加 Standalone TLS、服务端证书校验、CA/客户端证书名称和安全材料状态。
- 密码、CA PEM、客户端证书和私钥统一通过结构化 secret store 保存；旧版纯密码钥匙串值可继续读取。
- `test_connection`、`open_connection`、切换数据库以及由 active client 派生的 Pub/Sub/Profiler 路径统一使用 TLS client builder。
- TLS 使用 `rediss://`；支持自定义 CA、客户端证书/私钥和显式关闭服务端校验的开发场景。Rustls SNI 使用连接 host。
- 连接导出采用 v1 文档格式，只导出可迁移元数据和 TLS 配置，不读取或序列化任何敏感材料。
- 连接导入支持顶层数组、`connections`/`profiles` 容器及 RedisInsight 常见字段别名；采用追加模式、每条生成新 ID、逐条报告失败，并忽略敏感字段。
- 连接页提供 JSON 文件导入/Blob 导出、10 MiB 文件上限、部分成功反馈和 TLS/证书状态展示。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 通过：60 lib、5 commands、7 database analysis、21 domain、15 persistence；Redis integration 5 项按环境保持 ignored；doc tests 0 项 |
| `npm run test:frontend` | 通过：14 个测试文件、124 个测试 |
| `npm run build` | 通过：TypeScript/Vite 生产构建完成 |
| `npm run check:non-cloud` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 |
| `git diff --check` | 通过 |
| 敏感字段静态检查 | 通过；导出协议只在负向测试和忽略字段列表中出现敏感字段名，不构造敏感输出 |

测试输出中仍有既有 persistence 测试辅助函数的 `dead_code` 警告，以及 jsdom 对下载锚点点击的 `Not implemented: navigation` 提示；均未导致测试失败。

## 环境门槛与已知限制

- 当前环境未设置 `REDIX_TEST_REDIS_URL`、`REDIX_TEST_REDIS_TLS_URL` 或证书材料变量，因此没有声称真实 Redis/TLS 网络集成通过；真实 Redis 流程仍由现有 ignored 集成测试显式控制。
- TLS client builder 已通过无网络构造测试覆盖 rediss、校验开关、CA、mTLS 和不完整材料拒绝路径。
- 当前版本使用连接 host 作为 TLS SNI，不提供独立 `tlsServername` 字段。
- 证书正文使用表单 PEM 录入，不支持证书文件路径；导入文件中的密码或证书正文只计数并提示用户在本机重新录入。

## 提交

- `a460adc`：设计连接导入导出与 Standalone TLS
- `4d76b34`：扩展连接 TLS 模型并迁移结构化钥匙串
- `30de509`：接入 Standalone Rustls TLS 连接
- `1ff8f79`：增加连接导入导出领域协议
- `1c37416`：接入连接导入导出 Tauri 命令
- `bb5dc1d`：完善连接 TLS 证书表单与 typed bridge
- `faf635d`：增加连接配置导入导出界面
