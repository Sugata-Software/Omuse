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

/// Holding this descriptor holds a Linux flock, including against another open
/// descriptor in the same process. The lock releases automatically on crash/reboot.
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
    #[cfg(not(target_os = "linux"))]
    fn acquire(_: &Path) -> io::Result<Option<Self>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Recovery ownership requires Linux advisory locks",
        ))
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
        let path = root.join(format!("session-{}.comp", uuid::Uuid::new_v4()));
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
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Some(session) = name
                    .strip_prefix("session-")
                    .and_then(|s| s.strip_suffix(".comp"))
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
                let result = match doc {
                    RecoverySnapshot::Document(doc) => {
                        document::save_checked(&doc, path, || recovery_publish_is_current(shared))
                    }
                    RecoverySnapshot::Project(mut project) => {
                        project.save_checked(path, || recovery_publish_is_current(shared))
                    }
                };
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
                        .map_or(Ok(()), |parent| File::open(parent)?.sync_all())
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
            .join("session-11111111-1111-4111-8111-111111111111.comp");
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
