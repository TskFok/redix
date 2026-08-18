use std::sync::Arc;

use redix_lib::{
    commands::{browser, connections, workbench},
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
    let _ = connections::list_connections;
    let _ = connections::save_connection;
    let _ = connections::delete_connection;
    let _ = connections::test_connection;
    let _ = connections::open_connection;
    let _ = connections::close_connection;
    let _ = workbench::execute_command;
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
