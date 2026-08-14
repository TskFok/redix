use std::{collections::HashMap, sync::Arc};

use ::redis::Client;
use tokio::sync::RwLock;

use crate::{
    domain::{
        CommandResult, ConnectionInfo, ConnectionProfile, KeyValue, ScanPage, SetKeyInput,
        SetKeyTtlInput,
    },
    error::AppError,
    persistence::{ProfileRepository, SecretStore},
};

#[allow(async_fn_in_trait)]
pub trait RedisOperations: Send + Sync {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        password: Option<&str>,
    ) -> Result<ConnectionInfo, AppError>;
    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError>;
    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError>;
    async fn scan_keys(&self, input: crate::domain::ScanKeysInput) -> Result<ScanPage, AppError>;
    async fn get_key(&self, connection_id: &str, key: &str) -> Result<KeyValue, AppError>;
    async fn set_key(&self, input: SetKeyInput) -> Result<KeyValue, AppError>;
    async fn delete_key(&self, connection_id: &str, key: &str) -> Result<(), AppError>;
    async fn set_key_ttl(&self, input: SetKeyTtlInput) -> Result<i64, AppError>;
    async fn execute_command(
        &self,
        connection_id: &str,
        input: &str,
    ) -> Result<CommandResult, AppError>;
}

pub struct RedisService {
    profiles: Arc<dyn ProfileRepository>,
    secrets: Arc<dyn SecretStore>,
    active: Arc<RwLock<HashMap<String, Client>>>,
}

impl RedisService {
    pub fn new(profiles: Arc<dyn ProfileRepository>, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            profiles,
            secrets,
            active: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn inspect_client(client: &Client) -> Result<ConnectionInfo, AppError> {
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(map_connection_error)?;
        let pong: String = ::redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .map_err(map_connection_error)?;
        if pong != "PONG" {
            return Err(AppError::ConnectionFailed);
        }

        let server_info: String = ::redis::cmd("INFO")
            .arg("server")
            .query_async(&mut connection)
            .await
            .map_err(map_command_error)?;
        let server_version = server_info
            .lines()
            .find_map(|line| line.strip_prefix("redis_version:"))
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .unwrap_or("unknown")
            .to_owned();

        Ok(ConnectionInfo { server_version })
    }
}

impl RedisOperations for RedisService {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        password: Option<&str>,
    ) -> Result<ConnectionInfo, AppError> {
        profile.validate()?;
        let client = Client::open(connection_url(profile, password)?)
            .map_err(|_| AppError::InvalidConnection)?;
        Self::inspect_client(&client).await
    }

    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError> {
        let profile = self
            .profiles
            .load()?
            .into_iter()
            .find(|profile| profile.id == connection_id)
            .ok_or(AppError::InvalidConnection)?;
        profile.validate()?;
        let password = if profile.has_password {
            self.secrets.read(connection_id)?
        } else {
            None
        };
        let client = Client::open(connection_url(&profile, password.as_deref())?)
            .map_err(|_| AppError::InvalidConnection)?;
        let info = Self::inspect_client(&client).await?;
        self.active
            .write()
            .await
            .insert(connection_id.to_owned(), client);
        Ok(info)
    }

    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError> {
        self.active.write().await.remove(connection_id);
        Ok(())
    }

    async fn scan_keys(&self, _input: crate::domain::ScanKeysInput) -> Result<ScanPage, AppError> {
        Err(AppError::CommandFailed)
    }

    async fn get_key(&self, _connection_id: &str, _key: &str) -> Result<KeyValue, AppError> {
        Err(AppError::CommandFailed)
    }

    async fn set_key(&self, _input: SetKeyInput) -> Result<KeyValue, AppError> {
        Err(AppError::CommandFailed)
    }

    async fn delete_key(&self, _connection_id: &str, _key: &str) -> Result<(), AppError> {
        Err(AppError::CommandFailed)
    }

    async fn set_key_ttl(&self, _input: SetKeyTtlInput) -> Result<i64, AppError> {
        Err(AppError::CommandFailed)
    }

    async fn execute_command(
        &self,
        _connection_id: &str,
        _input: &str,
    ) -> Result<CommandResult, AppError> {
        Err(AppError::CommandFailed)
    }
}

pub fn connection_url(
    profile: &ConnectionProfile,
    password: Option<&str>,
) -> Result<String, AppError> {
    profile.validate()?;
    let host = standalone_host(&profile.host)?;
    let credentials = match (profile.username.as_deref(), password) {
        (Some(username), Some(password)) => {
            format!("{}:{}@", percent_encode(username), percent_encode(password))
        }
        (Some(username), None) => format!("{}@", percent_encode(username)),
        (None, Some(password)) => format!(":{}@", percent_encode(password)),
        (None, None) => String::new(),
    };
    Ok(format!(
        "redis://{credentials}{host}:{}/{}",
        profile.port, profile.database
    ))
}

pub fn validate_ttl(ttl_ms: i64) -> Result<(), AppError> {
    if ttl_ms < 0 {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

fn standalone_host(host: &str) -> Result<String, AppError> {
    let host = host.trim();
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(format!("[{host}]"));
    }
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    {
        return Err(AppError::InvalidConnection);
    }
    Ok(host.to_owned())
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn map_connection_error(error: ::redis::RedisError) -> AppError {
    if error.kind() == ::redis::ErrorKind::AuthenticationFailed {
        AppError::AuthenticationFailed
    } else {
        AppError::ConnectionFailed
    }
}

fn map_command_error(error: ::redis::RedisError) -> AppError {
    match error.kind() {
        ::redis::ErrorKind::AuthenticationFailed => AppError::AuthenticationFailed,
        ::redis::ErrorKind::Io => AppError::ConnectionFailed,
        _ => AppError::CommandFailed,
    }
}

#[cfg(test)]
mod tests {
    use crate::{domain::ConnectionProfile, error::AppError};

    use super::{connection_url, validate_ttl};

    fn valid_profile() -> ConnectionProfile {
        ConnectionProfile {
            id: "local".into(),
            name: "Local".into(),
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            database: 0,
            has_password: false,
        }
    }

    #[test]
    fn builds_a_standalone_url_with_encoded_credentials_and_database() {
        let mut profile = valid_profile();
        profile.username = Some("user name".into());
        profile.database = 3;

        let url = connection_url(&profile, Some("p@ss word")).unwrap();

        assert_eq!(url, "redis://user%20name:p%40ss%20word@127.0.0.1:6379/3");
    }

    #[test]
    fn rejects_negative_ttl_without_exposing_the_value() {
        let error = validate_ttl(-1).unwrap_err();

        assert_eq!(error, AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }
}
