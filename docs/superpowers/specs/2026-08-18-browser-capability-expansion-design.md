# Browser 能力扩展设计

日期：2026-08-18

## 背景

当前 Redix 已经有 Standalone 连接、SCAN 键列表、String/Hash/List/Set/Sorted Set 五类数据的基础编辑、TTL 和 Workbench。参考 RedisInsight 的 Browser 页面和 API 模块后，首批需要补齐最常用的本地管理能力，同时把 Streams 与 RedisJSON 纳入同一套类型化数据边界。

本设计只针对本地 Redis Standalone，不复制 RedisInsight 的 Electron/NestJS 实现，也不引入 Redis Cloud、Azure、AI、Telemetry 或远程插件能力。

## 目标与非目标

### 目标

- 在现有 Browser 中新增键、重命名键、勾选多个键并批量删除。
- 提供显式刷新入口，保留当前 pattern 和游标安全，刷新后不使用 KEYS。
- 为键详情补充可获取的元数据：类型、TTL、逻辑大小，以及可选的内存占用/编码/空闲时间。
- 支持 Redis Stream 的读取、编辑、保存和删除；本批次覆盖 entry，不覆盖 consumer group 管理和实时消费。
- 支持 RedisJSON 根文档的读取、格式化编辑、保存和删除；本批次不覆盖 JSONPath 子路径树形编辑。
- 对未安装 RedisJSON 的实例返回稳定的“不支持”错误，不泄露底层 Redis 错误文本。
- 保持现有五类数据的行为和接口兼容，所有新增 IPC 命令使用 snake_case 参数。

### 非目标

- Cluster、Sentinel、TLS、SSH、Vector Set、Redis Array、RedisSearch、Query Library。
- Stream consumer group、Pub/Sub、Monitor、Profiler、Slow Log 等需要长连接或后台任务的功能。
- 云账户、云数据库发现、云 API、Azure 集成、AI/Copilot、Telemetry 和远程插件市场。
- SQL 数据库和循环内 SQL 查询；本批次只使用 Redis 命令、JSON 文件和系统钥匙串。

## 用户流程

### Browser 列表

Browser 左侧继续使用 SCAN cursor MATCH pattern COUNT count。每行增加选择框，顶部显示已选择数量和批量删除按钮；刷新按钮从 cursor 0 开始重新扫描当前 pattern。刷新、过滤和连接切换都会清理已选择键，避免对已经不在当前列表中的键执行批量操作。

### 新增键

Browser 提供“新增键”面板，要求键名和数据类型。String、Hash、List、Set、Sorted Set 复用现有编辑器数据形态；Stream 使用 entry ID 与 field/value 行；JSON 使用 JSON 文本。提交调用 create_key，后端先以 EXISTS 防止覆盖已有键，再写入值并可选设置 TTL，成功后重新扫描并选中/读取新键。

### 键详情

详情头部提供刷新、重命名和删除动作。重命名使用 RENAMENX，目标键已存在时失败，不覆盖现有数据；成功后前端以新键名更新列表和详情身份。元数据读取失败时不阻塞基础值读取，缺失的可选字段显示为“—”。

### Stream

Stream 编辑器显示有限数量的 entry，每个 entry 由 ID 和 field/value 行组成。保存时用显式 ID 重建当前值，至少保留一条 entry；删除 entry 或删除整个键分别对应重建和 DEL。本批次限制单次读取/保存为 500 条 entry，避免一次 IPC 搬运无限数据。

### JSON

JSON 编辑器以格式化文本显示根文档。保存前在前端解析 JSON，后端再次使用 serde_json 序列化并执行 JSON.SET key . value。Redis 实例没有 RedisJSON 命令时返回稳定的 UNSUPPORTED_DATA_TYPE，而不是把未知命令文本展示给用户。

## 架构

### Rust 领域与 Redis 服务

