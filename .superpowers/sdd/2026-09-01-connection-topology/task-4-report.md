# Task 4 实施报告

## 状态

已完成 `RoutedClient`、`RoutedConnection`、bounded Cluster builder、standalone direct/tunneled transport、TLS custom stream、PubSub 与 MONITOR stream。本任务未迁移 `connection_manager`，未实现拓扑命令或 scan。

## RED → GREEN

1. `standalone_route_delegates_database_commands_and_driver`
   - RED：`RoutedClient`、`StandaloneClient` 未定义，测试编译失败。
   - GREEN：实现 direct adapter、`RoutedConnection::Standalone` 与 `ConnectionLike` 委托；真实 loopback RESP server 上 DB=3、PING 与 multiplexed driver 通过。
2. `cluster_route_is_db_zero_reuses_connection_and_hides_standalone_capability`
   - RED：`RoutedClient::Cluster` 未定义。
   - GREEN：在真实 loopback fake Cluster 握手上确认 db=0、cluster connection clone 复用、cluster 不暴露 standalone capability，单命令与 pipeline 均可路由。
3. `cluster_builder_uses_shared_acl_credentials_and_opens_once`
   - RED：`RoutedClient::cluster` 不存在。
   - GREEN：builder 使用共享 username/password，并在构建阶段只打开一次 `ClusterConnection`；后续 `connection()` 仅 clone 已打开连接。
4. `tunneled_pubsub_and_monitor_use_custom_stream_auth_select_and_monitor_order`
   - RED：`TunneledClient`、`StandaloneClient::Tunneled`、`pubsub()`、`monitor_stream()` 不存在。
   - GREEN：custom TCP stream 上验证 `AUTH username password`、`SELECT`、`MONITOR` 顺序和首条 simple-string；PubSub 使用 redis 1.5 的公开 `PubSub::new`。
5. `tunneled_tls_dials_loopback_but_uses_original_dns_name_for_sni_and_validation`
   - RED：TLS custom stream 返回固定 `UnsupportedFeature`。
   - GREEN：连接 loopback `local_endpoint`，但 rustls `ServerName` 使用不可解析的原始主机 `cache.internal`；本地 CA/leaf 握手和 PING 成功，从而同时证明 SNI 与证书主机名验证使用原始主机，且没有静默回直连。
6. 后续边界回归覆盖 direct/cluster pipeline、IP `ServerName`、TLS PubSub+MONITOR 两次原始 SNI、AUTH 组合、forward guard 生命周期，以及 bulk/malformed/超 256 KiB monitor line 只返回一个固定错误后关闭。

## 依赖与本地 API 决定

- 通过 Cargo 为 redis 1.5 增加 `cluster-async` feature。
- 通过 Cargo 增加直接依赖：`rustls = "0.23.42"`、`tokio-rustls = "0.26.4"`、`rustls-native-certs = "0.8.3"`、`rustls-pemfile = "2"`，锁文件仅由 Cargo 更新。
- redis 1.5 的 `MultiplexedConnection::new_with_config` 返回 `(connection, 'static driver future)`；custom stream 分支显式 spawn driver。
- redis 1.5 的 `PubSub::new` 是公开 API；`Monitor::new` 为 crate-private，因此 tunneled MONITOR 使用独立、有限的 RESP simple-string reader。
- rustls 显式使用 aws-lc provider，避免依赖 feature 合并后依赖进程默认 provider；DNS 与 IP 均通过 owned `ServerName::try_from`，所有 rustls/PEM/native-root 错误映射为固定 `AppError`。
- Cluster builder 配置 3 秒 connect/response timeout、3 retries、250 ms/2 s retry wait、db=0、共享 ACL、TLS/certificates 与 `read_from_replicas`。
- `SshTransport` 与现有 `Arc<SshForward>` 各有 crate 内生产构造入口；私有生命周期守卫保存精确的 `Arc<SshForward>`，不从公共 API 泄露 SSH 内部类型。

## 安全与边界

