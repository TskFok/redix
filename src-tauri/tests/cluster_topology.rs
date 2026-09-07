use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use redis::Value;
use redix_lib::{
    domain::{
        ClusterConfig, ClusterNodeHealth, ClusterNodeRole, ConnectionEndpoint, ConnectionProfile,
        NodeFailure, SlotRange,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{
        apply_node_info_results, cluster_node_info_command, merge_node_metrics, parse_cluster_info,
        parse_cluster_node_info_reply, parse_cluster_nodes, parse_cluster_shards,
        parse_cluster_shards_for_tls, ClusterNodeInfoResult, RedisOperations, RedisService,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn bulk(value: &str) -> Value {
    Value::BulkString(value.as_bytes().to_vec())
}

#[test]
fn shards_preserve_announced_endpoint_and_prefer_plain_port() {
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(16_383)]),
        bulk("nodes"),
        Value::Array(vec![Value::Array(vec![
            bulk("id"),
            bulk("node-1"),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("ip"),
            bulk("10.0.0.2"),
            bulk("port"),
            Value::Int(6379),
            bulk("tls-port"),
            Value::Int(6380),
            bulk("health"),
            bulk("online"),
        ])]),
    ])]);

    let nodes = parse_cluster_shards(reply).unwrap();

    assert_eq!(nodes[0].endpoint.host, "cache.internal");
    assert_eq!(nodes[0].endpoint.port, 6379);
    assert_eq!(nodes[0].connection_endpoint, None);
    assert_eq!(
        nodes[0].slots,
        vec![SlotRange {
            start: 0,
            end: 16_383
        }]
    );
}

#[test]
fn shards_tls_aware_parser_selects_ports_without_creating_connection_endpoints() {
    let reply = || {
        Value::Array(vec![Value::Array(vec![
            bulk("slots"),
            Value::Array(vec![Value::Int(0), Value::Int(1)]),
            bulk("nodes"),
            Value::Array(vec![Value::Array(vec![
                bulk("id"),
                bulk("node"),
                bulk("role"),
                bulk("master"),
                bulk("endpoint"),
                bulk("cache.internal"),
                bulk("port"),
                Value::Int(6379),
                bulk("tls-port"),
                Value::Int(6380),
                bulk("health"),
                bulk("online"),
            ])]),
        ])])
    };
    assert_eq!(
        parse_cluster_shards(reply()).unwrap()[0].endpoint.port,
        6379
    );
    let tls_nodes = parse_cluster_shards_for_tls(reply(), true).unwrap();
    assert_eq!(tls_nodes[0].endpoint.port, 6380);
    assert_eq!(tls_nodes[0].connection_endpoint, None);

    let tls_only = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(2), Value::Int(3)]),
        bulk("nodes"),
        Value::Array(vec![Value::Array(vec![
            bulk("id"),
            bulk("tls-only"),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("tls-port"),
            Value::Int(6380),
            bulk("health"),
            bulk("online"),
        ])]),
    ])]);
    assert_eq!(
        parse_cluster_shards(tls_only).unwrap()[0].endpoint.port,
        6380
    );

    let plain_only = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(4), Value::Int(5)]),
        bulk("nodes"),
        Value::Array(vec![Value::Array(vec![
            bulk("id"),
            bulk("plain-only"),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("port"),
            Value::Int(6379),
            bulk("health"),
            bulk("online"),
        ])]),
    ])]);
    assert_eq!(
        parse_cluster_shards_for_tls(plain_only, true).unwrap()[0]
            .endpoint
            .port,
        6379
    );
}

#[test]
fn shards_empty_or_nil_endpoint_falls_back_to_hostname_then_ip() {
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(1)]),
        bulk("nodes"),
        Value::Array(vec![
            Value::Array(vec![
                bulk("id"),
                bulk("primary"),
                bulk("role"),
                bulk("master"),
                bulk("endpoint"),
                bulk(""),
                bulk("hostname"),
                bulk("primary.internal"),
                bulk("port"),
                Value::Int(6379),
                bulk("health"),
                bulk("online"),
            ]),
            Value::Array(vec![
                bulk("id"),
                bulk("replica"),
                bulk("role"),
                bulk("replica"),
                bulk("endpoint"),
                Value::Nil,
                bulk("ip"),
                bulk("10.0.0.2"),
                bulk("port"),
                Value::Int(6379),
                bulk("health"),
                bulk("online"),
            ]),
        ]),
    ])]);
    let nodes = parse_cluster_shards(reply).unwrap();
    assert_eq!(nodes[0].endpoint.host, "primary.internal");
    assert_eq!(nodes[1].endpoint.host, "10.0.0.2");
}

