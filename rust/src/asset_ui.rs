//! Native local-asset browser for Create.
use super::create_ui::{note, section};
use super::inspector_ui::panel_button as button;
use super::inspector_ui::panel_input as input;
use super::*;
use anyhow::Context as _;
use image::ImageDecoder;
use omuse::asset_library::{AssetLibrary, AssetRecord, ImportMetadata};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

const MAX_VISIBLE_ASSETS: usize = 30;
const MAX_THUMBNAILS: usize = 128;
const MAX_PENDING_THUMBNAILS: usize = 128;
const MAX_IMPORT_FILES: usize = 64;
const THUMBNAIL_EDGE: u32 = 72;

/// An asset-library source read once from its content-addressed store. The
/// exact bytes are copied into the Create package when the operation commits,
/// keeping placed artwork editable after the local library moves or is pruned.
struct LoadedLibraryAsset {
    pixels: image::RgbaImage,
    source_bytes: Vec<u8>,
}

struct ThumbnailRequest {
    id: String,
    path: PathBuf,
}

#[derive(Default)]
struct ThumbnailCache {
    ready: HashMap<String, Arc<RenderImage>>,
    failed: HashMap<String, String>,
    queued: HashSet<String>,
    pending: VecDeque<ThumbnailRequest>,
    active: usize,
}

struct AssetImportJob {
    cancel: Arc<AtomicBool>,
    receiver: mpsc::Receiver<ImportEvent>,
    detail: String,
}

impl Drop for AssetImportJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

enum ImportEvent {
    Progress {
        completed: usize,
        total: usize,
        name: String,
    },
    Finished(ImportReport),
}

struct ImportReport {
    imported: usize,
    errors: Vec<String>,
    cancelled: bool,
}

pub(super) struct AssetUiState {
    selected_id: Option<String>,
    tag_input: Entity<InputState>,
    thumbnails: RefCell<ThumbnailCache>,
    import_job: Option<AssetImportJob>,
    import_report: Option<String>,
}

impl AssetUiState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        Self {
            selected_id: None,
            tag_input: cx.new(|cx| InputState::new(window, cx).default_value("")),
            thumbnails: RefCell::new(ThumbnailCache::default()),
            import_job: None,
            import_report: None,
        }
    }

    pub(super) fn release(&mut self, cx: &mut App) {
        self.import_job = None;
        let cache = self.thumbnails.get_mut();
        for (_, image) in cache.ready.drain() {
            cx.drop_image(image, None);
        }
        cache.pending.clear();
        cache.queued.clear();
    }
}

impl EditorView {
    pub(super) fn load_asset_library(&mut self) {
        let result = match self.create.library.as_mut() {
            Some(library) => library.reload(),
            None => AssetLibrary::open(omuse::identity::data_dir().join("assets")).map(|library| {
                self.create.library = Some(library);
            }),
        };
        match result {
            Ok(()) => self.create.library_error = None,
            Err(error) => self.create.library_error = Some(format!("{error:#}")),
        }
    }

