use redix_lib::{
    domain::{stream_entries::*, ConnectionProfile},
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
fn query() -> GetStreamEntriesInput {
    GetStreamEntriesInput {
        connection_id: "stream-test".into(),
        key: "events".into(),
        start: "-".into(),
        end: "+".into(),
        cursor: None,
        count: 500,
        reverse: false,
    }
}
fn add(id: &str, key: &str) -> AddStreamEntryInput {
    AddStreamEntryInput {
        connection_id: "stream-test".into(),
        key: key.into(),
        id: id.into(),
        fields: vec![
            StreamEntryField {
                field: "same".into(),
                value: "first".into(),
            },
            StreamEntryField {
                field: "same".into(),
                value: "".into(),
            },
        ],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "会启动并销毁独立临时 Redis：cargo test --test stream_entries_integration -- --ignored"]
async fn isolated_stream_pages_and_mutations_preserve_groups_pending_and_ttl() {
    let (_server, port) = IsolatedRedis::start();
    let service = RedisService::new(
        Arc::new(Profiles(ConnectionProfile {
            id: "stream-test".into(),
            name: "Stream test".into(),
            host: "127.0.0.1".into(),
            port,
            username: None,
            database: 0,
            has_password: false,
            tls: false,
            verify_server_cert: true,
            ssh: None,
            sentinel: None,
            ca_certificate_name: None,
            client_certificate_name: None,
            has_ca_certificate: false,
            has_client_certificate: false,
        })),
        Arc::new(Secrets),
    );
    service.open_connection("stream-test").await.unwrap();
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let mut setup = redis::pipe();
    for sequence in 1..=501 {
        setup
            .cmd("XADD")
            .arg("events")
            .arg(format!("0-{sequence}"))
            .arg("kind")
            .arg("test")
            .ignore();
    }
    setup
        .cmd("XGROUP")
        .arg("CREATE")
        .arg("events")
        .arg("workers")
        .arg("0")
        .ignore();
    setup.cmd("PEXPIRE").arg("events").arg(600_000).ignore();
    setup.query_async::<()>(&mut connection).await.unwrap();
    redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("workers")
        .arg("consumer")
        .arg("COUNT")
        .arg(1)
        .arg("STREAMS")
        .arg("events")
        .arg(">")
        .query_async::<redis::Value>(&mut connection)
        .await
        .unwrap();
    let before_ttl: i64 = redis::cmd("PTTL")
        .arg("events")
        .query_async(&mut connection)
        .await
        .unwrap();
    let high = "9007199254740993-18446744073709551615";
    assert_eq!(
        service.add_stream_entry(add(high, "events")).await.unwrap(),
        high
    );
    let first = service.get_stream_entries(query()).await.unwrap();
    assert_eq!(first.entries.len(), 500);
    assert_eq!(first.next_cursor.as_deref(), Some("0-500"));
    assert!(first.has_more);
    let mut next = query();
    next.cursor = first.next_cursor;
    let last = service.get_stream_entries(next).await.unwrap();
    assert_eq!(
        last.entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["0-501", high]
    );
    assert!(!last.has_more);
    assert_eq!(last.next_cursor, None);
    assert_eq!(last.entries[1].fields, add(high, "events").fields);
    let mut reverse = query();
    reverse.reverse = true;
    reverse.count = 2;
    let page = service.get_stream_entries(reverse.clone()).await.unwrap();
    assert_eq!(
        page.entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [high, "0-501"]
    );
    reverse.cursor = page.next_cursor;
    let page = service.get_stream_entries(reverse).await.unwrap();
    assert_eq!(
        page.entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["0-500", "0-499"]
    );
    let mut bounded = query();
    bounded.start = "0-499".into();
    bounded.end = "0-500".into();
    bounded.count = 1;
    bounded.reverse = true;
    let page = service.get_stream_entries(bounded.clone()).await.unwrap();
    assert_eq!(page.entries[0].id, "0-500");
    bounded.cursor = page.next_cursor;
    let page = service.get_stream_entries(bounded).await.unwrap();
    assert_eq!(page.entries[0].id, "0-499");
    assert!(!page.has_more);
    assert_eq!(
        service
            .delete_stream_entries(DeleteStreamEntriesInput {
                connection_id: "stream-test".into(),
                key: "events".into(),
                ids: vec!["0-1".into(), "42-0".into()]
            })
            .await
            .unwrap(),
        1
    );
    let after_ttl: i64 = redis::cmd("PTTL")
        .arg("events")
        .query_async(&mut connection)
        .await
        .unwrap();
    assert!(after_ttl > 0 && after_ttl <= before_ttl);
    let groups = redis::cmd("XINFO")
        .arg("GROUPS")
        .arg("events")
        .query_async::<Vec<redis::Value>>(&mut connection)
        .await
        .unwrap();
    assert_eq!(groups.len(), 1);
    let pending = redis::cmd("XPENDING")
        .arg("events")
        .arg("workers")
        .query_async::<Vec<redis::Value>>(&mut connection)
        .await
        .unwrap();
    assert_eq!(pending[0], redis::Value::Int(1));
    assert_eq!(
        service.add_stream_entry(add("*", "missing")).await,
        Err(AppError::KeyNotFound)
    );
    assert_eq!(
        redis::cmd("EXISTS")
            .arg("missing")
            .query_async::<u64>(&mut connection)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        service.add_stream_entry(add("0-2", "events")).await,
        Err(AppError::CommandFailed)
    );
    service.close_connection("stream-test").await.unwrap();
}