#[test]
fn shards_parser_accepts_a_real_resp2_wire_fixture() {
    let reply = redis::parse_redis_value(
        b"*1\r\n*4\r\n$5\r\nslots\r\n*2\r\n:0\r\n:1\r\n$5\r\nnodes\r\n*1\r\n*10\r\n$2\r\nid\r\n$4\r\nnode\r\n$4\r\nrole\r\n$6\r\nmaster\r\n$8\r\nendpoint\r\n$14\r\ncache.internal\r\n$4\r\nport\r\n:6379\r\n$6\r\nhealth\r\n$6\r\nonline\r\n",
    )
    .unwrap();
    let nodes = parse_cluster_shards(reply).unwrap();
    assert_eq!(nodes[0].id, "node");
    assert_eq!(nodes[0].endpoint.port, 6379);
}

#[test]
fn shards_parser_accepts_resp3_maps_and_attributes() {
    let reply = Value::Attribute {
        data: Box::new(Value::Array(vec![Value::Map(vec![
            (
                bulk("slots"),
                Value::Array(vec![Value::Int(1), Value::Int(2)]),
            ),
            (
                bulk("nodes"),
                Value::Array(vec![
                    Value::Map(vec![
                        (bulk("id"), bulk("node-1")),
                        (bulk("role"), bulk("master")),
                        (bulk("endpoint"), bulk("primary.internal")),
                        (bulk("port"), Value::Int(6379)),
                        (bulk("health"), bulk("online")),
                    ]),
                    Value::Map(vec![
                        (bulk("id"), bulk("node-2")),
                        (bulk("role"), bulk("replica")),
                        (bulk("endpoint"), bulk("replica.internal")),
                        (bulk("port"), Value::Int(6379)),
                        (bulk("health"), bulk("loading")),
                    ]),
                ]),
            ),
        ])])),
        attributes: vec![(bulk("server"), bulk("redis"))],
    };

    let nodes = parse_cluster_shards(reply).unwrap();

    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[1].role, ClusterNodeRole::Replica);
    assert_eq!(nodes[1].health, ClusterNodeHealth::Loading);
    assert_eq!(nodes[1].primary_id.as_deref(), Some("node-1"));
    assert!(nodes[1].slots.is_empty());
}

#[test]
fn nodes_parser_uses_announced_hostname_and_marks_disconnected_nodes_offline() {
    let nodes = parse_cluster_nodes(
        "id1 [::1]:7000@17000,cache.internal master - 0 0 1 disconnected 0-100\n",
    )
    .unwrap();

    assert_eq!(nodes[0].endpoint.host, "cache.internal");
    assert_eq!(nodes[0].endpoint.port, 7000);
    assert_eq!(nodes[0].health, ClusterNodeHealth::Offline);
    assert_eq!(nodes[0].slots, vec![SlotRange { start: 0, end: 100 }]);

    let ipv6 = parse_cluster_nodes("id2 [::1]:7001@17001 master - 0 0 2 connected\n").unwrap();
    assert_eq!(ipv6[0].endpoint.host, "::1");
}

#[test]
fn shards_infer_replica_primary_and_preserve_question_mark_endpoint() {
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(1)]),
        bulk("nodes"),
        Value::Array(vec![
            Value::Array(vec![
                bulk("id"),
                bulk("primary"),
                bulk("role"),
                bulk("master"),
                bulk("endpoint"),
                bulk("?"),
                bulk("ip"),
                bulk("10.0.0.1"),
                bulk("port"),
                Value::Int(6379),
                bulk("health"),
                bulk("online"),
            ]),
            Value::Array(vec![
                bulk("id"),
                bulk("replica"),
                bulk("role"),
                bulk("replica"),
                bulk("hostname"),
                bulk("replica.example"),
                bulk("ip"),
                bulk("10.0.0.2"),
                bulk("port"),
                Value::Int(0),
                bulk("tls-port"),
                Value::Int(6380),
                bulk("health"),
                bulk("online"),
            ]),
        ]),
    ])]);

    let nodes = parse_cluster_shards(reply).unwrap();
    assert_eq!(nodes[0].endpoint.host, "?");
    assert_eq!(nodes[1].endpoint.host, "replica.example");
    assert_eq!(nodes[1].endpoint.port, 6380);
    assert_eq!(nodes[1].primary_id.as_deref(), Some("primary"));
}

