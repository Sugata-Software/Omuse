//! Linux replacement access-policy preservation for generated package entries.
//! Only known generated counterparts are copied, never arbitrary old files.
//! Visible fd xattrs include POSIX ACLs; privileged hidden attributes, inode
//! flags, timestamps and Windows ACLs are outside this bounded contract.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The outer container remains private while its payload gets final shared
/// permissions. Keeping the package root itself private until rename is not
/// enough: changing it to group-writable exposes a sibling staging pathname.
pub fn create_package_stage(parent: &Path, prefix: &str) -> Result<(PathBuf, PathBuf)> {
    let wrapper = parent.join(format!("{prefix}{}", uuid::Uuid::new_v4()));
    create_private_directory(&wrapper)?;
    let payload = wrapper.join("package");
    if let Err(error) = fs::create_dir(&payload) {
        let _ = fs::remove_dir(&wrapper);
        return Err(error.into());
    }
    Ok((payload, wrapper))
}

pub fn create_private_directory(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// A stage lease can point at a previous package in its private wrapper, or a
/// distinct Windows fallback sibling. Own both paths until the lease releases.
pub fn cleanup_package_stage(payload: &Path, wrapper: Option<&Path>) {
    if let Some(wrapper) = wrapper {
        if !payload.starts_with(wrapper) {
            let _ = fs::remove_dir_all(payload);
        }
        #[cfg(target_os = "linux")]
        let _ = linux::prepare_private_wrapper_disposal(wrapper);
        let _ = fs::remove_dir_all(wrapper);
    } else {
        let _ = fs::remove_dir_all(payload);
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct PackageMetadataGuard {
    destination: PathBuf,
    stamp: Option<u64>,
}
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct PackageMetadataGuard;

impl PackageMetadataGuard {
    /// Capture before staging so concurrent permission/xattr changes cannot be
    /// silently replaced by an older access policy. ctime fences Linux xattrs.
    pub fn capture(destination: &Path) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let stamp = match fs::symlink_metadata(destination) {
                Ok(metadata) => {
                    ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "Invalid metadata source");
                    Some(crate::save_guard::package_stamp(destination).context("Cannot inspect existing package access metadata; save a copy under another name")?)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error).context("Cannot inspect existing package access metadata; save a copy under another name"),
            };
            Ok(Self {
                destination: destination.to_owned(),
                stamp,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = destination;
            Ok(Self)
        }
    }
    pub fn verify(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let unchanged = match self.stamp {
                Some(stamp) => crate::save_guard::package_stamp(&self.destination) == Some(stamp),
                None => {
                    matches!(fs::symlink_metadata(&self.destination), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                }
            };
            ensure!(
                unchanged,
                "The package access policy or contents changed while saving. Original retained; save a copy under another name"
            );
        }
        Ok(())
    }
    /// Seed a recognized directory before creating new children so ordinary
    /// umask, setgid and default-ACL inheritance use the original directory.
    /// The private outer wrapper prevents temporary shared access at its name.
    pub fn seed_directory(&self, target: &Path, relative: &Path) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.stamp.is_some() {
            // capture() fenced the starting policy. Full-tree verification is
            // deferred to finalize() and the final prepublication gate, rather
            // than rescanning an entire Create package for each directory.
            linux::seed_directory(target, &self.destination, relative)
                .context("Cannot preserve directory inheritance policy; original retained, save a copy under another name")?;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, relative);
        }
        Ok(())
    }
    /// Finalize children first, then their directories, under the private
    /// wrapper. Aliases are exact recognized Create page/component prefixes.
    pub fn finalize(&self, payload: &Path, aliases: &[(PathBuf, PathBuf)]) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.verify()?;
            linux::finalize(payload, self.stamp.map(|_| self.destination.as_path()), aliases)
                .context("Cannot preserve package ownership, permissions or visible extended attributes. Original retained; save a copy under another name")?;
            self.verify()?;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (payload, aliases);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        collections::BTreeMap,
        ffi::CString,
        fs::File,
        io,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
        path::Component,
    };
    const MAX_ENTRIES: usize = 100_000;
    const MAX_DEPTH: usize = 32;
    const MAX_PATH_BYTES: usize = 32 * 1024 * 1024;
    const MAX_NAMES: usize = 16 * 1024;
    const MAX_ATTRS: usize = 64;
    const MAX_VALUE: usize = 64 * 1024;
    const MAX_TOTAL: usize = 16 * 1024 * 1024;
    const O_NOFOLLOW: i32 = 0x20000;
    const O_CLOEXEC: i32 = 0x80000;
    const O_DIRECTORY: i32 = 0x10000;
    const O_NONBLOCK: i32 = 0x800;
    unsafe extern "C" {
        fn openat(dirfd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
        fn fchown(fd: i32, owner: u32, group: u32) -> i32;
        fn fchmod(fd: i32, mode: u32) -> i32;
        fn flistxattr(fd: i32, list: *mut std::ffi::c_char, size: usize) -> isize;
        fn fgetxattr(
            fd: i32,
            name: *const std::ffi::c_char,
            value: *mut std::ffi::c_void,
            size: usize,
        ) -> isize;
        fn fsetxattr(
            fd: i32,
            name: *const std::ffi::c_char,
            value: *const std::ffi::c_void,
            size: usize,
            flags: i32,
        ) -> i32;
        fn fremovexattr(fd: i32, name: *const std::ffi::c_char) -> i32;
    }
    fn syscall(result: i32) -> io::Result<()> {
        if result == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    fn root_file(path: &Path) -> io::Result<File> {
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC)
            .open(path)
    }
    /// No-follow at every component, not just the final filename. A symlink in
    /// an old images/page directory must never redirect the policy lookup.
    fn beneath(root: &File, relative: &Path) -> io::Result<Option<File>> {
        let parts: Vec<_> = relative.components().collect();
        let mut current = root.try_clone()?;
        for (index, component) in parts.iter().enumerate() {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid package metadata path",
                ));
            };
            let name = CString::new(name.as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL path"))?;
            let flags = O_NOFOLLOW
                | O_CLOEXEC
                | O_NONBLOCK
                | if index + 1 < parts.len() {
                    O_DIRECTORY
                } else {
                    0
                };
            // SAFETY: current owns the live directory fd; the NUL path and all
            // descriptors remain valid for this non-creating openat call.
            let fd = unsafe { openat(current.as_raw_fd(), name.as_ptr(), flags) };
            if fd == -1 {
                let error = io::Error::last_os_error();
                return if error.kind() == io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(error)
                };
            }
            // SAFETY: successful openat returns one owned descriptor.
            current = unsafe { File::from_raw_fd(fd) };
            let meta = current.metadata()?;
            if !meta.is_dir() && !meta.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Not a regular metadata counterpart",
                ));
            }
        }
        Ok(Some(current))
    }
    fn attributes(file: &File, used: &mut usize) -> Result<Option<BTreeMap<Vec<u8>, Vec<u8>>>> {
        // SAFETY: size0 requests the list length, with no writable buffer.
        let len = unsafe { flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0) };
        if len < 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(95) {
                Ok(None)
            } else {
                Err(error.into())
            };
        }
        ensure!(
            (len as usize) <= MAX_NAMES,
            "Extended attribute names exceed budget"
        );
        let mut names = vec![0u8; len as usize];
        // SAFETY: names has exactly the supplied writable length.
        let got = unsafe { flistxattr(file.as_raw_fd(), names.as_mut_ptr().cast(), names.len()) };
        ensure!(
            got >= 0,
            "Cannot read extended attribute list: {}",
            io::Error::last_os_error()
        );
        ensure!(
            got as usize == names.len(),
            "Extended attribute list changed while saving"
        );
        let mut attrs = BTreeMap::new();
        for name in names.split_inclusive(|byte| *byte == 0) {
            ensure!(
                name.last() == Some(&0) && name.len() > 1 && name.len() <= 256,
                "Invalid extended attribute name"
            );
            ensure!(attrs.len() < MAX_ATTRS, "Too many extended attributes");
            let c_name = CString::from_vec_with_nul(name.to_vec())?;
            // SAFETY: name is NUL-terminated; size0 queries the value length.
            let len =
                unsafe { fgetxattr(file.as_raw_fd(), c_name.as_ptr(), std::ptr::null_mut(), 0) };
            ensure!(
                len >= 0,
                "Cannot inspect extended attribute: {}",
                io::Error::last_os_error()
            );
            ensure!(
                len as usize <= MAX_VALUE,
                "Extended attribute value exceeds budget"
            );
            *used = used
                .checked_add(name.len() + len as usize)
                .context("Extended attribute budget overflow")?;
            ensure!(
                *used <= MAX_TOTAL,
                "Package extended attributes exceed budget"
            );
            let mut value = vec![0u8; len as usize];
            // SAFETY: live fd, NUL name, value buffer has supplied capacity.
            let got = unsafe {
                fgetxattr(
                    file.as_raw_fd(),
                    c_name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            };
            ensure!(
                got >= 0 && got as usize == value.len(),
                "Extended attribute changed or cannot be read: {}",
                io::Error::last_os_error()
            );
            ensure!(
                attrs
                    .insert(name[..name.len() - 1].to_vec(), value)
                    .is_none(),
                "Duplicate extended attribute"
            );
        }
        Ok(Some(attrs))
    }
    fn copy_metadata(source: &File, target: &File, used: &mut usize) -> Result<()> {
        let old = source.metadata()?;
        let current = target.metadata()?;
        ensure!(
            old.is_dir() == current.is_dir() && old.is_file() == current.is_file(),
            "Package metadata counterpart changed type"
        );
        ensure!(
            old.is_dir() || old.is_file(),
            "Invalid package metadata type"
        );
        let expected_attrs = attributes(source, used)?;
        let _ = attributes(target, used)?; // Refuse unreadable target metadata before ownership mutation.
        if old.uid() != current.uid() || old.gid() != current.gid() {
            // SAFETY: target owns its descriptor. Unchanged owner/group uses
            // the Linux -1 sentinel, avoiding unnecessary privileged chown.
            syscall(unsafe {
                fchown(
                    target.as_raw_fd(),
                    if old.uid() == current.uid() {
                        u32::MAX
                    } else {
                        old.uid()
                    },
                    if old.gid() == current.gid() {
                        u32::MAX
                    } else {
                        old.gid()
                    },
                )
            })?;
        }
        // chown may clear capability/security attributes. Compare *after* it.
        let actual_attrs = attributes(target, used)?;
        ensure!(
            expected_attrs.is_some() == actual_attrs.is_some(),
            "Extended attribute support differs between old and staged package"
        );
        if let (Some(expected), Some(actual)) = (&expected_attrs, &actual_attrs) {
            for name in actual.keys().filter(|name| !expected.contains_key(*name)) {
                let name = CString::new(name.as_slice())?;
                // SAFETY: valid live fd and NUL-terminated name.
                syscall(unsafe { fremovexattr(target.as_raw_fd(), name.as_ptr()) })?;
            }
            for (name, value) in expected
                .iter()
                .filter(|(name, value)| actual.get(*name) != Some(*value))
            {
                let name = CString::new(name.as_slice())?;
                // SAFETY: fd, NUL name and value remain live for the call.
                syscall(unsafe {
                    fsetxattr(
                        target.as_raw_fd(),
                        name.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                    )
                })?;
            }
        }
        // Chown precedes chmod. ACL mode masks are coherent with the original
        // mode; verify exact ACL bytes again after chmod rather than assume it.
        if target.metadata()?.mode() & 0o7777 != old.mode() & 0o7777 {
            // SAFETY: target owns this descriptor and mode is permission bits.
            syscall(unsafe { fchmod(target.as_raw_fd(), old.mode() & 0o7777) })?;
        }
        let result = target.metadata()?;
        ensure!(
            result.uid() == old.uid()
                && result.gid() == old.gid()
                && result.mode() & 0o7777 == old.mode() & 0o7777,
            "Staged ownership or permissions do not match existing package"
        );
        ensure!(
            attributes(target, used)? == expected_attrs,
            "Staged extended attributes/ACL do not match existing package"
        );
        Ok(())
    }
    /// Called only after a non-retained private staging lease has expired.
    /// Retired read-only directory policies must not make cleanup silently leak
    /// every old package. Never chmod a published destination or public aside.
    pub(super) fn prepare_private_wrapper_disposal(wrapper: &Path) -> Result<()> {
        let root = root_file(wrapper)?;
        ensure!(
            root.metadata()?.mode() & 0o077 == 0,
            "Not a private staging wrapper"
        );
        fn descend(directory: &File, depth: usize, count: &mut usize) -> Result<()> {
            ensure!(depth <= MAX_DEPTH + 1, "Cleanup depth exceeds budget");
            *count += 1;
            ensure!(*count <= MAX_ENTRIES + 1, "Cleanup entries exceed budget");
            let mode = directory.metadata()?.mode() & 0o7777 | 0o700;
            // SAFETY: this owned directory fd is inside the worker's expired
            // private wrapper. Only owner construction/disposal access changes.
            syscall(unsafe { fchmod(directory.as_raw_fd(), mode) })?;
            // Linux procfs resolves this retained fd even if an old pathname
            // changed. All descendant mutation uses openat+O_NOFOLLOW, not an
            // untrusted child pathname followed through a symlink.
            let fd_path = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
            for entry in fs::read_dir(fd_path)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    *count += 1;
                    ensure!(*count <= MAX_ENTRIES + 1, "Cleanup entries exceed budget");
                    continue;
                }
                let name = CString::new(entry.file_name().as_bytes())?;
                // SAFETY: live parent directory fd and NUL leaf name; no
                // creation and no symlink-following are permitted.
                let fd = unsafe {
                    openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK,
                    )
                };
                ensure!(
                    fd >= 0,
                    "Cannot open retired staging directory: {}",
                    io::Error::last_os_error()
                );
                // SAFETY: successful openat returns an owned descriptor.
                let child = unsafe { File::from_raw_fd(fd) };
                descend(&child, depth + 1, count)?;
            }
            Ok(())
        }
        descend(&root, 0, &mut 0)
    }
    pub(super) fn seed_directory(target: &Path, existing: &Path, relative: &Path) -> Result<()> {
        let root = root_file(existing)?;
        if let Some(source) = beneath(&root, relative)? {
            ensure!(
                source.metadata()?.is_dir(),
                "Directory metadata counterpart is not a directory"
            );
            let staged = root_file(target)?;
            copy_metadata(&source, &staged, &mut 0)?;
            // Keep construction possible even for an originally read-only
            // directory. The private wrapper blocks outside access; the final
            // child-first pass restores the exact old mode and access ACL.
            let construction_mode = source.metadata()?.mode() & 0o7777 | 0o700;
            syscall(unsafe { fchmod(staged.as_raw_fd(), construction_mode) })?;
        }
        Ok(())
    }
    fn alias(relative: &Path, aliases: &[(PathBuf, PathBuf)]) -> PathBuf {
        aliases
            .iter()
            .find_map(|(new, old)| relative.strip_prefix(new).ok().map(|tail| old.join(tail)))
            .unwrap_or_else(|| relative.to_owned())
    }
    pub(super) fn finalize(
        payload: &Path,
        existing: Option<&Path>,
        aliases: &[(PathBuf, PathBuf)],
    ) -> Result<()> {
        ensure!(aliases.len() <= 768, "Too many package metadata aliases");
        let mut pending = vec![PathBuf::new()];
        let mut generated = vec![];
        let mut path_bytes = 0usize;
        while let Some(relative) = pending.pop() {
            ensure!(
                generated.len() < MAX_ENTRIES,
                "Package metadata entry budget exceeded"
            );
            ensure!(
                relative.components().count() <= MAX_DEPTH,
                "Package metadata depth exceeded"
            );
            path_bytes = path_bytes
                .checked_add(relative.as_os_str().len())
                .context("Package metadata path overflow")?;
            ensure!(
                path_bytes <= MAX_PATH_BYTES,
                "Package metadata paths exceed budget"
            );
            let meta = fs::symlink_metadata(payload.join(&relative))?;
            ensure!(
                !meta.file_type().is_symlink() && (meta.is_dir() || meta.is_file()),
                "Invalid generated package entry"
            );
            if meta.is_dir() {
                for child in fs::read_dir(payload.join(&relative))? {
                    ensure!(
                        pending.len() + generated.len() < MAX_ENTRIES,
                        "Package metadata entry budget exceeded"
                    );
                    pending.push(relative.join(child?.file_name()));
                }
            }
            generated.push(relative);
        }
        generated.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        let new_root = root_file(payload)?;
        let old_root = existing.map(root_file).transpose()?;
        let mut used = 0usize;
        for relative in generated {
            let target =
                beneath(&new_root, &relative)?.context("Generated metadata target disappeared")?;
            if let Some(source_root) = &old_root {
                if let Some(source) = beneath(source_root, &alias(&relative, aliases))? {
                    copy_metadata(&source, &target, &mut used).with_context(|| {
                        format!("Cannot preserve metadata for {}", relative.display())
                    })?;
                }
            }
            // Metadata changes occur after original file writes: flush again.
            if target.metadata()?.is_dir() {
                match target.sync_all() {
                    Err(error) if super::super::directory_sync_unsupported(&error) => {}
                    result => result?,
                }
            } else {
                target.sync_all()?;
            }
        }
        Ok(())
    }
}
