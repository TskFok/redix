use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use futures_util::StreamExt;
use redis::aio::ConnectionLike;
use redix_lib::domain::{ClusterConfig, ConnectionEndpoint};
use redix_lib::redis::{RoutedClient, StandaloneClient, TlsClientMaterial, TunneledClient};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn spawn_fake_redis() -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut pending = Vec::new();
        let mut read = [0_u8; 4096];
        loop {
            let size = match socket.read(&mut read).await {
                Ok(0) | Err(_) => return,
                Ok(size) => size,
            };
            pending.extend_from_slice(&read[..size]);
            while let Some((consumed, command)) = parse_resp_command(&pending) {
                pending.drain(..consumed);
                let response = match command.first().map(Vec::as_slice) {
                    Some(b"PING") => b"+PONG\r\n".as_slice(),
                    _ => b"+OK\r\n".as_slice(),
                };
                socket.write_all(response).await.unwrap();
            }
        }
    });
    port
}

type ObservedCommands = Arc<Mutex<Vec<Vec<Vec<u8>>>>>;
type ObservedConnections = Arc<Mutex<Vec<Vec<Vec<Vec<u8>>>>>>;

const TEST_CA: &str = r#"-----BEGIN CERTIFICATE-----
MIIBlTCCATugAwIBAgIUNEVZjgQ6xHhWfUTAWLnOrb0sNRcwCgYIKoZIzj0EAwIw
GDEWMBQGA1UEAwwNUmVkaXggVGVzdCBDQTAeFw0yNjA5MDIwOTIzMDRaFw0zNjA4
MzAwOTIzMDRaMBgxFjAUBgNVBAMMDVJlZGl4IFRlc3QgQ0EwWTATBgcqhkjOPQIB
BggqhkjOPQMBBwNCAAShIa1pcamamf3GdeLyIRQ4cInInng0/5H8c2TsdMa+mHHK
JxInVV9VTSywLBIfNElryoxTtR/4d/4Jm/UopI+To2MwYTAdBgNVHQ4EFgQU8665
SzPKr5cvfi3TMbORTiT7Av4wHwYDVR0jBBgwFoAU8665SzPKr5cvfi3TMbORTiT7
Av4wDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwCgYIKoZIzj0EAwID
SAAwRQIhAMj0L7a36q+GFqPZ2GK+hhfHpsUtyjro9zlq6dQsfEabAiAc5lu5EY4e
Ajjlw7rdTaP3SJA8CVFyVOT4siWiAJvwsQ==
-----END CERTIFICATE-----
"#;
const TEST_SERVER_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBxTCCAWugAwIBAgIUJrqzrus0OeHJc4plw8TswbjYNhAwCgYIKoZIzj0EAwIw
GDEWMBQGA1UEAwwNUmVkaXggVGVzdCBDQTAeFw0yNjA5MDIwOTIzMDRaFw0zNjA4
MzAwOTIzMDRaMBkxFzAVBgNVBAMMDmNhY2hlLmludGVybmFsMFkwEwYHKoZIzj0C
AQYIKoZIzj0DAQcDQgAE9vYoE+AAk3CmXwFFM/EtIcDg4oscPWEiTb+FOm0VsiIu
973FLq4/eXTDtzxbZ/TlTgxUYq0+zKgSTNzo7bZ7E6OBkTCBjjAMBgNVHRMBAf8E
AjAAMA4GA1UdDwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEFBQcDATAZBgNVHREE
EjAQgg5jYWNoZS5pbnRlcm5hbDAdBgNVHQ4EFgQUW2ST5EeDywrEQXCq6V/szpaX
SnkwHwYDVR0jBBgwFoAU8665SzPKr5cvfi3TMbORTiT7Av4wCgYIKoZIzj0EAwID
SAAwRQIhAIf4cSkcsInz1Gs/XcFflIkmZzkM5ikRYN598hQP19WIAiBxGKqXQQmD
kOq6G4FuxZ3fU3d5yW+BLVwSneRiIaCbww==
-----END CERTIFICATE-----
"#;
const TEST_SERVER_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgmIYd7cs7Gxzdq0rS
Ymnw0YzaAQkbf2ywPQ+isYIgv++hRANCAAT29igT4ACTcKZfAUUz8S0hwODiixw9
YSJNv4U6bRWyIi73vcUurj95dMO3PFtn9OVODFRirT7MqBJM3OjttnsT
-----END PRIVATE KEY-----
"#;

