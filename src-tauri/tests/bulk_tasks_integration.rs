//! This ignored test starts and verifies its own disposable Redis; no user URL is accepted.
use redix_lib::{
    domain::ConnectionProfile,
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

use redix_lib::redis::bulk_tasks::{BulkTaskManager, BulkTaskStatus, StartBulkDeleteInput};
async fn completed(manager: &BulkTaskManager, id: &str) -> redix_lib::redis::bulk_tasks::BulkTask {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let task = manager
                .list()
                .unwrap()
                .into_iter()
                .find(|task| task.id == id)
                .unwrap();
            if task.status != BulkTaskStatus::Running {
                return task;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis验证后台任务"]
async fn deletes_distinct_keys_and_stops_when_original_connection_closes() {
    let (_server, service, mut connection) = isolated_server().await;
    let manager = BulkTaskManager::default();
    redis::cmd("MSET")
        .arg("first")
        .arg("1")
        .arg("second")
        .arg("2")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let task = service
        .start_bulk_delete(
            &manager,
            StartBulkDeleteInput {
                connection_id: "isolated".into(),
                keys: vec![
                    "first".into(),
                    "first".into(),
                    "second".into(),
                    "absent".into(),
                ],
            },
        )
        .await
        .unwrap();
    let task = completed(&manager, &task.id).await;
    assert_eq!(task.status, BulkTaskStatus::Completed);
    assert_eq!((task.processed, task.deleted, task.failed), (3, 2, 0));
    let mut pipe = redis::pipe();
    let keys: Vec<_> = (0..100).map(|n| format!("cancel:{n}")).collect();
    for key in &keys {
        pipe.cmd("SET").arg(key).arg("value").ignore();
    }
    pipe.query_async::<()>(&mut connection).await.unwrap();
    redis::cmd("CLIENT")
        .arg("PAUSE")
        .arg(300)
        .arg("WRITE")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let task = service
        .start_bulk_delete(
            &manager,
            StartBulkDeleteInput {
                connection_id: "isolated".into(),
                keys: keys.clone(),
            },
        )
        .await
        .unwrap();
    service.close_connection("isolated").await.unwrap();
    let task = completed(&manager, &task.id).await;
    assert_eq!(task.status, BulkTaskStatus::Cancelled);
    assert!(task.processed < 100);
    let left = redis::cmd("EXISTS")
        .arg(&keys)
        .query_async::<u64>(&mut connection)
        .await
        .unwrap();
    assert_eq!(left + task.deleted, 100);
}
