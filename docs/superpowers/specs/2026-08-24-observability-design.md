# 本地 Redis 运维观察能力设计

日期：2026-08-24

## 背景与目标

对照 `/Users/ushopal/workspace/myself/RedisInsight` 的非 Cloud 能力，当前 Redix 已完成连接管理、Browser、Workbench、Database、Query Library 和 Settings，但仍缺少 Slow Log 与 Pub/Sub。本批补齐这两个本地 Redis Standalone 能力，保持 Rust + Tauri 2 + React 架构，不引入 Redis Cloud、Azure、AI、Telemetry、远程插件或 SQL。

交付目标：

1. Slow Log 支持读取指定数量的慢命令、清空记录、查看当前配置和更新 `slowlog-max-len`/`slowlog-log-slower-than`。
2. Pub/Sub 支持普通频道订阅、模式订阅、取消订阅、发布消息和实时消息列表。
3. Pub/Sub 的后台连接、任务、事件监听在停止、连接关闭、连接切换和页面卸载时都可清理，不遗留 Redis socket 或 Tauri listener。
4. 所有错误向前端只暴露固定错误码和固定中文提示，不回传 Redis URI、密码或底层错误文本。

## 范围与非目标

### 本批范围

- 只支持已打开的本地 Redis Standalone TCP 连接。
- Slow Log 使用 Redis `SLOWLOG` 与 `CONFIG` 命令。
- Pub/Sub 使用 Redis 独立异步 Pub/Sub 连接；发布使用普通多路复用连接。
- Slow Log 查询默认最多 1000 条；Pub/Sub 前端消息缓存最多 5000 条，超过上限丢弃最旧消息并保留最新消息。
- 页面接入现有左侧工作区导航和连接上下文。

### 明确排除

- Redis Cloud、Azure Managed Redis、云账户、OAuth、云 API、云数据库发现和云 SDK。
- Profiler、MONITOR 实时命令分析、Cluster/Sentinel、TLS/SSH、证书管理。
- Pub/Sub Consumer Group、Redis Streams 实时消费、模块专用编辑器、远程插件和 AI 功能。
- 任何 SQL 查询；本项目继续不引入 SQL，也不得在循环遍历中查询 SQL。

## 方案比较与选择

### 方案 A：Pub/Sub 轮询

前端定时调用一次性 Redis 命令读取消息。实现成本低，但 Redis Pub/Sub 消息不会保存在键空间，轮询无法可靠获取实时消息，且会产生额外命令和竞态。放弃。

### 方案 B：Tauri 事件 + Rust 后台订阅任务（推荐）

Rust 从当前已打开连接创建独立 `PubSub` 连接，完成 `SUBSCRIBE`/`PSUBSCRIBE` 后由 `tokio::spawn` 持续读取消息，通过 `AppHandle::emit` 将固定 DTO 发给前端；Rust 保存可取消任务句柄，前端用 `listen` 注册/移除事件。该方案保留桌面应用边界，不开放本地 HTTP 端口，且能明确处理连接切换和页面卸载。

### 方案 C：本地 HTTP/WebSocket 服务

新增本地端口和 WebSocket 生命周期，表面上接近参考项目，但会引入端口占用、跨层认证和额外攻击面；对单窗口 Tauri 应用属于过度设计。放弃。

本批采用方案 B；Slow Log 仍走现有 typed `invoke`，避免为一次性操作增加事件复杂度。

## 架构设计

### Rust 领域与 Redis service

新增 `domain/observability.rs`，集中定义可序列化 DTO 和输入校验：

