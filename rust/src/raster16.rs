//! Full precision document compositing and export.
//!
//! This renderer deliberately has its own small compositor.  The established
//! renderer is a useful display path, but converting an advanced source to an
//! `RgbaImage` before export would lose the very data this module is intended
//! to preserve. RGB arithmetic follows the existing encoded-sRGB blend oracle,
//! with premultiplied-alpha equations; the public image is straight-alpha
//! sRGB `u16`.

#[allow(unused_imports)]
use super::*;
use crate::{
    advanced::MAX_ADVANCED_PIXELS,
    effects::MaskSampler,
    model::{Document, Layer},
    precision::{Rgba16Image, WorkingSpace},
};
use anyhow::{Context, Result, bail, ensure};
use image::{ImageBuffer, Rgba};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_DEPTH: usize = 64;
const MAX_LANCZOS_TAPS: usize = 66;

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Export cancelled");
    Ok(())
}

/// Composite a document without passing advanced sources through 8-bit
/// storage. Ordinary raster layers are promoted exactly (`byte * 257`).
pub fn composite16(doc: &Document) -> Result<Rgba16Image> {
    composite16_cancellable(doc, &AtomicBool::new(false))
}

fn composite16_cancellable(doc: &Document, cancel: &AtomicBool) -> Result<Rgba16Image> {
    check_cancel(cancel)?;
    ensure!(
        crate::model::valid_dimensions(doc.width, doc.height),
        "Invalid document dimensions"
    );
    ensure!(
        u64::from(doc.width) * u64::from(doc.height) <= MAX_ADVANCED_PIXELS,
        "16-bit compositing is limited to 16 megapixels"
    );
    validate_document(doc)?;
    check_cancel(cancel)?;
    let live = build_live_masks(doc, cancel)?;
    let mut output = ImageBuffer::from_pixel(doc.width, doc.height, Rgba(to_u16(doc.background)));
    render_layers(&mut output, &doc.layers, 0, 1.0, &[], &live, cancel)?;
    check_cancel(cancel)?;
    Ok(output)
}

/// Export full precision RGBA PNG or TIFF.  The temporary sibling is fsynced
/// before publication, matching the legacy export's crash-safe contract.
pub fn export16(doc: &Document, path: &Path) -> Result<()> {
    export16_cancellable(doc, path, &std::sync::atomic::AtomicBool::new(false))
}

