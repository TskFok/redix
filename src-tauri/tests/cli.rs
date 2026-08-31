mod support;

use redix_lib::{
    commands::{cli as cli_commands, connections},
    domain::{CliCommandInput, CliSessionInput, ConnectionProfile},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{CliManager, RedisOperations, RedisService},
    AppState,
};
use std::{
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;

struct Profiles(Mutex<Vec<ConnectionProfile>>);
impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, values: &[ConnectionProfile]) -> Result<(), AppError> {
        *self.0.lock().unwrap() = values.to_vec();
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
    port: u16,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Server {
    async fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("redis.conf");
        std::fs::write(
            &config,
            format!(
                "bind 127.0.0.1\nport {port}\nsave \"\"\nappendonly no\ndir {}\n",
                directory.path().display()
            ),
        )
        .unwrap();
        let child = Command::new("redis-server")
            .arg(config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("redis-server required");
        let server = Self {
            child,
            _directory: directory,
            port,
        };
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return server;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("Redis did not start");
    }
}

fn session() -> CliSessionInput {
    CliSessionInput {
        connection_id: "local".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
    }
}
fn command(session: &CliSessionInput, command: &str) -> CliCommandInput {
    CliCommandInput {
        connection_id: session.connection_id.clone(),
        session_id: session.session_id.clone(),
        command: command.into(),
    }
}

#[test]
fn cli_rejects_push_mode_and_reply_suppression_even_with_quoted_names() {
    let session = session();
    for raw in [
        "SUBSCRIBE events",
        "'MONITOR'",
        "PSUBSCRIBE *",
        "SSUBSCRIBE events",
        "CLIENT REPLY OFF",
        "CLIENT TRACKING ON",
        "SYNC",
        "PSYNC ? -1",
    ] {
        assert!(command(&session, raw).validate().is_err(), "{raw}");
    }
    assert!(command(&session, "MULTI").validate().is_ok());
    assert!(command(&session, "SELECT 1").validate().is_ok());
}

#[tokio::test]
async fn failed_cli_open_releases_its_reservation() {
    let service = RedisService::new(Arc::new(Profiles(Mutex::new(vec![]))), Arc::new(Secrets));
    let cli = CliManager::new();
    let input = session();
    for _ in 0..20 {
        assert_eq!(
            cli.open(&service, input.clone()).await,
            Err(AppError::ConnectionFailed)
        );
    }
}

#[tokio::test]
#[ignore = "需要本地 redis-server，使用 cargo test --test cli -- --ignored"]
async fn cli_keeps_transactions_and_database_state_without_affecting_browser() {
    let server = Server::start().await;
    let profile = ConnectionProfile {
        port: server.port,
        ..support::valid_profile()
    };
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![profile]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    assert_eq!(
        cli.execute(command(&input, "SELECT 1"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        "OK"
    );
    for expected in [1, 2] {
        assert_eq!(
            cli.execute(command(&input, "MULTI"))
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            "OK"
        );
        assert_eq!(
            cli.execute(command(&input, "INCR cli-counter"))
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            "QUEUED"
        );
        assert_eq!(
            cli.execute(command(&input, "EXEC"))
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            serde_json::json!([expected])
        );
    }
    assert_eq!(
        service
            .execute_command("local", "GET cli-counter")
            .await
            .unwrap()
            .value,
        serde_json::Value::Null
    );
    cli.execute(command(&input, "MULTI")).await.unwrap();
    cli.execute(command(&input, "SET uncommitted value"))
        .await
        .unwrap();
    cli.close(input.clone()).await.unwrap();
    assert!(cli.execute(command(&input, "PING")).await.is_err());
    let reopened = session();
    cli.open(&service, reopened.clone()).await.unwrap();
    assert_eq!(
        cli.execute(command(&reopened, "GET cli-counter"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        serde_json::Value::Null
    );
    cli.execute(command(&reopened, "SELECT 1")).await.unwrap();
    assert_eq!(
        cli.execute(command(&reopened, "GET uncommitted"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        serde_json::Value::Null
    );
    cli.close_connection("local").await;
    assert!(cli.execute(command(&reopened, "PING")).await.is_err());
    service.close_connection("local").await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server，超时用例约 5 秒"]
async fn cli_discards_blocked_socket_on_timeout_and_closes_in_flight_queries() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    cli.execute(command(&input, "CLIENT SETNAME cli-timeout"))
        .await
        .unwrap();
    let reply = cli
        .execute(command(&input, "BLPOP missing 0"))
        .await
        .unwrap();
    assert!(reply.session_closed);
    assert_eq!(reply.error_code.as_deref(), Some("COMMAND_TIMEOUT"));
    assert!(cli.execute(command(&input, "PING")).await.is_err());
    assert!(!service
        .execute_command("local", "CLIENT LIST")
        .await
        .unwrap()
        .value
        .as_str()
        .unwrap()
        .contains("name=cli-timeout"));
    cli.close(input).await.unwrap();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    cli.execute(command(&input, "CLIENT SETNAME cli-cancel"))
        .await
        .unwrap();
    let execute = cli.execute(command(&input, "BLPOP missing 0"));
    let close = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cli.close(input.clone()).await.unwrap();
    };
    let (reply, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(execute, close)
    })
    .await
    .unwrap();
    assert!(reply.unwrap().session_closed);
    assert!(!service
        .execute_command("local", "CLIENT LIST")
        .await
        .unwrap()
        .value
        .as_str()
        .unwrap()
        .contains("name=cli-cancel"));
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_bounds_session_count_and_reuses_capacity_after_close() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    for _ in 0..16 {
        cli.open(&service, session()).await.unwrap();
    }
    let input = session();
    assert_eq!(
        cli.open(&service, input.clone()).await,
        Err(AppError::InvalidInput)
    );
    cli.close_connection("local").await;
    cli.open(&service, input.clone()).await.unwrap();
    assert_eq!(
        cli.execute(command(&input, "PING"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        "PONG"
    );
    cli.close(input).await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_close_cancels_in_flight_open_without_waiting_for_network() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    service
        .execute_command("local", "CLIENT PAUSE 1500 ALL")
        .await
        .unwrap();
    let cli = CliManager::new();
    let input = session();
    let opening = cli.open(&service, input.clone());
    let close = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cli.close(input.clone()).await.unwrap();
    };
    let (opened, ()) = tokio::time::timeout(Duration::from_millis(700), async {
        tokio::join!(opening, close)
    })
    .await
    .expect("close must not wait for Redis handshake");
    assert!(opened.is_err());
    assert!(cli.execute(command(&input, "PING")).await.is_err());
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_exec_keeps_successful_items_when_other_commands_fail_at_runtime() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    for raw in [
        "SET review-type string",
        "MULTI",
        "LPUSH review-type value",
        "INCR review-committed",
    ] {
        cli.execute(command(&input, raw)).await.unwrap();
    }
    let reply = cli.execute(command(&input, "EXEC")).await.unwrap();
    assert_eq!(reply.error_code, None);
    assert!(!reply.session_closed);
    assert_eq!(
        reply.result.unwrap().value,
        serde_json::json!([{"error_code": "COMMAND_FAILED"}, 1])
    );
    assert_eq!(
        cli.execute(command(&input, "GET review-committed"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        "1"
    );
    let nested = cli
        .execute(command(
            &input,
            "EVAL 'return {{redis.error_reply(\"private-error-details\"), 42}}' 0",
        ))
        .await
        .unwrap();
    assert_eq!(
        nested.result.as_ref().unwrap().value,
        serde_json::json!([[{"error_code": "COMMAND_FAILED"}, 42]])
    );
    assert!(!serde_json::to_string(&nested)
        .unwrap()
        .contains("private-error-details"));
    let top = cli
        .execute(command(
            &input,
            "EVAL 'return redis.error_reply(\"private-error-details\")' 0",
        ))
        .await
        .unwrap();
    assert_eq!(top.error_code.as_deref(), Some("COMMAND_FAILED"));
    assert!(top.result.is_none());
    assert!(!serde_json::to_string(&top)
        .unwrap()
        .contains("private-error-details"));
    cli.close(input).await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_cancelled_open_cannot_remove_or_replace_reopened_session() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    service
        .execute_command("local", "CLIENT PAUSE 1000 ALL")
        .await
        .unwrap();
    let cli = CliManager::new();
    let input = session();
    let opening = cli.open(&service, input.clone());
    let replacement = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cli.close_connection("local").await;
        cli.open(&service, input.clone()).await.unwrap();
    };
    let (opened, ()) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(opening, replacement)
    })
    .await
    .unwrap();
    assert_eq!(opened, Err(AppError::OperationCancelled));
    assert_eq!(
        cli.execute(command(&input, "PING"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        "PONG"
    );
    cli.close(input).await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_serializes_commands_on_one_socket() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    let blocking = cli.execute(command(&input, "BLPOP queue 0"));
    let queued = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cli.execute(command(&input, "SET after-block yes"))
            .await
            .unwrap()
    };
    let release = async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            service
                .execute_command("local", "GET after-block")
                .await
                .unwrap()
                .value,
            serde_json::Value::Null
        );
        service
            .execute_command("local", "LPUSH queue released")
            .await
            .unwrap();
    };
    let (first, second, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(blocking, queued, release)
    })
    .await
    .unwrap();
    assert_eq!(
        first.unwrap().result.unwrap().value,
        serde_json::json!(["queue", "released"])
    );
    assert_eq!(second.result.unwrap().value, "OK");
    assert_eq!(
        service
            .execute_command("local", "GET after-block")
            .await
            .unwrap()
            .value,
        "yes"
    );
    cli.close(input).await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn cli_truncates_large_output_and_redacts_redis_errors_without_losing_session() {
    let server = Server::start().await;
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    service.open_connection("local").await.unwrap();
    let cli = CliManager::new();
    let input = session();
    cli.open(&service, input.clone()).await.unwrap();
    let large = cli
        .execute(command(&input, "EVAL 'return string.rep(\"x\",300000)' 0"))
        .await
        .unwrap();
    assert!(large.truncated);
    assert!(!large.session_closed);
    assert!(serde_json::to_vec(&large).unwrap().len() < 1024);
    let error = cli
        .execute(command(&input, "unknown-sensitive-command"))
        .await
        .unwrap();
    assert_eq!(error.error_code.as_deref(), Some("COMMAND_FAILED"));
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("unknown-sensitive-command"));
    assert_eq!(
        cli.execute(command(&input, "PING"))
            .await
            .unwrap()
            .result
            .unwrap()
            .value,
        "PONG"
    );
    cli.close(input).await.unwrap();
}

#[tokio::test]
#[ignore = "需要本地 redis-server"]
async fn closing_main_connection_via_command_also_closes_cli_socket() {
    let server = Server::start().await;
    let state = AppState::new(
        Arc::new(Profiles(Mutex::new(vec![ConnectionProfile {
            port: server.port,
            ..support::valid_profile()
        }]))),
        Arc::new(Secrets),
    );
    let app = tauri::test::mock_app();
    app.manage(state);
    connections::open_connection(app.state(), "local".into())
        .await
        .unwrap();
    let input = session();
    cli_commands::open_cli_session(app.state(), input.clone())
        .await
        .unwrap();
    connections::close_connection(app.state(), "local".into())
        .await
        .unwrap();
    assert_eq!(
        cli_commands::execute_cli_command(app.state(), command(&input, "PING"))
            .await
            .unwrap_err(),
        AppError::ConnectionFailed
    );
}
