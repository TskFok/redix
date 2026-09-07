use std::collections::{BTreeMap, HashSet};

use crate::{
    domain::{is_sensitive_command, QueryLibraryItemInput},
    error::AppError,
    persistence::VersionedJsonDocument,
};

pub const MAX_QUERY_PACKAGE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_QUERY_LIBRARY_ITEMS: usize = 500;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionTag {
    pub key: String,
    pub value: String,
}

pub type ConnectionTags = BTreeMap<String, Vec<ConnectionTag>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionTagsDocument {
    pub version: u32,
    pub connections: ConnectionTags,
}

impl Default for ConnectionTagsDocument {
    fn default() -> Self {
        Self {
            version: 1,
            connections: BTreeMap::new(),
        }
    }
}

impl VersionedJsonDocument for ConnectionTagsDocument {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let mut document: Self =
            serde_json::from_value(value).map_err(|_| AppError::PersistenceFailed)?;
        if document.version != 1 || document.connections.len() > 10_000 {
            return Err(AppError::PersistenceFailed);
        }
        for (id, tags) in &mut document.connections {
            if id.trim().is_empty() || id.chars().count() > 128 {
                return Err(AppError::PersistenceFailed);
            }
            *tags = normalize_connection_tags(std::mem::take(tags))
                .map_err(|_| AppError::PersistenceFailed)?;
        }
        Ok(document)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveConnectionTagsInput {
    pub connection_id: String,
    pub tags: Vec<ConnectionTag>,
}

pub fn normalize_connection_tags(tags: Vec<ConnectionTag>) -> Result<Vec<ConnectionTag>, AppError> {
    if tags.len() > 20 {
        return Err(AppError::InvalidInput);
    }
    let mut keys = HashSet::new();
    tags.into_iter()
        .map(|tag| {
            let tag = ConnectionTag {
                key: tag.key.trim().into(),
                value: tag.value.trim().into(),
            };
            if tag.key.is_empty()
                || tag.key.chars().count() > 40
                || tag.value.is_empty()
                || tag.value.chars().count() > 120
                || tag.key.chars().any(char::is_control)
                || tag.value.chars().any(char::is_control)
                || !keys.insert(tag.key.clone())
            {
                return Err(AppError::InvalidInput);
            }
            Ok(tag)
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryPackageItem {
    pub name: String,
    pub command: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryPackage {
    pub format: String,
    pub version: u32,
    pub items: Vec<QueryPackageItem>,
}

impl QueryPackage {
    pub fn parse(content: &str) -> Result<Self, AppError> {
        if content.len() > MAX_QUERY_PACKAGE_BYTES {
            return Err(AppError::InvalidInput);
        }
        let mut document: Self =
            serde_json::from_str(content).map_err(|_| AppError::InvalidInput)?;
        document.validate()?;
        for item in &mut document.items {
            item.name = item.name.trim().into();
            item.command = item.command.trim().into();
            item.tags = item.tags.iter().map(|tag| tag.trim().to_owned()).collect();
        }
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.format != "redix-query-library"
            || self.version != 1
            || self.items.len() > MAX_QUERY_LIBRARY_ITEMS
        {
            return Err(AppError::InvalidInput);
        }
        for item in &self.items {
            QueryLibraryItemInput {
                id: None,
                name: item.name.clone(),
                command: item.command.clone(),
                tags: item.tags.clone(),
            }
            .validate()
            .map_err(|_| AppError::InvalidInput)?;
            validate_query_text(&item.command)?;
        }
        Ok(())
    }
}

/// Workbench executes one non-comment command per line. Check every executable
/// line so a harmless first command cannot hide credentials in a later one.
pub fn validate_query_text(command: &str) -> Result<(), AppError> {
    let mut count = 0;
    for line in command
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
    {
        count += 1;
        if is_sensitive_command(line) {
            return Err(AppError::InvalidInput);
        }
    }
    if count == 0 {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
