//! Native, deterministic RGBA image adjustments. Parameters use normalized
//! sRGB values unless named otherwise. Existing adjustment variants preserve
//! alpha exactly. BloomGlow and VignetteOverlay explicitly composite new alpha.
//! Gaussian blur uses three box passes (a linear-time Gaussian approximation).
use anyhow::{Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// Filters are destructive pixel edits; callers provide undo/history snapshots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    Exposure {
        stops: f32,
    },
    Levels {
        black: f32,
        white: f32,
        gamma: f32,
    },
    /// Piecewise-linear curve, strictly increasing x, including x=0 and x=1.
    Curves {
        points: Vec<(f32, f32)>,
    },
    Hsl {
        hue_degrees: f32,
        saturation: f32,
        lightness: f32,
    },
    ColorBalance {
        red: f32,
        green: f32,
        blue: f32,
    },
    GaussianBlur {
        sigma: f32,
    },
    UnsharpMask {
        sigma: f32,
        amount: f32,
        threshold: f32,
    },
    Noise {
        amount: f32,
        seed: u64,
        monochrome: bool,
    },
    Vignette {
        amount: f32,
        midpoint: f32,
        feather: f32,
    },
    Bloom {
        sigma: f32,
        amount: f32,
        threshold: f32,
    },
    TonalContrast {
        shadows: f32,
        midtones: f32,
        highlights: f32,
    },
    /// Retro print / pixel finishes. Source alpha is preserved exactly.
    Dither(crate::dither::Settings),
    /// Screened bloom that can appear in transparent margins of the layer.
    /// The existing Bloom variant continues to preserve source alpha.
    BloomGlow {
        sigma: f32,
        amount: f32,
        threshold: f32,
    },
    /// Composite a coloured vignette, including on a blank transparent layer.
    VignetteOverlay {
        opacity: f32,
        midpoint: f32,
        feather: f32,
        color: [u8; 3],
    },
    /// Radius-based local contrast, weighted by the source luminance zones.
    LocalContrast {
        sigma: f32,
        shadows: f32,
        midtones: f32,
        highlights: f32,
    },
    Invert,
    Grayscale,
}
const MAX_BLUR_PIXELS: u64 = 16_777_216;

fn range(v: f32, low: f32, high: f32, name: &str) -> Result<()> {
    ensure!(
        v.is_finite() && (low..=high).contains(&v),
        "{name} must be finite and between {low} and {high}"
    );
    Ok(())
}
/// Validate completely before editing, so invalid settings never partially apply.
pub fn validate(filter: &Filter) -> Result<()> {
    match filter {
        Filter::Exposure { stops } => range(*stops, -20.0, 20.0, "Exposure")?,
        Filter::Levels {
            black,
            white,
            gamma,
        } => {
            range(*black, 0.0, 1.0, "Black point")?;
            range(*white, 0.0, 1.0, "White point")?;
            ensure!(white > black, "White point must exceed black point");
            range(*gamma, 0.05, 20.0, "Gamma")?;
        }
        Filter::Curves { points } => {
            ensure!(
                (2..=256).contains(&points.len()),
                "Curves need 2 to 256 points"
            );
            ensure!(
                points[0].0 == 0.0 && points[points.len() - 1].0 == 1.0,
                "Curve must include x=0 and x=1 endpoints"
            );
            let mut previous = -1.0;
            for &(x, y) in points {
                range(x, 0.0, 1.0, "Curve input")?;
                range(y, 0.0, 1.0, "Curve output")?;
                ensure!(
                    x > previous,
                    "Curve input coordinates must strictly increase"
                );
                previous = x;
            }
        }
        Filter::Hsl {
            hue_degrees,
            saturation,
            lightness,
        } => {
            range(*hue_degrees, -360.0, 360.0, "Hue")?;
            range(*saturation, -1.0, 1.0, "Saturation")?;
            range(*lightness, -1.0, 1.0, "Lightness")?;
        }
        Filter::ColorBalance { red, green, blue } => {
            range(*red, -1.0, 1.0, "Red")?;
            range(*green, -1.0, 1.0, "Green")?;
            range(*blue, -1.0, 1.0, "Blue")?;
        }
        Filter::GaussianBlur { sigma } => range(*sigma, 0.0, 128.0, "Blur sigma")?,
        Filter::UnsharpMask {
            sigma,
            amount,
            threshold,
        }
        | Filter::Bloom {
            sigma,
            amount,
            threshold,
        }
        | Filter::BloomGlow {
            sigma,
            amount,
            threshold,
        } => {
            range(*sigma, 0.0, 128.0, "Blur sigma")?;
            range(*amount, 0.0, 10.0, "Amount")?;
            range(*threshold, 0.0, 1.0, "Threshold")?;
        }
        Filter::Noise { amount, .. } => range(*amount, 0.0, 1.0, "Noise amount")?,
        Filter::Vignette {
            amount,
            midpoint,
            feather,
        } => {
            range(*amount, -1.0, 1.0, "Vignette amount")?;
            range(*midpoint, 0.0, 1.0, "Midpoint")?;
            range(*feather, 0.001, 1.0, "Feather")?;
        }
        Filter::TonalContrast {
            shadows,
            midtones,
            highlights,
        } => {
            range(*shadows, -1.0, 1.0, "Shadows")?;
            range(*midtones, -1.0, 1.0, "Midtones")?;
            range(*highlights, -1.0, 1.0, "Highlights")?;
        }
        Filter::Dither(settings) => settings.validate()?,
        Filter::VignetteOverlay {
            opacity,
            midpoint,
            feather,
            ..
        } => {
            range(*opacity, 0.0, 1.0, "Vignette opacity")?;
            range(*midpoint, 0.0, 1.0, "Midpoint")?;
            range(*feather, 0.001, 1.0, "Feather")?;
        }
        Filter::LocalContrast {
            sigma,
            shadows,
            midtones,
            highlights,
        } => {
            range(*sigma, 0.0, 128.0, "Local contrast radius")?;
            range(*shadows, -1.0, 1.0, "Shadows")?;
            range(*midtones, -1.0, 1.0, "Midtones")?;
            range(*highlights, -1.0, 1.0, "Highlights")?;
        }
        Filter::Invert | Filter::Grayscale => {}
    }
    Ok(())
}

