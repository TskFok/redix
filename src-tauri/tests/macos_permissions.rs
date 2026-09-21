#![cfg(target_os = "macos")]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use redix_lib::{
    domain::AppSettings,
    persistence::{prepare_sensitive_storage, JsonDocumentStore},
};

fn add_inheritable_read_acl(directory: &Path) {
    let output = Command::new("/bin/chmod")
        .args(["+a", "everyone allow read,file_inherit,directory_inherit"])
        .arg(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to seed ACL: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn acl_entries(path: &Path) -> Vec<String> {
    let output = Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to read ACL: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[test]
fn preparing_sensitive_storage_removes_directory_and_existing_file_acls() {
    let directory = tempfile::tempdir().unwrap();
    let data_dir = directory.path().join("data");
    fs::create_dir(&data_dir).unwrap();
    add_inheritable_read_acl(&data_dir);
    let existing = data_dir.join("workbench-history.json");
    let original = "{ \"version\": 1, \"entries\": [] }\n";
    fs::write(&existing, original).unwrap();
    // macOS can render the everyone principal as either a name or its UUID.
    assert!(!acl_entries(&data_dir).is_empty());
    assert!(acl_entries(&existing)
        .iter()
        .any(|entry| entry.contains("inherited")));

    prepare_sensitive_storage(&data_dir).unwrap();

    assert_eq!(fs::read_to_string(&existing).unwrap(), original);
    assert_eq!(
        fs::metadata(&data_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        acl_entries(&data_dir).is_empty(),
        "the private directory must not retain inheritable read ACLs"
    );
    assert!(
        acl_entries(&existing).is_empty(),
        "existing private files must not retain inherited read ACLs"
    );

    let new_path = data_dir.join("settings.json");
    JsonDocumentStore::new(new_path.clone())
        .save(&AppSettings::default())
        .unwrap();
    assert_eq!(
        fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(acl_entries(&new_path).is_empty());
}

#[test]
fn saved_documents_do_not_retain_inherited_read_acls() {
    let directory = tempfile::tempdir().unwrap();
    add_inheritable_read_acl(directory.path());
    let path = directory.path().join("settings.json");
    let settings = AppSettings::default();
    let store = JsonDocumentStore::new(path.clone());

    store.save(&settings).unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        acl_entries(&path).is_empty(),
        "private files must not retain inherited read ACLs"
    );
    assert_eq!(store.load::<AppSettings>().unwrap(), settings);
    assert!(
        !acl_entries(directory.path()).is_empty(),
        "saving a document must preserve an arbitrary parent directory's ACL"
    );
}
