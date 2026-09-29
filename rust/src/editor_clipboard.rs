//! In-process whole-layer clipboard. The immutable snapshot shares raster and
//! editable-source allocations; a paste gives every layer a new identity.
use super::{Editor, regenerate_forest_ids, selected_roots, tree_path, valid_insert_tree};
use crate::model::{Document, Layer};
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use serde_json::Value;
use std::{collections::HashSet, mem::size_of, sync::Arc};

const MAX_CLIPBOARD_BYTES: usize = 256 * 1024 * 1024;
const MAX_PREVIEW_PIXELS: u64 = 16_000_000;
const MAX_DEPTH: usize = 64;
const MAX_METADATA_DEPTH: usize = 128;

/// Editable, document-independent clipboard content. No mutable layer or pixel
/// references escape this type, including through a cloned clipboard.
#[derive(Clone, Debug)]
pub struct LayerClipboard {
    document: Arc<Document>,
    layer_count: usize,
    retained_bytes: usize,
}

impl LayerClipboard {
    pub fn root_count(&self) -> usize {
        self.document.layers.len()
    }

    pub fn layer_count(&self) -> usize {
        self.layer_count
    }

    /// Conservative retained payload accounting, including metadata, masks,
    /// editable recipes and original pixels. Shared RGBA allocations and shared
    /// editable states are counted once within this clipboard.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Render the copied forest over transparency in its source canvas space.
    /// This is an interoperability image, not a replacement for the editable
    /// snapshot. Refuse oversized surfaces before allocating a compositor.
    pub fn preview(&self) -> Result<RgbaImage> {
        ensure!(
            u64::from(self.document.width) * u64::from(self.document.height) <= MAX_PREVIEW_PIXELS,
            "Layer clipboard image interoperability is limited to a 16 megapixel source canvas"
        );
        let preview = crate::raster::composite(&self.document);
        ensure!(
            preview.dimensions() == (self.document.width, self.document.height),
            "The copied layers could not be rendered safely"
        );
        Ok(preview)
    }
}

impl Editor {
    /// Copy complete selected trees without changing the source or its history.
    /// An ancestor selection subsumes selected descendants. Locked layers are
    /// readable, but an external live-mask dependency must be selected too.
    pub fn copy_layers(&self, ids: &[String]) -> Result<LayerClipboard> {
        ensure!(
            self.floating.is_none(),
            "Commit the floating selection first"
        );
        ensure!(
            crate::model::valid_dimensions(self.document.width, self.document.height),
            "Invalid source canvas dimensions"
        );
        inspect_forest(&self.document.layers)?;
        let roots = selected_roots(&self.document.layers, ids)
            .context("A selected layer no longer exists")?;
        ensure!(!roots.is_empty(), "Select at least one layer to copy");
        let selected: Vec<_> = roots
            .iter()
            .map(|id| self.document.find_layer(id).expect("checked selected root"))
            .collect();

        // Admission precedes every deep metadata clone. Pixel buffers remain
        // shared even when the clipboard outlives the entire source editor.
        let mut budget = PayloadBudget::default();
        budget.add(size_of::<LayerClipboard>() + size_of::<Document>() + 128)?;
        let mut included = HashSet::new();
        for layer in &selected {
            budget.layer(layer, &mut included)?;
        }
        for layer in &selected {
            require_internal_links(layer, &included)?;
            ensure!(
                valid_insert_tree(layer),
                "A copied layer has invalid content"
            );
        }
        let document = Document {
            width: self.document.width,
            height: self.document.height,
            name: "Layer clipboard".into(),
            background: [0; 4],
            layers: selected.into_iter().cloned().collect(),
            metadata: serde_json::json!({}),
        };
        validate_content(&document)?;
        Ok(LayerClipboard {
            document: Arc::new(document),
            layer_count: included.len(),
            retained_bytes: budget.bytes,
        })
    }

