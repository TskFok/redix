use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use redix_lib::{
    commands::save_connection_inner,
    domain::{ConnectionProfile, SaveConnectionInput},
    error::AppError,
    persistence::{ProfileRepository, SecretStore},
    AppState,
};

struct EmptyProfileRepository;

impl ProfileRepository for EmptyProfileRepository {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(Vec::new())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct RecordingSecretStore {
    write_called: AtomicBool,
}

impl RecordingSecretStore {
    fn new() -> Self {
        Self {
            write_called: AtomicBool::new(false),
        }
    }

    fn was_write_called(&self) -> bool {
        self.write_called.load(Ordering::SeqCst)
    }
}

impl SecretStore for RecordingSecretStore {
    fn read(&self, _connection_id: &str) -> Result<Option<String>, AppError> {
        Ok(None)
    }

    fn write(&self, _connection_id: &str, _password: &str) -> Result<(), AppError> {
        self.write_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
        Ok(())
    }
}

fn invalid_profile() -> ConnectionProfile {
    ConnectionProfile {
        id: String::new(),
        name: "Invalid".into(),
        host: "127.0.0.1".into(),
        port: 6379,
        username: None,
        database: 0,
        has_password: false,
    }
}

fn test_app_state() -> (AppState, Arc<RecordingSecretStore>) {
    let secret_store = Arc::new(RecordingSecretStore::new());
    let state = AppState::new(Arc::new(EmptyProfileRepository), secret_store.clone());
    (state, secret_store)
}

#[tokio::test]
async fn save_connection_rejects_invalid_profile_before_persistence() {
    let (state, secret_store) = test_app_state();
    let input = SaveConnectionInput {
        profile: invalid_profile(),
        password: Some("must-not-be-written".into()),
    };

    let error = save_connection_inner(&state, input).await.unwrap_err();

    assert_eq!(error.code(), "INVALID_CONNECTION");
    assert!(!secret_store.was_write_called());
}
