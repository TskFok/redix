use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{ClusterConfig, ConnectionEndpoint, ConnectionProfile, SentinelConfig, SshConfig};
use crate::error::AppError;

pub const CONNECTION_EXPORT_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionExportDocument {
    pub version: u32,
    pub connections: Vec<ConnectionExportProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionExportProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshExportConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentinel: Option<SentinelExportConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterConfig>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub database: u8,
    pub tls: bool,
    pub verify_server_cert: bool,
    pub ca_certificate_name: Option<String>,
    pub client_certificate_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshExportConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth_method: super::SshAuthMethod,
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
}

impl From<SshExportConfig> for SshConfig {
    fn from(ssh: SshExportConfig) -> Self {
        Self {
            host: ssh.host,
            port: ssh.port,
            username: ssh.username,
            auth_method: ssh.auth_method,
            has_password: ssh.has_password,
            has_private_key: ssh.has_private_key,
            has_passphrase: ssh.has_passphrase,
            has_identity_file: ssh.has_identity_file,
            has_known_hosts_file: ssh.has_known_hosts_file,
            legacy_identity_file: None,
            legacy_known_hosts_file: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentinelExportConfig {
    pub master_name: String,
    pub nodes: Vec<ConnectionEndpoint>,
    pub username: Option<String>,
    pub tls: bool,
}

impl ConnectionExportDocument {
    pub fn from_profiles(profiles: &[ConnectionProfile]) -> Self {
        Self {
            version: CONNECTION_EXPORT_VERSION,
            connections: profiles
                .iter()
                .map(|profile| ConnectionExportProfile {
                    ssh: profile.ssh.as_ref().map(|ssh| SshExportConfig {
                        host: ssh.host.clone(),
                        port: ssh.port,
                        username: ssh.username.clone(),
                        auth_method: ssh.auth_method,
                        has_password: ssh.has_password,
                        has_private_key: ssh.has_private_key,
                        has_passphrase: ssh.has_passphrase,
                        has_identity_file: ssh.has_identity_file
                            || ssh.legacy_identity_file.is_some(),
                        has_known_hosts_file: ssh.has_known_hosts_file
                            || ssh.legacy_known_hosts_file.is_some(),
                    }),
                    sentinel: profile
                        .sentinel
                        .as_ref()
                        .map(|sentinel| SentinelExportConfig {
                            master_name: sentinel.master_name.clone(),
                            nodes: sentinel.nodes.clone(),
                            username: sentinel.username.clone(),
                            tls: sentinel.tls,
                        }),
                    cluster: profile.cluster.clone(),
                    name: profile.name.clone(),
                    host: profile.host.clone(),
                    port: profile.port,
                    username: profile.username.clone(),
                    database: profile.database,
                    tls: profile.tls,
                    verify_server_cert: profile.verify_server_cert,
                    ca_certificate_name: profile.ca_certificate_name.clone(),
                    client_certificate_name: profile.client_certificate_name.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConnectionsInput {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionImportFailure {
    pub index: usize,
    pub name: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConnectionsResult {
    pub imported: Vec<ConnectionProfile>,
    pub failed: Vec<ConnectionImportFailure>,
    pub ignored_secret_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedImportDocument {
    pub entries: Vec<NormalizedImportEntry>,
    pub ignored_secret_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedImportEntry {
    pub ssh: Option<SshConfig>,
    pub sentinel: Option<SentinelConfig>,
    pub cluster: Option<ClusterConfig>,
    pub source_index: usize,
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub database: Option<u8>,
    pub tls: bool,
    pub verify_server_cert: bool,
    pub ca_certificate_name: Option<String>,
    pub client_certificate_name: Option<String>,
    pub unsupported_type: Option<String>,
    pub sensitive_fields: usize,
}

pub fn normalize_import_document(raw: &str) -> Result<NormalizedImportDocument, AppError> {
    let value = serde_json::from_str::<Value>(raw).map_err(|_| AppError::InvalidInput)?;
    let values = top_level_entries(value)?;
    let mut ignored_secret_fields = 0;
    let entries = values
        .into_iter()
        .enumerate()
        .map(|(source_index, value)| {
            let entry = normalize_entry(source_index, value);
            ignored_secret_fields += entry.sensitive_fields;
            entry
        })
        .collect();

    Ok(NormalizedImportDocument {
        entries,
        ignored_secret_fields,
    })
}

fn top_level_entries(value: Value) -> Result<Vec<Value>, AppError> {
    match value {
        Value::Array(values) => Ok(values),
        Value::Object(object) => {
            if let Some(version) = object.get("version") {
                let version = parse_u32(version).ok_or(AppError::InvalidInput)?;
                if !matches!(version, 1 | CONNECTION_EXPORT_VERSION) {
                    return Err(AppError::InvalidInput);
                }
            }

            object
                .get("connections")
                .or_else(|| object.get("profiles"))
                .and_then(Value::as_array)
                .cloned()
                .ok_or(AppError::InvalidInput)
        }
        _ => Err(AppError::InvalidInput),
    }
}

fn normalize_entry(source_index: usize, value: Value) -> NormalizedImportEntry {
    let object = value.as_object();
    let name = object
        .and_then(|object| first_string(object, &["name", "connectionName"]))
        .unwrap_or_default();
    let host = object
        .and_then(|object| first_string(object, &["host", "hostname"]))
        .unwrap_or_default();
    let port = object.and_then(|object| first_u16(object, &["port"]));
    let username = object.and_then(|object| first_string(object, &["username", "authUser"]));
    let database = object.and_then(|object| first_u8(object, &["database", "db"]));
    let tls = object
        .and_then(|object| first_bool(object, &["tls", "ssl"]))
        .unwrap_or(false);
    let verify_server_cert = object
        .and_then(|object| first_bool(object, &["verify_server_cert", "verifyServerCert"]))
        .unwrap_or(true);
    let ca_certificate = object.and_then(|object| {
        first_value(
            object,
            &["ca_certificate", "caCert", "caCertificate", "caCertId"],
        )
    });
    let client_certificate = object.and_then(|object| {
        first_value(
            object,
            &["client_certificate", "clientCert", "clientCertificate"],
        )
    });
    let sensitive_fields = object.map(count_sensitive_fields).unwrap_or(0);

    NormalizedImportEntry {
        ssh: object
            .and_then(|object| object.get("ssh"))
            .and_then(|value| serde_json::from_value::<SshExportConfig>(value.clone()).ok())
            .map(SshConfig::from),
        sentinel: object
            .and_then(|object| object.get("sentinel"))
            .and_then(|value| serde_json::from_value::<SentinelConfig>(value.clone()).ok())
            .map(|mut sentinel| {
                sentinel.has_password = false;
                sentinel
            }),
        cluster: object
            .and_then(|object| object.get("cluster"))
            .and_then(|value| serde_json::from_value::<ClusterConfig>(value.clone()).ok()),
        source_index,
        name,
        host,
        port,
        username,
        database,
        tls,
        verify_server_cert,
        ca_certificate_name: ca_certificate.and_then(certificate_name),
        client_certificate_name: client_certificate.and_then(certificate_name),
        unsupported_type: object.and_then(unsupported_connection_type),
        sensitive_fields,
    }
}

fn first_value<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn first_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    first_value(object, keys)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn first_bool(object: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    first_value(object, keys).and_then(parse_bool)
}

fn first_u16(object: &Map<String, Value>, keys: &[&str]) -> Option<u16> {
    first_value(object, keys).and_then(parse_u16)
}

fn first_u8(object: &Map<String, Value>, keys: &[&str]) -> Option<u8> {
    first_value(object, keys).and_then(parse_u8)
}

fn parse_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value
            .as_str()
            .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            })
    })
}

fn parse_u16(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
}

fn parse_u8(value: &Value) -> Option<u8> {
    value
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
}

fn parse_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
}

fn certificate_name(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.contains("-----BEGIN") => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Value::Object(object) => first_string(object, &["name", "certificateName"]),
        _ => None,
    }
}

fn contains_pem(value: &Value) -> bool {
    value
        .as_str()
        .map(|value| value.contains("-----BEGIN "))
        .unwrap_or(false)
        || value.as_object().is_some_and(|object| {
            ["value", "body", "pem", "certificate", "key", "privateKey"]
                .iter()
                .any(|key| object.get(*key).is_some_and(contains_pem))
        })
}

fn count_sensitive_fields(object: &Map<String, Value>) -> usize {
    let mut count = 0;
    for key in [
        "password",
        "sentinel_password",
        "auth",
        "authPassword",
        "clientKey",
        "client_key",
    ] {
        if object.get(key).is_some_and(has_non_empty_value) {
            count += 1;
        }
    }
    if let Some(sentinel) = object.get("sentinel").and_then(Value::as_object) {
        count += count_sensitive_fields(sentinel);
    }
    if let Some(ssh) = object.get("ssh").and_then(Value::as_object) {
        count += count_sensitive_fields(ssh);
        count += ["privateKey", "private_key", "passphrase"]
            .iter()
            .filter(|key| ssh.get(**key).is_some_and(has_non_empty_value))
            .count();
    }
    for key in [
        "ca_certificate",
        "caCert",
        "caCertificate",
        "client_certificate",
        "clientCert",
        "clientCertificate",
    ] {
        if object.get(key).is_some_and(contains_pem) {
            count += 1;
        }
    }
    if let Some(ssh_options) = object.get("sshOptions").and_then(Value::as_object) {
        for key in ["password", "passphrase", "privateKey"] {
            if ssh_options.get(key).is_some_and(has_non_empty_value) {
                count += 1;
            }
        }
    }
    count
}

fn has_non_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        _ => true,
    }
}

fn unsupported_connection_type(object: &Map<String, Value>) -> Option<String> {
    if let Some(value) = object.get("ssh").filter(|value| !value.is_null()) {
        if serde_json::from_value::<SshExportConfig>(value.clone()).is_err() {
            return Some("ssh".to_owned());
        }
    }
    if let Some(value) = object.get("sentinel").filter(|value| !value.is_null()) {
        if serde_json::from_value::<SentinelConfig>(value.clone()).is_err() {
            return Some("sentinel".to_owned());
        }
    }
    if let Some(value) = object.get("cluster").filter(|value| !value.is_null()) {
        if serde_json::from_value::<ClusterConfig>(value.clone()).is_err() {
            return Some("cluster".to_owned());
        }
    }
    if let Some(connection_type) = first_string(object, &["connectionType", "type"]) {
        let normalized = connection_type.to_ascii_lowercase();
        if !matches!(normalized.as_str(), "standalone" | "single" | "tcp") {
            return Some(connection_type);
        }
    }

    for key in [
        "nodes",
        "sentinelMaster",
        "sentinelOptions",
        "sshOptions",
        "cloudDetails",
    ] {
        if object.contains_key(key) {
            return Some(key.to_owned());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::super::ConnectionProfile;

    #[test]
    fn export_document_contains_only_portable_fields() {
        let profile = profile_with_tls_and_secret_flags();
        let document = super::ConnectionExportDocument::from_profiles(&[profile]);
        let value = serde_json::to_value(document).unwrap();
        let raw = value.to_string();

        assert!(!raw.contains("password"));
        assert!(!raw.contains("BEGIN CERTIFICATE"));
        assert!(!raw.contains("BEGIN PRIVATE KEY"));
        assert!(value["connections"][0]["tls"].as_bool().unwrap());
    }

    #[test]
    fn normalizes_redisinsight_aliases_and_counts_ignored_sensitive_fields() {
        let raw = serde_json::json!([{
            "connectionName": "TLS Redis",
            "host": "redis.example",
            "port": "6380",
            "db": "2",
            "authUser": "default",
            "password": "secret",
            "ssl": true,
            "verifyServerCert": false,
            "caCert": {
                "name": "Root CA",
                "value": "-----BEGIN CERTIFICATE-----secret"
            }
        }])
        .to_string();
        let normalized = super::normalize_import_document(&raw).unwrap();

        assert_eq!(normalized.entries[0].name, "TLS Redis");
        assert_eq!(normalized.entries[0].database, Some(2));
        assert!(normalized.entries[0].tls);
        assert!(!normalized.entries[0].verify_server_cert);
        assert_eq!(
            normalized.entries[0].ca_certificate_name.as_deref(),
            Some("Root CA")
        );
        assert_eq!(normalized.ignored_secret_fields, 2);
    }

    #[test]
    fn rejects_non_standalone_connection_types_without_downgrading_them() {
        let raw = serde_json::json!({
            "connections": [{
                "name": "cluster",
                "host": "redis.example",
                "port": 6379,
                "connectionType": "CLUSTER",
                "nodes": []
            }]
        })
        .to_string();
        let normalized = super::normalize_import_document(&raw).unwrap();

        assert_eq!(
            normalized.entries[0].unsupported_type.as_deref(),
            Some("CLUSTER")
        );
    }

    fn profile_with_tls_and_secret_flags() -> ConnectionProfile {
        ConnectionProfile {
            ssh: None,
            sentinel: None,
            cluster: None,
            id: "secret-id".into(),
            name: "TLS Redis".into(),
            host: "redis.example".into(),
            port: 6380,
            username: Some("default".into()),
            database: 2,
            has_password: true,
            tls: true,
            verify_server_cert: true,
            ca_certificate_name: Some("Root CA".into()),
            client_certificate_name: Some("Client cert".into()),
            has_ca_certificate: true,
            has_client_certificate: true,
        }
    }
}
