use ::redis::{aio::MultiplexedConnection, Value};

use crate::{
    domain::{
        normalize_json_path, AppendJsonArrayInput, DeleteJsonPathInput, GetJsonPathInput,
        JsonMutationResult, JsonPathValue, ModuleCapabilities, SetJsonPathInput,
    },
    error::AppError,
};

use super::{
    connection_manager::{map_command_error, map_json_command_error},
    database_analysis::parse_module_list,
};

const MAX_JSON_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn parse_module_capabilities(value: Value) -> ModuleCapabilities {
    let modules = parse_module_list(value)
        .into_iter()
        .map(|mut module| {
            module.version = normalize_module_version(module.version);
            module
        })
        .collect();
    ModuleCapabilities::from_modules(modules)
}

pub(crate) fn json_path_uses_legacy_syntax(capabilities: &ModuleCapabilities) -> bool {
    capabilities
        .json_version
        .as_deref()
        .is_none_or(|version| version.starts_with("1."))
}

pub(crate) async fn read_json_path(
    connection: &mut MultiplexedConnection,
    input: GetJsonPathInput,
    legacy: bool,
) -> Result<JsonPathValue, AppError> {
    let path = normalize_json_path(&input.path, legacy)?;
    let reply = ::redis::cmd("JSON.GET")
        .arg(&input.key)
        .arg(&path)
        .query_async::<Option<String>>(connection)
        .await
        .map_err(map_json_command_error)?;
    let value = parse_json_get_reply(reply, &path, legacy)?;
    let ttl_ms = read_ttl_ms(connection, &input.key).await?;
    Ok(JsonPathValue {
        key: input.key,
        path: input.path,
        value,
        ttl_ms,
    })
}

pub(crate) async fn write_json_path(
    connection: &mut MultiplexedConnection,
    input: SetJsonPathInput,
    legacy: bool,
) -> Result<JsonMutationResult, AppError> {
    let path = normalize_json_path(&input.path, legacy)?;
    let encoded = serde_json::to_string(&input.value).map_err(|_| AppError::InvalidInput)?;
    let reply = ::redis::cmd("JSON.SET")
        .arg(&input.key)
        .arg(&path)
        .arg(encoded)
        .query_async::<Value>(connection)
        .await
        .map_err(map_json_command_error)?;
    let affected = parse_json_set_reply(reply)?;
    let ttl_ms = read_ttl_ms(connection, &input.key).await?;
    Ok(JsonMutationResult {
        key: input.key,
        path: input.path,
        affected,
        new_length: None,
        ttl_ms,
    })
}

pub(crate) async fn append_json_array_path(
    connection: &mut MultiplexedConnection,
    input: AppendJsonArrayInput,
    legacy: bool,
) -> Result<JsonMutationResult, AppError> {
    let path = normalize_json_path(&input.path, legacy)?;
    let mut command = ::redis::cmd("JSON.ARRAPPEND");
    command.arg(&input.key).arg(&path);
    for value in &input.values {
        command.arg(serde_json::to_string(value).map_err(|_| AppError::InvalidInput)?);
    }
    let reply = command
        .query_async::<Value>(connection)
        .await
        .map_err(map_json_command_error)?;
    let (affected, new_length) = parse_json_array_append_reply(reply)?;
    let ttl_ms = read_ttl_ms(connection, &input.key).await?;
    Ok(JsonMutationResult {
        key: input.key,
        path: input.path,
        affected,
        new_length,
        ttl_ms,
    })
}

pub(crate) async fn delete_json_path_value(
    connection: &mut MultiplexedConnection,
    input: DeleteJsonPathInput,
    legacy: bool,
) -> Result<JsonMutationResult, AppError> {
    let path = normalize_json_path(&input.path, legacy)?;
    let affected = ::redis::cmd("JSON.DEL")
        .arg(&input.key)
        .arg(&path)
        .query_async::<i64>(connection)
        .await
        .map_err(map_json_command_error)?;
    let ttl_ms = read_ttl_ms(connection, &input.key).await?;
    Ok(JsonMutationResult {
        key: input.key,
        path: input.path,
        affected: u64::try_from(affected).map_err(|_| AppError::CommandFailed)?,
        new_length: None,
        ttl_ms,
    })
}

pub(crate) fn parse_json_get_reply(
    reply: Option<String>,
    path: &str,
    legacy: bool,
) -> Result<Option<serde_json::Value>, AppError> {
    let Some(raw) = reply else {
        return Ok(None);
    };
    if raw.len() > MAX_JSON_RESPONSE_BYTES {
        return Err(AppError::CommandFailed);
    }

    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| AppError::CommandFailed)?;
    if legacy || !path.starts_with('$') {
        return Ok(Some(value));
    }

    match value {
        serde_json::Value::Array(values) if values.is_empty() => Ok(None),
        serde_json::Value::Array(mut values) if values.len() == 1 => Ok(Some(values.remove(0))),
        value => Ok(Some(value)),
    }
}

