//! Folders that only the current user can open, such as AI job and sign-in
//! workspaces.
//!
//! Unix uses mode 0o700. Windows sets a protected access list that grants only
//! the current user full control. A Windows folder counts as private when its
//! access list allows no one else except SYSTEM and Administrators, which can
//! open any user's files anyway, as root can on Unix.
use std::{io, path::Path};

/// Restrict an existing directory to the current user.
pub fn make_private(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(windows)]
    {
        windows::make_private(path)
    }
}

/// Whether an existing directory is private to the current user.
pub fn is_private(path: &Path) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(std::fs::metadata(path)?.permissions().mode() & 0o077 == 0)
    }
    #[cfg(windows)]
    {
        windows::is_private(path)
    }
}

#[cfg(windows)]
mod windows {
    use std::{io, os::windows::ffi::OsStrExt, path::Path, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree},
        Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT, SetNamedSecurityInfoW},
        Security::{
            ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, AddAccessAllowedAceEx,
            CONTAINER_INHERIT_ACE, CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
            GetLengthSid, GetTokenInformation, InitializeAcl, OBJECT_INHERIT_ACE,
            PROTECTED_DACL_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, TOKEN_QUERY,
            TOKEN_USER, TokenUser, WELL_KNOWN_SID_TYPE, WinBuiltinAdministratorsSid,
            WinCreatorOwnerRightsSid, WinCreatorOwnerSid, WinLocalSystemSid,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const ACCESS_DENIED_ACE_TYPE: u8 = 1;
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;

    /// A security identifier in a DWORD-aligned buffer.
    pub(super) struct Sid(Vec<u32>);

    impl Sid {
        fn copy(sid: PSID) -> Self {
            // SAFETY: `sid` points to a valid SID of GetLengthSid bytes.
            unsafe {
                let length = GetLengthSid(sid) as usize;
                let mut buffer = vec![0u32; length.div_ceil(4)];
                ptr::copy_nonoverlapping(sid as *const u8, buffer.as_mut_ptr().cast(), length);
                Self(buffer)
            }
        }

        pub(super) fn as_psid(&self) -> PSID {
            self.0.as_ptr() as PSID
        }

        pub(super) fn well_known(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Self> {
            let mut buffer = vec![0u32; (SECURITY_MAX_SID_SIZE as usize).div_ceil(4)];
            let mut size = (buffer.len() * 4) as u32;
            // SAFETY: the buffer holds SECURITY_MAX_SID_SIZE bytes.
            let created = unsafe {
                CreateWellKnownSid(kind, ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size)
            };
            if created == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(buffer))
        }

        pub(super) fn current_user() -> io::Result<Self> {
            // SAFETY: the token handle is closed before returning, and the
            // TOKEN_USER buffer is 8-byte aligned and sized by the first call.
            unsafe {
                let mut token: HANDLE = ptr::null_mut();
                if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let mut length = 0u32;
                GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length);
                let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
                let read = GetTokenInformation(
                    token,
                    TokenUser,
                    buffer.as_mut_ptr().cast(),
                    length,
                    &mut length,
                );
                let error = io::Error::last_os_error();
                CloseHandle(token);
                if read == 0 {
                    return Err(error);
                }
                let user = &*(buffer.as_ptr() as *const TOKEN_USER);
                Ok(Self::copy(user.User.Sid))
            }
        }
    }

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut units: Vec<u16> = crate::durable_fs::win32_path(path)
            .as_os_str()
            .encode_wide()
            .collect();
        if units.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Path contains NUL",
            ));
        }
        units.push(0);
        Ok(units)
    }

    /// Replace the folder's access list with one inheritable entry that gives
    /// each listed SID full control, and stop inheriting from the parent.
    pub(super) fn set_access(path: &Path, sids: &[&Sid]) -> io::Result<()> {
        let name = wide(path)?;
        // SAFETY: the ACL buffer is DWORD aligned and sized for its header and
        // one ACCESS_ALLOWED_ACE per SID (each ACE stores the SID in place of
        // its SidStart field); all pointers outlive the calls.
        unsafe {
            let size = std::mem::size_of::<ACL>()
                + sids
                    .iter()
                    .map(|sid| {
                        std::mem::size_of::<ACCESS_ALLOWED_ACE>() - std::mem::size_of::<u32>()
                            + GetLengthSid(sid.as_psid()) as usize
                    })
                    .sum::<usize>();
            let mut buffer = vec![0u32; size.div_ceil(4)];
            let acl = buffer.as_mut_ptr() as *mut ACL;
            if InitializeAcl(acl, (buffer.len() * 4) as u32, ACL_REVISION) == 0 {
                return Err(io::Error::last_os_error());
            }
            for sid in sids {
                if AddAccessAllowedAceEx(
                    acl,
                    ACL_REVISION,
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                    FILE_ALL_ACCESS,
                    sid.as_psid(),
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            let status = SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                acl,
                ptr::null(),
            );
            if status != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
        }
        Ok(())
    }

    pub(super) fn make_private(path: &Path) -> io::Result<()> {
        set_access(path, &[&Sid::current_user()?])
    }

    pub(super) fn is_private(path: &Path) -> io::Result<bool> {
        let allowed = [
            Sid::current_user()?,
            Sid::well_known(WinLocalSystemSid)?,
            Sid::well_known(WinBuiltinAdministratorsSid)?,
            Sid::well_known(WinCreatorOwnerSid)?,
            Sid::well_known(WinCreatorOwnerRightsSid)?,
        ];
        let name = wide(path)?;
        // SAFETY: GetNamedSecurityInfoW returns a descriptor that owns `dacl`;
        // it is freed with LocalFree after the ACEs have been read.
        unsafe {
            let mut dacl: *mut ACL = ptr::null_mut();
            let mut descriptor = ptr::null_mut();
            let status = GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            );
            if status != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
            let result = (|| {
                // A null access list grants everyone full access.
                if dacl.is_null() {
                    return Ok(false);
                }
                for index in 0..u32::from((*dacl).AceCount) {
                    let mut ace = ptr::null_mut();
                    if GetAce(dacl, index, &mut ace) == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    match (*(ace as *const ACE_HEADER)).AceType {
                        ACCESS_DENIED_ACE_TYPE => {}
                        ACCESS_ALLOWED_ACE_TYPE => {
                            let sid = &(*(ace as *const ACCESS_ALLOWED_ACE)).SidStart as *const u32
                                as PSID;
                            if !allowed
                                .iter()
                                .any(|candidate| EqualSid(candidate.as_psid(), sid) != 0)
                            {
                                return Ok(false);
                            }
                        }
                        // Object and callback entries are not used on files;
                        // treat an unrecognised grant as not private.
                        _ => return Ok(false),
                    }
                }
                Ok(true)
            })();
            LocalFree(descriptor);
            result
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::Security::WinBuiltinUsersSid;

        #[test]
        fn private_folders_admit_only_the_current_user() {
            let temp = tempfile::tempdir().unwrap();
            let folder = temp.path().join("workspace");
            std::fs::create_dir(&folder).unwrap();
            make_private(&folder).unwrap();
            assert!(is_private(&folder).unwrap());
            // The owner can still use it, and new files inherit the access list.
            std::fs::write(folder.join("prompt.json"), b"{}").unwrap();
            std::fs::create_dir(folder.join("nested")).unwrap();
            assert!(is_private(&folder.join("nested")).unwrap());

            let users = Sid::well_known(WinBuiltinUsersSid).unwrap();
            set_access(&folder, &[&Sid::current_user().unwrap(), &users]).unwrap();
            assert!(!is_private(&folder).unwrap());
        }
    }
}
