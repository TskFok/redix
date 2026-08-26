use std::sync::Arc;

use tauri::Manager;

use redix_lib::{
    commands::{
        browser, connections, database, json, observability, query_library, settings, workbench,
    },
    domain::{
        AppSettings, CommandHistoryEntry, CommandResult, QueryLibraryItemInput,
        SaveCommandHistoryInput,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    AppState,
};

struct EmptyProfiles;

impl ProfileRepository for EmptyProfiles {
    fn load(&self) -> Result<Vec<redix_lib::domain::ConnectionProfile>, AppError> {
        Ok(Vec::new())
    }

    fn save(&self, _profiles: &[redix_lib::domain::ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct EmptySecrets;

impl SecretStore for EmptySecrets {
    fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(None)
    }

    fn write(&self, _connection_id: &str, _secrets: &ConnectionSecrets) -> Result<(), AppError> {
        Ok(())
    }

    fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
        Ok(())
    }
}

#[test]
fn exposes_all_tauri_command_adapters() {
    let _ = browser::scan_keys;
    let _ = browser::get_key;
    let _ = browser::set_key;
    let _ = browser::delete_key;
    let _ = browser::set_key_ttl;
    let _ = browser::create_key;
    let _ = browser::rename_key;
    let _ = browser::delete_keys;
    let _ = browser::get_key_info;
    let _ = browser::get_stream_consumer_groups;
    let _ = browser::create_stream_consumer_group;
    let _ = browser::delete_stream_consumer_group;
    let _ = browser::get_stream_consumers;
    let _ = browser::get_stream_pending_entries;
    let _ = browser::acknowledge_stream_pending_entries;
    let _ = browser::delete_stream_consumer;
    let _ = browser::export_keys;
    let _ = browser::import_keys;
    let _ = connections::list_connections;
    let _ = connections::save_connection;
    let _ = connections::delete_connection;
    let _ = connections::export_connections;
    let _ = connections::import_connections;
    let _ = connections::test_connection;
    let _ = connections::open_connection;
    let _ = connections::close_connection;
    let _ = database::get_instance_overview;
    let _ = database::get_instance_details;
    let _ = database::get_database_overview;
    let _ = database::analyze_database;
    let _ = database::select_database;
    let _ = json::get_module_capabilities;
    let _ = json::get_json_path;
    let _ = json::set_json_path;
    let _ = json::append_json_array;
    let _ = json::delete_json_path;
    let _ = observability::get_slow_logs;
    let _ = observability::clear_slow_logs;
    let _ = observability::get_slow_log_config;
    let _ = observability::update_slow_log_config;
    let _ = observability::start_pub_sub;
    let _ = observability::stop_pub_sub;
    let _ = observability::publish_pub_sub;
    let _ = observability::start_profiler;
    let _ = observability::stop_profiler;
    let _ = query_library::list_query_library;
    let _ = query_library::save_query_library_item;
    let _ = query_library::delete_query_library_item;
    let _ = settings::get_app_settings;
    let _ = settings::save_app_settings;
    let _ = workbench::execute_command;
    let _ = workbench::execute_commands;
    let _ = workbench::get_command_catalog;
    let _ = workbench::list_command_history;
    let _ = workbench::save_command_history;
}

#[test]
fn open_connection_accepts_snake_case_connection_id_from_frontend() {
    let app = tauri::test::mock_builder()
        .manage(AppState::new(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
        ))
        .invoke_handler(tauri::generate_handler![connections::open_connection])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("test app must build");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview must build");

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "open_connection".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: serde_json::json!({ "connection_id": "missing" }).into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_owned(),
        },
    )
    .expect_err("missing profile should reach the command and return AppError");

    assert_eq!(
        response,
        serde_json::json!({
            "code": "INVALID_CONNECTION",
            "message": "连接配置无效"
        })
    );
}

#[test]
fn observability_commands_reach_redis_service_and_validate_inputs() {
    let app = tauri::test::mock_builder()
        .manage(AppState::new(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
        ))
        .invoke_handler(tauri::generate_handler![observability::get_slow_logs])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("test app must build");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview must build");

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "get_slow_logs".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: serde_json::json!({
                "input": {"connection_id": "missing", "count": 10}
            })
            .into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_owned(),
        },
    )
    .expect_err("missing active connection should return a typed Redis error");
    assert_eq!(
        response,
        serde_json::json!({
            "code": "CONNECTION_FAILED",
            "message": "无法连接到 Redis 服务器"
        })
    );
}

