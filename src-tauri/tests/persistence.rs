mod support;

use std::{
    collections::HashMap,
    fs,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    domain::{
        AppSettings, CommandHistoryDocument, CommandHistoryEntry, CommandResult,
        QueryLibraryDocument,
    },
    error::AppError,
    persistence::{
        decode_stored_secret, migrate_legacy_ssh_paths, ConnectionSecrets, JsonDocumentStore,
        JsonProfileRepository, ProfileRepository, SecretStore, SystemKeyring,
        VersionedJsonDocument,
    },
};
use support::valid_profile;

static CURRENT_DIRECTORY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct InMemorySecretStore {
    values: Mutex<HashMap<String, ConnectionSecrets>>,
}

struct InMemoryProfileRepository {
    values: Mutex<Vec<redix_lib::domain::ConnectionProfile>>,
}

impl InMemoryProfileRepository {
    fn new(values: Vec<redix_lib::domain::ConnectionProfile>) -> Self {
        Self {
            values: Mutex::new(values),
        }
    }
}

impl ProfileRepository for InMemoryProfileRepository {
    fn load(&self) -> Result<Vec<redix_lib::domain::ConnectionProfile>, AppError> {
        Ok(self.values.lock().unwrap().clone())
    }

    fn save(&self, profiles: &[redix_lib::domain::ConnectionProfile]) -> Result<(), AppError> {
        *self.values.lock().unwrap() = profiles.to_vec();
        Ok(())
    }
}

impl SecretStore for InMemorySecretStore {
    fn read(&self, connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(self
            .values
            .lock()
            .expect("test secret store lock must not be poisoned")
            .get(connection_id)
            .cloned())
    }

    fn write(&self, connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret store lock must not be poisoned")
            .insert(connection_id.to_owned(), secrets.clone());
        Ok(())
    }

    fn delete(&self, connection_id: &str) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret store lock must not be poisoned")
            .remove(connection_id);
        Ok(())
    }
}

