//! Local, content-addressed Create asset library.
//!
//! Pixel bounds are checked before decode, imported bytes are hashed while
//! copied, and metadata changes are published atomically.  The library never
//! follows a symlink for its root, metadata file, asset directory, or imports.

use crate::model::valid_dimensions;
use anyhow::{Context, Result, bail, ensure};
use image::{ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_LIBRARY_ASSETS: usize = 50_000;
pub const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_LIBRARY_VERIFY_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TAGS: usize = 64;
const MAX_TAG_BYTES: usize = 64;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_QUERY_BYTES: usize = 512;
const MAX_SEARCH_RESULTS: usize = 1_000;
const LOCK_ATTEMPTS: usize = 200;

#[derive(Clone, Debug, Default)]
pub struct ImportMetadata {
    pub tags: BTreeSet<String>,
    pub favorite: bool,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    pub sha256: String,
    pub storage_name: String,
    pub name: String,
    /// Compatibility/display alias retained for callers that distinguish the
    /// imported filename from a future editable library title.
    #[serde(default)]
    pub original_name: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub tags: BTreeSet<String>,
    pub favorite: bool,
    pub provenance: String,
    pub imported_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LibraryIndex {
    format_version: u32,
    #[serde(default)]
    revision: u64,
    assets: Vec<AssetRecord>,
}

pub struct AssetLibrary {
    root: PathBuf,
    index: LibraryIndex,
}

impl AssetLibrary {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_owned();
        ensure!(!root.as_os_str().is_empty(), "missing asset library path");
        ensure_real_directory(&root, true)?;
        let assets = root.join("assets");
        ensure_real_directory(&assets, true)?;
        let index = load_index(&root)?;
        Ok(Self { root, index })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn assets(&self) -> &[AssetRecord] {
        &self.index.assets
    }

    pub fn get(&self, id: &str) -> Option<&AssetRecord> {
        self.index.assets.iter().find(|asset| asset.id == id)
    }

    pub fn asset_path(&self, id: &str) -> Result<PathBuf> {
        let asset = self.get(id).context("Unknown library asset")?;
        Ok(self.root.join("assets").join(&asset.storage_name))
    }

    /// Import a supported still image. Identical bytes reuse the existing
    /// record and merge the supplied tags/favourite state.
    pub fn import(
        &mut self,
        source: impl AsRef<Path>,
        metadata: ImportMetadata,
    ) -> Result<AssetRecord> {
        validate_import_metadata(&metadata)?;
        let source = source.as_ref();
        ensure_regular_file(source, MAX_ASSET_BYTES)?;
        let (source_format, _, _) = inspect_image(source)?;
        let extension = extension(source_format).context("Unsupported library image format")?;

        let temporary = self
            .root
            .join("assets")
            .join(format!(".import-{}.tmp", uuid::Uuid::new_v4()));
        let copy_result = copy_and_hash(source, &temporary);
        let (digest, bytes) = match copy_result {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
        };
        let _lock = LibraryLock::acquire(&self.root)?;
        self.index = load_index(&self.root)?;
        ensure!(
            self.index.assets.len() < MAX_LIBRARY_ASSETS,
            "asset library has reached its 50,000 item limit"
        );
        let outcome = (|| -> Result<AssetRecord> {
            let (copied_format, width, height) = inspect_image(&temporary)?;
            ensure!(
                copied_format == source_format,
                "source changed while it was being imported"
            );
            let storage_name = format!("{digest}.{extension}");
            let storage_path = self.root.join("assets").join(&storage_name);

            if let Some(existing_index) = self
                .index
                .assets
                .iter()
                .position(|asset| asset.sha256 == digest)
            {
                let existing = &self.index.assets[existing_index];
                ensure!(
                    existing.bytes == bytes && existing.width == width && existing.height == height,
                    "duplicate asset metadata is inconsistent"
                );
                let _ = fs::remove_file(&temporary);
                let mut next = self.index.clone();
                next.assets[existing_index].tags.extend(metadata.tags);
                next.assets[existing_index].favorite |= metadata.favorite;
                if next.assets[existing_index].provenance.is_empty() {
                    next.assets[existing_index].provenance = metadata.provenance;
                }
                next.revision = next
                    .revision
                    .checked_add(1)
                    .context("asset library revision overflow")?;
                self.publish_index(&next)?;
                self.index = next;
                return Ok(self.index.assets[existing_index].clone());
            }

            if storage_path.exists() {
                ensure_regular_file(&storage_path, MAX_ASSET_BYTES)?;
                ensure!(
                    hash_file(&storage_path)?.0 == digest,
                    "content-addressed asset path contains different bytes"
                );
                fs::remove_file(&temporary)?;
            } else {
                match fs::hard_link(&temporary, &storage_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        ensure_regular_file(&storage_path, MAX_ASSET_BYTES)?;
                        ensure!(
                            hash_file(&storage_path)?.0 == digest,
                            "content-addressed asset path contains different bytes"
                        );
                    }
                    Err(error) => return Err(error).context("Publishing library asset"),
                }
                let _ = fs::remove_file(&temporary);
                File::open(&self.root.join("assets"))?.sync_all()?;
            }

            let name = clean_name(
                source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Imported image"),
            );
            let record = AssetRecord {
                id: uuid::Uuid::new_v4().to_string().to_uppercase(),
                sha256: digest,
                storage_name,
                name: name.clone(),
                original_name: name,
                media_type: media_type(copied_format).into(),
                width,
                height,
                bytes,
                tags: metadata.tags,
                favorite: metadata.favorite,
                provenance: metadata.provenance,
                imported_unix_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            };
            let mut next = self.index.clone();
            next.assets.push(record.clone());
            next.revision = next
                .revision
                .checked_add(1)
                .context("asset library revision overflow")?;
            self.publish_index(&next)?;
            self.index = next;
            Ok(record)
        })();
        let _ = fs::remove_file(&temporary);
        outcome
    }

    pub fn set_tags(&mut self, id: &str, tags: BTreeSet<String>) -> Result<()> {
        validate_tags(&tags)?;
        let _lock = LibraryLock::acquire(&self.root)?;
        self.index = load_index(&self.root)?;
        let mut next = self.index.clone();
        let asset = next
            .assets
            .iter_mut()
            .find(|asset| asset.id == id)
            .context("Unknown library asset")?;
        asset.tags = tags;
        next.revision = next
            .revision
            .checked_add(1)
            .context("asset library revision overflow")?;
        self.publish_index(&next)?;
        self.index = next;
        Ok(())
    }

    pub fn set_favorite(&mut self, id: &str, favorite: bool) -> Result<()> {
        let _lock = LibraryLock::acquire(&self.root)?;
        self.index = load_index(&self.root)?;
        let mut next = self.index.clone();
        let asset = next
            .assets
            .iter_mut()
            .find(|asset| asset.id == id)
            .context("Unknown library asset")?;
        asset.favorite = favorite;
        next.revision = next
            .revision
            .checked_add(1)
            .context("asset library revision overflow")?;
        self.publish_index(&next)?;
        self.index = next;
        Ok(())
    }

    /// Case-insensitive AND search across the original name, tags and
    /// provenance. Favourite results sort first, then newest imports.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<&AssetRecord>> {
        ensure!(
            query.len() <= MAX_QUERY_BYTES,
            "asset search query is too long"
        );
        ensure!(
            limit <= MAX_SEARCH_RESULTS,
            "asset search limit is too large"
        );
        if limit == 0 {
            return Ok(Vec::new());
        }
        let terms = query
            .split_whitespace()
            .map(|term| term.to_lowercase())
            .collect::<Vec<_>>();
        let mut found = self
            .index
            .assets
            .iter()
            .filter(|asset| {
                let searchable = format!(
                    "{} {} {}",
                    asset.name,
                    asset.provenance,
                    asset.tags.iter().cloned().collect::<Vec<_>>().join(" ")
                )
                .to_lowercase();
                terms.iter().all(|term| searchable.contains(term))
            })
            .collect::<Vec<_>>();
        found.sort_by(|left, right| {
            right
                .favorite
                .cmp(&left.favorite)
                .then_with(|| right.imported_unix_ms.cmp(&left.imported_unix_ms))
                .then_with(|| left.name.cmp(&right.name))
        });
        found.truncate(limit);
        Ok(found)
    }

    /// Re-hash every indexed asset and reject missing, replaced or corrupted
    /// content. This is explicit because a large library may take time.
    pub fn verify_integrity(&self) -> Result<()> {
        let mut checked = 0u64;
        for asset in &self.index.assets {
            checked = checked
                .checked_add(asset.bytes)
                .context("library byte count overflow")?;
            ensure!(
                checked <= MAX_LIBRARY_VERIFY_BYTES,
                "library exceeds the integrity verification byte limit"
            );
            let path = self.root.join("assets").join(&asset.storage_name);
            ensure_regular_file(&path, MAX_ASSET_BYTES)?;
            let (digest, bytes) = hash_file(&path)?;
            ensure!(bytes == asset.bytes, "asset size mismatch: {}", asset.id);
            ensure!(digest == asset.sha256, "asset hash mismatch: {}", asset.id);
            let (format, width, height) = inspect_image(&path)?;
            ensure!(
                media_type(format) == asset.media_type,
                "asset type mismatch: {}",
                asset.id
            );
            ensure!(
                (width, height) == (asset.width, asset.height),
                "asset dimensions mismatch: {}",
                asset.id
            );
        }
        Ok(())
    }

    /// Refresh search/display state after another Omuse window changes the
    /// shared library.
    pub fn reload(&mut self) -> Result<()> {
        self.index = load_index(&self.root)?;
        Ok(())
    }

    fn publish_index(&self, index: &LibraryIndex) -> Result<()> {
        validate_index(&self.root, index)?;
        let current = load_index(&self.root)?;
        ensure!(
            current.revision.checked_add(1) == Some(index.revision),
            "asset library changed outside its lock; reload and retry"
        );
        let metadata = self.root.join("library.json");
        let temporary = self
            .root
            .join(format!(".library-{}.tmp", uuid::Uuid::new_v4()));
        let outcome = (|| -> Result<()> {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, index)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            ensure!(
                writer.get_ref().metadata()?.len() <= MAX_METADATA_BYTES,
                "asset library metadata exceeds its size limit"
            );
            writer.get_ref().sync_all()?;
            fs::rename(&temporary, &metadata)?;
            // The complete index is already visible. A directory fsync error
            // cannot honestly be reported as an uncommitted metadata change.
            let _ = File::open(&self.root).and_then(|directory| directory.sync_all());
            Ok(())
        })();
        if outcome.is_err() {
            let _ = fs::remove_file(temporary);
        }
        outcome
    }
}