    /// Insert copied trees at the root, above the active top-level branch. All
    /// admission checks happen before ending a stroke or changing the document;
    /// the accepted paste is one undo step and preserves the pixel selection.
    pub fn paste_layers(&mut self, clipboard: &LayerClipboard) -> Result<Vec<String>> {
        ensure!(
            self.floating.is_none(),
            "Commit the floating selection first"
        );
        let (current_count, current_pixels) = inspect_forest(&self.document.layers)?;
        let (added_count, added_pixels) = inspect_forest(&clipboard.document.layers)?;
        ensure!(
            current_count.saturating_add(added_count) <= crate::model::MAX_LAYERS,
            "Pasting these layers would exceed the document layer limit"
        );
        ensure!(
            current_pixels.saturating_add(added_pixels) <= crate::model::MAX_PIXELS,
            "Pasting these layers would exceed the document image pixel limit"
        );
        let mut copies = clipboard.document.layers.clone();
        regenerate_forest_ids(&mut copies);
        let copied_ids: Vec<_> = copies.iter().map(|layer| layer.id.clone()).collect();
        let insertion = root_insertion(self);
        let mut proposed = self.document.clone();
        proposed
            .layers
            .splice(insertion..insertion, copies.iter().cloned());
        // This also checks live-mask surface/work limits in the destination
        // canvas, which may be much larger than the source canvas.
        validate_content(&proposed)?;
        drop(proposed);

        self.finish_stroke();
        let before = self.snapshot();
        let insertion = root_insertion(self);
        self.document.layers.splice(insertion..insertion, copies);
        self.active_layer = copied_ids.last().expect("nonempty copied forest").clone();
        self.commit(before);
        Ok(copied_ids)
    }
}

fn root_insertion(editor: &Editor) -> usize {
    tree_path(&editor.document.layers, &editor.active_layer)
        .and_then(|path| {
            editor
                .document
                .layers
                .iter()
                .position(|layer| layer.id == path[0])
        })
        .map_or(editor.document.layers.len(), |index| index + 1)
}

/// Check depth before using the editor's recursive selection/clone helpers.
/// A malformed public Document must not produce an unbounded recursive copy.
fn inspect_forest(layers: &[Layer]) -> Result<(usize, u64)> {
    fn visit<'a>(
        layers: &'a [Layer],
        depth: usize,
        ids: &mut HashSet<&'a str>,
        pixels: &mut u64,
    ) -> Result<()> {
        ensure!(
            depth <= MAX_DEPTH || layers.is_empty(),
            "Layer nesting exceeds 64 levels"
        );
        for layer in layers {
            ensure!(
                !layer.id.is_empty() && layer.id.len() <= 128 && ids.insert(&layer.id),
                "Layer identities are invalid or duplicated"
            );
            ensure!(ids.len() <= crate::model::MAX_LAYERS, "Too many layers");
            for image in [&layer.image, &layer.mask].into_iter().flatten() {
                ensure!(
                    crate::model::valid_dimensions(image.width(), image.height()),
                    "Layer image dimensions exceed supported limits"
                );
                *pixels =
                    pixels.saturating_add(u64::from(image.width()) * u64::from(image.height()));
                ensure!(
                    *pixels <= crate::model::MAX_PIXELS,
                    "Document image pixel limit exceeded"
                );
            }
            visit(&layer.children, depth + 1, ids, pixels)?;
        }
        Ok(())
    }
    let mut ids = HashSet::new();
    let mut pixels = 0;
    visit(layers, 1, &mut ids, &mut pixels)?;
    Ok((ids.len(), pixels))
}

fn require_internal_links(layer: &Layer, included: &HashSet<&str>) -> Result<()> {
    if let Some(value) = layer
        .metadata
        .get("maskSourceID")
        .filter(|value| !value.is_null())
    {
        let source = value
            .as_str()
            .context("Invalid live mask source identity")?;
        ensure!(
            included.contains(source),
            "Select the live mask source together with its dependent layers before copying"
        );
    }
    for child in &layer.children {
        require_internal_links(child, included)?;
    }
    Ok(())
}

fn validate_content(document: &Document) -> Result<()> {
    fn objects(layers: &[Layer]) -> Result<()> {
        for layer in layers {
            crate::objects::validate_live_object(layer)?;
            if let Some(state) = &layer.advanced {
                ensure!(
                    !layer.is_group()
                        && layer.image.as_ref().is_some_and(|image| {
                            image.dimensions() == state.result.dimensions()
                        }),
                    "Editable source requires matching cached layer pixels"
                );
            }
            objects(&layer.children)?;
        }
        Ok(())
    }
    objects(&document.layers)?;
    crate::advanced::validate_document_budget(document)?;
    let errors = crate::raster::validate(document);
    ensure!(
        errors.is_empty(),
        "Cannot paste these layers safely: {}",
        errors.join("; ")
    );
    Ok(())
}

#[derive(Default)]
struct PayloadBudget {
    bytes: usize,
    pixels: HashSet<usize>,
    editable: HashSet<usize>,
}

