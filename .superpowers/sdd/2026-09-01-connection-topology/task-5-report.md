# Task 5 实施报告

## 状态

已完成 `ConnectionManager` 到 `RoutedClient` / `RoutedConnection` 的迁移，并保持既有 IPC 方法签名不变。Cluster、Sentinel、Standalone direct/SSH/TLS/SSH+TLS 均经统一连接生命周期发布；本任务未实现拓扑命令、全节点 SCAN 或前端功能。

## RED → GREEN

1. Cluster 现有命令路径
   - RED：新增 Browser、JSON、Workbench、CLI、失败发布、非零 DB 与 CROSSSLOT 测试后，`ConnectionHandle` 和 CLI 仍要求 concrete `Client` / `MultiplexedConnection`，Cluster 无法经现有服务路径工作。
   - GREEN：handle 保存 `RoutedClient`、原 profile、`ConnectionTarget` 与 SSH transport；所有通用命令 helper 和 CLI socket 改用 `RoutedConnection`。fake Cluster 上 Browser、JSON、Workbench、CLI 全部通过，失败构建不发布 handle，非零 DB 固定返回 `UnsupportedFeature`，CROSSSLOT 固定映射为 `CrossSlot` 且不复制服务端文本。
2. Cluster capability gate
   - 最初测试错误地尝试把 `MockRuntime` AppHandle 传入固定 Wry IPC 签名，属于测试自身编译错误，未作为产品 RED 证据。
   - 修正后的 RED 直接验证缺失的 `standalone_client` capability boundary；GREEN 后 Cluster 返回固定 `UnsupportedFeature`。PubSub 与 Profiler 都先经过同一 boundary，再分别把已经构造的 `PubSub` / `MonitorLineStream` 交给 manager，不会选择随机节点。
3. Sentinel SSH 与 SSH+TLS
   - RED：测试所需 fake SSH forwarding transport API 缺失；补齐非生产可见的测试 backend 后，旧连接分支仍不能通过每个 seed 与 primary forward 完成发现。
   - GREEN：每个 Sentinel seed 都由同一个 `SshTransport` 建立对应 forward，TLS 始终保留原 seed endpoint 作为 SNI；发现 primary 后再建立 primary forward，并只发布该 `StandaloneClient`。测试使用不可解析逻辑主机，记录的实际 target 严格为 seed 1、seed 2、primary，从而证明没有直连。Standalone SSH+TLS 同样保留原 endpoint/SNI 与 forward 生命周期。
4. 生命周期竞争
   - RED：暂停 fake Cluster 的 inspection 后并发 close，旧 open 曾有发布陈旧连接的风险。
   - GREEN：沿用 generation → profile transaction 的既有锁顺序与 publication token 校验；close 使 pending open 返回 `OperationCancelled`，且不发布 handle。既有 open/select/close、凭据/profile 竞争与 capability generation 单测继续全绿。
5. database analysis
   - RED：分析 helper 参数绑定 `MultiplexedConnection`，无法接受 Cluster routed connection。
   - GREEN：SCAN 与 metadata pipeline 均通过 `RoutedConnection`；fake Cluster 回归验证 DB 0、key 统计和 memory 聚合。

## 实现要点

- `ConnectionHandle` 持有 `RoutedClient`、profile、`ConnectionTarget` 与可选 `SshTransport`；`RedisService` 产出 `routed_connection`、`standalone_client`、`connection_target`。
- Standalone direct/SSH/TLS/SSH+TLS 统一经 `StandaloneClient`。SSH 分支只从 direct client 读取已验证的 Redis settings，不在原地址建连；实际 socket 始终由 `TunneledClient` 的 loopback forward 创建。
- Cluster 只由 bounded Cluster builder 构造；失败不会回退或发布 Standalone。`select_database` 只允许 DB 0。
- Sentinel discovery 使用 Sentinel 专属 ACL/密码访问 seed，主节点使用 Redis ACL/密码；每次 seed 尝试和 primary 都经对应 forward，失败 seed 的 forward 随临时 client 释放，published primary client 保有必要 forward，handle 保有 transport。
- `PubSubManager::start` 消费构造好的 `redis::aio::PubSub`；`ProfilerManager::start` 消费统一的 `MonitorLineStream`。
- `profile.rs` 已由前置任务提供完整 `ConnectionTarget::try_from` 与 Cluster DB 0 合同，本任务直接复用，未重复修改。
- 没有新增 SQL，也没有循环 SQL 查询。

## 验证

- 开发期 focused（非隔离 loopback）：
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib redis::connection_manager::tests`：30 passed。
  - `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection`：22 passed。
  - `cargo test --manifest-path src-tauri/Cargo.toml --test commands`：11 passed。
  - `cargo test --manifest-path src-tauri/Cargo.toml --test database_analysis`：8 passed。
  - `cargo test --manifest-path src-tauri/Cargo.toml --test sentinel`：7 passed，2 ignored。
- `npm run test:rust`（非隔离 loopback）：exit 0；lib 166 passed、1 ignored，全部常规 integration targets 通过，外部 Redis/Redis Stack/sshd 用例按既有标记 ignored。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`：exit 0。
- `git diff --check`：exit 0。
- `rg -n 'MultiplexedConnection' src-tauri/src/redis`：只命中 `routed_connection.rs` 与 `standalone_transport.rs` 的 routed/transport 构造和 wrapper，没有 operation helper 或 CLI socket 残留。

## Concerns

- 隔离沙箱曾使依赖 loopback 的 focused tests 在 `TcpListener::bind` 返回 `EPERM`；同一测试组合与最终全量套件已在非隔离环境通过，行为断言无失败。
- 编译仍只有 `redis/array.rs` 的 5 个既有 dead-code warnings，以及部分 integration target 的既有 `invalid_profile` warning；本任务未新增生产 warning。
- 真实外部 Redis、Redis Stack 与 sshd 测试继续 ignored；本任务的 Sentinel SSH target/SNI/no-direct 由真实 loopback socket 加 fake SSH session backend 确定性覆盖。
