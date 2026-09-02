use std::{path::PathBuf, sync::Arc};

use redix_lib::{
    domain::{ConnectionProfile, SshAuthMethod},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};

mod support;

struct Profiles;

impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(Vec::new())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct Secrets;

impl SecretStore for Secrets {
    fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(None)
    }

    fn write(&self, _connection_id: &str, _secrets: &ConnectionSecrets) -> Result<(), AppError> {
        Ok(())
    }

    fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
        Ok(())
    }
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("真实 sshd fixture 不可用：缺少环境变量 {name}"))
}

#[derive(Clone, Copy)]
enum FixtureAuth {
    Agent,
    Password,
    PrivateKeyMemory,
    PrivateKeyFile,
}

fn ssh_profile(auth: FixtureAuth) -> ConnectionProfile {
    let mut profile = support::valid_profile();
    let auth_method = match auth {
        FixtureAuth::Agent => SshAuthMethod::Agent,
        FixtureAuth::Password => SshAuthMethod::Password,
        FixtureAuth::PrivateKeyMemory | FixtureAuth::PrivateKeyFile => SshAuthMethod::PrivateKey,
    };
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": required("REDIX_TEST_SSH_HOST"),
            "port": required("REDIX_TEST_SSH_PORT").parse::<u16>().unwrap(),
            "username": required("REDIX_TEST_SSH_USERNAME"),
            "auth_method": auth_method,
            "has_password": matches!(auth, FixtureAuth::Password),
            "has_private_key": matches!(auth, FixtureAuth::PrivateKeyMemory),
            "has_identity_file": matches!(auth, FixtureAuth::PrivateKeyFile),
            "has_known_hosts_file": true
        }))
        .unwrap(),
    );
    profile
}

fn fixture(auth: FixtureAuth) -> (ConnectionProfile, ConnectionSecrets) {
    let mut profile = ssh_profile(auth);
    profile.host = required("REDIX_TEST_REDIS_HOST");
    profile.port = required("REDIX_TEST_REDIS_PORT").parse().unwrap();
    let mut secrets = ConnectionSecrets {
        ssh_known_hosts_file: Some(required("REDIX_TEST_SSH_KNOWN_HOSTS")),
        ..Default::default()
    };
    match auth {
        FixtureAuth::Agent => {}
        FixtureAuth::Password => {
            secrets.ssh_password = Some(required("REDIX_TEST_SSH_PASSWORD"));
        }
        FixtureAuth::PrivateKeyMemory => {
            secrets.ssh_private_key = Some(required("REDIX_TEST_SSH_PRIVATE_KEY"));
            secrets.ssh_passphrase = std::env::var("REDIX_TEST_SSH_PASSPHRASE").ok();
        }
        FixtureAuth::PrivateKeyFile => {
            secrets.ssh_identity_file = Some(required("REDIX_TEST_SSH_IDENTITY_FILE"));
            secrets.ssh_passphrase = std::env::var("REDIX_TEST_SSH_PASSPHRASE").ok();
        }
    }
    (profile, secrets)
}

async fn assert_real_forward(auth: FixtureAuth) {
    let (profile, secrets) = fixture(auth);
    RedisService::new(Arc::new(Profiles), Arc::new(Secrets))
        .test_connection(&profile, &secrets)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与 ssh-agent fixture"]
async fn real_sshd_agent_authentication_and_forwarding() {
    assert_real_forward(FixtureAuth::Agent).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与密码 fixture"]
async fn real_sshd_password_authentication_and_forwarding() {
    assert_real_forward(FixtureAuth::Password).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与内存私钥 fixture"]
async fn real_sshd_memory_private_key_authentication_and_forwarding() {
    assert_real_forward(FixtureAuth::PrivateKeyMemory).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与私钥文件 fixture"]
async fn real_sshd_identity_file_authentication_and_forwarding() {
    assert_real_forward(FixtureAuth::PrivateKeyFile).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd fixture"]
async fn real_sshd_rejects_unknown_host_key() {
    let mut profile = ssh_profile(FixtureAuth::Agent);
    profile.host = "127.0.0.1".into();
    profile.port = 1;
    let directory = tempfile::tempdir().unwrap();
    let empty_known_hosts: PathBuf = directory.path().join("known_hosts");
    std::fs::write(&empty_known_hosts, "").unwrap();
    let secrets = ConnectionSecrets {
        ssh_known_hosts_file: Some(empty_known_hosts.to_string_lossy().into_owned()),
        ..Default::default()
    };

    let error = RedisService::new(Arc::new(Profiles), Arc::new(Secrets))
        .test_connection(&profile, &secrets)
        .await
        .unwrap_err();
    assert_eq!(error, AppError::SshTunnelFailed);
}
