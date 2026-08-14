mod support;

use redix_lib::{
    domain::{ConnectionProfile, ScanKeysInput},
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
    let json = serde_json::to_string(&profile).unwrap();

    assert!(!json.contains("password"));
    assert!(json.contains("has_password"));
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
