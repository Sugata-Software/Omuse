//! Durable writes and package publication shared by Linux and Windows.
//!
//! Linux package writers keep their `renameat2` exchanges. These helpers give
//! every platform the same flush call and supply the Windows publication steps.
use std::{fs, io, path::Path};

#[path = "package_metadata.rs"]
mod package_metadata;
pub use package_metadata::{
    PackageMetadataGuard, cleanup_package_stage, create_package_stage, create_private_directory,
};

/// Publication has already made a complete new package visible. A failed
/// final directory flush must not be confused with a rejected transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Confirmed,
    DurabilityUnconfirmed(String),
}

impl SaveOutcome {
    pub fn warning(&self) -> Option<&str> {
        match self {
            Self::Confirmed => None,
            Self::DurabilityUnconfirmed(message) => Some(message),
        }
    }

    /// Compatibility for non-interactive callers. The message explicitly says
    /// that the new package is present; callers must not roll publication back.
    pub fn require_confirmation(self) -> anyhow::Result<()> {
        match self {
            Self::Confirmed => Ok(()),
            Self::DurabilityUnconfirmed(message) => anyhow::bail!(message),
        }
    }
}

pub fn confirm_publication(destination: &Path, parent: &Path) -> SaveOutcome {
    #[cfg(any(test, feature = "ui-test"))]
    if let Some(result) = publication_test_fault(destination) {
        return publication_outcome(destination, result);
    }
    publication_outcome(destination, sync_directory(parent))
}

/// A private wrapper makes the publication cross-directory even on one
/// filesystem. Flush both the source wrapper and destination parent; a failed
/// flush after rename reports publication truthfully and retains prior data.
pub fn confirm_package_publication(
    destination: &Path,
    parent: &Path,
    wrapper: &Path,
) -> SaveOutcome {
    let source_result = sync_directory(wrapper);
    let destination_result = confirm_publication(destination, parent);
    match (source_result, destination_result) {
        (Ok(()), outcome) => outcome,
        (Err(error), outcome) => SaveOutcome::DurabilityUnconfirmed(format!(
            "{} was written, but its staging parent could not be flushed ({error}). Durability is unconfirmed. Keep recovery data and save a copy if needed.{}",
            destination.display(),
            outcome
                .warning()
                .map(|message| format!(" {message}"))
                .unwrap_or_default()
        )),
    }
}

#[cfg(any(test, feature = "ui-test"))]
fn publication_faults()
-> &'static std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, io::ErrorKind>> {
    static FAULTS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, io::ErrorKind>>,
    > = std::sync::OnceLock::new();
    FAULTS.get_or_init(Default::default)
}

/// Match canonicalized save destinations before the destination itself exists.
/// In particular, Windows canonical parents carry a verbatim path prefix that
/// `absolute()` alone does not add to the injection key.
#[cfg(any(test, feature = "ui-test"))]
pub(crate) fn fault_destination_key(path: &Path) -> Option<std::path::PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    Some(
        absolute
            .parent()?
            .canonicalize()
            .ok()?
            .join(absolute.file_name()?),
    )
}

#[cfg(any(test, feature = "ui-test"))]
fn publication_test_fault(destination: &Path) -> Option<io::Result<()>> {
    let key = fault_destination_key(destination)?;
    publication_faults()
        .lock()
        .unwrap()
        .get(&key)
        .copied()
        .map(|kind| Err(io::Error::from(kind)))
}

/// Scoped, path-specific fault injection; absent from production builds. Unlike
/// a process environment switch this cannot affect another test's save path.
#[cfg(any(test, feature = "ui-test"))]
pub fn inject_publication_failure(path: &Path, kind: io::ErrorKind) -> PublicationFault {
    let path = fault_destination_key(path).unwrap();
    assert!(
        publication_faults()
            .lock()
            .unwrap()
            .insert(path.clone(), kind)
            .is_none()
    );
    PublicationFault(path)
}