fn temporary_directory(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("redix-{test_name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("test directory must be created");
    path
}

fn remove_temporary_directory(path: &Path) {
    fs::remove_dir_all(path).expect("test directory must be removed");
}

fn in_current_directory<T>(directory: &Path, operation: impl FnOnce() -> T) -> T {
    let _lock = CURRENT_DIRECTORY_LOCK
        .lock()
        .expect("current directory lock must not be poisoned");
    let previous = std::env::current_dir().expect("test current directory must be readable");
    std::env::set_current_dir(directory).expect("test current directory must be changed");

    let result = catch_unwind(AssertUnwindSafe(operation));
    std::env::set_current_dir(previous).expect("test current directory must be restored");

    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
struct TestDocument {
    version: u32,
    value: String,
}

impl VersionedJsonDocument for TestDocument {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        serde_json::from_value(value).map_err(|_| AppError::PersistenceFailed)
    }
}

#[test]
fn versioned_document_round_trips_and_replaces_atomically() {
    let directory = temporary_directory("document-round-trip");
    let path = directory.join("settings.json");
    let store = JsonDocumentStore::new(path.clone());
    let document = TestDocument {
        version: 1,
        value: "ready".into(),
    };

    store.save(&document).unwrap();
    assert_eq!(store.load::<TestDocument>().unwrap(), document);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    remove_temporary_directory(&directory);
}

#[test]
fn malformed_versioned_document_returns_safe_persistence_error() {
    let directory = temporary_directory("document-malformed");
    let path = directory.join("settings.json");
    fs::write(&path, "{ invalid json").unwrap();

    let error = JsonDocumentStore::new(path)
        .load::<TestDocument>()
        .unwrap_err();
    assert_eq!(error, AppError::PersistenceFailed);
    remove_temporary_directory(&directory);
}

#[test]
fn malformed_versioned_document_can_restore_default_without_overwriting_source() {
    let directory = temporary_directory("document-default");
    let path = directory.join("settings.json");
    let original = "{ invalid json";
    fs::write(&path, original).unwrap();

    let loaded = JsonDocumentStore::new(path.clone())
        .load_or_default::<TestDocument>()
        .unwrap();
    assert_eq!(loaded, TestDocument::default());
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    remove_temporary_directory(&directory);
}

#[test]
fn command_history_document_round_trips_through_versioned_store() {
    let directory = temporary_directory("workbench-history");
    let path = directory.join("workbench-history.json");
    let store = JsonDocumentStore::new(path);
    let document = CommandHistoryDocument {
        version: 1,
        entries: vec![CommandHistoryEntry {
            connection_id: "local".into(),
            command: "PING".into(),
            result: Some(CommandResult {
                kind: "string".into(),
                value: serde_json::json!("PONG"),
            }),
            error_code: None,
            created_at: "2026-08-19T00:00:00Z".into(),
        }],
    };

    store.save(&document).unwrap();
    assert_eq!(store.load::<CommandHistoryDocument>().unwrap(), document);
    remove_temporary_directory(&directory);
}

#[test]
fn local_resource_documents_migrate_legacy_version_zero_values() {
    let query = QueryLibraryDocument::migrate(serde_json::json!({
        "items": [{"name": "读取用户", "command": "GET user:1"}]
    }))
    .unwrap();
    assert_eq!(query.version, 1);
    assert_eq!(query.items[0].id, "legacy-0");
    assert!(query.items[0].tags.is_empty());

    let settings = AppSettings::migrate(serde_json::json!({
        "theme": "dark",
        "scan_count": 200
    }))
    .unwrap();
    assert_eq!(settings.version, 1);
    assert_eq!(settings.theme, "dark");
    assert_eq!(settings.result_format, "raw");
    assert_eq!(settings.scan_count, 200);
    assert!(!settings.continue_on_error);
}

#[test]
fn corrupted_local_resource_returns_default_without_overwriting_source() {
    let directory = temporary_directory("local-resource-default");
    let path = directory.join("query-library.json");
    let original = "{ invalid local resource";
    fs::write(&path, original).unwrap();

    let loaded = JsonDocumentStore::new(path.clone())
        .load_or_default::<QueryLibraryDocument>()
        .unwrap();
    assert_eq!(loaded, QueryLibraryDocument::default());
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    remove_temporary_directory(&directory);
}

#[test]
fn repository_round_trips_profiles_under_profiles_key_without_password_key() {
    let directory = temporary_directory("round-trip");
    let path = directory.join("nested/connections.json");
    let repository = JsonProfileRepository::new(path.clone());
    let mut profile = valid_profile();
    profile.has_password = true;

    repository.save(&[profile.clone()]).unwrap();
    assert_eq!(repository.load().unwrap(), vec![profile]);

    let raw = fs::read_to_string(path).unwrap();
    let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let profiles = document["profiles"]
        .as_array()
        .expect("profiles must be an array");
    let stored = profiles[0]
        .as_object()
        .expect("stored profile must be a JSON object");

    assert!(!stored.contains_key("password"));
    assert!(stored.contains_key("has_password"));
    assert!(!raw.contains("correct-horse-battery-staple"));

    remove_temporary_directory(&directory);
}

#[test]
fn repository_saves_a_relative_filename_without_creating_an_empty_parent() {
    let directory = tempfile::tempdir().unwrap();
    let repository = JsonProfileRepository::new(PathBuf::from("connections.json"));

    in_current_directory(directory.path(), || {
        repository.save(&[valid_profile()]).unwrap();
        assert_eq!(repository.load().unwrap(), vec![valid_profile()]);
    });
}

#[test]
fn repository_replaces_an_existing_profile_file_with_updated_content() {
    let directory = temporary_directory("replace-existing");
    let path = directory.join("connections.json");
    let repository = JsonProfileRepository::new(path.clone());
    let first = valid_profile();
    let mut updated = first.clone();
    updated.name = "Updated local".into();

    repository.save(&[first]).unwrap();
    let second_save = repository.save(&[updated.clone()]);

    assert!(
        second_save.is_ok(),
        "replacing existing profile file failed: {second_save:?}"
    );
    assert_eq!(repository.load().unwrap(), vec![updated]);
    remove_temporary_directory(&directory);
}

#[test]
fn repository_treats_an_empty_file_as_an_empty_profile_list() {
    let directory = temporary_directory("empty-file");
    let path = directory.join("connections.json");
    fs::write(&path, " \n\t ").unwrap();

    let loaded = JsonProfileRepository::new(path).load().unwrap();

    assert!(loaded.is_empty());
    remove_temporary_directory(&directory);
}

#[test]
fn repository_maps_malformed_json_to_a_safe_persistence_error() {
    let directory = temporary_directory("malformed-json");
    let path = directory.join("connections.json");
    fs::write(&path, "{ invalid json").unwrap();

    let error = JsonProfileRepository::new(path).load().unwrap_err();

    assert_eq!(error, AppError::PersistenceFailed);
    assert_eq!(error.message(), "本地数据保存失败");
    remove_temporary_directory(&directory);
}

#[test]
fn repository_leaves_no_temporary_file_after_an_atomic_save() {
    let directory = temporary_directory("atomic-save");
    let path = directory.join("connections.json");
    let repository = JsonProfileRepository::new(path.clone());

    repository.save(&[valid_profile()]).unwrap();

    assert!(path.is_file());
    assert_eq!(
        fs::read_dir(&directory).unwrap().count(),
        1,
        "a completed atomic save must clean up its sibling temporary file"
    );
    remove_temporary_directory(&directory);
}

#[test]
fn repository_preserves_an_existing_directory_and_marker_when_save_fails() {
    let directory = temporary_directory("failed-save");
    let target = directory.join("connections.json");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("marker"), "keep me").unwrap();
    let repository = JsonProfileRepository::new(target.clone());

    let error = repository.save(&[valid_profile()]).unwrap_err();

    assert_eq!(error, AppError::PersistenceFailed);
    assert_eq!(
        fs::read_to_string(target.join("marker")).unwrap(),
        "keep me"
    );
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    remove_temporary_directory(&directory);
}

