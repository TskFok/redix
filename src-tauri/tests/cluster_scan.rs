use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use futures_util::future::BoxFuture;
use redix_lib::{
    domain::{
        ClusterNode, ClusterNodeHealth, ClusterNodeMetrics, ClusterNodeRole, ConnectionEndpoint,
        ScanCursor,
    },
    error::AppError,
    redis::{
        scan_cluster, ClusterNodeConnectionFactory, ClusterScanBackend, ClusterScanNode,
        ClusterScanState, NodeScanCursor,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn nodes(count: usize) -> Vec<ClusterScanNode> {
    (0..count)
        .map(|index| ClusterScanNode {
            node_id: format!("n{index}"),
            endpoint: ConnectionEndpoint {
                host: format!("node-{index}.internal"),
                port: 7_000 + u16::try_from(index).unwrap(),
            },
        })
        .collect()
}

fn known_nodes(values: &[ClusterScanNode]) -> HashSet<String> {
    values.iter().map(|node| node.node_id.clone()).collect()
}

#[test]
fn scan_cursor_serde_accepts_legacy_numbers_and_opaque_cluster_strings() {
    assert_eq!(
        serde_json::from_str::<ScanCursor>("42").unwrap(),
        ScanCursor::Standalone(42)
    );
    assert_eq!(
        serde_json::from_str::<ScanCursor>(r#""cluster:opaque""#).unwrap(),
        ScanCursor::Cluster("cluster:opaque".into())
    );
    assert_eq!(
        serde_json::to_string(&ScanCursor::Standalone(7)).unwrap(),
        "7"
    );
    assert_eq!(
        serde_json::to_string(&ScanCursor::Cluster("cluster:value".into())).unwrap(),
        r#""cluster:value""#
    );
}

#[test]
fn cluster_cursor_round_trips_and_rejects_unknown_duplicate_or_excess_nodes() {
    let known = nodes(2);
    let state = ClusterScanState::new(
        7,
        vec![
            NodeScanCursor {
                node_id: "n0".into(),
                cursor: 12,
            },
            NodeScanCursor {
                node_id: "n1".into(),
                cursor: 0,
            },
        ],
    );
    let cursor = state.encode().unwrap();
    assert!(cursor.starts_with("cluster:"));
    assert_eq!(
        ClusterScanState::decode(&cursor, 7, &known_nodes(&known)).unwrap(),
        state
    );
    assert_eq!(
        ClusterScanState::decode(&cursor, 8, &known_nodes(&known)).unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        ClusterScanState::decode(&cursor, 7, &HashSet::from(["n0".into()])).unwrap_err(),
        AppError::InvalidInput
    );

    let duplicate = ClusterScanState::new(
        7,
        vec![
            NodeScanCursor {
                node_id: "n0".into(),
                cursor: 1,
            },
            NodeScanCursor {
                node_id: "n0".into(),
                cursor: 2,
            },
        ],
    )
    .encode();
    assert_eq!(duplicate.unwrap_err(), AppError::InvalidInput);

    let too_many = ClusterScanState::new(
        7,
        (0..129)
            .map(|index| NodeScanCursor {
                node_id: format!("n{index}"),
                cursor: 0,
            })
            .collect(),
    );
    assert_eq!(too_many.encode().unwrap_err(), AppError::InvalidInput);
}

#[test]
fn cluster_cursor_rejects_unknown_versions_and_oversized_encoded_input() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let unknown_version = format!(
        "cluster:{}",
        URL_SAFE_NO_PAD
            .encode(br#"{"version":2,"generation":7,"nodes":[],"pending_nodes":[],"next_node":0}"#)
    );
    assert_eq!(
        ClusterScanState::decode(&unknown_version, 7, &HashSet::new()).unwrap_err(),
        AppError::InvalidInput
    );
    let oversized = format!("cluster:{}", "a".repeat(32 * 1024 + 1));
    assert_eq!(
        ClusterScanState::decode(&oversized, 7, &HashSet::new()).unwrap_err(),
        AppError::InvalidInput
    );
}

#[test]
fn cluster_cursor_rejects_duplicate_or_unknown_pending_nodes_and_bad_rotation_index() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let known = HashSet::from(["n0".to_owned(), "n1".to_owned()]);
    for payload in [
        r#"{"version":1,"generation":7,"nodes":[{"node_id":"n0","cursor":0},{"node_id":"n1","cursor":0}],"pending_nodes":["n0","n0"],"next_node":0}"#,
        r#"{"version":1,"generation":7,"nodes":[{"node_id":"n0","cursor":0},{"node_id":"n1","cursor":0}],"pending_nodes":["n2"],"next_node":0}"#,
        r#"{"version":1,"generation":7,"nodes":[{"node_id":"n0","cursor":0},{"node_id":"n1","cursor":0}],"pending_nodes":["n0"],"next_node":2}"#,
        r#"{"version":1,"generation":7,"nodes":[{"node_id":"n0","cursor":0},{"node_id":"n1","cursor":0}],"pending_nodes":["n0"],"next_node":0,"endpoint":"seed.internal:6379"}"#,
    ] {
        let cursor = format!("cluster:{}", URL_SAFE_NO_PAD.encode(payload));
        assert_eq!(
            ClusterScanState::decode(&cursor, 7, &known).unwrap_err(),
            AppError::InvalidInput
        );
    }
}

#[derive(Clone, Default)]
struct FakeBackend {
    replies: Arc<Mutex<HashMap<String, Result<(u64, Vec<Vec<u8>>), AppError>>>>,
    calls: Arc<Mutex<Vec<(String, u64, usize)>>>,
    endpoints: Arc<Mutex<Vec<(String, ConnectionEndpoint)>>>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    delay: Duration,
}

impl FakeBackend {
    fn with_reply(self, node: &str, reply: Result<(u64, Vec<Vec<u8>>), AppError>) -> Self {
        self.replies.lock().unwrap().insert(node.into(), reply);
        self
    }
}

impl ClusterScanBackend for FakeBackend {
    fn scan_node<'a>(
        &'a self,
        node: &'a ClusterScanNode,
        cursor: u64,
        _pattern: &'a str,
        count: usize,
    ) -> BoxFuture<'a, Result<(u64, Vec<Vec<u8>>), AppError>> {
        Box::pin(async move {
            self.endpoints
                .lock()
                .unwrap()
                .push((node.node_id.clone(), node.endpoint.clone()));
            self.calls
                .lock()
                .unwrap()
                .push((node.node_id.clone(), cursor, count));
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            self.replies
                .lock()
                .unwrap()
                .get(&node.node_id)
                .cloned()
                .unwrap_or_else(|| Ok((0, Vec::new())))
        })
    }
}

