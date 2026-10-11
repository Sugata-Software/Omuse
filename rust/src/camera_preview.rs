//! Bounded, transient previews for pointwise photo grades. The sampled pixels
//! retain the full renderer's colour/alpha math; spatial effects and complex
//! documents use the full renderer. A draft is always followed by full detail.
use super::Settings;
use crate::model::Document;
use anyhow::{Result, ensure};
use image::RgbaImage;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_DRAFT_PIXELS: u64 = 1_048_576;
const MAX_EDGE: f64 = 1536.;

/// Only one plain, canvas-sized photo is admitted. In particular, downsampling
/// a selection, a mask or a spatial operator would change its meaning.
pub fn supported(document: &Document, layer_id: &str, selected: bool, settings: &Settings) -> bool {
    if selected || document.layers.len() != 1 || super::validate(settings).is_err() {
        return false;
    }
    let layer = &document.layers[0];
    if layer.id != layer_id
        || !layer.visible
        || layer.locked
        || layer.is_group()
        || layer.advanced.is_some()
        || layer.vector_scene.is_some()
        || layer.mask.is_some()
        || layer.offset_x != 0.
        || layer.offset_y != 0.
        || layer.rotation != 0.
        || layer.scale_x != 1.
        || layer.scale_y != 1.
        || layer.blend_mode != "Normal"
        || !layer.opacity.is_finite()
        || !(0. ..=1.).contains(&layer.opacity)
        || !crate::raster::region_metadata_is_supported(layer)
        || layer
            .metadata
            .get(crate::advanced::RASTER_BLEND_IF_KEY)
            .is_some()
        || !layer
            .image
            .as_ref()
            .is_some_and(|image| image.dimensions() == (document.width, document.height))
    {
        return false;
    }
    // Whitelist per-pixel controls. New top-level settings fall back; changes
    // inside an admitted group also require a sampling-semantics review.
    let defaults = Settings::default();
    let mut rest = settings.clone();
    rest.tone_mapping = defaults.tone_mapping;
    rest.white_balance = defaults.white_balance;
    rest.temperature = defaults.temperature;
    rest.tint = defaults.tint;
    rest.exposure = defaults.exposure;
    rest.contrast = defaults.contrast;
    rest.highlights = defaults.highlights;
    rest.shadows = defaults.shadows;
    rest.whites = defaults.whites;
    rest.blacks = defaults.blacks;
    rest.vibrance = defaults.vibrance;
    rest.saturation = defaults.saturation;
    rest.curve = defaults.curve.clone();
    rest.mixer = defaults.mixer.clone();
    rest.grading = defaults.grading.clone();
    rest.calibration = defaults.calibration.clone();
    rest == defaults
}

/// `physical_scale` is canvas zoom times display scale. At 100% or above there
/// is no draft. A smaller draft must save at least half the grading work.
pub fn dimensions(width: u32, height: u32, physical_scale: f32) -> Option<(u32, u32)> {
    if !crate::model::valid_dimensions(width, height)
        || u64::from(width) * u64::from(height) < 262_144
        || u64::from(width) * u64::from(height) > 16_777_216
        || !physical_scale.is_finite()
        || physical_scale <= 0.
        || physical_scale >= 1.
    {
        return None;
    }
    let scale = f64::from(physical_scale)
        .min(MAX_EDGE / f64::from(width.max(height)))
        .min((MAX_DRAFT_PIXELS as f64 / (f64::from(width) * f64::from(height))).sqrt());
    let w = (f64::from(width) * scale).floor().max(1.) as u32;
    let h = (f64::from(height) * scale).floor().max(1.) as u32;
    (u64::from(w) * u64::from(h) * 2 <= u64::from(width) * u64::from(height)).then_some((w, h))
}

/// A single cached resolution, at most 4 MiB of new pixels. Original pixels are
/// shared with the captured edit; this is not an overall process-memory limit.
pub struct DraftSource {
    source: Arc<RgbaImage>,
    pixels: RgbaImage,
}

impl DraftSource {
    pub fn new(source: Arc<RgbaImage>, size: (u32, u32), cancel: &AtomicBool) -> Result<Self> {
        let (w, h) = size;
        ensure!(!cancel.load(Ordering::Relaxed), "Camera Raw cancelled");
        ensure!(
            w > 0
                && h > 0
                && w <= source.width()
                && h <= source.height()
                && u64::from(w) * u64::from(h) <= MAX_DRAFT_PIXELS,
            "Invalid photo preview dimensions"
        );
        let len = w as usize * h as usize * 4;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len)?;
        // Sample pixel centres without averaging before nonlinear grading.
        // This commutes with every admitted colour operation, including its
        // premultiplied-byte quantization. Fine patterns can alias in the draft;
        // the automatic full-resolution refinement replaces it.
        for y in 0..h {
            ensure!(!cancel.load(Ordering::Relaxed), "Camera Raw cancelled");
            let sy = coordinate(y, h, source.height());
            for x in 0..w {
                let sx = coordinate(x, w, source.width());
                bytes.extend_from_slice(&source.get_pixel(sx, sy).0);
            }
        }
        Ok(Self {
            source,
            pixels: RgbaImage::from_raw(w, h, bytes).expect("validated preview buffer"),
        })
    }

    pub fn matches(&self, source: &Arc<RgbaImage>, size: (u32, u32)) -> bool {
        Arc::ptr_eq(&self.source, source) && self.pixels.dimensions() == size
    }

    pub fn pixels(&self) -> &RgbaImage {
        &self.pixels
    }
}

fn coordinate(at: u32, count: u32, extent: u32) -> u32 {
    (((u64::from(at) * 2 + 1) * u64::from(extent)) / (u64::from(count) * 2)) as u32
}
