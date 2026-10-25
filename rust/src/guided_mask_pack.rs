//! Explicit optional pack acquisition. Only fixed pinned HTTPS weights are
//! downloaded; no model/upstream code is executed and no generic URL is accepted.
use crate::{
    asset_library::sha256_hex,
    guided_mask::{self, ManagedPack},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const LICENCE: &[u8] = include_bytes!("../licenses/efficientsam/LICENSE");
const CARD: &[u8] = include_bytes!("../licenses/efficientsam/MODEL_CARD.md");
const REVISION: &str = "d8dbb1eee73bfb3392aa6f6e8944aeb13f3f4036";
const ENCODER_URL: &str = "https://huggingface.co/spaces/yunyangx/EfficientSAM/resolve/d8dbb1eee73bfb3392aa6f6e8944aeb13f3f4036/efficientsam_ti_encoder.onnx";
const DECODER_URL: &str = "https://huggingface.co/spaces/yunyangx/EfficientSAM/resolve/d8dbb1eee73bfb3392aa6f6e8944aeb13f3f4036/efficientsam_ti_decoder.onnx";
struct Artifact<'a> {
    name: &'a str,
    url: &'a str,
    bytes: u64,
    sha256: &'a str,
}
pub const TOTAL_BYTES: u64 = guided_mask::ENCODER_BYTES + guided_mask::DECODER_BYTES;
fn pinned_artifacts() -> [Artifact<'static>; 2] {
    [
        Artifact {
            name: guided_mask::ENCODER_FILE,
            url: ENCODER_URL,
            bytes: guided_mask::ENCODER_BYTES,
            sha256: guided_mask::ENCODER_SHA256,
        },
        Artifact {
            name: guided_mask::DECODER_FILE,
            url: DECODER_URL,
            bytes: guided_mask::DECODER_BYTES,
            sha256: guided_mask::DECODER_SHA256,
        },
    ]
}
pub fn download_verified(
    pack: &ManagedPack,
    cancel: &AtomicBool,
) -> Result<crate::durable_fs::SaveOutcome> {
    install(pack.directory(), &pinned_artifacts(), &Curl, cancel)
}
/// Explicit verification is read-only. Presence metadata alone is never proof
/// that the installed pair, licence and provenance match the managed pack.
pub fn verify_existing(pack: &ManagedPack, cancel: &AtomicBool) -> Result<()> {
    verify_directory(pack.directory(), &pinned_artifacts(), cancel)
}
#[derive(Debug)]
pub struct RepairReport {
    pub outcome: crate::durable_fs::SaveOutcome,
    /// Previous bytes are retained here, never deleted by a successful repair.
    pub retained_previous: Option<PathBuf>,
}
/// Called only by an explicit Repair action. Open/availability never repair or
/// download. A verified concurrent winner is preserved, and an absent pack is
/// installed without replacing a winner. Existing links/reparse trees refuse.
pub fn repair_verified(pack: &ManagedPack, cancel: &AtomicBool) -> Result<RepairReport> {
    repair_with_hooks(
        pack.directory(),
        &pinned_artifacts(),
        &Curl,
        cancel,
        || Ok(()),
        || Ok(()),
    )
}
trait Transport {
    fn fetch(&self, artifact: &Artifact<'_>, destination: &Path, cancel: &AtomicBool)
    -> Result<()>;
}
struct Curl;
struct DownloadChild(std::process::Child);
impl Drop for DownloadChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn curl_command(artifact: &Artifact<'_>, destination: &Path) -> Command {
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    command
        .args([
            "--disable",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            "180",
            "--connect-timeout",
            "20",
            "--retry",
            "0",
            "--max-filesize",
        ])
        .arg(artifact.bytes.to_string())
        .arg("--output")
        .arg(destination)
        .arg("--url")
        .arg(artifact.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
impl Transport for Curl {
    fn fetch(
        &self,
        artifact: &Artifact<'_>,
        destination: &Path,
        cancel: &AtomicBool,
    ) -> Result<()> {
        check(cancel)?;
        // No shell and no environment-provided URL. Curl's byte cap is backed
        // by supervisor checks and exact bounded verification before publishing.
        let mut child=DownloadChild(curl_command(artifact,destination).spawn().context("Download needs curl (curl.exe on Windows); install the helper or use a verified offline model pack")?);
        let start = Instant::now();
        loop {
            let oversized = fs::metadata(destination).is_ok_and(|m| m.len() > artifact.bytes);
            if cancel.load(Ordering::Acquire)
                || oversized
                || start.elapsed() > Duration::from_secs(185)
            {
                let _ = child.0.kill();
                let _ = child.0.wait();
                check(cancel)?;
                ensure!(!oversized, "Model download exceeded its exact byte limit");
                anyhow::bail!("Model download timed out");
            }
            if let Some(status) = child.0.try_wait()? {
                ensure!(
                    status.success(),
                    "Model download failed (curl exit {:?}); check the connection and pinned model host, then retry",
                    status.code()
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        check(cancel)
    }
}
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Acquire), "Model download cancelled");
    Ok(())
}
struct Stage {
    path: PathBuf,
    committed: bool,
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn install(
    destination: &Path,
    artifacts: &[Artifact<'_>],
    transport: &dyn Transport,
    cancel: &AtomicBool,
) -> Result<crate::durable_fs::SaveOutcome> {
    install_with_hook(destination, artifacts, transport, cancel, || Ok(()))
}
fn install_with_hook(
    destination: &Path,
    artifacts: &[Artifact<'_>],
    transport: &dyn Transport,
    cancel: &AtomicBool,
    before_publish: impl FnOnce() -> Result<()>,
) -> Result<crate::durable_fs::SaveOutcome> {
    check(cancel)?;
    ensure!(
        !destination.exists(),
        "A model pack folder already exists; the existing pack was preserved. Verify or repair that folder before downloading another copy"
    );
    let parent = destination
        .parent()
        .context("Model pack has no managed parent folder")?;
    fs::create_dir_all(parent)?;
    let mut stage = prepare_stage(destination, artifacts, transport, cancel)?;
    before_publish()?;
    check(cancel)?;
    // Destination is never populated incrementally. Existing/incomplete packs
    // are preserved, and concurrent publication refuses to replace a winner.
    rename_new(&stage.path, destination).context(
        "Publishing the verified model pack without replacing an existing folder or link",
    )?;
    stage.committed = true;
    Ok(match crate::durable_fs::sync_directory(parent) {
        Ok(()) => crate::durable_fs::SaveOutcome::Confirmed,
        Err(e) => crate::durable_fs::SaveOutcome::DurabilityUnconfirmed(format!(
            "The complete verified model pack is present at {}, but its parent folder could not be flushed ({e}). Publication succeeded; durability is unconfirmed. Verify the pack before use and check storage health.",
            destination.display()
        )),
    })
}

fn prepare_stage(
    destination: &Path,
    artifacts: &[Artifact<'_>],
    transport: &dyn Transport,
    cancel: &AtomicBool,
) -> Result<Stage> {
    check(cancel)?;
    let parent = destination
        .parent()
        .context("Model pack has no managed parent folder")?;
    fs::create_dir_all(parent)?;
    let stage_path = parent.join(format!(".guided-mask-stage-{}", uuid::Uuid::new_v4()));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&stage_path)?;
    let stage = Stage {
        path: stage_path,
        committed: false,
    };
    for artifact in artifacts {
        check(cancel)?;
        ensure!(
            Path::new(artifact.name)
                .file_name()
                .is_some_and(|n| n == artifact.name)
                && !artifact.name.contains(['/', '\\']),
            "Invalid model pack filename"
        );
        let path = stage.path.join(artifact.name);
        transport.fetch(artifact, &path, cancel)?;
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == artifact.bytes,
            "Model download length/type verification failed"
        );
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(usize::try_from(artifact.bytes)?)?;
        let mut block = [0_u8; 65536];
        loop {
            check(cancel)?;
            let n = file.read(&mut block)?;
            if n == 0 {
                break;
            }
            ensure!(
                bytes
                    .len()
                    .checked_add(n)
                    .is_some_and(|n| n <= artifact.bytes as usize),
                "Model download grew during verification"
            );
            bytes.extend_from_slice(&block[..n]);
        }
        ensure!(
            bytes.len() as u64 == artifact.bytes && sha256_hex(&bytes) == artifact.sha256,
            "Model download checksum verification failed"
        );
        file.sync_all()?;
    }
    let manifest = expected_manifest(artifacts);
    for (name, data) in [
        ("LICENSE", LICENCE.to_vec()),
        ("MODEL_CARD.md", CARD.to_vec()),
        ("manifest.json", serde_json::to_vec_pretty(&manifest)?),
    ] {
        check(cancel)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.path.join(name))?;
        file.write_all(&data)?;
        file.sync_all()?;
    }
    check(cancel)?;
    crate::durable_fs::sync_directory(&stage.path)
        .context("Flushing the complete model stage before publication")?;
    Ok(stage)
}

fn plain(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    metadata.is_dir() || metadata.is_file()
}
fn tree_stamp(path: &Path, cancel: &AtomicBool) -> Result<u64> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut pending = vec![path.to_owned()];
    let mut count = 0usize;
    while let Some(current) = pending.pop() {
        check(cancel)?;
        count += 1;
        ensure!(
            count <= 4096,
            "Model pack has too many entries to repair safely; the existing folder is retained at {}",
            path.display()
        );
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            plain(&metadata),
            "Model pack contains an unsupported link/reparse/special entry; the existing source is retained at {}",
            path.display()
        );
        current.strip_prefix(path)?.hash(&mut hash);
        metadata.is_dir().hash(&mut hash);
        metadata.len().hash(&mut hash);
        metadata.modified()?.hash(&mut hash);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.dev().hash(&mut hash);
            metadata.ino().hash(&mut hash);
            if current != path {
                metadata.ctime().hash(&mut hash);
                metadata.ctime_nsec().hash(&mut hash);
            }
        }
        #[cfg(windows)]
        {
            windows_file_id(&current)?.hash(&mut hash);
        }
        if metadata.is_dir() {
            let mut children = Vec::new();
            for entry in fs::read_dir(&current)? {
                ensure!(
                    count + pending.len() + children.len() < 4096,
                    "Model pack has too many entries to repair safely; source retained at {}",
                    path.display()
                );
                children.try_reserve(1)?;
                children.push(entry?.path());
            }
            children.sort();
            pending.try_reserve(children.len())?;
            pending.extend(children);
        }
    }
    Ok(hash.finish())
}
#[cfg(windows)]
fn windows_file_id(path: &Path) -> std::io::Result<(u32, u64)> {
    use std::{
        ffi::c_void,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    #[repr(C)]
    struct Info {
        attributes: u32,
        creation: [u32; 2],
        access: [u32; 2],
        write: [u32; 2],
        volume: u32,
        size_hi: u32,
        size_lo: u32,
        links: u32,
        index_hi: u32,
        index_lo: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(handle: *mut c_void, info: *mut Info) -> i32;
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)?;
    let mut info = std::mem::MaybeUninit::<Info>::uninit();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    if info.attributes & 0x400 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Model pack reparse points cannot be repaired safely",
        ));
    }
    Ok((
        info.volume,
        (u64::from(info.index_hi) << 32) | u64::from(info.index_lo),
    ))
}
fn bounded_read(path: &Path, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    check(cancel)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && plain(&metadata) && metadata.len() <= limit,
        "Model pack member length/type verification failed: {}",
        path.display()
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x80000);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let mut file = options.open(path)?;
    let mut data = Vec::new();
    data.try_reserve_exact(usize::try_from(limit)?)?;
    let mut block = [0u8; 65536];
    loop {
        check(cancel)?;
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        ensure!(
            data.len().checked_add(n).is_some_and(|n| n as u64 <= limit),
            "Model pack member grew during verification"
        );
        data.extend_from_slice(&block[..n]);
    }
    ensure!(
        metadata.len() == file.metadata()?.len()
            && metadata.modified()? == file.metadata()?.modified()?,
        "Model pack changed during verification"
    );
    check(cancel)?;
    Ok(data)
}
fn expected_manifest(artifacts: &[Artifact<'_>]) -> serde_json::Value {
    let records=artifacts.iter().map(|artifact|serde_json::json!({"file":artifact.name,"url":artifact.url,"bytes":artifact.bytes,"sha256":artifact.sha256})).collect::<Vec<_>>();
    serde_json::json!({"version":1,"id":guided_mask::MODEL_ID,"revision":REVISION,"source":"https://github.com/yformer/EfficientSAM","authorModelHost":"https://huggingface.co/spaces/yunyangx/EfficientSAM","licence":"Apache-2.0","attribution":"EfficientSAM, Meta Platforms, Inc. and affiliates; original model authors. The official repository credits Kentaro Wada (labelme) for ONNX conversion.","capability":"Experimental positive points and one box; at most six point tokens. No qualified negative model labels.","artifacts":records})
}
fn verify_directory(path: &Path, artifacts: &[Artifact<'_>], cancel: &AtomicBool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Optional model pack is missing at {}", path.display()))?;
    ensure!(
        metadata.is_dir() && plain(&metadata),
        "Model pack folder is not a plain directory; source retained at {}",
        path.display()
    );
    let before = tree_stamp(path, cancel)?;
    for artifact in artifacts {
        let data = bounded_read(&path.join(artifact.name), artifact.bytes, cancel)?;
        ensure!(
            data.len() as u64 == artifact.bytes && sha256_hex(&data) == artifact.sha256,
            "Model pack checksum verification failed for {}",
            artifact.name
        );
    }
    ensure!(
        bounded_read(&path.join("LICENSE"), LICENCE.len() as u64, cancel)? == LICENCE,
        "Model pack licence differs from its pinned licence"
    );
    ensure!(
        bounded_read(&path.join("MODEL_CARD.md"), CARD.len() as u64, cancel)? == CARD,
        "Model pack card differs from its pinned model card"
    );
    let manifest: serde_json::Value = serde_json::from_slice(&bounded_read(
        &path.join("manifest.json"),
        128 * 1024,
        cancel,
    )?)?;
    ensure!(
        manifest == expected_manifest(artifacts),
        "Model pack provenance manifest does not match the pinned pair"
    );
    ensure!(
        tree_stamp(path, cancel)? == before,
        "Model pack changed during verification; retry without replacing it"
    );
    Ok(())
}
struct RepairLock {
    _file: fs::File,
}
impl RepairLock {
    fn acquire(destination: &Path, cancel: &AtomicBool) -> Result<Self> {
        let parent = destination.parent().context("Model pack has no parent")?;
        fs::create_dir_all(parent)?;
        let name = destination
            .file_name()
            .context("Model pack folder has no name")?
            .to_string_lossy();
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(0x20000 | 0x80000);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        let lock_path = parent.join(format!(".{name}.repair.lock"));
        let file = options.open(&lock_path)?;
        ensure!(
            fs::symlink_metadata(&lock_path)?.is_file()
                && plain(&fs::symlink_metadata(&lock_path)?),
            "Model repair lock is not a plain file; the existing source was preserved"
        );
        let start = Instant::now();
        loop {
            check(cancel)?;
            #[cfg(target_os = "linux")]
            let locked = {
                use std::os::fd::AsRawFd;
                unsafe extern "C" {
                    fn flock(fd: i32, operation: i32) -> i32;
                }
                if unsafe { flock(file.as_raw_fd(), 2 | 4) } == 0 {
                    true
                } else {
                    let e = std::io::Error::last_os_error();
                    ensure!(
                        e.kind() == std::io::ErrorKind::WouldBlock,
                        "Cannot lock model pack repair: {e}"
                    );
                    false
                }
            };
            #[cfg(not(target_os = "linux"))]
            let locked = match file.try_lock() {
                Ok(()) => true,
                Err(fs::TryLockError::WouldBlock) => false,
                Err(fs::TryLockError::Error(e)) => return Err(e.into()),
            };
            if locked {
                return Ok(Self { _file: file });
            }
            ensure!(
                start.elapsed() < Duration::from_secs(2),
                "Another Omuse window is repairing this model pack; try again shortly"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
fn repaired_outcome(
    parent: &Path,
    destination: &Path,
    previous: &Path,
) -> crate::durable_fs::SaveOutcome {
    match crate::durable_fs::sync_directory(parent) {
        Ok(()) => crate::durable_fs::SaveOutcome::Confirmed,
        Err(e) => crate::durable_fs::SaveOutcome::DurabilityUnconfirmed(format!(
            "The complete verified repaired model pack is present at {} and the previous pack is retained at {}, but their parent folder could not be flushed ({e}). Publication succeeded; durability is unconfirmed. Keep the retained pack and check storage health.",
            destination.display(),
            previous.display()
        )),
    }
}
// Metadata stamps fence identity and ordinary concurrent edits. An external
// Windows copier can preserve timestamps while fixing bytes in place, so a
// newly verified displaced tree always wins even if its stamp still matches.
fn displaced_source_unchanged(
    path: &Path,
    expected: Option<u64>,
    artifacts: &[Artifact<'_>],
) -> bool {
    let never_cancel = AtomicBool::new(false);
    tree_stamp(path, &never_cancel).ok() == expected
        && verify_directory(path, artifacts, &never_cancel).is_err()
}
fn repair_with_hooks(
    destination: &Path,
    artifacts: &[Artifact<'_>],
    transport: &dyn Transport,
    cancel: &AtomicBool,
    before_publish: impl FnOnce() -> Result<()>,
    after_publish: impl FnOnce() -> Result<()>,
) -> Result<RepairReport> {
    check(cancel)?;
    #[cfg(not(any(target_os = "linux", windows)))]
    anyhow::bail!(
        "Safe model-pack repair is unsupported on this platform; previous source is retained at {}",
        destination.display()
    );
    let _lock = RepairLock::acquire(destination, cancel)?;
    if verify_directory(destination, artifacts, cancel).is_ok() {
        return Ok(RepairReport {
            outcome: crate::durable_fs::SaveOutcome::Confirmed,
            retained_previous: None,
        });
    }
    check(cancel)?;
    let original = match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && plain(&metadata),
                "Model-pack repair refuses a linked/reparse/non-directory destination; source is retained at {}",
                destination.display()
            );
            Some(tree_stamp(destination, cancel)?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut stage = prepare_stage(destination, artifacts, transport, cancel)?;
    verify_directory(&stage.path, artifacts, cancel)?;
    // A different writer may have completed the same pinned pair while the
    // network stage was built. Preserve that winner and discard only our stage.
    if verify_directory(destination, artifacts, cancel).is_ok() {
        return Ok(RepairReport {
            outcome: crate::durable_fs::SaveOutcome::Confirmed,
            retained_previous: None,
        });
    }
    check(cancel)?;
    match original {
        Some(stamp) => ensure!(
            tree_stamp(destination, cancel)? == stamp,
            "The existing model pack changed during repair; it was preserved at {}. Retry verification",
            destination.display()
        ),
        None => ensure!(
            matches!(fs::symlink_metadata(destination),Err(e) if e.kind()==std::io::ErrorKind::NotFound),
            "Another model pack appeared; the winner was preserved"
        ),
    }
    let staged = tree_stamp(&stage.path, cancel)?;
    before_publish()?;
    check(cancel)?;
    if original.is_none() {
        rename_new(&stage.path, destination)
            .context("A concurrent model pack was preserved instead of replaced")?;
        stage.committed = true;
        // No prior source exists, but reuse install's publication framing.
        let outcome = match crate::durable_fs::sync_directory(destination.parent().unwrap()) {
            Ok(()) => crate::durable_fs::SaveOutcome::Confirmed,
            Err(e) => crate::durable_fs::SaveOutcome::DurabilityUnconfirmed(format!(
                "The complete verified model pack is present at {}, but parent sync failed ({e}); durability is unconfirmed",
                destination.display()
            )),
        };
        return Ok(RepairReport {
            outcome,
            retained_previous: None,
        });
    }
    let parent = destination.parent().unwrap();
    #[cfg(target_os = "linux")]
    {
        exchange_pack(&stage.path, destination).with_context(|| {
            format!(
                "Atomic model-pack repair is unavailable; the previous source was preserved at {}",
                destination.display()
            )
        })?;
        // The staging name now contains someone else's old bytes. No Drop or
        // late error is ever allowed to delete it.
        stage.committed = true;
        let hook = after_publish();
        let never_cancel = AtomicBool::new(false);
        let previous_unchanged = displaced_source_unchanged(&stage.path, original, artifacts);
        let destination_ours = tree_stamp(destination, &never_cancel).ok() == Some(staged);
        if !previous_unchanged || !destination_ours || hook.is_err() {
            if destination_ours && exchange_pack(&stage.path, destination).is_ok() {
                // Restored the displaced winner/original. The staged path once
                // more belongs to our verified replacement and can be cleaned.
                ensure!(
                    rollback_stage_is_ours(&stage, staged),
                    "A concurrent model-pack owner moved during rollback. The restored source is at {} and the additional owner is retained at {}; no recovery tree was deleted",
                    destination.display(),
                    stage.path.display()
                );
                match crate::durable_fs::sync_directory(parent) {
                    Ok(()) => stage.committed = false,
                    Err(error) => {
                        // A visible rollback is not a durability confirmation.
                        // Keep our complete replacement too: deleting its name
                        // after an unflushed rollback weakens crash recovery.
                        anyhow::bail!(
                            "The displaced model-pack source is restored at {}, but rollback parent sync failed ({error}); durability is unconfirmed. The complete verified replacement is retained at {}. Preserve both paths and check storage health before retrying",
                            destination.display(),
                            stage.path.display()
                        );
                    }
                }
                anyhow::bail!(
                    "The model pack changed at publication or the repair hook failed; the displaced source was restored at {}",
                    destination.display()
                );
            }
            anyhow::bail!(
                "Model-pack repair could not safely restore a concurrently changed destination. Preserve both {} and the displaced source retained at {} before retrying",
                destination.display(),
                stage.path.display()
            );
        }
        let retained = parent.join(format!(".guided-mask-previous-{}", uuid::Uuid::new_v4()));
        let previous = if rename_new(&stage.path, &retained).is_ok() {
            retained
        } else {
            stage.path.clone()
        };
        let outcome = repaired_outcome(parent, destination, &previous);
        Ok(RepairReport {
            outcome,
            retained_previous: Some(previous),
        })
    }
    #[cfg(windows)]
    {
        // Windows has no atomic directory exchange. Move the old plain tree to
        // a reserved no-replace quarantine and preserve it on every late error.
        // Avoid durable_fs::exchange_dirs: its third-move failure can delete the
        // old aside, which is unacceptable for explicit model-pack repair.
        let previous = parent.join(format!(".guided-mask-previous-{}", uuid::Uuid::new_v4()));
        rename_new(destination, &previous)?;
        let never_cancel = AtomicBool::new(false);
        if !displaced_source_unchanged(&previous, original, artifacts)
            || cancel.load(Ordering::Acquire)
        {
            let restored = rename_new(&previous, destination).is_ok();
            if restored && let Err(error) = crate::durable_fs::sync_directory(parent) {
                stage.committed = true;
                anyhow::bail!(
                    "The previous model-pack source is restored at {}, but parent sync failed ({error}); durability is unconfirmed. The complete replacement is retained at {}",
                    destination.display(),
                    stage.path.display()
                );
            }
            anyhow::bail!(
                "Model-pack repair stopped or its source changed; previous source {}",
                if restored {
                    format!("restored at {}", destination.display())
                } else {
                    format!("retained at {}", previous.display())
                }
            );
        }
        if let Err(e) = rename_new(&stage.path, destination) {
            let restored = rename_new(&previous, destination).is_ok();
            anyhow::bail!(
                "Model-pack publication refused ({e}); previous source {}",
                if restored {
                    format!("restored at {}", destination.display())
                } else {
                    format!("retained at {}", previous.display())
                }
            );
        }
        stage.committed = true;
        let hook = after_publish();
        if !displaced_source_unchanged(&previous, original, artifacts) {
            // An external writer may finish through pre-existing file handles
            // after quarantine. Restore its completed source only while the
            // destination is still our exact published stage; never overwrite
            // a newer owner. All failure paths retain both complete trees.
            if tree_stamp(destination, &never_cancel).ok() == Some(staged)
                && rename_new(destination, &stage.path).is_ok()
            {
                ensure!(
                    rollback_stage_is_ours(&stage, staged),
                    "A concurrent model-pack owner moved during rollback. Preserve the additional owner at {} and the previous source at {}; no recovery tree was deleted",
                    stage.path.display(),
                    previous.display()
                );
                if rename_new(&previous, destination).is_ok() {
                    match crate::durable_fs::sync_directory(parent) {
                        Ok(()) => stage.committed = false,
                        Err(error) => anyhow::bail!(
                            "The concurrent model-pack source is restored at {}, but rollback sync failed ({error}); durability is unconfirmed. Replacement retained at {}",
                            destination.display(),
                            stage.path.display()
                        ),
                    }
                    anyhow::bail!(
                        "The concurrent model-pack source was restored at {}; its verified winner was preserved",
                        destination.display()
                    );
                }
                anyhow::bail!(
                    "A concurrent winner could not be restored safely; preserve destination {}, previous source {} and replacement {}",
                    destination.display(),
                    previous.display(),
                    stage.path.display()
                );
            }
            anyhow::bail!(
                "The previous model-pack source changed during repair; preserve destination {} and source retained at {}",
                destination.display(),
                previous.display()
            );
        }
        if hook.is_err() || tree_stamp(destination, &never_cancel).ok() != Some(staged) {
            anyhow::bail!(
                "Model-pack publication changed after repair; preserve destination {} and previous source retained at {}",
                destination.display(),
                previous.display()
            );
        }
        let outcome = repaired_outcome(parent, destination, &previous);
        Ok(RepairReport {
            outcome,
            retained_previous: Some(previous),
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    unreachable!()
}
fn rollback_stage_is_ours(stage: &Stage, expected: u64) -> bool {
    tree_stamp(&stage.path, &AtomicBool::new(false)).ok() == Some(expected)
}
#[cfg(all(test, target_os = "linux"))]
mod rollback_owner_tests {
    use super::*;
    #[test]
    fn late_owner_caught_by_rollback_is_retained_instead_of_stage_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let ours = temp.path().join("ours");
        let stage_path = temp.path().join("stage");
        let destination = temp.path().join("destination");
        for path in [&ours, &stage_path, &destination] {
            fs::create_dir(path).unwrap();
        }
        fs::write(ours.join("data"), b"our verified replacement").unwrap();
        let expected = tree_stamp(&ours, &AtomicBool::new(false)).unwrap();
        fs::write(stage_path.join("data"), b"displaced original").unwrap();
        fs::write(destination.join("data"), b"a newer concurrent owner").unwrap();
        // An uncooperative writer replaced destination after the pre-rollback
        // ownership check. The rollback catches that owner at our stage name.
        exchange_pack(&stage_path, &destination).unwrap();
        let stage = Stage {
            path: stage_path.clone(),
            committed: true,
        };
        assert!(!rollback_stage_is_ours(&stage, expected));
        drop(stage);
        assert_eq!(
            fs::read(stage_path.join("data")).unwrap(),
            b"a newer concurrent owner"
        );
        assert_eq!(
            fs::read(destination.join("data")).unwrap(),
            b"displaced original"
        );
    }
}
#[cfg(target_os = "linux")]
fn exchange_pack(from: &Path, to: &Path) -> std::io::Result<()> {
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
    if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 2) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
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
    // AT_FDCWD=-100, RENAME_NOREPLACE=1. Owned C paths outlive the call.
    if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(windows)]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    crate::durable_fs::rename_no_replace(from, to)
}
#[cfg(not(any(target_os = "linux", windows)))]
fn rename_new(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Atomic model-pack publication is not supported on this platform",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        data: Vec<u8>,
        cancel: bool,
    }
    impl Transport for Mock {
        fn fetch(&self, _: &Artifact<'_>, destination: &Path, cancel: &AtomicBool) -> Result<()> {
            fs::write(destination, &self.data)?;
            if self.cancel {
                cancel.store(true, Ordering::Release);
            }
            Ok(())
        }
    }
    fn artifact(hash: &str) -> Artifact<'_> {
        Artifact {
            name: "fixture.onnx",
            url: ENCODER_URL,
            bytes: 3,
            sha256: hash,
        }
    }
    #[test]
    fn publication_race_preserves_an_empty_directory_and_an_existing_winner() {
        for empty in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let dest = temp.path().join("pack");
            let hash = sha256_hex(b"abc");
            let result = install_with_hook(
                &dest,
                &[artifact(&hash)],
                &Mock {
                    data: b"abc".to_vec(),
                    cancel: false,
                },
                &AtomicBool::new(false),
                || {
                    fs::create_dir(&dest)?;
                    if !empty {
                        fs::write(dest.join("winner"), b"keep winner")?;
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert!(dest.is_dir());
            if empty {
                assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
            } else {
                assert_eq!(fs::read(dest.join("winner")).unwrap(), b"keep winner");
            }
            assert_eq!(
                fs::read_dir(temp.path()).unwrap().count(),
                1,
                "losing stage must be removed"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn publication_race_preserves_a_symlink_winner() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let hash = sha256_hex(b"abc");
        let result = install_with_hook(
            &dest,
            &[artifact(&hash)],
            &Mock {
                data: b"abc".to_vec(),
                cancel: false,
            },
            &AtomicBool::new(false),
            || {
                std::os::unix::fs::symlink(&target, &dest)?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(
            fs::symlink_metadata(&dest)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&dest).unwrap(), target);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }
    #[test]
    fn cancellation_after_verification_cleans_the_stage_before_publication() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        let cancel = AtomicBool::new(false);
        assert!(
            install_with_hook(
                &dest,
                &[artifact(&hash)],
                &Mock {
                    data: b"abc".to_vec(),
                    cancel: false
                },
                &cancel,
                || {
                    cancel.store(true, Ordering::Release);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    #[test]
    fn final_sync_failure_reports_present_complete_pack_instead_of_download_failure() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        let fault = std::cell::RefCell::new(None);
        let outcome = install_with_hook(
            &dest,
            &[artifact(&hash)],
            &Mock {
                data: b"abc".to_vec(),
                cancel: false,
            },
            &AtomicBool::new(false),
            || {
                *fault.borrow_mut() = Some(crate::durable_fs::inject_directory_sync_failure(
                    temp.path(),
                    std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                ));
                Ok(())
            },
        )
        .unwrap();
        assert!(
            outcome
                .warning()
                .unwrap()
                .contains("complete verified model pack is present")
        );
        assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"abc");
        assert!(dest.join("manifest.json").is_file());
    }
    #[test]
    fn verified_complete_pack_is_atomically_published_with_licence() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        install(
            &dest,
            &[artifact(&hash)],
            &Mock {
                data: b"abc".to_vec(),
                cancel: false,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"abc");
        assert!(
            dest.join("LICENSE").is_file()
                && dest.join("MODEL_CARD.md").is_file()
                && dest.join("manifest.json").is_file()
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    fn failed_hash_or_cancel_never_publishes_partial_pack() {
        for cancel in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let dest = temp.path().join("pack");
            let hash = sha256_hex(b"abc");
            assert!(
                install(
                    &dest,
                    &[artifact(&hash)],
                    &Mock {
                        data: b"bad".to_vec(),
                        cancel
                    },
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert!(!dest.exists());
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn existing_pack_is_preserved_and_curl_has_only_fixed_https_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("keep"), b"original").unwrap();
        let hash = sha256_hex(b"abc");
        assert!(
            install(
                &dest,
                &[artifact(&hash)],
                &Mock {
                    data: b"abc".to_vec(),
                    cancel: false
                },
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(fs::read(dest.join("keep")).unwrap(), b"original");
        let command = curl_command(&artifact(&hash), &temp.path().join("a b.onnx"));
        let args = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|a| a == ["--proto", "=https"]));
        assert!(args.windows(2).any(|a| a == ["--proto-redir", "=https"]));
        assert!(args.windows(2).any(|a| a == ["--max-filesize", "3"]));
        assert_eq!(args.last().unwrap(), ENCODER_URL);
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    struct Mock {
        data: &'static [u8],
        cancel: bool,
    }
    impl Transport for Mock {
        fn fetch(&self, _: &Artifact<'_>, destination: &Path, cancel: &AtomicBool) -> Result<()> {
            fs::write(destination, self.data)?;
            if self.cancel {
                cancel.store(true, Ordering::Release);
            }
            Ok(())
        }
    }
    struct Never;
    impl Transport for Never {
        fn fetch(&self, _: &Artifact<'_>, _: &Path, _: &AtomicBool) -> Result<()> {
            panic!("Verified pack must not download again")
        }
    }
    fn artifacts(hash: &str) -> [Artifact<'_>; 1] {
        [Artifact {
            name: "fixture.onnx",
            url: ENCODER_URL,
            bytes: 3,
            sha256: hash,
        }]
    }
    fn valid(dest: &Path, hash: &str) {
        install(
            dest,
            &artifacts(hash),
            &Mock {
                data: b"abc",
                cancel: false,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    }
    fn damaged(dest: &Path, hash: &str) {
        valid(dest, hash);
        fs::write(dest.join("fixture.onnx"), b"bad").unwrap();
    }
    #[test]
    fn explicit_verification_rejects_wrong_hash_missing_member_and_wrong_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        valid(&dest, &hash);
        verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).unwrap();
        fs::write(dest.join("fixture.onnx"), b"bad").unwrap();
        assert!(verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).is_err());
        fs::remove_file(dest.join("fixture.onnx")).unwrap();
        assert!(verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).is_err());
        fs::write(dest.join("fixture.onnx"), b"abc").unwrap();
        fs::write(dest.join("manifest.json"), b"{}").unwrap();
        assert!(verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn explicit_repair_preserves_old_bytes_and_publishes_only_a_complete_verified_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        damaged(&dest, &hash);
        fs::write(dest.join("private-original-note"), b"retain this too").unwrap();
        let result = repair_with_hooks(
            &dest,
            &artifacts(&hash),
            &Mock {
                data: b"abc",
                cancel: false,
            },
            &AtomicBool::new(false),
            || Ok(()),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(result.outcome, crate::durable_fs::SaveOutcome::Confirmed);
        let previous = result.retained_previous.expect("Corrupt original retained");
        assert_ne!(previous, dest);
        assert_eq!(fs::read(previous.join("fixture.onnx")).unwrap(), b"bad");
        assert_eq!(
            fs::read(previous.join("private-original-note")).unwrap(),
            b"retain this too"
        );
        verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).unwrap();
    }
    #[test]
    fn already_verified_pack_is_not_downloaded_or_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        valid(&dest, &hash);
        let before = tree_stamp(&dest, &AtomicBool::new(false)).unwrap();
        let result = repair_with_hooks(
            &dest,
            &artifacts(&hash),
            &Never,
            &AtomicBool::new(false),
            || Ok(()),
            || Ok(()),
        )
        .unwrap();
        assert!(result.retained_previous.is_none());
        assert_eq!(result.outcome, crate::durable_fs::SaveOutcome::Confirmed);
        assert_eq!(tree_stamp(&dest, &AtomicBool::new(false)).unwrap(), before);
    }
    #[test]
    fn cancellation_or_bad_replacement_never_moves_the_prior_pack() {
        for mode in 0..3 {
            let temp = tempfile::tempdir().unwrap();
            let dest = temp.path().join("pack");
            let hash = sha256_hex(b"abc");
            damaged(&dest, &hash);
            let before = tree_stamp(&dest, &AtomicBool::new(false)).unwrap();
            let cancel = AtomicBool::new(false);
            let result = repair_with_hooks(
                &dest,
                &artifacts(&hash),
                &Mock {
                    data: if mode == 0 { b"bad" } else { b"abc" },
                    cancel: mode == 1,
                },
                &cancel,
                || {
                    if mode == 2 {
                        cancel.store(true, Ordering::Release);
                    }
                    Ok(())
                },
                || Ok(()),
            );
            assert!(result.is_err());
            assert_eq!(tree_stamp(&dest, &AtomicBool::new(false)).unwrap(), before);
            assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"bad");
            assert!(fs::read_dir(temp.path()).unwrap().all(|e| {
                !e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".guided-mask-stage-")
            }));
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn late_verified_winner_is_restored_after_exchange_identity_fence() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let old = temp.path().join("writer-retained-original");
        let hash = sha256_hex(b"abc");
        damaged(&dest, &hash);
        let result = repair_with_hooks(
            &dest,
            &artifacts(&hash),
            &Mock {
                data: b"abc",
                cancel: false,
            },
            &AtomicBool::new(false),
            || {
                fs::rename(&dest, &old)?;
                valid(&dest, &hash);
                fs::write(dest.join("winner"), b"preserve concurrent verified writer")?;
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        verify_directory(&dest, &artifacts(&hash), &AtomicBool::new(false)).unwrap();
        assert_eq!(
            fs::read(dest.join("winner")).unwrap(),
            b"preserve concurrent verified writer"
        );
        assert_eq!(fs::read(old.join("fixture.onnx")).unwrap(), b"bad");
        assert!(fs::read_dir(temp.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".guided-mask-stage-")
        }));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn late_destination_change_is_never_overwritten_and_displaced_source_is_retained() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let moved = temp.path().join("writer-moved-new-pack");
        let hash = sha256_hex(b"abc");
        damaged(&dest, &hash);
        let result = repair_with_hooks(
            &dest,
            &artifacts(&hash),
            &Mock {
                data: b"abc",
                cancel: false,
            },
            &AtomicBool::new(false),
            || Ok(()),
            || {
                fs::rename(&dest, &moved)?;
                valid(&dest, &hash);
                fs::write(dest.join("winner"), b"new owner")?;
                Ok(())
            },
        );
        let error = result.unwrap_err().to_string();
        assert!(error.contains("displaced source retained"), "{error}");
        assert_eq!(fs::read(dest.join("winner")).unwrap(), b"new owner");
        let retained = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".guided-mask-stage-")
            })
            .unwrap();
        assert_eq!(fs::read(retained.join("fixture.onnx")).unwrap(), b"bad");
    }
    #[test]
    fn postpublication_sync_failure_reports_both_present_new_pack_and_retained_old_source() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        damaged(&dest, &hash);
        let fault = std::cell::RefCell::new(None);
        let result = repair_with_hooks(
            &dest,
            &artifacts(&hash),
            &Mock {
                data: b"abc",
                cancel: false,
            },
            &AtomicBool::new(false),
            || {
                *fault.borrow_mut() = Some(crate::durable_fs::inject_directory_sync_failure(
                    temp.path(),
                    std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                ));
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        let prior = result.retained_previous.unwrap();
        let warning = result.outcome.warning().unwrap();
        assert!(warning.contains("complete verified repaired model pack is present"));
        assert!(warning.contains(&prior.display().to_string()));
        assert!(warning.contains("durability is unconfirmed"));
        assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"abc");
        assert_eq!(fs::read(prior.join("fixture.onnx")).unwrap(), b"bad");
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn postexchange_failure_rolls_back_without_deleting_original_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        damaged(&dest, &hash);
        let before = tree_stamp(&dest, &AtomicBool::new(false)).unwrap();
        assert!(
            repair_with_hooks(
                &dest,
                &artifacts(&hash),
                &Mock {
                    data: b"abc",
                    cancel: false
                },
                &AtomicBool::new(false),
                || Ok(()),
                || anyhow::bail!("owned late fault")
            )
            .is_err()
        );
        assert_eq!(tree_stamp(&dest, &AtomicBool::new(false)).unwrap(), before);
        assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"bad");
    }
    #[cfg(unix)]
    #[test]
    fn repair_refuses_linked_source_without_traversing_or_replacing_it() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        let dest = temp.path().join("pack");
        fs::create_dir(&actual).unwrap();
        fs::write(actual.join("private"), b"keep").unwrap();
        std::os::unix::fs::symlink(&actual, &dest).unwrap();
        let hash = sha256_hex(b"abc");
        assert!(
            repair_with_hooks(
                &dest,
                &artifacts(&hash),
                &Never,
                &AtomicBool::new(false),
                || Ok(()),
                || Ok(())
            )
            .is_err()
        );
        assert_eq!(fs::read_link(&dest).unwrap(), actual);
        assert_eq!(fs::read(actual.join("private")).unwrap(), b"keep");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod rollback_durability_tests {
    use super::*;
    struct Mock;
    impl Transport for Mock {
        fn fetch(&self, _: &Artifact<'_>, destination: &Path, _: &AtomicBool) -> Result<()> {
            fs::write(destination, b"abc")?;
            Ok(())
        }
    }
    #[test]
    fn unflushed_rollback_retains_replacement_and_reports_uncertainty() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("pack");
        let hash = sha256_hex(b"abc");
        let artifacts = [Artifact {
            name: "fixture.onnx",
            url: ENCODER_URL,
            bytes: 3,
            sha256: &hash,
        }];
        let cancel = AtomicBool::new(false);
        install(&dest, &artifacts, &Mock, &cancel).unwrap();
        fs::write(dest.join("fixture.onnx"), b"bad").unwrap();
        let fault = std::cell::RefCell::new(None);
        let error = repair_with_hooks(
            &dest,
            &artifacts,
            &Mock,
            &cancel,
            || Ok(()),
            || {
                *fault.borrow_mut() = Some(crate::durable_fs::inject_directory_sync_failure(
                    temp.path(),
                    std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                ));
                anyhow::bail!("owned post-exchange failure")
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("source is restored"), "{error}");
        assert!(error.contains("durability is unconfirmed"), "{error}");
        assert!(
            error.contains("complete verified replacement is retained"),
            "{error}"
        );
        assert_eq!(fs::read(dest.join("fixture.onnx")).unwrap(), b"bad");
        let retained = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".guided-mask-stage-")
            })
            .expect("unflushed rollback retains recovery stage");
        verify_directory(&retained, &artifacts, &cancel).unwrap();
    }
}

#[cfg(test)]
mod displaced_winner_tests {
    use super::*;
    struct Mock;
    impl Transport for Mock {
        fn fetch(&self, _: &Artifact<'_>, path: &Path, _: &AtomicBool) -> Result<()> {
            fs::write(path, b"abc")?;
            Ok(())
        }
    }
    #[test]
    fn newly_verified_displaced_tree_wins_even_when_its_metadata_stamp_matches() {
        let temp = tempfile::tempdir().unwrap();
        let displaced = temp.path().join("displaced");
        let hash = sha256_hex(b"abc");
        let artifacts = [Artifact {
            name: "fixture.onnx",
            url: ENCODER_URL,
            bytes: 3,
            sha256: &hash,
        }];
        let cancel = AtomicBool::new(false);
        install(&displaced, &artifacts, &Mock, &cancel).unwrap();
        let matching_metadata = Some(tree_stamp(&displaced, &cancel).unwrap());
        // Exercise the shared fence decision independently from OS timestamp
        // precision/manipulation. A matching metadata stamp cannot authorize
        // replacing a now-verified displaced winner on either platform.
        assert!(!displaced_source_unchanged(
            &displaced,
            matching_metadata,
            &artifacts
        ));
        fs::write(displaced.join("fixture.onnx"), b"bad").unwrap();
        let bad_stamp = Some(tree_stamp(&displaced, &cancel).unwrap());
        assert!(displaced_source_unchanged(
            &displaced, bad_stamp, &artifacts
        ));
    }
}
