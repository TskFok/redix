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

use redix_lib::{
    domain::{AnalyzeDatabaseInput, ClusterConfig, ConnectionEndpoint},
    redis::analysis_tasks::{
        AnalysisTaskKey, AnalysisTaskManager, AnalysisTaskResult, AnalysisTaskStatus,
        StartAnalysisTaskInput,
    },
};
fn task_input(connection_id: &str, pattern: &str) -> StartAnalysisTaskInput {
    StartAnalysisTaskInput {
        analysis: AnalyzeDatabaseInput {
            connection_id: connection_id.into(),
            pattern: pattern.into(),
            delimiter: ":".into(),
            max_keys: 1000,
        },
        database: 0,
        timeout_seconds: Some(5),
    }
}
async fn finish(
    manager: &AnalysisTaskManager,
    id: &str,
    connection_id: &str,
) -> AnalysisTaskResult {
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            let result = manager
                .get(&AnalysisTaskKey {
                    task_id: id.into(),
                    connection_id: connection_id.into(),
                    database: 0,
                })
                .unwrap();
            if result.task.status != AnalysisTaskStatus::Running {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
async fn command_calls(connection: &mut redis::aio::MultiplexedConnection, name: &str) -> u64 {
    let info: String = redis::cmd("INFO")
        .arg("commandstats")
        .query_async(connection)
        .await
        .unwrap();
    info.lines()
        .find_map(|line| {
            line.strip_prefix(&format!("cmdstat_{name}:calls="))
                .and_then(|rest| rest.split(',').next())
                .and_then(|count| count.parse().ok())
        })
        .unwrap_or(0)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "自行启动隔离临时Redis验证同步与后台分析一致"]
async fn standalone_report_matches_existing_algorithm_and_cancel_close_timeout_stop_followup_reads()
{
    let (_server, service, mut connection) = isolated_server().await;
    let manager = AnalysisTaskManager::default();
    let mut pipe = redis::pipe();
    for number in 0..10 {
        pipe.cmd("SET")
            .arg(format!("analysis:{number}"))
            .arg("sample")
            .ignore();
    }
    pipe.query_async::<()>(&mut connection).await.unwrap();
    let input = task_input("isolated", "analysis:*");
    let expected = service
        .analyze_database(input.analysis.clone())
        .await
        .unwrap();
    let task = service.start_analysis_task(&manager, input).await.unwrap();
    let completed = finish(&manager, &task.id, "isolated").await;
    assert_eq!(completed.task.status, AnalysisTaskStatus::Completed);
    assert_eq!(completed.report.unwrap(), expected);
    for action in ["cancel", "close", "timeout"] {
        let scans = command_calls(&mut connection, "scan").await;
        let types = command_calls(&mut connection, "type").await;
        let lengths = command_calls(&mut connection, "strlen").await;
        let pause_ms = if action == "timeout" { 1500 } else { 300 };
        redis::cmd("CLIENT")
            .arg("PAUSE")
            .arg(pause_ms)
            .arg("ALL")
            .query_async::<()>(&mut connection)
            .await
            .unwrap();
        let mut input = task_input("isolated", "analysis:*");
        input.timeout_seconds = Some(1);
        let task = service.start_analysis_task(&manager, input).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        match action {
            "cancel" => manager
                .cancel(&AnalysisTaskKey {
                    task_id: task.id.clone(),
                    connection_id: "isolated".into(),
                    database: 0,
                })
                .unwrap(),
            "close" => service.close_connection("isolated").await.unwrap(),
            _ => {}
        }
        let stopped = finish(&manager, &task.id, "isolated").await;
        assert_eq!(
            stopped.task.status,
            if action == "timeout" {
                AnalysisTaskStatus::TimedOut
            } else {
                AnalysisTaskStatus::Cancelled
            }
        );
        assert!(stopped.report.is_none());
        // The one already-dispatched SCAN may run after CLIENT PAUSE expires; no subsequent pipeline may be sent.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let after_scans = command_calls(&mut connection, "scan").await;
        assert!(after_scans <= scans + 1, "stopped task sent another SCAN");
        assert_eq!(
            command_calls(&mut connection, "type").await,
            types,
            "stopped task sent metadata reads"
        );
        assert_eq!(
            command_calls(&mut connection, "strlen").await,
            lengths,
            "stopped task sent length reads"
        );
        if action == "close" {
            service.open_connection("isolated").await.unwrap();
        }
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
    RedisService::new(Arc::new(Profiles(profile)), Arc::new(Secrets))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置隔离 REDIX_TEST_REDIS_CLUSTER_URLS 运行"]
async fn cluster_background_report_matches_existing_primary_quotas_and_nodes() {
    let Ok(seeds) = std::env::var("REDIX_TEST_REDIS_CLUSTER_URLS") else {
        eprintln!("skipped: 后台 Cluster 分析测试需要隔离 REDIX_TEST_REDIS_CLUSTER_URLS");
        return;
    };
    let service = cluster_service(&seeds);
    service.open_connection("cluster").await.unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!("redix:background-analysis:{nonce}");
    let mut keys = Vec::new();
    for number in 0..24 {
        let key = format!("{prefix}:{number}");
        service
            .execute_command("cluster", &format!("SET {key} sample"))
            .await
            .unwrap();
        keys.push(key);
    }
    let flow = async {
        let input = task_input("cluster", &format!("{prefix}:*"));
        let expected = service
            .analyze_database(input.analysis.clone())
            .await
            .unwrap();
        let manager = AnalysisTaskManager::default();
        let task = service.start_analysis_task(&manager, input).await.unwrap();
        let completed = finish(&manager, &task.id, "cluster").await;
        assert_eq!(completed.task.status, AnalysisTaskStatus::Completed);
        assert_eq!(completed.report.unwrap(), expected);
    }
    .await;
    for key in keys {
        service
            .execute_command("cluster", &format!("UNLINK {key}"))
            .await
            .unwrap();
    }
    service.close_connection("cluster").await.unwrap();
    flow
}
