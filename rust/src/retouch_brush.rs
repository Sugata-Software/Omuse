//! Bounded, transactional Blur, Smudge and Liquify strokes.
//! Liquify composes a displacement field and samples untouched source pixels
//! once. Smudge carries the paint left by its preceding dab. Brush footprints
//! and path spacing stay fractional, independent of pointer event frequency.
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetouchMode {
    Blur,
    Smudge,
    Liquify,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
}

/// Input/output are straight-alpha RGBA. No partial result escapes on failure.
pub fn apply(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    mode: RetouchMode,
) -> Result<RgbaImage> {
    apply_cancellable(
        source,
        points,
        diameter,
        hardness,
        strength,
        mode,
        &AtomicBool::new(false),
    )
}

pub fn apply_cancellable(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    mode: RetouchMode,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    check_cancelled(cancelled)?;
    ensure!(
        crate::model::valid_dimensions(source.width(), source.height()),
        "invalid image dimensions"
    );
    ensure!(
        (2. ..=4096.).contains(&diameter) && diameter.is_finite(),
        "diameter must be 2-4096 pixels"
    );
    ensure!(
        hardness.is_finite() && (0. ..=0.98).contains(&hardness),
        "hardness must be 0-0.98"
    );
    ensure!(
        strength.is_finite() && (0.01..=1.).contains(&strength),
        "strength must be 0.01-1"
    );
    ensure!(
        points.len() <= 100_000
            && points.iter().all(|p| p.x.is_finite()
                && p.y.is_finite()
                && p.x.abs() <= 1_000_000.
                && p.y.abs() <= 1_000_000.),
        "invalid stroke points"
    );
    if points.is_empty() || (points.len() == 1 && mode != RetouchMode::Blur) {
        return Ok(source.clone());
    }
    ensure!(
        u64::from(source.width()) * u64::from(source.height()) <= 16_777_216,
        "Retouch supports images up to 16 million pixels"
    );
    let spacing = match mode {
        RetouchMode::Blur => (diameter * 0.025).max(0.25),
        RetouchMode::Smudge => (diameter * 0.08).max(1.),
        RetouchMode::Liquify => (diameter * 0.025).max(1.),
    };
    let distance: f64 = points
        .windows(2)
        .map(|pair| {
            (f64::from(pair[1].x) - f64::from(pair[0].x))
                .hypot(f64::from(pair[1].y) - f64::from(pair[0].y))
        })
        .sum();
    let dabs = (distance / f64::from(spacing)).ceil() as u64 + 2;
    let side = diameter.ceil() as u64 + 3;
    ensure!(
        dabs.saturating_mul(side.saturating_mul(side)) <= 200_000_000,
        "Stroke is too long for this brush size; use shorter strokes"
    );
    let Some(area) = Area::for_points(source, points, diameter / 2. + 1.) else {
        return Ok(source.clone());
    };
    let result = match mode {
        RetouchMode::Blur => blur_stroke(
            source, points, diameter, hardness, strength, spacing, area, cancelled,
        ),
        RetouchMode::Smudge => smudge_stroke(
            source, points, diameter, hardness, strength, spacing, cancelled,
        ),
        RetouchMode::Liquify => liquify_stroke(
            source, points, diameter, hardness, strength, spacing, area, cancelled,
        ),
    }?;
    check_cancelled(cancelled)?;
    Ok(result)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "retouch stroke cancelled"
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct Area {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}
impl Area {
    fn width(self) -> usize {
        (self.right - self.left) as usize
    }
    fn height(self) -> usize {
        (self.bottom - self.top) as usize
    }
    fn len(self) -> usize {
        self.width() * self.height()
    }
    fn index(self, x: u32, y: u32) -> usize {
        (y - self.top) as usize * self.width() + (x - self.left) as usize
    }
    fn contains(self, x: i64, y: i64) -> bool {
        x >= i64::from(self.left)
            && y >= i64::from(self.top)
            && x < i64::from(self.right)
            && y < i64::from(self.bottom)
    }
    fn for_points(image: &RgbaImage, points: &[StrokePoint], radius: f32) -> Option<Self> {
        let mut min = StrokePoint {
            x: f32::INFINITY,
            y: f32::INFINITY,
        };
        let mut max = StrokePoint {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        };
        for point in points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
        let area = Self {
            left: (min.x - radius).floor().max(0.).min(image.width() as f32) as u32,
            top: (min.y - radius).floor().max(0.).min(image.height() as f32) as u32,
            right: (max.x + radius).ceil().max(0.).min(image.width() as f32) as u32,
            bottom: (max.y + radius).ceil().max(0.).min(image.height() as f32) as u32,
        };
        (area.left < area.right && area.top < area.bottom).then_some(area)
    }
    fn expand(self, margin: u32, width: u32, height: u32) -> Self {
        Self {
            left: self.left.saturating_sub(margin),
            top: self.top.saturating_sub(margin),
            right: self.right.saturating_add(margin).min(width),
            bottom: self.bottom.saturating_add(margin).min(height),
        }
    }
    fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }
}

