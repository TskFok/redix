mod support;

use redix_lib::{
    domain::{
        ClusterConfig, ConnectionEndpoint, ConnectionExportDocument, ConnectionProfile,
        SelectDatabaseInput,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};
use std::{
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

fn sentinel_profile(port: u16) -> ConnectionProfile {
    let mut value = serde_json::to_value(support::valid_profile()).unwrap();
    value["host"] = serde_json::json!("invalid.example");
    value["has_password"] = serde_json::json!(true);
    value["sentinel"] = serde_json::json!({"master_name":"primary", "nodes":[{"host":"127.0.0.1","port":1},{"host":"127.0.0.1","port":port}], "username":null, "has_password":true,"tls":false});
    serde_json::from_value(value).unwrap()
}

#[test]
fn sentinel_topology_round_trips_without_password_flags_in_export() {
    let profile = sentinel_profile(26379);
    let value = serde_json::to_value(&profile).unwrap();
    assert_eq!(value["sentinel"]["master_name"], "primary");
    let exported =
        serde_json::to_value(ConnectionExportDocument::from_profiles(&[profile])).unwrap();
    assert_eq!(
        exported["connections"][0]["sentinel"]["nodes"][1]["port"],
        26379
    );
    assert!(!exported.to_string().contains("password"));
}

#[test]
fn export_v2_preserves_cluster_but_import_still_accepts_v1() {
    let mut profile = support::valid_profile();
    profile.host = "127.0.0.1".into();
    profile.port = 7000;
    profile.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000,
        }],
        read_from_replicas: true,
    });

    let exported = ConnectionExportDocument::from_profiles(&[profile]);
    assert_eq!(exported.version, 2);
    assert!(exported.connections[0].cluster.is_some());

    let v1 = serde_json::json!({
        "version": 1,
        "connections": [{
            "name": "legacy",
            "host": "127.0.0.1",
            "port": 6379,
            "username": null,
            "database": 0,
            "tls": false,
            "verify_server_cert": true,
            "ca_certificate_name": null,
            "client_certificate_name": null
        }]
    });
    assert!(redix_lib::domain::normalize_import_document(&v1.to_string()).is_ok());
}

#[test]
fn invalid_sentinel_configuration_cannot_silently_downgrade_to_standalone() {
    for sentinel in [
        serde_json::json!({"master_name":"", "nodes":[{"host":"127.0.0.1","port":26379}]}),
        serde_json::json!({"master_name":"primary", "nodes":[]}),
    ] {
        let mut value = serde_json::to_value(support::valid_profile()).unwrap();
        value["sentinel"] = sentinel;
        let profile: ConnectionProfile = serde_json::from_value(value).unwrap();
        assert_eq!(profile.validate(), Err(AppError::InvalidConnection));
    }
}

#[test]
fn sentinel_export_import_preserves_topology_and_ignores_embedded_secrets() {
    let export = ConnectionExportDocument::from_profiles(&[sentinel_profile(26379)]);
    let mut value = serde_json::to_value(export).unwrap();
    value["connections"][0]["sentinel"]["password"] = serde_json::json!("must-not-import");
    value["connections"][0]["sentinel"]["has_password"] = serde_json::json!(true);
    let normalized = redix_lib::domain::normalize_import_document(&value.to_string()).unwrap();
    let sentinel = normalized.entries[0].sentinel.as_ref().unwrap();
    assert_eq!(sentinel.master_name, "primary");
    assert!(!sentinel.has_password);
    assert_eq!(normalized.ignored_secret_fields, 1);
    assert!(normalized.entries[0].unsupported_type.is_none());
    value["connections"][0]["sentinel"] = serde_json::json!({"broken":true});
    let malformed = redix_lib::domain::normalize_import_document(&value.to_string()).unwrap();
    assert_eq!(
        malformed.entries[0].unsupported_type.as_deref(),
        Some("sentinel")
    );
}

