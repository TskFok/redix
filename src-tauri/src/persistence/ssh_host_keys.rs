use std::{
    collections::HashMap,
    io::{ErrorKind, Read},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine,
};
use ring::digest::{digest, SHA256};
use ssh2::{CheckResult, HostKeyType, KnownHostFileKind, KnownHosts};
use uuid::Uuid;

use super::private_file::{lock_private_file, open_private_file, write_private_file};
use crate::error::AppError;

const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING_CHALLENGES: usize = 128;
const MAX_STORE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SshHostKeyChallenge {
    pub challenge_id: String,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

struct PendingHostKey {
    challenge: SshHostKeyChallenge,
    key: Vec<u8>,
    created: Instant,
}

pub struct SshHostKeyStore {
    path: PathBuf,
    pending: Mutex<HashMap<Uuid, PendingHostKey>>,
}

impl SshHostKeyStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub fn verify(
        &self,
        host: &str,
        port: u16,
        key: &[u8],
        key_type: HostKeyType,
    ) -> Result<(), AppError> {
        let algorithm = key_algorithm(key_type)?;
        if !valid_host(host) || port == 0 || !valid_key(key, algorithm) {
            return Err(AppError::SshTunnelFailed);
        }
        let (known_hosts, _) = self.load()?;
        match known_hosts.check_port(host, port, key) {
            CheckResult::Match => Ok(()),
            CheckResult::Mismatch => Err(AppError::SshHostKeyChanged),
            CheckResult::Failure => Err(AppError::SshHostKeyStoreFailed),
            CheckResult::NotFound => {
                let now = Instant::now();
                let mut pending = self
                    .pending
                    .lock()
                    .map_err(|_| AppError::SshHostKeyStoreFailed)?;
                pending.retain(|_, value| now.duration_since(value.created) < CHALLENGE_TTL);
                if pending.len() >= MAX_PENDING_CHALLENGES {
                    if let Some(oldest) = pending
                        .iter()
                        .min_by_key(|(_, value)| value.created)
                        .map(|(id, _)| *id)
                    {
                        pending.remove(&oldest);
                    }
                }
                let id = Uuid::new_v4();
                pending.insert(
                    id,
                    PendingHostKey {
                        challenge: SshHostKeyChallenge {
                            challenge_id: id.to_string(),
                            host: host.to_owned(),
                            port,
                            algorithm: algorithm.to_owned(),
                            fingerprint: format!(
                                "SHA256:{}",
                                STANDARD_NO_PAD.encode(digest(&SHA256, key).as_ref())
                            ),
                        },
                        key: key.to_vec(),
                        created: now,
                    },
                );
                Err(AppError::SshHostKeyUnknown(id))
            }
        }
    }

    pub fn get_challenge(&self, id: Uuid) -> Result<SshHostKeyChallenge, AppError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| AppError::SshHostKeyStoreFailed)?;
        pending.retain(|_, value| value.created.elapsed() < CHALLENGE_TTL);
        pending
            .get(&id)
            .map(|value| value.challenge.clone())
            .ok_or(AppError::SshHostKeyChallengeExpired)
    }

    pub fn trust(&self, id: Uuid) -> Result<(), AppError> {
        // Consume once before I/O so a replay cannot approve another key or retry a failed write.
        let value = self
            .pending
            .lock()
            .map_err(|_| AppError::SshHostKeyStoreFailed)?
            .remove(&id)
            .filter(|value| value.created.elapsed() < CHALLENGE_TTL)
            .ok_or(AppError::SshHostKeyChallengeExpired)?;
        let _lock = lock_private_file(&self.path.with_extension("lock"))
            .map_err(|_| AppError::SshHostKeyStoreFailed)?;
        if value.created.elapsed() >= CHALLENGE_TTL {
            return Err(AppError::SshHostKeyChallengeExpired);
        }
        let (known_hosts, mut raw) = self.load()?;
        match known_hosts.check_port(&value.challenge.host, value.challenge.port, &value.key) {
            CheckResult::Match => return Ok(()),
            CheckResult::Mismatch => return Err(AppError::SshHostKeyChanged),
            CheckResult::Failure => return Err(AppError::SshHostKeyStoreFailed),
            CheckResult::NotFound => (),
        }
        if !raw.is_empty() && !raw.ends_with('\n') {
            raw.push('\n');
        }
        raw.push_str(&format!(
            "[{}]:{} {} {}\n",
            value.challenge.host,
            value.challenge.port,
            value.challenge.algorithm,
            STANDARD.encode(&value.key)
        ));
        if raw.len() as u64 > MAX_STORE_BYTES {
            return Err(AppError::SshHostKeyStoreFailed);
        }
        write_private_file(&self.path, raw.as_bytes()).map_err(|_| AppError::SshHostKeyStoreFailed)
    }

    pub fn discard(&self, id: Uuid) -> Result<(), AppError> {
        self.pending
            .lock()
            .map_err(|_| AppError::SshHostKeyStoreFailed)?
            .remove(&id);
        Ok(())
    }

    fn load(&self) -> Result<(KnownHosts, String), AppError> {
        let mut raw = String::new();
        match open_private_file(&self.path) {
            Ok(file) => {
                file.take(MAX_STORE_BYTES + 1)
                    .read_to_string(&mut raw)
                    .map_err(|_| AppError::SshHostKeyStoreFailed)?;
                if raw.len() as u64 > MAX_STORE_BYTES {
                    return Err(AppError::SshHostKeyStoreFailed);
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => (),
            Err(_) => return Err(AppError::SshHostKeyStoreFailed),
        }
        let session = ssh2::Session::new().map_err(|_| AppError::SshHostKeyStoreFailed)?;
        let mut known_hosts = session
            .known_hosts()
            .map_err(|_| AppError::SshHostKeyStoreFailed)?;
        for line in raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            // libssh2 can ignore unsupported/malformed lines. Validate the key fields first.
            let mut fields = line.split_whitespace();
            let host = fields.next().ok_or(AppError::SshHostKeyStoreFailed)?;
            let algorithm = fields.next().ok_or(AppError::SshHostKeyStoreFailed)?;
            let encoded_key = fields.next().ok_or(AppError::SshHostKeyStoreFailed)?;
            let key = STANDARD
                .decode(encoded_key)
                .map_err(|_| AppError::SshHostKeyStoreFailed)?;
            if !valid_host_field(host)
                || !supported_algorithm(algorithm)
                || !valid_key(&key, algorithm)
            {
                return Err(AppError::SshHostKeyStoreFailed);
            }
            known_hosts
                .read_str(line, KnownHostFileKind::OpenSSH)
                .map_err(|_| AppError::SshHostKeyStoreFailed)?;
        }
        Ok((known_hosts, raw))
    }
}

