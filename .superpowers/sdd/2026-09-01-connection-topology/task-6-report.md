# Task 6 实施报告

## 状态

已实现 Cluster 全 primary 的有界 SCAN、版本化 opaque cursor、轮转公平、二进制 key 去重、节点部分失败与下页重试，以及复用 active handle 已加载 ACL/TLS/证书的 `ClusterNodeConnectionFactory`。Standalone 数字 cursor 保持 untagged serde 兼容；普通 key 读写和 metadata 继续经 `RoutedConnection::Cluster`，未改变 MOVED/ASK 路由语义。本任务未实现 Task 7 拓扑 IPC/分析，也未修改 Task 8–9 前端。

## RED → GREEN

1. Cursor、轮转、去重与部分失败
   - RED：首次运行 `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan`，编译明确失败于缺失 `scan_cluster`、`ClusterScanBackend`、`ClusterScanNode`、`ClusterScanState`、`NodeScanCursor`，没有测试语法或额外依赖错误。
   - GREEN：实现 `cluster:` + URL-safe no-padding base64 的 version 1 JSON cursor；focused scanner 测试覆盖 generation 和 topology node-set 漂移、未知版本、32 KiB 输入上限、重复/未知/超过 128 节点、并发 8、COUNT 分摊、轮转、二进制 bytes 去重、4 MiB raw batch 上限、完成节点不重扫、失败 cursor=0 仍重试。
2. `ScanCursor` 与节点直连 factory
   - RED：第二轮 focused 编译失败于缺失 `ScanCursor` 与 `ClusterNodeConnectionFactory`。
   - GREEN：`ScanCursor::{Standalone(u64), Cluster(String)}` 使用 untagged serde，旧数字请求/响应不加 tag；factory 捕获 username/password/TLS material，以 3 秒 connect/response timeout 连接传入 announced endpoint，测试真实记录 `AUTH cluster-user cluster-secret`。空、非法或不可达地址只返回固定 `ClusterNodeUnavailable`，没有 seed fallback，也不暴露 redis-rs/raw RESP 文本。
3. cursor=0 完成/待重试歧义裁决
   - `NodeScanCursor` 的 Redis cursor=0 同时可能表示初始、完成，且失败节点必须保留 0 供下页重试。公开 `ClusterScanState` 继续只有 `generation` 与 `nodes`；versioned wire 内增加 bounded `pending_nodes` 与 `next_node`。它们只包含已知 primary ID/数值，不含 endpoint、用户名、密码或证书。
   - decode 严格验证 pending 是已知 primary 的无重复子集、`next_node` 在 node-set 范围内、node-set 与当前 topology 完全相等。测试证明完成 primary 不会在下一页重扫，失败 cursor=0 会重试，10 节点时第一页最多扫描 8 个、下一页从 n8 开始。
4. Cursor 未知字段安全边界
   - RED：构造带额外 `endpoint` 字段的合法 version 1 cursor，旧 serde 默认忽略该字段，测试实际得到 `Ok(ClusterScanState)`。
   - GREEN：versioned wire、`ClusterScanState` 和 `NodeScanCursor` 均使用 `deny_unknown_fields`；同一 cursor 返回固定 `InvalidInput`，避免接受藏有 endpoint/凭据的 payload。
5. 最终页面 4 MiB 上限
   - RED：新增最终 `ScanPage` serialized-size 回归时，编译失败于缺失 `ensure_cluster_scan_page_size`。
   - GREEN：Cluster raw SCAN batch 先按 key bytes 做 checked 4 MiB 累计，metadata 组装后再按实际 JSON 序列化长度做 4 MiB 上限校验；任何单次 Redis SCAN batch 都整批接收和推进，绝不为满足页面大小截断后仍推进 cursor。
6. `connection_endpoint` 记录
   - RED：节点 DTO 测试失败于 factory 缺失 `connection_for_node`。
   - GREEN：只有 announced endpoint 真实连接成功后才写 `ClusterNode.connection_endpoint = Some(announced)`，展示用 `endpoint` 原值不变；非法 `?` endpoint 失败后 `connection_endpoint` 仍为 `None`。

## 设计与边界

