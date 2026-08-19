use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppError;

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
        let raw = match fs::read_to_string(&self.path) {
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
        self.ensure_parent_directory()?;
        let serialized =
            serde_json::to_vec_pretty(document).map_err(|_| AppError::PersistenceFailed)?;
        let temporary_path = self.temporary_path();

        let write_result = (|| -> Result<(), AppError> {
            let mut temporary_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|_| AppError::PersistenceFailed)?;
            temporary_file
                .write_all(&serialized)
                .map_err(|_| AppError::PersistenceFailed)?;
            temporary_file
                .sync_all()
                .map_err(|_| AppError::PersistenceFailed)?;
            drop(temporary_file);
            replace_target(&temporary_path, &self.path).map_err(|_| AppError::PersistenceFailed)
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }

        write_result
    }

    fn read_raw(&self) -> Result<Option<String>, AppError> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => Ok(Some(raw)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(AppError::PersistenceFailed),
        }
    }

    fn temporary_path(&self) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.json");

        self.path
            .with_file_name(format!(".{file_name}.{timestamp}.{sequence}.tmp"))
    }

    fn ensure_parent_directory(&self) -> Result<(), AppError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|_| AppError::PersistenceFailed)?;
        }

        Ok(())
    }
}

fn replace_target(temporary_path: &Path, target_path: &Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::rename(temporary_path, target_path)
    }

    #[cfg(windows)]
    {
        replace_target_windows(temporary_path, target_path)
    }
}

#[cfg(windows)]
fn replace_target_windows(temporary_path: &Path, target_path: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH,
    };

    let temporary_path_wide = temporary_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_path_wide = target_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_exists = match fs::metadata(target_path) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    let succeeded = unsafe {
        if target_exists {
            ReplaceFileW(
                target_path_wide.as_ptr(),
                temporary_path_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            MoveFileExW(
                temporary_path_wide.as_ptr(),
                target_path_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };

    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
