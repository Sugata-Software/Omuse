//! CPU reference renderer. Pixels are straight-alpha sRGB; blending follows the
//! W3C compositing equations. Transforms rotate clockwise about scaled bounds.
#[path = "raster16.rs"]
mod high_precision;
use crate::model::{Document, Layer};
use anyhow::{Context, Result, bail};
pub use high_precision::{composite16, export16, export16_cancellable};
use image::{
    DynamicImage, ImageEncoder, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage,
    codecs::{
        jpeg::{JpegEncoder, PixelDensity},
        webp::WebPEncoder,
    },
};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

pub const MAX_PIXELS: u64 = crate::model::MAX_PIXELS;
const MAX_DEPTH: usize = 64;

fn valid_size(w: u32, h: u32) -> bool {
    crate::model::valid_dimensions(w, h)
}

/// Invalid/oversized documents produce an empty surface; export rejects them.
/// Prefer `validate` before presenting or exporting imported documents.
pub fn composite(doc: &Document) -> RgbaImage {
    if !valid_size(doc.width, doc.height) || !validate(doc).is_empty() {
        return RgbaImage::new(0, 0);
    }
    fn has_editable(layers: &[Layer]) -> bool {
        layers
            .iter()
            .any(|l| l.advanced.is_some() || has_editable(&l.children))
    }
    if has_editable(&doc.layers)
        && let Ok(full) = composite16(doc)
    {
        // Quantize once at the display boundary, retaining sub-byte detail
        // throughout editable filtering and layer composition.
        return RgbaImage::from_fn(full.width(), full.height(), |x, y| {
            Rgba(
                full.get_pixel(x, y)
                    .0
                    .map(|v| ((u32::from(v) + 128) / 257) as u8),
            )
        });
    }
    let live_masks = build_live_masks(doc);
    let mut result = RgbaImage::from_pixel(doc.width, doc.height, Rgba(doc.background));
    draw_layers(&mut result, &doc.layers, 0, 1.0, &[], &live_masks);
    result
}

/// Refresh a rectangular canvas region without allocating a full composite.
///
/// This exact fast path intentionally accepts only flat, integer-aligned raster
/// layers with Normal blending. Unsupported documents return `false` before a
/// target byte is changed, allowing the caller to fall back to `composite`.
pub fn composite_region(
    doc: &Document,
    target: &mut RgbaImage,
    region: crate::model::PixelRect,
) -> bool {
    if target.dimensions() != (doc.width, doc.height) || !validate(doc).is_empty() {
        return false;
    }
    if doc.layers.iter().any(|layer| {
        layer.is_group()
            || layer.advanced.is_some()
            || !layer.children.is_empty()
            || layer.image.is_none()
            || layer.mask.is_some()
            || !region_metadata_is_supported(layer)
            || !matches!(mode(&layer.blend_mode), Some(Mode::Normal))
            || layer.rotation != 0.0
            || layer.scale_x != 1.0
            || layer.scale_y != 1.0
            || !layer.offset_x.is_finite()
            || !layer.offset_y.is_finite()
            || layer.offset_x.fract() != 0.0
            || layer.offset_y.fract() != 0.0
    }) {
        return false;
    }

    let x0 = region.x.min(doc.width);
    let y0 = region.y.min(doc.height);
    let x1 = region.x.saturating_add(region.width).min(doc.width);
    let y1 = region.y.saturating_add(region.height).min(doc.height);
    if x0 >= x1 || y0 >= y1 {
        return true;
    }

    for y in y0..y1 {
        for x in x0..x1 {
            target.put_pixel(x, y, Rgba(doc.background));
        }
    }
    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let image = layer.image.as_ref().unwrap();
        let offset_x = layer.offset_x as i64;
        let offset_y = layer.offset_y as i64;
        let layer_x0 = offset_x.max(i64::from(x0));
        let layer_y0 = offset_y.max(i64::from(y0));
        let layer_x1 = offset_x
            .saturating_add(i64::from(image.width()))
            .min(i64::from(x1));
        let layer_y1 = offset_y
            .saturating_add(i64::from(image.height()))
            .min(i64::from(y1));
        if layer_x0 >= layer_x1 || layer_y0 >= layer_y1 {
            continue;
        }
        let opacity = layer.opacity.clamp(0.0, 1.0);
        let blend_if = crate::advanced::layer_blend_if(layer).ok().flatten();
        for y in layer_y0..layer_y1 {
            for x in layer_x0..layer_x1 {
                let mut source = image
                    .get_pixel((x - offset_x) as u32, (y - offset_y) as u32)
                    .0;
                let destination = target.get_pixel_mut(x as u32, y as u32);
                let pixel_opacity = blend_if.as_ref().map_or(opacity, |settings| {
                    opacity
                        * crate::advanced_ops::blend_if_coverage(source, destination.0, settings)
                });
                if pixel_opacity != 1.0 {
                    source[3] = (f32::from(source[3]) * pixel_opacity).round() as u8;
                }
                if source[3] != 0 {
                    destination.0 = normal_over(destination.0, source);
                }
            }
        }
    }
    true
}

/// Saved projects retain their complete layer records as metadata. These keys
/// are inert copies: the renderer uses the parsed `Layer` fields and decoded
/// image instead. Keep this allowlist closed so unknown extensions and future
/// rendering semantics fall back to the complete compositor. Blend If is the
/// one active metadata record here; regional blending evaluates it per pixel.
fn region_metadata_is_supported(layer: &Layer) -> bool {
    let Some(metadata) = layer.metadata.as_object() else {
        return false;
    };
    metadata.iter().all(|(key, value)| match key.as_str() {
        "id" | "name" | "isVisible" | "locked" | "opacity" | "blendMode" | "parentID"
        | "imageFile" | "maskFile" => true,
        // `isGroup: true` already fails `layer.is_group()` above. False is the
        // ordinary persisted raster marker and does not affect drawing.
        "isGroup" => value.as_bool() == Some(false),
        // At identity scale/rotation with integer offsets, sampling is unused.
        // Other known transform members are stale serialized copies of fields
        // already checked directly by composite_region.
        "transform" => value.as_object().is_some_and(|transform| {
            transform.iter().all(|(key, value)| match key.as_str() {
                "origin" | "size" | "rotation" | "flipX" | "flipY" => true,
                "sampling" => matches!(value.as_str(), Some("Nearest" | "Smooth" | "High quality")),
                _ => false,
            })
        }),
        // Import records this after converting pixels to sRGB. It is provenance
        // for reporting and round trips; the raster renderer never consults it.
        "sourceColorProfile" => value.as_object().is_some_and(|profile| {
            profile.len() == 3
                && profile.get("description").is_some_and(|v| v.is_string())
                && profile.get("iccBytes").is_some_and(|v| v.is_u64())
                && profile.get("fnv1a64").is_some_and(|v| v.as_str().is_some())
        }),
        crate::advanced::RASTER_BLEND_IF_KEY => {
            serde_json::from_value::<crate::advanced_ops::BlendIf>(value.clone())
                .ok()
                .is_some_and(|settings| crate::advanced_ops::validate_blend_if(&settings).is_ok())
        }
        _ => false,
    })
}