- `RedisService::scan_keys` 从一次 `ActiveConnectionSnapshot` 取得 generation、Routed client、profile/target 和 factory；Cluster 分支先经 routed connection 执行 `CLUSTER SHARDS`，只在 unsupported-command 时回退 `CLUSTER NODES`，再筛选全部 primary。
- 节点 fan-out 每页从 `next_node` 轮转选择最多 8 个 unfinished primary，并发创建 direct multiplexed connections；COUNT 使用 `ceil(requested / unfinished)` 且最少 1。成功节点保存 Redis 返回 cursor，返回 0 后移出 pending；失败节点 cursor 不变，响应仅复用 Task 1 `NodeFailure { node_id, code: "CLUSTER_NODE_UNAVAILABLE" }`。
- scanner 以 `Vec<u8>`/`HashSet<Vec<u8>>` 去重迁移槽重复 key；Browser DTO 仍为字符串，因此 service metadata 阶段只接受有效 UTF-8 key，不能把有损转换后的不同二进制 key 错合并。
- key summary 的 `TYPE`/`PTTL`/size/`MEMORY`/`OBJECT` 均继续通过一个 `RoutedConnection` 查询，确保 slot command 由 redis-rs 处理 MOVED/ASK；direct node connection 只用于明确的拓扑 SCAN。
- factory 在 active handle 建立时从已传入的 `ConnectionSecrets` 和经校验 `TlsClientMaterial` 构建一次。fan-out 循环不调用 `SecretStore::read`、profile repository 或 SQL；证书/用户名/密码只保存在内存 factory，不进入 cursor/失败 DTO。
- topology node-set 每页重新发现，decode 要求与 cursor 中完整 node-set 相等；active connection generation 改变后旧 cursor 也会因 generation 不符返回 `InvalidInput`。

## 测试证据

