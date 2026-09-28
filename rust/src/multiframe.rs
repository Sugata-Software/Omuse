//! Deterministic, bounded multi-image registration and compositing.

use crate::precision::{Rgba16, TiledRgba16, WorkingSpace};
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Multi-image operations use the same full-resolution ceiling as advanced
/// document sources. Alignment may inspect fewer pixels through its bounded
/// sampling option, but it never accepts a larger source surface.
pub const MAX_MULTI_IMAGE_PIXELS: u64 = 16_777_216;
pub const MAX_MULTI_IMAGE_FRAMES: usize = 256;

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
pub struct MultiImageOptions {
    pub max_shift: i32,
    pub min_overlap: f32,
    pub min_score: f32,
    pub max_alignment_pixels: u64,
    pub max_output_pixels: u64,
    pub max_memory_bytes: usize,
    pub cancellation: Option<CancellationToken>,
}
impl Default for MultiImageOptions {
    fn default() -> Self {
        Self {
            max_shift: 64,
            min_overlap: 0.15,
            min_score: 0.45,
            max_alignment_pixels: 262_144,
            max_output_pixels: MAX_MULTI_IMAGE_PIXELS,
            max_memory_bytes: 768 * 1024 * 1024,
            cancellation: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Translation {
    pub dx: i32,
    pub dy: i32,
    pub score: f32,
    pub overlap: u64,
}

#[derive(Clone, Debug)]
pub struct ExposureFrame {
    pub image: TiledRgba16,
    pub exposure_stops: f32,
}

#[derive(Clone, Debug)]
pub struct FocusFrame {
    pub image: TiledRgba16,
}

#[derive(Clone, Debug)]
pub struct MultiImageResult {
    pub image: TiledRgba16,
    pub alignments: Vec<Translation>,
}

#[derive(Clone, Debug)]
pub struct RadianceImage {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}
impl RadianceImage {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn get_pixel(&self, x: u32, y: u32) -> [f32; 4] {
        self.pixels[y as usize * self.width as usize + x as usize]
    }
    pub fn tone_map_preview(&self, exposure: f32) -> Result<RgbaImage> {
        self.tone_map_preview_with_cancel(exposure, None)
    }
    pub fn tone_map_preview_with_cancel(
        &self,
        exposure: f32,
        cancellation: Option<&CancellationToken>,
    ) -> Result<RgbaImage> {
        ensure!(exposure.is_finite(), "invalid tone-map exposure");
        let scale = 2.0f32.powf(exposure);
        let mut output = RgbaImage::new(self.width, self.height);
        for y in 0..self.height {
            check_cancel(cancellation)?;
            for x in 0..self.width {
                let p = self.get_pixel(x, y);
                let mut pixel = [0; 4];
                for c in 0..3 {
                    let value = (p[c].max(0.) * scale) / (1. + p[c].max(0.) * scale);
                    pixel[c] = (value.clamp(0., 1.) * 255.).round() as u8;
                }
                pixel[3] = (p[3].clamp(0., 1.) * 255.).round() as u8;
                output.put_pixel(x, y, Rgba(pixel));
            }
        }
        Ok(output)
    }
    pub fn to_linear_tiled(&self, scale: f32) -> Result<TiledRgba16> {
        self.to_linear_tiled_with_cancel(scale, None)
    }
    pub fn to_linear_tiled_with_cancel(
        &self,
        scale: f32,
        cancellation: Option<&CancellationToken>,
    ) -> Result<TiledRgba16> {
        ensure!(
            scale.is_finite() && scale > 0.,
            "radiance scale must be positive"
        );
        ensure!(
            u64::from(self.width) * u64::from(self.height) <= MAX_MULTI_IMAGE_PIXELS,
            "radiance image exceeds the 16 megapixel limit"
        );
        let mut output = TiledRgba16::new(self.width, self.height, WorkingSpace::LinearSrgb)?;
        for y in 0..self.height {
            check_cancel(cancellation)?;
            for x in 0..self.width {
                let p = self.get_pixel(x, y);
                output.set_pixel(
                    x,
                    y,
                    Rgba16(
                        [
                            (p[0].max(0.) / scale).clamp(0., 1.) * 65_535. as f32,
                            (p[1].max(0.) / scale).clamp(0., 1.) * 65_535. as f32,
                            (p[2].max(0.) / scale).clamp(0., 1.) * 65_535. as f32,
                            p[3].clamp(0., 1.) * 65_535. as f32,
                        ]
                        .map(|v| v.round() as u16),
                    ),
                )?;
            }
        }
        Ok(output)
    }
}

pub fn align_translation(
    reference: &TiledRgba16,
    candidate: &TiledRgba16,
    options: MultiImageOptions,
) -> Result<Translation> {
    ensure!(
        reference.dimensions() == candidate.dimensions(),
        "alignment requires equal dimensions"
    );
    ensure!(
        reference.working_space() == candidate.working_space(),
        "alignment requires matching working spaces"
    );
    validate_options(&options)?;
    let (width, height) = reference.dimensions();
    let area = u64::from(width) * u64::from(height);
    ensure!(
        area <= MAX_MULTI_IMAGE_PIXELS,
        "alignment image exceeds the 16 megapixel limit"
    );
    let max_shift = options.max_shift.unsigned_abs() as u32;
    // A coarse grid keeps the search bounded even at the maximum 1024-pixel
    // shift.  A second local pass recovers the exact integer translation.
    let span = max_shift.saturating_mul(2).saturating_add(1) as u64;
    let coarse_count = span.saturating_mul(span);
    let coarse_stride = ((coarse_count as f64 / MAX_ALIGNMENT_CANDIDATES as f64)
        .sqrt()
        .ceil() as i32)
        .max(1);
    let coarse_axis = axis_shifts(max_shift as i32, coarse_stride);
    let coarse_shifts = coarse_axis
        .iter()
        .flat_map(|&dy| coarse_axis.iter().map(move |&dx| (dx, dy)))
        .collect::<Vec<_>>();
    let coarse_candidates = coarse_shifts.len() as u64;
    let coarse_budget = (MAX_ALIGNMENT_COMPARISONS / coarse_candidates.max(1)).max(16);
    let coarse_target = options.max_alignment_pixels.min(area).min(coarse_budget);
    let coarse_samples = sample_luma(reference, coarse_target, options.cancellation.as_ref())?;
    ensure!(
        coarse_samples.len() >= 2,
        "alignment sampling produced too few pixels"
    );
    ensure!(
        texture_variance(&coarse_samples) > 1e-7,
        "alignment has insufficient image texture"
    );
    let minimum_overlap = minimum_sample_overlap(coarse_samples.len(), options.min_overlap);
    let mut best = search_translation(
        reference,
        candidate,
        &coarse_samples,
        &coarse_shifts,
        minimum_overlap,
        &options,
    )?
    .ok_or_else(|| anyhow::anyhow!("images have insufficient overlap"))?;

    let refine_radius = coarse_stride;
    let refine_shifts = local_shifts(best.dx, best.dy, refine_radius, max_shift as i32);
    let refine_candidates = refine_shifts.len() as u64;
    let refine_budget = (MAX_ALIGNMENT_COMPARISONS / refine_candidates.max(1)).max(16);
    let refine_target = options.max_alignment_pixels.min(area).min(refine_budget);
    let refine_samples = sample_luma(reference, refine_target, options.cancellation.as_ref())?;
    let refine_minimum = minimum_sample_overlap(refine_samples.len(), options.min_overlap);
    if let Some(refined) = search_translation(
        reference,
        candidate,
        &refine_samples,
        &refine_shifts,
        refine_minimum,
        &options,
    )? {
        best = refined;
    }
    ensure!(
        best.score >= options.min_score,
        "images have weak registration overlap"
    );
    Ok(best)
}

const MAX_ALIGNMENT_CANDIDATES: u64 = 4_096;
const MAX_ALIGNMENT_COMPARISONS: u64 = 8_000_000;
const MAX_ALIGNMENT_SAMPLE_PIXELS: u64 = 1_048_576;

fn axis_shifts(max_shift: i32, stride: i32) -> Vec<i32> {
    let mut values = Vec::new();
    let mut value = -max_shift;
    while value <= max_shift {
        values.push(value);
        value = value.saturating_add(stride.max(1));
    }
    if *values.last().unwrap_or(&0) != max_shift {
        values.push(max_shift);
    }
    if !values.contains(&0) {
        values.push(0);
        values.sort_unstable();
    }
    values
}

fn local_shifts(dx: i32, dy: i32, radius: i32, max_shift: i32) -> Vec<(i32, i32)> {
    let lo_x = (dx - radius).max(-max_shift);
    let hi_x = (dx + radius).min(max_shift);
    let lo_y = (dy - radius).max(-max_shift);
    let hi_y = (dy + radius).min(max_shift);
    let mut values = Vec::new();
    for y in lo_y..=hi_y {
        for x in lo_x..=hi_x {
            values.push((x, y));
        }
    }
    values
}

fn sample_luma(
    image: &TiledRgba16,
    target: u64,
    cancellation: Option<&CancellationToken>,
) -> Result<Vec<(u32, u32, f32)>> {
    let (width, height) = image.dimensions();
    let area = u64::from(width) * u64::from(height);
    let target = target.max(1).min(area).min(MAX_ALIGNMENT_SAMPLE_PIXELS);
    let step = ((area as f64 / target as f64).sqrt().ceil() as u32).max(1);
    let mut samples = Vec::with_capacity(target as usize);
    for y in (0..height).step_by(step as usize) {
        check_cancel(cancellation)?;
        for x in (0..width).step_by(step as usize) {
            samples.push((x, y, luma(image.get_pixel(x, y))));
        }
    }
    Ok(samples)
}

fn texture_variance(samples: &[(u32, u32, f32)]) -> f32 {
    let count = samples.len().max(1) as f32;
    let mean = samples.iter().map(|(_, _, value)| value).sum::<f32>() / count;
    samples
        .iter()
        .map(|(_, _, value)| (value - mean).powi(2))
        .sum::<f32>()
        / count
}

fn minimum_sample_overlap(sample_count: usize, fraction: f32) -> u64 {
    ((sample_count as f32 * fraction).ceil() as u64).max(1)
}

fn search_translation(
    reference: &TiledRgba16,
    candidate: &TiledRgba16,
    samples: &[(u32, u32, f32)],
    shifts: &[(i32, i32)],
    minimum_overlap: u64,
    options: &MultiImageOptions,
) -> Result<Option<Translation>> {
    let (width, height) = reference.dimensions();
    let mut best = None;
    for &(dx, dy) in shifts {
        check_cancel(options.cancellation.as_ref())?;
        let mut sum_a = 0.;
        let mut sum_b = 0.;
        let mut sum_aa = 0.;
        let mut sum_bb = 0.;
        let mut sum_ab = 0.;
        let mut overlap = 0u64;
        for &(x, y, a) in samples {
            let cx = x as i32 - dx;
            let cy = y as i32 - dy;
            if cx < 0 || cy < 0 || cx >= width as i32 || cy >= height as i32 {
                continue;
            }
            let b = luma(candidate.get_pixel(cx as u32, cy as u32));
            sum_a += a;
            sum_b += b;
            sum_aa += a * a;
            sum_bb += b * b;
            sum_ab += a * b;
            overlap += 1;
        }
        if overlap < minimum_overlap {
            continue;
        }
        let n = overlap as f32;
        let covariance = sum_ab - sum_a * sum_b / n;
        let variance_a = (sum_aa - sum_a * sum_a / n).max(0.);
        let variance_b = (sum_bb - sum_b * sum_b / n).max(0.);
        if variance_a <= 1e-7 || variance_b <= 1e-7 {
            continue;
        }
        let score = (covariance / (variance_a * variance_b).sqrt()).clamp(-1., 1.);
        let candidate_translation = Translation {
            dx,
            dy,
            score,
            overlap,
        };
        if best.is_none_or(|old: Translation| {
            score > old.score
                || (score == old.score
                    && (dy.abs(), dx.abs(), dy, dx) < (old.dy.abs(), old.dx.abs(), old.dy, old.dx))
        }) {
            best = Some(candidate_translation);
        }
    }
    Ok(best)
}

pub fn merge_exposure(
    frames: &[ExposureFrame],
    options: MultiImageOptions,
) -> Result<(RadianceImage, Vec<Translation>)> {
    ensure!(!frames.is_empty(), "HDR merge needs at least one frame");
    ensure!(
        frames.len() <= MAX_MULTI_IMAGE_FRAMES,
        "HDR merge has too many frames"
    );
    validate_options(&options)?;
    let (width, height) = frames[0].image.dimensions();
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_MULTI_IMAGE_PIXELS,
        "HDR image exceeds the 16 megapixel limit"
    );
    let pixels = u64::from(width) * u64::from(height);
    let source_bytes = frames
        .iter()
        .try_fold(0u64, |sum, frame| {
            sum.checked_add(frame.image.logical_bytes() as u64)
        })
        .ok_or_else(|| anyhow::anyhow!("HDR source memory overflow"))?;
    let conversion_bytes = pixels
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_mul(frames.len() as u64))
        .ok_or_else(|| anyhow::anyhow!("HDR conversion memory overflow"))?;
    let radiance_bytes = pixels
        .checked_mul(16)
        .ok_or_else(|| anyhow::anyhow!("HDR radiance memory overflow"))?;
    ensure!(
        source_bytes
            .saturating_add(conversion_bytes)
            .saturating_add(radiance_bytes)
            .saturating_add(alignment_scratch_budget(&options))
            <= options.max_memory_bytes as u64,
        "HDR radiance exceeds memory limit"
    );
    let mut linear = Vec::with_capacity(frames.len());
    for frame in frames {
        ensure!(
            frame.image.dimensions() == (width, height),
            "HDR frames must have equal dimensions"
        );
        ensure!(
            frame.exposure_stops.is_finite() && (-30. ..=30.).contains(&frame.exposure_stops),
            "invalid exposure value"
        );
        check_cancel(options.cancellation.as_ref())?;
        let mut copy = frame.image.clone();
        copy.convert_working_space(WorkingSpace::LinearSrgb)?;
        check_cancel(options.cancellation.as_ref())?;
        linear.push((copy, frame.exposure_stops));
    }
    let mut alignments = vec![Translation {
        dx: 0,
        dy: 0,
        score: 1.,
        overlap: u64::from(width) * u64::from(height),
    }];
    for (image, _) in linear.iter().skip(1) {
        alignments.push(align_translation(&linear[0].0, image, options.clone())?);
    }
    let mut pixels = vec![[0.; 4]; width as usize * height as usize];
    for y in 0..height {
        for x in 0..width {
            check_cancel(options.cancellation.as_ref())?;
            let mut sum = [0.; 4];
            let mut weight_sum = 0.;
            for ((image, exposure), alignment) in linear.iter().zip(&alignments) {
                let sx = x as i32 - alignment.dx;
                let sy = y as i32 - alignment.dy;
                if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                    continue;
                }
                let p = image.get_pixel(sx as u32, sy as u32);
                let rgb = [
                    f32::from(p.0[0]) / 65_535.,
                    f32::from(p.0[1]) / 65_535.,
                    f32::from(p.0[2]) / 65_535.,
                ];
                let luminance = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let weight = (1. - ((luminance - 0.5).abs() * 2.)).clamp(0.05, 1.);
                let alpha = f32::from(p.0[3]) / 65_535.;
                let exposure = 2.0f32.powf(*exposure);
                for c in 0..3 {
                    sum[c] += rgb[c] / exposure * alpha * weight;
                }
                sum[3] += alpha * weight;
                weight_sum += weight;
            }
            if weight_sum > 0. {
                let alpha = (sum[3] / weight_sum).clamp(0., 1.);
                if sum[3] > 0. {
                    let alpha_weight = sum[3];
                    for value in &mut sum[..3] {
                        *value /= alpha_weight;
                    }
                }
                sum[3] = alpha;
            }
            pixels[y as usize * width as usize + x as usize] = sum;
        }
    }
    Ok((
        RadianceImage {
            width,
            height,
            pixels,
        },
        alignments,
    ))
}

