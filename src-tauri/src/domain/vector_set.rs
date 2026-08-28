use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::error::AppError;

pub const MAX_VECTOR_ELEMENTS_PER_PAGE: usize = 200;
pub const MAX_VECTOR_TOP_K: u32 = 200;
pub const MAX_VECTOR_BATCH_ELEMENTS: usize = 200;
pub const MAX_VECTOR_DIMENSION: usize = 4096;
pub const MAX_VECTOR_ATTRIBUTE_BYTES: usize = 64 * 1024;
pub const MAX_VECTOR_BINARY_BYTES: usize = 4 * 1024 * 1024;
const MAX_VECTOR_NAME_BYTES: usize = 512;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSetElementPayload {
    pub name: String,
    pub vector_values: Option<Vec<f64>>,
    pub vector_fp32_base64: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

impl VectorSetElementPayload {
    pub fn validate(&self, expected_dimension: Option<u32>) -> Result<(), AppError> {
        validate_element_name(&self.name)?;
        let source_count =
            self.vector_values.is_some() as usize + self.vector_fp32_base64.is_some() as usize;
        if source_count != 1 {
            return Err(AppError::InvalidInput);
        }

        let dimension = if let Some(values) = &self.vector_values {
            if values.is_empty() || values.len() > MAX_VECTOR_DIMENSION {
                return Err(AppError::InvalidInput);
            }
            if values.iter().any(|value| !value.is_finite()) {
                return Err(AppError::InvalidInput);
            }
            values.len()
        } else if let Some(encoded) = &self.vector_fp32_base64 {
            decode_fp32_base64(encoded)?.len()
        } else {
            return Err(AppError::InvalidInput);
        };

        if dimension == 0
            || dimension > MAX_VECTOR_DIMENSION
            || expected_dimension.is_some_and(|expected| expected as usize != dimension)
        {
            return Err(AppError::InvalidInput);
        }
        validate_attributes(self.attributes.as_ref())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct VectorSetElementInput {
    pub connection_id: String,
    pub key: String,
    pub element: String,
}

impl VectorSetElementInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_element_name(&self.element)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSetSummary {
    pub key: String,
    pub total: String,
    pub dimension: Option<u32>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSetElement {
    pub name: String,
    pub score: Option<f64>,
    pub vector_base64: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSetPage {
    pub elements: Vec<VectorSetElement>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSimilarityMatch {
    pub name: String,
    pub score: f64,
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSimilarityResult {
    pub matches: Vec<VectorSimilarityMatch>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CreateVectorSetInput {
    pub connection_id: String,
    pub key: String,
    pub dimension: u32,
    pub quantization: Option<String>,
    pub elements: Vec<VectorSetElementPayload>,
    pub ttl_ms: Option<i64>,
}

impl CreateVectorSetInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_dimension(self.dimension)?;
        validate_quantization(self.quantization.as_deref())?;
        validate_ttl(self.ttl_ms)?;
        if self.elements.is_empty() || self.elements.len() > MAX_VECTOR_BATCH_ELEMENTS {
            return Err(AppError::InvalidInput);
        }
        for element in &self.elements {
            element.validate(Some(self.dimension))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AddVectorSetElementsInput {
    pub connection_id: String,
    pub key: String,
    pub elements: Vec<VectorSetElementPayload>,
}

impl AddVectorSetElementsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.elements.is_empty() || self.elements.len() > MAX_VECTOR_BATCH_ELEMENTS {
            return Err(AppError::InvalidInput);
        }
        for element in &self.elements {
            element.validate(None)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ListVectorSetElementsInput {
    pub connection_id: String,
    pub key: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub limit: usize,
}

impl ListVectorSetElementsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if !(1..=MAX_VECTOR_ELEMENTS_PER_PAGE).contains(&self.limit) {
            return Err(AppError::InvalidInput);
        }
        if self.start.as_deref().is_some_and(|value| value.is_empty())
            || self.end.as_deref().is_some_and(|value| value.is_empty())
        {
            return Err(AppError::InvalidInput);
        }
        if let Some(start) = &self.start {
            validate_element_name(start)?;
        }
        if let Some(end) = &self.end {
            validate_element_name(end)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SetVectorSetAttributesInput {
    pub connection_id: String,
    pub key: String,
    pub element: String,
    pub attributes: serde_json::Value,
}

impl SetVectorSetAttributesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        validate_element_name(&self.element)?;
        validate_attributes(Some(&self.attributes))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteVectorSetElementsInput {
    pub connection_id: String,
    pub key: String,
    pub elements: Vec<String>,
}

impl DeleteVectorSetElementsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if self.elements.is_empty() || self.elements.len() > MAX_VECTOR_BATCH_ELEMENTS {
            return Err(AppError::InvalidInput);
        }
        for element in &self.elements {
            validate_element_name(element)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSimilarityQueryInput {
    pub connection_id: String,
    pub key: String,
    pub by_element: Option<String>,
    pub by_vector: Option<Vec<f64>>,
    pub by_vector_base64: Option<String>,
    pub count: u32,
    pub with_attributes: bool,
}

impl VectorSimilarityQueryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_connection_and_key(&self.connection_id, &self.key)?;
        if !(1..=MAX_VECTOR_TOP_K).contains(&self.count) {
            return Err(AppError::InvalidInput);
        }
        let source_count = self.by_element.is_some() as usize
            + self.by_vector.is_some() as usize
            + self.by_vector_base64.is_some() as usize;
        if source_count != 1 {
            return Err(AppError::InvalidInput);
        }
        if let Some(element) = &self.by_element {
            validate_element_name(element)?;
        }
        if let Some(values) = &self.by_vector {
            if values.is_empty()
                || values.len() > MAX_VECTOR_DIMENSION
                || values.iter().any(|value| !value.is_finite())
            {
                return Err(AppError::InvalidInput);
            }
        }
        if let Some(encoded) = &self.by_vector_base64 {
            if decode_fp32_base64(encoded)?.is_empty() {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(())
    }
}

pub fn decode_fp32_base64(value: &str) -> Result<Vec<f32>, AppError> {
    let bytes = STANDARD.decode(value).map_err(|_| AppError::InvalidInput)?;
    if bytes.is_empty() || bytes.len() > MAX_VECTOR_BINARY_BYTES || !bytes.len().is_multiple_of(4) {
        return Err(AppError::InvalidInput);
    }
    let mut values = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !value.is_finite() {
            return Err(AppError::InvalidInput);
        }
        values.push(value);
    }
    if values.len() > MAX_VECTOR_DIMENSION {
        return Err(AppError::InvalidInput);
    }
    Ok(values)
}

fn validate_connection_and_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() || key.trim().is_empty() {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_element_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() || name.len() > MAX_VECTOR_NAME_BYTES {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_dimension(dimension: u32) -> Result<(), AppError> {
    if dimension == 0 || dimension as usize > MAX_VECTOR_DIMENSION {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_quantization(quantization: Option<&str>) -> Result<(), AppError> {
    if quantization.is_some_and(|value| value.trim().is_empty() || value.len() > 32) {
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

fn validate_attributes(attributes: Option<&serde_json::Value>) -> Result<(), AppError> {
    let Some(attributes) = attributes else {
        return Ok(());
    };
    let encoded = serde_json::to_vec(attributes).map_err(|_| AppError::InvalidInput)?;
    if encoded.len() > MAX_VECTOR_ATTRIBUTE_BYTES {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}
