//! This ignored test starts and verifies its own disposable Redis; no user URL is accepted.
use redix_lib::{
    domain::{collection::*, ConnectionProfile, RedisValue, RenameKeyInput},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};
use std::{
    collections::HashSet,
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

struct Profiles(ConnectionProfile);
impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(vec![self.0.clone()])
    }
    fn save(&self, _: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}
struct Secrets;
impl SecretStore for Secrets {
    fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(None)
    }
    fn write(&self, _: &str, _: &ConnectionSecrets) -> Result<(), AppError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}
struct Server {
    child: Child,
    _directory: tempfile::TempDir,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn isolated_server() -> (Server, RedisService, redis::aio::MultiplexedConnection) {
    isolated_server_with_disabled_commands(&[]).await
}

async fn isolated_server_with_disabled_commands(
    disabled_commands: &[&str],
) -> (Server, RedisService, redis::aio::MultiplexedConnection) {
    let directory = tempfile::tempdir().unwrap();
    let reservation = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let mut command = Command::new("redis-server");
    command
        .args([
            "--bind",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--save",
            "",
            "--appendonly",
            "no",
            "--enable-debug-command",
            "yes",
            "--dir",
        ])
        .arg(directory.path());
    for name in disabled_commands {
        command.args(["--rename-command", name, ""]);
    }
    let child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("需要本机 redis-server；测试仅启动隔离临时实例");
    let mut server = Server {
        child,
        _directory: directory,
    };
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let connection = loop {
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "临时Redis意外退出"
        );
        assert!(Instant::now() < deadline, "等待临时Redis超时");
        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
            let info: String = redis::cmd("INFO")
                .arg("server")
                .query_async(&mut connection)
                .await
                .unwrap();
            assert!(
                info.lines()
                    .any(|line| line == format!("process_id:{}", server.child.id())),
                "端口所有者不匹配；拒绝写入"
            );
            break connection;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let profile = ConnectionProfile {
        id: "isolated".into(),
        name: "临时集合测试".into(),
        host: "127.0.0.1".into(),
        port,
        database: 0,
        username: None,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ssh: None,
        sentinel: None,
        cluster: None,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    };
    let service = RedisService::new(Arc::new(Profiles(profile)), Arc::new(Secrets));
    service.open_connection("isolated").await.unwrap();
    (server, service, connection)
}

fn request(key: &str, kind: CollectionKind) -> CollectionPageInput {
    CollectionPageInput {
        connection_id: "isolated".into(),
        key: key.into(),
        kind,
        cursor: "0".into(),
        count: 100,
        pattern: "*".into(),
        order: CollectionOrder::Scan,
    }
}
async fn mutate(service: &RedisService, key: &str, mutation: CollectionMutation) {
    let context = format!("{key}: {mutation:?}");
    service
        .mutate_collection(CollectionMutationInput {
            connection_id: "isolated".into(),
            key: key.into(),
            mutation,
        })
        .await
        .unwrap_or_else(|error| panic!("{context}: {error:?}"));
}

fn mutation_input(key: &str, operation: serde_json::Value) -> CollectionMutationInput {
    serde_json::from_value(serde_json::json!({
        "connection_id": "isolated", "key": key, "mutation": operation
    }))
    .unwrap()
}

async fn field_ttl(raw: &mut redis::aio::MultiplexedConnection, field: &str) -> i64 {
    let values: Vec<i64> = redis::cmd("HPTTL")
        .arg("ttl-hash")
        .arg("FIELDS")
        .arg(1)
        .arg(field)
        .query_async(raw)
        .await
        .unwrap();
    assert_eq!(values.len(), 1);
    values[0]
}

async fn field_deadline(raw: &mut redis::aio::MultiplexedConnection, field: &str) -> i64 {
    let values: Vec<i64> = redis::cmd("HPEXPIRETIME")
        .arg("ttl-hash")
        .arg("FIELDS")
        .arg(1)
        .arg(field)
        .query_async(raw)
        .await
        .unwrap();
    assert_eq!(values.len(), 1);
    values[0]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，覆盖Hash字段TTL"]
async fn hash_field_expiry_can_be_read_updated_and_persisted_without_changing_key_expiry() {
    let (_server, service, mut raw) = isolated_server().await;
    // Force lazy expiry, so background cleanup cannot hide accidental resurrection.
    redis::cmd("DEBUG")
        .arg("SET-ACTIVE-EXPIRE")
        .arg(0)
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    redis::pipe()
        .cmd("HSET")
        .arg("ttl-hash")
        .arg("editable")
        .arg("before")
        .arg("other")
        .arg("untouched")
        .arg("expiring")
        .arg("temporary")
        .ignore()
        .cmd("PEXPIRE")
        .arg("ttl-hash")
        .arg(60000)
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    let key_deadline: i64 = redis::cmd("PEXPIRETIME")
        .arg("ttl-hash")
        .query_async(&mut raw)
        .await
        .unwrap();
    let initial_page = serde_json::to_value(
        service
            .get_collection_page(request("ttl-hash", CollectionKind::Hash))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(initial_page["hash_field_ttl_supported"], true);
    assert!(initial_page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["ttl_ms"] == -1));
    redis::cmd("HPEXPIRE")
        .arg("ttl-hash")
        .arg(20000)
        .arg("FIELDS")
        .arg(1)
        .arg("other")
        .query_async::<Vec<i64>>(&mut raw)
        .await
        .unwrap();
    let other_deadline = field_deadline(&mut raw, "other").await;

    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_expire", "field": "editable", "ttl_ms": "30000"
            }),
        ))
        .await
        .unwrap();
    assert!((1..=30000).contains(&field_ttl(&mut raw, "editable").await));
    let original_deadline = field_deadline(&mut raw, "editable").await;
    let page = serde_json::to_value(
        service
            .get_collection_page(request("ttl-hash", CollectionKind::Hash))
            .await
            .unwrap(),
    )
    .unwrap();
    let editable = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "editable")
        .unwrap();
    assert!((1..=30000).contains(&editable["ttl_ms"].as_i64().unwrap()));
    mutate(
        &service,
        "ttl-hash",
        CollectionMutation::HashSet {
            field: "editable".into(),
            value: "after".into(),
        },
    )
    .await;
    assert_eq!(
        field_deadline(&mut raw, "editable").await,
        original_deadline,
        "编辑值必须保留字段到期时间"
    );
    assert_eq!(
        redis::cmd("HGET")
            .arg("ttl-hash")
            .arg("editable")
            .query_async::<String>(&mut raw)
            .await
            .unwrap(),
        "after"
    );

    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_expire", "field": "editable", "ttl_ms": "45000"
            }),
        ))
        .await
        .unwrap();
    assert!(field_deadline(&mut raw, "editable").await > original_deadline + 14000);
    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_persist", "field": "editable"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(field_ttl(&mut raw, "editable").await, -1);
    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_persist", "field": "editable"
            }),
        ))
        .await
        .unwrap();

    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_expire", "field": "expiring", "ttl_ms": "1"
            }),
        ))
        .await
        .unwrap();
    std::thread::sleep(Duration::from_millis(15));
    for operation in [
        serde_json::json!({"operation": "hash_expire", "field": "expiring", "ttl_ms": "30000"}),
        serde_json::json!({"operation": "hash_persist", "field": "expiring"}),
        serde_json::json!({"operation": "hash_expire", "field": "absent", "ttl_ms": "30000"}),
        serde_json::json!({"operation": "hash_persist", "field": "absent"}),
    ] {
        let context = operation.to_string();
        assert_eq!(
            service
                .mutate_collection(mutation_input("ttl-hash", operation))
                .await,
            Err(AppError::KeyNotFound),
            "过期或缺失字段操作：{context}"
        );
    }
    assert_eq!(field_ttl(&mut raw, "expiring").await, -2);
    assert_eq!(field_ttl(&mut raw, "absent").await, -2);
    assert_eq!(field_deadline(&mut raw, "other").await, other_deadline);
    assert_eq!(
        redis::cmd("HGET")
            .arg("ttl-hash")
            .arg("other")
            .query_async::<String>(&mut raw)
            .await
            .unwrap(),
        "untouched"
    );
    assert_eq!(
        redis::cmd("HLEN")
            .arg("ttl-hash")
            .query_async::<u64>(&mut raw)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        redis::cmd("PEXPIRETIME")
            .arg("ttl-hash")
            .query_async::<i64>(&mut raw)
            .await
            .unwrap(),
        key_deadline
    );
    redis::pipe()
        .cmd("HSET")
        .arg("last-field")
        .arg("field")
        .arg("before")
        .ignore()
        .cmd("HPEXPIRE")
        .arg("last-field")
        .arg(1)
        .arg("FIELDS")
        .arg(1)
        .arg("field")
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    std::thread::sleep(Duration::from_millis(15));
    assert_eq!(
        service
            .mutate_collection(mutation_input(
                "last-field",
                serde_json::json!({
                    "operation": "hash_set", "field": "field", "value": "must not resurrect the key"
                })
            ))
            .await,
        Err(AppError::KeyNotFound),
        "最后一个字段过期后编辑不能重建key"
    );
    assert!(!redis::cmd("EXISTS")
        .arg("last-field")
        .query_async::<bool>(&mut raw)
        .await
        .unwrap());
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，验证字段TTL参数边界"]
async fn hash_field_expiry_rejects_invalid_milliseconds_without_writes() {
    let (_server, service, mut raw) = isolated_server().await;
    redis::cmd("HSET")
        .arg("ttl-hash")
        .arg("field")
        .arg("value")
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    for ttl in [
        "0",
        "-1",
        "1.5",
        "",
        " 1",
        "+1",
        "3153600000001",
        "9007199254740991",
        "9007199254740992",
        "9223372036854775808",
    ] {
        assert_eq!(
            service
                .mutate_collection(mutation_input(
                    "ttl-hash",
                    serde_json::json!({
                        "operation": "hash_expire", "field": "field", "ttl_ms": ttl
                    })
                ))
                .await,
            Err(AppError::InvalidInput),
            "不合法TTL：{ttl:?}"
        );
        assert_eq!(field_ttl(&mut raw, "field").await, -1);
    }
    service
        .mutate_collection(mutation_input(
            "ttl-hash",
            serde_json::json!({
                "operation": "hash_expire", "field": "field", "ttl_ms": "3153600000000"
            }),
        ))
        .await
        .unwrap();
    assert!(field_ttl(&mut raw, "field").await > 3153599999000);
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，禁用字段TTL命令模拟旧版本"]
async fn hash_without_field_expiry_commands_keeps_normal_pages_and_edits_available() {
    let (_server, service, mut raw) = isolated_server_with_disabled_commands(&[
        "HPTTL",
        "HPEXPIRE",
        "HPEXPIREAT",
        "HPEXPIRETIME",
        "HPERSIST",
    ])
    .await;
    redis::pipe()
        .cmd("HSET")
        .arg("legacy")
        .arg("field")
        .arg("before")
        .arg("other")
        .arg("untouched")
        .ignore()
        .cmd("PEXPIRE")
        .arg("legacy")
        .arg(60000)
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    let key_deadline = redis::cmd("PEXPIRETIME")
        .arg("legacy")
        .query_async::<i64>(&mut raw)
        .await
        .unwrap();
    let page = serde_json::to_value(
        service
            .get_collection_page(request("legacy", CollectionKind::Hash))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page["hash_field_ttl_supported"], false);
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["ttl_ms"].is_null()));
    mutate(
        &service,
        "legacy",
        CollectionMutation::HashSet {
            field: "field".into(),
            value: "after".into(),
        },
    )
    .await;
    mutate(
        &service,
        "legacy",
        CollectionMutation::HashSet {
            field: "added".into(),
            value: "new".into(),
        },
    )
    .await;
    for operation in [
        serde_json::json!({"operation": "hash_expire", "field": "field", "ttl_ms": "30000"}),
        serde_json::json!({"operation": "hash_persist", "field": "field"}),
    ] {
        assert_eq!(
            service
                .mutate_collection(mutation_input("legacy", operation))
                .await,
            Err(AppError::UnsupportedFeature)
        );
    }
    let values: Vec<String> = redis::cmd("HMGET")
        .arg("legacy")
        .arg(&["field", "other", "added"])
        .query_async(&mut raw)
        .await
        .unwrap();
    assert_eq!(values, ["after", "untouched", "new"]);
    mutate(
        &service,
        "legacy",
        CollectionMutation::HashDelete {
            field: "added".into(),
        },
    )
    .await;
    assert_eq!(
        redis::cmd("HLEN")
            .arg("legacy")
            .query_async::<u64>(&mut raw)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        redis::cmd("PEXPIRETIME")
            .arg("legacy")
            .query_async::<i64>(&mut raw)
            .await
            .unwrap(),
        key_deadline
    );
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，覆盖字段TTL的ACL限制"]
async fn hash_field_expiry_acl_failures_never_partially_modify_values_or_deadlines() {
    let (_server, service, mut raw) = isolated_server().await;
    redis::pipe()
        .cmd("HSET")
        .arg("ttl-hash")
        .arg("field")
        .arg("before")
        .ignore()
        .cmd("HPEXPIRE")
        .arg("ttl-hash")
        .arg(30000)
        .arg("FIELDS")
        .arg(1)
        .arg("field")
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    let deadline = field_deadline(&mut raw, "field").await;
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("default")
        .arg("-hpttl")
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    let page = service
        .get_collection_page(request("ttl-hash", CollectionKind::Hash))
        .await
        .unwrap();
    assert_eq!(page.hash_field_ttl_supported, Some(false));
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].value, "before");
    assert_eq!(page.entries[0].ttl_ms, None);
    assert_eq!(
        service
            .mutate_collection(mutation_input(
                "ttl-hash",
                serde_json::json!({
                    "operation": "hash_set", "field": "field", "value": "must not be written"
                })
            ))
            .await,
        Err(AppError::UnsupportedFeature)
    );
    assert_eq!(field_deadline(&mut raw, "field").await, deadline);
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("default")
        .arg("+hpttl")
        .arg("-hpexpire")
        .arg("-hpersist")
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    for operation in [
        serde_json::json!({"operation": "hash_set", "field": "field", "value": "must not be written"}),
        serde_json::json!({"operation": "hash_expire", "field": "field", "ttl_ms": "45000"}),
        serde_json::json!({"operation": "hash_persist", "field": "field"}),
    ] {
        let context = operation.to_string();
        assert_eq!(
            service
                .mutate_collection(mutation_input("ttl-hash", operation))
                .await,
            Err(AppError::UnsupportedFeature),
            "ACL禁止的操作：{context}"
        );
        assert_eq!(field_deadline(&mut raw, "field").await, deadline);
        assert_eq!(
            redis::cmd("HGET")
                .arg("ttl-hash")
                .arg("field")
                .query_async::<String>(&mut raw)
                .await
                .unwrap(),
            "before"
        );
    }
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，覆盖服务端分数排序"]
async fn zset_score_order_is_global_across_pages_with_negative_and_equal_scores() {
    let (_server, service, mut raw) = isolated_server().await;
    redis::cmd("ZADD")
        .arg("ranked")
        .arg(10)
        .arg("z")
        .arg(-5)
        .arg("b")
        .arg(-5)
        .arg("a")
        .arg(0)
        .arg("middle")
        .arg(10)
        .arg("y")
        .arg(-0.5)
        .arg("negative")
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    for (order, expected) in [
        (
            "score_asc",
            vec![
                ("a", -5.0),
                ("b", -5.0),
                ("negative", -0.5),
                ("middle", 0.0),
                ("y", 10.0),
                ("z", 10.0),
            ],
        ),
        (
            "score_desc",
            vec![
                ("z", 10.0),
                ("y", 10.0),
                ("middle", 0.0),
                ("negative", -0.5),
                ("b", -5.0),
                ("a", -5.0),
            ],
        ),
    ] {
        let mut input: CollectionPageInput = serde_json::from_value(serde_json::json!({
            "connection_id": "isolated", "key": "ranked", "kind": "zset",
            "cursor": "0", "count": 2, "pattern": "*", "order": order
        }))
        .unwrap();
        let mut actual = Vec::new();
        let mut pages = 0;
        loop {
            let page = service.get_collection_page(input.clone()).await.unwrap();
            assert_eq!(page.total, "6");
            assert_eq!(page.entries.len(), 2, "{order}必须在服务端按排名分页");
            actual.extend(
                page.entries
                    .into_iter()
                    .map(|entry| (entry.id, entry.score.unwrap())),
            );
            pages += 1;
            assert!(pages <= 3, "排序游标必须结束");
            if !page.has_more {
                break;
            }
            input.cursor = page.next_cursor;
        }
        assert_eq!(pages, 3);
        assert_eq!(
            actual,
            expected
                .into_iter()
                .map(|(member, score)| (member.to_string(), score))
                .collect::<Vec<_>>()
        );
        let partial = service
            .get_collection_page(CollectionPageInput {
                cursor: "5".into(),
                ..input.clone()
            })
            .await
            .unwrap();
        assert_eq!(partial.entries.len(), 1);
        assert_eq!(
            partial.entries[0].id,
            if order == "score_asc" { "z" } else { "a" }
        );
        assert!(!partial.has_more);
        let beyond = service
            .get_collection_page(CollectionPageInput {
                cursor: "6".into(),
                ..input.clone()
            })
            .await
            .unwrap();
        assert!(beyond.entries.is_empty());
        assert!(!beyond.has_more);
        assert_eq!(beyond.total, "6");
        assert_eq!(
            service
                .get_collection_page(CollectionPageInput {
                    cursor: "9223372036854775806".into(),
                    ..input
                })
                .await,
            Err(AppError::InvalidInput)
        );
    }
    // Older callers omit order; their ZSCAN MATCH filtering must still work.
    let mut scan: CollectionPageInput = serde_json::from_value(serde_json::json!({
        "connection_id": "isolated", "key": "ranked", "kind": "zset",
        "cursor": "0", "count": 2, "pattern": "*i*"
    }))
    .unwrap();
    let mut matches = HashSet::new();
    let mut pages = 0;
    loop {
        let page = service.get_collection_page(scan.clone()).await.unwrap();
        matches.extend(page.entries.into_iter().map(|entry| entry.id));
        pages += 1;
        assert!(pages <= 20, "扫描游标必须结束");
        if !page.has_more {
            break;
        }
        scan.cursor = page.next_cursor;
    }
    assert_eq!(
        matches,
        HashSet::from(["negative".to_string(), "middle".to_string()])
    );
    for order in [CollectionOrder::ScoreAsc, CollectionOrder::ScoreDesc] {
        assert_eq!(
            service
                .get_collection_page(CollectionPageInput {
                    order,
                    pattern: "a*".into(),
                    ..request("ranked", CollectionKind::Zset)
                })
                .await,
            Err(AppError::InvalidInput)
        );
    }
    service.close_connection("isolated").await.unwrap();
}

