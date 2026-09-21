use std::path::PathBuf;

use super::private_file::{read_private_file, write_private_file};

use serde::{Deserialize, Serialize};

use crate::{domain::ConnectionProfile, error::AppError, persistence::SecretStore};

pub trait ProfileRepository: Send + Sync {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError>;
    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError>;
}

pub fn migrate_legacy_ssh_paths(
    profiles: &dyn ProfileRepository,
    secrets: &dyn SecretStore,
) -> Result<(), AppError> {
    let loaded = profiles.load()?;
    let mut migrated = loaded.clone();
    let mut changed = false;

    for profile in &mut migrated {
        let Some(ssh) = profile.ssh.as_mut() else {
            continue;
        };
        // Older releases encoded identity-file authentication as Agent + a badge.
        // Make the method explicit before removing that transport compatibility rule.
        if ssh.auth_method == crate::domain::SshAuthMethod::Agent
            && (ssh.has_identity_file || ssh.legacy_identity_file.is_some())
        {
            ssh.auth_method = crate::domain::SshAuthMethod::PrivateKey;
            changed = true;
        }
        let Some(identity_file) = ssh.legacy_identity_file.clone() else {
            if ssh.legacy_known_hosts_file.is_none() {
                continue;
            }
            // The known-hosts-only case is handled below without duplicating the copy-first path.
            let mut stored = secrets.read(&profile.id)?.unwrap_or_default();
            stored.ssh_known_hosts_file = ssh.legacy_known_hosts_file.clone();
            secrets.write(&profile.id, &stored)?;
            ssh.has_known_hosts_file = true;
            ssh.legacy_known_hosts_file = None;
            changed = true;
            continue;
        };

        let mut stored = secrets.read(&profile.id)?.unwrap_or_default();
        stored.ssh_identity_file = Some(identity_file);
        if let Some(known_hosts_file) = ssh.legacy_known_hosts_file.clone() {
            stored.ssh_known_hosts_file = Some(known_hosts_file);
        }
        // Persist the copied value before removing it from the profile. A retry is idempotent.
        secrets.write(&profile.id, &stored)?;
        ssh.has_identity_file = true;
        ssh.has_known_hosts_file |= ssh.legacy_known_hosts_file.is_some();
        ssh.legacy_identity_file = None;
        ssh.legacy_known_hosts_file = None;
        changed = true;
    }

    if changed {
        profiles.save(&migrated)?;
    }
    Ok(())
}

pub struct JsonProfileRepository {
    pub path: PathBuf,
}

#[derive(Deserialize, Serialize)]
struct ProfileDocument {
    profiles: Vec<ConnectionProfile>,
}

impl JsonProfileRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ProfileRepository for JsonProfileRepository {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        let raw = match read_private_file(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(AppError::PersistenceFailed),
        };

        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str::<ProfileDocument>(&raw)
            .map(|document| document.profiles)
            .map_err(|_| AppError::PersistenceFailed)
    }

    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        let document = ProfileDocument {
            profiles: profiles.to_vec(),
        };
        let serialized =
            serde_json::to_vec_pretty(&document).map_err(|_| AppError::PersistenceFailed)?;
        write_private_file(&self.path, &serialized).map_err(|_| AppError::PersistenceFailed)
    }
}
