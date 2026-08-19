use std::sync::Arc;

use tauri::Manager;

use redix_lib::{
    commands::{browser, connections, workbench},
    domain::{CommandHistoryEntry, CommandResult, SaveCommandHistoryInput},
    error::AppError,
    persistence::{ProfileRepository, SecretStore},
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
    fn read(&self, _connection_id: &str) -> Result<Option<String>, AppError> {
        Ok(None)
    }

    fn write(&self, _connection_id: &str, _password: &str) -> Result<(), AppError> {
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
    let _ = browser::export_keys;
    let _ = browser::import_keys;
    let _ = connections::list_connections;
    let _ = connections::save_connection;
    let _ = connections::delete_connection;
    let _ = connections::test_connection;
    let _ = connections::open_connection;
    let _ = connections::close_connection;
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
