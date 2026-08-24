use crate::{
    domain::{SlowLogConfig, SlowLogEntry},
    error::AppError,
};

pub fn parse_slow_log_reply(value: ::redis::Value) -> Result<Vec<SlowLogEntry>, AppError> {
    let entries = match value {
        ::redis::Value::Array(entries) | ::redis::Value::Set(entries) => entries,
        ::redis::Value::Attribute { data, .. } => return parse_slow_log_reply(*data),
        _ => return Err(AppError::CommandFailed),
    };

    entries.into_iter().map(parse_slow_log_entry).collect()
}

fn parse_slow_log_entry(value: ::redis::Value) -> Result<SlowLogEntry, AppError> {
    let fields = match value {
        ::redis::Value::Array(fields) | ::redis::Value::Set(fields) => fields,
        ::redis::Value::Attribute { data, .. } => return parse_slow_log_entry(*data),
        _ => return Err(AppError::CommandFailed),
    };
    if !(fields.len() == 5 || fields.len() == 6) {
        return Err(AppError::CommandFailed);
    }

    let mut fields = fields.into_iter();
    let id = value_to_u64(fields.next().ok_or(AppError::CommandFailed)?)?;
    let time = value_to_i64(fields.next().ok_or(AppError::CommandFailed)?)?;
    let duration_us = value_to_u64(fields.next().ok_or(AppError::CommandFailed)?)?;
    let args = value_to_strings(fields.next().ok_or(AppError::CommandFailed)?)?;
    let source = value_to_string(fields.next().ok_or(AppError::CommandFailed)?)?;
    let client = match fields.next() {
        None | Some(::redis::Value::Nil) => None,
        Some(value) => Some(value_to_string(value)?),
    };

    Ok(SlowLogEntry {
        id,
        time,
        duration_us,
        args,
        source,
        client,
    })
}

pub fn parse_slow_log_config_reply(value: ::redis::Value) -> Result<SlowLogConfig, AppError> {
    let values = match value {
        ::redis::Value::Array(values) | ::redis::Value::Set(values) => values,
        ::redis::Value::Map(entries) => entries
            .into_iter()
            .flat_map(|(key, value)| [key, value])
            .collect(),
        ::redis::Value::Attribute { data, .. } => return parse_slow_log_config_reply(*data),
        _ => return Err(AppError::CommandFailed),
    };
    if values.len() % 2 != 0 {
        return Err(AppError::CommandFailed);
    }

    let mut max_len = None;
    let mut slower_than = None;
    for pair in values.chunks_exact(2) {
        let name = value_to_string(pair[0].clone())?;
        match name.as_str() {
            "slowlog-max-len" => max_len = Some(value_to_u64(pair[1].clone())?),
            "slowlog-log-slower-than" => slower_than = Some(value_to_i64(pair[1].clone())?),
            _ => {}
        }
    }

    Ok(SlowLogConfig {
        slowlog_max_len: max_len.ok_or(AppError::CommandFailed)?,
        slowlog_log_slower_than: slower_than.ok_or(AppError::CommandFailed)?,
    })
}

fn value_to_strings(value: ::redis::Value) -> Result<Vec<String>, AppError> {
    let values = match value {
        ::redis::Value::Array(values) | ::redis::Value::Set(values) => values,
        ::redis::Value::Attribute { data, .. } => return value_to_strings(*data),
        _ => return Err(AppError::CommandFailed),
    };
    values.into_iter().map(value_to_string).collect()
}

fn value_to_string(value: ::redis::Value) -> Result<String, AppError> {
    match value {
        ::redis::Value::BulkString(value) => {
            String::from_utf8(value).map_err(|_| AppError::CommandFailed)
        }
        ::redis::Value::SimpleString(value) => Ok(value),
        ::redis::Value::Okay => Ok("OK".into()),
        ::redis::Value::VerbatimString { text, .. } => Ok(text),
        ::redis::Value::BigNumber(value) => Ok(value.to_string()),
        ::redis::Value::Int(value) => Ok(value.to_string()),
        _ => Err(AppError::CommandFailed),
    }
}

fn value_to_i64(value: ::redis::Value) -> Result<i64, AppError> {
    value_to_string(value)?
        .parse::<i64>()
        .map_err(|_| AppError::CommandFailed)
}

fn value_to_u64(value: ::redis::Value) -> Result<u64, AppError> {
    value_to_string(value)?
        .parse::<u64>()
        .map_err(|_| AppError::CommandFailed)
}

#[cfg(test)]
mod tests {
    use super::parse_slow_log_reply;

    #[test]
    fn parses_slow_log_entries_with_optional_client_name() {
        let reply = ::redis::Value::Array(vec![::redis::Value::Array(vec![
            ::redis::Value::Int(7),
            ::redis::Value::Int(1_710_000_000),
            ::redis::Value::Int(2_500),
            ::redis::Value::Array(vec![
                ::redis::Value::BulkString(b"SET".to_vec()),
                ::redis::Value::BulkString(b"demo".to_vec()),
                ::redis::Value::BulkString(b"hello world".to_vec()),
            ]),
            ::redis::Value::BulkString(b"127.0.0.1:6379".to_vec()),
            ::redis::Value::BulkString(b"redix-test".to_vec()),
        ])]);

        let entries = parse_slow_log_reply(reply).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 7);
        assert_eq!(entries[0].duration_us, 2_500);
        assert_eq!(entries[0].args, vec!["SET", "demo", "hello world"]);
        assert_eq!(entries[0].source, "127.0.0.1:6379");
        assert_eq!(entries[0].client.as_deref(), Some("redix-test"));
    }

    #[test]
    fn rejects_incomplete_slow_log_reply_without_exposing_values() {
        let reply = ::redis::Value::Array(vec![::redis::Value::Array(vec![
            ::redis::Value::Int(7),
            ::redis::Value::Int(1_710_000_000),
        ])]);

        let error = parse_slow_log_reply(reply).unwrap_err();
        assert_eq!(error.code(), "COMMAND_FAILED");
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn parses_slow_log_config_from_resp3_map() {
        let reply = ::redis::Value::Map(vec![
            (
                ::redis::Value::SimpleString("slowlog-max-len".into()),
                ::redis::Value::Int(128),
            ),
            (
                ::redis::Value::SimpleString("slowlog-log-slower-than".into()),
                ::redis::Value::Int(10_000),
            ),
        ]);

        let config = super::parse_slow_log_config_reply(reply).unwrap();
        assert_eq!(config.slowlog_max_len, 128);
        assert_eq!(config.slowlog_log_slower_than, 10_000);
    }
}
