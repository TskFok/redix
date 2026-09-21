pub mod analysis_history;
mod document_store;
#[cfg(target_os = "macos")]
mod macos_permissions;
mod private_file;
mod profile_store;
mod secret_store;
#[cfg(windows)]
mod windows_permissions;

pub use document_store::{JsonDocumentStore, VersionedJsonDocument};
pub use private_file::prepare_sensitive_storage;
pub use profile_store::{migrate_legacy_ssh_paths, JsonProfileRepository, ProfileRepository};
pub use secret_store::{decode_stored_secret, ConnectionSecrets, SecretStore, SystemKeyring};
