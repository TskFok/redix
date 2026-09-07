use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use redix_lib::domain::{
    parse_command_stats, AnalysisAccumulator, AnalysisKeyMetadata, AnalysisProgress,
    AnalyzeDatabaseInput, ClusterConfig, ConnectionEndpoint, ConnectionProfile, InstanceDetails,
    ModuleSummary, NodeFailure,
};
use redix_lib::{
    error::AppError,
    persistence::ProfileRepository,
    redis::{
        allocate_primary_key_limits, merge_node_reports, NodeAnalysisState, RedisOperations,
        RedisService,
    },
};

mod fixtures {
    use redix_lib::{
        domain::ConnectionProfile,
        error::AppError,
        persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    };

    #[derive(Default)]
    pub struct TestProfiles {
        pub profiles: Vec<ConnectionProfile>,
    }

    impl ProfileRepository for TestProfiles {
        fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
            Ok(self.profiles.clone())
        }

        fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct TestSecrets;

    impl SecretStore for TestSecrets {
        fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(None)
        }

        fn write(
            &self,
            _connection_id: &str,
            _secrets: &ConnectionSecrets,
        ) -> Result<(), AppError> {
            Ok(())
        }

        fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
            Ok(())
        }
    }
}

use fixtures::{TestProfiles, TestSecrets};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn cluster_profile(port: u16) -> ConnectionProfile {
    ConnectionProfile {
        ssh: None,
        sentinel: None,
        cluster: Some(ClusterConfig {
            nodes: vec![ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port,
            }],
            read_from_replicas: false,
        }),
        id: "cluster-analysis".into(),
        name: "Cluster analysis".into(),
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
    }
}