pub fn apply(image: &mut RgbaImage, filter: &Filter) -> Result<()> {
    let cancelled = AtomicBool::new(false);
    apply_cancellable(image, filter, &cancelled)
}

/// Apply a filter while polling a caller-owned cancellation flag.
///
/// The edit is performed on a private working image and committed only after
/// the complete operation succeeds.  This makes cancellation (and any error
/// raised after work has started) transactional for callers holding the
/// original image.
pub fn apply_cancellable(
    image: &mut RgbaImage,
    filter: &Filter,
    cancelled: &AtomicBool,
) -> Result<()> {
    validate(filter)?;
    ensure!(
        crate::model::valid_dimensions(image.width(), image.height()),
        "Image dimensions exceed supported bounds"
    );
    if matches!(
        filter,
        Filter::GaussianBlur { .. }
            | Filter::UnsharpMask { .. }
            | Filter::Bloom { .. }
            | Filter::BloomGlow { .. }
            | Filter::LocalContrast { .. }
    ) {
        ensure!(
            u64::from(image.width()) * u64::from(image.height()) <= MAX_BLUR_PIXELS,
            "Blur-based filters are limited to 16 megapixels to bound memory use"
        );
    }
    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
    if let Filter::Dither(settings) = filter {
        return crate::dither::apply(image, settings, cancelled);
    }
    let mut working = image.clone();
    apply_in_place(&mut working, filter, cancelled)?;
    *image = working;
    Ok(())
}