fn load_index(root: &Path) -> Result<LibraryIndex> {
    let metadata = root.join("library.json");
    let index = if metadata.exists() {
        ensure_regular_file(&metadata, MAX_METADATA_BYTES)?;
        let file = File::open(&metadata)?;
        serde_json::from_reader(BufReader::new(file)).context("Invalid asset library metadata")?
    } else {
        LibraryIndex {
            format_version: 1,
            revision: 0,
            assets: Vec::new(),
        }
    };
    validate_index(root, &index)?;
    Ok(index)
}

struct LibraryLock {
    file: File,
}

impl LibraryLock {
    fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".library.lock");
        let file = open_lock_file(&path)?;
        for _ in 0..LOCK_ATTEMPTS {
            match try_lock(&file) {
                Ok(true) => return Ok(Self { file }),
                Ok(false) => thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => return Err(error).context("Locking asset library metadata"),
            }
        }
        bail!("asset library is busy in another Omuse window; try again")
    }
}

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = unlock(&self.file);
    }
}

#[cfg(target_os = "linux")]
fn open_lock_file(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // O_NOFOLLOW rejects a substituted symlink; O_CLOEXEC keeps the lock out
    // of provider/encoder child processes.
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(0x20000 | 0x80000)
        .open(path)?)
}