#[test]
fn info_parser_and_metric_merge_tolerate_partial_sections() {
    let summary = parse_cluster_info(
        "# Cluster\ncluster_state:ok\ncluster_slots_assigned:16384\ncluster_size:3\n",
    )
    .unwrap();
    assert_eq!(summary.state, "ok");
    assert_eq!(summary.slots_assigned, 16_384);
    assert_eq!(summary.slots_ok, 0);
    assert_eq!(summary.size, 3);
    assert_eq!(summary.known_nodes, 0);

    let mut node = parse_cluster_nodes("id1 127.0.0.1:7000@17000 master - 0 0 1 connected\n")
        .unwrap()
        .remove(0);
    merge_node_metrics(
        &mut node,
        "# Memory\nused_memory:1024\n# Stats\ninstantaneous_ops_per_sec:7\nkeyspace_hits:3\nkeyspace_misses:1\n",
    )
    .unwrap();
    assert_eq!(node.metrics.used_memory_bytes, Some(1024));
    assert_eq!(node.metrics.ops_per_second, Some(7));
    assert_eq!(node.metrics.connected_clients, None);
    assert_eq!(node.metrics.cache_hit_ratio, Some(75.0));
}

#[test]
fn topology_keeps_all_nodes_and_sorts_sanitized_info_failures() {
    let mut nodes = parse_cluster_nodes(
        "node-1 10.0.0.1:7001@17001 master - 0 0 1 connected 0-5000\n\
         node-2 10.0.0.2:7002@17002 master - 0 0 2 connected 5001-10000\n\
         node-3 10.0.0.3:7003@17003 master - 0 0 3 connected 10001-16383\n",
    )
    .unwrap();

    let failures = apply_node_info_results(
        &mut nodes,
        vec![
            ClusterNodeInfoResult::failure("node-3"),
            ClusterNodeInfoResult::success(
                "node-1",
                ConnectionEndpoint {
                    host: "127.0.0.1".into(),
                    port: 47001,
                },
                "used_memory:1024\nconnected_clients:4\n",
            ),
            ClusterNodeInfoResult::failure("node-2"),
        ],
    );

    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].endpoint.host, "10.0.0.1");
    assert_eq!(nodes[0].metrics.used_memory_bytes, Some(1024));
    assert_eq!(nodes[0].metrics.connected_clients, Some(4));
    assert_eq!(nodes[0].connection_endpoint.as_ref().unwrap().port, 47001);
    assert_eq!(nodes[1].connection_endpoint, None);
    assert_eq!(nodes[2].connection_endpoint, None);
    assert_eq!(
        failures,
        vec![
            NodeFailure {
                node_id: "node-2".into(),
                code: "CLUSTER_NODE_UNAVAILABLE".into(),
            },
            NodeFailure {
                node_id: "node-3".into(),
                code: "CLUSTER_NODE_UNAVAILABLE".into(),
            },
        ]
    );
}

#[test]
fn node_info_command_requests_only_the_explicit_bounded_sections() {
    assert_eq!(
        cluster_node_info_command().get_packed_command(),
        b"*7\r\n$4\r\nINFO\r\n$6\r\nserver\r\n$7\r\nclients\r\n$6\r\nmemory\r\n$5\r\nstats\r\n$11\r\nreplication\r\n$8\r\nkeyspace\r\n"
            .to_vec()
    );
}

#[test]
fn node_info_reply_rejects_oversized_non_utf8_and_wrapped_payloads() {
    assert_eq!(
        parse_cluster_node_info_reply(Value::BulkString(vec![b'x'; 4 * 1024 * 1024 + 1])),
        Err(AppError::ClusterNodeUnavailable)
    );
    assert_eq!(
        parse_cluster_node_info_reply(Value::BulkString(vec![0xff])),
        Err(AppError::ClusterNodeUnavailable)
    );
    assert_eq!(
        parse_cluster_node_info_reply(Value::Attribute {
            data: Box::new(Value::BulkString(b"used_memory:1\n".to_vec())),
            attributes: vec![(bulk("source"), bulk("node"))],
        }),
        Err(AppError::ClusterNodeUnavailable)
    );
}

#[test]
fn info_and_metrics_enforce_bounds_and_ignore_malformed_optional_values() {
    assert_eq!(
        parse_cluster_info("cluster_slots_ok:16385\n"),
        Err(AppError::ClusterTopologyFailed)
    );
    assert_eq!(
        parse_cluster_info("cluster_size:129\n"),
        Err(AppError::ClusterTopologyFailed)
    );
    assert_eq!(
        parse_cluster_info("cluster_known_nodes:129\n"),
        Err(AppError::ClusterTopologyFailed)
    );

    let mut node = parse_cluster_nodes("id1 127.0.0.1:7000@17000 master - 0 0 1 connected\n")
        .unwrap()
        .remove(0);
    node.metrics.used_memory_bytes = Some(11);
    merge_node_metrics(
        &mut node,
        "used_memory:not-a-number\nconnected_clients:2\ninstantaneous_input_kbps:nan\n",
    )
    .unwrap();
    assert_eq!(node.metrics.used_memory_bytes, Some(11));
    assert_eq!(node.metrics.connected_clients, Some(2));
    assert_eq!(node.metrics.network_in_kbps, None);
}