#[cfg(any(test, feature = "ui-test"))]
pub struct PublicationFault(std::path::PathBuf);

#[cfg(any(test, feature = "ui-test"))]
impl Drop for PublicationFault {
    fn drop(&mut self) {
        publication_faults().lock().unwrap().remove(&self.0);
    }
}

fn publication_outcome(destination: &Path, result: io::Result<()>) -> SaveOutcome {
    match result {
        Ok(()) => SaveOutcome::Confirmed,
        // Several filesystems do not support directory fsync. Retrying cannot
        // make them support it. Do not classify real I/O/access errors this way.
        Err(error) if directory_sync_unsupported(&error) => SaveOutcome::Confirmed,
        Err(error) => SaveOutcome::DurabilityUnconfirmed(format!(
            "{} was written, but its folder could not be flushed to storage ({error}). \
             Durability is unconfirmed. Keep recovery data and retry saving, or save a copy to another folder.",
            destination.display()
        )),
    }
}

fn directory_sync_unsupported(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        return matches!(error.raw_os_error(), Some(22 | 95));
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        false
    }
}

/// Flush a file or directory, including its metadata, to stable storage.
pub fn sync_path(path: impl AsRef<Path>) -> io::Result<()> {
    open_for_sync(path)?.sync_all()
}

/// Flush a known directory. Only unsupported directory sync is tolerated;
/// files and genuine access/I/O failures keep their strict durability contract.
pub fn sync_directory(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    let directory = open_for_sync(path)?;
    if !directory.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a directory",
        ));
    }
    #[cfg(any(test, feature = "ui-test"))]
    let result = directory_test_fault(path).unwrap_or_else(|| directory.sync_all());
    #[cfg(not(any(test, feature = "ui-test")))]
    let result = directory.sync_all();
    match result {
        Err(error) if directory_sync_unsupported(&error) => Ok(()),
        result => result,
    }
}

#[cfg(any(test, feature = "ui-test"))]
#[derive(Clone, Copy)]
enum DirectorySyncError {
    Os(i32),
    Kind(io::ErrorKind),
}

#[cfg(any(test, feature = "ui-test"))]
fn directory_faults()
-> &'static std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, DirectorySyncError>> {
    static FAULTS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, DirectorySyncError>>,
    > = std::sync::OnceLock::new();
    FAULTS.get_or_init(Default::default)
}

#[cfg(any(test, feature = "ui-test"))]
fn directory_test_fault(path: &Path) -> Option<io::Result<()>> {
    let absolute = path.canonicalize().ok()?;
    let faults = directory_faults().lock().unwrap();
    let (_, error) = faults
        .iter()
        .filter(|(scope, _)| absolute.starts_with(scope))
        .max_by_key(|(scope, _)| scope.components().count())?;
    Some(Err(match *error {
        DirectorySyncError::Os(code) => io::Error::from_raw_os_error(code),
        DirectorySyncError::Kind(kind) => io::Error::from(kind),
    }))
}

/// Inject only directory sync errors beneath one unique test-owned root. File
/// fsync, directory opens and unrelated concurrent saves remain unaffected.
#[cfg(any(test, feature = "ui-test"))]
pub fn inject_directory_sync_failure(scope: &Path, error: io::Error) -> DirectorySyncFault {
    let scope = scope.canonicalize().unwrap();
    let error = match error.raw_os_error() {
        Some(code) => DirectorySyncError::Os(code),
        None => DirectorySyncError::Kind(error.kind()),
    };
    assert!(
        directory_faults()
            .lock()
            .unwrap()
            .insert(scope.clone(), error)
            .is_none()
    );
    DirectorySyncFault(scope)
}

#[cfg(any(test, feature = "ui-test"))]
pub struct DirectorySyncFault(std::path::PathBuf);

