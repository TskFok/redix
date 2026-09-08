use std::sync::Arc;

use redix_lib::{
    domain::ConnectionProfile,
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
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

#[tokio::test]
async fn test_and_open_connection_return_refusal_diagnostics_with_stable_error_contract() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({
        "id": "local", "name": "Local", "host": "127.0.0.1", "port": port,
        "database": 0, "has_password": false, "tls": false, "verify_server_cert": true
    }))
    .unwrap();
    let service = RedisService::new(Arc::new(Profiles(profile.clone())), Arc::new(Secrets));
    let testing = service
        .test_connection(&profile, &ConnectionSecrets::default())
        .await
        .unwrap_err();
    let opening = service.open_connection("local").await.unwrap_err();

    for error in [testing, opening] {
        let result = serde_json::to_value(error).unwrap();
        assert_eq!(result["code"], "CONNECTION_FAILED");
        assert_eq!(result["message"], "无法连接到 Redis 服务器");
        assert!(
            result["diagnostics"]
                .as_str()
                .expect("missing refusal diagnostics")
                .contains("连接被拒绝"),
            "{result}"
        );
    }
}
