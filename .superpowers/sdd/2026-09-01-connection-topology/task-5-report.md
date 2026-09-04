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

## Fix round 1

### 状态

已按 review ruling 修复 active handle 快照一致性、capability gate、database analysis 标注、PubSub/Profiler 注册、CLI reopen 和 Sentinel SSH 服务发布生命周期。IPC 签名未改变，没有提前实现 Task 6/7。

### RED → GREEN

1. active snapshot 与 capability-gated command
   - RED：旧实现先完成旧 handle 的 RedisJSON capability probe，再经 connection id 重新取连接；并发 replace 后，测试实际从新实例读到 `"new"`，证明 capability 与命令跨代混用。
   - GREEN：新增 generation → capabilities → active 锁序下的 `ActiveConnectionSnapshot`，一次克隆 token、client、profile、target 与缓存；未缓存 probe 和后续 gated command 共用同一 `RoutedConnection`。旧 probe 的 stale 结果不写缓存，也不切换到新 handle。Array、Vector Set、Search、Search Aggregate 与 RedisJSON 调用点均改用同一上下文。
2. database analysis / overview
   - RED：分析连接选中 DB 1 后并发切换到 DB 2，旧实现随后重新读取 profile，报告被错误标成 DB 2。
   - GREEN：`analyze_database` 与 `get_database_overview` 都只从一个 active snapshot 取得 client 与 `profile.database`；SELECT 屏障测试验证分析结果和 overview fallback 均保持 DB 1 标注。
3. PubSub / Profiler
   - RED：manager 用 `HashMap::insert` 替换任务时只丢弃旧 `JoinHandle`，旧 transport 没有被 abort；并且不存在可把 ready 注册、running 事件和启动屏障纳入同一原子区的共享 helper。
   - GREEN：网络连接、订阅与 MONITOR prepare 均在 generation 锁外；prepare 后持 generation read guard 校验 token，并同步调用 manager。共享 `TaskRegistry::register_ready` 在一个临界区内替换并显式 abort 旧任务、发送旧 stopped、发送新 running、放行已注册 worker；自然结束只由仍匹配 session 的 registry entry 发一次 stopped 并清理。Tauri mock AppHandle 真实覆盖 PubSub/Profiler 两条 `start`，另有双并发 ready 注册事件顺序、start-vs-close 和 stale start-vs-replace 屏障测试。
4. CLI reopen
   - RED：成功 reopen 后旧 CLI session 仍能向旧实例执行命令。
   - GREEN：Tauri `open_connection` 只在 Redis reopen 成功后关闭该 connection id 的旧 CLI session；失败保留旧 session。CLI session open 与 Redis open/select/close/delete 共用 lifecycle mutex，避免握手跨过 handle 切换。测试验证旧 socket 释放、旧 session 失效、重开后命令只到新实例；失败 reopen 后旧实例仍可用；并发 CLI open 等待 reopen 后使用新 handle。
5. Sentinel SSH 服务发布与凭据隔离
   - RED：原测试 helper 不解析 RESP，无法证明 Sentinel seed 和 Redis primary 使用不同 ACL，也未经过 `RedisService::open_connection` publication。
   - GREEN：测试 fake server 完整解析并记录 RESP 命令；仅在 `cfg(test)` 内注入 fake SSH transport，通过真实 `connect_handle` 与 service publication。断言 seed 使用 Sentinel username/password、primary 使用 Redis username/password 且互不串用；失败 seed forward 释放；丢弃外部 transport 后已发布 primary 仍可执行 PING；close handle 后 primary forward listener 不再接受连接。生产 API 未暴露测试 hook。

### 跨文件迁移审计

- `src-tauri/src/redis/search_aggregate.rs` 是独立于 `connection_manager.rs` 的现存 Search capability-gated operation；原先先取 capabilities、再按 connection id 取连接，仍会跨代混用，因此必须改为共享 `search_connection` 的同快照 capability+connection 上下文。
- `src-tauri/src/commands/database.rs` 不属于 capability gate；它是 CLI lifecycle 修复的必要调用点。`select_database` 会发布替换 handle 并关闭旧 CLI session，必须与并发 CLI open 共用 lifecycle mutex，避免 CLI 握手跨过 DB handle 切换。该改动只做 Task 5 既有选择数据库路径的序列化，没有引入 Task 6/7 行为。