#[cfg(any(test, feature = "ui-test"))]
impl Drop for DirectorySyncFault {
    fn drop(&mut self) {
        directory_faults().lock().unwrap().remove(&self.0);
    }
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

/// A failed replacement normally leaves the old destination in place. If the
/// rollback also fails, both complete packages remain at explicit recovery paths.
#[derive(Debug)]
pub enum ExchangeError {
    Move(io::Error),
    RolledBack {
        destination: std::path::PathBuf,
        previous_root: fs::Metadata,
        publication: io::Error,
    },
    RollbackFailed {
        previous: std::path::PathBuf,
        staged: std::path::PathBuf,
        publication: io::Error,
        rollback: io::Error,
    },
}
impl From<io::Error> for ExchangeError {
    fn from(error: io::Error) -> Self {
        Self::Move(error)
    }
}
impl std::fmt::Display for ExchangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Move(error) => write!(formatter, "{error}"),
            Self::RolledBack { publication, .. } => write!(
                formatter,
                "New package was not published ({publication}); the previous package was restored at its original destination"
            ),
            Self::RollbackFailed {
                previous,
                staged,
                publication,
                rollback,
            } => write!(
                formatter,
                "New package was not published ({publication}); restoring the previous destination also failed ({rollback}). The complete previous package is preserved at {} and the staged new package at {}. Reopen a recovery package or save a copy; the original destination may be absent.",
                previous.display(),
                staged.display()
            ),
        }
    }
}
impl std::error::Error for ExchangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Move(error) => error,
            Self::RollbackFailed { publication, .. } | Self::RolledBack { publication, .. } => {
                publication
            }
        })
    }
}

/// Swap two existing directories and return the actual previous-package path.
/// Windows uses three moves; after publication a failed final move leaves the
/// complete old package at its hidden sibling. Never delete that fallback.
pub fn exchange_dirs(from: &Path, to: &Path) -> Result<std::path::PathBuf, ExchangeError> {
    #[cfg(windows)]
    {
        exchange_with(from, to, move_no_replace)
    }
    #[cfg(not(windows))]
    {
        let _ = (from, to);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Package exchange is not implemented on this platform",
        )
        .into())
    }
}

#[cfg(any(windows, test, feature = "ui-test"))]
fn exchange_with(
    from: &Path,
    to: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<std::path::PathBuf, ExchangeError> {
    for path in [from, to] {
        if !fs::symlink_metadata(path)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a directory", path.display()),
            )
            .into());
        }
    }
    let previous_root = fs::symlink_metadata(to)?;
    let name = to
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Package path has no name"))?;
    let aside = to.with_file_name(format!(
        ".{}.previous-{}",
        name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    rename(to, &aside)?;
    if let Err(publication) = rename(from, to) {
        return match rename(&aside, to) {
            Ok(()) => Err(ExchangeError::RolledBack {
                destination: to.to_owned(),
                previous_root,
                publication,
            }),
            Err(rollback) => Err(ExchangeError::RollbackFailed {
                previous: aside,
                staged: from.to_owned(),
                publication,
                rollback,
            }),
        };
    }
    Ok(if rename(&aside, from).is_ok() {
        from.to_owned()
    } else {
        aside
    })
}

#[cfg(any(test, feature = "ui-test"))]
fn emulated_exchange_faults()
-> &'static std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Vec<usize>>> {
    static FAULTS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Vec<usize>>>,
    > = std::sync::OnceLock::new();
    FAULTS.get_or_init(Default::default)
}

/// Exercise the exact portable Windows move state machine on test-owned paths.
/// The production Linux atomic exchange never uses this hook without a fault.
#[cfg(any(test, feature = "ui-test"))]
pub fn emulated_exchange(
    from: &Path,
    to: &Path,
) -> Option<Result<std::path::PathBuf, ExchangeError>> {
    let key = fault_destination_key(to)?;
    let failures = emulated_exchange_faults()
        .lock()
        .unwrap()
        .get(&key)
        .cloned()?;
    let mut step = 0usize;
    Some(exchange_with(from, to, |source, destination| {
        step += 1;
        if failures.contains(&step) {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            rename_no_replace(source, destination)
        }
    }))
}

