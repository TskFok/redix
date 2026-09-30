use std::{
    collections::BTreeMap,
    io::ErrorKind,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::{
    private_file::{lock_private_file, read_private_file, write_private_file},
    ConnectionSecrets, SecretStore,
};

const VAULT_VERSION: u32 = 1;
const VAULT_AAD: &[u8] = b"redix.connection-secrets.v1";
const KEY_BYTES: usize = 32;

// None is a durable deletion marker: old keychain entries must never resurrect.
type Secrets = BTreeMap<String, Option<ConnectionSecrets>>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncryptedDocument {
    version: u32,
    nonce: String,
    ciphertext: String,
}

pub trait VaultKeyStore: Send + Sync {
    fn read_key(&self) -> Result<Option<Vec<u8>>, AppError>;
    fn write_key(&self, key: &[u8]) -> Result<(), AppError>;
}

pub struct SystemVaultKeyStore;

impl SystemVaultKeyStore {
    fn entry(&self) -> Result<keyring::Entry, AppError> {
        // Separate service avoids collisions with any legacy connection ID.
        keyring::Entry::new("redix.vault", "master-key-v1").map_err(|_| AppError::PersistenceFailed)
    }
}

impl VaultKeyStore for SystemVaultKeyStore {
    fn read_key(&self) -> Result<Option<Vec<u8>>, AppError> {
        match self.entry()?.get_secret() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }

    fn write_key(&self, key: &[u8]) -> Result<(), AppError> {
        self.entry()?
            .set_secret(key)
            .map_err(|_| AppError::PersistenceFailed)
    }
}

pub struct CredentialVault {
    path: PathBuf,
    keys: Arc<dyn VaultKeyStore>,
    legacy: Arc<dyn SecretStore>,
    // Shared by every connection for this process, never sent to the frontend.
    key: Mutex<Option<LessSafeKey>>,
}

impl CredentialVault {
    pub fn new(path: PathBuf, keys: Arc<dyn VaultKeyStore>, legacy: Arc<dyn SecretStore>) -> Self {
        Self {
            path,
            keys,
            legacy,
            key: Mutex::new(None),
        }
    }

    fn unlock<'a>(
        &self,
        cached: &'a mut Option<LessSafeKey>,
        allow_create: bool,
    ) -> Result<&'a LessSafeKey, AppError> {
        if cached.is_none() {
            let bytes = match self.keys.read_key()? {
                Some(bytes) => bytes,
                None if allow_create => {
                    let mut bytes = vec![0; KEY_BYTES];
                    SystemRandom::new()
                        .fill(&mut bytes)
                        .map_err(|_| AppError::PersistenceFailed)?;
                    // Persist the key before any ciphertext. Failed unlocks are retryable.
                    self.keys.write_key(&bytes)?;
                    bytes
                }
                None => return Err(AppError::PersistenceFailed),
            };
            let key = UnboundKey::new(&aead::AES_256_GCM, &bytes)
                .map_err(|_| AppError::PersistenceFailed)?;
            *cached = Some(LessSafeKey::new(key));
        }
        cached.as_ref().ok_or(AppError::PersistenceFailed)
    }

    fn load(&self, key: &mut Option<LessSafeKey>) -> Result<Secrets, AppError> {
        let raw = match read_private_file(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Secrets::new()),
            Err(_) => return Err(AppError::PersistenceFailed),
        };
        let document: EncryptedDocument =
            serde_json::from_str(&raw).map_err(|_| AppError::PersistenceFailed)?;
        if document.version != VAULT_VERSION {
            return Err(AppError::PersistenceFailed);
        }
        let nonce: [u8; aead::NONCE_LEN] = STANDARD
            .decode(document.nonce)
            .map_err(|_| AppError::PersistenceFailed)?
            .try_into()
            .map_err(|_| AppError::PersistenceFailed)?;
        let mut ciphertext = STANDARD
            .decode(document.ciphertext)
            .map_err(|_| AppError::PersistenceFailed)?;
        // An existing vault with a missing key is an error, never a new empty vault.
        let plaintext = self.unlock(key, false)?.open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(VAULT_AAD),
            &mut ciphertext,
        );
        match plaintext {
            Ok(plaintext) => {
                serde_json::from_slice(plaintext).map_err(|_| AppError::PersistenceFailed)
            }
            Err(_) => {
                *key = None;
                Err(AppError::PersistenceFailed)
            }
        }
    }

    fn save(&self, secrets: &Secrets, key: &mut Option<LessSafeKey>) -> Result<(), AppError> {
        let key = self.unlock(key, true)?;
        let mut ciphertext =
            serde_json::to_vec(secrets).map_err(|_| AppError::PersistenceFailed)?;
        let mut nonce = [0; aead::NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| AppError::PersistenceFailed)?;
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(VAULT_AAD),
            &mut ciphertext,
        )
        .map_err(|_| AppError::PersistenceFailed)?;
        let document = EncryptedDocument {
            version: VAULT_VERSION,
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        let encoded = serde_json::to_vec(&document).map_err(|_| AppError::PersistenceFailed)?;
        write_private_file(&self.path, &encoded).map_err(|_| AppError::PersistenceFailed)
    }
}