impl PayloadBudget {
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        ensure!(
            self.bytes <= MAX_CLIPBOARD_BYTES,
            "Layer clipboard exceeds the 256 MiB retained payload limit"
        );
        Ok(())
    }

    fn vector<T>(&mut self, values: &Vec<T>) -> Result<()> {
        self.add(values.capacity().saturating_mul(size_of::<T>()))
    }

    fn layer<'a>(&mut self, layer: &'a Layer, included: &mut HashSet<&'a str>) -> Result<()> {
        included.insert(layer.id.as_str());
        self.add(size_of::<Layer>())?;
        self.add(layer.id.capacity())?;
        self.add(layer.name.capacity())?;
        self.add(layer.blend_mode.capacity())?;
        self.metadata(&layer.metadata, 0)?;
        for image in [&layer.image, &layer.mask].into_iter().flatten() {
            if self.pixels.insert(image.allocation_id()) {
                self.add(
                    image
                        .as_raw()
                        .capacity()
                        .saturating_add(size_of::<RgbaImage>() + 32),
                )?;
            }
        }
        if let Some(state) = &layer.advanced
            && self.editable.insert(Arc::as_ptr(state) as usize)
        {
            self.advanced(state)?;
        }
        self.add(
            (layer.children.capacity() - layer.children.len()).saturating_mul(size_of::<Layer>()),
        )?;
        for child in &layer.children {
            self.layer(child, included)?;
        }
        Ok(())
    }

    fn metadata(&mut self, value: &Value, depth: usize) -> Result<()> {
        ensure!(
            depth <= MAX_METADATA_DEPTH,
            "Layer metadata nesting exceeds 128 levels"
        );
        match value {
            Value::String(value) => self.add(value.capacity())?,
            Value::Array(values) => {
                self.vector(values)?;
                for value in values {
                    self.metadata(value, depth + 1)?;
                }
            }
            Value::Object(values) => {
                // Conservative allowance for the map's nodes, keys and inline
                // Values. String payloads and nested allocations are separate.
                self.add(values.len().saturating_add(1).saturating_mul(256))?;
                for (key, value) in values {
                    self.add(key.capacity())?;
                    self.metadata(value, depth + 1)?;
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
        Ok(())
    }

    fn advanced(&mut self, state: &crate::advanced::LayerState) -> Result<()> {
        // The established counter includes source/result tiles, original RAW
        // bytes and all node/operation mask capacities. Charge its shared tiles
        // conservatively and cover the remaining typed recipe allocations here.
        self.add(size_of::<crate::advanced::LayerState>() + 32)?;
        self.add(state.retained_bytes())?;
        for image in [&state.source, &state.result] {
            let (width, height) = image.dimensions();
            let tiles = (width.div_ceil(image.tile_size()) as usize)
                .saturating_mul(height.div_ceil(image.tile_size()) as usize);
            self.add(tiles.saturating_mul(64).saturating_add(128))?;
        }
        if let Some(raw) = &state.raw_bytes {
            self.add(raw.capacity().saturating_sub(raw.len()).saturating_add(64))?;
        }
        let recipe = &state.recipe;
        for value in [
            Some(&recipe.source_id),
            Some(&recipe.source_name),
            recipe.linked_path.as_ref(),
            recipe.raw_extension.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            self.add(value.capacity())?;
        }
        if let Some(path) = &recipe.vector {
            self.vector(&path.subpaths)?;
            for subpath in &path.subpaths {
                self.vector(&subpath.anchors)?;
            }
        }
        self.vector(&recipe.nodes)?;
        for node in &recipe.nodes {
            self.add(node.id.capacity())?;
            self.add(node.name.capacity())?;
            use crate::advanced_ops::AdvancedOperation;
            match &node.operation {
                AdvancedOperation::Filter(crate::filters::Filter::Curves { points }) => {
                    self.vector(points)?
                }
                AdvancedOperation::CameraRaw(settings) => {
                    for points in [
                        &settings.curve.rgb,
                        &settings.curve.red,
                        &settings.curve.green,
                        &settings.curve.blue,
                    ] {
                        self.vector(points)?;
                    }
                    for values in [
                        &settings.mixer.hue,
                        &settings.mixer.saturation,
                        &settings.mixer.luminance,
                    ] {
                        self.vector(values)?;
                    }
                    self.vector(&settings.mixer.points)?;
                    self.vector(&settings.geometry.guides)?;
                }
                AdvancedOperation::Warp(mesh) => {
                    self.vector(&mesh.points)?;
                    self.vector(&mesh.pins)?;
                }
                AdvancedOperation::Filter(_)
                | AdvancedOperation::Denoise { .. }
                | AdvancedOperation::BlendIf(_)
                | AdvancedOperation::FrequencySeparation(_)
                | AdvancedOperation::DodgeBurn(_)
                | AdvancedOperation::ContentAwareReplace(_) => {}
            }
        }
        Ok(())
    }
}
