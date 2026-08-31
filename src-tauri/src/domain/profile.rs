use crate::error::AppError;

fn default_verify_server_cert() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SentinelConfig {
    pub master_name: String,
    pub nodes: Vec<ConnectionEndpoint>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub tls: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentinel: Option<SentinelConfig>,
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub database: u8,
    pub has_password: bool,
    #[serde(default)]
    pub tls: bool,
    #[serde(default = "default_verify_server_cert")]
    pub verify_server_cert: bool,
    #[serde(default)]
    pub ca_certificate_name: Option<String>,
    #[serde(default)]
    pub client_certificate_name: Option<String>,
    #[serde(default)]
    pub has_ca_certificate: bool,
    #[serde(default)]
    pub has_client_certificate: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub identity_file: Option<String>,
    #[serde(default)]
    pub known_hosts_file: Option<String>,
}

impl SshConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        let safe_host = self.host.parse::<std::net::Ipv6Addr>().is_ok()
            || (!self.host.is_empty()
                && !self.host.starts_with('-')
                && self.host.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                }));
        if !safe_host
            || self.port == 0
            || self.username.is_empty()
            || !self
                .username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            || self
                .identity_file
                .iter()
                .chain(self.known_hosts_file.iter())
                .any(|path| {
                    !std::path::Path::new(path).is_absolute() || path.contains(['\n', '\r', '\0'])
                })
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

impl ConnectionProfile {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.host.trim().is_empty()
            || self.port == 0
            || self.database > 15
        {
            return Err(AppError::InvalidConnection);
        }

        if let Some(sentinel) = &self.sentinel {
            if sentinel.master_name.trim().is_empty()
                || sentinel.nodes.is_empty()
                || sentinel.nodes.len() > 32
                || sentinel
                    .nodes
                    .iter()
                    .any(|node| node.host.trim().is_empty() || node.port == 0)
            {
                return Err(AppError::InvalidConnection);
            }
        }

        if let Some(ssh) = &self.ssh {
            ssh.validate()?;
            if self.tls || self.sentinel.is_some() {
                return Err(AppError::InvalidConnection);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveConnectionInput {
    pub profile: ConnectionProfile,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub sentinel_password: Option<String>,
    #[serde(default)]
    pub ca_certificate: Option<String>,
    #[serde(default)]
    pub client_certificate: Option<String>,
    #[serde(default)]
    pub client_key: Option<String>,
    #[serde(default)]
    pub clear_ca_certificate: bool,
    #[serde(default)]
    pub clear_client_certificate: bool,
}

pub type TestConnectionInput = SaveConnectionInput;

pub fn validate_certificate_pem(value: &str) -> Result<(), AppError> {
    validate_pem_block(value, "CERTIFICATE", "CERTIFICATE")
}

pub fn validate_private_key_pem(value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    let markers = [
        ("PRIVATE KEY", "PRIVATE KEY"),
        ("RSA PRIVATE KEY", "RSA PRIVATE KEY"),
        ("EC PRIVATE KEY", "EC PRIVATE KEY"),
    ];

    if trimmed.is_empty()
        || !markers.iter().any(|(begin, end)| {
            trimmed.contains(&format!("-----BEGIN {begin}-----"))
                && trimmed.contains(&format!("-----END {end}-----"))
        })
    {
        return Err(AppError::InvalidInput);
    }

    Ok(())
}

fn validate_pem_block(value: &str, begin: &str, end: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed.contains(&format!("-----BEGIN {begin}-----"))
        || !trimmed.contains(&format!("-----END {end}-----"))
    {
        return Err(AppError::InvalidInput);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_profile_defaults_to_plain_redis_and_verified_tls() {
        let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({
            "id": "local",
            "name": "Local",
            "host": "127.0.0.1",
            "port": 6379,
            "username": null,
            "database": 0,
            "has_password": false
        }))
        .unwrap();

        assert!(!profile.tls);
        assert!(profile.verify_server_cert);
        assert!(!profile.has_ca_certificate);
        assert!(!profile.has_client_certificate);
    }

    #[test]
    fn pem_validation_requires_certificate_or_private_key_markers() {
        assert!(
            validate_certificate_pem("-----BEGIN CERTIFICATE-----x-----END CERTIFICATE-----")
                .is_ok()
        );
        assert!(validate_private_key_pem(
            "-----BEGIN RSA PRIVATE KEY-----x-----END RSA PRIVATE KEY-----"
        )
        .is_ok());
        assert!(validate_certificate_pem("not-pem").is_err());
    }
}