#[tokio::test]
async fn failed_node_keeps_its_cursor_while_successful_nodes_progress_and_retry() {
    let cluster_nodes = nodes(2);
    let backend = FakeBackend::default()
        .with_reply("n0", Ok((41, vec![b"healthy".to_vec()])))
        .with_reply("n1", Err(AppError::CommandFailed));

    let page = scan_cluster(&backend, 9, &cluster_nodes, None, "*", 100)
        .await
        .unwrap();
    assert_eq!(page.keys, vec![b"healthy".to_vec()]);
    assert_eq!(page.node_failures.len(), 1);
    assert_eq!(page.node_failures[0].node_id, "n1");
    assert_eq!(page.node_failures[0].code, "CLUSTER_NODE_UNAVAILABLE");
    let state = ClusterScanState::decode(&page.cursor, 9, &known_nodes(&cluster_nodes)).unwrap();
    assert_eq!(state.cursor_for("n0"), Some(41));
    assert_eq!(state.cursor_for("n1"), Some(0));

    let retry_backend = FakeBackend::default()
        .with_reply("n0", Ok((0, vec![b"last".to_vec()])))
        .with_reply("n1", Ok((0, vec![b"recovered".to_vec()])));
    let retry = scan_cluster(
        &retry_backend,
        9,
        &cluster_nodes,
        Some(&page.cursor),
        "*",
        100,
    )
    .await
    .unwrap();
    assert_eq!(retry.keys, vec![b"last".to_vec(), b"recovered".to_vec()]);
    assert!(retry.node_failures.is_empty());
    assert!(!retry.has_more);
}

