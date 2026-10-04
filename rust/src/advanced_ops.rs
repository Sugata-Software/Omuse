//! Bounded, deterministic image operations used by the editable advanced stack.
//!
//! The public model deliberately contains no editor or document references. It
//! can therefore be persisted by a caller and evaluated on a worker thread.
//! Every operation validates its complete input before changing a pixel and the
//! cancellable entry point checks an `AtomicBool` between rows and stages.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::{camera_raw, filters};

pub const MAX_OPERATION_PIXELS: u64 = 16_777_216;
pub const MAX_NODES: usize = 1_024;
pub const MAX_ID_LENGTH: usize = 128;
pub const MAX_NAME_LENGTH: usize = 256;
pub const MAX_PINS: usize = 256;
pub const MAX_MESH_ROWS: u32 = 64;
pub const MAX_MESH_COLUMNS: u32 = 64;
pub const MAX_DENOISE_RADIUS: u8 = 32;
pub const MAX_DODGE_BURN_RADIUS: u8 = 128;
pub const MAX_WARP_RADIUS: f32 = 4_096.0;
pub const MAX_CONTENT_PIXELS: u64 = 4_000_000;
pub const MAX_CONTENT_SEARCH_RADIUS: u32 = 64;
pub const MAX_CONTENT_PATCH_RADIUS: u8 = 4;
pub const MAX_CONTENT_WORK: u64 = 256_000_000;
pub const MIN_CONTENT_CANDIDATES: u64 = 16;
/// Aggregate storage permitted for persisted operation and node masks. This
/// keeps a valid 16 MP mask from being replicated across an unbounded stack.
pub const MAX_STACK_MASK_BYTES: usize = 256 * 1024 * 1024;

/// A persisted grayscale coverage plane. Values are coverage in `0..=255`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftMask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl SoftMask {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let mask = Self {
            width,
            height,
            data,
        };
        mask.validate(None)?;
        Ok(mask)
    }

    fn validate(&self, expected: Option<(u32, u32)>) -> Result<()> {
        ensure!(
            self.width > 0 && self.height > 0,
            "soft mask dimensions must be nonzero"
        );
        ensure!(
            crate::model::valid_dimensions(self.width, self.height),
            "soft mask dimensions exceed Omuse limits"
        );
        ensure!(
            u64::from(self.width) * u64::from(self.height) <= MAX_OPERATION_PIXELS,
            "soft mask exceeds the 16 megapixel operation limit"
        );
        let count = usize::try_from(u64::from(self.width) * u64::from(self.height))
            .map_err(|_| anyhow::anyhow!("soft mask is too large"))?;
        ensure!(
            self.data.len() == count,
            "soft mask data length does not match dimensions"
        );
        if let Some((width, height)) = expected {
            ensure!(
                (self.width, self.height) == (width, height),
                "soft mask dimensions must match the image"
            );
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn value(&self, x: u32, y: u32) -> f32 {
        f32::from(self.data[(y * self.width + x) as usize]) / 255.0
    }

    /// Estimated retained allocation used by this mask. Capacity is used so
    /// callers accounting cloned/persisted stacks do not undercount a Vec
    /// whose allocation is larger than its logical length.
    pub fn owned_bytes(&self) -> usize {
        self.data.capacity()
    }
}

/// One editable stack entry. `soft_mask` is evaluated as node coverage after
/// the operation has produced its candidate image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterNode {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub opacity: f32,
    pub operation: AdvancedOperation,
    pub soft_mask: Option<SoftMask>,
}

/// Operations intentionally remain typed so editors can expose their fields
/// without parsing an unbounded JSON blob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AdvancedOperation {
    Filter(filters::Filter),
    CameraRaw(camera_raw::Settings),
    Denoise { radius: u8, strength: f32 },
    BlendIf(BlendIf),
    Warp(WarpMesh),
    FrequencySeparation(FrequencySeparation),
    DodgeBurn(DodgeBurn),
    ContentAwareReplace(ContentAwareReplace),
    TargetColourUniformity(TargetColourUniformity),
    ReferenceColourMatch(crate::color_match::Settings),
}

