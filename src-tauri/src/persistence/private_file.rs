use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Write},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

use crate::error::AppError;

/// Restrict only the application-owned directory, never an arbitrary store's parent.
pub fn prepare_sensitive_storage(data_dir: &Path) -> Result<(), AppError> {
    let prepare = || -> io::Result<()> {
        create_parent(data_dir)?;
        restrict_permissions(data_dir, true)?;
        for name in [
            "workbench-history.json",
            "analysis-history.json",
            "connections.json",
            "query-library.json",
            "connection-tags.json",
            "settings.json",
        ] {
            prepare_file(&data_dir.join(name))?;
        }
        Ok(())
    };
    prepare().map_err(|_| AppError::PersistenceFailed)
}

fn create_parent(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)
}

fn restrict_permissions(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular private file or directory",
        ));
    }
    #[cfg(unix)]
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )?;
    // macOS allow ACLs can grant access even when the POSIX mode is 0600/0700.
    #[cfg(target_os = "macos")]
    super::macos_permissions::clear_acl(path)?;
    #[cfg(windows)]
    super::windows_permissions::restrict_permissions(path)?;
    Ok(())
}

fn prepare_file(path: &Path) -> io::Result<()> {
    match restrict_permissions(path, false) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        result => result?,
    }
    cleanup_abandoned_files(path)
}

pub(super) fn open_private_file(path: &Path) -> io::Result<File> {
    prepare_file(path)?;
    File::open(path)
}

pub(super) fn read_private_file(path: &Path) -> io::Result<String> {
    let mut raw = String::new();
    open_private_file(path)?.read_to_string(&mut raw)?;
    Ok(raw)
}

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    create_parent(parent)?;
    prepare_file(path)?;
    let mut temporary = create_private_temporary(path, parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    // Keep the lock and RAII cleanup alive until replacement succeeds, including on unwind.
    // std::fs::rename supports replacing an open destination on Windows via POSIX semantics;
    // tempfile::persist only uses MoveFileExW, which fails while a cleanup reader holds it open.
    fs::rename(temporary.path(), path)?;
    Ok(())
}

fn create_private_temporary(path: &Path, parent: &Path) -> io::Result<tempfile::NamedTempFile> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?
        .to_string_lossy();
    let mut builder = tempfile::Builder::new();
    let prefix = format!(".{name}.");
    builder.prefix(&prefix).suffix(".tmp").rand_bytes(16);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    // These files become permanent through rename, so do not set FILE_ATTRIBUTE_TEMPORARY.
    // make_in still supplies random names and removes the file on failure or unwind.
    let temporary = builder.make_in(parent, |path| options.open(path))?;
    // Apply Windows DACLs while the file is still empty. Unix mode is set at creation.
    restrict_permissions(temporary.path(), false)?;
    // Cleanup skips empty files; lock before the first byte to avoid racing another instance.
    temporary.as_file().lock()?;
    Ok(temporary)
}

fn is_temporary_name(name: &str, target: &str) -> bool {
    let Some(suffix) = name
        .strip_prefix(&format!(".{target}."))
        .and_then(|name| name.strip_suffix(".tmp"))
    else {
        return false;
    };
    // Current random names and the exact timestamp/sequence format of older releases.
    if suffix.len() == 16 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return true;
    }
    let Some((timestamp, sequence)) = suffix.split_once('.') else {
        return false;
    };
    !timestamp.is_empty()
        && !sequence.is_empty()
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

fn cleanup_abandoned_files(path: &Path) -> io::Result<()> {
    let Some(target) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !is_temporary_name(&entry.file_name().to_string_lossy(), target) {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_file() => (),
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
        let temporary_path = entry.path();
        // Another instance may have renamed or removed this entry since read_dir.
        // Never let a disappearing temporary file masquerade as a missing JSON document.
        match restrict_permissions(&temporary_path, false) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        // An empty file can be a writer that has not acquired its lock yet; it has no secrets.
        if file.metadata()?.len() == 0 {
            continue;
        }
        match file.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => continue,
            Err(TryLockError::Error(error)) => return Err(error),
        }
        match fs::remove_file(&temporary_path) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_replacement_removes_its_private_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workbench-history.json");
        {
            let mut temporary = create_private_temporary(&path, directory.path()).unwrap();
            temporary.write_all(b"private command").unwrap();
            fs::create_dir(&path).unwrap();
            fs::write(path.join("marker"), b"keep").unwrap();
            assert!(fs::rename(temporary.path(), &path).is_err());
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        assert_eq!(fs::read(path.join("marker")).unwrap(), b"keep");
    }

    #[test]
    fn an_unwinding_writer_removes_its_private_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workbench-history.json");
        let result = std::panic::catch_unwind(|| {
            let mut temporary = create_private_temporary(&path, directory.path()).unwrap();
            temporary.write_all(b"private command").unwrap();
            panic!("simulate an interrupted writer");
        });
        assert!(result.is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn abrupt_exit_writer() {
        let Some(directory) = std::env::var_os("REDIX_PRIVATE_TEMP_CRASH_DIR") else {
            return;
        };
        let directory = Path::new(&directory);
        let mut temporary =
            create_private_temporary(&directory.join("workbench-history.json"), directory).unwrap();
        temporary.write_all(b"private command").unwrap();
        temporary.as_file().sync_all().unwrap();
        // Simulate process death after writing, before rename: no destructors run.
        std::process::exit(0);
    }

    #[test]
    fn an_abruptly_exited_writer_leaves_only_private_recoverable_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::private_file::tests::abrupt_exit_writer",
            ])
            .env("REDIX_PRIVATE_TEMP_CRASH_DIR", directory.path())
            .status()
            .unwrap();
        assert!(status.success());
        let entries: Vec<_> = fs::read_dir(directory.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let temporary = entries[0].as_ref().unwrap().path();
        assert_eq!(fs::read(&temporary).unwrap(), b"private command");
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&temporary).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            open_private_file(&directory.path().join("workbench-history.json"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