#[tokio::test]
async fn scan_is_bounded_to_eight_concurrent_nodes_and_rotates_fairly() {
    let cluster_nodes = nodes(10);
    let backend = FakeBackend {
        delay: Duration::from_millis(20),
        ..FakeBackend::default()
    };
    for node in &cluster_nodes {
        backend.replies.lock().unwrap().insert(
            node.node_id.clone(),
            Ok((1, vec![node.node_id.as_bytes().to_vec()])),
        );
    }

    let first = scan_cluster(&backend, 3, &cluster_nodes, None, "*", 3)
        .await
        .unwrap();
    assert_eq!(backend.max_active.load(Ordering::SeqCst), 8);
    let calls = backend.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 8);
    assert!(calls.iter().all(|(_, _, count)| *count == 1));

    backend.calls.lock().unwrap().clear();
    backend.endpoints.lock().unwrap().clear();
    let mut reordered = cluster_nodes[1..].to_vec();
    reordered.push(cluster_nodes[0].clone());
    reordered
        .iter_mut()
        .find(|node| node.node_id == "n8")
        .unwrap()
        .endpoint
        .port = 9_999;
    scan_cluster(&backend, 3, &reordered, Some(&first.cursor), "*", 3)
        .await
        .unwrap();
    assert_eq!(backend.calls.lock().unwrap()[0].0, "n8");
    assert_eq!(backend.endpoints.lock().unwrap()[0].1.port, 9_999);
}

#[tokio::test]
async fn completed_nodes_are_not_scanned_again_on_the_next_page() {
    let cluster_nodes = nodes(2);
    let first_backend = FakeBackend::default()
        .with_reply("n0", Ok((0, vec![b"done".to_vec()])))
        .with_reply("n1", Ok((5, vec![b"pending".to_vec()])));
    let first = scan_cluster(&first_backend, 4, &cluster_nodes, None, "*", 100)
        .await
        .unwrap();

    let second_backend = FakeBackend::default()
        .with_reply("n0", Ok((0, vec![b"must-not-repeat".to_vec()])))
        .with_reply("n1", Ok((0, vec![b"last".to_vec()])));
    let second = scan_cluster(
        &second_backend,
        4,
        &cluster_nodes,
        Some(&first.cursor),
        "*",
        100,
    )
    .await
    .unwrap();
    assert_eq!(
        second_backend.calls.lock().unwrap().as_slice(),
        &[("n1".into(), 5, 100)]
    );
    assert_eq!(second.keys, vec![b"last".to_vec()]);
    assert!(!second.has_more);
}

#[tokio::test]
async fn scan_deduplicates_binary_keys_and_never_truncates_a_redis_batch() {
    let cluster_nodes = nodes(2);
    let binary = vec![0xff, 0, b'k'];
    let backend = FakeBackend::default()
        .with_reply("n0", Ok((0, vec![binary.clone(), b"unique-0".to_vec()])))
        .with_reply("n1", Ok((0, vec![binary.clone(), b"unique-1".to_vec()])));
    let page = scan_cluster(&backend, 1, &cluster_nodes, None, "*", 1)
        .await
        .unwrap();
    assert_eq!(page.keys.len(), 3);
    assert_eq!(page.keys.iter().filter(|key| **key == binary).count(), 1);

    let oversized_backend =
        FakeBackend::default().with_reply("n0", Ok((7, vec![vec![b'x'; 4 * 1024 * 1024 + 1]])));
    assert_eq!(
        scan_cluster(&oversized_backend, 1, &nodes(1), None, "*", 1)
            .await
            .unwrap_err(),
        AppError::InvalidInput
    );
}

