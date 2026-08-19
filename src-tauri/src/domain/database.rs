use std::collections::HashMap;

use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct InstanceOverview {
    pub server_version: Option<String>,
    pub redis_mode: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub connected_clients: Option<u64>,
    pub used_memory_bytes: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    pub total_commands_processed: Option<u64>,
    pub keyspace_hits: Option<u64>,
    pub keyspace_misses: Option<u64>,
    pub role: Option<String>,
    pub modules: Vec<ModuleSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ModuleSummary {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DatabaseOverview {
    pub database: u8,
    pub key_count: Option<u64>,
    pub expires: Option<u64>,
    pub avg_ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SelectDatabaseInput {
    pub connection_id: String,
    pub database: u8,
}

impl SelectDatabaseInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() || self.database > 15 {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

pub fn parse_info_sections(raw: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::new();
    let mut current_section = String::new();

    for raw_line in raw.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(section) = line.strip_prefix('#') {
            current_section = section.trim().to_string();
            if !current_section.is_empty() {
                sections
                    .entry(current_section.clone())
                    .or_insert_with(HashMap::new);
            }
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        sections
            .entry(current_section.clone())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value.trim().to_string());
    }

    sections
}

pub fn parse_keyspace_line(database: &str, line: &str) -> Result<DatabaseOverview, AppError> {
    let database = database.trim();
    let digits = database
        .strip_prefix("db")
        .filter(|digits| {
            !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit())
        })
        .ok_or(AppError::PersistenceFailed)?;
    let database = digits
        .parse::<u8>()
        .map_err(|_| AppError::PersistenceFailed)?;
    if database > 15 {
        return Err(AppError::PersistenceFailed);
    }

    let mut key_count = None;
    let mut expires = None;
    let mut avg_ttl_ms = None;

    for item in line.split(',') {
        let item = item.trim();
        if item.is_empty() {
            return Err(AppError::PersistenceFailed);
        }
        let Some((key, value)) = item.split_once('=') else {
            return Err(AppError::PersistenceFailed);
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "keys" => key_count = Some(parse_metric(value)?),
            "expires" => expires = Some(parse_metric(value)?),
            "avg_ttl" => avg_ttl_ms = Some(parse_metric(value)?),
            _ => {}
        }
    }

    let key_count = key_count.ok_or(AppError::PersistenceFailed)?;
    Ok(DatabaseOverview {
        database,
        key_count: Some(key_count),
        expires,
        avg_ttl_ms,
    })
}

impl InstanceOverview {
    pub fn from_info_and_modules(
        sections: &HashMap<String, HashMap<String, String>>,
        modules: Vec<ModuleSummary>,
    ) -> Result<Self, AppError> {
        let max_memory_bytes = optional_metric(sections, "Memory", "maxmemory")?;

        Ok(Self {
            server_version: optional_text(sections, "Server", "redis_version"),
            redis_mode: optional_text(sections, "Server", "redis_mode"),
            uptime_seconds: optional_metric(sections, "Server", "uptime_in_seconds")?,
            connected_clients: optional_metric(sections, "Clients", "connected_clients")?,
            used_memory_bytes: optional_metric(sections, "Memory", "used_memory")?,
            max_memory_bytes: max_memory_bytes.filter(|value| *value > 0),
            total_commands_processed: optional_metric(
                sections,
                "Stats",
                "total_commands_processed",
            )?,
            keyspace_hits: optional_metric(sections, "Stats", "keyspace_hits")?,
            keyspace_misses: optional_metric(sections, "Stats", "keyspace_misses")?,
            role: optional_text(sections, "Replication", "role"),
            modules,
        })
    }
}

fn optional_text(
    sections: &HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Option<String> {
    sections
        .get(section)
        .and_then(|values| values.get(key))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_metric(
    sections: &HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Result<Option<u64>, AppError> {
    sections
        .get(section)
        .and_then(|values| values.get(key))
        .map(|value| parse_metric(value).map(Some))
        .unwrap_or(Ok(None))
}

fn parse_metric(value: &str) -> Result<u64, AppError> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| AppError::PersistenceFailed)
}
