//! Cooperative per-destination save locks plus a package conflict fingerprint.
//! Staging can take time: callers recheck under the lock immediately before the
//! atomic exchange, preserving another writer's completed version.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    path::Path,
    time::{Duration, Instant},
};

pub fn package_stamp(path: &Path) -> Option<u64> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut pending = vec![path.to_owned()];
    let mut count = 0usize;
    while let Some(current) = pending.pop() {
        count += 1;
        if count > 100_000 {
            return None;
        }
        let metadata = fs::symlink_metadata(&current).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        current.strip_prefix(path).ok()?.hash(&mut hash);
        let kind = if metadata.is_dir() {
            1u8
        } else if metadata.is_file() {
            2
        } else {
            0
        };
        kind.hash(&mut hash);
        metadata.len().hash(&mut hash);
        metadata.modified().ok()?.hash(&mut hash);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.dev().hash(&mut hash);
            metadata.ino().hash(&mut hash);
            metadata.ctime().hash(&mut hash);
            metadata.ctime_nsec().hash(&mut hash);
        }
        if metadata.is_dir() {
            let mut children = fs::read_dir(&current)
                .ok()?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()
                .ok()?;
            children.sort();
            pending.extend(children);
        }
    }
    Some(hash.finish())
}

/// Read a project package only if its on-disk tree stays unchanged throughout
/// the read. Standalone image files do not have a package identity, so callers
/// receive `None` for their stamp and retain their existing file-decoding
/// behavior.
pub fn read_consistent<T>(
    path: &Path,
    read: impl FnOnce() -> Result<T>,
) -> Result<(T, Option<u64>)> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("The source is unavailable: {}", path.display()))?;
    ensure!(
        !metadata.file_type().is_symlink() || !path.is_dir(),
        "The project package cannot be read safely"
    );
    if !metadata.is_dir() {
        return read().map(|value| (value, None));
    }

    let before = package_stamp(path).context("The project package cannot be read safely")?;
    let value = read()?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("The project changed while opening: {}", path.display()))?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "The project changed while opening. Try again."
    );
    let after = package_stamp(path).context("The project changed while opening. Try again.")?;
    ensure!(
        after == before,
        "The project changed while opening. Try again."
    );
    Ok((value, Some(after)))
}

pub struct SaveGuard {
    _lock: File,
}
impl SaveGuard {
    pub fn acquire(path: &Path) -> Result<Self> {
        Self::acquire_in(
            path,
            &crate::identity::data_dir().join("save-locks"),
            Duration::from_secs(2),
        )
    }
    fn acquire_in(path: &Path, root: &Path, timeout: Duration) -> Result<Self> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let canonical_parent = parent
            .canonicalize()
            .context("The destination folder is unavailable")?;
        let canonical =
            canonical_parent.join(path.file_name().context("Missing project file name")?);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        canonical.hash(&mut hash);
        fs::create_dir_all(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(0x20000 | 0x80000);
        }
        let lock = options.open(root.join(format!("{:016x}.lock", hash.finish())))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe extern "C" {
                fn flock(fd: i32, operation: i32) -> i32;
            }
            let started = Instant::now();
            loop {
                if unsafe { flock(lock.as_raw_fd(), 2 | 4) } == 0 {
                    break;
                }
                let error = std::io::Error::last_os_error();
                ensure!(
                    error.kind() == std::io::ErrorKind::WouldBlock,
                    "Cannot lock the destination: {error}"
                );
                ensure!(
                    started.elapsed() < timeout,
                    "Another Omuse window is saving this project. Try again when it finishes."
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(Self { _lock: lock })
    }
    pub fn verify(&self, path: &Path, expected: Option<u64>) -> Result<()> {
        let unchanged = match expected {
            Some(stamp) => package_stamp(path) == Some(stamp),
            None => {
                matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
            }
        };
        ensure!(
            unchanged,
            "The project changed on disk while saving. Use Save as to keep both versions."
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consistent_read_returns_a_stamp_only_for_an_unchanged_package() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("project.omuse");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("project.json"), "original").unwrap();

        let (value, stamp) = read_consistent(&package, || Ok::<_, anyhow::Error>("read"))
            .expect("unchanged package should be read");
        assert_eq!(value, "read");
        assert_eq!(stamp, package_stamp(&package));

        let file = temp.path().join("reference.png");
        fs::write(&file, b"image").unwrap();
        let (value, stamp) = read_consistent(&file, || Ok::<_, anyhow::Error>("file")).unwrap();
        assert_eq!(value, "file");
        assert_eq!(stamp, None);
    }

    #[test]
    fn consistent_read_rejects_a_package_changed_between_stamps() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("project.omuse");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("project.json"), "original").unwrap();

