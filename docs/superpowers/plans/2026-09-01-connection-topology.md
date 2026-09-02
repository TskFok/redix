# 连接与拓扑实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不破坏现有 Standalone/Sentinel 功能的前提下，实现 Redis Cluster、跨节点扫描/实例详情/分析、三平台 SSH 认证与合法 TLS/SSH/Sentinel 组合。

**Architecture:** profile 继续兼容现有可选 Sentinel JSON，新增可选 Cluster 配置；运行时统一转换为 `ConnectionTarget`。普通命令通过内部 `RoutedClient`/`RoutedConnection` 复用 redis crate 的 MOVED/ASK 与 slot 刷新，全拓扑操作显式使用 primary 节点集合和 `NodeScope`。SSH 改为应用自持 listener 与 `ssh2` direct-tcpip 转发，凭据只进系统安全存储。

**Tech Stack:** Rust 2021、Tauri 2、redis 1.5 `cluster-async`、Tokio、ssh2 0.9.5、React 19、TypeScript 5.8、Vitest、Python 3、GitHub Actions。

**Spec:** `docs/superpowers/specs/2026-09-01-redisinsight-local-full-parity-design.md`

## Global Constraints

- 永久排除 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics 和远程插件。
- Cluster 只允许数据库 0；不把随机节点结果伪装成全拓扑结果。
- Standalone TCP/TLS 与 Sentinel TCP/TLS 可与 SSH 组合；Sentinel 的种子节点和发现出的 primary 都必须走同一条已认证 SSH 会话。Cluster + SSH 在参考产品也没有可证明的稳定多节点地址改写语义，本批次稳定返回 `UNSUPPORTED_FEATURE`，不得静默直连。
- Cluster seed 1–32 个，拓扑节点最多 128 个，slot 固定为 0–16383，重试与响应必须有上限。
- `CLUSTER SHARDS` 优先，`CLUSTER NODES` 回退；展示 endpoint 与实际连接 endpoint 分开保存。
- 所有凭据和 SSH 私钥/口令只进 `ConnectionSecrets`，普通导出仅包含非敏感认证元数据。
- 不使用 `KEYS`，不引入 SQL，不在循环遍历中查询 SQL。
- macOS、Windows、Linux 均编译和测试；当前机器只声明真实完成的 macOS 结果。
- 默认当前 `main` 分支，简体中文提交；参考项目只读。

## File Map

| 文件 | 职责 |
|---|---|
| `src-tauri/src/domain/topology.rs` | `ConnectionTarget`、Cluster profile、节点作用域和拓扑 DTO |
| `src-tauri/src/domain/profile.rs` | profile 兼容字段、SSH 认证元数据和组合校验 |
| `src-tauri/src/domain/connection_transfer.rs` | Cluster/SSH 非敏感导入导出与版本兼容 |
| `src-tauri/src/persistence/secret_store.rs` | SSH password/private key/passphrase 安全存储 |
| `src-tauri/src/redis/cluster_topology.rs` | CLUSTER INFO/SHARDS/NODES 和节点 INFO 解析 |
| `src-tauri/src/redis/ssh.rs` | 三平台 ssh2 会话、known_hosts、认证和按 endpoint 创建的 direct-tcpip proxy |
| `src-tauri/src/redis/standalone_transport.rs` | Direct/SSH TCP 与可选 TLS 的 async stream、原目标 SNI 和 MultiplexedConnection |
| `src-tauri/src/redis/monitor_transport.rs` | Direct/SSH/TLS 的有界 MONITOR 握手和逐行解码 |
| `src-tauri/src/redis/routed_connection.rs` | Standalone/Cluster client 与 ConnectionLike 适配 |
| `src-tauri/src/redis/cluster_node_connection.rs` | 使用 Cluster profile 的认证/TLS/超时创建指定节点连接 |
| `src-tauri/src/redis/cluster_scan.rs` | 有界多 primary SCAN、opaque cursor、去重与部分失败 |
| `src-tauri/src/redis/connection_manager.rs` | 连接代次、target 构建、服务集成和现有操作路由 |
| `src-tauri/src/commands/topology.rs` | Cluster topology/refresh typed commands |
| `src/lib/types.ts`、`src/lib/tauri.ts` | TypeScript 合同和 typed wrapper |
| `src/features/connections/*` | Cluster/SSH/TLS 表单、兼容编辑和错误提示 |
| `src/features/topology/*` | Cluster 摘要、节点表、刷新和部分失败 UI |
| `scripts/test-local-cluster.py` | 隔离三节点 Cluster 集成环境 |
| `.github/workflows/cross-platform.yml` | macOS/Windows/Linux 编译测试与无签名打包冒烟 |

---

### Task 1: Profile、拓扑目标与安全秘密合同

**Files:**
- Create: `src-tauri/src/domain/topology.rs`
- Modify: `src-tauri/src/domain/profile.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/domain/connection_transfer.rs`
- Modify: `src-tauri/src/persistence/secret_store.rs`
- Modify: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/domain.rs`
- Test: `src-tauri/tests/sentinel.rs`
- Test: `src-tauri/tests/persistence.rs`

**Interfaces:**
- Produces: `ConnectionTarget::try_from(&ConnectionProfile) -> Result<ConnectionTarget, AppError>`
- Produces: `ClusterConfig { nodes, read_from_replicas }`
- Produces: `NodeScope::{Routed, PrimaryNodes, AllNodes, Node(String)}`
- Produces: `NodeFailure { node_id, code }`
- Produces: `SshAuthMethod::{Agent, Password, PrivateKey}`, path-free `SshConfig` IPC and extended `ConnectionSecrets`

- [ ] **Step 1: Write failing profile, migration, combination and secret tests**

```rust
#[test]
fn cluster_profile_is_db_zero_and_cannot_silently_mix_topologies() {
    let mut profile = valid_profile();
    profile.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint { host: "127.0.0.1".into(), port: 7000 }],
        read_from_replicas: false,
    });
    assert_eq!(ConnectionTarget::try_from(&profile).unwrap().kind(), "cluster");
    profile.database = 1;
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));
    profile.database = 0;
    profile.sentinel = Some(valid_sentinel());
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));
}