    pub(super) fn create_assets_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = section("YOUR ASSETS", cx)
            .child(note(
                "A local library shared by your Omuse projects. Imports and previews run in the background.",
                cx,
            ))
            .child(note("Search library", cx))
            .child(input(
                "create-asset-search",
                &self.create.fields[9],
                window,
                cx,
            ))
            .child(
                button(
                    "asset-import",
                    "Add images to library…",
                    ButtonVariant::Primary,
                    cx,
                )
                .w_full()
                .disabled(self.asset_ui.import_job.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.choose_assets(window, cx))),
            );
        if let Some(job) = &self.asset_ui.import_job {
            body = body.child(note(job.detail.clone(), cx)).child(
                button(
                    "asset-import-cancel",
                    "Cancel import",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(job) = &this.asset_ui.import_job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    cx.notify();
                })),
            );
        }
        if let Some(report) = &self.asset_ui.import_report {
            body = body.child(note(report.clone(), cx));
        }
        if let Some(error) = &self.create.library_error {
            body = body.child(note(error.clone(), cx));
        }

        let assets = self
            .create
            .library
            .as_ref()
            .map(|library| {
                let query = self.create.value(9, cx);
                let query = if query == "Search assets" { "" } else { &query };
                library
                    .search(query, MAX_VISIBLE_ASSETS)
                    .map(|assets| assets.into_iter().cloned().collect::<Vec<_>>())
            })
            .transpose();
        match assets {
            Ok(Some(assets)) => {
                if assets.is_empty() {
                    body = body.child(note("No matching assets.", cx));
                }
                for asset in assets {
                    self.queue_asset_thumbnail(&asset, cx);
                    body = body.child(self.asset_row(&asset, cx));
                }
                self.pump_asset_thumbnails(cx);
            }
            Ok(None) => body = body.child(note("Open the library to add local assets.", cx)),
            Err(error) => body = body.child(note(error.to_string(), cx)),
        }

        if let Some(id) = &self.asset_ui.selected_id
            && let Some(asset) = self
                .create
                .library
                .as_ref()
                .and_then(|library| library.get(id))
                .cloned()
        {
            body = body.child(self.asset_detail(&asset, window, cx));
        }
        body.into_any_element()
    }

    fn asset_row(&self, asset: &AssetRecord, cx: &mut Context<Self>) -> AnyElement {
        let id = asset.id.clone();
        let selected = self.asset_ui.selected_id.as_deref() == Some(&asset.id);
        let thumbnail = self
            .asset_ui
            .thumbnails
            .borrow()
            .ready
            .get(&asset.id)
            .cloned();
        let image = match thumbnail {
            Some(image) => gpui_kit::img(image)
                .size(px(48.))
                .object_fit(gpui_kit::ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .size(px(48.))
                .rounded(px(6.))
                .bg(cx.omarchy().background)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .text_color(cx.omarchy().secondary)
                .child("IMAGE")
                .into_any_element(),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(image)
            .child(
                button(
                    SharedString::from(format!("asset-{id}")),
                    SharedString::from(asset.name.clone()),
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(selected)
                .flex_1()
                .min_w_0()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_library_asset(&id, window, cx)
                })),
            )
            .child(note(if asset.favorite { "★" } else { "☆" }, cx))
            .into_any_element()
    }

    fn asset_detail(
        &self,
        asset: &AssetRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = asset.id.clone();
        let favourite_id = id.clone();
        let insert_id = id.clone();
        let cutout_id = id.clone();
        let replace_id = id.clone();
        let frame_state = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|layer| omuse::create::frame_spec(layer).ok().flatten())
            .map(|_| {
                !layer_is_locked(
                    &self.editor.document.layers,
                    &self.editor.active_layer,
                    false,
                )
            });
        let tags = if asset.tags.is_empty() {
            "No tags".into()
        } else {
            asset.tags.iter().cloned().collect::<Vec<_>>().join(", ")
        };
        section("SELECTED ASSET", cx)
            .child(note(asset.name.clone(), cx))
            .child(note(
                format!(
                    "{} × {} · {} KiB · {}",
                    asset.width,
                    asset.height,
                    asset.bytes.div_ceil(1024),
                    asset.media_type
                ),
                cx,
            ))
            .child(note(format!("Tags: {tags}"), cx))
            .child(note(
                if asset.provenance.is_empty() {
                    "Provenance: not recorded".into()
                } else {
                    format!("Provenance: {}", asset.provenance)
                },
                cx,
            ))
            .child(note(
                "Placing or replacing packages this checked library source with the project (up to 64 MiB).",
                cx,
            ))
            .child(input("asset-tags", &self.asset_ui.tag_input, window, cx))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button("asset-save-tags", "Save tags", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let result = this.save_asset_tags(&id, cx);
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button(
                            "asset-favourite",
                            if asset.favorite {
                                "Unfavourite"
                            } else {
                                "Favourite"
                            },
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let result = this.toggle_asset_favourite(&favourite_id, cx);
                            this.create_error(result, cx);
                        })),
                    ),
            )
            .child(
                button(
                    "asset-insert",
                    "Place on current page",
                    ButtonVariant::Primary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(move |this, _, _, cx| {
                    let result = this.insert_library_asset(&insert_id, cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                button(
                    "asset-replace-frame",
                    "Replace selected frame",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .disabled(frame_state != Some(true))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let result = this.replace_selected_frame(&replace_id, cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note(
                match frame_state {
                    Some(true) => "Keeps the frame crop and layout.",
                    Some(false) => "Unlock the selected frame or its group to replace it.",
                    None => "Select an image frame to replace its source.",
                },
                cx,
            ))
            .child(
                button(
                    "asset-insert-cutout",
                    "Place and remove background…",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .disabled(u64::from(asset.width) * u64::from(asset.height) > 16_777_216)
                .on_click(cx.listener(move |this, _, window, cx| {
                    let result = this.insert_library_asset_for_cutout(&cutout_id, window, cx);
                    this.create_error(result, cx);
                })),
            )
            .into_any_element()
    }

    fn select_library_asset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.asset_ui.selected_id = Some(id.to_owned());
        let tags = self
            .create
            .library
            .as_ref()
            .and_then(|library| library.get(id))
            .map(|asset| asset.tags.iter().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        self.asset_ui
            .tag_input
            .update(cx, |input, cx| input.set_value(tags, window, cx));
        cx.notify();
    }

    fn save_asset_tags(&mut self, id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let tags = self
            .asset_ui
            .tag_input
            .read(cx)
            .value()
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        self.create
            .library
            .as_mut()
            .context("Asset library is unavailable")?
            .set_tags(id, tags)?;
        cx.notify();
        Ok(())
    }

    fn toggle_asset_favourite(&mut self, id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let library = self
            .create
            .library
            .as_mut()
            .context("Asset library is unavailable")?;
        let favourite = !library
            .get(id)
            .context("Asset is no longer available")?
            .favorite;
        library.set_favorite(id, favourite)?;
        cx.notify();
        Ok(())
    }

    pub(super) fn choose_assets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.asset_ui.import_job.is_some() {
            return;
        }
        let task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add images to Omuse".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let paths = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if let Ok(Ok(Some(paths))) = paths {
                    let result = this.start_asset_import(paths, cx);
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn start_asset_import(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=MAX_IMPORT_FILES).contains(&paths.len()),
            "Choose 1–{MAX_IMPORT_FILES} images per import"
        );
        anyhow::ensure!(
            self.asset_ui.import_job.is_none(),
            "An asset import is already running"
        );
        let root = omuse::identity::data_dir().join("assets");
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(16);
        std::thread::Builder::new()
            .name("omuse-asset-import".into())
            .spawn(move || {
                let mut errors = Vec::new();
                let mut imported = 0usize;
                let mut library = match AssetLibrary::open(root) {
                    Ok(library) => library,
                    Err(error) => {
                        let _ = sender.send(ImportEvent::Finished(ImportReport {
                            imported: 0,
                            errors: vec![format!("Cannot open asset library: {error:#}")],
                            cancelled: false,
                        }));
                        return;
                    }
                };
                let total = paths.len();
                for (index, path) in paths.into_iter().enumerate() {
                    if worker_cancel.load(Ordering::Relaxed) {
                        let _ = sender.send(ImportEvent::Finished(ImportReport {
                            imported,
                            errors,
                            cancelled: true,
                        }));
                        return;
                    }
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("image")
                        .to_owned();
                    let provenance = format!("Imported from {}", path.display());
                    match library.import(
                        &path,
                        ImportMetadata {
                            provenance,
                            ..Default::default()
                        },
                    ) {
                        Ok(_) => imported += 1,
                        Err(error) => errors.push(format!("{name}: {error:#}")),
                    }
                    let _ = sender.try_send(ImportEvent::Progress {
                        completed: index + 1,
                        total,
                        name,
                    });
                }
                let _ = sender.send(ImportEvent::Finished(ImportReport {
                    imported,
                    errors,
                    cancelled: false,
                }));
            })?;
        self.asset_ui.import_job = Some(AssetImportJob {
            cancel,
            receiver,
            detail: "Preparing asset import…".into(),
        });
        self.asset_ui.import_report = None;
        self.poll_asset_import(cx);
        Ok(())
    }

    fn poll_asset_import(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let keep = view
                    .update(cx, |this, cx| {
                        let mut finished = None;
                        if let Some(job) = &mut this.asset_ui.import_job {
                            while let Ok(event) = job.receiver.try_recv() {
                                match event {
                                    ImportEvent::Progress {
                                        completed,
                                        total,
                                        name,
                                    } => {
                                        job.detail =
                                            format!("Processed {completed} of {total}: {name}");
                                    }
                                    ImportEvent::Finished(report) => {
                                        finished = Some(report);
                                        break;
                                    }
                                }
                            }
                        }
                        if let Some(report) = finished {
                            this.asset_ui.import_job = None;
                            this.load_asset_library();
                            let mut detail = if report.cancelled {
                                format!("Import cancelled after {} images", report.imported)
                            } else {
                                format!("Imported {} images", report.imported)
                            };
                            if !report.errors.is_empty() {
                                detail.push_str(&format!(
                                    "; {} failed: {}",
                                    report.errors.len(),
                                    report.errors.join(" | ")
                                ));
                            }
                            this.asset_ui.import_report = Some(detail);
                        }
                        cx.notify();
                        this.asset_ui.import_job.is_some()
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn insert_library_asset(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.start_library_insert(id, false, None, cx)
    }

    fn insert_library_asset_for_cutout(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.start_library_insert(id, true, Some(window), cx)
    }

    fn replace_selected_frame(&mut self, id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let library = self
            .create
            .library
            .as_ref()
            .context("Asset library is unavailable")?;
        let asset = library
            .get(id)
            .context("Asset is no longer available")?
            .clone();
        let path = library.asset_path(id)?;
        let layer_id = self.editor.active_layer.clone();
        anyhow::ensure!(
            self.editor
                .document
                .find_layer(&layer_id)
                .and_then(|layer| omuse::create::frame_spec(layer).ok().flatten())
                .is_some(),
            "Select an image frame before replacing its source"
        );
        anyhow::ensure!(
            !layer_is_locked(&self.editor.document.layers, &layer_id, false),
            "Unlock the selected frame or its group before replacing it"
        );

        let source_epoch = self.create.epoch;
        let source_revision = self.editor.revision();
        self.status = format!("Loading {} for frame…", asset.name);
        let expected_bytes = asset.bytes;
        let expected_hash = asset.sha256.clone();
        let task = cx.background_executor().spawn(async move {
            let source_bytes = read_library_source(&path, expected_bytes, &expected_hash)?;
            let pixels = decode_asset_full(&source_bytes)?;
            Ok::<_, anyhow::Error>(LoadedLibraryAsset {
                pixels,
                source_bytes,
            })
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                let result = (|| -> anyhow::Result<()> {
                    anyhow::ensure!(
                        this.create.epoch == source_epoch
                            && this.editor.revision() == source_revision,
                        "The page changed while the asset was loading; replacement was cancelled"
                    );
                    anyhow::ensure!(
                        !layer_is_locked(&this.editor.document.layers, &layer_id, false),
                        "Unlock the selected frame or its group before replacing it"
                    );
                    this.ensure_create()?;
                    this.sync_create()?;
                    let mut draft = this
                        .create
                        .session
                        .as_ref()
                        .context("Create project closed")?
                        .project
                        .clone();
                    let loaded = result?;
                    let resource_id =
                        package_library_source(&mut draft, &asset, loaded.source_bytes)?;
                    let mut document = this.editor.document.clone();
                    let layer = document
                        .find_layer_mut(&layer_id)
                        .context("The selected frame is no longer available")?;
                    omuse::create::replace_frame_image(layer, loaded.pixels)?;
                    record_frame_source(layer, &asset, &resource_id);
                    draft.replace_active_document(document)?;
                    this.apply_creative_project(draft, cx)?;
                    this.editor.active_layer = layer_id.clone();
                    this.status = format!("Replaced frame with {}", asset.name);
                    Ok(())
                })();
                this.create_error(result, cx);
            });
        })
        .detach();
        Ok(())
    }

    fn start_library_insert(
        &mut self,
        id: &str,
        cutout: bool,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let library = self
            .create
            .library
            .as_ref()
            .context("Asset library is unavailable")?;
        let path = library.asset_path(id)?;
        let asset = library
            .get(id)
            .context("Asset is no longer available")?
            .clone();
        let name = asset.name.clone();
        let source_epoch = self.create.epoch;
        let source_revision = self.editor.revision();
        self.status = format!("Loading {name}…");
        let task = cx.background_executor().spawn(async move {
            let source_bytes = read_library_source(&path, asset.bytes, &asset.sha256)?;
            let pixels = decode_asset_full(&source_bytes)?;
            let mut layer = Layer::paint(&name, pixels.width(), pixels.height());
            layer.image = Some(pixels.into());
            layer.name = name;
            Ok::<_, anyhow::Error>((layer, source_bytes, asset))
        });
        if let Some(window) = window.take() {
            cx.spawn_in(window, async move |view, cx| {
                let result = task.await;
                let _ = view.update_in(cx, |this, window, cx| match result {
                    Ok((layer, source_bytes, asset))
                        if this.create.epoch == source_epoch
                            && this.editor.revision() == source_revision =>
                    {
                        let operation = this.place_library_asset(layer, &asset, source_bytes, cx);
                        if operation.is_ok() && cutout {
                            this.start_subject(false, window, cx);
                        }
                        this.create_error(operation, cx);
                    }
                    Ok(_) => {
                        this.status =
                            "Asset loaded, but the page changed; placement was cancelled".into();
                        cx.notify();
                    }
                    Err(error) => {
                        this.status = format!("Asset placement: {error:#}");
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            cx.spawn(async move |view, cx| {
                let result = task.await;
                let _ = view.update(cx, |this, cx| match result {
                    Ok((layer, source_bytes, asset))
                        if this.create.epoch == source_epoch
                            && this.editor.revision() == source_revision =>
                    {
                        let result = this.place_library_asset(layer, &asset, source_bytes, cx);
                        this.create_error(result, cx);
                    }
                    Ok(_) => {
                        this.status =
                            "Asset loaded, but the page changed; placement was cancelled".into();
                        cx.notify();
                    }
                    Err(error) => {
                        this.status = format!("Asset placement: {error:#}");
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        Ok(())
    }

    /// Commit a placed library image and the exact checked source bytes in one
    /// project replacement. The source resource is reusable by later pages and
    /// survives a library deletion without relying on an external path.
    fn place_library_asset(
        &mut self,
        mut layer: Layer,
        asset: &AssetRecord,
        source_bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.ensure_create()?;
        self.sync_create()?;
        let mut draft = self
            .create
            .session
            .as_ref()
            .context("Create project closed")?
            .project
            .clone();
        let resource_id = package_library_source(&mut draft, asset, source_bytes)?;
        record_asset_source(&mut layer, asset, &resource_id);
        let id = layer.id.clone();
        let mut document = self.editor.document.clone();
        document.layers.push(layer);
        draft.replace_active_document(document)?;
        self.apply_creative_project(draft, cx)?;
        self.editor.active_layer = id;
        self.status = format!("Placed {} (source packaged in this project)", asset.name);
        self.schedule_content_recovery();
        Ok(())
    }

    fn queue_asset_thumbnail(&self, asset: &AssetRecord, cx: &mut Context<Self>) {
        let Some(path) = self
            .create
            .library
            .as_ref()
            .and_then(|library| library.asset_path(&asset.id).ok())
        else {
            return;
        };
        let mut cache = self.asset_ui.thumbnails.borrow_mut();
        if cache.ready.contains_key(&asset.id)
            || cache.failed.contains_key(&asset.id)
            || cache.queued.contains(&asset.id)
            || cache.ready.len() + cache.failed.len() + cache.queued.len() >= MAX_THUMBNAILS
            || cache.pending.len() >= MAX_PENDING_THUMBNAILS
        {
            return;
        }
        cache.queued.insert(asset.id.clone());
        cache.pending.push_back(ThumbnailRequest {
            id: asset.id.clone(),
            path,
        });
        drop(cache);
        cx.notify();
    }

    fn pump_asset_thumbnails(&self, cx: &mut Context<Self>) {
        let request = {
            let mut cache = self.asset_ui.thumbnails.borrow_mut();
            if cache.active >= 1 {
                None
            } else {
                cache.pending.pop_front().inspect(|_| cache.active += 1)
            }
        };
        let Some(request) = request else {
            return;
        };
        let id = request.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { decode_asset_thumbnail(&request.path) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                let mut cache = this.asset_ui.thumbnails.borrow_mut();
                cache.active = cache.active.saturating_sub(1);
                cache.queued.remove(&id);
                match result {
                    Ok(pixels) => {
                        cache.ready.insert(id, render_image(&pixels));
                    }
                    Err(error) => {
                        cache.failed.insert(id, format!("{error:#}"));
                    }
                }
                drop(cache);
                this.pump_asset_thumbnails(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

fn decode_asset_thumbnail(path: &Path) -> anyhow::Result<image::RgbaImage> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_file() && metadata.len() <= omuse::asset_library::MAX_ASSET_BYTES,
        "thumbnail source is not a supported regular file"
    );
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    anyhow::ensure!(
        omuse::model::valid_dimensions(width, height),
        "thumbnail source exceeds image bounds"
    );
    let image = image::DynamicImage::from_decoder(decoder)?;
    Ok(image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE).to_rgba8())
}

fn read_library_source(
    path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> anyhow::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_file()
            && metadata.len() == expected_bytes
            && metadata.len() <= omuse::create_project::MAX_SHARED_RESOURCE_BYTES,
        "asset source cannot be packaged: it must be an unchanged regular file of at most 64 MiB"
    );
    let mut bytes = Vec::with_capacity(expected_bytes as usize);
    File::open(path)?
        .take(omuse::create_project::MAX_SHARED_RESOURCE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 == expected_bytes,
        "asset source changed while it was being packaged"
    );
    anyhow::ensure!(
        omuse::asset_library::sha256_hex(&bytes) == expected_sha256,
        "asset source hash no longer matches the selected library record"
    );
    Ok(bytes)
}

fn decode_asset_full(bytes: &[u8]) -> anyhow::Result<image::RgbaImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    anyhow::ensure!(
        omuse::model::valid_dimensions(width, height),
        "asset source exceeds image limits"
    );
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.to_rgba8())
}

fn package_library_source(
    project: &mut omuse::create_project::Project,
    asset: &AssetRecord,
    bytes: Vec<u8>,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        bytes.len() as u64 == asset.bytes,
        "asset source length does not match the library record"
    );
    anyhow::ensure!(
        omuse::asset_library::sha256_hex(&bytes) == asset.sha256,
        "asset source hash does not match the library record"
    );
    let name = format!("Library source {}", asset.sha256);
    if let Some(existing) = project
        .resource_summaries()
        .into_iter()
        .find(|resource| resource.name == name)
    {
        anyhow::ensure!(
            existing.byte_len == asset.bytes && existing.media_type == asset.media_type,
            "A packaged source with this content identity has incompatible metadata"
        );
        return Ok(existing.id);
    }
    project.add_resource(name, asset.media_type.clone(), bytes)
}

fn asset_source_value(asset: &AssetRecord, resource_id: &str) -> serde_json::Value {
    serde_json::json!({
        "assetId": asset.id,
        "name": asset.name,
        "sha256": asset.sha256,
        "provenance": asset.provenance,
        "projectResourceId": resource_id,
    })
}

fn record_asset_source(layer: &mut Layer, asset: &AssetRecord, resource_id: &str) {
    if !layer.metadata.is_object() {
        layer.metadata = serde_json::json!({});
    }
    if !layer
        .metadata
        .get("omuseCreate")
        .is_some_and(serde_json::Value::is_object)
    {
        layer.metadata["omuseCreate"] = serde_json::json!({});
    }
    layer.metadata["omuseCreate"]["assetSource"] = asset_source_value(asset, resource_id);
}

fn record_frame_source(layer: &mut Layer, asset: &AssetRecord, resource_id: &str) {
    record_asset_source(layer, asset, resource_id);
    layer.metadata["omuseCreate"]["frameSource"] = asset_source_value(asset, resource_id);
}

fn layer_is_locked(layers: &[Layer], id: &str, parent_locked: bool) -> bool {
    layer_lock_state(layers, id, parent_locked).unwrap_or(false)
}

fn layer_lock_state(layers: &[Layer], id: &str, parent_locked: bool) -> Option<bool> {
    for layer in layers {
        let locked = parent_locked || layer.locked;
        if layer.id == id {
            return Some(locked);
        }
        if let Some(found) = layer_lock_state(&layer.children, id, locked) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_source_capture_rejects_same_length_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("selected.png");
        std::fs::write(&source, b"original bytes").unwrap();
        let expected = omuse::asset_library::sha256_hex(b"original bytes");

        assert_eq!(
            read_library_source(&source, 14, &expected).unwrap(),
            b"original bytes"
        );

        std::fs::write(&source, b"tampered bytes").unwrap();
        let error = read_library_source(&source, 14, &expected).unwrap_err();
        assert!(error.to_string().contains("hash"));
    }
}