/// Keep a residual distance across event boundaries. Always emit the final
/// endpoint, including a motion shorter than one normal dab interval.
fn walk_path(
    points: &[StrokePoint],
    spacing: f32,
    mut dab: impl FnMut(StrokePoint, StrokePoint) -> Result<()>,
) -> Result<()> {
    let mut previous = points[0];
    let mut remainder = 0f64;
    for pair in points.windows(2) {
        let (dx, dy) = (
            f64::from(pair[1].x) - f64::from(pair[0].x),
            f64::from(pair[1].y) - f64::from(pair[0].y),
        );
        let distance = dx.hypot(dy);
        if distance == 0. {
            continue;
        }
        let mut travelled = f64::from(spacing) - remainder;
        while travelled <= distance + 1e-9 {
            let t = (travelled / distance).min(1.);
            let next = StrokePoint {
                x: (f64::from(pair[0].x) + dx * t) as f32,
                y: (f64::from(pair[0].y) + dy * t) as f32,
            };
            if next != previous {
                dab(previous, next)?;
                previous = next;
            }
            travelled += f64::from(spacing);
        }
        remainder = (distance - (travelled - f64::from(spacing))).max(0.);
    }
    let last = *points.last().unwrap();
    if last != previous {
        dab(previous, last)?;
    }
    Ok(())
}

fn weight(distance: f32, radius: f32, hardness: f32) -> f32 {
    let u = distance / radius;
    if u >= 1. {
        return 0.;
    }
    if u <= hardness {
        return 1.;
    }
    let t = (1. - u) / (1. - hardness);
    t * t * (3. - 2. * t)
}

fn premultiplied(pixel: Rgba<u8>) -> [f32; 4] {
    let alpha = f32::from(pixel[3]);
    [
        f32::from(pixel[0]) * alpha / 255.,
        f32::from(pixel[1]) * alpha / 255.,
        f32::from(pixel[2]) * alpha / 255.,
        alpha,
    ]
}
fn straight(pixel: [f32; 4]) -> Rgba<u8> {
    let alpha = pixel[3].clamp(0., 255.);
    let rounded = alpha.round() as u8;
    if rounded == 0 {
        return Rgba([0; 4]);
    }
    Rgba([
        (pixel[0] * 255. / alpha).round().clamp(0., 255.) as u8,
        (pixel[1] * 255. / alpha).round().clamp(0., 255.) as u8,
        (pixel[2] * 255. / alpha).round().clamp(0., 255.) as u8,
        rounded,
    ])
}
fn mix<const N: usize>(a: [f32; N], b: [f32; N], amount: f32) -> [f32; N] {
    std::array::from_fn(|c| a[c] + (b[c] - a[c]) * amount)
}
/// Coordinates are canvas pixel centres. Both one-pixel axes and image edges
/// are valid; interpolation operates in premultiplied space to avoid fringes.
fn sample_image(image: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let x = (x - 0.5).clamp(0., image.width() as f32 - 1.);
    let y = (y - 0.5).clamp(0., image.height() as f32 - 1.);
    let (left, top) = (x.floor() as u32, y.floor() as u32);
    let (right, bottom) = (
        (left + 1).min(image.width() - 1),
        (top + 1).min(image.height() - 1),
    );
    let a = mix(
        premultiplied(*image.get_pixel(left, top)),
        premultiplied(*image.get_pixel(right, top)),
        x - left as f32,
    );
    let b = mix(
        premultiplied(*image.get_pixel(left, bottom)),
        premultiplied(*image.get_pixel(right, bottom)),
        x - left as f32,
    );
    mix(a, b, y - top as f32)
}

