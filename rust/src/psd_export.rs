//! Bounded, converted 8-bit layered PSD export. `.omuse` remains the editable
//! master: live text, vectors and recipes are exported as their raster appearance.
//! This adapter deliberately does not promise Photoshop round-trip fidelity.
use crate::{
    effects::LayerEffects,
    model::{Document, Layer},
    vector_svg::{self, PreparedExport},
};
use anyhow::{Context, Result, bail, ensure};
use image::{Rgba, RgbaImage};
use photocraft_psd::{BlendMode, GroupSpec, LayerSpec, MaskSpec, PixelData, PsdBuilder, Rect};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
const MAX_WORKING_BYTES: u64 = 768 * 1024 * 1024;
const MAX_RECORDS: usize = 1024;

#[derive(Clone, Debug, Default)]
pub struct ExportReport {
    pub layer_count: usize,
    pub group_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct EncodedExport {
    pub bytes: Vec<u8>,
    pub report: ExportReport,
}

#[derive(Debug)]
pub struct PreparedPsdExport {
    /// Recheck the editor revision and cancellation immediately before publish.
    pub prepared: PreparedExport,
    pub report: ExportReport,
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "PSD export cancelled");
    Ok(())
}

/// PSD's RGB merged preview stores colour against white; its individual layer
/// channels remain straight RGBA. ImageMagick and psd-tools remove this matte
/// when reading the preview. Work in encoded sRGB, matching those readers.
fn matte_merged_preview(image: &mut RgbaImage, cancel: &AtomicBool) -> Result<()> {
    for (index, pixel) in image.pixels_mut().enumerate() {
        if index % 16_384 == 0 {
            check(cancel)?;
        }
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            // At zero alpha readers leave hidden RGB alone. Keeping it also
            // avoids changing irrelevant samples; opaque colour is exact.
            continue;
        }
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
    }
    check(cancel)
}

/// Validate all semantics and estimate peak adapter allocations before copying
/// any image. Encoding may finish a bounded codec call before noticing cancel.
pub fn encode_document(doc: &Document, cancel: &AtomicBool) -> Result<EncodedExport> {
    check(cancel)?;
    ensure!(
        crate::model::valid_dimensions(doc.width, doc.height)
            && doc.width <= 30_000
            && doc.height <= 30_000,
        "PSD export needs a valid canvas no larger than 30,000 pixels per side"
    );
    let mut budget = Budget::default();
    budget.add_metadata(&doc.metadata, 0)?;
    budget.add_surface(doc.width, doc.height)?; // merged preview
    if doc.background[3] != 0 {
        budget.add_surface(doc.width, doc.height)?;
        budget.records += 1;
    }
    preflight(&doc.layers, 0, &mut budget, cancel)?;
    ensure!(
        budget.records <= MAX_RECORDS,
        "PSD export exceeds 1,024 layer records"
    );
    ensure!(
        budget.records > 0,
        "PSD export needs at least one layer or a visible canvas background"
    );
    let issues = crate::raster::validate(doc);
    ensure!(
        issues.is_empty(),
        "Cannot export this document: {}",
        issues.join("; ")
    );

    let mut report = ExportReport::default();
    report.warnings.push("This is a converted 8-bit layered PSD with untagged sRGB pixels. Keep the .omuse original for full precision, live text, vectors, recipes and original metadata.".into());
    let mut converted = doc.clone();
    converted.layers = convert_layers(&doc.layers, &mut report, cancel)?;
    if doc.background[3] != 0 {
        let mut background = Layer::paint("Canvas background", doc.width, doc.height);
        background.image =
            Some(RgbaImage::from_pixel(doc.width, doc.height, Rgba(doc.background)).into());
        converted.layers.insert(0, background);
        report.layer_count += 1;
        report
            .warnings
            .push("The canvas background was added as the bottom pixel layer.".into());
    }
    converted.background = [0; 4];
    check(cancel)?;
    let mut composite = crate::raster::composite(&converted);
    ensure!(
        composite.dimensions() == (doc.width, doc.height),
        "Cannot render PSD merged preview"
    );
    check(cancel)?;
    let dpi = doc
        .metadata
        .get("resolution")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(72.);
    ensure!(
        dpi.is_finite() && (1. ..=9600.).contains(&dpi),
        "Invalid PSD resolution"
    );
    let mut builder = PsdBuilder::new(doc.width, doc.height).resolution(dpi);
    append_layers(&mut builder, &converted.layers, cancel)?;
    matte_merged_preview(&mut composite, cancel)?;
    builder.composite(PixelData::Rgba8(composite.into_raw()));
    check(cancel)?;
    let file = builder.build().context("Cannot encode Photoshop layers")?;
    drop(builder);
    drop(converted);
    check(cancel)?;
    let bytes = file
        .to_bytes()
        .context("Cannot write Photoshop structure")?;
    ensure!(bytes.len() <= MAX_FILE_BYTES, "PSD export exceeds 256 MiB");
    check(cancel)?;
    Ok(EncodedExport { bytes, report })
}

