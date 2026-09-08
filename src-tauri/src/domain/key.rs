use crate::error::AppError;
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum ScanCursor {
    Standalone(u64),
    Cluster(String),
}

impl Default for ScanCursor {
    fn default() -> Self {
        Self::Standalone(0)
    }
}

impl From<u64> for ScanCursor {
    fn from(cursor: u64) -> Self {
        Self::Standalone(cursor)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanKeysInput {
    pub connection_id: String,
    pub cursor: ScanCursor,
    pub pattern: String,
    pub count: usize,
    pub key_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanAllKeysInput {
    pub connection_id: String,
    pub pattern: String,
    pub count: usize,
    pub key_type: Option<String>,
}

impl From<ScanAllKeysInput> for ScanKeysInput {
    fn from(input: ScanAllKeysInput) -> Self {
        Self {
            connection_id: input.connection_id,
            cursor: ScanCursor::default(),
            pattern: input.pattern,
            count: input.count,
            key_type: input.key_type,
        }
    }
}

impl ScanKeysInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.pattern.trim().is_empty()
            || !(1..=10_000).contains(&self.count)
            || self
                .key_type
                .as_deref()
                .is_some_and(|key_type| normalize_key_type(key_type).is_none())
        {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

pub fn normalize_key_type(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "string" => Some("string"),
        "hash" => Some("hash"),
        "list" => Some("list"),
        "set" => Some("set"),
        "zset" | "sortedset" | "sorted-set" => Some("zset"),
        "stream" => Some("stream"),
        "json" | "rejson-rl" | "rejson-rs" => Some("json"),
        "array" => Some("array"),
        "vectorset" | "vector-set" => Some("vector-set"),
        _ => None,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeySummary {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub size: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub encoding: Option<String>,
    pub idle_seconds: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScanPage {
    pub cursor: ScanCursor,
    pub keys: Vec<KeySummary>,
    pub has_more: bool,
    #[serde(default)]
    pub node_failures: Vec<crate::domain::NodeFailure>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExportKeysInput {
    pub connection_id: String,
    pub keys: Vec<String>,
}

impl ExportKeysInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.keys.is_empty()
            || self.keys.len() > 1_000
            || self.keys.iter().any(|key| key.trim().is_empty())
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExportedKey {
    pub key: String,
    pub ttl_ms: i64,
    pub value: RedisValue,
}

impl ExportedKey {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.key.trim().is_empty() || self.ttl_ms < -1 {
            return Err(AppError::InvalidConnection);
        }
        self.value.validate()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ImportKeysInput {
    pub connection_id: String,
    pub entries: Vec<ExportedKey>,
}

impl ImportKeysInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.entries.is_empty()
            || self.entries.len() > 1_000
        {
            return Err(AppError::InvalidConnection);
        }
        for entry in &self.entries {
            entry.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub server_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_endpoint: Option<super::ConnectionEndpoint>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetKeyInput {
    pub connection_id: String,
    pub key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetKeyInput {
    pub connection_id: String,
    pub key: String,
    pub value: RedisValue,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateKeyInput {
    pub connection_id: String,
    pub key: String,
    pub value: RedisValue,
    pub ttl_ms: Option<i64>,
}

impl CreateKeyInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.ttl_ms.is_some_and(|ttl| ttl < 0) {
            return Err(AppError::InvalidConnection);
        }
        self.value.validate()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameKeyInput {
    pub connection_id: String,
    pub key: String,
    pub new_key: String,
}

impl RenameKeyInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.new_key.trim().is_empty() {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteKeysInput {
    pub connection_id: String,
    pub keys: Vec<String>,
}

impl DeleteKeysInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.keys.is_empty()
            || self.keys.iter().any(|key| key.trim().is_empty())
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyInfoInput {
    pub connection_id: String,
    pub key: String,
}

impl KeyInfoInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteKeyInput {
    pub connection_id: String,
    pub key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetKeyTtlInput {
    pub connection_id: String,
    pub key: String,
    pub ttl_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecuteCommandInput {
    pub connection_id: String,
    pub command: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum RedisValue {
    String {
        value: String,
    },
    Hash {
        fields: Vec<HashEntry>,
    },
    List {
        items: Vec<String>,
    },
    Set {
        members: Vec<String>,
    },
    SortedSet {
        members: Vec<SortedSetEntry>,
    },
    Json {
        value: serde_json::Value,
    },
    Stream {
        entries: Vec<StreamEntry>,
    },
    Array {
        length: String,
        count: String,
    },
    VectorSet {
        total: String,
        dimension: Option<u32>,
        quantization: Option<String>,
    },
}

impl RedisValue {
    pub fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::String { .. } | Self::Json { .. } => Ok(()),
            Self::Array { .. } | Self::VectorSet { .. } => Err(AppError::UnsupportedFeature),
            Self::Hash { fields } => {
                let mut names = HashSet::with_capacity(fields.len());
                if fields.is_empty()
                    || fields.iter().any(|entry| entry.field.trim().is_empty())
                    || fields
                        .iter()
                        .any(|entry| !names.insert(entry.field.as_str()))
                {
                    return Err(AppError::CommandFailed);
                }
                Ok(())
            }
            Self::List { items } => {
                if items.is_empty() {
                    Err(AppError::CommandFailed)
                } else {
                    Ok(())
                }
            }
            Self::Set { members } => {
                if members.is_empty() {
                    Err(AppError::CommandFailed)
                } else {
                    Ok(())
                }
            }
            Self::SortedSet { members } => {
                if members.is_empty()
                    || members
                        .iter()
                        .any(|entry| entry.member.trim().is_empty() || !entry.score.is_finite())
                {
                    Err(AppError::CommandFailed)
                } else {
                    Ok(())
                }
            }
            Self::Stream { entries } => {
                if entries.is_empty()
                    || entries.iter().any(|entry| {
                        entry.id.trim().is_empty()
                            || entry.fields.is_empty()
                            || entry
                                .fields
                                .iter()
                                .any(|field| field.field.trim().is_empty())
                            || {
                                let mut names = HashSet::with_capacity(entry.fields.len());
                                entry
                                    .fields
                                    .iter()
                                    .any(|field| !names.insert(field.field.as_str()))
                            }
                    })
                {
                    Err(AppError::CommandFailed)
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct HashEntry {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SortedSetEntry {
    pub member: String,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamEntry {
    pub id: String,
    pub fields: Vec<StreamField>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamField {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct KeyInfo {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub size: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub encoding: Option<String>,
    pub idle_seconds: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeyValue {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub value: RedisValue,
}

fn validate_connection_and_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() || key.trim().is_empty() {
        Err(AppError::InvalidConnection)
    } else {
        Ok(())
    }
}
