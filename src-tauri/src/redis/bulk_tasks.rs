use crate::error::AppError;
use std::{
    collections::HashSet,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// Deliberately excludes ClusterConnection: it retries ambiguous write failures.
#[derive(Clone)]
pub(crate) enum BulkDeleteTarget {
    Single(super::standalone_transport::ManagedMultiplexedConnection),
    Cluster {
        owners: Arc<std::collections::HashMap<::redis::cluster_routing::Slot, usize>>,
        nodes: Arc<Vec<Result<::redis::aio::MultiplexedConnection, AppError>>>,
    },
}
impl BulkDeleteTarget {
    pub(crate) async fn delete(&self, key: &str) -> Result<u64, AppError> {
        let command = ::redis::cmd("UNLINK");
        let mut command = command;
        command.arg(key);
        match self {
            Self::Single(connection) => command.query_async(&mut connection.clone()).await,
            Self::Cluster { owners, nodes } => {
                let owner = owners
                    .get(&::redis::cluster_routing::Slot::for_key(key))
                    .ok_or(AppError::ClusterTopologyFailed)?;
                let mut connection = nodes[*owner].clone()?;
                // A MOVED/ASK or socket error is returned to the task. No replay or failover.
                command.query_async(&mut connection).await
            }
        }
        .map_err(super::connection_manager::map_command_error)
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct StartBulkDeleteInput {
    pub connection_id: String,
    pub keys: Vec<String>,
}
impl StartBulkDeleteInput {
    pub(crate) fn normalize(mut self) -> Result<Self, AppError> {
        if self.connection_id.is_empty()
            || self.connection_id.len() > 256
            || self.keys.is_empty()
            || self.keys.len() > 10_000
            || self.keys.iter().any(|key| key.len() > 16_384)
            || self.keys.iter().map(String::len).sum::<usize>() > 4 * 1024 * 1024
        {
            return Err(AppError::InvalidInput);
        }
        let mut seen = HashSet::new();
        self.keys.retain(|key| seen.insert(key.clone()));
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkTaskStatus {
    Running,
    Completed,
    PartialFailure,
    Cancelled,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct BulkTask {
    pub id: String,
    pub connection_id: String,
    pub total: usize,
    pub processed: usize,
    pub deleted: u64,
    /// Redis/network errors may have happened after applying UNLINK; no automatic retry.
    pub failed: usize,
    pub status: BulkTaskStatus,
    pub cancel_requested: bool,
}
struct TaskEntry {
    summary: Mutex<BulkTask>,
    cancelled: AtomicBool,
}
#[derive(Default)]
pub struct BulkTaskManager {
    tasks: Mutex<Vec<Arc<TaskEntry>>>,
}
impl Drop for BulkTaskManager {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.lock() {
            for task in tasks.iter() {
                task.cancelled.store(true, Ordering::Release);
            }
        }
    }
}
impl BulkTaskManager {
    pub fn list(&self) -> Result<Vec<BulkTask>, AppError> {
        self.tasks
            .lock()
            .map_err(|_| AppError::CommandFailed)?
            .iter()
            .map(|entry| {
                entry
                    .summary
                    .lock()
                    .map(|summary| summary.clone())
                    .map_err(|_| AppError::CommandFailed)
            })
            .collect()
    }
    pub fn cancel(&self, id: &str) -> Result<(), AppError> {
        let tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        for entry in tasks.iter() {
            let mut summary = entry.summary.lock().map_err(|_| AppError::CommandFailed)?;
            if summary.id == id {
                if summary.status == BulkTaskStatus::Running {
                    entry.cancelled.store(true, Ordering::Release);
                    summary.cancel_requested = true;
                }
                return Ok(());
            }
        }
        Err(AppError::InvalidInput)
    }
    pub fn start<F, Fut>(
        &self,
        input: StartBulkDeleteInput,
        mut execute: F,
    ) -> Result<BulkTask, AppError>
    where
        F: FnMut(String) -> Fut + Send + 'static,
        Fut: Future<Output = Result<u64, AppError>> + Send + 'static,
    {
        let input = input.normalize()?;
        let mut tasks = self.tasks.lock().map_err(|_| AppError::CommandFailed)?;
        let mut running = 0;
        let mut removable = None;
        for (index, entry) in tasks.iter().enumerate() {
            if entry
                .summary
                .lock()
                .map_err(|_| AppError::CommandFailed)?
                .status
                == BulkTaskStatus::Running
            {
                running += 1;
            } else if removable.is_none() {
                removable = Some(index);
            }
        }
        if running >= 4 {
            return Err(AppError::InvalidInput);
        }
        if tasks.len() >= 20 {
            tasks.remove(removable.ok_or(AppError::InvalidInput)?);
        }
        let summary = BulkTask {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: input.connection_id,
            total: input.keys.len(),
            processed: 0,
            deleted: 0,
            failed: 0,
            status: BulkTaskStatus::Running,
            cancel_requested: false,
        };
        let entry = Arc::new(TaskEntry {
            summary: Mutex::new(summary.clone()),
            cancelled: AtomicBool::new(false),
        });
        tasks.push(entry.clone());
        // Each command has one key, so Cluster routes every write to that key's primary.
        // Keep the original connection handle in execute; never resolve a reconnected profile here.
        tokio::spawn(async move {
            for key in input.keys {
                if entry.cancelled.load(Ordering::Acquire) {
                    break;
                }
                let result = execute(key).await;
                let Ok(mut task) = entry.summary.lock() else {
                    return;
                };
                if result == Err(AppError::OperationCancelled) {
                    entry.cancelled.store(true, Ordering::Release);
                    task.cancel_requested = true;
                    break;
                }
                task.processed += 1;
                match result {
                    Ok(deleted) => task.deleted += deleted,
                    Err(_) => task.failed += 1,
                }
            }
            if let Ok(mut task) = entry.summary.lock() {
                task.status = if task.processed == task.total {
                    if task.failed == 0 {
                        BulkTaskStatus::Completed
                    } else {
                        BulkTaskStatus::PartialFailure
                    }
                } else {
                    BulkTaskStatus::Cancelled
                };
            };
        });
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn input(keys: &[&str]) -> StartBulkDeleteInput {
        StartBulkDeleteInput {
            connection_id: "local".into(),
            keys: keys.iter().map(|key| key.to_string()).collect(),
        }
    }

    async fn finish(manager: &BulkTaskManager, id: &str) -> BulkTask {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let task = manager
                    .list()
                    .unwrap()
                    .into_iter()
                    .find(|task| task.id == id)
                    .unwrap();
                if task.status != BulkTaskStatus::Running {
                    return task;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("task must finish")
    }

    #[tokio::test]
    async fn deduplicates_and_reports_partial_failure_without_retry() {
        let manager = BulkTaskManager::default();
        let task = manager
            .start(input(&["a", "a", "bad", "missing"]), |key| async move {
                if key == "bad" {
                    Err(AppError::CommandFailed)
                } else {
                    Ok(u64::from(key != "missing"))
                }
            })
            .unwrap();
        let result = finish(&manager, &task.id).await;
        assert_eq!(result.total, 3);
        assert_eq!(result.processed, 3);
        assert_eq!(result.deleted, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.status, BulkTaskStatus::PartialFailure);
    }

    #[tokio::test]
    async fn cancellation_waits_for_inflight_result_and_prevents_next_key() {
        let manager = BulkTaskManager::default();
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let task = manager
            .start(input(&["first", "second"]), {
                let started = started.clone();
                let release = release.clone();
                move |_| {
                    let started = started.clone();
                    let release = release.clone();
                    async move {
                        started.notify_one();
                        release.notified().await;
                        Ok(1)
                    }
                }
            })
            .unwrap();
        started.notified().await;
        manager.cancel(&task.id).unwrap();
        assert!(manager.list().unwrap()[0].cancel_requested);
        release.notify_one();
        let result = finish(&manager, &task.id).await;
        assert_eq!(result.status, BulkTaskStatus::Cancelled);
        assert_eq!(result.processed, 1);
        assert_eq!(result.deleted, 1);
    }

    #[tokio::test]
    async fn obsolete_connection_stops_without_counting_unattempted_keys() {
        let manager = BulkTaskManager::default();
        let task = manager
            .start(input(&["a", "b"]), |_| async {
                Err(AppError::OperationCancelled)
            })
            .unwrap();
        let result = finish(&manager, &task.id).await;
        assert_eq!(result.processed, 0);
        assert_eq!(result.status, BulkTaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn bounds_input_and_active_tasks() {
        let manager = BulkTaskManager::default();
        assert!(manager.start(input(&[]), |_| async { Ok(1) }).is_err());
        assert!(manager
            .start(input(&[&"a".repeat(16_385)]), |_| async { Ok(1) })
            .is_err());
        for _ in 0..4 {
            manager
                .start(input(&["a"]), |_| {
                    std::future::pending::<Result<u64, AppError>>()
                })
                .unwrap();
        }
        assert!(manager.start(input(&["a"]), |_| async { Ok(1) }).is_err());
    }
}