/// Report semantics the reference renderer cannot safely reproduce. Unknown
/// metadata remains in the document and is never removed by this renderer.
pub fn validate(doc: &Document) -> Vec<String> {
    let mut errors = Vec::new();
    if !valid_size(doc.width, doc.height) {
        errors.push("Document dimensions exceed renderer limits".into());
    }
    let mut live_sources = std::collections::HashSet::new();
    fn sources(layers: &[Layer], out: &mut std::collections::HashSet<String>) {
        for l in layers {
            if let Some(id) = l
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
            {
                out.insert(id.to_owned());
            }
            sources(&l.children, out);
        }
    }
    sources(&doc.layers, &mut live_sources);
    fn links(layers: &[Layer], out: &mut HashMap<String, String>) {
        for l in layers {
            if let Some(id) = l
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
            {
                out.insert(l.id.clone(), id.into());
            }
            links(&l.children, out)
        }
    }
    let mut graph = HashMap::new();
    links(&doc.layers, &mut graph);
    for (target, source) in &graph {
        if doc.find_layer(source).is_none_or(|l| {
            l.is_group() || l.metadata.get("adjustment").is_some_and(|a| !a.is_null())
        }) {
            errors.push(format!("Invalid live mask source for {target}"));
            continue;
        }
        let mut seen = std::collections::HashSet::new();
        let mut id = target;
        while let Some(next) = graph.get(id) {
            if !seen.insert(id) || seen.len() > 256 {
                errors.push("Live mask dependency cycle or excessive depth".into());
                break;
            }
            id = next;
        }
    }
    let live_pixels = u64::from(doc.width) * u64::from(doc.height) * graph.len() as u64;
    if live_pixels > MAX_PIXELS {
        errors.push("Live mask dependency surfaces exceed renderer memory limit".into());
    }
    fn mask_taps(layer: &Layer) -> u64 {
        if !mask_enabled(layer) || layer.mask.is_none() {
            return 0;
        }
        let folder_style = layer.is_group()
            || layer
                .metadata
                .get("adjustment")
                .is_some_and(|v| !v.is_null());
        let sampling = if folder_style {
            layer.metadata.pointer("/transform/sampling")
        } else {
            layer
                .metadata
                .pointer("/maskPlacement/sampling")
                .or_else(|| layer.metadata.pointer("/transform/sampling"))
        };
        match sampling
            .and_then(serde_json::Value::as_str)
            .unwrap_or("High quality")
        {
            "Nearest" => 1,
            "Smooth" => 4,
            _ => 36,
        }
    }
    fn layer_area(layer: &Layer, cw: u32, ch: u32) -> u64 {
        if layer
            .metadata
            .get("adjustment")
            .is_some_and(|v| !v.is_null())
        {
            return u64::from(cw) * u64::from(ch);
        }
        let Some(image) = &layer.image else { return 0 };
        let (sin, cos) = f64::from(layer.rotation).to_radians().sin_cos();
        let w = (f64::from(image.width()) * f64::from(layer.scale_x).abs() * cos.abs()
            + f64::from(image.height()) * f64::from(layer.scale_y).abs() * sin.abs())
        .ceil()
        .clamp(0., f64::from(cw)) as u64;
        let h = (f64::from(image.width()) * f64::from(layer.scale_x).abs() * sin.abs()
            + f64::from(image.height()) * f64::from(layer.scale_y).abs() * cos.abs())
        .ceil()
        .clamp(0., f64::from(ch)) as u64;
        w.saturating_mul(h)
    }
    fn render_work(layer: &Layer, inherited: u64, cw: u32, ch: u32) -> u64 {
        let area = layer_area(layer, cw, ch);
        let own = mask_taps(layer);
        let transformed = layer.scale_x != 1.
            || layer.scale_y != 1.
            || layer.rotation.rem_euclid(360.) != 0.
            || layer.offset_x.fract() != 0.
            || layer.offset_y.fract() != 0.;
        let high = layer
            .metadata
            .pointer("/transform/sampling")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("High quality")
            == "High quality";
        let mut work = area.saturating_mul(inherited.saturating_add(own));
        if layer.image.is_some() && transformed && high {
            let taps = |scale: f32| {
                let f = f64::from(scale.abs()).min(1.).max(3. / 32.);
                (6. / f).ceil() as u64 + 2
            };
            work = work.saturating_add(
                area.saturating_mul(taps(layer.scale_x))
                    .saturating_mul(taps(layer.scale_y)),
            );
        }
        if own > 0
            && layer.metadata.get("effects").is_some_and(|v| !v.is_null())
            && layer
                .metadata
                .get("maskPlacement")
                .is_some_and(|v| !v.is_null())
        {
            if let Some(image) = &layer.image {
                work = work.saturating_add(
                    u64::from(image.width())
                        .saturating_mul(u64::from(image.height()))
                        .saturating_mul(own),
                );
            }
        }
        work
    }
    fn visit(
        layers: &[Layer],
        depth: usize,
        inherited_mask_taps: u64,
        live_sources: &std::collections::HashSet<String>,
        canvas_width: u32,
        canvas_height: u32,
        high_quality_work: &mut u64,
        errors: &mut Vec<String>,
    ) {
        if depth >= MAX_DEPTH && !layers.is_empty() {
            errors.push("Layer nesting exceeds 64 levels".into());
            return;
        }
        for l in layers {
            if let Err(error) = crate::advanced::layer_blend_if(l) {
                errors.push(format!("{}: {error}", l.name));
            }
            if let Err(error) = crate::effects::validate_layer_metadata(&l.metadata)
                .and_then(|_| crate::effects::validate_mask_metadata(&l.metadata))
            {
                errors.push(format!("{}: {error}", l.name));
            }
            if mode(&l.blend_mode).is_none() {
                errors.push(format!(
                    "{}: unsupported blend mode {}",
                    l.name, l.blend_mode
                ));
            }
            if ![
                l.offset_x, l.offset_y, l.rotation, l.scale_x, l.scale_y, l.opacity,
            ]
            .iter()
            .all(|x| x.is_finite())
            {
                errors.push(format!("{}: non-finite transform or opacity", l.name));
            }
            if l.scale_x == 0.0 || l.scale_y == 0.0 {
                errors.push(format!("{}: zero scale", l.name));
            }
            if !l.children.is_empty() && l.image.is_some() {
                errors.push(format!("{}: layer has both pixels and children", l.name));
            }
            *high_quality_work = high_quality_work.saturating_add(render_work(
                l,
                inherited_mask_taps,
                canvas_width,
                canvas_height,
            ));
            let child_mask_taps =
                inherited_mask_taps.saturating_add(if l.is_group() { mask_taps(l) } else { 0 });
            visit(
                &l.children,
                depth + 1,
                child_mask_taps,
                live_sources,
                canvas_width,
                canvas_height,
                high_quality_work,
                errors,
            );
        }
    }
    let mut high_quality_work = 0;
    visit(
        &doc.layers,
        0,
        0,
        &live_sources,
        doc.width,
        doc.height,
        &mut high_quality_work,
        &mut errors,
    );
    for source in graph.values() {
        if let Some(layer) = doc.find_layer(source) {
            high_quality_work =
                high_quality_work.saturating_add(render_work(layer, 0, doc.width, doc.height));
        }
    }
    if high_quality_work > 1_000_000_000 {
        errors.push("Image and mask resampling exceed renderer work limit".into());
    }
    errors
}

/// Export atomically through a sibling temporary file. JPEG has no alpha and
/// is explicitly flattened against white; alpha is retained in PNG/WebP/TIFF.
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    pub jpeg_quality: u8,
    pub matte: [u8; 3],
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            jpeg_quality: 80,
            matte: [255; 3],
        }
    }
}
pub fn export(doc: &Document, path: &Path) -> Result<()> {
    export_with_options(doc, path, ExportOptions::default())
}
/// Maximum full-resolution canvas for an in-memory JPEG preview.
pub const MAX_JPEG_PREVIEW_PIXELS: u64 = 16_000_000;

fn validate_export(doc: &Document, options: ExportOptions) -> Result<()> {
    anyhow::ensure!(
        (1..=100).contains(&options.jpeg_quality),
        "JPEG quality must be 1–100"
    );
    let errors = validate(doc);
    if !errors.is_empty() {
        bail!("Cannot faithfully export: {}", errors.join("; "));
    }
    Ok(())
}

fn export_resolution(doc: &Document) -> f64 {
    doc.metadata
        .get("resolution")
        .and_then(serde_json::Value::as_f64)
        .filter(|v| v.is_finite() && (1.0..=9600.0).contains(v))
        .unwrap_or(72.0)
}

fn export_image(doc: &Document, format: ImageFormat, options: ExportOptions) -> DynamicImage {
    let rgba = composite(doc);
    if format == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(RgbImage::from_fn(doc.width, doc.height, |x, y| {
            let p = rgba.get_pixel(x, y).0;
            let a = u32::from(p[3]);
            Rgb(std::array::from_fn(|i| {
                ((u32::from(p[i]) * a + u32::from(options.matte[i]) * (255 - a) + 127) / 255) as u8
            }))
        }))
    } else {
        DynamicImage::ImageRgba8(rgba)
    }
}

fn write_jpeg(
    image: &DynamicImage,
    output: impl std::io::Write,
    options: ExportOptions,
    resolution: f64,
) -> Result<()> {
    // LittleCMS includes profile creation time. Reuse the same valid profile
    // so a preview and a later export produce identical bytes in this process.
    static ICC: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let icc = if let Some(profile) = ICC.get() {
        profile
    } else {
        let profile = crate::color_management::srgb_profile()?;
        let _ = ICC.set(profile);
        ICC.get().expect("sRGB profile initialized")
    };
    let mut encoder = JpegEncoder::new_with_quality(output, options.jpeg_quality);
    encoder.set_pixel_density(PixelDensity::dpi(
        resolution.round().clamp(1., u16::MAX as f64) as u16,
    ));
    encoder.set_icc_profile(icc.clone())?;
    image.write_with_encoder(encoder)?;
    Ok(())
}

/// Encode the actual exported JPEG for a full-resolution preview. This shares
/// compositing, matte, quality, ICC, and DPI with file export. Large canvases are
/// rejected before rendering; file export retains its normal document limit.
pub fn encode_jpeg(doc: &Document, options: ExportOptions) -> Result<Vec<u8>> {
    anyhow::ensure!(
        u64::from(doc.width) * u64::from(doc.height) <= MAX_JPEG_PREVIEW_PIXELS,
        "JPEG preview is limited to 16 megapixels"
    );
    validate_export(doc, options)?;
    let image = export_image(doc, ImageFormat::Jpeg, options);
    let mut bytes = Vec::new();
    write_jpeg(&image, &mut bytes, options, export_resolution(doc))?;
    Ok(bytes)
}

pub fn export_with_options(doc: &Document, path: &Path, options: ExportOptions) -> Result<()> {
    export_with_options_checked(doc, path, options, || Ok(()))
}

#[derive(Clone, Debug)]
pub struct ExportCancellation {
    state: Arc<Mutex<ExportCancellationState>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportCancellationState {
    Active,
    Cancelled,
    Published,
}

impl ExportCancellation {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ExportCancellationState::Active)),
        }
    }

    /// Request cancellation. Returns `false` only once publication has already
    /// committed and the destination is visible.
    pub fn cancel(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match *state {
            ExportCancellationState::Active => {
                *state = ExportCancellationState::Cancelled;
                true
            }
            ExportCancellationState::Cancelled => true,
            ExportCancellationState::Published => false,
        }
    }

    fn check_active(&self) -> Result<()> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match *state {
            ExportCancellationState::Active => Ok(()),
            ExportCancellationState::Cancelled => bail!("Export cancelled"),
            ExportCancellationState::Published => bail!("Export already published"),
        }
    }

    /// Hold the same state gate across the final cancellation check and rename.
    /// `cancel` therefore either prevents publication or observes it as complete.
    fn publish(&self, temp: &Path, path: &Path) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match *state {
            ExportCancellationState::Cancelled => bail!("Export cancelled"),
            ExportCancellationState::Published => bail!("Export already published"),
            ExportCancellationState::Active => {}
        }
        std::fs::rename(temp, path).context("Publishing exported image")?;
        *state = ExportCancellationState::Published;
        Ok(())
    }
}