        let error = read_consistent(&package, || -> Result<()> {
            fs::write(package.join("project.json"), "replacement package contents")?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("changed while opening"));
    }

    #[cfg(unix)]
    fn set_modified(path: &Path, modified: std::time::SystemTime) {
        File::open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn consistent_read_rejects_a_replaced_package_with_matching_contents_and_mtime() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("project.omuse");
        let replacement = temp.path().join("replacement.omuse");
        let moved = temp.path().join("moved.omuse");
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::create_dir(&package).unwrap();
        fs::write(package.join("project.json"), "original").unwrap();
        fs::create_dir(&replacement).unwrap();
        fs::write(replacement.join("project.json"), "original").unwrap();
        for source in [&package, &replacement] {
            set_modified(&source.join("project.json"), modified);
            set_modified(source, modified);
        }

        let error = read_consistent(&package, || -> Result<()> {
            fs::rename(&package, &moved)?;
            fs::rename(&replacement, &package)?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("changed while opening"));
    }

    #[test]
    fn consistent_read_rejects_a_package_replaced_with_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("project.omuse");
        let moved = temp.path().join("moved.omuse");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("project.json"), "original").unwrap();

        let error = read_consistent(&package, || -> Result<()> {
            fs::rename(&package, &moved)?;
            fs::write(&package, "not a package")?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("changed while opening"));
    }

    #[test]
    fn consistent_read_preserves_the_reader_error() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("project.omuse");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("project.json"), "original").unwrap();

        let error = read_consistent(&package, || -> Result<()> {
            anyhow::bail!("reader failure")
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "reader failure");
    }

    #[test]
    fn concurrent_writer_waits_and_stale_snapshot_cannot_publish() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("project.omuse");
        let root = temp.path().join("locks");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("project.json"), "old").unwrap();
        let before = package_stamp(&path);
        let lock = SaveGuard::acquire_in(&path, &root, Duration::from_millis(30)).unwrap();
        assert!(SaveGuard::acquire_in(&path, &root, Duration::from_millis(30)).is_err());
        fs::write(path.join("external-write"), "newer work").unwrap();
        assert!(lock.verify(&path, before).is_err());
        drop(lock);
        assert!(SaveGuard::acquire_in(&path, &root, Duration::from_millis(30)).is_ok());
    }
    #[test]
    fn appearing_destination_invalidates_new_save_expectation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("new.omuse");
        let lock =
            SaveGuard::acquire_in(&path, &temp.path().join("locks"), Duration::from_millis(30))
                .unwrap();
        assert!(lock.verify(&path, None).is_ok());
        fs::create_dir(&path).unwrap();
        assert!(lock.verify(&path, None).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn unreadable_or_symlink_destination_is_not_treated_as_absent() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("new.omuse");
        let lock =
            SaveGuard::acquire_in(&path, &temp.path().join("locks"), Duration::from_millis(30))
                .unwrap();
        std::os::unix::fs::symlink(temp.path().join("missing-target"), &path).unwrap();
        assert_eq!(package_stamp(&path), None);
        assert!(lock.verify(&path, None).is_err());
    }
}