- custom connection、PubSub、MONITOR setup、TCP connect 与 TLS handshake 均受 3 秒边界约束。
- MONITOR `+OK` acknowledgement 有 8 KiB 上限；事件只接受 RESP simple-string，payload 上限 256 KiB。
- malformed、bulk 或 oversized 事件只产出一次固定 `AppError::CommandFailed`，随后 reader drop 并结束 stream。
- 密码、客户端私钥和原始 redis/rustls 错误未写日志、未写错误消息、未进入 serde DTO。

## 验证

- `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection`
  - 12 passed，0 failed。
- brief 原组合命令 `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection --test sentinel --lib redis::connection_manager::tests`
  - exit 0；28 个 connection manager unit tests passed。Cargo 的尾部过滤器也作用于 integration binaries，因此该次 routed/sentinel 显示 0 tests，未把它当作完整覆盖。
- 单独 `cargo test --manifest-path src-tauri/Cargo.toml --test sentinel`
  - 7 passed，0 failed，2 ignored（既有测试要求本地 redis-server/SSH）。
- `cargo test --manifest-path src-tauri/Cargo.toml --no-run`
  - exit 0，全部 lib/main/integration test targets 编译成功。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
  - exit 0。
- `git diff --check`
  - exit 0。

## Concerns

- 编译仍显示 `redis/array.rs` 的 5 个既有 dead-code warnings；Task 4 未新增 warning。
- Sentinel 的 2 个外部环境测试保持 ignored。最终 fresh verification 在隔离沙箱内绑定 loopback 时以 `EPERM` 失败（11 项网络测试均停在 `TcpListener::bind`，不是行为断言失败）；随后按要求在非隔离环境完整重跑，routed 12/12、sentinel 7/7 非 ignored、connection manager 28/28 均通过。

## Fix round 1

### Review 问题与 RED → GREEN

1. 稳定响应超时接口
   - RED：新增调用测试后编译报错，`RoutedConnection` 没有 `set_response_timeout`。
   - GREEN：公开 `RoutedConnection::set_response_timeout(Duration)`；Standalone 委托 managed multiplexed connection，真实阻塞 `BLPOP` 在设置的 50 ms 后返回超时。redis-rs 1.5 的异步 `ClusterConnection` 没有 post-connect setter，因此 Cluster 分支是有注释的 no-op，并继续使用唯一构建入口上的 3 秒 response timeout；调用后 PING 与 db=0 行为不变。Task 5 可直接使用该接口，无需再改 `routed_connection.rs`。
2. custom multiplexed driver 生命周期
   - RED：review 测试首先因 `RoutedConnection` 不可 clone 编译失败，也确认原实现只 `tokio::spawn(driver)` 后丢弃 handle。
   - GREEN：引入 cloneable `ManagedMultiplexedConnection`。custom stream 的所有 clone 共享 `DriverGuard`，最后一个 clone drop 时通过 `AbortHandle` 终止 driver；direct 上游连接也统一装入 wrapper，并保留 redis-rs 自持的 task handle。真实 loopback 测试发出阻塞 `BLPOP`，取消请求任务并释放全部 connection clone 后，服务端在 1 秒内观察到 EOF。`ConnectionLike` 单命令、非零 pipeline offset/count、`get_db` 和 timeout setter 均由 wrapper 委托。
3. TLS insecure 语义
   - RED：自签 leaf 未加入 root 时，原 `verify_server_cert=false` 仍返回 `ConnectionFailed`，说明只忽略 hostname、仍要求信任链。
   - GREEN：insecure 不加载或要求 native/custom roots，并跳过链和 hostname 验证；TLS 1.2/1.3 的 `CertificateVerify` 仍调用 `rustls::crypto::verify_tls12_signature` / `verify_tls13_signature`，使用证书公钥验证握手签名。真实 TLS loopback 覆盖未受信自签 DNS/IP 成功、默认 secure 拒绝未受信证书、错误 CA 拒绝、secure 正确 CA 成功及原始 DNS SNI。mTLS cert/key 缺一或两者无效均只返回固定 `AppError::InvalidInput`。