fn list_index(key: &str, index: &str) -> ListIndexInput {
    ListIndexInput {
        connection_id: "isolated".into(),
        key: key.into(),
        index: index.into(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，覆盖列表定点查询"]
async fn list_index_lookup_handles_signed_boundaries_and_returns_an_editable_absolute_index() {
    let (_server, service, mut raw) = isolated_server().await;
    redis::pipe()
        .cmd("RPUSH")
        .arg("lookup")
        .arg(&["head", "重复", "", "tail"])
        .ignore()
        .cmd("PEXPIRE")
        .arg("lookup")
        .arg(60000)
        .ignore()
        .cmd("SET")
        .arg("not-list")
        .arg("string")
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    let key_deadline = redis::cmd("PEXPIRETIME")
        .arg("lookup")
        .query_async::<i64>(&mut raw)
        .await
        .unwrap();
    for (index, expected_id, expected_value) in [
        ("0", "0", "head"),
        ("1", "1", "重复"),
        ("2", "2", ""),
        ("3", "3", "tail"),
        ("-1", "3", "tail"),
        ("-2", "2", ""),
        ("-4", "0", "head"),
    ] {
        let entry = service
            .get_list_entry(list_index("lookup", index))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (entry.id.as_str(), entry.value.as_str()),
            (expected_id, expected_value),
            "索引：{index}"
        );
        assert_eq!(entry.score, None);
        assert_eq!(entry.ttl_ms, None);
    }
    for index in ["4", "-5", "9223372036854775807", "-9223372036854775808"] {
        assert_eq!(
            service.get_list_entry(list_index("lookup", index)).await,
            Ok(None),
            "越界索引：{index}"
        );
    }
    for index in [
        "9223372036854775808",
        "-9223372036854775809",
        "1.5",
        "",
        "+1",
        " 1",
        "- 1",
    ] {
        assert_eq!(
            service.get_list_entry(list_index("lookup", index)).await,
            Err(AppError::InvalidInput),
            "非法索引：{index:?}"
        );
    }
    assert_eq!(
        service.get_list_entry(list_index("missing", "0")).await,
        Err(AppError::KeyNotFound)
    );
    assert_eq!(
        service.get_list_entry(list_index("not-list", "0")).await,
        Err(AppError::UnsupportedDataType)
    );
    let tail = service
        .get_list_entry(list_index("lookup", "-1"))
        .await
        .unwrap()
        .unwrap();
    mutate(
        &service,
        "lookup",
        CollectionMutation::ListSet {
            index: tail.id,
            value: "edited tail".into(),
        },
    )
    .await;
    let values: Vec<String> = redis::cmd("LRANGE")
        .arg("lookup")
        .arg(0)
        .arg(-1)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert_eq!(values, ["head", "重复", "", "edited tail"]);
    assert_eq!(
        redis::cmd("PEXPIRETIME")
            .arg("lookup")
            .query_async::<i64>(&mut raw)
            .await
            .unwrap(),
        key_deadline
    );
    redis::pipe()
        .cmd("RPUSH")
        .arg("binary-list")
        .arg(&[0xff_u8, 0, 0xfe])
        .ignore()
        .cmd("RPUSH")
        .arg("oversized-list")
        .arg("x".repeat(MAX_COLLECTION_VALUE_BYTES + 1))
        .ignore()
        .cmd("RPUSH")
        .arg("escaped-list")
        .arg("\0".repeat(MAX_COLLECTION_VALUE_BYTES))
        .ignore()
        .query_async::<()>(&mut raw)
        .await
        .unwrap();
    for key in ["binary-list", "oversized-list", "escaped-list"] {
        assert_eq!(
            service.get_list_entry(list_index(key, "0")).await,
            Err(AppError::CommandFailed),
            "{key}不能以损坏或截断的数据返回"
        );
    }
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis，覆盖列表两端批量删除"]
async fn list_trim_removes_the_requested_end_and_preserves_expiry_until_empty() {
    let (_server, service, mut raw) = isolated_server().await;
    let mut seed = redis::pipe();
    for key in [
        "head",
        "tail",
        "all-head",
        "all-tail",
        "huge-head",
        "huge-tail",
    ] {
        seed.cmd("RPUSH")
            .arg(key)
            .arg(&["a", "b", "c", "d", "e"])
            .ignore()
            .cmd("PEXPIRE")
            .arg(key)
            .arg(60000)
            .ignore();
    }
    seed.query_async::<()>(&mut raw).await.unwrap();
    for (key, from_head, expected) in [
        ("head", true, vec!["c", "d", "e"]),
        ("tail", false, vec!["a", "b", "c"]),
    ] {
        let deadline = redis::cmd("PEXPIRETIME")
            .arg(key)
            .query_async::<i64>(&mut raw)
            .await
            .unwrap();
        service
            .mutate_collection(mutation_input(
                key,
                serde_json::json!({
                    "operation": "list_trim", "count": "2", "from_head": from_head
                }),
            ))
            .await
            .unwrap();
        let values: Vec<String> = redis::cmd("LRANGE")
            .arg(key)
            .arg(0)
            .arg(-1)
            .query_async(&mut raw)
            .await
            .unwrap();
        assert_eq!(values, expected);
        assert_eq!(
            redis::cmd("PEXPIRETIME")
                .arg(key)
                .query_async::<i64>(&mut raw)
                .await
                .unwrap(),
            deadline
        );
        for count in [
            "0",
            "-1",
            "1.5",
            "",
            "+1",
            " 1",
            "9223372036854775807",
            "9223372036854775808",
        ] {
            assert_eq!(
                service
                    .mutate_collection(mutation_input(
                        key,
                        serde_json::json!({
                            "operation": "list_trim", "count": count, "from_head": from_head
                        })
                    ))
                    .await,
                Err(AppError::InvalidInput)
            );
        }
        assert_eq!(
            redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .query_async::<Vec<String>>(&mut raw)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(
            redis::cmd("PEXPIRETIME")
                .arg(key)
                .query_async::<i64>(&mut raw)
                .await
                .unwrap(),
            deadline
        );
    }
    for (key, from_head, count) in [
        ("all-head", true, "5"),
        ("all-tail", false, "6"),
        ("huge-head", true, "9223372036854775806"),
        ("huge-tail", false, "9223372036854775806"),
    ] {
        service
            .mutate_collection(mutation_input(
                key,
                serde_json::json!({
                    "operation": "list_trim", "count": count, "from_head": from_head
                }),
            ))
            .await
            .unwrap();
        assert!(
            !redis::cmd("EXISTS")
                .arg(key)
                .query_async::<bool>(&mut raw)
                .await
                .unwrap(),
            "{key}应删除为空"
        );
        // An empty Redis list is absent; retrying must not recreate it.
        assert_eq!(
            service
                .mutate_collection(mutation_input(
                    key,
                    serde_json::json!({
                        "operation": "list_trim", "count": "1", "from_head": from_head
                    })
                ))
                .await,
            Err(AppError::KeyNotFound)
        );
        assert!(!redis::cmd("EXISTS")
            .arg(key)
            .query_async::<bool>(&mut raw)
            .await
            .unwrap());
    }
    service.close_connection("isolated").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis：cargo test --test collection_integration -- --ignored"]
async fn collection_pages_and_incremental_writes_preserve_unloaded_data_and_ttl() {
    let (_server, service, mut raw) = isolated_server().await;
    let missing = CollectionMutationInput {
        connection_id: "isolated".into(),
        key: "missing".into(),
        mutation: CollectionMutation::HashSet {
            field: "field".into(),
            value: "value".into(),
        },
    };
    assert_eq!(
        service.mutate_collection(missing).await,
        Err(AppError::KeyNotFound)
    );
    assert!(!redis::cmd("EXISTS")
        .arg("missing")
        .query_async::<bool>(&mut raw)
        .await
        .unwrap());
    let mut seed = redis::pipe();
    for index in 0..1205 {
        seed.cmd("HSET")
            .arg("hash")
            .arg(format!("f{index}"))
            .arg(format!("v{index}"))
            .ignore();
        seed.cmd("RPUSH")
            .arg("list")
            .arg(format!("v{index}"))
            .ignore();
        seed.cmd("SADD")
            .arg("set")
            .arg(format!("m{index}"))
            .ignore();
        seed.cmd("ZADD")
            .arg("zset")
            .arg(index)
            .arg(format!("m{index}"))
            .ignore();
    }
    for key in ["hash", "list", "set", "zset"] {
        seed.cmd("PEXPIRE").arg(key).arg(60000).ignore();
    }
    seed.query_async::<()>(&mut raw).await.unwrap();
    let wrong_type = CollectionMutationInput {
        connection_id: "isolated".into(),
        key: "hash".into(),
        mutation: CollectionMutation::ListAppend {
            value: "value".into(),
            prepend: false,
        },
    };
    assert_eq!(
        service.mutate_collection(wrong_type).await,
        Err(AppError::UnsupportedDataType)
    );

    // The browser must open a large collection without calling HGETALL/LRANGE 0 -1.
    let browser = service.get_browser_key("isolated", "hash").await.unwrap();
    assert!(matches!(browser.value, RedisValue::Hash { fields } if fields.is_empty()));
    for (key, kind) in [
        ("hash", CollectionKind::Hash),
        ("list", CollectionKind::List),
        ("set", CollectionKind::Set),
        ("zset", CollectionKind::Zset),
    ] {
        let mut request = request(key, kind);
        let mut seen = HashSet::new();
        let mut pages = 0;
        loop {
            let page = service.get_collection_page(request.clone()).await.unwrap();
            assert_eq!(page.total, "1205");
            assert!((1..=60000).contains(&page.ttl_ms));
            assert!(page.entries.len() <= 2000);
            seen.extend(page.entries.into_iter().map(|entry| entry.id));
            pages += 1;
            assert!(pages <= 100, "游标未结束");
            if !page.has_more {
                break;
            }
            request.cursor = page.next_cursor;
        }
        assert_eq!(seen.len(), 1205);
        assert!(pages > 1);
    }

    mutate(
        &service,
        "hash",
        CollectionMutation::HashSet {
            field: "f0".into(),
            value: "changed".into(),
        },
    )
    .await;
    mutate(
        &service,
        "hash",
        CollectionMutation::HashDelete { field: "f1".into() },
    )
    .await;
    mutate(
        &service,
        "set",
        CollectionMutation::SetAdd {
            member: "added".into(),
        },
    )
    .await;
    mutate(
        &service,
        "set",
        CollectionMutation::SetRemove {
            member: "m1".into(),
        },
    )
    .await;
    mutate(
        &service,
        "zset",
        CollectionMutation::ZsetAdd {
            member: "m0".into(),
            score: 9000.25,
        },
    )
    .await;
    mutate(
        &service,
        "zset",
        CollectionMutation::ZsetRemove {
            member: "m1".into(),
        },
    )
    .await;
    mutate(
        &service,
        "list",
        CollectionMutation::ListSet {
            index: "0".into(),
            value: "changed".into(),
        },
    )
    .await;
    mutate(
        &service,
        "list",
        CollectionMutation::ListAppend {
            value: "tail".into(),
            prepend: false,
        },
    )
    .await;
    mutate(
        &service,
        "list",
        CollectionMutation::ListAppend {
            value: "head".into(),
            prepend: true,
        },
    )
    .await;
    let replies: Vec<redis::Value> = redis::pipe()
        .cmd("HGET").arg("hash").arg("f0").cmd("HGET").arg("hash").arg("f1204")
        .cmd("HEXISTS").arg("hash").arg("f1").cmd("HLEN").arg("hash")
        .cmd("SISMEMBER").arg("set").arg("m1204").cmd("SISMEMBER").arg("set").arg("m1")
        .cmd("ZSCORE").arg("zset").arg("m0").cmd("ZSCORE").arg("zset").arg("m1")
        .cmd("LINDEX").arg("list").arg(0).cmd("LINDEX").arg("list").arg(1)
        .cmd("LINDEX").arg("list").arg(1205).cmd("LINDEX").arg("list").arg(1206)
        .cmd("EVAL").arg("return {redis.call('PTTL','hash'),redis.call('PTTL','list'),redis.call('PTTL','set'),redis.call('PTTL','zset')}").arg(0)
        .query_async(&mut raw).await.unwrap();
    let changed = redis::from_redis_value::<String>(replies[0].clone()).unwrap();
    let unseen = redis::from_redis_value::<String>(replies[1].clone()).unwrap();
    let removed = redis::from_redis_value::<bool>(replies[2].clone()).unwrap();
    let hash_len = redis::from_redis_value::<u64>(replies[3].clone()).unwrap();
    let set_member = redis::from_redis_value::<bool>(replies[4].clone()).unwrap();
    let set_removed = redis::from_redis_value::<bool>(replies[5].clone()).unwrap();
    let score = redis::from_redis_value::<f64>(replies[6].clone()).unwrap();
    let zset_removed = redis::from_redis_value::<Option<f64>>(replies[7].clone()).unwrap();
    let list_head = redis::from_redis_value::<String>(replies[8].clone()).unwrap();
    let list_changed = redis::from_redis_value::<String>(replies[9].clone()).unwrap();
    let list_unseen = redis::from_redis_value::<String>(replies[10].clone()).unwrap();
    let list_tail = redis::from_redis_value::<String>(replies[11].clone()).unwrap();
    let ttls = redis::from_redis_value::<Vec<i64>>(replies[12].clone()).unwrap();
    assert_eq!(changed, "changed");
    assert_eq!(unseen, "v1204");
    assert!(!removed);
    assert_eq!(hash_len, 1204);
    assert!(set_member);
    assert!(!set_removed);
    assert_eq!(score, 9000.25);
    assert_eq!(zset_removed, None);
    assert_eq!(
        (
            list_head.as_str(),
            list_changed.as_str(),
            list_unseen.as_str(),
            list_tail.as_str()
        ),
        ("head", "changed", "v1204", "tail")
    );
    assert!(ttls.iter().all(|ttl| (1..=60000).contains(ttl)));
    let renamed = service
        .rename_browser_key(RenameKeyInput {
            connection_id: "isolated".into(),
            key: "hash".into(),
            new_key: "renamed".into(),
        })
        .await
        .unwrap();
    assert!(matches!(renamed.value, RedisValue::Hash { fields } if fields.is_empty()));
    let renamed_page = service
        .get_collection_page(CollectionPageInput {
            pattern: "f1204".into(),
            ..request("renamed", CollectionKind::Hash)
        })
        .await
        .unwrap();
    // MATCH can yield empty intermediate pages, so verify the unpaged field directly.
    assert!(renamed_page.ttl_ms > 0);
    assert_eq!(
        redis::cmd("HGET")
            .arg("renamed")
            .arg("f1204")
            .query_async::<String>(&mut raw)
            .await
            .unwrap(),
        "v1204"
    );
    assert!(service
        .get_collection_page(CollectionPageInput {
            count: 501,
            ..request("renamed", CollectionKind::Hash)
        })
        .await
        .is_err());
    service.close_connection("isolated").await.unwrap();
}