#[cfg(not(target_os = "linux"))]
fn open_lock_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?)
}

#[cfg(target_os = "linux")]
fn try_lock(file: &File) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    // SAFETY: flock receives a live owned descriptor and retains no pointers.
    let result = unsafe { flock(file.as_raw_fd(), 2 | 4) };
    if result == 0 {
        Ok(true)
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(11) | Some(35)) {
            Ok(false)
        } else {
            Err(error)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn try_lock(_: &File) -> std::io::Result<bool> {
    Ok(true)
}

#[cfg(target_os = "linux")]
fn unlock(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    // SAFETY: flock receives a live owned descriptor and retains no pointers.
    if unsafe { flock(file.as_raw_fd(), 8) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn unlock(_: &File) -> std::io::Result<()> {
    Ok(())
}

fn validate_index(root: &Path, index: &LibraryIndex) -> Result<()> {
    ensure!(
        index.format_version == 1,
        "unsupported asset library version"
    );
    ensure!(
        index.assets.len() <= MAX_LIBRARY_ASSETS,
        "asset library has too many items"
    );
    let mut ids = HashSet::new();
    let mut hashes = HashSet::new();
    for asset in &index.assets {
        ensure!(uuid::Uuid::parse_str(&asset.id).is_ok(), "invalid asset ID");
        ensure!(ids.insert(&asset.id), "duplicate asset ID");
        ensure!(hashes.insert(&asset.sha256), "duplicate asset hash record");
        ensure!(
            asset.sha256.len() == 64 && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid asset hash"
        );
        ensure!(
            valid_dimensions(asset.width, asset.height),
            "invalid asset dimensions"
        );
        ensure!(
            (1..=MAX_ASSET_BYTES).contains(&asset.bytes),
            "invalid asset byte size"
        );
        ensure!(
            !asset.name.is_empty()
                && asset.name.len() <= MAX_TEXT_BYTES
                && !asset.name.chars().any(char::is_control),
            "asset name is too long"
        );
        ensure!(
            asset.original_name.is_empty()
                || (asset.original_name.len() <= MAX_TEXT_BYTES
                    && !asset.original_name.chars().any(char::is_control)),
            "asset original name is invalid"
        );
        ensure!(
            asset.provenance.len() <= MAX_TEXT_BYTES,
            "asset provenance is too long"
        );
        validate_tags(&asset.tags)?;
        let expected_prefix = format!("{}.", asset.sha256);
        ensure!(
            asset.storage_name.starts_with(&expected_prefix)
                && Path::new(&asset.storage_name).components().count() == 1,
            "invalid asset storage name"
        );
        let expected_extension = match asset.media_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/tiff" => "tiff",
            "image/bmp" => "bmp",
            _ => bail!("invalid asset media type"),
        };
        ensure!(
            asset.storage_name == format!("{}.{}", asset.sha256, expected_extension),
            "asset storage name does not match its media type"
        );
        let path = root.join("assets").join(&asset.storage_name);
        ensure_regular_file(&path, MAX_ASSET_BYTES)?;
        ensure!(
            fs::metadata(path)?.len() == asset.bytes,
            "asset size mismatch"
        );
    }
    Ok(())
}

fn validate_import_metadata(metadata: &ImportMetadata) -> Result<()> {
    validate_tags(&metadata.tags)?;
    ensure!(
        metadata.provenance.len() <= MAX_TEXT_BYTES
            && !metadata.provenance.chars().any(char::is_control),
        "invalid asset provenance"
    );
    Ok(())
}

fn validate_tags(tags: &BTreeSet<String>) -> Result<()> {
    ensure!(tags.len() <= MAX_TAGS, "too many asset tags");
    for tag in tags {
        ensure!(
            !tag.trim().is_empty()
                && tag.len() <= MAX_TAG_BYTES
                && !tag.chars().any(char::is_control),
            "invalid asset tag"
        );
    }
    Ok(())
}

fn clean_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| !character.is_control())
        .take(255)
        .collect::<String>();
    if cleaned.trim().is_empty() {
        "Imported image".into()
    } else {
        cleaned
    }
}

