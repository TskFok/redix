use std::{
    ffi::{c_char, c_int, c_void, CString},
    io,
    os::unix::ffi::OsStrExt,
    path::Path,
};

// macOS SDK <sys/acl.h>: acl_type_t is a C enum and ACL_TYPE_EXTENDED is 0x100.
const ACL_TYPE_EXTENDED: c_int = 0x0000_0100;

unsafe extern "C" {
    fn acl_init(count: c_int) -> *mut c_void;
    fn acl_set_file(path: *const c_char, acl_type: c_int, acl: *mut c_void) -> c_int;
    fn acl_free(acl: *mut c_void) -> c_int;
}

/// POSIX modes do not remove macOS extended ACL grants or inheritance rules.
pub(super) fn clear_acl(path: &Path) -> io::Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))?;

    // SAFETY: zero requests a valid empty ACL, which is released below after its only use.
    let acl = unsafe { acl_init(0) };
    if acl.is_null() {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: path is NUL-terminated and acl is a live allocation from acl_init.
    let status = unsafe { acl_set_file(path.as_ptr(), ACL_TYPE_EXTENDED, acl) };
    // Capture errno before freeing the ACL, since acl_free can change it.
    let result = if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    };

    // SAFETY: acl was allocated by acl_init, has not been freed, and is no longer used.
    let freed = unsafe { acl_free(acl) };
    result?;
    if freed != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_the_native_error() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            clear_acl(&directory.path().join("missing.json"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn nul_in_path_is_rejected() {
        assert_eq!(
            clear_acl(Path::new("invalid\0path")).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
