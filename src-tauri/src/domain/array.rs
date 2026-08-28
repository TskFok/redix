use crate::error::AppError;

pub const MAX_ARRAY_ELEMENTS_PER_READ: usize = 500;
pub const MAX_ARRAY_BATCH_ELEMENTS: usize = 500;
pub const MAX_ARRAY_VALUE_BYTES: usize = 1024 * 1024;
pub const MAX_ARRAY_INDEX_BYTES: usize = 20;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArrayCreateMode {
    Contiguous,
    Sparse,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayElement {
    pub index: String,
    pub value: String,
}

impl ArrayElement {
    pub fn validate(&self) -> Result<(), AppError> {
        normalize_array_index(&self.index)?;
        validate_array_value(&self.value)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayCell {
    pub index: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArraySummary {
    pub key: String,
    pub length: String,
    pub count: String,
    pub next_index: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayRange {
    pub cells: Vec<ArrayCell>,
    pub start: String,
    pub end: String,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayScan {
    pub elements: Vec<ArrayElement>,
    pub next_start: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArraySearchResult {
    pub elements: Vec<ArrayElement>,
    pub total: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayAggregateResult {
    pub operation: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayMutationResult {
    pub affected: u64,
    pub key_exists: bool,
    pub next_index: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayKeyInput {
    pub connection_id: String,
    pub key: String,
}

impl ArrayKeyInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CreateArrayInput {
    pub connection_id: String,
    pub key: String,
    pub mode: ArrayCreateMode,
    pub start_index: Option<String>,
    pub values: Vec<String>,
    pub elements: Vec<ArrayElement>,
    pub ttl_ms: Option<i64>,
}

impl CreateArrayInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_ttl(self.ttl_ms)?;

        match self.mode {
            ArrayCreateMode::Contiguous => {
                if self.values.is_empty()
                    || self.values.len() > MAX_ARRAY_BATCH_ELEMENTS
                    || !self.elements.is_empty()
                {
                    return Err(AppError::InvalidInput);
                }
                if let Some(start_index) = &self.start_index {
                    normalize_array_index(start_index)?;
                }
                for value in &self.values {
                    validate_array_value(value)?;
                }
            }
            ArrayCreateMode::Sparse => {
                if self.elements.is_empty()
                    || self.elements.len() > MAX_ARRAY_BATCH_ELEMENTS
                    || !self.values.is_empty()
                {
                    return Err(AppError::InvalidInput);
                }
                if self.start_index.is_some() {
                    return Err(AppError::InvalidInput);
                }
                let mut indexes = std::collections::HashSet::with_capacity(self.elements.len());
                for element in &self.elements {
                    element.validate()?;
                    let index = normalize_array_index(&element.index)?;
                    if !indexes.insert(index) {
                        return Err(AppError::InvalidInput);
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayRangeInput {
    pub connection_id: String,
    pub key: String,
    pub start: String,
    pub end: String,
}

impl ArrayRangeInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_range(&self.start, &self.end)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayScanInput {
    pub connection_id: String,
    pub key: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub limit: usize,
}

impl ArrayScanInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if !(1..=MAX_ARRAY_ELEMENTS_PER_READ).contains(&self.limit) {
            return Err(AppError::InvalidInput);
        }
        validate_optional_range(self.start.as_deref(), self.end.as_deref())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayElementInput {
    pub connection_id: String,
    pub key: String,
    pub index: String,
}

impl ArrayElementInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        normalize_array_index(&self.index).map(|_| ())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayMultiGetInput {
    pub connection_id: String,
    pub key: String,
    pub indices: Vec<String>,
}

impl ArrayMultiGetInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.indices.is_empty() || self.indices.len() > MAX_ARRAY_BATCH_ELEMENTS {
            return Err(AppError::InvalidInput);
        }
        for index in &self.indices {
            normalize_array_index(index)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SetArrayElementInput {
    pub connection_id: String,
    pub key: String,
    pub index: String,
    pub value: String,
}

impl SetArrayElementInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        normalize_array_index(&self.index)?;
        validate_array_value(&self.value)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AppendArrayInput {
    pub connection_id: String,
    pub key: String,
    pub values: Vec<String>,
}

impl AppendArrayInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_array_values(&self.values)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteArrayElementsInput {
    pub connection_id: String,
    pub key: String,
    pub indices: Vec<String>,
}

impl DeleteArrayElementsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.indices.is_empty() || self.indices.len() > MAX_ARRAY_BATCH_ELEMENTS {
            return Err(AppError::InvalidInput);
        }
        for index in &self.indices {
            normalize_array_index(index)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteArrayRangeInput {
    pub connection_id: String,
    pub key: String,
    pub start: String,
    pub end: String,
}

impl DeleteArrayRangeInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_range(&self.start, &self.end)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayPredicate {
    pub criteria: String,
    pub value: String,
}

impl ArrayPredicate {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.criteria.trim().is_empty() || self.criteria.len() > 64 {
            return Err(AppError::InvalidInput);
        }
        validate_array_value(&self.value)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchArrayInput {
    pub connection_id: String,
    pub key: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub predicates: Vec<ArrayPredicate>,
    pub combinator: Option<String>,
    pub nocase: bool,
    pub with_values: bool,
    pub limit: usize,
}

impl SearchArrayInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.predicates.is_empty()
            || self.predicates.len() > MAX_ARRAY_BATCH_ELEMENTS
            || !(1..=MAX_ARRAY_ELEMENTS_PER_READ).contains(&self.limit)
        {
            return Err(AppError::InvalidInput);
        }
        if self
            .combinator
            .as_deref()
            .is_some_and(|value| !matches!(value.to_ascii_uppercase().as_str(), "AND" | "OR"))
        {
            return Err(AppError::InvalidInput);
        }
        for predicate in &self.predicates {
            predicate.validate()?;
        }
        validate_optional_range(self.start.as_deref(), self.end.as_deref())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ArrayAggregateOperation {
    Count,
    Sum,
    Average,
    Min,
    Max,
    Match,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AggregateArrayInput {
    pub connection_id: String,
    pub key: String,
    pub operation: ArrayAggregateOperation,
    pub start: Option<String>,
    pub end: Option<String>,
    pub values: Vec<String>,
    pub limit: usize,
}

impl AggregateArrayInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.values.len() > MAX_ARRAY_BATCH_ELEMENTS
            || self
                .values
                .iter()
                .any(|value| value.len() > MAX_ARRAY_VALUE_BYTES)
            || self.limit == 0
            || self.limit > MAX_ARRAY_ELEMENTS_PER_READ
        {
            return Err(AppError::InvalidInput);
        }
        validate_optional_range(self.start.as_deref(), self.end.as_deref())
    }
}

pub fn normalize_array_index(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_ARRAY_INDEX_BYTES
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(AppError::InvalidInput);
    }
    value
        .parse::<u64>()
        .map(|index| index.to_string())
        .map_err(|_| AppError::InvalidInput)
}

fn validate_connection_and_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() || key.trim().is_empty() {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_ttl(ttl_ms: Option<i64>) -> Result<(), AppError> {
    if ttl_ms.is_some_and(|ttl| ttl < 0) {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_array_value(value: &str) -> Result<(), AppError> {
    if value.len() > MAX_ARRAY_VALUE_BYTES {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_array_values(values: &[String]) -> Result<(), AppError> {
    if values.is_empty() || values.len() > MAX_ARRAY_BATCH_ELEMENTS {
        return Err(AppError::InvalidInput);
    }
    for value in values {
        validate_array_value(value)?;
    }
    Ok(())
}

fn validate_range(start: &str, end: &str) -> Result<(), AppError> {
    let start = normalize_array_index(start)?
        .parse::<u64>()
        .map_err(|_| AppError::InvalidInput)?;
    let end = normalize_array_index(end)?
        .parse::<u64>()
        .map_err(|_| AppError::InvalidInput)?;
    if start > end
        || end
            .checked_sub(start)
            .and_then(|length| length.checked_add(1))
            .is_none_or(|length| length > MAX_ARRAY_ELEMENTS_PER_READ as u64)
    {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_optional_range(start: Option<&str>, end: Option<&str>) -> Result<(), AppError> {
    match (start, end) {
        (None, None) => Ok(()),
        (Some(start), Some(end)) => validate_range(start, end),
        _ => Err(AppError::InvalidInput),
    }
}