/// Prepare a synced sibling file; no destination is visible yet. The caller
/// must recheck its editor guard/cancellation before publishing on the UI thread.
pub fn prepare_export(
    path: &Path,
    doc: &Document,
    cancel: &AtomicBool,
) -> Result<PreparedPsdExport> {
    ensure!(
        path.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("psd")),
        "Choose a .psd filename for layered Photoshop export"
    );
    let encoded = encode_document(doc, cancel)?;
    check(cancel)?;
    let prepared = vector_svg::prepare_bytes_export_bounded(path, &encoded.bytes, MAX_FILE_BYTES)?;
    check(cancel)?;
    Ok(PreparedPsdExport {
        prepared,
        report: encoded.report,
    })
}

/// CLI convenience for creating a new file. Never replaces an existing path.
pub fn export(doc: &Document, path: &Path) -> Result<ExportReport> {
    let ready = prepare_export(path, doc, &AtomicBool::new(false))?;
    ready.prepared.publish()?.finish()?;
    Ok(ready.report)
}

#[derive(Default)]
struct Budget {
    pixels: u64,
    records: usize,
    metadata_bytes: usize,
}
impl Budget {
    fn add_metadata(&mut self, value: &serde_json::Value, depth: usize) -> Result<()> {
        ensure!(depth <= 64, "PSD source metadata nesting exceeds limits");
        self.metadata_bytes = self.metadata_bytes.saturating_add(64);
        match value {
            serde_json::Value::String(s) => {
                self.metadata_bytes = self.metadata_bytes.saturating_add(s.len())
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    self.add_metadata(value, depth + 1)?;
                }
            }
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    self.metadata_bytes = self.metadata_bytes.saturating_add(key.len());
                    self.add_metadata(value, depth + 1)?;
                }
            }
            _ => {}
        }
        ensure!(
            self.metadata_bytes <= 8 * 1024 * 1024,
            "PSD source metadata exceeds the 8 MiB export budget"
        );
        Ok(())
    }
    fn add_surface(&mut self, width: u32, height: u32) -> Result<()> {
        ensure!(
            crate::model::valid_dimensions(width, height) && width <= 30_000 && height <= 30_000,
            "PSD layer or effect dimensions exceed limits"
        );
        self.pixels = self
            .pixels
            .checked_add(u64::from(width) * u64::from(height))
            .context("PSD allocation estimate overflow")?;
        // Includes source copies, converted surfaces, planar/compressed channel
        // buffers and the writer's nested section copies. This is additional
        // memory, excluding the already-open document. Metadata is bounded below.
        ensure!(
            self.pixels
                .saturating_mul(24)
                .saturating_add(8 * 1024 * 1024)
                <= MAX_WORKING_BYTES,
            "PSD export exceeds its 768 MiB temporary-memory budget; reduce layer dimensions or export a flattened PNG/TIFF"
        );
        Ok(())
    }
}