fn valid_host(host: &str) -> bool {
    host.len() <= 256
        && (host.parse::<std::net::Ipv6Addr>().is_ok()
            || (!host.is_empty()
                && !host.starts_with('-')
                && host.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                })))
}

fn valid_key(key: &[u8], algorithm: &str) -> bool {
    if key.len() > MAX_KEY_BYTES {
        return false;
    }
    let mut remaining = key;
    if take_key_field(&mut remaining) != Some(algorithm.as_bytes()) {
        return false;
    }
    let valid =
        match algorithm {
            "ssh-ed25519" => take_key_field(&mut remaining).is_some_and(|value| value.len() == 32),
            "ssh-rsa" => (0..2)
                .all(|_| take_key_field(&mut remaining).is_some_and(|value| !value.is_empty())),
            "ssh-dss" => (0..4)
                .all(|_| take_key_field(&mut remaining).is_some_and(|value| !value.is_empty())),
            "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => {
                take_key_field(&mut remaining) == Some(algorithm[11..].as_bytes())
                    && take_key_field(&mut remaining).is_some_and(|value| !value.is_empty())
            }
            _ => false,
        };
    valid && remaining.is_empty()
}

fn take_key_field<'a>(remaining: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u32::from_be_bytes(remaining.get(..4)?.try_into().ok()?) as usize;
    let bytes = remaining.get(4..)?;
    let value = bytes.get(..length)?;
    *remaining = bytes.get(length..)?;
    Some(value)
}

