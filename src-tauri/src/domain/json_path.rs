use crate::error::AppError;

const MAX_JSON_PATH_BYTES: usize = 512;
const MAX_JSON_PAYLOAD_BYTES: usize = 5 * 1024 * 1024;
const MAX_JSON_ARRAY_APPEND_VALUES: usize = 500;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
}

impl GetJsonPathInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_json_path(&self.path, false)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SetJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
    pub value: serde_json::Value,
}

impl SetJsonPathInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_json_path(&self.path, false)?;
        validate_json_payload(&self.value)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AppendJsonArrayInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
    pub values: Vec<serde_json::Value>,
}

impl AppendJsonArrayInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_json_path(&self.path, false)?;
        validate_json_array_append(&self.values)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
}

impl DeleteJsonPathInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_json_path(&self.path, false)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct JsonPathValue {
    pub key: String,
    pub path: String,
    pub value: Option<serde_json::Value>,
    pub ttl_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JsonMutationResult {
    pub key: String,
    pub path: String,
    pub affected: u64,
    pub new_length: Option<u64>,
    pub ttl_ms: i64,
}

pub fn validate_json_path(path: &str, _allow_legacy_root: bool) -> Result<(), AppError> {
    let path_bytes = path.len();
    if !(1..=MAX_JSON_PATH_BYTES).contains(&path_bytes) {
        return Err(AppError::InvalidInput);
    }

    if !path.starts_with('$') && !path.starts_with('.') {
        return Err(AppError::InvalidInput);
    }

    if path.chars().any(|character| {
        character.is_control() || matches!(character, '*' | '?' | ';')
    }) || path.contains("..")
    {
        return Err(AppError::InvalidInput);
    }

    Ok(())
}

pub fn normalize_json_path(path: &str, legacy: bool) -> Result<String, AppError> {
    validate_json_path(path, legacy)?;

    if legacy {
        if path == "$" {
            return Ok(".".to_string());
        }
        if let Some(stripped) = path.strip_prefix("$.") {
            return Ok(format!(".{stripped}"));
        }
    }

    Ok(path.to_string())
}

pub fn validate_json_array_append(values: &[serde_json::Value]) -> Result<(), AppError> {
    if values.is_empty() || values.len() > MAX_JSON_ARRAY_APPEND_VALUES {
        return Err(AppError::InvalidInput);
    }

    let mut total_bytes = 0usize;
    for value in values {
        let encoded = serde_json::to_vec(value).map_err(|_| AppError::InvalidInput)?;
        total_bytes = total_bytes.saturating_add(encoded.len());
        if total_bytes > MAX_JSON_PAYLOAD_BYTES {
            return Err(AppError::InvalidInput);
        }
    }

    Ok(())
}

fn validate_connection_and_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() || key.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }

    Ok(())
}

fn validate_json_payload(value: &serde_json::Value) -> Result<(), AppError> {
    let encoded = serde_json::to_vec(value).map_err(|_| AppError::InvalidInput)?;
    if encoded.len() > MAX_JSON_PAYLOAD_BYTES {
        return Err(AppError::InvalidInput);
    }

    Ok(())
}
