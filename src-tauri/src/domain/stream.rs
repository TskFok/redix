use crate::error::AppError;

const MAX_STREAM_NAME_LENGTH: usize = 256;
const MAX_PENDING_COUNT: u32 = 500;
const MAX_ACK_ENTRIES: usize = 500;

fn validate_connection_and_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() || key.trim().is_empty() {
        return Err(AppError::InvalidConnection);
    }
    Ok(())
}

fn validate_stream_name(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > MAX_STREAM_NAME_LENGTH {
        return Err(AppError::InvalidConnection);
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetStreamConsumerGroupsInput {
    pub connection_id: String,
    pub key: String,
}

impl GetStreamConsumerGroupsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamConsumerGroup {
    pub name: String,
    pub consumers: u64,
    pub pending: u64,
    pub last_delivered_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CreateStreamConsumerGroupInput {
    pub connection_id: String,
    pub key: String,
    pub name: String,
    pub last_delivered_id: String,
}

impl CreateStreamConsumerGroupInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.name)?;
        if self.last_delivered_id.trim().is_empty()
            || self.last_delivered_id.chars().count() > MAX_STREAM_NAME_LENGTH
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteStreamConsumerGroupInput {
    pub connection_id: String,
    pub key: String,
    pub name: String,
}

impl DeleteStreamConsumerGroupInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.name)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetStreamConsumersInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
}

impl GetStreamConsumersInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.group)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamConsumer {
    pub name: String,
    pub pending: u64,
    pub idle_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetStreamPendingEntriesInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub count: u32,
    pub consumer: Option<String>,
}

impl GetStreamPendingEntriesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.group)?;
        if !(1..=MAX_PENDING_COUNT).contains(&self.count) {
            return Err(AppError::InvalidConnection);
        }
        if self
            .consumer
            .as_deref()
            .is_some_and(|consumer| validate_stream_name(consumer).is_err())
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamPendingEntry {
    pub id: String,
    pub consumer: String,
    pub idle_ms: u64,
    pub deliveries: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AcknowledgeStreamPendingEntriesInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub entries: Vec<String>,
}

impl AcknowledgeStreamPendingEntriesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.group)?;
        if self.entries.is_empty()
            || self.entries.len() > MAX_ACK_ENTRIES
            || self.entries.iter().any(|entry| {
                entry.trim().is_empty() || entry.chars().count() > MAX_STREAM_NAME_LENGTH
            })
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteStreamConsumerInput {
    pub connection_id: String,
    pub key: String,
    pub group: String,
    pub consumer: String,
}

impl DeleteStreamConsumerInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_stream_name(&self.group)?;
        validate_stream_name(&self.consumer)
    }
}
