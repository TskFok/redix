use ::redis::{aio::MultiplexedConnection, Value};

use crate::{
    domain::{
        normalize_json_path, AppendJsonArrayInput, DeleteJsonPathInput, GetJsonPathInput,
        JsonMutationResult, JsonPathValue, ModuleCapabilities, SetJsonPathInput,
    },
    error::AppError,
};

use super::connection_manager::{map_command_error, map_json_command_error};

const MAX_JSON_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const UNKNOWN_TTL_MS: i64 = -3;

#[derive(Debug, PartialEq)]
pub(crate) struct ParsedJsonGetReply {
    pub found: bool,
    pub value: Option<serde_json::Value>,
}

pub(crate) fn parse_module_capabilities(value: Value) -> Result<ModuleCapabilities, AppError> {
    let entries = module_entries(value)?;
    let mut modules = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut module = parse_module_entry(entry)?;
        module.version = normalize_module_version(module.version);
        modules.push(module);
    }
    Ok(ModuleCapabilities::from_modules(modules))
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
        .query_async::<Value>(connection)
        .await
        .map_err(map_json_command_error)?;
    let parsed = parse_json_get_reply(reply, &path, legacy)?;
    let ttl_ms = read_ttl_ms(connection, &input.key).await?;
    Ok(JsonPathValue {
        key: input.key,
        path: input.path,
        found: parsed.found,
        value: parsed.value,
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
    let ttl_ms = read_mutation_ttl_ms(connection, &input.key).await;
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
    let ttl_ms = read_mutation_ttl_ms(connection, &input.key).await;
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
    let ttl_ms = read_mutation_ttl_ms(connection, &input.key).await;
    Ok(JsonMutationResult {
        key: input.key,
        path: input.path,
        affected: u64::try_from(affected).map_err(|_| AppError::CommandFailed)?,
        new_length: None,
        ttl_ms,
    })
}

pub(crate) fn parse_json_get_reply(
    reply: Value,
    path: &str,
    legacy: bool,
) -> Result<ParsedJsonGetReply, AppError> {
    let value = parse_json_get_reply_value(reply)?;
    let Some(value) = value else {
        return Ok(ParsedJsonGetReply {
            found: false,
            value: None,
        });
    };

    if legacy || !path.starts_with('$') {
        return Ok(ParsedJsonGetReply {
            found: true,
            value: Some(value),
        });
    }

    match value {
        serde_json::Value::Array(values) if values.is_empty() => Ok(ParsedJsonGetReply {
            found: false,
            value: None,
        }),
        serde_json::Value::Array(mut values) if values.len() == 1 => Ok(ParsedJsonGetReply {
            found: true,
            value: Some(values.remove(0)),
        }),
        value => Ok(ParsedJsonGetReply {
            found: true,
            value: Some(value),
        }),
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

async fn read_mutation_ttl_ms(connection: &mut MultiplexedConnection, key: &str) -> i64 {
    resolve_mutation_ttl_ms(read_ttl_ms(connection, key).await)
}

fn resolve_mutation_ttl_ms(ttl: Result<i64, AppError>) -> i64 {
    ttl.unwrap_or(UNKNOWN_TTL_MS)
}

fn parse_json_get_reply_value(reply: Value) -> Result<Option<serde_json::Value>, AppError> {
    match reply {
        Value::Nil => Ok(None),
        Value::BulkString(bytes) => parse_json_bytes(bytes),
        Value::SimpleString(text) => parse_json_text(text),
        Value::Attribute { data, .. } => parse_json_get_reply_value(*data),
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_json_bytes(bytes: Vec<u8>) -> Result<Option<serde_json::Value>, AppError> {
    if bytes.len() > MAX_JSON_RESPONSE_BYTES {
        return Err(AppError::CommandFailed);
    }

    let text = String::from_utf8(bytes).map_err(|_| AppError::CommandFailed)?;
    parse_json_text(text)
}

fn parse_json_text(text: String) -> Result<Option<serde_json::Value>, AppError> {
    if text.len() > MAX_JSON_RESPONSE_BYTES {
        return Err(AppError::CommandFailed);
    }

    let value = serde_json::from_str(&text).map_err(|_| AppError::CommandFailed)?;
    Ok(Some(value))
}

fn module_entries(value: Value) -> Result<Vec<Value>, AppError> {
    match value {
        Value::Array(entries) | Value::Set(entries) => Ok(entries),
        Value::Attribute { data, .. } => module_entries(*data),
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_module_entry(value: Value) -> Result<crate::domain::ModuleSummary, AppError> {
    let mut name = None;
    let mut version = None;

    match value {
        Value::Map(entries) => {
            for (key, value) in entries {
                update_module_field(&key, &value, &mut name, &mut version);
            }
        }
        Value::Array(entries) => {
            let mut pairs = entries.chunks_exact(2);
            for pair in &mut pairs {
                update_module_field(&pair[0], &pair[1], &mut name, &mut version);
            }
            if !pairs.remainder().is_empty() {
                return Err(AppError::CommandFailed);
            }
        }
        Value::Attribute { data, .. } => return parse_module_entry(*data),
        _ => return Err(AppError::CommandFailed),
    }

    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return Err(AppError::CommandFailed);
    };

    Ok(crate::domain::ModuleSummary {
        name,
        version: version.filter(|version| !version.trim().is_empty()),
    })
}

fn update_module_field(
    key: &Value,
    value: &Value,
    name: &mut Option<String>,
    version: &mut Option<String>,
) {
    let Some(key) = value_as_string(key) else {
        return;
    };

    match key.as_str() {
        "name" => *name = value_as_string(value),
        "ver" | "version" => *version = value_as_string(value),
        _ => {}
    }
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::BulkString(bytes) => String::from_utf8(bytes.clone()).ok(),
        Value::SimpleString(text) => Some(text.clone()),
        Value::Int(number) => Some(number.to_string()),
        Value::Attribute { data, .. } => value_as_string(data),
        _ => None,
    }
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

        let resp2_capabilities = parse_module_capabilities(resp2).unwrap();
        let resp3_capabilities = parse_module_capabilities(resp3).unwrap();

        assert!(resp2_capabilities.json_supported);
        assert_eq!(resp2_capabilities.json_version.as_deref(), Some("2.0.0"));
        assert!(resp3_capabilities.json_supported);
        assert_eq!(resp3_capabilities.json_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn empty_module_list_is_a_valid_no_modules_result() {
        let capabilities = parse_module_capabilities(Value::Array(vec![])).unwrap();

        assert_eq!(capabilities.modules, vec![]);
        assert!(!capabilities.json_supported);
        assert_eq!(capabilities.json_version, None);
    }

    #[test]
    fn malformed_module_list_top_level_and_entries_map_to_command_failed() {
        assert_eq!(
            parse_module_capabilities(Value::BulkString(b"oops".to_vec())).unwrap_err(),
            AppError::CommandFailed
        );
        assert_eq!(
            parse_module_capabilities(Value::Array(vec![Value::BulkString(b"oops".to_vec())]))
                .unwrap_err(),
            AppError::CommandFailed
        );
        assert_eq!(
            parse_module_capabilities(Value::Array(vec![Value::Array(vec![
                Value::BulkString(b"name".to_vec()),
                Value::BulkString(b"RedisJSON".to_vec()),
                Value::BulkString(b"ver".to_vec()),
            ])]))
            .unwrap_err(),
            AppError::CommandFailed
        );
    }

    #[test]
    fn module_list_entries_require_a_name_but_ignore_unknown_fields() {
        let with_unknown_fields = Value::Array(vec![Value::Map(vec![
            (
                Value::SimpleString("name".into()),
                Value::SimpleString("RedisJSON".into()),
            ),
            (Value::SimpleString("ver".into()), Value::Int(20000)),
            (
                Value::SimpleString("extra".into()),
                Value::SimpleString("ignored".into()),
            ),
        ])]);
        let capabilities = parse_module_capabilities(with_unknown_fields).unwrap();
        assert!(capabilities.json_supported);

        let missing_name = Value::Array(vec![Value::Map(vec![(
            Value::SimpleString("ver".into()),
            Value::Int(20000),
        )])]);
        assert_eq!(
            parse_module_capabilities(missing_name).unwrap_err(),
            AppError::CommandFailed
        );
    }

    #[test]
    fn normalizes_json_get_root_reply_without_unwrapping_nested_arrays() {
        let object_reply = Value::BulkString(br#"[{"name":"redix"}]"#.to_vec());
        let object = parse_json_get_reply(object_reply, "$", false)
            .unwrap()
            .value;
        assert_eq!(object, Some(serde_json::json!({"name": "redix"})));

        let array_reply = Value::BulkString(br#"[[{"name":"redix"}]]"#.to_vec());
        let array = parse_json_get_reply(array_reply, "$", false).unwrap().value;
        assert_eq!(array, Some(serde_json::json!([{"name": "redix"}])));
    }

    #[test]
    fn json_get_distinguishes_missing_path_from_json_null() {
        let missing = parse_json_get_reply(Value::Nil, "$.missing", false).unwrap();
        assert!(!missing.found);
        assert_eq!(missing.value, None);

        let null_value =
            parse_json_get_reply(Value::BulkString(b"[null]".to_vec()), "$", false).unwrap();
        assert!(null_value.found);
        assert_eq!(null_value.value, Some(serde_json::Value::Null));
    }

    #[test]
    fn json_get_response_size_limit_accepts_exact_boundary_and_rejects_overflow() {
        let exact = format!("\"{}\"", "x".repeat((4 * 1024 * 1024) - 2)).into_bytes();
        let exact_value = parse_json_get_reply(Value::BulkString(exact), "$", false).unwrap();
        assert!(exact_value.found);
        assert_eq!(
            exact_value.value,
            Some(serde_json::Value::String("x".repeat((4 * 1024 * 1024) - 2)))
        );

        let overflow = format!("\"{}\"", "x".repeat((4 * 1024 * 1024) - 1)).into_bytes();
        assert_eq!(
            parse_json_get_reply(Value::BulkString(overflow), "$", false).unwrap_err(),
            AppError::CommandFailed
        );
    }

    #[test]
    fn malformed_json_get_reply_maps_to_command_failed() {
        let error = parse_json_get_reply(Value::BulkString(b"{".to_vec()), "$", false).unwrap_err();

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

    #[test]
    fn json_array_append_tracks_all_affected_matches() {
        assert_eq!(
            parse_json_array_append_reply(Value::Array(vec![Value::Int(4), Value::Int(5)]))
                .unwrap(),
            (2, Some(5))
        );
    }

    #[test]
    fn pttl_failures_after_mutation_fall_back_to_unknown_ttl() {
        assert_eq!(resolve_mutation_ttl_ms(Ok(-1)), -1);
        assert_eq!(
            resolve_mutation_ttl_ms(Err(AppError::ConnectionFailed)),
            UNKNOWN_TTL_MS
        );
    }
}
