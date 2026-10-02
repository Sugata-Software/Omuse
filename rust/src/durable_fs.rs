//! Durable writes and package publication shared by Linux and Windows.
//!
//! Linux package writers keep their `renameat2` exchanges. These helpers give
//! every platform the same flush call and supply the Windows publication steps.
use std::{fs, io, path::Path};

/// Flush a file or directory, including its metadata, to stable storage.
pub fn sync_path(path: impl AsRef<Path>) -> io::Result<()> {
    open_for_sync(path)?.sync_all()
}

/// Open a file or directory so that `sync_all` can flush it.
///
/// Windows flushes only handles opened for writing and opens directories only
/// with backup semantics; elsewhere a read-only handle is sufficient.
pub fn open_for_sync(path: impl AsRef<Path>) -> io::Result<fs::File> {
    let path = path.as_ref();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        fs::File::open(path)
    }
}

/// Move `from` to `to`, failing if `to` already exists.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        move_no_replace(from, to)
    }
    #[cfg(not(windows))]
    {
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destination already exists",
            ));
        }
        fs::rename(from, to)
    }
}

/// Swap two existing directories: `to` receives the staged package and `from`
/// receives the previous one, matching Linux `RENAME_EXCHANGE`.
///
/// Windows has no atomic directory exchange. The previous package first moves
/// to a hidden sibling, so a crash between the two moves leaves it complete
/// under that name rather than losing it; a failed second move restores it.
pub fn exchange_dirs(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        for path in [from, to] {
            if !fs::symlink_metadata(path)?.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not a directory", path.display()),
                ));
            }
        }
        let name = to.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Package path has no name")
        })?;
        let aside = to.with_file_name(format!(
            ".{}.previous-{}",
            name.to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        move_no_replace(to, &aside)?;
        if let Err(error) = move_no_replace(from, to) {
            return Err(match move_no_replace(&aside, to) {
                Ok(()) => error,
                Err(_) => io::Error::new(
                    error.kind(),
                    format!(
                        "{error}; the previous package was kept at {}",
                        aside.display()
                    ),
                ),
            });
        }
        // Publication has succeeded. Hand the previous package back at the
        // staging path as an exchange would; failing that, discard the copy.
        if move_no_replace(&aside, from).is_err() {
            let _ = fs::remove_dir_all(&aside);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (from, to);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Package exchange is not implemented on this platform",
        ))
    }
}

#[cfg(windows)]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    use std::time::{Duration, Instant};
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    // Antivirus, indexing and sync clients such as OneDrive open files in a
    // package while scanning or uploading it, which blocks moving its folder.
    const BUDGET: Duration = Duration::from_secs(10);
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut units: Vec<u16> = win32_path(path).as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Path contains NUL",
            ));
        }
        units.push(0);
        Ok(units)
    }
    let (from, to) = (wide(from)?, wide(to)?);
    let started = Instant::now();
    let mut delay = Duration::from_millis(25);
    loop {
        // SAFETY: both NUL-terminated buffers outlive the call and are not
        // retained. Without MOVEFILE_REPLACE_EXISTING an existing `to` fails.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        // Retry access-denied, sharing and lock violations with backoff.
        if started.elapsed() < BUDGET && matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
            std::thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(500));
            continue;
        }
        return Err(error);
    }
}

