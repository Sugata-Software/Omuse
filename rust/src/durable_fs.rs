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
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    const RETRIES: u32 = 40;
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
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
    let mut attempt = 0;
    loop {
        // SAFETY: both NUL-terminated buffers outlive the call and are not
        // retained. Without MOVEFILE_REPLACE_EXISTING an existing `to` fails.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        // Antivirus and indexing services briefly open files in new packages:
        // retry access-denied and sharing violations for about one second.
        if attempt < RETRIES && matches!(error.raw_os_error(), Some(5 | 32)) {
            attempt += 1;
            std::thread::sleep(std::time::Duration::from_millis(25));
            continue;
        }
        return Err(error);
    }
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
    fn exchange_dirs_keeps_the_target_when_the_stage_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("Art.omuse");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("manifest.json"), b"old").unwrap();
        assert!(exchange_dirs(&temp.path().join("missing"), &target).is_err());
        assert_eq!(fs::read(target.join("manifest.json")).unwrap(), b"old");
    }
}