impl Default for ExportCancellation {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation leaves an existing destination intact until the final
/// publication gate commits. Codecs may finish encoding privately first.
pub fn export_with_options_cancellable(
    doc: &Document,
    path: &Path,
    options: ExportOptions,
    cancel: &ExportCancellation,
) -> Result<()> {
    export_with_options_checked_with_publish(
        doc,
        path,
        options,
        || cancel.check_active(),
        |temp, path| cancel.publish(temp, path),
    )
}

fn export_with_options_checked(
    doc: &Document,
    path: &Path,
    options: ExportOptions,
    check: impl Fn() -> Result<()>,
) -> Result<()> {
    export_with_options_checked_with_publish(doc, path, options, check, |temp, path| {
        std::fs::rename(temp, path).context("Publishing exported image")
    })
}

fn export_with_options_checked_with_publish(
    doc: &Document,
    path: &Path,
    options: ExportOptions,
    check: impl Fn() -> Result<()>,
    publish: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    check()?;
    validate_export(doc, options)?;
    let format = ImageFormat::from_path(path).context("Choose PNG, JPEG, WebP, or TIFF")?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Tiff
    ) {
        bail!("Export supports PNG, JPEG, WebP, and TIFF");
    }
    let image = export_image(doc, format, options);
    check()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(".omuse-export-{}.tmp", uuid::Uuid::new_v4()));
    let outcome = (|| -> Result<()> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let resolution = export_resolution(doc);
        match format {
            ImageFormat::Png => {
                let rgba = image.to_rgba8();
                let mut info = png::Info::with_size(doc.width, doc.height);
                info.color_type = png::ColorType::Rgba;
                info.bit_depth = png::BitDepth::Eight;
                info.icc_profile = Some(std::borrow::Cow::Owned(
                    crate::color_management::srgb_profile()?,
                ));
                let ppm = (resolution / 0.0254).round().clamp(1., u32::MAX as f64) as u32;
                info.pixel_dims = Some(png::PixelDimensions {
                    xppu: ppm,
                    yppu: ppm,
                    unit: png::Unit::Meter,
                });
                let mut encoder = png::Encoder::with_info(&mut output, info)?;
                // Lossless fast compression keeps large photo exports responsive.
                encoder.set_compression(png::Compression::Fast);
                let mut writer = encoder.write_header()?;
                writer.write_image_data(rgba.as_raw())?;
            }
            ImageFormat::Jpeg => {
                write_jpeg(&image, &mut output, options, resolution)?;
            }
            ImageFormat::WebP => {
                let mut encoder = WebPEncoder::new_lossless(&mut output);
                encoder.set_icc_profile(crate::color_management::srgb_profile()?)?;
                image.write_with_encoder(encoder)?;
            }
            ImageFormat::Tiff => {
                let rgba = image.to_rgba8();
                let mut encoder = tiff::encoder::TiffEncoder::new(&mut output)?;
                let mut encoded =
                    encoder.new_image::<tiff::encoder::colortype::RGBA8>(doc.width, doc.height)?;
                encoded.encoder().write_tag(
                    tiff::tags::Tag::IccProfile,
                    crate::color_management::srgb_profile()?.as_slice(),
                )?;
                encoded
                    .encoder()
                    .write_tag(tiff::tags::Tag::ExtraSamples, &[2u16][..])?;
                encoded.resolution(
                    tiff::tags::ResolutionUnit::Inch,
                    tiff::encoder::Rational {
                        n: (resolution * 1000.).round() as u32,
                        d: 1000,
                    },
                );
                encoded.write_data(rgba.as_raw())?;
            }
            _ => unreachable!(),
        }
        output.sync_all()?;
        check()?;
        publish(&temp, path)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    outcome
}