async fn spawn_fake_cluster() -> (u16, ObservedCommands) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let server_observed = observed.clone();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let connection_observed = server_observed.clone();
            tokio::spawn(async move {
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut read).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        connection_observed.lock().unwrap().push(command.clone());
                        let response = match command.first().map(Vec::as_slice) {
                            Some(b"CLUSTER")
                                if command.get(1).map(Vec::as_slice) == Some(b"SLOTS") =>
                            {
                                format!(
                                    "*1\r\n*3\r\n:0\r\n:16383\r\n*2\r\n$9\r\n127.0.0.1\r\n:{port}\r\n"
                                )
                            }
                            Some(b"PING") => "+PONG\r\n".to_owned(),
                            _ => "+OK\r\n".to_owned(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    (port, observed)
}

async fn spawn_fake_monitor_server() -> (u16, ObservedConnections) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let server_observed = observed.clone();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let connection_observed = server_observed.clone();
            tokio::spawn(async move {
                let mut commands = Vec::new();
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut read).await {
                        Ok(0) | Err(_) => break,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let is_monitor = command.first().map(Vec::as_slice) == Some(b"MONITOR");
                        commands.push(command);
                        socket.write_all(b"+OK\r\n").await.unwrap();
                        if is_monitor {
                            socket
                                .write_all(b"+1.0 [3 local] \"PING\"\r\n")
                                .await
                                .unwrap();
                        }
                    }
                }
                connection_observed.lock().unwrap().push(commands);
            });
        }
    });
    (port, observed)
}

async fn spawn_fake_tls_redis() -> (u16, Arc<Mutex<Vec<String>>>) {
    let certs = rustls_pemfile::certs(&mut TEST_SERVER_CERT.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = rustls_pemfile::private_key(&mut TEST_SERVER_KEY.as_bytes())
        .unwrap()
        .unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .unwrap();
    let config = Arc::new(config);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_names = Arc::new(Mutex::new(Vec::new()));
    let observed = server_names.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let config = config.clone();
            let observed = observed.clone();
            tokio::spawn(async move {
                let start = tokio_rustls::LazyConfigAcceptor::new(
                    rustls::server::Acceptor::default(),
                    socket,
                )
                .await
                .unwrap();
                observed.lock().unwrap().push(
                    start
                        .client_hello()
                        .server_name()
                        .unwrap_or_default()
                        .to_owned(),
                );
                let mut stream = start.into_stream(config).await.unwrap();
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match stream.read(&mut read).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let is_monitor = command.first().map(Vec::as_slice) == Some(b"MONITOR");
                        let response = if command.first().map(Vec::as_slice) == Some(b"PING") {
                            b"+PONG\r\n".as_slice()
                        } else {
                            b"+OK\r\n".as_slice()
                        };
                        stream.write_all(response).await.unwrap();
                        if is_monitor {
                            stream
                                .write_all(b"+1.0 [3 local] \"PING\"\r\n")
                                .await
                                .unwrap();
                        }
                    }
                }
            });
        }
    });
    (port, server_names)
}

async fn spawn_monitor_reply_server(reply: Vec<u8>) -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut pending = Vec::new();
        let mut read = [0_u8; 4096];
        loop {
            let size = socket.read(&mut read).await.unwrap();
            if size == 0 {
                return;
            }
            pending.extend_from_slice(&read[..size]);
            if let Some((_, command)) = parse_resp_command(&pending) {
                assert_eq!(command, vec![b"MONITOR".to_vec()]);
                socket.write_all(b"+OK\r\n").await.unwrap();
                socket.write_all(&reply).await.unwrap();
                return;
            }
        }
    });
    port
}

