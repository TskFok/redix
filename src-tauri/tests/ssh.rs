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

fn fixture(auth_method: SshAuthMethod) -> (ConnectionProfile, ConnectionSecrets) {
    let mut profile = support::valid_profile();
    profile.host = required("REDIX_TEST_REDIS_HOST");
    profile.port = required("REDIX_TEST_REDIS_PORT").parse().unwrap();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": required("REDIX_TEST_SSH_HOST"),
            "port": required("REDIX_TEST_SSH_PORT").parse::<u16>().unwrap(),
            "username": required("REDIX_TEST_SSH_USERNAME"),
            "auth_method": auth_method,
            "has_password": matches!(auth_method, SshAuthMethod::Password),
            "has_private_key": matches!(auth_method, SshAuthMethod::PrivateKey),
            "has_identity_file": std::env::var_os("REDIX_TEST_SSH_IDENTITY_FILE").is_some(),
            "has_known_hosts_file": true
        }))
        .unwrap(),
    );
    let secrets = ConnectionSecrets {
        ssh_password: std::env::var("REDIX_TEST_SSH_PASSWORD").ok(),
        ssh_private_key: std::env::var("REDIX_TEST_SSH_PRIVATE_KEY").ok(),
        ssh_passphrase: std::env::var("REDIX_TEST_SSH_PASSPHRASE").ok(),
        ssh_identity_file: std::env::var("REDIX_TEST_SSH_IDENTITY_FILE").ok(),
        ssh_known_hosts_file: Some(required("REDIX_TEST_SSH_KNOWN_HOSTS")),
        ..Default::default()
    };
    (profile, secrets)
}

async fn assert_real_forward(auth_method: SshAuthMethod) {
    let (profile, secrets) = fixture(auth_method);
    RedisService::new(Arc::new(Profiles), Arc::new(Secrets))
        .test_connection(&profile, &secrets)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与 ssh-agent fixture"]
async fn real_sshd_agent_authentication_and_forwarding() {
    assert_real_forward(SshAuthMethod::Agent).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与密码 fixture"]
async fn real_sshd_password_authentication_and_forwarding() {
    assert_real_forward(SshAuthMethod::Password).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis、known_hosts 与私钥 fixture"]
async fn real_sshd_private_key_authentication_and_forwarding() {
    assert_real_forward(SshAuthMethod::PrivateKey).await;
}

#[tokio::test]
#[ignore = "需要外部真实 sshd fixture"]
async fn real_sshd_rejects_unknown_host_key() {
    let (profile, mut secrets) = fixture(SshAuthMethod::PrivateKey);
    let directory = tempfile::tempdir().unwrap();
    let empty_known_hosts: PathBuf = directory.path().join("known_hosts");
    std::fs::write(&empty_known_hosts, "").unwrap();
    secrets.ssh_known_hosts_file = Some(empty_known_hosts.to_string_lossy().into_owned());

    let error = RedisService::new(Arc::new(Profiles), Arc::new(Secrets))
        .test_connection(&profile, &secrets)
        .await
        .unwrap_err();
    assert_eq!(error, AppError::SshTunnelFailed);
}
