//! Canvas-space Blur, Smudge, and Liquify stroke kernels ported from the
//! preserved editor. Smudge and Liquify mirror `WarpStroke`; Blur uses the same
//! frozen-source stroke model with the crate's bounded Gaussian approximation.
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};

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

/// Process one complete stroke. Input and output use straight-alpha RGBA.
/// The source is cloned before editing; no partial result escapes on failure.
pub fn apply(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    mode: RetouchMode,
) -> Result<RgbaImage> {
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
    if points.is_empty() {
        return Ok(source.clone());
    }
    let spacing = match mode {
        RetouchMode::Blur => (diameter * 0.025).max(0.25),
        RetouchMode::Smudge => (diameter * 0.08).max(1.),
        RetouchMode::Liquify => (diameter * 0.025).max(1.),
    };
    let dabs = points
        .windows(2)
        .try_fold(1u64, |count, pair| {
            count.checked_add(
                ((pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y) / spacing).ceil() as u64,
            )
        })
        .ok_or_else(|| anyhow::anyhow!("Stroke work exceeds limits"))?;
    let side = (diameter.ceil() as u64) + 3;
    ensure!(
        dabs.saturating_mul(side.saturating_mul(side)) <= 200_000_000,
        "Stroke is too long for this brush size; use shorter strokes"
    );
    ensure!(
        u64::from(source.width()) * u64::from(source.height()) <= 16_777_216,
        "Retouch supports images up to 16 million pixels"
    );
    match mode {
        RetouchMode::Blur => blur_stroke(source, points, diameter, hardness, strength),
        RetouchMode::Smudge => Ok(unpremultiply(&warp_stroke(
            source, points, diameter, hardness, strength, true,
        ))),
        RetouchMode::Liquify => Ok(unpremultiply(&warp_stroke(
            source, points, diameter, hardness, strength, false,
        ))),
    }
}

fn blur_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
) -> Result<RgbaImage> {
    let mut softened = source.clone();
    crate::filters::apply(
        &mut softened,
        &crate::filters::Filter::GaussianBlur {
            sigma: (diameter / 10.).clamp(1.5, 30.),
        },
    )?;
    let original = premultiply(source);
    let softened = premultiply(&softened);
    let mut coverage = vec![0f32; source.width() as usize * source.height() as usize];
    let spacing = (diameter * 0.025).max(0.25);
    dab_coverage(
        &mut coverage,
        source.width(),
        source.height(),
        points[0],
        diameter,
        hardness,
    );
    for pair in points.windows(2) {
        walk(pair[0], pair[1], spacing, |point, _| {
            dab_coverage(
                &mut coverage,
                source.width(),
                source.height(),
                point,
                diameter,
                hardness,
            )
        });
    }
    let mut output = original.clone();
    let sample = softened.as_raw();
    for (index, pixel) in output.pixels_mut().enumerate() {
        let amount = coverage[index] * strength;
        for channel in 0..4 {
            pixel[channel] = (f32::from(pixel[channel])
                + (f32::from(sample[index * 4 + channel]) - f32::from(pixel[channel])) * amount)
                .round()
                .clamp(0., 255.) as u8;
        }
    }
    Ok(unpremultiply(&output))
}

fn warp_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    smudge_mode: bool,
) -> RgbaImage {
    let mut pixels = premultiply(source);
    if points.len() < 2 {
        return pixels;
    }
    let radius = (diameter / 2.).ceil() as i32;
    let mut carried = if smudge_mode {
        pick_up(&pixels, points[0], radius)
    } else {
        Vec::new()
    };
    let spacing = (diameter * if smudge_mode { 0.08 } else { 0.025 }).max(1.);
    let mut last = points[0];
    for &endpoint in &points[1..] {
        if (endpoint.x - last.x).hypot(endpoint.y - last.y) < spacing {
            continue;
        }
        let mut previous = last;
        walk(last, endpoint, spacing, |point, _| {
            if smudge_mode {
                smudge(
                    &mut pixels,
                    point,
                    diameter,
                    hardness,
                    strength,
                    &mut carried,
                );
            } else {
                push(&mut pixels, previous, point, diameter, hardness, strength);
            }
            previous = point;
        });
        last = endpoint;
    }
    pixels
}

fn walk(from: StrokePoint, to: StrokePoint, spacing: f32, mut dab: impl FnMut(StrokePoint, usize)) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let distance = dx.hypot(dy);
    if distance < spacing {
        return;
    }
    let steps = (distance / spacing).ceil() as usize;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let point = StrokePoint {
            x: from.x + dx * t,
            y: from.y + dy * t,
        };
        dab(point, step);
    }
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
fn dab_coverage(
    coverage: &mut [f32],
    width: u32,
    height: u32,
    center: StrokePoint,
    diameter: f32,
    hardness: f32,
) {
    let radius = (diameter / 2.).ceil() as i32;
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    for dy in -radius..=radius {
        let y = cy + dy;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for dx in -radius..=radius {
            let x = cx + dx;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let w = weight(((dx * dx + dy * dy) as f32).sqrt(), diameter / 2., hardness);
            let value = &mut coverage[y as usize * width as usize + x as usize];
            *value = 1. - (1. - *value) * (1. - w);
        }
    }
}