fn blur_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    spacing: f32,
    area: Area,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let sigma = (diameter / 10.).clamp(1.5, 30.);
    // Each of the three box-pass radii is at most sigma + 1. This guard
    // contains the complete finite kernel, including all rounding passes.
    let padded = area.expand(
        (sigma * 3.).ceil() as u32 + 3,
        source.width(),
        source.height(),
    );
    let mut softened = image::imageops::crop_imm(
        source,
        padded.left,
        padded.top,
        padded.width() as u32,
        padded.height() as u32,
    )
    .to_image();
    crate::filters::apply_cancellable(
        &mut softened,
        &crate::filters::Filter::GaussianBlur { sigma },
        cancelled,
    )?;
    let mut coverage = vec![0f32; area.len()];
    let mut dab = |center: StrokePoint| -> Result<()> {
        check_cancelled(cancelled)?;
        let Some(bounds) = Area::for_points(source, &[center], diameter / 2.) else {
            return Ok(());
        };
        for y in bounds.top..bounds.bottom {
            check_cancelled(cancelled)?;
            for x in bounds.left..bounds.right {
                let w = weight(
                    (x as f32 + 0.5 - center.x).hypot(y as f32 + 0.5 - center.y),
                    diameter / 2.,
                    hardness,
                );
                let slot = &mut coverage[area.index(x, y)];
                *slot = 1. - (1. - *slot) * (1. - w);
            }
        }
        Ok(())
    };
    dab(points[0])?;
    walk_path(points, spacing, |_, to| dab(to))?;
    let mut output = source.clone();
    for y in area.top..area.bottom {
        check_cancelled(cancelled)?;
        for x in area.left..area.right {
            let amount = coverage[area.index(x, y)] * strength;
            if amount == 0. {
                continue;
            }
            let original = premultiplied(*source.get_pixel(x, y));
            let blurred = premultiplied(*softened.get_pixel(x - padded.left, y - padded.top));
            output.put_pixel(x, y, straight(mix(original, blurred, amount)));
        }
    }
    Ok(output)
}

fn pick_up(
    image: &RgbaImage,
    center: StrokePoint,
    radius: usize,
    carried: &mut [[f32; 4]],
    cancelled: &AtomicBool,
) -> Result<()> {
    let side = radius * 2 + 1;
    for y in 0..side {
        check_cancelled(cancelled)?;
        for x in 0..side {
            carried[y * side + x] = sample_image(
                image,
                center.x + x as f32 - radius as f32,
                center.y + y as f32 - radius as f32,
            );
        }
    }
    Ok(())
}
fn sample_carried(carried: &[[f32; 4]], side: usize, x: f32, y: f32) -> [f32; 4] {
    let (x, y) = (
        x.clamp(0., (side - 1) as f32),
        y.clamp(0., (side - 1) as f32),
    );
    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (right, bottom) = ((left + 1).min(side - 1), (top + 1).min(side - 1));
    mix(
        mix(
            carried[top * side + left],
            carried[top * side + right],
            x - left as f32,
        ),
        mix(
            carried[bottom * side + left],
            carried[bottom * side + right],
            x - left as f32,
        ),
        y - top as f32,
    )
}
fn smudge_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    spacing: f32,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let radius = (diameter / 2.).ceil() as usize;
    let side = radius * 2 + 1;
    let mut carried = vec![[0.; 4]; side * side];
    pick_up(source, points[0], radius, &mut carried, cancelled)?;
    let mut output = source.clone();
    walk_path(points, spacing, |from, to| {
        check_cancelled(cancelled)?;
        let amount = strength * ((to.x - from.x).hypot(to.y - from.y) / spacing).min(1.);
        if let Some(bounds) = Area::for_points(source, &[to], diameter / 2.) {
            for y in bounds.top..bounds.bottom {
                check_cancelled(cancelled)?;
                for x in bounds.left..bounds.right {
                    let (dx, dy) = (x as f32 + 0.5 - to.x, y as f32 + 0.5 - to.y);
                    let w = weight(dx.hypot(dy), diameter / 2., hardness) * amount;
                    if w == 0. {
                        continue;
                    }
                    let ink =
                        sample_carried(&carried, side, dx + radius as f32, dy + radius as f32);
                    let under = premultiplied(*output.get_pixel(x, y));
                    output.put_pixel(x, y, straight(mix(under, ink, w)));
                }
            }
        }
        // Pick up the deposited paint, rather than retaining a second copy of
        // the initial stamp. This makes the trail fade instead of echoing.
        pick_up(&output, to, radius, &mut carried, cancelled)
    })?;
    Ok(output)
}

