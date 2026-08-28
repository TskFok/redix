use std::collections::HashSet;

use crate::error::AppError;

pub const REDISEARCH_MIN_VERSION: &str = "2.0.0";
pub const MAX_SEARCH_NAME_BYTES: usize = 256;
pub const MAX_SEARCH_KEY_BYTES: usize = 512;
pub const MAX_SEARCH_QUERY_BYTES: usize = 4096;
pub const MAX_SEARCH_ATTRIBUTES: usize = 256;
pub const MAX_SEARCH_FIELDS: usize = 64;
pub const MAX_SEARCH_PREFIXES: usize = 64;
pub const MAX_SEARCH_INDEXES: usize = 500;
pub const MAX_SEARCH_OFFSET: u64 = 100_000;
pub const MAX_SEARCH_PAGE: u32 = 200;
pub const MAX_SEARCH_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ListSearchIndexesInput {
    pub connection_id: String,
}

impl ListSearchIndexesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchIndexSummary {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ListSearchIndexesResult {
    pub indexes: Vec<SearchIndexSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchKeyType {
    Hash,
    Json,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchFieldType {
    Text,
    Tag,
    Numeric,
    Geo,
    Geoshape,
    Vector,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchIndexFieldInput {
    pub name: String,
    pub field_type: SearchFieldType,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CreateSearchIndexInput {
    pub connection_id: String,
    pub index: String,
    pub key_type: SearchKeyType,
    pub prefixes: Vec<String>,
    pub fields: Vec<SearchIndexFieldInput>,
}

impl CreateSearchIndexInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)?;
        validate_search_name(&self.index)?;
        if self.fields.is_empty() || self.fields.len() > MAX_SEARCH_FIELDS {
            return Err(AppError::InvalidInput);
        }
        if self.prefixes.len() > MAX_SEARCH_PREFIXES {
            return Err(AppError::InvalidInput);
        }

        let mut field_names = HashSet::with_capacity(self.fields.len());
        for field in &self.fields {
            validate_search_name(&field.name)?;
            if !field_names.insert(field.name.trim().to_owned()) {
                return Err(AppError::InvalidInput);
            }
        }
        for prefix in &self.prefixes {
            validate_search_name(prefix)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchIndexInput {
    pub connection_id: String,
    pub index: String,
}

impl SearchIndexInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)?;
        validate_search_name(&self.index)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetKeySearchIndexesInput {
    pub connection_id: String,
    pub key: String,
}

impl GetKeySearchIndexesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)?;
        if self.key.trim().is_empty() || self.key.len() > MAX_SEARCH_KEY_BYTES {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct KeySearchIndexSummary {
    pub name: String,
    pub key_type: String,
    pub prefixes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchIndexAttribute {
    pub identifier: String,
    pub field_type: String,
    pub sortable: bool,
    pub no_index: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchIndexInfo {
    pub index_name: String,
    pub key_type: String,
    pub prefixes: Vec<String>,
    pub attributes: Vec<SearchIndexAttribute>,
    pub num_docs: Option<u64>,
    pub num_terms: Option<u64>,
    pub num_records: Option<u64>,
    pub total_index_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchQueryInput {
    pub connection_id: String,
    pub index: String,
    pub query: String,
    pub offset: u64,
    pub limit: u32,
}

impl SearchQueryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)?;
        validate_search_name(&self.index)?;
        if self.query.trim().is_empty() || self.query.len() > MAX_SEARCH_QUERY_BYTES {
            return Err(AppError::InvalidInput);
        }
        if self.offset > MAX_SEARCH_OFFSET || !(1..=MAX_SEARCH_PAGE).contains(&self.limit) {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchKeyResult {
    pub key: String,
    pub key_type: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchQueryResult {
    pub total: u64,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub max_results: Option<u64>,
    pub keys: Vec<SearchKeyResult>,
}

fn validate_connection_id(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(AppError::InvalidConnection)
    } else {
        Ok(())
    }
}

fn validate_search_name(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > MAX_SEARCH_NAME_BYTES {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn search_version_supported(version: Option<&str>) -> bool {
    let Some(version) = version else {
        return false;
    };
    let mut parts = version.trim().split('.');
    let mut parsed = [0_u64; 3];
    for part in &mut parsed {
        let Some(value) = parts.next() else {
            break;
        };
        let Ok(value) = value.parse::<u64>() else {
            return false;
        };
        *part = value;
    }
    if parts.next().is_some() || version.trim().is_empty() {
        return false;
    }

    parsed >= [2, 0, 0]
}
