use std::sync::{Arc, Mutex};

use tauri::Manager;

use redix_lib::{
    commands::{
        analysis_history, array, browser, collection, connections, database, json, observability,
        query_library, search, search_aggregate, settings, stream_entries, vector_set, workbench,
    },
    domain::{
        AppSettings, CliCommandInput, CliSessionInput, ClusterConfig, CommandHistoryEntry,
        CommandResult, ConnectionEndpoint, ConnectionProfile, GetJsonPathInput,
        QueryLibraryItemInput, SaveCommandHistoryInput, SelectDatabaseInput,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{CliManager, RedisOperations, RedisService},
    AppState,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    let _ = array::create_array;
    let _ = array::get_array_summary;
    let _ = array::get_array_range;
    let _ = array::scan_array;
    let _ = array::get_array_elements;
    let _ = array::set_array_element;
    let _ = array::append_array_elements;
    let _ = array::delete_array_elements;
    let _ = array::delete_array_range;
    let _ = array::search_array;
    let _ = array::aggregate_array;
    let _ = vector_set::create_vector_set;
    let _ = vector_set::add_vector_set_elements;
    let _ = vector_set::get_vector_set_summary;
    let _ = vector_set::list_vector_set_elements;
    let _ = vector_set::get_vector_set_element;
    let _ = vector_set::set_vector_set_attributes;
    let _ = vector_set::delete_vector_set_attributes;
    let _ = vector_set::delete_vector_set_elements;
    let _ = vector_set::search_vector_set;
    let _ = vector_set::download_vector_embedding;
    let _ = browser::scan_keys;
    let _ = browser::get_key;
    let _ = browser::get_browser_key;
    let _ = browser::rename_browser_key;
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
    let _ = browser::claim_stream_pending_entries;
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
    let _ = search::list_search_indexes;
    let _ = search::create_search_index;
    let _ = search::get_search_index;
    let _ = search::delete_search_index;
    let _ = search::search_keys;
    let _ = search::get_key_search_indexes;
    let _ = search_aggregate::aggregate_search;
    let _ = collection::get_collection_page;
    let _ = collection::mutate_collection;
    let _ = stream_entries::get_stream_entries;
    let _ = stream_entries::add_stream_entry;
    let _ = stream_entries::delete_stream_entries;
    let _ = analysis_history::list_analysis_history;
    let _ = analysis_history::save_analysis_history;
    let _ = analysis_history::get_analysis_history;
    let _ = analysis_history::delete_analysis_history;
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
    let _ = workbench::delete_command_history;
    let _ = workbench::clear_command_history;
}

#[test]
fn search_command_rejects_empty_connection_id_before_connecting() {
    let app = tauri::test::mock_builder()
        .manage(AppState::new(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
        ))
        .invoke_handler(tauri::generate_handler![search::list_search_indexes])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("test app must build");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("test webview must build");

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "list_search_indexes".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: serde_json::json!({ "connection_id": " " }).into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_owned(),
        },
    )
    .expect_err("empty connection id must fail validation");
    assert_eq!(
        response,
        serde_json::json!({
            "code": "INVALID_CONNECTION",
            "message": "连接配置无效"
        })
    );
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

struct StoredProfiles(Mutex<Vec<ConnectionProfile>>);

impl ProfileRepository for StoredProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        *self.0.lock().unwrap() = profiles.to_vec();
        Ok(())
    }
}

fn cluster_profile(port: u16) -> ConnectionProfile {
    ConnectionProfile {
        ssh: None,
        sentinel: None,
        cluster: Some(ClusterConfig {
            nodes: vec![ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port,
            }],
            read_from_replicas: false,
        }),
        id: "cluster".into(),
        name: "Cluster".into(),
        host: "127.0.0.1".into(),
        port,
        username: None,
        database: 0,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    }
}

async fn spawn_command_cluster() -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut read).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let response = match command.first().map(Vec::as_slice) {
                            Some(b"CLUSTER")
                                if command.get(1).map(Vec::as_slice) == Some(b"SLOTS") =>
                            {
                                format!(
                                    "*1\r\n*3\r\n:0\r\n:16383\r\n*2\r\n$9\r\n127.0.0.1\r\n:{port}\r\n"
                                )
                            }
                            Some(b"PING") => "+PONG\r\n".into(),
                            Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            Some(b"SET") => "+OK\r\n".into(),
                            Some(b"TYPE") => "+string\r\n".into(),
                            Some(b"GET") => "$3\r\nAda\r\n".into(),
                            Some(b"PTTL") => ":-1\r\n".into(),
                            Some(b"MODULE") => {
                                "*1\r\n*4\r\n$4\r\nname\r\n$6\r\nReJSON\r\n$3\r\nver\r\n:20810\r\n"
                                    .into()
                            }
                            Some(b"JSON.GET") => "$7\r\n[\"Ada\"]\r\n".into(),
                            Some(b"EVAL") => "-CROSSSLOT private server detail\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    port
}

