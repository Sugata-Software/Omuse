//! Bounded, local persistence for completed Omuse AI alternatives.
//!
//! This deliberately records only completed results.  It copies returned image
//! files into a private store so a provider job directory can disappear without
//! losing the reviewable alternative.  A saved record is never an authority to
//! apply an edit: callers must compare [`SourceIdentity`] with the live canvas.

use crate::ai::{ProviderId, ResultAsset};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 16;
pub const MAX_ASSETS_PER_ENTRY: usize = 4;
pub const MAX_ENTRY_ASSET_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL_ASSET_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_CONTEXT_ASSETS_PER_ENTRY: usize = 8;
pub const MAX_CONTEXT_ASSET_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TOTAL_CONTEXT_BYTES: u64 = 96 * 1024 * 1024;
const MAX_INDEX_BYTES: u64 = 512 * 1024;
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_SUMMARY_BYTES: usize = 8 * 1024;
const MAX_PLAN_BYTES: usize = 256 * 1024;
const MAX_PROVENANCE_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIdentity {
    pub document_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub page_id: Option<String>,
    /// The ephemeral window session prevents a revision reset after restart
    /// from being mistaken for the source of a completed plan.
    pub session_id: String,
    pub epoch: u64,
    pub revision: u64,
    #[serde(default)]
    pub selection_revision: u64,
    #[serde(default)]
    pub project_generation: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Assistant,
    GenerateImage,
    EditImage,
}

/// Explicit user context retained with an alternative.  This deliberately
/// excludes the source canvas and arbitrary files from the job directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRole {
    Reference,
    SelectionMask,
}

const fn default_variation_index() -> u8 {
    1
}