#[test]
fn in_memory_secret_store_supports_write_read_and_delete() {
    let secrets = InMemorySecretStore::default();

    secrets
        .write(
            "local",
            &ConnectionSecrets {
                password: Some("secret".into()),
                ..ConnectionSecrets::default()
            },
        )
        .unwrap();
    assert_eq!(
        secrets
            .read("local")
            .unwrap()
            .and_then(|value| value.password),
        Some("secret".into())
    );
    secrets.delete("local").unwrap();
    assert_eq!(secrets.read("local").unwrap(), None);
}

#[test]
fn ssh_secrets_round_trip_without_entering_profile_json() {
    let secrets = ConnectionSecrets {
        ssh_password: Some("ssh-secret".into()),
        ssh_private_key: Some("PRIVATE".into()),
        ssh_passphrase: Some("passphrase".into()),
        ssh_identity_file: Some("/private/key".into()),
        ssh_known_hosts_file: Some("/private/known_hosts".into()),
        ..Default::default()
    };
    let encoded = serde_json::to_string(&secrets).unwrap();
    assert_eq!(decode_stored_secret(&encoded).unwrap(), secrets);

    let mut profile = valid_profile();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example",
            "port": 22,
            "username": "operator",
            "has_password": true,
            "has_private_key": true,
            "has_passphrase": true,
            "has_identity_file": true,
            "has_known_hosts_file": true
        }))
        .unwrap(),
    );
    let encoded_profile = serde_json::to_string(&profile).unwrap();
    assert!(!encoded_profile.contains("ssh-secret"));
    assert!(!encoded_profile.contains("PRIVATE"));
    assert!(!encoded_profile.contains("/private/"));
}

