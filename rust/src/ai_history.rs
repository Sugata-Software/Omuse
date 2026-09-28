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

impl HistoryStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_owned();
        ensure!(!root.as_os_str().is_empty(), "missing AI history directory");
        ensure_private_directory(&root)?;
        ensure_private_directory(&root.join("assets"))?;
        ensure_private_directory(&root.join("context"))?;
        let index = read_index(&root)?;
        let mut entries = Vec::with_capacity(index.entries.len());
        for mut entry in index.entries {
            if entry.group_id.is_empty() {
                entry.group_id = entry.id.clone();
            }
            if valid_entry(&entry, &root).is_ok() {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.completed_unix_ms));
        let mut store = Self { root, entries };
        // Keep every valid entry through pruning so files for old alternatives
        // are removed with their index records rather than becoming private
        // orphaned bytes after a restart.
        store.prune_and_publish()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[StoredProposal] {
        &self.entries
    }

    pub fn persist(&mut self, input: NewProposal) -> Result<StoredProposal> {
        validate_input(&input)?;
        let id = input.id.to_uppercase();
        ensure!(
            !self.entries.iter().any(|entry| entry.id == id),
            "AI result is already retained"
        );
        let mut stored_assets: Vec<StoredAsset> = Vec::with_capacity(input.assets.len());
        for (index, asset) in input.assets.iter().enumerate() {
            let file = format!("assets/{id}-{index}.asset");
            let destination = self.root.join(&file);
            if let Err(error) = copy_regular_file(&asset.path, &destination, asset.byte_len) {
                for copied in &stored_assets {
                    let _ = fs::remove_file(self.root.join(&copied.file));
                }
                return Err(error);
            }
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
            let file = format!("context/{id}-{index}.asset");
            let destination = self.root.join(&file);
            if let Err(error) = copy_regular_file_limited(
                &context.path,
                &destination,
                context.byte_len,
                MAX_CONTEXT_ASSET_BYTES,
            ) {
                for copied in &stored_assets {
                    let _ = fs::remove_file(self.root.join(&copied.file));
                }
                for copied in &stored_context {
                    let _ = fs::remove_file(self.root.join(&copied.file));
                }
                return Err(error);
            }
            stored_context.push(StoredContextAsset {
                file,
                media_type: context.media_type.clone(),
                byte_len: context.byte_len,
                role: context.role,
                content_hash: context.content_hash.clone(),
            });
        }
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
        self.entries.insert(0, entry.clone());
        if let Err(error) = self.prune_and_publish() {
            self.entries.retain(|item| item.id != entry.id);
            for asset in &entry.assets {
                let _ = fs::remove_file(self.root.join(&asset.file));
            }
            for context in &entry.context_assets {
                let _ = fs::remove_file(self.root.join(&context.file));
            }
            return Err(error);
        }
        Ok(entry)
    }

    /// Removes a dismissed alternative and its private review artifacts.
    /// Project and library resources are outside this store and are never
    /// removed by dismissal. The index is published before files are removed,
    /// so a crash can leave harmless orphaned bytes but cannot resurrect a
    /// dismissed result.
    pub fn discard(&mut self, id: &str) -> Result<bool> {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return Ok(false);
        };
        let entry = self.entries.remove(index);
        if let Err(error) = self.prune_and_publish() {
            self.entries.insert(index, entry);
            return Err(error);
        }
        for asset in entry.assets {
            let _ = fs::remove_file(self.root.join(asset.file));
        }
        for context in entry.context_assets {
            let _ = fs::remove_file(self.root.join(context.file));
        }
        Ok(true)
    }

    fn prune_and_publish(&mut self) -> Result<()> {
        let mut prior = std::mem::take(&mut self.entries);
        prior.sort_by_key(|entry| std::cmp::Reverse(entry.completed_unix_ms));
        let mut kept_bytes = 0_u64;
        let mut kept = Vec::with_capacity(prior.len().min(MAX_ENTRIES));
        let mut dropped = Vec::new();
        for entry in prior {
            let bytes = entry
                .assets
                .iter()
                .fold(0_u64, |total, asset| total.saturating_add(asset.byte_len))
                .saturating_add(
                    entry
                        .context_assets
                        .iter()
                        .fold(0_u64, |total, asset| total.saturating_add(asset.byte_len)),
                );
            if kept.len() < MAX_ENTRIES && kept_bytes.saturating_add(bytes) <= MAX_TOTAL_ASSET_BYTES
            {
                kept_bytes = kept_bytes.saturating_add(bytes);
                kept.push(entry);
            } else {
                dropped.push(entry);
            }
        }
        self.entries = kept;
        write_index(
            &self.root,
            &HistoryIndex {
                version: 1,
                entries: self.entries.clone(),
            },
        )?;
        for entry in dropped {
            for asset in entry.assets {
                let _ = fs::remove_file(self.root.join(asset.file));
            }
            for context in entry.context_assets {
                let _ = fs::remove_file(self.root.join(context.file));
            }
        }
        Ok(())
    }
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

fn write_index(root: &Path, index: &HistoryIndex) -> Result<()> {
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
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, root.join("history.json"))?;
        File::open(root)?.sync_all()?;
        Ok(())
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
        let copied = std::io::copy(&mut input, &mut output)?;
        ensure!(
            copied == expected_bytes,
            "AI result asset changed while being retained"
        );
        output.sync_all()?;
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