fn parse_json_set_reply(reply: Value) -> Result<u64, AppError> {
    match reply {
        Value::Okay => Ok(1),
        Value::SimpleString(value) if value.eq_ignore_ascii_case("OK") => Ok(1),
        Value::BulkString(value) if value.eq_ignore_ascii_case(b"OK") => Ok(1),
        Value::Nil => Ok(0),
        Value::Attribute { data, .. } => parse_json_set_reply(*data),
        _ => Err(AppError::CommandFailed),
    }
}

pub(crate) fn parse_json_array_append_reply(reply: Value) -> Result<(u64, Option<u64>), AppError> {
    match reply {
        Value::Int(value) => Ok((
            1,
            Some(u64::try_from(value).map_err(|_| AppError::CommandFailed)?),
        )),
        Value::Array(values) | Value::Set(values) => parse_json_array_append_values(values),
        Value::Attribute { data, .. } => parse_json_array_append_reply(*data),
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_json_array_append_values(values: Vec<Value>) -> Result<(u64, Option<u64>), AppError> {
    let mut affected = 0_u64;
    let mut new_length = None;
    for value in values {
        if let Value::Attribute { data, .. } = value {
            let (nested_affected, nested_length) = parse_json_array_append_reply(*data)?;
            affected = affected.saturating_add(nested_affected);
            if nested_length.is_some() {
                new_length = nested_length;
            }
            continue;
        }
        if let Value::Int(length) = value {
            affected = affected.saturating_add(1);
            new_length = Some(u64::try_from(length).map_err(|_| AppError::CommandFailed)?);
        }
    }
    Ok((affected, new_length))
}

async fn read_ttl_ms(connection: &mut MultiplexedConnection, key: &str) -> Result<i64, AppError> {
    ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)
}

fn normalize_module_version(version: Option<String>) -> Option<String> {
    let version = version?;
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(trimmed.to_owned());
    }
    let encoded = trimmed.parse::<u64>().ok()?;
    Some(format!(
        "{}.{}.{}",
        encoded / 10_000,
        (encoded / 100) % 100,
        encoded % 100
    ))
}

#[cfg(test)]
mod tests {
    use ::redis::Value;

    use crate::error::AppError;

    use super::*;

    #[test]
    fn parses_resp2_and_resp3_module_lists_for_json_capability() {
        let resp2 = Value::Array(vec![Value::Array(vec![
            Value::BulkString(b"name".to_vec()),
            Value::BulkString(b"ReJSON".to_vec()),
            Value::BulkString(b"ver".to_vec()),
            Value::Int(20000),
        ])]);
        let resp3 = Value::Attribute {
            data: Box::new(Value::Array(vec![Value::Map(vec![
                (
                    Value::SimpleString("name".into()),
                    Value::SimpleString("RedisJSON".into()),
                ),
                (Value::SimpleString("ver".into()), Value::Int(20000)),
            ])])),
            attributes: vec![],
        };

        let resp2_capabilities = parse_module_capabilities(resp2);
        let resp3_capabilities = parse_module_capabilities(resp3);

        assert!(resp2_capabilities.json_supported);
        assert_eq!(resp2_capabilities.json_version.as_deref(), Some("2.0.0"));
        assert!(resp3_capabilities.json_supported);
        assert_eq!(resp3_capabilities.json_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn normalizes_json_get_root_reply_without_unwrapping_nested_arrays() {
        let object_reply = Some(r#"[{"name":"redix"}]"#.to_owned());
        let object = parse_json_get_reply(object_reply, "$", false)
            .unwrap()
            .unwrap();
        assert_eq!(object, serde_json::json!({"name": "redix"}));

        let array_reply = Some(r#"[[{"name":"redix"}]]"#.to_owned());
        let array = parse_json_get_reply(array_reply, "$", false)
            .unwrap()
            .unwrap();
        assert_eq!(array, serde_json::json!([{"name": "redix"}]));
    }

    #[test]
    fn missing_json_reply_is_not_reported_as_a_server_error() {
        assert_eq!(
            parse_json_get_reply(None, "$.missing", false).unwrap(),
            None
        );
    }

    #[test]
    fn malformed_json_get_reply_maps_to_command_failed() {
        let error = parse_json_get_reply(Some("{".to_owned()), "$", false).unwrap_err();

        assert_eq!(error, AppError::CommandFailed);
    }

    #[test]
    fn parses_modern_and_legacy_json_array_append_replies() {
        assert_eq!(
            parse_json_array_append_reply(Value::Array(vec![Value::Int(4)])).unwrap(),
            (1, Some(4))
        );
        assert_eq!(
            parse_json_array_append_reply(Value::Int(5)).unwrap(),
            (1, Some(5))
        );
    }
}