#[cfg(any(test, feature = "ui-test"))]
pub fn inject_emulated_exchange_failure(
    path: &Path,
    failed_steps: &[usize],
) -> EmulatedExchangeFault {
    let key = fault_destination_key(path).unwrap();
    assert!(
        emulated_exchange_faults()
            .lock()
            .unwrap()
            .insert(key.clone(), failed_steps.to_vec())
            .is_none()
    );
    EmulatedExchangeFault(key)
}
#[cfg(any(test, feature = "ui-test"))]
pub struct EmulatedExchangeFault(std::path::PathBuf);
#[cfg(any(test, feature = "ui-test"))]
impl Drop for EmulatedExchangeFault {
    fn drop(&mut self) {
        emulated_exchange_faults().lock().unwrap().remove(&self.0);
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
    fn publication_classifies_unsupported_sync_separately_from_io_failure() {
        let path = Path::new("Art.omuse");
        assert_eq!(publication_outcome(path, Ok(())), SaveOutcome::Confirmed);
        assert_eq!(
            publication_outcome(path, Err(io::Error::from(io::ErrorKind::Unsupported))),
            SaveOutcome::Confirmed
        );
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::Other] {
            let outcome = publication_outcome(path, Err(io::Error::from(kind)));
            let warning = outcome.warning().unwrap();
            assert!(warning.contains("was written") && warning.contains("unconfirmed"));
            assert!(outcome.require_confirmation().is_err());
        }
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                publication_outcome(path, Err(io::Error::from_raw_os_error(22))),
                SaveOutcome::Confirmed
            );
            assert!(
                publication_outcome(path, Err(io::Error::from_raw_os_error(5)))
                    .warning()
                    .is_some()
            );
        }
    }

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
    fn unsupported_directory_sync_permits_document_and_create_publication() {
        use crate::{create_project::Project, document, model::Document};
        let temp = tempfile::tempdir().unwrap();
        let _fault = inject_directory_sync_failure(temp.path(), io::ErrorKind::Unsupported.into());
        let document = Document::new(8, 8);
        let ordinary = temp.path().join("Art.omuse");
        assert_eq!(
            document::save_checked_report(&document, &ordinary, || Ok(())).unwrap(),
            SaveOutcome::Confirmed
        );
        let mut project = Project::new("Collection", document);
        let create = temp.path().join("Collection.omuse");
        assert_eq!(
            project.save_checked_report(&create, || Ok(())).unwrap(),
            SaveOutcome::Confirmed
        );
        assert!(ordinary.join("manifest.json").is_file());
        assert!(create.join("project.json").is_file());
        let file = temp.path().join("regular-file");
        fs::write(&file, b"strict file fsync").unwrap();
        assert_eq!(
            sync_directory(&file).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        sync_path(&file).unwrap();
    }

    #[test]
    fn genuine_directory_io_failure_aborts_before_publication_and_keeps_old_packages() {
        use crate::{create_project::Project, document, model::Document};
        let temp = tempfile::tempdir().unwrap();
        let ordinary = temp.path().join("Art.omuse");
        let create = temp.path().join("Collection.omuse");
        let mut artwork = Document::new(8, 8);
        artwork.name = "Before".into();
        document::save(&artwork, &ordinary).unwrap();
        let mut project = Project::new("Before", artwork.clone());
        project.save(&create).unwrap();
        let old_document = fs::read(ordinary.join("manifest.json")).unwrap();
        let old_create = fs::read(create.join("project.json")).unwrap();
        let old_stamp = crate::save_guard::package_stamp(&create).unwrap();
        artwork.name = "After".into();
        project.title = "After".into();
        #[cfg(unix)]
        let error = io::Error::from_raw_os_error(5); // EIO, not unsupported.
        #[cfg(not(unix))]
        let error = io::Error::from(io::ErrorKind::Other);
        let fault = inject_directory_sync_failure(temp.path(), error);
        let mut document_published = false;
        assert!(
            document::save_checked_report(&artwork, &ordinary, || {
                document_published = true;
                Ok(())
            })
            .is_err()
        );
        let mut create_published = false;
        assert!(
            project
                .save_checked_report(&create, || {
                    create_published = true;
                    Ok(())
                })
                .is_err()
        );
        assert!(!document_published && !create_published);
        assert_eq!(
            fs::read(ordinary.join("manifest.json")).unwrap(),
            old_document
        );
        assert_eq!(fs::read(create.join("project.json")).unwrap(), old_create);
        assert_eq!(crate::save_guard::package_stamp(&create), Some(old_stamp));
        let mut names: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                std::ffi::OsString::from("Art.omuse"),
                std::ffi::OsString::from("Collection.omuse")
            ]
        );
        drop(fault);
        assert_eq!(
            project.save_checked_report(&create, || Ok(())).unwrap(),
            SaveOutcome::Confirmed
        );
    }

    #[test]
    fn scoped_fault_keys_match_canonical_save_paths_before_destination_creation() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().canonicalize().unwrap();
        let target = temp.path().join(".").join("not-created.omuse");
        assert_eq!(
            fault_destination_key(&target),
            Some(parent.join("not-created.omuse"))
        );
        let fault = inject_publication_failure(&target, io::ErrorKind::Other);
        assert!(
            confirm_publication(&parent.join("not-created.omuse"), &parent)
                .warning()
                .is_some()
        );
        drop(fault);
        let fault =
            inject_directory_sync_failure(&temp.path().join("."), io::ErrorKind::Other.into());
        assert_eq!(
            sync_directory(&parent).unwrap_err().kind(),
            io::ErrorKind::Other
        );
        drop(fault);
        sync_directory(&parent).unwrap();
    }

    #[test]
    fn emulated_exchange_returns_actual_previous_path_after_third_move_failure() {
        let temp = tempfile::tempdir().unwrap();
        let (stage, target) = (temp.path().join("stage"), temp.path().join("target.omuse"));
        fs::create_dir(&stage).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(stage.join("marker"), b"new").unwrap();
        fs::write(target.join("marker"), b"old").unwrap();
        let _fault = inject_emulated_exchange_failure(&target, &[3]);
        let previous = emulated_exchange(&stage, &target).unwrap().unwrap();
        assert_ne!(previous, stage);
        assert_eq!(fs::read(previous.join("marker")).unwrap(), b"old");
        assert_eq!(fs::read(target.join("marker")).unwrap(), b"new");
        assert!(!stage.exists());
    }

    #[test]
    fn emulated_exchange_failed_publication_restores_old_or_exposes_both_recovery_paths() {
        for failed_steps in [&[2][..], &[2, 3][..]] {
            let temp = tempfile::tempdir().unwrap();
            let (stage, target) = (temp.path().join("stage"), temp.path().join("target.omuse"));
            fs::create_dir(&stage).unwrap();
            fs::create_dir(&target).unwrap();
            fs::write(stage.join("marker"), b"new").unwrap();
            fs::write(target.join("marker"), b"old").unwrap();
            let _fault = inject_emulated_exchange_failure(&target, failed_steps);
            let error = emulated_exchange(&stage, &target).unwrap().unwrap_err();
            assert_eq!(fs::read(stage.join("marker")).unwrap(), b"new");
            if failed_steps.len() == 1 {
                assert!(matches!(error, ExchangeError::RolledBack { .. }));
                assert_eq!(fs::read(target.join("marker")).unwrap(), b"old");
                assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
            } else {
                let ExchangeError::RollbackFailed {
                    previous, staged, ..
                } = &error
                else {
                    panic!("{error}");
                };
                assert_eq!(staged, &stage);
                assert_eq!(fs::read(previous.join("marker")).unwrap(), b"old");
                assert!(!target.exists());
                assert!(
                    error.to_string().contains("not published")
                        && error.to_string().contains("may be absent")
                );
            }
        }
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
