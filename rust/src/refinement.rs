//! Cancellable subject matte refinement and edge decontamination.
//!
//! This module is deliberately independent of the editor. The input matte and
//! guide are never modified; callers receive a refined mask and a new image.
//! Brush corrections are applied before the existing guided matte refinement,
//! then a bounded nearest-interior decontamination pass removes colour spill
//! from partial-alpha edges.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};
use image::{GrayImage, Luma, Rgba, RgbaImage};

use crate::matte;

pub const MAX_PIXELS: u64 = 16_777_216;
pub const MAX_BRUSH_CORRECTIONS: usize = 4_096;
pub const MAX_DEFRINGE_RADIUS: u8 = 64;
pub const MAX_BRUSH_WORK: u64 = 64_000_000;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub matte: matte::Settings,
    pub defringe_radius: u8,
    pub defringe_strength: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            matte: matte::Settings::default(),
            defringe_radius: 0,
            defringe_strength: 0.0,
        }
    }
}

/// A soft foreground/background paint correction in image pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushCorrection {
    pub x: u32,
    pub y: u32,
    pub radius: f32,
    pub hardness: f32,
    pub foreground: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Refined {
    pub mask: GrayImage,
    pub image: RgbaImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewBackground {
    Original,
    Black,
    White,
    Checkerboard,
    Mask,
}

pub fn validate_settings(settings: &Settings, corrections: &[BrushCorrection]) -> Result<()> {
    ensure!(
        settings.defringe_radius <= MAX_DEFRINGE_RADIUS,
        "defringe radius exceeds {MAX_DEFRINGE_RADIUS}"
    );
    ensure!(
        settings.defringe_strength.is_finite() && (0.0..=1.0).contains(&settings.defringe_strength),
        "defringe strength must be finite and between 0 and 1"
    );
    ensure!(
        settings.matte.refine.is_finite()
            && (0.0..=100.0).contains(&settings.matte.refine)
            && settings.matte.contrast.is_finite()
            && (0.0..=100.0).contains(&settings.matte.contrast)
            && settings.matte.shift.is_finite()
            && (-100.0..=100.0).contains(&settings.matte.shift),
        "invalid matte settings"
    );
    ensure!(
        corrections.len() <= MAX_BRUSH_CORRECTIONS,
        "brush correction count exceeds {MAX_BRUSH_CORRECTIONS}"
    );
    for correction in corrections {
        ensure!(
            correction.radius.is_finite() && (0.001..=4_096.0).contains(&correction.radius),
            "brush radius must be finite and between 0.001 and 4096"
        );
        ensure!(
            correction.hardness.is_finite() && (0.0..=1.0).contains(&correction.hardness),
            "brush hardness must be finite and between 0 and 1"
        );
    }
    Ok(())
}

/// Apply brush corrections, guided matte refinement, and edge decontamination.
/// Cancellation is checked before every stage and for every processed row.
pub fn refine(
    base_gray: &GrayImage,
    guide_rgba: &RgbaImage,
    settings: &Settings,
    corrections: &[BrushCorrection],
    cancelled: &AtomicBool,
) -> Result<Refined> {
    validate_inputs(base_gray, guide_rgba, settings, corrections)?;
    check_cancelled(cancelled)?;

    let mut corrected = base_gray.clone();
    apply_corrections(&mut corrected, guide_rgba, corrections, cancelled)?;
    check_cancelled(cancelled)?;

    // The existing matte kernel is deterministic and validates its own settings.
    // It operates on the corrected copy, leaving both caller-owned inputs intact.
    let mut mask = matte::refine(&corrected, guide_rgba, settings.matte)?;
    check_cancelled(cancelled)?;
    for y in 0..mask.height() {
        check_cancelled(cancelled)?;
        for x in 0..mask.width() {
            // Fully transparent guide pixels cannot become foreground through a
            // guided edge response.
            if guide_rgba.get_pixel(x, y)[3] == 0 {
                mask.put_pixel(x, y, Luma([0]));
            }
        }
    }

    let mut image = guide_rgba.clone();
    decontaminate(&mut image, guide_rgba, &mask, settings, cancelled)?;
    check_cancelled(cancelled)?;
    for y in 0..image.height() {
        check_cancelled(cancelled)?;
        for x in 0..image.width() {
            image.get_pixel_mut(x, y)[3] = mask.get_pixel(x, y)[0];
        }
    }
    Ok(Refined { mask, image })
}

fn validate_inputs(
    base_gray: &GrayImage,
    guide_rgba: &RgbaImage,
    settings: &Settings,
    corrections: &[BrushCorrection],
) -> Result<()> {
    ensure!(
        base_gray.dimensions() == guide_rgba.dimensions(),
        "matte and guide dimensions must match"
    );
    ensure!(
        crate::model::valid_dimensions(base_gray.width(), base_gray.height()),
        "refinement dimensions exceed Omuse limits"
    );
    ensure!(
        u64::from(base_gray.width()) * u64::from(base_gray.height()) <= MAX_PIXELS,
        "refinement is limited to 16 million pixels"
    );
    validate_settings(settings, corrections)?;
    let estimated_brush_work = corrections.iter().fold(0u64, |total, correction| {
        let span = (2.0 * correction.radius + 1.0).ceil() as u64;
        total.saturating_add(span.saturating_mul(span))
    });
    ensure!(
        estimated_brush_work <= MAX_BRUSH_WORK,
        "brush corrections exceed the bounded work budget"
    );
    for correction in corrections {
        ensure!(
            correction.x < base_gray.width() && correction.y < base_gray.height(),
            "brush correction center must be inside the image"
        );
    }
    Ok(())
}

fn apply_corrections(
    mask: &mut GrayImage,
    guide: &RgbaImage,
    corrections: &[BrushCorrection],
    cancelled: &AtomicBool,
) -> Result<()> {
    for correction in corrections {
        check_cancelled(cancelled)?;
        let radius = correction.radius;
        let min_x = ((correction.x as f32 - radius).floor().max(0.0)) as u32;
        let max_x = ((correction.x as f32 + radius)
            .ceil()
            .min(mask.width().saturating_sub(1) as f32)) as u32;
        let min_y = ((correction.y as f32 - radius).floor().max(0.0)) as u32;
        let max_y = ((correction.y as f32 + radius)
            .ceil()
            .min(mask.height().saturating_sub(1) as f32)) as u32;
        for y in min_y..=max_y {
            check_cancelled(cancelled)?;
            for x in min_x..=max_x {
                if guide.get_pixel(x, y)[3] == 0 {
                    continue;
                }
                let dx = x as f32 - correction.x as f32;
                let dy = y as f32 - correction.y as f32;
                let distance = (dx * dx + dy * dy).sqrt() / radius;
                if distance >= 1.0 {
                    continue;
                }
                let coverage = brush_coverage(distance, correction.hardness);
                let target = if correction.foreground { 255 } else { 0 };
                let old = mask.get_pixel(x, y)[0];
                mask.put_pixel(x, y, Luma([lerp_byte(old, target, coverage)]));
            }
        }
    }
    Ok(())
}

fn brush_coverage(distance: f32, hardness: f32) -> f32 {
    if hardness >= 1.0 {
        return 1.0;
    }
    let start = hardness;
    let value = ((1.0 - distance) / (1.0 - start)).clamp(0.0, 1.0);
    smoothstep(value)
}

fn decontaminate(
    output: &mut RgbaImage,
    original: &RgbaImage,
    mask: &GrayImage,
    settings: &Settings,
    cancelled: &AtomicBool,
) -> Result<()> {
    if settings.defringe_radius == 0 || settings.defringe_strength == 0.0 {
        return Ok(());
    }
    let nearest = nearest_interior(mask, original, settings.defringe_radius, cancelled)?;
    let radius_sq = i32::from(settings.defringe_radius) * i32::from(settings.defringe_radius);
    for y in 0..output.height() {
        check_cancelled(cancelled)?;
        for x in 0..output.width() {
            let alpha = mask.get_pixel(x, y)[0];
            if alpha == 0 || alpha == 255 || original.get_pixel(x, y)[3] == 0 {
                continue;
            }
            let index = (y * output.width() + x) as usize;
            let Some(interior) = nearest[index] else {
                continue;
            };
            let ix = interior % output.width();
            let iy = interior / output.width();
            let dx = ix as i32 - x as i32;
            let dy = iy as i32 - y as i32;
            if dx * dx + dy * dy > radius_sq {
                continue;
            }
            let source = original.get_pixel(x, y).0;
            let interior = original.get_pixel(ix, iy).0;
            // More transparent edge pixels receive more correction. Opaque
            // interiors and mask-zero background pixels were excluded above.
            let amount = settings.defringe_strength * (1.0 - f32::from(alpha) / 255.0);
            let pixel = output.get_pixel_mut(x, y);
            for channel in 0..3 {
                pixel[channel] = lerp_byte(source[channel], interior[channel], amount);
            }
        }
    }
    Ok(())
}

/// A two-pass eight-neighbour distance propagation gives a deterministic
/// nearest interior colour in linear image time, avoiding radius-squared work
/// for large canvases.
fn nearest_interior(
    mask: &GrayImage,
    guide: &RgbaImage,
    radius: u8,
    cancelled: &AtomicBool,
) -> Result<Vec<Option<u32>>> {
    let width = mask.width();
    let height = mask.height();
    let count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| anyhow::anyhow!("refinement image is too large"))?;
    let mut nearest = vec![None; count];
    for y in 0..height {
        check_cancelled(cancelled)?;
        for x in 0..width {
            if mask.get_pixel(x, y)[0] == 255 && guide.get_pixel(x, y)[3] != 0 {
                nearest[(y * width + x) as usize] = Some(y * width + x);
            }
        }
    }
    for y in 0..height {
        check_cancelled(cancelled)?;
        for x in 0..width {
            let index = (y * width + x) as usize;
            for candidate in neighbours_forward(x, y, width, height, &nearest) {
                nearest[index] = closer(index as u32, nearest[index], candidate, width);
            }
        }
    }
    for y in (0..height).rev() {
        check_cancelled(cancelled)?;
        for x in (0..width).rev() {
            let index = (y * width + x) as usize;
            for candidate in neighbours_backward(x, y, width, height, &nearest) {
                nearest[index] = closer(index as u32, nearest[index], candidate, width);
            }
        }
    }
    let radius_sq = u32::from(radius) * u32::from(radius);
    for (index, value) in nearest.iter_mut().enumerate() {
        if let Some(candidate) = *value {
            let x = index as u32 % width;
            let y = index as u32 / width;
            let cx = candidate % width;
            let cy = candidate / width;
            let dx = x.abs_diff(cx);
            let dy = y.abs_diff(cy);
            if dx * dx + dy * dy > radius_sq {
                *value = None;
            }
        }
    }
    Ok(nearest)
}