fn valid_host_field(field: &str) -> bool {
    if field.starts_with('|') {
        let parts = field.split('|').collect::<Vec<_>>();
        return parts.len() == 4
            && parts[0].is_empty()
            && parts[1] == "1"
            && parts[2..]
                .iter()
                .all(|value| STANDARD.decode(value).is_ok_and(|bytes| bytes.len() == 20));
    }
    field.split(',').all(|host| {
        if let Some(bracketed) = host.strip_prefix('[') {
            bracketed.rsplit_once("]:").is_some_and(|(host, port)| {
                valid_host(host) && port.parse::<u16>().is_ok_and(|port| port != 0)
            })
        } else {
            valid_host(host)
        }
    })
}

fn supported_algorithm(algorithm: &str) -> bool {
    matches!(
        algorithm,
        "ssh-rsa"
            | "ssh-dss"
            | "ecdsa-sha2-nistp256"
            | "ecdsa-sha2-nistp384"
            | "ecdsa-sha2-nistp521"
            | "ssh-ed25519"
    )
}

fn key_algorithm(key_type: HostKeyType) -> Result<&'static str, AppError> {
    match key_type {
        HostKeyType::Rsa => Ok("ssh-rsa"),
        HostKeyType::Dss => Ok("ssh-dss"),
        HostKeyType::Ecdsa256 => Ok("ecdsa-sha2-nistp256"),
        HostKeyType::Ecdsa384 => Ok("ecdsa-sha2-nistp384"),
        HostKeyType::Ecdsa521 => Ok("ecdsa-sha2-nistp521"),
        HostKeyType::Ed25519 => Ok("ssh-ed25519"),
        HostKeyType::Unknown => Err(AppError::SshTunnelFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(&11_u32.to_be_bytes());
        key.extend_from_slice(b"ssh-ed25519");
        key.extend_from_slice(&32_u32.to_be_bytes());
        key.extend_from_slice(&[7_u8; 32]);
        key
    }

    fn challenge_id(store: &SshHostKeyStore, host: &str) -> Uuid {
        match store.verify(host, 22, &key(), HostKeyType::Ed25519) {
            Err(AppError::SshHostKeyUnknown(id)) => id,
            result => panic!("expected challenge, got {result:?}"),
        }
    }

    #[test]
    fn expired_challenges_cannot_be_displayed_or_confirmed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("known_hosts");
        let store = SshHostKeyStore::new(path.clone());
        let id = challenge_id(&store, "example.com");
        store.pending.lock().unwrap().get_mut(&id).unwrap().created =
            Instant::now() - CHALLENGE_TTL;
        assert_eq!(
            store.get_challenge(id),
            Err(AppError::SshHostKeyChallengeExpired)
        );
        assert_eq!(store.trust(id), Err(AppError::SshHostKeyChallengeExpired));
        assert!(!path.exists());
    }

    #[test]
    fn too_many_pending_challenges_expire_the_oldest_without_trusting_it() {
        let directory = tempfile::tempdir().unwrap();
        let store = SshHostKeyStore::new(directory.path().join("known_hosts"));
        let oldest = challenge_id(&store, "oldest.example");
        for index in 0..MAX_PENDING_CHALLENGES {
            challenge_id(&store, &format!("host-{index}.example"));
        }
        assert_eq!(
            store.trust(oldest),
            Err(AppError::SshHostKeyChallengeExpired)
        );
        assert_eq!(store.pending.lock().unwrap().len(), MAX_PENDING_CHALLENGES);
    }
}
