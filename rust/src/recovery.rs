//! Per-window recovery with one pending snapshot and advisory ownership locks.
//!
//! Recovery packages are never user save targets. UUID names avoid PID reuse and
//! same-process window collisions. A worker holds its lock until every accepted
//! write/clear finishes; discovery returns only unlocked packages and retains them.
use omuse::{document, model::Document};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const DEBOUNCE: Duration = Duration::from_millis(150);

struct State {
    /// There can be one in-flight save and at most one newer pending snapshot.
    pending: Option<RecoverySnapshot>,
    clear_requested: Option<u64>,
    generation: u64,
    clear_completed: u64,
    /// Set when scheduling; cleared only after an acknowledged successful removal.
    scheduled: bool,
    in_flight: bool,
    shutdown: bool,
    alive: bool,
    error: Option<String>,
    #[cfg(test)]
    pause_next_save: Option<Arc<std::sync::Barrier>>,
    #[cfg(test)]
    pause_before_format_exchange: Option<FormatExchangePause>,
}

#[cfg(test)]
struct FormatExchangePause {
    staged: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Holding this descriptor holds a Linux flock (a `LockFileEx` lock on Windows),
/// including against another open descriptor in the same process. The lock
/// releases automatically on crash/reboot.
struct Ownership {
    _file: File,
}
impl Ownership {
    #[cfg(target_os = "linux")]
    fn acquire(path: &Path) -> io::Result<Option<Self>> {
        use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
        // O_NOFOLLOW prevents a damaged/malicious lock symlink from redirecting
        // ownership checks. Discovery never truncates or writes an existing lock.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(0o400000)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Recovery lock is not a regular file",
            ));
        }
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        loop {
            // SAFETY: the descriptor belongs to `file` and stays open throughout
            // this call. LOCK_EX | LOCK_NB is nonblocking exclusive ownership.
            if unsafe { flock(file.as_raw_fd(), 2 | 4) } == 0 {
                return Ok(Some(Self { _file: file }));
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(None),
                _ => return Err(error),
            }
        }
    }
    /// Windows `LockFileEx` locks are per handle as well, so a second handle in
    /// the same process is refused, and they are released when the process ends.
    #[cfg(not(target_os = "linux"))]
    fn acquire(path: &Path) -> io::Result<Option<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Recovery lock is not a regular file",
            ));
        }
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(error),
        }
    }
}

pub struct Recovery {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    discovery_roots: Vec<PathBuf>,
    path: PathBuf,
}
impl Recovery {
    pub fn new() -> Self {
        Self::at_roots(
            omuse::identity::data_dir().join("recovery"),
            vec![omuse::identity::legacy_data_dir().join("recovery")],
        )
    }

    pub(crate) fn at(root: PathBuf) -> Self {
        Self::at_roots(root, Vec::new())
    }

