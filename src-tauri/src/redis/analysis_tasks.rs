use crate::{
    domain::{AnalysisProgress, AnalyzeDatabaseInput, DatabaseAnalysisReport},
    error::AppError,
};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

const MAX_TASKS: usize = 16;
const MAX_RUNNING: usize = 2;
const MAX_REPORT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartAnalysisTaskInput {
    pub analysis: AnalyzeDatabaseInput,
    pub database: u8,
    pub timeout_seconds: Option<u64>,
}
impl StartAnalysisTaskInput {
    pub fn validate(&self) -> Result<(), AppError> {
        self.analysis.validate()?;
        validate_id(&self.analysis.connection_id)?;
        if self.analysis.pattern.is_empty()
            || !(1..=900).contains(&self.timeout_seconds.unwrap_or(300))
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisTaskScope {
    pub connection_id: String,
    pub database: u8,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisTaskKey {
    pub connection_id: String,
    pub database: u8,
    pub task_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisTaskStatus {
    Running,
    Completed,
    PartialFailure,
    Cancelled,
    TimedOut,
    Failed,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct AnalysisTask {
    pub id: String,
    pub connection_id: String,
    pub database: u8,
    pub analysis: AnalyzeDatabaseInput,
    pub status: AnalysisTaskStatus,
    pub progress: AnalysisProgress,
    pub nodes_total: usize,
    pub nodes_completed: usize,
    pub error_code: Option<String>,
    pub started_at: u64,
    pub cancel_requested: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct AnalysisTaskResult {
    pub task: AnalysisTask,
    pub report: Option<DatabaseAnalysisReport>,
}
struct TaskState {
    task: AnalysisTask,
    report: Option<DatabaseAnalysisReport>,
    nodes: HashMap<String, AnalysisProgress>,
    finished_nodes: HashSet<String>,
}
struct TaskEntry {
    state: Mutex<TaskState>,
    cancelled: AtomicBool,
}
#[derive(Clone)]
pub struct AnalysisGeneration {
    token: u64,
    values: Arc<RwLock<HashMap<String, u64>>>,
}
impl AnalysisGeneration {
    pub fn new(token: u64, values: Arc<RwLock<HashMap<String, u64>>>) -> Self {
        Self { token, values }
    }
}
#[derive(Clone)]
pub struct AnalysisControl {
    entry: Arc<TaskEntry>,
    generation: AnalysisGeneration,
    connection_id: String,
}
impl AnalysisControl {
    pub async fn checkpoint(&self) -> Result<(), AppError> {
        if self.entry.cancelled.load(Ordering::Acquire)
            || self
                .generation
                .values
                .read()
                .await
                .get(&self.connection_id)
                .copied()
                .unwrap_or(0)
                != self.generation.token
        {
            return Err(AppError::OperationCancelled);
        }
        Ok(())
    }
    pub fn set_nodes_total(&self, total: usize) {
        if let Ok(mut state) = self.entry.state.lock() {
            state.task.nodes_total = total;
        }
    }
    pub fn node_finished(&self, node: &str) {
        if let Ok(mut state) = self.entry.state.lock() {
            state.finished_nodes.insert(node.to_owned());
            state.task.nodes_completed = state.finished_nodes.len();
        }
    }
    pub fn progress(&self, node: &str, progress: AnalysisProgress) {
        if let Ok(mut state) = self.entry.state.lock() {
            state.nodes.insert(node.to_owned(), progress);
            state.task.progress.scanned = state
                .nodes
                .values()
                .fold(0_u64, |sum, node| sum.saturating_add(node.scanned));
            state.task.progress.processed = state
                .nodes
                .values()
                .fold(0_u64, |sum, node| sum.saturating_add(node.processed));
            state.task.progress.truncated = state.nodes.values().any(|node| node.truncated);
        }
    }
}
#[derive(Default)]
pub struct AnalysisTaskManager {
    tasks: Mutex<Vec<Arc<TaskEntry>>>,
}
impl Drop for AnalysisTaskManager {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.lock() {
            for task in tasks.iter() {
                task.cancelled.store(true, Ordering::Release);
            }
        }
    }
}
fn validate_id(id: &str) -> Result<(), AppError> {
    if id.trim().is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}
impl AnalysisTaskManager {
    pub fn list(&self, scope: &AnalysisTaskScope) -> Result<Vec<AnalysisTask>, AppError> {
        validate_id(&scope.connection_id)?;
        let tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        let mut result = Vec::new();
        for entry in tasks.iter().rev() {
            let state = entry.state.lock().map_err(|_| AppError::CommandFailed)?;
            if state.task.connection_id == scope.connection_id
                && state.task.database == scope.database
            {
                result.push(state.task.clone());
            }
        }
        Ok(result)
    }
    fn find(&self, key: &AnalysisTaskKey) -> Result<Arc<TaskEntry>, AppError> {
        validate_id(&key.connection_id)?;
        validate_id(&key.task_id)?;
        let tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        for entry in tasks.iter() {
            let state = entry.state.lock().map_err(|_| AppError::CommandFailed)?;
            if state.task.id == key.task_id
                && state.task.connection_id == key.connection_id
                && state.task.database == key.database
            {
                return Ok(entry.clone());
            }
        }
        Err(AppError::InvalidInput)
    }
    pub fn get(&self, key: &AnalysisTaskKey) -> Result<AnalysisTaskResult, AppError> {
        let entry = self.find(key)?;
        let state = entry.state.lock().map_err(|_| AppError::CommandFailed)?;
        Ok(AnalysisTaskResult {
            task: state.task.clone(),
            report: state.report.clone(),
        })
    }
    pub fn cancel(&self, key: &AnalysisTaskKey) -> Result<(), AppError> {
        let entry = self.find(key)?;
        let mut state = entry.state.lock().map_err(|_| AppError::CommandFailed)?;
        if state.task.status == AnalysisTaskStatus::Running {
            entry.cancelled.store(true, Ordering::Release);
            state.task.cancel_requested = true;
        }
        Ok(())
    }
    pub fn start<F, Fut>(
        &self,
        input: StartAnalysisTaskInput,
        generation: AnalysisGeneration,
        execute: F,
    ) -> Result<AnalysisTask, AppError>
    where
        F: FnOnce(AnalysisControl) -> Fut + Send + 'static,
        Fut: Future<Output = Result<DatabaseAnalysisReport, AppError>> + Send + 'static,
    {
        input.validate()?;
        let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        let mut running = 0;
        let mut removable = None;
        for (index, entry) in tasks.iter().enumerate() {
            let state = entry.state.lock().map_err(|_| AppError::CommandFailed)?;
            if state.task.status == AnalysisTaskStatus::Running {
                running += 1;
                if state.task.connection_id == input.analysis.connection_id
                    && state.task.database == input.database
                {
                    return Err(AppError::InvalidInput);
                }
            } else if removable.is_none() {
                removable = Some(index);
            }
        }
        if running >= MAX_RUNNING {
            return Err(AppError::InvalidInput);
        }
        if tasks.len() >= MAX_TASKS {
            tasks.remove(removable.ok_or(AppError::InvalidInput)?);
        }
        let task = AnalysisTask {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: input.analysis.connection_id.clone(),
            database: input.database,
            analysis: input.analysis.clone(),
            status: AnalysisTaskStatus::Running,
            progress: AnalysisProgress {
                scanned: 0,
                processed: 0,
                max_keys: input.analysis.max_keys,
                truncated: false,
            },
            nodes_total: 1,
            nodes_completed: 0,
            error_code: None,
            cancel_requested: false,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };
        let entry = Arc::new(TaskEntry {
            state: Mutex::new(TaskState {
                task: task.clone(),
                report: None,
                nodes: HashMap::new(),
                finished_nodes: HashSet::new(),
            }),
            cancelled: AtomicBool::new(false),
        });
        tasks.push(entry.clone());
        let control = AnalysisControl {
            entry: entry.clone(),
            generation,
            connection_id: input.analysis.connection_id.clone(),
        };
        tokio::spawn(async move {
            let stopped = async {
                loop {
                    control.checkpoint().await?;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                #[allow(unreachable_code)]
                Ok::<(), AppError>(())
            };
            let outcome = tokio::select! {
                biased;
                _ = stopped => Err((AnalysisTaskStatus::Cancelled, "OPERATION_CANCELLED")),
                _ = tokio::time::sleep(Duration::from_secs(input.timeout_seconds.unwrap_or(300))) => Err((AnalysisTaskStatus::TimedOut, "ANALYSIS_TIMEOUT")),
                result = async { control.checkpoint().await?; execute(control.clone()).await } => result.map_err(|error| (if error == AppError::OperationCancelled { AnalysisTaskStatus::Cancelled } else { AnalysisTaskStatus::Failed }, error.code())),
            };
            // The generation read guard covers final publication; lifecycle writers invalidate it first.
            let generations = control.generation.values.read().await;
            let Ok(mut state) = entry.state.lock() else {
                return;
            };
            if entry.cancelled.load(Ordering::Acquire)
                || generations
                    .get(&control.connection_id)
                    .copied()
                    .unwrap_or(0)
                    != control.generation.token
            {
                state.task.status = AnalysisTaskStatus::Cancelled;
                state.task.cancel_requested = true;
                state.task.error_code = Some("OPERATION_CANCELLED".into());
                return;
            }
            match outcome {
                Ok(report) => {
                    if report.database != input.database
                        || report.pattern != input.analysis.pattern
                        || report.delimiter != input.analysis.delimiter
                        || report.progress.max_keys != input.analysis.max_keys
                        || serde_json::to_vec(&report)
                            .map_or(true, |bytes| bytes.len() > MAX_REPORT_BYTES)
                    {
                        state.task.status = AnalysisTaskStatus::Failed;
                        state.task.error_code = Some("INVALID_ANALYSIS_RESULT".into());
                    } else if !report.failed_nodes.is_empty() && report.node_results.is_empty() {
                        state.task.status = AnalysisTaskStatus::Failed;
                        state.task.error_code = Some("CLUSTER_NODE_UNAVAILABLE".into());
                    } else {
                        state.task.status = if report.failed_nodes.is_empty() {
                            AnalysisTaskStatus::Completed
                        } else {
                            AnalysisTaskStatus::PartialFailure
                        };
                        state.task.progress = report.progress.clone();
                        state.report = Some(report);
                    }
                }
                Err((status, code)) => {
                    state.task.status = status;
                    state.task.error_code = Some(code.into());
                }
            }
        });
        Ok(task)
    }
}