#[test]
fn ssh_secrets_round_trip_without_entering_export() {
    let secrets = ConnectionSecrets {
        ssh_password: Some("ssh-secret".into()),
        ssh_private_key: Some("PRIVATE".into()),
        ssh_passphrase: Some("passphrase".into()),
        ssh_identity_file: Some("/private/key".into()),
        ssh_known_hosts_file: Some("/private/known_hosts".into()),
        ..Default::default()
    };
    let encoded = serde_json::to_string(&secrets).unwrap();
    assert_eq!(decode_stored_secret(&encoded).unwrap(), secrets);
    let exported = serde_json::to_string(&ConnectionExportDocument::from_profiles(&[ssh_profile()])).unwrap();
    assert!(!exported.contains("ssh-secret"));
    assert!(!exported.contains("PRIVATE"));
    assert!(!exported.contains("/private/"));
}

#[test]
fn export_v2_preserves_cluster_but_import_still_accepts_v1() {
    let exported = ConnectionExportDocument::from_profiles(&[cluster_profile()]);
    assert_eq!(exported.version, 2);
    assert!(exported.connections[0].cluster.is_some());
    assert!(normalize_import_document(legacy_v1_document()).is_ok());
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain --test sentinel --test persistence`

Expected: FAIL because `ClusterConfig`, `ConnectionTarget`, `SshAuthMethod` and SSH secret fields do not exist.

- [ ] **Step 3: Implement the exact domain contracts and backward-compatible defaults**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterConfig {
    pub nodes: Vec<ConnectionEndpoint>,
    #[serde(default)]
    pub read_from_replicas: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionTarget {
    Standalone,
    Sentinel(SentinelConfig),
    Cluster(ClusterConfig),
}

impl ConnectionTarget {
    pub fn kind(&self) -> &'static str {
        match self { Self::Standalone => "standalone", Self::Sentinel(_) => "sentinel", Self::Cluster(_) => "cluster" }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod { #[default] Agent, Password, PrivateKey }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth_method: SshAuthMethod,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub has_private_key: bool,
    #[serde(default)]
    pub has_passphrase: bool,
    #[serde(default)]
    pub has_identity_file: bool,
    #[serde(default)]
    pub has_known_hosts_file: bool,
    #[serde(default, rename = "identity_file", skip_serializing)]
    pub(crate) legacy_identity_file: Option<String>,
    #[serde(default, rename = "known_hosts_file", skip_serializing)]
    pub(crate) legacy_known_hosts_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeScope { Routed, PrimaryNodes, AllNodes, Node(String) }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeFailure { pub node_id: String, pub code: String }
```

Add `cluster: Option<ClusterConfig>` to `ConnectionProfile`. For a Cluster profile, keep the required legacy `host`/`port` fields equal to the first seed for old list/sort code, while `ConnectionTarget` and all Redis operations use `cluster.nodes`; validation rejects mismatches so the two representations cannot drift. `ConnectionTarget::try_from` must reject simultaneous Sentinel/Cluster, Cluster database != 0, empty/duplicate/invalid seeds, more than 32 seeds, and Cluster + SSH. Sentinel + SSH remains valid and every discovery connection must use the SSH transport added in Tasks 3–5.

Bump `CONNECTION_EXPORT_VERSION` to 2, add optional non-secret Cluster data and a dedicated path-free `SshExportConfig` to `ConnectionExportProfile`/normalized imports, and accept both versions 1 and 2. Version 1 without Cluster remains Standalone/Sentinel; unknown versions remain rejected. Extend `ConnectionSecrets`, known-key decoding and `is_empty()` with SSH password/private key/passphrase plus identity/known_hosts paths. At startup, idempotently copy legacy `identity_file`/`known_hosts_file` values into the secret store before rewriting the profile; copy-first means an interrupted migration never loses the path. New profile JSON, IPC, logs and exports expose only `has_*` booleans. Add fixed errors `ClusterTopologyFailed`, `ClusterNodeUnavailable`, `PartialFailure`, `CrossSlot` with stable codes/messages.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain --test sentinel --test persistence`

Expected: PASS; legacy profiles without `cluster` or `auth_method` still deserialize as Standalone/Agent.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain src-tauri/src/persistence/secret_store.rs src-tauri/src/error.rs src-tauri/src/lib.rs src-tauri/tests/domain.rs src-tauri/tests/sentinel.rs src-tauri/tests/persistence.rs
git commit -m "增加连接拓扑与 SSH 安全合同"
```

### Task 2: Cluster topology DTO 与 RESP2/RESP3 parser

**Files:**
- Create: `src-tauri/src/redis/cluster_topology.rs`
- Modify: `src-tauri/src/redis/mod.rs`
- Modify: `src-tauri/src/domain/topology.rs`
- Test: `src-tauri/tests/cluster_topology.rs`

**Interfaces:**
- Consumes: `ClusterConfig`, `ConnectionEndpoint`
- Produces: `parse_cluster_info(&str) -> Result<ClusterSummary, AppError>`
- Produces: `parse_cluster_shards(Value) -> Result<Vec<ClusterNode>, AppError>`
- Produces: `parse_cluster_shards_for_tls(Value, bool) -> Result<Vec<ClusterNode>, AppError>`
- Produces: `parse_cluster_nodes(&str) -> Result<Vec<ClusterNode>, AppError>`
- Produces: `merge_node_metrics(&mut ClusterNode, &str) -> Result<(), AppError>`

- [ ] **Step 1: Write failing fixtures for SHARDS, NODES, IPv6, TLS port and partial INFO**

```rust
#[test]
fn shards_prefers_announced_endpoint_and_preserves_tls_port() {
    let reply = redis::Value::Array(vec![redis::Value::Array(vec![
        bulk("slots"), redis::Value::Array(vec![0.into(), 16383.into()]),
        bulk("nodes"), redis::Value::Array(vec![redis::Value::Array(vec![
            bulk("id"), bulk("node-1"), bulk("role"), bulk("master"),
            bulk("endpoint"), bulk("cache.internal"), bulk("ip"), bulk("10.0.0.2"),
            bulk("port"), 0.into(), bulk("tls-port"), 6380.into(), bulk("health"), bulk("online"),
        ])]),
    ])]);
    let nodes = parse_cluster_shards(reply).unwrap();
    assert_eq!(nodes[0].endpoint.host, "cache.internal");
    assert_eq!(nodes[0].endpoint.port, 6380);
    assert_eq!(nodes[0].slots, vec![SlotRange { start: 0, end: 16383 }]);
}

#[test]
fn nodes_parser_accepts_bracketed_ipv6_and_marks_failures() {
    let nodes = parse_cluster_nodes("id1 [::1]:7000@17000 master,fail - 0 0 1 connected 0-100\n").unwrap();
    assert_eq!(nodes[0].health, ClusterNodeHealth::Offline);
}
```

- [ ] **Step 2: Run parser test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_topology`

Expected: FAIL because parser module and DTO do not exist.

- [ ] **Step 3: Implement bounded parser and DTO**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterSummary {
    pub state: String,
    pub slots_assigned: u16,
    pub slots_ok: u16,
    pub slots_pfail: u16,
    pub slots_fail: u16,
    pub current_epoch: u64,
    pub size: u16,
    pub known_nodes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotRange { pub start: u16, pub end: u16 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClusterNodeRole { Primary, Replica }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClusterNodeHealth { Online, Offline, Loading }

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClusterNodeMetrics {
    pub used_memory_bytes: Option<u64>,
    pub ops_per_second: Option<u64>,
    pub connections_received: Option<u64>,
    pub connected_clients: Option<u64>,
    pub commands_processed: Option<u64>,
    pub network_in_kbps: Option<f64>,
    pub network_out_kbps: Option<f64>,
    pub cache_hit_ratio: Option<f64>,
    pub replication_offset: Option<u64>,
    pub replication_lag: Option<u64>,
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterNode {
    pub id: String,
    pub endpoint: ConnectionEndpoint,
    pub connection_endpoint: Option<ConnectionEndpoint>,
    pub role: ClusterNodeRole,
    pub health: ClusterNodeHealth,
    pub primary_id: Option<String>,
    pub slots: Vec<SlotRange>,
    pub metrics: ClusterNodeMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterTopology {
    pub summary: ClusterSummary,
    pub nodes: Vec<ClusterNode>,
    pub failures: Vec<NodeFailure>,
}
```

Limit nodes to 128, slot ranges to 16,384, ids/hosts to 256 bytes and the decoded topology structural envelope to 4 MiB. The parser receives an already-normalized `redis::Value`, so it must not claim to recover the original RESP wire length. Prefer SHARDS; only fall back to NODES when SHARDS returns unsupported-command behavior. Keep `announced_endpoint` separate from an optional `connection_endpoint` chosen later by the service. The default parser chooses the non-TLS port; the TLS-aware entry point chooses `tls-port`, with the other advertised port used only as a fallback.

- [ ] **Step 4: Run parser and domain tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_topology --test domain`

Expected: PASS for RESP2 arrays, RESP3 maps/attributes, NODES fallback, malformed bounds and partial INFO.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/topology.rs src-tauri/src/redis/cluster_topology.rs src-tauri/src/redis/mod.rs src-tauri/tests/cluster_topology.rs
git commit -m "实现 Cluster 拓扑解析与节点模型"
```

### Task 3: 三平台 SSH 会话、认证和隧道所有权

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock` through Cargo only
- Rewrite: `src-tauri/src/redis/ssh.rs`
- Modify: `src-tauri/src/commands/connections.rs`
- Test: `src-tauri/tests/ssh.rs`
- Test: `src-tauri/src/redis/ssh.rs`

**Interfaces:**
- Consumes: `SshConfig`, `SshAuthMethod`, SSH fields in `ConnectionSecrets`
- Produces: `SshTransport::connect(config: &SshConfig, secrets: &ConnectionSecrets) -> Result<SshTransport, AppError>`
- Produces: `SshTransport::forward(endpoint: &ConnectionEndpoint) -> Result<Arc<SshForward>, AppError>`
- Produces: owned `SshForward::local_endpoint() -> ConnectionEndpoint`; Drop aborts the accept task, closes proxy sockets and disconnects the SSH session when its final transport owner is released

- [ ] **Step 1: Add failing unit tests for host key policy, auth selection and owned listener cleanup**

```rust
#[test]
fn unknown_or_changed_host_key_is_never_accepted() {
    assert_eq!(verify_host_key(CheckResult::NotFound), Err(AppError::SshTunnelFailed));
    assert_eq!(verify_host_key(CheckResult::Mismatch), Err(AppError::SshTunnelFailed));
}

#[tokio::test]
async fn dropping_forward_closes_the_app_owned_listener() {
    let transport = test_backend_transport().await.unwrap();
    let forward = transport.forward(&endpoint("redis.internal", 6379)).await.unwrap();
    let endpoint = forward.local_endpoint();
    drop(forward);
    assert!(tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await.is_err());
}

#[tokio::test]
async fn one_authenticated_session_forwards_sentinel_seed_and_primary() {
    let transport = test_backend_transport().await.unwrap();
    let seed = transport.forward(&endpoint("sentinel.internal", 26379)).await.unwrap();
    let primary = transport.forward(&endpoint("redis.internal", 6379)).await.unwrap();
    assert_ne!(seed.local_endpoint(), primary.local_endpoint());
    assert_eq!(test_backend().authenticated_session_count(), 1);
}
```

- [ ] **Step 2: Run SSH tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ssh --lib redis::ssh::tests`

Expected: FAIL because ssh2 backend, host-key verifier and secret-aware start signature do not exist.

- [ ] **Step 3: Add ssh2 with vendored OpenSSL and implement the owned proxy**

Run: `cargo add ssh2@0.9.5 --features vendored-openssl --manifest-path src-tauri/Cargo.toml`

```rust
pub(super) struct SshForward {
    endpoint: ConnectionEndpoint,
    cancel: tokio::sync::watch::Sender<bool>,
    accept_task: tokio::task::JoinHandle<()>,
    _session: Arc<ssh2::Session>,
}

#[derive(Clone)]
pub(super) struct SshTransport {
    session: Arc<ssh2::Session>,
}

impl SshTransport {
    pub async fn forward(&self, target: &ConnectionEndpoint) -> Result<Arc<SshForward>, AppError> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await
            .map_err(|_| AppError::SshTunnelFailed)?;
        let endpoint = ConnectionEndpoint { host: "127.0.0.1".into(), port: listener.local_addr().map_err(|_| AppError::SshTunnelFailed)?.port() };
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let accept_task = spawn_direct_tcpip_proxy(listener, self.session.clone(), target.clone(), receiver);
        Ok(Arc::new(SshForward { endpoint, cancel, accept_task, _session: self.session.clone() }))
    }
}
```

`SshTransport::connect` authenticates once; every forward binds its listener before publishing the endpoint and every accepted local socket maps to `channel_direct_tcpip` for that forward's original target. Blocking libssh2 reads/writes run only in bounded `spawn_blocking` proxy workers. Dropping a forward aborts its accept task and closes active local sockets; dropping the final transport disconnects the session. Password/private-key/passphrase and identity/known_hosts paths come only from secrets. known_hosts must use that secret path or the platform user's standard OpenSSH file and reject missing/mismatch; no trust-on-first-use.

- [ ] **Step 4: Run unit and ignored real-sshd tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ssh --lib redis::ssh::tests`

Run when local sshd tools are available: `cargo test --manifest-path src-tauri/Cargo.toml --test ssh -- --ignored --nocapture --test-threads=1`

Expected: unit tests PASS on all platforms; real test proves agent/password/private-key auth, strict host-key rejection, forwarding and listener cleanup.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/redis/ssh.rs src-tauri/src/commands/connections.rs src-tauri/tests/ssh.rs
git commit -m "实现跨平台 SSH 安全隧道"
```

### Task 4: RoutedClient、ClusterClient 与通用 ConnectionLike

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock` through Cargo only
- Create: `src-tauri/src/redis/standalone_transport.rs`
- Create: `src-tauri/src/redis/routed_connection.rs`
- Create: `src-tauri/src/redis/monitor_transport.rs`
- Modify: `src-tauri/src/redis/mod.rs`
- Test: `src-tauri/tests/routed_connection.rs`

**Interfaces:**
- Consumes: `ConnectionTarget`, TLS material, optional `SshTransport`/`SshForward`
- Produces: `StandaloneClient::{Direct(Client), Tunneled(TunneledClient)}`
- Produces: `RoutedClient::{Standalone(StandaloneClient), Cluster(ClusterConnection)}`
- Produces: `RoutedConnection::{Standalone(MultiplexedConnection), Cluster(ClusterConnection)}` implementing `redis::aio::ConnectionLike`
- Produces: `RoutedClient::connection()`, `standalone_client()`, `topology_kind()`
- Produces: `StandaloneClient::pubsub()` and `StandaloneClient::monitor_stream()` for Direct/SSH/TLS transports
- Produces: `type MonitorLineStream = Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>>`

- [ ] **Step 1: Write failing delegation, database and capability tests**

```rust
#[test]
fn routed_cluster_connection_is_always_db_zero() {
    let connection = RoutedConnection::Cluster(fake_cluster_connection());
    assert_eq!(redis::aio::ConnectionLike::get_db(&connection), 0);
}

#[test]
fn standalone_only_socket_is_not_exposed_for_cluster() {
    assert_eq!(cluster_client().standalone_client(), Err(AppError::UnsupportedFeature));
}

#[tokio::test]
async fn tunneled_tls_dials_local_endpoint_but_verifies_original_server_name() {
    let connector = recording_tls_connector();
    tunneled_client("cache.internal", endpoint("127.0.0.1", 45001), connector.clone())
        .connection().await.unwrap();
    assert_eq!(connector.dialed_endpoint(), endpoint("127.0.0.1", 45001));
    assert_eq!(connector.server_name(), "cache.internal");
}

#[tokio::test]
async fn tunneled_pubsub_and_monitor_authenticate_on_custom_streams() {
    let client = fake_tunneled_client_with_password();
    client.pubsub().await.unwrap();
    let mut monitor = client.monitor_stream().await.unwrap();
    assert_eq!(monitor.next().await.unwrap().unwrap(), "1.0 [0 local] \"PING\"");
    assert_eq!(fake_server().observed_sni(), vec!["cache.internal", "cache.internal"]);
}
```

- [ ] **Step 2: Enable async Cluster and verify RED**

Modify redis features to include `cluster-async`. Add exact direct dependencies `rustls = "0.23.42"`, `tokio-rustls = "0.26.4"`, `rustls-native-certs = "0.8.3"` and `rustls-pemfile = "2"` used by the custom tunneled stream, then run:

`cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection`

Expected: FAIL until routed enums and ConnectionLike delegation exist.

- [ ] **Step 3: Implement the adapter and bounded Cluster builder**

```rust
#[derive(Clone)]
pub enum StandaloneClient {
    Direct(redis::Client),
    Tunneled(TunneledClient),
}

#[derive(Clone)]
pub struct TunneledClient {
    pub redis_info: redis::RedisConnectionInfo,
    pub original_endpoint: ConnectionEndpoint,
    pub local_endpoint: ConnectionEndpoint,
    pub tls: Option<TlsClientMaterial>,
    pub _forward: Arc<SshForward>,
}

#[derive(Clone)]
pub enum RoutedClient {
    Standalone(StandaloneClient),
    Cluster(redis::cluster_async::ClusterConnection),
}

pub enum RoutedConnection {
    Standalone(redis::aio::MultiplexedConnection),
    Cluster(redis::cluster_async::ClusterConnection),
}

impl redis::aio::ConnectionLike for RoutedConnection {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> redis::RedisFuture<'a, redis::Value> {
        match self { Self::Standalone(c) => c.req_packed_command(cmd), Self::Cluster(c) => c.req_packed_command(cmd) }
    }
    fn req_packed_commands<'a>(&'a mut self, pipe: &'a redis::Pipeline, offset: usize, count: usize) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self { Self::Standalone(c) => c.req_packed_commands(pipe, offset, count), Self::Cluster(c) => c.req_packed_commands(pipe, offset, count) }
    }
    fn get_db(&self) -> i64 { match self { Self::Standalone(c) => c.get_db(), Self::Cluster(c) => c.get_db() } }
}
```

`TunneledClient::connection` must dial `local_endpoint`, wrap the stream with rustls using `original_endpoint.host` as `ServerName`, pass the original Redis auth/DB `RedisConnectionInfo` to `MultiplexedConnection::new_with_config`, spawn the returned driver future, and return the connection. `pubsub()` opens the same Direct/SSH/TLS stream and calls public `redis::aio::PubSub::new(&redis_info, stream)`. `monitor_stream()` normalizes both direct `Monitor::into_on_message::<String>()` and the custom transport into `MonitorLineStream`; the custom branch uses `monitor_transport.rs` to send packed AUTH (username when present), SELECT and MONITOR commands, requires bounded `+OK` replies, then reads only RESP simple-string monitor lines with a 256 KiB per-line limit. Malformed, bulk or oversized replies emit one fixed error and close the stream. Build Cluster with 3-second connection/response timeouts, 3 retries, 250 ms min and 2 s max retry waits, shared username/password, TLS/certificates and `read_from_replicas`. Store the opened cloneable `ClusterConnection` in `RoutedClient::Cluster`; do not recreate it per operation and do not expose raw Redis errors.

- [ ] **Step 4: Run routed and existing connection tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection --test sentinel --lib redis::connection_manager::tests`