```rust
pub struct SlowLogEntry {
    pub id: u64,
    pub time: i64,
    pub duration_us: u64,
    pub args: Vec<String>,
    pub source: String,
    pub client: Option<String>,
}

pub struct SlowLogConfig {
    pub slowlog_max_len: u64,
    pub slowlog_log_slower_than: i64,
}

pub struct GetSlowLogsInput {
    pub connection_id: String,
    pub count: i64,
}

pub struct UpdateSlowLogConfigInput {
    pub connection_id: String,
    pub slowlog_max_len: Option<u64>,
    pub slowlog_log_slower_than: Option<i64>,
}

pub struct PubSubTopic {
    pub name: String,
    pub pattern: bool,
}

pub struct StartPubSubInput {
    pub connection_id: String,
    pub session_id: String,
    pub topics: Vec<PubSubTopic>,
}

pub struct PubSubSession {
    pub connection_id: String,
    pub session_id: String,
    pub topics: Vec<PubSubTopic>,
}

pub struct StopPubSubInput {
    pub connection_id: String,
    pub session_id: String,
}

pub struct PublishPubSubInput {
    pub connection_id: String,
    pub channel: String,
    pub message: String,
}

pub struct PubSubMessageEvent {
    pub connection_id: String,
    pub session_id: String,
    pub channel: String,
    pub pattern: Option<String>,
    pub message: String,
    pub received_at_ms: u64,
}

pub struct PubSubStatusEvent {
    pub connection_id: String,
    pub session_id: String,
    pub state: String,
    pub error_code: Option<String>,
}
```

`SlowLogEntry.args` 保留 Redis 返回的逐项参数，前端负责展示为单行命令；这样不会因为空格、二进制或引号丢失边界。Redis RESP 数组解析只接受预期的整数/字符串结构，异常结构统一映射为 `COMMAND_FAILED`。

`GetSlowLogsInput.count` 允许 `-1` 或 `1..=1000`；`-1` 由 service 先读取当前 `slowlog-max-len` 后以该上限读取，避免无界返回。配置校验为 `slowlog_max_len >= 0`、`slowlog_log_slower_than >= -1`，且至少提供一个更新字段。

`PubSubTopic` 的 `name` 去除首尾空白且长度为 1..=256；一次最多 20 个主题，禁止空主题和重复主题。`session_id` 由前端生成随机非敏感标识，仅用于过滤事件，不作为 Redis 凭据。

`RedisService` 增加：

- Slow Log request-response 方法：`get_slow_logs`、`clear_slow_logs`、`get_slow_log_config`、`update_slow_log_config`。
- Pub/Sub 方法：`start_pub_sub`、`stop_pub_sub`、`publish_pub_sub`，以及按 connection id 清理订阅任务的内部方法。
- 一个按 `connection_id` 保存活动订阅任务的生命周期表。启动同一连接的新 session 时先取消旧任务；`close_connection`、重新 `open_connection` 和 `select_database` 替换 client 前先取消该连接任务。

Slow Log 命令映射：

| service 方法 | Redis 命令 | 返回 |
|---|---|---|
| `get_slow_logs` | `SLOWLOG GET count` | `Vec<SlowLogEntry>` |
| `clear_slow_logs` | `SLOWLOG RESET` | `()` |
| `get_slow_log_config` | `CONFIG GET slowlog-*` | `SlowLogConfig` |
| `update_slow_log_config` | 需要时执行两个 `CONFIG SET` | 更新后的 `SlowLogConfig` |

`update_slow_log_config` 先读取当前配置，按输入字段执行设置，再返回最终配置；没有输入字段直接返回 `INVALID_INPUT`。普通 Standalone 不需要目标项目的 Cluster fan-out 分支。

Pub/Sub 启动流程：

1. 校验输入并从 active client 表取出 `Client`，不存在时返回 `CONNECTION_FAILED`。
2. 创建独立 async Pub/Sub 连接，按 topic 的 `pattern` 字段调用 `psubscribe` 或 `subscribe`；订阅失败时不登记任务并返回 `COMMAND_FAILED`。
3. 取消同一 `connection_id` 的旧任务，将新的 `JoinHandle` 登记到生命周期表。
4. 后台任务读取消息并发出 `redix://pubsub/message`；任务正常停止或连接错误时发出 `redix://pubsub/status`，状态为 `stopped` 或 `error`，错误只携带固定错误码。
5. `stop_pub_sub` 移除并 abort 对应任务；停止不存在的 session 返回成功，保证页面卸载幂等。

发布流程使用普通 multiplexed connection 执行 `PUBLISH channel message`，返回 Redis 的接收客户端数量；发布不依赖订阅任务是否已启动。

### Tauri command 与事件边界

新增命令模块 `commands/observability.rs`，注册以下命令：

