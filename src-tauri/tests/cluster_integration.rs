use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
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

        // 不同 slot 的长度、过期时间和编码用于检查并发 Pipeline 回填时的键与响应对应关系。
        let metadata_started = Instant::now();
        let mut expected_metadata = HashMap::new();
        for (index, key) in keys.iter().enumerate() {
            let value = "x".repeat((index + 1) * 16);
            let ttl_ms = if index % 2 == 0 {
                -1
            } else {
                600_000 + index as i64 * 60_000
            };
            let expiration = if ttl_ms == -1 {
                String::new()
            } else {
                format!(" PX {ttl_ms}")
            };
            service
                .execute_command("cluster", &format!("SET {key} {value}{expiration}"))
                .await
                .unwrap();
            let encoding = service
                .execute_command("cluster", &format!("OBJECT ENCODING {key}"))
                .await
                .unwrap()
                .value
                .as_str()
                .unwrap()
                .to_owned();
            expected_metadata.insert(key.clone(), (value.len() as u64, ttl_ms, encoding));
        }

        let metadata_page = service
            .scan_keys(ScanKeysInput {
                connection_id: "cluster".into(),
                cursor: ScanCursor::Standalone(0),
                pattern: format!("{prefix}:*"),
                count: 1_000,
                key_type: None,
            })
            .await
            .unwrap();
        assert!(metadata_page.node_failures.is_empty());
        assert_eq!(metadata_page.keys.len(), keys.len());
        assert_eq!(
            metadata_page
                .keys
                .iter()
                .map(|key| &key.key)
                .collect::<HashSet<_>>(),
            keys.iter().collect::<HashSet<_>>()
        );
        assert!(
            metadata_page
                .keys
                .iter()
                .map(|key| redis_slot(&key.key))
                .collect::<HashSet<_>>()
                .len()
                > 1,
            "同一 SCAN 页必须包含多个 slot 才能验证集群 Pipeline 分组"
        );
        let elapsed_ms = metadata_started.elapsed().as_millis() as i64 + 2_000;
        for summary in &metadata_page.keys {
            let (size, ttl_ms, encoding) = &expected_metadata[&summary.key];
            assert_eq!(summary.key_type, "string", "{}", summary.key);
            assert_eq!(summary.size, Some(*size), "{}", summary.key);
            if *ttl_ms == -1 {
                assert_eq!(summary.ttl_ms, -1, "{}", summary.key);
            } else {
                assert!(
                    (*ttl_ms - elapsed_ms..=*ttl_ms).contains(&summary.ttl_ms),
                    "{} 的 TTL 响应错位：{}，预期接近 {}",
                    summary.key,
                    summary.ttl_ms,
                    ttl_ms
                );
            }
            assert_eq!(summary.encoding.as_ref(), Some(encoding), "{}", summary.key);
            assert!(summary.memory_bytes.is_some_and(|bytes| bytes > 0));
            assert!(summary.idle_seconds.is_some());
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

        let all_keys = service
            .scan_all_keys(redix_lib::domain::ScanAllKeysInput {
                connection_id: "cluster".into(),
                pattern: format!("{prefix}:*"),
                count: 1,
                key_type: None,
            })
            .await
            .unwrap();
        assert_eq!(all_keys.len(), keys.len());
        assert_eq!(
            all_keys
                .into_iter()
                .map(|key| key.key)
                .collect::<HashSet<_>>(),
            keys.iter().cloned().collect()
        );

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
async fn cluster_scan_pipeline_handles_asking_during_slot_migration() {
    let seeds = std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS")
        .expect("cluster launcher must set REDIX_TEST_REDIS_CLUSTER_URLS");
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();
    let topology = service.get_cluster_topology("cluster").await.unwrap();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!("redix:asking:{{migration-{suffix}}}");
    let keys = [format!("{prefix}:short"), format!("{prefix}:long")];
    let values = ["short".to_owned(), "long".repeat(20)];
    let slot = redis_slot(&keys[0]);
    let source = topology
        .nodes
        .iter()
        .find(|node| {
            node.role == ClusterNodeRole::Primary
                && node
                    .slots
                    .iter()
                    .any(|range| range.start <= slot && slot <= range.end)
        })
        .unwrap();
    let target = topology
        .nodes
        .iter()
        .find(|node| node.role == ClusterNodeRole::Primary && node.id != source.id)
        .unwrap();
    let mut source_connection = redis::Client::open(format!(
        "redis://{}:{}/",
        source.endpoint.host, source.endpoint.port
    ))
    .unwrap()
    .get_multiplexed_async_connection()
    .await
    .unwrap();
    let mut target_connection = redis::Client::open(format!(
        "redis://{}:{}/",
        target.endpoint.host, target.endpoint.port
    ))
    .unwrap()
    .get_multiplexed_async_connection()
    .await
    .unwrap();

    // 所有迁槽后的检查都在清理之后断言，失败时也必须恢复共享测试集群。
    let started = Instant::now();
    let flow = async {
        for (key, value) in keys.iter().zip(&values) {
            redis::cmd("SET")
                .arg(key)
                .arg(value)
                .arg("PX")
                .arg(600_000)
                .query_async::<String>(&mut source_connection)
                .await?;
        }
        redis::cmd("CLUSTER")
            .arg("SETSLOT")
            .arg(slot)
            .arg("IMPORTING")
            .arg(&source.id)
            .query_async::<String>(&mut target_connection)
            .await?;
        redis::cmd("CLUSTER")
            .arg("SETSLOT")
            .arg(slot)
            .arg("MIGRATING")
            .arg(&target.id)
            .query_async::<String>(&mut source_connection)
            .await?;
        for key in &keys {
            redis::cmd("MIGRATE")
                .arg(&target.endpoint.host)
                .arg(target.endpoint.port)
                .arg(key)
                .arg(0)
                .arg(5_000)
                .query_async::<String>(&mut source_connection)
                .await?;
        }
        // Slot 所有者仍为 source，键已在 target；每次元数据读取都必须处理 ASK。
        let page = service
            .scan_keys(ScanKeysInput {
                connection_id: "cluster".into(),
                cursor: ScanCursor::Standalone(0),
                pattern: format!("{prefix}:*"),
                count: 1_000,
                key_type: None,
            })
            .await;
        let all_keys = service
            .scan_all_keys(redix_lib::domain::ScanAllKeysInput {
                connection_id: "cluster".into(),
                pattern: format!("{prefix}:*"),
                count: 1_000,
                key_type: None,
            })
            .await;
        Ok::<_, redis::RedisError>((page, all_keys))
    }
    .await;

    let mut cleanup_errors = Vec::new();
    for key in &keys {
        // IMPORTING 节点需逐条 ASKING 后删除；不能对清理命令使用普通 Pipeline。
        let _ = redis::cmd("ASKING")
            .query_async::<String>(&mut target_connection)
            .await;
        if let Err(error) = redis::cmd("DEL")
            .arg(key)
            .query_async::<u64>(&mut target_connection)
            .await
        {
            cleanup_errors.push(error);
        }
    }
    for connection in [&mut source_connection, &mut target_connection] {
        if let Err(error) = redis::cmd("CLUSTER")
            .arg("SETSLOT")
            .arg(slot)
            .arg("STABLE")
            .query_async::<String>(connection)
            .await
        {
            cleanup_errors.push(error);
        }
    }
    for key in &keys {
        if let Err(error) = redis::cmd("DEL")
            .arg(key)
            .query_async::<u64>(&mut source_connection)
            .await
        {
            cleanup_errors.push(error);
        }
    }
    service.close_connection("cluster").await.unwrap();
    assert!(
        cleanup_errors.is_empty(),
        "迁槽清理失败：{cleanup_errors:?}"
    );

    let (page, all_keys) = flow.expect("迁槽测试准备失败");
    let page = page.expect("迁槽中的单页 SCAN 应逐命令处理 ASK 后读取元数据");
    assert!(page.node_failures.is_empty());
    let all_keys = all_keys.expect("迁槽中的全量 SCAN 应逐命令处理 ASK 后读取元数据");
    let elapsed_ms = started.elapsed().as_millis() as i64 + 2_000;
    for summaries in [&page.keys, &all_keys] {
        assert_eq!(summaries.len(), keys.len());
        for (key, value) in keys.iter().zip(&values) {
            let summary = summaries
                .iter()
                .find(|summary| &summary.key == key)
                .unwrap();
            assert_eq!(summary.key_type, "string");
            assert_eq!(summary.size, Some(value.len() as u64));
            assert!((600_000 - elapsed_ms..=600_000).contains(&summary.ttl_ms));
            assert!(summary.memory_bytes.is_some_and(|bytes| bytes > 0));
            assert!(summary.encoding.is_some());
        }
    }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "由 scripts/test-local-cluster.py 设置 REDIX_TEST_REDIS_CLUSTER_URLS"]
async fn cluster_bulk_delete_uses_direct_primary_connections_for_all_slots() {
    use redix_lib::redis::bulk_tasks::{BulkTaskManager, BulkTaskStatus, StartBulkDeleteInput};
    let seeds =
        std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS").expect("isolated cluster launcher required");
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let keys: Vec<String> = (0..60).map(|i| format!("redix:bulk:{nonce}:{i}")).collect();
    let mut connection = service.routed_connection("cluster").await.unwrap();
    for key in &keys {
        redis::cmd("SET")
            .arg(key)
            .arg("v")
            .query_async::<()>(&mut connection)
            .await
            .unwrap();
    }
    let manager = BulkTaskManager::default();
    let task = service
        .start_bulk_delete(
            &manager,
            StartBulkDeleteInput {
                connection_id: "cluster".into(),
                keys: keys.clone(),
            },
        )
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let latest = manager
                .list()
                .unwrap()
                .into_iter()
                .find(|item| item.id == task.id)
                .unwrap();
            if latest.status != BulkTaskStatus::Running {
                break latest;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result.status, BulkTaskStatus::Completed);
    assert_eq!(result.deleted, 60);
    for key in keys {
        assert_eq!(
            redis::cmd("EXISTS")
                .arg(key)
                .query_async::<u64>(&mut connection)
                .await
                .unwrap(),
            0
        );
    }
    service.close_connection("cluster").await.unwrap();
}