#[test]
fn parsers_reject_malformed_or_oversized_topology_replies() {
    assert_eq!(
        parse_cluster_nodes("id1 127.0.0.1:7000@17000 master - 0 0 1 connected 100-0\n"),
        Err(AppError::ClusterTopologyFailed)
    );
    assert_eq!(
        parse_cluster_info(&"x".repeat(4 * 1024 * 1024 + 1)),
        Err(AppError::ClusterTopologyFailed)
    );
    assert_eq!(
        parse_cluster_shards(Value::Array(vec![Value::Array(vec![
            bulk("slots"),
            Value::Array(vec![Value::Int(-1), Value::Int(1)]),
            bulk("nodes"),
            Value::Array(vec![]),
        ])])),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn shards_reject_multiple_primaries_in_one_shard() {
    let primary = |id| {
        Value::Array(vec![
            bulk("id"),
            bulk(id),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("port"),
            Value::Int(6379),
            bulk("health"),
            bulk("online"),
        ])
    };
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(1)]),
        bulk("nodes"),
        Value::Array(vec![primary("node-1"), primary("node-2")]),
    ])]);
    assert_eq!(
        parse_cluster_shards(reply),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn shards_only_classifies_unknown_command_as_fallback_eligible() {
    let unsupported = redis::parse_redis_value(b"-ERR unknown subcommand 'SHARDS'\r\n").unwrap();
    assert_eq!(
        parse_cluster_shards(unsupported),
        Err(AppError::UnsupportedFeature)
    );

    let unavailable = redis::parse_redis_value(b"-NOAUTH Authentication required.\r\n").unwrap();
    assert_eq!(
        parse_cluster_shards(unavailable),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn nodes_parser_caps_slot_ranges_across_the_entire_reply() {
    let slots = (0..9_000)
        .map(|slot| slot.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let reply = format!(
        "id1 127.0.0.1:7000@17000 master - 0 0 1 connected {slots}\n\
         id2 127.0.0.1:7001@17001 master - 0 0 2 connected {slots}\n"
    );

    assert!(matches!(
        parse_cluster_nodes(&reply),
        Err(AppError::ClusterTopologyFailed)
    ));
}

#[test]
fn shards_parser_caps_total_output_slot_ranges() {
    let shard = |id: &str| {
        Value::Array(vec![
            bulk("slots"),
            Value::Array(std::iter::repeat_n(Value::Int(0), 18_000).collect()),
            bulk("nodes"),
            Value::Array(vec![Value::Array(vec![
                bulk("id"),
                bulk(id),
                bulk("role"),
                bulk("master"),
                bulk("endpoint"),
                bulk("cache.internal"),
                bulk("port"),
                Value::Int(6379),
                bulk("health"),
                bulk("online"),
            ])]),
        ])
    };
    assert_eq!(
        parse_cluster_shards(Value::Array(vec![shard("node-1"), shard("node-2")])),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn topology_limits_accept_exact_boundaries_and_reject_the_next_value() {
    let nodes = (0..128)
        .map(|index| format!("id{index} 127.0.0.1:7000@17000 master - 0 0 1 connected"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse_cluster_nodes(&nodes).unwrap().len(), 128);
    assert_eq!(
        parse_cluster_nodes(&format!(
            "{nodes}\nid128 127.0.0.1:7000@17000 master - 0 0 1 connected"
        )),
        Err(AppError::ClusterTopologyFailed)
    );

    let max_id = "n".repeat(256);
    assert!(parse_cluster_nodes(&format!(
        "{max_id} 127.0.0.1:7000@17000 master - 0 0 1 connected"
    ))
    .is_ok());
    let too_long_id = "n".repeat(257);
    assert_eq!(
        parse_cluster_nodes(&format!(
            "{too_long_id} 127.0.0.1:7000@17000 master - 0 0 1 connected"
        )),
        Err(AppError::ClusterTopologyFailed)
    );
    let max_host = "h".repeat(256);
    assert!(parse_cluster_nodes(&format!(
        "id 127.0.0.1:7000@17000,{max_host} master - 0 0 1 connected"
    ))
    .is_ok());
    let too_long_host = "h".repeat(257);
    assert_eq!(
        parse_cluster_nodes(&format!(
            "id 127.0.0.1:7000@17000,{too_long_host} master - 0 0 1 connected"
        )),
        Err(AppError::ClusterTopologyFailed)
    );

    let slots = std::iter::repeat_n("0", 16_384)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(parse_cluster_nodes(&format!(
        "id 127.0.0.1:7000@17000 master - 0 0 1 connected {slots}"
    ))
    .is_ok());
    let too_many_slots = std::iter::repeat_n("0", 16_385)
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        parse_cluster_nodes(&format!(
            "id 127.0.0.1:7000@17000 master - 0 0 1 connected {too_many_slots}"
        )),
        Err(AppError::ClusterTopologyFailed)
    );
}

struct TopologyProfiles(Vec<ConnectionProfile>);

impl ProfileRepository for TopologyProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.clone())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct TopologySecrets;

impl SecretStore for TopologySecrets {
    fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(None)
    }

    fn write(&self, _connection_id: &str, _secrets: &ConnectionSecrets) -> Result<(), AppError> {
        Ok(())
    }

    fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
        Ok(())
    }
}

fn topology_profile(port: u16) -> ConnectionProfile {
    ConnectionProfile {
        id: "topology-cluster".into(),
        name: "Topology cluster".into(),
        host: "127.0.0.1".into(),
        port,
        username: None,
        database: 0,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
        ssh: None,
        sentinel: None,
        cluster: Some(ClusterConfig {
            nodes: vec![ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port,
            }],
            read_from_replicas: false,
        }),
    }
}

fn parse_resp_command(buffer: &[u8]) -> Option<(usize, Vec<Vec<u8>>)> {
    if buffer.first() != Some(&b'*') {
        return None;
    }
    let header_end = buffer.windows(2).position(|pair| pair == b"\r\n")?;
    let count = std::str::from_utf8(&buffer[1..header_end])
        .ok()?
        .parse::<usize>()
        .ok()?;
    let mut offset = header_end + 2;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        let length_end = buffer[offset..]
            .windows(2)
            .position(|pair| pair == b"\r\n")?
            + offset;
        let length = std::str::from_utf8(&buffer[offset + 1..length_end])
            .ok()?
            .parse::<usize>()
            .ok()?;
        offset = length_end + 2;
        let end = offset.checked_add(length)?;
        if buffer.get(end..end + 2)? != b"\r\n" {
            return None;
        }
        args.push(buffer[offset..end].to_vec());
        offset = end + 2;
    }
    Some((offset, args))
}

async fn spawn_topology_cluster(
    gate_node_info: bool,
) -> (
    u16,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    spawn_topology_cluster_options(gate_node_info, false, false, false).await
}

async fn spawn_topology_cluster_options(
    gate_node_info: bool,
    healthy_nodes: bool,
    gate_keyspace: bool,
    fail_keyspace: bool,
) -> (
    u16,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    spawn_topology_cluster_info_options(
        gate_node_info,
        healthy_nodes,
        gate_keyspace,
        fail_keyspace,
        None,
    )
    .await
}

async fn spawn_topology_cluster_info_options(
    gate_node_info: bool,
    healthy_nodes: bool,
    gate_keyspace: bool,
    fail_keyspace: bool,
    multi_info_error: Option<&'static str>,
) -> (
    u16,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let unavailable = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let cluster_info_calls = Arc::new(AtomicUsize::new(0));
    let node_info_commands = Arc::new(Mutex::new(Vec::new()));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let gate = Arc::new(tokio::sync::Mutex::new(Some((entered_tx, resume_rx))));
    let calls = Arc::clone(&cluster_info_calls);
    let info_commands = Arc::clone(&node_info_commands);
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let calls = Arc::clone(&calls);
            let info_commands = Arc::clone(&info_commands);
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                let mut attempted_multi_info = false;
                let mut pending = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&buffer[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let response = match (
                            command.first().map(Vec::as_slice),
                            command.get(1).map(Vec::as_slice),
                        ) {
                            (Some(b"CLUSTER"), Some(b"SLOTS")) => format!(
                                "*1\r\n*3\r\n:0\r\n:16383\r\n*2\r\n$9\r\n127.0.0.1\r\n:{port}\r\n"
                            ),
                            (Some(b"CLUSTER"), Some(b"SHARDS")) => {
                                "-ERR unknown subcommand 'SHARDS'\r\n".into()
                            }
                            (Some(b"CLUSTER"), Some(b"NODES")) => {
                                let second_port = if healthy_nodes { port } else { unavailable };
                                let mut body = format!(
                                    "node-1 127.0.0.1:{port}@1 master - 0 0 1 connected 0-8191\nnode-2 127.0.0.1:{second_port}@1 master - 0 0 2 connected 8192-16383\n"
                                );
                                if healthy_nodes {
                                    body.push_str(&format!("replica-1 127.0.0.1:{port}@1 slave node-1 0 0 1 connected\n"));
                                }
                                format!("${}\r\n{body}\r\n", body.len())
                            }
                            (Some(b"CLUSTER"), Some(b"INFO")) => {
                                calls.fetch_add(1, Ordering::SeqCst);
                                let body = "cluster_state:ok\ncluster_slots_assigned:16384\ncluster_slots_ok:16384\ncluster_size:2\ncluster_known_nodes:2\n";
                                format!("${}\r\n{body}\r\n", body.len())
                            }
                            (Some(b"INFO"), _) if command.len() == 7 => {
                                info_commands.lock().unwrap().push(command);
                                attempted_multi_info = true;
                                if gate_node_info {
                                    if let Some((entered, resume)) = gate.lock().await.take() {
                                        let _ = entered.send(());
                                        let _ = resume.await;
                                    }
                                }
                                if let Some(error) = multi_info_error {
                                    format!("-{error}\r\n")
                                } else {
                                    let body = "# Server\nuptime_in_seconds:60\n# Clients\nconnected_clients:4\n# Memory\nused_memory:1024\n# Stats\ninstantaneous_ops_per_sec:7\ntotal_commands_processed:12\n";
                                    format!("${}\r\n{body}\r\n", body.len())
                                }
                            }
                            (Some(b"INFO"), None) if attempted_multi_info => {
                                info_commands.lock().unwrap().push(command);
                                let body = "# Server\nredis_version:6.2.0\nuptime_in_seconds:60\n# Clients\nconnected_clients:4\n# Memory\nused_memory:1024\n# Stats\ninstantaneous_ops_per_sec:7\ntotal_commands_processed:12\n";
                                format!("${}\r\n{body}\r\n", body.len())
                            }
                            (Some(b"INFO"), Some(b"keyspace")) => {
                                if gate_keyspace {
                                    if let Some((entered, resume)) = gate.lock().await.take() {
                                        let _ = entered.send(());
                                        let _ = resume.await;
                                    }
                                }
                                if fail_keyspace {
                                    "-NOPERM keyspace access denied\r\n".into()
                                } else {
                                    let body = "# Keyspace\ndb0:keys=7,expires=2,avg_ttl=100\n";
                                    format!("${}\r\n{body}\r\n", body.len())
                                }
                            }
                            (Some(b"DBSIZE"), _) => ":7\r\n".into(),
                            (Some(b"INFO"), _) => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            (Some(b"MODULE"), _) => "*0\r\n".into(),
                            (Some(b"PING"), _) => "+PONG\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    (
        port,
        cluster_info_calls,
        node_info_commands,
        entered_rx,
        resume_tx,
    )
}

#[tokio::test]
async fn live_topology_redis6_info_falls_back_on_the_same_connection() {
    for error in [
        "ERR syntax error",
        "ERR wrong number of arguments for 'info' command",
        "ERR wrong number of arguments for 'INFO' command",
    ] {
        let (port, cluster_info_calls, commands, _, _) =
            spawn_topology_cluster_info_options(false, true, false, false, Some(error)).await;
        let service = RedisService::new(
            Arc::new(TopologyProfiles(vec![topology_profile(port)])),
            Arc::new(TopologySecrets),
        );
        service.open_connection("topology-cluster").await.unwrap();
        let topology = service
            .get_cluster_topology("topology-cluster")
            .await
            .unwrap();
        assert!(
            topology.failures.is_empty(),
            "{error}: {:?}",
            topology.failures
        );
        assert_eq!(topology.nodes.len(), 3);
        assert!(topology
            .nodes
            .iter()
            .all(|node| node.metrics.used_memory_bytes == Some(1024)));
        assert_eq!(cluster_info_calls.load(Ordering::SeqCst), 1);
        let commands = commands.lock().unwrap();
        assert_eq!(
            commands.iter().filter(|command| command.len() == 7).count(),
            3
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| command.as_slice() == [b"INFO".to_vec()])
                .count(),
            3
        );
    }
}

#[tokio::test]
async fn live_topology_does_not_retry_acl_or_unrelated_info_errors() {
    for error in [
        "NOPERM syntax error",
        "NOPERM this user has no permissions to run the 'info' command",
        "ERR syntax error: unavailable",
        "ERR wrong number of arguments for 'get' command",
        "ERR temporarily unavailable",
    ] {
        let (port, cluster_info_calls, commands, _, _) =
            spawn_topology_cluster_info_options(false, true, false, false, Some(error)).await;
        let service = RedisService::new(
            Arc::new(TopologyProfiles(vec![topology_profile(port)])),
            Arc::new(TopologySecrets),
        );
        service.open_connection("topology-cluster").await.unwrap();
        let topology = service
            .get_cluster_topology("topology-cluster")
            .await
            .unwrap();
        assert_eq!(topology.failures.len(), 3, "{error}");
        assert_eq!(cluster_info_calls.load(Ordering::SeqCst), 1);
        let commands = commands.lock().unwrap();
        assert_eq!(commands.len(), 3, "{error}");
        assert!(commands.iter().all(|command| command.len() == 7));
    }
}

#[tokio::test]
async fn cluster_overviews_aggregate_all_instances_but_only_primary_keyspaces() {
    let (port, _, _, _, _) = spawn_topology_cluster_options(false, true, false, false).await;
    let service = RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    );
    service.open_connection("topology-cluster").await.unwrap();
    let overview = service
        .get_instance_overview("topology-cluster")
        .await
        .unwrap();
    assert_eq!(overview.redis_mode.as_deref(), Some("cluster"));
    assert_eq!(overview.used_memory_bytes, Some(3072));
    assert_eq!(overview.connected_clients, Some(12));
    assert_eq!(overview.total_commands_processed, Some(36));
    assert_eq!(overview.uptime_seconds, None);
    assert_eq!(overview.role, None);
    let databases = service
        .get_database_overview("topology-cluster")
        .await
        .unwrap();
    assert_eq!(databases.len(), 1);
    assert_eq!(databases[0].database, 0);
    assert_eq!(databases[0].key_count, Some(14));
    assert_eq!(databases[0].expires, Some(4));
    assert_eq!(databases[0].avg_ttl_ms, None);
    assert_eq!(
        service.get_instance_details("topology-cluster").await,
        Err(AppError::UnsupportedFeature)
    );
}