fn neighbours_forward(
    x: u32,
    y: u32,
    width: u32,
    _height: u32,
    nearest: &[Option<u32>],
) -> [Option<u32>; 4] {
    [
        (x > 0)
            .then(|| nearest[(y * width + x - 1) as usize])
            .flatten(),
        (y > 0)
            .then(|| nearest[((y - 1) * width + x) as usize])
            .flatten(),
        (x > 0 && y > 0)
            .then(|| nearest[((y - 1) * width + x - 1) as usize])
            .flatten(),
        (x + 1 < width && y > 0)
            .then(|| nearest[((y - 1) * width + x + 1) as usize])
            .flatten(),
    ]
}

fn neighbours_backward(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    nearest: &[Option<u32>],
) -> [Option<u32>; 4] {
    [
        (x + 1 < width)
            .then(|| nearest[(y * width + x + 1) as usize])
            .flatten(),
        (y + 1 < height)
            .then(|| nearest[((y + 1) * width + x) as usize])
            .flatten(),
        (x + 1 < width && y + 1 < height)
            .then(|| nearest[((y + 1) * width + x + 1) as usize])
            .flatten(),
        (x > 0 && y + 1 < height)
            .then(|| nearest[((y + 1) * width + x - 1) as usize])
            .flatten(),
    ]
}