- RED（核心 API）：`cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan`，缺失 scanner/cursor API 编译失败。
- RED（serde/factory）：同命令，缺失 `ScanCursor`/`ClusterNodeConnectionFactory` 编译失败。
- RED（未知字段）：`cargo test ... --test cluster_scan cluster_cursor_rejects_duplicate_or_unknown_pending_nodes_and_bad_rotation_index`，带 endpoint cursor 被错误接受，断言失败。
- RED（页面上限）：`cargo test ... --lib redis::key_ops::tests::cluster_scan_page_rejects_serialized_output_over_four_mibibytes`，缺失 size gate 编译失败。
- RED（成功地址）：`cargo test ... --test cluster_scan node_factory_records_connection_endpoint_only_after_a_successful_dial`，缺失 `connection_for_node` 编译失败。
- GREEN focused（非隔离 loopback）：`cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan --test redis_integration`：Cluster 10/10 passed；Redis integration 8 ignored（需要外部 Redis/Redis Stack）。
- GREEN related：`cargo test --manifest-path src-tauri/Cargo.toml --test domain --test redis_integration`：domain 43/43 passed；环境型 Redis integration 保持 ignored。
- GREEN full：`npm run test:rust`：exit 0；lib 179 passed、1 ignored；全部常规 integration targets 通过，外部 Redis/Redis Stack/sshd 用例维持既有 ignored。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`：exit 0。
- `git diff --check`：exit 0。

## 审计与不确定项

- `rg` 审计确认 Cluster scanner/factory 不包含 `SecretStore::read`、repository load 或 SQL；唯一 secret read 仍在连接快照建立路径，位于任何 node fan-out 之前。
- scanner/raw factory 的确定性测试使用本机 loopback；受限沙箱内 `TcpListener::bind` 返回 `EPERM`，同一 focused/full 组合已在允许 loopback 的非隔离环境通过。
- 当前没有 Task 10 的真实三节点 Cluster fixture，因此真实跨 primary key 放置测试尚未执行；本任务由确定性 10-node fake backend、真实 RESP ACL socket 和既有 redis-rs Cluster routing tests 覆盖。Task 10 仍负责真实 Cluster 环境验证。
- Redis key 本身允许任意 bytes，而现有 `KeySummary.key` 是 `String`。本任务在 scanner 层按原始 bytes 正确去重；无法无损表达为现有 IPC 字符串的 key 固定返回 `InvalidInput`，不做 lossy 合并或泄漏。若未来要展示任意二进制 key，需要单独扩展跨 Rust/TypeScript DTO，超出 Task 6 合同。
- 编译仍只有既有 `redis/array.rs` 5 个 dead-code warning 和部分测试 fixture warning；本任务未新增生产 warning。

## Fix round 1

### Review 裁决与修复

本轮按 `progress.md` 的 review/ruling 仅修复三个 Task 6 问题，未增加 Task 7 IPC/分析或 Task 8–9 DTO/前端改动：

1. 游标调度顺序固定为 cursor 内持久化的 `wire.nodes` 顺序。当前页重新发现的 topology 仅建立 `node_id → 最新 announced endpoint` 映射；每次选择同时保留持久化下标，`next_node` 精确更新为最后一个实际选中节点的下一持久化位置。因此同一 node-set 在 SHARDS/NODES 返回顺序变化后不会跳过节点，仍会使用该节点的最新 announced endpoint。
2. 现有 `KeySummary.key: String` 不变。生产 `RedisClusterScanBackend` 在把一次 Redis SCAN batch 交给聚合器前先验证整批 key 均为无损 UTF-8；任一 key 不可显示时，整批返回固定 `ClusterNodeUnavailable`。聚合器因此把该节点写成复用 Task 1 的 `NodeFailure`、保留旧 cursor 并保持 pending，其他节点仍成功；raw scanner 的 bytes 去重逻辑未改变。
3. `ClusterNodeConnectionFactory` 新增 handle-scoped、`Arc<RwLock<...>>` clone 共享的成功 endpoint 表，按 `node_id` 索引。生产 backend 必须通过 `connection_for_node_id` 拨号，只有 direct connection 成功后才记录 exact announced endpoint。后续同节点失败拨号不会覆盖上次成功记录，未知/失败节点不会伪造记录；不存在 seed fallback。公开 `connection(endpoint)` 合同与原有 routed key-slot 行为保持不变。

### RED → GREEN

1. 持久化轮转顺序
   - RED：`cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan scan_is_bounded_to_eight_concurrent_nodes_and_rotates_fairly`。第二页把相同 node-set 从 `n0..n9` 重排为 `n1..n9,n0` 后，实际首个节点为 `n9`，断言期望 `n8`，稳定失败；这证明旧实现错误地用新 topology 顺序解释持久化 `next_node`。
   - GREEN：`round_robin_pending` 改为返回 `(wire_index, node_id)`；第二页首个节点为 `n8`，并断言实际 backend 收到的是当前 topology 更新后的 `n8:9999` endpoint。focused test 1/1 passed。
2. 生产二进制 key 批次
   - RED：`cargo test --manifest-path src-tauri/Cargo.toml --lib production_cluster_backend_keeps_cursor_when_a_node_returns_binary_keys -- --nocapture`。真实 loopback RESP 节点经生产 `RedisClusterScanBackend` 返回 `[0xff, 0x00, 'k']` 后，页面错误包含 visible 与 binary 两个 key，断言只允许 visible key 失败。
   - GREEN：生产 backend 在返回成功 tuple 前验证整批 UTF-8；回归 1/1 passed。visible 节点推进到 17，binary 节点生成 `CLUSTER_NODE_UNAVAILABLE` 且 cursor 从 41 原样保留，页面可重试，没有全局 `InvalidInput` 或 raw RESP 泄漏。
3. 生产成功 endpoint 的共享可观察状态
   - RED：`cargo test --manifest-path src-tauri/Cargo.toml --lib production_cluster_backend_records_only_successful_endpoints_in_shared_factory -- --nocapture`。测试因 `ClusterNodeConnectionFactory::connection_endpoint` 缺失而编译失败，证明生产调用后没有 handle/factory clone 可观察记录。
   - GREEN：生产 backend 改走 `connection_for_node_id`；回归 1/1 passed。原始 factory clone 可读取成功 endpoint，随后同 node ID 的非法 `?` announced endpoint 只产生节点失败且不覆盖旧记录，未知节点保持 `None`。

### 回归与审计证据

- Focused + related：`cargo test --manifest-path src-tauri/Cargo.toml --lib --test cluster_scan --test domain --test redis_integration`：lib 181 passed / 1 ignored；Cluster SCAN 10/10；domain 43/43；Redis integration 8 个外部环境用例保持 ignored。
- Full：`npm run test:rust`：exit 0；lib 181 passed / 1 ignored，所有常规 integration targets 通过，外部 Redis/Redis Stack/sshd 用例保持既有 ignored。
- Format/diff：`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` 与 `git diff --check` 均 exit 0。
- Fan-out 审计：`RedisService::scan_keys` 只从一次 active snapshot 取得已经构建的 factory；`RedisClusterScanBackend::scan_node` 与 `ClusterNodeConnectionFactory` 不引用 `SecretStore`、profile repository 或 SQL。节点选择/并发循环只使用 factory 已捕获的用户名、密码、TLS/证书和固定 timeout，以及当前 primary 的 node ID/announced endpoint。
- 本轮未发现新合同冲突。初始报告中“二进制 key 固定返回全局 `InvalidInput`”是不再成立的旧行为，已由本轮 ruling 覆盖：现在是节点级固定失败并保留可重试 cursor。