pub fn focus_stack(frames: &[FocusFrame], options: MultiImageOptions) -> Result<MultiImageResult> {
    ensure!(!frames.is_empty(), "focus stack needs at least one frame");
    ensure!(
        frames.len() <= MAX_MULTI_IMAGE_FRAMES,
        "focus stack has too many frames"
    );
    validate_options(&options)?;
    let (width, height) = frames[0].image.dimensions();
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_MULTI_IMAGE_PIXELS,
        "focus image exceeds the 16 megapixel limit"
    );
    let source_bytes = frames
        .iter()
        .try_fold(0u64, |sum, frame| {
            sum.checked_add(frame.image.logical_bytes() as u64)
        })
        .ok_or_else(|| anyhow::anyhow!("focus source memory overflow"))?;
    ensure!(
        source_bytes
            .saturating_add(u64::from(width) * u64::from(height) * 8)
            .saturating_add(alignment_scratch_budget(&options))
            <= options.max_memory_bytes as u64,
        "focus stack exceeds memory limit"
    );
    let mut images = Vec::with_capacity(frames.len());
    for frame in frames {
        ensure!(
            frame.image.dimensions() == (width, height),
            "focus frames must have equal dimensions"
        );
        ensure!(
            frame.image.working_space() == frames[0].image.working_space(),
            "focus frames must use the same working space"
        );
        images.push(frame.image.clone());
    }
    let mut alignments = vec![Translation {
        dx: 0,
        dy: 0,
        score: 1.,
        overlap: u64::from(width) * u64::from(height),
    }];
    for image in images.iter().skip(1) {
        alignments.push(align_translation(&images[0], image, options.clone())?);
    }
    let mut output = TiledRgba16::new(width, height, images[0].working_space())?;
    for y in 0..height {
        for x in 0..width {
            check_cancel(options.cancellation.as_ref())?;
            let mut sum = [0f32; 4];
            let mut total = 0f32;
            for (image, alignment) in images.iter().zip(&alignments) {
                let sx = x as i32 - alignment.dx;
                let sy = y as i32 - alignment.dy;
                if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                    continue;
                }
                let p = image.get_pixel(sx as u32, sy as u32);
                let sharp = local_sharpness(image, sx as u32, sy as u32).max(0.001);
                let alpha = f32::from(p.0[3]) / 65_535.;
                for c in 0..3 {
                    sum[c] += f32::from(p.0[c]) * alpha * sharp;
                }
                sum[3] += alpha * sharp;
                total += sharp;
            }
            let mut p = [0u16; 4];
            if total > 0. {
                p[3] = (sum[3] / total * 65_535.).round().clamp(0., 65_535.) as u16;
                if sum[3] > 0. {
                    for c in 0..3 {
                        p[c] = (sum[c] / sum[3]).round().clamp(0., 65_535.) as u16;
                    }
                }
            }
            output.set_pixel(x, y, Rgba16(p))?;
        }
    }
    Ok(MultiImageResult {
        image: output,
        alignments,
    })
}