fn closer(current: u32, old: Option<u32>, candidate: Option<u32>, width: u32) -> Option<u32> {
    let Some(candidate) = candidate else {
        return old;
    };
    let better = |value: u32| {
        let x = current % width;
        let y = current / width;
        let cx = value % width;
        let cy = value / width;
        x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2)
    };
    match old {
        Some(old) if better(old) <= better(candidate) => Some(old),
        _ => Some(candidate),
    }
}

/// Render a refined foreground over a selected display background.
pub fn display_preview(
    refined: &Refined,
    original: &RgbaImage,
    background: PreviewBackground,
) -> Result<RgbaImage> {
    ensure!(
        refined.image.dimensions() == refined.mask.dimensions()
            && refined.image.dimensions() == original.dimensions(),
        "preview dimensions must match"
    );
    if background == PreviewBackground::Mask {
        return Ok(RgbaImage::from_fn(
            refined.mask.width(),
            refined.mask.height(),
            |x, y| {
                let value = refined.mask.get_pixel(x, y)[0];
                Rgba([value, value, value, 255])
            },
        ));
    }
    let mut output = RgbaImage::new(refined.mask.width(), refined.mask.height());
    let width = output.width();
    for (index, pixel) in output.pixels_mut().enumerate() {
        let x = index as u32 % width;
        let y = index as u32 / width;
        let background = match background {
            PreviewBackground::Original => original.get_pixel(x, y).0,
            PreviewBackground::Black => [0, 0, 0, 255],
            PreviewBackground::White => [255, 255, 255, 255],
            PreviewBackground::Checkerboard => {
                if ((x / 8) + (y / 8)) % 2 == 0 {
                    [190, 190, 190, 255]
                } else {
                    [235, 235, 235, 255]
                }
            }
            PreviewBackground::Mask => unreachable!(),
        };
        *pixel = Rgba(over(background, refined.image.get_pixel(x, y).0));
    }
    Ok(output)
}