fn ensure_real_directory(path: &Path, create: bool) -> Result<()> {
    if !path.exists() && create {
        fs::create_dir_all(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_dir(),
        "{} is not a real directory",
        path.display()
    );
    Ok(())
}

fn ensure_regular_file(path: &Path, maximum: u64) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Cannot inspect {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "{} is not a regular file",
        path.display()
    );
    ensure!(
        metadata.len() > 0 && metadata.len() <= maximum,
        "file exceeds supported bounds"
    );
    Ok(())
}

fn inspect_image(path: &Path) -> Result<(ImageFormat, u32, u32)> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format().context("Unknown image format")?;
    ensure!(
        extension(format).is_some(),
        "Unsupported library image format"
    );
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    ensure!(
        valid_dimensions(width, height),
        "image exceeds supported pixel bounds"
    );
    let decoded_bytes = decoder.total_bytes();
    ensure!(
        decoded_bytes <= MAX_ASSET_BYTES,
        "decoded image exceeds the library memory bound"
    );
    let decoded_bytes = usize::try_from(decoded_bytes).context("decoded image is too large")?;
    let mut decoded = vec![0u8; decoded_bytes];
    decoder
        .read_image(&mut decoded)
        .context("Image data is truncated or corrupt")?;
    Ok((format, width, height))
}

