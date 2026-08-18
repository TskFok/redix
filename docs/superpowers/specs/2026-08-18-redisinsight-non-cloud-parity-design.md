# RedisInsight 非 Cloud 功能差异补全设计

日期：2026-08-18

## 1. 背景

当前 Redix 已经完成本地 Redis Standalone 的连接管理、Browser 基础键浏览与编辑、Stream/RedisJSON 首批能力、键新增/重命名/批量删除/元数据刷新，以及 Workbench 单条命令执行。参考 `/Users/ushopal/workspace/myself/RedisInsight` 当前版本后，仍有 Browser 生产力、Workbench/CLI、实例与数据库概览、本地资源管理、运维观察和高级连接能力等差距。

RedisInsight 同时包含 Redis Cloud、Azure、AI、Telemetry、远程插件和 RDI 等不属于本项目目标的能力。本设计只补齐本地 Redis 可交付能力，继续保持 Rust + Tauri + React 的轻量架构，不复制 Electron、NestJS、Redux 或 Monaco 插件运行时。

## 2. 目标与非目标

### 2.1 本阶段目标

本阶段按以下顺序补齐第一批高收益本地能力：

1. Browser 生产力：键名搜索、类型过滤、刷新、更丰富的元数据，以及当前选中键的本地导入/导出边界。
2. Workbench 增强：内置命令目录和参数提示、多命令顺序执行、raw/text/JSON 结果展示、复制结果、按连接持久化非敏感历史。
3. 数据库与实例概览：服务器信息、版本、内存、客户端数、数据库大小、数据库切换和基础只读统计。
4. Query Library 与设置：版本化 JSON 存储的命令收藏、搜索、编辑、删除、回填和本地偏好设置。

后续批次保留以下顺序：

- Slow Log、Pub/Sub、Profiler 等本地运维观察能力。
- TLS、证书、SSH、Sentinel、Cluster 等连接拓扑与安全能力。
- Vector Set、Array、Search/Query 索引及复杂结果可视化等 Redis 模块能力。

### 2.2 非目标

- Redis Cloud 登录、账户、组织、订阅、云数据库发现、云 API、云 SDK 和云端点。
- Azure Managed Redis、云 OAuth、AI/Copilot、Telemetry/Analytics、RDI 和远程插件市场。
- 本阶段不引入 Monaco、插件可视化运行时或本地 HTTP 服务。
- 不引入 SQL 存储，也不在循环遍历中执行 SQL 查询；本地资源使用 JSON 文件和系统钥匙串。
- 本阶段不实现 Pub/Sub、Profiler、Slow Log 的长连接流程，不把 TLS/SSH/拓扑重构混入第一批。

## 3. 方案比较

| 方案 | 优点 | 代价 | 结论 |
|------|------|------|------|
| 继续使用 React/Vite + Tauri typed IPC + Rust domain/service | 延续现有代码；依赖少；Redis 访问和安全边界集中在 Rust；每批可独立测试 | IPC DTO 与命令数量逐步增加 | 采用 |
| 以 Workbench 命令执行为中心，Browser 多数能力通过 Redis 命令拼装 | 初期实现快；复用单一命令入口 | 类型安全、错误映射、权限边界和用户体验较弱 | 不采用 |
| 增加本地 Axum HTTP 服务 | API 边界清晰，便于独立调试 | 引入端口、服务生命周期和本地网络安全复杂度 | 不采用 |

## 4. 总体架构

```text
React/Vite 页面
    │ typed Tauri wrapper
    ▼
Tauri commands
    │ 输入校验 / DTO / 固定错误
    ▼
Rust application state
    ├── connection manager
    ├── Redis service
    ├── JSON persistence
    └── keyring adapter
            │ async Redis protocol
            ▼
        本地 Redis Standalone
```

- `src/features/*` 负责页面、交互和局部状态；不在 React 组件中拼接 Redis 命令。
- `src/lib/tauri.ts` 继续提供 typed wrapper 和错误归一化。
- `src-tauri/src/domain` 保存可序列化 DTO、输入模型、命令结果和本地资源模型。
- `src-tauri/src/commands` 只负责边界校验、调用 service 和返回安全 DTO。
- `src-tauri/src/redis` 负责连接复用、能力探测、数据查询和错误映射。
- `src-tauri/src/persistence` 负责版本化 JSON 文件、原子写入和系统钥匙串。
- Rust 按 `connections / browser / database / workbench / settings / observability` 组织命令边界；本阶段 observability 只预留接口，不实现长连接功能。

统一数据流：

```text
React 页面
  → typed Tauri wrapper
  → Tauri command
  → Rust domain/service
  → 当前 Redis 连接或本地 JSON/钥匙串
  → 固定错误码 / typed DTO
  → React 状态、反馈和视图
```

