use ::redis::{aio::MultiplexedConnection, Value};

use crate::{
    domain::{
        parse_info_sections, AnalysisAccumulator, AnalysisKeyMetadata, AnalyzeDatabaseInput,
        DatabaseAnalysisReport, InstanceDetails, ModuleSummary,
    },
    error::AppError,
};

use super::connection_manager::map_command_error;

pub(crate) async fn load_instance_details(
    connection: &mut MultiplexedConnection,
) -> Result<InstanceDetails, AppError> {
    let info = ::redis::cmd("INFO")
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    let sections = parse_info_sections(&info);
    let modules = ::redis::cmd("MODULE")
        .arg("LIST")
        .query_async::<Value>(connection)
        .await
        .map(parse_module_list)
        .unwrap_or_default();
    InstanceDetails::from_info_and_modules(&sections, modules)
}

pub(crate) async fn analyze_connection(
    connection: &mut MultiplexedConnection,
    database: u8,
    input: &AnalyzeDatabaseInput,
) -> Result<DatabaseAnalysisReport, AppError> {
    let mut cursor = 0_u64;
    let mut scanned = 0_u64;
    let mut processed = 0_u64;
    let mut accumulator = AnalysisAccumulator::new(
        database,
        input.pattern.clone(),
        input.delimiter.clone(),
        input.max_keys,
    );

    loop {
        let (next_cursor, keys): (u64, Vec<String>) = ::redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&input.pattern)
            .arg("COUNT")
            .arg(500_i64)
            .query_async(connection)
            .await
            .map_err(map_command_error)?;
        scanned += keys.len() as u64;
        let remaining = input.max_keys.saturating_sub(processed);
        let batch = keys
            .into_iter()
            .take(remaining as usize)
            .collect::<Vec<_>>();
        let metadata = load_key_metadata(connection, &batch).await?;
        for item in metadata {
            if item.key_type.is_empty() {
                continue;
            }
            accumulator.process(item);
            processed += 1;
        }
        if processed >= input.max_keys && next_cursor != 0 {
            return Ok(accumulator.finish(scanned, processed, true));
        }
        cursor = next_cursor;
        if cursor == 0 {
            return Ok(accumulator.finish(scanned, processed, false));
        }
    }
}

async fn load_key_metadata(
    connection: &mut MultiplexedConnection,
    keys: &[String],
) -> Result<Vec<AnalysisKeyMetadata>, AppError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut metadata_pipeline = ::redis::pipe();
    for key in keys {
        metadata_pipeline
            .cmd("TYPE")
            .arg(key)
            .cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .cmd("TTL")
            .arg(key);
    }
    let replies = metadata_pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;

    let mut metadata = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let reply_index = index * 3;
            AnalysisKeyMetadata {
                key: key.clone(),
                key_type: value_as_string(replies.get(reply_index)).unwrap_or_default(),
                length: None,
                memory_bytes: value_as_optional(replies.get(reply_index + 1)),
                ttl_seconds: value_as_i64(replies.get(reply_index + 2)).unwrap_or(-2),
            }
        })
        .collect::<Vec<_>>();

    let mut length_pipeline = ::redis::pipe();
    let mut length_indexes = Vec::new();
    for (index, item) in metadata.iter().enumerate() {
        let Some(command) = length_command(&item.key_type) else {
            continue;
        };
        length_pipeline.cmd(command).arg(&item.key);
        length_indexes.push(index);
    }
    if length_indexes.is_empty() {
        return Ok(metadata);
    }

    let replies = length_pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;
    for (index, reply) in length_indexes.into_iter().zip(replies.iter()) {
        metadata[index].length = value_as_optional(Some(reply));
    }
    Ok(metadata)
}

fn length_command(key_type: &str) -> Option<&'static str> {
    match key_type {
        "string" => Some("STRLEN"),
        "list" => Some("LLEN"),
        "hash" => Some("HLEN"),
        "set" => Some("SCARD"),
        "zset" => Some("ZCARD"),
        "stream" => Some("XLEN"),
        _ => None,
    }
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| ::redis::from_redis_value_ref::<String>(value).ok())
}

fn value_as_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|value| ::redis::from_redis_value_ref::<i64>(value).ok())
}

fn value_as_optional(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        ::redis::from_redis_value_ref::<Option<u64>>(value)
            .ok()
            .flatten()
    })
}

pub(crate) fn parse_module_list(value: Value) -> Vec<ModuleSummary> {
    let entries = match value {
        Value::Array(entries) | Value::Set(entries) => entries,
        Value::Attribute { data, .. } => return parse_module_list(*data),
        _ => return Vec::new(),
    };

    entries.into_iter().filter_map(parse_module_entry).collect()
}

pub(crate) fn parse_module_entry(value: Value) -> Option<ModuleSummary> {
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
        }
        Value::Attribute { data, .. } => return parse_module_entry(*data),
        _ => return None,
    }

    name.filter(|name| !name.trim().is_empty())
        .map(|name| ModuleSummary {
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
    let Some(key) = ::redis::from_redis_value_ref::<String>(key).ok() else {
        return;
    };
    match key.as_str() {
        "name" => *name = ::redis::from_redis_value_ref::<String>(value).ok(),
        "ver" | "version" => *version = ::redis::from_redis_value_ref::<String>(value).ok(),
        _ => {}
    }
}