fn preflight(
    layers: &[Layer],
    depth: usize,
    budget: &mut Budget,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(
        depth < 64 || layers.is_empty(),
        "PSD layer nesting exceeds 64 levels"
    );
    for layer in layers {
        check(cancel)?;
        budget.add_metadata(&layer.metadata, 0)?;
        budget.records += if layer.is_group() { 2 } else { 1 };
        ensure!(
            budget.records <= MAX_RECORDS,
            "PSD export exceeds 1,024 layer records"
        );
        ensure!(
            layer.name.chars().count() <= 512,
            "PSD layer names are limited to 512 characters"
        );
        ensure!(
            layer.opacity.is_finite() && (0. ..=1.).contains(&layer.opacity),
            "Invalid opacity on {}",
            layer.name
        );
        integer_coordinate(layer.offset_x)?;
        integer_coordinate(layer.offset_y)?;
        ensure!(
            layer.scale_x == 1. && layer.scale_y == 1. && layer.rotation == 0.,
            "{}: PSD export does not yet preserve scaled, flipped or rotated layer transforms. Save an .omuse master, then use Flatten image in a separate copy before exporting",
            layer.name
        );
        ensure!(
            !present(layer, "adjustment")
                && !present(layer, "maskSourceID")
                && layer
                    .metadata
                    .get("psdClipping")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true),
            "{}: PSD export does not yet preserve adjustment or clipping layers. Save an .omuse master, then use Flatten image in a separate copy before exporting",
            layer.name
        );
        ensure!(
            crate::advanced::layer_blend_if(layer)?.is_none(),
            "{}: PSD export does not yet preserve Blend If. Save an .omuse master, then use Flatten image in a separate copy before exporting",
            layer.name
        );
        crate::effects::validate_mask_metadata(&layer.metadata)?;
        if layer.is_group() {
            ensure!(
                layer.opacity == 1.
                    && layer.mask.is_none()
                    && !present(layer, "effects")
                    && blend(&layer.blend_mode)? == BlendMode::Normal
                    && layer.offset_x == 0.
                    && layer.offset_y == 0.
                    && layer.image.is_none()
                    && layer.advanced.is_none()
                    && layer.vector_scene.is_none(),
                "{}: PSD export supports unmasked pass-through groups at full opacity. Save an .omuse master, then use Flatten image in a separate copy before exporting",
                layer.name
            );
            preflight(&layer.children, depth + 1, budget, cancel)?;
            continue;
        }
        blend(&layer.blend_mode)?;
        let image = layer
            .image
            .as_ref()
            .with_context(|| format!("{} has no raster appearance to export", layer.name))?;
        budget.add_surface(image.width(), image.height())?;
        let mask_rect = mask_geometry(layer)?;
        if let Some(mask) = &layer.mask {
            budget.add_surface(mask.width(), mask.height())?;
            ensure!(
                mask.pixels()
                    .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255),
                "{}: PSD export requires an opaque grayscale pixel mask",
                layer.name
            );
        }
        if let Some(fx) = effects(layer)? {
            if let Some(rect) = mask_rect {
                ensure!(
                    rect.left == layer.offset_x as i32
                        && rect.top == layer.offset_y as i32
                        && rect.right - rect.left == image.width() as i32
                        && rect.bottom - rect.top == image.height() as i32,
                    "{}: PSD export cannot preserve these layer effects with this mask placement. Save an .omuse master, then use Flatten image in a separate copy before exporting",
                    layer.name
                );
            }
            let pad = effect_pad(&fx);
            budget.add_surface(
                image
                    .width()
                    .checked_add(2 * pad)
                    .context("PSD effect width overflow")?,
                image
                    .height()
                    .checked_add(2 * pad)
                    .context("PSD effect height overflow")?,
            )?;
        }
    }
    Ok(())
}

fn present(layer: &Layer, key: &str) -> bool {
    layer.metadata.get(key).is_some_and(|v| !v.is_null())
}

fn integer_coordinate(value: f32) -> Result<i32> {
    ensure!(
        value.is_finite() && value.fract() == 0. && value.abs() <= 1_000_000.,
        "PSD export requires integer-aligned layer and mask positions within ±1,000,000 pixels. Save an .omuse master, then use Flatten image in a separate copy before exporting"
    );
    Ok(value as i32)
}

