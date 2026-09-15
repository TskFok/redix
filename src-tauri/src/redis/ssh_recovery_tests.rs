use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

struct RedisFixture {
    forward: Arc<SshForward>,
    writes: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for RedisFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_command(reader: &mut BufReader<tokio::net::TcpStream>) -> Option<Vec<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.ok()? == 0 {
        return None;
    }
    let count: usize = line.strip_prefix('*')?.trim().parse().ok()?;
    let mut arguments = Vec::new();
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.ok()?;
        let size: usize = line.strip_prefix('$')?.trim().parse().ok()?;
        let mut bytes = vec![0; size + 2];
        reader.read_exact(&mut bytes).await.ok()?;
        arguments.push(String::from_utf8(bytes[..size].to_vec()).ok()?);
    }
    Some(arguments)
}

async fn redis_fixture(mode: &'static str) -> RedisFixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let writes = Arc::new(AtomicUsize::new(0));
    let server_writes = writes.clone();
    let task = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let writes = server_writes.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(socket);
                while let Some(args) = read_command(&mut reader).await {
                    if mode == "closed" {
                        break;
                    }
                    let reply = match args[0].as_str() {
                        "AUTH" if mode == "auth_error" => "-WRONGPASS fixture\r\n",
                        "PING" if mode == "ping_denied" => {
                            "-NOPERM this user has no permissions to run the 'ping' command\r\n"
                        }
                        "PING" => "+PONG\r\n",
                        "SET" => {
                            writes.fetch_add(1, Ordering::SeqCst);
                            if mode == "lost_write_reply" {
                                break;
                            }
                            "+OK\r\n"
                        }
                        _ => "+OK\r\n",
                    };
                    if reader.get_mut().write_all(reply.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    let target = ConnectionEndpoint {
        host: "redis.internal".into(),
        port: 6379,
    };
    let (transport, _) =
        super::super::ssh::test_forwarding_transport(vec![(target.clone(), address)]);
    let forward = transport.forward(&target).await.unwrap();
    RedisFixture {
        forward,
        writes,
        task,
    }
}

fn client_with_reconnect(
    forward: Arc<SshForward>,
    reconnect: impl Fn() -> futures_util::future::BoxFuture<'static, Result<Arc<SshForward>, AppError>>
        + Send
        + Sync
        + 'static,
) -> TunneledClient {
    let mut client = TunneledClient::from_ssh_forward(
        redis::RedisConnectionInfo::default()
            .set_password("fixture")
            .set_skip_set_lib_name(),
        ConnectionEndpoint {
            host: "redis.internal".into(),
            port: 6379,
        },
        None,
        forward.clone(),
    );
    client._forward = TunnelLifetime::Recoverable(Arc::new(RecoverableSshTunnel {
        state: tokio::sync::Mutex::new(SshTunnelState {
            forward,
            retry_after: None,
        }),
        reconnect: Box::new(reconnect),
    }));
    client
}

fn recovery_client(
    old: &RedisFixture,
    replacement: &RedisFixture,
) -> (TunneledClient, Arc<AtomicUsize>) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = attempts.clone();
    let replacement = replacement.forward.clone();
    let client = client_with_reconnect(old.forward.clone(), move || {
        counter.fetch_add(1, Ordering::SeqCst);
        let replacement = replacement.clone();
        Box::pin(async move { Ok(replacement) })
    });
    (client, attempts)
}

#[tokio::test]
async fn reconnects_failed_ssh_before_returning_a_new_redis_connection() {
    let old = redis_fixture("closed").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    let mut connection = client
        .connection()
        .await
        .expect("dead SSH handshake should recover");
    let pong: String = redis::cmd("PING")
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(pong, "PONG");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn passwordless_db_zero_is_probed_before_returning_a_redis_socket() {
    let old = redis_fixture("closed").await;
    let new = redis_fixture("healthy").await;
    let (mut client, attempts) = recovery_client(&old, &new);
    client.redis_info = redis::RedisConnectionInfo::default().set_skip_set_lib_name();
    let mut connection = client.connection().await.unwrap();
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "an empty setup pipeline must still detect a dead SSH session"
    );
    assert_eq!(
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .unwrap(),
        "PONG"
    );
}

#[tokio::test]
async fn passwordless_pubsub_is_probed_before_returning_a_session() {
    let old = redis_fixture("closed").await;
    let new = redis_fixture("healthy").await;
    let (mut client, attempts) = recovery_client(&old, &new);
    client.redis_info = redis::RedisConnectionInfo::default().set_skip_set_lib_name();
    let mut connection = client.pubsub().await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(connection.ping::<String>().await.unwrap(), "PONG");
}

#[tokio::test]
async fn probe_permission_denial_proves_transport_is_alive_without_requiring_new_acl_rights() {
    let old = redis_fixture("ping_denied").await;
    let new = redis_fixture("healthy").await;
    let (mut client, attempts) = recovery_client(&old, &new);
    client.redis_info = redis::RedisConnectionInfo::default().set_skip_set_lib_name();
    let mut connection = client
        .connection()
        .await
        .expect("a Sentinel account need not grant PING");
    assert_eq!(
        redis::cmd("SET")
            .arg("key")
            .arg("value")
            .query_async::<String>(&mut connection)
            .await
            .unwrap(),
        "OK"
    );
    assert!(client.pubsub().await.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_connections_share_one_ssh_reconnect() {
    let old = redis_fixture("closed").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    let results = futures_util::future::join_all((0..4).map(|_| client.connection())).await;
    assert!(
        results.iter().all(Result::is_ok),
        "all new connections should recover"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn redis_authentication_errors_do_not_reconnect_ssh() {
    let old = redis_fixture("auth_error").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    assert!(matches!(
        client.connection().await,
        Err(AppError::AuthenticationFailed)
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failed_business_command_is_never_replayed_after_ssh_disconnect() {
    let old = redis_fixture("lost_write_reply").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    let mut connection = client.connection().await.unwrap();
    let result = redis::cmd("SET")
        .arg("key")
        .arg("value")
        .query_async::<String>(&mut connection)
        .await;
    assert!(result.is_err());
    assert_eq!(old.writes.load(Ordering::SeqCst), 1);
    assert_eq!(new.writes.load(Ordering::SeqCst), 0);
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_failed_reconnects_are_bounded() {
    let old = redis_fixture("closed").await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = attempts.clone();
    let client = client_with_reconnect(old.forward.clone(), move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(AppError::SshTunnelFailed) })
    });
    let results = futures_util::future::join_all((0..4).map(|_| client.connection())).await;
    assert!(results.iter().all(Result::is_err));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn new_pubsub_and_monitor_sessions_recover_the_same_ssh_tunnel() {
    let old = redis_fixture("closed").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    assert!(client.pubsub().await.is_ok());
    assert!(client.monitor_stream().await.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn replacing_a_forward_keeps_existing_redis_sockets_alive() {
    let old = redis_fixture("healthy").await;
    let new = redis_fixture("healthy").await;
    let (client, attempts) = recovery_client(&old, &new);
    let mut existing = client.connection().await.unwrap();
    // Refuse new connections while keeping the already accepted Redis socket alive.
    // A failed new handshake must not destroy an unrelated working CLI/PubSub socket.
    old.task.abort();
    while !old.task.is_finished() {
        tokio::task::yield_now().await;
    }
    drop(old);
    let _replacement = client.connection().await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    let pong = redis::cmd("PING")
        .query_async::<String>(&mut existing)
        .await;
    assert_eq!(pong.unwrap(), "PONG");
}

#[tokio::test]
async fn cancelling_recovery_releases_the_tunnel_without_publishing_later() {
    let old = redis_fixture("closed").await;
    let started = Arc::new(tokio::sync::Notify::new());
    let notify = started.clone();
    let client = client_with_reconnect(old.forward.clone(), move || {
        let notify = notify.clone();
        Box::pin(async move {
            notify.notify_one();
            std::future::pending().await
        })
    });
    let TunnelLifetime::Recoverable(tunnel) = &client._forward else {
        unreachable!()
    };
    let weak = Arc::downgrade(tunnel);
    let pending = tokio::spawn(async move { client.connection().await });
    tokio::time::timeout(Duration::from_secs(1), started.notified())
        .await
        .unwrap();
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));
    assert!(
        weak.upgrade().is_none(),
        "cancelled recovery must not keep the client alive"
    );
}
