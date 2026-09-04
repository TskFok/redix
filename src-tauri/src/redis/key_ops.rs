use crate::{
    domain::{
        normalize_key_type, HashEntry, KeySummary, RedisValue, SortedSetEntry, StreamEntry,
        StreamField,
    },
    error::AppError,
};

use super::{
    connection_manager::{key_size, map_command_error},
    RoutedConnection,
};

const MAX_CLUSTER_SCAN_PAGE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn ensure_cluster_scan_page_size(
    page: &crate::domain::ScanPage,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(page).map_err(|_| AppError::InvalidInput)?;
    if bytes.len() > MAX_CLUSTER_SCAN_PAGE_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

pub(crate) async fn load_key_summaries(
    connection: &mut RoutedConnection,
    keys: Vec<Vec<u8>>,
    requested_type: Option<&str>,
) -> Result<Vec<KeySummary>, AppError> {
    let mut summaries = Vec::with_capacity(keys.len());
    for raw_key in keys {
        let key = String::from_utf8(raw_key.clone()).map_err(|_| AppError::InvalidInput)?;
        let key_type: String = ::redis::cmd("TYPE")
            .arg(&raw_key)
            .query_async(connection)
            .await
            .map_err(map_command_error)?;
        if requested_type.is_some() && normalize_key_type(&key_type) != requested_type {
            continue;
        }
        let ttl_ms: i64 = ::redis::cmd("PTTL")
            .arg(&raw_key)
            .query_async(connection)
            .await
            .map_err(map_command_error)?;
        let size = key_size(connection, &key, &key_type).await.ok().flatten();
        let memory_bytes = ::redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(&raw_key)
            .query_async::<Option<u64>>(connection)
            .await
            .ok()
            .flatten();
        let encoding = ::redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(&raw_key)
            .query_async::<Option<String>>(connection)
            .await
            .ok()
            .flatten();
        let idle_seconds = ::redis::cmd("OBJECT")
            .arg("IDLETIME")
            .arg(&raw_key)
            .query_async::<Option<u64>>(connection)
            .await
            .ok()
            .flatten();
        summaries.push(KeySummary {
            key,
            key_type,
            ttl_ms,
            size,
            memory_bytes,
            encoding,
            idle_seconds,
        });
    }
    Ok(summaries)
}

pub fn decode_stream_entry(id: &str, entries: Vec<String>) -> Result<StreamEntry, AppError> {
    if id.trim().is_empty() || entries.is_empty() || entries.len() % 2 != 0 {
        return Err(AppError::CommandFailed);
    }

    let fields = entries
        .chunks_exact(2)
        .map(|pair| {
            if pair[0].trim().is_empty() {
                return Err(AppError::CommandFailed);
            }
            Ok(StreamField {
                field: pair[0].clone(),
                value: pair[1].clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let entry = StreamEntry {
        id: id.to_owned(),
        fields,
    };
    RedisValue::Stream {
        entries: vec![entry.clone()],
    }
    .validate()?;
    Ok(entry)
}

pub fn encode_stream_entry(entry: &StreamEntry) -> Result<Vec<String>, AppError> {
    RedisValue::Stream {
        entries: vec![entry.clone()],
    }
    .validate()?;

    let mut values = Vec::with_capacity(entry.fields.len() * 2 + 1);
    values.push(entry.id.clone());
    for field in &entry.fields {
        values.push(field.field.clone());
        values.push(field.value.clone());
    }
    Ok(values)
}

pub fn decode_json_value(raw: &str) -> Result<serde_json::Value, AppError> {
    serde_json::from_str(raw).map_err(|_| AppError::CommandFailed)
}

pub fn encode_json_value(value: &serde_json::Value) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| AppError::CommandFailed)
}

pub fn decode_key_value(key_type: &str, entries: Vec<String>) -> Result<RedisValue, AppError> {
    match key_type {
        "string" => match entries.as_slice() {
            [value] => Ok(RedisValue::String {
                value: value.clone(),
            }),
            _ => Err(AppError::CommandFailed),
        },
        "hash" => entries
            .into_iter()
            .map(|entry| {
                let (field, value) = entry.split_once('=').ok_or(AppError::CommandFailed)?;
                Ok(HashEntry {
                    field: field.to_owned(),
                    value: value.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(|fields| RedisValue::Hash { fields }),
        "list" => Ok(RedisValue::List { items: entries }),
        "set" => Ok(RedisValue::Set { members: entries }),
        "zset" => entries
            .into_iter()
            .map(|entry| {
                let (member, score) = entry.rsplit_once('=').ok_or(AppError::CommandFailed)?;
                let score = score.parse::<f64>().map_err(|_| AppError::CommandFailed)?;
                if !score.is_finite() {
                    return Err(AppError::CommandFailed);
                }
                Ok(SortedSetEntry {
                    member: member.to_owned(),
                    score,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(|members| RedisValue::SortedSet { members }),
        _ => Err(AppError::UnsupportedDataType),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_json_value, decode_key_value, decode_stream_entry, encode_json_value,
        ensure_cluster_scan_page_size,
    };
    use crate::domain::{
        HashEntry, KeySummary, RedisValue, ScanCursor, ScanPage, SortedSetEntry, StreamEntry,
        StreamField,
    };
    use crate::error::AppError;

    #[test]
    fn cluster_scan_page_rejects_serialized_output_over_four_mibibytes() {
        let page = ScanPage {
            cursor: ScanCursor::Cluster("cluster:cursor".into()),
            keys: vec![KeySummary {
                key: "x".repeat(4 * 1024 * 1024),
                key_type: "string".into(),
                ttl_ms: -1,
                size: Some(1),
                memory_bytes: None,
                encoding: None,
                idle_seconds: None,
            }],
            has_more: true,
            node_failures: Vec::new(),
        };

        assert_eq!(
            ensure_cluster_scan_page_size(&page).unwrap_err(),
            AppError::InvalidInput
        );
    }

    #[test]
    fn maps_unknown_redis_type_to_unsupported_data_type() {
        let error = decode_key_value("vector", vec![]).unwrap_err();

        assert_eq!(error.code(), "UNSUPPORTED_DATA_TYPE");
    }

    #[test]
    fn decodes_scalar_and_collection_values() {
        assert_eq!(
            decode_key_value("string", vec!["hello".into()]).unwrap(),
            RedisValue::String {
                value: "hello".into()
            }
        );
        assert_eq!(
            decode_key_value("hash", vec!["name=redix".into()]).unwrap(),
            RedisValue::Hash {
                fields: vec![HashEntry {
                    field: "name".into(),
                    value: "redix".into()
                }]
            }
        );
        assert_eq!(
            decode_key_value("list", vec!["a".into(), "b".into()]).unwrap(),
            RedisValue::List {
                items: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(
            decode_key_value("set", vec!["a".into(), "b".into()]).unwrap(),
            RedisValue::Set {
                members: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(
            decode_key_value("zset", vec!["redix=1.5".into()]).unwrap(),
            RedisValue::SortedSet {
                members: vec![SortedSetEntry {
                    member: "redix".into(),
                    score: 1.5
                }]
            }
        );
    }

    #[test]
    fn rejects_malformed_value_entries_without_exposing_input() {
        let error = decode_key_value("zset", vec!["member=not-a-score".into()]).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn decodes_stream_entries_from_flattened_redis_fields() {
        assert_eq!(
            decode_stream_entry(
                "1710000000000-0",
                vec![
                    "event".into(),
                    "created".into(),
                    "owner".into(),
                    "redix".into()
                ],
            )
            .unwrap(),
            StreamEntry {
                id: "1710000000000-0".into(),
                fields: vec![
                    StreamField {
                        field: "event".into(),
                        value: "created".into(),
                    },
                    StreamField {
                        field: "owner".into(),
                        value: "redix".into(),
                    },
                ],
            }
        );
    }

    #[test]
    fn rejects_odd_stream_fields_without_exposing_field_values() {
        let error = decode_stream_entry("1-0", vec!["event".into()]).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn parses_and_serializes_json_as_a_valid_document() {
        let value = decode_json_value(r#"{"name":"Alice","items":[1,true]}"#).unwrap();

        assert_eq!(
            value,
            serde_json::json!({"name": "Alice", "items": [1, true]})
        );
        let encoded = encode_json_value(&value).unwrap();
        assert_eq!(decode_json_value(&encoded).unwrap(), value);
    }

    #[test]
    fn rejects_invalid_json_as_command_failure() {
        let error = decode_json_value(r#"{"name":"#).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert!(!error.to_string().contains("name"));
    }
}