fn apply_in_place(image: &mut RgbaImage, filter: &Filter, cancelled: &AtomicBool) -> Result<()> {
    match filter {
        Filter::BloomGlow {
            sigma,
            amount,
            threshold,
        } => {
            if *amount == 0. {
                return Ok(());
            }
            let mut bright = image.clone();
            for (index, p) in bright.pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                let weight = if *threshold >= 1. {
                    0.
                } else {
                    ((lum(rgb(p.0)) - threshold) / (1. - threshold)).clamp(0., 1.)
                };
                // Weight alpha, not straight RGB: otherwise the blur would
                // amplify low-coverage fringes when unpremultiplying its result.
                p[3] = byte(f32::from(p[3]) / 255. * weight);
            }
            let glow = blurred_cancellable(&bright, *sigma, cancelled)?;
            for (index, (p, g)) in image.pixels_mut().zip(glow.pixels()).enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                let alpha = (f32::from(g[3]) / 255. * amount).clamp(0., 1.);
                if alpha == 0. {
                    continue;
                }
                let original_alpha = f32::from(p[3]) / 255.;
                let combined = original_alpha + alpha * (1. - original_alpha);
                for c in 0..3 {
                    let source = f32::from(p[c]) / 255.;
                    let light = f32::from(g[c]) / 255.;
                    // Screen where artwork exists, and use the glow's colour
                    // where it introduces new coverage into a transparent gap.
                    let screened = 1. - (1. - source) * (1. - light * alpha);
                    p[c] = byte(
                        (screened * original_alpha + light * alpha * (1. - original_alpha))
                            / combined,
                    );
                }
                p[3] = byte(combined);
            }
        }
        Filter::VignetteOverlay {
            opacity,
            midpoint,
            feather,
            color,
        } => {
            if *opacity == 0. {
                return Ok(());
            }
            let (w, h) = (image.width() as f32, image.height() as f32);
            for (index, (x, y, p)) in image.enumerate_pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                let dx = (x as f32 + 0.5 - w / 2.) / (w / 2.);
                let dy = (y as f32 + 0.5 - h / 2.) / (h / 2.);
                let distance = (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2;
                let t = ((distance - midpoint) / feather).clamp(0., 1.);
                let alpha = opacity * t * t * (3. - 2. * t);
                if alpha == 0. {
                    continue;
                }
                let original_alpha = f32::from(p[3]) / 255.;
                let combined = alpha + original_alpha * (1. - alpha);
                for c in 0..3 {
                    p[c] = byte(
                        (f32::from(color[c]) / 255. * alpha
                            + f32::from(p[c]) / 255. * original_alpha * (1. - alpha))
                            / combined,
                    );
                }
                p[3] = byte(combined);
            }
        }
        Filter::LocalContrast {
            sigma,
            shadows,
            midtones,
            highlights,
        } => {
            if *sigma == 0. || (*shadows == 0. && *midtones == 0. && *highlights == 0.) {
                return Ok(());
            }
            let local = blurred_cancellable(image, *sigma, cancelled)?;
            for (index, (p, b)) in image.pixels_mut().zip(local.pixels()).enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                let color = rgb(p.0);
                let l = lum(color);
                let shadow = (1. - 2. * l).max(0.);
                let highlight = (2. * l - 1.).max(0.);
                let strength = shadow * shadows
                    + (1. - shadow - highlight) * midtones
                    + highlight * highlights;
                let detail = (l - lum(rgb(b.0))) * strength * 2.;
                for c in 0..3 {
                    p[c] = byte(color[c] + detail);
                }
            }
        }
        Filter::GaussianBlur { sigma } => {
            let blurred = blurred_cancellable(image, *sigma, cancelled)?;
            for (index, (p, b)) in image.pixels_mut().zip(blurred.pixels()).enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] > 0 {
                    p[0] = b[0];
                    p[1] = b[1];
                    p[2] = b[2];
                }
            }
        }
        Filter::UnsharpMask {
            sigma,
            amount,
            threshold,
        } => {
            let blurred = blurred_cancellable(image, *sigma, cancelled)?;
            for (index, (p, b)) in image.pixels_mut().zip(blurred.pixels()).enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                for c in 0..3 {
                    let original = f32::from(p[c]) / 255.0;
                    let delta = original - f32::from(b[c]) / 255.0;
                    if delta.abs() >= *threshold {
                        p[c] = byte(original + amount * delta);
                    }
                }
            }
        }
        Filter::Bloom {
            sigma,
            amount,
            threshold,
        } => {
            let mut bright = image.clone();
            for (index, p) in bright.pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                let rgb = rgb(p.0);
                let l = lum(rgb);
                let weight = if *threshold >= 1.0 {
                    0.0
                } else {
                    ((l - threshold) / (1.0 - threshold)).clamp(0.0, 1.0)
                };
                for c in 0..3 {
                    p[c] = byte(rgb[c] * weight);
                }
            }
            let bloom = blurred_cancellable(&bright, *sigma, cancelled)?;
            for (index, (p, b)) in image.pixels_mut().zip(bloom.pixels()).enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                for c in 0..3 {
                    // Screen blend avoids hard clipping at bright edges.
                    let glow = (f32::from(b[c]) / 255.0 * amount).clamp(0.0, 1.0);
                    p[c] = byte(1.0 - (1.0 - f32::from(p[c]) / 255.0) * (1.0 - glow));
                }
            }
        }
        Filter::Noise {
            amount,
            seed,
            monochrome,
        } => {
            let mut state = *seed ^ 0x9e3779b97f4a7c15;
            if state == 0 {
                state = 1;
            }
            for (index, p) in image.pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                let n = random(&mut state) * amount;
                for c in 0..3 {
                    let delta = if *monochrome {
                        n
                    } else {
                        random(&mut state) * amount
                    };
                    p[c] = byte(f32::from(p[c]) / 255.0 + delta);
                }
            }
        }
        Filter::Vignette {
            amount,
            midpoint,
            feather,
        } => {
            let w = image.width() as f32;
            let h = image.height() as f32;
            for (index, (x, y, p)) in image.enumerate_pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                let dx = (x as f32 + 0.5 - w / 2.0) / (w / 2.0);
                let dy = (y as f32 + 0.5 - h / 2.0) / (h / 2.0);
                let distance = (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2;
                let t = ((distance - midpoint) / feather).clamp(0.0, 1.0);
                let strength = t * t * (3.0 - 2.0 * t) * amount;
                for c in 0..3 {
                    let v = f32::from(p[c]) / 255.0;
                    p[c] = byte(if strength >= 0.0 {
                        v * (1.0 - strength)
                    } else {
                        v + (1.0 - v) * -strength
                    });
                }
            }
        }
        _ => {
            // Curves and levels are a fixed per-channel mapping. A lookup table
            // keeps their cost linear and independent of control-point count.
            let lut = if matches!(
                filter,
                Filter::Curves { .. } | Filter::Levels { .. } | Filter::Exposure { .. }
            ) {
                Some(std::array::from_fn::<u8, 256, _>(|i| {
                    byte(map_channel(i as f32 / 255.0, filter))
                }))
            } else {
                None
            };
            for (index, p) in image.pixels_mut().enumerate() {
                if index & 4095 == 0 {
                    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
                }
                if p[3] == 0 {
                    continue;
                }
                if let Some(lut) = &lut {
                    for c in 0..3 {
                        p[c] = lut[p[c] as usize];
                    }
                    continue;
                }
                let mut color = rgb(p.0);
                match filter {
                    Filter::Hsl {
                        hue_degrees,
                        saturation,
                        lightness,
                    } => {
                        let (h, s, l) = to_hsl(color);
                        color = from_hsl(
                            (h + hue_degrees / 360.0).rem_euclid(1.0),
                            (s + saturation).clamp(0.0, 1.0),
                            (l + lightness).clamp(0.0, 1.0),
                        );
                    }
                    Filter::ColorBalance { red, green, blue } => {
                        for (c, shift) in color.iter_mut().zip([red, green, blue]) {
                            *c += shift;
                        }
                    }
                    Filter::TonalContrast {
                        shadows,
                        midtones,
                        highlights,
                    } => {
                        let l = lum(color);
                        let shadow = (1.0 - 2.0 * l).max(0.0);
                        let highlight = (2.0 * l - 1.0).max(0.0);
                        let middle = 1.0 - shadow - highlight;
                        let strength =
                            shadow * shadows + middle * midtones + highlight * highlights;
                        for c in &mut color {
                            *c = 0.5 + (*c - 0.5) * (1.0 + strength);
                        }
                    }
                    Filter::Invert => {
                        for c in &mut color {
                            *c = 1.0 - *c;
                        }
                    }
                    Filter::Grayscale => {
                        let l = lum(color);
                        color = [l; 3];
                    }
                    _ => unreachable!("specialized filters handled above"),
                }
                for c in 0..3 {
                    p[c] = byte(color[c]);
                }
            }
        }
    }
    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
    Ok(())
}
fn rgb(p: [u8; 4]) -> [f32; 3] {
    [
        f32::from(p[0]) / 255.0,
        f32::from(p[1]) / 255.0,
        f32::from(p[2]) / 255.0,
    ]
}
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
fn lum(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn map_channel(v: f32, filter: &Filter) -> f32 {
    match filter {
        Filter::Exposure { stops } => srgb(linear(v) * 2.0f32.powf(*stops)),
        Filter::Levels {
            black,
            white,
            gamma,
        } => ((v - black) / (white - black))
            .clamp(0.0, 1.0)
            .powf(1.0 / gamma),
        Filter::Curves { points } => {
            let i = points
                .partition_point(|p| p.0 < v)
                .max(1)
                .min(points.len() - 1);
            let (x0, y0) = points[i - 1];
            let (x1, y1) = points[i];
            y0 + (y1 - y0) * (v - x0) / (x1 - x0)
        }
        _ => v,
    }
}
fn random(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    ((*state >> 40) as f32 / 16_777_215.0) * 2.0 - 1.0
}
fn to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let hi = c.into_iter().fold(0.0, f32::max);
    let lo = c.into_iter().fold(1.0, f32::min);
    let delta = hi - lo;
    let l = (hi + lo) / 2.0;
    if delta == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if hi == c[0] {
        ((c[1] - c[2]) / delta).rem_euclid(6.0)
    } else if hi == c[1] {
        (c[2] - c[0]) / delta + 2.0
    } else {
        (c[0] - c[1]) / delta + 4.0
    };
    (h / 6.0, s, l)
}
fn from_hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let rgb = match hp.floor() as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.map(|v| v + m)
}