fn over(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    let alpha = f32::from(foreground[3]) / 255.0;
    [
        lerp_byte(background[0], foreground[0], alpha),
        lerp_byte(background[1], foreground[1], alpha),
        lerp_byte(background[2], foreground[2], alpha),
        255,
    ]
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Relaxed), "refinement cancelled");
    Ok(())
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp_byte(a: u8, b: u8, amount: f32) -> u8 {
    (f32::from(a) + (f32::from(b) - f32::from(a)) * amount.clamp(0.0, 1.0)).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide() -> RgbaImage {
        RgbaImage::from_fn(9, 3, |x, _| {
            if x < 4 {
                Rgba([0, 0, 220, 255])
            } else {
                Rgba([220, 30, 20, 255])
            }
        })
    }

    #[test]
    fn foreground_and_background_corrections_are_exact_at_centres() {
        let base = GrayImage::from_pixel(9, 3, Luma([128]));
        let guide = guide();
        let corrections = [
            BrushCorrection {
                x: 1,
                y: 1,
                radius: 1.0,
                hardness: 1.0,
                foreground: true,
            },
            BrushCorrection {
                x: 7,
                y: 1,
                radius: 1.0,
                hardness: 1.0,
                foreground: false,
            },
        ];
        let refined = refine(
            &base,
            &guide,
            &Settings::default(),
            &corrections,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(refined.mask.get_pixel(1, 1)[0], 255);
        assert_eq!(refined.mask.get_pixel(7, 1)[0], 0);
    }

    #[test]
    fn defringe_changes_partial_edges_but_preserves_opaque_and_background() {
        let base = GrayImage::from_fn(9, 1, |x, _| {
            Luma([if x < 3 {
                0
            } else if x == 3 {
                96
            } else {
                255
            }])
        });
        let guide = RgbaImage::from_fn(9, 1, |x, _| {
            if x < 4 {
                Rgba([0, 0, 220, 255])
            } else {
                Rgba([220, 30, 20, 255])
            }
        });
        let settings = Settings {
            defringe_radius: 3,
            defringe_strength: 1.0,
            ..Default::default()
        };
        let refined = refine(&base, &guide, &settings, &[], &AtomicBool::new(false)).unwrap();
        assert_eq!(
            &refined.image.get_pixel(0, 0).0[..3],
            &guide.get_pixel(0, 0).0[..3]
        );
        assert_eq!(refined.image.get_pixel(0, 0)[3], 0);
        assert_eq!(
            &refined.image.get_pixel(8, 0).0[..3],
            &guide.get_pixel(8, 0).0[..3]
        );
        assert_ne!(refined.image.get_pixel(3, 0)[2], guide.get_pixel(3, 0)[2]);
        assert_eq!(refined.image.get_pixel(8, 0)[3], 255);
    }

    #[test]
    fn zero_alpha_is_excluded_and_input_is_unchanged() {
        let base = GrayImage::from_pixel(3, 1, Luma([255]));
        let guide = RgbaImage::from_fn(3, 1, |x, _| {
            Rgba([10 + x as u8, 20, 30, if x == 0 { 0 } else { 255 }])
        });
        let original = guide.clone();
        let refined = refine(
            &base,
            &guide,
            &Settings::default(),
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(guide, original);
        assert_eq!(refined.mask.get_pixel(0, 0)[0], 0);
        assert_eq!(refined.image.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn invalid_and_cancelled_requests_fail() {
        let base = GrayImage::new(2, 2);
        let guide = RgbaImage::new(2, 2);
        let invalid = Settings {
            defringe_strength: f32::NAN,
            ..Default::default()
        };
        assert!(refine(&base, &guide, &invalid, &[], &AtomicBool::new(false)).is_err());
        let cancelled = AtomicBool::new(true);
        assert!(refine(&base, &guide, &Settings::default(), &[], &cancelled).is_err());
    }

    #[test]
    fn previews_render_each_background_and_mask() {
        let refined = Refined {
            mask: GrayImage::from_pixel(2, 2, Luma([128])),
            image: RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 128])),
        };
        let original = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 255, 255]));
        for background in [
            PreviewBackground::Original,
            PreviewBackground::Black,
            PreviewBackground::White,
            PreviewBackground::Checkerboard,
            PreviewBackground::Mask,
        ] {
            assert!(display_preview(&refined, &original, background).is_ok());
        }
        assert_eq!(
            display_preview(&refined, &original, PreviewBackground::Mask)
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [128, 128, 128, 255]
        );
    }
}
