mod support;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    error::AppError,
    persistence::{JsonProfileRepository, ProfileRepository, SecretStore, SystemKeyring},
};
use support::valid_profile;

#[derive(Default)]
struct InMemorySecretStore {
    values: Mutex<HashMap<String, String>>,
}

impl SecretStore for InMemorySecretStore {
    fn read(&self, connection_id: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .values
            .lock()
            .expect("test secret store lock must not be poisoned")
            .get(connection_id)
            .cloned())
    }

    fn write(&self, connection_id: &str, password: &str) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret store lock must not be poisoned")
            .insert(connection_id.to_owned(), password.to_owned());
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
fn in_memory_secret_store_supports_write_read_and_delete() {
    let secrets = InMemorySecretStore::default();

    secrets.write("local", "secret").unwrap();
    assert_eq!(secrets.read("local").unwrap().as_deref(), Some("secret"));
    secrets.delete("local").unwrap();
    assert_eq!(secrets.read("local").unwrap(), None);
}

#[test]
fn system_keyring_uses_the_standalone_service_and_account_namespace() {
    let keyring = SystemKeyring::new();

    assert_eq!(keyring.service, "redix");
    assert_eq!(SystemKeyring::account_for("local"), "redix/local");
}
