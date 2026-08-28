use ::redis::{Cmd, Value};
use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::{
    domain::{
        DeleteVectorSetElementsInput, ListVectorSetElementsInput, SetVectorSetAttributesInput,
        VectorSetElement, VectorSetElementPayload, VectorSimilarityMatch,
        VectorSimilarityQueryInput, MAX_VECTOR_ATTRIBUTE_BYTES, MAX_VECTOR_BINARY_BYTES,
        MAX_VECTOR_DIMENSION, MAX_VECTOR_ELEMENTS_PER_PAGE, MAX_VECTOR_TOP_K,
    },
    error::AppError,
};

const MAX_VECTOR_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) use crate::domain::decode_fp32_base64;

pub(crate) fn encode_fp32_base64(values: &[f64]) -> Result<String, AppError> {
    if values.is_empty() || values.len() > MAX_VECTOR_DIMENSION {
        return Err(AppError::InvalidInput);
    }
    let byte_len = values
        .len()
        .checked_mul(4)
        .filter(|length| *length <= MAX_VECTOR_BINARY_BYTES)
        .ok_or(AppError::InvalidInput)?;
    let mut bytes = Vec::with_capacity(byte_len);
    for value in values {
        if !value.is_finite() {
            return Err(AppError::InvalidInput);
        }
        let value = *value as f32;
        if !value.is_finite() {
            return Err(AppError::InvalidInput);
        }
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(STANDARD.encode(bytes))
}

pub(crate) fn parse_vector_set_info(
    value: Value,
) -> Result<(Option<u32>, Option<String>), AppError> {
    ensure_response_size(&value)?;
    let mut pairs = Vec::new();
    collect_info_pairs(value, &mut pairs)?;
    let mut dimension = None;
    let mut quantization = None;
    for (key, value) in pairs {
        match key.to_ascii_lowercase().as_str() {
            "vector-dim" | "vector_dim" | "dimension" => {
                let parsed = decimal_u64(&value)?.ok_or(AppError::CommandFailed)?;
                let parsed = u32::try_from(parsed).map_err(|_| AppError::CommandFailed)?;
                if parsed == 0 || parsed as usize > MAX_VECTOR_DIMENSION {
                    return Err(AppError::CommandFailed);
                }
                dimension = Some(parsed);
            }
            "quant-type" | "quant_type" | "quantization" => {
                quantization = Some(value);
            }
            _ => {}
        }
    }
    Ok((dimension, quantization))
}

pub(crate) fn parse_vector_set_page(value: Value, limit: usize) -> Result<Vec<String>, AppError> {
    if !(1..=MAX_VECTOR_ELEMENTS_PER_PAGE).contains(&limit) {
        return Err(AppError::InvalidInput);
    }
    ensure_response_size(&value)?;
    let mut names = Vec::new();
    collect_vector_names(value, &mut names)?;
    if names.len() > limit {
        return Err(AppError::CommandFailed);
    }
    Ok(names)
}

pub(crate) fn parse_vector_set_element(
    vector: Value,
    attributes: Value,
    name: &str,
) -> Result<VectorSetElement, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }
    ensure_response_size(&vector)?;
    ensure_response_size(&attributes)?;
    let vector_base64 = parse_vector_payload(vector)?;
    let attributes = parse_attributes(attributes)?;
    Ok(VectorSetElement {
        name: name.to_owned(),
        score: None,
        vector_base64,
        attributes,
    })
}

