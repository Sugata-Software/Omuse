//! Versioned, bounded multi-page Create projects.
//!
//! A Create project is a directory package. Its `project.json` envelope owns
//! page order, brand kits, shared resources and reusable components; every page
//! and component is an ordinary `.omuse` package handled by the document
//! engine. Version 1 collections retain read support for nested `.comp`
//! packages and are upgraded on save. Opening a project records inactive
//! package paths and only decodes a page when a caller asks for its document.

use crate::{
    document,
    model::{Document, Layer, valid_dimensions},
    save_guard,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component as PathComponent, Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const PROJECT_FORMAT: &str = "com.omuse.create-project";
pub const PROJECT_VERSION: u32 = 2;
pub const MAX_PAGES: usize = 256;
pub const MAX_BRANDS: usize = 64;
pub const MAX_COMPONENTS: usize = 512;
pub const MAX_SHARED_RESOURCES: usize = 2_048;
pub const MAX_SHARED_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL_SHARED_RESOURCE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_TOTAL_PAGE_CANVAS_PIXELS: u64 = 512_000_000;
pub const MAX_RESIDENT_PROJECT_IMAGE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PROJECT_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
enum DocumentPackageLayout {
    LegacyComp,
    Omuse,
}

impl DocumentPackageLayout {
    fn from_version(version: u32) -> Result<Self> {
        match version {
            1 => Ok(Self::LegacyComp),
            PROJECT_VERSION => Ok(Self::Omuse),
            _ => anyhow::bail!("Unsupported Create project version {version}"),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::LegacyComp => "comp",
            Self::Omuse => "omuse",
        }
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

fn canonical_id(raw: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(raw)
        .context("Invalid project UUID")?
        .to_string()
        .to_uppercase())
}

fn valid_label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontPair {
    pub heading: String,
    pub body: String,
}

impl Default for FontPair {
    fn default() -> Self {
        Self {
            heading: "sans-serif".into(),
            body: "sans-serif".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandTextStyle {
    pub font: String,
    pub size: f32,
    #[serde(default)]
    pub tracking: f32,
    #[serde(default)]
    pub leading: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandKit {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub colors: BTreeMap<String, [u8; 4]>,
    #[serde(default)]
    pub fonts: FontPair,
    #[serde(default)]
    pub text_styles: BTreeMap<String, BrandTextStyle>,
    #[serde(default)]
    pub spacing: BTreeMap<String, f32>,
    #[serde(default)]
    pub logo_resource_ids: Vec<String>,
}

/// Collection-scoped state that is neutral for existing packages. Shared
/// artwork is represented by an ordinary reusable component so its source
/// pixels remain editable and survive save/reopen with the rest of the
/// project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_background_component_id: Option<String>,
    /// Disallow remote AI submission for this project while retaining all
    /// local editing, save, and export features. Absent legacy metadata is
    /// deliberately permissive for backward-compatible reopening.
    #[serde(default)]
    pub local_only: bool,
}

impl BrandKit {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            colors: BTreeMap::new(),
            fonts: FontPair::default(),
            text_styles: BTreeMap::new(),
            spacing: BTreeMap::new(),
            logo_resource_ids: vec![],
        }
    }

    pub fn validate(&self) -> Result<()> {
        canonical_id(&self.id)?;
        ensure!(valid_label(&self.name, 512), "Invalid brand name");
        ensure!(self.colors.len() <= 128, "Too many brand colors");
        ensure!(self.text_styles.len() <= 128, "Too many brand text styles");
        ensure!(self.spacing.len() <= 128, "Too many brand spacing tokens");
        for (role, style) in &self.text_styles {
            ensure!(valid_label(role, 128), "Invalid brand text role");
            ensure!(valid_label(&style.font, 512), "Invalid brand font name");
            ensure!(
                style.size.is_finite() && (1.0..=2_000.0).contains(&style.size),
                "Invalid brand font size"
            );
            ensure!(
                style.tracking.is_finite() && (-100.0..=1_000.0).contains(&style.tracking),
                "Invalid brand tracking"
            );
            ensure!(
                style.leading.is_finite() && (0.0..=5_000.0).contains(&style.leading),
                "Invalid brand leading"
            );
        }
        for (role, value) in &self.spacing {
            ensure!(valid_label(role, 128), "Invalid brand spacing role");
            ensure!(
                value.is_finite() && (0.0..=30_000.0).contains(value),
                "Invalid brand spacing value"
            );
        }
        canonical_id_list(&self.logo_resource_ids, "brand logo resource")?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSummary {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub template_id: Option<String>,
    pub is_loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentSummary {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub is_loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSummary {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub byte_len: u64,
    pub is_loaded: bool,
}

#[derive(Clone, Debug)]
enum DocumentStorage {
    Loaded(Document),
    /// The desktop's page Editor owns this document. A structural Project with
    /// checked-out pages is deliberately not a save/export snapshot: callers
    /// must restore every page from its Editor before reading its artwork.
    CheckedOut,
    Lazy {
        path: PathBuf,
        /// Structural edits materialize the previous document here before
        /// replacing/removing it. Project clones share the cache, so an undo
        /// snapshot never starts reading newer pixels from an exchanged save
        /// package that reused the same page ID.
        cache: Arc<Mutex<Option<Document>>>,
    },
}

/// The package directory and tree fingerprint observed when this project was
/// opened or atomically rebased. Lazy reads use it to refuse a replacement
/// package whose metadata no longer belongs to this in-memory session.
#[derive(Clone, Debug)]
struct PackageSource {
    root: PathBuf,
    stamp: u64,
}

#[derive(Clone, Debug)]
struct Page {
    id: String,
    name: String,
    width: u32,
    height: u32,
    template_id: Option<String>,
    storage: DocumentStorage,
}

impl Page {
    fn from_document(name: String, mut document: Document) -> Self {
        document.name = name.clone();
        Self {
            id: new_id(),
            name,
            width: document.width,
            height: document.height,
            template_id: None,
            storage: DocumentStorage::Loaded(document),
        }
    }

    fn load(&mut self, source: Option<&PackageSource>) -> Result<&Document> {
        if let DocumentStorage::Lazy { path, cache } = &self.storage {
            let mut document = {
                let mut guard = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Create page cache is unavailable"))?;
                if guard.is_none() {
                    *guard = Some(
                        read_lazy_document(path, source)
                            .with_context(|| format!("Cannot open Create page {}", self.name))?,
                    );
                }
                guard.as_ref().unwrap().clone()
            };
            ensure!(
                document.width == self.width && document.height == self.height,
                "Page dimensions differ from the Create manifest"
            );
            document.name = self.name.clone();
            self.storage = DocumentStorage::Loaded(document);
        }
        match &self.storage {
            DocumentStorage::Loaded(document) => Ok(document),
            DocumentStorage::CheckedOut => {
                anyhow::bail!("Create page is checked out to an editor; restore its document first")
            }
            DocumentStorage::Lazy { .. } => unreachable!(),
        }
    }

    fn load_mut(&mut self, source: Option<&PackageSource>) -> Result<&mut Document> {
        self.load(source)?;
        match &mut self.storage {
            DocumentStorage::Loaded(document) => Ok(document),
            DocumentStorage::Lazy { .. } | DocumentStorage::CheckedOut => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
struct ReusableComponent {
    id: String,
    name: String,
    revision: u64,
    storage: DocumentStorage,
}

#[derive(Clone, Debug)]
enum ResourceStorage {
    Loaded(Vec<u8>),
    Lazy {
        path: PathBuf,
        /// Project copies use this cache as a copy-on-write snapshot before an
        /// atomic package exchange removes the previous resource path.
        cache: Arc<Mutex<Option<Vec<u8>>>>,
    },
}

#[derive(Clone, Debug)]
struct SharedResource {
    id: String,
    name: String,
    media_type: String,
    byte_len: u64,
    storage: ResourceStorage,
}

#[derive(Clone, Debug)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub brand_kits: Vec<BrandKit>,
    pub active_brand_id: Option<String>,
    pub metadata: ProjectMetadata,
    pages: Vec<Page>,
    active_page_id: String,
    components: Vec<ReusableComponent>,
    resources: Vec<SharedResource>,
    source: Option<PackageSource>,
}

impl Project {
    pub fn new(title: impl Into<String>, document: Document) -> Self {
        let title = title.into();
        let page = Page::from_document("Page 1".into(), document);
        let active_page_id = page.id.clone();
        Self {
            id: new_id(),
            title,
            brand_kits: vec![],
            active_brand_id: None,
            metadata: ProjectMetadata::default(),
            pages: vec![page],
            active_page_id,
            components: vec![],
            resources: vec![],
            source: None,
        }
    }

    /// Open a Create package, or wrap any legacy document/image accepted by the
    /// existing document loader as a one-page in-memory Create project.
    pub fn open(path: &Path) -> Result<Self> {
        if path.is_dir() && path.join("project.json").exists() {
            return open_create_package(path);
        }
        let document = document::open(path)?;
        let title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled")
            .to_owned();
        Ok(Self::new(title, document))
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.save_checked(path, || Ok(()))
    }

    /// Save the package atomically, checking caller-owned save conditions at
    /// the last safe point before publication. The callback runs only after
    /// every staged file and directory has been synced, while the destination
    /// package is still untouched. An error therefore leaves the prior package
    /// intact and lets the staging-directory guard clean up the draft.
    pub fn save_checked<F>(&mut self, path: &Path, before_publish: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.validate()?;
        // A clone made for undo/recovery shares a lazy cache with this project.
        // Before exchanging a package directory, materialize only those shared
        // lazy values. The live project is immediately rebound to empty caches
        // below, so ordinary saves and read-only traversal remain lazy.
        self.freeze_shared_snapshots()?;
        write_create_package(self, path, before_publish)?;
        let root = absolute_destination(path)?;
        let stamp = save_guard::package_stamp(&root)
            .context("Saved Create project could not be fingerprinted")?;
        self.source = Some(PackageSource {
            root: root.clone(),
            stamp,
        });
        for page in &mut self.pages {
            if matches!(page.storage, DocumentStorage::Lazy { .. }) {
                page.storage = DocumentStorage::Lazy {
                    path: page_path(&root, &page.id, DocumentPackageLayout::Omuse),
                    cache: Arc::new(Mutex::new(None)),
                };
            }
        }
        for component in &mut self.components {
            if matches!(component.storage, DocumentStorage::Lazy { .. }) {
                component.storage = DocumentStorage::Lazy {
                    path: component_path(&root, &component.id, DocumentPackageLayout::Omuse),
                    cache: Arc::new(Mutex::new(None)),
                };
            }
        }
        for resource in &mut self.resources {
            if matches!(resource.storage, ResourceStorage::Lazy { .. }) {
                resource.storage = ResourceStorage::Lazy {
                    path: resource_path(&root, &resource.id),
                    cache: Arc::new(Mutex::new(None)),
                };
            }
        }
        Ok(())
    }

    /// Preserve lazy data held by another project clone before an atomic save
    /// swaps away its on-disk package. `Project::clone` deliberately shares
    /// these small synchronization cells, which makes the check exact: a cache
    /// with one strong reference belongs only to this live project.
    fn freeze_shared_snapshots(&self) -> Result<()> {
        for page in &self.pages {
            let DocumentStorage::Lazy { path, cache } = &page.storage else {
                continue;
            };
            if Arc::strong_count(cache) <= 1 {
                continue;
            }
            let mut guard = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Create page cache is unavailable"))?;
            if guard.is_none() {
                let mut document = read_lazy_document(path, self.source.as_ref())
                    .with_context(|| format!("Cannot snapshot Create page {}", page.name))?;
                ensure!(
                    document.width == page.width && document.height == page.height,
                    "Page dimensions differ from the Create manifest"
                );
                document.name = page.name.clone();
                *guard = Some(document);
            }
        }
        for component in &self.components {
            let DocumentStorage::Lazy { path, cache } = &component.storage else {
                continue;
            };
            if Arc::strong_count(cache) <= 1 {
                continue;
            }
            let mut guard = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Component cache is unavailable"))?;
            if guard.is_none() {
                *guard = Some(
                    read_lazy_document(path, self.source.as_ref())
                        .with_context(|| format!("Cannot snapshot component {}", component.name))?,
                );
            }
        }
        for resource in &self.resources {
            let ResourceStorage::Lazy { path, cache } = &resource.storage else {
                continue;
            };
            if Arc::strong_count(cache) <= 1 {
                continue;
            }
            let mut guard = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Shared resource cache is unavailable"))?;
            if guard.is_none() {
                *guard = Some(read_lazy_resource(
                    path,
                    resource.byte_len,
                    self.source.as_ref(),
                )?);
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        canonical_id(&self.id)?;
        ensure!(valid_label(&self.title, 2_048), "Invalid project title");
        ensure!(
            !self.pages.is_empty() && self.pages.len() <= MAX_PAGES,
            "A Create project needs 1-{MAX_PAGES} pages"
        );
        ensure!(self.brand_kits.len() <= MAX_BRANDS, "Too many brand kits");
        ensure!(
            self.components.len() <= MAX_COMPONENTS,
            "Too many components"
        );
        ensure!(
            self.resources.len() <= MAX_SHARED_RESOURCES,
            "Too many shared resources"
        );
        let mut ids = HashSet::new();
        let mut canvas_pixels = 0u64;
        let mut resident_allocations = HashSet::new();
        let mut resident_bytes = 0u64;
        for page in &self.pages {
            canonical_id(&page.id)?;
            ensure!(
                !matches!(page.storage, DocumentStorage::CheckedOut),
                "Create page is checked out to an editor; restore all documents before validation"
            );
            ensure!(ids.insert(page.id.as_str()), "Duplicate page ID");
            ensure!(valid_label(&page.name, 2_048), "Invalid page name");
            ensure!(
                valid_dimensions(page.width, page.height),
                "Invalid page dimensions"
            );
            canvas_pixels = canvas_pixels
                .checked_add(u64::from(page.width) * u64::from(page.height))
                .context("Project canvas pixel count overflow")?;
            if let Some(template) = &page.template_id {
                ensure!(valid_label(template, 256), "Invalid template ID");
            }
            if let DocumentStorage::Loaded(document) = &page.storage {
                ensure!(
                    document.width == page.width && document.height == page.height,
                    "Loaded page dimensions differ from its record"
                );
                count_resident_images(
                    &document.layers,
                    &mut resident_allocations,
                    &mut resident_bytes,
                )?;
            }
        }
        ensure!(
            canvas_pixels <= MAX_TOTAL_PAGE_CANVAS_PIXELS,
            "Project pages exceed the authored canvas pixel budget"
        );
        ensure!(
            self.pages.iter().any(|page| page.id == self.active_page_id),
            "Active page is missing"
        );
        let mut brand_ids = HashSet::new();
        for brand in &self.brand_kits {
            brand.validate()?;
            ensure!(brand_ids.insert(brand.id.as_str()), "Duplicate brand ID");
        }
        if let Some(active) = &self.active_brand_id {
            ensure!(
                brand_ids.contains(active.as_str()),
                "Active brand is missing"
            );
        }
        let resource_ids = self
            .resources
            .iter()
            .map(|resource| resource.id.as_str())
            .collect::<HashSet<_>>();
        let mut total = 0u64;
        for resource in &self.resources {
            canonical_id(&resource.id)?;
            ensure!(valid_label(&resource.name, 2_048), "Invalid resource name");
            ensure!(
                valid_media_type(&resource.media_type),
                "Invalid resource media type"
            );
            ensure!(
                resource.byte_len <= MAX_SHARED_RESOURCE_BYTES,
                "Shared resource is too large"
            );
            total = total
                .checked_add(resource.byte_len)
                .context("Shared resource size overflow")?;
            if let ResourceStorage::Loaded(bytes) = &resource.storage {
                ensure!(
                    bytes.len() as u64 == resource.byte_len,
                    "Shared resource length changed without updating its record"
                );
            }
        }
        ensure!(
            resource_ids.len() == self.resources.len(),
            "Duplicate shared resource ID"
        );
        ensure!(
            total <= MAX_TOTAL_SHARED_RESOURCE_BYTES,
            "Shared resources exceed the project budget"
        );
        let mut component_ids = HashSet::new();
        for component in &self.components {
            canonical_id(&component.id)?;
            ensure!(
                component_ids.insert(component.id.as_str()),
                "Duplicate component ID"
            );
            ensure!(
                valid_label(&component.name, 2_048),
                "Invalid component name"
            );
            ensure!(component.revision > 0, "Invalid component revision");
            if let DocumentStorage::Loaded(document) = &component.storage {
                count_resident_images(
                    &document.layers,
                    &mut resident_allocations,
                    &mut resident_bytes,
                )?;
            }
        }
        if let Some(component_id) = &self.metadata.shared_background_component_id {
            canonical_id(component_id)?;
            ensure!(
                component_ids.contains(component_id.as_str()),
                "Shared background references a missing component"
            );
        }
        ensure!(
            resident_bytes <= MAX_RESIDENT_PROJECT_IMAGE_BYTES,
            "Loaded project images exceed the resident memory budget"
        );
        for brand in &self.brand_kits {
            for id in &brand.logo_resource_ids {
                ensure!(
                    resource_ids.contains(id.as_str()),
                    "Brand references a missing logo resource"
                );
            }
        }
        Ok(())
    }

    pub fn active_page_id(&self) -> &str {
        &self.active_page_id
    }

    pub fn set_active_page(&mut self, id: &str) -> Result<()> {
        ensure!(
            self.pages.iter().any(|page| page.id == id),
            "Page not found"
        );
        self.active_page_id = id.to_owned();
        Ok(())
    }

    pub fn page_summaries(&self) -> Vec<PageSummary> {
        self.pages
            .iter()
            .map(|page| PageSummary {
                id: page.id.clone(),
                name: page.name.clone(),
                width: page.width,
                height: page.height,
                template_id: page.template_id.clone(),
                is_loaded: !matches!(page.storage, DocumentStorage::Lazy { .. }),
            })
            .collect()
    }

    pub fn page_ids(&self) -> Vec<String> {
        self.pages.iter().map(|page| page.id.clone()).collect()
    }

    pub fn active_document(&mut self) -> Result<&Document> {
        let id = self.active_page_id.clone();
        self.page_document(&id)
    }

    pub fn active_document_mut(&mut self) -> Result<&mut Document> {
        let id = self.active_page_id.clone();
        self.page_document_mut(&id)
    }

    pub fn page_document(&mut self, id: &str) -> Result<&Document> {
        let source = self.source.clone();
        self.pages
            .iter_mut()
            .find(|page| page.id == id)
            .context("Page not found")?
            .load(source.as_ref())
    }

    pub fn page_document_mut(&mut self, id: &str) -> Result<&mut Document> {
        let source = self.source.clone();
        self.pages
            .iter_mut()
            .find(|page| page.id == id)
            .context("Page not found")?
            .load_mut(source.as_ref())
    }

    /// Copy an editor document back into its active page. Existing pixel
    /// allocations remain shared until either copy is edited.
    pub fn replace_active_document(&mut self, document: Document) -> Result<()> {
        let id = self.active_page_id.clone();
        self.replace_page_document(&id, document)
    }

    pub fn replace_page_document(&mut self, id: &str, mut document: Document) -> Result<()> {
        ensure!(
            valid_dimensions(document.width, document.height),
            "Invalid page dimensions"
        );
        let source = self.source.clone();
        let page = self
            .pages
            .iter_mut()
            .find(|page| page.id == id)
            .context("Page not found")?;
        // Populate a shared lazy cache first so already-cloned project undo
        // snapshots retain the document that is about to be replaced.
        if !matches!(page.storage, DocumentStorage::CheckedOut) {
            page.load(source.as_ref())?;
        }
        document.name = page.name.clone();
        page.width = document.width;
        page.height = document.height;
        page.storage = DocumentStorage::Loaded(document);
        Ok(())
    }

    /// Release redundant page documents while a desktop session's Editors own
    /// their current artwork. Metadata stays available for the page strip.
    /// Preflight the whole batch before releasing anything; unknown/duplicate
    /// IDs or invalid dimensions therefore leave the Project untouched.
    ///
    /// This is an in-memory ownership state only. Clone this Project and use
    /// `replace_page_document` for every checked-out page to build an immutable
    /// save/recovery/export snapshot. Reading or saving an incomplete snapshot
    /// returns an error rather than stale artwork.
    pub fn checkout_page_documents(&mut self, documents: &[(&str, &Document)]) -> Result<()> {
        let mut ids = HashSet::new();
        for (id, document) in documents {
            ensure!(ids.insert(*id), "Duplicate checked-out page ID");
            ensure!(
                self.pages.iter().any(|page| page.id == *id),
                "Page not found"
            );
            ensure!(
                valid_dimensions(document.width, document.height),
                "Invalid page dimensions"
            );
        }
        // Freeze shared lazy caches before removing their path from this live
        // Project. Otherwise a later same-path save could invalidate an older
        // clone. Use temporary Pages so a failed load leaves live storage and
        // metadata untouched; successfully cached old pixels remain valid.
        for (id, _) in documents {
            let page = self.pages.iter().find(|page| page.id == *id).unwrap();
            if let DocumentStorage::Lazy { cache, .. } = &page.storage {
                if Arc::strong_count(cache) > 1 {
                    page.clone().load(self.source.as_ref())?;
                }
            }
        }
        for (id, document) in documents {
            let page = self.pages.iter_mut().find(|page| page.id == *id).unwrap();
            page.width = document.width;
            page.height = document.height;
            page.storage = DocumentStorage::CheckedOut;
        }
        Ok(())
    }

    pub fn set_page_name(&mut self, id: &str, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        ensure!(valid_label(&name, 2_048), "Invalid page name");
        let page = self
            .pages
            .iter_mut()
            .find(|page| page.id == id)
            .context("Page not found")?;
        page.name = name.clone();
        if let DocumentStorage::Loaded(document) = &mut page.storage {
            document.name = name;
        }
        Ok(())
    }

    pub fn set_page_template(&mut self, id: &str, template_id: Option<&str>) -> Result<()> {
        if let Some(value) = template_id {
            ensure!(valid_label(value, 256), "Invalid template ID");
        }
        let page = self
            .pages
            .iter_mut()
            .find(|page| page.id == id)
            .context("Page not found")?;
        page.template_id = template_id.map(str::to_owned);
        Ok(())
    }

    pub fn add_page(&mut self, name: impl Into<String>, document: Document) -> Result<String> {
        let name = name.into();
        self.validate_page_admission(&name, document.width, document.height)?;
        let page = Page::from_document(name, document);
        let id = page.id.clone();
        self.pages.push(page);
        Ok(id)
    }

    /// Apply the same limits to imports, new canvases and duplicates before
    /// allocating pixels, loading a lazy source or changing the page list.
    /// An admitted page must not make the collection impossible to save/undo.
    fn validate_page_admission(&self, name: &str, width: u32, height: u32) -> Result<()> {
        ensure!(
            self.pages.len() < MAX_PAGES,
            "Project already has {MAX_PAGES} pages"
        );
        ensure!(valid_label(name, 2_048), "Invalid page name");
        ensure!(valid_dimensions(width, height), "Invalid page dimensions");
        let existing_pixels = self.pages.iter().try_fold(0u64, |total, page| {
            total
                .checked_add(u64::from(page.width) * u64::from(page.height))
                .context("Project canvas pixel count overflow")
        })?;
        ensure!(
            existing_pixels.saturating_add(u64::from(width) * u64::from(height))
                <= MAX_TOTAL_PAGE_CANVAS_PIXELS,
            "Project pages exceed the authored canvas pixel budget"
        );
        Ok(())
    }

    pub fn add_blank_page(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
    ) -> Result<String> {
        let name = name.into();
        // Reject a full collection or invalid name before allocating the new
        // canvas; add_page repeats the cheap check for imported documents.
        self.validate_page_admission(&name, width, height)?;
        self.add_page(name, Document::new(width, height))
    }

    pub fn duplicate_page(&mut self, id: &str) -> Result<String> {
        let index = self
            .pages
            .iter()
            .position(|page| page.id == id)
            .context("Page not found")?;
        let name = format!("{} copy", self.pages[index].name);
        self.validate_page_admission(&name, self.pages[index].width, self.pages[index].height)?;
        let source = self.source.clone();
        let mut document = self.pages[index].load(source.as_ref())?.clone();
        document.name = name.clone();
        document.metadata["documentID"] = serde_json::json!(new_id());
        let mut page = Page::from_document(name, document);
        page.template_id = self.pages[index].template_id.clone();
        let new_id = page.id.clone();
        self.pages.insert(index + 1, page);
        Ok(new_id)
    }

    pub fn remove_page(&mut self, id: &str) -> Result<()> {
        ensure!(
            self.pages.len() > 1,
            "A project must retain at least one page"
        );
        let index = self
            .pages
            .iter()
            .position(|page| page.id == id)
            .context("Page not found")?;
        // See `replace_page_document`: snapshots made before this mutation may
        // outlive the next atomic save, where this page path disappears.
        let source = self.source.clone();
        self.pages[index].load(source.as_ref())?;
        self.pages.remove(index);
        if self.active_page_id == id {
            self.active_page_id = self.pages[index.min(self.pages.len() - 1)].id.clone();
        }
        Ok(())
    }

    pub fn reorder_page(&mut self, id: &str, new_index: usize) -> Result<()> {
        ensure!(
            new_index < self.pages.len(),
            "Page destination is out of range"
        );
        let old_index = self
            .pages
            .iter()
            .position(|page| page.id == id)
            .context("Page not found")?;
        if old_index != new_index {
            let page = self.pages.remove(old_index);
            self.pages.insert(new_index, page);
        }
        Ok(())
    }

    /// Visit pages sequentially. A lazy page is decoded for the callback and
    /// released afterwards, so an export or preview pass cannot retain every
    /// page in a carousel.
    pub fn for_each_page_document(
        &mut self,
        mut visit: impl FnMut(&PageSummary, &Document) -> Result<()>,
    ) -> Result<()> {
        let source = self.source.clone();
        // This traversal never stores decoded lazy pages. One source session
        // therefore protects the whole read-only pass without a full package
        // fingerprint for every page.
        let source_session = PackageReadSession::new(source.as_ref());
        for index in 0..self.pages.len() {
            let summary = {
                let page = &self.pages[index];
                PageSummary {
                    id: page.id.clone(),
                    name: page.name.clone(),
                    width: page.width,
                    height: page.height,
                    template_id: page.template_id.clone(),
                    is_loaded: true,
                }
            };
            match &self.pages[index].storage {
                DocumentStorage::Loaded(document) => visit(&summary, document)?,
                DocumentStorage::CheckedOut => {
                    anyhow::bail!(
                        "Create page is checked out to an editor; restore its document first"
                    )
                }
                DocumentStorage::Lazy { path, cache } => {
                    let cached = cache
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Create page cache is unavailable"))?
                        .clone();
                    if let Some(mut document) = cached {
                        document.name = summary.name.clone();
                        visit(&summary, &document)?;
                    } else {
                        let mut document = source_session
                            .read(|| document::open(path))
                            .with_context(|| format!("Cannot open Create page {}", summary.name))?;
                        ensure!(
                            document.width == summary.width && document.height == summary.height,
                            "Page dimensions differ from the Create manifest"
                        );
                        document.name = summary.name.clone();
                        visit(&summary, &document)?;
                    }
                }
            }
        }
        source_session.finish()?;
        Ok(())
    }

    pub fn add_brand(&mut self, brand: BrandKit) -> Result<String> {
        ensure!(self.brand_kits.len() < MAX_BRANDS, "Too many brand kits");
        brand.validate()?;
        ensure!(
            !self.brand_kits.iter().any(|item| item.id == brand.id),
            "Duplicate brand ID"
        );
        let id = brand.id.clone();
        self.brand_kits.push(brand);
        if self.active_brand_id.is_none() {
            self.active_brand_id = Some(id.clone());
        }
        Ok(id)
    }

    pub fn active_brand(&self) -> Option<&BrandKit> {
        let id = self.active_brand_id.as_deref()?;
        self.brand_kits.iter().find(|brand| brand.id == id)
    }

    pub fn add_resource(
        &mut self,
        name: impl Into<String>,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<String> {
        ensure!(
            self.resources.len() < MAX_SHARED_RESOURCES,
            "Too many shared resources"
        );
        ensure!(
            bytes.len() as u64 <= MAX_SHARED_RESOURCE_BYTES,
            "Shared resource is too large"
        );
        let total = self
            .resources
            .iter()
            .map(|resource| resource.byte_len)
            .sum::<u64>();
        ensure!(
            total.saturating_add(bytes.len() as u64) <= MAX_TOTAL_SHARED_RESOURCE_BYTES,
            "Shared resources exceed the project budget"
        );
        let resource = SharedResource {
            id: new_id(),
            name: name.into(),
            media_type: media_type.into(),
            byte_len: bytes.len() as u64,
            storage: ResourceStorage::Loaded(bytes),
        };
        ensure!(valid_label(&resource.name, 2_048), "Invalid resource name");
        ensure!(
            valid_media_type(&resource.media_type),
            "Invalid resource media type"
        );
        let id = resource.id.clone();
        self.resources.push(resource);
        Ok(id)
    }

    pub fn resource_summaries(&self) -> Vec<ResourceSummary> {
        self.resources
            .iter()
            .map(|resource| ResourceSummary {
                id: resource.id.clone(),
                name: resource.name.clone(),
                media_type: resource.media_type.clone(),
                byte_len: resource.byte_len,
                is_loaded: matches!(resource.storage, ResourceStorage::Loaded(_)),
            })
            .collect()
    }

    pub fn resource_bytes(&mut self, id: &str) -> Result<&[u8]> {
        let source = self.source.clone();
        let resource = self
            .resources
            .iter_mut()
            .find(|resource| resource.id == id)
            .context("Shared resource not found")?;
        if let ResourceStorage::Lazy { path, cache } = &resource.storage {
            let bytes = {
                let mut guard = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Shared resource cache is unavailable"))?;
                if guard.is_none() {
                    *guard = Some(read_lazy_resource(
                        path,
                        resource.byte_len,
                        source.as_ref(),
                    )?);
                }
                guard.as_ref().unwrap().clone()
            };
            resource.storage = ResourceStorage::Loaded(bytes);
        }
        match &resource.storage {
            ResourceStorage::Loaded(bytes) => Ok(bytes),
            ResourceStorage::Lazy { .. } => unreachable!(),
        }
    }

    pub fn define_component(
        &mut self,
        name: impl Into<String>,
        layers: Vec<Layer>,
    ) -> Result<String> {
        ensure!(
            self.components.len() < MAX_COMPONENTS,
            "Too many reusable components"
        );
        let name = name.into();
        ensure!(valid_label(&name, 2_048), "Invalid component name");
        ensure!(!layers.is_empty(), "A component needs at least one layer");
        let mut document = Document::new(1, 1);
        document.layers = layers;
        document.name = name.clone();
        let component = ReusableComponent {
            id: new_id(),
            name,
            revision: 1,
            storage: DocumentStorage::Loaded(document),
        };
        let id = component.id.clone();
        self.components.push(component);
        Ok(id)
    }

    pub fn update_component(&mut self, id: &str, layers: Vec<Layer>) -> Result<u64> {
        ensure!(!layers.is_empty(), "A component needs at least one layer");
        let source = self.source.clone();
        let component = self
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .context("Component not found")?;
        if let DocumentStorage::Lazy { path, cache } = &component.storage {
            let mut guard = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Component cache is unavailable"))?;
            if guard.is_none() {
                *guard = Some(read_lazy_document(path, source.as_ref())?);
            }
        }
        let mut document = Document::new(1, 1);
        document.layers = layers;
        document.name = component.name.clone();
        component.revision = component
            .revision
            .checked_add(1)
            .context("Component revision overflow")?;
        component.storage = DocumentStorage::Loaded(document);
        Ok(component.revision)
    }

    pub fn component_summaries(&self) -> Vec<ComponentSummary> {
        self.components
            .iter()
            .map(|component| ComponentSummary {
                id: component.id.clone(),
                name: component.name.clone(),
                revision: component.revision,
                is_loaded: matches!(component.storage, DocumentStorage::Loaded(_)),
            })
            .collect()
    }

    pub fn component_snapshot(&mut self, id: &str) -> Result<(u64, Vec<Layer>)> {
        let source = self.source.clone();
        let component = self
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .context("Component not found")?;
        if let DocumentStorage::Lazy { path, cache } = &component.storage {
            let document = {
                let mut guard = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Component cache is unavailable"))?;
                if guard.is_none() {
                    *guard =
                        Some(read_lazy_document(path, source.as_ref()).with_context(|| {
                            format!("Cannot open component {}", component.name)
                        })?);
                }
                guard.as_ref().unwrap().clone()
            };
            component.storage = DocumentStorage::Loaded(document);
        }
        match &component.storage {
            DocumentStorage::Loaded(document) => Ok((component.revision, document.layers.clone())),
            DocumentStorage::CheckedOut => anyhow::bail!("Component document is unavailable"),
            DocumentStorage::Lazy { .. } => unreachable!(),
        }
    }
}

fn count_resident_images(
    layers: &[Layer],
    allocations: &mut HashSet<usize>,
    bytes: &mut u64,
) -> Result<()> {
    for layer in layers {
        for image in [&layer.image, &layer.mask].into_iter().flatten() {
            if allocations.insert(image.allocation_id()) {
                *bytes = bytes
                    .checked_add(
                        u64::from(image.width())
                            .checked_mul(u64::from(image.height()))
                            .and_then(|pixels| pixels.checked_mul(4))
                            .context("Resident image byte count overflow")?,
                    )
                    .context("Resident image byte count overflow")?;
            }
        }
        count_resident_images(&layer.children, allocations, bytes)?;
    }
    Ok(())
}

fn valid_media_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'+' | b'-' | b'.'))
}

fn canonical_id_list(values: &[String], label: &str) -> Result<()> {
    let mut ids = HashSet::new();
    for value in values {
        canonical_id(value).with_context(|| format!("Invalid {label} ID"))?;
        ensure!(ids.insert(value), "Duplicate {label} ID");
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u32,
    project_id: String,
    title: String,
    active_page_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_brand_id: Option<String>,
    #[serde(default)]
    metadata: ProjectMetadata,
    pages: Vec<PageRecord>,
    #[serde(default)]
    brands: Vec<BrandKit>,
    #[serde(default)]
    components: Vec<ComponentRecord>,
    #[serde(default)]
    resources: Vec<ResourceRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageRecord {
    id: String,
    name: String,
    width: u32,
    height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentRecord {
    id: String,
    name: String,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceRecord {
    id: String,
    name: String,
    media_type: String,
    byte_len: u64,
}

fn open_create_package(path: &Path) -> Result<Project> {
    let (mut project, stamp) =
        save_guard::read_consistent(path, || open_create_package_unchecked(path))?;
    let stamp = stamp.context("Create package source identity is unavailable")?;
    project.source = Some(PackageSource {
        root: path.to_owned(),
        stamp,
    });
    Ok(project)
}

fn open_create_package_unchecked(path: &Path) -> Result<Project> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Create package must not be a symbolic link"
    );
    let manifest_path = path.join("project.json");
    regular_file(&manifest_path, MAX_PROJECT_MANIFEST_BYTES)?;
    let mut bytes = vec![];
    File::open(&manifest_path)?
        .take(MAX_PROJECT_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_PROJECT_MANIFEST_BYTES,
        "Create manifest is too large"
    );
    let manifest: Manifest =
        serde_json::from_slice(&bytes).context("Invalid Create project manifest")?;
    ensure!(
        manifest.format == PROJECT_FORMAT,
        "Not an Omuse Create project"
    );
    // The declared version owns the nested layout. A missing or malformed
    // current package must never fall back to a stale legacy sibling.
    let layout = DocumentPackageLayout::from_version(manifest.version)?;
    let project_id = canonical_id(&manifest.project_id)?;
    let active_page_id = canonical_id(&manifest.active_page_id)?;
    let active_brand_id = manifest
        .active_brand_id
        .as_deref()
        .map(canonical_id)
        .transpose()?;
    ensure!(
        !manifest.pages.is_empty() && manifest.pages.len() <= MAX_PAGES,
        "Invalid Create page count"
    );
    ensure!(manifest.brands.len() <= MAX_BRANDS, "Too many brand kits");
    ensure!(
        manifest.components.len() <= MAX_COMPONENTS,
        "Too many components"
    );
    ensure!(
        manifest.resources.len() <= MAX_SHARED_RESOURCES,
        "Too many shared resources"
    );
    let mut pages = Vec::with_capacity(manifest.pages.len());
    for record in manifest.pages {
        let id = canonical_id(&record.id)?;
        ensure!(valid_label(&record.name, 2_048), "Invalid page name");
        ensure!(
            valid_dimensions(record.width, record.height),
            "Invalid page dimensions"
        );
        let source = page_path(path, &id, layout);
        ensure!(
            source.is_dir() && !fs::symlink_metadata(&source)?.file_type().is_symlink(),
            "Missing or unsafe page package"
        );
        pages.push(Page {
            id,
            name: record.name,
            width: record.width,
            height: record.height,
            template_id: record.template_id,
            storage: DocumentStorage::Lazy {
                path: source,
                cache: Arc::new(Mutex::new(None)),
            },
        });
    }
    let mut components = Vec::with_capacity(manifest.components.len());
    for record in manifest.components {
        let id = canonical_id(&record.id)?;
        ensure!(record.revision > 0, "Invalid component revision");
        let source = component_path(path, &id, layout);
        ensure!(
            source.is_dir() && !fs::symlink_metadata(&source)?.file_type().is_symlink(),
            "Missing or unsafe component package"
        );
        components.push(ReusableComponent {
            id,
            name: record.name,
            revision: record.revision,
            storage: DocumentStorage::Lazy {
                path: source,
                cache: Arc::new(Mutex::new(None)),
            },
        });
    }
    let mut resources = Vec::with_capacity(manifest.resources.len());
    let mut total_resource_bytes = 0u64;
    for record in manifest.resources {
        let id = canonical_id(&record.id)?;
        ensure!(
            record.byte_len <= MAX_SHARED_RESOURCE_BYTES,
            "Shared resource is too large"
        );
        total_resource_bytes = total_resource_bytes
            .checked_add(record.byte_len)
            .context("Shared resource size overflow")?;
        let source = resource_path(path, &id);
        regular_file(&source, MAX_SHARED_RESOURCE_BYTES)?;
        ensure!(
            fs::metadata(&source)?.len() == record.byte_len,
            "Shared resource length differs from its record"
        );
        resources.push(SharedResource {
            id,
            name: record.name,
            media_type: record.media_type,
            byte_len: record.byte_len,
            storage: ResourceStorage::Lazy {
                path: source,
                cache: Arc::new(Mutex::new(None)),
            },
        });
    }
    ensure!(
        total_resource_bytes <= MAX_TOTAL_SHARED_RESOURCE_BYTES,
        "Shared resources exceed the project budget"
    );
    let project = Project {
        id: project_id,
        title: manifest.title,
        brand_kits: manifest.brands,
        active_brand_id,
        metadata: manifest.metadata,
        pages,
        active_page_id,
        components,
        resources,
        source: None,
    };
    project.validate()?;
    Ok(project)
}

fn page_path(root: &Path, id: &str, layout: DocumentPackageLayout) -> PathBuf {
    root.join("pages")
        .join(format!("{id}.{}", layout.extension()))
}
fn component_path(root: &Path, id: &str, layout: DocumentPackageLayout) -> PathBuf {
    root.join("components")
        .join(format!("{id}.{}", layout.extension()))
}
fn resource_path(root: &Path, id: &str) -> PathBuf {
    root.join("resources").join(format!("{id}.bin"))
}

fn regular_file(path: &Path, limit: u64) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Cannot inspect {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= limit,
        "Not a regular file or exceeds its size limit: {}",
        path.display()
    );
    Ok(())
}

fn read_from_package_source<T>(
    source: Option<&PackageSource>,
    read: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let Some(source) = source else {
        return read();
    };
    ensure!(
        save_guard::package_stamp(&source.root) == Some(source.stamp),
        "The Create project changed on disk. Reopen it before loading more content."
    );
    let (value, observed) = save_guard::read_consistent(&source.root, read)?;
    ensure!(
        observed == Some(source.stamp),
        "The Create project changed on disk. Reopen it before loading more content."
    );
    Ok(value)
}

fn read_lazy_document(path: &Path, source: Option<&PackageSource>) -> Result<Document> {
    read_from_package_source(source, || document::open(path))
}

fn read_lazy_resource(
    path: &Path,
    expected_bytes: u64,
    source: Option<&PackageSource>,
) -> Result<Vec<u8>> {
    read_from_package_source(source, || {
        read_lazy_resource_unchecked(path, expected_bytes)
    })
}

fn read_lazy_resource_unchecked(path: &Path, expected_bytes: u64) -> Result<Vec<u8>> {
    regular_file(path, MAX_SHARED_RESOURCE_BYTES)?;
    let mut bytes = Vec::with_capacity(expected_bytes as usize);
    File::open(path)?
        .take(MAX_SHARED_RESOURCE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == expected_bytes,
        "Shared resource length differs from its record"
    );
    Ok(bytes)
}

/// A bounded source check around one staged package write. The project can
/// contain hundreds of lazy pages and resources; checking the full directory
/// fingerprint around every individual copy would turn a normal save into an
/// O(n²) walk. This session checks the original source before any lazy read
/// and once more immediately before publication. A changed source aborts the
/// staging directory, so no partially mixed source can be cached or published.
struct PackageReadSession<'a> {
    source: Option<&'a PackageSource>,
    used_source: std::cell::Cell<bool>,
}

impl<'a> PackageReadSession<'a> {
    fn new(source: Option<&'a PackageSource>) -> Self {
        Self {
            source,
            used_source: std::cell::Cell::new(false),
        }
    }

    fn read<T>(&self, read: impl FnOnce() -> Result<T>) -> Result<T> {
        // A frozen snapshot may outlive a successful atomic exchange of its
        // original package. Cached values have no remaining disk dependency;
        // only actual uncached reads need the original source identity.
        if !self.used_source.get() {
            self.verify_source()?;
            self.used_source.set(true);
        }
        read()
    }

    fn finish(&self) -> Result<()> {
        if self.used_source.get() {
            self.verify_source()?;
        }
        Ok(())
    }

    fn verify_source(&self) -> Result<()> {
        if let Some(source) = self.source {
            ensure!(
                save_guard::package_stamp(&source.root) == Some(source.stamp),
                "The Create project changed on disk. Reopen it before saving."
            );
        }
        Ok(())
    }
}

fn copy_lazy_resource(
    path: &Path,
    expected_bytes: u64,
    session: &PackageReadSession<'_>,
    output: &mut File,
) -> Result<()> {
    session.read(|| {
        regular_file(path, MAX_SHARED_RESOURCE_BYTES)?;
        let input = File::open(path)?;
        let copied = std::io::copy(&mut input.take(MAX_SHARED_RESOURCE_BYTES + 1), output)?;
        ensure!(
            copied == expected_bytes,
            "Shared resource length changed during save"
        );
        Ok(())
    })
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn absolute_destination(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.components()
            .all(|part| !matches!(part, PathComponent::ParentDir)),
        "Save path must not contain parent traversal"
    );
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(parent.is_dir(), "Save directory does not exist");
    Ok(parent
        .canonicalize()?
        .join(path.file_name().context("Missing Create package name")?))
}

fn write_create_package<F>(project: &Project, path: &Path, before_publish: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let destination = absolute_destination(path)?;
    if destination.exists() {
        let metadata = fs::symlink_metadata(&destination)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Refusing to replace a non-directory or symbolic link"
        );
        let current = destination.join("project.json");
        regular_file(&current, MAX_PROJECT_MANIFEST_BYTES)?;
        let value: serde_json::Value = serde_json::from_reader(File::open(current)?)?;
        ensure!(
            value.get("format").and_then(serde_json::Value::as_str) == Some(PROJECT_FORMAT),
            "Refusing to replace a directory that is not an Omuse Create project"
        );
    }
    let parent = destination
        .parent()
        .context("Missing Create package parent")?;
    let stage = Staging(parent.join(format!(".omuse-create-stage-{}", uuid::Uuid::new_v4())));
    fs::create_dir(&stage.0)?;
    for directory in ["pages", "components", "resources"] {
        fs::create_dir(stage.0.join(directory))?;
    }

    let source_session = PackageReadSession::new(project.source.as_ref());
    for page in &project.pages {
        save_stored_document(
            &page.storage,
            &page_path(&stage.0, &page.id, DocumentPackageLayout::Omuse),
            &source_session,
        )
        .with_context(|| format!("Cannot save page {}", page.name))?;
    }
    for component in &project.components {
        save_stored_document(
            &component.storage,
            &component_path(&stage.0, &component.id, DocumentPackageLayout::Omuse),
            &source_session,
        )
        .with_context(|| format!("Cannot save component {}", component.name))?;
    }
    for resource in &project.resources {
        let target = resource_path(&stage.0, &resource.id);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        match &resource.storage {
            ResourceStorage::Loaded(bytes) => output.write_all(bytes)?,
            ResourceStorage::Lazy {
                path: source,
                cache,
            } => {
                let cached = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Shared resource cache is unavailable"))?
                    .clone();
                if let Some(bytes) = cached {
                    ensure!(
                        bytes.len() as u64 == resource.byte_len,
                        "Shared resource length changed during save"
                    );
                    output.write_all(&bytes)?;
                } else {
                    copy_lazy_resource(source, resource.byte_len, &source_session, &mut output)?;
                }
            }
        }
        output.sync_all()?;
    }

    let manifest = Manifest {
        format: PROJECT_FORMAT.into(),
        version: PROJECT_VERSION,
        project_id: project.id.clone(),
        title: project.title.clone(),
        active_page_id: project.active_page_id.clone(),
        active_brand_id: project.active_brand_id.clone(),
        metadata: project.metadata.clone(),
        pages: project
            .pages
            .iter()
            .map(|page| PageRecord {
                id: page.id.clone(),
                name: page.name.clone(),
                width: page.width,
                height: page.height,
                template_id: page.template_id.clone(),
            })
            .collect(),
        brands: project.brand_kits.clone(),
        components: project
            .components
            .iter()
            .map(|component| ComponentRecord {
                id: component.id.clone(),
                name: component.name.clone(),
                revision: component.revision,
            })
            .collect(),
        resources: project
            .resources
            .iter()
            .map(|resource| ResourceRecord {
                id: resource.id.clone(),
                name: resource.name.clone(),
                media_type: resource.media_type.clone(),
                byte_len: resource.byte_len,
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    ensure!(
        bytes.len() as u64 <= MAX_PROJECT_MANIFEST_BYTES,
        "Create manifest is too large"
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.0.join("project.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    // Windows cannot move a directory while a file inside it is open.
    drop(file);
    for directory in ["pages", "components", "resources"] {
        crate::durable_fs::sync_path(stage.0.join(directory))?;
    }
    crate::durable_fs::sync_path(&stage.0)?;
    before_publish()?;
    source_session.finish()?;
    if destination.exists() {
        exchange(&stage.0, &destination)?;
    } else {
        rename_new(&stage.0, &destination)?;
    }
    let _ = crate::durable_fs::sync_path(parent);
    Ok(())
}

fn save_stored_document(
    storage: &DocumentStorage,
    target: &Path,
    source_session: &PackageReadSession<'_>,
) -> Result<()> {
    match storage {
        DocumentStorage::Loaded(document) => document::save(document, target),
        DocumentStorage::CheckedOut => anyhow::bail!("Cannot save a checked-out Create page"),
        DocumentStorage::Lazy { path, cache } => {
            // Opening and immediately saving validates every nested path and
            // asset while bounding peak memory to one inactive document.
            let cached = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Create document cache is unavailable"))?
                .clone();
            let document = cached
                .map(Ok)
                .unwrap_or_else(|| source_session.read(|| document::open(path)))?;
            document::save(&document, target)
        }
    }
}

#[cfg(target_os = "linux")]
fn rename_flags(from: &Path, to: &Path, flags: u32) -> Result<()> {
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
    // SAFETY: owned NUL-terminated paths live for the duration of the syscall.
    if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), flags) } != 0 {
        return Err(anyhow::Error::from(std::io::Error::last_os_error()))
            .context("Atomic Create project replacement failed; original retained");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn exchange(from: &Path, to: &Path) -> Result<()> {
    rename_flags(from, to, 2)
}
#[cfg(target_os = "linux")]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
    rename_flags(from, to, 1)
}
#[cfg(not(target_os = "linux"))]
fn exchange(from: &Path, to: &Path) -> Result<()> {
    crate::durable_fs::exchange_dirs(from, to)
        .context("Create project replacement failed; original retained")
}
#[cfg(not(target_os = "linux"))]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
    crate::durable_fs::rename_no_replace(from, to).context("Publishing the saved Create project")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use tempfile::tempdir;

    fn document(color: [u8; 4]) -> Document {
        let mut document = Document::new(8, 6);
        document.layers[0].image = Some(RgbaImage::from_pixel(8, 6, Rgba(color)).into());
        document
    }

    fn write_legacy_collection(path: &Path) -> (String, String, String, String) {
        let mut first_document = document([10, 20, 30, 255]);
        first_document.metadata["creation-note"] = serde_json::json!({"preserve": [1, 2, 3]});
        first_document.layers[0].name = "Editable artwork".into();
        first_document.layers[0].opacity = 0.625;
        first_document.layers[0].metadata["layer-note"] = serde_json::json!({"preserve": true});
        let prototype = first_document.layers[0].clone();
        let mut project = Project::new("Legacy campaign", first_document);
        let first = project.active_page_id().to_owned();
        project.set_page_name(&first, "Cover").unwrap();
        project
            .set_page_template(&first, Some("social-square"))
            .unwrap();
        let second = project
            .add_page("Story", document([40, 50, 60, 255]))
            .unwrap();
        project.set_active_page(&second).unwrap();
        let component = project
            .define_component("Shared artwork", vec![prototype.clone()])
            .unwrap();
        project
            .update_component(&component, vec![prototype])
            .unwrap();
        let resource = project
            .add_resource("Logo", "image/png", vec![1, 3, 5, 7])
            .unwrap();
        let mut brand = BrandKit::new("Campaign brand");
        brand.colors.insert("Accent".into(), [50, 100, 150, 255]);
        brand.spacing.insert("Margin".into(), 24.0);
        brand.logo_resource_ids.push(resource.clone());
        project.add_brand(brand).unwrap();
        project.metadata.local_only = true;
        project.metadata.shared_background_component_id = Some(component.clone());
        project.save(path).unwrap();

        // Version 1 has the same envelope fields but explicitly uses .comp
        // nested documents. Construct that on-disk format independently of
        // the reader's layout selector, then open it as a fresh session.
        for directory in ["pages", "components"] {
            for entry in fs::read_dir(path.join(directory)).unwrap() {
                let source = entry.unwrap().path();
                fs::rename(&source, source.with_extension("comp")).unwrap();
            }
        }
        let manifest_path = path.join("project.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["version"] = serde_json::json!(1);
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        (first, second, component, resource)
    }

    fn assert_all_lazy(project: &Project) {
        assert!(project.page_summaries().iter().all(|page| !page.is_loaded));
        assert!(
            project
                .component_summaries()
                .iter()
                .all(|component| !component.is_loaded)
        );
        assert!(
            project
                .resource_summaries()
                .iter()
                .all(|resource| !resource.is_loaded)
        );
    }

    fn layer_pixel(layer: &Layer) -> [u8; 4] {
        layer.image.as_ref().unwrap().get_pixel(0, 0).0
    }

    #[test]
    fn legacy_collection_save_as_migrates_documents_and_preserves_metadata() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("Legacy.omuse");
        let target = directory.path().join("Current.omuse");
        let (first, second, component, resource) = write_legacy_collection(&source);
        let original_manifest = fs::read(source.join("project.json")).unwrap();
        let mut expected_manifest: serde_json::Value =
            serde_json::from_slice(&original_manifest).unwrap();
        expected_manifest["version"] = serde_json::json!(2);

        let mut migrated = Project::open(&source).unwrap();
        assert_all_lazy(&migrated);
        migrated.save(&target).unwrap();
        assert_all_lazy(&migrated);
        assert_eq!(
            fs::read(source.join("project.json")).unwrap(),
            original_manifest
        );
        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(target.join("project.json")).unwrap()).unwrap();
        assert_eq!(written, expected_manifest);
        for (directory, id) in [
            ("pages", &first),
            ("pages", &second),
            ("components", &component),
        ] {
            assert!(source.join(directory).join(format!("{id}.comp")).is_dir());
            assert!(target.join(directory).join(format!("{id}.omuse")).is_dir());
            assert!(!target.join(directory).join(format!("{id}.comp")).exists());
        }

        // The saved live project must now resolve its lazy paths independently
        // of the legacy source, as must a freshly reopened project.
        fs::remove_dir_all(&source).unwrap();
        let mut reopened = Project::open(&target).unwrap();
        for project in [&mut migrated, &mut reopened] {
            assert_eq!(project.active_page_id(), second);
            let cover = project.page_document(&first).unwrap();
            assert_eq!(cover.name, "Cover");
            assert_eq!(
                cover.metadata["creation-note"],
                serde_json::json!({"preserve": [1, 2, 3]})
            );
            assert_eq!(cover.layers[0].name, "Editable artwork");
            assert_eq!(cover.layers[0].opacity, 0.625);
            assert_eq!(
                cover.layers[0].metadata["layer-note"],
                serde_json::json!({"preserve": true})
            );
            assert_eq!(layer_pixel(&cover.layers[0]), [10, 20, 30, 255]);
            assert_eq!(
                layer_pixel(&project.page_document(&second).unwrap().layers[0]),
                [40, 50, 60, 255]
            );
            let (revision, layers) = project.component_snapshot(&component).unwrap();
            assert_eq!(revision, 2);
            assert_eq!(layer_pixel(&layers[0]), [10, 20, 30, 255]);
            assert_eq!(
                layers[0].metadata["layer-note"],
                serde_json::json!({"preserve": true})
            );
            assert_eq!(project.resource_bytes(&resource).unwrap(), &[1, 3, 5, 7]);
        }
    }

    #[test]
    fn legacy_collection_same_path_migration_keeps_lazy_undo_snapshots() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Campaign.omuse");
        let (first, second, component, resource) = write_legacy_collection(&path);
        let mut current = Project::open(&path).unwrap();
        let mut undo = current.clone();
        current
            .replace_page_document(&first, document([70, 80, 90, 255]))
            .unwrap();
        current.save(&path).unwrap();
        assert!(!path.join("pages").join(format!("{second}.comp")).exists());
        assert!(path.join("pages").join(format!("{second}.omuse")).is_dir());
        assert!(
            !current
                .page_summaries()
                .iter()
                .find(|page| page.id == second)
                .unwrap()
                .is_loaded
        );
        assert!(!current.component_summaries()[0].is_loaded);
        assert!(!current.resource_summaries()[0].is_loaded);

        assert_eq!(
            layer_pixel(&undo.page_document(&first).unwrap().layers[0]),
            [10, 20, 30, 255]
        );
        assert_eq!(
            layer_pixel(&undo.page_document(&second).unwrap().layers[0]),
            [40, 50, 60, 255]
        );
        assert_eq!(
            layer_pixel(&undo.component_snapshot(&component).unwrap().1[0]),
            [10, 20, 30, 255]
        );
        assert_eq!(undo.resource_bytes(&resource).unwrap(), &[1, 3, 5, 7]);
        assert_eq!(
            layer_pixel(&current.page_document(&first).unwrap().layers[0]),
            [70, 80, 90, 255]
        );
        assert_eq!(
            layer_pixel(&current.page_document(&second).unwrap().layers[0]),
            [40, 50, 60, 255]
        );
        assert_eq!(
            layer_pixel(&current.component_snapshot(&component).unwrap().1[0]),
            [10, 20, 30, 255]
        );
        assert_eq!(current.resource_bytes(&resource).unwrap(), &[1, 3, 5, 7]);

        // Undo can publish its preserved legacy pixels back to the same path,
        // writing the current layout instead of reviving old .comp packages.
        undo.save(&path).unwrap();
        let mut reopened = Project::open(&path).unwrap();
        assert_eq!(
            layer_pixel(&reopened.page_document(&first).unwrap().layers[0]),
            [10, 20, 30, 255]
        );
        assert!(
            !path
                .join("components")
                .join(format!("{component}.comp"))
                .exists()
        );
    }

    #[test]
    fn declared_collection_version_rejects_missing_packages_without_extension_fallback() {
        for version in [1, 2] {
            for nested in ["pages", "components"] {
                let directory = tempdir().unwrap();
                let path = directory.path().join("Missing.omuse");
                let (first, _, component, _) = write_legacy_collection(&path);
                if version == 2 {
                    Project::open(&path).unwrap().save(&path).unwrap();
                }
                let id = if nested == "pages" { first } else { component };
                let (expected, other) = if version == 1 {
                    ("comp", "omuse")
                } else {
                    ("omuse", "comp")
                };
                fs::rename(
                    path.join(nested).join(format!("{id}.{expected}")),
                    path.join(nested).join(format!("{id}.{other}")),
                )
                .unwrap();
                let error = Project::open(&path).unwrap_err().to_string();
                assert!(error.contains("Missing or unsafe"), "{error}");
            }
        }
    }

    #[test]
    fn malformed_current_packages_never_fall_back_to_valid_legacy_siblings() {
        for nested in ["pages", "components"] {
            let directory = tempdir().unwrap();
            let path = directory.path().join("Malformed.omuse");
            let (first, _, component, _) = write_legacy_collection(&path);
            Project::open(&path).unwrap().save(&path).unwrap();
            let id = if nested == "pages" {
                &first
            } else {
                &component
            };
            let current = path.join(nested).join(format!("{id}.omuse"));
            let legacy = path.join(nested).join(format!("{id}.comp"));
            fs::rename(&current, &legacy).unwrap();
            fs::create_dir(&current).unwrap();
            fs::write(current.join("manifest.json"), b"invalid JSON").unwrap();
            let before = fs::read(path.join("project.json")).unwrap();
            let mut project = Project::open(&path).unwrap();
            assert_all_lazy(&project);
            if nested == "pages" {
                assert!(project.page_document(&first).is_err());
            } else {
                assert!(project.component_snapshot(&component).is_err());
            }
            assert!(project.save(&path).is_err());
            assert_eq!(fs::read(path.join("project.json")).unwrap(), before);
            assert!(document::open(&legacy).is_ok());
        }
    }

    #[test]
    fn legacy_migration_refuses_an_external_source_replacement() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Changed.omuse");
        write_legacy_collection(&path);
        let mut project = Project::open(&path).unwrap();
        let mut replacement = Project::new("External edit", document([91, 92, 93, 255]));
        replacement.save(&path).unwrap();
        let before = fs::read(path.join("project.json")).unwrap();
        let error = format!("{:#}", project.save(&path).unwrap_err());
        assert!(error.contains("changed on disk"), "{error}");
        assert_eq!(fs::read(path.join("project.json")).unwrap(), before);
        assert_all_lazy(&project);
    }

    #[test]
    fn unknown_collection_versions_are_rejected_before_reading_nested_packages() {
        for version in [0, 3] {
            let directory = tempdir().unwrap();
            let path = directory.path().join("Unknown.omuse");
            write_legacy_collection(&path);
            let manifest_path = path.join("project.json");
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
            manifest["version"] = serde_json::json!(version);
            fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert!(
                Project::open(&path)
                    .unwrap_err()
                    .to_string()
                    .contains("Unsupported Create project version")
            );
        }
    }

    #[test]
    fn project_roundtrip_keeps_inactive_pages_and_resources_lazy() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Campaign.omuse");
        let mut project = Project::new("Campaign", document([10, 20, 30, 255]));
        let second = project
            .add_page("Story", document([90, 80, 70, 255]))
            .unwrap();
        project.set_active_page(&second).unwrap();
        let resource = project
            .add_resource("Logo", "image/png", vec![1, 2, 3, 4])
            .unwrap();
        project.save(&path).unwrap();

        let mut reopened = Project::open(&path).unwrap();
        assert_eq!(reopened.page_summaries().len(), 2);
        assert!(reopened.page_summaries().iter().all(|page| !page.is_loaded));
        let mut visited = 0;
        reopened
            .for_each_page_document(|_, document| {
                visited += 1;
                assert_eq!(document.layers.len(), 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, 2);
        assert!(
            reopened.page_summaries().iter().all(|page| !page.is_loaded),
            "read-only traversal must not retain inactive documents"
        );
        assert!(!reopened.resource_summaries()[0].is_loaded);
        assert_eq!(reopened.resource_bytes(&resource).unwrap(), &[1, 2, 3, 4]);
        assert_eq!(
            reopened.active_document().unwrap().layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [90, 80, 70, 255]
        );
        assert_eq!(
            reopened
                .page_summaries()
                .iter()
                .filter(|page| page.is_loaded)
                .count(),
            1
        );
    }

    #[test]
    fn unopened_lazy_page_rejects_an_externally_replaced_package_without_caching() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Changed.omuse");
        let mut project = Project::new("Original", document([10, 20, 30, 255]));
        let second = project
            .add_page("Unopened", document([40, 50, 60, 255]))
            .unwrap();
        project.save(&path).unwrap();

        let mut reopened = Project::open(&path).unwrap();
        assert!(reopened.page_summaries().iter().all(|page| !page.is_loaded));

        let mut replacement = Project::new("External", document([90, 80, 70, 255]));
        replacement.save(&path).unwrap();

        let error = format!("{:#}", reopened.page_document(&second).unwrap_err());
        assert!(error.contains("changed on disk"), "{error}");
        assert!(
            reopened
                .page_summaries()
                .iter()
                .any(|page| page.id == second && !page.is_loaded),
            "a rejected external page must never populate the lazy cache"
        );
    }

    #[test]
    fn lazy_resource_rejects_an_externally_replaced_package_without_caching() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("ChangedResource.omuse");
        let mut project = Project::new("Original", document([10, 20, 30, 255]));
        let resource = project
            .add_resource("Logo", "image/png", vec![1, 2, 3, 4])
            .unwrap();
        project.save(&path).unwrap();

        let mut reopened = Project::open(&path).unwrap();
        assert!(!reopened.resource_summaries()[0].is_loaded);

        let mut replacement = Project::new("External", document([90, 80, 70, 255]));
        replacement.save(&path).unwrap();

        let error = reopened.resource_bytes(&resource).unwrap_err().to_string();
        assert!(error.contains("changed on disk"), "{error}");
        assert!(
            !reopened.resource_summaries()[0].is_loaded,
            "a rejected external resource must never populate the lazy cache"
        );
    }

    #[test]
    fn page_operations_preserve_order_and_cow_pixels() {
        let mut project = Project::new("Campaign", document([1, 2, 3, 255]));
        let first = project.active_page_id().to_owned();
        let duplicate = project.duplicate_page(&first).unwrap();
        let first_pixels = project.page_document(&first).unwrap().layers[0]
            .image
            .as_ref()
            .unwrap()
            .clone();
        let duplicate_pixels = project.page_document(&duplicate).unwrap().layers[0]
            .image
            .as_ref()
            .unwrap()
            .clone();
        assert!(first_pixels.shares_pixels_with(&duplicate_pixels));
        project.reorder_page(&duplicate, 0).unwrap();
        assert_eq!(project.page_summaries()[0].id, duplicate);
        project.remove_page(&first).unwrap();
        assert_eq!(project.page_summaries().len(), 1);
    }

    #[test]
    fn failed_validation_does_not_replace_a_saved_project() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Atomic.omuse");
        let mut project = Project::new("Good", document([1, 2, 3, 255]));
        project.save(&path).unwrap();
        let before = fs::read(path.join("project.json")).unwrap();
        project.title.clear();
        assert!(project.save(&path).is_err());
        assert_eq!(fs::read(path.join("project.json")).unwrap(), before);
    }

    #[test]
    fn publish_check_failure_keeps_the_existing_package() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Checked.omuse");
        let mut project = Project::new("First", document([1, 2, 3, 255]));
        project.save(&path).unwrap();
        let before = fs::read(path.join("project.json")).unwrap();

        project.title = "Second".into();
        assert!(
            project
                .save_checked(&path, || anyhow::bail!("package changed externally"))
                .is_err()
        );
        assert_eq!(fs::read(path.join("project.json")).unwrap(), before);
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse-create-stage-")
        }));
    }

    #[test]
    fn project_metadata_roundtrips_with_a_component_reference() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Metadata.omuse");
        let mut project = Project::new("Campaign", document([1, 2, 3, 255]));
        assert!(!project.metadata.local_only);
        let prototype = project.active_document().unwrap().layers[0].clone();
        let component = project
            .define_component("Background", vec![prototype])
            .unwrap();
        project.metadata.shared_background_component_id = Some(component.clone());
        project.metadata.local_only = true;
        project.save(&path).unwrap();

        let reopened = Project::open(&path).unwrap();
        assert_eq!(
            reopened.metadata.shared_background_component_id,
            Some(component)
        );
        assert!(reopened.metadata.local_only);
    }

    #[test]
    fn old_lazy_snapshot_survives_an_atomic_save_exchange() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("Snapshot.omuse");
        let mut project = Project::new("Campaign", document([1, 2, 3, 255]));
        let second = project
            .add_page("Second", document([4, 5, 6, 255]))
            .unwrap();
        let logo = project
            .add_resource("Logo", "image/png", vec![10, 20, 30])
            .unwrap();
        project.save(&path).unwrap();

        let mut current = Project::open(&path).unwrap();
        let mut snapshot = current.clone();
        let first = current.active_page_id().to_owned();
        current
            .replace_page_document(&first, document([70, 80, 90, 255]))
            .unwrap();
        current.save(&path).unwrap();

        assert_eq!(
            snapshot.page_document(&first).unwrap().layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [1, 2, 3, 255]
        );
        assert_eq!(
            snapshot.page_document(&second).unwrap().layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [4, 5, 6, 255]
        );
        assert_eq!(snapshot.resource_bytes(&logo).unwrap(), &[10, 20, 30]);
        assert_eq!(
            current.page_document(&first).unwrap().layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [70, 80, 90, 255]
        );
        assert_eq!(
            current.page_document(&second).unwrap().layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [4, 5, 6, 255],
            "the saved live project must rebase its remaining lazy page to the new package"
        );
    }
}
