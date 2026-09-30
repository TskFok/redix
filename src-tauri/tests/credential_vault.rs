use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
    time::Duration,
};

use redix_lib::{
    error::AppError,
    persistence::{ConnectionSecrets, CredentialVault, SecretStore, VaultKeyStore},
};

// Only the operating-system credential boundary is replaced. Encryption, file I/O,
// locking, migration, and cache behavior all run through the production vault.
#[derive(Default)]
struct MemoryKeys {
    key: Mutex<Option<Vec<u8>>>,
    reads: AtomicUsize,
    writes: AtomicUsize,
    reject_next_read: AtomicBool,
    reject_writes: AtomicBool,
    slow_reads: AtomicBool,
}

impl MemoryKeys {
    fn existing() -> Arc<Self> {
        Arc::new(Self {
            key: Mutex::new(Some(vec![0x42; 32])),
            ..Self::default()
        })
    }
}

impl VaultKeyStore for MemoryKeys {
    fn read_key(&self) -> Result<Option<Vec<u8>>, AppError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.slow_reads.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(25));
        }
        if self.reject_next_read.swap(false, Ordering::SeqCst) {
            return Err(AppError::PersistenceFailed);
        }
        Ok(self.key.lock().unwrap().clone())
    }

    fn write_key(&self, key: &[u8]) -> Result<(), AppError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.reject_writes.load(Ordering::SeqCst) {
            return Err(AppError::PersistenceFailed);
        }
        *self.key.lock().unwrap() = Some(key.to_vec());
        Ok(())
    }
}

#[derive(Default)]
struct MemoryLegacy {
    values: Mutex<HashMap<String, ConnectionSecrets>>,
    reads: AtomicUsize,
    reject_reads: AtomicBool,
    reject_deletes: AtomicBool,
}

impl SecretStore for MemoryLegacy {
    fn read(&self, id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.reject_reads.load(Ordering::SeqCst) {
            return Err(AppError::PersistenceFailed);
        }
        Ok(self.values.lock().unwrap().get(id).cloned())
    }

    fn write(&self, id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
        self.values
            .lock()
            .unwrap()
            .insert(id.to_owned(), secrets.clone());
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        if self.reject_deletes.load(Ordering::SeqCst) {
            return Err(AppError::PersistenceFailed);
        }
        self.values.lock().unwrap().remove(id);
        Ok(())
    }
}

fn password(value: &str) -> ConnectionSecrets {
    ConnectionSecrets {
        password: Some(value.to_owned()),
        ..ConnectionSecrets::default()
    }
}

fn open(path: &Path, keys: &Arc<MemoryKeys>, legacy: &Arc<MemoryLegacy>) -> CredentialVault {
    CredentialVault::new(path.to_owned(), keys.clone(), legacy.clone())
}

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    Arc<MemoryKeys>,
    Arc<MemoryLegacy>,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials.vault");
    (directory, path, MemoryKeys::existing(), Arc::default())
}