pub(crate) fn parse_vsim_reply(
    value: Value,
    with_attributes: bool,
) -> Result<Vec<VectorSimilarityMatch>, AppError> {
    ensure_response_size(&value)?;
    let values = match unwrap_attributes(value) {
        Value::Nil => return Ok(Vec::new()),
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => values,
        _ => return Err(AppError::CommandFailed),
    };
    let stride = if with_attributes { 3 } else { 2 };
    let mut matches = Vec::new();
    if values.first().is_some_and(is_pair_container) {
        for entry in values {
            let pair = match unwrap_attributes(entry) {
                Value::Array(pair) | Value::Set(pair) if pair.len() >= stride => pair,
                _ => return Err(AppError::CommandFailed),
            };
            matches.push(parse_vsim_match(&pair, with_attributes)?);
        }
    } else {
        if values.len() % stride != 0 {
            return Err(AppError::CommandFailed);
        }
        for pair in values.chunks_exact(stride) {
            matches.push(parse_vsim_match(pair, with_attributes)?);
        }
    }
    if matches.len() > MAX_VECTOR_TOP_K as usize {
        return Err(AppError::CommandFailed);
    }
    Ok(matches)
}

pub(crate) fn build_vadd_command(
    key: &str,
    element: &VectorSetElementPayload,
    expected_dimension: Option<u32>,
) -> Result<Cmd, AppError> {
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }
    element.validate(expected_dimension)?;
    let mut command = ::redis::cmd("VADD");
    command.arg(key);
    if let Some(values) = &element.vector_values {
        command.arg("VALUES").arg(values.len());
        for value in values {
            command.arg(value.to_string());
        }
    } else if let Some(encoded) = &element.vector_fp32_base64 {
        let bytes = decode_fp32_bytes(encoded)?;
        command.arg("FP32").arg(bytes);
    } else {
        return Err(AppError::InvalidInput);
    }
    command.arg(&element.name);
    if let Some(attributes) = &element.attributes {
        command
            .arg("SETATTR")
            .arg(serialize_attributes(attributes)?);
    }
    Ok(command)
}

pub(crate) fn build_vrange_command(input: &ListVectorSetElementsInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("VRANGE");
    command
        .arg(&input.key)
        .arg(input.start.as_deref().unwrap_or("-"))
        .arg(input.end.as_deref().unwrap_or("+"))
        .arg(input.limit);
    Ok(command)
}

pub(crate) fn build_vsim_command(input: &VectorSimilarityQueryInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("VSIM");
    command.arg(&input.key);
    if let Some(element) = &input.by_element {
        command.arg("ELE").arg(element);
    } else if let Some(values) = &input.by_vector {
        command.arg("VALUES").arg(values.len());
        for value in values {
            command.arg(value.to_string());
        }
    } else if let Some(encoded) = &input.by_vector_base64 {
        command.arg("FP32").arg(decode_fp32_bytes(encoded)?);
    } else {
        return Err(AppError::InvalidInput);
    }
    command.arg("COUNT").arg(input.count).arg("WITHSCORES");
    if input.with_attributes {
        command.arg("WITHATTRIBS");
    }
    Ok(command)
}

pub(crate) fn build_vsetattr_command(input: &SetVectorSetAttributesInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("VSETATTR");
    command
        .arg(&input.key)
        .arg(&input.element)
        .arg(serialize_attributes(&input.attributes)?);
    Ok(command)
}

pub(crate) fn build_vrem_command(input: &DeleteVectorSetElementsInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("VREM");
    command.arg(&input.key);
    for element in &input.elements {
        command.arg(element);
    }
    Ok(command)
}

pub(crate) fn build_vemb_command(key: &str, element: &str) -> Result<Cmd, AppError> {
    validate_key_and_element(key, element)?;
    let mut command = ::redis::cmd("VEMB");
    command.arg(key).arg(element);
    Ok(command)
}

pub(crate) fn build_vgetattr_command(key: &str, element: &str) -> Result<Cmd, AppError> {
    validate_key_and_element(key, element)?;
    let mut command = ::redis::cmd("VGETATTR");
    command.arg(key).arg(element);
    Ok(command)
}

fn decode_fp32_bytes(value: &str) -> Result<Vec<u8>, AppError> {
    let _ = decode_fp32_base64(value)?;
    let bytes = STANDARD.decode(value).map_err(|_| AppError::InvalidInput)?;
    if bytes.is_empty() || bytes.len() > MAX_VECTOR_BINARY_BYTES || !bytes.len().is_multiple_of(4) {
        return Err(AppError::InvalidInput);
    }
    Ok(bytes)
}

