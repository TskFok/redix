use redix_lib::{
    domain::{AnalysisAccumulator, DatabaseAnalysisReport},
    error::AppError,
    persistence::analysis_history::{
        AnalysisHistoryStore, SaveAnalysisInput, MAX_HISTORY_PER_DATABASE,
    },
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

#[cfg(unix)]
#[test]
fn saved_analysis_history_is_readable_only_by_its_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis-history.json");
    let store = AnalysisHistoryStore::new(path.clone());

    store.save(input("one", 0)).unwrap();

    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn reading_existing_history_restricts_permissions_without_rewriting_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis-history.json");
    let store = AnalysisHistoryStore::new(path.clone());
    let item = store.save(input("one", 0)).unwrap();
    let original = std::fs::read(&path).unwrap();

    for read_details in [false, true] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        if read_details {
            assert_eq!(store.get("one", 0, &item.id).unwrap().id, item.id);
        } else {
            assert_eq!(store.list("one", 0).unwrap().len(), 1);
        }

        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "reading history must restrict permissions, read_details={read_details}"
        );
    }
}

#[cfg(unix)]
#[test]
fn reading_history_removes_legacy_temporary_files_but_preserves_unrelated_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis-history.json");
    let store = AnalysisHistoryStore::new(path.clone());
    store.save(input("one", 0)).unwrap();
    let orphan = dir.path().join(".analysis-history.json.123.0.tmp");
    std::fs::write(&orphan, "private analysis report").unwrap();
    let unrelated = dir.path().join(".workbench-history.json.123.0.tmp");
    std::fs::write(&unrelated, "keep unrelated history").unwrap();
    let unrecognized = dir.path().join(".analysis-history.json.backup.tmp");
    std::fs::write(&unrecognized, "keep unrecognized backup").unwrap();

    assert_eq!(store.list("one", 0).unwrap().len(), 1);

    assert_eq!(
        std::fs::read_to_string(&unrelated).unwrap(),
        "keep unrelated history"
    );
    assert_eq!(
        std::fs::read_to_string(&unrecognized).unwrap(),
        "keep unrecognized backup"
    );
    assert!(!orphan.exists(), "legacy temporary history must be removed");
}