impl AdvancedOperation {
    /// Estimated retained bytes for operation-owned mask payloads. The
    /// operation model owns these buffers even when the node has no soft mask.
    pub fn owned_bytes(&self) -> usize {
        match self {
            Self::Warp(mesh) => mesh
                .freeze_mask
                .as_ref()
                .map_or(0, SoftMask::owned_bytes)
                .saturating_add(mesh.protect_mask.as_ref().map_or(0, SoftMask::owned_bytes)),
            Self::ContentAwareReplace(settings) => settings
                .target_mask
                .owned_bytes()
                .saturating_add(settings.allowed_source_mask.owned_bytes()),
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterStack {
    pub nodes: Vec<FilterNode>,
}

impl FilterNode {
    /// Estimated bytes retained by masks attached to this node and operation.
    pub fn owned_bytes(&self) -> usize {
        self.soft_mask
            .as_ref()
            .map_or(0, SoftMask::owned_bytes)
            .saturating_add(self.operation.owned_bytes())
    }
}

impl FilterStack {
    pub fn owned_bytes(&self) -> usize {
        self.nodes.iter().fold(0usize, |total, node| {
            total.saturating_add(node.owned_bytes())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlendIfChannel {
    Luminance,
    Red,
    Green,
    Blue,
}

/// A split range: the two middle values provide smooth falloffs on each side.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendIfRange {
    pub black: f32,
    pub black_split: f32,
    pub white_split: f32,
    pub white: f32,
}

impl Default for BlendIfRange {
    fn default() -> Self {
        Self {
            black: 0.0,
            black_split: 0.0,
            white_split: 1.0,
            white: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendIf {
    pub source_channel: BlendIfChannel,
    pub backdrop_channel: BlendIfChannel,
    pub source: BlendIfRange,
    pub backdrop: BlendIfRange,
}

impl Default for BlendIf {
    fn default() -> Self {
        Self {
            source_channel: BlendIfChannel::Luminance,
            backdrop_channel: BlendIfChannel::Luminance,
            source: BlendIfRange::default(),
            backdrop: BlendIfRange::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarpPin {
    /// Pin coordinates are normalized to the image bounds.
    pub source: [f32; 2],
    pub target: [f32; 2],
    pub radius: f32,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarpMesh {
    pub columns: u32,
    pub rows: u32,
    /// Target positions for a regular normalized source grid, row-major.
    pub points: Vec<[f32; 2]>,
    pub pins: Vec<WarpPin>,
    pub freeze_mask: Option<SoftMask>,
    pub protect_mask: Option<SoftMask>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrequencySeparation {
    pub radius: u8,
    /// Low-frequency adjustment, where zero preserves the low plane.
    pub low_amount: f32,
    /// High-frequency multiplier, where one preserves the residual.
    pub high_amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DodgeBurnMode {
    Dodge,
    Burn,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DodgeBurn {
    pub mode: DodgeBurnMode,
    pub amount: f32,
    pub radius: u8,
}

/// Persisted recipes without an algorithm keep the original sampling behaviour.
/// New recipes opt in explicitly; unknown algorithms fail deserialization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentAwareAlgorithm {
    #[default]
    Legacy,
    ContextualV1,
}

impl ContentAwareAlgorithm {
    fn is_legacy(&self) -> bool {
        *self == Self::Legacy
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentAwareReplace {
    #[serde(default, skip_serializing_if = "ContentAwareAlgorithm::is_legacy")]
    pub algorithm: ContentAwareAlgorithm,
    pub target_mask: SoftMask,
    pub allowed_source_mask: SoftMask,
    pub search_radius: u32,
    pub patch_radius: u8,
    pub feather: f32,
}

/// Bring colours around a user-selected reference hue toward that reference.
/// Selection is colour-based only; no semantic subject or skin detection is
/// performed. Angles are circular HSL hue distances in encoded sRGB.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetColourUniformity {
    pub target_rgb: [u8; 3],
    /// Full-strength half-width around the target hue, in degrees.
    pub hue_range_degrees: f32,
    /// Additional cubic falloff beyond the full-strength range, in degrees.
    pub hue_falloff_degrees: f32,
    pub hue_uniformity: f32,
    pub saturation_uniformity: f32,
    pub lightness_uniformity: f32,
}

/// Low plane plus signed high-frequency residual. `reconstruct` returns the
/// exact original bytes (for valid layers), making the decomposition useful to
/// an editor that exposes separate low/high controls.
#[derive(Clone, Debug, PartialEq)]
pub struct FrequencyLayers {
    pub low: RgbaImage,
    pub residual: Vec<[i16; 3]>,
    pub alpha: Vec<u8>,
}

impl FrequencyLayers {
    pub fn reconstruct(&self) -> Result<RgbaImage> {
        let (width, height) = self.low.dimensions();
        let count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| anyhow::anyhow!("frequency layers are too large"))?;
        ensure!(
            self.residual.len() == count && self.alpha.len() == count,
            "frequency layer lengths mismatch"
        );
        let mut result = RgbaImage::new(width, height);
        for (index, p) in result.pixels_mut().enumerate() {
            let low = self.low.as_raw();
            let base = index * 4;
            p.0 = [
                add_residual(low[base], self.residual[index][0]),
                add_residual(low[base + 1], self.residual[index][1]),
                add_residual(low[base + 2], self.residual[index][2]),
                self.alpha[index],
            ];
        }
        Ok(result)
    }

    pub fn high_pixel(&self, x: u32, y: u32) -> [i16; 3] {
        self.residual[(y * self.low.width() + x) as usize]
    }
}

/// Validate a complete stack against a source image before evaluation.
pub fn validate_stack(source: &RgbaImage, nodes: &[FilterNode]) -> Result<()> {
    validate_image(source)?;
    validate_stack_dimensions(source.width(), source.height(), nodes)
}

/// Validate stack metadata and all dimensioned masks without allocating a
/// raster. This is intended for save/load and editor commit paths.
pub fn validate_stack_dimensions(width: u32, height: u32, nodes: &[FilterNode]) -> Result<()> {
    ensure!(
        crate::model::valid_dimensions(width, height),
        "image dimensions exceed Omuse limits"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_OPERATION_PIXELS,
        "advanced operations are limited to 16 million pixels"
    );
    ensure!(
        nodes.len() <= MAX_NODES,
        "filter stack exceeds {MAX_NODES} nodes"
    );
    let mut ids = HashSet::with_capacity(nodes.len());
    let mut mask_bytes = 0usize;
    for node in nodes {
        ensure!(
            !node.id.is_empty() && node.id.len() <= MAX_ID_LENGTH,
            "filter node id is invalid"
        );
        ensure!(
            node.name.len() <= MAX_NAME_LENGTH,
            "filter node name is too long"
        );
        ensure!(
            node.opacity.is_finite() && (0.0..=1.0).contains(&node.opacity),
            "node opacity must be 0..=1"
        );
        ensure!(
            ids.insert(node.id.clone()),
            "filter node ids must be unique"
        );
        if let Some(mask) = &node.soft_mask {
            mask.validate(Some((width, height)))?;
        }
        validate_operation_dimensions((width, height), &node.operation)?;
        mask_bytes = mask_bytes.saturating_add(node.owned_bytes());
        ensure!(
            mask_bytes <= MAX_STACK_MASK_BYTES,
            "filter stack masks exceed the {MAX_STACK_MASK_BYTES} byte storage budget"
        );
    }
    Ok(())
}

pub fn validate_operation(source: &RgbaImage, operation: &AdvancedOperation) -> Result<()> {
    validate_operation_dimensions(source.dimensions(), operation)
}

pub fn validate_operation_dimensions(
    dimensions: (u32, u32),
    operation: &AdvancedOperation,
) -> Result<()> {
    let (width, height) = dimensions;
    ensure!(
        crate::model::valid_dimensions(width, height),
        "image dimensions exceed Omuse limits"
    );
    match operation {
        AdvancedOperation::Filter(filter) => filters::validate(filter),
        AdvancedOperation::CameraRaw(settings) => camera_raw::validate(settings),
        AdvancedOperation::ReferenceColourMatch(settings) => settings.validate(),
        AdvancedOperation::Denoise { radius, strength } => {
            ensure!(
                *radius <= MAX_DENOISE_RADIUS,
                "denoise radius exceeds {MAX_DENOISE_RADIUS}"
            );
            finite_range(*strength, 0.0..=1.0, "denoise strength")
        }
        AdvancedOperation::BlendIf(_) => anyhow::bail!(
            "Blend If requires the actual layer backdrop; use the layer Blend If workspace, not a filter-stack node"
        ),
        AdvancedOperation::Warp(settings) => settings.validate(dimensions),
        AdvancedOperation::FrequencySeparation(settings) => {
            radius_limit(settings.radius)?;
            finite_range(settings.low_amount, -1.0..=1.0, "low frequency amount")?;
            finite_range(settings.high_amount, 0.0..=2.0, "high frequency amount")
        }
        AdvancedOperation::DodgeBurn(settings) => {
            ensure!(
                settings.radius <= MAX_DODGE_BURN_RADIUS,
                "dodge/burn radius exceeds {MAX_DODGE_BURN_RADIUS}"
            );
            finite_range(settings.amount, 0.0..=1.0, "dodge/burn amount")
        }
        AdvancedOperation::ContentAwareReplace(settings) => {
            ensure!(
                u64::from(width) * u64::from(height) <= MAX_CONTENT_PIXELS,
                "content-aware replacement is limited to 4 million pixels"
            );
            settings.target_mask.validate(Some(dimensions))?;
            settings.allowed_source_mask.validate(Some(dimensions))?;
            ensure!(
                settings.search_radius <= MAX_CONTENT_SEARCH_RADIUS,
                "content search radius exceeds {MAX_CONTENT_SEARCH_RADIUS}"
            );
            ensure!(
                settings.patch_radius <= MAX_CONTENT_PATCH_RADIUS,
                "content patch radius exceeds {MAX_CONTENT_PATCH_RADIUS}"
            );
            content_candidate_budget(settings)?;
            finite_range(settings.feather, 0.0..=1.0, "content replacement feather")
        }
        AdvancedOperation::TargetColourUniformity(settings) => {
            let target = settings.target_rgb.map(|value| f64::from(value) / 255.);
            let target_saturation = rgb_to_hsl(target)[1];
            ensure!(
                target_saturation >= 0.02,
                "target colour must have at least 2% HSL saturation to define a stable hue"
            );
            finite_range(
                settings.hue_range_degrees,
                0.0..=180.0,
                "target colour hue range",
            )?;
            finite_range(
                settings.hue_falloff_degrees,
                0.0..=180.0,
                "target colour hue falloff",
            )?;
            ensure!(
                settings.hue_range_degrees + settings.hue_falloff_degrees <= 180.0,
                "target colour hue range plus falloff must not exceed 180 degrees"
            );
            finite_range(settings.hue_uniformity, 0.0..=1.0, "hue uniformity")?;
            finite_range(
                settings.saturation_uniformity,
                0.0..=1.0,
                "saturation uniformity",
            )?;
            finite_range(
                settings.lightness_uniformity,
                0.0..=1.0,
                "lightness uniformity",
            )
        }
    }
}

fn radius_limit(radius: u8) -> Result<()> {
    ensure!(
        radius <= MAX_DENOISE_RADIUS,
        "radius exceeds {MAX_DENOISE_RADIUS}"
    );
    Ok(())
}

fn validate_image(image: &RgbaImage) -> Result<()> {
    ensure!(
        crate::model::valid_dimensions(image.width(), image.height()),
        "image dimensions exceed Omuse limits"
    );
    ensure!(
        u64::from(image.width()) * u64::from(image.height()) <= MAX_OPERATION_PIXELS,
        "advanced operations are limited to 16 million pixels"
    );
    Ok(())
}

fn finite_range(value: f32, range: std::ops::RangeInclusive<f32>, label: &str) -> Result<()> {
    ensure!(
        value.is_finite() && range.contains(&value),
        "{label} must be finite and in the supported range"
    );
    Ok(())
}

/// Return a deterministic candidate budget before the search starts. The
/// budget is based on the number of target pixels and patch area, so a large
/// target cannot accidentally turn the bounded search into a quadratic job.
fn content_candidate_budget(settings: &ContentAwareReplace) -> Result<usize> {
    let targets = settings
        .target_mask
        .data
        .iter()
        .filter(|value| **value > 0)
        .count() as u64;
    if targets == 0 {
        return Ok(0);
    }
    let patch = match settings.algorithm {
        ContentAwareAlgorithm::Legacy => settings.patch_radius,
        // Point-sized patches still need immediate, legitimate neighbour context.
        ContentAwareAlgorithm::ContextualV1 => settings.patch_radius.max(1),
    };
    let side = u64::from(patch) * 2 + 1;
    let patch_area = side.saturating_mul(side);
    let search_side = u64::from(settings.search_radius) * 2 + 1;
    let scan_area = search_side
        .min(u64::from(settings.target_mask.width))
        .saturating_mul(search_side.min(u64::from(settings.target_mask.height)));
    let contextual = settings.algorithm == ContentAwareAlgorithm::ContextualV1;
    let overhead = if contextual {
        u64::from(settings.target_mask.width)
            .saturating_mul(u64::from(settings.target_mask.height))
            .saturating_mul(3)
            .saturating_add(targets.saturating_mul(8))
    } else {
        0
    };
    let scan_work = targets.saturating_mul(scan_area).saturating_add(overhead);
    let minimum_candidates = MIN_CONTENT_CANDIDATES + if contextual { 4 } else { 0 };
    let minimum_work = scan_work.saturating_add(
        targets
            .saturating_mul(minimum_candidates)
            .saturating_mul(patch_area),
    );
    ensure!(
        minimum_work <= MAX_CONTENT_WORK,
        "content-aware target is too large for the minimum candidate quality budget"
    );
    let budget = (MAX_CONTENT_WORK.saturating_sub(scan_work) / targets.max(1) / patch_area.max(1))
        .min(512)
        .max(minimum_candidates);
    Ok(budget as usize)
}

/// Evaluate a stack, returning a fresh image.
pub fn evaluate(source: &RgbaImage, nodes: &[FilterNode]) -> Result<RgbaImage> {
    let cancelled = AtomicBool::new(false);
    evaluate_cancellable(source, nodes, &cancelled)
}

/// Evaluate a stack while checking cancellation at row and operation
/// boundaries. The caller's source is never modified, including cancellation.
pub fn evaluate_cancellable(
    source: &RgbaImage,
    nodes: &[FilterNode],
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    validate_stack(source, nodes)?;
    check_cancelled(cancelled)?;
    let backdrop = source.clone();
    let mut working = source.clone();
    for node in nodes {
        check_cancelled(cancelled)?;
        if !node.enabled || node.opacity == 0.0 {
            continue;
        }
        let mut candidate = working.clone();
        apply_operation(
            &mut candidate,
            &working,
            &backdrop,
            &node.operation,
            cancelled,
        )?;
        blend_node(
            &mut working,
            &candidate,
            node.opacity,
            node.soft_mask.as_ref(),
            cancelled,
        )?;
    }
    check_cancelled(cancelled)?;
    Ok(working)
}

fn apply_operation(
    candidate: &mut RgbaImage,
    current: &RgbaImage,
    backdrop: &RgbaImage,
    operation: &AdvancedOperation,
    cancelled: &AtomicBool,
) -> Result<()> {
    match operation {
        AdvancedOperation::Filter(filter) => {
            filters::apply_cancellable(candidate, filter, cancelled)
        }
        AdvancedOperation::CameraRaw(settings) => {
            let output = camera_raw::apply(candidate, settings)?;
            *candidate = output;
            Ok(())
        }
        AdvancedOperation::Denoise { radius, strength } => {
            denoise(candidate, *radius, *strength, cancelled)
        }
        AdvancedOperation::BlendIf(settings) => {
            apply_blend_if_to_image(candidate, backdrop, settings, cancelled)
        }
        AdvancedOperation::Warp(settings) => apply_warp(candidate, settings, cancelled),
        AdvancedOperation::FrequencySeparation(settings) => {
            apply_frequency(candidate, settings, cancelled)
        }
        AdvancedOperation::DodgeBurn(settings) => apply_dodge_burn(candidate, settings, cancelled),
        AdvancedOperation::ContentAwareReplace(settings) => {
            apply_content_aware(candidate, current, settings, cancelled)
        }
        AdvancedOperation::TargetColourUniformity(settings) => {
            apply_target_colour_uniformity(candidate, settings, cancelled)
        }
        AdvancedOperation::ReferenceColourMatch(settings) => {
            *candidate = crate::color_match::apply8(current, settings, cancelled)?;
            Ok(())
        }
    }
}

fn blend_node(
    target: &mut RgbaImage,
    candidate: &RgbaImage,
    opacity: f32,
    mask: Option<&SoftMask>,
    cancelled: &AtomicBool,
) -> Result<()> {
    for y in 0..target.height() {
        check_cancelled(cancelled)?;
        for x in 0..target.width() {
            let coverage = opacity * mask.map_or(1.0, |m| m.value(x, y));
            if coverage <= 0.0 {
                continue;
            }
            let old = *target.get_pixel(x, y);
            let next = *candidate.get_pixel(x, y);
            let mut pixel = old.0;
            for channel in 0..4 {
                pixel[channel] = lerp_byte(old[channel], next[channel], coverage);
            }
            *target.get_pixel_mut(x, y) = Rgba(pixel);
        }
    }
    Ok(())
}

/// Evaluate the source/backdrop split ranges independently and combine them
/// with the conservative minimum. This is useful to previews as well as the
/// stack evaluator.
pub fn blend_if_coverage(source: [u8; 4], backdrop: [u8; 4], settings: &BlendIf) -> f32 {
    let source_value = channel_value(source, settings.source_channel);
    let backdrop_value = channel_value(backdrop, settings.backdrop_channel);
    split_coverage(source_value, settings.source)
        .min(split_coverage(backdrop_value, settings.backdrop))
}

/// Validate a Blend If record independently of a stack. Layer/editor code can
/// call this before committing source/backdrop state to a document.
pub fn validate_blend_if(settings: &BlendIf) -> Result<()> {
    validate_split_range(settings.source)?;
    validate_split_range(settings.backdrop)
}

/// Apply Blend If against an explicitly supplied raster backdrop. Editable
/// filter nodes cannot supply that backdrop; layer compositors use this API.
pub fn apply_blend_if(
    source: &RgbaImage,
    backdrop: &RgbaImage,
    settings: &BlendIf,
) -> Result<RgbaImage> {
    let cancelled = AtomicBool::new(false);
    apply_blend_if_cancellable(source, backdrop, settings, &cancelled)
}

pub fn apply_blend_if_cancellable(
    source: &RgbaImage,
    backdrop: &RgbaImage,
    settings: &BlendIf,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    validate_image(source)?;
    validate_image(backdrop)?;
    ensure!(
        source.dimensions() == backdrop.dimensions(),
        "Blend If source and backdrop dimensions must match"
    );
    validate_blend_if(settings)?;
    let mut output = source.clone();
    apply_blend_if_to_image(&mut output, backdrop, settings, cancelled)?;
    Ok(output)
}

fn apply_blend_if_to_image(
    image: &mut RgbaImage,
    backdrop: &RgbaImage,
    settings: &BlendIf,
    cancelled: &AtomicBool,
) -> Result<()> {
    for y in 0..image.height() {
        check_cancelled(cancelled)?;
        for x in 0..image.width() {
            let coverage = blend_if_coverage(
                image.get_pixel(x, y).0,
                backdrop.get_pixel(x, y).0,
                settings,
            );
            let p = image.get_pixel_mut(x, y);
            p[3] = (f32::from(p[3]) * coverage).round() as u8;
        }
    }
    Ok(())
}

fn split_coverage(value: f32, range: BlendIfRange) -> f32 {
    smooth_ramp_up(value, range.black, range.black_split)
        * smooth_ramp_down(value, range.white_split, range.white)
}

/// Encoded channel values supplied by the high-precision compositor. Keeping
/// these normalized values avoids quantizing a soft tonal boundary to 8 bits.
pub(crate) fn blend_if_coverage_normalized(
    source: [f32; 4],
    backdrop: [f32; 4],
    settings: &BlendIf,
) -> f32 {
    let channel = |pixel: [f32; 4], channel| match channel {
        BlendIfChannel::Luminance => 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2],
        BlendIfChannel::Red => pixel[0],
        BlendIfChannel::Green => pixel[1],
        BlendIfChannel::Blue => pixel[2],
    };
    split_coverage(channel(source, settings.source_channel), settings.source).min(split_coverage(
        channel(backdrop, settings.backdrop_channel),
        settings.backdrop,
    ))
}

fn smooth_ramp_up(value: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return f32::from(value >= end);
    }
    smoothstep(((value - start) / (end - start)).clamp(0.0, 1.0))
}

fn smooth_ramp_down(value: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return f32::from(value <= start);
    }
    1.0 - smoothstep(((value - start) / (end - start)).clamp(0.0, 1.0))
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn smoothstep64(value: f64) -> f64 {
    value * value * (3.0 - 2.0 * value)
}

fn rgb_to_hsl(rgb: [f64; 3]) -> [f64; 3] {
    let maximum = rgb.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let minimum = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let chroma = maximum - minimum;
    let lightness = (maximum + minimum) * 0.5;
    if chroma <= f64::EPSILON {
        return [0., 0., lightness];
    }
    let saturation = chroma / (1. - (2. * lightness - 1.).abs());
    let hue_sector = if maximum == rgb[0] {
        ((rgb[1] - rgb[2]) / chroma).rem_euclid(6.)
    } else if maximum == rgb[1] {
        (rgb[2] - rgb[0]) / chroma + 2.
    } else {
        (rgb[0] - rgb[1]) / chroma + 4.
    };
    [(hue_sector / 6.).rem_euclid(1.), saturation, lightness]
}

fn hsl_to_rgb(hsl: [f64; 3]) -> [f64; 3] {
    let [hue, saturation, lightness] = hsl;
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue.rem_euclid(1.) * 6.;
    let x = chroma * (1. - (sector.rem_euclid(2.) - 1.).abs());
    let rgb = match sector.floor() as u8 {
        0 => [chroma, x, 0.],
        1 => [x, chroma, 0.],
        2 => [0., chroma, x],
        3 => [0., x, chroma],
        4 => [x, 0., chroma],
        _ => [chroma, 0., x],
    };
    let offset = lightness - chroma * 0.5;
    rgb.map(|channel| (channel + offset).clamp(0., 1.))
}

fn target_hue_coverage(distance_degrees: f64, settings: &TargetColourUniformity) -> f64 {
    let range = f64::from(settings.hue_range_degrees);
    if distance_degrees <= range {
        return 1.;
    }
    let falloff = f64::from(settings.hue_falloff_degrees);
    if falloff <= 0. || distance_degrees >= range + falloff {
        return 0.;
    }
    smoothstep64(1. - (distance_degrees - range) / falloff)
}

#[derive(Clone, Copy)]
pub(crate) struct TargetColourUniformityKernel {
    settings: TargetColourUniformity,
    target_hsl: [f64; 3],
}

impl TargetColourUniformityKernel {
    pub(crate) fn new(settings: &TargetColourUniformity) -> Self {
        Self {
            settings: *settings,
            target_hsl: rgb_to_hsl(settings.target_rgb.map(|value| f64::from(value) / 255.)),
        }
    }

    /// Apply the shared encoded-sRGB HSL kernel to normalized channels.
    /// `None` means zero membership or no-op, allowing wide-gamut callers to
    /// retain the original samples without a colour-space round trip.
    pub(crate) fn apply(&self, rgb: [f64; 3]) -> Option<[f64; 3]> {
        let settings = &self.settings;
        if settings.hue_uniformity == 0.
            && settings.saturation_uniformity == 0.
            && settings.lightness_uniformity == 0.
        {
            return None;
        }
        let source = rgb_to_hsl(rgb);
        // HSL assigns an arbitrary hue to grays. Fade that convention out
        // through near-neutral colours so a red target cannot tint them.
        let chroma_coverage = smoothstep64(((source[1] - 0.02) / 0.08).clamp(0., 1.));
        if chroma_coverage == 0. {
            return None;
        }
        let hue_delta = (self.target_hsl[0] - source[0] + 0.5).rem_euclid(1.) - 0.5;
        let coverage = chroma_coverage * target_hue_coverage(hue_delta.abs() * 360., settings);
        if coverage == 0. {
            return None;
        }
        let hsl = [
            (source[0] + hue_delta * f64::from(settings.hue_uniformity) * coverage).rem_euclid(1.),
            source[1]
                + (self.target_hsl[1] - source[1])
                    * f64::from(settings.saturation_uniformity)
                    * coverage,
            source[2]
                + (self.target_hsl[2] - source[2])
                    * f64::from(settings.lightness_uniformity)
                    * coverage,
        ];
        Some(hsl_to_rgb(hsl))
    }
}

/// Convenience wrapper used by byte rendering and math tests.
#[cfg(test)]
pub(crate) fn target_colour_uniformity_rgb(
    rgb: [f64; 3],
    settings: &TargetColourUniformity,
) -> [f64; 3] {
    TargetColourUniformityKernel::new(settings)
        .apply(rgb)
        .unwrap_or(rgb)
}

fn apply_target_colour_uniformity(
    image: &mut RgbaImage,
    settings: &TargetColourUniformity,
    cancelled: &AtomicBool,
) -> Result<()> {
    if settings.hue_uniformity == 0.
        && settings.saturation_uniformity == 0.
        && settings.lightness_uniformity == 0.
    {
        return Ok(());
    }
    let kernel = TargetColourUniformityKernel::new(settings);
    for row in image.rows_mut() {
        check_cancelled(cancelled)?;
        for pixel in row {
            let source = pixel.0;
            let rgb = [source[0], source[1], source[2]].map(|value| f64::from(value) / 255.);
            let Some(adjusted) = kernel.apply(rgb) else {
                continue;
            };
            for channel in 0..3 {
                pixel[channel] = (adjusted[channel] * 255.).round().clamp(0., 255.) as u8;
            }
        }
    }
    Ok(())
}

fn channel_value(pixel: [u8; 4], channel: BlendIfChannel) -> f32 {
    match channel {
        BlendIfChannel::Luminance => {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
                / 255.0
        }
        BlendIfChannel::Red => f32::from(pixel[0]) / 255.0,
        BlendIfChannel::Green => f32::from(pixel[1]) / 255.0,
        BlendIfChannel::Blue => f32::from(pixel[2]) / 255.0,
    }
}

fn validate_split_range(range: BlendIfRange) -> Result<()> {
    for value in [
        range.black,
        range.black_split,
        range.white_split,
        range.white,
    ] {
        finite_range(value, 0.0..=1.0, "Blend If range")?;
    }
    ensure!(
        range.black <= range.black_split
            && range.black_split <= range.white_split
            && range.white_split <= range.white,
        "Blend If split range must be ordered"
    );
    Ok(())
}

impl WarpMesh {
    fn validate(&self, dimensions: (u32, u32)) -> Result<()> {
        ensure!(
            self.columns >= 2 && self.columns <= MAX_MESH_COLUMNS,
            "warp mesh columns must be 2..={MAX_MESH_COLUMNS}"
        );
        ensure!(
            self.rows >= 2 && self.rows <= MAX_MESH_ROWS,
            "warp mesh rows must be 2..={MAX_MESH_ROWS}"
        );
        let count = usize::try_from(u64::from(self.columns) * u64::from(self.rows))
            .map_err(|_| anyhow::anyhow!("warp mesh is too large"))?;
        ensure!(
            self.points.len() == count,
            "warp mesh point count does not match rows and columns"
        );
        ensure!(
            self.pins.len() <= MAX_PINS,
            "warp pin count exceeds {MAX_PINS}"
        );
        for point in &self.points {
            for value in point {
                ensure!(
                    value.is_finite() && (-2.0..=3.0).contains(value),
                    "warp mesh point is outside bounds"
                );
            }
        }
        for pin in &self.pins {
            for value in pin.source.into_iter().chain(pin.target) {
                ensure!(
                    value.is_finite() && (-1.0..=2.0).contains(&value),
                    "warp pin coordinate is outside bounds"
                );
            }
            ensure!(
                pin.radius.is_finite() && (0.001..=MAX_WARP_RADIUS).contains(&pin.radius),
                "warp pin radius is invalid"
            );
            ensure!(
                pin.strength.is_finite() && (-2.0..=2.0).contains(&pin.strength),
                "warp pin strength is invalid"
            );
        }
        if let Some(mask) = &self.freeze_mask {
            mask.validate(Some(dimensions))?;
        }
        if let Some(mask) = &self.protect_mask {
            mask.validate(Some(dimensions))?;
        }
        Ok(())
    }
}

fn apply_warp(image: &mut RgbaImage, warp: &WarpMesh, cancelled: &AtomicBool) -> Result<()> {
    let original = image.clone();
    let width = image.width();
    let height = image.height();
    for y in 0..height {
        check_cancelled(cancelled)?;
        for x in 0..width {
            let p = [
                x as f32 / (width.saturating_sub(1).max(1)) as f32,
                y as f32 / (height.saturating_sub(1).max(1)) as f32,
            ];
            let freeze = warp
                .freeze_mask
                .as_ref()
                .map_or(0.0, |mask| mask.value(x, y));
            let protect = warp
                .protect_mask
                .as_ref()
                .map_or(0.0, |mask| mask.value(x, y));
            let mut source = p;
            for _ in 0..8 {
                let displacement = warp_displacement(source, warp);
                source[0] = p[0] - displacement[0] * (1.0 - freeze);
                source[1] = p[1] - displacement[1] * (1.0 - freeze);
            }
            let sampled = sample_premultiplied(
                &original,
                source[0] * (width.saturating_sub(1)) as f32,
                source[1] * (height.saturating_sub(1)) as f32,
            );
            let unchanged = original.get_pixel(x, y).0;
            let amount = 1.0 - protect;
            *image.get_pixel_mut(x, y) = Rgba([
                lerp_byte(unchanged[0], sampled[0], amount),
                lerp_byte(unchanged[1], sampled[1], amount),
                lerp_byte(unchanged[2], sampled[2], amount),
                lerp_byte(unchanged[3], sampled[3], amount),
            ]);
        }
    }
    Ok(())
}

pub(crate) fn warp_displacement(p: [f32; 2], warp: &WarpMesh) -> [f32; 2] {
    let mut output = [0.0, 0.0];
    if warp.columns >= 2 && warp.rows >= 2 {
        let gx = p[0].clamp(0.0, 1.0) * (warp.columns - 1) as f32;
        let gy = p[1].clamp(0.0, 1.0) * (warp.rows - 1) as f32;
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let x1 = (x0 + 1).min(warp.columns as usize - 1);
        let y1 = (y0 + 1).min(warp.rows as usize - 1);
        let tx = gx.fract();
        let ty = gy.fract();
        let at = |x: usize, y: usize| warp.points[y * warp.columns as usize + x];
        let a = at(x0, y0);
        let b = at(x1, y0);
        let c = at(x0, y1);
        let d = at(x1, y1);
        let target = [
            lerp(lerp(a[0], b[0], tx), lerp(c[0], d[0], tx), ty),
            lerp(lerp(a[1], b[1], tx), lerp(c[1], d[1], tx), ty),
        ];
        output = [target[0] - p[0], target[1] - p[1]];
    }
    for pin in &warp.pins {
        let dx = p[0] - pin.source[0];
        let dy = p[1] - pin.source[1];
        let distance = (dx * dx + dy * dy).sqrt();
        let weight = (1.0 - distance / pin.radius).clamp(0.0, 1.0);
        let weight = weight * weight * (3.0 - 2.0 * weight) * pin.strength;
        output[0] += (pin.target[0] - pin.source[0]) * weight;
        output[1] += (pin.target[1] - pin.source[1]) * weight;
    }
    output
}

fn sample_premultiplied(image: &RgbaImage, x: f32, y: f32) -> [u8; 4] {
    let max_x = image.width().saturating_sub(1) as f32;
    let max_y = image.height().saturating_sub(1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x.fract();
    let ty = y.fract();
    let sample = |xx: u32, yy: u32| {
        let p = image.get_pixel(xx, yy).0;
        let alpha = f32::from(p[3]) / 255.0;
        [
            f32::from(p[0]) / 255.0 * alpha,
            f32::from(p[1]) / 255.0 * alpha,
            f32::from(p[2]) / 255.0 * alpha,
            alpha,
        ]
    };
    let a = sample(x0, y0);
    let b = sample(x1, y0);
    let c = sample(x0, y1);
    let d = sample(x1, y1);
    let mut v = [0.0; 4];
    for i in 0..4 {
        v[i] = lerp(lerp(a[i], b[i], tx), lerp(c[i], d[i], tx), ty);
    }
    let alpha = v[3].clamp(0.0, 1.0);
    if alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }
    [
        byte(v[0] / alpha),
        byte(v[1] / alpha),
        byte(v[2] / alpha),
        byte(alpha),
    ]
}

fn denoise(image: &mut RgbaImage, radius: u8, strength: f32, cancelled: &AtomicBool) -> Result<()> {
    if radius == 0 || strength == 0.0 {
        return Ok(());
    }
    let mut blurred = image.clone();
    filters::apply_cancellable(
        &mut blurred,
        &filters::Filter::GaussianBlur {
            sigma: f32::from(radius) / 2.0,
        },
        cancelled,
    )?;
    for y in 0..image.height() {
        check_cancelled(cancelled)?;
        for x in 0..image.width() {
            let original = image.get_pixel(x, y).0;
            let smooth = blurred.get_pixel(x, y).0;
            let difference = ((channel_value(original, BlendIfChannel::Luminance)
                - channel_value(smooth, BlendIfChannel::Luminance))
            .abs()
                * 8.0)
                .clamp(0.0, 1.0);
            let amount = strength * (1.0 - difference);
            let p = image.get_pixel_mut(x, y);
            for c in 0..3 {
                p[c] = lerp_byte(original[c], smooth[c], amount);
            }
        }
    }
    Ok(())
}

pub fn separate_frequencies(image: &RgbaImage, radius: u8) -> Result<FrequencyLayers> {
    separate_frequencies_cancellable(image, radius, &AtomicBool::new(false))
}
fn separate_frequencies_cancellable(
    image: &RgbaImage,
    radius: u8,
    cancelled: &AtomicBool,
) -> Result<FrequencyLayers> {
    check_cancelled(cancelled)?;
    validate_image(image)?;
    ensure!(
        radius <= MAX_DENOISE_RADIUS,
        "frequency radius exceeds {MAX_DENOISE_RADIUS}"
    );
    let mut low = image.clone();
    if radius > 0 {
        filters::apply_cancellable(
            &mut low,
            &filters::Filter::GaussianBlur {
                sigma: f32::from(radius) / 2.0,
            },
            cancelled,
        )?;
    }
    let mut residual = Vec::with_capacity(image.pixels().len());
    let mut alpha = Vec::with_capacity(image.pixels().len());
    for (source, low_pixel) in image.pixels().zip(low.pixels()) {
        residual.push([
            i16::from(source[0]) - i16::from(low_pixel[0]),
            i16::from(source[1]) - i16::from(low_pixel[1]),
            i16::from(source[2]) - i16::from(low_pixel[2]),
        ]);
        alpha.push(source[3]);
    }
    Ok(FrequencyLayers {
        low,
        residual,
        alpha,
    })
}

fn apply_frequency(
    image: &mut RgbaImage,
    settings: &FrequencySeparation,
    cancelled: &AtomicBool,
) -> Result<()> {
    let layers = separate_frequencies_cancellable(image, settings.radius, cancelled)?;
    for y in 0..image.height() {
        check_cancelled(cancelled)?;
        for x in 0..image.width() {
            let index = (y * image.width() + x) as usize;
            let low = layers.low.get_pixel(x, y).0;
            let high = layers.residual[index];
            let mut output = [0u8; 4];
            for c in 0..3 {
                let low_value = f32::from(low[c]) * (1.0 + settings.low_amount);
                let high_value = f32::from(high[c]) * settings.high_amount;
                output[c] = byte((low_value + high_value) / 255.0);
            }
            output[3] = layers.alpha[index];
            *image.get_pixel_mut(x, y) = Rgba(output);
        }
    }
    Ok(())
}

fn apply_dodge_burn(
    image: &mut RgbaImage,
    settings: &DodgeBurn,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut luminance = RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y).0;
        let v = byte(channel_value(p, BlendIfChannel::Luminance));
        Rgba([v, v, v, p[3]])
    });
    if settings.radius > 0 {
        filters::apply_cancellable(
            &mut luminance,
            &filters::Filter::GaussianBlur {
                sigma: f32::from(settings.radius) / 2.0,
            },
            cancelled,
        )?;
    }
    for y in 0..image.height() {
        check_cancelled(cancelled)?;
        for x in 0..image.width() {
            let local = f32::from(luminance.get_pixel(x, y)[0]) / 255.0;
            let source = image.get_pixel(x, y).0;
            let factor = match settings.mode {
                DodgeBurnMode::Dodge => 1.0 + settings.amount * (1.0 - local),
                DodgeBurnMode::Burn => 1.0 - settings.amount * local,
            };
            let p = image.get_pixel_mut(x, y);
            for c in 0..3 {
                p[c] = byte(f32::from(source[c]) / 255.0 * factor);
            }
        }
    }
    Ok(())
}

fn apply_content_aware(
    image: &mut RgbaImage,
    current: &RgbaImage,
    settings: &ContentAwareReplace,
    cancelled: &AtomicBool,
) -> Result<()> {
    visit_content_samples(current, settings, cancelled, |x, y, sx, sy, amount| {
        let source = current.get_pixel(sx, sy).0;
        let p = image.get_pixel_mut(x, y);
        for c in 0..4 {
            p[c] = lerp_byte(p[c], source[c], amount);
        }
        Ok(())
    })
}

/// Select samples with byte previews; callbacks can copy original high-bit-depth
/// values. Candidate sampling covers the entire allowed search rectangle.
pub(crate) fn visit_content_samples(
    original: &RgbaImage,
    settings: &ContentAwareReplace,
    cancelled: &AtomicBool,
    sample: impl FnMut(u32, u32, u32, u32, f32) -> Result<()>,
) -> Result<()> {
    match settings.algorithm {
        ContentAwareAlgorithm::Legacy => {
            visit_content_samples_legacy(original, settings, cancelled, sample)
        }
        ContentAwareAlgorithm::ContextualV1 => {
            visit_content_samples_contextual(original, settings, cancelled, sample)
        }
    }
}

// Preserve the original algorithm byte-for-byte for already saved recipes.
fn visit_content_samples_legacy(
    original: &RgbaImage,
    settings: &ContentAwareReplace,
    cancelled: &AtomicBool,
    mut sample: impl FnMut(u32, u32, u32, u32, f32) -> Result<()>,
) -> Result<()> {
    let candidate_limit = content_candidate_budget(settings)?;
    if candidate_limit == 0 {
        return Ok(());
    }
    let width = original.width();
    let height = original.height();
    let mut candidates = Vec::new();
    let radius = i32::try_from(settings.search_radius).unwrap_or(i32::MAX);
    let patch = i32::from(settings.patch_radius);
    for y in 0..height {
        check_cancelled(cancelled)?;
        for x in 0..width {
            let target_coverage = settings.target_mask.value(x, y);
            if target_coverage <= 0.0 {
                continue;
            }
            let mut best = None;
            let mut best_score = f32::INFINITY;
            let y0 = (i32::try_from(y).unwrap_or(i32::MAX) - radius).max(0);
            let y1 = (i32::try_from(y).unwrap_or(i32::MAX) + radius)
                .min(i32::try_from(height).unwrap_or(i32::MAX) - 1);
            let x0 = (i32::try_from(x).unwrap_or(i32::MAX) - radius).max(0);
            let x1 = (i32::try_from(x).unwrap_or(i32::MAX) + radius)
                .min(i32::try_from(width).unwrap_or(i32::MAX) - 1);
            check_cancelled(cancelled)?;
            candidates.clear();
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    let coverage = settings.allowed_source_mask.value(cx as u32, cy as u32);
                    if coverage > 0. && settings.target_mask.value(cx as u32, cy as u32) == 0. {
                        candidates.push((cx, cy, coverage));
                    }
                }
            }
            ensure!(
                !candidates.is_empty(),
                "No allowed source within the search radius for target pixel ({x}, {y}); enlarge the radius or move the sampling area"
            );
            let step = candidates.len().div_ceil(candidate_limit);
            for &(cx, cy, source_coverage) in candidates.iter().step_by(step.max(1)) {
                let cxu = cx as u32;
                let cyu = cy as u32;
                let mut score = 0.0;
                let mut samples = 0u32;
                for dy in -patch..=patch {
                    for dx in -patch..=patch {
                        let tx = i32::try_from(x).unwrap_or(0) + dx;
                        let ty = i32::try_from(y).unwrap_or(0) + dy;
                        let sx = cx + dx;
                        let sy = cy + dy;
                        if tx < 0
                            || ty < 0
                            || sx < 0
                            || sy < 0
                            || tx >= i32::try_from(width).unwrap_or(0)
                            || ty >= i32::try_from(height).unwrap_or(0)
                            || sx >= i32::try_from(width).unwrap_or(0)
                            || sy >= i32::try_from(height).unwrap_or(0)
                        {
                            continue;
                        }
                        if settings.target_mask.value(tx as u32, ty as u32) > 0.0 {
                            continue;
                        }
                        let a = original.get_pixel(tx as u32, ty as u32).0;
                        let b = original.get_pixel(sx as u32, sy as u32).0;
                        for c in 0..3 {
                            let delta = f32::from(a[c]) - f32::from(b[c]);
                            score += delta * delta;
                        }
                        samples += 1;
                    }
                }
                if samples > 0 {
                    score /= samples as f32;
                }
                if score < best_score {
                    best_score = score;
                    best = Some((cxu, cyu, source_coverage));
                }
            }
            if let Some((sx, sy, source_coverage)) = best {
                let amount = target_coverage
                    * source_coverage
                    * (1.0 - settings.feather
                        + settings.feather
                            * (1.0 - best_score / (255.0 * 255.0 * 3.0)).clamp(0.0, 1.0));
                sample(x, y, sx, sy, amount)?;
            }
        }
    }
    Ok(())
}

/// Fill from known boundaries inward. Each reconstructed context pixel points
/// to its immutable donor in the original image, including for the 16-bit path.
/// A candidate must explain real context; an empty comparison is never a match.
fn visit_content_samples_contextual(
    original: &RgbaImage,
    settings: &ContentAwareReplace,
    cancelled: &AtomicBool,
    mut sample: impl FnMut(u32, u32, u32, u32, f32) -> Result<()>,
) -> Result<()> {
    check_cancelled(cancelled)?;
    let candidate_limit = content_candidate_budget(settings)?;
    if candidate_limit == 0 {
        return Ok(());
    }
    let (width, height) = original.dimensions();
    ensure!(
        settings.search_radius <= MAX_CONTENT_SEARCH_RADIUS
            && settings.patch_radius <= MAX_CONTENT_PATCH_RADIUS,
        "Invalid removal search or patch radius"
    );
    let count = (u64::from(width) * u64::from(height)) as usize;
    ensure!(
        count as u64 <= MAX_CONTENT_PIXELS,
        "Content-aware source is too large"
    );
    settings.target_mask.validate(Some((width, height)))?;
    settings
        .allowed_source_mask
        .validate(Some((width, height)))?;
    let targets = settings.target_mask.data.iter().filter(|&&v| v > 0).count();
    const PENDING: u32 = u32::MAX;
    const QUEUED: u32 = u32::MAX - 1;
    let mut donors = Vec::new();
    donors
        .try_reserve_exact(count)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal context"))?;
    donors.extend(
        settings
            .target_mask
            .data
            .iter()
            .enumerate()
            .map(|(i, &v)| if v == 0 { i as u32 } else { PENDING }),
    );
    // At most two u32 arrays over the 4M-pixel source (32 MB total), plus
    // at most 129*129 candidate indices. All reservations are fallible.
    check_cancelled(cancelled)?;
    let mut front = Vec::new();
    front
        .try_reserve_exact(targets)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal boundary"))?;
    for i in 0..count {
        if i % width as usize == 0 {
            check_cancelled(cancelled)?;
        }
        if donors[i] == PENDING
            && content_neighbours(i as u32, width, height)
                .into_iter()
                .flatten()
                .any(|j| donors[j as usize] < count as u32)
        {
            donors[i] = QUEUED;
            front.push(i as u32);
        }
    }
    ensure!(
        !front.is_empty(),
        "Removal needs some unselected surrounding context"
    );
    let radius = settings.search_radius as i32;
    let patch = i32::from(settings.patch_radius.max(1));
    let search_side = settings.search_radius as usize * 2 + 1;
    let mut candidates = Vec::<u32>::new();
    candidates
        .try_reserve_exact(search_side * search_side)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal sampling"))?;
    let mut cursor = 0;
    while cursor < front.len() {
        check_cancelled(cancelled)?;
        let i = front[cursor];
        cursor += 1;
        let (x, y) = (i % width, i / width);
        let (x0, x1) = (
            (x as i32 - radius).max(0),
            (x as i32 + radius).min(width as i32 - 1),
        );
        let (y0, y1) = (
            (y as i32 - radius).max(0),
            (y as i32 + radius).min(height as i32 - 1),
        );
        candidates.clear();
        for cy in y0..=y1 {
            check_cancelled(cancelled)?;
            for cx in x0..=x1 {
                let j = cy as u32 * width + cx as u32;
                if settings.allowed_source_mask.data[j as usize] > 0
                    && settings.target_mask.data[j as usize] == 0
                {
                    candidates.push(j);
                }
            }
        }
        ensure!(
            !candidates.is_empty(),
            "No allowed source within the search radius for target pixel ({x}, {y}); enlarge the radius or move the sampling area"
        );
        // Test continuations of reconstructed neighbouring patches as well as
        // a uniform sample across the entire allowed rectangle. These four
        // proposals are included in the validated candidate work budget.
        let mut coherent = [None; 4];
        for (slot, neighbour) in coherent
            .iter_mut()
            .zip(content_neighbours(i, width, height))
        {
            let Some(j) = neighbour else { continue };
            let donor = donors[j as usize];
            if donor >= count as u32 || settings.target_mask.data[j as usize] == 0 {
                continue;
            }
            let sx = (donor % width) as i32 + x as i32 - (j % width) as i32;
            let sy = (donor / width) as i32 + y as i32 - (j / width) as i32;
            if sx < x0 || sx > x1 || sy < y0 || sy > y1 {
                continue;
            }
            let source = sy as u32 * width + sx as u32;
            if settings.allowed_source_mask.data[source as usize] > 0
                && settings.target_mask.data[source as usize] == 0
            {
                *slot = Some(source);
            }
        }
        let regular_limit = candidate_limit.saturating_sub(4).max(1);
        let step = candidates.len().div_ceil(regular_limit).max(1);
        let mut best = None;
        let mut best_score = f32::INFINITY;
        let mut best_distance = u64::MAX;
        for donor in coherent
            .into_iter()
            .flatten()
            .chain(candidates.iter().step_by(step).copied())
        {
            let (sx, sy) = (donor % width, donor / width);
            let Some(score) =
                content_context_score(original, settings, &donors, (x, y), (sx, sy), patch)
            else {
                continue;
            };
            let distance = u64::from(x.abs_diff(sx)).pow(2) + u64::from(y.abs_diff(sy)).pow(2);
            if score < best_score || (score == best_score && distance < best_distance) {
                best = Some(donor);
                best_score = score;
                best_distance = distance;
            }
        }
        let donor = best.ok_or_else(|| anyhow::anyhow!("No allowed source patch has enough context for target pixel ({x}, {y}); enlarge the sampling area or reduce the patch radius"))?;
        let (sx, sy) = (donor % width, donor / width);
        let amount = settings.target_mask.value(x, y)
            * settings.allowed_source_mask.value(sx, sy)
            * (1.0 - settings.feather
                + settings.feather * (1.0 - best_score / (255.0 * 255.0 * 3.0)).clamp(0.0, 1.0));
        check_cancelled(cancelled)?;
        sample(x, y, sx, sy, amount)?;
        donors[i as usize] = donor;
        for neighbour in content_neighbours(i, width, height).into_iter().flatten() {
            if donors[neighbour as usize] == PENDING {
                donors[neighbour as usize] = QUEUED;
                front.push(neighbour);
            }
        }
    }
    ensure!(
        cursor == targets,
        "Removal could not reach all selected pixels from known context"
    );
    Ok(())
}

fn content_neighbours(i: u32, width: u32, height: u32) -> [Option<u32>; 4] {
    let (x, y) = (i % width, i / width);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < width).then(|| i + 1),
        (y > 0).then(|| i - width),
        (y + 1 < height).then(|| i + width),
    ]
}

fn content_context_score(
    original: &RgbaImage,
    settings: &ContentAwareReplace,
    donors: &[u32],
    target: (u32, u32),
    source: (u32, u32),
    patch: i32,
) -> Option<f32> {
    let (width, height) = original.dimensions();
    let mut score = 0.;
    let mut samples = 0;
    for dy in -patch..=patch {
        for dx in -patch..=patch {
            let (tx, ty) = (target.0 as i32 + dx, target.1 as i32 + dy);
            if tx < 0 || ty < 0 || tx >= width as i32 || ty >= height as i32 {
                continue;
            }
            let known = donors[(ty as u32 * width + tx as u32) as usize];
            if known as usize >= donors.len() {
                continue;
            }
            let (sx, sy) = (source.0 as i32 + dx, source.1 as i32 + dy);
            // Every compared donor sample must belong to the user's allowed,
            // unselected region; hidden target colours are never evidence.
            if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                return None;
            }
            let j = (sy as u32 * width + sx as u32) as usize;
            if settings.allowed_source_mask.data[j] == 0 || settings.target_mask.data[j] != 0 {
                return None;
            }
            let a = original.get_pixel(known % width, known / width).0;
            let b = original.get_pixel(sx as u32, sy as u32).0;
            for c in 0..3 {
                let delta =
                    (f32::from(a[c]) * f32::from(a[3]) - f32::from(b[c]) * f32::from(b[3])) / 255.;
                score += delta * delta;
            }
            let alpha = f32::from(a[3]) - f32::from(b[3]);
            score += alpha * alpha;
            samples += 1;
        }
    }
    (samples > 0).then(|| score / samples as f32)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "advanced operation cancelled"
    );
    Ok(())
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn lerp_byte(a: u8, b: u8, t: f32) -> u8 {
    byte(lerp(f32::from(a) / 255.0, f32::from(b) / 255.0, t))
}
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
fn add_residual(value: u8, residual: i16) -> u8 {
    (i16::from(value) + residual).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn node(operation: AdvancedOperation) -> FilterNode {
        FilterNode {
            id: "test".into(),
            name: "Test".into(),
            enabled: true,
            opacity: 1.0,
            operation,
            soft_mask: None,
        }
    }

    fn uniformity() -> TargetColourUniformity {
        TargetColourUniformity {
            target_rgb: [255, 9, 0],
            hue_range_degrees: 10.,
            hue_falloff_degrees: 20.,
            hue_uniformity: 1.,
            saturation_uniformity: 0.,
            lightness_uniformity: 0.,
        }
    }

    #[test]
    fn target_colour_wrap_falloff_and_outside_range_are_independent() {
        let settings = uniformity();
        let at = |degrees: f64| hsl_to_rgb([degrees / 360., 0.8, 0.5]);
        let target_hue =
            rgb_to_hsl(settings.target_rgb.map(|value| f64::from(value) / 255.))[0] * 360.;
        let inside = rgb_to_hsl(target_colour_uniformity_rgb(at(358.), &settings));
        let feathered = rgb_to_hsl(target_colour_uniformity_rgb(at(22.), &settings));
        let outside = at(90.);
        assert!(
            ((inside[0] * 360. - target_hue).rem_euclid(360.))
                .min((target_hue - inside[0] * 360.).rem_euclid(360.))
                < 0.1,
            "wrap-around target hue: {inside:?}"
        );
        assert!(
            (11.5..12.3).contains(&(feathered[0] * 360.)),
            "half-falloff hue: {feathered:?}"
        );
        assert_eq!(target_colour_uniformity_rgb(outside, &settings), outside);

        // 348° toward 12° by one half takes the short arc through 0° red.
        // These RGB triples are analytic HSL endpoints, independent of the
        // conversion helpers used by the implementation.
        let halfway = target_colour_uniformity_rgb(
            [1., 0., 0.2],
            &TargetColourUniformity {
                target_rgb: [255, 51, 0],
                hue_range_degrees: 180.,
                hue_falloff_degrees: 0.,
                hue_uniformity: 0.5,
                saturation_uniformity: 0.,
                lightness_uniformity: 0.,
            },
        );
        assert!((halfway[0] - 1.).abs() < 1e-12, "{halfway:?}");
        assert!(halfway[1].abs() < 1e-12, "{halfway:?}");
        assert!(halfway[2].abs() < 1e-12, "{halfway:?}");
    }

    #[test]
    fn target_colour_full_uniformity_hits_the_reference_rgb_exactly() {
        let settings = TargetColourUniformity {
            target_rgb: [51, 153, 204],
            hue_range_degrees: 180.,
            hue_falloff_degrees: 0.,
            hue_uniformity: 1.,
            saturation_uniformity: 1.,
            lightness_uniformity: 1.,
        };
        let source = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 20, 91]));
        let output = evaluate(
            &source,
            &[node(AdvancedOperation::TargetColourUniformity(settings))],
        )
        .unwrap();
        assert_eq!(output.get_pixel(0, 0).0, [51, 153, 204, 91]);
    }

    #[test]
    fn target_colour_keeps_neutrals_and_lightness_texture_when_requested() {
        let settings = TargetColourUniformity {
            target_rgb: [210, 100, 60],
            hue_range_degrees: 180.,
            hue_falloff_degrees: 0.,
            hue_uniformity: 1.,
            saturation_uniformity: 1.,
            lightness_uniformity: 0.,
        };
        let gray = [0.4; 3];
        assert_eq!(target_colour_uniformity_rgb(gray, &settings), gray);
        for lightness in [0.21, 0.43, 0.77] {
            let source = hsl_to_rgb([25. / 360., 0.55, lightness]);
            let result = rgb_to_hsl(target_colour_uniformity_rgb(source, &settings));
            assert!((result[2] - lightness).abs() < 1e-12, "{result:?}");
        }
    }

    #[test]
    fn target_colour_validates_bounds_and_preserves_alpha_masks_and_noop() {
        let mut settings = uniformity();
        assert!(
            validate_operation_dimensions(
                (2, 1),
                &AdvancedOperation::TargetColourUniformity(settings)
            )
            .is_ok()
        );
        settings.target_rgb = [128; 3];
        assert!(
            validate_operation_dimensions(
                (2, 1),
                &AdvancedOperation::TargetColourUniformity(settings)
            )
            .is_err()
        );
        settings = uniformity();
        settings.hue_range_degrees = 170.;
        settings.hue_falloff_degrees = 20.;
        assert!(
            validate_operation_dimensions(
                (2, 1),
                &AdvancedOperation::TargetColourUniformity(settings)
            )
            .is_err()
        );

        let source = RgbaImage::from_pixel(2, 1, Rgba([210, 90, 50, 73]));
        let mut entry = node(AdvancedOperation::TargetColourUniformity(uniformity()));
        entry.opacity = 0.5;
        entry.soft_mask = Some(SoftMask::new(2, 1, vec![0, 255]).unwrap());
        let output = evaluate(&source, &[entry]).unwrap();
        assert_eq!(output.get_pixel(0, 0), source.get_pixel(0, 0));
        assert_ne!(output.get_pixel(1, 0).0[..3], source.get_pixel(1, 0).0[..3]);
        assert_eq!(output.get_pixel(1, 0)[3], 73);

        let mut no_op = uniformity();
        no_op.hue_uniformity = 0.;
        let unchanged = evaluate(
            &source,
            &[node(AdvancedOperation::TargetColourUniformity(no_op))],
        )
        .unwrap();
        assert_eq!(unchanged, source);
        assert!(
            evaluate_cancellable(
                &source,
                &[node(
                    AdvancedOperation::TargetColourUniformity(uniformity())
                )],
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }

    #[test]
    fn split_ranges_are_smooth_and_bounded() {
        let range = BlendIfRange {
            black: 0.1,
            black_split: 0.3,
            white_split: 0.7,
            white: 0.9,
        };
        assert_eq!(split_coverage(0.0, range), 0.0);
        assert_eq!(split_coverage(1.0, range), 0.0);
        assert!((split_coverage(0.5, range) - 1.0).abs() < 1e-6);
        assert!(split_coverage(0.2, range) > 0.0 && split_coverage(0.2, range) < 1.0);
    }

    #[test]
    fn blend_if_uses_source_and_backdrop() {
        let settings = BlendIf {
            source_channel: BlendIfChannel::Red,
            backdrop_channel: BlendIfChannel::Blue,
            source: BlendIfRange {
                black: 0.0,
                black_split: 0.5,
                white_split: 1.0,
                white: 1.0,
            },
            backdrop: BlendIfRange::default(),
        };
        assert_eq!(
            blend_if_coverage([0, 0, 0, 255], [255, 0, 0, 255], &settings),
            0.0
        );
        assert!(blend_if_coverage([255, 0, 0, 255], [0, 0, 255, 255], &settings) > 0.99);
    }

    #[test]
    fn frequency_residual_reconstructs_exactly() {
        let source = RgbaImage::from_fn(7, 3, |x, y| {
            Rgba([
                (x * 37 + y * 11) as u8,
                (x * 13) as u8,
                (y * 71) as u8,
                (x * 19 + 20) as u8,
            ])
        });
        let layers = separate_frequencies(&source, 3).unwrap();
        assert_eq!(layers.reconstruct().unwrap(), source);
    }

    #[test]
    fn alpha_and_protected_warp_pixels_are_preserved() {
        let source = RgbaImage::from_fn(3, 1, |x, _| {
            Rgba([x as u8 * 80, 40, 20, if x == 0 { 17 } else { 255 }])
        });
        let mask = SoftMask::new(3, 1, vec![255, 0, 0]).unwrap();
        let warp = WarpMesh {
            columns: 2,
            rows: 2,
            points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            pins: vec![WarpPin {
                source: [0.5, 0.5],
                target: [0.9, 0.5],
                radius: 1.0,
                strength: 1.0,
            }],
            freeze_mask: None,
            protect_mask: Some(mask),
        };
        let output = evaluate(&source, &[node(AdvancedOperation::Warp(warp))]).unwrap();
        assert_eq!(output.get_pixel(0, 0).0, source.get_pixel(0, 0).0);
        assert_eq!(output.get_pixel(0, 0)[3], 17);
    }

    #[test]
    fn cancellation_is_reported_before_writing() {
        let source = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
        let cancelled = AtomicBool::new(true);
        let result = evaluate_cancellable(
            &source,
            &[node(AdvancedOperation::Filter(filters::Filter::Invert))],
            &cancelled,
        );
        assert!(result.is_err());
        assert_eq!(source, RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255])));
    }

    #[test]
    fn invalid_nodes_are_rejected_before_evaluation() {
        let source = RgbaImage::new(1, 1);
        let mut n = node(AdvancedOperation::Denoise {
            radius: 1,
            strength: 1.0,
        });
        n.opacity = 2.0;
        assert!(validate_stack(&source, &[n]).is_err());
    }

    #[test]
    fn dimension_validation_does_not_require_a_raster() {
        let mask = SoftMask::new(2, 2, vec![255; 4]).unwrap();
        let mut n = node(AdvancedOperation::Denoise {
            radius: 0,
            strength: 0.0,
        });
        n.soft_mask = Some(mask);
        assert!(validate_stack_dimensions(2, 2, &[n]).is_ok());
        assert!(
            validate_stack_dimensions(
                3,
                2,
                &[node(AdvancedOperation::Denoise {
                    radius: 0,
                    strength: 0.0,
                })]
            )
            .is_ok()
        );
    }

    #[test]
    fn owned_bytes_accounts_operation_and_node_masks() {
        let target = SoftMask::new(2, 2, vec![255; 4]).unwrap();
        let allowed = SoftMask::new(2, 2, vec![0; 4]).unwrap();
        let operation = AdvancedOperation::ContentAwareReplace(ContentAwareReplace {
            algorithm: ContentAwareAlgorithm::Legacy,
            target_mask: target,
            allowed_source_mask: allowed,
            search_radius: 1,
            patch_radius: 0,
            feather: 0.0,
        });
        assert_eq!(operation.owned_bytes(), 8);
        let mut entry = node(operation);
        entry.soft_mask = Some(SoftMask::new(2, 2, vec![255; 4]).unwrap());
        assert_eq!(entry.owned_bytes(), 12);
        assert_eq!(FilterStack { nodes: vec![entry] }.owned_bytes(), 12);
    }

    #[test]
    fn content_replacement_requires_both_masks() {
        let source = RgbaImage::from_fn(4, 1, |x, _| Rgba([x as u8 * 50, 0, 0, 255]));
        let target = SoftMask::new(4, 1, vec![0, 255, 0, 0]).unwrap();
        let allowed = SoftMask::new(4, 1, vec![255, 0, 0, 0]).unwrap();
        let op = AdvancedOperation::ContentAwareReplace(ContentAwareReplace {
            algorithm: ContentAwareAlgorithm::Legacy,
            target_mask: target,
            allowed_source_mask: allowed,
            search_radius: 2,
            patch_radius: 0,
            feather: 0.0,
        });
        let output = evaluate(&source, &[node(op)]).unwrap();
        assert_eq!(output.get_pixel(1, 0)[0], 0);
    }

    #[test]
    fn content_search_rejects_unbounded_minimum_work() {
        let target = SoftMask::new(1_000, 1_000, vec![255; 1_000_000]).unwrap();
        let allowed = SoftMask::new(1_000, 1_000, vec![255; 1_000_000]).unwrap();
        let settings = ContentAwareReplace {
            algorithm: ContentAwareAlgorithm::Legacy,
            target_mask: target,
            allowed_source_mask: allowed,
            search_radius: 64,
            patch_radius: 4,
            feather: 0.0,
        };
        assert!(content_candidate_budget(&settings).is_err());
    }

    #[test]
    fn dodge_burn_accepts_the_ui_tonal_radius_limit() {
        let operation = AdvancedOperation::DodgeBurn(DodgeBurn {
            mode: DodgeBurnMode::Dodge,
            amount: 0.5,
            radius: 128,
        });
        assert!(validate_operation_dimensions((1, 1), &operation).is_ok());
    }
}

#[cfg(test)]
#[path = "advanced_ops_content_tests.rs"]
mod content_tests;
