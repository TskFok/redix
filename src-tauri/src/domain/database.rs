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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InstanceDetails {
    pub overview: InstanceOverview,
    pub clients: ClientDetails,
    pub memory: MemoryDetails,
    pub stats: StatsDetails,
    pub persistence: PersistenceDetails,
    pub replication: ReplicationDetails,
    pub command_stats: Vec<CommandStat>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ClientDetails {
    pub connected_clients: Option<u64>,
    pub blocked_clients: Option<u64>,
    pub tracking_clients: Option<u64>,
    pub max_clients: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MemoryDetails {
    pub used_memory_bytes: Option<u64>,
    pub used_memory_peak_bytes: Option<u64>,
    pub used_memory_rss_bytes: Option<u64>,
    pub mem_fragmentation_ratio: Option<f64>,
    pub allocator_active_bytes: Option<u64>,
    pub allocator_resident_bytes: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StatsDetails {
    pub instantaneous_ops_per_sec: Option<u64>,
    pub expired_keys: Option<u64>,
    pub evicted_keys: Option<u64>,
    pub hit_rate: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PersistenceDetails {
    pub loading: Option<bool>,
    pub rdb_last_save_time: Option<u64>,
    pub rdb_changes_since_last_save: Option<u64>,
    pub aof_enabled: Option<bool>,
    pub aof_rewrite_in_progress: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReplicationDetails {
    pub role: Option<String>,
    pub connected_replicas: Option<u64>,
    pub master_link_status: Option<String>,
    pub master_repl_offset: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandStat {
    pub command: String,
    pub calls: Option<u64>,
    pub usec: Option<u64>,
    pub usec_per_call: Option<f64>,
    pub rejected_calls: Option<u64>,
    pub failed_calls: Option<u64>,
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

impl InstanceDetails {
    pub fn from_info_and_modules(
        sections: &HashMap<String, HashMap<String, String>>,
        modules: Vec<ModuleSummary>,
    ) -> Result<Self, AppError> {
        let keyspace_hits = optional_metric(sections, "Stats", "keyspace_hits")?;
        let keyspace_misses = optional_metric(sections, "Stats", "keyspace_misses")?;
        let hit_rate = match (keyspace_hits, keyspace_misses) {
            (Some(hits), Some(misses)) => hits
                .checked_add(misses)
                .filter(|total| *total > 0)
                .map(|total| hits as f64 / total as f64),
            _ => None,
        };

        Ok(Self {
            overview: InstanceOverview::from_info_and_modules(sections, modules)?,
            clients: ClientDetails {
                connected_clients: optional_metric(sections, "Clients", "connected_clients")?,
                blocked_clients: optional_metric(sections, "Clients", "blocked_clients")?,
                tracking_clients: optional_metric(sections, "Clients", "tracking_clients")?,
                max_clients: optional_metric(sections, "Clients", "maxclients")?,
            },
            memory: MemoryDetails {
                used_memory_bytes: optional_metric(sections, "Memory", "used_memory")?,
                used_memory_peak_bytes: optional_metric(sections, "Memory", "used_memory_peak")?,
                used_memory_rss_bytes: optional_metric(sections, "Memory", "used_memory_rss")?,
                mem_fragmentation_ratio: optional_float(
                    sections,
                    "Memory",
                    "mem_fragmentation_ratio",
                )?,
                allocator_active_bytes: optional_metric(sections, "Memory", "allocator_active")?,
                allocator_resident_bytes: optional_metric(
                    sections,
                    "Memory",
                    "allocator_resident",
                )?,
            },
            stats: StatsDetails {
                instantaneous_ops_per_sec: optional_metric(
                    sections,
                    "Stats",
                    "instantaneous_ops_per_sec",
                )?,
                expired_keys: optional_metric(sections, "Stats", "expired_keys")?,
                evicted_keys: optional_metric(sections, "Stats", "evicted_keys")?,
                hit_rate,
            },
            persistence: PersistenceDetails {
                loading: optional_flag(sections, "Persistence", "loading")?,
                rdb_last_save_time: optional_metric(sections, "Persistence", "rdb_last_save_time")?,
                rdb_changes_since_last_save: optional_metric(
                    sections,
                    "Persistence",
                    "rdb_changes_since_last_save",
                )?,
                aof_enabled: optional_flag(sections, "Persistence", "aof_enabled")?,
                aof_rewrite_in_progress: optional_flag(
                    sections,
                    "Persistence",
                    "aof_rewrite_in_progress",
                )?,
            },
            replication: ReplicationDetails {
                role: optional_text(sections, "Replication", "role"),
                connected_replicas: optional_metric(sections, "Replication", "connected_replicas")?,
                master_link_status: optional_text(sections, "Replication", "master_link_status"),
                master_repl_offset: optional_metric(sections, "Replication", "master_repl_offset")?,
            },
            command_stats: parse_command_stats(sections),
        })
    }
}

pub fn parse_command_stats(
    sections: &HashMap<String, HashMap<String, String>>,
) -> Vec<CommandStat> {
    let Some(command_stats) = sections.get("Commandstats") else {
        return Vec::new();
    };

    let mut stats = command_stats
        .iter()
        .filter_map(|(key, value)| parse_command_stat(key, value))
        .collect::<Vec<_>>();
    stats.sort_by(|left, right| left.command.cmp(&right.command));
    stats
}

fn parse_command_stat(key: &str, value: &str) -> Option<CommandStat> {
    let command = key.strip_prefix("cmdstat_")?;
    if command.is_empty() || !command.is_ascii() {
        return None;
    }

    let fields = value
        .split(',')
        .map(str::trim)
        .filter_map(|item| item.split_once('='))
        .collect::<HashMap<_, _>>();
    let calls = fields.get("calls")?.trim().parse().ok()?;

    Some(CommandStat {
        command: command.to_ascii_uppercase(),
        calls: Some(calls),
        usec: optional_field_metric(&fields, "usec"),
        usec_per_call: optional_field_float(&fields, "usec_per_call"),
        rejected_calls: optional_field_metric(&fields, "rejected_calls"),
        failed_calls: optional_field_metric(&fields, "failed_calls"),
    })
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

fn optional_float(
    sections: &HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Result<Option<f64>, AppError> {
    sections
        .get(section)
        .and_then(|values| values.get(key))
        .map(|value| value.trim().parse::<f64>())
        .transpose()
        .map_err(|_| AppError::PersistenceFailed)
}

fn optional_flag(
    sections: &HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Result<Option<bool>, AppError> {
    sections
        .get(section)
        .and_then(|values| values.get(key))
        .map(|value| match value.trim() {
            "0" => Ok(Some(false)),
            "1" => Ok(Some(true)),
            _ => Err(AppError::PersistenceFailed),
        })
        .unwrap_or(Ok(None))
}

fn optional_field_metric(fields: &HashMap<&str, &str>, key: &str) -> Option<u64> {
    fields.get(key)?.trim().parse().ok()
}

fn optional_field_float(fields: &HashMap<&str, &str>, key: &str) -> Option<f64> {
    fields.get(key)?.trim().parse().ok()
}

fn parse_metric(value: &str) -> Result<u64, AppError> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| AppError::PersistenceFailed)
}
