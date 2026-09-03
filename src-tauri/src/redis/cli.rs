use super::{connection_manager::command_result, RedisService};
use crate::{
    domain::{CliCommandInput, CliReply, CliSessionInput, CommandResult},
    error::AppError,
};
use redis::aio::ConnectionLike;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{watch, Mutex};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SESSIONS: usize = 16;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

struct CliSession {
    connection_id: String,
    socket: Mutex<Option<super::RoutedConnection>>,
    closed: watch::Sender<bool>,
}

#[derive(Default)]
pub struct CliManager {
    sessions: Mutex<HashMap<String, Arc<CliSession>>>,
}

impl CliManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn open(&self, redis: &RedisService, input: CliSessionInput) -> Result<(), AppError> {
        input.validate()?;
        let (closed, mut cancelled) = watch::channel(false);
        let session = Arc::new(CliSession {
            connection_id: input.connection_id.clone(),
            socket: Mutex::new(None),
            closed,
        });
        // Reserve before network I/O so close can cancel an unfinished handshake.
        {
            let mut sessions = self.sessions.lock().await;
            if sessions.contains_key(&input.session_id) || sessions.len() >= MAX_SESSIONS {
                return Err(AppError::InvalidInput);
            }
            sessions.insert(input.session_id.clone(), session.clone());
        }
        let result = tokio::select! {
            biased;
            _ = cancelled.changed() => Err(AppError::OperationCancelled),
            result = tokio::time::timeout(COMMAND_TIMEOUT, redis.routed_connection(&input.connection_id)) => {
                result.unwrap_or(Err(AppError::ConnectionFailed))
            },
        };
        // Use the same lock order as execute, and never commit to a replaced slot.
        let mut socket = session.socket.lock().await;
        let mut sessions = self.sessions.lock().await;
        if !sessions
            .get(&input.session_id)
            .is_some_and(|current| Arc::ptr_eq(current, &session))
            || *cancelled.borrow()
        {
            return Err(AppError::OperationCancelled);
        }
        match result {
            Ok(mut connection) => {
                connection.set_response_timeout(COMMAND_TIMEOUT);
                *socket = Some(connection);
                Ok(())
            }
            Err(error) => {
                sessions.remove(&input.session_id);
                session.closed.send_replace(true);
                Err(error)
            }
        }
    }

    pub async fn execute(&self, input: CliCommandInput) -> Result<CliReply, AppError> {
        let args = input.validate()?;
        let session = self
            .sessions
            .lock()
            .await
            .get(&input.session_id)
            .cloned()
            .ok_or(AppError::ConnectionFailed)?;
        if session.connection_id != input.connection_id {
            return Err(AppError::InvalidInput);
        }
        let mut cancelled = session.closed.subscribe();
        let mut socket = session.socket.lock().await;
        if *cancelled.borrow() {
            return Err(AppError::ConnectionFailed);
        }
        let connection = socket.as_mut().ok_or(AppError::ConnectionFailed)?;
        let mut command = redis::cmd(&args[0]);
        command.arg(&args[1..]);
        let response = tokio::select! {
            biased;
            _ = cancelled.changed() => None,
            // query_async recursively extracts nested RESP errors, losing successful
            // EXEC items. Keep the raw response and redact each nested error below.
            result = tokio::time::timeout(COMMAND_TIMEOUT, connection.req_packed_command(&command)) => Some(result),
        };
        let (reply, discard) = match response {
            None => (CliReply::error("OPERATION_CANCELLED", true), true),
            Some(Err(_)) => (CliReply::error("COMMAND_TIMEOUT", true), true),
            Some(Ok(Err(error))) => {
                let discard = error.is_io_error() || error.is_timeout();
                (
                    CliReply::error(
                        if error.is_timeout() {
                            "COMMAND_TIMEOUT"
                        } else if discard {
                            "CONNECTION_FAILED"
                        } else if error.kind()
                            == redis::ErrorKind::Server(redis::ServerErrorKind::CrossSlot)
                        {
                            "CROSS_SLOT"
                        } else {
                            "COMMAND_FAILED"
                        },
                        discard,
                    ),
                    discard,
                )
            }
            Some(Ok(Ok(value))) => {
                let quit = args[0].eq_ignore_ascii_case("QUIT");
                let reply = match cli_command_result(value) {
                    Ok(result) => {
                        let truncated = serde_json::to_vec(&result)
                            .map_err(|_| AppError::CommandFailed)?
                            .len()
                            > MAX_OUTPUT_BYTES;
                        let result = if truncated {
                            CommandResult {
                                kind: "string".into(),
                                value: "输出超过 256 KiB，已省略；请使用范围命令缩小结果。".into(),
                            }
                        } else {
                            result
                        };
                        CliReply {
                            result: Some(result),
                            error_code: None,
                            session_closed: quit,
                            truncated,
                        }
                    }
                    Err(_) => CliReply::error("COMMAND_FAILED", quit),
                };
                (reply, quit)
            }
        };
        if discard {
            session.closed.send_replace(true);
            socket.take();
            let mut sessions = self.sessions.lock().await;
            if sessions
                .get(&input.session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &session))
            {
                sessions.remove(&input.session_id);
            }
        }
        Ok(reply)
    }

    pub async fn close(&self, input: CliSessionInput) -> Result<(), AppError> {
        input.validate()?;
        let session = {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(&input.session_id) {
                if session.connection_id != input.connection_id {
                    return Err(AppError::InvalidInput);
                }
            }
            sessions.remove(&input.session_id)
        };
        if let Some(session) = session {
            session.closed.send_replace(true);
            session.socket.lock().await.take();
        }
        Ok(())
    }

    pub async fn close_connection(&self, connection_id: &str) {
        let removed = {
            let mut sessions = self.sessions.lock().await;
            let ids = sessions
                .iter()
                .filter(|(_, session)| session.connection_id == connection_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| sessions.remove(&id))
                .collect::<Vec<_>>()
        };
        for session in removed {
            session.closed.send_replace(true);
            session.socket.lock().await.take();
        }
    }
}

fn cli_command_result(value: redis::Value) -> Result<CommandResult, AppError> {
    let values = match value {
        redis::Value::Array(values)
        | redis::Value::Set(values)
        | redis::Value::Push { data: values, .. } => {
            values.into_iter().map(cli_nested_value).collect()
        }
        redis::Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| serde_json::json!([cli_nested_value(key), cli_nested_value(value)]))
            .collect(),
        redis::Value::Attribute { data, .. } => return cli_command_result(*data),
        // A top-level server error still fails the command with a fixed error code.
        value => return command_result(value),
    };
    Ok(CommandResult {
        kind: "array".into(),
        value: serde_json::Value::Array(values),
    })
}

fn cli_nested_value(value: redis::Value) -> serde_json::Value {
    cli_command_result(value)
        .map(|result| result.value)
        .unwrap_or_else(|_| serde_json::json!({ "error_code": "COMMAND_FAILED" }))
}