impl SecretStore for CredentialVault {
    fn read(&self, connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        let mut key = self.key.lock().map_err(|_| AppError::PersistenceFailed)?;
        let _file_lock = lock_private_file(&self.path.with_extension("lock"))
            .map_err(|_| AppError::PersistenceFailed)?;
        let mut secrets = self.load(&mut key)?;
        if let Some(value) = secrets.get(connection_id) {
            return Ok(value.clone());
        }
        let legacy = self.legacy.read(connection_id)?;
        if let Some(value) = legacy.as_ref() {
            secrets.insert(connection_id.to_owned(), Some(value.clone()));
            // Copy first. Retain the old entry for compatibility until explicit deletion;
            // deleting a macOS keychain entry can itself prompt for authorization again.
            self.save(&secrets, &mut key)?;
        }
        Ok(legacy)
    }

    fn write(&self, connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
        if secrets.is_empty() {
            return self.delete(connection_id);
        }
        let mut key = self.key.lock().map_err(|_| AppError::PersistenceFailed)?;
        let _file_lock = lock_private_file(&self.path.with_extension("lock"))
            .map_err(|_| AppError::PersistenceFailed)?;
        // Reload under the process-shared file lock; only the master key is cached.
        let mut stored = self.load(&mut key)?;
        stored.insert(connection_id.to_owned(), Some(secrets.clone()));
        self.save(&stored, &mut key)
    }

    fn delete(&self, connection_id: &str) -> Result<(), AppError> {
        let mut key = self.key.lock().map_err(|_| AppError::PersistenceFailed)?;
        let _file_lock = lock_private_file(&self.path.with_extension("lock"))
            .map_err(|_| AppError::PersistenceFailed)?;
        let mut stored = self.load(&mut key)?;
        stored.insert(connection_id.to_owned(), None);
        // Commit the tombstone before cleanup, so interruption cannot revive old secrets.
        self.save(&stored, &mut key)?;
        self.legacy.delete(connection_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryKeys;
    impl VaultKeyStore for MemoryKeys {
        fn read_key(&self) -> Result<Option<Vec<u8>>, AppError> {
            Ok(Some(vec![42; 32]))
        }
        fn write_key(&self, _: &[u8]) -> Result<(), AppError> {
            Ok(())
        }
    }

    struct EmptyLegacy;
    impl SecretStore for EmptyLegacy {
        fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &ConnectionSecrets) -> Result<(), AppError> {
            Ok(())
        }
        fn delete(&self, _: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn saved_connections_share_an_encrypted_file_without_plaintext_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connection-secrets.json");
        let vault = CredentialVault::new(path.clone(), Arc::new(MemoryKeys), Arc::new(EmptyLegacy));
        for (id, password) in [
            ("a", "first-private-password"),
            ("b", "second-private-password"),
        ] {
            vault
                .write(
                    id,
                    &ConnectionSecrets {
                        password: Some(password.into()),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let encoded = std::fs::read_to_string(path)
            .expect("credentials must be persisted in the encrypted vault");
        assert!(!encoded.contains("first-private-password"));
        assert!(!encoded.contains("second-private-password"));
        assert_eq!(
            vault.read("a").unwrap().unwrap().password.as_deref(),
            Some("first-private-password")
        );
        assert_eq!(
            vault.read("b").unwrap().unwrap().password.as_deref(),
            Some("second-private-password")
        );
    }
}