fn mask_geometry(layer: &Layer) -> Result<Option<Rect>> {
    let Some(mask) = &layer.mask else {
        return Ok(None);
    };
    let image = layer
        .image
        .as_ref()
        .context("Masked PSD layer has no image")?;
    ensure!(
        layer
            .metadata
            .get("maskLinked")
            .and_then(serde_json::Value::as_bool)
            != Some(false),
        "{}: PSD export cannot preserve an unlinked pixel mask. Save an .omuse master, then use Flatten image in a separate copy before exporting",
        layer.name
    );
    let (mut left, mut top) = (
        integer_coordinate(layer.offset_x)?,
        integer_coordinate(layer.offset_y)?,
    );
    if let Some(p) = layer.metadata.get("maskPlacement").filter(|v| !v.is_null()) {
        let pair = |key: &str| -> Result<[f32; 2]> {
            let a = p
                .get(key)
                .and_then(serde_json::Value::as_array)
                .context("Invalid mask placement")?;
            ensure!(a.len() == 2, "Invalid mask placement");
            Ok([
                a[0].as_f64().context("Invalid mask coordinate")? as f32,
                a[1].as_f64().context("Invalid mask coordinate")? as f32,
            ])
        };
        let origin = pair("origin")?;
        let size = pair("size")?;
        left = integer_coordinate(origin[0])?;
        top = integer_coordinate(origin[1])?;
        ensure!(
            size == [mask.width() as f32, mask.height() as f32]
                && p.get("rotation")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.)
                    == 0.
                && p.get("flipX").and_then(serde_json::Value::as_bool) != Some(true)
                && p.get("flipY").and_then(serde_json::Value::as_bool) != Some(true),
            "{}: PSD export does not yet preserve scaled or rotated mask placement. Save an .omuse master, then use Flatten image in a separate copy before exporting",
            layer.name
        );
    } else {
        ensure!(
            mask.dimensions() == image.dimensions(),
            "{}: PSD export cannot preserve this resized mask. Save an .omuse master, then use Flatten image in a separate copy before exporting",
            layer.name
        );
    }
    Ok(Some(Rect::from_xywh(
        left,
        top,
        mask.width(),
        mask.height(),
    )))
}

fn effects(layer: &Layer) -> Result<Option<LayerEffects>> {
    layer
        .metadata
        .get("effects")
        .filter(|v| !v.is_null())
        .map(LayerEffects::parse)
        .transpose()
}

fn effect_pad(fx: &LayerEffects) -> u32 {
    let mut margin = 0f32;
    if let Some(x) = &fx.stroke
        && x.enabled != Some(false)
        && !x.inside
    {
        margin = margin.max(x.size);
    }
    if let Some(x) = &fx.shadow
        && x.enabled != Some(false)
    {
        margin = margin.max(x.distance + x.blur * 3.);
    }
    if let Some(x) = &fx.outer_glow
        && x.enabled != Some(false)
    {
        margin = margin.max(x.size * 3.);
    }
    margin.ceil() as u32 + 2
}

fn warn(report: &mut ExportReport, layer: &Layer, message: &str) {
    // Preflight bounds the document to 1,024 records and names to 512
    // characters. Keep every conversion note within that bounded document;
    // silently dropping later notes could hide an important conversion.
    report.warnings.push(format!("{}: {message}", layer.name));
}