fn extension(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::WebP => Some("webp"),
        ImageFormat::Tiff => Some("tiff"),
        ImageFormat::Bmp => Some("bmp"),
        _ => None,
    }
}

fn media_type(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Tiff => "image/tiff",
        ImageFormat::Bmp => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn copy_and_hash(source: &Path, destination: &Path) -> Result<(String, u64)> {
    let mut input = BufReader::new(File::open(source)?);
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut output = BufWriter::new(output);
    let mut sha = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .context("asset byte count overflow")?;
        ensure!(
            bytes <= MAX_ASSET_BYTES,
            "asset exceeds supported byte size"
        );
        sha.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
    }
    output.flush()?;
    output.get_ref().sync_all()?;
    ensure!(bytes > 0, "empty assets are not supported");
    Ok((hex(&sha.finish()), bytes))
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut input = BufReader::new(File::open(path)?);
    let mut sha = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .context("asset byte count overflow")?;
        ensure!(
            bytes <= MAX_ASSET_BYTES,
            "asset exceeds supported byte size"
        );
        sha.update(&buffer[..read]);
    }
    Ok((hex(&sha.finish()), bytes))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    text
}

/// Hash a bounded byte buffer with the same content address used by the local
/// library. Consumers that package an already-selected asset can verify the
/// exact captured bytes without reopening an arbitrary source path.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut sha = Sha256::new();
    sha.update(bytes);
    hex(&sha.finish())
}

