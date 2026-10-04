//! Embedded original sources and editable recipes. Cached 8-bit layers remain
//! readable by the legacy renderer; originals and 16-bit results are separate.
use crate::{
    advanced_ops::{AdvancedOperation, BlendIf, FilterNode},
    precision::{TiledImage16, WorkingSpace},
    vector_path::VectorPath,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const MAX_ADVANCED_PIXELS: u64 = 16_777_216;
pub const RASTER_BLEND_IF_KEY: &str = "rustBlendIf";
pub const MAX_DOCUMENT_BYTES: usize = 768 * 1024 * 1024;
const MAX_RECIPE_BYTES: u64 = 128 * 1024 * 1024;

/// Blend If survives rasterizing an editable source as typed layer metadata.
/// Metadata takes precedence while the editable representation is detached.
pub fn layer_blend_if(layer: &crate::model::Layer) -> Result<Option<BlendIf>> {
    if let Some(value) = layer.metadata.get(RASTER_BLEND_IF_KEY) {
        ensure!(!value.is_null(), "rustBlendIf must be an object");
        let settings: BlendIf =
            serde_json::from_value(value.clone()).context("Invalid rustBlendIf metadata")?;
        crate::advanced_ops::validate_blend_if(&settings)?;
        return Ok(Some(settings));
    }
    let settings = layer
        .advanced
        .as_ref()
        .and_then(|state| state.recipe.blend_if.clone());
    if let Some(settings) = &settings {
        crate::advanced_ops::validate_blend_if(settings)?;
    }
    Ok(settings)
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum Component {
    #[default]
    Image,
    LowFrequency {
        sigma: f32,
    },
    HighFrequency {
        sigma: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerRecipe {
    pub version: u32,
    pub source_id: String,
    pub source_name: String,
    pub linked_path: Option<String>,
    pub raw_extension: Option<String>,
    pub raw_settings: Option<crate::raw_import::DevelopSettings>,
    pub working_space: WorkingSpace,
    pub nodes: Vec<FilterNode>,
    pub blend_if: Option<BlendIf>,
    pub vector: Option<VectorPath>,
    pub vector_is_mask: bool,
    pub vector_fill: [u8; 4],
    pub vector_stroke: Option<crate::vector_path::StrokeStyle>,
    #[serde(default)]
    pub component: Component,
}

#[derive(Clone, Debug)]
pub struct LayerState {
    pub source: Arc<TiledImage16>,
    pub result: Arc<TiledImage16>,
    pub recipe: LayerRecipe,
    pub raw_bytes: Option<Arc<Vec<u8>>>,
}

impl LayerState {
    pub fn retained_bytes(&self) -> usize {
        self.source
            .memory_bytes()
            .saturating_add(self.result.memory_bytes())
            .saturating_add(self.raw_bytes.as_ref().map_or(0, |b| b.len()))
            .saturating_add(
                self.recipe
                    .nodes
                    .iter()
                    .map(FilterNode::owned_bytes)
                    .sum::<usize>(),
            )
    }
    pub fn from_image(image: &image::RgbaImage, name: &str) -> Result<Self> {
        let source = Arc::new(TiledImage16::from_rgba8(image)?);
        Ok(Self::from_source(source, name))
    }

    /// Retain a high-bit-depth import without first passing through the 8-bit
    /// display cache. The result and immutable source initially share tiles.
    pub fn from_rgba16(image: &crate::precision::Rgba16Image, name: &str) -> Result<Self> {
        let source = Arc::new(TiledImage16::from_rgba16(image)?);
        Ok(Self::from_source(source, name))
    }

    fn from_source(source: Arc<TiledImage16>, name: &str) -> Self {
        Self {
            result: source.clone(),
            source,
            raw_bytes: None,
            recipe: LayerRecipe {
                version: 1,
                source_id: uuid::Uuid::new_v4().to_string(),
                source_name: name.into(),
                linked_path: None,
                raw_extension: None,
                raw_settings: None,
                working_space: WorkingSpace::Srgb,
                nodes: vec![],
                blend_if: None,
                vector: None,
                vector_is_mask: false,
                vector_fill: [0, 0, 0, 255],
                vector_stroke: None,
                component: Component::Image,
            },
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.retained_bytes() <= MAX_DOCUMENT_BYTES,
            "Editable layer exceeds 768 MiB memory budget"
        );
        ensure!(
            self.recipe.version == 1,
            "Unsupported editable-source version"
        );
        uuid::Uuid::parse_str(&self.recipe.source_id).context("Invalid source identity")?;
        ensure!(
            self.recipe.source_name.len() <= 16_384,
            "Source name too long"
        );
        ensure!(
            self.recipe
                .linked_path
                .as_ref()
                .is_none_or(|s| s.len() <= 16_384),
            "Linked path too long"
        );
        let (w, h) = self.source.dimensions();
        ensure!(
            crate::model::valid_dimensions(w, h)
                && u64::from(w) * u64::from(h) <= MAX_ADVANCED_PIXELS,
            "Editable source exceeds 16 million pixels"
        );
        ensure!(
            self.result.dimensions() == (w, h),
            "Editable source cache dimensions differ"
        );
        if let Some(settings) = &self.recipe.raw_settings {
            settings.validate()?;
        }
        ensure!(
            self.recipe
                .raw_extension
                .as_ref()
                .is_none_or(|s| !s.is_empty()
                    && s.len() <= 10
                    && s.bytes().all(|b| b.is_ascii_alphanumeric())),
            "Invalid RAW source extension"
        );
        ensure!(
            self.raw_bytes.is_some() == self.recipe.raw_extension.is_some()
                && self.raw_bytes.is_some() == self.recipe.raw_settings.is_some(),
            "Incomplete embedded RAW source"
        );
        if let Some(raw) = &self.raw_bytes {
            ensure!(
                raw.len() <= 512 * 1024 * 1024,
                "Embedded RAW exceeds 512 MiB"
            );
        }
        if let Some(blend) = &self.recipe.blend_if {
            crate::advanced_ops::validate_blend_if(blend)?;
        }
        if let Some(path) = &self.recipe.vector {
            path.validate()?;
        }
        ensure!(
            self.recipe.vector.is_some()
                || (!self.recipe.vector_is_mask && self.recipe.vector_stroke.is_none()),
            "Vector mask or stroke metadata requires a retained path"
        );
        if let Some(stroke) = self.recipe.vector_stroke {
            ensure!(
                stroke.width.is_finite() && stroke.width > 0. && stroke.width <= 100_000.,
                "Invalid retained vector stroke width"
            );
        }
        ensure!(
            self.recipe.nodes.len() <= 128,
            "At most 128 editable filters per layer"
        );
        crate::advanced_ops::validate_stack_dimensions(w, h, &self.recipe.nodes)?;
        match self.recipe.component {
            Component::Image => {}
            Component::LowFrequency { sigma } | Component::HighFrequency { sigma } => {
                ensure!(
                    sigma.is_finite() && (0.1..=128.).contains(&sigma),
                    "Invalid frequency radius"
                );
                ensure!(
                    self.recipe.working_space == WorkingSpace::Srgb,
                    "Frequency layers require encoded sRGB working space for Linear Light reconstruction"
                );
            }
        }
        Ok(())
    }

    pub fn evaluate(&self, cancel: &AtomicBool) -> Result<Self> {
        self.validate()?;
        ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
        let mut result = (*self.source).clone();
        if let Some(path) = &self.recipe.vector {
            if !self.recipe.vector_is_mask {
                let (w, h) = self.source.dimensions();
                let pixels = crate::vector_path::rasterize_rgba(
                    path,
                    w,
                    h,
                    Some(self.recipe.vector_fill),
                    self.recipe.vector_stroke,
                    0.25,
                    || cancel.load(Ordering::Relaxed),
                )?;
                result = TiledImage16::from_rgba8(&pixels)?;
                result.convert_working_space(self.recipe.working_space)?;
            }
        }
        // Filters, retouching and deformation keep high precision. The legacy
        // Camera Raw operation is the remaining byte-based compatibility node.
        for node in &self.recipe.nodes {
            ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
            if !node.enabled || node.opacity == 0. {
                continue;
            }
            let computed = match &node.operation {
                AdvancedOperation::Filter(filter) => {
                    result.filtered_with_cancel(filter, || cancel.load(Ordering::Relaxed))?
                }
                _ => {
                    if let Some(full) =
                        crate::advanced16::evaluate(&result, &node.operation, cancel)?
                    {
                        full
                    } else {
                        let mut full = node.clone();
                        full.opacity = 1.;
                        full.soft_mask = None;
                        let pixels = crate::advanced_ops::evaluate_cancellable(
                            &result.to_rgba8_in(WorkingSpace::Srgb)?,
                            &[full],
                            cancel,
                        )?;
                        let mut converted = TiledImage16::from_rgba8(&pixels)?;
                        converted.convert_working_space(self.recipe.working_space)?;
                        converted
                    }
                }
            };
            let mut blended = result.to_rgba16();
            let filtered = computed.to_rgba16();
            for (index, (dst, src)) in blended.pixels_mut().zip(filtered.pixels()).enumerate() {
                if index % blended_width(self.source.dimensions()) == 0 {
                    ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
                }
                let amount = node.opacity
                    * node
                        .soft_mask
                        .as_ref()
                        .map_or(1., |mask| mask.data[index] as f32 / 255.);
                if dst[3] == src[3] {
                    // Equal alpha cancels from the straight-colour blend.
                    // Avoid premultiplying and dividing by it: that round trip
                    // can move an exact half-channel value just below .5 and
                    // make identical colour edits depend on source opacity.
                    // This also retains hidden RGB when both alphas are zero.
                    let t = f64::from(amount);
                    for c in 0..3 {
                        dst[c] = (f64::from(dst[c]) * (1. - t) + f64::from(src[c]) * t)
                            .round()
                            .clamp(0., 65535.) as u16;
                    }
                    continue;
                }
                let a0 = dst[3] as f64 / 65535.;
                let a1 = src[3] as f64 / 65535.;
                let t = amount as f64;
                let alpha = a0 * (1. - t) + a1 * t;
                for c in 0..3 {
                    dst[c] = if alpha > 0. {
                        ((dst[c] as f64 * a0 * (1. - t) + src[c] as f64 * a1 * t) / alpha)
                            .round()
                            .clamp(0., 65535.) as u16
                    } else {
                        // Fully transparent pixels still carry straight RGB.
                        // Preserve/interpolate those hidden channels so a
                        // zero-coverage node is exact and later alpha or mask
                        // edits cannot reveal an artificial black fringe.
                        (dst[c] as f64 * (1. - t) + src[c] as f64 * t)
                            .round()
                            .clamp(0., 65535.) as u16
                    };
                }
                dst[3] = (alpha * 65535.).round() as u16;
            }
            result = TiledImage16::from_rgba16_in(&blended, self.recipe.working_space)?;
        }
        match self.recipe.component {
            Component::Image => {}
            Component::LowFrequency { sigma } => {
                result = result.filtered_with_cancel(
                    &crate::filters::Filter::GaussianBlur { sigma },
                    || cancel.load(Ordering::Relaxed),
                )?;
            }
            Component::HighFrequency { sigma } => {
                let low = result.filtered_with_cancel(
                    &crate::filters::Filter::GaussianBlur { sigma },
                    || cancel.load(Ordering::Relaxed),
                )?;
                let mut high = result.to_rgba16();
                for (x, y, pixel) in high.enumerate_pixels_mut() {
                    if x == 0 {
                        ensure!(
                            !cancel.load(Ordering::Relaxed),
                            "Frequency separation cancelled"
                        );
                    }
                    ensure!(
                        pixel[3] == 65535,
                        "Frequency layers require an opaque source"
                    );
                    let low = low.get_pixel(x, y).0;
                    for c in 0..3 {
                        pixel[c] =
                            ((pixel[c] as i32 - low[c] as i32 + 65535) as f32 / 2.).round() as u16;
                    }
                }
                result = TiledImage16::from_rgba16_in(&high, self.recipe.working_space)?;
            }
        }
        let mut output = self.clone();
        output.result = Arc::new(result);
        ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
        Ok(output)
    }

    pub fn proxy(&self) -> Result<image::RgbaImage> {
        self.result.to_rgba8_in(WorkingSpace::Srgb)
    }

    /// Assets are inside the staging package and fsynced before publication.
    pub fn save_assets(&self, images: &Path, id: &str) -> Result<()> {
        self.validate()?;
        for (suffix, pixels) in [("source", &self.source), ("result", &self.result)] {
            let path = images.join(format!("{id}.{suffix}16.png"));
            image::DynamicImage::ImageRgba16(pixels.to_rgba16())
                .save_with_format(&path, image::ImageFormat::Png)?;
            crate::durable_fs::sync_path(path)?;
        }
        let recipe = serde_json::to_vec(&self.recipe)?;
        ensure!(
            recipe.len() as u64 <= MAX_RECIPE_BYTES,
            "Editable recipe exceeds 128 MiB"
        );
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(images.join(format!("{id}.editable.json.z")))?;
        let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::fast());
        encoder.write_all(&recipe)?;
        encoder.finish()?.sync_all()?;
        if let Some(raw) = &self.raw_bytes {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(images.join(format!("{id}.original.raw")))?;
            file.write_all(raw)?;
            file.sync_all()?;
        }
        Ok(())
    }

    pub fn load_assets(images: &Path, id: &str) -> Result<Self> {
        Self::load_assets_bounded(images, id, MAX_DOCUMENT_BYTES)
    }

    pub fn load_assets_bounded(images: &Path, id: &str, budget: usize) -> Result<Self> {
        uuid::Uuid::parse_str(id).context("Invalid editable asset identity")?;
        ensure!(
            std::fs::symlink_metadata(images)?.file_type().is_dir(),
            "Editable asset folder must be a regular directory"
        );
        fn bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
            ensure!(
                std::fs::symlink_metadata(path)?.file_type().is_file(),
                "Editable assets must be regular files"
            );
            let file = std::fs::File::open(path)?;
            ensure!(
                file.metadata()?.len() <= max,
                "Editable asset exceeds size limit"
            );
            let mut bytes = vec![];
            file.take(max + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= max,
                "Editable asset exceeds size limit"
            );
            Ok(bytes)
        }
        let compressed = bounded(
            &images.join(format!("{id}.editable.json.z")),
            MAX_RECIPE_BYTES,
        )?;
        let mut decoded = vec![];
        flate2::read::ZlibDecoder::new(&compressed[..])
            .take(MAX_RECIPE_BYTES + 1)
            .read_to_end(&mut decoded)?;
        ensure!(
            decoded.len() as u64 <= MAX_RECIPE_BYTES,
            "Editable recipe expands beyond limit"
        );
        let recipe: LayerRecipe = serde_json::from_slice(&decoded)?;
        ensure!(
            recipe.version == 1 && recipe.nodes.len() <= 128,
            "Invalid editable recipe version or node count"
        );
        let mut retained = recipe
            .nodes
            .iter()
            .map(FilterNode::owned_bytes)
            .sum::<usize>();
        ensure!(
            retained <= budget,
            "Editable document exceeds memory budget"
        );
        let mut load16 = |suffix: &str| -> Result<Arc<TiledImage16>> {
            let bytes = bounded(
                &images.join(format!("{id}.{suffix}16.png")),
                256 * 1024 * 1024,
            )?;
            let dimensions = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes),
                image::ImageFormat::Png,
            )
            .into_dimensions()?;
            ensure!(
                crate::model::valid_dimensions(dimensions.0, dimensions.1)
                    && u64::from(dimensions.0) * u64::from(dimensions.1) <= MAX_ADVANCED_PIXELS,
                "Editable master exceeds pixel limit"
            );
            retained = retained.saturating_add(dimensions.0 as usize * dimensions.1 as usize * 8);
            ensure!(
                retained <= budget,
                "Editable document exceeds memory budget"
            );
            let mut reader = image::ImageReader::with_format(
                std::io::Cursor::new(bytes),
                image::ImageFormat::Png,
            );
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(30_000);
            limits.max_image_height = Some(30_000);
            limits.max_alloc = Some(MAX_ADVANCED_PIXELS * 8);
            reader.limits(limits);
            let pixels = reader.decode()?;
            ensure!(
                pixels.color() == image::ColorType::Rgba16,
                "Editable master must be RGBA16"
            );
            Ok(Arc::new(TiledImage16::from_rgba16_in(
                &pixels.to_rgba16(),
                recipe.working_space,
            )?))
        };
        let source = load16("source")?;
        let result = load16("result")?;
        let raw_bytes = if recipe.raw_extension.is_some() {
            Some(Arc::new(bounded(
                &images.join(format!("{id}.original.raw")),
                (512 * 1024 * 1024).min(budget.saturating_sub(retained) as u64),
            )?))
        } else {
            None
        };
        let state = Self {
            source,
            result,
            recipe,
            raw_bytes,
        };
        state.validate()?;
        ensure!(
            state.retained_bytes() <= budget,
            "Editable document exceeds memory budget"
        );
        Ok(state)
    }
}

fn blended_width(dimensions: (u32, u32)) -> usize {
    dimensions.0 as usize
}

/// Bound committed editable assets, including operation masks and embedded RAW.
/// Shared source tiles are conservatively charged per layer; this also keeps
/// a saved package within the same budget after sharing is reconstructed.
pub fn validate_document_budget(document: &crate::model::Document) -> Result<()> {
    fn visit(layers: &[crate::model::Layer], total: &mut usize) -> Result<()> {
        for layer in layers {
            if let Some(state) = &layer.advanced {
                state.validate()?;
                *total = total.saturating_add(state.retained_bytes());
                ensure!(
                    *total <= MAX_DOCUMENT_BYTES,
                    "Editable document exceeds 768 MiB memory budget"
                );
            }
            visit(&layer.children, total)?;
        }
        Ok(())
    }
    visit(&document.layers, &mut 0)
}
