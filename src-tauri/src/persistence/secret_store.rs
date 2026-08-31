use crate::error::AppError;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionSecrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentinel_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_certificate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
}

impl ConnectionSecrets {
    pub fn is_empty(&self) -> bool {
        self.password.is_none()
            && self.sentinel_password.is_none()
            && self.ca_certificate.is_none()
            && self.client_certificate.is_none()
            && self.client_key.is_none()
    }

    pub fn has_client_certificate(&self) -> bool {
        self.client_certificate.is_some() && self.client_key.is_some()
    }
}

pub trait SecretStore: Send + Sync {
    fn read(&self, connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError>;
    fn write(&self, connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError>;
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
    fn read(&self, connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        match self.entry(connection_id)?.get_password() {
            Ok(value) => Ok(Some(decode_stored_secret(&value)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }

    fn write(&self, connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
        if secrets.is_empty() {
            return self.delete(connection_id);
        }

        let value = serde_json::to_string(secrets).map_err(|_| AppError::PersistenceFailed)?;
        self.entry(connection_id)?
            .set_password(&value)
            .map_err(|_| AppError::PersistenceFailed)
    }

    fn delete(&self, connection_id: &str) -> Result<(), AppError> {
        match self.entry(connection_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }
}

fn decode_stored_secret(value: &str) -> Result<ConnectionSecrets, AppError> {
    let parsed = serde_json::from_str::<serde_json::Value>(value).ok();
    if let Some(object) = parsed.as_ref().and_then(serde_json::Value::as_object) {
        let known_keys = [
            "sentinel_password",
            "password",
            "ca_certificate",
            "client_certificate",
            "client_key",
        ];
        if object.keys().any(|key| known_keys.contains(&key.as_str())) {
            return serde_json::from_value(parsed.expect("parsed value must exist"))
                .map_err(|_| AppError::PersistenceFailed);
        }
    }

    Ok(ConnectionSecrets {
        password: Some(value.to_owned()),
        ..ConnectionSecrets::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_secret_store_accepts_legacy_raw_password() {
        let secrets = decode_stored_secret("old-password").unwrap();
        assert_eq!(secrets.password.as_deref(), Some("old-password"));
        assert!(secrets.ca_certificate.is_none());
    }
}
