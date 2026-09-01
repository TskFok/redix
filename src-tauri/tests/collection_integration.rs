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
    let directory = tempfile::tempdir().unwrap();
    let reservation = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let child = Command::new("redis-server")
        .args([
            "--bind",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--save",
            "",
            "--appendonly",
            "no",
            "--dir",
        ])
        .arg(directory.path())
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
