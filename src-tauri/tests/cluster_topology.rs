use redis::Value;
use redix_lib::{
    domain::{ClusterNodeHealth, ClusterNodeRole, SlotRange},
    error::AppError,
    redis::{merge_node_metrics, parse_cluster_info, parse_cluster_nodes, parse_cluster_shards},
};

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