async fn monitor_commands(redis_info: redis::RedisConnectionInfo) -> Vec<Vec<Vec<u8>>> {
    let (port, observed) = spawn_fake_monitor_server().await;
    let client = TunneledClient::new(
        redis_info,
        ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        },
        None,
        Arc::new(()),
    );
    let mut monitor = client.monitor_stream().await.unwrap();
    assert!(monitor.next().await.unwrap().is_ok());
    drop(monitor);
    for _ in 0..100 {
        if let Some(commands) = observed.lock().unwrap().first().cloned() {
            return commands;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("monitor connection was not closed");
}

fn parse_resp_command(input: &[u8]) -> Option<(usize, Vec<Vec<u8>>)> {
    let (count, mut cursor) = parse_number_line(input, b'*', 0)?;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        let (length, data_start) = parse_number_line(input, b'$', cursor)?;
        let data_end = data_start.checked_add(length)?;
        if input.get(data_end..data_end + 2)? != b"\r\n" {
            return None;
        }
        args.push(input[data_start..data_end].to_vec());
        cursor = data_end + 2;
    }
    Some((cursor, args))
}

fn parse_number_line(input: &[u8], prefix: u8, start: usize) -> Option<(usize, usize)> {
    if *input.get(start)? != prefix {
        return None;
    }
    let line = input.get(start + 1..)?;
    let end = line.windows(2).position(|window| window == b"\r\n")?;
    let number = std::str::from_utf8(&line[..end]).ok()?.parse().ok()?;
    Some((number, start + 1 + end + 2))
}

#[tokio::test]
async fn standalone_route_delegates_database_commands_and_driver() {
    let port = spawn_fake_redis().await;
    let direct = redis::Client::open(format!("redis://127.0.0.1:{port}/3")).unwrap();
    let client = RoutedClient::Standalone(StandaloneClient::Direct(direct));

    assert_eq!(client.topology_kind(), "standalone");
    assert!(client.standalone_client().is_ok());

    let mut connection = client.connection().await.unwrap();
    assert_eq!(ConnectionLike::get_db(&connection), 3);
    let pong = tokio::time::timeout(
        Duration::from_secs(1),
        redis::cmd("PING").query_async::<String>(&mut connection),
    )
    .await
    .expect("multiplexed driver was not running")
    .unwrap();
    assert_eq!(pong, "PONG");
}

#[tokio::test]
async fn standalone_route_delegates_pipelines_with_requested_response_count() {
    let port = spawn_fake_redis().await;
    let client = RoutedClient::Standalone(StandaloneClient::Direct(
        redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap(),
    ));
    let mut connection = client.connection().await.unwrap();
    let mut pipeline = redis::pipe();
    pipeline.cmd("PING").cmd("PING");
    let values = pipeline
        .query_async::<(String, String)>(&mut connection)
        .await
        .unwrap();
    assert_eq!(values, ("PONG".into(), "PONG".into()));
}

#[tokio::test]
async fn cluster_route_is_db_zero_reuses_connection_and_hides_standalone_capability() {
    let (port, _) = spawn_fake_cluster().await;
    let cluster = redis::cluster::ClusterClient::new(vec![format!("redis://127.0.0.1:{port}/")])
        .unwrap()
        .get_async_connection()
        .await
        .unwrap();
    let client = RoutedClient::Cluster(cluster);

    assert_eq!(client.topology_kind(), "cluster");
    assert!(matches!(
        client.standalone_client(),
        Err(redix_lib::error::AppError::UnsupportedFeature)
    ));

    let mut first = client.connection().await.unwrap();
    let second = client.connection().await.unwrap();
    assert_eq!(ConnectionLike::get_db(&first), 0);
    assert_eq!(ConnectionLike::get_db(&second), 0);
    assert_eq!(
        redis::cmd("PING")
            .query_async::<String>(&mut first)
            .await
            .unwrap(),
        "PONG"
    );
    let mut pipeline = redis::pipe();
    pipeline.cmd("PING").cmd("PING");
    assert_eq!(
        pipeline
            .query_async::<(String, String)>(&mut first)
            .await
            .unwrap(),
        ("PONG".into(), "PONG".into())
    );
}