#[tokio::test]
async fn cluster_overviews_do_not_present_partial_node_counts_as_complete_totals() {
    let (port, _, _, _, _) = spawn_topology_cluster(false).await;
    let service = RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    );
    service.open_connection("topology-cluster").await.unwrap();
    let overview = service
        .get_instance_overview("topology-cluster")
        .await
        .unwrap();
    assert_eq!(overview.used_memory_bytes, None);
    assert_eq!(overview.connected_clients, None);
    assert_eq!(overview.total_commands_processed, None);
    let databases = service
        .get_database_overview("topology-cluster")
        .await
        .unwrap();
    assert_eq!(databases[0].database, 0);
    assert_eq!(databases[0].key_count, None);
    assert_eq!(databases[0].expires, None);
}

#[tokio::test]
async fn cluster_database_overview_rejects_a_replaced_generation() {
    let (port, _, _, entered, resume) =
        spawn_topology_cluster_options(false, true, true, false).await;
    let service = Arc::new(RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    ));
    service.open_connection("topology-cluster").await.unwrap();
    let request_service = Arc::clone(&service);
    let request = tokio::spawn(async move {
        request_service
            .get_database_overview("topology-cluster")
            .await
    });
    entered.await.unwrap();
    service.open_connection("topology-cluster").await.unwrap();
    resume.send(()).unwrap();
    assert_eq!(request.await.unwrap(), Err(AppError::OperationCancelled));
}

