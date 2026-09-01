use std::{path::PathBuf, sync::Arc};

use tauri::Manager;

pub mod commands;
pub mod domain;
pub mod error;
pub mod persistence;
pub mod redis;

use persistence::{
    migrate_legacy_ssh_paths, JsonProfileRepository, ProfileRepository, SecretStore, SystemKeyring,
};

pub struct AppState {
    pub(crate) profiles: Arc<dyn ProfileRepository>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) redis: redis::RedisService,
    pub(crate) cli: redis::CliManager,
    pub(crate) data_dir: PathBuf,
    pub(crate) analysis_history: persistence::analysis_history::AnalysisHistoryStore,
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
            cli: redis::CliManager::new(),
            analysis_history: persistence::analysis_history::AnalysisHistoryStore::new(
                data_dir.join("analysis-history.json"),
            ),
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
            migrate_legacy_ssh_paths(profiles.as_ref(), secrets.as_ref())?;
            app.manage(AppState::with_data_dir(profiles, secrets, data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::stream_entries::get_stream_entries,
            commands::stream_entries::add_stream_entry,
            commands::stream_entries::delete_stream_entries,
            commands::collection::get_collection_page,
            commands::collection::mutate_collection,
            commands::search_aggregate::aggregate_search,
            commands::browser::get_browser_key,
            commands::browser::rename_browser_key,
            commands::analysis_history::list_analysis_history,
            commands::analysis_history::save_analysis_history,
            commands::analysis_history::get_analysis_history,
            commands::analysis_history::delete_analysis_history,
            commands::cli::open_cli_session,
            commands::cli::execute_cli_command,
            commands::cli::close_cli_session,
            commands::array::create_array,
            commands::array::get_array_summary,
            commands::array::get_array_range,
            commands::array::scan_array,
            commands::array::get_array_elements,
            commands::array::set_array_element,
            commands::array::append_array_elements,
            commands::array::delete_array_elements,
            commands::array::delete_array_range,
            commands::array::search_array,
            commands::array::aggregate_array,
            commands::vector_set::create_vector_set,
            commands::vector_set::add_vector_set_elements,
            commands::vector_set::get_vector_set_summary,
            commands::vector_set::list_vector_set_elements,
            commands::vector_set::get_vector_set_element,
            commands::vector_set::set_vector_set_attributes,
            commands::vector_set::delete_vector_set_attributes,
            commands::vector_set::delete_vector_set_elements,
            commands::vector_set::search_vector_set,
            commands::vector_set::download_vector_embedding,
            commands::connections::list_connections,
            commands::connections::save_connection,
            commands::connections::delete_connection,
            commands::connections::export_connections,
            commands::connections::import_connections,
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
            commands::browser::get_stream_consumer_groups,
            commands::browser::create_stream_consumer_group,
            commands::browser::delete_stream_consumer_group,
            commands::browser::claim_stream_pending_entries,
            commands::browser::get_stream_consumers,
            commands::browser::get_stream_pending_entries,
            commands::browser::acknowledge_stream_pending_entries,
            commands::browser::delete_stream_consumer,
            commands::browser::export_keys,
            commands::browser::import_keys,
            commands::database::get_instance_overview,
            commands::database::get_instance_details,
            commands::database::get_database_overview,
            commands::database::analyze_database,
            commands::database::select_database,
            commands::json::get_module_capabilities,
            commands::json::get_json_path,
            commands::json::set_json_path,
            commands::json::append_json_array,
            commands::json::delete_json_path,
            commands::search::list_search_indexes,
            commands::search::create_search_index,
            commands::search::get_search_index,
            commands::search::delete_search_index,
            commands::search::search_keys,
            commands::search::get_key_search_indexes,
            commands::observability::get_slow_logs,
            commands::observability::clear_slow_logs,
            commands::observability::get_slow_log_config,
            commands::observability::update_slow_log_config,
            commands::observability::start_pub_sub,
            commands::observability::stop_pub_sub,
            commands::observability::publish_pub_sub,
            commands::observability::start_profiler,
            commands::observability::stop_profiler,
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
            commands::workbench::delete_command_history,
            commands::workbench::clear_command_history,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Redix Tauri 应用失败");
}
