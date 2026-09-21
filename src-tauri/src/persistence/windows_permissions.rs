use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
};

/// Replaces inherited and explicit access rules with full control for the owner.
/// OWNER RIGHTS follows the object's existing owner without granting an additional
/// user or administrator group access. OI/CI also protect newly created children.
pub(super) fn restrict_permissions(path: &Path) -> io::Result<()> {
    apply_dacl(path, "D:P(A;OICI;FA;;;OW)")
}

fn apply_dacl(path: &Path, sddl: &str) -> io::Result<()> {
    let mut wide_path = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide_path.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "文件路径不能包含 NUL 字符",
        ));
    }
    wide_path.push(0);
    let wide_sddl = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut raw_descriptor = ptr::null_mut();
    // SAFETY: The terminated SDDL and output pointer remain valid for the call.
    // On success Windows allocates a descriptor released by LocalFree below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut raw_descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let descriptor = LocalAllocation(raw_descriptor);
    let mut dacl = ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    // SAFETY: The descriptor guard retains ownership and the output values are live.
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    if present == 0 || dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "文件权限描述符缺少限制访问的 DACL",
        ));
    }

    // SAFETY: The path is terminated and dacl belongs to the live descriptor.
    // PROTECTED is explicit: otherwise broad permissions can be inherited again.
    // Passing no owner/group leaves the object's ownership unchanged.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: This guard owns a single LocalAlloc-family allocation returned
        // by the Windows security APIs. No borrowed pointer outlives the guard.
        unsafe { LocalFree(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Security::Authorization::GetNamedSecurityInfoW;
    use windows_sys::Win32::Security::{
        GetAce, GetSecurityDescriptorControl, IsWellKnownSid, WinCreatorOwnerRightsSid,
        WinWorldSid, ACCESS_ALLOWED_ACE, ACE_HEADER, CONTAINER_INHERIT_ACE, INHERITED_ACE,
        OBJECT_INHERIT_ACE, SE_DACL_PROTECTED,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;

    fn assert_owner_only(path: &Path, protected: bool, inherited: bool, directory: bool) {
        let wide_path = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut raw_descriptor = std::ptr::null_mut();
        let mut dacl = std::ptr::null_mut();
        // SAFETY: The path is terminated and all out parameters point to live variables.
        let status = unsafe {
            GetNamedSecurityInfoW(
                wide_path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut raw_descriptor,
            )
        };
        assert_eq!(status, 0, "读取实际文件安全描述符失败：{status}");
        let descriptor = LocalAllocation(raw_descriptor);
        assert!(!dacl.is_null(), "NULL DACL 会向所有用户开放访问");

        let mut control = 0;
        let mut revision = 0;
        // SAFETY: The descriptor remains owned by the guard for this whole assertion.
        assert_ne!(
            unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) },
            0
        );
        assert_eq!(control & SE_DACL_PROTECTED != 0, protected);

        // Inspect the raw ACL so inherited ACEs on newly created children are
        // included, rather than querying only explicit access entries.
        // SAFETY: dacl is non-null and belongs to the live descriptor.
        assert_eq!(unsafe { (*dacl).AceCount }, 1, "DACL 必须只允许所有者访问");
        let mut raw_ace = std::ptr::null_mut();
        // SAFETY: The ACL contains one ACE and the output pointer is live.
        assert_ne!(unsafe { GetAce(dacl, 0, &mut raw_ace) }, 0);
        assert!(!raw_ace.is_null());
        // SAFETY: Every ACE starts with ACE_HEADER and is retained by descriptor.
        let header = unsafe { &*raw_ace.cast::<ACE_HEADER>() };
        assert_eq!(u32::from(header.AceType), ACCESS_ALLOWED_ACE_TYPE);
        assert!(usize::from(header.AceSize) >= std::mem::size_of::<ACCESS_ALLOWED_ACE>());
        let ace = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
        // SAFETY: The type and minimum size were checked above.
        assert_eq!(unsafe { (*ace).Mask }, FILE_ALL_ACCESS);
        // SAFETY: SidStart is the start of the variable-length SID in this ACE.
        // Keep its pointer tied to the whole allocation, not a copied DWORD.
        let sid = unsafe { std::ptr::addr_of!((*ace).SidStart) }
            .cast_mut()
            .cast();
        // SAFETY: The SID belongs to the valid ACE in the live descriptor.
        assert_ne!(unsafe { IsWellKnownSid(sid, WinCreatorOwnerRightsSid) }, 0);
        assert_eq!(unsafe { IsWellKnownSid(sid, WinWorldSid) }, 0);
        let inheritance = u32::from(header.AceFlags);
        assert_eq!(inheritance & INHERITED_ACE != 0, inherited);
        if directory {
            assert_eq!(
                inheritance & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE),
                OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
            );
        }
    }

    #[test]
    fn removes_everyone_access_from_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("profiles.json");
        fs::write(&path, b"{}").unwrap();
        apply_dacl(&path, "D:P(A;;FA;;;WD)").unwrap();

        restrict_permissions(&path).unwrap();

        assert_owner_only(&path, true, false, false);
        fs::write(&path, b"{\"private\":true}").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"private\":true}");
        restrict_permissions(&path).unwrap();
        assert_owner_only(&path, true, false, false);
    }

    #[test]
    fn blocks_broad_permissions_inherited_from_the_parent() {
        let temp = tempfile::tempdir().unwrap();
        apply_dacl(temp.path(), "D:P(A;OICI;FA;;;WD)").unwrap();
        let path = temp.path().join("profiles.json");
        fs::File::create(&path).unwrap();

        restrict_permissions(&path).unwrap();
        assert_owner_only(&path, true, false, false);

        // Changing a broad parent ACL must not reopen this protected file.
        apply_dacl(temp.path(), "D:P(A;OICI;FA;;;WD)").unwrap();
        assert_owner_only(&path, true, false, false);
    }

    #[test]
    fn protects_a_directory_and_inherits_owner_only_access_into_new_children() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("private");
        fs::create_dir(&directory).unwrap();
        apply_dacl(&directory, "D:P(A;OICI;FA;;;WD)").unwrap();

        restrict_permissions(&directory).unwrap();

        assert_owner_only(&directory, true, false, true);
        let child = directory.join("empty.tmp");
        fs::File::create(&child).unwrap();
        assert_owner_only(&child, false, true, false);
        let child_directory = directory.join("nested");
        fs::create_dir(&child_directory).unwrap();
        assert_owner_only(&child_directory, false, true, true);
    }

    #[test]
    fn reports_missing_paths_instead_of_claiming_protection() {
        let temp = tempfile::tempdir().unwrap();
        let error = restrict_permissions(&temp.path().join("missing.json")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn rejects_embedded_nul_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("profiles.json");
        fs::write(&path, b"{}").unwrap();
        let mut invalid = path.as_os_str().encode_wide().collect::<Vec<_>>();
        invalid.extend([0, b'x' as u16]);
        let invalid = OsString::from_wide(&invalid);

        let error = restrict_permissions(Path::new(&invalid)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
