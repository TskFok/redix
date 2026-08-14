use crate::{
    domain::{HashEntry, RedisValue, SortedSetEntry},
    error::AppError,
};

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
    use super::decode_key_value;
    use crate::domain::{HashEntry, RedisValue, SortedSetEntry};

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
}