const fn default_variation_total() -> u8 {
    1
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredContextAsset {
    /// A path relative to the history root. It is validated before use.
    pub file: String,
    pub media_type: String,
    pub byte_len: u64,
    pub role: ContextRole,
    pub content_hash: String,
}

impl StoredContextAsset {
    pub fn absolute_path(&self, root: &Path) -> Result<PathBuf> {
        let path = safe_relative_path(&self.file)?;
        let absolute = root.join(path);
        let metadata = fs::symlink_metadata(&absolute)
            .with_context(|| format!("AI result context is missing: {}", absolute.display()))?;
        ensure!(
            metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == self.byte_len
                && metadata.len() <= MAX_CONTEXT_ASSET_BYTES,
            "AI result context is no longer valid"
        );
        Ok(absolute)
    }
}

#[derive(Clone, Debug)]
pub struct NewContextAsset {
    pub path: PathBuf,
    pub media_type: String,
    pub byte_len: u64,
    pub role: ContextRole,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAsset {
    /// A path relative to the history root.  It is validated before use.
    pub file: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub byte_len: u64,
    #[serde(default)]
    pub provider_item_id: Option<String>,
}

impl StoredAsset {
    pub fn absolute_path(&self, root: &Path) -> Result<PathBuf> {
        let path = safe_relative_path(&self.file)?;
        let absolute = root.join(path);
        let metadata = fs::symlink_metadata(&absolute)
            .with_context(|| format!("AI result asset is missing: {}", absolute.display()))?;
        ensure!(
            metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == self.byte_len
                && metadata.len() <= MAX_ENTRY_ASSET_BYTES,
            "AI result asset is no longer valid"
        );
        Ok(absolute)
    }

    pub fn as_result_asset(&self, root: &Path) -> Result<ResultAsset> {
        Ok(ResultAsset {
            path: self.absolute_path(root)?,
            media_type: self.media_type.clone(),
            width: self.width,
            height: self.height,
            byte_len: self.byte_len,
            provider_item_id: self.provider_item_id.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredProposal {
    pub id: String,
    /// Results requested as explicit variations share this stable identifier.
    /// Older entries use their result ID as a one-result group when loaded.
    #[serde(default)]
    pub group_id: String,
    #[serde(default = "default_variation_index")]
    pub variation_index: u8,
    #[serde(default = "default_variation_total")]
    pub variation_total: u8,
    pub source: SourceIdentity,
    pub operation: Operation,
    pub provider: ProviderId,
    pub prompt: String,
    pub summary: String,
    /// Original validated assistant-plan JSON.  It is prepared again only
    /// against a matching, still-live source document.
    #[serde(default)]
    pub plan_json: Option<String>,
    #[serde(default)]
    pub assets: Vec<StoredAsset>,
    #[serde(default)]
    pub context_assets: Vec<StoredContextAsset>,
    #[serde(default)]
    pub image_edit: bool,
    /// Sanitized, bounded provenance.  This is deliberately metadata only:
    /// references are represented by hashes, never source paths or credentials.
    #[serde(default)]
    pub provenance: serde_json::Value,
    pub completed_unix_ms: u64,
}

#[derive(Clone, Debug)]
pub struct NewProposal {
    /// The provider job ID is retained so the review card, history row, and
    /// library provenance all refer to one stable result.
    pub id: String,
    pub group_id: String,
    pub variation_index: u8,
    pub variation_total: u8,
    pub source: SourceIdentity,
    pub operation: Operation,
    pub provider: ProviderId,
    pub prompt: String,
    pub summary: String,
    pub plan_json: Option<String>,
    pub assets: Vec<ResultAsset>,
    pub context_assets: Vec<NewContextAsset>,
    pub image_edit: bool,
    pub provenance: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndex {
    version: u32,
    entries: Vec<StoredProposal>,
}

impl Default for HistoryIndex {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

pub struct HistoryStore {
    root: PathBuf,
    entries: Vec<StoredProposal>,
}

/// A failed directory sync after rename cannot undo the now-visible index.
/// Keep both generations' files in that case so either index remains readable.
enum IndexPublication {
    Durable,
    VisibleButUnsynced(anyhow::Error),
}

struct Retention {
    index: HistoryIndex,
    dropped: Vec<StoredProposal>,
}

/// Unique copies may be prepared without holding the index lock. Until the
/// index references them, any error must remove only these new files.
#[derive(Default)]
struct PendingArtifacts(Vec<PathBuf>);

impl PendingArtifacts {
    fn retain(&mut self) {
        self.0.clear();
    }
}

impl Drop for PendingArtifacts {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

impl HistoryStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_owned();
        ensure!(!root.as_os_str().is_empty(), "missing AI history directory");
        ensure_private_directory(&root)?;
        let _lock = lock_history(&root)?;
        ensure_private_directory(&root.join("assets"))?;
        ensure_private_directory(&root.join("context"))?;
        let retained = retain_entries(read_valid_entries(&root)?)?;
        let mut store = Self {
            root,
            entries: Vec::new(),
        };
        // Keep every valid entry through pruning so files for old alternatives
        // are removed with their index records rather than becoming private
        // orphaned bytes after a restart.
        store.publish(retained, write_index)?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[StoredProposal] {
        &self.entries
    }

    pub fn persist(&mut self, input: NewProposal) -> Result<StoredProposal> {
        self.persist_with_writer(input, write_index)
    }

    fn persist_with_writer(
        &mut self,
        input: NewProposal,
        writer: impl FnOnce(&Path, &HistoryIndex) -> Result<IndexPublication>,
    ) -> Result<StoredProposal> {
        validate_input(&input)?;
        let id = input.id.to_uppercase();
        let copy_id = uuid::Uuid::new_v4();
        let mut pending = PendingArtifacts::default();
        let mut stored_assets: Vec<StoredAsset> = Vec::with_capacity(input.assets.len());
        for (index, asset) in input.assets.iter().enumerate() {
            let file = format!("assets/{id}-{copy_id}-{index}.asset");
            let destination = self.root.join(&file);
            copy_regular_file(&asset.path, &destination, asset.byte_len)?;
            pending.0.push(destination);
            stored_assets.push(StoredAsset {
                file,
                media_type: asset.media_type.clone(),
                width: asset.width,
                height: asset.height,
                byte_len: asset.byte_len,
                provider_item_id: asset.provider_item_id.clone(),
            });
        }
        let mut stored_context: Vec<StoredContextAsset> =
            Vec::with_capacity(input.context_assets.len());
        for (index, context) in input.context_assets.iter().enumerate() {
            let file = format!("context/{id}-{copy_id}-{index}.asset");
            let destination = self.root.join(&file);
            copy_regular_file_limited(
                &context.path,
                &destination,
                context.byte_len,
                MAX_CONTEXT_ASSET_BYTES,
            )?;
            pending.0.push(destination);
            stored_context.push(StoredContextAsset {
                file,
                media_type: context.media_type.clone(),
                byte_len: context.byte_len,
                role: context.role,
                content_hash: context.content_hash.clone(),
            });
        }
        // Copies and their directory entries must be durable before an index
        // can reference them. Large file copying stays outside the lock.
        if !stored_assets.is_empty() {
            crate::durable_fs::sync_path(self.root.join("assets"))?;
        }
        if !stored_context.is_empty() {
            crate::durable_fs::sync_path(self.root.join("context"))?;
        }
        let _lock = lock_history(&self.root)?;
        let mut entries = read_valid_entries(&self.root)?;
        ensure!(
            !entries
                .iter()
                .any(|entry| entry.id.eq_ignore_ascii_case(&id)),
            "AI result is already retained"
        );
        let entry = StoredProposal {
            id,
            group_id: input.group_id,
            variation_index: input.variation_index,
            variation_total: input.variation_total,
            source: input.source,
            operation: input.operation,
            provider: input.provider,
            prompt: input.prompt,
            summary: input.summary,
            plan_json: input.plan_json,
            assets: stored_assets,
            context_assets: stored_context,
            image_edit: input.image_edit,
            provenance: input.provenance,
            completed_unix_ms: now_unix_ms(),
        };
        entries.insert(0, entry.clone());
        let retained = retain_entries(entries)?;
        ensure!(
            retained
                .index
                .entries
                .iter()
                .any(|item| item.id == entry.id),
            "AI result cannot fit in the retained history budget"
        );
        self.publish(retained, |root, index| {
            let publication = writer(root, index)?;
            // Also preserve the new files if rename succeeded but the final
            // directory sync failed: the visible index already needs them.
            pending.retain();
            Ok(publication)
        })?;
        Ok(entry)
    }

    /// Removes a dismissed alternative and its private review artifacts.
    /// Project and library resources are outside this store and are never
    /// removed by dismissal. The index is published before files are removed,
    /// so a crash can leave harmless orphaned bytes but cannot resurrect a
    /// dismissed result.
    pub fn discard(&mut self, id: &str) -> Result<bool> {
        self.discard_with_writer(id, write_index)
    }

    fn discard_with_writer(
        &mut self,
        id: &str,
        writer: impl FnOnce(&Path, &HistoryIndex) -> Result<IndexPublication>,
    ) -> Result<bool> {
        let _lock = lock_history(&self.root)?;
        let mut entries = read_valid_entries(&self.root)?;
        let Some(index) = entries
            .iter()
            .position(|entry| entry.id.eq_ignore_ascii_case(id))
        else {
            self.entries = entries;
            return Ok(false);
        };
        let entry = entries.remove(index);
        let mut retained = retain_entries(entries)?;
        retained.dropped.push(entry);
        self.publish(retained, writer)?;
        Ok(true)
    }

    fn publish(
        &mut self,
        retained: Retention,
        writer: impl FnOnce(&Path, &HistoryIndex) -> Result<IndexPublication>,
    ) -> Result<()> {
        let publication = writer(&self.root, &retained.index)?;
        self.entries = retained.index.entries;
        if let IndexPublication::VisibleButUnsynced(error) = publication {
            return Err(
                error.context("AI history was saved but its durability could not be confirmed")
            );
        }
        for entry in retained.dropped {
            remove_unreferenced_artifacts(&self.root, &entry, &self.entries);
        }
        Ok(())
    }
}

fn read_valid_entries(root: &Path) -> Result<Vec<StoredProposal>> {
    let index = read_index(root)?;
    let mut entries = Vec::with_capacity(index.entries.len());
    for mut entry in index.entries {
        if entry.group_id.is_empty() {
            entry.group_id = entry.id.clone();
        }
        if valid_entry(&entry, root).is_ok() {
            entries.push(entry);
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.completed_unix_ms));
    Ok(entries)
}

fn retain_entries(mut entries: Vec<StoredProposal>) -> Result<Retention> {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.completed_unix_ms));
    let mut index = HistoryIndex::default();
    let mut kept_bytes = 0_u64;
    let mut dropped = Vec::new();
    for entry in entries {
        let bytes = entry
            .assets
            .iter()
            .map(|asset| asset.byte_len)
            .chain(entry.context_assets.iter().map(|asset| asset.byte_len))
            .fold(0_u64, u64::saturating_add);
        if index.entries.len() >= MAX_ENTRIES
            || kept_bytes.saturating_add(bytes) > MAX_TOTAL_ASSET_BYTES
        {
            dropped.push(entry);
            continue;
        }
        index.entries.push(entry);
        // Count the exact on-disk representation, including JSON escaping and
        // formatting. Raw plan lengths alone do not bound index size.
        if serde_json::to_vec_pretty(&index)?.len() as u64 > MAX_INDEX_BYTES {
            dropped.push(index.entries.pop().expect("just inserted history entry"));
        } else {
            kept_bytes = kept_bytes.saturating_add(bytes);
        }
    }
    Ok(Retention { index, dropped })
}

fn remove_unreferenced_artifacts(root: &Path, entry: &StoredProposal, kept: &[StoredProposal]) {
    for file in entry
        .assets
        .iter()
        .map(|asset| &asset.file)
        .chain(entry.context_assets.iter().map(|asset| &asset.file))
    {
        let referenced = kept.iter().any(|entry| {
            entry.assets.iter().any(|asset| &asset.file == file)
                || entry.context_assets.iter().any(|asset| &asset.file == file)
        });
        if !referenced {
            let _ = fs::remove_file(root.join(file));
        }
    }
}

fn lock_history(root: &Path) -> Result<File> {
    let path = root.join(".history.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.file_type().is_file(),
            "AI history lock is not a regular file"
        );
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "AI history lock is not a regular file"
    );
    // Keep this inode for the lifetime of the store. Replacing/removing it
    // would let windows lock different files and race their index updates.
    file.lock().context("Cannot lock AI result history")?;
    Ok(file)
}

fn validate_input(input: &NewProposal) -> Result<()> {
    uuid::Uuid::parse_str(&input.id).context("invalid AI result ID")?;
    uuid::Uuid::parse_str(&input.group_id).context("invalid AI result group ID")?;
    valid_source(&input.source)?;
    ensure!(
        input.prompt.len() <= MAX_PROMPT_BYTES,
        "AI prompt is too large"
    );
    ensure!(
        input.summary.len() <= MAX_SUMMARY_BYTES,
        "AI result summary is too large"
    );
    if let Some(plan) = &input.plan_json {
        ensure!(plan.len() <= MAX_PLAN_BYTES, "AI result plan is too large");
    }
    ensure!(
        serde_json::to_vec(&input.provenance)?.len() <= MAX_PROVENANCE_BYTES,
        "AI result provenance is too large"
    );
    ensure!(
        input.assets.len() <= MAX_ASSETS_PER_ENTRY,
        "AI result has too many assets to retain"
    );
    ensure!(
        input.variation_total > 0
            && input.variation_total <= 4
            && input.variation_index > 0
            && input.variation_index <= input.variation_total,
        "AI result variation identity is invalid"
    );
    let asset_total = input.assets.iter().try_fold(0_u64, |total, asset| {
        let metadata = fs::symlink_metadata(&asset.path)?;
        ensure!(
            metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == asset.byte_len
                && metadata.len() <= MAX_ENTRY_ASSET_BYTES,
            "AI result asset is not a supported regular file"
        );
        Ok::<_, anyhow::Error>(total.saturating_add(asset.byte_len))
    })?;
    ensure!(
        asset_total <= MAX_TOTAL_ASSET_BYTES,
        "AI result assets exceed history limit"
    );
    ensure!(
        input.context_assets.len() <= MAX_CONTEXT_ASSETS_PER_ENTRY,
        "AI result has too much retained input context"
    );
    let context_total = input
        .context_assets
        .iter()
        .try_fold(0_u64, |total, context| {
            ensure!(
                !context.media_type.is_empty() && context.media_type.len() <= 128,
                "AI result context media type is invalid"
            );
            ensure!(
                !context.content_hash.is_empty() && context.content_hash.len() <= 128,
                "AI result context hash is invalid"
            );
            let metadata = fs::symlink_metadata(&context.path)?;
            ensure!(
                metadata.file_type().is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() == context.byte_len
                    && metadata.len() <= MAX_CONTEXT_ASSET_BYTES,
                "AI result context changed before it could be retained"
            );
            Ok::<_, anyhow::Error>(total.saturating_add(context.byte_len))
        })?;
    ensure!(
        context_total <= MAX_TOTAL_CONTEXT_BYTES,
        "AI result input context exceeds history limit"
    );
    ensure!(
        asset_total.saturating_add(context_total) <= MAX_TOTAL_ASSET_BYTES,
        "AI result and input context exceed history limit"
    );
    Ok(())
}

fn valid_entry(entry: &StoredProposal, root: &Path) -> Result<()> {
    uuid::Uuid::parse_str(&entry.id).context("invalid AI history record ID")?;
    uuid::Uuid::parse_str(&entry.group_id).context("invalid AI result group ID")?;
    valid_source(&entry.source)?;
    ensure!(
        entry.prompt.len() <= MAX_PROMPT_BYTES,
        "AI history prompt is too large"
    );
    ensure!(
        entry.summary.len() <= MAX_SUMMARY_BYTES,
        "AI history summary is too large"
    );
    if let Some(plan) = &entry.plan_json {
        ensure!(plan.len() <= MAX_PLAN_BYTES, "AI history plan is too large");
    }
    ensure!(
        serde_json::to_vec(&entry.provenance)?.len() <= MAX_PROVENANCE_BYTES,
        "AI history provenance is too large"
    );
    ensure!(
        entry.assets.len() <= MAX_ASSETS_PER_ENTRY,
        "AI history has too many assets"
    );
    ensure!(
        entry.variation_total > 0
            && entry.variation_total <= 4
            && entry.variation_index > 0
            && entry.variation_index <= entry.variation_total,
        "AI history variation identity is invalid"
    );
    ensure!(
        entry.context_assets.len() <= MAX_CONTEXT_ASSETS_PER_ENTRY,
        "AI history has too much retained input context"
    );
    let mut asset_total = 0_u64;
    for asset in &entry.assets {
        asset.absolute_path(root)?;
        asset_total = asset_total.saturating_add(asset.byte_len);
    }
    let mut context_total = 0_u64;
    for context in &entry.context_assets {
        ensure!(
            !context.media_type.is_empty() && context.media_type.len() <= 128,
            "AI history context media type is invalid"
        );
        ensure!(
            !context.content_hash.is_empty() && context.content_hash.len() <= 128,
            "AI history context hash is invalid"
        );
        context.absolute_path(root)?;
        context_total = context_total.saturating_add(context.byte_len);
    }
    ensure!(
        context_total <= MAX_TOTAL_CONTEXT_BYTES,
        "AI history input context exceeds history limit"
    );
    ensure!(
        asset_total.saturating_add(context_total) <= MAX_TOTAL_ASSET_BYTES,
        "AI history entry exceeds retained storage limit"
    );
    Ok(())
}

fn valid_source(source: &SourceIdentity) -> Result<()> {
    ensure!(
        !source.document_id.is_empty() && source.document_id.len() <= 128,
        "AI source document identity is invalid"
    );
    ensure!(
        !source.session_id.is_empty() && source.session_id.len() <= 128,
        "AI source session identity is invalid"
    );
    for identity in [&source.project_id, &source.page_id] {
        if let Some(identity) = identity {
            ensure!(identity.len() <= 128, "AI source identity is too long");
        }
    }
    Ok(())
}

fn read_index(root: &Path) -> Result<HistoryIndex> {
    let path = root.join("history.json");
    if !path.exists() {
        return Ok(HistoryIndex::default());
    }
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_INDEX_BYTES,
        "AI history index is not a supported regular file"
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take(MAX_INDEX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_INDEX_BYTES,
        "AI history index is too large"
    );
    let index: HistoryIndex = serde_json::from_slice(&bytes).context("Invalid AI history")?;
    ensure!(index.version == 1, "Unsupported AI history version");
    ensure!(
        index.entries.len() <= MAX_ENTRIES.saturating_mul(4),
        "AI history contains too many records"
    );
    Ok(index)
}

fn write_index(root: &Path, index: &HistoryIndex) -> Result<IndexPublication> {
    write_index_with_sync(root, index, |directory| directory.sync_all())
}

fn write_index_with_sync(
    root: &Path,
    index: &HistoryIndex,
    sync_directory: impl FnOnce(&File) -> std::io::Result<()>,
) -> Result<IndexPublication> {
    let bytes = serde_json::to_vec_pretty(index)?;
    ensure!(
        bytes.len() as u64 <= MAX_INDEX_BYTES,
        "AI history index is too large"
    );
    let temporary = root.join(format!(".history-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| -> Result<IndexPublication> {
        let directory = crate::durable_fs::open_for_sync(root)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, root.join("history.json"))?;
        Ok(match sync_directory(&directory) {
            Ok(()) => IndexPublication::Durable,
            Err(error) => IndexPublication::VisibleButUnsynced(error.into()),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "AI history directory is not a real directory"
        );
    } else {
        fs::create_dir_all(path)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    ensure!(!path.is_absolute(), "AI history path is absolute");
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "AI history path escapes its store"
    );
    Ok(path.to_owned())
}

fn copy_regular_file(source: &Path, destination: &Path, expected_bytes: u64) -> Result<()> {
    copy_regular_file_limited(source, destination, expected_bytes, MAX_ENTRY_ASSET_BYTES)
}

fn copy_regular_file_limited(
    source: &Path,
    destination: &Path,
    expected_bytes: u64,
    byte_limit: u64,
) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == expected_bytes
            && expected_bytes <= byte_limit,
        "AI result asset changed before it could be retained"
    );
    let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut input = File::open(source)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&temporary)?;
        copy_exact_bounded(&mut input, &mut output, expected_bytes)?;
        output.sync_all()?;
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn copy_exact_bounded(
    input: &mut impl Read,
    output: &mut impl Write,
    expected_bytes: u64,
) -> Result<()> {
    let copied = std::io::copy(&mut (&mut *input).take(expected_bytes), output)?;
    // Detect a growing source without writing even one byte beyond the
    // declared budget, or draining an endlessly growing producer.
    let mut excess = [0_u8; 1];
    ensure!(
        copied == expected_bytes && input.read(&mut excess)? == 0,
        "AI result asset changed while being retained"
    );
    Ok(())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::{
        collections::BTreeSet,
        sync::{Arc, Barrier},
    };

    fn source() -> SourceIdentity {
        SourceIdentity {
            document_id: "doc-a".into(),
            project_id: Some("project-a".into()),
            page_id: Some("page-a".into()),
            session_id: "window-a".into(),
            epoch: 7,
            revision: 12,
            selection_revision: 4,
            project_generation: Some(3),
        }
    }

    fn input(path: PathBuf) -> NewProposal {
        let bytes = fs::metadata(&path).unwrap().len();
        NewProposal {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: uuid::Uuid::new_v4().to_string(),
            variation_index: 1,
            variation_total: 1,
            source: source(),
            operation: Operation::GenerateImage,
            provider: ProviderId::CodexSubscription,
            prompt: "A muted coastal poster".into(),
            summary: "Coastal poster alternative".into(),
            plan_json: None,
            assets: vec![ResultAsset {
                path,
                media_type: "image/png".into(),
                width: 2,
                height: 2,
                byte_len: bytes,
                provider_item_id: Some("provider-result".into()),
            }],
            context_assets: vec![],
            image_edit: false,
            provenance: serde_json::json!({"intent": "generate"}),
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, HistoryStore) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("result.png");
        ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([10, 20, 30, 255]))
            .save(&source)
            .unwrap();
        let history = HistoryStore::open(directory.path().join("history")).unwrap();
        (directory, source, history)
    }

    fn artifact_files(root: &Path) -> BTreeSet<PathBuf> {
        ["assets", "context"]
            .into_iter()
            .flat_map(|directory| {
                fs::read_dir(root.join(directory))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
            })
            .collect()
    }

    fn fail_index_rename(root: &Path, index: &HistoryIndex) -> Result<IndexPublication> {
        // Exercise a real failed atomic rename without depending on the
        // account's effective permissions or filesystem capacity.
        let path = root.join("history.json");
        let backup = root.join("history.backup");
        fs::rename(&path, &backup).unwrap();
        fs::create_dir(&path).unwrap();
        let result = write_index(root, index);
        fs::remove_dir(&path).unwrap();
        fs::rename(&backup, &path).unwrap();
        assert!(result.is_err());
        result
    }

    fn fail_directory_sync(root: &Path, index: &HistoryIndex) -> Result<IndexPublication> {
        write_index_with_sync(root, index, |_| {
            Err(std::io::Error::other("injected directory sync failure"))
        })
    }

    #[test]
    fn stale_windows_merge_new_results_and_do_not_resurrect_dismissed_results() {
        let (_directory, source, mut first) = fixture();
        let mut second = HistoryStore::open(first.root()).unwrap();
        let a = first.persist(input(source.clone())).unwrap();
        let b = second.persist(input(source.clone())).unwrap();
        assert_eq!(second.entries(), &[b.clone(), a.clone()]);
        // First has never seen b, but must still be able to dismiss it.
        assert!(first.discard(&b.id).unwrap());
        let c = second.persist(input(source)).unwrap();
        let reopened = HistoryStore::open(first.root()).unwrap();
        assert_eq!(reopened.entries(), &[c, a]);
        assert!(!second.discard(&b.id).unwrap());
        assert_eq!(second.entries(), reopened.entries());
        assert!(!first.root().join(&b.assets[0].file).exists());
    }

    #[test]
    fn duplicate_from_a_stale_window_preserves_the_first_result_and_its_artifact() {
        let (_directory, source, mut first) = fixture();
        let mut second = HistoryStore::open(first.root()).unwrap();
        let proposal = input(source);
        let retained = first.persist(proposal.clone()).unwrap();
        let bytes = fs::read(first.root().join(&retained.assets[0].file)).unwrap();
        let files = artifact_files(first.root());
        let error = second.persist(proposal).unwrap_err();
        assert!(error.to_string().contains("already retained"));
        assert_eq!(artifact_files(first.root()), files);
        assert_eq!(
            fs::read(first.root().join(&retained.assets[0].file)).unwrap(),
            bytes
        );
        assert_eq!(
            HistoryStore::open(first.root()).unwrap().entries(),
            &[retained]
        );
    }

    #[test]
    fn concurrent_windows_retain_every_completed_result() {
        let (_directory, source, history) = fixture();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let mut window = HistoryStore::open(history.root()).unwrap();
                let proposal = input(source.clone());
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    window.persist(proposal).unwrap().id
                })
            })
            .collect();
        let expected: BTreeSet<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let reopened = HistoryStore::open(history.root()).unwrap();
        let actual: BTreeSet<_> = reopened
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(artifact_files(history.root()).len(), 8);
        for entry in reopened.entries() {
            valid_entry(entry, history.root()).unwrap();
        }
    }

    #[test]
    fn history_lock_is_exclusive_and_released_when_the_operation_ends() {
        let (_directory, _source, history) = fixture();
        let lock = lock_history(history.root()).unwrap();
        let contender = File::open(history.root().join(".history.lock")).unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(lock);
        contender.try_lock().unwrap();
    }

    #[test]
    fn failed_publication_while_pruning_preserves_all_previous_results() {
        let (_directory, source, mut history) = fixture();
        for _ in 0..MAX_ENTRIES {
            history.persist(input(source.clone())).unwrap();
        }
        let previous = history.entries().to_vec();
        let index = fs::read(history.root().join("history.json")).unwrap();
        let files = artifact_files(history.root());
        assert!(
            history
                .persist_with_writer(input(source.clone()), fail_index_rename)
                .is_err()
        );
        assert_eq!(history.entries(), previous);
        assert_eq!(
            fs::read(history.root().join("history.json")).unwrap(),
            index
        );
        assert_eq!(artifact_files(history.root()), files);
        assert_eq!(
            HistoryStore::open(history.root()).unwrap().entries(),
            previous
        );
        assert!(!fs::read_dir(history.root()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        // A subsequent successful save still prunes exactly the oldest row.
        let latest = history.persist(input(source)).unwrap();
        assert_eq!(history.entries()[0], latest);
        assert_eq!(&history.entries()[1..], &previous[..MAX_ENTRIES - 1]);
        assert!(
            !history
                .root()
                .join(&previous.last().unwrap().assets[0].file)
                .exists()
        );
    }

    #[test]
    fn failed_discard_preserves_the_result_and_private_context() {
        let (_directory, source, mut history) = fixture();
        let mut proposal = input(source.clone());
        proposal.context_assets.push(NewContextAsset {
            path: source.clone(),
            media_type: "image/png".into(),
            byte_len: fs::metadata(source).unwrap().len(),
            role: ContextRole::Reference,
            content_hash: "fnv1a64:reference".into(),
        });
        let saved = history.persist(proposal).unwrap();
        let files = artifact_files(history.root());
        assert!(
            history
                .discard_with_writer(&saved.id, fail_index_rename)
                .is_err()
        );
        assert_eq!(history.entries(), &[saved.clone()]);
        assert_eq!(artifact_files(history.root()), files);
        assert_eq!(
            HistoryStore::open(history.root()).unwrap().entries(),
            &[saved]
        );
    }

    #[test]
    fn sync_failure_after_rename_keeps_files_for_both_index_generations() {
        let (_directory, source, mut history) = fixture();
        for _ in 0..MAX_ENTRIES {
            history.persist(input(source.clone())).unwrap();
        }
        let previous_files = artifact_files(history.root());
        let proposal = input(source);
        let new_id = proposal.id.to_uppercase();
        let error = history
            .persist_with_writer(proposal, fail_directory_sync)
            .unwrap_err();
        assert!(error.to_string().contains("durability"));
        assert_eq!(history.entries()[0].id, new_id);
        assert_eq!(
            read_index(history.root()).unwrap().entries,
            history.entries()
        );
        let files = artifact_files(history.root());
        assert!(previous_files.is_subset(&files));
        assert_eq!(files.len(), MAX_ENTRIES + 1);
        assert_eq!(
            HistoryStore::open(history.root()).unwrap().entries(),
            history.entries()
        );
        for entry in history.entries() {
            valid_entry(entry, history.root()).unwrap();
        }
    }

    #[test]
    fn discard_sync_failure_tracks_the_visible_index_without_deleting_old_files() {
        let (_directory, source, mut history) = fixture();
        let saved = history.persist(input(source)).unwrap();
        let files = artifact_files(history.root());
        assert!(
            history
                .discard_with_writer(&saved.id, fail_directory_sync)
                .is_err()
        );
        assert!(history.entries().is_empty());
        assert!(read_index(history.root()).unwrap().entries.is_empty());
        assert_eq!(artifact_files(history.root()), files);
    }

    #[test]
    fn large_plan_history_prunes_to_the_serialized_index_budget() {
        let (_directory, source, mut history) = fixture();
        let mut saved = Vec::new();
        for _ in 0..3 {
            let mut proposal = input(source.clone());
            proposal.plan_json = Some(
                serde_json::json!({
                    "summary": "x".repeat(200 * 1024), "operations": []
                })
                .to_string(),
            );
            saved.push(history.persist(proposal).unwrap());
        }
        assert_eq!(history.entries(), &[saved[2].clone(), saved[1].clone()]);
        assert!(
            fs::metadata(history.root().join("history.json"))
                .unwrap()
                .len()
                <= MAX_INDEX_BYTES
        );
        assert!(!history.root().join(&saved[0].assets[0].file).exists());
        assert_eq!(artifact_files(history.root()).len(), 2);
        assert_eq!(
            HistoryStore::open(history.root()).unwrap().entries(),
            history.entries()
        );
    }

    #[test]
    fn a_single_escaped_plan_over_budget_cannot_displace_retained_results() {
        let (_directory, source, mut history) = fixture();
        let saved = history.persist(input(source.clone())).unwrap();
        let files = artifact_files(history.root());
        let mut oversized = input(source);
        let prefix = "{\"operations\":[]}";
        let plan = format!("{prefix}{}", "\n".repeat(MAX_PLAN_BYTES - prefix.len()));
        serde_json::from_str::<serde_json::Value>(&plan).unwrap();
        oversized.plan_json = Some(plan);
        validate_input(&oversized).unwrap();
        let error = history.persist(oversized).unwrap_err();
        assert!(error.to_string().contains("history budget"));
        assert_eq!(history.entries(), &[saved.clone()]);
        assert_eq!(artifact_files(history.root()), files);
        assert_eq!(
            HistoryStore::open(history.root()).unwrap().entries(),
            &[saved]
        );
    }

    #[test]
    fn bounded_copy_rejects_growth_without_writing_past_the_declared_size() {
        struct GrowingSource {
            read: usize,
        }
        impl Read for GrowingSource {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes.fill(42);
                self.read += bytes.len();
                Ok(bytes.len())
            }
        }
        let mut source = GrowingSource { read: 0 };
        let mut output = Vec::new();
        assert!(copy_exact_bounded(&mut source, &mut output, 4096).is_err());
        assert_eq!(source.read, 4097);
        assert_eq!(output, vec![42; 4096]);
    }

    #[test]
    fn bounded_copy_accepts_exact_lengths_and_rejects_shrinking_sources() {
        for bytes in [b"".as_slice(), b"result bytes".as_slice()] {
            let mut output = Vec::new();
            copy_exact_bounded(&mut &bytes[..], &mut output, bytes.len() as u64).unwrap();
            assert_eq!(output, bytes);
        }
        let mut output = Vec::new();
        assert!(copy_exact_bounded(&mut b"short".as_slice(), &mut output, 20).is_err());
        assert_eq!(output, b"short");
        assert!(copy_exact_bounded(&mut b"grew".as_slice(), &mut Vec::new(), 0).is_err());
    }

    #[test]
    fn completed_result_survives_reopen_with_source_identity() {
        let directory = tempfile::tempdir().unwrap();
        let input_path = directory.path().join("result.png");
        ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&input_path)
            .unwrap();
        let root = directory.path().join("history");
        let stored = HistoryStore::open(&root)
            .unwrap()
            .persist(input(input_path))
            .unwrap();
        let reopened = HistoryStore::open(&root).unwrap();
        assert_eq!(reopened.entries(), &[stored.clone()]);
        assert_eq!(reopened.entries()[0].source, source());
        assert!(
            reopened.entries()[0].assets[0]
                .absolute_path(reopened.root())
                .unwrap()
                .is_file()
        );
    }

    #[test]
    fn discard_removes_the_indexed_result_and_private_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let input_path = directory.path().join("result.png");
        ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([9, 8, 7, 255]))
            .save(&input_path)
            .unwrap();
        let root = directory.path().join("history");
        let mut history = HistoryStore::open(&root).unwrap();
        let stored = history.persist(input(input_path)).unwrap();
        let artifact = stored.assets[0].absolute_path(history.root()).unwrap();
        assert!(history.discard(&stored.id).unwrap());
        assert!(!artifact.exists());
        assert!(HistoryStore::open(&root).unwrap().entries().is_empty());
    }

    #[test]
    fn explicit_reference_context_survives_reopen_and_is_removed_with_review_copy() {
        let directory = tempfile::tempdir().unwrap();
        let result_path = directory.path().join("result.png");
        let reference_path = directory.path().join("reference.png");
        ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([9, 8, 7, 255]))
            .save(&result_path)
            .unwrap();
        ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([1, 3, 5, 255]))
            .save(&reference_path)
            .unwrap();
        let bytes = fs::metadata(&reference_path).unwrap().len();
        let mut proposal = input(result_path);
        proposal.context_assets.push(NewContextAsset {
            path: reference_path,
            media_type: "image/png".into(),
            byte_len: bytes,
            role: ContextRole::Reference,
            content_hash: "fnv1a64:test".into(),
        });
        let root = directory.path().join("history");
        let mut history = HistoryStore::open(&root).unwrap();
        let stored = history.persist(proposal).unwrap();
        let context = stored.context_assets[0]
            .absolute_path(history.root())
            .unwrap();
        assert!(context.is_file());
        let reopened = HistoryStore::open(&root).unwrap();
        assert_eq!(reopened.entries()[0].context_assets.len(), 1);
        assert!(history.discard(&stored.id).unwrap());
        assert!(!context.exists());
    }

    #[test]
    fn reject_path_escape_in_persisted_history() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("history");
        ensure_private_directory(&root).unwrap();
        ensure_private_directory(&root.join("assets")).unwrap();
        let entry = StoredProposal {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: uuid::Uuid::new_v4().to_string(),
            variation_index: 1,
            variation_total: 1,
            source: source(),
            operation: Operation::Assistant,
            provider: ProviderId::CodexSubscription,
            prompt: "x".into(),
            summary: "x".into(),
            plan_json: Some("{\"summary\":\"x\",\"operations\":[]}".into()),
            assets: vec![StoredAsset {
                file: "../outside.png".into(),
                media_type: "image/png".into(),
                width: 1,
                height: 1,
                byte_len: 1,
                provider_item_id: None,
            }],
            context_assets: vec![],
            image_edit: false,
            provenance: serde_json::Value::Null,
            completed_unix_ms: 1,
        };
        write_index(
            &root,
            &HistoryIndex {
                version: 1,
                entries: vec![entry],
            },
        )
        .unwrap();
        assert!(HistoryStore::open(&root).unwrap().entries().is_empty());
    }
}