Expected: PASS; existing Standalone/TLS/Sentinel URL and generation tests remain green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/redis/standalone_transport.rs src-tauri/src/redis/routed_connection.rs src-tauri/src/redis/monitor_transport.rs src-tauri/src/redis/mod.rs src-tauri/tests/routed_connection.rs
git commit -m "接入异步 Cluster 路由连接"
```

### Task 5: ConnectionManager 生命周期与现有命令兼容

**Files:**
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/redis/observability.rs`
- Modify: `src-tauri/src/redis/cli.rs`
- Modify: `src-tauri/src/redis/json_ops.rs`
- Modify: `src-tauri/src/redis/database_analysis.rs`
- Modify: `src-tauri/src/domain/profile.rs`
- Test: `src-tauri/tests/commands.rs`
- Test: `src-tauri/tests/database_analysis.rs`
- Test: `src-tauri/tests/routed_connection.rs`

**Interfaces:**
- Consumes: `RoutedClient`, `RoutedConnection`, `ConnectionTarget`
- Produces: existing `RedisOperations` signatures remain stable except topology-aware DTO additions
- Produces: `RedisService::routed_connection`, `standalone_client`, `connection_target`
- Refactors: `PubSubManager::start(app, pubsub, input)` and `ProfilerManager::start(app, monitor_stream, input)` so custom SSH/TLS streams never fall back to direct Redis

