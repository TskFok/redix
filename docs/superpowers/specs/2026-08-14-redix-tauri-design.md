# Redix：排除 Redis Cloud 的 Rust + Tauri Redis 客户端设计

## 1. 目标

在当前空目录中创建一个 Rust + Tauri 桌面客户端，参考 RedisInsight 的本地 Redis 管理体验，先交付一个可运行的 MVP：连接 Redis Standalone、浏览和过滤键、编辑常用数据结构，并通过 Workbench 执行 Redis 命令。

本项目是功能重构，不直接复制 RedisInsight 的 Electron、React 或 NestJS 代码。首版优先形成清晰、可测试的 Tauri IPC 与 Rust 领域层边界，为后续扩展监控和 Redis 模块能力保留接口。

## 2. 范围

### 2.1 首个 MVP 包含

- Redis Standalone TCP 连接。
- 连接配置的新增、编辑、删除、测试和本地持久化。
- Browser：使用 `SCAN` 分页加载键，支持按模式过滤。
- 键详情：显示类型、TTL、长度/大小和值。
- 基础 CRUD：`String`、`Hash`、`List`、`Set`、`Sorted Set`。
- TTL 查看和设置。
- Workbench：对当前连接执行 Redis 命令，展示成功结果或结构化错误。
- 深色默认主题和浅色主题切换。
- Rust 单元测试、Redis 集成测试、前端测试和 Tauri 构建检查。

### 2.2 后续阶段

以下能力不进入首个交付闭环，但可在不改变核心连接模型的前提下扩展：

- Redis Cluster、Sentinel。
- TLS、SSH 隧道和其他高级连接方式。
- Stream、RedisJSON、Search/Query、Vector Set、Time Series 等模块能力。
- Profiler、Slow Log、Pub/Sub、批量操作和数据分析。
- Workbench 高级自动补全、可视化和插件系统。

### 2.3 明确排除

新项目不包含任何 Redis Cloud 或云服务管理能力：

- 不提供 Redis Cloud 登录、账户、组织、项目或订阅管理。
- 不提供云数据库发现、创建、删除、扩缩容或云端状态同步。
- 不引入 Redis Cloud/Azure 云 SDK、云 API 客户端或云端点配置。
- 不在菜单、导航、连接表单或本地数据模型中保留 Cloud 专用入口。
- 不将云凭据保存到本地，也不为云登录保留 feature flag。

## 3. 技术方案

### 3.1 方案比较

| 方案 | 优点 | 代价 | 结论 |
|------|------|------|------|
| Tauri 2 + React/Vite + Rust Redis domain | 适合多面板工作区；前端开发效率高；Redis 访问集中在 Rust；扩展边界清晰 | 需要维护前端构建和 Rust IPC 类型 | 采用 |
| Tauri 2 + 原生 HTML/CSS/TypeScript + Rust | 依赖更少；初始包体较小 | 复杂列表、编辑器和状态管理需要手工实现 | 不采用 |
| Tauri 2 + React/Vite + Axum 本地 HTTP 服务 + Rust domain | HTTP 接口容易独立测试 | 增加端口、服务生命周期和本地安全边界 | 不采用 |

### 3.2 总体架构

```text
React/Vite UI
    │ Tauri invoke
    ▼
Tauri commands
    │ typed domain models
    ▼
Rust application state
    ├── connection manager
    ├── persistence
    └── Redis service
            │ async TCP
            ▼
        Redis Standalone
```

- Tauri 2 负责桌面窗口、IPC、应用目录和打包。
- React/Vite 负责连接页、Browser、Workbench 和本地 UI 状态。
- Rust 负责 Redis 连接、命令执行、数据结构转换、错误映射和持久化。
- 不启动本地 HTTP 服务，不引入 SQL 数据库；连接配置使用 JSON 文件，敏感凭据使用系统钥匙串。
- Redis 访问使用异步 Tokio 运行时和 Redis Rust 客户端；共享连接状态由应用状态容器管理。

## 4. 目录结构

```text
redix/
├── src/
│   ├── components/
│   ├── features/
│   │   ├── connections/
│   │   ├── browser/
│   │   └── workbench/
│   ├── lib/
│   │   └── tauri.ts
│   └── App.tsx
├── src-tauri/
│   ├── src/
│   │   ├── commands/
│   │   ├── domain/
│   │   ├── persistence/
│   │   ├── redis/
│   │   ├── error.rs
│   │   └── lib.rs
│   ├── capabilities/
│   └── tauri.conf.json
├── docs/
└── package.json
```

Rust 模块职责保持单一：

- `domain`：连接配置、键摘要、值模型和命令结果等序列化模型。
- `redis`：建立连接、SCAN、键读写、TTL 和 Workbench 命令执行。
- `persistence`：读写应用数据目录中的配置，以及系统钥匙串中的密码。
- `commands`：只负责校验输入、调用领域服务和返回 Tauri 可序列化结果。
- `error.rs`：将底层错误转换为稳定的前端错误码和安全提示。

## 5. 数据模型与命令边界

### 5.1 连接配置

