use std::collections::HashMap;

use crate::error::AppError;

pub const ANALYSIS_MIN_KEYS: u64 = 1_000;
pub const ANALYSIS_MAX_KEYS: u64 = 1_000_000;
const TOP_ITEMS_LIMIT: usize = 15;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalyzeDatabaseInput {
    pub connection_id: String,
    pub pattern: String,
    pub delimiter: String,
    pub max_keys: u64,
}

impl AnalyzeDatabaseInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.pattern.chars().count() > 512
            || self.delimiter.chars().count() == 0
            || self.delimiter.chars().count() > 8
            || !(ANALYSIS_MIN_KEYS..=ANALYSIS_MAX_KEYS).contains(&self.max_keys)
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisProgress {
    pub scanned: u64,
    pub processed: u64,
    pub max_keys: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TypeSummary {
    pub r#type: String,
    pub total: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisSummary {
    pub total: u64,
    pub observed: u64,
    pub types: Vec<TypeSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisKeyMetadata {
    pub key: String,
    pub key_type: String,
    pub length: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub ttl_seconds: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisKey {
    pub key: String,
    pub key_type: String,
    pub length: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NamespaceSummary {
    pub namespace: String,
    pub keys: u64,
    pub memory_bytes: u64,
    pub types: Vec<TypeSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExpirationGroup {
    pub label: String,
    pub keys: u64,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DatabaseAnalysisReport {
    pub database: u8,
    pub pattern: String,
    pub delimiter: String,
    pub progress: AnalysisProgress,
    pub total_keys: AnalysisSummary,
    pub total_memory: AnalysisSummary,
    pub top_keys_by_length: Vec<AnalysisKey>,
    pub top_keys_by_memory: Vec<AnalysisKey>,
    pub top_namespaces_by_keys: Vec<NamespaceSummary>,
    pub top_namespaces_by_memory: Vec<NamespaceSummary>,
    pub expiration_groups: Vec<ExpirationGroup>,
}

#[derive(Debug, Default)]
struct NamespaceAccumulator {
    keys: u64,
    memory_bytes: u64,
    types: HashMap<String, u64>,
}

#[derive(Debug)]
pub struct AnalysisAccumulator {
    database: u8,
    pattern: String,
    delimiter: String,
    max_keys: u64,
    key_types: HashMap<String, u64>,
    memory_types: HashMap<String, u64>,
    total_keys: u64,
    total_memory: u64,
    memory_observed: u64,
    namespaces: HashMap<String, NamespaceAccumulator>,
    top_keys_by_length: Vec<AnalysisKey>,
    top_keys_by_memory: Vec<AnalysisKey>,
    expiration_groups: HashMap<&'static str, ExpirationGroup>,
}

impl AnalysisAccumulator {
    pub fn new(database: u8, pattern: String, delimiter: String, max_keys: u64) -> Self {
        let expiration_groups = expiration_labels()
            .into_iter()
            .map(|label| {
                (
                    label,
                    ExpirationGroup {
                        label: label.to_string(),
                        keys: 0,
                        memory_bytes: 0,
                    },
                )
            })
            .collect();

        Self {
            database,
            pattern,
            delimiter,
            max_keys,
            key_types: HashMap::new(),
            memory_types: HashMap::new(),
            total_keys: 0,
            total_memory: 0,
            memory_observed: 0,
            namespaces: HashMap::new(),
            top_keys_by_length: Vec::new(),
            top_keys_by_memory: Vec::new(),
            expiration_groups,
        }
    }

    pub fn process(&mut self, metadata: AnalysisKeyMetadata) {
        self.total_keys += 1;
        *self.key_types.entry(metadata.key_type.clone()).or_default() += 1;

        if let Some(memory_bytes) = metadata.memory_bytes {
            self.total_memory += memory_bytes;
            self.memory_observed += 1;
            *self
                .memory_types
                .entry(metadata.key_type.clone())
                .or_default() += memory_bytes;
        }

        if let Some(namespace) = namespace_for(&metadata.key, &self.delimiter) {
            let entry = self.namespaces.entry(namespace.to_string()).or_default();
            entry.keys += 1;
            entry.memory_bytes += metadata.memory_bytes.unwrap_or_default();
            *entry.types.entry(metadata.key_type.clone()).or_default() += 1;
        }

        let key = AnalysisKey {
            key: metadata.key,
            key_type: metadata.key_type,
            length: metadata.length,
            memory_bytes: metadata.memory_bytes,
            ttl_seconds: (metadata.ttl_seconds >= -1).then_some(metadata.ttl_seconds),
        };
        if key.length.is_some() {
            insert_top_key(&mut self.top_keys_by_length, key.clone(), |item| {
                item.length
            });
        }
        if key.memory_bytes.is_some() {
            insert_top_key(&mut self.top_keys_by_memory, key.clone(), |item| {
                item.memory_bytes
            });
        }

        if let Some(group) = expiration_group(metadata.ttl_seconds) {
            let entry = self
                .expiration_groups
                .get_mut(group)
                .expect("fixed expiration label must be initialized");
            entry.keys += 1;
            entry.memory_bytes += key.memory_bytes.unwrap_or_default();
        }
    }

    pub fn finish(self, scanned: u64, processed: u64, truncated: bool) -> DatabaseAnalysisReport {
        let mut namespaces = self
            .namespaces
            .into_iter()
            .map(|(namespace, value)| NamespaceSummary {
                namespace,
                keys: value.keys,
                memory_bytes: value.memory_bytes,
                types: summaries(value.types),
            })
            .collect::<Vec<_>>();
        let mut namespaces_by_memory = namespaces.clone();
        namespaces.sort_by(|left, right| {
            right
                .keys
                .cmp(&left.keys)
                .then_with(|| left.namespace.cmp(&right.namespace))
        });
        namespaces_by_memory.sort_by(|left, right| {
            right
                .memory_bytes
                .cmp(&left.memory_bytes)
                .then_with(|| left.namespace.cmp(&right.namespace))
        });

        let mut expiration_groups = expiration_labels()
            .into_iter()
            .filter_map(|label| self.expiration_groups.get(label).cloned())
            .collect::<Vec<_>>();
        expiration_groups.retain(|group| group.keys > 0);

        DatabaseAnalysisReport {
            database: self.database,
            pattern: self.pattern,
            delimiter: self.delimiter,
            progress: AnalysisProgress {
                scanned,
                processed,
                max_keys: self.max_keys,
                truncated,
            },
            total_keys: AnalysisSummary {
                total: self.total_keys,
                observed: self.total_keys,
                types: summaries(self.key_types),
            },
            total_memory: AnalysisSummary {
                total: self.total_memory,
                observed: self.memory_observed,
                types: summaries(self.memory_types),
            },
            top_keys_by_length: self.top_keys_by_length,
            top_keys_by_memory: self.top_keys_by_memory,
            top_namespaces_by_keys: namespaces.into_iter().take(TOP_ITEMS_LIMIT).collect(),
            top_namespaces_by_memory: namespaces_by_memory
                .into_iter()
                .take(TOP_ITEMS_LIMIT)
                .collect(),
            expiration_groups,
        }
    }
}

fn summaries(values: HashMap<String, u64>) -> Vec<TypeSummary> {
    let mut summaries = values
        .into_iter()
        .map(|(r#type, total)| TypeSummary { r#type, total })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .total
            .cmp(&left.total)
            .then_with(|| left.r#type.cmp(&right.r#type))
    });
    summaries
}

fn namespace_for<'a>(key: &'a str, delimiter: &str) -> Option<&'a str> {
    key.split_once(delimiter)
        .map(|(namespace, _)| namespace)
        .filter(|namespace| !namespace.is_empty())
}

fn insert_top_key(
    keys: &mut Vec<AnalysisKey>,
    key: AnalysisKey,
    metric: impl Fn(&AnalysisKey) -> Option<u64>,
) {
    keys.push(key);
    keys.sort_by(|left, right| {
        metric(right)
            .cmp(&metric(left))
            .then_with(|| left.key.cmp(&right.key))
    });
    keys.truncate(TOP_ITEMS_LIMIT);
}

fn expiration_labels() -> [&'static str; 7] {
    [
        "No Expiry",
        "<1 hr",
        "1-4 Hrs",
        "4-12 Hrs",
        "12-24 Hrs",
        "1-7 Days",
        ">7 Days",
    ]
}

fn expiration_group(ttl_seconds: i64) -> Option<&'static str> {
    match ttl_seconds {
        -1 => Some("No Expiry"),
        -2 => None,
        seconds if seconds < 3_600 => Some("<1 hr"),
        seconds if seconds < 14_400 => Some("1-4 Hrs"),
        seconds if seconds < 43_200 => Some("4-12 Hrs"),
        seconds if seconds < 86_400 => Some("12-24 Hrs"),
        seconds if seconds < 604_800 => Some("1-7 Days"),
        _ => Some(">7 Days"),
    }
}
