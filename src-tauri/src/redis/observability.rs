use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use tauri::Emitter;
use tokio::task::AbortHandle;

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

struct RegisteredTask {
    session_id: String,
    abort: AbortHandle,
    emit_stopped: Option<Box<dyn FnOnce() + Send>>,
}

impl RegisteredTask {
    fn stop(mut self) {
        self.emit_stopped();
        self.abort.abort();
    }

    fn complete(mut self) {
        self.emit_stopped();
    }

    fn emit_stopped(&mut self) {
        if let Some(emit_stopped) = self.emit_stopped.take() {
            emit_stopped();
        }
    }
}

#[derive(Default)]
struct TaskRegistry {
    tasks: Mutex<HashMap<String, RegisteredTask>>,
}

impl TaskRegistry {
    fn register_ready(
        &self,
        connection_id: String,
        task: RegisteredTask,
        ready: impl FnOnce(),
    ) -> Result<(), AppError> {
        let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        let replaced = tasks.insert(connection_id, task);
        if let Some(replaced) = replaced {
            replaced.stop();
        }
        ready();
        Ok(())
    }

    fn remove(
        &self,
        connection_id: &str,
        session_id: Option<&str>,
    ) -> Result<Option<RegisteredTask>, AppError> {
        let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        Ok(
            if session_id.is_none_or(|session_id| {
                tasks
                    .get(connection_id)
                    .is_some_and(|task| task.session_id == session_id)
            }) {
                tasks.remove(connection_id)
            } else {
                None
            },
        )
    }

    fn finish(&self, connection_id: &str, session_id: &str) -> Option<RegisteredTask> {
        self.remove(connection_id, Some(session_id)).ok().flatten()
    }
}

pub struct PubSubManager {
    registry: Arc<TaskRegistry>,
}

pub struct ProfilerManager {
    registry: Arc<TaskRegistry>,
}

