use std::collections::HashMap;

use crate::{
    domain::{StreamConsumer, StreamConsumerGroup, StreamPendingEntry},
    error::AppError,
};

pub fn parse_stream_consumer_groups(
    value: ::redis::Value,
) -> Result<Vec<StreamConsumerGroup>, AppError> {
    rows(value)?
        .into_iter()
        .map(|row| {
            let mut fields = field_map(row)?;
            Ok(StreamConsumerGroup {
                name: take_string(&mut fields, "name")?,
                consumers: take_u64(&mut fields, "consumers")?,
                pending: take_u64(&mut fields, "pending")?,
                last_delivered_id: take_string(&mut fields, "last-delivered-id")?,
            })
        })
        .collect()
}

pub fn parse_stream_consumers(value: ::redis::Value) -> Result<Vec<StreamConsumer>, AppError> {
    rows(value)?
        .into_iter()
        .map(|row| {
            let mut fields = field_map(row)?;
            Ok(StreamConsumer {
                name: take_string(&mut fields, "name")?,
                pending: take_u64(&mut fields, "pending")?,
                idle_ms: take_u64(&mut fields, "idle")?,
            })
        })
        .collect()
}

pub fn parse_stream_pending_entries(
    value: ::redis::Value,
) -> Result<Vec<StreamPendingEntry>, AppError> {
    rows(value)?
        .into_iter()
        .map(|row| {
            let fields = sequence(row)?;
            if fields.len() != 4 {
                return Err(AppError::CommandFailed);
            }
            let mut fields = fields.into_iter();
            Ok(StreamPendingEntry {
                id: value_to_string(fields.next().ok_or(AppError::CommandFailed)?)?,
                consumer: value_to_string(fields.next().ok_or(AppError::CommandFailed)?)?,
                idle_ms: value_to_u64(fields.next().ok_or(AppError::CommandFailed)?)?,
                deliveries: value_to_u64(fields.next().ok_or(AppError::CommandFailed)?)?,
            })
        })
        .collect()
}

fn rows(value: ::redis::Value) -> Result<Vec<::redis::Value>, AppError> {
    match unwrap(value) {
        ::redis::Value::Array(values) | ::redis::Value::Set(values) => Ok(values),
        ::redis::Value::Map(entries) => Ok(vec![::redis::Value::Map(entries)]),
        _ => Err(AppError::CommandFailed),
    }
}

fn sequence(value: ::redis::Value) -> Result<Vec<::redis::Value>, AppError> {
    match unwrap(value) {
        ::redis::Value::Array(values) | ::redis::Value::Set(values) => Ok(values),
        _ => Err(AppError::CommandFailed),
    }
}

fn field_map(value: ::redis::Value) -> Result<HashMap<String, ::redis::Value>, AppError> {
    match unwrap(value) {
        ::redis::Value::Array(values) | ::redis::Value::Set(values) => {
            if values.len() % 2 != 0 {
                return Err(AppError::CommandFailed);
            }
            let mut fields = HashMap::with_capacity(values.len() / 2);
            let mut values = values.into_iter();
            while let Some(key) = values.next() {
                let key = value_to_string(key)?;
                let value = values.next().ok_or(AppError::CommandFailed)?;
                fields.insert(key, unwrap(value));
            }
            Ok(fields)
        }
        ::redis::Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| Ok((value_to_string(key)?, unwrap(value))))
            .collect(),
        _ => Err(AppError::CommandFailed),
    }
}

fn take_string(
    fields: &mut HashMap<String, ::redis::Value>,
    name: &str,
) -> Result<String, AppError> {
    value_to_string(fields.remove(name).ok_or(AppError::CommandFailed)?)
}

fn take_u64(fields: &mut HashMap<String, ::redis::Value>, name: &str) -> Result<u64, AppError> {
    value_to_u64(fields.remove(name).ok_or(AppError::CommandFailed)?)
}

fn unwrap(value: ::redis::Value) -> ::redis::Value {
    match value {
        ::redis::Value::Attribute { data, .. } => unwrap(*data),
        value => value,
    }
}

fn value_to_string(value: ::redis::Value) -> Result<String, AppError> {
    match unwrap(value) {
        ::redis::Value::BulkString(value) => {
            String::from_utf8(value).map_err(|_| AppError::CommandFailed)
        }
        ::redis::Value::SimpleString(value) => Ok(value),
        ::redis::Value::Okay => Ok("OK".into()),
        ::redis::Value::Int(value) => Ok(value.to_string()),
        ::redis::Value::BigNumber(value) => Ok(value.to_string()),
        ::redis::Value::Double(value) => Ok(value.to_string()),
        ::redis::Value::Boolean(value) => Ok(value.to_string()),
        ::redis::Value::VerbatimString { text, .. } => Ok(text),
        _ => Err(AppError::CommandFailed),
    }
}

