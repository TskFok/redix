use std::collections::HashSet;

use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SlowLogEntry {
    pub id: u64,
    pub time: i64,
    pub duration_us: u64,
    pub args: Vec<String>,
    pub source: String,
    pub client: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SlowLogConfig {
    pub slowlog_max_len: u64,
    pub slowlog_log_slower_than: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetSlowLogsInput {
    pub connection_id: String,
    pub count: i64,
}

impl GetSlowLogsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.count == 0
            || self.count < -1
            || self.count > 1_000
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UpdateSlowLogConfigInput {
    pub connection_id: String,
    pub slowlog_max_len: Option<u64>,
    pub slowlog_log_slower_than: Option<i64>,
}

impl UpdateSlowLogConfigInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || (self.slowlog_max_len.is_none() && self.slowlog_log_slower_than.is_none())
            || self.slowlog_log_slower_than.is_some_and(|value| value < -1)
            || self
                .slowlog_max_len
                .is_some_and(|value| value > i64::MAX as u64)
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct PubSubTopic {
    pub name: String,
    pub pattern: bool,
}

impl PubSubTopic {
    pub fn normalized(&self) -> Self {
        Self {
            name: self.name.trim().to_owned(),
            pattern: self.pattern,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StartPubSubInput {
    pub connection_id: String,
    pub session_id: String,
    pub topics: Vec<PubSubTopic>,
}

impl StartPubSubInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.session_id.trim().is_empty()
            || self.topics.is_empty()
            || self.topics.len() > 20
        {
            return Err(AppError::InvalidInput);
        }

        let mut unique = HashSet::with_capacity(self.topics.len());
        for topic in &self.topics {
            let normalized = topic.normalized();
            if normalized.name.is_empty()
                || normalized.name.len() > 256
                || !unique.insert(normalized)
            {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(())
    }

    pub fn normalized_topics(&self) -> Vec<PubSubTopic> {
        self.topics.iter().map(PubSubTopic::normalized).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PubSubSession {
    pub connection_id: String,
    pub session_id: String,
    pub topics: Vec<PubSubTopic>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StopPubSubInput {
    pub connection_id: String,
    pub session_id: String,
}

impl StopPubSubInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() || self.session_id.trim().is_empty() {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PublishPubSubInput {
    pub connection_id: String,
    pub channel: String,
    pub message: String,
}

impl PublishPubSubInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.channel.trim().is_empty()
            || self.channel.trim().len() > 256
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PubSubMessageEvent {
    pub connection_id: String,
    pub session_id: String,
    pub channel: String,
    pub pattern: Option<String>,
    pub message: String,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PubSubStatusEvent {
    pub connection_id: String,
    pub session_id: String,
    pub state: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StartProfilerInput {
    pub connection_id: String,
    pub session_id: String,
}

impl StartProfilerInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() || self.session_id.trim().is_empty() {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StopProfilerInput {
    pub connection_id: String,
    pub session_id: String,
}

impl StopProfilerInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() || self.session_id.trim().is_empty() {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProfilerSession {
    pub connection_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MonitorEntry {
    pub time: String,
    pub database: u8,
    pub source: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProfilerEvent {
    pub connection_id: String,
    pub session_id: String,
    pub time: String,
    pub database: u8,
    pub source: String,
    pub args: Vec<String>,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProfilerStatusEvent {
    pub connection_id: String,
    pub session_id: String,
    pub state: String,
    pub error_code: Option<String>,
}