#[test]
fn multiple_connections_and_reconnects_unlock_the_master_key_once_per_instance() {
    let (_directory, path, keys, legacy) = fixture();
    {
        let seed = open(&path, &keys, &legacy);
        seed.write("first", &password("first-password")).unwrap();
        seed.write("second", &password("second-password")).unwrap();
    }
    keys.reads.store(0, Ordering::SeqCst);
    let vault = open(&path, &keys, &legacy);

    assert_eq!(
        vault.read("first").unwrap(),
        Some(password("first-password"))
    );
    assert_eq!(
        vault.read("second").unwrap(),
        Some(password("second-password"))
    );
    assert_eq!(
        vault.read("first").unwrap(),
        Some(password("first-password"))
    );
    vault.write("third", &password("third-password")).unwrap();
    vault.write("first", &password("rotated-password")).unwrap();
    assert_eq!(
        vault.read("first").unwrap(),
        Some(password("rotated-password"))
    );
    assert_eq!(keys.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn large_tls_ssh_and_sentinel_secrets_survive_restart_without_plaintext_on_disk() {
    let (_directory, path, keys, legacy) = fixture();
    let secrets = ConnectionSecrets {
        password: Some("redis-private-password-phrase".into()),
        sentinel_password: Some("sentinel-private-password-phrase".into()),
        ca_certificate: Some(format!("CA-CERTIFICATE-MARKER{}", "ca-body".repeat(2_000))),
        client_certificate: Some(format!(
            "CLIENT-CERTIFICATE-MARKER{}",
            "cert-body".repeat(2_000)
        )),
        client_key: Some(format!(
            "CLIENT-PRIVATE-KEY-MARKER{}",
            "key-body".repeat(2_000)
        )),
        ssh_password: Some("ssh-private-password-phrase".into()),
        ssh_private_key: Some(format!(
            "SSH-PRIVATE-KEY-MARKER{}",
            "ssh-body".repeat(2_000)
        )),
        ssh_passphrase: Some("ssh-private-passphrase-phrase".into()),
        ssh_identity_file: Some("/private/customer/identity-file-marker".into()),
        ssh_known_hosts_file: Some("/private/customer/known-hosts-marker".into()),
    };
    {
        let vault = open(&path, &keys, &legacy);
        vault.write("production", &secrets).unwrap();
    }

    let persisted = fs::read(&path).unwrap();
    for marker in [
        "redis-private-password-phrase",
        "sentinel-private-password-phrase",
        "CA-CERTIFICATE-MARKER",
        "CLIENT-CERTIFICATE-MARKER",
        "CLIENT-PRIVATE-KEY-MARKER",
        "ssh-private-password-phrase",
        "SSH-PRIVATE-KEY-MARKER",
        "ssh-private-passphrase-phrase",
        "/private/customer/identity-file-marker",
        "/private/customer/known-hosts-marker",
    ] {
        assert!(!persisted
            .windows(marker.len())
            .any(|bytes| bytes == marker.as_bytes()));
    }
    let restarted = open(&path, &keys, &legacy);
    assert_eq!(restarted.read("production").unwrap(), Some(secrets));
}

#[test]
fn independent_instances_refresh_the_file_before_writing_other_connections() {
    let (_directory, path, keys, legacy) = fixture();
    let first = open(&path, &keys, &legacy);
    let second = open(&path, &keys, &legacy);
    first.write("first", &password("one")).unwrap();
    assert_eq!(second.read("first").unwrap(), Some(password("one")));
    first.write("second", &password("two")).unwrap();
    second.write("third", &password("three")).unwrap();
    first.write("first", &password("rotated")).unwrap();

    let restarted = open(&path, &keys, &legacy);
    assert_eq!(restarted.read("first").unwrap(), Some(password("rotated")));
    assert_eq!(restarted.read("second").unwrap(), Some(password("two")));
    assert_eq!(restarted.read("third").unwrap(), Some(password("three")));
    assert_eq!(second.read("first").unwrap(), Some(password("rotated")));
}

#[test]
fn independent_instances_serialize_concurrent_writes_without_losing_connections() {
    let (_directory, path, keys, legacy) = fixture();
    let start = Arc::new(Barrier::new(9));
    let handles: Vec<_> = (0..8)
        .map(|number| {
            let vault = open(&path, &keys, &legacy);
            let start = start.clone();
            thread::spawn(move || {
                start.wait();
                vault.write(
                    &format!("connection-{number}"),
                    &password(&format!("password-{number}")),
                )
            })
        })
        .collect();
    start.wait();
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    let restarted = open(&path, &keys, &legacy);
    for number in 0..8 {
        assert_eq!(
            restarted.read(&format!("connection-{number}")).unwrap(),
            Some(password(&format!("password-{number}")))
        );
    }
}

#[test]
fn concurrent_new_instances_create_only_one_master_key_for_the_shared_vault() {
    let (_directory, path, _, legacy) = fixture();
    let keys = Arc::new(MemoryKeys::default());
    keys.slow_reads.store(true, Ordering::SeqCst);
    let start = Arc::new(Barrier::new(5));
    let handles: Vec<_> = (0..4)
        .map(|number| {
            let vault = open(&path, &keys, &legacy);
            let start = start.clone();
            thread::spawn(move || {
                start.wait();
                vault.write(
                    &format!("connection-{number}"),
                    &password(&format!("password-{number}")),
                )
            })
        })
        .collect();
    start.wait();
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    assert_eq!(keys.key.lock().unwrap().as_ref().unwrap().len(), 32);
    let restarted = open(&path, &keys, &legacy);
    for number in 0..4 {
        assert_eq!(
            restarted.read(&format!("connection-{number}")).unwrap(),
            Some(password(&format!("password-{number}")))
        );
    }
}

#[test]
fn concurrent_first_reads_share_one_successful_unlock() {
    let (_directory, path, keys, legacy) = fixture();
    open(&path, &keys, &legacy)
        .write("first", &password("one"))
        .unwrap();
    keys.reads.store(0, Ordering::SeqCst);
    keys.slow_reads.store(true, Ordering::SeqCst);
    let vault = Arc::new(open(&path, &keys, &legacy));
    let start = Arc::new(Barrier::new(9));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let vault = vault.clone();
            let start = start.clone();
            thread::spawn(move || {
                start.wait();
                vault.read("first")
            })
        })
        .collect();
    start.wait();
    for handle in handles {
        assert_eq!(handle.join().unwrap().unwrap(), Some(password("one")));
    }
    assert_eq!(keys.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn legacy_credentials_are_migrated_durably_before_the_legacy_store_is_unavailable() {
    let (_directory, path, keys, legacy) = fixture();
    let old = password("legacy-private-password");
    legacy.write("old", &old).unwrap();
    assert!(!path.exists());
    {
        let vault = open(&path, &keys, &legacy);
        assert_eq!(vault.read("old").unwrap(), Some(old.clone()));
    }
    assert!(path.is_file());
    legacy.reject_reads.store(true, Ordering::SeqCst);
    let restarted = open(&path, &keys, &legacy);
    assert_eq!(restarted.read("old").unwrap(), Some(old));
}

#[test]
fn failed_master_key_persistence_does_not_remove_legacy_credentials() {
    let (_directory, path, _, legacy) = fixture();
    let keys = Arc::new(MemoryKeys::default());
    keys.reject_writes.store(true, Ordering::SeqCst);
    legacy.write("old", &password("preserve-me")).unwrap();
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("old"), Err(AppError::PersistenceFailed));
    assert_eq!(legacy.read("old").unwrap(), Some(password("preserve-me")));
    assert!(!path.exists());

    keys.reject_writes.store(false, Ordering::SeqCst);
    assert_eq!(vault.read("old").unwrap(), Some(password("preserve-me")));
    assert_eq!(
        open(&path, &keys, &legacy).read("old").unwrap(),
        Some(password("preserve-me"))
    );
}

#[test]
fn failed_vault_file_persistence_keeps_legacy_credentials_for_retry() {
    let (directory, _, keys, legacy) = fixture();
    let blocked_parent = directory.path().join("parent-is-a-file");
    fs::write(&blocked_parent, b"keep this file").unwrap();
    let path = blocked_parent.join("credentials.vault");
    legacy.write("old", &password("preserve-me")).unwrap();

    assert_eq!(
        open(&path, &keys, &legacy).read("old"),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(legacy.read("old").unwrap(), Some(password("preserve-me")));
    assert_eq!(fs::read(&blocked_parent).unwrap(), b"keep this file");
}

#[test]
fn cancelling_unlock_can_be_retried_without_caching_the_error() {
    let (_directory, path, keys, legacy) = fixture();
    open(&path, &keys, &legacy)
        .write("first", &password("one"))
        .unwrap();
    keys.reads.store(0, Ordering::SeqCst);
    keys.reject_next_read.store(true, Ordering::SeqCst);
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("first"), Err(AppError::PersistenceFailed));
    assert_eq!(vault.read("first").unwrap(), Some(password("one")));
    assert_eq!(vault.read("first").unwrap(), Some(password("one")));
    assert_eq!(keys.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn rejected_legacy_access_is_retryable_and_is_not_saved_as_an_empty_credential() {
    let (_directory, path, keys, legacy) = fixture();
    legacy.write("old", &password("legacy-value")).unwrap();
    legacy.reject_reads.store(true, Ordering::SeqCst);
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("old"), Err(AppError::PersistenceFailed));
    legacy.reject_reads.store(false, Ordering::SeqCst);
    assert_eq!(vault.read("old").unwrap(), Some(password("legacy-value")));
    legacy.reject_reads.store(true, Ordering::SeqCst);
    assert_eq!(
        open(&path, &keys, &legacy).read("old").unwrap(),
        Some(password("legacy-value"))
    );
}

#[test]
fn corrupted_vault_is_not_replaced_or_hidden_by_legacy_fallback() {
    let (_directory, path, keys, legacy) = fixture();
    let corrupt = b"this is not an encrypted credential vault";
    fs::write(&path, corrupt).unwrap();
    legacy.write("old", &password("legacy-value")).unwrap();
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("old"), Err(AppError::PersistenceFailed));
    assert_eq!(
        vault.write("new", &password("replacement")),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(fs::read(&path).unwrap(), corrupt);
    assert_eq!(legacy.reads.load(Ordering::SeqCst), 0);
    assert_eq!(keys.writes.load(Ordering::SeqCst), 0);
}

#[test]
fn tampered_vault_is_rejected_without_overwriting_the_evidence() {
    let (_directory, path, keys, legacy) = fixture();
    open(&path, &keys, &legacy)
        .write("first", &password("one"))
        .unwrap();
    let mut tampered = fs::read(&path).unwrap();
    let middle = tampered.len() / 2;
    // A one-byte change must fail whether the persisted envelope is binary or JSON.
    tampered[middle] = if tampered[middle] == b'A' { b'B' } else { b'A' };
    fs::write(&path, &tampered).unwrap();
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("first"), Err(AppError::PersistenceFailed));
    assert_eq!(
        vault.write("second", &password("two")),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(fs::read(&path).unwrap(), tampered);
}

#[test]
fn missing_master_key_never_reinitializes_an_existing_vault() {
    let (_directory, path, keys, legacy) = fixture();
    open(&path, &keys, &legacy)
        .write("first", &password("one"))
        .unwrap();
    let original = fs::read(&path).unwrap();
    *keys.key.lock().unwrap() = None;
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("first"), Err(AppError::PersistenceFailed));
    assert_eq!(
        vault.write("second", &password("two")),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(keys.writes.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn wrong_master_key_cannot_read_or_overwrite_existing_credentials() {
    let (_directory, path, keys, legacy) = fixture();
    open(&path, &keys, &legacy)
        .write("first", &password("one"))
        .unwrap();
    let original = fs::read(&path).unwrap();
    *keys.key.lock().unwrap() = Some(vec![0x24; 32]);
    let vault = open(&path, &keys, &legacy);

    assert_eq!(vault.read("first"), Err(AppError::PersistenceFailed));
    assert_eq!(
        vault.write("second", &password("two")),
        Err(AppError::PersistenceFailed)
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn deleting_or_writing_empty_secrets_does_not_revive_legacy_values_after_restart() {
    let (_directory, path, keys, legacy) = fixture();
    legacy.write("deleted", &password("old-delete")).unwrap();
    legacy.write("emptied", &password("old-empty")).unwrap();
    let vault = open(&path, &keys, &legacy);
    vault.write("deleted", &password("new-delete")).unwrap();
    vault.write("emptied", &password("new-empty")).unwrap();
    vault.delete("deleted").unwrap();
    vault
        .write("emptied", &ConnectionSecrets::default())
        .unwrap();
    assert_eq!(legacy.read("deleted").unwrap(), None);
    assert_eq!(legacy.read("emptied").unwrap(), None);

    // Simulate a stale old-format entry surviving or being restored independently.
    legacy.write("deleted", &password("old-delete")).unwrap();
    legacy.write("emptied", &password("old-empty")).unwrap();
    let restarted = open(&path, &keys, &legacy);
    assert_eq!(restarted.read("deleted").unwrap(), None);
    assert_eq!(restarted.read("emptied").unwrap(), None);
}

#[test]
fn failed_legacy_cleanup_cannot_resurrect_deleted_credentials() {
    let (_directory, path, keys, legacy) = fixture();
    legacy.write("deleted", &password("old-delete")).unwrap();
    let vault = open(&path, &keys, &legacy);
    vault.write("deleted", &password("new-delete")).unwrap();
    // Ensure an old entry exists even when successful writes clean it up eagerly.
    legacy.write("deleted", &password("old-delete")).unwrap();
    legacy.reject_deletes.store(true, Ordering::SeqCst);
    let _cleanup_result = vault.delete("deleted");

    assert_eq!(
        legacy.read("deleted").unwrap(),
        Some(password("old-delete"))
    );
    assert_eq!(open(&path, &keys, &legacy).read("deleted").unwrap(), None);
}