#[tokio::test]
async fn cluster_database_overview_falls_back_to_primary_dbsize_without_inventing_expirations() {
    let (port, _, _, _, _) = spawn_topology_cluster_options(false, true, false, true).await;
    let service = RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    );
    service.open_connection("topology-cluster").await.unwrap();
    let databases = service
        .get_database_overview("topology-cluster")
        .await
        .unwrap();
    assert_eq!(databases.len(), 1);
    assert_eq!(databases[0].database, 0);
    assert_eq!(databases[0].key_count, Some(14));
    assert_eq!(databases[0].expires, None);
    assert_eq!(databases[0].avg_ttl_ms, None);
}

#[test]
fn cluster_overview_missing_and_overflowing_metrics_are_independently_unknown() {
    use redix_lib::domain::{ClusterTopology, DatabaseOverview, InstanceOverview};
    let mut nodes = parse_cluster_nodes("a 127.0.0.1:7000@1 master - 0 0 1 connected 0-8191\nb 127.0.0.1:7001@1 master - 0 0 2 connected 8192-16383\n").unwrap();
    nodes[0].metrics.used_memory_bytes = Some(u64::MAX);
    nodes[1].metrics.used_memory_bytes = Some(1);
    nodes[0].metrics.connected_clients = Some(4);
    nodes[1].metrics.connected_clients = Some(5);
    nodes[0].metrics.commands_processed = Some(10);
    let topology = ClusterTopology {
        summary: parse_cluster_info("cluster_state:ok\n").unwrap(),
        nodes,
        failures: vec![],
    };
    let overview = InstanceOverview::from_cluster_topology(&topology);
    assert_eq!(overview.used_memory_bytes, None);
    assert_eq!(overview.connected_clients, Some(9));
    assert_eq!(overview.total_commands_processed, None);
    let databases = [
        Some(DatabaseOverview {
            database: 0,
            key_count: Some(u64::MAX),
            expires: Some(1),
            avg_ttl_ms: Some(100),
        }),
        Some(DatabaseOverview {
            database: 0,
            key_count: Some(1),
            expires: Some(2),
            avg_ttl_ms: Some(200),
        }),
    ];
    let database = DatabaseOverview::from_cluster_primaries(&databases);
    assert_eq!(database.key_count, None);
    assert_eq!(database.expires, Some(3));
    assert_eq!(database.avg_ttl_ms, None);
    assert_eq!(
        DatabaseOverview::from_cluster_primaries(&[]).key_count,
        None
    );
}

