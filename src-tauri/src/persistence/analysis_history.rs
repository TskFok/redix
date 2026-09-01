use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{JsonDocumentStore, VersionedJsonDocument};
use crate::{
    domain::{AnalyzeDatabaseInput, DatabaseAnalysisReport},
    error::AppError,
};
use serde::{Deserialize, Serialize};

pub const MAX_HISTORY_PER_DATABASE: usize = 20;
const MAX_HISTORY_ITEMS: usize = 50;
const MAX_REPORT_BYTES: usize = 256 * 1024;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveAnalysisInput {
    pub connection_id: String,
    pub report: DatabaseAnalysisReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedAnalysis {
    pub id: String,
    pub connection_id: String,
    pub saved_at: u64,
    pub report: DatabaseAnalysisReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisHistorySummary {
    pub id: String,
    pub saved_at: u64,
    pub database: u8,
    pub pattern: String,
    pub total_keys: u64,
    pub total_memory: u64,
    pub truncated: bool,
}

impl SavedAnalysis {
    fn summary(&self) -> AnalysisHistorySummary {
        AnalysisHistorySummary {
            id: self.id.clone(),
            saved_at: self.saved_at,
            database: self.report.database,
            pattern: self.report.pattern.clone(),
            total_keys: self.report.total_keys.total,
            total_memory: self.report.total_memory.total,
            truncated: self.report.progress.truncated,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct HistoryDocument {
    version: u32,
    items: Vec<SavedAnalysis>,
}

impl Default for HistoryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            items: vec![],
        }
    }
}

impl VersionedJsonDocument for HistoryDocument {
    fn version(&self) -> u32 {
        self.version
    }
    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let document: Self =
            serde_json::from_value(value).map_err(|_| AppError::PersistenceFailed)?;
        if document.version != 1 || document.items.len() > MAX_HISTORY_ITEMS {
            return Err(AppError::PersistenceFailed);
        }
        let mut ids = std::collections::HashSet::new();
        let mut counts = std::collections::HashMap::new();
        for item in &document.items {
            validate_report(&item.connection_id, &item.report)
                .map_err(|_| AppError::PersistenceFailed)?;
            if uuid::Uuid::parse_str(&item.id).is_err() || !ids.insert(&item.id) {
                return Err(AppError::PersistenceFailed);
            }
            let count = counts
                .entry((&item.connection_id, item.report.database))
                .or_insert(0);
            *count += 1;
            if *count > MAX_HISTORY_PER_DATABASE {
                return Err(AppError::PersistenceFailed);
            }
        }
        Ok(document)
    }
}

/// One instance per application; its lock covers each complete read/modify/write transaction.
pub struct AnalysisHistoryStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl AnalysisHistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<HistoryDocument, AppError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HistoryDocument::default())
            }
            Err(_) => return Err(AppError::PersistenceFailed),
        };
        let mut bytes = Vec::new();
        file.take((MAX_DOCUMENT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| AppError::PersistenceFailed)?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(AppError::PersistenceFailed);
        }
        HistoryDocument::migrate(
            serde_json::from_slice(&bytes).map_err(|_| AppError::PersistenceFailed)?,
        )
    }

    pub fn list(
        &self,
        connection_id: &str,
        database: u8,
    ) -> Result<Vec<AnalysisHistorySummary>, AppError> {
        validate_connection(connection_id)?;
        let _guard = self.lock.lock().map_err(|_| AppError::PersistenceFailed)?;
        let mut items: Vec<_> = self
            .load()?
            .items
            .into_iter()
            .filter(|item| item.connection_id == connection_id && item.report.database == database)
            .map(|item| item.summary())
            .collect();
        items.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.id.cmp(&b.id)));
        Ok(items)
    }

    pub fn save(&self, input: SaveAnalysisInput) -> Result<AnalysisHistorySummary, AppError> {
        validate_report(&input.connection_id, &input.report)?;
        let _guard = self.lock.lock().map_err(|_| AppError::PersistenceFailed)?;
        let mut document = self.load()?;
        if document.items.len() >= MAX_HISTORY_ITEMS
            || document
                .items
                .iter()
                .filter(|item| {
                    item.connection_id == input.connection_id
                        && item.report.database == input.report.database
                })
                .count()
                >= MAX_HISTORY_PER_DATABASE
        {
            return Err(AppError::InvalidInput);
        }
        let item = SavedAnalysis {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: input.connection_id,
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            report: input.report,
        };
        let summary = item.summary();
        document.items.push(item);
        // Match the pretty representation used by JsonDocumentStore, including indentation.
        if serde_json::to_vec_pretty(&document)
            .map_err(|_| AppError::PersistenceFailed)?
            .len()
            > MAX_DOCUMENT_BYTES
        {
            return Err(AppError::InvalidInput);
        }
        JsonDocumentStore::new(self.path.clone()).save(&document)?;
        Ok(summary)
    }

    pub fn get(
        &self,
        connection_id: &str,
        database: u8,
        id: &str,
    ) -> Result<SavedAnalysis, AppError> {
        validate_connection(connection_id)?;
        let _guard = self.lock.lock().map_err(|_| AppError::PersistenceFailed)?;
        self.load()?
            .items
            .into_iter()
            .find(|item| {
                item.connection_id == connection_id
                    && item.report.database == database
                    && item.id == id
            })
            .ok_or(AppError::InvalidInput)
    }

    pub fn delete(&self, connection_id: &str, database: u8, id: &str) -> Result<(), AppError> {
        validate_connection(connection_id)?;
        let _guard = self.lock.lock().map_err(|_| AppError::PersistenceFailed)?;
        let mut document = self.load()?;
        let index = document
            .items
            .iter()
            .position(|item| {
                item.connection_id == connection_id
                    && item.report.database == database
                    && item.id == id
            })
            .ok_or(AppError::InvalidInput)?;
        document.items.remove(index);
        JsonDocumentStore::new(self.path.clone()).save(&document)
    }
}

fn validate_connection(id: &str) -> Result<(), AppError> {
    if id.trim().is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

fn validate_report(connection_id: &str, report: &DatabaseAnalysisReport) -> Result<(), AppError> {
    validate_connection(connection_id)?;
    AnalyzeDatabaseInput {
        connection_id: connection_id.into(),
        pattern: report.pattern.clone(),
        delimiter: report.delimiter.clone(),
        max_keys: report.progress.max_keys,
    }
    .validate()?;
    if report.pattern.is_empty()
        || report.top_keys_by_length.len() > 15
        || report.top_keys_by_memory.len() > 15
        || report.top_namespaces_by_keys.len() > 15
        || report.top_namespaces_by_memory.len() > 15
        || report.expiration_groups.len() > 7
        || report.total_keys.types.len() > 128
        || report.total_memory.types.len() > 128
        || serde_json::to_vec(report)
            .map_err(|_| AppError::InvalidInput)?
            .len()
            > MAX_REPORT_BYTES
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
