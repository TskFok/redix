mod support;

use redix_lib::{
    commands::{local_products::*, query_library::save_query_library_item_inner},
    domain::{local_products::*, QueryLibraryItemInput},
    persistence::{JsonProfileRepository, ProfileRepository, SystemKeyring},
    AppState,
};
use std::sync::Arc;

fn state(path: &std::path::Path) -> AppState {
    let profiles = Arc::new(JsonProfileRepository::new(path.join("connections.json")));
    profiles.save(&[support::valid_profile()]).unwrap();
    AppState::with_data_dir(profiles, Arc::new(SystemKeyring::new()), path.into())
}

fn package(command: &str) -> String {
    serde_json::json!({"format":"redix-query-library","version":1,"items":[{"name":"读取","command":command,"tags":["示例"]}]}).to_string()
}

#[test]
fn package_roundtrip_appends_without_reusing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let first = import_query_package_inner(&state, package("GET one")).unwrap();
    let document = export_query_package_inner(&state).unwrap();
    let second =
        import_query_package_inner(&state, serde_json::to_string(&document).unwrap()).unwrap();
    assert_ne!(first[0].id, second[0].id);
    assert_eq!(export_query_package_inner(&state).unwrap().items.len(), 2);
}

#[test]
fn invalid_package_and_sensitive_later_lines_preserve_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    import_query_package_inner(&state, package("GET public")).unwrap();
    let path = dir.path().join("query-library.json");
    let original = std::fs::read(&path).unwrap();
    for content in [
        "bad json".into(),
        package("GET public\nAUTH secret"),
        package("# heading\n\"ACL\" SETUSER user >secret"),
        package("GET public\nCONFIG SET requirepass secret"),
        package("GET public\nHELLO 3 AUTH user pass"),
        package("GET x").replace("\"version\":1", "\"version\":2"),
    ] {
        assert!(import_query_package_inner(&state, content).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    let content = serde_json::json!({"format":"redix-query-library","version":1,"items":[{"name":"valid","command":"GET a","tags":[]},{"name":"invalid","command":"AUTH secret","tags":[]}]}).to_string();
    assert!(import_query_package_inner(&state, content).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn corrupt_storage_is_not_overwritten_by_import_or_normal_save() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let path = dir.path().join("query-library.json");
    std::fs::write(&path, "broken").unwrap();
    assert!(import_query_package_inner(&state, package("GET x")).is_err());
    assert!(save_query_library_item_inner(
        &state,
        QueryLibraryItemInput {
            id: None,
            name: "read".into(),
            command: "GET x".into(),
            tags: vec![]
        }
    )
    .is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "broken");
}

#[test]
fn tags_validate_unique_keys_persist_and_require_existing_connection() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let tag = ConnectionTag {
        key: " env ".into(),
        value: " prod ".into(),
    };
    let saved = save_connection_tags_inner(
        &state,
        SaveConnectionTagsInput {
            connection_id: "local".into(),
            tags: vec![tag.clone()],
        },
    )
    .unwrap();
    assert_eq!(saved[0].key, "env");
    assert_eq!(list_connection_tags_inner(&state).unwrap()["local"], saved);
    assert!(save_connection_tags_inner(
        &state,
        SaveConnectionTagsInput {
            connection_id: "local".into(),
            tags: vec![tag.clone(), tag.clone()]
        }
    )
    .is_err());
    assert!(save_connection_tags_inner(
        &state,
        SaveConnectionTagsInput {
            connection_id: "missing".into(),
            tags: vec![tag]
        }
    )
    .is_err());
    save_connection_tags_inner(
        &state,
        SaveConnectionTagsInput {
            connection_id: "local".into(),
            tags: vec![],
        },
    )
    .unwrap();
    assert!(!list_connection_tags_inner(&state)
        .unwrap()
        .contains_key("local"));
}

#[test]
fn concurrent_package_imports_preserve_all_items() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(state(dir.path()));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let state = state.clone();
            std::thread::spawn(move || {
                import_query_package_inner(&state, package("GET x")).unwrap()
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(export_query_package_inner(&state).unwrap().items.len(), 8);
}

#[test]
fn full_library_rejects_whole_package_without_eviction() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let item = serde_json::json!({"name":"read","command":"GET x","tags":[]});
    let content =
        serde_json::json!({"format":"redix-query-library","version":1,"items":vec![item; 500]})
            .to_string();
    assert_eq!(
        import_query_package_inner(&state, content).unwrap().len(),
        500
    );
    let path = dir.path().join("query-library.json");
    let original = std::fs::read(&path).unwrap();
    assert!(import_query_package_inner(&state, package("GET another")).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn package_and_regular_editor_saves_share_a_transaction_lock() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(state(dir.path()));
    let threads: Vec<_> = (0..12)
        .map(|index| {
            let state = state.clone();
            std::thread::spawn(move || {
                if index % 2 == 0 {
                    import_query_package_inner(&state, package("GET imported")).unwrap();
                } else {
                    save_query_library_item_inner(
                        &state,
                        QueryLibraryItemInput {
                            id: None,
                            name: "editor".into(),
                            command: "GET editor".into(),
                            tags: vec![],
                        },
                    )
                    .unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(export_query_package_inner(&state).unwrap().items.len(), 12);
}

#[test]
fn tags_corruption_cannot_be_overwritten_and_deleted_profiles_are_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let path = dir.path().join("connection-tags.json");
    std::fs::write(&path, "broken").unwrap();
    assert!(save_connection_tags_inner(
        &state,
        SaveConnectionTagsInput {
            connection_id: "local".into(),
            tags: vec![]
        }
    )
    .is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    std::fs::write(
        &path,
        r#"{"version":1,"connections":{"deleted":[{"key":"env","value":"prod"}]}}"#,
    )
    .unwrap();
    assert!(list_connection_tags_inner(&state).unwrap().is_empty());
}
