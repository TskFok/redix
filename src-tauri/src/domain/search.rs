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
pub const MAX_SEARCH_FIELD_BYTES: usize = 256 * 1024;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub field_type: SearchFieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<SearchVectorConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SearchVectorAlgorithm {
    Flat,
    Hnsw,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SearchVectorDataType {
    Float32,
    Float64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SearchVectorDistanceMetric {
    Cosine,
    L2,
    Ip,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchVectorConfig {
    pub algorithm: SearchVectorAlgorithm,
    pub data_type: SearchVectorDataType,
    pub dimension: u32,
    pub distance_metric: SearchVectorDistanceMetric,
    pub initial_capacity: Option<u32>,
    pub block_size: Option<u32>,
    pub m: Option<u32>,
    pub ef_construction: Option<u32>,
    pub ef_runtime: Option<u32>,
}

impl SearchVectorConfig {
    fn validate(&self) -> Result<(), AppError> {
        if !(1..=32_768).contains(&self.dimension)
            || self
                .initial_capacity
                .is_some_and(|value| !(1..=1_000_000).contains(&value))
            || self
                .block_size
                .is_some_and(|value| !(1..=1_000_000).contains(&value))
            || self.m.is_some_and(|value| !(1..=512).contains(&value))
            || self
                .ef_construction
                .is_some_and(|value| !(1..=4096).contains(&value))
            || self
                .ef_runtime
                .is_some_and(|value| !(1..=4096).contains(&value))
            || (self.algorithm == SearchVectorAlgorithm::Flat
                && (self.m.is_some()
                    || self.ef_construction.is_some()
                    || self.ef_runtime.is_some()))
            || (self.algorithm == SearchVectorAlgorithm::Hnsw && self.block_size.is_some())
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }

    /// Each option is a separate Redis argument; the caller derives the attribute count.
    pub fn command_arguments(&self) -> Vec<String> {
        let mut args = vec![
            "TYPE".into(),
            match self.data_type {
                SearchVectorDataType::Float32 => "FLOAT32",
                SearchVectorDataType::Float64 => "FLOAT64",
            }
            .into(),
            "DIM".into(),
            self.dimension.to_string(),
            "DISTANCE_METRIC".into(),
            match self.distance_metric {
                SearchVectorDistanceMetric::Cosine => "COSINE",
                SearchVectorDistanceMetric::L2 => "L2",
                SearchVectorDistanceMetric::Ip => "IP",
            }
            .into(),
        ];
        for (name, value) in [
            ("INITIAL_CAP", self.initial_capacity),
            ("BLOCK_SIZE", self.block_size),
            ("M", self.m),
            ("EF_CONSTRUCTION", self.ef_construction),
            ("EF_RUNTIME", self.ef_runtime),
        ] {
            if let Some(value) = value {
                args.push(name.into());
                args.push(value.to_string());
            }
        }
        args
    }

    pub fn algorithm_name(&self) -> &'static str {
        match self.algorithm {
            SearchVectorAlgorithm::Flat => "FLAT",
            SearchVectorAlgorithm::Hnsw => "HNSW",
        }
    }
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
            match (&field.field_type, &field.vector) {
                (SearchFieldType::Vector, Some(vector)) => vector.validate()?,
                (SearchFieldType::Vector, None) | (_, Some(_)) => {
                    return Err(AppError::InvalidInput)
                }
                (_, None) => {}
            }
            if !field_names.insert(field.name.trim().to_owned()) {
                return Err(AppError::InvalidInput);
            }
        }
        let mut aliases = HashSet::new();
        for field in &self.fields {
            if let Some(alias) = &field.alias {
                if !safe_vector_field(alias)
                    || !aliases.insert(alias)
                    || (alias != &field.name && field_names.contains(alias.as_str()))
                {
                    return Err(AppError::InvalidInput);
                }
            }
        }
        for prefix in &self.prefixes {
            validate_search_name(prefix)?;
        }
        Ok(())
    }

    pub fn validate_search_version(&self, version: Option<&str>) -> Result<(), AppError> {
        if self
            .fields
            .iter()
            .any(|field| field.field_type == SearchFieldType::Vector)
            && !search_version_at_least(version, [2, 4, 0])
        {
            return Err(AppError::UnsupportedFeature);
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_name: Option<String>,
    pub field_type: String,
    pub sortable: bool,
    pub no_index: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<SearchVectorFieldInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchVectorFieldInfo {
    pub data_type: String,
    pub dimension: u32,
    pub distance_metric: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchVectorQueryInput {
    pub connection_id: String,
    pub index: String,
    pub field: String,
    pub vector: Vec<f64>,
    pub count: u32,
    pub filter: String,
}

impl SearchVectorQueryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_id(&self.connection_id)?;
        validate_search_name(&self.index)?;
        if !safe_vector_field(&self.field)
            || !(1..=MAX_SEARCH_PAGE).contains(&self.count)
            || self.vector.is_empty()
            || self.vector.len() > 32_768
            || self.vector.iter().any(|value| !value.is_finite())
            || !valid_vector_filter(&self.filter)
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }

    pub fn validate_search_version(version: Option<&str>) -> Result<(), AppError> {
        if search_version_at_least(version, [2, 4, 0]) {
            Ok(())
        } else {
            Err(AppError::UnsupportedFeature)
        }
    }

    pub fn encode_vector(&self, schema: &SearchVectorFieldInfo) -> Result<Vec<u8>, AppError> {
        self.validate()?;
        if !(1..=32_768).contains(&schema.dimension)
            || self.vector.len() != schema.dimension as usize
            || !matches!(schema.distance_metric.as_str(), "COSINE" | "L2" | "IP")
        {
            return Err(AppError::InvalidInput);
        }
        let mut bytes = Vec::with_capacity(self.vector.len() * 8);
        let mut nonzero = false;
        for value in &self.vector {
            match schema.data_type.as_str() {
                "FLOAT32" => {
                    let value = *value as f32;
                    if !value.is_finite() {
                        return Err(AppError::InvalidInput);
                    }
                    nonzero |= value != 0.0;
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                "FLOAT64" => {
                    nonzero |= *value != 0.0;
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                _ => return Err(AppError::UnsupportedFeature),
            }
        }
        if schema.distance_metric == "COSINE" && !nonzero {
            return Err(AppError::InvalidInput);
        }
        Ok(bytes)
    }
}

pub fn safe_vector_field(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEARCH_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_vector_filter(value: &str) -> bool {
    if value.trim().is_empty()
        || value.len() > MAX_SEARCH_QUERY_BYTES
        || value.chars().any(char::is_control)
    {
        return false;
    }
    // User filters remain one Redis argument. Balanced grouping and no nested query suffix
    // keep the generated KNN clause at the root while preserving escaped/quoted literals.
    let mut depth = 0_u32;
    let mut range = None;
    let mut quoted = false;
    let mut escaped = false;
    let mut previous = '\0';
    for character in value.chars() {
        if escaped {
            escaped = false;
            previous = '\0';
            continue;
        }
        if character == '\\' {
            escaped = true;
            previous = '\0';
            continue;
        }
        if character == '"' {
            quoted = !quoted;
        }
        if !quoted {
            match character {
                '>' if previous == '=' => return false,
                '[' | '{' if range.is_none() => range = Some(character),
                ']' if range == Some('[') => range = None,
                '}' if range == Some('{') => range = None,
                '[' | '{' | ']' | '}' => return false,
                '(' if range.is_none() => depth += 1,
                ')' if range.is_none() && depth == 0 => return false,
                ')' if range.is_none() => depth -= 1,
                _ => (),
            }
        }
        previous = character;
    }
    depth == 0 && range.is_none() && !quoted && !escaped
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SearchVectorMatch {
    pub key: String,
    pub distance: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SearchVectorQueryResult {
    pub matches: Vec<SearchVectorMatch>,
    pub returned: usize,
    pub count: u32,
    pub distance_metric: String,
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
    #[serde(default)]
    pub include_content: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<SearchDocumentField>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchDocumentField {
    pub name: String,
    pub value: serde_json::Value,
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
    search_version_at_least(version, [2, 0, 0])
}

fn search_version_at_least(version: Option<&str>, minimum: [u64; 3]) -> bool {
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

    parsed >= minimum
}

#[cfg(test)]
mod vector_tests {
    use super::*;
    use serde_json::{json, Value};

    fn vector_input(config: Value) -> CreateSearchIndexInput {
        serde_json::from_value(json!({
            "connection_id": "local", "index": "idx", "key_type": "hash", "prefixes": [],
            "fields": [{"name": "embedding", "field_type": "vector", "vector": config}]
        }))
        .unwrap()
    }

    fn flat() -> Value {
        json!({"algorithm": "FLAT", "data_type": "FLOAT32", "dimension": 128, "distance_metric": "COSINE"})
    }

    #[test]
    fn vector_rejects_invalid_bounds_and_algorithm_options() {
        for (field, value) in [
            ("dimension", 0),
            ("dimension", 32769),
            ("initial_capacity", 0),
            ("block_size", 1000001),
            ("m", 16),
            ("ef_construction", 200),
            ("ef_runtime", 10),
        ] {
            let mut config = flat();
            config[field] = json!(value);
            assert_eq!(vector_input(config).validate(), Err(AppError::InvalidInput));
        }
        for (field, value) in [
            ("m", 0),
            ("m", 513),
            ("ef_construction", 4097),
            ("ef_runtime", 4097),
            ("block_size", 100),
        ] {
            let mut config = flat();
            config["algorithm"] = json!("HNSW");
            config[field] = json!(value);
            assert_eq!(vector_input(config).validate(), Err(AppError::InvalidInput));
        }
        assert_eq!(vector_input(flat()).validate(), Ok(()));
        let mut input = vector_input(flat());
        input.fields[0].field_type = SearchFieldType::Text;
        assert_eq!(input.validate(), Err(AppError::InvalidInput));
    }

    #[test]
    fn vector_rejects_unsupported_enums_unknown_options_and_fractional_dimensions() {
        for (field, value) in [
            ("algorithm", json!("HNSW SCHEMA bad TEXT")),
            ("data_type", json!("UNKNOWN")),
            ("distance_metric", json!("OTHER")),
            ("dimension", json!(1.5)),
            ("unexpected_option", json!(42)),
        ] {
            let mut config = flat();
            config[field] = value;
            assert!(serde_json::from_value::<SearchVectorConfig>(config).is_err());
        }
    }

    #[test]
    fn vector_version_check_and_old_non_vector_payload_are_compatible() {
        let input = vector_input(flat());
        for version in [None, Some("bad"), Some("2.2.0"), Some("2.3.99")] {
            assert_eq!(
                input.validate_search_version(version),
                Err(AppError::UnsupportedFeature)
            );
        }
        for version in ["2.4.0", "2.10.0", "8.0.0"] {
            assert_eq!(input.validate_search_version(Some(version)), Ok(()));
        }
        let legacy: CreateSearchIndexInput = serde_json::from_value(json!({
            "connection_id": "local", "index": "idx", "key_type": "hash", "prefixes": [],
            "fields": [{"name": "name", "field_type": "text"}]
        }))
        .unwrap();
        assert_eq!(legacy.validate(), Ok(()));
        assert_eq!(legacy.validate_search_version(Some("2.0.0")), Ok(()));
        assert!(serde_json::to_value(legacy).unwrap()["fields"][0]
            .get("vector")
            .is_none());
        let attribute: SearchIndexAttribute = serde_json::from_value(json!({
            "identifier": "name", "field_type": "TEXT", "sortable": false, "no_index": false
        }))
        .unwrap();
        assert_eq!(attribute.query_name, None);
    }
}