    fn at_roots(root: PathBuf, mut legacy_roots: Vec<PathBuf>) -> Self {
        legacy_roots.retain(|candidate| candidate != &root);
        let mut discovery_roots = vec![root.clone()];
        discovery_roots.extend(legacy_roots);
        let path = root.join(format!("session-{}.omuse", uuid::Uuid::new_v4()));
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pending: None,
                clear_requested: None,
                generation: 0,
                clear_completed: 0,
                scheduled: false,
                in_flight: false,
                shutdown: false,
                alive: false,
                error: None,
                #[cfg(test)]
                pause_next_save: None,
                #[cfg(test)]
                pause_before_format_exchange: None,
            }),
            changed: Condvar::new(),
        });
        let owner = fs::create_dir_all(&root)
            .and_then(|_| Ownership::acquire(&path.with_extension("lock")))
            .and_then(|owner| {
                owner.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Recovery session is already owned",
                    )
                })
            });
        let mut recovery = Self {
            shared,
            worker: None,
            discovery_roots,
            path,
        };
        match owner {
            Ok(owner) => {
                let shared = recovery.shared.clone();
                let path = recovery.path.clone();
                shared.lock().alive = true;
                match thread::Builder::new()
                    .name("omuse-recovery".into())
                    .spawn(move || {
                        // Keep ownership through the final write and panic handling.
                        let _owner = owner;
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            writer(&shared, &path)
                        }));
                        let mut state = shared.lock();
                        if result.is_err() {
                            state.error = Some("Recovery writer stopped unexpectedly".into());
                        }
                        state.alive = false;
                        shared.changed.notify_all();
                    }) {
                    Ok(worker) => recovery.worker = Some(worker),
                    Err(error) => {
                        let mut state = recovery.shared.lock();
                        state.alive = false;
                        state.error = Some(format!("Recovery writer could not start: {error}"));
                    }
                }
            }
            Err(error) => {
                recovery.shared.lock().error = Some(format!("Recovery is unavailable: {error}"))
            }
        }
        recovery
    }

    pub fn schedule(&self, doc: &Document, dirty: bool) {
        if !dirty {
            self.clear();
            return;
        }
        self.schedule_snapshot(RecoverySnapshot::Document(doc.clone()), dirty);
    }

    #[cfg(test)]
    pub fn schedule_project(&self, project: &omuse::create_project::Project, dirty: bool) {
        self.schedule_snapshot(RecoverySnapshot::Project(project.clone()), dirty);
    }

    pub fn schedule_project_owned(&self, project: omuse::create_project::Project, dirty: bool) {
        self.schedule_snapshot(RecoverySnapshot::Project(project), dirty);
    }

    /// Test/benchmark synchronization only. Unlike clear, this preserves the
    /// acknowledged recovery package and changes none of the worker's timing.
    #[cfg(test)]
    pub(crate) fn wait_idle_for_test(&self, timeout: Duration) -> anyhow::Result<PathBuf> {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.lock();
        while state.pending.is_some() || state.in_flight || state.clear_requested.is_some() {
            anyhow::ensure!(state.alive, "Recovery writer is unavailable");
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| anyhow::anyhow!("Timed out waiting for recovery"))?;
            state = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        anyhow::ensure!(state.error.is_none(), "Recovery failed: {:?}", state.error);
        Ok(self.path.clone())
    }

    fn schedule_snapshot(&self, snapshot: RecoverySnapshot, dirty: bool) {
        if !dirty {
            self.clear();
            return;
        }
        let mut state = self.shared.lock();
        if !state.alive || state.shutdown {
            return;
        }
        // Replacing this slot drops the superseded snapshot; a burst cannot grow
        // an unbounded queue or hold every historical version of the document.
        state.pending = Some(snapshot);
        state.scheduled = true;
        self.shared.changed.notify_all();
    }

    pub fn clear(&self) {
        let mut state = self.shared.lock();
        if !state.scheduled
            && !state.in_flight
            && state.pending.is_none()
            && state.clear_requested.is_none()
        {
            return;
        }
        if !state.alive {
            state.error.get_or_insert_with(|| {
                "Recovery writer is unavailable; previous recovery was retained".into()
            });
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.pending = None;
        state.clear_requested = Some(generation);
        self.shared.changed.notify_all();
        // No timeout may falsely acknowledge a clear while an older save can
        // still publish. The worker serializes clear after any in-flight save.
        while state.clear_completed < generation && state.alive {
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn error(&self) -> Option<String> {
        self.shared.lock().error.clone()
    }

    pub fn available(&self) -> Option<PathBuf> {
        let mut candidates = Vec::new();
        for root in &self.discovery_roots {
            let entries = match fs::read_dir(root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    self.shared.lock().error = Some(format!(
                        "Recovery discovery failed for {}: {error}",
                        root.display()
                    ));
                    continue;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path == self.path || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                if !document::is_omuse_path(&path)
                    && !path.extension().is_some_and(|extension| {
                        extension.as_encoded_bytes().eq_ignore_ascii_case(b"comp")
                    })
                {
                    continue;
                }
                let Some(session) = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("session-"))
                else {
                    continue;
                };
                // Also retain/discover historical PID-named packages. No PID existence
                // inference: a reused PID says nothing about ownership of old pixels.
                if uuid::Uuid::parse_str(session).is_err() && session.parse::<u32>().is_err() {
                    continue;
                }
                if !["manifest.json", "project.json"].iter().any(|name| {
                    fs::symlink_metadata(path.join(name)).is_ok_and(|m| m.file_type().is_file())
                }) {
                    continue;
                }
                match Ownership::acquire(&path.with_extension("lock")) {
                    Ok(Some(_unowned)) => {
                        candidates.push((entry.metadata().and_then(|m| m.modified()).ok(), path))
                    }
                    Ok(None) => {} // Another window/process is actively writing it.
                    Err(error) => {
                        self.shared.lock().error =
                            Some(format!("Recovery ownership check failed: {error}"))
                    }
                }
            }
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates.pop().map(|(_, path)| path)
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.shutdown = true;
            self.shared.changed.notify_all();
        }
        // Flush only the most recent pending snapshot. Ownership does not become
        // available to a new window while the final save is still being written.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

enum RecoverySnapshot {
    Document(Document),
    Project(omuse::create_project::Project),
}
enum Work {
    Save(RecoverySnapshot),
    Clear(u64),
}

fn recovery_publish_is_current(shared: &Shared) -> anyhow::Result<()> {
    // A clear can arrive while a full package is being staged.  Do not exchange
    // that obsolete snapshot into the recovery location just before clear
    // removes it; the save callback leaves the prior package intact and lets
    // the queued clear acknowledge the discard normally.  Shutdown is
    // deliberately not checked here because Drop must flush its final snapshot.
    anyhow::ensure!(
        shared.lock().clear_requested.is_none(),
        "Recovery was cleared before the staged snapshot could be published"
    );
    Ok(())
}

fn save_snapshot_at(
    snapshot: RecoverySnapshot,
    shared: &Shared,
    path: &Path,
) -> anyhow::Result<()> {
    match snapshot {
        RecoverySnapshot::Document(doc) => {
            document::save_checked(&doc, path, || recovery_publish_is_current(shared))
        }
        RecoverySnapshot::Project(mut project) => {
            project.save_checked(path, || recovery_publish_is_current(shared))
        }
    }
}

struct RecoveryStage(PathBuf);
impl Drop for RecoveryStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn save_snapshot(snapshot: RecoverySnapshot, shared: &Shared, path: &Path) -> anyhow::Result<()> {
    let changes_format = match &snapshot {
        RecoverySnapshot::Document(_) => path.join("project.json").is_file(),
        RecoverySnapshot::Project(_) => path.join("manifest.json").is_file(),
    };
    if !changes_format {
        return save_snapshot_at(snapshot, shared, path);
    }

    // A photo can become a Create collection in the same window. Both package
    // writers correctly refuse to overwrite another format at a user-selected
    // save path, but this UUID destination belongs exclusively to this worker.
    // Stage the new format separately and exchange only after it is complete:
    // clearing the old recovery first would lose it if conversion failed.
    anyhow::ensure!(
        fs::symlink_metadata(path)?.file_type().is_dir(),
        "Recovery destination must be a directory"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Recovery directory is unavailable"))?;
    let stage_path = parent.join(format!(".omuse-recovery-stage-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage_path)?;
    let stage = RecoveryStage(stage_path);
    let prepared = stage.0.join("snapshot.omuse");
    save_snapshot_at(snapshot, shared, &prepared)?;
    #[cfg(test)]
    {
        let pause = shared.lock().pause_before_format_exchange.take();
        if let Some(pause) = pause {
            let _ = pause.staged.send(());
            // Dropping the test's sender also releases the worker, so a failed
            // assertion cannot leave Recovery::drop waiting on this hook.
            let _ = pause.resume.recv();
        }
    }
    recovery_publish_is_current(shared)?;
    exchange_recovery(&prepared, path)?;
    // Publication has succeeded; match the package writers' best-effort
    // parent sync rather than reporting a completed exchange as an old save.
    let _ = omuse::durable_fs::sync_path(parent);
    Ok(())
}

#[cfg(target_os = "linux")]
fn exchange_recovery(from: &Path, to: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renameat2(
            olddirfd: i32,
            oldpath: *const std::ffi::c_char,
            newdirfd: i32,
            newpath: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: owned NUL-terminated paths live through the call; RENAME_EXCHANGE
    // swaps complete directories atomically on the same recovery filesystem.
    if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 2) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("Recovery format conversion failed; previous recovery retained");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn exchange_recovery(from: &Path, to: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    omuse::durable_fs::exchange_dirs(from, to)
        .context("Recovery format conversion failed; previous recovery retained")
}

fn writer(shared: &Shared, path: &Path) {
    loop {
        let work = {
            let mut state = shared.lock();
            while state.pending.is_none() && state.clear_requested.is_none() && !state.shutdown {
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
            if state.pending.is_some() && state.clear_requested.is_none() && !state.shutdown {
                let deadline = Instant::now() + DEBOUNCE;
                while state.clear_requested.is_none() && !state.shutdown {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    let (next, _) = shared
                        .changed
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(|e| e.into_inner());
                    state = next;
                }
            }
            if let Some(generation) = state.clear_requested.take() {
                Work::Clear(generation)
            } else if let Some(doc) = state.pending.take() {
                state.in_flight = true;
                Work::Save(doc)
            } else if state.shutdown {
                return;
            } else {
                continue;
            }
        };
        match work {
            Work::Save(doc) => {
                #[cfg(test)]
                {
                    let pause = shared.lock().pause_next_save.take();
                    if let Some(pause) = pause {
                        pause.wait();
                        pause.wait();
                    }
                }
                let result = save_snapshot(doc, shared, path);
                let mut state = shared.lock();
                state.in_flight = false;
                state.error = result
                    .err()
                    .map(|error| format!("Recovery write failed: {error:#}"));
                shared.changed.notify_all();
            }
            Work::Clear(generation) => {
                let result = match fs::remove_dir_all(path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
                .and_then(|()| {
                    // Persist the directory entry removal before acknowledging a
                    // discard, so a reboot cannot restore an older recovery name.
                    path.parent()
                        .map_or(Ok(()), |parent| omuse::durable_fs::sync_path(parent))
                });
                let mut state = shared.lock();
                if let Err(error) = result {
                    state.error = Some(format!("Recovery cleanup could not be completed: {error}"));
                } else {
                    state.scheduled = state.pending.is_some();
                    state.error = None;
                }
                state.clear_completed = generation;
                shared.changed.notify_all();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "Timed out waiting for recovery");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn clean_clear_does_not_schedule_work_or_wait_for_worker() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        for _ in 0..20 {
            recovery.clear();
        }
        let state = recovery.shared.lock();
        assert_eq!(
            state.generation, 0,
            "clean clear must not request acknowledgments"
        );
        assert!(state.clear_requested.is_none() && state.pending.is_none());
    }

    #[test]
    fn same_process_windows_have_unique_paths_and_skip_live_owners() {
        let temp = tempfile::tempdir().unwrap();
        let first = Recovery::at(temp.path().to_owned());
        let second = Recovery::at(temp.path().to_owned());
        assert_ne!(first.path, second.path);
        assert_eq!(first.path.extension().unwrap(), "omuse");
        assert_eq!(second.path.extension().unwrap(), "omuse");
        first.schedule(&Document::new(8, 8), true);
        second.schedule(&Document::new(8, 8), true);
        wait_until(|| {
            first.path.join("manifest.json").is_file()
                && second.path.join("manifest.json").is_file()
        });
        assert!(first.available().is_none());
        assert!(second.available().is_none());
        let first_path = first.path.clone();
        drop(first);
        assert_eq!(second.available(), Some(first_path));
    }

    #[test]
    fn stale_pid_named_recovery_survives_pid_reuse_and_clear() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp
            .path()
            .join(format!("session-{}.comp", std::process::id()));
        document::save(&Document::new(8, 8), &legacy).unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        assert_eq!(recovery.available(), Some(legacy.clone()));
        let owner = Ownership::acquire(&legacy.with_extension("lock"))
            .unwrap()
            .unwrap();
        assert!(
            recovery.available().is_none(),
            "advisory ownership beats filename/PID guesses"
        );
        drop(owner);
        recovery.schedule(&Document::new(12, 9), true);
        recovery.clear();
        assert_eq!(recovery.available(), Some(legacy.clone()));
        assert!(
            legacy.join("manifest.json").is_file(),
            "clear must retain previously discovered recovery"
        );
    }

    #[test]
    fn canonical_writer_discovers_unlocked_legacy_packages_but_skips_live_ones() {
        let temp = tempfile::tempdir().unwrap();
        let canonical_root = temp.path().join("omuse/recovery");
        let legacy_root = temp.path().join("compositor-rust/recovery");
        let active = Recovery::at(legacy_root.clone());
        active.schedule(&Document::new(8, 8), true);
        wait_until(|| active.path.join("manifest.json").is_file());

        let stale = legacy_root.join("session-4242.comp");
        document::save(&Document::new(12, 9), &stale).unwrap();
        let observer = Recovery::at_roots(canonical_root.clone(), vec![legacy_root.clone()]);
        assert!(observer.path.starts_with(&canonical_root));
        assert_eq!(observer.available(), Some(stale.clone()));
        assert!(stale.join("manifest.json").is_file());
        assert!(active.path.join("manifest.json").is_file());

        let owner = Ownership::acquire(&stale.with_extension("lock"))
            .unwrap()
            .unwrap();
        assert!(observer.available().is_none());
        drop(owner);
        assert_eq!(observer.available(), Some(stale));
    }

    #[test]
    fn mixed_omuse_and_legacy_recovery_discovery_retains_ownership_checks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = [
            temp.path()
                .join(format!("session-{}.omuse", uuid::Uuid::new_v4())),
            temp.path()
                .join(format!("session-{}.OMUSE", uuid::Uuid::new_v4())),
            temp.path()
                .join(format!("session-{}.comp", uuid::Uuid::new_v4())),
            temp.path().join("session-4242.comp"),
        ];
        let mut owners = Vec::new();
        for path in &paths {
            document::save(&Document::new(8, 8), path).unwrap();
            owners.push(Ownership::acquire(&path.with_extension("lock")).unwrap());
            assert!(owners.last().unwrap().is_some());
        }
        for name in [
            "session-not-a-session.omuse",
            "artwork.omuse",
            "session-4243.png",
        ] {
            document::save(&Document::new(8, 8), &temp.path().join(name)).unwrap();
        }
        let observer = Recovery::at(temp.path().to_owned());
        assert!(observer.available().is_none());
        for (path, owner) in paths.iter().zip(&mut owners) {
            drop(owner.take());
            assert_eq!(observer.available(), Some(path.clone()));
            *owner = Ownership::acquire(&path.with_extension("lock")).unwrap();
            assert!(owner.is_some());
            assert!(observer.available().is_none());
            assert!(document::open(path).is_ok());
        }
        observer.clear();
        assert!(
            paths
                .iter()
                .all(|path| path.join("manifest.json").is_file())
        );
    }

    #[test]
    fn bounded_pending_snapshot_and_clear_cannot_resurrect_discarded_work() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut doc = Document::new(32, 24);
        for revision in 0..100 {
            doc.layers[0].name = format!("Revision {revision}");
            recovery.schedule(&doc, true);
        }
        recovery.clear();
        assert!(!recovery.path.exists());
        thread::sleep(DEBOUNCE * 2);
        assert!(
            !recovery.path.exists(),
            "queued old save resurrected after clear acknowledgment"
        );
        {
            let state = recovery.shared.lock();
            assert!(state.pending.is_none() && !state.in_flight && !state.scheduled);
        }
        recovery.schedule(&doc, true);
        wait_until(|| recovery.path.join("manifest.json").is_file());
        assert_eq!(
            document::open(&recovery.path).unwrap().layers[0].name,
            "Revision 99"
        );
        recovery.clear();
        assert!(!recovery.path.exists());
    }

    #[test]
    fn clear_acknowledges_an_actual_in_flight_write_before_returning() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let barrier = Arc::new(std::sync::Barrier::new(2));
        recovery.shared.lock().pause_next_save = Some(barrier.clone());
        recovery.schedule(&Document::new(8, 8), true);
        barrier.wait(); // Worker has taken the snapshot; the write is in flight.
        assert!(recovery.shared.lock().in_flight);
        recovery.schedule(&Document::new(12, 12), true); // one newer queued snapshot
        thread::scope(|scope| {
            let clear = scope.spawn(|| recovery.clear());
            wait_until(|| recovery.shared.lock().clear_requested.is_some());
            barrier.wait(); // The old save reaches its publish check after clear was requested.
            clear.join().unwrap();
        });
        assert!(!recovery.path.exists());
        thread::sleep(DEBOUNCE * 2);
        assert!(!recovery.path.exists());
        assert!(recovery.shared.lock().pending.is_none());
    }

    #[test]
    fn clear_during_collection_conversion_does_not_publish_or_leave_a_draft() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let photo = Document::new(8, 8);
        recovery.schedule(&photo, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        let mut project = omuse::create_project::Project::new("Campaign", photo);
        project.add_blank_page("Second", 12, 9).unwrap();

        let barrier = Arc::new(std::sync::Barrier::new(2));
        recovery.shared.lock().pause_next_save = Some(barrier.clone());
        recovery.schedule_project(&project, true);
        barrier.wait();
        thread::scope(|scope| {
            let clear = scope.spawn(|| recovery.clear());
            wait_until(|| recovery.shared.lock().clear_requested.is_some());
            barrier.wait();
            clear.join().unwrap();
        });
        drop(recovery);
        assert!(!path.exists());
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse")
        }));
    }

    #[test]
    fn concurrent_clear_waiters_all_receive_acknowledgments() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        recovery.schedule(&Document::new(8, 8), true);
        thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| recovery.clear());
            }
        });
        assert!(!recovery.path.exists());
    }

    #[test]
    fn ownership_child_process() {
        let Some(root) = omuse::identity::env_var_os("OMUSE_RECOVERY_LOCK_TEST") else {
            return;
        };
        let root = PathBuf::from(root);
        let _owner =
            Ownership::acquire(&root.join("session-11111111-1111-4111-8111-111111111111.lock"))
                .unwrap()
                .unwrap();
        fs::write(root.join("child-ready"), b"locked").unwrap();
        loop {
            thread::park();
        }
    }

    #[test]
    fn crashed_process_releases_ownership_and_preserves_recovery() {
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("session-11111111-1111-4111-8111-111111111111.omuse");
        document::save(&Document::new(8, 8), &path).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "recovery::tests::ownership_child_process",
                "--nocapture",
            ])
            .env("OMUSE_RECOVERY_LOCK_TEST", temp.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let child = Child(child);
        wait_until(|| temp.path().join("child-ready").exists());
        let observer = Recovery::at(temp.path().to_owned());
        assert!(observer.available().is_none());
        drop(child); // Simulate process death, without cleanly dropping its owner.
        assert_eq!(observer.available(), Some(path.clone()));
        assert!(document::open(&path).is_ok());
    }

    #[test]
    fn drop_flushes_only_latest_snapshot_and_releases_lock() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let path = recovery.path.clone();
        let mut doc = Document::new(8, 8);
        recovery.schedule(&doc, true);
        doc.layers[0].name = "Latest".into();
        recovery.schedule(&doc, true);
        drop(recovery);
        let observer = Recovery::at(temp.path().to_owned());
        assert_eq!(observer.available(), Some(path.clone()));
        assert_eq!(document::open(&path).unwrap().layers[0].name, "Latest");
    }

    #[test]
    fn errors_are_visible_and_successful_operations_clear_them() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut unsupported = Document::new(8, 8);
        unsupported.layers[0].metadata["adjustment"] = serde_json::json!({"kind": "unsupported"});
        recovery.schedule(&unsupported, true);
        wait_until(|| recovery.error().is_some());
        assert!(recovery.error().unwrap().contains("Recovery write failed"));
        recovery.schedule(&Document::new(8, 8), true);
        wait_until(|| recovery.path.join("manifest.json").is_file() && recovery.error().is_none());
        recovery.clear();
        assert!(recovery.error().is_none());
        let invalid_root = temp.path().join("regular-file");
        fs::write(&invalid_root, b"not a directory").unwrap();
        let disabled = Recovery::at(invalid_root);
        assert!(disabled.error().unwrap().contains("unavailable"));
        disabled.clear();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn symlinked_lock_is_not_followed() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("important-file");
        fs::write(&target, b"Keep these bytes").unwrap();
        let lock = temp.path().join("bad.lock");
        std::os::unix::fs::symlink(&target, &lock).unwrap();
        assert!(Ownership::acquire(&lock).is_err());
        assert_eq!(fs::read(target).unwrap(), b"Keep these bytes");
    }
}