#[tokio::test]
async fn cluster_builder_uses_shared_acl_credentials_and_opens_once() {
    let (port, observed) = spawn_fake_cluster().await;
    let client = RoutedClient::cluster(
        &ClusterConfig {
            nodes: vec![ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port,
            }],
            read_from_replicas: false,
        },
        Some("operator"),
        Some("redis-secret"),
        None,
    )
    .await
    .unwrap();

    let _first = client.connection().await.unwrap();
    let _second = client.connection().await.unwrap();
    let commands = observed.lock().unwrap();
    let auth = commands
        .iter()
        .find(|command| command.first().map(Vec::as_slice) == Some(b"AUTH"))
        .unwrap();
    assert_eq!(
        auth,
        &vec![
            b"AUTH".to_vec(),
            b"operator".to_vec(),
            b"redis-secret".to_vec(),
        ]
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| command.first().map(Vec::as_slice) == Some(b"CLUSTER"))
            .count(),
        1
    );
}

#[tokio::test]
async fn tunneled_pubsub_and_monitor_use_custom_stream_auth_select_and_monitor_order() {
    let (port, observed) = spawn_fake_monitor_server().await;
    let redis_info = redis::RedisConnectionInfo::default()
        .set_username("operator")
        .set_password("redis-secret")
        .set_db(3)
        .set_skip_set_lib_name();
    let client = StandaloneClient::Tunneled(TunneledClient::new(
        redis_info,
        ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        },
        None,
        Arc::new(()),
    ));

    let pubsub = client.pubsub().await.unwrap();
    let mut monitor = client.monitor_stream().await.unwrap();
    assert_eq!(
        monitor.next().await.unwrap().unwrap(),
        "1.0 [3 local] \"PING\""
    );
    drop(pubsub);
    drop(monitor);
    let mut connections = Vec::new();
    for _ in 0..100 {
        connections = observed.lock().unwrap().clone();
        if connections.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    connections.sort_by_key(Vec::len);
    assert_eq!(connections.len(), 2);
    assert_eq!(
        connections[0],
        vec![
            vec![
                b"AUTH".to_vec(),
                b"operator".to_vec(),
                b"redis-secret".to_vec()
            ],
            vec![b"SELECT".to_vec(), b"3".to_vec()],
        ]
    );
    assert_eq!(
        connections[1],
        vec![
            vec![
                b"AUTH".to_vec(),
                b"operator".to_vec(),
                b"redis-secret".to_vec()
            ],
            vec![b"SELECT".to_vec(), b"3".to_vec()],
            vec![b"MONITOR".to_vec()],
        ]
    );
}

#[tokio::test]
async fn tunneled_tls_dials_loopback_but_uses_original_dns_name_for_sni_and_validation() {
    let (port, server_names) = spawn_fake_tls_redis().await;
    let client = TunneledClient::new(
        redis::RedisConnectionInfo::default()
            .set_db(0)
            .set_skip_set_lib_name(),
        ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        },
        Some(TlsClientMaterial {
            root_cert: Some(TEST_CA.as_bytes().to_vec()),
            ..TlsClientMaterial::default()
        }),
        Arc::new(()),
    );

    let mut connection = client.connection().await.unwrap();
    assert_eq!(
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .unwrap(),
        "PONG"
    );
    assert_eq!(server_names.lock().unwrap().as_slice(), ["cache.internal"]);
}

#[tokio::test]
async fn tunneled_tls_accepts_an_ip_server_name_without_dns_or_panic() {
    let (port, server_names) = spawn_fake_tls_redis().await;
    let client = TunneledClient::new(
        redis::RedisConnectionInfo::default().set_skip_set_lib_name(),
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        },
        Some(TlsClientMaterial {
            root_cert: Some(TEST_CA.as_bytes().to_vec()),
            verify_server_cert: false,
            ..TlsClientMaterial::default()
        }),
        Arc::new(()),
    );

    let mut connection = client.connection().await.unwrap();
    assert_eq!(
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .unwrap(),
        "PONG"
    );
    assert_eq!(server_names.lock().unwrap().as_slice(), [""]);
}

