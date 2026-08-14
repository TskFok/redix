use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanKeysInput {
    pub connection_id: String,
    pub cursor: u64,
    pub pattern: String,
    pub count: usize,
}

impl ScanKeysInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(1..=500).contains(&self.count) {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeySummary {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScanPage {
    pub cursor: u64,
    pub keys: Vec<KeySummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub server_version: String,
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
    String { value: String },
    Hash { fields: Vec<HashEntry> },
    List { items: Vec<String> },
    Set { members: Vec<String> },
    SortedSet { members: Vec<SortedSetEntry> },
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeyValue {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub value: RedisValue,
}