// Original Compositor folders are pass-through: their opacity and masks
// multiply each descendant, while transforms belong to individual paint layers.
#[derive(Clone)]
struct FolderSampler<'a> {
    mask: crate::effects::MaskSampler<'a>,
    width: f64,
    height: f64,
    sx: f64,
    sy: f64,
    cx: f64,
    cy: f64,
    sin: f64,
    cos: f64,
}
impl<'a> FolderSampler<'a> {
    fn new(layer: &'a Layer) -> Option<Self> {
        let image = layer.mask.as_ref()?;
        let width = f64::from(image.width());
        let height = f64::from(image.height());
        let size = layer
            .metadata
            .get("transform")
            .and_then(|t| t.get("size"))
            .and_then(serde_json::Value::as_array);
        let extent_x = size
            .and_then(|s| s.first())
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(width);
        let extent_y = size
            .and_then(|s| s.get(1))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(height);
        let sx = extent_x / width * f64::from(layer.scale_x);
        let sy = extent_y / height * f64::from(layer.scale_y);
        let (sin, cos) = f64::from(layer.rotation).to_radians().sin_cos();
        Some(Self {
            mask: crate::effects::MaskSampler::new_folder(&layer.metadata, image),
            width,
            height,
            sx,
            sy,
            cx: f64::from(layer.offset_x) + width * sx.abs() / 2.,
            cy: f64::from(layer.offset_y) + height * sy.abs() / 2.,
            sin,
            cos,
        })
    }
    fn coverage(&self, x: f64, y: f64) -> f32 {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let u = (dx * self.cos + dy * self.sin) / self.sx + self.width / 2.;
        let v = (-dx * self.sin + dy * self.cos) / self.sy + self.height / 2.;
        self.mask.coverage(x, y, u, v, self.width, self.height)
    }
}
fn draw_layers<'a>(
    target: &mut RgbaImage,
    layers: &'a [Layer],
    depth: usize,
    inherited_opacity: f32,
    inherited_masks: &[FolderSampler<'a>],
    live_masks: &HashMap<String, RgbaImage>,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    let mut skip_until = 0usize;
    for (index, layer) in layers.iter().enumerate() {
        if index < skip_until {
            continue;
        }
        if !layer.visible || !layer.opacity.is_finite() || layer.opacity <= 0.0 {
            continue;
        }
        if !layer.is_group()
            && layer
                .metadata
                .get("adjustment")
                .is_none_or(serde_json::Value::is_null)
            && layer
                .metadata
                .get("maskSourceID")
                .is_none_or(serde_json::Value::is_null)
        {
            let mut end = index + 1;
            while end < layers.len()
                && layers[end]
                    .metadata
                    .get("maskSourceID")
                    .and_then(serde_json::Value::as_str)
                    == Some(layer.id.as_str())
            {
                end += 1;
            }
            if end > index + 1 {
                let mut base = layer.clone();
                base.blend_mode = "Normal".into();
                let blend_if = crate::advanced::layer_blend_if(&base).ok().flatten();
                if blend_if.is_some() {
                    if let Some(state) = base.advanced.as_mut() {
                        std::sync::Arc::make_mut(state).recipe.blend_if = None;
                    }
                    if let Some(metadata) = base.metadata.as_object_mut() {
                        metadata.remove(crate::advanced::RASTER_BLEND_IF_KEY);
                    }
                }
                let mut plane = RgbaImage::new(target.width(), target.height());
                draw_layers(
                    &mut plane,
                    std::slice::from_ref(&base),
                    depth,
                    inherited_opacity,
                    inherited_masks,
                    live_masks,
                );
                let alpha: Vec<u8> = plane.pixels().map(|p| p[3]).collect();
                let tonal_alpha: Option<Vec<f32>> = blend_if.as_ref().map(|settings| {
                    plane
                        .pixels()
                        .zip(target.pixels())
                        .map(|(src, dst)| {
                            crate::advanced_ops::blend_if_coverage(src.0, dst.0, settings)
                        })
                        .collect()
                });
                for p in plane.pixels_mut() {
                    p[3] = 255;
                }
                for child in &layers[index + 1..end] {
                    let mut child = child.clone();
                    if let Some(metadata) = child.metadata.as_object_mut() {
                        metadata.remove("maskSourceID");
                    }
                    draw_layers(
                        &mut plane,
                        std::slice::from_ref(&child),
                        depth,
                        inherited_opacity,
                        &[],
                        live_masks,
                    );
                }
                let blend = mode(&layer.blend_mode).unwrap_or(Mode::Normal);
                for (i, ((dst, src), alpha)) in target
                    .pixels_mut()
                    .zip(plane.pixels())
                    .zip(alpha)
                    .enumerate()
                {
                    let mut pixel = src.0;
                    pixel[3] =
                        (alpha as f32 * tonal_alpha.as_ref().map_or(1., |v| v[i])).round() as u8;
                    dst.0 = over(dst.0, pixel, blend);
                }
                skip_until = end;
                continue;
            }
        }
        // Clipping stacks remove only their immediate child's link. Keep the
        // shared dependency surfaces available to nested descendants, while
        // avoiding a second application of the base mask to that child.
        let live_mask = layer
            .metadata
            .get("maskSourceID")
            .and_then(serde_json::Value::as_str)
            .and_then(|_| live_masks.get(&layer.id));
        if let Some(adjustment) = layer.metadata.get("adjustment").filter(|v| !v.is_null()) {
            if let Ok(adjusted) = crate::effects::apply_adjustment(target, adjustment) {
                let blend = mode(&layer.blend_mode).unwrap_or(Mode::Normal);
                let base = (layer.opacity * inherited_opacity).clamp(0.0, 1.0);
                let own_mask = (mask_enabled(layer) && layer.mask.is_some())
                    .then(|| FolderSampler::new(layer))
                    .flatten();
                for (x, y, dst) in target.enumerate_pixels_mut() {
                    let src = adjusted.get_pixel(x, y).0;
                    let original = dst.0;
                    let mut amount = base;
                    if let Some(mask) = &own_mask {
                        amount *= mask.coverage(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    }
                    if let Some(clip) = live_mask {
                        amount *= f32::from(clip.get_pixel(x, y)[3]) / 255.0;
                    }
                    for folder in inherited_masks {
                        amount *= folder.coverage(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    }
                    let blended = over(
                        [original[0], original[1], original[2], 255],
                        [src[0], src[1], src[2], 255],
                        blend,
                    );
                    for c in 0..3 {
                        dst[c] = (f32::from(original[c]) * (1.0 - amount)
                            + f32::from(blended[c]) * amount)
                            .round() as u8;
                    }
                    dst[3] = original[3];
                }
            }
        } else if layer.is_group() {
            let mut masks = inherited_masks.to_vec();
            if mask_enabled(layer) && layer.mask.is_some() {
                if let Some(mask) = FolderSampler::new(layer) {
                    masks.push(mask);
                }
            }
            draw_layers(
                target,
                &layer.children,
                depth + 1,
                inherited_opacity * layer.opacity.clamp(0.0, 1.0),
                &masks,
                live_masks,
            );
        } else if let Some(image) = &layer.image {
            if let Some(value) = layer.metadata.get("effects").filter(|v| !v.is_null()) {
                if let Ok(fx) = crate::effects::LayerEffects::parse(value) {
                    let placed_mask = effect_mask(layer, image);
                    let own_mask = if mask_enabled(layer) {
                        placed_mask.as_ref().or(layer.mask.as_deref())
                    } else {
                        None
                    };
                    if let Ok((made, inset)) = crate::effects::render(image, own_mask, &fx) {
                        let mut derived = layer.clone();
                        derived.image = Some(made.into());
                        derived.mask = None;
                        derived.offset_x -= inset as f32 * derived.scale_x.abs();
                        derived.offset_y -= inset as f32 * derived.scale_y.abs();
                        draw_image(
                            target,
                            derived.image.as_ref().unwrap(),
                            &derived,
                            inherited_opacity,
                            inherited_masks,
                            live_mask,
                        );
                        continue;
                    }
                }
            }
            draw_image(
                target,
                image,
                layer,
                inherited_opacity,
                inherited_masks,
                live_mask,
            );
        }
    }
}
fn mask_enabled(layer: &Layer) -> bool {
    layer
        .metadata
        .get("maskEnabled")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
}
fn sample(image: &RgbaImage, u: f64, v: f64, smooth: bool) -> [u8; 4] {
    if !smooth {
        return image.get_pixel(u.max(0.0) as u32, v.max(0.0) as u32).0;
    }
    // Interpolate premultiplied channels, preventing transparent-edge color halos.
    let px = u - 0.5;
    let py = v - 0.5;
    let ix = px.floor() as i64;
    let iy = py.floor() as i64;
    let fx = (px - px.floor()) as f32;
    let fy = (py - py.floor()) as f32;
    let mut sum = [0.0f32; 4];
    for (ox, oy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let x = (ix + ox).clamp(0, i64::from(image.width()) - 1) as u32;
        let y = (iy + oy).clamp(0, i64::from(image.height()) - 1) as u32;
        let p = image.get_pixel(x, y).0;
        let a = f32::from(p[3]) / 255.0;
        for c in 0..3 {
            sum[c] += f32::from(p[c]) * a * weight;
        }
        sum[3] += a * weight;
    }
    if sum[3] <= 0.0 {
        return [0; 4];
    }
    [
        (sum[0] / sum[3]).round() as u8,
        (sum[1] / sum[3]).round() as u8,
        (sum[2] / sum[3]).round() as u8,
        (sum[3] * 255.0).round() as u8,
    ]
}

pub(crate) fn sample_linear(image: &RgbaImage, u: f64, v: f64) -> [u8; 4] {
    sample(image, u, v, true)
}

pub(crate) fn sample_lanczos(image: &RgbaImage, u: f64, v: f64, sx: f64, sy: f64) -> [u8; 4] {
    fn sinc(x: f64) -> f64 {
        if x.abs() < 1e-9 {
            1.
        } else {
            let p = std::f64::consts::PI * x;
            p.sin() / p
        }
    }
    fn axis(x: f64, scale: f64) -> (i64, i64, f64) {
        let f = scale.abs().min(1.).max(3. / 32.);
        let radius = 3. / f;
        ((x - radius).floor() as i64, (x + radius).ceil() as i64, f)
    }
    // The scale floor bounds each axis to at most 66 candidate taps. Cache
    // weights independently, avoiding repeated horizontal sinc calls for each
    // row. Keep the original y-major/x-minor accumulation order exactly: a
    // separable intermediate sum would change floating-point rounding.
    fn taps(position: f64, scale: f64, limit: u32) -> ([(u32, f64); 66], usize) {
        let (first, last, factor) = axis(position, scale);
        let mut taps = [(0, 0.); 66];
        let mut count = 0;
        for index in first..=last {
            let distance = (position - (index as f64 + 0.5)) * factor;
            if distance.abs() >= 3. {
                continue;
            }
            taps[count] = (
                index.clamp(0, i64::from(limit) - 1) as u32,
                sinc(distance) * sinc(distance / 3.),
            );
            count += 1;
        }
        (taps, count)
    }
    let (xs, x_count) = taps(u, sx, image.width());
    let (ys, y_count) = taps(v, sy, image.height());
    let mut sum = [0.; 4];
    let mut weight = 0.;
    for &(y, wy) in &ys[..y_count] {
        for &(x, wx) in &xs[..x_count] {
            let w = wx * wy;
            let p = image.get_pixel(x, y).0;
            let a = f64::from(p[3]) / 255.;
            for c in 0..3 {
                sum[c] += f64::from(p[c]) * a * w;
            }
            sum[3] += a * w;
            weight += w;
        }
    }
    if weight.abs() < 1e-9 || sum[3] <= 0. {
        return [0; 4];
    }
    [
        (sum[0] / sum[3]).clamp(0., 255.).round() as u8,
        (sum[1] / sum[3]).clamp(0., 255.).round() as u8,
        (sum[2] / sum[3]).clamp(0., 255.).round() as u8,
        (sum[3] / weight * 255.).clamp(0., 255.).round() as u8,
    ]
}

fn draw_image(
    target: &mut RgbaImage,
    image: &RgbaImage,
    layer: &Layer,
    inherited_opacity: f32,
    inherited_masks: &[FolderSampler<'_>],
    live_mask: Option<&RgbaImage>,
) {
    static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let workers = *WORKERS.get_or_init(|| {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(4)
    });
    draw_image_with_workers(
        target,
        image,
        layer,
        inherited_opacity,
        inherited_masks,
        live_mask,
        workers,
    );
}

fn row_worker_count(width: u32, rows: u32, costly: bool, requested: usize) -> usize {
    let pixels = u64::from(width) * u64::from(rows);
    let threshold = if costly { 128 * 1024 } else { 1024 * 1024 };
    if pixels < threshold {
        return 1;
    }
    requested.clamp(1, 4).min((rows as usize / 32).max(1))
}

fn draw_image_with_workers(
    target: &mut RgbaImage,
    image: &RgbaImage,
    layer: &Layer,
    inherited_opacity: f32,
    inherited_masks: &[FolderSampler<'_>],
    live_mask: Option<&RgbaImage>,
    workers: usize,
) {
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let (sx, sy) = (f64::from(layer.scale_x), f64::from(layer.scale_y));
    if w == 0.0
        || h == 0.0
        || sx == 0.0
        || sy == 0.0
        || ![
            sx,
            sy,
            layer.offset_x as f64,
            layer.offset_y as f64,
            layer.rotation as f64,
        ]
        .iter()
        .all(|v| v.is_finite())
    {
        return;
    }
    let angle = f64::from(layer.rotation).rem_euclid(360.0).to_radians();
    let (sin, cos) = angle.sin_cos();
    let cx = f64::from(layer.offset_x) + w * sx.abs() * 0.5;
    let cy = f64::from(layer.offset_y) + h * sy.abs() * 0.5;
    let ex = (w * sx * cos).abs() * 0.5 + (h * sy * sin).abs() * 0.5;
    let ey = (w * sx * sin).abs() * 0.5 + (h * sy * cos).abs() * 0.5;
    let x0 = (cx - ex).floor().max(0.0).min(f64::from(target.width())) as u32;
    let y0 = (cy - ey).floor().max(0.0).min(f64::from(target.height())) as u32;
    let x1 = (cx + ex).ceil().max(0.0).min(f64::from(target.width())) as u32;
    let y1 = (cy + ey).ceil().max(0.0).min(f64::from(target.height())) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let blend = mode(&layer.blend_mode).unwrap_or(Mode::Normal);
    let blend_if = crate::advanced::layer_blend_if(layer).ok().flatten();
    if matches!(blend, Mode::Normal)
        && blend_if.is_none()
        && sx == 1.0
        && sy == 1.0
        && angle == 0.0
        && layer.offset_x.fract() == 0.0
        && layer.offset_y.fract() == 0.0
        && inherited_masks.is_empty()
        && live_mask.is_none()
        && (layer.mask.is_none() || !mask_enabled(layer))
    {
        // The overwhelmingly common editing path needs neither coordinate
        // interpolation nor transform equations per pixel. Opacity still uses
        // the exact alpha rounding and source-over routine from the general
        // path, so partially transparent layers remain byte-for-byte equal.
        let opacity = layer.opacity.clamp(0.0, 1.0) * inherited_opacity;
        let stride = target.width() as usize * 4;
        let source_stride = image.width() as usize * 4;
        let source_x = (f64::from(x0) - f64::from(layer.offset_x)) as usize;
        let row_bytes = (x1 - x0) as usize * 4;
        let process_rows = |rows: &mut [u8], first_y: u32| {
            for (row_index, row) in rows.chunks_exact_mut(stride).enumerate() {
                let y = first_y + row_index as u32;
                let source_y = (f64::from(y) - f64::from(layer.offset_y)) as usize;
                let from = source_y * source_stride + source_x * 4;
                let start = x0 as usize * 4;
                let destination = &mut row[start..start + row_bytes];
                let source = &image.as_raw()[from..from + row_bytes];
                // Empty paint layers and fully opaque photograph rows avoid
                // per-pixel source-over arithmetic without changing rounding.
                if source.chunks_exact(4).all(|pixel| pixel[3] == 0) {
                    continue;
                }
                if opacity == 1.0 {
                    if source.chunks_exact(4).all(|pixel| pixel[3] == 255) {
                        destination.copy_from_slice(source);
                        continue;
                    }
                    for (dst, src) in destination.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
                        let result = normal_over(
                            [dst[0], dst[1], dst[2], dst[3]],
                            [src[0], src[1], src[2], src[3]],
                        );
                        dst.copy_from_slice(&result);
                    }
                } else {
                    for (dst, src) in destination.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
                        let mut pixel = [src[0], src[1], src[2], src[3]];
                        pixel[3] = (f32::from(pixel[3]) * opacity).round() as u8;
                        let result = normal_over([dst[0], dst[1], dst[2], dst[3]], pixel);
                        dst.copy_from_slice(&result);
                    }
                }
            }
        };
        let rows = &mut (&mut **target)[y0 as usize * stride..y1 as usize * stride];
        let count = row_worker_count(x1 - x0, y1 - y0, false, workers);
        if count == 1 {
            process_rows(rows, y0);
        } else {
            let rows_per_chunk = ((y1 - y0) as usize).div_ceil(count);
            std::thread::scope(|scope| {
                for (index, chunk) in rows.chunks_mut(rows_per_chunk * stride).enumerate() {
                    let process_rows = &process_rows;
                    scope.spawn(move || process_rows(chunk, y0 + (index * rows_per_chunk) as u32));
                }
            });
        }
        return;
    }
    let smooth = !(sx == 1.0
        && sy == 1.0
        && angle == 0.0
        && layer.offset_x.fract() == 0.0
        && layer.offset_y.fract() == 0.0)
        && layer
            .metadata
            .get("transform")
            .and_then(|t| t.get("sampling"))
            .and_then(serde_json::Value::as_str)
            != Some("Nearest");
    let high_quality = smooth
        && layer
            .metadata
            .get("transform")
            .and_then(|t| t.get("sampling"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("High quality")
            == "High quality";
    let mask_sampler = if mask_enabled(layer) {
        layer
            .mask
            .as_ref()
            .map(|mask| crate::effects::MaskSampler::new(&layer.metadata, mask))
    } else {
        None
    };
    let stride = target.width() as usize * 4;
    let process_rows = |rows: &mut [u8], first_y: u32| {
        for (row_index, row) in rows.chunks_exact_mut(stride).enumerate() {
            let y = first_y + row_index as u32;
            for x in x0..x1 {
                let dx = f64::from(x) + 0.5 - cx;
                let dy = f64::from(y) + 0.5 - cy;
                let u = (dx * cos + dy * sin) / sx + w * 0.5;
                let v = (-dx * sin + dy * cos) / sy + h * 0.5;
                // Small tolerance prevents 90-degree floating-point edges disappearing.
                if u < -1e-9 || v < -1e-9 || u >= w || v >= h {
                    continue;
                }
                let mut p = if high_quality {
                    sample_lanczos(image, u.max(0.), v.max(0.), sx, sy)
                } else {
                    sample(image, u.max(0.0), v.max(0.0), smooth)
                };
                let mut opacity = layer.opacity.clamp(0.0, 1.0) * inherited_opacity;
                if let Some(mask) = &mask_sampler {
                    opacity *= mask.coverage(
                        f64::from(x) + 0.5,
                        f64::from(y) + 0.5,
                        u.max(0.0),
                        v.max(0.0),
                        w,
                        h,
                    );
                }
                if let Some(clip) = live_mask {
                    opacity *= f32::from(clip.get_pixel(x, y)[3]) / 255.0;
                }
                for folder in inherited_masks {
                    opacity *= folder.coverage(f64::from(x) + 0.5, f64::from(y) + 0.5);
                }
                if let Some(settings) = blend_if.as_ref() {
                    let offset = x as usize * 4;
                    opacity *= crate::advanced_ops::blend_if_coverage(
                        p,
                        [
                            row[offset],
                            row[offset + 1],
                            row[offset + 2],
                            row[offset + 3],
                        ],
                        settings,
                    );
                }
                p[3] = (f32::from(p[3]) * opacity).round() as u8;
                if p[3] > 0 {
                    let offset = x as usize * 4;
                    let dst = &mut row[offset..offset + 4];
                    let result = over([dst[0], dst[1], dst[2], dst[3]], p, blend);
                    dst.copy_from_slice(&result);
                }
            }
        }
    };
    let count = row_worker_count(
        x1 - x0,
        y1 - y0,
        high_quality || mask_sampler.is_some() || !inherited_masks.is_empty(),
        workers,
    );
    let affected_rows = &mut (&mut **target)[y0 as usize * stride..y1 as usize * stride];
    if count == 1 {
        process_rows(affected_rows, y0);
    } else {
        let rows_per_chunk =
            (y1 - y0) as usize / count + usize::from((y1 - y0) as usize % count != 0);
        // Each thread owns complete disjoint target rows. Source pixels and
        // prepared mask samplers are immutable; every pixel follows the same
        // scalar computation as the serial path. Scope joins all workers and
        // propagates failures rather than returning a partially drawn image.
        std::thread::scope(|scope| {
            let process_rows = &process_rows;
            for (index, rows) in affected_rows
                .chunks_mut(rows_per_chunk * stride)
                .enumerate()
            {
                scope.spawn(move || process_rows(rows, y0 + (index * rows_per_chunk) as u32));
            }
        });
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Dodge,
    Burn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Add,
    Subtract,
    LinearBurn,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
pub fn supported_blend_mode(value: &str) -> bool {
    mode(value).is_some()
}

fn mode(value: &str) -> Option<Mode> {
    let name: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    Some(match name.as_str() {
        "normal" | "sourceover" => Mode::Normal,
        "multiply" => Mode::Multiply,
        "screen" => Mode::Screen,
        "overlay" => Mode::Overlay,
        "darken" => Mode::Darken,
        "lighten" => Mode::Lighten,
        "colordodge" => Mode::Dodge,
        "colorburn" => Mode::Burn,
        "hardlight" => Mode::HardLight,
        "softlight" => Mode::SoftLight,
        "difference" => Mode::Difference,
        "exclusion" => Mode::Exclusion,
        "add" | "lineardodge" | "lineardodgeadd" => Mode::Add,
        "subtract" => Mode::Subtract,
        "hue" => Mode::Hue,
        "saturation" => Mode::Saturation,
        "linearburn" => Mode::LinearBurn,
        "vividlight" => Mode::VividLight,
        "linearlight" => Mode::LinearLight,
        "pinlight" => Mode::PinLight,
        "hardmix" => Mode::HardMix,
        "divide" => Mode::Divide,
        "color" => Mode::Color,
        "luminosity" => Mode::Luminosity,
        _ => return None,
    })
}
fn channel(b: f32, s: f32, mode: Mode) -> f32 {
    match mode {
        Mode::Multiply => b * s,
        Mode::Screen => b + s - b * s,
        Mode::Overlay => {
            if b <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        Mode::Darken => b.min(s),
        Mode::Lighten => b.max(s),
        Mode::Dodge => {
            if b == 0.0 {
                0.0
            } else if s == 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        Mode::Burn => {
            if b == 1.0 {
                1.0
            } else if s == 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        Mode::HardLight => {
            if s <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        Mode::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        Mode::Difference => (b - s).abs(),
        Mode::Exclusion => b + s - 2.0 * b * s,
        Mode::LinearBurn => (b + s - 1.0).max(0.0),
        Mode::VividLight => {
            if s <= 0.5 {
                channel(b, 2.0 * s, Mode::Burn)
            } else {
                channel(b, 2.0 * s - 1.0, Mode::Dodge)
            }
        }
        Mode::LinearLight => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        Mode::PinLight => {
            if s <= 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        Mode::HardMix => {
            if channel(b, s, Mode::VividLight) < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        Mode::Divide => {
            if s == 0.0 {
                1.0
            } else {
                (b / s).min(1.0)
            }
        }
        Mode::Add => (b + s).min(1.0),
        Mode::Subtract => (b - s).max(0.0),
        _ => s,
    }
}
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn sat(c: [f32; 3]) -> f32 {
    c.into_iter().fold(0.0, f32::max) - c.into_iter().fold(1.0, f32::min)
}
fn set_lum(mut c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    for x in &mut c {
        *x += d;
    }
    let n = c.into_iter().fold(f32::INFINITY, f32::min);
    let x = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if n < 0.0 {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut c {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    c
}
fn set_sat(mut c: [f32; 3], s: f32) -> [f32; 3] {
    let mut ix = [0, 1, 2];
    ix.sort_by(|a, b| c[*a].total_cmp(&c[*b]));
    let [lo, mid, hi] = ix;
    if c[hi] > c[lo] {
        c[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
        c[hi] = s;
    } else {
        c[mid] = 0.0;
        c[hi] = 0.0;
    }
    c[lo] = 0.0;
    c
}
fn normal_over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    if src[3] == 0 {
        return dst;
    }
    if src[3] == 255 || dst[3] == 0 {
        return src;
    }
    let sa = u32::from(src[3]);
    let da = u32::from(dst[3]);
    let inverse = 255 - sa;
    if da == 255 {
        return [
            ((u32::from(src[0]) * sa + u32::from(dst[0]) * inverse + 127) / 255) as u8,
            ((u32::from(src[1]) * sa + u32::from(dst[1]) * inverse + 127) / 255) as u8,
            ((u32::from(src[2]) * sa + u32::from(dst[2]) * inverse + 127) / 255) as u8,
            255,
        ];
    }
    let alpha = sa * 255 + da * inverse;
    let mut out = [0; 4];
    for c in 0..3 {
        out[c] = ((u32::from(src[c]) * sa * 255 + u32::from(dst[c]) * da * inverse + alpha / 2)
            / alpha) as u8;
    }
    out[3] = ((alpha + 127) / 255) as u8;
    out
}

fn over(dst: [u8; 4], src: [u8; 4], mode: Mode) -> [u8; 4] {
    if matches!(mode, Mode::Normal) {
        return normal_over(dst, src);
    }
    if src[3] == 0 {
        return dst;
    }
    if dst[3] == 0 || (src[3] == 255 && matches!(mode, Mode::Normal)) {
        return src;
    }
    let sa = f32::from(src[3]) / 255.0;
    let da = f32::from(dst[3]) / 255.0;
    let a = sa + da * (1.0 - sa);
    if a <= 0.0 {
        return [0; 4];
    }
    let s = std::array::from_fn(|i| f32::from(src[i]) / 255.0);
    let b = std::array::from_fn(|i| f32::from(dst[i]) / 255.0);
    let mixed = match mode {
        Mode::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Mode::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Mode::Color => set_lum(s, lum(b)),
        Mode::Luminosity => set_lum(b, lum(s)),
        _ => std::array::from_fn(|i| channel(b[i], s[i], mode)),
    };
    let mut result = [0; 4];
    for i in 0..3 {
        result[i] = (((sa * (1.0 - da) * s[i] + sa * da * mixed[i] + (1.0 - sa) * da * b[i]) / a)
            .clamp(0.0, 1.0)
            * 255.0)
            .round() as u8;
    }
    result[3] = (a * 255.0).round() as u8;
    result
}

fn build_live_masks(doc: &Document) -> HashMap<String, RgbaImage> {
    fn find<'a>(layers: &'a [Layer], id: &str) -> Option<&'a Layer> {
        layers.iter().find_map(|l| {
            if l.id.eq_ignore_ascii_case(id) {
                Some(l)
            } else {
                find(&l.children, id)
            }
        })
    }
    fn parent_opacity(layers: &[Layer], id: &str, opacity: f32) -> Option<f32> {
        for layer in layers {
            if layer.id.eq_ignore_ascii_case(id) {
                return Some(opacity);
            }
            if let Some(found) =
                parent_opacity(&layer.children, id, opacity * layer.opacity.clamp(0., 1.))
            {
                return Some(found);
            }
        }
        None
    }
    let mut links = Vec::new();
    fn collect(layers: &[Layer], out: &mut Vec<(String, String)>) {
        for l in layers {
            if let Some(s) = l
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
            {
                out.push((l.id.clone(), s.to_owned()));
            }
            collect(&l.children, out)
        }
    }
    collect(&doc.layers, &mut links);
    if u64::from(doc.width) * u64::from(doc.height) * links.len() as u64 > MAX_PIXELS {
        return HashMap::new();
    }
    let mut result: HashMap<String, RgbaImage> = HashMap::new();
    for _ in 0..=links.len() {
        let mut changed = false;
        for (target, source) in &links {
            if result.contains_key(target) {
                continue;
            }
            let Some(layer) = find(&doc.layers, source) else {
                continue;
            };
            let dependency = layer
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str);
            if dependency.is_some() && !result.contains_key(&layer.id) {
                continue;
            }
            let mut copy = layer.clone();
            copy.visible = true;
            let mut plane = RgbaImage::new(doc.width, doc.height);
            let empty = HashMap::new();
            draw_layers(
                &mut plane,
                std::slice::from_ref(&copy),
                0,
                parent_opacity(&doc.layers, source, 1.).unwrap_or(1.),
                &[],
                &empty,
            );
            if let Some(parent) = result.get(&layer.id) {
                for (p, m) in plane.pixels_mut().zip(parent.pixels()) {
                    p[3] = (u16::from(p[3]) * u16::from(m[3]) / 255) as u8;
                }
            }
            result.insert(target.clone(), plane);
            changed = true;
        }
        if !changed {
            break;
        }
    }
    result
}

fn effect_mask(layer: &Layer, image: &RgbaImage) -> Option<RgbaImage> {
    let mask = layer.mask.as_ref()?;
    if layer
        .metadata
        .get("maskPlacement")
        .is_none_or(serde_json::Value::is_null)
    {
        return None;
    }
    let (w, h) = (image.width(), image.height());
    let (sx, sy) = (f64::from(layer.scale_x), f64::from(layer.scale_y));
    let angle = f64::from(layer.rotation).to_radians();
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = (
        f64::from(layer.offset_x) + f64::from(w) * sx.abs() / 2.,
        f64::from(layer.offset_y) + f64::from(h) * sy.abs() / 2.,
    );
    let sampler = crate::effects::MaskSampler::new(&layer.metadata, mask);
    Some(RgbaImage::from_fn(w, h, |x, y| {
        let dx = (f64::from(x) + 0.5 - f64::from(w) / 2.) * sx;
        let dy = (f64::from(y) + 0.5 - f64::from(h) / 2.) * sy;
        let canvas_x = cx + dx * cos - dy * sin;
        let canvas_y = cy + dx * sin + dy * cos;
        let a = (sampler.coverage(
            canvas_x,
            canvas_y,
            f64::from(x) + 0.5,
            f64::from(y) + 0.5,
            f64::from(w),
            f64::from(h),
        ) * 255.)
            .round() as u8;
        Rgba([a, a, a, 255])
    }))
}

#[cfg(test)]
mod live_mask_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn identity_fast_path_still_applies_live_mask() {
        let mut source = Layer::paint("Source", 1, 1);
        source.image = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 255])).into());
        let mut target = Layer::paint("Target", 2, 1);
        target.image = Some(RgbaImage::from_pixel(2, 1, Rgba([255, 0, 0, 255])).into());
        target.metadata["maskSourceID"] = json!(source.id.clone());
        let doc = Document {
            width: 2,
            height: 1,
            name: "clip".into(),
            background: [0; 4],
            layers: vec![source, target],
            metadata: json!({}),
        };
        assert!(validate(&doc).is_empty());
        let made = composite(&doc);
        assert_eq!(made.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(made.get_pixel(1, 0).0, [0, 0, 0, 0]);
    }
}

#[cfg(test)]
mod quality_export_tests {
    use super::*;
    use image::ImageDecoder;
    use serde_json::json;

    #[test]
    fn cancelled_export_preserves_destination_at_the_publication_boundary() {
        use std::cell::Cell;
        let temp = tempfile::tempdir().unwrap();
        let mut doc = Document::new(8, 5);
        doc.background = [35, 85, 135, 255];
        for extension in ["png", "jpg", "webp", "tiff"] {
            let path = temp.path().join(format!("previous.{extension}"));
            std::fs::write(&path, b"Keep the previous export").unwrap();
            let checkpoints = Cell::new(0);
            let result = export_with_options_checked(&doc, &path, Default::default(), || {
                checkpoints.set(checkpoints.get() + 1);
                anyhow::ensure!(checkpoints.get() < 3, "Cancelled before publication");
                Ok(())
            });
            assert!(result.is_err(), "{extension}");
            assert_eq!(
                checkpoints.get(),
                3,
                "The encoded file must reach publication"
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"Keep the previous export");
            assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".omuse-export-")
            }));
        }
    }

    #[test]
    fn cancelled_export_does_not_create_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cancelled.png");
        let cancel = ExportCancellation::new();
        assert!(cancel.cancel());
        assert!(
            export_with_options_cancellable(
                &Document::new(2, 2),
                &path,
                Default::default(),
                &cancel
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn publication_gate_distinguishes_cancelled_and_completed_exports() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("existing.png");
        let staged = temp.path().join("staged.png");
        std::fs::write(&destination, b"keep existing export").unwrap();
        std::fs::write(&staged, b"new export").unwrap();

        let cancelled = ExportCancellation::new();
        assert!(cancelled.cancel());
        assert!(cancelled.publish(&staged, &destination).is_err());
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"keep existing export"
        );
        assert!(staged.exists());
        std::fs::remove_file(&staged).unwrap();

        let published_destination = temp.path().join("published.png");
        let published_stage = temp.path().join("published-stage.png");
        std::fs::write(&published_stage, b"published export").unwrap();
        let published = ExportCancellation::new();
        published
            .publish(&published_stage, &published_destination)
            .unwrap();
        assert_eq!(
            std::fs::read(&published_destination).unwrap(),
            b"published export"
        );
        assert!(
            !published.cancel(),
            "cancellation after publication must report the saved export"
        );
    }

    fn lanczos_reference_before_axis_cache(
        image: &RgbaImage,
        u: f64,
        v: f64,
        sx: f64,
        sy: f64,
    ) -> [u8; 4] {
        fn sinc(x: f64) -> f64 {
            if x.abs() < 1e-9 {
                1.
            } else {
                let p = std::f64::consts::PI * x;
                p.sin() / p
            }
        }
        fn axis(x: f64, scale: f64) -> (i64, i64, f64) {
            let f = scale.abs().min(1.).max(3. / 32.);
            let radius = 3. / f;
            ((x - radius).floor() as i64, (x + radius).ceil() as i64, f)
        }
        let (x0, x1, xf) = axis(u, sx);
        let (y0, y1, yf) = axis(v, sy);
        let mut sum = [0.; 4];
        let mut weight = 0.;
        for y in y0..=y1 {
            let yd = (v - (y as f64 + 0.5)) * yf;
            if yd.abs() >= 3. {
                continue;
            }
            let wy = sinc(yd) * sinc(yd / 3.);
            for x in x0..=x1 {
                let xd = (u - (x as f64 + 0.5)) * xf;
                if xd.abs() >= 3. {
                    continue;
                }
                let wx = sinc(xd) * sinc(xd / 3.);
                let w = wx * wy;
                let p = image
                    .get_pixel(
                        x.clamp(0, i64::from(image.width()) - 1) as u32,
                        y.clamp(0, i64::from(image.height()) - 1) as u32,
                    )
                    .0;
                let a = f64::from(p[3]) / 255.;
                for c in 0..3 {
                    sum[c] += f64::from(p[c]) * a * w;
                }
                sum[3] += a * w;
                weight += w;
            }
        }
        if weight.abs() < 1e-9 || sum[3] <= 0. {
            return [0; 4];
        }
        [
            (sum[0] / sum[3]).clamp(0., 255.).round() as u8,
            (sum[1] / sum[3]).clamp(0., 255.).round() as u8,
            (sum[2] / sum[3]).clamp(0., 255.).round() as u8,
            (sum[3] / weight * 255.).clamp(0., 255.).round() as u8,
        ]
    }

    #[test]
    fn cached_lanczos_matches_original_at_fractional_edges_scales_and_alpha() {
        let scales = [
            (1., 1.),
            (0.85, 0.73),
            (0.5, 2.),
            (-0.85, 0.73),
            (0.125, 0.25),
            (3. / 32., 3. / 32.),
            (0.001, -0.07),
            (0., 0.5),
            (2.5, -3.),
        ];
        for (width, height) in [(1, 1), (8, 7), (19, 11)] {
            let image = RgbaImage::from_fn(width, height, |x, y| {
                Rgba([
                    ((x * 73 + y * 11) % 256) as u8,
                    ((x * 29 + y * 191) % 256) as u8,
                    ((x * 137 + y * 43) % 256) as u8,
                    [0, 1, 37, 128, 254, 255][((x + 3 * y) % 6) as usize],
                ])
            });
            let positions = [
                (-0.25, -0.125),
                (0., 0.),
                (0.1, 0.9),
                (0.5, 0.5),
                (1.25, 2.375),
                (width as f64 * 0.413, height as f64 * 0.729),
                (width as f64 - 0.5, height as f64 - 0.5),
                (width as f64 - 0.01, height as f64 - 0.125),
                (width as f64 + 0.25, height as f64 + 0.125),
            ];
            for (sx, sy) in scales {
                for (u, v) in positions {
                    assert_eq!(
                        sample_lanczos(&image, u, v, sx, sy),
                        lanczos_reference_before_axis_cache(&image, u, v, sx, sy),
                        "image={width}x{height} position=({u},{v}) scale=({sx},{sy})"
                    );
                }
            }
        }
    }
    #[test]
    fn identity_high_quality_with_opacity_and_mask_matches_exact_sampling() {
        let mut layer = Layer::paint("identity", 8, 7);
        layer.image = Some(
            RgbaImage::from_fn(8, 7, |x, y| {
                Rgba([(x * 31) as u8, (y * 37) as u8, 193, ((x + y) * 17) as u8])
            })
            .into(),
        );
        layer.opacity = 0.63;
        layer.mask = Some(RgbaImage::from_pixel(8, 7, Rgba([255; 4])).into());
        layer.metadata = json!({"transform":{"sampling":"High quality"}});
        let mut doc = Document::new(8, 7);
        doc.layers = vec![layer];
        let high_quality = composite(&doc);
        doc.layers[0].metadata = json!({"transform":{"sampling":"Nearest"}});
        assert_eq!(high_quality, composite(&doc));
    }

    #[test]
    fn identity_opacity_fast_path_matches_general_masked_path() {
        let image = RgbaImage::from_fn(19, 13, |x, y| {
            Rgba([
                ((x * 73 + y * 11) % 256) as u8,
                ((x * 29 + y * 191) % 256) as u8,
                ((x * 137 + y * 43) % 256) as u8,
                [0, 1, 37, 128, 254, 255][((x + 3 * y) % 6) as usize],
            ])
        });
        let background = RgbaImage::from_fn(23, 17, |x, y| {
            Rgba([
                ((x * 19 + y * 31) % 256) as u8,
                ((x * 47 + y * 7) % 256) as u8,
                113,
                [0, 63, 191, 255][((x + y) % 4) as usize],
            ])
        });
        let full_live_mask = RgbaImage::from_pixel(23, 17, Rgba([0, 0, 0, 255]));
        for (offset_x, offset_y) in [(-7.0, -3.0), (0.0, 0.0), (8.0, 6.0)] {
            for (opacity, inherited) in [(1.0, 1.0), (0.63, 1.0), (0.73, 0.41)] {
                let mut layer = Layer::paint("identity", image.width(), image.height());
                layer.offset_x = offset_x;
                layer.offset_y = offset_y;
                layer.opacity = opacity;

                let mut fast = background.clone();
                draw_image_with_workers(&mut fast, &image, &layer, inherited, &[], None, 1);

                // A full-alpha live mask contributes exactly 1.0 opacity while
                // deliberately selecting the general transform loop.
                let mut general = background.clone();
                draw_image_with_workers(
                    &mut general,
                    &image,
                    &layer,
                    inherited,
                    &[],
                    Some(&full_live_mask),
                    1,
                );
                assert_eq!(
                    fast, general,
                    "offset=({offset_x},{offset_y}) opacity={opacity} inherited={inherited}"
                );
            }
        }
    }

    #[test]
    fn small_row_workloads_remain_serial_and_workers_are_bounded() {
        assert_eq!(row_worker_count(128, 128, true, 4), 1);
        assert_eq!(row_worker_count(512, 384, false, 4), 1);
        assert_eq!(row_worker_count(512, 384, true, 32), 4);
        assert_eq!(row_worker_count(8192, 40, true, 4), 1);
        assert_eq!(row_worker_count(512, 384, true, 1), 1);
    }

    #[test]
    fn parallel_photo_rows_match_serial_and_general_render_at_clipped_edges() {
        let (width, height) = (1088, 1024);
        let image = RgbaImage::from_fn(width, height, |x, y| {
            let alpha = match y % 3 {
                0 => 0,
                1 => 255,
                _ => ((x * 17 + y * 31) % 256) as u8,
            };
            Rgba([(x % 256) as u8, (y % 256) as u8, 193, alpha])
        });
        let background = RgbaImage::from_fn(width, height, |x, y| {
            Rgba([73, 121, ((x + y) % 256) as u8, ((x * 3 + y) % 256) as u8])
        });
        let live_mask = RgbaImage::from_pixel(width, height, Rgba([255; 4]));
        for (x, y, opacity) in [(-5.0, 7.0, 1.0), (7.0, -5.0, 0.63)] {
            let mut layer = Layer::paint("Photo", width, height);
            layer.offset_x = x;
            layer.offset_y = y;
            layer.opacity = opacity;
            let clipped_width = width - x.abs() as u32;
            let clipped_height = height - y.abs() as u32;
            assert_eq!(row_worker_count(clipped_width, clipped_height, false, 4), 4);
            let mut serial = background.clone();
            let mut parallel = background.clone();
            let mut general = background.clone();
            draw_image_with_workers(&mut serial, &image, &layer, 1.0, &[], None, 1);
            draw_image_with_workers(&mut parallel, &image, &layer, 1.0, &[], None, 4);
            draw_image_with_workers(&mut general, &image, &layer, 1.0, &[], Some(&live_mask), 1);
            assert_eq!(
                parallel, serial,
                "parallel photo rows at ({x},{y}), opacity {opacity}"
            );
            assert_eq!(
                parallel, general,
                "photo fast path at ({x},{y}), opacity {opacity}"
            );
        }
    }

    #[test]
    fn parallel_rows_match_serial_complete_image_with_blends_transforms_and_masks() {
        let (width, height) = (512, 384);
        let image = RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                ((x * 29 + y * 73) % 256) as u8,
                ((x * 83 + y * 17) % 256) as u8,
                ((x * 7 + y * 131) % 256) as u8,
                [0, 17, 91, 128, 231, 255][((x + 3 * y) % 6) as usize],
            ])
        });
        let mut layer = Layer::paint("transformed", width, height);
        layer.image = Some(image.clone().into());
        layer.offset_x = -17.25;
        layer.offset_y = -13.75;
        layer.scale_x = -1.1;
        layer.scale_y = 1.07;
        layer.rotation = 11.5;
        layer.opacity = 0.73;
        layer.metadata = json!({"transform":{"sampling":"High quality"},"maskPlacement":{
            "origin":[3.0,4.0],"size":[512.0,384.0],"rotation":-7.0,"sampling":"Smooth"
        }});
        layer.mask = Some(
            RgbaImage::from_fn(17, 13, |x, y| {
                let v = ((x * 13 + y * 19) % 256) as u8;
                Rgba([v, v, v, 255])
            })
            .into(),
        );
        let mut folder = Layer::group("folder");
        folder.mask = Some(
            RgbaImage::from_fn(23, 19, |x, y| {
                let v = ((x * 7 + y * 11) % 256) as u8;
                Rgba([v, v, v, 255])
            })
            .into(),
        );
        folder.offset_x = -5.0;
        folder.offset_y = 2.0;
        folder.rotation = -5.0;
        folder.metadata =
            json!({"isGroup":true,"transform":{"size":[512.0,384.0],"sampling":"Smooth"}});
        let folders = [FolderSampler::new(&folder).unwrap()];
        let live = RgbaImage::from_fn(width, height, |x, y| {
            Rgba([0, 0, 0, ((x * 3 + y * 5) % 256) as u8])
        });
        let background = RgbaImage::from_fn(width, height, |x, y| {
            Rgba([57, ((x + y) % 256) as u8, 219, ((x + 2 * y) % 256) as u8])
        });
        let mut serial = background.clone();
        let mut parallel = background;
        for blend in ["Normal", "Multiply", "Luminosity"] {
            layer.blend_mode = blend.into();
            draw_image_with_workers(&mut serial, &image, &layer, 0.81, &folders, Some(&live), 1);
            draw_image_with_workers(
                &mut parallel,
                &image,
                &layer,
                0.81,
                &folders,
                Some(&live),
                4,
            );
            assert_eq!(serial, parallel, "blend {blend}");
        }
        assert!(
            serial.pixels().any(|p| p[0] != 57),
            "fixture must change pixels"
        );
    }

    #[test]
    fn lanczos_is_premultiplied_and_scale_aware() {
        let image = RgbaImage::from_fn(8, 1, |x, _| {
            if x < 4 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 0])
            }
        });
        let edge = sample_lanczos(&image, 4., 0.5, 0.25, 1.);
        assert!(edge[0] > 240 && edge[2] < 15, "{edge:?}");
        let checker = RgbaImage::from_fn(32, 1, |x, _| {
            if x % 2 == 0 {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let reduced = sample_lanczos(&checker, 16., 0.5, 0.125, 1.);
        assert!((105..=150).contains(&reduced[0]), "{reduced:?}");
    }
    #[test]
    fn canonical_high_quality_is_used_by_composite() {
        let mut layer = Layer::paint("checker", 32, 1);
        layer.image = Some(
            RgbaImage::from_fn(32, 1, |x, _| {
                if x % 2 == 0 {
                    Rgba([255; 4])
                } else {
                    Rgba([0, 0, 0, 255])
                }
            })
            .into(),
        );
        layer.scale_x = 0.125;
        layer.metadata = json!({"transform":{"sampling":"High quality"}});
        let doc = Document {
            width: 4,
            height: 1,
            name: "hq".into(),
            background: [0; 4],
            layers: vec![layer],
            metadata: json!({}),
        };
        assert!(validate(&doc).is_empty(), "{:?}", validate(&doc));
        let made = composite(&doc);
        assert!((105..=150).contains(&made.get_pixel(2, 0)[0]));
    }
    #[test]
    fn missing_sampling_matches_saved_high_quality_default() {
        let checker = RgbaImage::from_fn(32, 1, |x, _| {
            if x % 2 == 0 {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let make = |metadata| {
            let mut layer = Layer::paint("checker", 32, 1);
            layer.image = Some(checker.clone().into());
            layer.scale_x = 0.125;
            layer.metadata = metadata;
            Document {
                width: 4,
                height: 1,
                name: "round trip".into(),
                background: [0; 4],
                layers: vec![layer],
                metadata: json!({}),
            }
        };
        let before_save = composite(&make(json!({})));
        let after_reopen = composite(&make(json!({"transform":{"sampling":"High quality"}})));
        assert_eq!(before_save, after_reopen);
    }
    #[test]
    fn smooth_group_mask_is_sampled_in_folder_space() {
        let mut child = Layer::paint("child", 4, 1);
        child.image = Some(RgbaImage::from_pixel(4, 1, Rgba([255; 4])).into());
        let mut group = Layer::group("group");
        group.mask = Some(
            RgbaImage::from_fn(2, 1, |x, _| {
                if x == 0 {
                    Rgba([0, 0, 0, 255])
                } else {
                    Rgba([255; 4])
                }
            })
            .into(),
        );
        group.children.push(child);
        group.metadata = json!({"isGroup":true,"transform":{"size":[4,1],"sampling":"Smooth"}});
        let doc = Document {
            width: 4,
            height: 1,
            name: "folder mask".into(),
            background: [0; 4],
            layers: vec![group],
            metadata: json!({}),
        };
        let made = composite(&doc);
        assert!((45..=85).contains(&made.get_pixel(1, 0)[3]), "{made:?}");
    }
    #[test]
    fn mask_work_is_bounded_without_allocating_a_large_canvas() {
        let mut adjustment = Layer::paint("masked adjustment", 1, 1);
        adjustment.image = None;
        adjustment.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255; 4])).into());
        adjustment.metadata = json!({
            "adjustment": {"kind":"Invert"},
            "transform": {"sampling":"High quality"}
        });
        let large = Document {
            width: 10_000,
            height: 10_000,
            name: "bounded".into(),
            background: [0; 4],
            layers: vec![adjustment.clone()],
            metadata: json!({}),
        };
        assert!(
            validate(&large)
                .iter()
                .any(|e| e.contains("resampling exceed"))
        );
        let small = Document {
            width: 100,
            height: 100,
            ..large
        };
        assert!(validate(&small).is_empty(), "{:?}", validate(&small));
    }
    #[test]
    fn jpeg_preview_matches_export_bytes_with_alpha_matte_quality_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preview.jpg");
        let mut doc = Document::new(19, 13);
        doc.layers[0].image = Some(
            RgbaImage::from_fn(19, 13, |x, y| {
                Rgba([
                    (x * 13) as u8,
                    (y * 19) as u8,
                    ((x + y) * 7) as u8,
                    ((x * 17 + y * 23) % 256) as u8,
                ])
            })
            .into(),
        );
        for resolution in [json!(300.0), json!("invalid"), json!(0), json!(9601)] {
            doc.metadata["resolution"] = resolution;
            for quality in [1, 73, 100] {
                let options = ExportOptions {
                    jpeg_quality: quality,
                    matte: [15, 74, 139],
                };
                let preview = encode_jpeg(&doc, options).unwrap();
                export_with_options(&doc, &path, options).unwrap();
                assert_eq!(preview, std::fs::read(&path).unwrap());
                let jfif = preview.windows(5).position(|x| x == b"JFIF\0").unwrap();
                assert_eq!(preview[jfif + 7], 1);
                let expected_dpi = if doc.metadata["resolution"] == json!(300.0) {
                    300
                } else {
                    72
                };
                assert_eq!(
                    u16::from_be_bytes([preview[jfif + 8], preview[jfif + 9]]),
                    expected_dpi
                );
                let mut decoder = image::ImageReader::new(std::io::Cursor::new(&preview))
                    .with_guessed_format()
                    .unwrap()
                    .into_decoder()
                    .unwrap();
                assert!(
                    decoder
                        .icc_profile()
                        .unwrap()
                        .is_some_and(|p| p.len() > 100)
                );
                let decoded = image::load_from_memory(&preview).unwrap();
                assert_eq!((decoded.width(), decoded.height()), (19, 13));
            }
        }
    }

    #[test]
    fn jpeg_preview_preflight_rejects_large_invalid_and_unsupported_documents() {
        let mut doc = Document::new(1, 1);
        for jpeg_quality in [0, 101, 255] {
            assert!(
                encode_jpeg(
                    &doc,
                    ExportOptions {
                        jpeg_quality,
                        ..ExportOptions::default()
                    }
                )
                .is_err()
            );
        }
        doc.width = 4001;
        doc.height = 4000;
        assert!(
            encode_jpeg(&doc, ExportOptions::default())
                .unwrap_err()
                .to_string()
                .contains("16 megapixels")
        );
        doc.width = 0;
        assert!(encode_jpeg(&doc, ExportOptions::default()).is_err());
        doc.width = 1;
        doc.height = 1;
        doc.layers[0].blend_mode = "Unknown blend".into();
        assert!(encode_jpeg(&doc, ExportOptions::default()).is_err());
    }

    #[test]
    fn png_jpeg_tiff_keep_resolution_and_icc() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(2, 2);
        doc.metadata["resolution"] = json!(300.0);
        for ext in ["png", "jpg", "tiff"] {
            export(&doc, &dir.path().join(format!("dpi.{ext}"))).unwrap();
        }
        let png_file = std::fs::File::open(dir.path().join("dpi.png")).unwrap();
        let png = png::Decoder::new(std::io::BufReader::new(png_file))
            .read_info()
            .unwrap();
        let dims = png.info().pixel_dims.unwrap();
        assert_eq!(dims.unit, png::Unit::Meter);
        assert!((dims.xppu as i64 - 11811).abs() <= 1);
        assert!(
            png.info()
                .icc_profile
                .as_ref()
                .is_some_and(|p| p.len() > 100)
        );
        let jpg = std::fs::read(dir.path().join("dpi.jpg")).unwrap();
        let jfif = jpg.windows(5).position(|x| x == b"JFIF\0").unwrap();
        assert_eq!(jpg[jfif + 7], 1);
        assert_eq!(u16::from_be_bytes([jpg[jfif + 8], jpg[jfif + 9]]), 300);
        let mut jd = image::ImageReader::open(dir.path().join("dpi.jpg"))
            .unwrap()
            .into_decoder()
            .unwrap();
        assert!(jd.icc_profile().unwrap().is_some_and(|p| p.len() > 100));
        let file = std::fs::File::open(dir.path().join("dpi.tiff")).unwrap();
        let mut td = tiff::decoder::Decoder::new(std::io::BufReader::new(file)).unwrap();
        let resolution = match td.get_tag(tiff::tags::Tag::XResolution).unwrap() {
            tiff::decoder::ifd::Value::Rational(n, d) => n as f64 / d as f64,
            value => panic!("unexpected TIFF XResolution value: {value:?}"),
        };
        assert!((resolution - 300.).abs() < 0.01);
        assert_eq!(
            td.get_tag_unsigned::<u16>(tiff::tags::Tag::ResolutionUnit)
                .unwrap(),
            2
        );
        assert!(
            td.get_tag_u8_vec(tiff::tags::Tag::IccProfile)
                .unwrap()
                .len()
                > 100
        );
    }
}