async fn spawn_analysis_cluster_with_keys(
    key_count: usize,
) -> (
    u16,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let unavailable_port = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (scan_entered_tx, scan_entered_rx) = tokio::sync::oneshot::channel();
    let (scan_resume_tx, scan_resume_rx) = tokio::sync::oneshot::channel();
    let scan_gate = Arc::new(tokio::sync::Mutex::new(Some((
        scan_entered_tx,
        scan_resume_rx,
    ))));
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let scan_gate = Arc::clone(&scan_gate);
            tokio::spawn(async move {
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
                        let response: String = match (
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
                                let body = format!(
                                    "analysis-node 127.0.0.1:{port}@1 master - 0 0 1 connected 0-8191\nfailed-node 127.0.0.1:{unavailable_port}@1 master - 0 0 2 connected 8192-16383\n"
                                );
                                format!("${}\r\n{body}\r\n", body.len())
                            }
                            (Some(b"PING"), _) => "+PONG\r\n".into(),
                            (Some(b"INFO"), _)
                                if command.get(1).map(Vec::as_slice) == Some(b"keyspace") =>
                            {
                                "-ERR keyspace unavailable\r\n".into()
                            }
                            (Some(b"INFO"), _) => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            (Some(b"MODULE"), _) => "*0\r\n".into(),
                            (Some(b"SCAN"), _) => {
                                if let Some((entered, resume)) = scan_gate.lock().await.take() {
                                    let _ = entered.send(());
                                    let _ = resume.await;
                                }
                                let mut response = format!("*2\r\n$1\r\n0\r\n*{key_count}\r\n");
                                for index in 1..=key_count {
                                    let key = format!("key:{index}");
                                    response.push_str(&format!("${}\r\n{key}\r\n", key.len()));
                                }
                                response
                            }
                            (Some(b"TYPE"), _) => "+string\r\n".into(),
                            (Some(b"MEMORY"), _) => ":64\r\n".into(),
                            (Some(b"TTL"), _) => ":-1\r\n".into(),
                            (Some(b"STRLEN"), _) => ":3\r\n".into(),
                            (Some(b"DBSIZE"), _) => ":7\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    (port, scan_entered_rx, scan_resume_tx)
}

async fn spawn_select_gated_analysis_server() -> (
    u16,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let select_count = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let gate = Arc::new(tokio::sync::Mutex::new(Some((entered_tx, resume_rx))));
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let select_count = Arc::clone(&select_count);
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
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
                        let response: String = match command.first().map(Vec::as_slice) {
                            Some(b"SELECT") => {
                                if select_count.fetch_add(1, Ordering::SeqCst) == 1 {
                                    if let Some((entered, resume)) = gate.lock().await.take() {
                                        entered.send(()).unwrap();
                                        resume.await.unwrap();
                                    }
                                }
                                "+OK\r\n".into()
                            }
                            Some(b"PING") => "+PONG\r\n".into(),
                            Some(b"INFO")
                                if command.get(1).map(Vec::as_slice) == Some(b"keyspace") =>
                            {
                                "-ERR keyspace unavailable\r\n".into()
                            }
                            Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            Some(b"SCAN") => "*2\r\n$1\r\n0\r\n*1\r\n$5\r\nkey:1\r\n".into(),
                            Some(b"TYPE") => "+string\r\n".into(),
                            Some(b"MEMORY") => ":64\r\n".into(),
                            Some(b"TTL") => ":-1\r\n".into(),
                            Some(b"STRLEN") => ":3\r\n".into(),
                            Some(b"DBSIZE") => ":7\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    (port, entered_rx, resume_tx)
}

struct MutableProfiles(Mutex<Vec<ConnectionProfile>>);

impl ProfileRepository for MutableProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        *self.0.lock().unwrap() = profiles.to_vec();
        Ok(())
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

#[tokio::test]
async fn cluster_routes_existing_database_analysis_helpers() {
    let (port, scan_entered, scan_resume) = spawn_analysis_cluster_with_keys(1).await;
    let service = Arc::new(RedisService::new(
        Arc::new(TestProfiles {
            profiles: vec![cluster_profile(port)],
        }),
        Arc::new(TestSecrets),
    ));
    service.open_connection("cluster-analysis").await.unwrap();

    let request_service = Arc::clone(&service);
    let request = tokio::spawn(async move {
        request_service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "cluster-analysis".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
    });
    scan_entered.await.unwrap();
    scan_resume.send(()).unwrap();
    let report = request.await.unwrap().unwrap();
    assert_eq!(report.database, 0);
    assert_eq!(report.progress.processed, 1);
    assert_eq!(report.total_memory.total, 64);
    assert_eq!(report.node_results.len(), 1);
    assert_eq!(report.node_results[0].node_id, "analysis-node");
    assert!(report.node_results[0].report.node_results.is_empty());
    assert!(report.node_results[0].report.failed_nodes.is_empty());
    assert_eq!(
        report.failed_nodes,
        vec![NodeFailure {
            node_id: "failed-node".into(),
            code: "CLUSTER_NODE_UNAVAILABLE".into(),
        }]
    );
    assert!(report.progress.truncated);
}

#[tokio::test]
async fn cluster_analysis_rejects_a_result_after_the_active_generation_is_closed() {
    let (port, scan_entered, scan_resume) = spawn_analysis_cluster_with_keys(1).await;
    let service = Arc::new(RedisService::new(
        Arc::new(TestProfiles {
            profiles: vec![cluster_profile(port)],
        }),
        Arc::new(TestSecrets),
    ));
    service.open_connection("cluster-analysis").await.unwrap();

    let request_service = Arc::clone(&service);
    let request = tokio::spawn(async move {
        request_service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "cluster-analysis".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
    });
    scan_entered.await.unwrap();
    service.close_connection("cluster-analysis").await.unwrap();
    scan_resume.send(()).unwrap();

    assert_eq!(request.await.unwrap(), Err(AppError::OperationCancelled));
}

#[tokio::test]
async fn cluster_analysis_limits_a_complete_scan_page_to_its_primary_quota() {
    let (port, scan_entered, scan_resume) = spawn_analysis_cluster_with_keys(700).await;
    let service = Arc::new(RedisService::new(
        Arc::new(TestProfiles {
            profiles: vec![cluster_profile(port)],
        }),
        Arc::new(TestSecrets),
    ));
    service.open_connection("cluster-analysis").await.unwrap();
    let request_service = Arc::clone(&service);
    let request = tokio::spawn(async move {
        request_service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "cluster-analysis".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
    });
    scan_entered.await.unwrap();
    scan_resume.send(()).unwrap();
    let report = request.await.unwrap().unwrap();
    // Two advertised primaries receive 500 keys each; one is unavailable.
    assert_eq!(report.progress.scanned, 700);
    assert_eq!(report.progress.processed, 500);
    assert_eq!(report.total_keys.total, 500);
    assert_eq!(report.total_memory.total, 32_000);
    assert_eq!(report.node_results[0].report.progress.max_keys, 500);
    assert_eq!(report.node_results[0].report.progress.processed, 500);
    assert!(report.node_results[0].report.progress.truncated);
    assert_eq!(report.progress.max_keys, 1_000);
}

fn node_report(node_id: &str, keys: &[(&str, u64)]) -> NodeAnalysisState {
    let mut accumulator = AnalysisAccumulator::new(0, "*".into(), ":".into(), 500);
    for (key, memory_bytes) in keys {
        accumulator.process(AnalysisKeyMetadata {
            key: (*key).into(),
            key_type: "string".into(),
            length: Some(1),
            memory_bytes: Some(*memory_bytes),
            ttl_seconds: -1,
        });
    }
    NodeAnalysisState {
        node_id: node_id.into(),
        endpoint: ConnectionEndpoint {
            host: format!("{node_id}.example"),
            port: 7000,
        },
        accumulator,
        progress: AnalysisProgress {
            scanned: keys.len() as u64,
            processed: keys.len() as u64,
            max_keys: 500,
            truncated: false,
        },
    }
}

#[test]
fn cluster_analysis_merges_successful_primaries_without_recursive_reports() {
    let report = merge_node_reports(
        vec![
            node_report("node-2", &[("orders:2", 20), ("orders:3", 30)]),
            node_report("node-1", &[("orders:1", 10)]),
        ],
        vec![NodeFailure {
            node_id: "node-3".into(),
            code: "CLUSTER_NODE_UNAVAILABLE".into(),
        }],
    );

    assert_eq!(report.total_keys.total, 3);
    assert_eq!(report.total_memory.total, 60);
    assert_eq!(report.progress.scanned, 3);
    assert_eq!(report.progress.processed, 3);
    assert!(report.progress.truncated);
    assert_eq!(report.top_namespaces_by_keys[0].namespace, "orders");
    assert_eq!(report.top_namespaces_by_keys[0].keys, 3);
    assert_eq!(report.node_results[0].node_id, "node-1");
    assert_eq!(report.node_results[1].node_id, "node-2");
    assert!(report
        .node_results
        .iter()
        .all(|node| node.report.node_results.is_empty() && node.report.failed_nodes.is_empty()));
    assert_eq!(report.failed_nodes[0].node_id, "node-3");
}

#[test]
fn cluster_analysis_ranks_namespaces_after_merging_all_node_accumulators() {
    let mut nodes = Vec::new();
    for node_index in 0..2 {
        let mut keys = Vec::new();
        // Each node's 15 local leaders hide the shared namespace in both rankings.
        for namespace in 0..15 {
            for key in 0..3 {
                keys.push((format!("node{node_index}-ns{namespace}:{key}"), 1));
            }
        }
        keys.push(("shared:1".to_owned(), 1));
        keys.push(("shared:2".to_owned(), 1));
        let borrowed = keys
            .iter()
            .map(|(key, bytes)| (key.as_str(), *bytes))
            .collect::<Vec<_>>();
        nodes.push(node_report(&format!("node-{node_index}"), &borrowed));
    }
    let report = merge_node_reports(nodes, vec![]);
    assert_eq!(report.top_namespaces_by_keys[0].namespace, "shared");
    assert_eq!(report.top_namespaces_by_keys[0].keys, 4);
    assert_eq!(report.top_namespaces_by_memory[0].namespace, "shared");
    assert_eq!(report.top_namespaces_by_memory[0].memory_bytes, 4);
    assert_eq!(report.top_namespaces_by_keys.len(), 15);
    assert!(report.node_results.iter().all(|node| {
        node.report.top_namespaces_by_keys.len() == 15
            && node
                .report
                .top_namespaces_by_keys
                .iter()
                .all(|item| item.namespace != "shared")
            && node
                .report
                .top_namespaces_by_memory
                .iter()
                .all(|item| item.namespace != "shared")
    }));
}

#[test]
fn cluster_analysis_preserves_namespaces_only_ranked_by_memory() {
    let mut keys = (0..15)
        .flat_map(|namespace| (0..2).map(move |key| (format!("small{namespace}:{key}"), 1)))
        .collect::<Vec<_>>();
    keys.push(("large:1".to_owned(), 1_000));
    let borrowed = keys
        .iter()
        .map(|(key, bytes)| (key.as_str(), *bytes))
        .collect::<Vec<_>>();
    let report = merge_node_reports(vec![node_report("node-1", &borrowed)], vec![]);
    assert_eq!(report.top_namespaces_by_memory[0].namespace, "large");
    assert_eq!(report.top_namespaces_by_memory[0].memory_bytes, 1_000);
    assert!(report
        .top_namespaces_by_keys
        .iter()
        .all(|item| item.namespace != "large"));
}

#[test]
fn analysis_memory_counters_saturate_within_and_across_nodes() {
    let report = merge_node_reports(
        vec![
            node_report("node-1", &[("huge:1", u64::MAX), ("huge:2", 1)]),
            node_report("node-2", &[("huge:3", 1)]),
        ],
        vec![],
    );
    assert_eq!(report.total_memory.total, u64::MAX);
    assert_eq!(report.total_memory.observed, 3);
    assert_eq!(report.total_memory.types[0].total, u64::MAX);
    assert_eq!(report.top_namespaces_by_memory[0].memory_bytes, u64::MAX);
    assert_eq!(report.top_namespaces_by_memory[0].keys, 3);
    assert_eq!(report.expiration_groups[0].memory_bytes, u64::MAX);
    assert_eq!(report.node_results[0].report.total_memory.total, u64::MAX);
}

#[test]
fn old_standalone_analysis_json_defaults_node_arrays_to_empty() {
    let report = AnalysisAccumulator::new(0, "*".into(), ":".into(), 1_000).finish(0, 0, false);
    let mut value = serde_json::to_value(report).unwrap();
    value.as_object_mut().unwrap().remove("node_results");
    value.as_object_mut().unwrap().remove("failed_nodes");

    let restored =
        serde_json::from_value::<redix_lib::domain::DatabaseAnalysisReport>(value).unwrap();
    assert!(restored.node_results.is_empty());
    assert!(restored.failed_nodes.is_empty());
}

#[test]
fn cluster_analysis_divides_the_key_budget_fairly_without_losing_remainder() {
    assert_eq!(
        allocate_primary_key_limits(1_000, 3).unwrap(),
        vec![334, 333, 333]
    );
    assert_eq!(
        allocate_primary_key_limits(1_000, 0),
        Err(AppError::ClusterTopologyFailed)
    );
    assert_eq!(
        allocate_primary_key_limits(1_000, 129),
        Err(AppError::ClusterTopologyFailed)
    );
}

#[tokio::test]
async fn database_analysis_keeps_database_annotation_with_its_selected_handle_during_replace() {
    let (port, analyze_selected, resume_analyze) = spawn_select_gated_analysis_server().await;
    let mut profile = cluster_profile(port);
    profile.id = "analysis-race".into();
    profile.cluster = None;
    profile.database = 1;
    let profiles = Arc::new(MutableProfiles(Mutex::new(vec![profile])));
    let service = Arc::new(RedisService::new(profiles, Arc::new(TestSecrets)));
    service.open_connection("analysis-race").await.unwrap();

    let analysis_service = Arc::clone(&service);
    let analysis = tokio::spawn(async move {
        analysis_service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "analysis-race".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
    });
    analyze_selected.await.unwrap();
    service
        .select_database(redix_lib::domain::SelectDatabaseInput {
            connection_id: "analysis-race".into(),
            database: 2,
        })
        .await
        .unwrap();
    resume_analyze.send(()).unwrap();

    assert_eq!(analysis.await.unwrap().unwrap().database, 1);
}

#[tokio::test]
async fn database_overview_fallback_keeps_database_with_its_selected_handle_during_replace() {
    let (port, overview_selected, resume_overview) = spawn_select_gated_analysis_server().await;
    let mut profile = cluster_profile(port);
    profile.id = "overview-race".into();
    profile.cluster = None;
    profile.database = 1;
    let profiles = Arc::new(MutableProfiles(Mutex::new(vec![profile])));
    let service = Arc::new(RedisService::new(profiles, Arc::new(TestSecrets)));
    service.open_connection("overview-race").await.unwrap();

    let overview_service = Arc::clone(&service);
    let overview = tokio::spawn(async move {
        overview_service
            .get_database_overview("overview-race")
            .await
    });
    overview_selected.await.unwrap();
    service
        .select_database(redix_lib::domain::SelectDatabaseInput {
            connection_id: "overview-race".into(),
            database: 2,
        })
        .await
        .unwrap();
    resume_overview.send(()).unwrap();

    assert_eq!(overview.await.unwrap().unwrap()[0].database, 1);
}

#[tokio::test]
async fn analysis_and_details_require_an_open_connection() {
    let service = RedisService::new(
        Arc::new(TestProfiles::default()),
        Arc::new(TestSecrets::default()),
    );
    assert_eq!(
        service.get_instance_details("missing").await.unwrap_err(),
        AppError::ConnectionFailed
    );
    assert_eq!(
        service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "missing".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1000,
            })
            .await
            .unwrap_err(),
        AppError::ConnectionFailed
    );
}

#[test]
fn validates_analysis_input_limits() {
    let mut input = AnalyzeDatabaseInput {
        connection_id: "local".into(),
        pattern: "*".into(),
        delimiter: ":".into(),
        max_keys: 100_000,
    };
    assert_eq!(input.validate(), Ok(()));

    input.max_keys = 1_000_000;
    assert_eq!(input.validate(), Ok(()));

    input.max_keys = 1_000_001;
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");

    input.max_keys = 999;
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
    input.max_keys = 100_000;
    input.delimiter = String::new();
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
}

#[test]
fn aggregates_types_namespaces_top_keys_memory_coverage_and_ttl_groups() {
    let mut accumulator = AnalysisAccumulator::new(0, "*".into(), ":".into(), 1000);
    accumulator.process(AnalysisKeyMetadata {
        key: "user:1".into(),
        key_type: "string".into(),
        length: Some(12),
        memory_bytes: Some(128),
        ttl_seconds: -1,
    });
    accumulator.process(AnalysisKeyMetadata {
        key: "user:2".into(),
        key_type: "hash".into(),
        length: Some(4),
        memory_bytes: Some(256),
        ttl_seconds: 120,
    });
    let report = accumulator.finish(3, 2, false);
    assert_eq!(report.database, 0);
    assert_eq!(report.progress.scanned, 3);
    assert_eq!(report.progress.processed, 2);
    assert_eq!(report.total_keys.total, 2);
    assert_eq!(report.total_keys.observed, 2);
    assert_eq!(report.total_memory.total, 384);
    assert_eq!(report.total_memory.observed, 2);
    assert_eq!(report.top_namespaces_by_keys[0].namespace, "user");
    assert_eq!(report.top_keys_by_memory[0].key, "user:2");
    assert!(report
        .expiration_groups
        .iter()
        .any(|group| group.label == "No Expiry"));
    assert!(!report.progress.truncated);
}

#[test]
fn parses_commandstats_and_optional_instance_metrics_without_raw_text() {
    let mut sections = HashMap::new();
    sections.insert(
        "Server".into(),
        HashMap::from([("redis_version".into(), "7.2.5".into())]),
    );
    sections.insert(
        "Clients".into(),
        HashMap::from([
            ("connected_clients".into(), "3".into()),
            ("blocked_clients".into(), "1".into()),
        ]),
    );
    sections.insert(
        "Commandstats".into(),
        HashMap::from([(
            "cmdstat_get".into(),
            "calls=4,usec=20,usec_per_call=5.0,rejected_calls=0,failed_calls=1".into(),
        )]),
    );

    let details = InstanceDetails::from_info_and_modules(
        &sections,
        vec![ModuleSummary {
            name: "ReJSON".into(),
            version: Some("2.8.10".into()),
        }],
    )
    .unwrap();
    assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
    assert_eq!(details.clients.blocked_clients, Some(1));
    assert_eq!(parse_command_stats(&sections)[0].command, "GET");
}

#[test]
fn maps_standard_connected_slaves_to_connected_replicas() {
    let sections = HashMap::from([(
        "Replication".into(),
        HashMap::from([("connected_slaves".into(), "2".into())]),
    )]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.replication.connected_replicas, Some(2));
}

#[test]
fn malformed_optional_info_fields_degrade_independently() {
    let sections = HashMap::from([
        (
            "Server".into(),
            HashMap::from([
                ("redis_version".into(), "7.2.5".into()),
                ("uptime_in_seconds".into(), "not-a-number".into()),
            ]),
        ),
        (
            "Clients".into(),
            HashMap::from([
                ("connected_clients".into(), "invalid".into()),
                ("blocked_clients".into(), "1".into()),
            ]),
        ),
        (
            "Memory".into(),
            HashMap::from([
                ("used_memory".into(), "invalid".into()),
                ("used_memory_peak".into(), "2048".into()),
                ("mem_fragmentation_ratio".into(), "NaN".into()),
            ]),
        ),
        (
            "Stats".into(),
            HashMap::from([
                ("total_commands_processed".into(), "invalid".into()),
                ("keyspace_hits".into(), "invalid".into()),
                ("keyspace_misses".into(), "2".into()),
                ("instantaneous_ops_per_sec".into(), "invalid".into()),
                ("expired_keys".into(), "4".into()),
            ]),
        ),
        (
            "Persistence".into(),
            HashMap::from([
                ("loading".into(), "invalid".into()),
                ("rdb_last_save_time".into(), "invalid".into()),
                ("aof_enabled".into(), "1".into()),
            ]),
        ),
        (
            "Replication".into(),
            HashMap::from([
                ("role".into(), "master".into()),
                ("connected_slaves".into(), "3".into()),
                ("master_repl_offset".into(), "invalid".into()),
            ]),
        ),
    ]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
    assert_eq!(details.overview.uptime_seconds, None);
    assert_eq!(details.overview.connected_clients, None);
    assert_eq!(details.overview.used_memory_bytes, None);
    assert_eq!(details.overview.total_commands_processed, None);
    assert_eq!(details.clients.blocked_clients, Some(1));
    assert_eq!(details.memory.used_memory_bytes, None);
    assert_eq!(details.memory.used_memory_peak_bytes, Some(2048));
    assert_eq!(details.memory.mem_fragmentation_ratio, None);
    assert_eq!(details.stats.instantaneous_ops_per_sec, None);
    assert_eq!(details.stats.expired_keys, Some(4));
    assert_eq!(details.stats.hit_rate, None);
    assert_eq!(details.persistence.loading, None);
    assert_eq!(details.persistence.rdb_last_save_time, None);
    assert_eq!(details.persistence.aof_enabled, Some(true));
    assert_eq!(details.replication.role.as_deref(), Some("master"));
    assert_eq!(details.replication.connected_replicas, Some(3));
    assert_eq!(details.replication.master_repl_offset, None);
}

#[test]
fn degrades_hit_rate_to_none_when_info_counters_overflow() {
    let sections = HashMap::from([(
        "Stats".into(),
        HashMap::from([
            ("keyspace_hits".into(), u64::MAX.to_string()),
            ("keyspace_misses".into(), "1".into()),
        ]),
    )]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.stats.hit_rate, None);
}