## 5. 第一批功能设计

### 5.1 Browser 生产力

Browser 继续以 `SCAN cursor MATCH pattern COUNT count` 为唯一键列表分页基础，不使用阻塞式 `KEYS`。前端维护 pattern、游标、类型筛选、加载状态、选择项和请求序号。类型过滤在增量扫描过程中按每个 `KeySummary.key_type` 过滤当前页，游标仍然使用 Redis 返回的 SCAN 游标，不为了补满过滤页而做全量枚举。

新增能力：

- 按键名 pattern 搜索，保留当前分页和空状态反馈。
- 按 Redis 类型过滤；类型过滤不改变后端的增量扫描边界。
- 显式刷新，从游标 0 重新扫描当前 pattern，并清空过期选择。
- 列表显示类型、TTL、逻辑大小以及可用时的内存占用、编码和空闲时间。
- 当前选中键的本地导出，以及受校验限制的本地导入；文件内容只经过 typed DTO 进入 Rust，不执行任意脚本。

导入导出首批只覆盖已有 String、Hash、List、Set、Sorted Set、Stream、JSON 数据模型。未知模块类型导出为不可编辑的摘要或被稳定拒绝，不阻塞其他键。导入默认禁止覆盖已有键，用户必须显式删除或重命名后再导入。文件选择使用浏览器原生 file input，导出使用 Blob 下载，不新增 Tauri 文件对话框插件；导入文件先在前端解析为受支持的 JSON DTO，再通过一次 typed IPC 交给 Rust 校验和写入 Redis。

### 5.2 Workbench 增强

Workbench 增加独立的命令目录服务和结果视图模型：

- 命令目录以本地静态资源或 Rust 内置数据提供，不依赖云端请求。
- 输入命令时按命令名和参数位置提供轻量提示；不引入 Monaco。
- 支持多条命令顺序执行，每条命令返回自己的状态、结果或错误，并记录失败位置。
- 默认遇到错误后停止后续命令；用户可切换为继续执行模式。
- 结果提供 raw、text、JSON 三种查看方式，长结果可折叠并支持复制。
- 历史按连接 ID 隔离，保存前过滤认证、ACL 和敏感配置类命令；失败历史也不保存密码或原始底层错误。

多命令执行仍通过一次 typed IPC 请求传递命令列表，由 Rust 顺序执行并返回结果数组；不在前端循环调用单命令 IPC，避免连接状态和取消逻辑分散。

### 5.3 数据库与实例概览

新增只读聚合查询，返回：

- Redis 版本、运行模式和基本服务器信息。
- 内存使用/峰值、连接客户端数等可选指标。
- 当前数据库键数量、过期键数量和数据库大小等可用指标。
- 当前连接数据库编号和可切换数据库列表。

后端集中执行和转换信息，前端只接收 `InstanceOverview` 与 `DatabaseOverview` DTO。单个指标不可用时返回 `null` 和可读状态，不影响其他指标和 Browser。复杂数据库分析扫描和推荐引擎后置，不在第一批自动扫描全库。

数据库切换只作用于当前连接会话，并同步保存 profile 的 `database` 字段；切换成功前不改变前端 active database，失败时恢复原状态。

### 5.4 Query Library 与设置

Query Library 使用独立的版本化 JSON 文件保存：

```text
QueryLibraryFile
├── version: number
└── items: QueryLibraryItem[]
    ├── id
    ├── name
    ├── command
    ├── tags[]
    └── updated_at
```

支持新增、编辑、删除、搜索和回填 Workbench。用户主动保存的查询可以包含普通业务命令，但认证和敏感命令不允许保存。

设置首批覆盖主题、结果展示模式、默认扫描数量和多命令错误策略。设置文件同样有版本号；读取失败时保留原文件、记录安全的本地诊断并恢复默认值。

所有 JSON 文件写入使用临时文件、刷新/关闭文件句柄和原子替换；不使用数据库表，不执行 SQL。

## 6. 领域模型与 IPC 边界

前端新增或扩展以下 typed 模型：

- `BrowserFilter`、`KeyMetadata`、`ExportedKey`、`ImportKeysInput`。
- `CommandDefinition`、`ExecuteCommandsInput`、`CommandExecutionItem`、`CommandHistoryEntry`。
- `InstanceOverview`、`DatabaseOverview`、`SelectDatabaseInput`。
- `QueryLibraryItem`、`QueryLibraryFile`、`AppSettings`。

第一批命令按以下职责扩展，现有命令保持兼容：