/// The form of `path` to pass to Win32 calls. Rust's standard library adds the
/// `\\?\` prefix to long paths itself; direct calls need it too unless the
/// system enables long paths. Short paths are returned unchanged.
#[cfg(windows)]
pub fn win32_path(path: &Path) -> std::path::PathBuf {
    use std::{
        ffi::OsString,
        path::{Component, Prefix},
    };
    // CreateDirectoryW's limit, the smallest of the classic MAX_PATH limits.
    const LIMIT: usize = 248;
    if path.as_os_str().len() < LIMIT {
        return path.to_owned();
    }
    let Ok(absolute) = std::path::absolute(path) else {
        return path.to_owned();
    };
    let prefixed = match absolute.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) => {
                let mut value = OsString::from(r"\\?\");
                value.push(absolute.as_os_str());
                value
            }
            Prefix::UNC(..) => {
                // \\server\share\rest becomes \\?\UNC\server\share\rest.
                let text = absolute.to_string_lossy();
                match text.strip_prefix(r"\\") {
                    Some(rest) if absolute.to_str().is_some() => {
                        OsString::from(format!(r"\\?\UNC\{rest}"))
                    }
                    _ => return absolute,
                }
            }
            _ => return absolute,
        },
        _ => return absolute,
    };
    prefixed.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_path_flushes_files_and_directories() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("manifest.json");
        fs::write(&file, b"{}").unwrap();
        sync_path(&file).unwrap();
        sync_path(temp.path()).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{}");
        assert!(sync_path(temp.path().join("missing")).is_err());
    }

    #[test]
    fn rename_no_replace_refuses_existing_files_and_directories() {
        let temp = tempfile::tempdir().unwrap();
        let (source, file, directory) = (
            temp.path().join("source"),
            temp.path().join("file"),
            temp.path().join("directory"),
        );
        fs::create_dir(&source).unwrap();
        fs::write(&file, b"keep").unwrap();
        fs::create_dir(&directory).unwrap();
        assert!(rename_no_replace(&source, &file).is_err());
        assert!(rename_no_replace(&source, &directory).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"keep");
        let published = temp.path().join("published");
        rename_no_replace(&source, &published).unwrap();
        assert!(published.is_dir() && !source.exists());
    }

    #[cfg(windows)]
    #[test]
    fn exchange_dirs_publishes_the_stage_and_returns_the_previous_package() {
        let temp = tempfile::tempdir().unwrap();
        let (stage, target) = (temp.path().join("stage"), temp.path().join("Art.omuse"));
        fs::create_dir(&stage).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(stage.join("manifest.json"), b"new").unwrap();
        fs::write(target.join("manifest.json"), b"old").unwrap();
        exchange_dirs(&stage, &target).unwrap();
        assert_eq!(fs::read(target.join("manifest.json")).unwrap(), b"new");
        assert_eq!(fs::read(stage.join("manifest.json")).unwrap(), b"old");
        let names: Vec<_> = fs::read_dir(temp.path()).unwrap().collect();
        assert_eq!(names.len(), 2, "no hidden previous package remains");
    }

    #[cfg(windows)]
    #[test]
    fn win32_path_prefixes_only_long_paths() {
        let short = Path::new(r"C:\Art\Project.omuse");
        assert_eq!(win32_path(short), short);
        let long = format!(r"C:\{}\Project.omuse", "folder\\".repeat(40));
        let converted = win32_path(Path::new(&long));
        let converted = converted.to_str().unwrap();
        assert!(converted.starts_with(r"\\?\C:\folder\folder"));
        // Verbatim paths are not normalized by Windows, so separators must be.
        assert!(!converted[4..].contains(r"\\"));
        let unc = format!(r"\\server\share\{}\Project.omuse", "folder\\".repeat(40));
        assert!(
            win32_path(Path::new(&unc))
                .to_str()
                .unwrap()
                .starts_with(r"\\?\UNC\server\share\folder")
        );
    }

    #[cfg(windows)]
    #[test]
    fn packages_publish_and_exchange_beyond_max_path() {
        let temp = tempfile::tempdir().unwrap();
        let mut deep = temp.path().to_path_buf();
        while deep.as_os_str().len() < 300 {
            deep.push("a-deep-folder-for-long-paths");
        }
        fs::create_dir_all(&deep).unwrap();
        let (stage, target) = (deep.join("stage"), deep.join("Art.omuse"));
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("manifest.json"), b"first").unwrap();
        rename_no_replace(&stage, &target).unwrap();
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("manifest.json"), b"second").unwrap();
        exchange_dirs(&stage, &target).unwrap();
        assert_eq!(fs::read(target.join("manifest.json")).unwrap(), b"second");
    }

    /// A sync client or scanner holding a file inside the package blocks the
    /// folder move until it closes; publication must wait rather than fail.
    #[cfg(windows)]
    #[test]
    fn exchange_dirs_waits_for_a_briefly_held_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let (stage, target) = (temp.path().join("stage"), temp.path().join("Art.omuse"));
        fs::create_dir(&stage).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(stage.join("manifest.json"), b"new").unwrap();
        fs::write(target.join("manifest.json"), b"old").unwrap();
        // Share read and write but not delete, as an uploading client may.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(target.join("manifest.json"))
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            drop(held);
        });
        exchange_dirs(&stage, &target).unwrap();
        release.join().unwrap();
        assert_eq!(fs::read(target.join("manifest.json")).unwrap(), b"new");
    }

    #[cfg(windows)]
    #[test]
    fn exchange_dirs_keeps_the_target_when_the_stage_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("Art.omuse");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("manifest.json"), b"old").unwrap();
        assert!(exchange_dirs(&temp.path().join("missing"), &target).is_err());
        assert_eq!(fs::read(target.join("manifest.json")).unwrap(), b"old");
    }
}
