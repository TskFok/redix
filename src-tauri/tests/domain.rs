mod support;

use redix_lib::{
    domain::{
        command_catalog, is_sensitive_command, parse_info_sections, parse_keyspace_line,
        AppSettings, ConnectionProfile, CreateKeyInput, DeleteKeysInput, ExportedKey,
        GetSlowLogsInput, ImportKeysInput, KeyInfoInput, PubSubTopic, QueryLibraryItemInput,
        RedisValue, RenameKeyInput, ScanKeysInput, SelectDatabaseInput, StartProfilerInput,
        StartPubSubInput, StopProfilerInput, StreamEntry, StreamField,
    },
    error::AppError,
};
use support::{invalid_profile, valid_profile};

#[test]
fn command_catalog_contains_safe_high_frequency_commands() {
    let names = command_catalog()
        .into_iter()
        .map(|item| item.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&"PING".into()));
    assert!(names.contains(&"GET".into()));
    assert!(names.contains(&"SET".into()));
}

#[test]
fn sensitive_commands_are_not_saved_to_history() {
    assert!(is_sensitive_command("AUTH secret"));
    assert!(is_sensitive_command("CONFIG SET requirepass secret"));
    assert!(is_sensitive_command("ACL SETUSER alice on >secret"));
    assert!(!is_sensitive_command("GET user:1"));
}