```text
browser_search / scan_keys
get_key_info
export_keys
import_keys
get_command_catalog
execute_commands
list_command_history
save_command_history
get_instance_overview
get_database_overview
select_database
list_query_library
save_query_library_item
delete_query_library_item
get_app_settings
save_app_settings
```

具体命令名以现有 snake_case 约定和 TDD 测试先行结果为准；如果某个能力可以安全复用已有命令，不新增重复入口。所有命令参数和返回值均为可序列化 DTO，不把 Redis 客户端、密码或底层错误对象暴露给前端。

## 7. 错误、兼容与安全

统一使用固定错误码：

- `CONNECTION_NOT_OPEN`
- `INVALID_INPUT`
- `KEY_NOT_FOUND`
- `UNSUPPORTED_DATA_TYPE`
- `COMMAND_FAILED`
- `PERSISTENCE_FAILED`
- `OPERATION_CANCELLED`

兼容策略：

- 使用 `COMMAND INFO`、`MODULE LIST` 等只读命令探测 Redis 和模块能力。
- RedisJSON、Vector Set、Array、Search 等能力不可用时，只禁用对应操作并显示稳定提示。
- 数据库概览对不存在、权限不足或版本不支持的指标局部降级。
- 批量操作返回成功数、失败数和失败项，不因一个键失败而丢失整体结果。
- 所有异步请求携带连接/页面/键身份 token；连接切换、组件卸载和详情切换后丢弃过期结果。

安全边界：

- 密码只进入系统钥匙串，普通 JSON、日志、错误和 UI 状态不保存密码。
- `AUTH`、`HELLO`、敏感 `ACL`/`CONFIG` 命令不进入自动历史；Query Library 也拒绝保存这些命令。
- 不向前端回传 URI、密码、云账户、云端点或 Redis 底层错误文本。
- 导入文件只解析受支持的 JSON DTO，不执行文件中的命令或脚本。
- 继续禁止 Redis Cloud、Azure、AI、Telemetry、远程插件和 SQL 存储入口。

## 8. 后续长连接与高级连接边界

Slow Log、Pub/Sub、Profiler 需要独立的 Tauri event channel，而不是循环调用普通 request-response IPC。事件资源必须绑定连接 ID 和订阅 ID，并在页面卸载、连接切换、停止操作和应用退出时清理。

TLS、证书、SSH、Sentinel、Cluster 需要扩展连接 profile、连接 manager 和 Redis client 生命周期，单独设计和验收；第一批不改变当前 Standalone TCP 连接模型。

## 9. 测试与验收

所有功能遵循 Red-Green-Refactor：先编写失败行为测试，确认失败原因，再实现最小代码，最后重构。

Rust 测试覆盖：

- DTO 校验、命令目录、结果格式化和错误映射。
- JSON 文件版本迁移、原子写入、损坏文件恢复和敏感字段过滤。
- Browser 过滤/元数据、Workbench 多命令、实例概览、数据库切换和 Query Library service。
- 真实 Standalone Redis 的 Browser、Workbench 和概览流程；Redis 模块缺失时验证稳定降级。

前端测试覆盖：

- Browser 搜索、类型过滤、刷新、导入导出、空状态和过期请求保护。
- Workbench 补全、多命令执行、停止策略、结果格式切换、复制和历史过滤。
- 实例信息、数据库切换、Query Library、设置读写、加载/错误/空状态。
- 现有连接管理、Browser 编辑器、Stream/JSON 和 Workbench 回归。

交付前运行：

```text
npm run check:non-cloud
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

环境允许时，设置 `REDIX_TEST_REDIS_URL` 执行 ignored 的真实 Redis 集成测试，并运行 Tauri release bundle。所有缺失环境依赖、测试失败和降级行为都记录到 `progress.md`。

第一批交付必须满足：Browser、Workbench、数据库/实例概览、Query Library/设置均可从应用导航进入；Redis 模块缺失不影响基础连接；敏感信息不落盘；非 Cloud 范围扫描通过；已有 MVP 测试全部回归通过。

## 10. 实施顺序

1. 先补充领域模型、错误码、持久化和能力探测的失败测试。
2. 实现 Browser 生产力增强和本地导入导出边界。
3. 实现 Workbench 命令目录、多命令结果、格式化和历史持久化。
4. 实现实例/数据库概览与数据库切换。
5. 实现 Query Library、设置和应用导航入口。
6. 每个子批次运行前端/Rust/集成测试，完成非 Cloud 静态审查。
7. 更新 README、范围文档、进度记录并运行完整交付验证。

本设计只定义当前第一批和后续边界，不承诺一次性复刻 RedisInsight 全部功能；任何新增的长连接、拓扑、模块或云相关能力都必须另行设计和验收。
