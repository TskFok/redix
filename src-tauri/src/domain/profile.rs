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
pub struct ClusterConfig {
    pub nodes: Vec<ConnectionEndpoint>,
    #[serde(default)]
    pub read_from_replicas: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentinel: Option<SentinelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterConfig>,
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

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    #[default]
    Agent,
    Password,
    PrivateKey,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth_method: SshAuthMethod,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub has_private_key: bool,
    #[serde(default)]
    pub has_passphrase: bool,
    #[serde(default)]
    pub has_identity_file: bool,
    #[serde(default)]
    pub has_known_hosts_file: bool,
    #[serde(default, rename = "identity_file", skip_serializing)]
    pub(crate) legacy_identity_file: Option<String>,
    #[serde(default, rename = "known_hosts_file", skip_serializing)]
    pub(crate) legacy_known_hosts_file: Option<String>,
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
                .legacy_identity_file
                .iter()
                .chain(self.legacy_known_hosts_file.iter())
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

        if self.sentinel.is_some() && self.cluster.is_some() {
            return Err(AppError::InvalidConnection);
        }

        if let Some(sentinel) = &self.sentinel {
            validate_topology_nodes(&sentinel.nodes)?;
            if sentinel.master_name.trim().is_empty() {
                return Err(AppError::InvalidConnection);
            }
        }

        if let Some(cluster) = &self.cluster {
            validate_topology_nodes(&cluster.nodes)?;
            let seed = cluster.nodes.first().ok_or(AppError::InvalidConnection)?;
            if self.database != 0 || self.host != seed.host || self.port != seed.port {
                return Err(AppError::InvalidConnection);
            }
        }

        if let Some(ssh) = &self.ssh {
            ssh.validate()?;
        }

        Ok(())
    }
}

fn validate_topology_nodes(nodes: &[ConnectionEndpoint]) -> Result<(), AppError> {
    if nodes.is_empty() || nodes.len() > 32 {
        return Err(AppError::InvalidConnection);
    }

    let mut seen = std::collections::HashSet::with_capacity(nodes.len());
    for node in nodes {
        if !is_safe_endpoint_host(&node.host)
            || node.port == 0
            || !seen.insert((&node.host, node.port))
        {
            return Err(AppError::InvalidConnection);
        }
    }
    Ok(())
}

fn is_safe_endpoint_host(host: &str) -> bool {
    host.parse::<std::net::Ipv6Addr>().is_ok()
        || (!host.is_empty()
            && !host.starts_with('-')
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveConnectionInput {
    pub profile: ConnectionProfile,
    #[serde(default)]
    pub ssh_password: Option<String>,
    #[serde(default)]
    pub ssh_private_key: Option<String>,
    #[serde(default)]
    pub ssh_passphrase: Option<String>,
    #[serde(default)]
    pub ssh_identity_file: Option<String>,
    #[serde(default)]
    pub ssh_known_hosts_file: Option<String>,
    #[serde(default)]
    pub clear_ssh_secrets: bool,
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