async fn spawn_paused_cluster() -> (
    u16,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let (ping_tx, ping_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let ping_tx = Arc::new(Mutex::new(Some(ping_tx)));
    let resume_rx = Arc::new(tokio::sync::Mutex::new(Some(resume_rx)));
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let ping_tx = ping_tx.clone();
            let resume_rx = resume_rx.clone();
            tokio::spawn(async move {
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut read).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let response = match command.first().map(Vec::as_slice) {
                            Some(b"CLUSTER") => format!(
                                "*1\r\n*3\r\n:0\r\n:16383\r\n*2\r\n$9\r\n127.0.0.1\r\n:{port}\r\n"
                            ),
                            Some(b"PING") => {
                                let ping_sender = ping_tx.lock().unwrap().take();
                                if let Some(ping_tx) = ping_sender {
                                    ping_tx.send(()).unwrap();
                                    let resume = resume_rx.lock().await.take();
                                    if let Some(resume_rx) = resume {
                                        let _ = resume_rx.await;
                                    }
                                }
                                "+PONG\r\n".into()
                            }
                            Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    (port, ping_rx, resume_tx)
}

async fn spawn_capability_server(
    json_supported: bool,
    json_value: &'static str,
    module_gate: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    )>,
) -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let gate = Arc::new(tokio::sync::Mutex::new(module_gate));
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                let mut pending = Vec::new();
                let mut read = [0_u8; 4096];
                loop {
                    let size = match socket.read(&mut read).await {
                        Ok(0) | Err(_) => return,
                        Ok(size) => size,
                    };
                    pending.extend_from_slice(&read[..size]);
                    while let Some((consumed, command)) = parse_resp_command(&pending) {
                        pending.drain(..consumed);
                        let response = match command.first().map(Vec::as_slice) {
                            Some(b"PING") => "+PONG\r\n".into(),
                            Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            Some(b"MODULE") => {
                                if let Some((entered, resume)) = gate.lock().await.take() {
                                    entered.send(()).unwrap();
                                    resume.await.unwrap();
                                }
                                if json_supported {
                                    "*1\r\n*4\r\n$4\r\nname\r\n$6\r\nReJSON\r\n$3\r\nver\r\n:20810\r\n"
                                        .into()
                                } else {
                                    "*0\r\n".into()
                                }
                            }
                            Some(b"COMMAND") => "*0\r\n".into(),
                            Some(b"JSON.GET") => {
                                format!("${}\r\n{}\r\n", json_value.len(), json_value)
                            }
                            Some(b"PTTL") => ":-1\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });
    port
}

fn parse_resp_command(buffer: &[u8]) -> Option<(usize, Vec<Vec<u8>>)> {
    if buffer.first() != Some(&b'*') {
        return None;
    }
    let header_end = buffer.windows(2).position(|pair| pair == b"\r\n")?;
    let count = std::str::from_utf8(&buffer[1..header_end])
        .ok()?
        .parse::<usize>()
        .ok()?;
    let mut offset = header_end + 2;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        if buffer.get(offset) != Some(&b'$') {
            return None;
        }
        let length_end = buffer[offset..]
            .windows(2)
            .position(|pair| pair == b"\r\n")?
            + offset;
        let length = std::str::from_utf8(&buffer[offset + 1..length_end])
            .ok()?
            .parse::<usize>()
            .ok()?;
        offset = length_end + 2;
        let end = offset.checked_add(length)?;
        if buffer.len() < end + 2 || &buffer[end..end + 2] != b"\r\n" {
            return None;
        }
        args.push(buffer[offset..end].to_vec());
        offset = end + 2;
    }
    Some((offset, args))
}

#[tokio::test]
async fn existing_browser_workbench_and_cli_paths_use_cluster_routing() {
    let port = spawn_command_cluster().await;
    let service = RedisService::new(
        Arc::new(StoredProfiles(Mutex::new(vec![cluster_profile(port)]))),
        Arc::new(EmptySecrets),
    );
    service.open_connection("cluster").await.unwrap();
    assert_eq!(
        service.connection_target("cluster").await.unwrap().kind(),
        "cluster"
    );

    service
        .execute_command("cluster", "SET {user:1}:name Ada")
        .await
        .unwrap();
    assert_eq!(
        service
            .get_key("cluster", "{user:1}:name")
            .await
            .unwrap()
            .key_type,
        "string"
    );
    assert_eq!(
        service
            .execute_command("cluster", "GET {user:1}:name")
            .await
            .unwrap()
            .value,
        serde_json::json!("Ada")
    );
    assert_eq!(
        service
            .get_json_path(GetJsonPathInput {
                connection_id: "cluster".into(),
                key: "{user:1}:json".into(),
                path: "$.name".into(),
            })
            .await
            .unwrap()
            .value,
        Some(serde_json::json!("Ada"))
    );

    let cli = CliManager::new();
    cli.open(
        &service,
        CliSessionInput {
            connection_id: "cluster".into(),
            session_id: "00000000-0000-4000-8000-000000000005".into(),
        },
    )
    .await
    .unwrap();
    let reply = cli
        .execute(CliCommandInput {
            connection_id: "cluster".into(),
            session_id: "00000000-0000-4000-8000-000000000005".into(),
            command: "PING".into(),
        })
        .await
        .unwrap();
    assert_eq!(reply.result.unwrap().value, serde_json::json!("PONG"));
}

#[tokio::test]
async fn failed_cluster_open_never_publishes_a_standalone_handle() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let service = RedisService::new(
        Arc::new(StoredProfiles(Mutex::new(vec![cluster_profile(port)]))),
        Arc::new(EmptySecrets),
    );

    assert_eq!(
        service.open_connection("cluster").await,
        Err(AppError::ConnectionFailed)
    );
    assert_eq!(
        service.execute_command("cluster", "PING").await,
        Err(AppError::ConnectionFailed)
    );
}

#[tokio::test]
async fn closing_a_cluster_while_inspection_is_pending_prevents_stale_publication() {
    let (port, ping, resume) = spawn_paused_cluster().await;
    let service = Arc::new(RedisService::new(
        Arc::new(StoredProfiles(Mutex::new(vec![cluster_profile(port)]))),
        Arc::new(EmptySecrets),
    ));
    let opening_service = service.clone();
    let opening = tokio::spawn(async move { opening_service.open_connection("cluster").await });
    tokio::time::timeout(std::time::Duration::from_secs(3), ping)
        .await
        .unwrap()
        .unwrap();

    service.close_connection("cluster").await.unwrap();
    resume.send(()).unwrap();

    assert_eq!(opening.await.unwrap(), Err(AppError::OperationCancelled));
    assert_eq!(
        service.execute_command("cluster", "PING").await,
        Err(AppError::ConnectionFailed)
    );
}

#[tokio::test]
async fn cluster_rejects_nonzero_database_pubsub_and_profiler_without_node_fallback() {
    let port = spawn_command_cluster().await;
    let service = RedisService::new(
        Arc::new(StoredProfiles(Mutex::new(vec![cluster_profile(port)]))),
        Arc::new(EmptySecrets),
    );
    service.open_connection("cluster").await.unwrap();
    assert_eq!(
        service
            .select_database(SelectDatabaseInput {
                connection_id: "cluster".into(),
                database: 1,
            })
            .await,
        Err(AppError::UnsupportedFeature)
    );

    assert!(matches!(
        service.standalone_client("cluster").await,
        Err(AppError::UnsupportedFeature)
    ));
}

#[tokio::test]
async fn cluster_crossslot_maps_to_fixed_error_without_server_detail() {
    let port = spawn_command_cluster().await;
    let service = RedisService::new(
        Arc::new(StoredProfiles(Mutex::new(vec![cluster_profile(port)]))),
        Arc::new(EmptySecrets),
    );
    service.open_connection("cluster").await.unwrap();

    let error = service
        .execute_command(
            "cluster",
            "EVAL \"return redis.call('mget', KEYS[1], KEYS[2])\" 2 {one}:key {two}:key",
        )
        .await
        .unwrap_err();
    assert_eq!(error, AppError::CrossSlot);
    assert_eq!(error.message(), "Redis 集群键槽不一致");
    assert!(!error.message().contains("private server detail"));
}

#[tokio::test]
async fn capability_probe_and_gated_command_stay_on_the_same_replaced_handle() {
    let (probe_entered_tx, probe_entered_rx) = tokio::sync::oneshot::channel();
    let (resume_probe_tx, resume_probe_rx) = tokio::sync::oneshot::channel();
    let old_port =
        spawn_capability_server(true, "[\"old\"]", Some((probe_entered_tx, resume_probe_rx))).await;
    let new_port = spawn_capability_server(false, "[\"new\"]", None).await;
    let profiles = Arc::new(StoredProfiles(Mutex::new(vec![ConnectionProfile {
        cluster: None,
        id: "replaced".into(),
        name: "Old".into(),
        host: "127.0.0.1".into(),
        port: old_port,
        ..cluster_profile(old_port)
    }])));
    let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
    service.open_connection("replaced").await.unwrap();

    let probing_service = Arc::clone(&service);
    let gated_command = tokio::spawn(async move {
        probing_service
            .get_json_path(GetJsonPathInput {
                connection_id: "replaced".into(),
                key: "document".into(),
                path: "$".into(),
            })
            .await
    });
    probe_entered_rx.await.unwrap();

    profiles.0.lock().unwrap()[0].port = new_port;
    profiles.0.lock().unwrap()[0].name = "New".into();
    service.open_connection("replaced").await.unwrap();
    resume_probe_tx.send(()).unwrap();

    assert_eq!(
        gated_command.await.unwrap().unwrap().value,
        Some(serde_json::json!("old"))
    );
    assert!(
        !service
            .get_module_capabilities("replaced")
            .await
            .unwrap()
            .json_supported
    );
}
