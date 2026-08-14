use crate::error::AppError;

pub trait SecretStore: Send + Sync {
    fn read(&self, connection_id: &str) -> Result<Option<String>, AppError>;
    fn write(&self, connection_id: &str, password: &str) -> Result<(), AppError>;
    fn delete(&self, connection_id: &str) -> Result<(), AppError>;
}

pub struct SystemKeyring {
    pub service: String,
}

impl SystemKeyring {
    pub fn new() -> Self {
        Self {
            service: "redix".to_owned(),
        }
    }

    pub fn account_for(connection_id: &str) -> String {
        format!("redix/{connection_id}")
    }

    fn entry(&self, connection_id: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(&self.service, &Self::account_for(connection_id))
            .map_err(|_| AppError::PersistenceFailed)
    }
}

impl Default for SystemKeyring {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for SystemKeyring {
    fn read(&self, connection_id: &str) -> Result<Option<String>, AppError> {
        match self.entry(connection_id)?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }

    fn write(&self, connection_id: &str, password: &str) -> Result<(), AppError> {
        self.entry(connection_id)?
            .set_password(password)
            .map_err(|_| AppError::PersistenceFailed)
    }

    fn delete(&self, connection_id: &str) -> Result<(), AppError> {
        match self.entry(connection_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }
}
