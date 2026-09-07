use crate::{
    domain::{
        stream_entries::{parse_stream_entry_id, GetStreamEntriesInput},
        ClaimStreamPendingEntriesInput, StreamPendingEntry,
    },
    error::AppError,
};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UpdateStreamGroupIdInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub last_delivered_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetStreamPendingPageInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub consumer: Option<String>,
    pub start: String,
    pub end: String,
    pub cursor: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamPendingPage {
    pub entries: Vec<StreamPendingEntry>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ClaimStreamPendingAdvancedInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub consumer: String,
    pub min_idle_ms: u64,
    pub entries: Vec<String>,
    pub idle_ms: Option<u64>,
    pub time_ms: Option<i64>,
    pub retry_count: Option<u64>,
    #[serde(default)]
    pub force: bool,
}

fn validate_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() || name.chars().count() > 256 {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

impl UpdateStreamGroupIdInput {
    pub fn validate(&self) -> Result<(), AppError> {
        GetStreamEntriesInput {
            connection_id: self.connection_id.clone(),
            key: self.key.clone(),
            start: "-".into(),
            end: "+".into(),
            cursor: None,
            count: 1,
            reverse: false,
        }
        .validate()?;
        validate_name(&self.group)?;
        if self.last_delivered_id == "$" {
            return Ok(());
        }
        if self.last_delivered_id.contains('-') {
            parse_stream_entry_id(&self.last_delivered_id)?;
        } else if self.last_delivered_id.is_empty()
            || !self
                .last_delivered_id
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            || self.last_delivered_id.parse::<u64>().is_err()
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

impl GetStreamPendingPageInput {
    pub fn validate(&self) -> Result<(), AppError> {
        GetStreamEntriesInput {
            connection_id: self.connection_id.clone(),
            key: self.key.clone(),
            start: self.start.clone(),
            end: self.end.clone(),
            cursor: self.cursor.clone(),
            count: self.count,
            reverse: false,
        }
        .validate()?;
        validate_name(&self.group)?;
        if let Some(consumer) = &self.consumer {
            validate_name(consumer)?;
        }
        Ok(())
    }
}

impl ClaimStreamPendingAdvancedInput {
    pub fn validate(&self) -> Result<(), AppError> {
        ClaimStreamPendingEntriesInput {
            connection_id: self.connection_id.clone(),
            key: self.key.clone(),
            group: self.group.clone(),
            consumer: self.consumer.clone(),
            min_idle_ms: self.min_idle_ms,
            entries: self.entries.clone(),
        }
        .validate()?;
        if self.idle_ms.is_some_and(|value| value > MAX_SAFE_INTEGER)
            || self
                .retry_count
                .is_some_and(|value| value > MAX_SAFE_INTEGER)
            || self
                .time_ms
                .is_some_and(|value| value.unsigned_abs() > MAX_SAFE_INTEGER)
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