扩展 RedisValue 为 Stream 和 Json，新增 StreamEntry/StreamField，并新增 CreateKeyInput、RenameKeyInput、DeleteKeysInput、KeyInfoInput 和 KeyInfo DTO。RedisOperations 增加创建、重命名、批量删除和元数据查询方法；现有 get_key/set_key 根据 TYPE 分派五类基础类型、Stream 和 RedisJSON。

scan_keys 继续按游标工作。key_size 对 Stream 使用 XLEN，对 JSON 返回可选值；未知模块类型只能返回 size: null，不能因为列表中有一个未知类型而让整页扫描失败。批量删除使用一个 DEL 命令携带多个 key 参数，不在 Rust 中循环查询 Redis。

可选元数据使用 MEMORY USAGE、OBJECT ENCODING 和 OBJECT IDLETIME；任何单项不支持或失败都降级为 None，只有核心 TYPE/PTTL/基础 size 失败才返回命令错误。所有 Redis 错误继续映射到固定 AppError，不携带 URI、密码或底层错误文本。

### Tauri IPC

新增命令：

~~~text
create_key(input: CreateKeyInput) -> KeyValue
rename_key(input: RenameKeyInput) -> KeyValue
delete_keys(input: DeleteKeysInput) -> u64
get_key_info(input: KeyInfoInput) -> KeyInfo
~~~

已有 scan_keys、get_key、set_key、delete_key、set_key_ttl 保持兼容。前端 bridge 只负责 typed wrapper 和安全错误归一化，不在 React 组件中拼接 Redis 命令。

### React UI

BrowserPage 继续作为请求生命周期边界，维护 selected keys、刷新请求序号和新建面板状态。KeyList 负责选择框、全选/取消全选、批量操作和刷新入口；新建键放在独立 AddKey 组件中；KeyDetails 负责重命名、元数据刷新和详情身份切换；KeyEditor 扩展 Stream/JSON 编辑器。

为避免旧详情异步响应覆盖新键，重命名后沿用当前 connection/key token 规则；批量删除完成后按后端返回的删除数量移除列表中的对应项，并清除详情。所有错误只使用固定 code 到中文文案的映射。

## 错误与边界

- 空键名、新键名为空、重复目标键、空集合、无效 TTL、非法 JSON、非法 Stream entry 在前端和 Rust 输入校验中均拒绝。
- RENAMENX 返回 0、DEL 返回 0、读取不存在键统一映射为稳定命令/键错误，不显示原始 Redis 响应。
- JSON 命令不存在、Stream 命令不支持或数据类型与 DTO 不匹配映射为 UNSUPPORTED_DATA_TYPE。
- Stream 读取和保存最多 500 条 entry；超过上限时只处理当前窗口内容，并在前端显示数量限制提示。
- 所有新增异步回调必须在组件卸载、连接切换和详情键切换后丢弃过期结果。

## 测试与验收

- Rust 领域测试覆盖新增 DTO 校验、JSON/Stream 编解码和未知模块错误映射。
- Rust 命令/Redis 集成测试覆盖创建、重命名、批量删除、Stream、JSON（RedisJSON 可用时）和元数据；普通测试默认不依赖 Redis，集成测试继续显式 ignored。
- 前端 bridge 测试覆盖四个新增命令的命名和 payload。
- React Testing Library 覆盖刷新、选择/批量删除、新增键、重命名、Stream/JSON 编辑校验及过期请求保护。
- 保留现有前端测试、Rust 测试、npm run check:non-cloud、npm run build、cargo fmt --check 和 git diff --check。
- 设置 REDIX_TEST_REDIS_URL 时执行真实 Standalone Redis 流程；未安装 RedisJSON 时，JSON 测试必须验证稳定的 unsupported 结果，而不是失败整个测试套件。

## 范围检查

本设计只改变 Browser 及其共享 Redis 数据模型，不改变当前连接配置、Workbench 和视觉壳层。所有实现文件都位于 src/、src-tauri/src/、对应测试目录和本地文档；不添加 Cloud/Azure/AI/Telemetry 入口或第三方网络依赖。