impl Default for PubSubManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PubSubManager {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(TaskRegistry::default()),
        }
    }

    pub fn start<R: tauri::Runtime>(
        &self,
        app: tauri::AppHandle<R>,
        pubsub: ::redis::aio::PubSub,
        input: StartPubSubInput,
    ) -> Result<PubSubSession, AppError> {
        input.validate()?;
        let topics = input.normalized_topics();

        let connection_id = input.connection_id.clone();
        let session_id = input.session_id.clone();
        let task_connection_id = connection_id.clone();
        let task_session_id = session_id.clone();
        let task_app = app.clone();
        let task_registry = Arc::clone(&self.registry);
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }
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

            if let Some(task) = task_registry.finish(&task_connection_id, &task_session_id) {
                task.complete();
            }
        });
        let stop_app = app.clone();
        let stop_connection_id = connection_id.clone();
        let stop_session_id = session_id.clone();
        let task = RegisteredTask {
            session_id: session_id.clone(),
            abort: handle.abort_handle(),
            emit_stopped: Some(Box::new(move || {
                let _ = stop_app.emit(
                    PUBSUB_STATUS_EVENT,
                    PubSubStatusEvent {
                        connection_id: stop_connection_id,
                        session_id: stop_session_id,
                        state: "stopped".into(),
                        error_code: None,
                    },
                );
            })),
        };
        let ready_app = app.clone();
        let ready_connection_id = connection_id.clone();
        let ready_session_id = session_id.clone();
        if let Err(error) = self
            .registry
            .register_ready(connection_id.clone(), task, || {
                let _ = ready_app.emit(
                    PUBSUB_STATUS_EVENT,
                    PubSubStatusEvent {
                        connection_id: ready_connection_id,
                        session_id: ready_session_id,
                        state: "running".into(),
                        error_code: None,
                    },
                );
                let _ = start_tx.send(());
            })
        {
            handle.abort();
            return Err(error);
        }

        let session = PubSubSession {
            connection_id,
            session_id,
            topics,
        };
        Ok(session)
    }

    pub fn stop(&self, input: StopPubSubInput) -> Result<(), AppError> {
        input.validate()?;
        let task = self
            .registry
            .remove(&input.connection_id, Some(&input.session_id))?;
        if let Some(task) = task {
            task.stop();
        }
        Ok(())
    }

    pub fn cancel_connection(&self, connection_id: &str) {
        let task = self.registry.remove(connection_id, None).ok().flatten();
        if let Some(task) = task {
            task.stop();
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
            registry: Arc::new(TaskRegistry::default()),
        }
    }

    pub fn start<R: tauri::Runtime>(
        &self,
        app: tauri::AppHandle<R>,
        mut monitor: MonitorLineStream,
        input: StartProfilerInput,
    ) -> Result<ProfilerSession, AppError> {
        input.validate()?;

        let connection_id = input.connection_id.clone();
        let session_id = input.session_id.clone();
        let task_connection_id = connection_id.clone();
        let task_session_id = session_id.clone();
        let task_app = app.clone();
        let task_registry = Arc::clone(&self.registry);
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }
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

            if let Some(task) = task_registry.finish(&task_connection_id, &task_session_id) {
                task.complete();
            }
        });
        let stop_app = app.clone();
        let stop_connection_id = connection_id.clone();
        let stop_session_id = session_id.clone();
        let task = RegisteredTask {
            session_id: session_id.clone(),
            abort: handle.abort_handle(),
            emit_stopped: Some(Box::new(move || {
                let _ = stop_app.emit(
                    PROFILER_STATUS_EVENT,
                    ProfilerStatusEvent {
                        connection_id: stop_connection_id,
                        session_id: stop_session_id,
                        state: "stopped".into(),
                        error_code: None,
                    },
                );
            })),
        };
        let ready_app = app.clone();
        let ready_connection_id = connection_id.clone();
        let ready_session_id = session_id.clone();
        if let Err(error) = self
            .registry
            .register_ready(connection_id.clone(), task, || {
                let _ = ready_app.emit(
                    PROFILER_STATUS_EVENT,
                    ProfilerStatusEvent {
                        connection_id: ready_connection_id,
                        session_id: ready_session_id,
                        state: "running".into(),
                        error_code: None,
                    },
                );
                let _ = start_tx.send(());
            })
        {
            handle.abort();
            return Err(error);
        }

        let session = ProfilerSession {
            connection_id,
            session_id,
        };
        Ok(session)
    }

    pub fn stop(&self, input: StopProfilerInput) -> Result<(), AppError> {
        input.validate()?;
        let task = self
            .registry
            .remove(&input.connection_id, Some(&input.session_id))?;
        if let Some(task) = task {
            task.stop();
        }
        Ok(())
    }

    pub fn cancel_connection(&self, connection_id: &str) {
        let task = self.registry.remove(connection_id, None).ok().flatten();
        if let Some(task) = task {
            task.stop();
        }
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn map_pubsub_error(error: ::redis::RedisError) -> AppError {
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::{
        parse_monitor_line, parse_slow_log_reply, ProfilerManager, PubSubManager, RegisteredTask,
        TaskRegistry,
    };
    use crate::domain::{PubSubTopic, StartProfilerInput, StartPubSubInput};
    use futures_util::Stream;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::io::AsyncReadExt;

    struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    async fn assert_replacement_aborts_old_task(registry: &Arc<TaskRegistry>, prefix: &str) {
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let old = tokio::spawn(async move {
            let _drop_signal = DropSignal(Some(dropped_tx));
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        let stopped = Arc::new(AtomicUsize::new(0));
        let old_stopped = Arc::clone(&stopped);
        registry
            .register_ready(
                "shared".into(),
                RegisteredTask {
                    session_id: format!("{prefix}-old"),
                    abort: old.abort_handle(),
                    emit_stopped: Some(Box::new(move || {
                        old_stopped.fetch_add(1, Ordering::SeqCst);
                    })),
                },
                || {},
            )
            .unwrap();

        let replacement = tokio::spawn(std::future::pending::<()>());
        registry
            .register_ready(
                "shared".into(),
                RegisteredTask {
                    session_id: format!("{prefix}-new"),
                    abort: replacement.abort_handle(),
                    emit_stopped: None,
                },
                || {},
            )
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
            .await
            .expect("replaced task must be aborted")
            .unwrap();
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
        replacement.abort();
    }

    #[tokio::test]
    async fn pubsub_and_profiler_share_atomic_replacement_that_aborts_the_old_task() {
        let pubsub = PubSubManager::new();
        let profiler = ProfilerManager::new();

        assert_replacement_aborts_old_task(&pubsub.registry, "pubsub").await;
        assert_replacement_aborts_old_task(&profiler.registry, "profiler").await;
    }

    #[tokio::test]
    async fn concurrent_ready_registrations_leave_one_live_task_and_abort_the_other() {
        struct CountDrop(Arc<AtomicUsize>);
        impl Drop for CountDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let registry = Arc::new(TaskRegistry::default());
        let dropped = Arc::new(AtomicUsize::new(0));
        let first_dropped = Arc::clone(&dropped);
        let first_worker = tokio::spawn(async move {
            let _drop = CountDrop(first_dropped);
            std::future::pending::<()>().await;
        });
        let second_dropped = Arc::clone(&dropped);
        let second_worker = tokio::spawn(async move {
            let _drop = CountDrop(second_dropped);
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let first_registry = Arc::clone(&registry);
        let first_barrier = Arc::clone(&barrier);
        let first = tokio::spawn(async move {
            first_barrier.wait().await;
            first_registry.register_ready(
                "local".into(),
                RegisteredTask {
                    session_id: "first".into(),
                    abort: first_worker.abort_handle(),
                    emit_stopped: None,
                },
                || {},
            )
        });
        let second_registry = Arc::clone(&registry);
        let second_barrier = Arc::clone(&barrier);
        let second = tokio::spawn(async move {
            second_barrier.wait().await;
            second_registry.register_ready(
                "local".into(),
                RegisteredTask {
                    session_id: "second".into(),
                    abort: second_worker.abort_handle(),
                    emit_stopped: None,
                },
                || {},
            )
        });
        barrier.wait().await;
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while dropped.load(Ordering::SeqCst) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(registry.tasks.lock().unwrap().len(), 1);

        registry.remove("local", None).unwrap().unwrap().stop();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while dropped.load(Ordering::SeqCst) != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_registration_finishes_each_running_event_before_replacement() {
        let registry = Arc::new(TaskRegistry::default());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let entered = Arc::new(std::sync::Barrier::new(2));
        let resume = Arc::new(std::sync::Barrier::new(2));
        let first_registry = Arc::clone(&registry);
        let first_events = Arc::clone(&events);
        let first_stopped_events = Arc::clone(&events);
        let first_entered = Arc::clone(&entered);
        let first_resume = Arc::clone(&resume);
        let first = tokio::spawn(async move {
            let worker = tokio::spawn(std::future::pending::<()>());
            first_registry.register_ready(
                "local".into(),
                RegisteredTask {
                    session_id: "first".into(),
                    abort: worker.abort_handle(),
                    emit_stopped: Some(Box::new(move || {
                        first_stopped_events.lock().unwrap().push("first-stopped");
                    })),
                },
                move || {
                    first_events.lock().unwrap().push("first-running");
                    first_entered.wait();
                    first_resume.wait();
                },
            )
        });
        entered.wait();

        let second_registry = Arc::clone(&registry);
        let second_events = Arc::clone(&events);
        let mut second = tokio::spawn(async move {
            let worker = tokio::spawn(std::future::pending::<()>());
            second_registry.register_ready(
                "local".into(),
                RegisteredTask {
                    session_id: "second".into(),
                    abort: worker.abort_handle(),
                    emit_stopped: None,
                },
                move || second_events.lock().unwrap().push("second-running"),
            )
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
                .await
                .is_err()
        );
        resume.wait();
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["first-running", "first-stopped", "second-running"]
        );
        registry.remove("local", None).unwrap().unwrap().stop();
    }

    #[tokio::test]
    async fn pubsub_start_path_aborts_the_replaced_ready_transport() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let manager = PubSubManager::new();
        let (first_transport, mut first_peer) = tokio::io::duplex(1024);
        let redis_info = ::redis::RedisConnectionInfo::default().set_skip_set_lib_name();
        let first = ::redis::aio::PubSub::new(&redis_info, first_transport)
            .await
            .unwrap();
        manager
            .start(
                app.handle().clone(),
                first,
                StartPubSubInput {
                    connection_id: "local".into(),
                    session_id: "first".into(),
                    topics: vec![PubSubTopic {
                        name: "events".into(),
                        pattern: false,
                    }],
                },
            )
            .unwrap();

        let (second_transport, _second_peer) = tokio::io::duplex(1024);
        let second = ::redis::aio::PubSub::new(&redis_info, second_transport)
            .await
            .unwrap();
        manager
            .start(
                app.handle().clone(),
                second,
                StartPubSubInput {
                    connection_id: "local".into(),
                    session_id: "second".into(),
                    topics: vec![PubSubTopic {
                        name: "events".into(),
                        pattern: false,
                    }],
                },
            )
            .unwrap();

        let mut byte = [0_u8; 1];
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                first_peer.read(&mut byte)
            )
            .await
            .unwrap()
            .unwrap(),
            0
        );
        manager.cancel_connection("local");
    }

    struct PendingMonitor(Option<tokio::sync::oneshot::Sender<()>>);

    impl Stream for PendingMonitor {
        type Item = Result<String, crate::error::AppError>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    impl Drop for PendingMonitor {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[tokio::test]
    async fn profiler_start_path_aborts_the_replaced_ready_transport() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let manager = ProfilerManager::new();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        manager
            .start(
                app.handle().clone(),
                Box::pin(PendingMonitor(Some(dropped_tx))),
                StartProfilerInput {
                    connection_id: "local".into(),
                    session_id: "first".into(),
                },
            )
            .unwrap();
        manager
            .start(
                app.handle().clone(),
                Box::pin(PendingMonitor(None)),
                StartProfilerInput {
                    connection_id: "local".into(),
                    session_id: "second".into(),
                },
            )
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
            .await
            .expect("replaced profiler transport must be dropped")
            .unwrap();
        manager.cancel_connection("local");
    }

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