fn sample_offset(offsets: &[[f32; 2]], area: Area, x: f32, y: f32) -> [f32; 2] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (left, top) = (x.floor() as i64, y.floor() as i64);
    let at = |x: i64, y: i64| {
        if area.contains(x, y) {
            offsets[area.index(x as u32, y as u32)]
        } else {
            [0.; 2]
        }
    };
    mix(
        mix(at(left, top), at(left + 1, top), x - left as f32),
        mix(at(left, top + 1), at(left + 1, top + 1), x - left as f32),
        y - top as f32,
    )
}
fn liquify_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    spacing: f32,
    area: Area,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let mut offsets = vec![[0f32; 2]; area.len()];
    let mut scratch = Vec::new();
    walk_path(points, spacing, |from, to| {
        check_cancelled(cancelled)?;
        let Some(bounds) = Area::for_points(source, &[to], diameter / 2.) else {
            return Ok(());
        };
        let movement = [(to.x - from.x) * strength, (to.y - from.y) * strength];
        let margin = movement[0].abs().max(movement[1].abs()).ceil() as u32 + 1;
        let sampled = bounds
            .expand(margin, source.width(), source.height())
            .intersect(area);
        scratch.clear();
        for y in sampled.top..sampled.bottom {
            let start = area.index(sampled.left, y);
            scratch.extend_from_slice(&offsets[start..start + sampled.width()]);
        }
        for y in bounds.top..bounds.bottom {
            check_cancelled(cancelled)?;
            for x in bounds.left..bounds.right {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w = weight((px - to.x).hypot(py - to.y), diameter / 2., hardness);
                if w == 0. {
                    continue;
                }
                let step = [movement[0] * w, movement[1] * w];
                let old = sample_offset(
                    &scratch,
                    sampled,
                    (px - step[0]).clamp(0.5, source.width() as f32 - 0.5),
                    (py - step[1]).clamp(0.5, source.height() as f32 - 0.5),
                );
                offsets[area.index(x, y)] = [old[0] - step[0], old[1] - step[1]];
            }
        }
        Ok(())
    })?;
    let mut output = source.clone();
    for y in area.top..area.bottom {
        check_cancelled(cancelled)?;
        for x in area.left..area.right {
            let offset = offsets[area.index(x, y)];
            if offset != [0.; 2] {
                output.put_pixel(
                    x,
                    y,
                    straight(sample_image(
                        source,
                        x as f32 + 0.5 + offset[0],
                        y as f32 + 0.5 + offset[1],
                    )),
                );
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn points() -> Vec<StrokePoint> {
        vec![StrokePoint { x: 2., y: 4. }, StrokePoint { x: 7., y: 4. }]
    }
    #[test]
    fn empty_and_anchor_only_strokes_are_unchanged() {
        let image = RgbaImage::from_pixel(8, 8, Rgba([20, 40, 60, 128]));
        assert_eq!(
            apply(&image, &[], 8., 0.5, 0.5, RetouchMode::Smudge).unwrap(),
            image
        );
        assert_eq!(
            apply(&image, &points()[..1], 8., 0.5, 0.5, RetouchMode::Liquify).unwrap(),
            image
        );
    }
    #[test]
    fn smudge_carries_color_and_keeps_premultiplied_alpha_sound() {
        let mut image = RgbaImage::new(10, 9);
        for y in 0..9 {
            for x in 0..4 {
                image.put_pixel(x, y, Rgba([255, 0, 0, 128]));
            }
        }
        let out = apply(&image, &points(), 4., 0.8, 0.8, RetouchMode::Smudge).unwrap();
        assert!(out.get_pixel(6, 4)[0] > 0);
        assert!(out.pixels().all(|p| p[3] <= 128));
    }
    #[test]
    fn liquify_pushes_pixels_in_drag_direction() {
        let mut image = RgbaImage::new(12, 9);
        image.put_pixel(3, 4, Rgba([255; 4]));
        let out = apply(
            &image,
            &[StrokePoint { x: 3., y: 4. }, StrokePoint { x: 7., y: 4. }],
            8.,
            0.5,
            1.,
            RetouchMode::Liquify,
        )
        .unwrap();
        assert!(out.get_pixel(7, 4)[3] > 0);
        assert_ne!(out, image);
    }
    #[test]
    fn blur_uses_frozen_source_and_preserves_transparent_color_rules() {
        let mut image = RgbaImage::new(9, 9);
        image.put_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let out = apply(
            &image,
            &[StrokePoint { x: 4., y: 4. }],
            6.,
            0.5,
            1.,
            RetouchMode::Blur,
        )
        .unwrap();
        assert!(out.get_pixel(4, 4)[0] > 0);
        assert_eq!(out.get_pixel(0, 0), &Rgba([0; 4]));
    }
    #[test]
    fn invalid_settings_fail_before_mutation() {
        let image = RgbaImage::new(2, 2);
        assert!(apply(&image, &points(), 1., 0.5, 0.5, RetouchMode::Blur).is_err());
        assert!(apply(&image, &points(), 4., 1., 0.5, RetouchMode::Blur).is_err());
    }

    #[test]
    fn pointer_event_segmentation_does_not_change_a_straight_stroke() {
        let source = RgbaImage::from_fn(48, 32, |x, y| {
            Rgba([
                (x * 5) as u8,
                (y * 7) as u8,
                if x % 2 == 0 { 220 } else { 30 },
                if y < 16 { 123 } else { 255 },
            ])
        });
        let sparse = [
            StrokePoint { x: 10.5, y: 17.5 },
            StrokePoint { x: 29.75, y: 17.5 },
        ];
        let dense: Vec<_> = (0..=77)
            .map(|step| StrokePoint {
                x: 10.5 + step as f32 / 4.,
                y: 17.5,
            })
            .collect();
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            assert_eq!(
                apply(&source, &sparse, 10., 0.5, 0.7, mode).unwrap(),
                apply(&source, &dense, 10., 0.5, 0.7, mode).unwrap(),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn dense_pointer_events_use_path_work_budget_and_off_canvas_strokes_are_no_ops() {
        let source = RgbaImage::from_fn(96, 32, |x, y| Rgba([x as u8 * 2, y as u8, 77, 129]));
        let dense: Vec<_> = (0..=20_000)
            .map(|step| StrokePoint {
                x: 30.5 + step as f32 / 20_000.,
                y: 16.5,
            })
            .collect();
        for mode in [RetouchMode::Smudge, RetouchMode::Liquify] {
            assert!(apply(&source, &dense, 128., 0.5, 0.7, mode).is_ok());
            assert_eq!(
                apply(
                    &source,
                    &[
                        StrokePoint { x: -20., y: -20. },
                        StrokePoint { x: -10., y: -10. }
                    ],
                    4.,
                    0.5,
                    0.7,
                    mode
                )
                .unwrap(),
                source
            );
        }
    }

    #[test]
    fn short_final_motion_is_kept_and_subpixel_centres_are_distinct() {
        let source = RgbaImage::from_fn(24, 12, |x, _| {
            Rgba([if x % 2 == 0 { 0 } else { 240 }, 0, 0, 255])
        });
        for mode in [RetouchMode::Smudge, RetouchMode::Liquify] {
            let half = apply(
                &source,
                &[
                    StrokePoint { x: 8.5, y: 5.5 },
                    StrokePoint { x: 9., y: 5.5 },
                ],
                8.,
                0.98,
                1.,
                mode,
            )
            .unwrap();
            let quarter = apply(
                &source,
                &[
                    StrokePoint { x: 8.5, y: 5.5 },
                    StrokePoint { x: 8.75, y: 5.5 },
                ],
                8.,
                0.98,
                1.,
                mode,
            )
            .unwrap();
            assert_ne!(half, source, "{mode:?} dropped a half-pixel stroke");
            assert_ne!(half, quarter, "{mode:?} rounded away a subpixel endpoint");
        }
    }

    #[test]
    fn liquify_samples_stripes_from_the_original_only_once() {
        let source = RgbaImage::from_fn(80, 24, |x, _| {
            Rgba([if x % 2 == 0 { 0 } else { 220 }, 0, 0, 255])
        });
        let output = apply(
            &source,
            &[
                StrokePoint { x: 30.5, y: 12.5 },
                StrokePoint { x: 31.5, y: 12.5 },
                StrokePoint { x: 32.5, y: 12.5 },
            ],
            12.,
            0.98,
            0.5,
            RetouchMode::Liquify,
        )
        .unwrap();
        // Two half-pixel pushes are one source pixel, not two image blurs.
        for x in 29..35 {
            assert_eq!(
                output.get_pixel(x, 12),
                source.get_pixel(x - 1, 12),
                "stripe at {x}"
            );
        }
        assert_eq!(output.get_pixel(60, 12), source.get_pixel(60, 12));
    }

    #[test]
    fn smudge_fades_one_trail_instead_of_repeating_the_original_stamp() {
        let source = RgbaImage::from_fn(220, 64, |x, y| {
            let inside = (x as f32 + 0.5 - 40.5).hypot(y as f32 + 0.5 - 32.5) <= 6.;
            Rgba([if inside { 240 } else { 20 }; 4])
        });
        let output = apply(
            &source,
            &[
                StrokePoint { x: 40.5, y: 32.5 },
                StrokePoint { x: 180.5, y: 32.5 },
            ],
            32.,
            0.5,
            0.65,
            RetouchMode::Smudge,
        )
        .unwrap();
        let row: Vec<_> = (52..180).map(|x| output.get_pixel(x, 32)[3]).collect();
        let peaks = (2..row.len() - 2)
            .filter(|i| {
                row[*i] > row[*i - 2].saturating_add(2) && row[*i] > row[*i + 2].saturating_add(2)
            })
            .count();
        assert_eq!(peaks, 0, "echoes in the smudge trail");
        assert!(
            row[0] > row[row.len() - 1] + 20,
            "trail did not fade: {row:?}"
        );
    }

    #[test]
    fn untouched_transparent_and_translucent_pixels_remain_byte_exact() {
        let source = RgbaImage::from_fn(64, 48, |x, y| {
            Rgba([
                (x * 3) as u8,
                (y * 4) as u8,
                117,
                if x % 3 == 0 { 0 } else { 17 },
            ])
        });
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let result = apply(
                &source,
                &[
                    StrokePoint { x: 20.5, y: 20.5 },
                    StrokePoint { x: 25., y: 20.5 },
                ],
                8.,
                0.5,
                0.7,
                mode,
            )
            .unwrap();
            for (x, y, pixel) in source.enumerate_pixels() {
                if !(15..=30).contains(&x) || !(15..=26).contains(&y) {
                    assert_eq!(result.get_pixel(x, y), pixel, "{mode:?} altered {x},{y}");
                }
            }
        }
    }

    #[test]
    fn one_pixel_axes_and_cancelled_jobs_are_safe() {
        for (width, height) in [(1, 8), (8, 1), (1, 1)] {
            let source = RgbaImage::from_fn(width, height, |x, y| {
                Rgba([(x * 30) as u8, (y * 30) as u8, 90, 127])
            });
            for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
                let points = [
                    StrokePoint { x: 0.5, y: 0.5 },
                    StrokePoint {
                        x: width as f32 - 0.5,
                        y: height as f32 - 0.5,
                    },
                ];
                let output = apply(&source, &points, 4., 0.5, 0.8, mode).unwrap();
                assert!(output.pixels().all(|pixel| pixel[3] == 127));
                let cancelled = AtomicBool::new(true);
                assert!(
                    apply_cancellable(&source, &points, 4., 0.5, 0.8, mode, &cancelled)
                        .unwrap_err()
                        .to_string()
                        .contains("cancelled")
                );
            }
        }
    }

    #[test]
    fn blur_patch_matches_a_full_canvas_kernel_including_edges_and_large_radius() {
        let source = RgbaImage::from_fn(180, 145, |x, y| {
            Rgba([
                (x * 71 + y * 23) as u8,
                (x * 11 + y * 53) as u8,
                (x * 29) as u8,
                if (x + y) % 5 == 0 { 0 } else { 173 },
            ])
        });
        for diameter in [8., 37., 300.] {
            for point in [
                StrokePoint { x: 1.25, y: 2.75 },
                StrokePoint { x: 93.75, y: 71.25 },
            ] {
                let output =
                    apply(&source, &[point], diameter, 0.5, 0.8, RetouchMode::Blur).unwrap();
                let mut full = source.clone();
                crate::filters::apply(
                    &mut full,
                    &crate::filters::Filter::GaussianBlur {
                        sigma: (diameter / 10.).clamp(1.5, 30.),
                    },
                )
                .unwrap();
                for (x, y, actual) in output.enumerate_pixels() {
                    let coverage = weight(
                        (x as f32 + 0.5 - point.x).hypot(y as f32 + 0.5 - point.y),
                        diameter / 2.,
                        0.5,
                    ) * 0.8;
                    let expected = if coverage == 0. {
                        *source.get_pixel(x, y)
                    } else {
                        straight(mix(
                            premultiplied(*source.get_pixel(x, y)),
                            premultiplied(*full.get_pixel(x, y)),
                            coverage,
                        ))
                    };
                    assert!(
                        actual
                            .0
                            .into_iter()
                            .zip(expected.0)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "diameter {diameter} at {x},{y}: {actual:?} vs {expected:?}"
                    );
                }
            }
        }
    }
}