### 验证

- focused：observability 10/10；commands 12/12；database analysis 10/10；Sentinel 7 passed、2 ignored。
- `npm run test:rust`：exit 0；lib 177 passed、1 ignored，全部常规 integration targets 通过；外部 Redis、Redis Stack、sshd 用例保持既有 ignored 标记。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`：exit 0。
- `git diff --check`：exit 0。
- `rg -n 'MultiplexedConnection' src-tauri/src/redis`：只命中 routed/standalone transport wrapper 与构造，没有 operation helper 或 CLI socket 残留。

### Concerns

- 仍只有既有 `redis/array.rs` 5 个 dead-code warning 与 integration fixture 的 `invalid_profile` warning；本轮未新增生产 warning。
- 真实外部 Redis、Redis Stack、sshd 测试继续 ignored；新增竞态、Sentinel ACL 与 forward 生命周期均由本地确定性 fake transport/socket 覆盖。

## Fix round 2

### 状态

本轮只修复 re-review 指定的 observability 同 session 注册身份竞态，并使 Sentinel service publication 所有权测试释放所有 service/external 注入 clone；未改变 IPC、Task 6/7 或 SQL 路径。

### RED → GREEN

1. observability 内部 registration identity
   - RED：同一 connection id 与同一客户端 session id 注册两次；旧 worker 已进入完成清理并阻塞于 registry mutex，测试在锁内放入 replacement 后再恢复旧 worker。旧实现只比较 session id，错误领取 replacement、发送其 stopped，实际 `new_stopped` 从预期 0 变为 1，并使新 worker 无法再由 stop/replace abort。
   - GREEN：每条 PubSub/Profiler 注册创建独立的 `RegistrationIdentity(Arc<()>)`。registry entry 与 worker 各持同一 identity；自然完成必须同时匹配 connection id、对外 session id 和 `Arc::ptr_eq` 的内部 registration identity 才能领取 entry 与发送 stopped。外部 stop 仍只使用原 connection/session 语义。确定性回归验证旧 worker 被替换时只发送一次旧 stopped，新 entry 保留，且新 worker随后仍可被 stop 并实际 drop。
   - PubSub 与 Profiler 均走同一个 `TaskRegistry` 实现；既有 Tauri mock AppHandle 两条真实 manager `start` 回归继续覆盖该共享清理路径。
2. Sentinel publication 所有权测试
   - service `open_connection` 完成 publication 后，测试显式 `take` 并 drop `RedisService.test_ssh_transport` 的注入 clone，再 drop 外部 transport clone；此后 active handle 是唯一剩余所有权集合，仍必须可 PING 且 primary forward listener 存活，close handle 后 listener 必须关闭。失败 seed forward 回收和 Sentinel/Redis ACL 隔离断言保持不变。
   - 根据修订 ruling，`ConnectionHandle._ssh` 不是唯一合法 owner：Task 3 明确定义的 `RoutedClient::Standalone(TunneledClient) → Arc<SshForward> → Arc<SessionOwner>` 也在 published handle 内合法拥有 session。mutation 将 `_ssh` 置空后该测试仍通过，证明的是这条既有 client/forward 所有权链，不代表测试仍依赖 service 注入 clone；实际代码保留 `_ssh`，未破坏 Task 3。

### 验证

- RED：`cargo test --manifest-path src-tauri/Cargo.toml --lib displaced_same_session_cleanup_cannot_remove_the_replacement_registration -- --nocapture` 在旧实现失败：`new_stopped` 实际 1、预期 0。
- GREEN focused：observability 11/11；Sentinel service publication ownership 1/1。
- `npm run test:rust`：exit 0；lib 178 passed、1 ignored，全部常规 integration targets 通过；外部 Redis、Redis Stack、sshd 用例保持既有 ignored 标记。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check` 与 concrete connection 扫描均通过。