fn parse_vector_payload(value: Value) -> Result<Option<String>, AppError> {
    match unwrap_attributes(value) {
        Value::Nil => Ok(None),
        Value::BulkString(bytes) => {
            let encoded = STANDARD.encode(bytes);
            let _ = decode_fp32_base64(&encoded)?;
            Ok(Some(encoded))
        }
        Value::Array(values) | Value::Set(values) => {
            let mut numeric = Vec::with_capacity(values.len());
            for value in values {
                numeric.push(number_value(value)?);
            }
            Ok(Some(encode_fp32_base64(&numeric)?))
        }
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_vsim_match(
    values: &[Value],
    with_attributes: bool,
) -> Result<VectorSimilarityMatch, AppError> {
    let name = text_value(values.first().ok_or(AppError::CommandFailed)?.clone())?
        .ok_or(AppError::CommandFailed)?;
    let score = number_value(values.get(1).cloned().ok_or(AppError::CommandFailed)?)?;
    let attributes = if with_attributes {
        parse_attributes(values.get(2).cloned().ok_or(AppError::CommandFailed)?)?
    } else {
        None
    };
    Ok(VectorSimilarityMatch {
        name,
        score,
        attributes,
    })
}

fn collect_info_pairs(value: Value, pairs: &mut Vec<(String, String)>) -> Result<(), AppError> {
    match unwrap_attributes(value) {
        Value::Map(entries) => {
            for (key, value) in entries {
                let key = text_value(key)?.ok_or(AppError::CommandFailed)?;
                let value = text_value(value)?.ok_or(AppError::CommandFailed)?;
                pairs.push((key, value));
            }
        }
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => {
            if values.iter().all(is_pair_container) {
                for entry in values {
                    let pair = match unwrap_attributes(entry) {
                        Value::Array(pair) | Value::Set(pair) if pair.len() >= 2 => pair,
                        _ => return Err(AppError::CommandFailed),
                    };
                    let key = text_value(pair[0].clone())?.ok_or(AppError::CommandFailed)?;
                    let value = text_value(pair[1].clone())?.ok_or(AppError::CommandFailed)?;
                    pairs.push((key, value));
                }
            } else {
                if values.len() % 2 != 0 {
                    return Err(AppError::CommandFailed);
                }
                for pair in values.chunks_exact(2) {
                    let key = text_value(pair[0].clone())?.ok_or(AppError::CommandFailed)?;
                    let value = text_value(pair[1].clone())?.ok_or(AppError::CommandFailed)?;
                    pairs.push((key, value));
                }
            }
        }
        Value::Nil => {}
        _ => return Err(AppError::CommandFailed),
    }
    Ok(())
}

fn collect_vector_names(value: Value, names: &mut Vec<String>) -> Result<(), AppError> {
    match unwrap_attributes(value) {
        Value::Nil => Ok(()),
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => {
            for value in values {
                collect_vector_names(value, names)?;
            }
            Ok(())
        }
        value => {
            let name = text_value(value)?.ok_or(AppError::CommandFailed)?;
            if name.is_empty() {
                return Err(AppError::CommandFailed);
            }
            names.push(name);
            Ok(())
        }
    }
}

fn parse_attributes(value: Value) -> Result<Option<serde_json::Value>, AppError> {
    let value = unwrap_attributes(value);
    let Some(text) = text_value(value.clone())? else {
        return Ok(None);
    };
    if text.len() > MAX_VECTOR_ATTRIBUTE_BYTES {
        return Err(AppError::CommandFailed);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| AppError::CommandFailed)
}

fn serialize_attributes(value: &serde_json::Value) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value).map_err(|_| AppError::InvalidInput)?;
    if bytes.len() > MAX_VECTOR_ATTRIBUTE_BYTES {
        return Err(AppError::InvalidInput);
    }
    String::from_utf8(bytes).map_err(|_| AppError::InvalidInput)
}

