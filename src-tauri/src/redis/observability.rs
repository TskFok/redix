use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use tauri::Emitter;
use tokio::task::JoinHandle;

use crate::{
    domain::{
        MonitorEntry, ProfilerEvent, ProfilerSession, ProfilerStatusEvent, PubSubMessageEvent,
        PubSubSession, PubSubStatusEvent, SlowLogConfig, SlowLogEntry, StartProfilerInput,
        StartPubSubInput, StopProfilerInput, StopPubSubInput,
    },
    error::AppError,
};

use super::{tokenize_command, MonitorLineStream};

pub const PUBSUB_MESSAGE_EVENT: &str = "redix://pubsub/message";
pub const PUBSUB_STATUS_EVENT: &str = "redix://pubsub/status";
pub const PROFILER_EVENT: &str = "redix://profiler/event";
pub const PROFILER_STATUS_EVENT: &str = "redix://profiler/status";

pub fn parse_monitor_line(value: &str) -> Result<MonitorEntry, AppError> {
    let (raw_time, remainder) = value
        .trim()
        .split_once(' ')
        .ok_or(AppError::CommandFailed)?;
    let time = raw_time.trim_start_matches('[').trim();
    if time.is_empty()
        || time
            .parse::<f64>()
            .map_or(true, |timestamp| !timestamp.is_finite())
    {
        return Err(AppError::CommandFailed);
    }

    let metadata = remainder
        .trim_start()
        .strip_prefix('[')
        .ok_or(AppError::CommandFailed)?;
    let (metadata, command) = metadata.split_once(']').ok_or(AppError::CommandFailed)?;
    let mut fields = metadata.split_whitespace();
    let database = fields
        .next()
        .ok_or(AppError::CommandFailed)?
        .parse::<u8>()
        .map_err(|_| AppError::CommandFailed)?;
    if database > 15 {
        return Err(AppError::CommandFailed);
    }
    let source = fields.next().ok_or(AppError::CommandFailed)?;
    if source.is_empty() || fields.next().is_some() {
        return Err(AppError::CommandFailed);
    }

    Ok(MonitorEntry {
        time: time.to_owned(),
        database,
        source: source.to_owned(),
        args: tokenize_command(command.trim())?,
    })
}

struct PubSubTask {
    connection_id: String,
    session_id: String,
    app: tauri::AppHandle,
    handle: JoinHandle<()>,
}

pub struct PubSubManager {
    tasks: Mutex<HashMap<String, PubSubTask>>,
}

struct ProfilerTask {
    connection_id: String,
    session_id: String,
    app: tauri::AppHandle,
    handle: JoinHandle<()>,
}

pub struct ProfilerManager {
    tasks: Mutex<HashMap<String, ProfilerTask>>,
}

impl Default for PubSubManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PubSubManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start(
        &self,
        app: tauri::AppHandle,
        mut pubsub: ::redis::aio::PubSub,
        input: StartPubSubInput,
    ) -> Result<PubSubSession, AppError> {
        input.validate()?;
        let topics = input.normalized_topics();

        for topic in &topics {
            if topic.pattern {
                pubsub
                    .psubscribe(topic.name.as_str())
                    .await
                    .map_err(map_pubsub_error)?;
            } else {
                pubsub
                    .subscribe(topic.name.as_str())
                    .await
                    .map_err(map_pubsub_error)?;
            }
        }

        self.cancel_connection(&input.connection_id);

        let connection_id = input.connection_id.clone();
        let session_id = input.session_id.clone();
        let task_connection_id = connection_id.clone();
        let task_session_id = session_id.clone();
        let task_app = app.clone();
        let handle = tokio::spawn(async move {
            let mut stream = pubsub.into_on_message();
            while let Some(message) = stream.next().await {
                let pattern = message.get_pattern::<String>().ok();
                let event = PubSubMessageEvent {
                    connection_id: task_connection_id.clone(),
                    session_id: task_session_id.clone(),
                    channel: message.get_channel_name().to_owned(),
                    pattern,
                    message: String::from_utf8_lossy(message.get_payload_bytes()).into_owned(),
                    received_at_ms: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };
                let _ = task_app.emit(PUBSUB_MESSAGE_EVENT, event);
            }

            let _ = task_app.emit(
                PUBSUB_STATUS_EVENT,
                PubSubStatusEvent {
                    connection_id: task_connection_id,
                    session_id: task_session_id,
                    state: "stopped".into(),
                    error_code: None,
                },
            );
        });

        let task = PubSubTask {
            connection_id: connection_id.clone(),
            session_id: session_id.clone(),
            app: app.clone(),
            handle,
        };
        let mut tasks = match self.tasks.lock() {
            Ok(tasks) => tasks,
            Err(_) => {
                task.handle.abort();
                return Err(AppError::CommandFailed);
            }
        };
        tasks.insert(connection_id.clone(), task);
        drop(tasks);

        let session = PubSubSession {
            connection_id,
            session_id,
            topics,
        };
        let _ = app.emit(
            PUBSUB_STATUS_EVENT,
            PubSubStatusEvent {
                connection_id: session.connection_id.clone(),
                session_id: session.session_id.clone(),
                state: "running".into(),
                error_code: None,
            },
        );
        Ok(session)
    }

    pub fn stop(&self, input: StopPubSubInput) -> Result<(), AppError> {
        input.validate()?;
        let task = {
            let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
            if tasks
                .get(&input.connection_id)
                .is_some_and(|task| task.session_id == input.session_id)
            {
                tasks.remove(&input.connection_id)
            } else {
                None
            }
        };
        if let Some(task) = task {
            stop_task(task);
        }
        Ok(())
    }

    pub fn cancel_connection(&self, connection_id: &str) {
        let task = self
            .tasks
            .lock()
            .ok()
            .and_then(|mut tasks| tasks.remove(connection_id));
        if let Some(task) = task {
            stop_task(task);
        }
    }
}

