mod support;

use redix_lib::{
    domain::{
        ConnectionProfile, CreateKeyInput, DeleteKeysInput, KeyInfoInput, RedisValue,
        RenameKeyInput, ScanKeysInput, StreamEntry, StreamField,
    },
    error::AppError,
};
use support::{invalid_profile, valid_profile};

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
        };

        let error = input.validate().expect_err("invalid scan count must fail");
        assert_eq!(error.code(), "INVALID_CONNECTION");
    }
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
