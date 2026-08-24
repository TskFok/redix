use std::{path::PathBuf, sync::Arc};

use tauri::Manager;

pub mod commands;
pub mod domain;
pub mod error;
pub mod persistence;
pub mod redis;

use persistence::{JsonProfileRepository, ProfileRepository, SecretStore, SystemKeyring};

pub struct AppState {
    pub(crate) profiles: Arc<dyn ProfileRepository>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) redis: redis::RedisService,
    pub(crate) data_dir: PathBuf,
}

impl AppState {
    pub fn new(profiles: Arc<dyn ProfileRepository>, secrets: Arc<dyn SecretStore>) -> Self {
        Self::with_data_dir(profiles, secrets, PathBuf::from("."))
    }

    pub fn with_data_dir(
        profiles: Arc<dyn ProfileRepository>,
        secrets: Arc<dyn SecretStore>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            redis: redis::RedisService::new(profiles.clone(), secrets.clone()),
            profiles,
            secrets,
            data_dir,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("无法确定 Redix 应用数据目录");
            let profiles: Arc<dyn ProfileRepository> = Arc::new(JsonProfileRepository::new(
                data_dir.join("connections.json"),
            ));
            let secrets: Arc<dyn SecretStore> = Arc::new(SystemKeyring::new());
            app.manage(AppState::with_data_dir(profiles, secrets, data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connections::list_connections,
            commands::connections::save_connection,
            commands::connections::delete_connection,
            commands::connections::test_connection,
            commands::connections::open_connection,
            commands::connections::close_connection,
            commands::browser::scan_keys,
            commands::browser::get_key,
            commands::browser::set_key,
            commands::browser::create_key,
            commands::browser::rename_key,
            commands::browser::delete_key,
            commands::browser::delete_keys,
            commands::browser::set_key_ttl,
            commands::browser::get_key_info,
            commands::browser::export_keys,
            commands::browser::import_keys,
            commands::database::get_instance_overview,
            commands::database::get_database_overview,
            commands::database::select_database,
            commands::observability::get_slow_logs,
            commands::observability::clear_slow_logs,
            commands::observability::get_slow_log_config,
            commands::observability::update_slow_log_config,
            commands::observability::start_pub_sub,
            commands::observability::stop_pub_sub,
            commands::observability::publish_pub_sub,
            commands::query_library::list_query_library,
            commands::query_library::save_query_library_item,
            commands::query_library::delete_query_library_item,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::workbench::execute_command,
            commands::workbench::execute_commands,
            commands::workbench::get_command_catalog,
            commands::workbench::list_command_history,
            commands::workbench::save_command_history,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Redix Tauri 应用失败");
}