#[test]
fn rejects_empty_host_zero_port_and_database_above_fifteen() {
    let profile = ConnectionProfile {
        id: "local".into(),
        name: "Local".into(),
        host: "".into(),
        port: 0,
        username: None,
        database: 16,
        has_password: false,
    };

    let error = profile.validate().expect_err("invalid profile must fail");
    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn rejects_an_invalid_profile_from_the_shared_fixture() {
    let error = invalid_profile()
        .validate()
        .expect_err("invalid profile must fail");

    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn accepts_a_valid_connection_profile() {
    assert_eq!(valid_profile().validate(), Ok(()));
}

#[test]
fn serializes_profile_without_password_field() {
    let profile = valid_profile();
    let json = serde_json::to_value(&profile).unwrap();
    let object = json
        .as_object()
        .expect("profile must serialize as an object");

    assert!(!object.contains_key("password"));
    assert!(object.contains_key("has_password"));
}

#[test]
fn rejects_scan_counts_outside_one_through_five_hundred() {
    for count in [0, 501] {
        let input = ScanKeysInput {
            connection_id: "local".into(),
            cursor: 0,
            pattern: "*".into(),
            count,
            key_type: None,
        };

        let error = input.validate().expect_err("invalid scan count must fail");
        assert_eq!(error.code(), "INVALID_CONNECTION");
    }
}

#[test]
fn scan_filter_rejects_unknown_key_type_and_accepts_supported_type() {
    let mut input = ScanKeysInput {
        connection_id: "local".into(),
        cursor: 0,
        pattern: "*".into(),
        count: 100,
        key_type: Some("hash".into()),
    };
    assert_eq!(input.validate(), Ok(()));

    input.key_type = Some("vector".into());
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidConnection);
}

#[test]
fn import_rejects_empty_entries_and_exported_key_rejects_empty_name() {
    assert_eq!(
        ImportKeysInput {
            connection_id: "local".into(),
            entries: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );

    let empty_name = ExportedKey {
        key: String::new(),
        ttl_ms: -1,
        value: RedisValue::String {
            value: "value".into(),
        },
    };
    assert_eq!(
        empty_name.validate().unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn serializes_errors_with_stable_code_and_safe_message() {
    let json = serde_json::to_value(AppError::AuthenticationFailed).unwrap();

    assert_eq!(json["code"], "AUTHENTICATION_FAILED");
    assert_eq!(json["message"], "Redis 身份验证失败");
    assert!(!json.to_string().contains("password"));
}

#[test]
fn validates_browser_extension_inputs_without_accepting_empty_keys() {
    let value = RedisValue::Stream {
        entries: vec![StreamEntry {
            id: "1-0".into(),
            fields: vec![StreamField {
                field: "event".into(),
                value: "created".into(),
            }],
        }],
    };
    let valid = CreateKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        value,
        ttl_ms: Some(5_000),
    };
    assert_eq!(valid.validate(), Ok(()));

    let invalid_create = CreateKeyInput {
        key: " ".into(),
        ttl_ms: Some(-1),
        ..valid.clone()
    };
    assert_eq!(
        invalid_create.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_rename = RenameKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        new_key: " ".into(),
    };
    assert_eq!(
        invalid_rename.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_delete = DeleteKeysInput {
        connection_id: "local".into(),
        keys: vec!["events".into(), "".into()],
    };
    assert_eq!(
        invalid_delete.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_info = KeyInfoInput {
        connection_id: " ".into(),
        key: "events".into(),
    };
    assert_eq!(
        invalid_info.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );
}

#[test]
fn rejects_empty_streams_and_stream_entries_without_leaking_values() {
    let empty_stream = CreateKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        value: RedisValue::Stream { entries: vec![] },
        ttl_ms: None,
    };
    let error = empty_stream
        .validate()
        .expect_err("empty stream must not create a key");

    assert_eq!(error.code(), "COMMAND_FAILED");
    assert_eq!(error.to_string(), "Redis 命令执行失败");
}

#[test]
fn accepts_json_root_documents_as_browser_values() {
    let input = CreateKeyInput {
        connection_id: "local".into(),
        key: "profile:1".into(),
        value: RedisValue::Json {
            value: serde_json::json!({"name": "Alice", "active": true}),
        },
        ttl_ms: None,
    };

    assert_eq!(input.validate(), Ok(()));
}

#[test]
fn parses_info_sections_and_optional_metrics() {
    let sections = parse_info_sections(
        "# Server\nredis_version:7.2.5\nuptime_in_seconds:42\n\n# Clients\nconnected_clients:3\n# Keyspace\ndb0:keys=8,expires=2,avg_ttl=1200\n",
    );
    assert_eq!(sections["Server"]["redis_version"], "7.2.5");
    assert_eq!(sections["Clients"]["connected_clients"], "3");
    let db = parse_keyspace_line("db0", "keys=8,expires=2,avg_ttl=1200").unwrap();
    assert_eq!(db.database, 0);
    assert_eq!(db.key_count, Some(8));
    assert_eq!(db.expires, Some(2));
    assert_eq!(db.avg_ttl_ms, Some(1200));
}

#[test]
fn rejects_invalid_database_selection_and_keyspace_lines() {
    assert_eq!(
        SelectDatabaseInput {
            connection_id: "local".into(),
            database: 16,
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
    assert_eq!(
        parse_keyspace_line("dbx", "keys=1,expires=0").unwrap_err(),
        AppError::PersistenceFailed
    );
}

#[test]
fn query_library_rejects_sensitive_commands_and_accepts_normal_commands() {
    assert!(QueryLibraryItemInput {
        id: None,
        name: "读取用户".into(),
        command: "GET user:1".into(),
        tags: vec!["用户".into()],
    }
    .validate()
    .is_ok());
    assert_eq!(
        QueryLibraryItemInput {
            id: None,
            name: "认证".into(),
            command: "AUTH secret".into(),
            tags: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn settings_validate_fixed_enum_and_range() {
    assert!(AppSettings {
        version: 1,
        theme: "dark".into(),
        result_format: "json".into(),
        scan_count: 200,
        continue_on_error: true,
    }
    .validate()
    .is_ok());
    assert_eq!(
        AppSettings {
            scan_count: 1,
            ..AppSettings::default()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn rejects_invalid_observability_inputs_without_leaking_values() {
    let error = GetSlowLogsInput {
        connection_id: "".into(),
        count: 1001,
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
    assert_eq!(error.to_string(), "输入参数无效");

    let error = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![PubSubTopic {
            name: "   ".into(),
            pattern: false,
        }],
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
}

#[test]
fn normalizes_pubsub_topics_and_rejects_duplicates() {
    let input = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![
            PubSubTopic {
                name: " news.* ".into(),
                pattern: true,
            },
            PubSubTopic {
                name: "news.*".into(),
                pattern: true,
            },
        ],
    };
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
}

#[test]
fn validates_profiler_session_inputs() {
    assert_eq!(
        StartProfilerInput {
            connection_id: "".into(),
            session_id: "session".into(),
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        StopProfilerInput {
            connection_id: "local".into(),
            session_id: "".into(),
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
}
