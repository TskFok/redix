use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    domain::{
        AnalyzeDatabaseInput, CliCommandInput, CliSessionInput, ClusterConfig, ClusterNodeRole,
        ConnectionEndpoint, ConnectionProfile, ExecuteCommandsInput, GetSlowLogsInput,
        PublishPubSubInput, ScanCursor, ScanKeysInput, UpdateSlowLogConfigInput,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{CliManager, RedisOperations, RedisService},
};

struct Profiles(Vec<ConnectionProfile>);

impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.clone())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct Secrets;

impl SecretStore for Secrets {
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

fn cluster_service(seeds: &str) -> RedisService {
    let nodes = seeds
        .split(',')
        .map(|seed| {
            let url = redis::Client::open(seed.trim())
                .expect("REDIX_TEST_REDIS_CLUSTER_URLS must contain valid Redis URLs");
            match url.get_connection_info().addr() {
                redis::ConnectionAddr::Tcp(host, port) => ConnectionEndpoint {
                    host: host.clone(),
                    port: *port,
                },
                _ => panic!("REDIX_TEST_REDIS_CLUSTER_URLS must use plain TCP URLs"),
            }
        })
        .collect::<Vec<_>>();
    let first = nodes
        .first()
        .expect("REDIX_TEST_REDIS_CLUSTER_URLS must contain at least one seed")
        .clone();
    let profile = ConnectionProfile {
        ssh: None,
        sentinel: None,
        cluster: Some(ClusterConfig {
            nodes,
            read_from_replicas: false,
        }),
        id: "cluster".into(),
        name: "Isolated Cluster".into(),
        host: first.host,
        port: first.port,
        username: None,
        database: 0,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    };
    RedisService::new(Arc::new(Profiles(vec![profile])), Arc::new(Secrets))
}

fn redis_slot(key: &str) -> u16 {
    let bytes = key.as_bytes();
    let hashed = bytes
        .iter()
        .position(|byte| *byte == b'{')
        .and_then(|start| {
            let suffix = &bytes[start + 1..];
            suffix
                .iter()
                .position(|byte| *byte == b'}')
                .filter(|end| *end > 0)
                .map(|end| &suffix[..end])
        })
        .unwrap_or(bytes);
    let mut crc = 0_u16;
    for byte in hashed {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc % 16_384
}

async fn prepare_node_slow_logs(seeds: &str) -> Vec<(String, i64, u64, u64)> {
    let mut states = Vec::new();
    for seed in seeds.split(',') {
        let client = redis::Client::open(seed.trim()).unwrap();
        let mut connection = client.get_multiplexed_async_connection().await.unwrap();
        let config = redis::cmd("CONFIG")
            .arg("GET")
            .arg("slowlog-max-len")
            .arg("slowlog-log-slower-than")
            .query_async::<HashMap<String, String>>(&mut connection)
            .await
            .unwrap();
        let max_len = config["slowlog-max-len"].parse::<u64>().unwrap();
        let threshold = config["slowlog-log-slower-than"].parse::<i64>().unwrap();
        redis::cmd("CONFIG")
            .arg("SET")
            .arg("slowlog-log-slower-than")
            .arg(0)
            .query_async::<String>(&mut connection)
            .await
            .unwrap();
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .unwrap();
        redis::cmd("CONFIG")
            .arg("SET")
            .arg("slowlog-log-slower-than")
            .arg(threshold)
            .query_async::<String>(&mut connection)
            .await
            .unwrap();
        let length = redis::cmd("SLOWLOG")
            .arg("LEN")
            .query_async::<u64>(&mut connection)
            .await
            .unwrap();
        assert!(length > 0);
        states.push((seed.trim().to_owned(), threshold, max_len, length));
    }
    states
}

async fn read_node_slow_log_state(seeds: &str) -> Vec<(String, i64, u64, u64)> {
    let mut states = Vec::new();
    for seed in seeds.split(',') {
        let client = redis::Client::open(seed.trim()).unwrap();
        let mut connection = client.get_multiplexed_async_connection().await.unwrap();
        let config = redis::cmd("CONFIG")
            .arg("GET")
            .arg("slowlog-max-len")
            .arg("slowlog-log-slower-than")
            .query_async::<HashMap<String, String>>(&mut connection)
            .await
            .unwrap();
        let length = redis::cmd("SLOWLOG")
            .arg("LEN")
            .query_async::<u64>(&mut connection)
            .await
            .unwrap();
        states.push((
            seed.trim().to_owned(),
            config["slowlog-log-slower-than"].parse().unwrap(),
            config["slowlog-max-len"].parse().unwrap(),
            length,
        ));
    }
    states
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "由 scripts/test-local-cluster.py 设置 REDIX_TEST_REDIS_CLUSTER_URLS"]
async fn cluster_routes_scans_all_primaries_and_reports_topology_and_analysis() {
    let seeds = std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS")
        .expect("cluster launcher must set REDIX_TEST_REDIS_CLUSTER_URLS");
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();

    let topology = service.get_cluster_topology("cluster").await.unwrap();
    assert_eq!(topology.summary.slots_ok, 16_384);
    assert_eq!(topology.summary.slots_assigned, 16_384);
    assert!(topology.failures.is_empty());
    let primary_nodes = topology
        .nodes
        .iter()
        .filter(|node| node.role == ClusterNodeRole::Primary)
        .collect::<Vec<_>>();
    assert_eq!(primary_nodes.len(), 3);

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!("redix:task10:{suffix}");
    let mut keys_by_primary = HashMap::<String, Vec<String>>::new();
    for candidate in 0..100_000 {
        let key = format!("{prefix}:{{slot-{candidate}}}:key");
        let slot = redis_slot(&key);
        let owner = primary_nodes
            .iter()
            .find(|node| {
                node.slots
                    .iter()
                    .any(|range| range.start <= slot && slot <= range.end)
            })
            .expect("every key slot must have a primary owner");
        let owner_keys = keys_by_primary.entry(owner.id.clone()).or_default();
        if owner_keys.len() < 3 {
            owner_keys.push(key);
        }
        if keys_by_primary.len() == 3 && keys_by_primary.values().all(|keys| keys.len() == 3) {
            break;
        }
    }
    assert_eq!(keys_by_primary.len(), 3);
    assert!(keys_by_primary.values().all(|keys| keys.len() == 3));
    let keys = keys_by_primary
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let flow = async {
        for (index, key) in keys.iter().enumerate() {
            service
                .execute_command("cluster", &format!("SET {key} value-{index}"))
                .await
                .unwrap();
            let value = service
                .execute_command("cluster", &format!("GET {key}"))
                .await
                .unwrap();
            assert_eq!(value.value, serde_json::json!(format!("value-{index}")));
        }

        let mut cursor = ScanCursor::Standalone(0);
        let mut found = HashSet::new();
        let mut observed_opaque_has_more = false;
        let mut scan_completed = false;
        for _ in 0..512 {
            let page = service
                .scan_keys(ScanKeysInput {
                    connection_id: "cluster".into(),
                    cursor,
                    pattern: format!("{prefix}:*"),
                    count: 1,
                    key_type: None,
                })
                .await
                .unwrap();
            assert!(page.node_failures.is_empty());
            found.extend(page.keys.into_iter().map(|key| key.key));
            match &page.cursor {
                ScanCursor::Cluster(value) => {
                    assert!(value.starts_with("cluster:"));
                    observed_opaque_has_more |= page.has_more;
                }
                ScanCursor::Standalone(_) => panic!("Cluster SCAN must return an opaque cursor"),
            }
            cursor = page.cursor;
            if !page.has_more {
                scan_completed = true;
                break;
            }
        }
        assert!(
            scan_completed,
            "Cluster SCAN did not terminate within 512 pages"
        );
        assert!(observed_opaque_has_more);
        assert_eq!(found, keys.iter().cloned().collect());

        let analysis = service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "cluster".into(),
                pattern: format!("{prefix}:*"),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
            .unwrap();
        assert_eq!(analysis.database, 0);
        assert_eq!(analysis.total_keys.total, keys.len() as u64);
        assert!(analysis.failed_nodes.is_empty());
        assert_eq!(analysis.node_results.len(), 3);
        let analyzed_nodes = analysis
            .node_results
            .iter()
            .map(|result| result.node_id.as_str())
            .collect::<HashSet<_>>();
        let expected_nodes = primary_nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(analyzed_nodes, expected_nodes);

        let source = &keys_by_primary[&primary_nodes[0].id][0];
        let target = &keys_by_primary[&primary_nodes[1].id][0];
        assert_eq!(
            service
                .execute_command("cluster", &format!("RENAME {source} {target}"))
                .await
                .unwrap_err(),
            AppError::CrossSlot
        );
    }
    .await;

    for key in &keys {
        let _ = service
            .execute_command("cluster", &format!("DEL {key}"))
            .await;
    }
    service.close_connection("cluster").await.unwrap();
    flow
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "由 scripts/test-local-cluster.py 设置 REDIX_TEST_REDIS_CLUSTER_URLS"]
async fn cluster_rejects_socket_state_commands_without_polluting_routed_operations() {
    let seeds = std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS")
        .expect("cluster launcher must set REDIX_TEST_REDIS_CLUSTER_URLS");
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();

    let slow_log_before = prepare_node_slow_logs(&seeds).await;
    assert_eq!(
        service.get_slow_log_config("cluster").await.unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(
        service
            .get_slow_logs(GetSlowLogsInput {
                connection_id: "cluster".into(),
                count: 20,
            })
            .await
            .unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(
        service
            .update_slow_log_config(UpdateSlowLogConfigInput {
                connection_id: "cluster".into(),
                slowlog_max_len: Some(1),
                slowlog_log_slower_than: Some(0),
            })
            .await
            .unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(
        service.clear_slow_logs("cluster").await.unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(
        service
            .publish_pub_sub(PublishPubSubInput {
                connection_id: "cluster".into(),
                channel: "redix-task10".into(),
                message: "must-not-publish".into(),
            })
            .await
            .unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(read_node_slow_log_state(&seeds).await, slow_log_before);

    let topology = service.get_cluster_topology("cluster").await.unwrap();
    let primaries = topology
        .nodes
        .iter()
        .filter(|node| node.role == ClusterNodeRole::Primary)
        .collect::<Vec<_>>();
    let prefix = format!("redix:task10:probe:{}", uuid::Uuid::new_v4());
    let mut keys = HashMap::<String, String>::new();
    for candidate in 0..100_000 {
        let key = format!("{prefix}:{{slot-{candidate}}}");
        let slot = redis_slot(&key);
        let owner = primaries
            .iter()
            .find(|node| {
                node.slots
                    .iter()
                    .any(|range| range.start <= slot && slot <= range.end)
            })
            .unwrap();
        keys.entry(owner.id.clone()).or_insert(key);
        if keys.len() == primaries.len() {
            break;
        }
    }
    for key in keys.values() {
        service
            .execute_command("cluster", &format!("SET {key} baseline"))
            .await
            .unwrap();
    }

    let cli = CliManager::new();
    let session = CliSessionInput {
        connection_id: "cluster".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
    };
    cli.open(&service, session.clone()).await.unwrap();
    assert_eq!(
        cli.execute(CliCommandInput {
            connection_id: "cluster".into(),
            session_id: session.session_id.clone(),
            command: "MULTI".into(),
        })
        .await
        .unwrap_err(),
        AppError::UnsupportedFeature
    );
    assert_eq!(
        service
            .execute_command("cluster", "MULTI")
            .await
            .unwrap_err(),
        AppError::UnsupportedFeature
    );
    let batch = service
        .execute_commands(ExecuteCommandsInput {
            connection_id: "cluster".into(),
            commands: vec!["MULTI".into(), "GET harmless".into()],
            continue_on_error: true,
        })
        .await
        .unwrap();
    assert_eq!(batch[0].error_code.as_deref(), Some("UNSUPPORTED_FEATURE"));
    assert!(batch[1].error_code.is_none());
    for (node_id, key) in &keys {
        let get = service
            .execute_command("cluster", &format!("GET {key}"))
            .await;
        assert_eq!(
            get.unwrap().value,
            serde_json::json!("baseline"),
            "routed command was polluted on node {node_id}"
        );
    }
    let cli_key = keys.values().next().unwrap();
    let set = cli
        .execute(CliCommandInput {
            connection_id: "cluster".into(),
            session_id: session.session_id.clone(),
            command: format!("SET {cli_key} cli-value"),
        })
        .await
        .unwrap();
    assert!(set.error_code.is_none());
    let get = cli
        .execute(CliCommandInput {
            connection_id: "cluster".into(),
            session_id: session.session_id.clone(),
            command: format!("GET {cli_key}"),
        })
        .await
        .unwrap();
    assert_eq!(get.result.unwrap().value, serde_json::json!("cli-value"));
    cli.close(session).await.unwrap();
    for key in keys.values() {
        service
            .execute_command("cluster", &format!("DEL {key}"))
            .await
            .unwrap();
    }
    service.close_connection("cluster").await.unwrap();
}