#[test]
fn ssh_configuration_round_trips_and_rejects_option_injection_and_unsupported_combinations() {
    let mut value = serde_json::to_value(support::valid_profile()).unwrap();
    value["ssh"] = serde_json::json!({"host":"bastion.example", "port":22, "username":"operator", "identity_file":"/tmp/redix-id"});
    let profile: ConnectionProfile = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&profile).unwrap()["ssh"]["host"],
        "bastion.example"
    );
    assert!(profile.validate().is_ok());
    let export = serde_json::to_value(ConnectionExportDocument::from_profiles(&[profile])).unwrap();
    assert!(export["connections"][0]["ssh"]["has_identity_file"]
        .as_bool()
        .unwrap());
    assert!(export.to_string().contains("/tmp/redix-id") == false);
    let mut imported = export;
    imported["connections"][0]["ssh"]["private_key"] =
        serde_json::json!("private-key-must-not-import");
    let normalized = redix_lib::domain::normalize_import_document(&imported.to_string()).unwrap();
    assert_eq!(normalized.ignored_secret_fields, 1);
    assert_eq!(
        normalized.entries[0].ssh.as_ref().unwrap().host,
        "bastion.example"
    );
    value["ssh"]["host"] = serde_json::json!("-oProxyCommand=touch /tmp/pwn");
    assert!(serde_json::from_value::<ConnectionProfile>(value.clone())
        .unwrap()
        .validate()
        .is_err());
    value["ssh"]["host"] = serde_json::json!("bastion.example");
    value["tls"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ConnectionProfile>(value.clone())
        .unwrap()
        .validate()
        .is_ok());
    value["tls"] = serde_json::json!(false);
    value["sentinel"] = serde_json::to_value(sentinel_profile(26379)).unwrap()["sentinel"].clone();
    assert!(serde_json::from_value::<ConnectionProfile>(value)
        .unwrap()
        .validate()
        .is_ok());
}

#[tokio::test]
async fn unsupported_topology_and_routed_ssh_combinations_fail_before_network_io() {
    let mut profiles = Vec::new();
    let mut cluster = support::valid_profile();
    cluster.host = "invalid.example".into();
    cluster.port = 7000;
    cluster.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint {
            host: "invalid.example".into(),
            port: 7000,
        }],
        read_from_replicas: false,
    });
    profiles.push(cluster);

    let mut sentinel_ssh = sentinel_profile(26379);
    sentinel_ssh.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator"
        }))
        .unwrap(),
    );
    profiles.push(sentinel_ssh);

    let mut tls_ssh = support::valid_profile();
    tls_ssh.tls = true;
    tls_ssh.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator"
        }))
        .unwrap(),
    );
    profiles.push(tls_ssh);

    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(profiles.clone()))),
        Arc::new(Secrets(ConnectionSecrets::default())),
    );
    for profile in profiles {
        assert_eq!(
            service
                .test_connection(&profile, &ConnectionSecrets::default())
                .await
                .unwrap_err(),
            AppError::UnsupportedFeature
        );
    }
}

#[tokio::test]
async fn ssh_password_authentication_without_selected_secret_fails_closed() {
    let mut profile = support::valid_profile();
    profile.host = "invalid.example".into();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator",
            "auth_method": "password", "has_password": true
        }))
        .unwrap(),
    );
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![profile.clone()]))),
        Arc::new(Secrets(ConnectionSecrets {
            ssh_password: Some("ssh-secret".into()),
            ..Default::default()
        })),
    );

    assert_eq!(
        service
            .test_connection(&profile, &ConnectionSecrets::default())
            .await
            .unwrap_err(),
        AppError::SshTunnelFailed
    );
}

