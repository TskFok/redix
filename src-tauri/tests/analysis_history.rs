use redix_lib::{
    domain::{AnalysisAccumulator, DatabaseAnalysisReport},
    error::AppError,
    persistence::analysis_history::{
        AnalysisHistoryStore, SaveAnalysisInput, MAX_HISTORY_PER_DATABASE,
    },
};

fn report(database: u8) -> DatabaseAnalysisReport {
    AnalysisAccumulator::new(database, "*".into(), ":".into(), 1000).finish(0, 0, false)
}

fn input(connection: &str, database: u8) -> SaveAnalysisInput {
    SaveAnalysisInput {
        connection_id: connection.into(),
        report: report(database),
    }
}

#[test]
fn saves_reloads_and_isolates_connection_and_database() {
    let dir = tempfile::tempdir().unwrap();
    let store = AnalysisHistoryStore::new(dir.path().join("analysis-history.json"));
    let item = store.save(input("one", 0)).unwrap();
    store.save(input("one", 1)).unwrap();
    store.save(input("two", 0)).unwrap();
    let reopened = AnalysisHistoryStore::new(dir.path().join("analysis-history.json"));
    assert_eq!(reopened.list("one", 0).unwrap().len(), 1);
    assert_eq!(reopened.get("one", 0, &item.id).unwrap().report, report(0));
    assert_eq!(
        reopened.get("two", 0, &item.id),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        reopened.delete("one", 1, &item.id),
        Err(AppError::InvalidInput)
    );
    reopened.delete("one", 0, &item.id).unwrap();
    assert!(reopened.list("one", 0).unwrap().is_empty());
    assert_eq!(reopened.list("two", 0).unwrap().len(), 1);
}

#[test]
fn corrupt_and_unknown_version_history_are_not_silently_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis-history.json");
    let store = AnalysisHistoryStore::new(path.clone());
    for raw in ["broken-json", r#"{"version":99,"items":[]}"#] {
        std::fs::write(&path, raw).unwrap();
        assert_eq!(
            store.save(input("one", 0)),
            Err(AppError::PersistenceFailed)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    }
}

#[test]
fn limits_reports_and_does_not_evict_saved_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = AnalysisHistoryStore::new(dir.path().join("history.json"));
    let mut invalid = input("one", 0);
    invalid.report.pattern = "x".repeat(513);
    assert_eq!(store.save(invalid), Err(AppError::InvalidInput));
    for _ in 0..MAX_HISTORY_PER_DATABASE {
        store.save(input("one", 0)).unwrap();
    }
    assert_eq!(store.save(input("one", 0)), Err(AppError::InvalidInput));
    assert_eq!(
        store.list("one", 0).unwrap().len(),
        MAX_HISTORY_PER_DATABASE
    );
    store.save(input("two", 0)).unwrap();
}

#[test]
fn concurrent_saves_preserve_every_report() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(AnalysisHistoryStore::new(dir.path().join("history.json")));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || store.save(input("one", 0)).unwrap())
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(store.list("one", 0).unwrap().len(), 8);
}