#[tokio::test]
async fn tunneled_tls_pubsub_and_monitor_both_use_the_original_sni() {
    let (port, server_names) = spawn_fake_tls_redis().await;
    let client = TunneledClient::new(
        redis::RedisConnectionInfo::default()
            .set_username("operator")
            .set_password("redis-secret")
            .set_db(3)
            .set_skip_set_lib_name(),
        ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        },
        Some(TlsClientMaterial {
            root_cert: Some(TEST_CA.as_bytes().to_vec()),
            ..TlsClientMaterial::default()
        }),
        Arc::new(()),
    );

    let _pubsub = client.pubsub().await.unwrap();
    let mut monitor = client.monitor_stream().await.unwrap();
    assert_eq!(
        monitor.next().await.unwrap().unwrap(),
        "1.0 [3 local] \"PING\""
    );
    let mut names = server_names.lock().unwrap().clone();
    names.sort();
    assert_eq!(names, ["cache.internal", "cache.internal"]);
}

#[tokio::test]
async fn direct_monitor_is_normalized_to_the_common_result_stream() {
    let (port, _) = spawn_fake_monitor_server().await;
    let client = StandaloneClient::Direct(
        redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap(),
    );
    let mut monitor = client.monitor_stream().await.unwrap();
    assert_eq!(
        monitor.next().await.unwrap().unwrap(),
        "1.0 [3 local] \"PING\""
    );
}

#[tokio::test]
async fn monitor_rejects_bulk_malformed_and_oversized_lines_once_then_closes() {
    let mut oversized = Vec::with_capacity(256 * 1024 + 4);
    oversized.push(b'+');
    oversized.extend(std::iter::repeat_n(b'x', 256 * 1024 + 1));
    oversized.extend_from_slice(b"\r\n");
    for reply in [b"$4\r\nPING\r\n".to_vec(), b"+broken\n".to_vec(), oversized] {
        let port = spawn_monitor_reply_server(reply).await;
        let client = TunneledClient::new(
            redis::RedisConnectionInfo::default().set_skip_set_lib_name(),
            ConnectionEndpoint {
                host: "cache.internal".into(),
                port: 6379,
            },
            ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port,
            },
            None,
            Arc::new(()),
        );
        let mut stream = client.monitor_stream().await.unwrap();
        assert_eq!(
            stream.next().await.unwrap(),
            Err(redix_lib::error::AppError::CommandFailed)
        );
        assert!(stream.next().await.is_none());
    }
}

#[tokio::test]
async fn monitor_auth_command_matches_username_and_password_combinations() {
    assert_eq!(
        monitor_commands(
            redis::RedisConnectionInfo::default()
                .set_password("redis-secret")
                .set_skip_set_lib_name()
        )
        .await,
        vec![
            vec![b"AUTH".to_vec(), b"redis-secret".to_vec()],
            vec![b"MONITOR".to_vec()],
        ]
    );
    assert_eq!(
        monitor_commands(
            redis::RedisConnectionInfo::default()
                .set_username("operator")
                .set_skip_set_lib_name()
        )
        .await,
        vec![vec![b"MONITOR".to_vec()]]
    );
}

#[test]
fn tunneled_client_keeps_its_forward_guard_for_the_full_client_lifetime() {
    struct DropProbe(Arc<AtomicBool>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Arc::new(DropProbe(dropped.clone()));
    let client = TunneledClient::new(
        redis::RedisConnectionInfo::default(),
        ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6379,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 1,
        },
        None,
        guard.clone(),
    );
    drop(guard);
    assert!(!dropped.load(Ordering::SeqCst));
    drop(client);
    assert!(dropped.load(Ordering::SeqCst));
}
