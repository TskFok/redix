use ::redis::{Cmd, Value};

use super::{connection_manager::map_command_error, parse_stream_pending_entries};
use crate::{domain::stream_advanced::*, error::AppError, redis::RedisService};

impl RedisService {
    pub async fn update_stream_group_id(
        &self,
        input: UpdateStreamGroupIdInput,
    ) -> Result<(), AppError> {
        let command = build_setid_command(&input)?;
        let mut connection = self.connection(&input.connection_id).await?;
        command
            .query_async::<()>(&mut connection)
            .await
            .map_err(map_command_error)
    }

    pub async fn get_stream_pending_page(
        &self,
        input: GetStreamPendingPageInput,
    ) -> Result<StreamPendingPage, AppError> {
        let command = build_pending_command(&input)?;
        let mut connection = self.connection(&input.connection_id).await?;
        let value = command
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        pending_page(value, input.count as usize)
    }

    pub async fn claim_stream_pending_advanced(
        &self,
        input: ClaimStreamPendingAdvancedInput,
    ) -> Result<Vec<String>, AppError> {
        let command = build_claim_command(&input)?;
        let mut connection = self.connection(&input.connection_id).await?;
        command
            .query_async::<Vec<String>>(&mut connection)
            .await
            .map_err(map_command_error)
    }
}

fn build_setid_command(input: &UpdateStreamGroupIdInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("XGROUP");
    command
        .arg("SETID")
        .arg(&input.key)
        .arg(&input.group)
        .arg(&input.last_delivered_id);
    Ok(command)
}

fn build_pending_command(input: &GetStreamPendingPageInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let cursor = input.cursor.as_ref().map(|cursor| format!("({cursor}"));
    let mut command = ::redis::cmd("XPENDING");
    command
        .arg(&input.key)
        .arg(&input.group)
        .arg(cursor.as_deref().unwrap_or(&input.start))
        .arg(&input.end)
        .arg(input.count + 1);
    if let Some(consumer) = &input.consumer {
        command.arg(consumer);
    }
    Ok(command)
}

fn build_claim_command(input: &ClaimStreamPendingAdvancedInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("XCLAIM");
    command
        .arg(&input.key)
        .arg(&input.group)
        .arg(&input.consumer)
        .arg(input.min_idle_ms)
        .arg(&input.entries);
    // RedisInsight gives IDLE precedence when both optional values are supplied.
    if let Some(idle) = input.idle_ms {
        command.arg("IDLE").arg(idle);
    } else if let Some(time) = input.time_ms {
        command.arg("TIME").arg(time);
    }
    if let Some(retry_count) = input.retry_count {
        command.arg("RETRYCOUNT").arg(retry_count);
    }
    if input.force {
        command.arg("FORCE");
    }
    command.arg("JUSTID");
    Ok(command)
}

fn pending_page(value: Value, count: usize) -> Result<StreamPendingPage, AppError> {
    let mut entries = parse_stream_pending_entries(value)?;
    let has_more = entries.len() > count;
    entries.truncate(count);
    let next_cursor = if has_more {
        entries.last().map(|entry| entry.id.clone())
    } else {
        None
    };
    Ok(StreamPendingPage {
        entries,
        has_more,
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: Cmd) -> Vec<String> {
        let Value::Array(values) =
            ::redis::parse_redis_value(&command.get_packed_command()).unwrap()
        else {
            panic!("command array");
        };
        values
            .into_iter()
            .map(|value| ::redis::from_redis_value::<String>(value).unwrap())
            .collect()
    }

    #[test]
    fn emits_only_setid_for_the_selected_group() {
        assert_eq!(
            args(
                build_setid_command(&UpdateStreamGroupIdInput {
                    connection_id: "local".into(),
                    key: "events".into(),
                    group: "workers".into(),
                    last_delivered_id: "0".into()
                })
                .unwrap()
            ),
            vec!["XGROUP", "SETID", "events", "workers", "0"]
        );
    }

    #[test]
    fn pending_pages_use_exclusive_cursor_and_lookahead_without_losing_consumer_filter() {
        let command = build_pending_command(&GetStreamPendingPageInput {
            connection_id: "local".into(),
            key: "events".into(),
            group: "workers".into(),
            consumer: Some("worker".into()),
            start: "1-0".into(),
            end: "9-0".into(),
            cursor: Some("3-0".into()),
            count: 100,
        })
        .unwrap();
        assert_eq!(
            args(command),
            vec!["XPENDING", "events", "workers", "(3-0", "9-0", "101", "worker"]
        );
        let row = |id: &str| {
            Value::Array(vec![
                Value::BulkString(id.as_bytes().to_vec()),
                Value::BulkString(b"old".to_vec()),
                Value::Int(12),
                Value::Int(3),
            ])
        };
        let page = pending_page(Value::Array(vec![row("1-0"), row("2-0"), row("3-0")]), 2).unwrap();
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.next_cursor.as_deref(), Some("2-0"));
        assert!(page.has_more);
        let last = pending_page(Value::Array(vec![row("3-0")]), 2).unwrap();
        assert!(!last.has_more);
        assert!(last.next_cursor.is_none());
    }

    #[test]
    fn claim_emits_options_justid_and_idle_precedes_time() {
        let mut input = ClaimStreamPendingAdvancedInput {
            connection_id: "local".into(),
            key: "events".into(),
            group: "workers".into(),
            consumer: "next".into(),
            min_idle_ms: 10,
            entries: vec!["1-0".into()],
            idle_ms: Some(0),
            time_ms: Some(1234),
            retry_count: Some(7),
            force: true,
        };
        assert_eq!(
            args(build_claim_command(&input).unwrap()),
            vec![
                "XCLAIM",
                "events",
                "workers",
                "next",
                "10",
                "1-0",
                "IDLE",
                "0",
                "RETRYCOUNT",
                "7",
                "FORCE",
                "JUSTID"
            ]
        );
        input.idle_ms = None;
        input.retry_count = None;
        input.force = false;
        assert_eq!(
            args(build_claim_command(&input).unwrap()),
            vec!["XCLAIM", "events", "workers", "next", "10", "1-0", "TIME", "1234", "JUSTID"]
        );
    }
}