#[test]
fn legacy_ssh_paths_copy_to_secrets_before_profiles_are_rewritten() {
    let mut profile = valid_profile();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example",
            "port": 22,
            "username": "operator",
            "identity_file": "/private/key",
            "known_hosts_file": "/private/known_hosts"
        }))
        .unwrap(),
    );
    let repository = InMemoryProfileRepository::new(vec![profile]);
    let secrets = InMemorySecretStore::default();

    migrate_legacy_ssh_paths(&repository, &secrets).unwrap();

    let migrated = repository.load().unwrap();
    let ssh = migrated[0].ssh.as_ref().unwrap();
    assert!(ssh.has_identity_file);
    assert!(ssh.has_known_hosts_file);
    assert_eq!(
        ssh.auth_method,
        redix_lib::domain::SshAuthMethod::PrivateKey
    );
    assert!(!serde_json::to_string(ssh).unwrap().contains("/private/"));
    let stored = secrets.read("local").unwrap().unwrap();
    assert_eq!(stored.ssh_identity_file.as_deref(), Some("/private/key"));
    assert_eq!(
        stored.ssh_known_hosts_file.as_deref(),
        Some("/private/known_hosts")
    );

    migrate_legacy_ssh_paths(&repository, &secrets).unwrap();
    assert_eq!(repository.load().unwrap(), migrated);
}

#[test]
fn legacy_ssh_migration_does_not_rewrite_profiles_when_secret_copy_fails() {
    struct FailingSecrets;
    impl SecretStore for FailingSecrets {
        fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &ConnectionSecrets) -> Result<(), AppError> {
            Err(AppError::PersistenceFailed)
        }
        fn delete(&self, _: &str) -> Result<(), AppError> {
            Ok(())
        }
    }
    let mut profile = valid_profile();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator",
            "identity_file": "/private/key"
        }))
        .unwrap(),
    );
    let repository = InMemoryProfileRepository::new(vec![profile.clone()]);

    assert_eq!(
        migrate_legacy_ssh_paths(&repository, &FailingSecrets),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(repository.load().unwrap(), vec![profile]);
}

#[test]
fn already_copied_legacy_identity_auth_is_made_explicit_without_reading_profile_paths() {
    let mut profile = valid_profile();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator",
            "auth_method": "agent", "has_identity_file": true
        }))
        .unwrap(),
    );
    let repository = InMemoryProfileRepository::new(vec![profile]);
    let secrets = InMemorySecretStore::default();
    secrets
        .write(
            "local",
            &ConnectionSecrets {
                ssh_identity_file: Some("/private/key".into()),
                ..Default::default()
            },
        )
        .unwrap();
    migrate_legacy_ssh_paths(&repository, &secrets).unwrap();
    let migrated = repository.load().unwrap();
    assert_eq!(
        migrated[0].ssh.as_ref().unwrap().auth_method,
        redix_lib::domain::SshAuthMethod::PrivateKey
    );
    assert_eq!(
        secrets
            .read("local")
            .unwrap()
            .unwrap()
            .ssh_identity_file
            .as_deref(),
        Some("/private/key")
    );
}

#[test]
fn legacy_ssh_migration_keeps_legacy_profile_when_profile_rewrite_fails_after_copy() {
    struct FailingProfiles(Mutex<Vec<redix_lib::domain::ConnectionProfile>>);
    impl ProfileRepository for FailingProfiles {
        fn load(&self) -> Result<Vec<redix_lib::domain::ConnectionProfile>, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, _: &[redix_lib::domain::ConnectionProfile]) -> Result<(), AppError> {
            Err(AppError::PersistenceFailed)
        }
    }
    let mut profile = valid_profile();
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator",
            "known_hosts_file": "/private/known_hosts"
        }))
        .unwrap(),
    );
    let repository = FailingProfiles(Mutex::new(vec![profile.clone()]));
    let secrets = InMemorySecretStore::default();

    assert_eq!(
        migrate_legacy_ssh_paths(&repository, &secrets),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(repository.load().unwrap(), vec![profile]);
    assert_eq!(
        secrets
            .read("local")
            .unwrap()
            .unwrap()
            .ssh_known_hosts_file
            .as_deref(),
        Some("/private/known_hosts")
    );
}

#[test]
fn system_keyring_uses_the_standalone_service_and_account_namespace() {
    let keyring = SystemKeyring::new();

    assert_eq!(keyring.service, "redix");
    assert_eq!(SystemKeyring::account_for("local"), "redix/local");
}