- [ ] **Step 1: Write failing lifecycle and no-silent-downgrade tests**

```rust
#[tokio::test]
async fn failed_cluster_open_never_publishes_a_standalone_handle() {
    let service = service_with_profile(cluster_profile(vec![endpoint(1)]));
    assert_eq!(service.open_connection("cluster").await, Err(AppError::ConnectionFailed));
    assert_eq!(service.execute_command("cluster", "PING").await, Err(AppError::ConnectionFailed));
}

#[tokio::test]
async fn pubsub_and_profiler_reject_cluster_without_random_node_fallback() {
    let service = service_with_open_cluster();
    assert_eq!(service.start_pub_sub(app(), pubsub_input("cluster")).await, Err(AppError::UnsupportedFeature));
}

#[tokio::test]
async fn sentinel_over_ssh_forwards_discovery_and_primary_connections() {
    let service = service_with_sentinel_ssh_profile();
    service.open_connection("sentinel-ssh").await.unwrap();
    assert_eq!(service.execute_command("sentinel-ssh", "PING").await.unwrap().value, serde_json::json!("PONG"));
    assert_eq!(fake_ssh_backend().direct_targets(), vec![endpoint("sentinel.internal", 26379), endpoint("primary.internal", 6379)]);
}

#[tokio::test]
async fn existing_browser_json_workbench_and_cli_paths_use_cluster_routing() {
    let service = service_with_open_cluster();
    service.execute_command("cluster", "SET {user:1}:name Ada").await.unwrap();
    assert_eq!(service.get_key("cluster", "{user:1}:name").await.unwrap().key_type, "string");
    assert_eq!(service.execute_command("cluster", "GET {user:1}:name").await.unwrap().value, serde_json::json!("Ada"));
    open_and_ping_cli(&service, "cluster").await.unwrap();
}
```

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test routed_connection --test commands`

Expected: FAIL because `ConnectionHandle` still stores a single `Client` and cluster-only capability gates are absent.

- [ ] **Step 3: Replace handle internals without widening public IPC**

```rust
#[derive(Clone)]
struct ConnectionHandle {
    client: RoutedClient,
    profile: ConnectionProfile,
    target: ConnectionTarget,
    _ssh: Option<SshTransport>,
}
```

Preserve generation/profile-transaction lock order. Opening, closing, replacing and selecting DB must cancel stale tasks and capability snapshots. Change `RedisService::connection`, CLI session sockets and all production helper parameters in `connection_manager.rs`, `json_ops.rs` and `database_analysis.rs` from concrete `MultiplexedConnection` to `RoutedConnection`; `RoutedConnection::set_response_timeout` delegates for Standalone and relies on the Cluster builder timeout for Cluster. The production-source acceptance check `rg 'MultiplexedConnection' src-tauri/src/redis` may match only `routed_connection.rs` and transport construction, never an operation helper or CLI socket. Map Redis CROSSSLOT replies to the fixed `CrossSlot` error without copying raw text.

Cluster `select_database` accepts only 0. Sentinel discovery must create a forward for each tried seed, use the original seed hostname for TLS SNI, then create a forward for the discovered primary and publish only that `StandaloneClient`; no discovery or data socket may bypass SSH. Pub/Sub/Profiler ask `StandaloneClient` to construct their ready transport, and their managers consume the constructed `PubSub`/`MonitorLineStream`; Cluster still returns `UNSUPPORTED_FEATURE` instead of choosing a random node.

- [ ] **Step 4: Run the full non-network Rust suite**

Run: `npm run test:rust`

Expected: all regular Rust tests PASS; only documented environment-dependent tests remain ignored.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/observability.rs src-tauri/src/redis/cli.rs src-tauri/src/redis/json_ops.rs src-tauri/src/redis/database_analysis.rs src-tauri/src/domain/profile.rs src-tauri/tests/commands.rs src-tauri/tests/database_analysis.rs src-tauri/tests/routed_connection.rs
git commit -m "统一连接生命周期与拓扑能力门控"
```