fn value_to_u64(value: ::redis::Value) -> Result<u64, AppError> {
    value_to_string(value)?
        .parse::<u64>()
        .map_err(|_| AppError::CommandFailed)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_stream_consumer_groups, parse_stream_consumers, parse_stream_pending_entries,
    };

    #[test]
    fn parser_entry_points_are_exercised_by_the_red_suite() {
        let _ = (
            parse_stream_consumer_groups,
            parse_stream_consumers,
            parse_stream_pending_entries,
        );
    }

    #[test]
    fn parses_resp2_group_fields() {
        let reply = ::redis::Value::Array(vec![::redis::Value::Array(vec![
            ::redis::Value::SimpleString("name".into()),
            ::redis::Value::SimpleString("workers".into()),
            ::redis::Value::SimpleString("consumers".into()),
            ::redis::Value::Int(2),
            ::redis::Value::SimpleString("pending".into()),
            ::redis::Value::Int(3),
            ::redis::Value::SimpleString("last-delivered-id".into()),
            ::redis::Value::SimpleString("1-0".into()),
        ])]);

        let groups = parse_stream_consumer_groups(reply).unwrap();
        assert_eq!(groups[0].name, "workers");
        assert_eq!(groups[0].consumers, 2);
        assert_eq!(groups[0].pending, 3);
        assert_eq!(groups[0].last_delivered_id, "1-0");
    }

    #[test]
    fn parses_resp3_map_wrapped_in_attribute() {
        let reply = ::redis::Value::Attribute {
            data: Box::new(::redis::Value::Array(vec![::redis::Value::Map(vec![
                (
                    ::redis::Value::SimpleString("name".into()),
                    ::redis::Value::SimpleString("workers".into()),
                ),
                (
                    ::redis::Value::SimpleString("pending".into()),
                    ::redis::Value::Int(1),
                ),
                (
                    ::redis::Value::SimpleString("consumers".into()),
                    ::redis::Value::Int(1),
                ),
                (
                    ::redis::Value::SimpleString("last-delivered-id".into()),
                    ::redis::Value::SimpleString("2-0".into()),
                ),
            ])])),
            attributes: vec![],
        };

        let groups = parse_stream_consumer_groups(reply).unwrap();
        assert_eq!(groups[0].last_delivered_id, "2-0");
    }

    #[test]
    fn parses_consumers_and_pending_entries() {
        let consumers = ::redis::Value::Array(vec![::redis::Value::Array(vec![
            ::redis::Value::SimpleString("name".into()),
            ::redis::Value::SimpleString("consumer-1".into()),
            ::redis::Value::SimpleString("pending".into()),
            ::redis::Value::Int(2),
            ::redis::Value::SimpleString("idle".into()),
            ::redis::Value::Int(100),
        ])]);
        let pending = ::redis::Value::Array(vec![::redis::Value::Array(vec![
            ::redis::Value::SimpleString("1-0".into()),
            ::redis::Value::SimpleString("consumer-1".into()),
            ::redis::Value::Int(100),
            ::redis::Value::Int(2),
        ])]);

        assert_eq!(parse_stream_consumers(consumers).unwrap()[0].pending, 2);
        assert_eq!(
            parse_stream_pending_entries(pending).unwrap()[0].deliveries,
            2
        );
    }

    #[test]
    fn rejects_malformed_stream_group_replies_without_exposing_values() {
        let error = parse_stream_consumer_groups(::redis::Value::Array(vec![
            ::redis::Value::Array(vec![::redis::Value::SimpleString("secret-value".into())]),
        ]))
        .unwrap_err();

        assert_eq!(error, crate::error::AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn rejects_malformed_pending_delivery_count_without_exposing_values() {
        let error =
            parse_stream_pending_entries(::redis::Value::Array(vec![::redis::Value::Array(vec![
                ::redis::Value::SimpleString("1-0".into()),
                ::redis::Value::SimpleString("consumer-1".into()),
                ::redis::Value::Int(100),
                ::redis::Value::SimpleString("secret-value".into()),
            ])]))
            .unwrap_err();

        assert_eq!(error, crate::error::AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }
}
