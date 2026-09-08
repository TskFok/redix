use crate::{
    domain::{
        validate_certificate_pem, validate_private_key_pem, ConnectionExportDocument,
        ConnectionImportFailure, ConnectionInfo, ConnectionProfile, ImportConnectionsInput,
        ImportConnectionsResult, SaveConnectionInput, SshAuthMethod, TestConnectionInput,
    },
    error::AppError,
    persistence::ConnectionSecrets,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn list_connections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConnectionProfile>, AppError> {
    let _transaction = state.redis.profile_transaction().await;
    state.profiles.load()
}

#[tauri::command]
pub async fn save_connection(
    state: tauri::State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError> {
    save_connection_inner(state.inner(), input).await
}

pub(crate) async fn save_connection_inner(
    state: &AppState,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError> {
    let mut profile = input.profile.clone();
    profile.validate()?;

    let _transaction = state.redis.profile_transaction().await;
    let connection_id = profile.id.clone();
    let old_profiles = state.profiles.load()?;
    let old_profile = old_profiles.iter().find(|item| item.id == connection_id);
    let old_secret = state.secrets.read(&connection_id)?;
    let desired_secret = resolve_secrets(&mut profile, &input, old_profile, old_secret.as_ref())?;
    profile.has_password = desired_secret
        .as_ref()
        .and_then(|secrets| secrets.password.as_ref())
        .is_some();
    profile.has_ca_certificate = desired_secret
        .as_ref()
        .and_then(|secrets| secrets.ca_certificate.as_ref())
        .is_some();
    profile.has_client_certificate = desired_secret
        .as_ref()
        .map(ConnectionSecrets::has_client_certificate)
        .unwrap_or(false);

    let mut profiles = old_profiles.clone();
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }

    if desired_secret != old_secret {
        let result = match desired_secret.as_ref() {
            Some(secrets) => state.secrets.write(&connection_id, secrets),
            None => state.secrets.delete(&connection_id),
        };
        if result.is_err() {
            restore_profile_and_secret(state, &old_profiles, &connection_id, old_secret.as_ref())
                .map_err(|_| AppError::PersistenceFailed)?;
            return Err(AppError::PersistenceFailed);
        }
    }

    if state.profiles.save(&profiles).is_err() {
        restore_profile_and_secret(state, &old_profiles, &connection_id, old_secret.as_ref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(AppError::PersistenceFailed);
    }

    Ok(profile)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    delete_connection_inner(state.inner(), &connection_id).await
}

pub(crate) async fn delete_connection_inner(
    state: &AppState,
    connection_id: &str,
) -> Result<(), AppError> {
    let _cli_lifecycle = state.cli.lifecycle_guard().await;
    let _transaction = state.redis.profile_transaction().await;
    let old_profiles = state.profiles.load()?;
    if !old_profiles
        .iter()
        .any(|profile| profile.id == connection_id)
    {
        return Err(AppError::InvalidConnection);
    }
    let old_secret = state.secrets.read(connection_id)?;

    let mut profiles = old_profiles.clone();
    profiles.retain(|profile| profile.id != connection_id);
    if state.profiles.save(&profiles).is_err() {
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_ref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(AppError::PersistenceFailed);
    }

    if state.secrets.delete(connection_id).is_err() {
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_ref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(AppError::PersistenceFailed);
    }

    if let Err(error) = state.redis.close_connection(connection_id).await {
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_ref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(error);
    }
    state.cli.close_connection(connection_id).await;

    Ok(())
}

#[tauri::command]
pub async fn export_connections(
    state: tauri::State<'_, AppState>,
) -> Result<ConnectionExportDocument, AppError> {
    export_connections_inner(state.inner()).await
}

pub(crate) async fn export_connections_inner(
    state: &AppState,
) -> Result<ConnectionExportDocument, AppError> {
    let _transaction = state.redis.profile_transaction().await;
    let profiles = state.profiles.load()?;
    Ok(ConnectionExportDocument::from_profiles(&profiles))
}

#[tauri::command]
pub async fn import_connections(
    state: tauri::State<'_, AppState>,
    input: ImportConnectionsInput,
) -> Result<ImportConnectionsResult, AppError> {
    import_connections_inner(state.inner(), input).await
}

pub(crate) async fn import_connections_inner(
    state: &AppState,
    input: ImportConnectionsInput,
) -> Result<ImportConnectionsResult, AppError> {
    const MAX_IMPORT_BYTES: usize = 10 * 1024 * 1024;
    if input.content.len() > MAX_IMPORT_BYTES {
        return Err(AppError::InvalidInput);
    }

    let normalized = crate::domain::normalize_import_document(&input.content)?;
    let ignored_secret_fields = normalized.ignored_secret_fields;
    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for entry in normalized.entries {
        if let Some(unsupported_type) = entry.unsupported_type.as_deref() {
            failed.push(ConnectionImportFailure {
                index: entry.source_index,
                name: non_empty_name(&entry.name),
                code: AppError::InvalidConnection.code().to_owned(),
                message: format!("不支持的连接类型或拓扑：{unsupported_type}"),
            });
            continue;
        }

        let profile = ConnectionProfile {
            ssh: entry.ssh,
            sentinel: entry.sentinel,
            cluster: entry.cluster,
            id: uuid::Uuid::new_v4().to_string(),
            name: entry.name.clone(),
            host: entry.host,
            port: entry.port.unwrap_or_default(),
            username: entry.username,
            database: entry.database.unwrap_or_default(),
            has_password: false,
            tls: entry.tls,
            verify_server_cert: entry.verify_server_cert,
            ca_certificate_name: entry.ca_certificate_name,
            client_certificate_name: entry.client_certificate_name,
            has_ca_certificate: false,
            has_client_certificate: false,
        };

        if let Err(error) = profile.validate() {
            failed.push(ConnectionImportFailure {
                index: entry.source_index,
                name: non_empty_name(&profile.name),
                code: error.code().to_owned(),
                message: error.message().to_owned(),
            });
        } else {
            imported.push(profile);
        }
    }

    if !imported.is_empty() {
        let _transaction = state.redis.profile_transaction().await;
        let old_profiles = state.profiles.load()?;
        let mut profiles = old_profiles.clone();
        profiles.extend(imported.iter().cloned());
        if state.profiles.save(&profiles).is_err() {
            state
                .profiles
                .save(&old_profiles)
                .map_err(|_| AppError::PersistenceFailed)?;
            return Err(AppError::PersistenceFailed);
        }
    }

    Ok(ImportConnectionsResult {
        imported,
        failed,
        ignored_secret_fields,
    })
}

fn non_empty_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn resolve_secrets(
    profile: &mut ConnectionProfile,
    input: &SaveConnectionInput,
    old_profile: Option<&ConnectionProfile>,
    old_secret: Option<&ConnectionSecrets>,
) -> Result<Option<ConnectionSecrets>, AppError> {
    let mut secrets = old_secret.cloned().unwrap_or_default();
    resolve_ssh_secrets(profile, input, &mut secrets)?;

    if let Some(sentinel) = profile.sentinel.as_mut() {
        match input.sentinel_password.as_deref() {
            Some(password) if !password.is_empty() => {
                secrets.sentinel_password = Some(password.to_owned())
            }
            Some(_) => return Err(AppError::InvalidInput),
            None if sentinel.has_password => {
                if !old_profile
                    .and_then(|old| old.sentinel.as_ref())
                    .is_some_and(|old| old.has_password)
                    || secrets.sentinel_password.is_none()
                {
                    return Err(AppError::InvalidConnection);
                }
            }
            None => secrets.sentinel_password = None,
        }
        sentinel.has_password = secrets.sentinel_password.is_some();
    } else {
        secrets.sentinel_password = None;
    }

    match input.password.as_deref() {
        Some(password) if !password.is_empty() => secrets.password = Some(password.to_owned()),
        Some(_) => return Err(AppError::InvalidInput),
        None if profile.has_password => {
            if !old_profile.map(|item| item.has_password).unwrap_or(false)
                || secrets.password.is_none()
            {
                return Err(AppError::InvalidConnection);
            }
        }
        None => secrets.password = None,
    }

    if input.clear_ca_certificate {
        secrets.ca_certificate = None;
        profile.ca_certificate_name = None;
    } else if let Some(certificate) = input.ca_certificate.as_deref() {
        validate_certificate_pem(certificate)?;
        secrets.ca_certificate = Some(certificate.trim().to_owned());
        if profile
            .ca_certificate_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .is_none()
        {
            profile.ca_certificate_name =
                old_profile.and_then(|item| item.ca_certificate_name.clone());
        }
        if profile
            .ca_certificate_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .is_none()
        {
            return Err(AppError::InvalidInput);
        }
    } else if profile.has_ca_certificate {
        if secrets.ca_certificate.is_none() {
            return Err(AppError::InvalidConnection);
        }
    } else {
        secrets.ca_certificate = None;
        profile.ca_certificate_name = None;
    }

    if input.clear_client_certificate {
        secrets.client_certificate = None;
        secrets.client_key = None;
        profile.client_certificate_name = None;
    } else {
        match (
            input.client_certificate.as_deref(),
            input.client_key.as_deref(),
        ) {
            (Some(certificate), Some(key)) => {
                validate_certificate_pem(certificate)?;
                validate_private_key_pem(key)?;
                secrets.client_certificate = Some(certificate.trim().to_owned());
                secrets.client_key = Some(key.trim().to_owned());
                if profile
                    .client_certificate_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .is_none()
                {
                    profile.client_certificate_name =
                        old_profile.and_then(|item| item.client_certificate_name.clone());
                }
                if profile
                    .client_certificate_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .is_none()
                {
                    return Err(AppError::InvalidInput);
                }
            }
            (None, None) if profile.has_client_certificate => {
                if !secrets.has_client_certificate() {
                    return Err(AppError::InvalidConnection);
                }
            }
            (None, None) => {
                secrets.client_certificate = None;
                secrets.client_key = None;
                profile.client_certificate_name = None;
            }
            _ => return Err(AppError::InvalidInput),
        }
    }

    if secrets.is_empty() {
        Ok(None)
    } else {
        Ok(Some(secrets))
    }
}

fn resolve_ssh_secrets(
    profile: &mut ConnectionProfile,
    input: &SaveConnectionInput,
    secrets: &mut ConnectionSecrets,
) -> Result<(), AppError> {
    validate_ssh_material(
        [
            &input.ssh_password,
            &input.ssh_private_key,
            &input.ssh_passphrase,
        ],
        [&input.ssh_identity_file, &input.ssh_known_hosts_file],
    )?;
    if input.clear_ssh_secrets || profile.ssh.is_none() {
        secrets.ssh_password = None;
        secrets.ssh_private_key = None;
        secrets.ssh_passphrase = None;
        secrets.ssh_identity_file = None;
        secrets.ssh_known_hosts_file = None;
    }
    let Some(ssh) = profile.ssh.as_mut() else {
        return Ok(());
    };
    // Local paths supplied in a profile are never an alternative to SecretStore.
    ssh.legacy_identity_file = None;
    ssh.legacy_known_hosts_file = None;
    if let Some(path) = &input.ssh_known_hosts_file {
        secrets.ssh_known_hosts_file = Some(path.clone());
    }
    match ssh.auth_method {
        SshAuthMethod::Agent => {
            secrets.ssh_password = None;
            secrets.ssh_private_key = None;
            secrets.ssh_identity_file = None;
            secrets.ssh_passphrase = None;
        }
        SshAuthMethod::Password => {
            secrets.ssh_private_key = None;
            secrets.ssh_identity_file = None;
            secrets.ssh_passphrase = None;
            if let Some(password) = &input.ssh_password {
                secrets.ssh_password = Some(password.clone());
            }
            if secrets.ssh_password.as_deref().is_none_or(str::is_empty) {
                return Err(AppError::InvalidConnection);
            }
        }
        SshAuthMethod::PrivateKey => {
            secrets.ssh_password = None;
            if let Some(key) = &input.ssh_private_key {
                // ssh2 accepts OpenSSH, PEM and encrypted keys. TLS key validation
                // is intentionally not used; ssh2 validates the selected key at authentication.
                secrets.ssh_private_key = Some(key.clone());
            }
            if let Some(path) = &input.ssh_identity_file {
                secrets.ssh_identity_file = Some(path.clone());
            }
            if let Some(passphrase) = &input.ssh_passphrase {
                secrets.ssh_passphrase = Some(passphrase.clone());
            }
            if secrets.ssh_private_key.as_deref().is_none_or(str::is_empty)
                && secrets.ssh_identity_file.is_none()
            {
                return Err(AppError::InvalidConnection);
            }
        }
    }
    validate_ssh_material(
        [
            &secrets.ssh_password,
            &secrets.ssh_private_key,
            &secrets.ssh_passphrase,
        ],
        [&secrets.ssh_identity_file, &secrets.ssh_known_hosts_file],
    )?;
    ssh.has_password = secrets.ssh_password.is_some();
    ssh.has_private_key = secrets.ssh_private_key.is_some();
    ssh.has_passphrase = secrets.ssh_passphrase.is_some();
    ssh.has_identity_file = secrets.ssh_identity_file.is_some();
    ssh.has_known_hosts_file = secrets.ssh_known_hosts_file.is_some();
    Ok(())
}

fn validate_ssh_material(
    payloads: [&Option<String>; 3],
    paths: [&Option<String>; 2],
) -> Result<(), AppError> {
    const MAX_SSH_SECRET_BYTES: usize = 1024 * 1024;
    for value in payloads.into_iter().flatten() {
        if value.is_empty() || value.len() > MAX_SSH_SECRET_BYTES || value.contains('\0') {
            return Err(AppError::InvalidInput);
        }
    }
    for path in paths.into_iter().flatten() {
        if path.len() > 4096
            || !std::path::Path::new(path).is_absolute()
            || path.chars().any(char::is_control)
        {
            return Err(AppError::InvalidInput);
        }
    }
    Ok(())
}

fn restore_profile_and_secret(
    state: &AppState,
    profiles: &[ConnectionProfile],
    connection_id: &str,
    secret: Option<&ConnectionSecrets>,
) -> Result<(), AppError> {
    let profile_result = state.profiles.save(profiles);
    let secret_result = match secret {
        Some(secret) => state.secrets.write(connection_id, secret),
        None => state.secrets.delete(connection_id),
    };

    if profile_result.is_err() || secret_result.is_err() {
        Err(AppError::PersistenceFailed)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn test_connection(
    state: tauri::State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<ConnectionInfo, AppError> {
    let transaction = state.redis.profile_transaction().await;
    let old_profiles = state.profiles.load()?;
    let old_profile = old_profiles
        .iter()
        .find(|profile| profile.id == input.profile.id);
    let old_secret = if old_profile.is_some() {
        state.secrets.read(&input.profile.id)?
    } else {
        None
    };
    let mut profile = input.profile.clone();
    profile.validate()?;
    let secrets = resolve_secrets(&mut profile, &input, old_profile, old_secret.as_ref())?
        .unwrap_or_default();
    drop(transaction);
    state.redis.test_connection(&profile, &secrets).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionInfo, AppError> {
    open_connection_inner(state.inner(), &connection_id).await
}

pub(crate) async fn open_connection_inner(
    state: &AppState,
    connection_id: &str,
) -> Result<ConnectionInfo, AppError> {
    let _cli_lifecycle = state.cli.lifecycle_guard().await;
    let info = state.redis.open_connection(connection_id).await?;
    state.cli.close_connection(connection_id).await;
    Ok(info)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn close_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    let _cli_lifecycle = state.cli.lifecycle_guard().await;
    state.redis.close_connection(&connection_id).await?;
    state.cli.close_connection(&connection_id).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
    };

    use super::*;
    use crate::domain::{CliCommandInput, CliSessionInput};
    use crate::persistence::{ConnectionSecrets, ProfileRepository, SecretStore};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn absolute_test_path(name: &str) -> String {
        let path = std::env::temp_dir().join(name);
        assert!(path.is_absolute());
        path.to_string_lossy().into_owned()
    }

    fn json_contains_string(value: &serde_json::Value, needle: &str) -> bool {
        match value {
            serde_json::Value::String(value) => value.contains(needle),
            serde_json::Value::Array(values) => values
                .iter()
                .any(|value| json_contains_string(value, needle)),
            serde_json::Value::Object(values) => values
                .values()
                .any(|value| json_contains_string(value, needle)),
            _ => false,
        }
    }

    async fn spawn_cli_redis(label: &'static str) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_active = Arc::clone(&active);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let active = Arc::clone(&server_active);
                active.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    struct ActiveGuard(Arc<std::sync::atomic::AtomicUsize>);
                    impl Drop for ActiveGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _active = ActiveGuard(active);
                    let mut pending = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    loop {
                        let size = match socket.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(size) => size,
                        };
                        pending.extend_from_slice(&buffer[..size]);
                        while let Some((consumed, command)) = parse_test_resp_command(&pending) {
                            pending.drain(..consumed);
                            let response = match command.first().map(Vec::as_slice) {
                                Some(b"PING") => "+PONG\r\n".into(),
                                Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                                Some(b"GET") => format!("${}\r\n{label}\r\n", label.len()),
                                _ => "+OK\r\n".into(),
                            };
                            socket.write_all(response.as_bytes()).await.unwrap();
                        }
                    }
                });
            }
        });
        (port, active)
    }

    async fn spawn_paused_cli_redis(
        label: &'static str,
    ) -> (
        u16,
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let gate = Arc::new(tokio::sync::Mutex::new(Some((entered_tx, resume_rx))));
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let gate = Arc::clone(&gate);
                tokio::spawn(async move {
                    let mut pending = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    loop {
                        let size = match socket.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(size) => size,
                        };
                        pending.extend_from_slice(&buffer[..size]);
                        while let Some((consumed, command)) = parse_test_resp_command(&pending) {
                            pending.drain(..consumed);
                            let response = match command.first().map(Vec::as_slice) {
                                Some(b"PING") => {
                                    if let Some((entered, resume)) = gate.lock().await.take() {
                                        entered.send(()).unwrap();
                                        resume.await.unwrap();
                                    }
                                    "+PONG\r\n".into()
                                }
                                Some(b"INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                                Some(b"GET") => {
                                    format!("${}\r\n{label}\r\n", label.len())
                                }
                                _ => "+OK\r\n".into(),
                            };
                            socket.write_all(response.as_bytes()).await.unwrap();
                        }
                    }
                });
            }
        });
        (port, entered_rx, resume_tx)
    }

    fn parse_test_resp_command(buffer: &[u8]) -> Option<(usize, Vec<Vec<u8>>)> {
        if buffer.first() != Some(&b'*') {
            return None;
        }
        let header_end = buffer.windows(2).position(|pair| pair == b"\r\n")?;
        let count = std::str::from_utf8(&buffer[1..header_end])
            .ok()?
            .parse::<usize>()
            .ok()?;
        let mut offset = header_end + 2;
        let mut args = Vec::with_capacity(count);
        for _ in 0..count {
            let length_end = buffer[offset..]
                .windows(2)
                .position(|pair| pair == b"\r\n")?
                + offset;
            let length = std::str::from_utf8(&buffer[offset + 1..length_end])
                .ok()?
                .parse::<usize>()
                .ok()?;
            offset = length_end + 2;
            let end = offset.checked_add(length)?;
            if buffer.get(end..end + 2)? != b"\r\n" {
                return None;
            }
            args.push(buffer[offset..end].to_vec());
            offset = end + 2;
        }
        Some((offset, args))
    }

    async fn wait_for_active_connections(active: &std::sync::atomic::AtomicUsize, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while active.load(Ordering::SeqCst) != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    struct RecordingProfileRepository {
        profiles: Mutex<Vec<ConnectionProfile>>,
        save_failures: Mutex<VecDeque<bool>>,
        save_attempts: Mutex<Vec<Vec<ConnectionProfile>>>,
    }

    impl RecordingProfileRepository {
        fn new(profiles: Vec<ConnectionProfile>) -> Self {
            Self {
                profiles: Mutex::new(profiles),
                save_failures: Mutex::new(VecDeque::new()),
                save_attempts: Mutex::new(Vec::new()),
            }
        }

        fn fail_next_saves(&self, failures: &[bool]) {
            *self.save_failures.lock().unwrap() = failures.iter().copied().collect();
        }

        fn current(&self) -> Vec<ConnectionProfile> {
            self.profiles.lock().unwrap().clone()
        }

        fn save_attempts(&self) -> Vec<Vec<ConnectionProfile>> {
            self.save_attempts.lock().unwrap().clone()
        }
    }

    impl ProfileRepository for RecordingProfileRepository {
        fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
            Ok(self.current())
        }

        fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
            self.save_attempts.lock().unwrap().push(profiles.to_vec());
            if self
                .save_failures
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(false)
            {
                return Err(AppError::PersistenceFailed);
            }
            *self.profiles.lock().unwrap() = profiles.to_vec();
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum SecretOperation {
        Read,
        Write(String),
        Delete,
    }

    struct RecordingSecretStore {
        secret: Mutex<Option<ConnectionSecrets>>,
        write_failures: Mutex<VecDeque<bool>>,
        delete_failures: Mutex<VecDeque<bool>>,
        mutate_failed_write: AtomicBool,
        mutate_failed_delete: AtomicBool,
        operations: Mutex<Vec<SecretOperation>>,
    }

    impl RecordingSecretStore {
        fn new(secret: Option<&str>) -> Self {
            Self {
                secret: Mutex::new(secret.map(|password| ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                })),
                write_failures: Mutex::new(VecDeque::new()),
                delete_failures: Mutex::new(VecDeque::new()),
                mutate_failed_write: AtomicBool::new(false),
                mutate_failed_delete: AtomicBool::new(false),
                operations: Mutex::new(Vec::new()),
            }
        }

        fn fail_next_write_after_mutation(&self) {
            self.mutate_failed_write.store(true, Ordering::SeqCst);
            self.write_failures.lock().unwrap().push_back(true);
        }

        fn fail_next_delete_after_mutation(&self) {
            self.mutate_failed_delete.store(true, Ordering::SeqCst);
            self.delete_failures.lock().unwrap().push_back(true);
        }

        fn current(&self) -> Option<String> {
            self.secret
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|secrets| secrets.password.clone())
        }

        fn operations(&self) -> Vec<SecretOperation> {
            self.operations.lock().unwrap().clone()
        }
    }

    impl SecretStore for RecordingSecretStore {
        fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            self.operations.lock().unwrap().push(SecretOperation::Read);
            Ok(self.secret.lock().unwrap().clone())
        }

        fn write(&self, _connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
            self.operations.lock().unwrap().push(SecretOperation::Write(
                secrets.password.clone().unwrap_or_default(),
            ));
            let failed = self
                .write_failures
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(false);
            if failed {
                if self.mutate_failed_write.load(Ordering::SeqCst) {
                    *self.secret.lock().unwrap() = Some(secrets.clone());
                }
                return Err(AppError::PersistenceFailed);
            }
            *self.secret.lock().unwrap() = Some(secrets.clone());
            Ok(())
        }

        fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
            self.operations
                .lock()
                .unwrap()
                .push(SecretOperation::Delete);
            let failed = self
                .delete_failures
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(false);
            if failed {
                if self.mutate_failed_delete.load(Ordering::SeqCst) {
                    *self.secret.lock().unwrap() = None;
                }
                return Err(AppError::PersistenceFailed);
            }
            *self.secret.lock().unwrap() = None;
            Ok(())
        }
    }

    fn profile(id: &str, name: &str, has_password: bool) -> ConnectionProfile {
        ConnectionProfile {
            ssh: None,
            sentinel: None,
            cluster: None,
            id: id.into(),
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            database: 0,
            has_password,
            tls: false,
            verify_server_cert: true,
            ca_certificate_name: None,
            client_certificate_name: None,
            has_ca_certificate: false,
            has_client_certificate: false,
        }
    }

    fn state_with(
        profiles: Vec<ConnectionProfile>,
        secret: Option<&str>,
    ) -> (
        AppState,
        Arc<RecordingProfileRepository>,
        Arc<RecordingSecretStore>,
    ) {
        let profile_store = Arc::new(RecordingProfileRepository::new(profiles));
        let secret_store = Arc::new(RecordingSecretStore::new(secret));
        let state = AppState::new(profile_store.clone(), secret_store.clone());
        (state, profile_store, secret_store)
    }

    fn save_input(profile: ConnectionProfile, password: Option<&str>) -> SaveConnectionInput {
        SaveConnectionInput {
            ssh_password: None,
            ssh_private_key: None,
            ssh_passphrase: None,
            ssh_identity_file: None,
            ssh_known_hosts_file: None,
            clear_ssh_secrets: false,
            sentinel_password: None,
            profile,
            password: password.map(str::to_owned),
            ca_certificate: None,
            client_certificate: None,
            client_key: None,
            clear_ca_certificate: false,
            clear_client_certificate: false,
        }
    }

    fn ssh_request(auth: &str, fields: serde_json::Value) -> SaveConnectionInput {
        let mut request = serde_json::json!({
            "profile": profile("ssh-test", "SSH", false),
        });
        request["profile"]["ssh"] = serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "alice",
            "auth_method": auth,
        });
        for (key, value) in fields.as_object().unwrap() {
            request[key] = value.clone();
        }
        serde_json::from_value(request).unwrap()
    }

    #[tokio::test]
    async fn ssh_save_request_persists_secrets_and_derives_flags_without_exposing_paths() {
        let (state, profiles, secrets) = state_with(vec![], None);
        let known_hosts = absolute_test_path("redix-known-hosts");
        let input = ssh_request(
            "password",
            serde_json::json!({
                "ssh_password": "  password with whitespace  ",
                "ssh_known_hosts_file": known_hosts.clone()
            }),
        );
        let saved = save_connection_inner(&state, input).await.unwrap();
        let stored = secrets.read("ssh-test").unwrap().unwrap();
        assert_eq!(
            stored.ssh_password.as_deref(),
            Some("  password with whitespace  ")
        );
        assert_eq!(
            stored.ssh_known_hosts_file.as_deref(),
            Some(known_hosts.as_str())
        );
        assert!(saved.ssh.as_ref().unwrap().has_password);
        assert!(saved.ssh.as_ref().unwrap().has_known_hosts_file);
        let profile_json = serde_json::to_value(profiles.load().unwrap()).unwrap();
        let export_json =
            serde_json::to_value(export_connections_inner(&state).await.unwrap()).unwrap();
        assert!(!json_contains_string(&profile_json, &known_hosts));
        assert!(!json_contains_string(&export_json, &known_hosts));
    }

    #[tokio::test]
    async fn ssh_missing_auth_material_and_forged_flags_are_rejected() {
        for auth in ["password", "private_key"] {
            let (state, profiles, secrets) = state_with(vec![], None);
            let mut input = ssh_request(auth, serde_json::json!({}));
            let ssh = input.profile.ssh.as_mut().unwrap();
            ssh.has_password = true;
            ssh.has_private_key = true;
            ssh.has_identity_file = true;
            ssh.legacy_identity_file = Some(absolute_test_path("untrusted-profile-key"));
            assert!(matches!(
                save_connection_inner(&state, input).await,
                Err(AppError::InvalidConnection)
            ));
            assert!(profiles.load().unwrap().is_empty());
            assert!(secrets.read("ssh-test").unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn ssh_save_rejects_unsafe_paths_and_oversized_payloads() {
        let absolute_identity = absolute_test_path("redix-key");
        let absolute_hosts = absolute_test_path("redix-hosts");
        let oversized_absolute = absolute_test_path(&"x".repeat(8192));
        for fields in [
            serde_json::json!({"ssh_identity_file": "relative/key"}),
            serde_json::json!({"ssh_identity_file": format!("{absolute_identity}\tother")}),
            serde_json::json!({"ssh_known_hosts_file": format!("{absolute_hosts}\u{7f}")}),
            serde_json::json!({"ssh_known_hosts_file": oversized_absolute}),
            serde_json::json!({"ssh_private_key": "x".repeat(1024 * 1024 + 1)}),
            serde_json::json!({"ssh_password": "x".repeat(1024 * 1024 + 1)}),
        ] {
            let (state, profiles, _) = state_with(vec![], None);
            assert!(matches!(
                save_connection_inner(&state, ssh_request("agent", fields)).await,
                Err(AppError::InvalidInput)
            ));
            assert!(profiles.load().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn ssh_credentials_survive_omission_but_clear_replace_and_mode_switch_are_explicit() {
        let (state, _, store) = state_with(vec![], None);
        let identity = absolute_test_path("redix-key");
        let known_hosts = absolute_test_path("redix-known-hosts");
        let replacement_identity = absolute_test_path("redix-replacement-key");
        let first = ssh_request(
            "private_key",
            serde_json::json!({
                "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n-----END OPENSSH PRIVATE KEY-----",
                "ssh_passphrase": "  phrase  ", "ssh_identity_file": identity,
                "ssh_known_hosts_file": known_hosts, "password": "redis-password"
            }),
        );
        let saved = save_connection_inner(&state, first).await.unwrap();
        let original = store.read("ssh-test").unwrap().unwrap();
        assert_eq!(original.ssh_passphrase.as_deref(), Some("  phrase  "));
        assert!(original
            .ssh_private_key
            .as_deref()
            .unwrap()
            .contains("OPENSSH"));

        let mut omitted = save_input(saved.clone(), None);
        omitted.profile.ssh.as_mut().unwrap().has_private_key = false;
        omitted.profile.ssh.as_mut().unwrap().has_identity_file = false;
        save_connection_inner(&state, omitted).await.unwrap();
        assert_eq!(store.read("ssh-test").unwrap().unwrap(), original);

        let mut replacement = ssh_request(
            "private_key",
            serde_json::json!({
                "clear_ssh_secrets": true, "ssh_identity_file": replacement_identity.clone()
            }),
        );
        replacement.profile.has_password = true;
        let replaced = save_connection_inner(&state, replacement).await.unwrap();
        let stored = store.read("ssh-test").unwrap().unwrap();
        assert!(stored.ssh_private_key.is_none());
        assert!(stored.ssh_passphrase.is_none());
        assert!(stored.ssh_known_hosts_file.is_none());
        assert_eq!(
            stored.ssh_identity_file.as_deref(),
            Some(replacement_identity.as_str())
        );
        assert_eq!(stored.password.as_deref(), Some("redis-password"));

        let mut agent = save_input(replaced, None);
        agent.profile.ssh.as_mut().unwrap().auth_method = SshAuthMethod::Agent;
        let agent = save_connection_inner(&state, agent).await.unwrap();
        assert!(!agent.ssh.as_ref().unwrap().has_identity_file);
        assert!(store
            .read("ssh-test")
            .unwrap()
            .unwrap()
            .ssh_identity_file
            .is_none());

        let mut disabled = save_input(saved, None);
        disabled.profile.ssh = None;
        store
            .write(
                "ssh-test",
                &ConnectionSecrets {
                    password: Some("redis-password".into()),
                    ssh_password: Some("password".into()),
                    ssh_private_key: Some("key".into()),
                    ssh_passphrase: Some("phrase".into()),
                    ssh_identity_file: Some(absolute_test_path("disabled-key")),
                    ssh_known_hosts_file: Some(absolute_test_path("disabled-known-hosts")),
                    ..Default::default()
                },
            )
            .unwrap();
        save_connection_inner(&state, disabled).await.unwrap();
        let stored = store.read("ssh-test").unwrap().unwrap();
        assert_eq!(stored.password.as_deref(), Some("redis-password"));
        assert!(
            stored.ssh_password.is_none()
                && stored.ssh_private_key.is_none()
                && stored.ssh_passphrase.is_none()
                && stored.ssh_identity_file.is_none()
                && stored.ssh_known_hosts_file.is_none()
        );
    }

    #[tokio::test]
    async fn ssh_failed_profile_write_restores_all_previous_secrets() {
        let (state, profiles, secrets) = state_with(vec![], None);
        let known_hosts = absolute_test_path("rollback-known-hosts");
        save_connection_inner(
            &state,
            ssh_request(
                "password",
                serde_json::json!({
                    "ssh_password": "old", "ssh_known_hosts_file": known_hosts
                }),
            ),
        )
        .await
        .unwrap();
        let old_profiles = profiles.load().unwrap();
        let old_secrets = secrets.read("ssh-test").unwrap();
        profiles.fail_next_saves(&[true, false]);
        let result = save_connection_inner(
            &state,
            ssh_request(
                "password",
                serde_json::json!({
                    "clear_ssh_secrets": true, "ssh_password": "new"
                }),
            ),
        )
        .await;
        assert!(matches!(result, Err(AppError::PersistenceFailed)));
        assert_eq!(profiles.load().unwrap(), old_profiles);
        assert_eq!(secrets.read("ssh-test").unwrap(), old_secrets);
    }

    #[test]
    fn ssh_test_ipc_resolves_new_material_and_never_persists_it() {
        use tauri::Manager;
        let (state, profiles, secrets) = state_with(vec![], None);
        let app = tauri::test::mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![save_connection, test_connection])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let invoke = |cmd: &str, input: &SaveConnectionInput| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    // Use Tauri's platform-specific local origin for IPC authorization.
                    url: webview.url().unwrap(),
                    body: serde_json::json!({"input": input}).into(),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_owned(),
                },
            )
        };
        let initial = ssh_request("password", serde_json::json!({"ssh_password": "original"}));
        invoke("save_connection", &initial).unwrap();
        let original_profiles = profiles.load().unwrap();
        let original_secrets = secrets.read("ssh-test").unwrap();

        let cleared = ssh_request("password", serde_json::json!({"clear_ssh_secrets": true}));
        assert_eq!(
            invoke("test_connection", &cleared).unwrap_err()["code"],
            "INVALID_CONNECTION"
        );

        // A local non-SSH peer makes service entry observable without an external SSH daemon.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(AtomicBool::new(false));
        let did_accept = accepted.clone();
        let peer = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            did_accept.store(true, Ordering::SeqCst);
            socket.shutdown(std::net::Shutdown::Both).unwrap();
        });
        let mut replacement = ssh_request(
            "password",
            serde_json::json!({
                "clear_ssh_secrets": true, "ssh_password": "replacement"
            }),
        );
        let ssh = replacement.profile.ssh.as_mut().unwrap();
        ssh.host = "127.0.0.1".into();
        ssh.port = port;
        assert_eq!(
            invoke("test_connection", &replacement).unwrap_err()["code"],
            "SSH_TUNNEL_FAILED"
        );
        peer.join().unwrap();
        assert!(accepted.load(Ordering::SeqCst));
        assert_eq!(profiles.load().unwrap(), original_profiles);
        assert_eq!(secrets.read("ssh-test").unwrap(), original_secrets);
        assert!(
            app.state::<AppState>().profiles.load().unwrap()[0]
                .ssh
                .as_ref()
                .unwrap()
                .has_password
        );
    }

    #[tokio::test]
    async fn ssh_switching_modes_discards_unselected_material_without_clearing_other_credentials() {
        let (state, _, store) = state_with(vec![], None);
        let identity = absolute_test_path("switch-mode-key");
        let mut input = ssh_request(
            "private_key",
            serde_json::json!({"ssh_identity_file": identity}),
        );
        input.profile.has_password = true;
        input.profile.tls = true;
        input.profile.has_ca_certificate = true;
        input.profile.ca_certificate_name = Some("CA".into());
        store
            .write(
                "ssh-test",
                &ConnectionSecrets {
                    password: Some("redis".into()),
                    ca_certificate: Some("CA".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        // Existing Redis/TLS metadata retain their own established save contract.
        state.profiles.save(&[input.profile.clone()]).unwrap();
        let saved = save_connection_inner(&state, input).await.unwrap();
        let mut password = save_input(saved, None);
        password.profile.ssh.as_mut().unwrap().auth_method = SshAuthMethod::Password;
        password.ssh_password = Some("ssh".into());
        let saved = save_connection_inner(&state, password).await.unwrap();
        assert!(!saved.ssh.as_ref().unwrap().has_identity_file);
        let mut agent = save_input(saved, None);
        agent.profile.ssh.as_mut().unwrap().auth_method = SshAuthMethod::Agent;
        agent.ssh_password = Some("ignored".into());
        agent.ssh_identity_file = Some(absolute_test_path("ignored-agent-key"));
        agent.ssh_private_key = Some("ignored".into());
        let saved = save_connection_inner(&state, agent).await.unwrap();
        let ssh = saved.ssh.unwrap();
        assert!(!ssh.has_password && !ssh.has_private_key && !ssh.has_identity_file);
        let retained = store.read("ssh-test").unwrap().unwrap();
        assert_eq!(retained.password.as_deref(), Some("redis"));
        assert_eq!(retained.ca_certificate.as_deref(), Some("CA"));
    }

    #[tokio::test]
    async fn ssh_failed_secret_write_restores_previous_material_and_clear_without_replacement_is_atomic(
    ) {
        let (state, profiles, secrets) = state_with(vec![], None);
        save_connection_inner(
            &state,
            ssh_request("password", serde_json::json!({"ssh_password": "old"})),
        )
        .await
        .unwrap();
        let old_profiles = profiles.load().unwrap();
        let old_secrets = secrets.read("ssh-test").unwrap();
        secrets.fail_next_write_after_mutation();
        assert!(matches!(
            save_connection_inner(
                &state,
                ssh_request("password", serde_json::json!({"ssh_password": "new"}))
            )
            .await,
            Err(AppError::PersistenceFailed)
        ));
        assert_eq!(profiles.load().unwrap(), old_profiles);
        assert_eq!(secrets.read("ssh-test").unwrap(), old_secrets);
        assert!(matches!(
            save_connection_inner(
                &state,
                ssh_request("password", serde_json::json!({"clear_ssh_secrets": true}))
            )
            .await,
            Err(AppError::InvalidConnection)
        ));
        assert_eq!(secrets.read("ssh-test").unwrap(), old_secrets);
    }

    #[tokio::test]
    async fn ssh_retained_secret_paths_are_validated_before_save() {
        let (state, profiles, secrets) = state_with(vec![], None);
        let invalid_identity = format!("{}\tother", absolute_test_path("retained-key"));
        secrets
            .write(
                "ssh-test",
                &ConnectionSecrets {
                    ssh_identity_file: Some(invalid_identity),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(
            save_connection_inner(&state, ssh_request("private_key", serde_json::json!({}))).await,
            Err(AppError::InvalidInput)
        ));
        assert!(profiles.load().unwrap().is_empty());
    }

    #[tokio::test]
    async fn successful_reopen_closes_existing_cli_socket_before_new_commands() {
        let (old_port, old_active) = spawn_cli_redis("old").await;
        let (new_port, _) = spawn_cli_redis("new").await;
        let mut old_profile = profile("local", "Old", false);
        old_profile.port = old_port;
        let (state, profiles, _) = state_with(vec![old_profile], None);
        state.redis.open_connection("local").await.unwrap();
        let session = CliSessionInput {
            connection_id: "local".into(),
            session_id: "00000000-0000-4000-8000-000000000071".into(),
        };
        state.cli.open(&state.redis, session.clone()).await.unwrap();
        wait_for_active_connections(old_active.as_ref(), 1).await;

        let mut new_profile = profile("local", "New", false);
        new_profile.port = new_port;
        profiles.save(&[new_profile]).unwrap();
        open_connection_inner(&state, "local").await.unwrap();

        assert!(matches!(
            state
                .cli
                .execute(CliCommandInput {
                    connection_id: "local".into(),
                    session_id: session.session_id.clone(),
                    command: "GET source".into(),
                })
                .await,
            Err(AppError::ConnectionFailed)
        ));
        wait_for_active_connections(old_active.as_ref(), 0).await;
        state.cli.open(&state.redis, session.clone()).await.unwrap();
        assert_eq!(
            state
                .cli
                .execute(CliCommandInput {
                    connection_id: "local".into(),
                    session_id: session.session_id,
                    command: "GET source".into(),
                })
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            serde_json::json!("new")
        );
    }

    #[tokio::test]
    async fn failed_reopen_preserves_existing_cli_session() {
        let (old_port, old_active) = spawn_cli_redis("old").await;
        let mut old_profile = profile("local", "Old", false);
        old_profile.port = old_port;
        let (state, profiles, _) = state_with(vec![old_profile], None);
        state.redis.open_connection("local").await.unwrap();
        let session = CliSessionInput {
            connection_id: "local".into(),
            session_id: "00000000-0000-4000-8000-000000000072".into(),
        };
        state.cli.open(&state.redis, session.clone()).await.unwrap();
        wait_for_active_connections(old_active.as_ref(), 1).await;
        let unavailable = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let unavailable_port = unavailable.local_addr().unwrap().port();
        drop(unavailable);
        let mut broken = profile("local", "Broken", false);
        broken.port = unavailable_port;
        profiles.save(&[broken]).unwrap();

        assert_eq!(
            open_connection_inner(&state, "local").await,
            Err(AppError::ConnectionFailed)
        );
        assert_eq!(
            state
                .cli
                .execute(CliCommandInput {
                    connection_id: "local".into(),
                    session_id: session.session_id,
                    command: "GET source".into(),
                })
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            serde_json::json!("old")
        );
        assert_eq!(old_active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cli_open_waits_for_reopen_and_uses_the_new_handle() {
        let (old_port, _) = spawn_cli_redis("old").await;
        let (new_port, reopen_entered, resume_reopen) = spawn_paused_cli_redis("new").await;
        let mut old_profile = profile("local", "Old", false);
        old_profile.port = old_port;
        let (state, profiles, _) = state_with(vec![old_profile], None);
        let state = Arc::new(state);
        state.redis.open_connection("local").await.unwrap();
        let mut new_profile = profile("local", "New", false);
        new_profile.port = new_port;
        profiles.save(&[new_profile]).unwrap();
        let reopen_state = Arc::clone(&state);
        let reopen =
            tokio::spawn(async move { open_connection_inner(&reopen_state, "local").await });
        reopen_entered.await.unwrap();

        let session = CliSessionInput {
            connection_id: "local".into(),
            session_id: "00000000-0000-4000-8000-000000000073".into(),
        };
        let cli_state = Arc::clone(&state);
        let cli_session = session.clone();
        let mut cli_open =
            tokio::spawn(async move { cli_state.cli.open(&cli_state.redis, cli_session).await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut cli_open)
                .await
                .is_err()
        );
        resume_reopen.send(()).unwrap();
        reopen.await.unwrap().unwrap();
        cli_open.await.unwrap().unwrap();

        assert_eq!(
            state
                .cli
                .execute(CliCommandInput {
                    connection_id: "local".into(),
                    session_id: session.session_id,
                    command: "GET source".into(),
                })
                .await
                .unwrap()
                .result
                .unwrap()
                .value,
            serde_json::json!("new")
        );
    }

    struct PausedFailingProfiles {
        inner: RecordingProfileRepository,
        gate: Mutex<
            Option<(
                tokio::sync::oneshot::Sender<()>,
                std::sync::mpsc::Receiver<()>,
            )>,
        >,
    }

    impl ProfileRepository for PausedFailingProfiles {
        fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
            self.inner.load()
        }

        fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
            let gate = self.gate.lock().unwrap().take();
            if let Some((entered, resume)) = gate {
                self.inner.save(profiles)?;
                entered.send(()).unwrap();
                resume
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                return Err(AppError::PersistenceFailed);
            }
            self.inner.save(profiles)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_transaction_rollback_cannot_overwrite_a_concurrent_save() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let profiles = Arc::new(PausedFailingProfiles {
            inner: RecordingProfileRepository::new(vec![profile("local", "Original", true)]),
            gate: Mutex::new(Some((entered_tx, resume_rx))),
        });
        let secrets = Arc::new(RecordingSecretStore::new(Some("original")));
        let state = Arc::new(AppState::new(profiles.clone(), secrets.clone()));
        let first_state = Arc::clone(&state);
        let first = tokio::spawn(async move {
            save_connection_inner(
                &first_state,
                save_input(profile("local", "Rejected", true), Some("rejected")),
            )
            .await
        });
        entered_rx.await.unwrap();
        let second_state = Arc::clone(&state);
        let mut second = tokio::spawn(async move {
            save_connection_inner(
                &second_state,
                save_input(profile("local", "Winner", true), Some("winner")),
            )
            .await
        });

        // An unguarded save completes here; a transaction correctly waits for rollback.
        let early_result =
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut second).await;
        resume_tx.send(()).unwrap();
        assert_eq!(first.await.unwrap(), Err(AppError::PersistenceFailed));
        match early_result {
            Ok(result) => {
                result.unwrap().unwrap();
            }
            Err(_) => {
                second.await.unwrap().unwrap();
            }
        }

        assert_eq!(
            profiles.load().unwrap(),
            vec![profile("local", "Winner", true)]
        );
        assert_eq!(secrets.current().as_deref(), Some("winner"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_transaction_delete_rollback_preserves_a_concurrent_import() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let original = profile("local", "Original", false);
        let profiles = Arc::new(PausedFailingProfiles {
            inner: RecordingProfileRepository::new(vec![original.clone()]),
            gate: Mutex::new(Some((entered_tx, resume_rx))),
        });
        let state = Arc::new(AppState::new(
            profiles.clone(),
            Arc::new(RecordingSecretStore::new(None)),
        ));
        let first_state = Arc::clone(&state);
        let first =
            tokio::spawn(async move { delete_connection_inner(&first_state, "local").await });
        entered_rx.await.unwrap();
        let second_state = Arc::clone(&state);
        let mut second = tokio::spawn(async move {
            import_connections_inner(&second_state, ImportConnectionsInput {
                content: serde_json::json!({"connections":[{"name":"Imported","host":"127.0.0.1","port":6379}]}).to_string(),
            }).await
        });

        let early_result =
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut second).await;
        resume_tx.send(()).unwrap();
        assert_eq!(first.await.unwrap(), Err(AppError::PersistenceFailed));
        let imported = match early_result {
            Ok(result) => result.unwrap().unwrap().imported,
            Err(_) => second.await.unwrap().unwrap().imported,
        };
        assert_eq!(
            profiles.load().unwrap(),
            vec![original, imported[0].clone()]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn profile_transaction_import_rolls_back_a_partial_save_failure() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let original = profile("local", "Original", false);
        let profiles = Arc::new(PausedFailingProfiles {
            inner: RecordingProfileRepository::new(vec![original.clone()]),
            gate: Mutex::new(Some((entered_tx, resume_rx))),
        });
        let state = AppState::new(profiles.clone(), Arc::new(RecordingSecretStore::new(None)));
        let import = tokio::spawn(async move {
            import_connections_inner(&state, ImportConnectionsInput {
                content: serde_json::json!({"connections":[{"name":"Imported","host":"127.0.0.1","port":6379}]}).to_string(),
            }).await
        });
        entered_rx.await.unwrap();
        resume_tx.send(()).unwrap();
        assert_eq!(
            import.await.unwrap().unwrap_err(),
            AppError::PersistenceFailed
        );
        assert_eq!(profiles.load().unwrap(), vec![original]);
    }

    #[tokio::test]
    async fn export_connections_omits_password_and_certificate_material() {
        let mut profile = profile("tls", "TLS Redis", true);
        profile.tls = true;
        profile.ca_certificate_name = Some("Root CA".into());
        profile.client_certificate_name = Some("Client cert".into());
        profile.has_ca_certificate = true;
        profile.has_client_certificate = true;
        let (state, _, _) = state_with(vec![profile], Some("password"));

        let document = export_connections_inner(&state).await.unwrap();
        let raw = serde_json::to_string(&document).unwrap();

        assert_eq!(document.version, 2);
        assert!(!raw.contains("password"));
        assert!(!raw.contains("BEGIN CERTIFICATE"));
        assert!(!raw.contains("BEGIN PRIVATE KEY"));
    }

    #[tokio::test]
    async fn import_appends_valid_entries_with_fresh_ids_and_reports_invalid_entries() {
        let (state, profiles, secrets) =
            state_with(vec![profile("existing", "Existing", false)], None);
        let result = import_connections_inner(
            &state,
            ImportConnectionsInput {
                content: serde_json::json!({
                    "connections": [
                        {
                            "id": "existing",
                            "name": "Imported",
                            "host": "127.0.0.1",
                            "port": 6379
                        },
                        {"name": "Broken", "host": "", "port": 0},
                        {
                            "name": "With secret",
                            "host": "127.0.0.1",
                            "port": 6379,
                            "password": "do-not-store"
                        }
                    ]
                })
                .to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result.imported.len(), 2);
        assert_ne!(result.imported[0].id, "existing");
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].name.as_deref(), Some("Broken"));
        assert_eq!(result.ignored_secret_fields, 1);
        assert_eq!(profiles.current().len(), 3);
        assert!(secrets
            .operations()
            .iter()
            .all(|operation| !matches!(operation, SecretOperation::Write(_))));
        assert!(result.imported.iter().all(|profile| !profile.has_password));
    }

    #[tokio::test]
    async fn import_returns_persistence_error_without_partial_profile_save() {
        let (state, profiles, _) = state_with(vec![profile("existing", "Existing", false)], None);
        profiles.fail_next_saves(&[true]);

        let error = import_connections_inner(
            &state,
            ImportConnectionsInput {
                content: serde_json::json!({
                    "connections": [{
                        "name": "Imported",
                        "host": "127.0.0.1",
                        "port": 6379
                    }]
                })
                .to_string(),
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.current().len(), 1);
    }

    #[tokio::test]
    async fn save_connection_rejects_invalid_profile_before_persistence() {
        let (state, profiles, secrets) = state_with(Vec::new(), None);
        let mut invalid = profile("", "Invalid", false);
        invalid.port = 0;

        let error = save_connection_inner(&state, save_input(invalid, Some("secret")))
            .await
            .unwrap_err();

        assert_eq!(error, AppError::InvalidConnection);
        assert!(profiles.save_attempts().is_empty());
        assert_eq!(secrets.operations(), Vec::new());
    }

    #[tokio::test]
    async fn save_connection_adds_password_without_serializing_secret() {
        let (state, profiles, secrets) = state_with(Vec::new(), None);

        let saved = save_connection_inner(
            &state,
            save_input(profile("local", "Local", false), Some("secret")),
        )
        .await
        .unwrap();

        assert!(saved.has_password);
        assert_eq!(profiles.current(), vec![saved.clone()]);
        assert_eq!(secrets.current().as_deref(), Some("secret"));
        let serialized = serde_json::to_string(&saved).unwrap();
        assert!(!serialized.contains("\"password\""));
        assert!(!serialized.contains("secret"));
    }

    #[tokio::test]
    async fn save_connection_updates_profile_and_replaces_password() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old], Some("old-secret"));

        let saved = save_connection_inner(
            &state,
            save_input(profile("local", "Updated", true), Some("new-secret")),
        )
        .await
        .unwrap();

        assert_eq!(saved.name, "Updated");
        assert!(saved.has_password);
        assert_eq!(profiles.current(), vec![saved]);
        assert_eq!(secrets.current().as_deref(), Some("new-secret"));
    }

    #[tokio::test]
    async fn save_connection_without_password_preserves_existing_secret() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old], Some("old-secret"));

        let saved =
            save_connection_inner(&state, save_input(profile("local", "Updated", true), None))
                .await
                .unwrap();

        assert!(saved.has_password);
        assert_eq!(profiles.current(), vec![saved]);
        assert_eq!(secrets.current().as_deref(), Some("old-secret"));
        assert!(!secrets
            .operations()
            .iter()
            .any(|operation| matches!(operation, SecretOperation::Delete)));
    }

    #[tokio::test]
    async fn sentinel_secrets_remain_in_secret_store_and_can_be_cleared_without_touching_redis_password(
    ) {
        let (state, profiles, secrets) = state_with(Vec::new(), None);
        let mut input = save_input(profile("sentinel", "Sentinel", false), Some("redis-only"));
        input.profile.sentinel = Some(crate::domain::SentinelConfig {
            master_name: "primary".into(),
            nodes: vec![crate::domain::ConnectionEndpoint {
                host: "127.0.0.1".into(),
                port: 26379,
            }],
            username: Some("watcher".into()),
            has_password: false,
            tls: false,
        });
        input.sentinel_password = Some("sentinel-only".into());
        let saved = save_connection_inner(&state, input).await.unwrap();
        assert!(saved.sentinel.as_ref().unwrap().has_password);
        let secret = secrets.read("sentinel").unwrap().unwrap();
        assert_eq!(secret.password.as_deref(), Some("redis-only"));
        assert_eq!(secret.sentinel_password.as_deref(), Some("sentinel-only"));
        let exported =
            serde_json::to_string(&export_connections_inner(&state).await.unwrap()).unwrap();
        assert!(!exported.contains("password"));
        assert!(!serde_json::to_string(&profiles.current())
            .unwrap()
            .contains("sentinel-only"));
        let retained = save_connection_inner(&state, save_input(saved, None))
            .await
            .unwrap();
        assert_eq!(secrets.read("sentinel").unwrap().unwrap(), secret);
        let mut cleared = retained;
        cleared.sentinel.as_mut().unwrap().has_password = false;
        save_connection_inner(&state, save_input(cleared, None))
            .await
            .unwrap();
        let secret = secrets.read("sentinel").unwrap().unwrap();
        assert_eq!(secret.password.as_deref(), Some("redis-only"));
        assert_eq!(secret.sentinel_password, None);
    }

    #[tokio::test]
    async fn save_connection_without_password_clears_existing_secret() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old], Some("old-secret"));

        let saved =
            save_connection_inner(&state, save_input(profile("local", "Updated", false), None))
                .await
                .unwrap();

        assert!(!saved.has_password);
        assert_eq!(profiles.current(), vec![saved]);
        assert_eq!(secrets.current(), None);
        assert!(secrets
            .operations()
            .iter()
            .any(|operation| matches!(operation, SecretOperation::Delete)));
    }

    #[tokio::test]
    async fn save_connection_rejects_new_profile_claiming_an_existing_password() {
        let (state, profiles, secrets) = state_with(Vec::new(), None);

        let error = save_connection_inner(&state, save_input(profile("new", "New", true), None))
            .await
            .unwrap_err();

        assert_eq!(error, AppError::InvalidConnection);
        assert!(profiles.current().is_empty());
        assert_eq!(secrets.current(), None);
    }

    #[tokio::test]
    async fn save_connection_rejects_existing_password_flag_without_old_secret() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old.clone()], None);

        let error = save_connection_inner(&state, save_input(old, None))
            .await
            .unwrap_err();

        assert_eq!(error, AppError::InvalidConnection);
        assert_eq!(profiles.current(), vec![profile("local", "Old", true)]);
        assert_eq!(secrets.current(), None);
    }

    #[tokio::test]
    async fn save_connection_restores_state_when_secret_write_fails_after_mutation() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old.clone()], Some("old-secret"));
        secrets.fail_next_write_after_mutation();

        let error = save_connection_inner(
            &state,
            save_input(profile("local", "Updated", true), Some("new-secret")),
        )
        .await
        .unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.current(), vec![old]);
        assert_eq!(secrets.current().as_deref(), Some("old-secret"));
        assert!(secrets.operations().iter().any(
            |operation| matches!(operation, SecretOperation::Write(value) if value == "old-secret")
        ));
    }

    #[tokio::test]
    async fn save_connection_restores_state_when_secret_delete_fails_after_mutation() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old.clone()], Some("old-secret"));
        secrets.fail_next_delete_after_mutation();

        let error =
            save_connection_inner(&state, save_input(profile("local", "Updated", false), None))
                .await
                .unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.current(), vec![old]);
        assert_eq!(secrets.current().as_deref(), Some("old-secret"));
        assert!(secrets.operations().iter().any(
            |operation| matches!(operation, SecretOperation::Write(value) if value == "old-secret")
        ));
    }

    #[tokio::test]
    async fn save_connection_restores_state_when_profile_save_fails() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old.clone()], Some("old-secret"));
        profiles.fail_next_saves(&[true, false]);

        let error = save_connection_inner(
            &state,
            save_input(profile("local", "Updated", true), Some("new-secret")),
        )
        .await
        .unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.current(), vec![old]);
        assert_eq!(secrets.current().as_deref(), Some("old-secret"));
        assert_eq!(profiles.save_attempts().len(), 2);
    }

    #[tokio::test]
    async fn save_connection_returns_persistence_error_when_recovery_fails() {
        let old = profile("local", "Old", true);
        let (state, profiles, secrets) = state_with(vec![old], Some("old-secret"));
        profiles.fail_next_saves(&[true, true]);

        let error = save_connection_inner(
            &state,
            save_input(profile("local", "Updated", true), Some("new-secret")),
        )
        .await
        .unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.save_attempts().len(), 2);
        assert_eq!(secrets.current().as_deref(), Some("old-secret"));
    }

    #[tokio::test]
    async fn delete_connection_rolls_back_when_secret_delete_fails_after_mutation() {
        let old = profile("local", "Local", true);
        let (state, profiles, secrets) = state_with(vec![old.clone()], Some("secret"));
        secrets.fail_next_delete_after_mutation();

        let error = delete_connection_inner(&state, "local").await.unwrap_err();

        assert_eq!(error, AppError::PersistenceFailed);
        assert_eq!(profiles.current(), vec![old]);
        assert_eq!(secrets.current().as_deref(), Some("secret"));
        assert_eq!(profiles.save_attempts().len(), 2);
        assert!(secrets.operations().iter().any(
            |operation| matches!(operation, SecretOperation::Write(value) if value == "secret")
        ));
    }

    #[tokio::test]
    async fn delete_connection_removes_profile_and_secret_before_closing() {
        let old = profile("local", "Local", true);
        let (state, profiles, secrets) = state_with(vec![old], Some("secret"));

        delete_connection_inner(&state, "local").await.unwrap();

        assert!(profiles.current().is_empty());
        assert_eq!(secrets.current(), None);
    }
}