/// Cancellation before publication preserves any existing destination and
/// removes the temporary output. Publication is the transaction boundary.
pub fn export16_cancellable(
    doc: &Document,
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    check_cancel(cancel)?;
    let image = composite16_cancellable(doc, cancel)?;
    check_cancel(cancel)?;
    let extension = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        matches!(extension.as_str(), "png" | "tif" | "tiff"),
        "16-bit export supports PNG and TIFF"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = parent.join(format!(".omuse-export16-{}.tmp", uuid::Uuid::new_v4()));
    let outcome = (|| -> Result<()> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let profile = crate::color_management::srgb_profile()?;
        let resolution = doc
            .metadata
            .get("resolution")
            .and_then(Value::as_f64)
            .filter(|x| x.is_finite() && (1.0..=9600.0).contains(x))
            .unwrap_or(72.0);
        if extension == "png" {
            let mut info = png::Info::with_size(doc.width, doc.height);
            info.color_type = png::ColorType::Rgba;
            info.bit_depth = png::BitDepth::Sixteen;
            info.icc_profile = Some(std::borrow::Cow::Owned(profile));
            info.pixel_dims = Some(png::PixelDimensions {
                xppu: (resolution / 0.0254).round().clamp(1., u32::MAX as f64) as u32,
                yppu: (resolution / 0.0254).round().clamp(1., u32::MAX as f64) as u32,
                unit: png::Unit::Meter,
            });
            let mut encoder = png::Encoder::with_info(&mut output, info)?;
            // Use fast lossless compression for large photographic exports.
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder.write_header()?;
            // PNG stores sixteen-bit samples in network byte order.  The
            // image crate's `u16` backing storage is native-endian.
            let mut bytes = Vec::with_capacity(image.as_raw().len() * 2);
            for row in image.as_raw().chunks(image.width() as usize * 4) {
                check_cancel(cancel)?;
                for value in row {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            writer.write_image_data(&bytes)?;
        } else {
            let mut encoder = tiff::encoder::TiffEncoder::new(&mut output)?;
            let mut encoded =
                encoder.new_image::<tiff::encoder::colortype::RGBA16>(doc.width, doc.height)?;
            encoded
                .encoder()
                .write_tag(tiff::tags::Tag::IccProfile, profile.as_slice())?;
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
            encoded.write_data(image.as_raw())?;
        }
        output.sync_all()?;
        check_cancel(cancel)?;
        std::fs::rename(&temporary, path).context("Publishing 16-bit export")?;
        crate::durable_fs::sync_path(parent)
            .context("16-bit export published; directory sync failed")?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    outcome
}

fn validate_document(doc: &Document) -> Result<()> {
    fn visit(layers: &[Layer], depth: usize) -> Result<()> {
        ensure!(
            depth < MAX_DEPTH || layers.is_empty(),
            "Layer nesting exceeds 64 levels"
        );
        for layer in layers {
            ensure!(
                [
                    layer.offset_x,
                    layer.offset_y,
                    layer.rotation,
                    layer.scale_x,
                    layer.scale_y,
                    layer.opacity
                ]
                .iter()
                .all(|v| v.is_finite()),
                "Layer {} has a non-finite transform or opacity",
                layer.name
            );
            ensure!(
                layer.scale_x != 0.0 && layer.scale_y != 0.0,
                "Layer {} has zero scale",
                layer.name
            );
            ensure!(
                mode(&layer.blend_mode).is_some(),
                "Unsupported blend mode on {}",
                layer.name
            );
            ensure!(
                !(layer.image.is_some() && !layer.children.is_empty()),
                "Layer {} has both pixels and children",
                layer.name
            );
            if layer
                .metadata
                .get("adjustment")
                .is_some_and(|v| !v.is_null())
            {
                bail!(
                    "16-bit export does not support adjustment layers ({})",
                    layer.name
                );
            }
            if layer.metadata.get("effects").is_some_and(|v| !v.is_null()) {
                bail!(
                    "16-bit export does not support layer effects ({})",
                    layer.name
                );
            }
            if layer.mask.is_some() {
                crate::effects::validate_mask_metadata(&layer.metadata)
                    .with_context(|| format!("Invalid mask on {}", layer.name))?;
            }
            if let Some(state) = &layer.advanced {
                state
                    .validate()
                    .with_context(|| format!("Invalid advanced layer {}", layer.name))?;
            }
            if let Some(source) = layer.metadata.get("maskSourceID").and_then(Value::as_str) {
                ensure!(
                    !source.is_empty(),
                    "Empty live mask source on {}",
                    layer.name
                );
            }
            visit(&layer.children, depth + 1)?;
        }
        Ok(())
    }
    visit(&doc.layers, 0)?;
    // A source must be a paint layer.  Reject cycles and missing IDs before
    // rendering so a bad live mask cannot result in a partial export.
    let mut links = Vec::<(String, String)>::new();
    fn collect(layers: &[Layer], out: &mut Vec<(String, String)>) {
        for l in layers {
            if let Some(s) = l.metadata.get("maskSourceID").and_then(Value::as_str) {
                out.push((l.id.clone(), s.to_owned()));
            }
            collect(&l.children, out);
        }
    }
    collect(&doc.layers, &mut links);
    ensure!(
        u64::from(doc.width)
            .saturating_mul(u64::from(doc.height))
            .saturating_mul(links.len() as u64)
            <= crate::model::MAX_PIXELS,
        "Live mask dependency surfaces exceed 16-bit renderer memory limit"
    );
    for (target, source) in &links {
        let source_layer = doc
            .find_layer(source)
            .ok_or_else(|| anyhow::anyhow!("Missing live mask source {source} for {target}"))?;
        ensure!(
            !source_layer.is_group() && source_layer.image.is_some()
                || source_layer.advanced.is_some(),
            "Live mask source {source} must be an image layer"
        );
        let mut seen = std::collections::HashSet::new();
        let mut current = target.as_str();
        while let Some(next) = links
            .iter()
            .find(|(id, _)| id == current)
            .map(|(_, s)| s.as_str())
        {
            ensure!(
                seen.insert(current) && seen.len() <= 256,
                "Live mask dependency cycle or excessive depth involving {target}"
            );
            current = next;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct FolderMask<'a> {
    sampler: MaskSampler<'a>,
    width: f64,
    height: f64,
    sx: f64,
    sy: f64,
    cx: f64,
    cy: f64,
    sin: f64,
    cos: f64,
}
impl<'a> FolderMask<'a> {
    fn new(layer: &'a Layer) -> Option<Self> {
        let image = layer.mask.as_ref()?;
        let width = f64::from(image.width());
        let height = f64::from(image.height());
        let size = layer
            .metadata
            .pointer("/transform/size")
            .and_then(Value::as_array);
        let extent_x = size
            .and_then(|x| x.first())
            .and_then(Value::as_f64)
            .unwrap_or(width);
        let extent_y = size
            .and_then(|x| x.get(1))
            .and_then(Value::as_f64)
            .unwrap_or(height);
        let sx = extent_x / width * f64::from(layer.scale_x);
        let sy = extent_y / height * f64::from(layer.scale_y);
        if !sx.is_finite() || !sy.is_finite() || sx == 0. || sy == 0. {
            return None;
        }
        let (sin, cos) = f64::from(layer.rotation).to_radians().sin_cos();
        Some(Self {
            sampler: MaskSampler::new_folder(&layer.metadata, image),
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
        self.sampler.coverage(x, y, u, v, self.width, self.height)
    }
    fn coverage_at_filter_tap(&self, x: f64, y: f64) -> f32 {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let u = (dx * self.cos + dy * self.sin) / self.sx + self.width / 2.;
        let v = (-dx * self.sin + dy * self.cos) / self.sy + self.height / 2.;
        self.sampler
            .coverage_at_filter_tap(x, y, u, v, self.width, self.height)
    }
}

fn render_layers<'a>(
    target: &mut Rgba16Image,
    layers: &'a [Layer],
    depth: usize,
    inherited_opacity: f32,
    inherited_masks: &[FolderMask<'a>],
    live: &HashMap<String, Rgba16Image>,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(depth < MAX_DEPTH, "Layer nesting exceeds 64 levels");
    let mut skip_until = 0;
    for (index, layer) in layers.iter().enumerate() {
        check_cancel(cancel)?;
        if index < skip_until {
            continue;
        }
        if !layer.visible || layer.opacity <= 0. {
            continue;
        }
        if !layer.is_group()
            && layer
                .metadata
                .get("maskSourceID")
                .is_none_or(Value::is_null)
        {
            let mut end = index + 1;
            while end < layers.len()
                && layers[end]
                    .metadata
                    .get("maskSourceID")
                    .and_then(Value::as_str)
                    == Some(layer.id.as_str())
            {
                end += 1;
            }
            if end > index + 1 {
                // A contiguous clipping stack changes the base's colour, not
                // its coverage. Rendering each child source-over against the
                // canvas would instead add the base alpha repeatedly.
                let mut base = layer.clone();
                base.blend_mode = "Normal".into();
                let blend_if = crate::advanced::layer_blend_if(&base)?;
                if blend_if.is_some() {
                    if let Some(state) = base.advanced.as_mut() {
                        std::sync::Arc::make_mut(state).recipe.blend_if = None;
                    }
                    if let Some(metadata) = base.metadata.as_object_mut() {
                        metadata.remove(crate::advanced::RASTER_BLEND_IF_KEY);
                    }
                }
                let mut plane =
                    ImageBuffer::from_pixel(target.width(), target.height(), Rgba([0; 4]));
                render_layers(
                    &mut plane,
                    std::slice::from_ref(&base),
                    depth,
                    inherited_opacity,
                    inherited_masks,
                    live,
                    cancel,
                )?;
                let mut alpha =
                    Vec::with_capacity(plane.width() as usize * plane.height() as usize);
                for y in 0..plane.height() {
                    check_cancel(cancel)?;
                    alpha.extend((0..plane.width()).map(|x| plane.get_pixel(x, y)[3]));
                }
                // Evaluate the base's tonal cutout against its original colour
                // and the real backdrop, before the clipped colours replace it.
                let tonal_alpha: Option<Vec<f32>> = if let Some(settings) = blend_if.as_ref() {
                    let mut values = Vec::with_capacity(alpha.len());
                    for y in 0..plane.height() {
                        check_cancel(cancel)?;
                        values.extend((0..plane.width()).map(|x| {
                            let source = plane.get_pixel(x, y);
                            let backdrop = target.get_pixel(x, y);
                            crate::advanced_ops::blend_if_coverage_normalized(
                                source.0.map(|value| f32::from(value) / 65_535.),
                                backdrop.0.map(|value| f32::from(value) / 65_535.),
                                settings,
                            )
                        }));
                    }
                    Some(values)
                } else {
                    None
                };
                for y in 0..plane.height() {
                    check_cancel(cancel)?;
                    for x in 0..plane.width() {
                        plane.get_pixel_mut(x, y)[3] = 65_535;
                    }
                }
                for child in &layers[index + 1..end] {
                    let mut child = child.clone();
                    if let Some(metadata) = child.metadata.as_object_mut() {
                        metadata.remove("maskSourceID");
                    }
                    render_layers(
                        &mut plane,
                        std::slice::from_ref(&child),
                        depth,
                        inherited_opacity,
                        &[],
                        live,
                        cancel,
                    )?;
                }
                let blend = mode(&layer.blend_mode).unwrap_or(Mode::Normal);
                for y in 0..target.height() {
                    check_cancel(cancel)?;
                    for x in 0..target.width() {
                        let index = y as usize * target.width() as usize + x as usize;
                        let mut pixel = plane.get_pixel(x, y).0;
                        pixel[3] = (f32::from(alpha[index])
                            * tonal_alpha.as_ref().map_or(1., |values| values[index]))
                        .round() as u16;
                        let destination = target.get_pixel_mut(x, y);
                        destination.0 =
                            from_linear(over(to_linear(destination.0), to_linear(pixel), blend));
                    }
                }
                skip_until = end;
                continue;
            }
        }
        if layer.is_group() {
            ensure!(
                matches!(mode(&layer.blend_mode), Some(Mode::Normal)),
                "16-bit group blending is unsupported for {}",
                layer.name
            );
            let mut masks = inherited_masks.to_vec();
            if mask_enabled(layer) {
                if let Some(mask) = FolderMask::new(layer) {
                    masks.push(mask);
                }
            }
            render_layers(
                target,
                &layer.children,
                depth + 1,
                inherited_opacity * layer.opacity.clamp(0., 1.),
                &masks,
                live,
                cancel,
            )?;
        } else if let Some(image) = source_image(layer, cancel)? {
            // A clipped group can contain separately linked descendants. Keep
            // their surfaces available, but only apply a link when it remains
            // declared on this layer (the clipping stack clears its own link).
            let live_mask = layer
                .metadata
                .get("maskSourceID")
                .and_then(Value::as_str)
                .and_then(|_| live.get(&layer.id));
            draw_image(
                target,
                &image,
                layer,
                inherited_opacity,
                inherited_masks,
                live_mask,
                cancel,
            )?;
        }
    }
    Ok(())
}

fn source_image(layer: &Layer, cancel: &AtomicBool) -> Result<Option<Rgba16Image>> {
    if let Some(state) = &layer.advanced {
        check_cancel(cancel)?;
        let converted;
        let source = if state.recipe.working_space == WorkingSpace::Srgb {
            state.result.as_ref()
        } else {
            converted = state
                .result
                .converted_working_space_with_cancel(WorkingSpace::Srgb, || {
                    cancel.load(Ordering::Relaxed)
                })?;
            &converted
        };
        let (width, height) = source.dimensions();
        let mut image = Rgba16Image::new(width, height);
        for y in 0..height {
            check_cancel(cancel)?;
            for x in 0..width {
                image.put_pixel(x, y, Rgba(source.get_pixel(x, y).0));
            }
        }
        return Ok(Some(image));
    }
    let Some(source) = layer.image.as_ref() else {
        return Ok(None);
    };
    let mut image = Rgba16Image::new(source.width(), source.height());
    for y in 0..source.height() {
        check_cancel(cancel)?;
        for x in 0..source.width() {
            image.put_pixel(x, y, Rgba(to_u16(source.get_pixel(x, y).0)));
        }
    }
    Ok(Some(image))
}

fn draw_image(
    target: &mut Rgba16Image,
    source: &Rgba16Image,
    layer: &Layer,
    inherited: f32,
    folders: &[FolderMask<'_>],
    live: Option<&Rgba16Image>,
    cancel: &AtomicBool,
) -> Result<()> {
    let w = f64::from(source.width());
    let h = f64::from(source.height());
    let sx = f64::from(layer.scale_x);
    let sy = f64::from(layer.scale_y);
    let angle = f64::from(layer.rotation).rem_euclid(360.).to_radians();
    let (sin, cos) = angle.sin_cos();
    let cx = f64::from(layer.offset_x) + w * sx.abs() * 0.5;
    let cy = f64::from(layer.offset_y) + h * sy.abs() * 0.5;
    let ex = (w * sx * cos).abs() * 0.5 + (h * sy * sin).abs() * 0.5;
    let ey = (w * sx * sin).abs() * 0.5 + (h * sy * cos).abs() * 0.5;
    let x0 = (cx - ex).floor().max(0.).min(f64::from(target.width())) as u32;
    let y0 = (cy - ey).floor().max(0.).min(f64::from(target.height())) as u32;
    let x1 = (cx + ex).ceil().max(0.).min(f64::from(target.width())) as u32;
    let y1 = (cy + ey).ceil().max(0.).min(f64::from(target.height())) as u32;
    let sampling = layer
        .metadata
        .pointer("/transform/sampling")
        .and_then(Value::as_str);
    let smooth = !(sx == 1.0
        && sy == 1.0
        && angle == 0.0
        && layer.offset_x.fract() == 0.0
        && layer.offset_y.fract() == 0.0)
        && sampling != Some("Nearest");
    let high_quality = smooth && sampling.unwrap_or("High quality") == "High quality";
    let mask = mask_enabled(layer)
        .then(|| layer.mask.as_ref())
        .flatten()
        .map(|m| MaskSampler::new(&layer.metadata, m));
    // For a reduced layer, filtering colour and mask independently and then
    // multiplying their averages loses their correlation. Apply local and
    // inherited masks to each premultiplied source tap instead. Identity,
    // Nearest, and Smooth retain their established canvas semantics.
    let masks_in_filter = high_quality && (mask.is_some() || !folders.is_empty());
    let mode = mode(&layer.blend_mode).unwrap_or(Mode::Normal);
    let blend_if = crate::advanced::layer_blend_if(layer).ok().flatten();
    for y in y0..y1 {
        check_cancel(cancel)?;
        for x in x0..x1 {
            check_cancel(cancel)?;
            let dx = f64::from(x) + 0.5 - cx;
            let dy = f64::from(y) + 0.5 - cy;
            let u = (dx * cos + dy * sin) / sx + w * 0.5;
            let v = (-dx * sin + dy * cos) / sy + h * 0.5;
            if u < -1e-9 || v < -1e-9 || u >= w || v >= h {
                continue;
            }
            let u = u.max(0.);
            let v = v.max(0.);
            let mut pixel = if masks_in_filter {
                sample_lanczos_masked(source, u, v, sx, sy, |tap_x, tap_y| {
                    let local_u = f64::from(tap_x) + 0.5;
                    let local_v = f64::from(tap_y) + 0.5;
                    let local_dx = (local_u - w * 0.5) * sx;
                    let local_dy = (local_v - h * 0.5) * sy;
                    let canvas_x = cx + local_dx * cos - local_dy * sin;
                    let canvas_y = cy + local_dx * sin + local_dy * cos;
                    let mut coverage = 1.;
                    if let Some(sampler) = &mask {
                        coverage *= f64::from(
                            sampler
                                .coverage_at_filter_tap(canvas_x, canvas_y, local_u, local_v, w, h),
                        );
                    }
                    for folder in folders {
                        coverage *= f64::from(folder.coverage_at_filter_tap(canvas_x, canvas_y));
                    }
                    coverage
                })
            } else if high_quality {
                sample_lanczos(source, u, v, sx, sy)
            } else if smooth {
                sample_linear(source, u, v)
            } else {
                sample_nearest(source, u, v)
            };
            let mut opacity = layer.opacity.clamp(0., 1.) * inherited;
            if !masks_in_filter {
                if let Some(sampler) = &mask {
                    opacity *= sampler.coverage(f64::from(x) + 0.5, f64::from(y) + 0.5, u, v, w, h);
                }
                for folder in folders {
                    opacity *= folder.coverage(f64::from(x) + 0.5, f64::from(y) + 0.5);
                }
            }
            if let Some(clip) = live {
                opacity *= f32::from(clip.get_pixel(x, y)[3]) / 65535.;
            }
            let dst = target.get_pixel(x, y).0;
            if let Some(settings) = blend_if.as_ref() {
                // Blend If is defined by the displayed encoded channels. Use
                // the actual destination pixel, including the already-rendered
                // base of a clipping group, rather than a transparent plane.
                let source_channels = [
                    pixel.rgb[0] as f32,
                    pixel.rgb[1] as f32,
                    pixel.rgb[2] as f32,
                    pixel.a as f32,
                ];
                opacity *= crate::advanced_ops::blend_if_coverage_normalized(
                    source_channels,
                    dst.map(|value| f32::from(value) / 65535.),
                    settings,
                );
            }
            pixel.a *= f64::from(opacity.clamp(0., 1.));
            if pixel.a > 0. {
                target.put_pixel(x, y, Rgba(from_linear(over(to_linear(dst), pixel, mode))));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct LinearPixel {
    rgb: [f64; 3],
    a: f64,
}
fn to_linear(p: [u16; 4]) -> LinearPixel {
    LinearPixel {
        // Existing compositor blend modes operate on encoded sRGB channels.
        // Keep that appearance for 16-bit export while still carrying the
        // channels with premultiplied-alpha equations below.
        rgb: [
            f64::from(p[0]) / 65535.,
            f64::from(p[1]) / 65535.,
            f64::from(p[2]) / 65535.,
        ],
        a: f64::from(p[3]) / 65535.,
    }
}
fn from_linear(p: LinearPixel) -> [u16; 4] {
    [
        (p.rgb[0].clamp(0., 1.) * 65535.).round() as u16,
        (p.rgb[1].clamp(0., 1.) * 65535.).round() as u16,
        (p.rgb[2].clamp(0., 1.) * 65535.).round() as u16,
        (p.a.clamp(0., 1.) * 65535.).round() as u16,
    ]
}
fn to_u16(p: [u8; 4]) -> [u16; 4] {
    [
        u16::from(p[0]) * 257,
        u16::from(p[1]) * 257,
        u16::from(p[2]) * 257,
        u16::from(p[3]) * 257,
    ]
}
fn sample_nearest(image: &Rgba16Image, u: f64, v: f64) -> LinearPixel {
    let x = ((u - 0.5).round() as i64).clamp(0, i64::from(image.width() - 1)) as u32;
    let y = ((v - 0.5).round() as i64).clamp(0, i64::from(image.height() - 1)) as u32;
    to_linear(image.get_pixel(x, y).0)
}
fn sample_linear(image: &Rgba16Image, u: f64, v: f64) -> LinearPixel {
    let px = u - 0.5;
    let py = v - 0.5;
    let ix = px.floor() as i64;
    let iy = py.floor() as i64;
    let fx = px - px.floor();
    let fy = py - py.floor();
    let mut rgb = [0.; 3];
    let mut a = 0.;
    for (ox, oy, weight) in [
        (0, 0, (1. - fx) * (1. - fy)),
        (1, 0, fx * (1. - fy)),
        (0, 1, (1. - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let x = (ix + ox).clamp(0, i64::from(image.width() - 1)) as u32;
        let y = (iy + oy).clamp(0, i64::from(image.height() - 1)) as u32;
        let p = to_linear(image.get_pixel(x, y).0);
        a += p.a * weight;
        for c in 0..3 {
            rgb[c] += p.rgb[c] * p.a * weight;
        }
    }
    if a > 0. {
        for c in &mut rgb {
            *c /= a;
        }
    }
    LinearPixel { rgb, a }
}

fn sample_lanczos(image: &Rgba16Image, u: f64, v: f64, sx: f64, sy: f64) -> LinearPixel {
    sample_lanczos_masked(image, u, v, sx, sy, |_, _| 1.)
}

fn sample_lanczos_masked(
    image: &Rgba16Image,
    u: f64,
    v: f64,
    sx: f64,
    sy: f64,
    mut coverage: impl FnMut(u32, u32) -> f64,
) -> LinearPixel {
    fn sinc(x: f64) -> f64 {
        if x.abs() < 1e-9 {
            1.
        } else {
            let p = std::f64::consts::PI * x;
            p.sin() / p
        }
    }
    fn taps(position: f64, scale: f64, limit: u32) -> ([(u32, f64); MAX_LANCZOS_TAPS], usize) {
        // Match the canvas's scale-aware Lanczos-3 footprint. The scale floor
        // caps each axis at 66 candidates, so extreme reductions remain
        // bounded while still integrating up to a 32-pixel radius.
        let factor = scale.abs().min(1.).max(3. / 32.);
        let radius = 3. / factor;
        let first = (position - radius).floor() as i64;
        let last = (position + radius).ceil() as i64;
        let mut result = [(0, 0.); MAX_LANCZOS_TAPS];
        let mut count = 0;
        for index in first..=last {
            let distance = (position - (index as f64 + 0.5)) * factor;
            if distance.abs() >= 3. {
                continue;
            }
            debug_assert!(count < MAX_LANCZOS_TAPS);
            result[count] = (
                index.clamp(0, i64::from(limit) - 1) as u32,
                sinc(distance) * sinc(distance / 3.),
            );
            count += 1;
        }
        (result, count)
    }

    let (xs, x_count) = taps(u, sx, image.width());
    let (ys, y_count) = taps(v, sy, image.height());
    let mut premultiplied = [0.; 3];
    let mut alpha_sum = 0.;
    let mut weight_sum = 0.;
    for &(y, wy) in &ys[..y_count] {
        for &(x, wx) in &xs[..x_count] {
            let weight = wx * wy;
            let pixel = to_linear(image.get_pixel(x, y).0);
            let mask = coverage(x, y).clamp(0., 1.);
            for (sum, channel) in premultiplied.iter_mut().zip(pixel.rgb) {
                *sum += channel * pixel.a * mask * weight;
            }
            alpha_sum += pixel.a * mask * weight;
            weight_sum += weight;
        }
    }
    if weight_sum.abs() < 1e-9 || alpha_sum <= 0. {
        return LinearPixel {
            rgb: [0.; 3],
            a: 0.,
        };
    }
    LinearPixel {
        rgb: premultiplied.map(|channel| (channel / alpha_sum).clamp(0., 1.)),
        a: (alpha_sum / weight_sum).clamp(0., 1.),
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
fn mode(value: &str) -> Option<Mode> {
    Some(
        match value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .as_str()
        {
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
            "linearburn" => Mode::LinearBurn,
            "vividlight" => Mode::VividLight,
            "linearlight" => Mode::LinearLight,
            "pinlight" => Mode::PinLight,
            "hardmix" => Mode::HardMix,
            "divide" => Mode::Divide,
            "hue" => Mode::Hue,
            "saturation" => Mode::Saturation,
            "color" => Mode::Color,
            "luminosity" => Mode::Luminosity,
            _ => return None,
        },
    )
}
fn channel(b: f64, s: f64, m: Mode) -> f64 {
    match m {
        Mode::Multiply => b * s,
        Mode::Screen => b + s - b * s,
        Mode::Overlay => {
            if b <= 0.5 {
                2. * b * s
            } else {
                1. - 2. * (1. - b) * (1. - s)
            }
        }
        Mode::Darken => b.min(s),
        Mode::Lighten => b.max(s),
        Mode::Dodge => {
            if b == 0. {
                0.
            } else if s == 1. {
                1.
            } else {
                (b / (1. - s)).min(1.)
            }
        }
        Mode::Burn => {
            if b == 1. {
                1.
            } else if s == 0. {
                0.
            } else {
                1. - ((1. - b) / s).min(1.)
            }
        }
        Mode::HardLight => {
            if s <= 0.5 {
                2. * b * s
            } else {
                1. - 2. * (1. - b) * (1. - s)
            }
        }
        Mode::SoftLight => {
            if s <= 0.5 {
                b - (1. - 2. * s) * b * (1. - b)
            } else {
                let d = if b <= 0.25 {
                    ((16. * b - 12.) * b + 4.) * b
                } else {
                    b.sqrt()
                };
                b + (2. * s - 1.) * (d - b)
            }
        }
        Mode::Difference => (b - s).abs(),
        Mode::Exclusion => b + s - 2. * b * s,
        Mode::LinearBurn => (b + s - 1.).max(0.),
        Mode::VividLight => {
            if s <= 0.5 {
                channel(b, 2. * s, Mode::Burn)
            } else {
                channel(b, 2. * s - 1., Mode::Dodge)
            }
        }
        Mode::LinearLight => (b + 2. * s - 1.).clamp(0., 1.),
        Mode::PinLight => {
            if s <= 0.5 {
                b.min(2. * s)
            } else {
                b.max(2. * s - 1.)
            }
        }
        Mode::HardMix => {
            if channel(b, s, Mode::VividLight) < 0.5 {
                0.
            } else {
                1.
            }
        }
        Mode::Divide => {
            if s == 0. {
                1.
            } else {
                (b / s).min(1.)
            }
        }
        Mode::Add => (b + s).min(1.),
        Mode::Subtract => (b - s).max(0.),
        _ => s,
    }
}
fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn sat(c: [f64; 3]) -> f64 {
    c.iter().copied().fold(0., f64::max) - c.iter().copied().fold(1., f64::min)
}
fn set_lum(mut c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    for x in &mut c {
        *x += d
    }
    let n = c.iter().copied().fold(f64::INFINITY, f64::min);
    let x = c.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if n < 0. {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n)
        }
    }
    if x > 1. {
        for v in &mut c {
            *v = l + (*v - l) * (1. - l) / (x - l)
        }
    }
    c
}
fn set_sat(mut c: [f64; 3], s: f64) -> [f64; 3] {
    let mut ix = [0, 1, 2];
    ix.sort_by(|a, b| c[*a].total_cmp(&c[*b]));
    let [lo, mid, hi] = ix;
    if c[hi] > c[lo] {
        c[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
        c[hi] = s
    } else {
        c[mid] = 0.;
        c[hi] = 0.
    }
    c[lo] = 0.;
    c
}
fn over(dst: LinearPixel, src: LinearPixel, m: Mode) -> LinearPixel {
    if src.a <= 0. {
        return dst;
    }
    let sa = src.a;
    let da = dst.a;
    let a = sa + da * (1. - sa);
    if a <= 0. {
        return LinearPixel {
            rgb: [0.; 3],
            a: 0.,
        };
    }
    let mixed = match m {
        Mode::Hue => set_lum(set_sat(src.rgb, sat(dst.rgb)), lum(dst.rgb)),
        Mode::Saturation => set_lum(set_sat(dst.rgb, sat(src.rgb)), lum(dst.rgb)),
        Mode::Color => set_lum(src.rgb, lum(dst.rgb)),
        Mode::Luminosity => set_lum(dst.rgb, lum(src.rgb)),
        _ => std::array::from_fn(|i| channel(dst.rgb[i], src.rgb[i], m)),
    };
    let mut rgb = [0.; 3];
    for i in 0..3 {
        rgb[i] =
            (sa * (1. - da) * src.rgb[i] + sa * da * mixed[i] + (1. - sa) * da * dst.rgb[i]) / a;
    }
    LinearPixel { rgb, a }
}

fn mask_enabled(layer: &Layer) -> bool {
    layer.metadata.get("maskEnabled").and_then(Value::as_bool) != Some(false)
}

fn build_live_masks(doc: &Document, cancel: &AtomicBool) -> Result<HashMap<String, Rgba16Image>> {
    fn parent_opacity(layers: &[Layer], id: &str, opacity: f32) -> Option<f32> {
        for layer in layers {
            if layer.id == id {
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
            if let Some(s) = l.metadata.get("maskSourceID").and_then(Value::as_str) {
                out.push((l.id.clone(), s.to_owned()))
            }
            collect(&l.children, out)
        }
    }
    collect(&doc.layers, &mut links);
    let mut result = HashMap::new();
    // Layer order is a painting order, not a dependency order: a linked mask
    // may refer to a source (or another linked source) later in the document.
    for _ in 0..=links.len() {
        check_cancel(cancel)?;
        let mut changed = false;
        for (target, source) in &links {
            check_cancel(cancel)?;
            if result.contains_key(target) {
                continue;
            }
            let layer = doc
                .find_layer(source)
                .ok_or_else(|| anyhow::anyhow!("Missing live mask source {source}"))?;
            if layer
                .metadata
                .get("maskSourceID")
                .and_then(Value::as_str)
                .is_some()
                && !result.contains_key(&layer.id)
            {
                continue;
            }
            let mut copy = layer.clone();
            copy.visible = true;
            let mut plane = ImageBuffer::from_pixel(doc.width, doc.height, Rgba([0; 4]));
            render_layers(
                &mut plane,
                std::slice::from_ref(&copy),
                0,
                parent_opacity(&doc.layers, source, 1.).unwrap_or(1.),
                &[],
                &result,
                cancel,
            )?;
            result.insert(target.clone(), plane);
            changed = true;
        }
        if !changed {
            break;
        }
    }
    ensure!(
        result.len() == links.len(),
        "Unresolved live mask dependency"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::precision::TiledImage16;
    use image::RgbaImage;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn blend_if_retains_sub_byte_tonal_boundaries() {
        use crate::advanced_ops::{BlendIf, BlendIfChannel, BlendIfRange};
        let mut document = Document::new(3, 1);
        let exact =
            Rgba16Image::from_fn(3, 1, |x, _| Rgba([16383 + x as u16, 12001, 45003, 65535]));
        let mut state =
            crate::advanced::LayerState::from_image(&RgbaImage::new(3, 1), "sub-byte").unwrap();
        state.source = std::sync::Arc::new(TiledImage16::from_rgba16(&exact).unwrap());
        state.result = state.source.clone();
        state.recipe.blend_if = Some(BlendIf {
            source_channel: BlendIfChannel::Red,
            source: BlendIfRange {
                black: 0.,
                black_split: 0.5,
                white_split: 1.,
                white: 1.,
            },
            ..Default::default()
        });
        document.layers[0].image = Some(state.proxy().unwrap().into());
        document.layers[0].advanced = Some(std::sync::Arc::new(state));
        let output = composite16(&document).unwrap();
        assert!(output.get_pixel(0, 0)[3] < output.get_pixel(1, 0)[3]);
        assert!(output.get_pixel(1, 0)[3] < output.get_pixel(2, 0)[3]);
        for (actual, source) in output.pixels().zip(exact.pixels()) {
            assert_eq!(&actual.0[..3], &source.0[..3]);
        }
    }

    #[test]
    fn regular_layers_are_exactly_promoted_and_advanced_values_survive() {
        let mut doc = Document::new(2, 1);
        let image = RgbaImage::from_vec(2, 1, vec![1, 2, 3, 128, 250, 251, 252, 255]).unwrap();
        doc.layers[0].image = Some(image.into());
        let out = composite16(&doc).unwrap();
        assert_eq!(out.get_pixel(0, 0).0, [257, 514, 771, 32896]);
        let mut advanced = crate::advanced::LayerState::from_image(
            &RgbaImage::from_pixel(2, 1, Rgba([0, 0, 0, 255])),
            "x",
        )
        .unwrap();
        let mut exact = advanced.result.to_rgba16();
        exact.put_pixel(0, 0, Rgba([12345, 23456, 34567, 45678]));
        advanced.result = std::sync::Arc::new(TiledImage16::from_rgba16(&exact).unwrap());
        doc.layers[0].advanced = Some(std::sync::Arc::new(advanced));
        let out = composite16(&doc).unwrap();
        assert!(out.get_pixel(0, 0)[0] > 256 && out.get_pixel(0, 0)[0] < 65000);
    }

    #[test]
    fn transparent_over_uses_high_precision_alpha() {
        let mut d = Document::new(1, 1);
        d.background = [10, 20, 30, 255];
        d.layers[0].image = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128])).into());
        let p = composite16(&d).unwrap().get_pixel(0, 0).0;
        assert!(p[0] > 10000 && p[0] < 65535 && p[3] == 65535);
    }

    #[test]
    fn negative_scale_flips_source_at_pixel_centres() {
        let mut d = Document::new(2, 1);
        d.layers[0].image = Some(
            RgbaImage::from_vec(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255])
                .unwrap()
                .into(),
        );
        d.layers[0].scale_x = -1.0;
        d.layers[0].metadata["transform"] = json!({"sampling": "Nearest"});
        let out = composite16(&d).unwrap();
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 65535, 65535]);
        assert_eq!(out.get_pixel(1, 0).0, [65535, 0, 0, 65535]);
    }

    #[test]
    fn blend_if_reads_the_rendered_backdrop() {
        let mut doc = Document::new(1, 1);
        doc.layers[0].image = Some(RgbaImage::from_pixel(1, 1, Rgba([128, 0, 0, 255])).into());
        let mut state = crate::advanced::LayerState::from_image(
            &RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 255])),
            "blend-if",
        )
        .unwrap();
        state.recipe.blend_if = Some(crate::advanced_ops::BlendIf {
            source_channel: crate::advanced_ops::BlendIfChannel::Blue,
            backdrop_channel: crate::advanced_ops::BlendIfChannel::Red,
            source: Default::default(),
            backdrop: crate::advanced_ops::BlendIfRange {
                black: 0.,
                black_split: 0.,
                white_split: 0.2,
                white: 0.8,
            },
        });
        let mut layer = Layer::paint("blend-if", 1, 1);
        layer.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 255])).into());
        layer.advanced = Some(std::sync::Arc::new(state));
        doc.layers.push(layer);
        let out = composite16(&doc).unwrap().get_pixel(0, 0).0;
        assert!(
            out[2] > 20_000 && out[2] < 65_000,
            "backdrop-aware coverage was not applied: {out:?}"
        );
    }

    #[test]
    fn png16_export_round_trips_and_rejects_adjustments() {
        let d = Document::new(1, 1);
        let dir = tempdir().unwrap();
        let path = dir.path().join("x.png");
        export16(&d, &path).unwrap();
        let decoded = image::open(&path).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgba16);
        let mut bad = d.clone();
        bad.layers[0].metadata["adjustment"] = json!({"kind":"Invert"});
        assert!(composite16(&bad).is_err());

        let tiff = dir.path().join("x.tiff");
        export16(&d, &tiff).unwrap();
        assert_eq!(image::open(tiff).unwrap().color(), image::ColorType::Rgba16);
    }
}
