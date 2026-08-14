use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use serde::{Deserialize, Serialize};

use crate::{domain::ConnectionProfile, error::AppError};

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub trait ProfileRepository: Send + Sync {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError>;
    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError>;
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
            .unwrap_or("connections.json");

        self.path
            .with_file_name(format!(".{file_name}.{timestamp}.{sequence}.tmp"))
    }

    fn ensure_parent_directory(path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path
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

impl ProfileRepository for JsonProfileRepository {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        let raw = match fs::read_to_string(&self.path) {
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
        Self::ensure_parent_directory(&self.path)?;
        let document = ProfileDocument {
            profiles: profiles.to_vec(),
        };
        let serialized =
            serde_json::to_vec_pretty(&document).map_err(|_| AppError::PersistenceFailed)?;
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
}
