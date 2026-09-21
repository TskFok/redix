use std::path::PathBuf;

use super::private_file::{read_private_file, write_private_file};

use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppError;

pub trait VersionedJsonDocument: Serialize + DeserializeOwned + Default + PartialEq {
    fn version(&self) -> u32;
    fn migrate(value: serde_json::Value) -> Result<Self, AppError>;
}

pub struct JsonDocumentStore {
    pub path: PathBuf,
}

impl JsonDocumentStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load<T: VersionedJsonDocument>(&self) -> Result<T, AppError> {
        let Some(raw) = self.read_raw()? else {
            return Ok(T::default());
        };
        let value = serde_json::from_str::<serde_json::Value>(&raw)
            .map_err(|_| AppError::PersistenceFailed)?;
        T::migrate(value)
    }

    pub fn load_or_default<T: VersionedJsonDocument>(&self) -> Result<T, AppError> {
        let raw = match read_private_file(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            Err(_) => return Err(AppError::PersistenceFailed),
        };

        let value = match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(value) => value,
            Err(_) => return Ok(T::default()),
        };

        Ok(T::migrate(value).unwrap_or_default())
    }

    pub fn save<T: VersionedJsonDocument>(&self, document: &T) -> Result<(), AppError> {
        let serialized =
            serde_json::to_vec_pretty(document).map_err(|_| AppError::PersistenceFailed)?;
        write_private_file(&self.path, &serialized).map_err(|_| AppError::PersistenceFailed)
    }

    fn read_raw(&self) -> Result<Option<String>, AppError> {
        match read_private_file(&self.path) {
            Ok(raw) => Ok(Some(raw)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }
}