#[test]
fn command_history_commands_isolate_connections_and_filter_sensitive_entries() {
    let directory = tempfile::tempdir().expect("history directory must be created");
    let app = tauri::test::mock_builder()
        .manage(AppState::with_data_dir(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
            directory.path().to_path_buf(),
        ))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("test app must build");

    workbench::save_command_history(
        app.state(),
        SaveCommandHistoryInput {
            connection_id: "local".into(),
            entries: vec![
                CommandHistoryEntry {
                    connection_id: "local".into(),
                    command: "PING".into(),
                    result: Some(CommandResult {
                        kind: "string".into(),
                        value: serde_json::json!("PONG"),
                    }),
                    error_code: None,
                    created_at: "2026-08-19T00:00:00Z".into(),
                },
                CommandHistoryEntry {
                    connection_id: "local".into(),
                    command: "AUTH secret".into(),
                    result: None,
                    error_code: Some("COMMAND_FAILED".into()),
                    created_at: "2026-08-19T00:00:01Z".into(),
                },
            ],
        },
    )
    .expect("local history must save");

    workbench::save_command_history(
        app.state(),
        SaveCommandHistoryInput {
            connection_id: "remote".into(),
            entries: vec![CommandHistoryEntry {
                connection_id: "remote".into(),
                command: "DBSIZE".into(),
                result: Some(CommandResult {
                    kind: "number".into(),
                    value: serde_json::json!(2),
                }),
                error_code: None,
                created_at: "2026-08-19T00:00:02Z".into(),
            }],
        },
    )
    .expect("remote history must save");

    let local = workbench::list_command_history(app.state(), "local".into()).unwrap();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].command, "PING");
    assert_eq!(
        workbench::list_command_history(app.state(), "remote".into())
            .unwrap()
            .into_iter()
            .map(|entry| entry.command)
            .collect::<Vec<_>>(),
        vec!["DBSIZE"]
    );
}

#[test]
fn local_query_library_and_settings_commands_support_crud_and_safe_defaults() {
    let directory = tempfile::tempdir().expect("local resource directory must be created");
    let app = tauri::test::mock_builder()
        .manage(AppState::with_data_dir(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
            directory.path().to_path_buf(),
        ))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("test app must build");

    let saved = query_library::save_query_library_item(
        app.state(),
        QueryLibraryItemInput {
            id: None,
            name: "读取用户".into(),
            command: "GET user:1".into(),
            tags: vec!["用户".into()],
        },
    )
    .expect("query library item must save");
    assert!(!saved.id.is_empty());
    assert_eq!(
        query_library::list_query_library(app.state())
            .unwrap()
            .len(),
        1
    );

    let updated = query_library::save_query_library_item(
        app.state(),
        QueryLibraryItemInput {
            id: Some(saved.id.clone()),
            name: "读取新用户".into(),
            command: "GET user:2".into(),
            tags: vec![],
        },
    )
    .expect("query library item must update");
    assert_eq!(updated.id, saved.id);
    assert_eq!(
        query_library::list_query_library(app.state())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        query_library::list_query_library(app.state()).unwrap()[0].command,
        "GET user:2"
    );

    assert_eq!(
        query_library::save_query_library_item(
            app.state(),
            QueryLibraryItemInput {
                id: None,
                name: "认证".into(),
                command: "CONFIG SET requirepass secret".into(),
                tags: vec![],
            },
        )
        .unwrap_err(),
        AppError::InvalidConnection
    );

    query_library::delete_query_library_item(app.state(), saved.id)
        .expect("query library item must delete");
    assert!(query_library::list_query_library(app.state())
        .unwrap()
        .is_empty());

    let settings = AppSettings {
        version: 1,
        theme: "dark".into(),
        result_format: "json".into(),
        scan_count: 250,
        continue_on_error: true,
    };
    assert_eq!(
        settings::save_app_settings(app.state(), settings.clone()).unwrap(),
        settings
    );
    assert_eq!(settings::get_app_settings(app.state()).unwrap(), settings);
}