pub fn panorama(frames: &[TiledRgba16], options: MultiImageOptions) -> Result<MultiImageResult> {
    ensure!(!frames.is_empty(), "panorama needs at least one frame");
    ensure!(
        frames.len() <= MAX_MULTI_IMAGE_FRAMES,
        "panorama has too many frames"
    );
    validate_options(&options)?;
    let (width, height) = frames[0].dimensions();
    for frame in frames {
        ensure!(
            frame.dimensions() == (width, height),
            "panorama frames must have equal dimensions"
        );
        ensure!(
            frame.working_space() == frames[0].working_space(),
            "panorama frames must use the same working space"
        );
    }
    let source_bytes = frames
        .iter()
        .try_fold(0u64, |sum, frame| {
            sum.checked_add(frame.logical_bytes() as u64)
        })
        .ok_or_else(|| anyhow::anyhow!("panorama source memory overflow"))?;
    let max_span = u64::from(options.max_shift as u32).saturating_mul(2);
    let worst_width = u64::from(width).saturating_add(max_span);
    let worst_height = u64::from(height).saturating_add(max_span);
    let worst_output_bytes = worst_width
        .checked_mul(worst_height)
        .and_then(|pixels| pixels.checked_mul(8))
        .ok_or_else(|| anyhow::anyhow!("panorama worst-case memory overflow"))?;
    ensure!(
        source_bytes
            .saturating_add(worst_output_bytes)
            .saturating_add(alignment_scratch_budget(&options))
            <= options.max_memory_bytes as u64,
        "panorama worst-case memory exceeds limit"
    );
    let mut alignments = vec![Translation {
        dx: 0,
        dy: 0,
        score: 1.,
        overlap: u64::from(width) * u64::from(height),
    }];
    for frame in frames.iter().skip(1) {
        alignments.push(align_translation(&frames[0], frame, options.clone())?);
    }
    let min_x = alignments.iter().map(|a| a.dx).min().unwrap_or(0).min(0);
    let min_y = alignments.iter().map(|a| a.dy).min().unwrap_or(0).min(0);
    let max_x = alignments.iter().map(|a| a.dx).max().unwrap_or(0).max(0);
    let max_y = alignments.iter().map(|a| a.dy).max().unwrap_or(0).max(0);
    let out_width = (i64::from(width) + i64::from(max_x) - i64::from(min_x)) as u32;
    let out_height = (i64::from(height) + i64::from(max_y) - i64::from(min_y)) as u32;
    ensure!(
        u64::from(out_width) * u64::from(out_height) <= options.max_output_pixels
            && u64::from(out_width) * u64::from(out_height) <= MAX_MULTI_IMAGE_PIXELS,
        "panorama exceeds output pixel limit"
    );
    let output_bytes = u64::from(out_width)
        .checked_mul(u64::from(out_height))
        .and_then(|pixels| pixels.checked_mul(8))
        .ok_or_else(|| anyhow::anyhow!("panorama output memory overflow"))?;
    ensure!(
        source_bytes
            .saturating_add(output_bytes)
            .saturating_add(alignment_scratch_budget(&options))
            <= options.max_memory_bytes as u64,
        "panorama exceeds memory limit"
    );
    let mut output = TiledRgba16::new(out_width, out_height, frames[0].working_space())?;
    for y in 0..out_height {
        for x in 0..out_width {
            check_cancel(options.cancellation.as_ref())?;
            let world_x = x as i32 + min_x;
            let world_y = y as i32 + min_y;
            let mut sum = [0f32; 4];
            let mut total = 0.;
            for (frame, alignment) in frames.iter().zip(&alignments) {
                let sx = world_x - alignment.dx;
                let sy = world_y - alignment.dy;
                if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                    continue;
                }
                let edge = (sx
                    .min(width as i32 - 1 - sx)
                    .min(sy)
                    .min(height as i32 - 1 - sy) as f32
                    + 1.)
                    .max(1.);
                let p = frame.get_pixel(sx as u32, sy as u32);
                let alpha = f32::from(p.0[3]) / 65_535.;
                for c in 0..3 {
                    sum[c] += f32::from(p.0[c]) * alpha * edge;
                }
                sum[3] += alpha * edge;
                total += edge;
            }
            if total > 0. {
                let alpha = (sum[3] / total).clamp(0., 1.);
                let mut pixel = [0u16; 4];
                pixel[3] = (alpha * 65_535.).round() as u16;
                if sum[3] > 0. {
                    for c in 0..3 {
                        pixel[c] = (sum[c] / sum[3]).round().clamp(0., 65_535.) as u16;
                    }
                }
                output.set_pixel(x, y, Rgba16(pixel))?;
            }
        }
    }
    Ok(MultiImageResult {
        image: output,
        alignments,
    })
}