// Small dependency-free SHA-256 implementation for stable content addresses.
struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    length_bits: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            block_len: 0,
            length_bits: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.length_bits = self
            .length_bits
            .wrapping_add((input.len() as u64).wrapping_mul(8));
        if self.block_len > 0 {
            let take = (64 - self.block_len).min(input.len());
            self.block[self.block_len..self.block_len + take].copy_from_slice(&input[..take]);
            self.block_len += take;
            input = &input[take..];
            if self.block_len == 64 {
                let block = self.block;
                self.compress(&block);
                self.block_len = 0;
            }
        }
        while input.len() >= 64 {
            self.compress(input[..64].try_into().expect("64-byte SHA block"));
            input = &input[64..];
        }
        self.block[..input.len()].copy_from_slice(input);
        self.block_len = input.len();
    }

    fn finish(mut self) -> [u8; 32] {
        self.block[self.block_len] = 0x80;
        self.block_len += 1;
        if self.block_len > 56 {
            self.block[self.block_len..].fill(0);
            let block = self.block;
            self.compress(&block);
            self.block = [0; 64];
        } else {
            self.block[self.block_len..56].fill(0);
        }
        self.block[56..].copy_from_slice(&self.length_bits.to_be_bytes());
        let block = self.block;
        self.compress(&block);
        let mut output = [0u8; 32];
        for (chunk, value) in output.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&value.to_be_bytes());
        }
        output
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut schedule = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes(chunk.try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (state, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn sha256_matches_standard_vector() {
        let mut hash = Sha256::new();
        hash.update(b"abc");
        assert_eq!(
            hex(&hash.finish()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn import_search_reopen_and_integrity() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("Sunset Logo.png");
        RgbaImage::from_pixel(9, 7, Rgba([10, 20, 30, 255]))
            .save(&source)
            .unwrap();
        let mut library = AssetLibrary::open(directory.path().join("library")).unwrap();
        let asset = library
            .import(
                &source,
                ImportMetadata {
                    tags: BTreeSet::from(["brand".into(), "warm".into()]),
                    favorite: true,
                    provenance: "Sugata campaign".into(),
                },
            )
            .unwrap();
        assert_eq!((asset.width, asset.height), (9, 7));
        assert_eq!(library.search("warm sugata", 10).unwrap()[0].id, asset.id);
        library.verify_integrity().unwrap();
        let reopened = AssetLibrary::open(directory.path().join("library")).unwrap();
        assert_eq!(reopened.assets(), &[asset]);
    }

    #[test]
    fn tampering_is_detected_and_bad_tag_does_not_mutate_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("asset.png");
        RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let mut library = AssetLibrary::open(directory.path().join("library")).unwrap();
        let asset = library.import(&source, ImportMetadata::default()).unwrap();
        let before = fs::read(library.root().join("library.json")).unwrap();
        assert!(
            library
                .set_tags(&asset.id, BTreeSet::from(["bad\ntag".into()]))
                .is_err()
        );
        assert_eq!(
            fs::read(library.root().join("library.json")).unwrap(),
            before
        );
        fs::write(library.asset_path(&asset.id).unwrap(), b"different bytes").unwrap();
        assert!(library.verify_integrity().is_err());
    }

    #[test]
    fn stale_windows_merge_imports_tags_and_favourites_under_lock() {
        let directory = tempfile::tempdir().unwrap();
        let first_source = directory.path().join("first.png");
        let second_source = directory.path().join("second.png");
        RgbaImage::from_pixel(3, 2, Rgba([10, 0, 0, 255]))
            .save(&first_source)
            .unwrap();
        RgbaImage::from_pixel(4, 2, Rgba([0, 10, 0, 255]))
            .save(&second_source)
            .unwrap();
        let root = directory.path().join("library");
        let mut first_window = AssetLibrary::open(&root).unwrap();
        let mut second_window = AssetLibrary::open(&root).unwrap();

        let first = first_window
            .import(&first_source, ImportMetadata::default())
            .unwrap();
        let second = second_window
            .import(&second_source, ImportMetadata::default())
            .unwrap();
        first_window
            .set_tags(&first.id, BTreeSet::from(["updated".into()]))
            .unwrap();
        second_window.set_favorite(&second.id, true).unwrap();

        let final_index = AssetLibrary::open(&root).unwrap();
        assert_eq!(final_index.assets().len(), 2);
        assert!(final_index.get(&first.id).unwrap().tags.contains("updated"));
        assert!(final_index.get(&second.id).unwrap().favorite);
        final_index.verify_integrity().unwrap();
    }

    #[test]
    fn missing_indexed_content_is_rejected_on_open() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("asset.png");
        RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let root = directory.path().join("library");
        let mut library = AssetLibrary::open(&root).unwrap();
        let asset = library.import(&source, ImportMetadata::default()).unwrap();
        fs::remove_file(library.asset_path(&asset.id).unwrap()).unwrap();
        let error = AssetLibrary::open(&root).err().unwrap();
        assert!(error.to_string().contains("Cannot inspect"));
    }
}
