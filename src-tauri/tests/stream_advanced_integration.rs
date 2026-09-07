use redix_lib::{
    domain::{stream_advanced::*, ConnectionProfile, GetStreamConsumerGroupsInput},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

struct IsolatedRedis {
    process: Child,
    _directory: tempfile::TempDir,
}
impl Drop for IsolatedRedis {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
impl IsolatedRedis {
    fn start() -> (Self, u16) {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        let directory = tempfile::tempdir().unwrap();
        drop(reservation);
        let process = Command::new("redis-server")
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
            .expect("此 ignored 测试需要 redis-server");
        let mut server = Self {
            process,
            _directory: directory,
        };
        let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                server.process.try_wait().unwrap().is_none(),
                "临时 Redis 提前退出"
            );
            if let Ok(mut connection) = client.get_connection() {
                if let Ok(info) = redis::cmd("INFO")
                    .arg("server")
                    .query::<String>(&mut connection)
                {
                    // Never write to a pre-existing server if the selected port was raced.
                    if info
                        .lines()
                        .any(|line| line == format!("process_id:{}", server.process.id()))
                    {
                        break;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "临时 Redis 未就绪或端口已被其他进程占用"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        (server, port)
    }
}
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

fn pending_input() -> GetStreamPendingPageInput {
    GetStreamPendingPageInput {
        connection_id: "advanced-test".into(),
        key: "events".into(),
        group: "workers".into(),
        consumer: None,
        start: "-".into(),
        end: "+".into(),
        cursor: None,
        count: 100,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "启动独立临时 Redis：cargo test --test stream_advanced_integration -- --ignored"]
async fn isolated_stream_setid_claim_options_and_pending_pages_preserve_values_and_ttl() {
    let (_server, port) = IsolatedRedis::start();
    let service = RedisService::new(
        Arc::new(Profiles(ConnectionProfile {
            id: "advanced-test".into(),
            name: "Stream advanced test".into(),
            host: "127.0.0.1".into(),
            port,
            username: None,
            database: 0,
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
        })),
        Arc::new(Secrets),
    );
    service.open_connection("advanced-test").await.unwrap();
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let mut setup = redis::pipe();
    for sequence in 1..=4 {
        setup
            .cmd("XADD")
            .arg("events")
            .arg(format!("{sequence}-0"))
            .arg("name")
            .arg(format!("value-{sequence}"))
            .ignore();
    }
    setup
        .cmd("XGROUP")
        .arg("CREATE")
        .arg("events")
        .arg("workers")
        .arg("0")
        .ignore();
    setup
        .cmd("XGROUP")
        .arg("CREATE")
        .arg("events")
        .arg("untouched")
        .arg("0")
        .ignore();
    setup.cmd("PEXPIRE").arg("events").arg(600_000).ignore();
    setup.query_async::<()>(&mut connection).await.unwrap();
    redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("workers")
        .arg("old")
        .arg("COUNT")
        .arg(2)
        .arg("STREAMS")
        .arg("events")
        .arg(">")
        .query_async::<redis::Value>(&mut connection)
        .await
        .unwrap();
    let before = redis::cmd("XRANGE")
        .arg("events")
        .arg("-")
        .arg("+")
        .query_async::<redis::Value>(&mut connection)
        .await
        .unwrap();

    let first = service
        .get_stream_pending_page(GetStreamPendingPageInput {
            count: 1,
            ..pending_input()
        })
        .await
        .unwrap();
    assert_eq!(first.entries[0].id, "1-0");
    assert!(first.has_more);
    assert_eq!(first.next_cursor.as_deref(), Some("1-0"));
    let second = service
        .get_stream_pending_page(GetStreamPendingPageInput {
            count: 1,
            cursor: first.next_cursor,
            ..pending_input()
        })
        .await
        .unwrap();
    assert_eq!(second.entries[0].id, "2-0");
    assert!(!second.has_more);
    let bounded = service
        .get_stream_pending_page(GetStreamPendingPageInput {
            start: "2-0".into(),
            end: "2-0".into(),
            consumer: Some("old".into()),
            ..pending_input()
        })
        .await
        .unwrap();
    assert_eq!(bounded.entries.len(), 1);
    assert_eq!(bounded.entries[0].id, "2-0");

    let mut claim = ClaimStreamPendingAdvancedInput {
        connection_id: "advanced-test".into(),
        key: "events".into(),
        group: "workers".into(),
        consumer: "new".into(),
        min_idle_ms: 0,
        entries: vec!["1-0".into()],
        idle_ms: Some(5000),
        time_ms: None,
        retry_count: Some(7),
        force: false,
    };
    assert_eq!(
        service
            .claim_stream_pending_advanced(claim.clone())
            .await
            .unwrap(),
        vec!["1-0"]
    );
    let transferred = service
        .get_stream_pending_page(GetStreamPendingPageInput {
            consumer: Some("new".into()),
            ..pending_input()
        })
        .await
        .unwrap();
    assert_eq!(transferred.entries.len(), 1);
    assert_eq!(transferred.entries[0].deliveries, 7);
    assert!(transferred.entries[0].idle_ms >= 5000);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    claim.idle_ms = None;
    claim.time_ms = Some(now - 10_000);
    claim.retry_count = None;
    service
        .claim_stream_pending_advanced(claim.clone())
        .await
        .unwrap();
    let timed = service
        .get_stream_pending_page(GetStreamPendingPageInput {
            consumer: Some("new".into()),
            ..pending_input()
        })
        .await
        .unwrap();
    assert!(timed.entries[0].idle_ms >= 10_000);
    assert_eq!(timed.entries[0].deliveries, 7);
    claim.entries = vec!["3-0".into()];
    claim.force = true;
    claim.time_ms = None;
    claim.retry_count = Some(9);
    assert_eq!(
        service.claim_stream_pending_advanced(claim).await.unwrap(),
        vec!["3-0"]
    );

    service
        .update_stream_group_id(UpdateStreamGroupIdInput {
            connection_id: "advanced-test".into(),
            key: "events".into(),
            group: "workers".into(),
            last_delivered_id: "4-0".into(),
        })
        .await
        .unwrap();
    let groups = service
        .get_stream_consumer_groups(GetStreamConsumerGroupsInput {
            connection_id: "advanced-test".into(),
            key: "events".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        groups
            .iter()
            .find(|group| group.name == "workers")
            .unwrap()
            .last_delivered_id,
        "4-0"
    );
    let untouched = groups
        .iter()
        .find(|group| group.name == "untouched")
        .unwrap();
    assert_eq!(untouched.last_delivered_id, "0-0");
    assert_eq!(untouched.pending, 0);
    assert_eq!(untouched.consumers, 0);
    let remaining = service
        .get_stream_pending_page(pending_input())
        .await
        .unwrap();
    assert_eq!(remaining.entries.len(), 3);
    assert_eq!(
        remaining
            .entries
            .iter()
            .find(|entry| entry.id == "2-0")
            .unwrap()
            .consumer,
        "old"
    );
    assert_eq!(
        remaining
            .entries
            .iter()
            .find(|entry| entry.id == "3-0")
            .unwrap()
            .deliveries,
        9
    );
    let after = redis::cmd("XRANGE")
        .arg("events")
        .arg("-")
        .arg("+")
        .query_async::<redis::Value>(&mut connection)
        .await
        .unwrap();
    assert_eq!(before, after);
    let ttl = redis::cmd("PTTL")
        .arg("events")
        .query_async::<i64>(&mut connection)
        .await
        .unwrap();
    assert!(ttl > 590_000 && ttl <= 600_000);
    service.close_connection("advanced-test").await.unwrap();
}
