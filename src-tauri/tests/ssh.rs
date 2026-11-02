use std::{path::PathBuf, sync::Arc};

use redix_lib::{
    domain::{ConnectionProfile, SshAuthMethod},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore, SshHostKeyStore},
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
    let mut secrets = ConnectionSecrets::default();
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
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    std::fs::copy(required("REDIX_TEST_SSH_KNOWN_HOSTS"), &path).unwrap();
    RedisService::with_ssh_host_keys(
        Arc::new(Profiles),
        Arc::new(Secrets),
        Arc::new(SshHostKeyStore::new(path)),
    )
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
async fn real_sshd_requires_app_confirmation_even_with_legacy_known_hosts() {
    let mut profile = ssh_profile(FixtureAuth::Password);
    profile.host = "127.0.0.1".into();
    profile.port = 1;
    let directory = tempfile::tempdir().unwrap();
    let path: PathBuf = directory.path().join("known_hosts");
    let store = Arc::new(SshHostKeyStore::new(path.clone()));
    let secrets = ConnectionSecrets {
        // Existing per-connection trust files must never bypass app confirmation.
        ssh_known_hosts_file: Some(required("REDIX_TEST_SSH_KNOWN_HOSTS")),
        // No password: host verification must stop before authentication.
        ..Default::default()
    };
    let service =
        RedisService::with_ssh_host_keys(Arc::new(Profiles), Arc::new(Secrets), store.clone());
    let error = service
        .test_connection(&profile, &secrets)
        .await
        .unwrap_err();
    let AppError::SshHostKeyUnknown(id) = error else {
        panic!("expected a host key challenge before authentication, got {error:?}");
    };
    let challenge = store.get_challenge(id).unwrap();
    let ssh = profile.ssh.as_ref().unwrap();
    assert_eq!(challenge.host, ssh.host);
    assert_eq!(challenge.port, ssh.port);
    assert!(challenge.fingerprint.starts_with("SHA256:"));
    assert!(!path.exists(), "an unconfirmed key must not be persisted");
    store.discard(id).unwrap();
    assert!(!path.exists());
}

#[tokio::test]
#[ignore = "需要外部真实 sshd、Redis 与私钥文件 fixture"]
async fn real_sshd_confirmed_host_connects_and_persists_across_service_restart() {
    let (profile, secrets) = fixture(FixtureAuth::PrivateKeyFile);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = Arc::new(SshHostKeyStore::new(path.clone()));
    let service =
        RedisService::with_ssh_host_keys(Arc::new(Profiles), Arc::new(Secrets), store.clone());
    let error = service
        .test_connection(&profile, &secrets)
        .await
        .unwrap_err();
    let AppError::SshHostKeyUnknown(id) = error else {
        panic!("expected a host key challenge, got {error:?}");
    };
    store.trust(id).unwrap();
    service.test_connection(&profile, &secrets).await.unwrap();
    let restored = RedisService::with_ssh_host_keys(
        Arc::new(Profiles),
        Arc::new(Secrets),
        Arc::new(SshHostKeyStore::new(path)),
    );
    restored.test_connection(&profile, &secrets).await.unwrap();
}

#[tokio::test]
#[ignore = "需要外部真实 sshd fixture"]
async fn real_sshd_changed_host_is_rejected_before_authentication() {
    let profile = ssh_profile(FixtureAuth::Password);
    let ssh = profile.ssh.as_ref().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(SshHostKeyStore::new(directory.path().join("known_hosts")));
    let mut wrong_key = Vec::new();
    wrong_key.extend_from_slice(&11_u32.to_be_bytes());
    wrong_key.extend_from_slice(b"ssh-ed25519");
    wrong_key.extend_from_slice(&32_u32.to_be_bytes());
    wrong_key.extend_from_slice(&[7_u8; 32]);
    let error = store
        .verify(&ssh.host, ssh.port, &wrong_key, ssh2::HostKeyType::Ed25519)
        .unwrap_err();
    let AppError::SshHostKeyUnknown(id) = error else {
        panic!("expected initial challenge, got {error:?}");
    };
    store.trust(id).unwrap();
    let service = RedisService::with_ssh_host_keys(Arc::new(Profiles), Arc::new(Secrets), store);
    // No authentication material is supplied, so Changed proves verification runs first.
    assert_eq!(
        service
            .test_connection(&profile, &ConnectionSecrets::default())
            .await
            .unwrap_err(),
        AppError::SshHostKeyChanged
    );
}