impl Default for ProfilerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfilerManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start(
        &self,
        app: tauri::AppHandle,
        mut monitor: MonitorLineStream,
        input: StartProfilerInput,
    ) -> Result<ProfilerSession, AppError> {
        input.validate()?;
        self.cancel_connection(&input.connection_id);

        let connection_id = input.connection_id.clone();
        let session_id = input.session_id.clone();
        let task_connection_id = connection_id.clone();
        let task_session_id = session_id.clone();
        let task_app = app.clone();
        let handle = tokio::spawn(async move {
            while let Some(Ok(line)) = monitor.next().await {
                let Ok(entry) = parse_monitor_line(&line) else {
                    continue;
                };
                let event = ProfilerEvent {
                    connection_id: task_connection_id.clone(),
                    session_id: task_session_id.clone(),
                    time: entry.time,
                    database: entry.database,
                    source: entry.source,
                    args: entry.args,
                    received_at_ms: current_unix_millis(),
                };
                let _ = task_app.emit(PROFILER_EVENT, event);
            }

            let _ = task_app.emit(
                PROFILER_STATUS_EVENT,
                ProfilerStatusEvent {
                    connection_id: task_connection_id,
                    session_id: task_session_id,
                    state: "stopped".into(),
                    error_code: None,
                },
            );
        });

        let task = ProfilerTask {
            connection_id: connection_id.clone(),
            session_id: session_id.clone(),
            app: app.clone(),
            handle,
        };
        let mut tasks = match self.tasks.lock() {
            Ok(tasks) => tasks,
            Err(_) => {
                task.handle.abort();
                return Err(AppError::CommandFailed);
            }
        };
        tasks.insert(connection_id.clone(), task);
        drop(tasks);

        let session = ProfilerSession {
            connection_id,
            session_id,
        };
        let _ = app.emit(
            PROFILER_STATUS_EVENT,
            ProfilerStatusEvent {
                connection_id: session.connection_id.clone(),
                session_id: session.session_id.clone(),
                state: "running".into(),
                error_code: None,
            },
        );
        Ok(session)
    }

    pub fn stop(&self, input: StopProfilerInput) -> Result<(), AppError> {
        input.validate()?;
        let task = {
            let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
            if tasks
                .get(&input.connection_id)
                .is_some_and(|task| task.session_id == input.session_id)
            {
                tasks.remove(&input.connection_id)
            } else {
                None
            }
        };
        if let Some(task) = task {
            stop_profiler_task(task);
        }
        Ok(())
    }

    pub fn cancel_connection(&self, connection_id: &str) {
        let task = self
            .tasks
            .lock()
            .ok()
            .and_then(|mut tasks| tasks.remove(connection_id));
        if let Some(task) = task {
            stop_profiler_task(task);
        }
    }
}

fn stop_profiler_task(task: ProfilerTask) {
    let _ = task.app.emit(
        PROFILER_STATUS_EVENT,
        ProfilerStatusEvent {
            connection_id: task.connection_id,
            session_id: task.session_id,
            state: "stopped".into(),
            error_code: None,
        },
    );
    task.handle.abort();
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn stop_task(task: PubSubTask) {
    let _ = task.app.emit(
        PUBSUB_STATUS_EVENT,
        PubSubStatusEvent {
            connection_id: task.connection_id,
            session_id: task.session_id,
            state: "stopped".into(),
            error_code: None,
        },
    );
    task.handle.abort();
}

fn map_pubsub_error(error: ::redis::RedisError) -> AppError {
    match error.kind() {
        ::redis::ErrorKind::AuthenticationFailed => AppError::AuthenticationFailed,
        ::redis::ErrorKind::Io => AppError::ConnectionFailed,
        _ => AppError::CommandFailed,
    }
}

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
    use super::{parse_monitor_line, parse_slow_log_reply};

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

    #[test]
    fn parses_monitor_line_with_quoted_and_escaped_arguments() {
        let entry = parse_monitor_line(
            r#"1710000000.123456 [2 127.0.0.1:6379] "SET" "demo key" "hello \"redis\"""#,
        )
        .unwrap();

        assert_eq!(entry.time, "1710000000.123456");
        assert_eq!(entry.database, 2);
        assert_eq!(entry.source, "127.0.0.1:6379");
        assert_eq!(entry.args, vec!["SET", "demo key", "hello \"redis\""]);
    }

    #[test]
    fn rejects_malformed_monitor_line_without_exposing_content() {
        let error = parse_monitor_line("not a monitor line containing secret-value").unwrap_err();

        assert_eq!(error, crate::error::AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }
}