### Task 6: 多 primary SCAN、opaque cursor 与部分失败

**Files:**
- Create: `src-tauri/src/redis/cluster_scan.rs`
- Create: `src-tauri/src/redis/cluster_node_connection.rs`
- Modify: `src-tauri/src/domain/key.rs`
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/redis/key_ops.rs`
- Test: `src-tauri/tests/cluster_scan.rs`
- Test: `src-tauri/tests/redis_integration.rs`

**Interfaces:**
- Produces: `ClusterNodeConnectionFactory::connection(endpoint) -> Result<MultiplexedConnection, AppError>` using the Cluster profile's auth/TLS/certificates/timeouts
- Produces: `ScanCursor::{Standalone(u64), Cluster(String)}` with backward-compatible untagged serde
- Produces: `ClusterScanState { generation, nodes: Vec<NodeScanCursor> }`
- Extends: `ScanPage { cursor, keys, node_failures }`

- [ ] **Step 1: Write failing cursor, fairness, dedupe and retry tests**

```rust
#[test]
fn cluster_cursor_round_trips_and_rejects_unknown_or_excess_nodes() {
    let cursor = ClusterScanState::new(7, vec![node_cursor("n1", 12), node_cursor("n2", 0)]).encode().unwrap();
    assert_eq!(ClusterScanState::decode(&cursor, &known_nodes("n1", "n2")).unwrap().generation, 7);
    assert_eq!(ClusterScanState::decode(&cursor, &known_nodes("n1")).unwrap_err(), AppError::InvalidInput);
}