#[tokio::test]
async fn live_topology_falls_back_only_for_unknown_shards_and_keeps_failed_nodes() {
    let (port, cluster_info_calls, node_info_commands, _entered, _resume) =
        spawn_topology_cluster(false).await;
    let service = RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    );
    service.open_connection("topology-cluster").await.unwrap();

    let topology = service
        .get_cluster_topology("topology-cluster")
        .await
        .unwrap();
    let refreshed = service
        .refresh_cluster_topology("topology-cluster")
        .await
        .unwrap();

    assert_eq!(cluster_info_calls.load(Ordering::SeqCst), 2);
    assert_eq!(topology.summary.slots_ok, 16_384);
    assert_eq!(topology.nodes.len(), 2);
    assert_eq!(topology.nodes[0].endpoint.port, port);
    assert_eq!(
        topology.nodes[0].connection_endpoint.as_ref().unwrap().port,
        port
    );
    assert_eq!(topology.nodes[0].metrics.used_memory_bytes, Some(1024));
    assert_eq!(topology.nodes[1].connection_endpoint, None);
    assert_eq!(
        topology.failures,
        vec![NodeFailure {
            node_id: "node-2".into(),
            code: "CLUSTER_NODE_UNAVAILABLE".into(),
        }]
    );
    assert_eq!(refreshed.failures, topology.failures);
    let commands = node_info_commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands.iter().all(|command| {
        command
            == &[
                b"INFO".to_vec(),
                b"server".to_vec(),
                b"clients".to_vec(),
                b"memory".to_vec(),
                b"stats".to_vec(),
                b"replication".to_vec(),
                b"keyspace".to_vec(),
            ]
    }));
}