fn convert_layers(
    layers: &[Layer],
    report: &mut ExportReport,
    cancel: &AtomicBool,
) -> Result<Vec<Layer>> {
    let mut converted = Vec::with_capacity(layers.len());
    for layer in layers {
        check(cancel)?;
        let mut output = layer.clone();
        if layer.is_group() {
            report.group_count += 1;
            output.children = convert_layers(&layer.children, report, cancel)?;
        } else {
            report.layer_count += 1;
            if present(layer, "text") {
                warn(
                    report,
                    layer,
                    "Text was converted to pixels; characters and font settings are not live in the PSD.",
                );
            }
            if layer.vector_scene.is_some() || present(layer, "shape") {
                warn(
                    report,
                    layer,
                    "Vector artwork was converted to pixels; nodes are not editable in the PSD.",
                );
            }
            if layer.advanced.is_some() {
                warn(
                    report,
                    layer,
                    "The current 8-bit appearance was exported; full-precision sources and editable filter recipes remain in .omuse.",
                );
            }
            if let Some(fx) = effects(layer)? {
                let mask = if layer
                    .metadata
                    .get("maskEnabled")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
                {
                    None
                } else {
                    layer.mask.as_deref()
                };
                let (image, pad) = crate::effects::render(
                    layer.image.as_deref().context("Missing layer pixels")?,
                    mask,
                    &fx,
                )?;
                output.image = Some(image.into());
                output.offset_x -= pad as f32;
                output.offset_y -= pad as f32;
                output.mask = None;
                if let Some(metadata) = output.metadata.as_object_mut() {
                    metadata.remove("effects");
                    metadata.remove("maskPlacement");
                }
                warn(
                    report,
                    layer,
                    "Layer effects and their pixel mask appearance were rendered into pixels; style settings are not live in the PSD.",
                );
            }
            output.advanced = None;
            output.vector_scene = None;
            let opacity = (layer.opacity * 255.).round() / 255.;
            if opacity != layer.opacity {
                warn(
                    report,
                    layer,
                    "Opacity was rounded to the nearest Photoshop 8-bit opacity value.",
                );
            }
            output.opacity = opacity;
        }
        if layer.locked {
            warn(
                report,
                layer,
                "The editing lock is not retained in this converted PSD.",
            );
        }
        converted.push(output);
    }
    Ok(converted)
}

fn append_layers(builder: &mut PsdBuilder, layers: &[Layer], cancel: &AtomicBool) -> Result<()> {
    for layer in layers {
        check(cancel)?;
        if layer.is_group() {
            let mut spec = GroupSpec::new(&layer.name);
            spec.visible = layer.visible;
            builder.begin_group(spec);
            append_layers(builder, &layer.children, cancel)?;
            builder.end_group()?;
        } else {
            let image = layer
                .image
                .as_ref()
                .context("Missing PSD raster appearance")?;
            let mut spec = LayerSpec::new(
                &layer.name,
                layer.offset_x as i32,
                layer.offset_y as i32,
                image.width(),
                image.height(),
                PixelData::Rgba8(image.as_raw().clone()),
            );
            spec.visible = layer.visible;
            spec.opacity = (layer.opacity * 255.).round() as u8;
            spec.blend_mode = blend(&layer.blend_mode)?;
            if let (Some(mask), Some(rect)) = (&layer.mask, mask_geometry(layer)?) {
                spec.mask = Some(MaskSpec {
                    rect,
                    data: mask.pixels().map(|p| p[0]).collect(),
                    default_color: crate::effects::mask_outside_coverage(&layer.metadata, mask),
                    disabled: layer
                        .metadata
                        .get("maskEnabled")
                        .and_then(serde_json::Value::as_bool)
                        == Some(false),
                });
            }
            builder.push_layer(spec);
        }
    }
    Ok(())
}

fn blend(value: &str) -> Result<BlendMode> {
    let name: String = value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    Ok(match name.as_str() {
        "normal" | "sourceover" => BlendMode::Normal,
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "colordodge" => BlendMode::ColorDodge,
        "colorburn" => BlendMode::ColorBurn,
        "hardlight" => BlendMode::HardLight,
        "softlight" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "add" | "lineardodge" | "lineardodgeadd" => BlendMode::LinearDodge,
        "subtract" => BlendMode::Subtract,
        "linearburn" => BlendMode::LinearBurn,
        "vividlight" => BlendMode::VividLight,
        "linearlight" => BlendMode::LinearLight,
        "pinlight" => BlendMode::PinLight,
        "hardmix" => BlendMode::HardMix,
        "divide" => BlendMode::Divide,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => bail!("Unsupported Photoshop blend mode: {value}"),
    })
}
