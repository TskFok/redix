use redix_lib::{
    domain::{AnalysisAccumulator, AnalyzeDatabaseInput},
    error::AppError,
    redis::analysis_tasks::{
        AnalysisGeneration, AnalysisTaskKey, AnalysisTaskManager, AnalysisTaskScope,
        AnalysisTaskStatus, StartAnalysisTaskInput,
    },
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{Notify, RwLock};

fn input(database: u8) -> StartAnalysisTaskInput {
    StartAnalysisTaskInput {
        analysis: AnalyzeDatabaseInput {
            connection_id: "local".into(),
            pattern: "*".into(),
            delimiter: ":".into(),
            max_keys: 1000,
        },
        database,
        timeout_seconds: Some(1),
    }
}
fn scope(database: u8) -> AnalysisTaskScope {
    AnalysisTaskScope {
        connection_id: "local".into(),
        database,
    }
}
fn key(id: &str, database: u8) -> AnalysisTaskKey {
    AnalysisTaskKey {
        connection_id: "local".into(),
        database,
        task_id: id.into(),
    }
}
fn generation() -> (AnalysisGeneration, Arc<RwLock<HashMap<String, u64>>>) {
    let values = Arc::new(RwLock::new(HashMap::from([("local".into(), 7)])));
    (AnalysisGeneration::new(7, values.clone()), values)
}
async fn finish(manager: &AnalysisTaskManager, id: &str) -> AnalysisTaskStatus {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let task = manager.get(&key(id, 0)).unwrap().task;
            if task.status != AnalysisTaskStatus::Running {
                return task.status;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn completion_is_recoverable_in_memory_and_scoped_to_database() {
    let manager = AnalysisTaskManager::default();
    let (guard, _) = generation();
    let task = manager
        .start(input(0), guard, |_| async {
            Ok(AnalysisAccumulator::new(0, "*".into(), ":".into(), 1000).finish(0, 0, false))
        })
        .unwrap();
    assert_eq!(
        finish(&manager, &task.id).await,
        AnalysisTaskStatus::Completed
    );
    assert!(manager.get(&key(&task.id, 0)).unwrap().report.is_some());
    assert!(manager.list(&scope(1)).unwrap().is_empty());
    assert!(manager.get(&key(&task.id, 1)).is_err());
    assert!(manager.cancel(&key(&task.id, 1)).is_err());
}

#[tokio::test]
async fn cancellation_drops_in_flight_work_and_never_publishes_report() {
    let manager = AnalysisTaskManager::default();
    let (guard, _) = generation();
    let entered = Arc::new(Notify::new());
    let continued = Arc::new(AtomicBool::new(false));
    let task = manager
        .start(input(0), guard, {
            let entered = entered.clone();
            let continued = continued.clone();
            move |_| async move {
                entered.notify_one();
                tokio::time::sleep(Duration::from_secs(2)).await;
                continued.store(true, Ordering::Release);
                Err(AppError::CommandFailed)
            }
        })
        .unwrap();
    entered.notified().await;
    manager.cancel(&key(&task.id, 0)).unwrap();
    assert_eq!(
        finish(&manager, &task.id).await,
        AnalysisTaskStatus::Cancelled
    );
    assert!(!continued.load(Ordering::Acquire));
    assert!(manager.get(&key(&task.id, 0)).unwrap().report.is_none());
}

#[tokio::test]
async fn connection_generation_change_stops_work_before_another_scan() {
    let manager = AnalysisTaskManager::default();
    let (guard, values) = generation();
    let task = manager
        .start(input(0), guard, |control| async move {
            loop {
                control.checkpoint().await?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .unwrap();
    values.write().await.insert("local".into(), 8);
    assert_eq!(
        finish(&manager, &task.id).await,
        AnalysisTaskStatus::Cancelled
    );
    assert!(manager.get(&key(&task.id, 0)).unwrap().report.is_none());
}

#[tokio::test]
async fn deadline_reports_timeout_and_frees_running_slot() {
    let manager = AnalysisTaskManager::default();
    let (guard, _) = generation();
    let task = manager
        .start(input(0), guard, |_| std::future::pending())
        .unwrap();
    assert_eq!(
        finish(&manager, &task.id).await,
        AnalysisTaskStatus::TimedOut
    );
    assert!(manager.get(&key(&task.id, 0)).unwrap().report.is_none());
}

#[tokio::test]
async fn duplicate_scope_is_rejected_and_mismatched_result_is_not_published() {
    let manager = AnalysisTaskManager::default();
    let (guard, _) = generation();
    let task = manager
        .start(input(0), guard.clone(), |_| std::future::pending())
        .unwrap();
    assert!(manager
        .start(input(0), guard.clone(), |_| std::future::pending())
        .is_err());
    manager.cancel(&key(&task.id, 0)).unwrap();
    finish(&manager, &task.id).await;
    let task = manager
        .start(input(0), guard, |_| async {
            Ok(AnalysisAccumulator::new(1, "*".into(), ":".into(), 1000).finish(0, 0, false))
        })
        .unwrap();
    assert_eq!(finish(&manager, &task.id).await, AnalysisTaskStatus::Failed);
    assert!(manager.get(&key(&task.id, 0)).unwrap().report.is_none());
}