async fn spawn_authenticated_redis() -> (u16, Arc<Mutex<Vec<Vec<Vec<u8>>>>>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let server_observed = observed.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut pending = Vec::new();
        let mut buffer = [0_u8; 4_096];
        loop {
            let size = match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(size) => size,
            };
            pending.extend_from_slice(&buffer[..size]);
            while let Some((consumed, command)) = parse_resp_command(&pending) {
                pending.drain(..consumed);
                server_observed.lock().unwrap().push(command.clone());
                let reply = if command.first().map(Vec::as_slice) == Some(b"PING") {
                    b"+PONG\r\n".as_slice()
                } else {
                    b"+OK\r\n".as_slice()
                };
                socket.write_all(reply).await.unwrap();
            }
        }
    });
    (port, observed)
}

fn parse_resp_command(input: &[u8]) -> Option<(usize, Vec<Vec<u8>>)> {
    if input.first().copied()? != b'*' {
        return None;
    }
    let (line_end, count) = parse_resp_number(&input[1..])?;
    let mut offset = 1 + line_end;
    let mut command = Vec::with_capacity(count);
    for _ in 0..count {
        if input.get(offset).copied()? != b'$' {
            return None;
        }
        let (length_end, length) = parse_resp_number(&input[offset + 1..])?;
        offset += 1 + length_end;
        let end = offset.checked_add(length)?;
        if input.get(end..end + 2)? != b"\r\n" {
            return None;
        }
        command.push(input.get(offset..end)?.to_vec());
        offset = end + 2;
    }
    Some((offset, command))
}

fn parse_resp_number(input: &[u8]) -> Option<(usize, usize)> {
    let end = input.windows(2).position(|bytes| bytes == b"\r\n")?;
    let value = std::str::from_utf8(&input[..end]).ok()?.parse().ok()?;
    Some((end + 2, value))
}

#[tokio::test]
async fn node_connection_factory_reuses_captured_auth_and_never_falls_back_from_endpoint() {
    let (port, observed) = spawn_authenticated_redis().await;
    let factory = ClusterNodeConnectionFactory::new(
        Some("cluster-user".into()),
        Some("cluster-secret".into()),
        None,
    )
    .unwrap();
    let mut connection = factory
        .connection(&ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        })
        .await
        .unwrap();
    let pong: String = redis::cmd("PING")
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(pong, "PONG");
    assert!(observed.lock().unwrap().iter().any(|command| command
        == &vec![
            b"AUTH".to_vec(),
            b"cluster-user".to_vec(),
            b"cluster-secret".to_vec()
        ]));

    let error = factory
        .connection(&ConnectionEndpoint {
            host: String::new(),
            port,
        })
        .await
        .unwrap_err();
    assert_eq!(error, AppError::ClusterNodeUnavailable);
    assert_eq!(error.to_string(), "Redis 集群节点不可用");
}

#[tokio::test]
async fn node_factory_records_connection_endpoint_only_after_a_successful_dial() {
    let (port, _) = spawn_authenticated_redis().await;
    let announced = ConnectionEndpoint {
        host: "127.0.0.1".into(),
        port,
    };
    let mut node = ClusterNode {
        id: "primary-1".into(),
        endpoint: announced.clone(),
        connection_endpoint: None,
        role: ClusterNodeRole::Primary,
        health: ClusterNodeHealth::Online,
        primary_id: None,
        slots: Vec::new(),
        metrics: ClusterNodeMetrics::default(),
    };
    let factory = ClusterNodeConnectionFactory::new(None, None, None).unwrap();
    let _connection = factory.connection_for_node(&mut node).await.unwrap();
    assert_eq!(node.endpoint, announced);
    assert_eq!(node.connection_endpoint, Some(announced));

    let mut invalid = ClusterNode {
        endpoint: ConnectionEndpoint {
            host: "?".into(),
            port,
        },
        connection_endpoint: None,
        ..node
    };
    assert_eq!(
        factory.connection_for_node(&mut invalid).await.unwrap_err(),
        AppError::ClusterNodeUnavailable
    );
    assert_eq!(invalid.connection_endpoint, None);
}
