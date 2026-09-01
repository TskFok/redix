use super::connection_manager::map_command_error;
use crate::{domain::stream_entries::*, error::AppError, redis::RedisService};
use ::redis::{Cmd, Value};

impl RedisService {
    pub async fn get_stream_entries(
        &self,
        input: GetStreamEntriesInput,
    ) -> Result<StreamEntriesPage, AppError> {
        let command = build_range_command(&input)?;
        let mut connection = self.connection(&input.connection_id).await?;
        let value = command
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_page(value, input.count as usize)
    }

    pub async fn add_stream_entry(&self, input: AddStreamEntryInput) -> Result<String, AppError> {
        input.validate()?;
        let mut command = ::redis::cmd("XADD");
        // Editing an existing key must not silently recreate one that expired or was deleted.
        command.arg(&input.key).arg("NOMKSTREAM").arg(&input.id);
        for field in &input.fields {
            command.arg(&field.field).arg(&field.value);
        }
        let mut connection = self.connection(&input.connection_id).await?;
        command
            .query_async::<Option<String>>(&mut connection)
            .await
            .map_err(map_command_error)?
            .ok_or(AppError::KeyNotFound)
    }

    pub async fn delete_stream_entries(
        &self,
        input: DeleteStreamEntriesInput,
    ) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        ::redis::cmd("XDEL")
            .arg(&input.key)
            .arg(&input.ids)
            .query_async::<u64>(&mut connection)
            .await
            .map_err(map_command_error)
    }
}

fn build_range_command(input: &GetStreamEntriesInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd(if input.reverse { "XREVRANGE" } else { "XRANGE" });
    let cursor = input.cursor.as_ref().map(|id| format!("({id}"));
    let (first, last) = if input.reverse {
        (cursor.as_deref().unwrap_or(&input.end), &input.start)
    } else {
        (cursor.as_deref().unwrap_or(&input.start), &input.end)
    };
    command
        .arg(&input.key)
        .arg(first)
        .arg(last)
        .arg("COUNT")
        .arg(input.count + 1);
    Ok(command)
}

fn parse_page(value: Value, count: usize) -> Result<StreamEntriesPage, AppError> {
    if !(1..=MAX_STREAM_ENTRIES as usize).contains(&count) {
        return Err(AppError::InvalidInput);
    }
    let rows = array(value)?;
    if rows.len() > count + 1 {
        return Err(AppError::CommandFailed);
    }
    let has_more = rows.len() > count;
    let mut bytes = 0usize;
    let mut entries = Vec::with_capacity(rows.len().min(count));
    for row in rows.into_iter().take(count) {
        let mut row = array(row)?.into_iter();
        let id = text(row.next().ok_or(AppError::CommandFailed)?)?;
        parse_stream_entry_id(&id).map_err(|_| AppError::CommandFailed)?;
        let flat_fields = array(row.next().ok_or(AppError::CommandFailed)?)?;
        if row.next().is_some()
            || flat_fields.is_empty()
            || flat_fields.len() % 2 != 0
            || flat_fields.len() / 2 > 500
        {
            return Err(AppError::CommandFailed);
        }
        let mut values = flat_fields.into_iter();
        let mut fields = Vec::new();
        while let Some(field) = values.next() {
            let field = text(field)?;
            let value = text(values.next().ok_or(AppError::CommandFailed)?)?;
            bytes = bytes
                .checked_add(field.len())
                .and_then(|n| n.checked_add(value.len()))
                .ok_or(AppError::CommandFailed)?;
            if bytes > 4 * 1024 * 1024 {
                return Err(AppError::CommandFailed);
            }
            fields.push(StreamEntryField { field, value });
        }
        entries.push(StreamEntryRecord { id, fields });
    }
    let next_cursor = if has_more {
        entries.last().map(|entry| entry.id.clone())
    } else {
        None
    };
    Ok(StreamEntriesPage {
        entries,
        next_cursor,
        has_more,
    })
}

fn unwrap(value: Value) -> Value {
    match value {
        Value::Attribute { data, .. } => unwrap(*data),
        value => value,
    }
}
fn array(value: Value) -> Result<Vec<Value>, AppError> {
    match unwrap(value) {
        Value::Array(values) => Ok(values),
        _ => Err(AppError::CommandFailed),
    }
}
fn text(value: Value) -> Result<String, AppError> {
    match unwrap(value) {
        Value::BulkString(bytes) => String::from_utf8(bytes).map_err(|_| AppError::CommandFailed),
        Value::SimpleString(text) => Ok(text),
        _ => Err(AppError::CommandFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(reverse: bool) -> GetStreamEntriesInput {
        GetStreamEntriesInput {
            connection_id: "local".into(),
            key: "events".into(),
            start: "1-0".into(),
            end: "9-0".into(),
            cursor: Some("5-18446744073709551615".into()),
            count: 2,
            reverse,
        }
    }
    #[test]
    fn stream_range_emits_exclusive_cursor_and_bounded_lookahead_in_both_directions() {
        let ascending = build_range_command(&input(false))
            .unwrap()
            .get_packed_command();
        let expected = ::redis::cmd("XRANGE")
            .arg("events")
            .arg("(5-18446744073709551615")
            .arg("9-0")
            .arg("COUNT")
            .arg(3)
            .get_packed_command();
        assert_eq!(ascending, expected);
        let descending = build_range_command(&input(true))
            .unwrap()
            .get_packed_command();
        let expected = ::redis::cmd("XREVRANGE")
            .arg("events")
            .arg("(5-18446744073709551615")
            .arg("1-0")
            .arg("COUNT")
            .arg(3)
            .get_packed_command();
        assert_eq!(descending, expected);
    }
    fn entry(id: &str) -> Value {
        Value::Array(vec![
            Value::BulkString(id.as_bytes().to_vec()),
            Value::Array(vec![
                Value::BulkString(b"x".to_vec()),
                Value::BulkString(b"a".to_vec()),
                Value::BulkString(b"x".to_vec()),
                Value::BulkString(b"".to_vec()),
            ]),
        ])
    }
    #[test]
    fn stream_page_preserves_duplicate_fields_and_sets_last_visible_id_as_cursor() {
        let result = parse_page(
            Value::Array(vec![entry("9-0"), entry("8-0"), entry("7-0")]),
            2,
        )
        .unwrap();
        assert_eq!(
            result
                .entries
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["9-0", "8-0"]
        );
        assert_eq!(result.next_cursor.as_deref(), Some("8-0"));
        assert!(result.has_more);
        assert_eq!(
            result.entries[0].fields,
            vec![
                StreamEntryField {
                    field: "x".into(),
                    value: "a".into()
                },
                StreamEntryField {
                    field: "x".into(),
                    value: "".into()
                }
            ]
        );
        let last = parse_page(Value::Array(vec![entry("1-0")]), 2).unwrap();
        assert!(!last.has_more);
        assert_eq!(last.next_cursor, None);
    }
    #[test]
    fn stream_parser_rejects_malformed_data_without_lossy_decoding() {
        assert_eq!(
            parse_page(Value::Array(vec![Value::Int(3)]), 1),
            Err(AppError::CommandFailed)
        );
        let malformed = Value::Array(vec![Value::Array(vec![
            Value::BulkString(b"1-0".to_vec()),
            Value::Array(vec![
                Value::BulkString(b"field".to_vec()),
                Value::BulkString(vec![0xff]),
            ]),
        ])]);
        assert_eq!(parse_page(malformed, 1), Err(AppError::CommandFailed));
    }
}