- `get_slow_logs(input) -> Result<Vec<SlowLogEntry>, AppError>`
- `clear_slow_logs(connection_id) -> Result<(), AppError>`
- `get_slow_log_config(connection_id) -> Result<SlowLogConfig, AppError>`
- `update_slow_log_config(input) -> Result<SlowLogConfig, AppError>`
- `start_pub_sub(app, state, input) -> Result<PubSubSession, AppError>`
- `stop_pub_sub(state, input) -> Result<(), AppError>`
- `publish_pub_sub(state, input) -> Result<u64, AppError>`

事件名称固定为：

- `redix://pubsub/message`
- `redix://pubsub/status`

事件 payload 仅为上述 `PubSubMessageEvent` 和 `PubSubStatusEvent`。事件中不包含连接 URI、用户名、密码、Redis 原始错误字符串或文件路径。

新增 `AppError` 变体 `InvalidInput`、`OperationCancelled`；Redis 命令失败仍使用现有固定 `COMMAND_FAILED`，连接异常使用 `CONNECTION_FAILED`。

### 前端状态与页面

新增：

- `src/features/observability/slowLogState.ts`：Slow Log 的输入校验、配置归一化和固定错误提示。
- `src/features/observability/SlowLogPage.tsx`：数量选择、刷新、清空、配置编辑和表格。
- `src/features/observability/pubSubState.ts`：主题解析、消息缓存上限、事件过滤和固定错误提示。
- `src/features/observability/PubSubPage.tsx`：普通/模式订阅、取消订阅、发布消息、清空消息和事件列表。

`App.tsx` 新增 `slow-log`、`pub-sub` 工作区；无 active profile 时保持禁用。进入 Pub/Sub 页面时注册两个 Tauri listener，离开页面或 connection id 变化时先移除 listener，再调用 `stop_pub_sub`；调用失败只显示固定提示，不阻塞页面卸载。

Pub/Sub 页面只允许一个当前 session。订阅输入支持多行主题，每行一个主题，并由模式选择决定全部使用 `SUBSCRIBE` 或 `PSUBSCRIBE`；发布表单独立接受 channel 与 message。消息表按接收时间倒序展示，缓存超过 5000 条时删除最旧项并显示“消息过多，已丢弃较早消息”提示。

Slow Log 页面默认读取 50 条，提供 10/25/50/100/500 选项；展示 ID、执行时间、耗时、命令、来源和客户端；配置区展示微秒阈值与最大长度，保存后刷新配置和列表。清空操作需要二次确认，成功后清空当前表格并显示状态反馈。

### 错误与兼容策略

- Redis 不支持 `SLOWLOG` 或当前用户没有 ACL 权限：`COMMAND_FAILED`。
- Redis 不支持 `CONFIG SET` 或配置值非法：`COMMAND_FAILED` 或 `INVALID_INPUT`，前端只显示稳定中文提示。
- Pub/Sub 订阅连接断开：发送 `state=error`，页面保留已收到消息并允许重新订阅。
- 事件携带旧 `session_id` 或旧 `connection_id`：前端直接丢弃，避免连接切换后的旧消息污染新页面。
- `stop_pub_sub`、listener cleanup 和窗口关闭均设计为幂等。

## 测试与验收

### Rust

- 领域测试覆盖 Slow Log RESP 解析、配置校验、主题去重/长度校验和事件 DTO 序列化。
- Redis service 测试覆盖无活动连接、命令错误映射、Slow Log 配置更新和 Pub/Sub session 替换/停止；真实 Redis 流程加入显式 `ignored` 集成测试，可通过 `REDIX_TEST_REDIS_URL` 手动运行。
- Tauri command adapter 测试确认新增命令均注册、snake_case 输入有效、错误序列化只包含 `code/message`。

### 前端

- bridge 测试覆盖 7 个 request-response wrapper、Pub/Sub listener 注册/取消和事件 payload 类型。
- Slow Log 测试覆盖空状态、刷新、清空确认、配置保存、错误提示和列表展示。
- Pub/Sub 测试覆盖普通/模式订阅、取消、发布、清空、旧 session 事件过滤和 5000 条缓存上限。
- App smoke 测试覆盖新导航入口和未连接禁用状态。

### 交付检查

```bash
npm run test:frontend
npm run build
npm run check:non-cloud
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml -q
git diff --check
```

若本机有 Redis，再运行：

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
```

验收时逐项确认产品源码与菜单中没有 Redis Cloud、Azure、OAuth、云端 API、AI、Telemetry 或远程插件入口。