#[tokio::test]
async fn topology_rejects_a_result_after_the_active_generation_is_closed() {
    let (port, _cluster_info_calls, _node_info_commands, entered, resume) =
        spawn_topology_cluster(true).await;
    let service = Arc::new(RedisService::new(
        Arc::new(TopologyProfiles(vec![topology_profile(port)])),
        Arc::new(TopologySecrets),
    ));
    service.open_connection("topology-cluster").await.unwrap();

    let request_service = Arc::clone(&service);
    let request = tokio::spawn(async move {
        request_service
            .get_cluster_topology("topology-cluster")
            .await
    });
    entered.await.unwrap();
    service.close_connection("topology-cluster").await.unwrap();
    resume.send(()).unwrap();

    assert_eq!(request.await.unwrap(), Err(AppError::OperationCancelled));
}

#[test]
fn shards_parser_counts_resp_framing_in_its_conservative_size_bound() {
    let padding = Value::BulkString(vec![b'x'; 4 * 1024 * 1024 - 245]);
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(1)]),
        bulk("padding"),
        padding,
        bulk("nodes"),
        Value::Array(vec![Value::Array(vec![
            bulk("id"),
            bulk("node"),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("port"),
            Value::Int(6379),
            bulk("health"),
            bulk("online"),
        ])]),
    ])]);

    assert_eq!(
        parse_cluster_shards(reply),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn decoded_structural_envelope_counts_unknown_push_kind_text() {
    let reply = Value::Push {
        kind: redis::PushKind::Other("x".repeat(4 * 1024 * 1024)),
        data: vec![],
    };
    assert_eq!(
        parse_cluster_shards(reply),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[test]
fn decoded_structural_envelope_counts_unknown_verbatim_format_text() {
    let reply = Value::Array(vec![Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(1)]),
        bulk("ignored"),
        Value::VerbatimString {
            format: redis::VerbatimFormat::Unknown("x".repeat(4 * 1024 * 1024)),
            text: "value".into(),
        },
        bulk("nodes"),
        Value::Array(vec![Value::Array(vec![
            bulk("id"),
            bulk("node"),
            bulk("role"),
            bulk("master"),
            bulk("endpoint"),
            bulk("cache.internal"),
            bulk("port"),
            Value::Int(6379),
            bulk("health"),
            bulk("online"),
        ])]),
    ])]);
    assert_eq!(
        parse_cluster_shards(reply),
        Err(AppError::ClusterTopologyFailed)
    );
}