struct Profiles(Mutex<Vec<ConnectionProfile>>);
impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        *self.0.lock().unwrap() = profiles.to_vec();
        Ok(())
    }
}
struct Secrets(ConnectionSecrets);
impl SecretStore for Secrets {
    fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(Some(self.0.clone()))
    }
    fn write(&self, _: &str, _: &ConnectionSecrets) -> Result<(), AppError> {
        unreachable!()
    }
    fn delete(&self, _: &str) -> Result<(), AppError> {
        unreachable!()
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
    async fn start(sentinel_master: Option<u16>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let directory = tempfile::tempdir().unwrap();
        let mut config = format!(
            "bind 127.0.0.1\nport {port}\nsave \"\"\nappendonly no\ndir {}\n",
            directory.path().display()
        );
        if let Some(master) = sentinel_master {
            config.push_str(&format!("requirepass sentinel-secret\nsentinel monitor primary 127.0.0.1 {master} 1\nsentinel auth-pass primary redis-secret\n"));
        } else {
            config.push_str("requirepass redis-secret\n");
        }
        let path = directory.path().join("redis.conf");
        std::fs::write(&path, config).unwrap();
        let mut command = Command::new("redis-server");
        command.arg(path);
        if sentinel_master.is_some() {
            command.arg("--sentinel");
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("redis-server must be available");
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
        panic!("temporary Redis failed to start");
    }
    async fn connection(&self, sentinel: bool) -> redis::aio::MultiplexedConnection {
        let password = if sentinel {
            "sentinel-secret"
        } else {
            "redis-secret"
        };
        redis::Client::open(format!("redis://:{password}@127.0.0.1:{}", self.port))
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap()
    }
}

#[tokio::test]
#[ignore = "需要本地 redis-server，使用 cargo test --test sentinel -- --ignored"]
async fn sentinel_discovers_authenticated_master_falls_back_and_rediscovers_on_reconnect() {
    let first = Server::start(None).await;
    let second = Server::start(None).await;
    let sentinel = Server::start(Some(first.port)).await;
    let profile = sentinel_profile(sentinel.port);
    let secrets: ConnectionSecrets = serde_json::from_value(
        serde_json::json!({"password":"redis-secret","sentinel_password":"sentinel-secret"}),
    )
    .unwrap();
    let profiles = Arc::new(Profiles(Mutex::new(vec![profile.clone()])));
    let service = RedisService::new(profiles.clone(), Arc::new(Secrets(secrets.clone())));
    let info = service.test_connection(&profile, &secrets).await.unwrap();
    assert_eq!(
        serde_json::to_value(info).unwrap()["resolved_endpoint"]["port"],
        first.port
    );
    service.open_connection("local").await.unwrap();
    let selected = service
        .select_database(SelectDatabaseInput {
            connection_id: "local".into(),
            database: 2,
        })
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&selected).unwrap()["sentinel"]["master_name"],
        "primary"
    );
    service
        .execute_command("local", "SET redix:sentinel first")
        .await
        .unwrap();
    let mut first_connection = first.connection(false).await;
    redis::cmd("SELECT")
        .arg(2)
        .query_async::<String>(&mut first_connection)
        .await
        .unwrap();
    assert_eq!(
        redis::cmd("GET")
            .arg("redix:sentinel")
            .query_async::<String>(&mut first_connection)
            .await
            .unwrap(),
        "first"
    );
    let mut control = sentinel.connection(true).await;
    redis::cmd("SENTINEL")
        .arg("REMOVE")
        .arg("primary")
        .query_async::<String>(&mut control)
        .await
        .unwrap();
    redis::cmd("SENTINEL")
        .arg("MONITOR")
        .arg("primary")
        .arg("127.0.0.1")
        .arg(second.port)
        .arg(1)
        .query_async::<String>(&mut control)
        .await
        .unwrap();
    let reopened = service.open_connection("local").await.unwrap();
    assert_eq!(
        serde_json::to_value(reopened).unwrap()["resolved_endpoint"]["port"],
        second.port
    );
    service.close_connection("local").await.unwrap();
    assert_eq!(
        service.execute_command("local", "PING").await.unwrap_err(),
        AppError::ConnectionFailed
    );
    let mut second_connection = second.connection(false).await;
    redis::cmd("REPLICAOF")
        .arg("127.0.0.1")
        .arg(first.port)
        .query_async::<String>(&mut second_connection)
        .await
        .unwrap();
    assert_eq!(
        service.open_connection("local").await.unwrap_err(),
        AppError::ConnectionFailed
    );
    let wrong: ConnectionSecrets = serde_json::from_value(
        serde_json::json!({"password":"redis-secret","sentinel_password":"wrong"}),
    )
    .unwrap();
    assert_eq!(
        service.test_connection(&profile, &wrong).await.unwrap_err(),
        AppError::AuthenticationFailed
    );
}

#[tokio::test]
#[ignore = "需要本地 redis-server 和 ssh"]
async fn failed_ssh_tunnel_must_not_connect_directly_to_redis() {
    let redis = Server::start(None).await;
    let mut value = serde_json::to_value(support::valid_profile()).unwrap();
    value["port"] = serde_json::json!(redis.port);
    value["ssh"] = serde_json::json!({"host":"127.0.0.1", "port":1, "username":"operator"});
    let profile: ConnectionProfile = serde_json::from_value(value).unwrap();
    let secrets = ConnectionSecrets {
        password: Some("redis-secret".into()),
        ..Default::default()
    };
    let service = RedisService::new(
        Arc::new(Profiles(Mutex::new(vec![profile.clone()]))),
        Arc::new(Secrets(secrets.clone())),
    );
    assert_eq!(
        service
            .test_connection(&profile, &secrets)
            .await
            .unwrap_err()
            .code(),
        "SSH_TUNNEL_FAILED"
    );
}