#[tokio::test]
async fn failed_node_keeps_its_cursor_while_successful_nodes_progress() {
    let page = scan_cluster(fake_nodes().with_failure("n2"), initial_cursor(), "*", 100).await.unwrap();
    assert_eq!(page.node_failures[0].node_id, "n2");
    assert!(!page.keys.is_empty());
    assert_eq!(decode(page.cursor).cursor_for("n2"), 0);
}
```

- [ ] **Step 2: Run scan tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan`

Expected: FAIL because cursor codec and multi-node scanner do not exist.

- [ ] **Step 3: Implement bounded primary fan-out**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ScanCursor { Standalone(u64), Cluster(String) }
```

Use the `NodeFailure` defined by Task 1; do not create a second scan-specific failure type.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeScanCursor { pub node_id: String, pub cursor: u64 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterScanState { pub generation: u64, pub nodes: Vec<NodeScanCursor> }
```

Build `ClusterNodeConnectionFactory` once per active handle from already-loaded credentials and TLS material. It must connect to the node's advertised endpoint with the same username/password/certificates and 3-second timeouts; it never reads the secret store inside a node loop. An empty, invalid or unreachable announced endpoint becomes that node's `ClusterNodeUnavailable` failure and is never replaced with an unrelated seed. Record the actual successful address in `connection_endpoint` while preserving the announced address for display.

Encode Cluster cursors as `cluster:` plus URL-safe no-padding base64 of a versioned JSON `ClusterScanState`; reject encoded input over 32 KiB before decoding, unknown versions, duplicate/unknown nodes, more than 128 nodes and generation/topology mismatches. The cursor contains only node IDs and numeric cursors, never endpoints or credentials.

Round-robin primaries with concurrency 8, divide COUNT hint across unfinished nodes, never truncate a Redis SCAN batch while advancing its cursor, dedupe migrating-slot duplicates by key bytes, cap 128 nodes/4 MiB page, retain failed node cursor for retry, and invalidate cursor when connection generation/topology node set changes. Routed key read/write continues through ClusterConnection so key-slot commands follow MOVED/ASK.

- [ ] **Step 4: Run cluster scan and existing Browser tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_scan --test redis_integration`

Expected: regular tests PASS; environment-dependent real Redis flow remains ignored.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/key.rs src-tauri/src/redis/cluster_scan.rs src-tauri/src/redis/cluster_node_connection.rs src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/key_ops.rs src-tauri/tests/cluster_scan.rs src-tauri/tests/redis_integration.rs
git commit -m "实现 Cluster 多节点键扫描"
```

### Task 7: Cluster 拓扑详情、节点指标与跨节点分析

**Files:**
- Modify: `src-tauri/src/domain/topology.rs`
- Modify: `src-tauri/src/domain/database.rs`
- Modify: `src-tauri/src/domain/database_analysis.rs`
- Modify: `src-tauri/src/redis/cluster_topology.rs`
- Modify: `src-tauri/src/redis/cluster_node_connection.rs`
- Modify: `src-tauri/src/redis/database_analysis.rs`
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Create: `src-tauri/src/commands/topology.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/cluster_topology.rs`
- Test: `src-tauri/tests/database_analysis.rs`
- Test: `src-tauri/tests/commands.rs`

**Interfaces:**
- Produces: `get_cluster_topology(connection_id) -> ClusterTopology`
- Produces: `refresh_cluster_topology(connection_id) -> ClusterTopology`
- Extends: analysis with `node_results: Vec<NodeAnalysisResult>` and `failed_nodes: Vec<NodeFailure>`

- [ ] **Step 1: Write failing topology aggregation and partial-analysis tests**

```rust
#[tokio::test]
async fn topology_keeps_healthy_nodes_when_one_info_call_fails() {
    let topology = load_cluster_topology(fake_cluster().fail_info("node-2")).await.unwrap();
    assert_eq!(topology.nodes.len(), 3);
    assert_eq!(topology.failures, vec![NodeFailure { node_id: "node-2".into(), code: "CLUSTER_NODE_UNAVAILABLE".into() }]);
}

#[test]
fn cluster_analysis_total_is_sum_of_successful_primary_nodes() {
    let report = merge_node_reports(vec![ok_report("n1", 10), ok_report("n2", 20)], vec![failure("n3")]);
    assert_eq!(report.total_keys.total, 30);
    assert_eq!(report.failed_nodes.len(), 1);
}
```

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_topology --test database_analysis --test commands`

Expected: FAIL because topology commands and node-aware report fields do not exist.

- [ ] **Step 3: Implement explicit fan-out and typed commands**

Use SHARDS→NODES fallback, query `CLUSTER INFO` once and use the handle's `ClusterNodeConnectionFactory` for bounded `INFO server clients memory stats replication keyspace` on at most 128 nodes with concurrency 8 and per-node 3-second timeout. Credentials/TLS are captured once before fan-out, so no keyring or persistence lookup occurs inside the node loop. Aggregate instance totals only where summing is meaningful; keep rates and per-node metrics in node rows. Divide analysis `max_keys` across primaries, merge accumulators, and return partial results with stable node failures.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeAnalysisResult {
    pub node_id: String,
    pub endpoint: ConnectionEndpoint,
    pub report: Box<DatabaseAnalysisReport>,
}

// Added to DatabaseAnalysisReport for cluster responses; empty for standalone and old histories.
#[serde(default)]
pub node_results: Vec<NodeAnalysisResult>,
#[serde(default)]
pub failed_nodes: Vec<NodeFailure>,

#[tauri::command]
pub async fn get_cluster_topology(state: State<'_, AppState>, connection_id: String) -> Result<ClusterTopology, AppError> {
    state.redis.get_cluster_topology(&connection_id).await
}
```

Every boxed per-node report must have empty `node_results` and `failed_nodes`; only the outer report contains fan-out metadata. This prevents recursive content while reusing the existing analysis fields.

- [ ] **Step 4: Run topology, analysis and command tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_topology --test database_analysis --test commands`

Expected: PASS; Standalone report serialization remains accepted and Cluster partial failures are explicit.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain src-tauri/src/redis src-tauri/src/commands src-tauri/src/lib.rs src-tauri/tests/cluster_topology.rs src-tauri/tests/database_analysis.rs src-tauri/tests/commands.rs
git commit -m "增加 Cluster 拓扑详情与跨节点分析"
```