fn decimal_u64(value: &str) -> Result<Option<u64>, AppError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<u64>()
        .map(Some)
        .map_err(|_| AppError::CommandFailed)
}

fn number_value(value: Value) -> Result<f64, AppError> {
    let value = text_value(value)?.ok_or(AppError::CommandFailed)?;
    let value = value.parse::<f64>().map_err(|_| AppError::CommandFailed)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AppError::CommandFailed)
    }
}

fn text_value(value: Value) -> Result<Option<String>, AppError> {
    match unwrap_attributes(value) {
        Value::Nil => Ok(None),
        Value::BulkString(value) => String::from_utf8(value)
            .map(Some)
            .map_err(|_| AppError::CommandFailed),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => Ok(Some(value)),
        Value::Int(value) => Ok(Some(value.to_string())),
        Value::Double(value) if value.is_finite() => Ok(Some(value.to_string())),
        Value::Boolean(value) => Ok(Some(value.to_string())),
        Value::BigNumber(value) => Ok(Some(value.to_string())),
        _ => Err(AppError::CommandFailed),
    }
}

fn validate_key_and_element(key: &str, element: &str) -> Result<(), AppError> {
    if key.trim().is_empty() || element.trim().is_empty() {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

fn unwrap_attributes(mut value: Value) -> Value {
    while let Value::Attribute { data, .. } = value {
        value = *data;
    }
    value
}

fn is_pair_container(value: &Value) -> bool {
    matches!(
        unwrap_attributes(value.clone()),
        Value::Array(values) | Value::Set(values) if values.len() >= 2
    )
}

fn ensure_response_size(value: &Value) -> Result<(), AppError> {
    if redis_value_size(value) > MAX_VECTOR_RESPONSE_BYTES {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

fn redis_value_size(value: &Value) -> usize {
    match value {
        Value::Nil | Value::Okay => 0,
        Value::Int(_) | Value::Double(_) | Value::Boolean(_) => 8,
        Value::BulkString(value) => value.len(),
        Value::SimpleString(value) => value.len(),
        Value::VerbatimString { text, .. } => text.len(),
        Value::BigNumber(value) => value.to_string().len(),
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => values
            .iter()
            .map(redis_value_size)
            .sum::<usize>()
            .saturating_add(values.len() * 8),
        Value::Map(entries) => entries
            .iter()
            .map(|(key, value)| redis_value_size(key).saturating_add(redis_value_size(value)))
            .sum::<usize>()
            .saturating_add(entries.len() * 16),
        Value::Attribute { data, attributes } => redis_value_size(data).saturating_add(
            attributes
                .iter()
                .map(|(key, value)| redis_value_size(key).saturating_add(redis_value_size(value)))
                .sum(),
        ),
        Value::ServerError(value) => value.to_string().len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use ::redis::Value;

    use crate::domain::{
        DeleteVectorSetElementsInput, ListVectorSetElementsInput, SetVectorSetAttributesInput,
        VectorSetElementPayload, VectorSimilarityQueryInput,
    };

    use super::{
        build_vadd_command, build_vrange_command, build_vrem_command, build_vsetattr_command,
        build_vsim_command, decode_fp32_base64, encode_fp32_base64, parse_vector_set_element,
        parse_vector_set_info, parse_vector_set_page, parse_vsim_reply,
    };

    fn text(value: &str) -> Value {
        Value::BulkString(value.as_bytes().to_vec())
    }

    #[test]
    fn fp32_round_trip_uses_little_endian_and_rejects_non_finite_values() {
        let encoded = encode_fp32_base64(&[1.0, -2.5]).unwrap();
        assert_eq!(decode_fp32_base64(&encoded).unwrap(), vec![1.0, -2.5]);
        assert_eq!(
            encode_fp32_base64(&[f64::NAN]).unwrap_err(),
            crate::error::AppError::InvalidInput
        );
    }

    #[test]
    fn parses_flat_vsim_matches_with_optional_attributes() {
        let reply = Value::Array(vec![
            text("one"),
            Value::Double(0.95),
            text(r#"{"city":"shanghai"}"#),
            text("two"),
            Value::Double(0.81),
            Value::Nil,
        ]);
        let matches = parse_vsim_reply(reply, true).unwrap();
        assert_eq!(matches[0].name, "one");
        assert_eq!(
            matches[0].attributes,
            Some(serde_json::json!({"city":"shanghai"}))
        );
        assert_eq!(matches[1].attributes, None);
    }

    #[test]
    fn parses_resp2_resp3_vector_info_and_page_names() {
        let resp2 = Value::Array(vec![
            text("quant-type"),
            text("int8"),
            text("vector-dim"),
            Value::Int(3),
        ]);
        assert_eq!(
            parse_vector_set_info(resp2).unwrap(),
            (Some(3), Some("int8".into()))
        );

        let resp3 = Value::Attribute {
            data: Box::new(Value::Map(vec![
                (text("vector-dim"), Value::Int(2)),
                (text("quant-type"), text("f32")),
            ])),
            attributes: vec![],
        };
        assert_eq!(
            parse_vector_set_info(resp3).unwrap(),
            (Some(2), Some("f32".into()))
        );

        let names =
            parse_vector_set_page(Value::Set(vec![text("alpha"), text("beta")]), 2).unwrap();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn parses_vector_element_and_builds_binary_safe_commands() {
        let vector = Value::Array(vec![Value::Double(1.0), Value::Int(-2)]);
        let element =
            parse_vector_set_element(vector, text(r#"{"city":"shanghai"}"#), "one").unwrap();
        assert_eq!(
            element.vector_base64,
            Some(encode_fp32_base64(&[1.0, -2.0]).unwrap())
        );
        assert_eq!(
            element.attributes,
            Some(serde_json::json!({"city":"shanghai"}))
        );

        let vadd = build_vadd_command(
            "vectors",
            &VectorSetElementPayload {
                name: "one".into(),
                vector_values: Some(vec![1.0, -2.0]),
                vector_fp32_base64: None,
                attributes: Some(serde_json::json!({"city":"shanghai"})),
            },
            Some(2),
        )
        .unwrap();
        let packed = String::from_utf8(vadd.get_packed_command()).unwrap();
        assert!(packed.contains("$4\r\nVADD\r\n"));
        assert!(packed.contains("$6\r\nVALUES\r\n"));
        assert!(packed.contains("$7\r\nSETATTR\r\n"));

        let vsim = build_vsim_command(&VectorSimilarityQueryInput {
            connection_id: "local".into(),
            key: "vectors".into(),
            by_element: Some("one".into()),
            by_vector: None,
            by_vector_base64: None,
            count: 5,
            with_attributes: true,
        })
        .unwrap();
        assert!(String::from_utf8(vsim.get_packed_command())
            .unwrap()
            .contains("$11\r\nWITHATTRIBS\r\n"));

        let vrange = build_vrange_command(&ListVectorSetElementsInput {
            connection_id: "local".into(),
            key: "vectors".into(),
            start: None,
            end: None,
            limit: 10,
        })
        .unwrap();
        assert!(String::from_utf8(vrange.get_packed_command())
            .unwrap()
            .contains("$6\r\nVRANGE\r\n"));

        let vsetattr = build_vsetattr_command(&SetVectorSetAttributesInput {
            connection_id: "local".into(),
            key: "vectors".into(),
            element: "one".into(),
            attributes: serde_json::json!({"ok": true}),
        })
        .unwrap();
        assert!(String::from_utf8(vsetattr.get_packed_command())
            .unwrap()
            .contains("$8\r\nVSETATTR\r\n"));

        let vrem = build_vrem_command(&DeleteVectorSetElementsInput {
            connection_id: "local".into(),
            key: "vectors".into(),
            elements: vec!["one".into(), "two".into()],
        })
        .unwrap();
        assert!(String::from_utf8(vrem.get_packed_command())
            .unwrap()
            .contains("$4\r\nVREM\r\n"));
    }
}
