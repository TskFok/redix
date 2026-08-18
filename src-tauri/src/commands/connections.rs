use crate::{
    domain::{ConnectionInfo, ConnectionProfile, SaveConnectionInput, TestConnectionInput},
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn list_connections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConnectionProfile>, AppError> {
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
    let mut profile = input.profile;
    profile.validate()?;

    let connection_id = profile.id.clone();
    let old_profiles = state.profiles.load()?;
    let old_profile = old_profiles.iter().find(|item| item.id == connection_id);
    let old_secret = state.secrets.read(&connection_id)?;
    let desired_secret =
        resolve_secret(&profile, input.password, old_profile, old_secret.as_ref())?;
    profile.has_password = desired_secret.is_some();

    let mut profiles = old_profiles.clone();
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }

    if desired_secret != old_secret {
        let result = match desired_secret.as_deref() {
            Some(password) => state.secrets.write(&connection_id, password),
            None => state.secrets.delete(&connection_id),
        };
        if result.is_err() {
            restore_profile_and_secret(state, &old_profiles, &connection_id, old_secret.as_deref())
                .map_err(|_| AppError::PersistenceFailed)?;
            return Err(AppError::PersistenceFailed);
        }
    }

    if state.profiles.save(&profiles).is_err() {
        restore_profile_and_secret(state, &old_profiles, &connection_id, old_secret.as_deref())
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
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_deref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(AppError::PersistenceFailed);
    }

    if state.secrets.delete(connection_id).is_err() {
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_deref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(AppError::PersistenceFailed);
    }

    if let Err(error) = state.redis.close_connection(connection_id).await {
        restore_profile_and_secret(state, &old_profiles, connection_id, old_secret.as_deref())
            .map_err(|_| AppError::PersistenceFailed)?;
        return Err(error);
    }

    Ok(())
}

fn resolve_secret(
    profile: &ConnectionProfile,
    password: Option<String>,
    old_profile: Option<&ConnectionProfile>,
    old_secret: Option<&String>,
) -> Result<Option<String>, AppError> {
    if let Some(password) = password {
        return Ok(Some(password));
    }

    if !profile.has_password {
        return Ok(None);
    }

    match (old_profile, old_secret) {
        (Some(old_profile), Some(old_secret)) if old_profile.has_password => {
            Ok(Some(old_secret.clone()))
        }
        _ => Err(AppError::InvalidConnection),
    }
}

fn restore_profile_and_secret(
    state: &AppState,
    profiles: &[ConnectionProfile],
    connection_id: &str,
    secret: Option<&str>,
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
    state
        .redis
        .test_connection(&input.profile, input.password.as_deref())
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionInfo, AppError> {
    state.redis.open_connection(&connection_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn close_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    state.redis.close_connection(&connection_id).await
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
    use crate::persistence::{ProfileRepository, SecretStore};

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
        secret: Mutex<Option<String>>,
        write_failures: Mutex<VecDeque<bool>>,
        delete_failures: Mutex<VecDeque<bool>>,
        mutate_failed_write: AtomicBool,
        mutate_failed_delete: AtomicBool,
        operations: Mutex<Vec<SecretOperation>>,
    }

    impl RecordingSecretStore {
        fn new(secret: Option<&str>) -> Self {
            Self {
                secret: Mutex::new(secret.map(str::to_owned)),
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
            self.secret.lock().unwrap().clone()
        }

        fn operations(&self) -> Vec<SecretOperation> {
            self.operations.lock().unwrap().clone()
        }
    }

    impl SecretStore for RecordingSecretStore {
        fn read(&self, _connection_id: &str) -> Result<Option<String>, AppError> {
            self.operations.lock().unwrap().push(SecretOperation::Read);
            Ok(self.current())
        }

        fn write(&self, _connection_id: &str, password: &str) -> Result<(), AppError> {
            self.operations
                .lock()
                .unwrap()
                .push(SecretOperation::Write(password.to_owned()));
            let failed = self
                .write_failures
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(false);
            if failed {
                if self.mutate_failed_write.load(Ordering::SeqCst) {
                    *self.secret.lock().unwrap() = Some(password.to_owned());
                }
                return Err(AppError::PersistenceFailed);
            }
            *self.secret.lock().unwrap() = Some(password.to_owned());
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
            id: id.into(),
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            database: 0,
            has_password,
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
            profile,
            password: password.map(str::to_owned),
        }
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