### Task 8: TypeScript 合同与 typed IPC

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`
- Modify: `src/features/browser/browserState.ts`
- Test: `src/features/browser/browserState.test.ts`

**Interfaces:**
- Mirrors: all Rust topology/profile/scan/failure DTOs exactly in snake_case
- Produces: `getClusterTopology`, `refreshClusterTopology`
- Extends: `ScanKeysInput.cursor` and `ScanPage.cursor` to `number | string`

- [ ] **Step 1: Write failing bridge payload and cursor reducer tests**

```ts
it("invokes cluster topology commands with snake_case connection id", async () => {
  invokeMock.mockResolvedValue(clusterTopology);
  await expect(getClusterTopology("cluster-1")).resolves.toEqual(clusterTopology);
  expect(invokeMock).toHaveBeenCalledWith("get_cluster_topology", { connection_id: "cluster-1" });
});

it("keeps opaque cluster cursor without numeric coercion", () => {
  expect(applyScanPage(initialState, { cursor: "cluster:abc", keys: [], node_failures: [] }).cursor).toBe("cluster:abc");
});
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run: `npm test -- src/lib/tauri.test.ts src/features/browser/browserState.test.ts`

Expected: FAIL because types/wrappers and string cursor handling do not exist.

- [ ] **Step 3: Add exact TS DTOs and wrappers**

```ts
export type ScanCursor = number | string;
export type Workspace = "browser" | "search-query" | "workbench" | "cli" | "database" | "database-analysis" | "observability" | "topology" | "query-library" | "settings";
export type ConnectionTargetKind = "standalone" | "sentinel" | "cluster";
export type ClusterNodeRole = "primary" | "replica";
export type ClusterNodeHealth = "online" | "offline" | "loading";

export interface ClusterConfig {
  nodes: ConnectionEndpoint[];
  read_from_replicas: boolean;
}

export interface NodeFailure { node_id: string; code: string }
export interface SlotRange { start: number; end: number }
export interface ClusterSummary {
  state: string;
  slots_assigned: number;
  slots_ok: number;
  slots_pfail: number;
  slots_fail: number;
  current_epoch: number;
  size: number;
  known_nodes: number;
}
export interface ClusterNodeMetrics {
  used_memory_bytes: number | null;
  ops_per_second: number | null;
  connections_received: number | null;
  connected_clients: number | null;
  commands_processed: number | null;
  network_in_kbps: number | null;
  network_out_kbps: number | null;
  cache_hit_ratio: number | null;
  replication_offset: number | null;
  replication_lag: number | null;
  uptime_seconds: number | null;
}
export interface ClusterNode {
  id: string;
  endpoint: ConnectionEndpoint;
  connection_endpoint: ConnectionEndpoint | null;
  role: ClusterNodeRole;
  health: ClusterNodeHealth;
  primary_id: string | null;
  slots: SlotRange[];
  metrics: ClusterNodeMetrics;
}
export interface ClusterTopology {
  summary: ClusterSummary;
  nodes: ClusterNode[];
  failures: NodeFailure[];
}

export type SshAuthMethod = "agent" | "password" | "private_key";
export interface SshConfig {
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
  has_password: boolean;
  has_private_key: boolean;
  has_passphrase: boolean;
  has_identity_file: boolean;
  has_known_hosts_file: boolean;
}

export interface SshExportConfig {
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
}

export interface ConnectionProfile {
  ssh?: SshConfig | null;
  sentinel?: SentinelConfig | null;
  cluster?: ClusterConfig | null;
  id: string;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  has_password: boolean;
  tls: boolean;
  verify_server_cert: boolean;
  ca_certificate_name: string | null;
  client_certificate_name: string | null;
  has_ca_certificate: boolean;
  has_client_certificate: boolean;
}

export interface SaveConnectionInput {
  sentinel_password?: string | null;
  ssh_password?: string | null;
  ssh_private_key?: string | null;
  ssh_passphrase?: string | null;
  ssh_identity_file?: string | null;
  ssh_known_hosts_file?: string | null;
  profile: ConnectionProfile;
  password: string | null;
  ca_certificate: string | null;
  client_certificate: string | null;
  client_key: string | null;
  clear_ca_certificate: boolean;
  clear_client_certificate: boolean;
  clear_ssh_secrets: boolean;
}

export interface ConnectionExportProfile {
  ssh?: SshExportConfig | null;
  sentinel?: Omit<SentinelConfig, "has_password"> | null;
  cluster?: ClusterConfig | null;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  tls: boolean;
  verify_server_cert: boolean;
  ca_certificate_name: string | null;
  client_certificate_name: string | null;
}

export interface ScanKeysInput {
  connection_id: string;
  cursor: ScanCursor;
  pattern: string;
  count: number;
  key_type: string | null;
}

export interface ScanPage {
  cursor: ScanCursor;
  keys: KeySummary[];
  has_more: boolean;
  node_failures: NodeFailure[];
}

export interface NodeAnalysisResult {
  node_id: string;
  endpoint: ConnectionEndpoint;
  report: DatabaseAnalysisReport;
}

export interface DatabaseAnalysisReport {
  database: number;
  pattern: string;
  delimiter: string;
  progress: AnalysisProgress;
  total_keys: AnalysisSummary;
  total_memory: AnalysisSummary;
  top_keys_by_length: AnalysisKey[];
  top_keys_by_memory: AnalysisKey[];
  top_namespaces_by_keys: NamespaceSummary[];
  top_namespaces_by_memory: NamespaceSummary[];
  expiration_groups: ExpirationGroup[];
  node_results: NodeAnalysisResult[];
  failed_nodes: NodeFailure[];
}

export function getClusterTopology(connectionId: string): Promise<ClusterTopology> {
  return call<ClusterTopology>("get_cluster_topology", { connection_id: connectionId });
}
```

Add error copy for topology/node/partial/cross-slot failures. Do not consume backend `message` for unknown codes.

- [ ] **Step 4: Run bridge, Browser state and TypeScript build**

Run: `npm test -- src/lib/tauri.test.ts src/features/browser/browserState.test.ts`

Run: `npm run build`

Expected: PASS with no numeric coercion of Cluster cursors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/browser/browserState.ts src/features/browser/browserState.test.ts
git commit -m "接入 Cluster typed IPC 合同"
```

### Task 9: Cluster/SSH 连接表单与拓扑工作区

**Files:**
- Modify: `src/features/connections/connectionState.ts`
- Modify: `src/features/connections/ConnectionForm.tsx`
- Modify: `src/features/connections/ConnectionList.tsx`
- Test: `src/features/connections/connections.test.tsx`
- Create: `src/features/topology/TopologyPage.tsx`
- Create: `src/features/topology/topologyState.ts`
- Create: `src/features/topology/TopologyPage.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/app.smoke.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: Cluster/SSH profile DTOs and topology wrappers from Task 8
- Produces: `topology` workspace with refresh, summary, node table and partial failure banner