4. MONITOR framing
   - RED：真实字节流 `+embedded\\rcr\\r\\n` 被原 reader 作为合法 payload 返回。
   - GREEN：剥离 RESP 外层后拒绝 payload 中任意裸 CR/LF；覆盖 8 KiB ACK 上限、ACK malformed/bulk/partial EOF、合法 ACK 与事件分片、事件 partial EOF/bulk/malformed/bare CR/bare LF/超 256 KiB。事件错误只产出一次固定 `CommandFailed`，随后 stream 返回 `None` 并 drop socket。
5. 其余 review 覆盖
   - Standalone 与 Cluster 两个变体都用三条 ECHO pipeline 验证 `offset=1, count=1` 只返回中间响应。
   - Cluster mTLS 缺半固定映射 `InvalidInput`，不可用 seed 固定映射 `ConnectionFailed` 且不超过外层 5 秒观察边界；没有暴露 redis/rustls 原始错误或 ACL secret。
   - 两个编译/类型级 unit tests 分别固定 `from_ssh_forward(..., Arc<SshForward>)` 的精确类型，以及 sibling Redis 模块（即 Task 5 所在可见性范围）能够调用 `from_ssh_transport`。未重复依赖真实 sshd；`TunneledClient` 继续持有 `Arc<SshForward>` 的完整生命周期。

### redis-rs 1.5 本地源码决定

- `aio/multiplexed_connection.rs` 明确说明 custom `new_with_config` 返回的 driver 必须由调用方持有并在最后 clone 释放时 abort；上游 `set_task_handle` 是 `pub(crate)`，本项目不能调用，因此使用 managed wrapper，而不是 detached task。
- 同文件只给 `MultiplexedConnection` 提供公开 post-connect `set_response_timeout`；异步 `ClusterConnection` 没有对应方法，所以 Cluster 兼容 setter 只能保持 builder 的 3 秒值。
- `cluster_handling/client.rs` 的 retry 参数字段为私有，但公开 builder 方法 `retries`、`min_retry_wait`、`max_retry_wait` 和 `read_from_replicas` 会写入这些字段；`cluster_handling/async_connection/mod.rs` 会把 builder 的 connect/response timeout 传给每条异步节点连接。继续直接委托这些公开 API，未为测试增加生产 hook。

### Fix round 1 验证

- `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection`
  - 开发阶段曾在沙箱内 22 passed；最终 fresh verification 时沙箱策略转为拒绝 loopback bind，20 个网络测试均在 `TcpListener::bind` 得到 `EPERM`，2 个纯测试通过，不是行为断言失败。随后在非隔离环境重跑：22 passed，0 failed。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib constructor`
  - 2 passed，0 failed；验证 SSH 生产构造入口的精确类型与 sibling 可见性。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib redis::connection_manager::tests`
  - 28 passed，0 failed。
- `cargo test --manifest-path src-tauri/Cargo.toml --test sentinel`
  - 7 passed，0 failed，2 ignored（仍要求本地 redis-server/SSH）。
- `cargo test --manifest-path src-tauri/Cargo.toml --no-run`
  - exit 0，全部 lib/main/integration targets 编译成功，现有 CLI 对 `set_response_timeout(Duration)` 的调用也已编译通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` 与 `git diff --check`
  - 最终 fresh verification 均为 exit 0。

### Fix round 1 concerns

- 编译仍只有既有 `redis/array.rs` 5 个 dead-code warning；部分 integration target 另有既有 `tests/support::invalid_profile` warning，本轮 managed driver 未新增 warning。
- Cluster retry/read-replica 内部状态在 redis-rs 1.5 中私有；按 ruling 只保留本地上游源码委托证据，未增加仅供测试的生产接口。
- 未运行依赖真实 sshd 的两个 ignored Sentinel 测试；本轮按要求以精确 `Arc<SshForward>` 类型测试与 Task 3 已有生命周期测试作为生产分支证据。