fn blurred_cancellable(input: &RgbaImage, sigma: f32, cancelled: &AtomicBool) -> Result<RgbaImage> {
    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
    if sigma <= 0.0 {
        return Ok(input.clone());
    }
    let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut lower = ideal.floor() as u32;
    if lower % 2 == 0 {
        lower = lower.saturating_sub(1);
    }
    lower = lower.max(1);
    let upper = lower + 2;
    let lo = lower as f32;
    let count = ((12.0 * sigma * sigma - 3.0 * lo * lo - 12.0 * lo - 9.0) / (-4.0 * lo - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    let mut current = input.clone();
    let mut scratch = RgbaImage::new(input.width(), input.height());
    for pass in 0..3 {
        let radius = (if pass < count { lower } else { upper }) / 2;
        if radius == 0 {
            continue;
        }
        box_pass(&current, &mut scratch, radius, true, cancelled)?;
        box_pass(&scratch, &mut current, radius, false, cancelled)?;
    }
    ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
    Ok(current)
}
fn box_pass(
    input: &RgbaImage,
    output: &mut RgbaImage,
    radius: u32,
    horizontal: bool,
    cancelled: &AtomicBool,
) -> Result<()> {
    let (len, lines) = if horizontal {
        (input.width(), input.height())
    } else {
        (input.height(), input.width())
    };
    let get = |i: i64, line: u32| -> image::Rgba<u8> {
        let i = i.clamp(0, i64::from(len) - 1) as u32;
        *if horizontal {
            input.get_pixel(i, line)
        } else {
            input.get_pixel(line, i)
        }
    };
    let count = u64::from(radius) * 2 + 1;
    for line in 0..lines {
        ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
        let mut sum = [0u64; 4];
        let accumulate = |sum: &mut [u64; 4], p: image::Rgba<u8>, add: bool| {
            let values = [
                u64::from(p[0]) * u64::from(p[3]),
                u64::from(p[1]) * u64::from(p[3]),
                u64::from(p[2]) * u64::from(p[3]),
                u64::from(p[3]),
            ];
            for c in 0..4 {
                if add {
                    sum[c] += values[c];
                } else {
                    sum[c] -= values[c];
                }
            }
        };
        for i in -i64::from(radius)..=i64::from(radius) {
            accumulate(&mut sum, get(i, line), true);
        }
        for i in 0..len {
            if i & 4095 == 0 {
                ensure!(!cancelled.load(Ordering::Relaxed), "filter cancelled");
            }
            let p = if horizontal {
                output.get_pixel_mut(i, line)
            } else {
                output.get_pixel_mut(line, i)
            };
            if sum[3] == 0 {
                p.0 = [0; 4];
            } else {
                for c in 0..3 {
                    p[c] = ((sum[c] + sum[3] / 2) / sum[3]) as u8;
                }
                p[3] = ((sum[3] + count / 2) / count) as u8;
            }
            accumulate(&mut sum, get(i64::from(i) - i64::from(radius), line), false);
            accumulate(
                &mut sum,
                get(i64::from(i) + i64::from(radius) + 1, line),
                true,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_filter_is_transactional() {
        let mut image = RgbaImage::from_fn(32, 24, |x, y| {
            image::Rgba([
                x as u8,
                y as u8,
                128,
                if (x + y) % 5 == 0 { 0 } else { 255 },
            ])
        });
        let original = image.clone();
        let cancelled = AtomicBool::new(true);
        let result = apply_cancellable(&mut image, &Filter::GaussianBlur { sigma: 8. }, &cancelled);
        assert!(result.is_err());
        assert_eq!(image, original);
    }

    #[test]
    fn apply_cancellable_matches_regular_apply_when_not_cancelled() {
        let source = RgbaImage::from_fn(17, 11, |x, y| {
            image::Rgba([x as u8 * 7, y as u8 * 11, 95, 255])
        });
        let filter = Filter::Bloom {
            sigma: 1.2,
            amount: 0.65,
            threshold: 0.35,
        };
        let mut regular = source.clone();
        apply(&mut regular, &filter).unwrap();
        let mut cancellable = source;
        apply_cancellable(&mut cancellable, &filter, &AtomicBool::new(false)).unwrap();
        assert_eq!(regular, cancellable);
    }
}