fn validate_options(options: &MultiImageOptions) -> Result<()> {
    ensure!(
        options.max_shift >= 0 && options.max_shift <= 1024,
        "invalid alignment search bound"
    );
    ensure!(
        options.min_overlap.is_finite()
            && (0. ..=1.).contains(&options.min_overlap)
            && options.min_overlap > 0.,
        "invalid overlap threshold"
    );
    ensure!(
        options.min_score.is_finite() && (-1. ..=1.).contains(&options.min_score),
        "invalid registration score threshold"
    );
    ensure!(
        options.max_alignment_pixels > 0
            && options.max_alignment_pixels <= MAX_ALIGNMENT_SAMPLE_PIXELS
            && options.max_output_pixels > 0
            && options.max_memory_bytes > 0,
        "invalid multi-image limits"
    );
    Ok(())
}
fn alignment_scratch_budget(options: &MultiImageOptions) -> u64 {
    options
        .max_alignment_pixels
        .min(MAX_ALIGNMENT_SAMPLE_PIXELS)
        .saturating_mul(32)
}
fn check_cancel(token: Option<&CancellationToken>) -> Result<()> {
    ensure!(
        !token.is_some_and(CancellationToken::is_cancelled),
        "multi-image operation cancelled"
    );
    Ok(())
}
fn luma(pixel: Rgba16) -> f32 {
    (0.2126 * f32::from(pixel.0[0])
        + 0.7152 * f32::from(pixel.0[1])
        + 0.0722 * f32::from(pixel.0[2]))
        / 65_535.
}
fn local_sharpness(image: &TiledRgba16, x: u32, y: u32) -> f32 {
    let (width, height) = image.dimensions();
    let center = luma(image.get_pixel(x, y));
    let left = x.saturating_sub(1);
    let right = (x + 1).min(width.saturating_sub(1));
    let top = y.saturating_sub(1);
    let bottom = (y + 1).min(height.saturating_sub(1));
    let horizontal =
        (luma(image.get_pixel(left, y)) + luma(image.get_pixel(right, y)) - 2. * center).abs();
    let vertical =
        (luma(image.get_pixel(x, top)) + luma(image.get_pixel(x, bottom)) - 2. * center).abs();
    horizontal + vertical + 0.001
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_recovers_translation_deterministically() {
        let mut a = TiledRgba16::new(32, 24, WorkingSpace::LinearSrgb).unwrap();
        let mut b = TiledRgba16::new(32, 24, WorkingSpace::LinearSrgb).unwrap();
        for y in 2..22 {
            for x in 2..30 {
                let value = (((x * 17 + y * 31 + x * y) % 65_000) as u16).max(1);
                a.set_pixel(x, y, Rgba16([value, value / 2, value / 3, 65_535]))
                    .unwrap();
                if x >= 3 && y >= 1 {
                    b.set_pixel(x - 3, y - 1, Rgba16([value, value / 2, value / 3, 65_535]))
                        .unwrap();
                }
            }
        }
        let result = align_translation(
            &a,
            &b,
            MultiImageOptions {
                max_shift: 5,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((result.dx, result.dy), (3, 1));
    }
    #[test]
    fn constant_fields_and_cancellation_are_refused() {
        let a = TiledRgba16::new(8, 8, WorkingSpace::Srgb).unwrap();
        let b = a.clone();
        assert!(align_translation(&a, &b, Default::default()).is_err());
        let token = CancellationToken::new();
        token.cancel();
        let error = align_translation(
            &a,
            &b,
            MultiImageOptions {
                cancellation: Some(token),
                ..Default::default()
            },
        );
        assert!(error.is_err());
    }
    #[test]
    fn exposure_merge_retains_alpha_and_produces_tonemap_preview() {
        let mut a = TiledRgba16::new(4, 4, WorkingSpace::LinearSrgb).unwrap();
        let mut b = a.clone();
        a.set_pixel(1, 1, Rgba16([20_000, 10_000, 5_000, 40_000]))
            .unwrap();
        b.set_pixel(1, 1, Rgba16([40_000, 20_000, 10_000, 40_000]))
            .unwrap();
        let (radiance, alignments) = merge_exposure(
            &[
                ExposureFrame {
                    image: a,
                    exposure_stops: 0.,
                },
                ExposureFrame {
                    image: b,
                    exposure_stops: 1.,
                },
            ],
            MultiImageOptions {
                min_score: -1.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(alignments.len(), 2);
        assert!(radiance.get_pixel(1, 1)[0] > 0.);
        assert!(radiance.tone_map_preview(0.).unwrap().get_pixel(1, 1)[3] > 0);
    }

    #[test]
    fn registration_uses_sampled_overlap_for_large_sources() {
        let width = 520;
        let height = 520;
        let mut reference = TiledRgba16::new(width, height, WorkingSpace::Srgb).unwrap();
        let mut candidate = TiledRgba16::new(width, height, WorkingSpace::Srgb).unwrap();
        for y in 0..height {
            for x in 0..width {
                let value = ((x * 97 + y * 193 + (x ^ y) * 17) % 65_000 + 1) as u16;
                reference
                    .set_pixel(x, y, Rgba16([value, value / 2, value / 3, 65_535]))
                    .unwrap();
                if x >= 3 && y >= 2 {
                    candidate
                        .set_pixel(x - 3, y - 2, Rgba16([value, value / 2, value / 3, 65_535]))
                        .unwrap();
                }
            }
        }
        let result = align_translation(
            &reference,
            &candidate,
            MultiImageOptions {
                max_shift: 5,
                max_alignment_pixels: 1_024,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((result.dx, result.dy), (3, 2));
        assert!(result.overlap < u64::from(width) * u64::from(height));
    }

    #[test]
    fn focus_identity_keeps_outer_borders() {
        let mut image = TiledRgba16::new(3, 3, WorkingSpace::Srgb).unwrap();
        for y in 0..3 {
            for x in 0..3 {
                image
                    .set_pixel(
                        x,
                        y,
                        Rgba16([
                            1_000 + x as u16 * 100,
                            2_000 + y as u16 * 100,
                            3_000,
                            65_535,
                        ]),
                    )
                    .unwrap();
            }
        }
        let expected = image.to_rgba16();
        let result = focus_stack(&[FocusFrame { image }], MultiImageOptions::default()).unwrap();
        assert_eq!(result.image.to_rgba16(), expected);
    }

    #[test]
    fn panorama_does_not_mix_hidden_rgb_from_transparent_frames() {
        let width = 8;
        let height = 8;
        let mut opaque = TiledRgba16::new(width, height, WorkingSpace::Srgb).unwrap();
        let mut hidden = TiledRgba16::new(width, height, WorkingSpace::Srgb).unwrap();
        for y in 0..height {
            for x in 0..width {
                let value = ((x * 211 + y * 97) % 65_000 + 1) as u16;
                opaque
                    .set_pixel(x, y, Rgba16([value, value / 2, value / 3, 65_535]))
                    .unwrap();
                hidden
                    .set_pixel(x, y, Rgba16([65_535 - value, value, 65_535, 0]))
                    .unwrap();
            }
        }
        let result = panorama(
            &[opaque.clone(), hidden],
            MultiImageOptions {
                max_shift: 0,
                ..Default::default()
            },
        )
        .unwrap();
        for y in 0..height {
            for x in 0..width {
                let actual = result.image.get_pixel(x, y);
                let expected = opaque.get_pixel(x, y);
                assert_eq!(actual.0[0], expected.0[0]);
                assert_eq!(actual.0[1], expected.0[1]);
                assert_eq!(actual.0[2], expected.0[2]);
                assert!(actual.0[3].abs_diff(32_768) <= 1);
            }
        }
    }

    #[test]
    fn panorama_budget_counts_retained_sources() {
        let a = TiledRgba16::new(32, 32, WorkingSpace::Srgb).unwrap();
        let b = a.clone();
        let error = panorama(
            &[a, b],
            MultiImageOptions {
                max_shift: 0,
                max_memory_bytes: 8 * 1024,
                ..Default::default()
            },
        );
        assert!(error.is_err());
    }
}