#[cfg(test)]
mod create_recovery_tests {
    use super::*;

    fn pause_staged_conversion(
        recovery: &Recovery,
        project: &omuse::create_project::Project,
    ) -> std::sync::mpsc::Sender<()> {
        let (staged, ready) = std::sync::mpsc::channel();
        let (resume, receiver) = std::sync::mpsc::channel();
        recovery.shared.lock().pause_before_format_exchange = Some(FormatExchangePause {
            staged,
            resume: receiver,
        });
        recovery.schedule_project(project, true);
        ready
            .recv_timeout(Duration::from_secs(5))
            .expect("Conversion did not reach the completed staging boundary");
        resume
    }

    fn staged_snapshot(root: &Path) -> PathBuf {
        let stages: Vec<_> = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".omuse-recovery-stage-")
            })
            .collect();
        assert_eq!(stages.len(), 1);
        stages[0].join("snapshot.omuse")
    }

    fn assert_recovered_artwork(actual: &Document, expected: &Document) {
        assert_eq!(
            (actual.width, actual.height),
            (expected.width, expected.height)
        );
        assert_eq!(actual.background, expected.background);
        assert_eq!(
            actual.metadata["documentID"],
            expected.metadata["documentID"]
        );
        assert_eq!(actual.layers.len(), expected.layers.len());
        for (actual, expected) in actual.layers.iter().zip(&expected.layers) {
            assert_eq!(actual.id, expected.id);
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.image, expected.image);
        }
    }

    #[test]
    fn converting_an_edited_photo_to_a_collection_replaces_its_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut photo = Document::new(8, 8);
        photo.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(2, 3, image::Rgba([18, 52, 86, 255]));
        recovery.schedule(&photo, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        assert!(path.join("manifest.json").is_file());

        let mut project = omuse::create_project::Project::new("Photo campaign", photo.clone());
        let first = project.active_page_id().to_owned();
        let second = project.add_blank_page("New card", 12, 9).unwrap();
        project.set_active_page(&second).unwrap();
        project.page_document_mut(&second).unwrap().layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(4, 5, image::Rgba([144, 120, 60, 255]));
        recovery.schedule_project(&project, true);
        recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        drop(recovery);

        let mut restored = omuse::create_project::Project::open(&path).unwrap();
        assert_eq!(restored.title, "Photo campaign");
        assert_eq!(restored.page_ids(), vec![first.clone(), second.clone()]);
        assert_eq!(restored.active_page_id(), second);
        for page in [&first, &second] {
            assert_recovered_artwork(
                restored.page_document(page).unwrap(),
                project.page_document(page).unwrap(),
            );
        }
        assert!(!path.join("manifest.json").exists());
        let observer = Recovery::at(temp.path().to_owned());
        assert_eq!(observer.available(), Some(path));
    }

    #[test]
    fn collection_recovery_can_be_replaced_by_a_photo_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut project = omuse::create_project::Project::new("Campaign", Document::new(8, 8));
        project.add_blank_page("Second", 12, 9).unwrap();
        recovery.schedule_project(&project, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();

        let mut photo = Document::new(9, 7);
        photo.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(3, 4, image::Rgba([66, 99, 132, 255]));
        recovery.schedule(&photo, true);
        recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        drop(recovery);

        assert!(!path.join("project.json").exists());
        assert_recovered_artwork(&document::open(&path).unwrap(), &photo);
    }

    #[test]
    fn a_failed_collection_conversion_retains_the_last_photo_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let photo = Document::new(8, 8);
        recovery.schedule(&photo, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        let previous = omuse::save_guard::package_stamp(&path).unwrap();

        let mut project = omuse::create_project::Project::new("Campaign", photo.clone());
        let second = project.add_blank_page("Invalid page", 12, 9).unwrap();
        // Fail after the first page has been staged, so recovery must retain
        // its old package while disposing of a partially prepared collection.
        project.page_document_mut(&second).unwrap().layers[0].blend_mode = "Invalid".into();
        recovery.schedule_project(&project, true);
        assert!(recovery.wait_idle_for_test(Duration::from_secs(5)).is_err());
        assert_eq!(omuse::save_guard::package_stamp(&path), Some(previous));
        assert_recovered_artwork(&document::open(&path).unwrap(), &photo);
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse")
        }));

        project.page_document_mut(&second).unwrap().layers[0].blend_mode = "Normal".into();
        recovery.schedule_project(&project, true);
        recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        assert!(recovery.error().is_none());
        assert_eq!(
            omuse::create_project::Project::open(&path)
                .unwrap()
                .page_ids(),
            project.page_ids()
        );
    }

    #[test]
    fn clear_after_collection_staging_prevents_the_final_format_exchange() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut photo = Document::new(8, 8);
        photo.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(2, 3, image::Rgba([18, 52, 86, 255]));
        recovery.schedule(&photo, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        let previous = omuse::save_guard::package_stamp(&path).unwrap();
        let mut project = omuse::create_project::Project::new("Campaign", photo.clone());
        project.add_blank_page("Second", 12, 9).unwrap();

        let resume = pause_staged_conversion(&recovery, &project);
        let prepared = staged_snapshot(temp.path());
        assert_eq!(
            omuse::create_project::Project::open(&prepared)
                .unwrap()
                .page_ids(),
            project.page_ids(),
            "the replacement must already be complete when clear arrives"
        );
        assert_eq!(omuse::save_guard::package_stamp(&path), Some(previous));
        assert_recovered_artwork(&document::open(&path).unwrap(), &photo);

        thread::scope(|scope| {
            // Keep this sender inside the scope so an assertion failure drops
            // it before the scope waits for the blocked clear thread.
            let resume = resume;
            let clear = scope.spawn(|| recovery.clear());
            let deadline = Instant::now() + Duration::from_secs(5);
            while recovery.shared.lock().clear_requested.is_none() {
                assert!(Instant::now() < deadline, "Clear request was not queued");
                thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(omuse::save_guard::package_stamp(&path), Some(previous));
            drop(resume);
            clear.join().unwrap();
        });

        recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        assert!(!path.exists());
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse")
        }));
    }

    #[test]
    fn failed_final_format_exchange_retains_the_previous_recovery_and_can_retry() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut photo = Document::new(8, 8);
        photo.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(2, 3, image::Rgba([18, 52, 86, 255]));
        recovery.schedule(&photo, true);
        let path = recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        let previous = omuse::save_guard::package_stamp(&path).unwrap();
        let mut project = omuse::create_project::Project::new("Campaign", photo.clone());
        project.add_blank_page("Second", 12, 9).unwrap();

        let resume = pause_staged_conversion(&recovery, &project);
        let prepared = staged_snapshot(temp.path());
        assert!(prepared.join("project.json").is_file());
        // Cause the real outer renameat2 exchange to fail with ENOENT after
        // staging succeeds. The old destination remains present throughout.
        fs::remove_dir_all(&prepared).unwrap();
        drop(resume);

        assert!(recovery.wait_idle_for_test(Duration::from_secs(5)).is_err());
        assert!(
            recovery
                .error()
                .unwrap()
                .contains("format conversion failed; previous recovery retained")
        );
        assert_eq!(omuse::save_guard::package_stamp(&path), Some(previous));
        assert_recovered_artwork(&document::open(&path).unwrap(), &photo);
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse")
        }));

        recovery.schedule_project(&project, true);
        recovery.wait_idle_for_test(Duration::from_secs(5)).unwrap();
        assert!(recovery.error().is_none());
        assert_eq!(
            omuse::create_project::Project::open(&path)
                .unwrap()
                .page_ids(),
            project.page_ids()
        );
    }

    #[test]
    fn complete_collection_recovers_after_window_ends() {
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().to_owned());
        let mut project = omuse::create_project::Project::new("A campaign", Document::new(8, 8));
        let second = project.add_blank_page("Second", 12, 9).unwrap();
        project.set_active_page(&second).unwrap();
        recovery.schedule_project(&project, true);
        let path = recovery.path.clone();
        drop(recovery);
        let mut restored = omuse::create_project::Project::open(&path).unwrap();
        assert_eq!(restored.page_ids().len(), 2);
        assert_eq!(restored.active_page_id(), second);
        assert_eq!(restored.active_document().unwrap().width, 12);
        let discovery = Recovery::at(temp.path().to_owned());
        assert_eq!(discovery.available(), Some(path));
    }
}
