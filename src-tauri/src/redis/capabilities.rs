use std::collections::HashSet;

use ::redis::{Cmd, Value};

use crate::{
    domain::module_capabilities::{ARRAY_REQUIRED_COMMANDS, VECTOR_SET_REQUIRED_COMMANDS},
    error::AppError,
};

pub(crate) const ARRAY_COMMANDS: &[&str] = ARRAY_REQUIRED_COMMANDS;
pub(crate) const VECTOR_SET_COMMANDS: &[&str] = VECTOR_SET_REQUIRED_COMMANDS;

pub(crate) fn build_command_info_command() -> Cmd {
    let mut command = ::redis::cmd("COMMAND");
    command.arg("INFO");
    for name in ARRAY_COMMANDS.iter().chain(VECTOR_SET_COMMANDS.iter()) {
        command.arg(*name);
    }
    command
}

pub(crate) fn parse_command_info(value: Value) -> Result<HashSet<String>, AppError> {
    let mut commands = HashSet::new();
    collect_command_names(value, &mut commands)?;
    Ok(commands)
}

pub(crate) fn is_array_command_set_supported(commands: &HashSet<String>) -> bool {
    has_all_commands(commands, ARRAY_COMMANDS)
}

pub(crate) fn is_vector_set_command_set_supported(commands: &HashSet<String>) -> bool {
    has_all_commands(commands, VECTOR_SET_COMMANDS)
}

pub(crate) fn parse_vector_set_info_summary(
    value: Value,
) -> Result<(Option<u32>, Option<String>), AppError> {
    let mut dimension = None;
    let mut quantization = None;
    collect_vector_info_fields(value, &mut dimension, &mut quantization)?;
    Ok((dimension, quantization))
}

fn collect_vector_info_fields(
    value: Value,
    dimension: &mut Option<u32>,
    quantization: &mut Option<String>,
) -> Result<(), AppError> {
    match value {
        Value::Nil => Ok(()),
        Value::ServerError(_) => Err(AppError::CommandFailed),
        Value::Attribute { data, attributes } => {
            collect_vector_info_fields(*data, dimension, quantization)?;
            for (key, value) in attributes {
                collect_vector_info_pair(key, value, dimension, quantization)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_vector_info_pair(key, value, dimension, quantization)?;
            }
            Ok(())
        }
        Value::Array(values) | Value::Set(values) => {
            if values.len() % 2 == 0 {
                for pair in values.chunks_exact(2) {
                    collect_vector_info_pair(
                        pair[0].clone(),
                        pair[1].clone(),
                        dimension,
                        quantization,
                    )?;
                }
            }
            for value in values {
                collect_vector_info_fields(value, dimension, quantization)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn collect_vector_info_pair(
    key: Value,
    value: Value,
    dimension: &mut Option<u32>,
    quantization: &mut Option<String>,
) -> Result<(), AppError> {
    let Some(key) = value_as_text(&key) else {
        return Ok(());
    };
    match key.trim().to_ascii_lowercase().as_str() {
        "dimension" | "vector_dim" | "dim" => {
            if let Some(value) = value_as_text(&value) {
                *dimension = Some(value.parse().map_err(|_| AppError::CommandFailed)?);
            }
        }
        "quantization" | "quantization_type" | "type" => {
            if let Some(value) = value_as_text(&value) {
                *quantization = Some(value);
            }
        }
        _ => {}
    }
    Ok(())
}

fn value_as_text(value: &Value) -> Option<String> {
    match value {
        Value::BulkString(bytes) => String::from_utf8(bytes.clone()).ok(),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => {
            Some(value.clone())
        }
        Value::Int(value) => Some(value.to_string()),
        Value::Double(value) => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        Value::BigNumber(value) => Some(value.to_string()),
        Value::Attribute { data, .. } => value_as_text(data),
        _ => None,
    }
}

fn collect_command_names(value: Value, commands: &mut HashSet<String>) -> Result<(), AppError> {
    match value {
        Value::Nil => Ok(()),
        Value::ServerError(_) => Err(AppError::CommandFailed),
        Value::Attribute { data, attributes } => {
            collect_command_names(*data, commands)?;
            for (key, value) in attributes {
                collect_command_names(key, commands)?;
                collect_command_names(value, commands)?;
            }
            Ok(())
        }
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => {
            for value in values {
                collect_command_names(value, commands)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_command_names(key, commands)?;
                collect_command_names(value, commands)?;
            }
            Ok(())
        }
        Value::BulkString(bytes) => collect_candidate(bytes.as_slice(), commands),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => {
            collect_candidate(value.as_bytes(), commands)
        }
        Value::Int(_)
        | Value::Double(_)
        | Value::Boolean(_)
        | Value::Okay
        | Value::BigNumber(_) => Ok(()),
        _ => Ok(()),
    }
}

fn collect_candidate(bytes: &[u8], commands: &mut HashSet<String>) -> Result<(), AppError> {
    let Ok(value) = std::str::from_utf8(bytes) else {
        return Ok(());
    };
    let normalized = value.trim().to_ascii_uppercase();
    if ARRAY_COMMANDS
        .iter()
        .chain(VECTOR_SET_COMMANDS.iter())
        .any(|name| *name == normalized)
    {
        commands.insert(normalized);
    }
    Ok(())
}

fn has_all_commands(commands: &HashSet<String>, required: &[&str]) -> bool {
    required.iter().all(|name| {
        commands
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use ::redis::Value;

    use super::{
        is_array_command_set_supported, is_vector_set_command_set_supported, parse_command_info,
    };

    fn text(value: &str) -> Value {
        Value::SimpleString(value.to_owned())
    }

    #[test]
    fn command_info_parser_accepts_resp2_resp3_nil_and_attribute_entries() {
        let resp2 = Value::Array(vec![
            Value::Array(vec![text("ARLEN"), Value::Int(1)]),
            Value::Nil,
            Value::Array(vec![text("VADD"), Value::Int(1)]),
        ]);
        let parsed = parse_command_info(resp2).unwrap();
        assert!(parsed.contains("ARLEN"));
        assert!(parsed.contains("VADD"));
        assert_eq!(parsed.len(), 2);

        let resp3 = Value::Attribute {
            data: Box::new(Value::Set(vec![Value::Array(vec![
                text("VSIM"),
                Value::Int(1),
            ])])),
            attributes: vec![],
        };
        assert!(parse_command_info(resp3).unwrap().contains("VSIM"));
    }

    #[test]
    fn command_support_requires_the_complete_command_set() {
        assert!(!is_array_command_set_supported(
            &["ARLEN".to_owned(), "ARCOUNT".to_owned()]
                .into_iter()
                .collect()
        ));
        assert!(!is_vector_set_command_set_supported(
            &["VADD".to_owned(), "VCARD".to_owned()]
                .into_iter()
                .collect()
        ));
        let array_commands: HashSet<String> = super::ARRAY_COMMANDS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert!(is_array_command_set_supported(&array_commands));
        let vector_set_commands: HashSet<String> = super::VECTOR_SET_COMMANDS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert!(is_vector_set_command_set_supported(&vector_set_commands));
    }
}