```text
ConnectionProfile
├── id: string
├── name: string
├── host: string
├── port: u16
├── username: string?
└── database: u8
```

密码不属于普通 JSON 配置模型。配置文件只保存连接元数据和 `has_password` 状态；密码通过系统钥匙串以 `redix/<connection-id>` 作为服务和账户组合保存。删除连接时同时删除对应钥匙串条目。

### 5.2 Browser 模型

```text
KeySummary
├── key: string
├── key_type: string
├── ttl_ms: i64
└── size: u64?

ScanPage
├── cursor: u64
├── keys: KeySummary[]
└── has_more: bool
```

Browser 始终使用增量 `SCAN`，不执行 `KEYS`。打开键时再按类型读取详情，避免一次加载整个数据库。

### 5.3 Tauri commands

- `list_connections`
- `save_connection`
- `delete_connection`
- `test_connection`
- `open_connection`
- `close_connection`
- `scan_keys`
- `get_key`
- `set_key`
- `delete_key`
- `set_key_ttl`
- `execute_command`

Workbench 首版按单条命令执行，返回 JSON 可序列化的标量、数组、空值或错误对象。命令输入只作用于用户当前选择的本地连接；多命令脚本、事务编排和高级自动补全不在本阶段实现。

## 6. 用户界面与数据流

### 6.1 页面

- 连接页：展示已保存连接，提供新增、编辑、删除、测试连接和打开操作。
- Browser：左侧键列表和过滤栏，右侧键详情及编辑表单。
- Workbench：命令输入区、执行按钮、历史记录和结果面板。

### 6.2 典型流程

1. 应用启动，Rust 从应用数据目录加载连接元数据，前端展示连接列表。
2. 用户保存连接，前端调用 `save_connection`；Rust 校验 host/port，保存元数据并将密码写入系统钥匙串。
3. 用户打开连接，前端调用 `open_connection`；Rust 从钥匙串读取密码并建立 Redis 客户端，返回连接状态和基本 `INFO` 摘要（不保存完整敏感信息）。关闭连接调用 `close_connection`，释放对应的客户端状态。
4. Browser 调用 `scan_keys` 分页获取键摘要；用户点击键后调用 `get_key` 获取详情。
5. 编辑器根据键类型调用 `set_key`、`delete_key` 或 `set_key_ttl`。
6. Workbench 调用 `execute_command`，结果通过统一模型返回前端。

## 7. 错误处理与安全

统一错误类型至少区分：

- `InvalidConnection`: host、port、database 或字段校验失败。
- `ConnectionFailed`: DNS、TCP、超时或连接中断。
- `AuthenticationFailed`: Redis 认证失败。
- `UnsupportedDataType`: 当前 MVP 未实现的数据类型。
- `CommandFailed`: Redis 命令返回错误。
- `PersistenceFailed`: 配置文件或钥匙串读写失败。

前端只展示稳定错误码对应的用户提示；底层错误仅写入开发环境诊断日志，日志中不得包含密码或完整 URI。所有 Redis 命令都通过 Rust 服务执行，前端不持有原始 TCP 客户端。

## 8. 测试与验收

### 8.1 测试策略

- Rust 单元测试：连接配置校验、命令结果序列化、错误映射、键值转换。
- Redis 集成测试：使用本地 Redis 服务覆盖 PING、连接失败、SCAN、String/Hash/List/Set/ZSet CRUD、TTL 和删除。
- 前端测试：连接表单、连接列表、键过滤、详情编辑、Workbench 结果和错误提示。
- 构建验证：前端生产构建、`cargo test`、Tauri 开发构建；环境允许时运行完整桌面打包。

### 8.2 验收清单

1. 空配置启动后可以新增 Standalone 连接。
2. 可连接本地 Redis，并显示连接成功或明确失败原因。
3. Browser 使用 `SCAN` 分页加载和过滤键。
4. 可读写并删除 String、Hash、List、Set、Sorted Set。
5. Workbench 可执行 `PING`、`GET`、`SET` 等命令并展示结果。
6. 重启应用后连接元数据保留，密码不出现在普通配置文件中。
7. 单元测试、集成测试、前端测试和构建命令均有可复现结果。
8. `src/`、`src-tauri/`、菜单和依赖中不包含 Redis Cloud 功能。

## 9. 实施顺序

1. 初始化 Tauri 2 + React/Vite 工程和 Rust 测试骨架。
2. 先编写连接配置与 Redis 错误模型的失败测试，再实现领域模型。
3. 实现配置持久化和系统钥匙串适配。
4. 实现 Redis 连接、SCAN、键详情、CRUD 和 TTL commands。
5. 实现连接页、Browser 和 Workbench UI。
6. 补充集成测试、前端测试和 Cloud 排除审查。
7. 运行完整验证并整理运行文档。

## 10. 明确的非目标

本设计不承诺在首个版本复刻 RedisInsight 的全部功能，也不包含 Redis Cloud、Azure Managed Redis 或其他云服务能力。后续功能应单独设计和验收，不通过隐式加入依赖或菜单扩大本 MVP 范围。