- [ ] **Step 1: Write failing form and topology UI tests**

```tsx
it("builds a db-zero cluster profile from bounded seed rows", async () => {
  render(<ConnectionPage onOpenConnection={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("连接类型"), { target: { value: "cluster" } });
  fireEvent.change(screen.getByLabelText("Cluster 种子节点"), { target: { value: "127.0.0.1:7000\n127.0.0.1:7001" } });
  fireEvent.click(screen.getByRole("button", { name: "保存连接" }));
  await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledWith(expect.objectContaining({ profile: expect.objectContaining({ database: 0, cluster: expect.any(Object) }) })));
});

it("shows healthy nodes and a bounded partial-failure banner", async () => {
  getClusterTopologyMock.mockResolvedValue(topologyWithOneFailure);
  render(<TopologyPage connectionId="cluster-1" />);
  expect(await screen.findByText("node-1")).toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("1 个节点暂不可用");
});
```

- [ ] **Step 2: Run UI tests and verify RED**

Run: `npm test -- src/features/connections/connections.test.tsx src/features/topology/TopologyPage.test.tsx src/app.smoke.test.tsx`

Expected: FAIL because Cluster option, SSH auth fields and topology workspace do not exist.

- [ ] **Step 3: Implement the approved workflows**

Add topology selector `standalone | sentinel | cluster`; Cluster shows seed textarea, DB fixed to 0 and read-from-replicas option, and mirrors the first normalized seed into legacy host/port fields. SSH shows Agent/Password/Private Key, identity/known_hosts paths, private key PEM and passphrase; every path/value is sent only in save/test inputs and persisted in the secret store, while returned profiles display boolean badges only. Standalone and Sentinel SSH+TLS remain selectable with original endpoint SNI; Cluster + SSH is disabled with a local explanation. Topology page shows cluster state, slots, primary/replica, health, metrics, refresh and node failures.

```ts
export interface ConnectionFormValues {
  topology: "standalone" | "sentinel" | "cluster";
  cluster_nodes: string;
  cluster_read_from_replicas: boolean;
  ssh_auth_method: "agent" | "password" | "private_key";
  ssh_password: string;
  ssh_private_key: string;
  ssh_passphrase: string;
}
```

- [ ] **Step 4: Run connection/topology/smoke tests and build**

Run: `npm test -- src/features/connections/connections.test.tsx src/features/topology/TopologyPage.test.tsx src/app.smoke.test.tsx`

Run: `npm run build`

Expected: PASS; 375 px layout has no horizontal overflow by CSS inspection, and Cluster context shows seeds rather than a misleading single host.

- [ ] **Step 5: Commit**

```bash
git add src/features/connections src/features/topology src/App.tsx src/app.smoke.test.tsx src/styles.css
git commit -m "增加 Cluster 连接与拓扑工作区"
```

### Task 10: 隔离 Cluster、三平台 CI、范围文档与完整验收

**Files:**
- Create: `scripts/test-local-cluster.py`
- Create: `src-tauri/tests/cluster_integration.rs`
- Create: `.github/workflows/cross-platform.yml`
- Modify: `package.json`
- Modify: `package-lock.json` through npm only if scripts alter lock metadata
- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `docs/redisinsight-feature-matrix.md`
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

**Interfaces:**
- Produces: `npm run test:redis:cluster`
- Produces: CI matrix for `macos-latest`, `windows-latest`, `ubuntu-latest`

- [ ] **Step 1: Write the cluster integration test before the launcher**

```rust
#[tokio::test]
#[ignore = "由 scripts/test-local-cluster.py 设置 REDIX_TEST_REDIS_CLUSTER_URLS"]
async fn cluster_routes_slots_scans_all_primaries_and_reports_topology() {
    let seeds = std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS").expect("cluster launcher must set seeds");
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();
    service.execute_command("cluster", "SET {a}:one 1").await.unwrap();
    service.execute_command("cluster", "SET {b}:two 2").await.unwrap();
    let keys = collect_all_scan_pages(&service, "cluster").await;
    assert!(keys.contains("{a}:one"));
    assert!(keys.contains("{b}:two"));
    assert_eq!(service.get_cluster_topology("cluster").await.unwrap().summary.slots_ok, 16384);
}
```

- [ ] **Step 2: Run it directly and verify the documented failure/skip boundary**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test cluster_integration -- --ignored --nocapture`

Expected: FAIL with the explicit missing environment message; it must not connect to a user Redis.

- [ ] **Step 3: Implement isolated launcher and CI**

`scripts/test-local-cluster.py` must reserve random ports, create per-node temp dirs/configs, start 3 `redis-server` children with cluster enabled, use `redis-cli --cluster create ... --cluster-replicas 0 --cluster-yes`, verify every PID through INFO, remove inherited `REDIX_TEST_REDIS*`, set only the generated seed list, run the ignored integration test and always terminate children.

```yaml
strategy:
  fail-fast: false
  matrix:
    os: [macos-latest, windows-latest, ubuntu-latest]
steps:
  - uses: actions/checkout@v4
  - uses: actions/setup-node@v4
    with: { node-version: 22, cache: npm }
  - uses: dtolnay/rust-toolchain@stable
  - run: npm ci
  - run: npm run test:frontend
  - run: npm run test:rust
  - run: npm run build
  - run: npm run tauri:build -- --no-bundle
```

Add OS-specific prerequisites without signing, publishing or installing bundles. Cluster/sshd integration jobs may run only where prerequisites are installed and must state when skipped.

- [ ] **Step 4: Run the complete local verification matrix**

Run: `npm run test:frontend`

Run: `npm run test:rust`

Run: `npm run test:redis:local`

Run: `npm run test:redis:cluster`

Run: `npm run build`

Run: `npm run check:non-cloud`

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `CARGO_NET_OFFLINE=true npm run tauri:build -- --no-bundle`

Run: `git diff --check`

Expected: all available local checks PASS; unavailable TLS/Windows/Linux/sshd environments are recorded as not executed, never as passed.

- [ ] **Step 5: Update scope/matrix and commit verification evidence**

Document exact Cluster, SSH auth, TLS+SSH, Sentinel, DB 0, Pub/Sub/Profiler Cluster limitation and platform evidence. Mark Cluster supported only for completed routed commands, scan, topology and analysis; keep unsupported fan-out functions explicit.

```bash
git add scripts/test-local-cluster.py src-tauri/tests/cluster_integration.rs .github/workflows/cross-platform.yml package.json package-lock.json README.md docs task_plan.md findings.md progress.md
git commit -m "完成连接与拓扑跨平台验收"
```