fn pick_up(image: &RgbaImage, center: StrokePoint, radius: i32) -> Vec<f32> {
    let side = 2 * radius + 1;
    let mut carried = vec![0.; side as usize * side as usize * 4];
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    for dy in -radius..=radius {
        let y = cy + dy;
        if y < 0 || y >= image.height() as i32 {
            continue;
        }
        for dx in -radius..=radius {
            let x = cx + dx;
            if x < 0 || x >= image.width() as i32 {
                continue;
            }
            let c = ((dy + radius) * side + dx + radius) as usize * 4;
            let p = image.get_pixel(x as u32, y as u32);
            for k in 0..4 {
                carried[c + k] = f32::from(p[k]);
            }
        }
    }
    carried
}
fn smudge(
    image: &mut RgbaImage,
    center: StrokePoint,
    diameter: f32,
    hardness: f32,
    strength: f32,
    carried: &mut [f32],
) {
    let radius = (diameter / 2.).ceil() as i32;
    let side = 2 * radius + 1;
    let cx = center.x.round() as i32;
    let cy = center.y.round() as i32;
    for dy in -radius..=radius {
        let y = cy + dy;
        if y < 0 || y >= image.height() as i32 {
            continue;
        }
        for dx in -radius..=radius {
            let x = cx + dx;
            if x < 0 || x >= image.width() as i32 {
                continue;
            }
            let w = weight(((dx * dx + dy * dy) as f32).sqrt(), diameter / 2., hardness);
            if w <= 0. {
                continue;
            }
            let c = ((dy + radius) * side + dx + radius) as usize * 4;
            let pixel = image.get_pixel_mut(x as u32, y as u32);
            for k in 0..4 {
                let under = f32::from(pixel[k]);
                let painted = under + (carried[c + k] - under) * w;
                pixel[k] = painted.round().clamp(0., 255.) as u8;
                carried[c + k] = painted + (carried[c + k] - painted) * strength;
            }
        }
    }
}

fn push(
    image: &mut RgbaImage,
    from: StrokePoint,
    to: StrokePoint,
    diameter: f32,
    hardness: f32,
    strength: f32,
) {
    let radius = (diameter / 2.).ceil() as i32;
    let movement = ((to.x - from.x) * strength, (to.y - from.y) * strength);
    let margin = movement.0.abs().max(movement.1.abs()).ceil() as i32 + 2;
    let cx = to.x.round() as i32;
    let cy = to.y.round() as i32;
    let x0 = (cx - radius - margin).max(0);
    let x1 = (cx + radius + margin).min(image.width() as i32 - 1);
    let y0 = (cy - radius - margin).max(0);
    let y1 = (cy + radius + margin).min(image.height() as i32 - 1);
    if x0 > x1 || y0 > y1 {
        return;
    }
    let cw = x1 - x0 + 1;
    let ch = y1 - y0 + 1;
    let mut scratch = vec![0f32; cw as usize * ch as usize * 4];
    for y in 0..ch {
        for x in 0..cw {
            let p = image.get_pixel((x + x0) as u32, (y + y0) as u32);
            let s = (y as usize * cw as usize + x as usize) * 4;
            for k in 0..4 {
                scratch[s + k] = f32::from(p[k]);
            }
        }
    }
    for dy in -radius..=radius {
        let y = cy + dy;
        if y < y0 || y > y1 {
            continue;
        }
        for dx in -radius..=radius {
            let x = cx + dx;
            if x < x0 || x > x1 {
                continue;
            }
            let w = weight(((dx * dx + dy * dy) as f32).sqrt(), diameter / 2., hardness);
            if w <= 0. {
                continue;
            }
            let sx = ((x - x0) as f32 - movement.0 * w).clamp(0., (cw - 1) as f32);
            let sy = ((y - y0) as f32 - movement.1 * w).clamp(0., (ch - 1) as f32);
            let ix = (sx as i32).min(cw - 2);
            let iy = (sy as i32).min(ch - 2);
            if ix < 0 || iy < 0 {
                continue;
            }
            let fx = sx - ix as f32;
            let fy = sy - iy as f32;
            let s00 = (iy as usize * cw as usize + ix as usize) * 4;
            let s10 = s00 + 4;
            let s01 = s00 + cw as usize * 4;
            let s11 = s01 + 4;
            let pixel = image.get_pixel_mut(x as u32, y as u32);
            for k in 0..4 {
                let top = scratch[s00 + k] + (scratch[s10 + k] - scratch[s00 + k]) * fx;
                let bottom = scratch[s01 + k] + (scratch[s11 + k] - scratch[s01 + k]) * fx;
                pixel[k] = (top + (bottom - top) * fy).round().clamp(0., 255.) as u8;
            }
        }
    }
}

fn premultiply(image: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        let a = u16::from(p[3]);
        Rgba([
            ((u16::from(p[0]) * a + 127) / 255) as u8,
            ((u16::from(p[1]) * a + 127) / 255) as u8,
            ((u16::from(p[2]) * a + 127) / 255) as u8,
            p[3],
        ])
    })
}
fn unpremultiply(image: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        let a = u32::from(p[3]);
        if a == 0 {
            return Rgba([0; 4]);
        }
        Rgba([
            ((u32::from(p[0]) * 255 + a / 2) / a).min(255) as u8,
            ((u32::from(p[1]) * 255 + a / 2) / a).min(255) as u8,
            ((u32::from(p[2]) * 255 + a / 2) / a).min(255) as u8,
            p[3],
        ])
    })
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
}
