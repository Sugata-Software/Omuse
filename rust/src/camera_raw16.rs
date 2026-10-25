//! Native16V1: straight retained words; encoded-channel creative grades in the
//! retained primaries (sRGB or Display P3). Linear-sRGB storage is decoded only
//! at the kernel boundary. Spatial statistics are coverage-normalized. Existing
//! byte recipes never enter this pipeline. This is not a sensor-space decoder,
//! calibrated camera profile, HDR pipeline or Adobe appearance implementation.
use super::*;
use crate::precision::{Rgba16Image, TiledImage16, WorkingSpace};
use image::{ImageBuffer, Rgba};

pub const MAX_NATIVE_PIXELS: u64 = 16_777_216;
/// Working-buffer admission, independent of the caller's document/history budget.
/// A native job owns at most two word rasters plus four scalar f32 planes; the
/// immutable tiled input is shared, and final tiled output replaces scratch.
pub const MAX_NATIVE_WORK_BYTES: u64 = 512 * 1024 * 1024;

fn allocated<T: Clone>(len: usize, value: T) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(len)?;
    out.resize(len, value);
    Ok(out)
}
fn blank(width: u32, height: u32) -> Result<Rgba16Image> {
    let count = (u64::from(width) * u64::from(height) * 4).try_into()?;
    Ok(
        ImageBuffer::from_raw(width, height, allocated(count, 0u16)?)
            .expect("validated native word raster"),
    )
}
fn copied(image: &Rgba16Image) -> Result<Rgba16Image> {
    let mut copy = blank(image.width(), image.height())?;
    copy.as_mut().copy_from_slice(image.as_raw());
    Ok(copy)
}
fn flat(source: &TiledImage16, cancel: &AtomicBool) -> Result<Rgba16Image> {
    let mut out = blank(source.width(), source.height())?;
    for y in 0..source.height() {
        check_cancel(cancel)?;
        for x in 0..source.width() {
            out.put_pixel(x, y, Rgba(source.get_pixel(x, y).0));
        }
    }
    Ok(out)
}

fn tiled(image: &Rgba16Image, space: WorkingSpace, cancel: &AtomicBool) -> Result<TiledImage16> {
    check_cancel(cancel)?;
    let mut output = TiledImage16::new(image.width(), image.height(), space)?;
    for y in 0..image.height() {
        check_cancel(cancel)?;
        for x in 0..image.width() {
            output.set_pixel(x, y, crate::precision::Rgba16(image.get_pixel(x, y).0))?;
        }
    }
    check_cancel(cancel)?;
    Ok(output)
}

#[derive(Clone, Copy)]
struct Context {
    space: WorkingSpace,
}
impl Context {
    fn rgb(self, pixel: &Rgba<u16>) -> [f64; 3] {
        let rgb = [pixel[0], pixel[1], pixel[2]].map(|v| f64::from(v) / 65535.);
        if self.space == WorkingSpace::LinearSrgb {
            rgb.map(sr)
        } else {
            rgb
        }
    }
    fn write(self, pixel: &mut Rgba<u16>, rgb: [f64; 3]) {
        if pixel[3] == 0 {
            return;
        }
        let rgb = if self.space == WorkingSpace::LinearSrgb {
            rgb.map(li)
        } else {
            rgb
        };
        for channel in 0..3 {
            pixel[channel] = (c(rgb[channel]) * 65535.).round() as u16;
        }
    }
    fn luma(self, rgb: [f64; 3]) -> f64 {
        let weights = if self.space == WorkingSpace::DisplayP3 {
            [0.228974564, 0.691738522, 0.079286914]
        } else {
            [0.2126, 0.7152, 0.0722]
        };
        rgb.iter().zip(weights).map(|(v, w)| v * w).sum()
    }
    fn set_luma(self, rgb: &mut [f64; 3], target: f64, mapping: ToneMapping) {
        let old = self.luma(*rgb);
        let target = c(target);
        if mapping == ToneMapping::Legacy {
            if old < 1e-8 {
                if target > old {
                    *rgb = [target; 3];
                }
            } else {
                for v in rgb {
                    *v = c(*v * target / old);
                }
            }
        } else if target > old {
            let mix = (target - old) / (1. - old).max(f64::EPSILON);
            for v in rgb {
                *v = c(*v + (1. - *v) * mix);
            }
        } else if target < old {
            let mix = target / old.max(f64::EPSILON);
            for v in rgb {
                *v = c(*v * mix);
            }
        }
    }
    fn master(self, mut rgb: [f64; 3], s: &Settings) -> [f64; 3] {
        let tone = self.luma(rgb);
        let mapped = curve_value16(parametric(tone, &s.curve), &s.curve.rgb);
        self.set_luma(&mut rgb, mapped, s.tone_mapping);
        if s.curve.refine_saturation != 0. && tone > 1e-4 {
            let factor = 1. + f64::from(s.curve.refine_saturation) / 100. * (mapped / tone - 1.);
            let lum = self.luma(rgb);
            for v in &mut rgb {
                *v = c(lum + (*v - lum) * factor);
            }
        }
        rgb
    }
    fn curves(self, rgb: [f64; 3], s: &Settings) -> [f64; 3] {
        let rgb = self.master(rgb, s);
        [
            curve_value16(rgb[0], &s.curve.red),
            curve_value16(rgb[1], &s.curve.green),
            curve_value16(rgb[2], &s.curve.blue),
        ]
    }
    fn grading(self, mut rgb: [f64; 3], s: &Settings) -> [f64; 3] {
        let split = 0.5 - f64::from(s.grading.balance) / 100. * 0.2;
        let reach = 0.12 + f64::from(s.grading.blending) / 100. * 0.38;
        let lum = self.luma(rgb);
        let mut weights = [
            c((split + reach - lum) / (reach * 2.).max(0.05)),
            c(1. - (lum - split).abs() / (0.35 + reach)),
            c((lum - (split - reach)) / (reach * 2.).max(0.05)),
            1.,
        ];
        let sum = weights[0] + weights[1] + weights[2];
        if sum > 1e-4 {
            for w in &mut weights[..3] {
                *w /= sum;
            }
        }
        for (wheel, weight) in [
            s.grading.shadows,
            s.grading.midtones,
            s.grading.highlights,
            s.grading.global,
        ]
        .iter()
        .zip(weights)
        {
            let saturation = f64::from(wheel.saturation) / 100.;
            let luminance = f64::from(wheel.luminance) / 100.;
            if weight <= 0. || (saturation <= 0. && luminance == 0.) {
                continue;
            }
            let color = hsl(f64::from(wheel.hue) / 360., 1., 0.5);
            for ch in 0..3 {
                rgb[ch] = c(rgb[ch] + (color[ch] - 0.5) * saturation * weight * 0.85);
            }
            if luminance != 0. {
                let target = c(self.luma(rgb) + luminance * 0.25 * weight);
                self.set_luma(&mut rgb, target, s.tone_mapping);
            }
        }
        rgb
    }
}

// The preserved byte helper allocates all secants for every channel/pixel.
// Native evaluation computes only the at-most-three neighbouring secants.
fn curve_value16(x: f64, points: &[CurvePoint]) -> f64 {
    let index = points
        .windows(2)
        .position(|p| x < f64::from(p[1].x))
        .unwrap_or(points.len() - 2);
    let secant =
        |i: usize| f64::from((points[i + 1].y - points[i].y) / (points[i + 1].x - points[i].x));
    let slope = |i: usize| {
        if i == 0 {
            secant(0)
        } else if i == points.len() - 1 {
            secant(points.len() - 2)
        } else {
            let a = secant(i - 1);
            let b = secant(i);
            if a * b <= 0. {
                0.
            } else {
                2. / (1. / a + 1. / b)
            }
        }
    };
    let h = f64::from(points[index + 1].x - points[index].x);
    let t = ((x - f64::from(points[index].x)) / h).clamp(0., 1.);
    c(
        (2. * t.powi(3) - 3. * t.powi(2) + 1.) * f64::from(points[index].y)
            + (t.powi(3) - 2. * t.powi(2) + t) * h * slope(index)
            + (-2. * t.powi(3) + 3. * t.powi(2)) * f64::from(points[index + 1].y)
            + (t.powi(3) - t.powi(2)) * h * slope(index + 1),
    )
}

/// Colour values supplied to native sampler gestures are encoded in the image's
/// declared primaries and never reconstructed from its 8-bit display proxy.
pub fn sample_rgb(pixel: [u16; 4], space: WorkingSpace) -> [f64; 4] {
    let rgb = Context { space }.rgb(&Rgba(pixel));
    [rgb[0], rgb[1], rgb[2], f64::from(pixel[3]) / 65535.]
}
pub fn sample_luminance(rgb: [f64; 3], space: WorkingSpace) -> f64 {
    Context { space }.luma(rgb)
}
pub fn sample_master_curve(rgb: [f64; 3], settings: &Settings, space: WorkingSpace) -> [f64; 3] {
    Context { space }.master(rgb, settings)
}

fn basic(mut a: [f64; 3], s: &Settings, ctx: Context, auto: [f64; 3]) -> [f64; 3] {
    let w = f64::from(s.temperature) / 100.;
    let m = f64::from(s.tint) / 100.;
    let gain = [
        1. + 0.35 * w + 0.15 * m,
        1. - 0.3 * m,
        1. - 0.35 * w + 0.15 * m,
    ];
    for k in 0..3 {
        a[k] = c(li(a[k]) * gain[k] * auto[k] * 2f64.powf(f64::from(s.exposure)));
        a[k] = c(0.5 + (sr(a[k]) - 0.5) * (1. + f64::from(s.contrast) / 100.));
    }
    let mut lum = ctx.luma(a);
    let t = c((lum - 0.5) / 0.5).powi(2);
    lum = c(if s.highlights >= 0. {
        lum + f64::from(s.highlights) / 100. * t * (1. - lum)
    } else if s.tone_mapping == ToneMapping::SmoothV1 {
        lum + f64::from(s.highlights) / 100. * t * 0.25
    } else {
        lum + f64::from(s.highlights) / 100. * t * (lum - 0.5)
    });
    ctx.set_luma(&mut a, lum, s.tone_mapping);
    lum = ctx.luma(a);
    let t = c((0.5 - lum) / 0.5).powi(2);
    lum = c(
        if s.shadows >= 0. && s.tone_mapping == ToneMapping::SmoothV1 {
            lum + f64::from(s.shadows) / 100. * t * 0.25
        } else if s.shadows >= 0. {
            lum + f64::from(s.shadows) / 100. * t * (0.5 - lum)
        } else {
            lum + f64::from(s.shadows) / 100. * t * lum
        },
    );
    ctx.set_luma(&mut a, lum, s.tone_mapping);
    lum = ctx.luma(a);
    if lum > 0.75 {
        lum = c(0.75 + (lum - 0.75) * (1. + f64::from(s.whites) / 100.));
    }
    ctx.set_luma(&mut a, lum, s.tone_mapping);
    lum = ctx.luma(a);
    if lum < 0.25 {
        lum = c(0.25 + (lum - 0.25) * (1. - f64::from(s.blacks) / 100.));
    }
    ctx.set_luma(&mut a, lum, s.tone_mapping);
    let lum = ctx.luma(a);
    let max = a.iter().copied().fold(0., f64::max);
    let min = a.iter().copied().fold(1., f64::min);
    let chroma = max - min;
    let sat = if max > 1e-8 { chroma / max } else { 0. };
    let mut hue = 0.;
    if chroma > 1e-8 {
        hue = if a[0] >= a[1] && a[0] >= a[2] {
            60. * ((a[1] - a[2]) / chroma % 6.)
        } else if a[1] >= a[0] && a[1] >= a[2] {
            60. * ((a[2] - a[0]) / chroma + 2.)
        } else {
            60. * ((a[0] - a[1]) / chroma + 4.)
        };
        if hue < 0. {
            hue += 360.;
        }
    }
    let mut skin = 0.;
    if (10. ..=50.).contains(&hue) {
        skin = if hue <= 30. {
            (hue - 10.) / 20.
        } else {
            (50. - hue) / 20.
        };
        skin *= c((sat - 0.15) / 0.35);
    }
    let vibrance = f64::from(s.vibrance) / 100.;
    let mut amount = vibrance * (1. - sat);
    if vibrance > 0. {
        amount *= 1. - 0.7 * skin;
    }
    for v in &mut a {
        *v = c(lum + (*v - lum) * (1. + amount));
    }
    let lum = ctx.luma(a);
    for v in &mut a {
        *v = c(lum + (*v - lum) * (1. + f64::from(s.saturation) / 100.));
    }
    a
}

/// Bounded gray-world estimate over visible midtones after calibration.
/// It is a creative neutral estimate, not illuminant/camera-profile recovery.
fn auto_balance(
    image: &Rgba16Image,
    s: &Settings,
    ctx: Context,
    cancel: &AtomicBool,
) -> Result<[f64; 3]> {
    if s.white_balance != WhiteBalance::Auto {
        return Ok([1.; 3]);
    }
    let pixels = u64::from(image.width()) * u64::from(image.height());
    let stride = pixels.div_ceil(4096).max(1) as usize;
    let mut sums = [0.; 3];
    let mut weight = 0.;
    for i in (0..pixels as usize).step_by(stride) {
        if i % 1024 == 0 {
            check_cancel(cancel)?;
        }
        let p = image.get_pixel(
            (i % image.width() as usize) as u32,
            (i / image.width() as usize) as u32,
        );
        if p[3] == 0 {
            continue;
        }
        let rgb = calibrate(ctx.rgb(p), &s.calibration);
        let lum = ctx.luma(rgb);
        if !(0.02..0.98).contains(&lum) || rgb.iter().any(|v| *v <= 0.005 || *v >= 0.995) {
            continue;
        }
        let a = f64::from(p[3]) / 65535.;
        for c in 0..3 {
            sums[c] += li(rgb[c]) * a;
        }
        weight += a;
    }
    ensure!(
        weight > 0. && sums.iter().all(|v| *v > 1e-12),
        "Auto white balance needs visible unclipped midtones; use Custom or pick a neutral pixel"
    );
    let gains = [sums[1] / sums[0], 1., sums[1] / sums[2]];
    ensure!(
        gains.iter().all(|g| (0.25..=4.).contains(g)),
        "Auto white balance exceeds its bounded correction; use Custom or pick a neutral pixel"
    );
    Ok(gains)
}
fn point_colors_and_range(rgb: [f64; 3], s: &Settings) -> ([f64; 3], f64) {
    let (mut h, mut saturation, mut luminance) = rgb_hsl(rgb);
    let mut selected = 0f64;
    for point in &s.mixer.points {
        let weight = point_weight(h, saturation, luminance, point);
        if point.visualize {
            selected = selected.max(weight);
        }
        if weight <= 0. {
            continue;
        }
        h += f64::from(point.hue_shift) / 100. * weight * (30. / 360.);
        saturation = c(saturation * (1. + f64::from(point.saturation_shift) / 100. * weight));
        luminance = c(luminance + f64::from(point.luminance_shift) / 100. * weight * 0.25);
    }
    (hsl(h.rem_euclid(1.), saturation, luminance), selected)
}
fn point_colors(rgb: [f64; 3], s: &Settings) -> [f64; 3] {
    point_colors_and_range(rgb, s).0
}

fn plane(image: &Rgba16Image, ctx: Context, chroma: bool, cancel: &AtomicBool) -> Result<Vec<f32>> {
    let mut out = allocated(image.width() as usize * image.height() as usize, 0f32)?;
    for (i, p) in image.pixels().enumerate() {
        if i % 1024 == 0 {
            check_cancel(cancel)?;
        }
        if p[3] != 0 {
            let rgb = ctx.rgb(p);
            out[i] = if chroma {
                rgb_hsl(rgb).1 as f32
            } else {
                ctx.luma(rgb) as f32
            };
        }
    }
    Ok(out)
}

/// Coverage-weighted separable box blur. Horizontal numerator/coverage remain
/// unnormalized until the vertical pass, so alpha edges cannot acquire a black
/// halo. Clamp-edge extension matches the existing spatial control footprints.
fn blur(
    values: &[f32],
    image: &Rgba16Image,
    radius: usize,
    cancel: &AtomicBool,
) -> Result<Vec<f32>> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut numer = allocated(values.len(), 0f32)?;
    let mut cover = allocated(values.len(), 0f32)?;
    for y in 0..h {
        check_cancel(cancel)?;
        let get = |x: usize| {
            let i = y * w + x;
            let a = f64::from(image.get_pixel(x as u32, y as u32)[3]) / 65535.;
            (f64::from(values[i]) * a, a)
        };
        let mut n = 0.;
        let mut a = 0.;
        for k in 0..=radius * 2 {
            let (v, c) = get(k.saturating_sub(radius).min(w - 1));
            n += v;
            a += c;
        }
        for x in 0..w {
            if x % 1024 == 0 {
                check_cancel(cancel)?;
            }
            numer[y * w + x] = n as f32;
            cover[y * w + x] = a as f32;
            if x + 1 < w {
                let (v, c) = get(x.saturating_sub(radius));
                let (v2, c2) = get((x + radius + 1).min(w - 1));
                n += v2 - v;
                a += c2 - c;
            }
        }
    }
    let mut out = allocated(values.len(), 0f32)?;
    for x in 0..w {
        check_cancel(cancel)?;
        let mut n = 0f64;
        let mut a = 0f64;
        for k in 0..=radius * 2 {
            let i = k.saturating_sub(radius).min(h - 1) * w + x;
            n += f64::from(numer[i]);
            a += f64::from(cover[i]);
        }
        for y in 0..h {
            if y % 1024 == 0 {
                check_cancel(cancel)?;
            }
            out[y * w + x] = if a > 1e-12 {
                (n / a) as f32
            } else {
                values[y * w + x]
            };
            if y + 1 < h {
                let old = y.saturating_sub(radius) * w + x;
                let new = (y + radius + 1).min(h - 1) * w + x;
                n += f64::from(numer[new]) - f64::from(numer[old]);
                a += f64::from(cover[new]) - f64::from(cover[old]);
            }
        }
    }
    Ok(out)
}
fn pixel_loop(
    image: &mut Rgba16Image,
    ctx: Context,
    cancel: &AtomicBool,
    mut f: impl FnMut(usize, [f64; 3]) -> [f64; 3],
) -> Result<()> {
    for (index, p) in image.pixels_mut().enumerate() {
        if index % 1024 == 0 {
            check_cancel(cancel)?;
        }
        if p[3] != 0 {
            let rgb = f(index, ctx.rgb(p));
            ctx.write(p, rgb);
        }
    }
    Ok(())
}

fn effects(image: &mut Rgba16Image, s: &Settings, ctx: Context, cancel: &AtomicBool) -> Result<()> {
    if s.texture != 0. || s.clarity != 0. {
        // Process one footprint at a time to bound live scalar planes. Both
        // footprints read one immutable statistics plane; each bounded word
        // stage adds its own detail contribution to the current tone.
        let tone = plane(image, ctx, false, cancel)?;
        for (amount, radius) in [(s.texture, 1), (s.clarity, 4)] {
            if amount == 0. {
                continue;
            }
            let soft = blur(&tone, image, radius, cancel)?;
            pixel_loop(image, ctx, cancel, |i, mut rgb| {
                let target = c(ctx.luma(rgb) + f64::from((tone[i] - soft[i]) * amount / 100.));
                ctx.set_luma(&mut rgb, target, s.tone_mapping);
                rgb
            })?;
        }
    }
    if s.dehaze != 0. {
        pixel_loop(image, ctx, cancel, |_, mut rgb| {
            let d = f64::from(s.dehaze) / 100.;
            let old = ctx.luma(rgb);
            let pivot = 0.45 - 0.1 * d.max(0.);
            let mut target = c(pivot + (old - 0.45) * (1. + 0.8 * d));
            if d < 0. {
                target = c(target + (-d) * (1. - target) * 0.45);
            } else {
                target = c(target - d * (0.4 - target).max(0.));
            }
            ctx.set_luma(&mut rgb, target, s.tone_mapping);
            let lum = ctx.luma(rgb);
            for v in &mut rgb {
                *v = c(lum + (*v - lum) * (1. + 0.7 * d));
            }
            rgb
        })?;
    }
    if s.glow > 0. {
        let mut source = plane(image, ctx, false, cancel)?;
        let threshold = 0.55 + 0.4 * f64::from(s.glow_range) / 100.;
        for v in &mut source {
            *v = c((f64::from(*v) - threshold) / (1. - threshold).max(0.05)) as f32;
        }
        let base = if s.glow_style == GlowStyle::Bloom {
            2.
        } else {
            5.
        };
        let radius = (base * (1. + f64::from(s.glow_spread) / 100.))
            .round()
            .clamp(1., 64.) as usize;
        let glow = blur(&source, image, radius, cancel)?;
        drop(source);
        let warmth = f64::from(s.glow_warmth) / 100.;
        let (r, g, b, gain) = match s.glow_style {
            GlowStyle::Halation => (1., 0.35 - 0.3 * warmth, 0.2 - 0.2 * warmth, 1.),
            GlowStyle::Bloom => (
                0.75 + 0.25 * warmth,
                0.6 + 0.2 * warmth,
                0.75 - 0.6 * warmth,
                1.4,
            ),
            GlowStyle::Diffusion => (
                0.75 + 0.25 * warmth,
                0.6 + 0.2 * warmth,
                0.75 - 0.6 * warmth,
                1.,
            ),
        };
        pixel_loop(image, ctx, cancel, |i, mut rgb| {
            let add = f64::from(glow[i]) * f64::from(s.glow) / 100. * gain;
            rgb[0] = c(rgb[0] + add * r);
            rgb[1] = c(rgb[1] + add * g);
            rgb[2] = c(rgb[2] + add * b);
            rgb
        })?;
    }
    if s.vignette_amount != 0. {
        let (w, h) = (image.width() as usize, image.height() as usize);
        pixel_loop(image, ctx, cancel, |i, mut rgb| {
            let nx = (i % w) as f64 + 0.5;
            let ny = (i / w) as f64 + 0.5;
            let nx = nx / w as f64 * 2. - 1.;
            let ny = ny / h as f64 * 2. - 1.;
            let circle = nx.hypot(ny) / 2f64.sqrt();
            let square = nx.abs().max(ny.abs());
            let shape = (1. - f64::from(s.vignette_roundness) / 100.) * 0.5;
            let distance = circle + (square - circle) * shape;
            let start = f64::from(s.vignette_midpoint) / 100. * 0.85;
            let soft = (f64::from(s.vignette_feather) / 100.).max(0.05);
            let t = c((distance - start) / soft);
            let mask = t * t * (3. - 2. * t);
            let mut amount = f64::from(s.vignette_amount) / 100. * mask;
            if amount < 0. && s.vignette_style == VignetteStyle::HighlightPriority {
                amount *=
                    1. - f64::from(s.vignette_highlights) / 100. * c((ctx.luma(rgb) - 0.45) / 0.55);
            }
            for v in &mut rgb {
                if amount < 0. {
                    *v *= 1. + amount;
                } else {
                    *v += (1. - *v) * amount;
                }
            }
            if s.vignette_style == VignetteStyle::ColorPriority && mask > 0. {
                let lum = ctx.luma(rgb);
                let sat = 1. - 0.75 * mask * (f64::from(s.vignette_amount) / 100.).abs();
                for v in &mut rgb {
                    *v = c(lum + (*v - lum) * sat);
                }
            }
            rgb
        })?;
    }
    Ok(())
}

fn grain(image: &mut Rgba16Image, s: &Settings, ctx: Context, cancel: &AtomicBool) -> Result<()> {
    if s.grain_amount <= 0. {
        return Ok(());
    }
    let width = image.width() as usize;
    let size = 0.5 + f64::from(s.grain_size) / 100. * 19.5;
    let detail = (size * 0.35).max(0.5);
    let rough = f64::from(s.grain_roughness) / 100.;
    pixel_loop(image, ctx, cancel, |i, mut rgb| {
        let u = (i % width) as f64 + 0.5;
        let v = (i / width) as f64 + 0.5;
        let smooth = grain_field(u, v, size, 0);
        let fine = grain_field(u, v, detail, mix32(0xA511E9B3));
        let noise = smooth + (fine - smooth) * rough;
        let level = ctx.luma(rgb);
        let delta =
            noise * f64::from(s.grain_amount) / 100. * 0.35 * (0.4 + 2.4 * level * (1. - level));
        for ch in &mut rgb {
            *ch = c(*ch + delta);
        }
        rgb
    })
}

fn edge(values: &[f32], image: &Rgba16Image, x: usize, y: usize, radius: usize) -> f64 {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let center = values[y * w + x];
    let mut sum = 0.;
    let mut weight = 0.;
    for dy in [-(radius as isize), 0, radius as isize] {
        for dx in [-(radius as isize), 0, radius as isize] {
            if dx == 0 && dy == 0 {
                continue;
            }
            let sx = x as isize + dx;
            let sy = y as isize + dy;
            if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                let a = f64::from(image.get_pixel(sx as u32, sy as u32)[3]) / 65535.;
                sum += f64::from((values[sy as usize * w + sx as usize] - center).abs()) * a;
                weight += a;
            }
        }
    }
    if weight > 0. { sum / weight } else { 0. }
}
fn detail(
    image: &mut Rgba16Image,
    s: &DetailSettings,
    mapping: ToneMapping,
    ctx: Context,
    cancel: &AtomicBool,
) -> Result<()> {
    let width = image.width() as usize;
    if s.noise_luminance > 0. {
        let original = plane(image, ctx, false, cancel)?;
        let radius = (1. + f64::from(s.noise_luminance) / 50.)
            .round()
            .clamp(1., 64.) as usize;
        let blurred = blur(&original, image, radius, cancel)?;
        // Compute edge gates before mutating; edge() consults coverage only.
        for y in 0..image.height() {
            check_cancel(cancel)?;
            for x in 0..image.width() {
                let i = y as usize * width + x as usize;
                if image.get_pixel(x, y)[3] == 0 {
                    continue;
                }
                let e = edge(&original, image, x as usize, y as usize, 1);
                let local = f64::from(s.noise_luminance) / 100.
                    * (1. - f64::from(s.noise_luminance_detail) / 100. * (e * 6.).min(1.));
                let target = f64::from(original[i]) * (1. - local)
                    + f64::from(blurred[i]) * local
                    + f64::from(s.noise_luminance_contrast) / 100.
                        * 0.25
                        * f64::from(original[i] - blurred[i]);
                let p = image.get_pixel_mut(x, y);
                let mut rgb = ctx.rgb(p);
                ctx.set_luma(&mut rgb, target, mapping);
                ctx.write(p, rgb);
            }
        }
    }
    if s.noise_color > 0. {
        let original = plane(image, ctx, true, cancel)?;
        let radius = (1. + f64::from(s.noise_color_smoothness) / 40.)
            .round()
            .clamp(1., 64.) as usize;
        let blurred = blur(&original, image, radius, cancel)?;
        pixel_loop(image, ctx, cancel, |i, rgb| {
            let e = (original[i] - blurred[i]).abs();
            let local = f64::from(s.noise_color) / 100.
                * (1. - f64::from(s.noise_color_detail) / 100. * f64::from((e * 4.).min(1.)));
            let sat = f64::from(original[i]) * (1. - local) + f64::from(blurred[i]) * local;
            let (h, _, l) = rgb_hsl(rgb);
            hsl(h, sat, l)
        })?;
    }
    if s.sharpen_amount > 0. {
        let original = plane(image, ctx, false, cancel)?;
        let radius = (0.5 + f64::from(s.sharpen_radius) / 100. * 2.5)
            .round()
            .clamp(1., 64.) as usize;
        let blurred = blur(&original, image, radius, cancel)?;
        let detail = f64::from(s.sharpen_detail) / 100.;
        let threshold = f64::from(s.sharpen_masking) / 100. * 0.35;
        for y in 0..image.height() {
            check_cancel(cancel)?;
            for x in 0..image.width() {
                let i = y as usize * width + x as usize;
                if image.get_pixel(x, y)[3] == 0 {
                    continue;
                }
                let mask = c((edge(&original, image, x as usize, y as usize, radius)
                    * (0.5 + detail)
                    - threshold)
                    / (0.35 - threshold * 0.5).max(0.04));
                let target = c(f64::from(original[i])
                    + f64::from(original[i] - blurred[i]) * f64::from(s.sharpen_amount) / 100.
                        * mask
                        * (0.5 + detail));
                let p = image.get_pixel_mut(x, y);
                let mut rgb = ctx.rgb(p);
                ctx.set_luma(&mut rgb, target, mapping);
                ctx.write(p, rgb);
            }
        }
    }
    Ok(())
}

/// Premultiplied interpolation uses unrounded alpha for colour reconstruction.
/// Hidden RGB contributes only when every contributing alpha is zero; it never
/// leaks into covered colour. Exterior coverage is transparent black.
fn bilinear(image: &Rgba16Image, x: f64, y: f64) -> Rgba<u16> {
    if !x.is_finite()
        || !y.is_finite()
        || x < -1.
        || y < -1.
        || x > f64::from(image.width())
        || y > f64::from(image.height())
    {
        return Rgba([0; 4]);
    }
    let bx = x.floor() as i64;
    let by = y.floor() as i64;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let mut rgb = [0.; 3];
    let mut hidden = [0.; 3];
    let mut alpha = 0.;
    let mut weights = 0.;
    for j in 0..2 {
        for i in 0..2 {
            let px = bx + i;
            let py = by + j;
            if px < 0 || py < 0 || px >= i64::from(image.width()) || py >= i64::from(image.height())
            {
                continue;
            }
            let w = (if i == 0 { 1. - fx } else { fx }) * (if j == 0 { 1. - fy } else { fy });
            let p = image.get_pixel(px as u32, py as u32);
            let a = f64::from(p[3]) / 65535.;
            for c in 0..3 {
                rgb[c] += w * f64::from(p[c]) * a;
                hidden[c] += w * f64::from(p[c]);
            }
            alpha += w * a;
            weights += w;
        }
    }
    let mut out = [0; 4];
    out[3] = (alpha * 65535.).round().clamp(0., 65535.) as u16;
    for c in 0..3 {
        out[c] = if alpha > 0. {
            (rgb[c] / alpha).round().clamp(0., 65535.) as u16
        } else if weights > 0. {
            (hidden[c] / weights).round().clamp(0., 65535.) as u16
        } else {
            0
        };
    }
    Rgba(out)
}
fn distort(image: &Rgba16Image, k: f64, cancel: &AtomicBool) -> Result<Rgba16Image> {
    let (w, h) = (image.width(), image.height());
    let (cx, cy) = (f64::from(w) * 0.5, f64::from(h) * 0.5);
    let diagonal = cx * cx + cy * cy;
    let mut output = blank(w, h)?;
    for y in 0..h {
        check_cancel(cancel)?;
        for x in 0..w {
            let dx = f64::from(x) + 0.5 - cx;
            let dy = f64::from(y) + 0.5 - cy;
            let scale = 1. - k * (dx * dx + dy * dy) / diagonal;
            output.put_pixel(
                x,
                y,
                bilinear(image, cx + dx * scale - 0.5, cy + dy * scale - 0.5),
            );
        }
    }
    Ok(output)
}
fn optics(
    image: &mut Rgba16Image,
    s: &OpticsSettings,
    ctx: Context,
    cancel: &AtomicBool,
    capture: bool,
) -> Result<Option<Rgba16Image>> {
    let distortion = f64::from(s.distortion) / 100. * 0.35
        + if s.enable_lens_profile {
            f64::from(s.profile_distortion) / 100. * 0.35
        } else {
            0.
        };
    if distortion != 0. {
        *image = distort(image, distortion, cancel)?;
    }
    if s.remove_chromatic_aberration {
        let source = copied(image)?;
        let (w, h) = (image.width(), image.height());
        let (cx, cy) = (f64::from(w) * 0.5, f64::from(h) * 0.5);
        let max_radius = cx.hypot(cy);
        for y in 0..h {
            check_cancel(cancel)?;
            for x in 0..w {
                let p = image.get_pixel_mut(x, y);
                if p[3] == 0 {
                    continue;
                }
                let dx = f64::from(x) + 0.5 - cx;
                let dy = f64::from(y) + 0.5 - cy;
                let shift = 0.45 * (dx.hypot(dy) / max_radius).powi(2) * 2.5;
                let rx = (f64::from(x) - shift).round().clamp(0., f64::from(w - 1)) as u32;
                let bx = (f64::from(x) + shift).round().clamp(0., f64::from(w - 1)) as u32;
                let r = source.get_pixel(rx, y);
                let b = source.get_pixel(bx, y);
                let mut rgb = ctx.rgb(p);
                rgb[0] = c(ctx.rgb(r)[0] * f64::from(r[3]) / f64::from(p[3]));
                rgb[2] = c(ctx.rgb(b)[2] * f64::from(b[3]) / f64::from(p[3]));
                ctx.write(p, rgb);
            }
        }
    }
    let sample = if capture { Some(copied(image)?) } else { None };
    let vignette = f64::from(s.vignette_amount)
        + if s.enable_lens_profile {
            f64::from(s.profile_vignetting) / 100. * 35.
        } else {
            0.
        };
    let (w, h) = (image.width() as usize, image.height() as usize);
    pixel_loop(image, ctx, cancel, |i, mut rgb| {
        let hue = hue_degrees(rgb);
        let max = rgb.iter().copied().fold(0., f64::max);
        let min = rgb.iter().copied().fold(1., f64::min);
        if max - min > 1e-6 {
            let sat = (max - min) / max;
            let mut reduce = 0f64;
            if s.purple_amount > 0.
                && hue_in_range(
                    hue,
                    f64::from(s.purple_hue_low),
                    f64::from(s.purple_hue_high),
                )
            {
                reduce = reduce.max(f64::from(s.purple_amount) / 100.);
            }
            if s.green_amount > 0.
                && hue_in_range(hue, f64::from(s.green_hue_low), f64::from(s.green_hue_high))
            {
                reduce = reduce.max(f64::from(s.green_amount) / 100.);
            }
            if reduce > 0. {
                let lum = ctx.luma(rgb);
                let factor = 1. - reduce * sat;
                for v in &mut rgb {
                    *v = c(lum + (*v - lum) * factor);
                }
            }
        }
        if vignette != 0. {
            let nx = ((i % w) as f64 + 0.5) / w as f64 * 2. - 1.;
            let ny = ((i / w) as f64 + 0.5) / h as f64 * 2. - 1.;
            let distance = nx.hypot(ny) / 2f64.sqrt();
            let start = f64::from(s.vignette_midpoint) / 100. * 0.85;
            let t = c((distance - start) / 0.35);
            let lift = vignette / 100. * t * t * (3. - 2. * t);
            for v in &mut rgb {
                if lift > 0. {
                    *v = c(*v + (1. - *v) * lift);
                } else {
                    *v *= 1. + lift;
                }
            }
        }
        rgb
    })?;
    Ok(sample)
}

fn geometry(image: &Rgba16Image, s: &GeometrySettings, cancel: &AtomicBool) -> Result<Rgba16Image> {
    check_cancel(cancel)?;
    let guides: Vec<_> = s
        .guides
        .iter()
        .copied()
        .filter(|g| f64::from(g.end_x - g.start_x).hypot(f64::from(g.end_y - g.start_y)) > 0.01)
        .collect();
    let uses_guides = s.upright == UprightMode::Guided && !guides.is_empty();
    if !uses_guides
        && s.vertical == 0.
        && s.horizontal == 0.
        && s.rotate == 0.
        && s.aspect == 0.
        && s.scale == 0.
        && s.offset_x == 0.
        && s.offset_y == 0.
    {
        return copied(image);
    }
    let (mut vertical, mut horizontal, mut rotate) = (
        f64::from(s.vertical),
        f64::from(s.horizontal),
        f64::from(s.rotate),
    );
    if uses_guides {
        let first = guides[0];
        let angle = f64::from(first.end_y - first.start_y)
            .atan2(f64::from(first.end_x - first.start_x))
            .to_degrees();
        let mut correction = -angle;
        if correction > 45. {
            correction -= 90.
        } else if correction < -45. {
            correction += 90.
        }
        rotate += correction;
        if let Some(second) = guides.get(1) {
            let angle = f64::from(second.end_y - second.start_y)
                .atan2(f64::from(second.end_x - second.start_x))
                .to_degrees();
            if angle.abs() > 45. {
                vertical += if angle > 0. { 25. } else { -25. }
            } else {
                horizontal += if angle > 0. { 25. } else { -25. }
            }
        }
    }
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let strength = if s.projection == Projection::Perspective {
        1.
    } else {
        0.55
    };
    let v = vertical / 100. * w * 0.18 * strength;
    let hz = horizontal / 100. * h * 0.18 * strength;
    let aspect = 1. + f64::from(s.aspect) / 200.;
    let zoom = 1. + f64::from(s.scale) / 100.;
    let shift_x = f64::from(s.offset_x) / 100. * w * 0.15;
    let shift_y = f64::from(s.offset_y) / 100. * h * 0.15;
    let mut corners = [
        [-v + shift_x, 0. - shift_y],
        [w + v + shift_x, 0. - shift_y],
        [w + hz + shift_x, h + shift_y],
        [-hz + shift_x, h + shift_y],
    ];
    let center = [w / 2. + shift_x, h / 2. - shift_y];
    let radians = -rotate.to_radians();
    for point in &mut corners {
        let (dx, dy) = (point[0] - center[0], point[1] - center[1]);
        point[0] = center[0] + dx * radians.cos() - dy * radians.sin();
        point[1] = center[1] + dx * radians.sin() + dy * radians.cos();
        point[0] = center[0] + (point[0] - center[0]) * aspect * zoom;
        point[1] = center[1] + (point[1] - center[1]) / aspect * zoom;
    }
    let source = [[0., 0.], [w, 0.], [w, h], [0., h]];
    let map = solve_homography(corners, source).ok_or_else(|| {
        anyhow::anyhow!(
            "Native geometry has a singular projection; reduce its scale or perspective"
        )
    })?;
    let mut output = blank(image.width(), image.height())?;
    for y0 in 0..image.height() {
        check_cancel(cancel)?;
        for x0 in 0..image.width() {
            let x = f64::from(x0) + 0.5;
            let y = f64::from(y0) + 0.5;
            let denominator = map[6] * x + map[7] * y + 1.;
            let sx = (map[0] * x + map[1] * y + map[2]) / denominator - 0.5;
            let sy = (map[3] * x + map[4] * y + map[5]) / denominator - 0.5;
            output.put_pixel(x0, y0, bilinear(image, sx, sy));
        }
    }
    if !s.constrain_crop {
        return Ok(output);
    }
    let mut min_x = image.width();
    let mut min_y = image.height();
    let mut max_x = 0;
    let mut max_y = 0;
    for (x, y0, p) in output.enumerate_pixels() {
        if x % 1024 == 0 {
            check_cancel(cancel)?;
        }
        if p[3] > 0 {
            min_x = min_x.min(x);
            min_y = min_y.min(y0);
            max_x = max_x.max(x + 1);
            max_y = max_y.max(y0 + 1);
        }
    }
    if min_x >= max_x || min_y >= max_y {
        return Ok(output);
    }
    let crop_width = max_x - min_x;
    let crop_height = max_y - min_y;
    let scale = (w / f64::from(crop_width)).min(h / f64::from(crop_height));
    let nw = (f64::from(crop_width) * scale).round().max(1.).min(w) as u32;
    let nh = (f64::from(crop_height) * scale).round().max(1.).min(h) as u32;
    let ox = (image.width() - nw) / 2;
    let oy = (image.height() - nh) / 2;
    let mut fitted = blank(image.width(), image.height())?;
    for y in 0..nh {
        check_cancel(cancel)?;
        for x in 0..nw {
            let sx = f64::from(min_x)
                + (f64::from(x) + 0.5) * f64::from(crop_width) / f64::from(nw)
                - 0.5;
            let sy = f64::from(min_y)
                + (f64::from(y) + 0.5) * f64::from(crop_height) / f64::from(nh)
                - 0.5;
            fitted.put_pixel(x + ox, y + oy, bilinear(&output, sx, sy));
        }
    }
    check_cancel(cancel)?;
    Ok(fitted)
}

pub fn work_bytes(dimensions: (u32, u32)) -> Result<u64> {
    let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
    ensure!(
        crate::model::valid_dimensions(dimensions.0, dimensions.1) && pixels <= MAX_NATIVE_PIXELS,
        "Native Camera Raw supports images up to 16 million pixels"
    );
    let bytes = pixels
        .checked_mul(32)
        .ok_or_else(|| anyhow::anyhow!("Native Camera Raw memory overflow"))?;
    ensure!(
        bytes <= MAX_NATIVE_WORK_BYTES,
        "Native Camera Raw working buffers exceed 512 MiB"
    );
    Ok(bytes)
}

/// Conservative admission estimate for the native dialog's known owned CPU
/// surfaces and current retained history. This is neither a measured peak RSS
/// nor an OS/GPU/process-wide hard limit. Shared sources may be counted twice;
/// documents with many layers can be rejected before any new buffers exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiAdmission {
    pub existing_document_bytes: u64,
    pub history_bytes: u64,
    /// Canonical active Editor owners, including its history; zero for legacy callers.
    pub active_editor_bytes: u64,
    pub contextual_bytes: u64,
    pub kernel_scratch_bytes: u64,
    pub renderer_reserve_bytes: u64,
    pub estimated_bytes: u64,
}
pub const MAX_NATIVE_UI_ESTIMATE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub fn ui_admission(
    document: &crate::model::Document,
    history_bytes: u64,
    dimensions: (u32, u32),
    preview: bool,
    visualize: bool,
    selection_bytes: u64,
) -> Result<UiAdmission> {
    ui_admission_with_context(
        document,
        history_bytes,
        dimensions,
        preview,
        visualize,
        selection_bytes,
        0,
    )
}
/// Contextual owners include inactive Create pages/editors/checkpoints; callers
/// must refuse busy caches rather than warming them or supplying a lower bound.
pub fn ui_admission_with_context(
    document: &crate::model::Document,
    history_bytes: u64,
    dimensions: (u32, u32),
    preview: bool,
    visualize: bool,
    selection_bytes: u64,
    contextual_bytes: u64,
) -> Result<UiAdmission> {
    calculate_ui_admission(
        document,
        ActiveOwners::LegacyHistory(history_bytes),
        dimensions,
        preview,
        visualize,
        selection_bytes,
        contextual_bytes,
    )
}

/// Canonical active Editor ownership is counted once, including its document,
/// selection, history and pending interaction owners. `contextual_bytes` must
/// contain only owners outside that Editor (Create project/inactive pages,
/// collection checkpoints and other caller-owned retained context). The caller
/// must propagate busy/poisoned context errors, never supply a lower bound.
/// Renderer, capture and job reserves and the admission cap remain unchanged.
pub fn ui_admission_for_editor(
    editor: &crate::editor::Editor,
    dimensions: (u32, u32),
    preview: bool,
    visualize: bool,
    selection_bytes: u64,
    contextual_bytes: u64,
) -> Result<UiAdmission> {
    let active_editor_bytes = editor.resident_memory_estimate()?;
    calculate_ui_admission(
        &editor.document,
        ActiveOwners::Editor(active_editor_bytes),
        dimensions,
        preview,
        visualize,
        selection_bytes,
        contextual_bytes,
    )
}

enum ActiveOwners {
    LegacyHistory(u64),
    Editor(u64),
}
fn calculate_ui_admission(
    document: &crate::model::Document,
    owners: ActiveOwners,
    dimensions: (u32, u32),
    preview: bool,
    visualize: bool,
    selection_bytes: u64,
    contextual_bytes: u64,
) -> Result<UiAdmission> {
    fn visit(
        layers: &[crate::model::Layer],
        bytes: &mut u64,
        areas: &mut u64,
        count: &mut u64,
        include_bytes: bool,
    ) {
        for layer in layers {
            *count = count.saturating_add(1);
            if include_bytes {
                *bytes = bytes
                    .saturating_add(512)
                    .saturating_add(layer.name.len() as u64)
                    .saturating_add(layer.metadata.to_string().len() as u64);
            }
            for image in [&layer.image, &layer.mask].into_iter().flatten() {
                if include_bytes {
                    *bytes = bytes.saturating_add(image.as_raw().capacity() as u64);
                }
                *areas = areas.saturating_add(u64::from(image.width()) * u64::from(image.height()));
            }
            if include_bytes {
                if let Some(state) = &layer.advanced {
                    *bytes = bytes.saturating_add(state.retained_bytes() as u64);
                }
                if let Some(scene) = &layer.vector_scene {
                    *bytes = bytes.saturating_add(scene.retained_bytes() as u64);
                }
            }
            visit(&layer.children, bytes, areas, count, include_bytes);
        }
    }
    let kernel_scratch_bytes = work_bytes(dimensions)?;
    ensure!(
        crate::model::valid_dimensions(document.width, document.height),
        "Invalid native preview canvas dimensions"
    );
    let layer_pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
    let canvas_pixels = u64::from(document.width) * u64::from(document.height);
    let (mut existing_document_bytes, mut areas, mut count) = (0u64, 0u64, 0u64);
    let include_bytes = matches!(&owners, ActiveOwners::LegacyHistory(_));
    visit(
        &document.layers,
        &mut existing_document_bytes,
        &mut areas,
        &mut count,
        include_bytes,
    );
    if include_bytes {
        existing_document_bytes = existing_document_bytes
            .saturating_add(document.metadata.to_string().len() as u64)
            .saturating_add(document.name.len() as u64);
    }
    // Legacy callers retain their existing document/history accounting. The
    // typed Editor path supplies those same owners via its canonical estimate;
    // adding the legacy representation again would reject the next RAW edit.
    let (existing_document_bytes, history_bytes, active_editor_bytes) = match owners {
        ActiveOwners::LegacyHistory(history) => (existing_document_bytes, history, 0),
        ActiveOwners::Editor(bytes) => (0, 0, bytes),
    };
    // Sum rather than max across layer/clip/group surfaces to cover nested
    // renderer planes, clipping alpha/tonal planes, live masks, source words and
    // wide-gamut conversion. Render fallback/effect expansion has additional
    // application limits; this estimate does not replace their validation.
    let renderer_reserve_bytes = canvas_pixels
        .saturating_mul(24)
        .saturating_mul(count.max(1))
        .saturating_add(areas.saturating_mul(16))
        .saturating_add(canvas_pixels.saturating_mul(12));
    // Existing frame/display/source caches and previous native sampler, plus
    // bounded UI/metadata/slab headroom. One active and one replacement request
    // can retain captured selections and layer-space masks simultaneously.
    let resident = existing_document_bytes
        .saturating_add(history_bytes)
        .saturating_add(active_editor_bytes)
        .saturating_add(contextual_bytes)
        .saturating_add(canvas_pixels.saturating_mul(12))
        .saturating_add(layer_pixels.saturating_mul(12))
        .saturating_add(selection_bytes.saturating_mul(4))
        .saturating_add(layer_pixels.saturating_mul(2))
        .saturating_add(32 * 1024 * 1024);
    // Prepared output, stage sample, native range, proxies and transient P3
    // conversion coexist after grading. The kernel planes are already gone.
    let post_kernel = layer_pixels
        .saturating_mul(if preview {
            if visualize { 48 } else { 40 }
        } else {
            24
        })
        .saturating_add(renderer_reserve_bytes);
    let estimated_bytes = resident.saturating_add(kernel_scratch_bytes.max(post_kernel));
    ensure!(
        estimated_bytes <= MAX_NATIVE_UI_ESTIMATE_BYTES,
        "Native Camera Raw conservative memory estimate is {} MiB (limit {} MiB); reduce canvas/layers or retained history before retrying. The project is unchanged",
        estimated_bytes.div_ceil(1024 * 1024),
        MAX_NATIVE_UI_ESTIMATE_BYTES / (1024 * 1024)
    );
    Ok(UiAdmission {
        existing_document_bytes,
        history_bytes,
        active_editor_bytes,
        contextual_bytes,
        kernel_scratch_bytes,
        renderer_reserve_bytes,
        estimated_bytes,
    })
}

pub fn apply(
    source: &TiledImage16,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<TiledImage16> {
    Ok(develop(source, settings, cancel, None)?.0)
}
pub fn apply_with_sample(
    source: &TiledImage16,
    settings: &Settings,
    cancel: &AtomicBool,
    stage: SampleStage,
) -> Result<(TiledImage16, TiledImage16)> {
    let (result, sample) = develop(source, settings, cancel, Some(stage))?;
    Ok((result, sample.expect("requested native sampling stage")))
}
/// Prepared once per native job; byte recipes keep their preserved pipeline.
/// A neutral group is skipped only when its retained-domain map is identity.
/// Non-default groups retain their original order and arithmetic unchanged.
struct PointPlan {
    calibration: bool,
    basic: bool,
    curves: bool,
    sector_mixer: bool,
    points: bool,
    grading: bool,
}
impl PointPlan {
    fn new(s: &Settings, defaults: &Settings) -> Self {
        Self {
            calibration: s.calibration != defaults.calibration,
            basic: s.white_balance == WhiteBalance::Auto
                || [
                    s.temperature,
                    s.tint,
                    s.exposure,
                    s.contrast,
                    s.highlights,
                    s.shadows,
                    s.whites,
                    s.blacks,
                    s.vibrance,
                    s.saturation,
                ]
                .into_iter()
                .any(|v| v != 0.),
            curves: s.curve != defaults.curve,
            sector_mixer: s
                .mixer
                .hue
                .iter()
                .chain(&s.mixer.saturation)
                .chain(&s.mixer.luminance)
                .any(|v| *v != 0.),
            points: !s.mixer.points.is_empty(),
            grading: s.grading != defaults.grading,
        }
    }
}

fn develop(
    source: &TiledImage16,
    s: &Settings,
    cancel: &AtomicBool,
    capture: Option<SampleStage>,
) -> Result<(TiledImage16, Option<TiledImage16>)> {
    check_cancel(cancel)?;
    validate(s)?;
    ensure!(
        s.geometry.scale > -100.,
        "Native geometry scale must exceed -100%; zero-size geometry cannot be applied"
    );
    if s.geometry.upright == UprightMode::Guided {
        ensure!(
            s.geometry
                .guides
                .iter()
                .any(
                    |g| f64::from(g.end_x - g.start_x).hypot(f64::from(g.end_y - g.start_y)) > 0.01
                ),
            "Draw a nonzero geometry guide before using Guided upright"
        );
    }
    work_bytes(source.dimensions())?;
    ensure!(
        s.processing == Processing::Native16V1,
        "Native Camera Raw requires Native16V1 processing"
    );
    let defaults = Settings {
        processing: s.processing,
        tone_mapping: s.tone_mapping,
        ..Settings::default()
    };
    if *s == defaults {
        return Ok((source.clone(), capture.map(|_| source.clone())));
    }
    let plan = PointPlan::new(s, &defaults);
    let ctx = Context {
        space: source.working_space(),
    };
    let input = flat(source, cancel)?;
    let mut image = geometry(&input, &s.geometry, cancel)?;
    drop(input);
    let mut sample = if capture.is_some() && capture != Some(SampleStage::Optics) {
        Some(blank(image.width(), image.height())?)
    } else {
        None
    };
    let sample_width = image.width() as usize;
    let auto = auto_balance(&image, s, ctx, cancel)?;
    let mut sector_settings = s.clone();
    sector_settings.mixer.points.clear();
    for (i, p) in image.pixels_mut().enumerate() {
        if i % 1024 == 0 {
            check_cancel(cancel)?;
        }
        if let Some(sample) = &mut sample {
            sample.as_mut()[i * 4..i * 4 + 4].copy_from_slice(&p.0);
        }
        if p[3] == 0 {
            continue;
        }
        let mut rgb = ctx.rgb(p);
        if plan.calibration {
            rgb = calibrate(rgb, &s.calibration);
        }
        if capture == Some(SampleStage::WhiteBalance) {
            if let Some(sample) = &mut sample {
                ctx.write(
                    sample.get_pixel_mut((i % sample_width) as u32, (i / sample_width) as u32),
                    rgb,
                );
            }
        }
        if plan.basic {
            rgb = basic(rgb, s, ctx, auto);
        }
        if capture == Some(SampleStage::Curve) {
            if let Some(sample) = &mut sample {
                ctx.write(
                    sample.get_pixel_mut((i % sample_width) as u32, (i / sample_width) as u32),
                    rgb,
                );
            }
        }
        if plan.curves {
            rgb = ctx.curves(rgb, s);
        }
        if capture == Some(SampleStage::Mixer) {
            if let Some(sample) = &mut sample {
                ctx.write(
                    sample.get_pixel_mut((i % sample_width) as u32, (i / sample_width) as u32),
                    rgb,
                );
            }
        }
        if plan.sector_mixer {
            rgb = apply_mixer(rgb, &sector_settings);
        }
        if capture == Some(SampleStage::PointColor) {
            if let Some(sample) = &mut sample {
                ctx.write(
                    sample.get_pixel_mut((i % sample_width) as u32, (i / sample_width) as u32),
                    rgb,
                );
            }
        }
        if plan.points {
            rgb = point_colors(rgb, s);
        }
        if plan.grading {
            rgb = ctx.grading(rgb, s);
        }
        ctx.write(p, rgb);
    }
    effects(&mut image, s, ctx, cancel)?;
    grain(&mut image, s, ctx, cancel)?;
    if let Some(optics_sample) = optics(
        &mut image,
        &s.optics,
        ctx,
        cancel,
        capture == Some(SampleStage::Optics),
    )? {
        sample = Some(optics_sample);
    }
    detail(&mut image, &s.detail, s.tone_mapping, ctx, cancel)?;
    check_cancel(cancel)?;
    let result = tiled(&image, ctx.space, cancel)?;
    drop(image);
    let sample = sample.map(|p| tiled(&p, ctx.space, cancel)).transpose()?;
    check_cancel(cancel)?;
    Ok((result, sample))
}

/// Exact centre samples of original retained words. Only pointwise settings
/// qualify; spatial effects, grain, geometry, selections and complex composites
/// must use full-detail evaluation. This function does not resize an image.
pub fn sampled_source(
    source: &TiledImage16,
    size: (u32, u32),
    cancel: &AtomicBool,
) -> Result<TiledImage16> {
    ensure!(
        size.0 > 0
            && size.1 > 0
            && size.0 <= source.width()
            && size.1 <= source.height()
            && u64::from(size.0) * u64::from(size.1) <= preview::MAX_DRAFT_PIXELS,
        "Invalid native preview dimensions"
    );
    let mut out = blank(size.0, size.1)?;
    for y in 0..size.1 {
        check_cancel(cancel)?;
        let sy = sample_coordinate(y, size.1, source.height());
        for x in 0..size.0 {
            let sx = sample_coordinate(x, size.0, source.width());
            out.put_pixel(x, y, Rgba(source.get_pixel(sx, sy).0));
        }
    }
    tiled(&out, source.working_space(), cancel)
}
pub fn sample_coordinate(at: u32, count: u32, extent: u32) -> u32 {
    (((u64::from(at) * 2 + 1) * u64::from(extent)) / (u64::from(count) * 2)) as u32
}
pub fn pointwise(settings: &Settings) -> bool {
    if settings.white_balance == WhiteBalance::Auto
        || settings.mixer.points.iter().any(|p| p.visualize)
    {
        return false;
    }
    let defaults = Settings::default();
    let mut rest = settings.clone();
    rest.processing = defaults.processing;
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

/// Point-colour range view is transient display data. Apply/save always retain
/// the unmodified native result, regardless of these visualization switches.
pub fn range_preview(
    result: &TiledImage16,
    sample: &TiledImage16,
    s: &Settings,
    cancel: &AtomicBool,
) -> Result<TiledImage16> {
    ensure!(
        result.dimensions() == sample.dimensions()
            && result.working_space() == sample.working_space(),
        "Point-colour reference does not match the result"
    );
    let ctx = Context {
        space: result.working_space(),
    };
    let mut output = TiledImage16::new(result.width(), result.height(), result.working_space())?;
    for y in 0..result.height() {
        check_cancel(cancel)?;
        for x in 0..result.width() {
            let mut p = Rgba(result.get_pixel(x, y).0);
            if p[3] == 0 {
                output.set_pixel(x, y, crate::precision::Rgba16(p.0))?;
                continue;
            }
            let source = ctx.rgb(&Rgba(sample.get_pixel(x, y).0));
            let weight = point_colors_and_range(source, s).1;
            let rgb = ctx.rgb(&p);
            let gray = ctx.luma(rgb);
            ctx.write(&mut p, rgb.map(|v| gray + (v - gray) * weight));
            output.set_pixel(x, y, crate::precision::Rgba16(p.0))?;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_blur_matches_independent_square_oracle_at_borders_and_transparency() {
        let image = Rgba16Image::from_fn(7, 5, |x, y| {
            Rgba([
                1000,
                2000,
                3000,
                [0, 1, 900, 32768, 65535][((x + 2 * y) % 5) as usize],
            ])
        });
        let values: Vec<f32> = (0..35).map(|i| ((i * 29 + 3) % 37) as f32 / 37.).collect();
        let cancel = AtomicBool::new(false);
        for radius in [1, 2, 4] {
            let result = blur(&values, &image, radius, &cancel).unwrap();
            for y in 0..5usize {
                for x in 0..7usize {
                    let mut sum = 0.;
                    let mut coverage = 0.;
                    for dy in -(radius as isize)..=radius as isize {
                        for dx in -(radius as isize)..=radius as isize {
                            let sx = (x as isize + dx).clamp(0, 6) as usize;
                            let sy = (y as isize + dy).clamp(0, 4) as usize;
                            let a = f64::from(image.get_pixel(sx as u32, sy as u32)[3]) / 65535.;
                            sum += f64::from(values[sy * 7 + sx]) * a;
                            coverage += a;
                        }
                    }
                    let expected = if coverage > 0. {
                        sum / coverage
                    } else {
                        f64::from(values[y * 7 + x])
                    };
                    assert!(
                        (f64::from(result[y * 7 + x]) - expected).abs() < 0.000001,
                        "radius {radius} at{x},{y}"
                    );
                }
            }
        }
    }
    #[test]
    fn native_bilinear_uses_real_coverage_and_retains_hidden_colour_only_without_coverage() {
        let mut image = Rgba16Image::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([65535, 0, 0, 65535])
            } else {
                Rgba([0, 0, 65535, 0])
            }
        });
        assert_eq!(bilinear(&image, 0.5, 0.).0, [65535, 0, 0, 32768]);
        image.get_pixel_mut(0, 0)[3] = 0;
        assert_eq!(bilinear(&image, 0.5, 0.).0, [32768, 0, 32768, 0]);
        assert_eq!(bilinear(&image, 0., 0.), *image.get_pixel(0, 0));
        assert_eq!(bilinear(&image, f64::NAN, 0.).0, [0; 4]);
        assert_eq!(bilinear(&image, -2., 0.).0, [0; 4]);
    }
    #[test]
    fn native_geometry_integer_translation_agrees_with_coordinate_oracle() {
        let image = Rgba16Image::from_fn(10, 10, |x, y| {
            Rgba([
                1001 + x as u16 * 79,
                17003 + y as u16 * 83,
                30007,
                [0, 1, 65535][((x + y) % 3) as usize],
            ])
        });
        // offset_x=100 translates the destination quadrilateral by 15% of
        // width: with width20 that is exactly three pixels.
        let image = Rgba16Image::from_fn(20, 10, |x, y| *image.get_pixel(x % 10, y));
        let settings = GeometrySettings {
            offset_x: 100.,
            ..Default::default()
        };
        let output = geometry(&image, &settings, &AtomicBool::new(false)).unwrap();
        for y in 0..10 {
            for x in 3..20 {
                assert_eq!(output.get_pixel(x, y), image.get_pixel(x - 3, y));
            }
        }
    }
    #[test]
    fn native_scalar_constant_field_does_not_dark_fringe_against_transparent_colours() {
        let image = Rgba16Image::from_fn(13, 9, |x, _| {
            if x < 6 {
                Rgba([24001, 24001, 24001, 65535])
            } else {
                Rgba([65535, 0, 65535, 0])
            }
        });
        let source = TiledImage16::from_rgba16(&image).unwrap();
        let settings = Settings {
            clarity: 80.,
            texture: 60.,
            ..Settings::for_native_edit()
        };
        let output = apply(&source, &settings, &AtomicBool::new(false)).unwrap();
        assert_eq!(output.to_rgba16(), image);
    }
}

#[cfg(test)]
mod alpha_sampling_tests {
    use super::*;
    #[test]
    fn native_ca_samples_associated_channels_instead_of_invisible_straight_colour() {
        let mut image = Rgba16Image::from_pixel(40, 4, Rgba([0, 22001, 0, 65535]));
        // At x1,y0 the radial shift rounds the red lookup to x0. A single word
        // of source coverage cannot become full red at the opaque recipient.
        image.put_pixel(0, 0, Rgba([65535, 22001, 0, 1]));
        let settings = OpticsSettings {
            remove_chromatic_aberration: true,
            ..Default::default()
        };
        optics(
            &mut image,
            &settings,
            Context {
                space: WorkingSpace::Srgb,
            },
            &AtomicBool::new(false),
            false,
        )
        .unwrap();
        assert_eq!(image.get_pixel(1, 0)[0], 1);
        assert_eq!(image.get_pixel(1, 0)[3], 65535);
        assert_eq!(image.get_pixel(0, 0)[3], 1);
    }
}

#[cfg(test)]
mod create_context_admission_tests {
    use super::*;
    #[test]
    fn retained_create_context_is_added_without_replacing_the_active_editor_budget() {
        let document = crate::model::Document::new(8, 6);
        let base = ui_admission(&document, 0, (8, 6), true, false, 0).unwrap();
        let extra =
            ui_admission_with_context(&document, 0, (8, 6), true, false, 0, 123456).unwrap();
        assert_eq!(extra.contextual_bytes, 123456);
        assert_eq!(extra.existing_document_bytes, base.existing_document_bytes);
        assert_eq!(extra.estimated_bytes - base.estimated_bytes, 123456);
        assert!(ui_admission_with_context(&document, 0, (8, 6), true, false, 0, u64::MAX).is_err());
    }
}

#[cfg(test)]
mod prepared_point_pipeline_tests {
    use super::*;
    #[test]
    fn neutral_point_groups_keep_exact_words_and_sampler_stages_for_below_threshold_glow() {
        let pixels = Rgba16Image::from_fn(13, 9, |x, y| {
            Rgba([
                1001 + x as u16 * 137,
                12003 + y as u16 * 211,
                23007 + (x + y) as u16 * 173,
                [0, 1, 2, 257, 32768, 65534, 65535][((x + y) % 7) as usize],
            ])
        });
        // Active spatial controls force the native pipeline, but all source
        // levels remain below even the default glow threshold. The prepared
        // neutral point stages must not introduce a transfer/HSL/curve drift.
        let settings = Settings {
            glow: 46.,
            ..Settings::for_native_edit()
        };
        for space in [
            WorkingSpace::Srgb,
            WorkingSpace::LinearSrgb,
            WorkingSpace::DisplayP3,
        ] {
            let source = TiledImage16::from_rgba16_in(&pixels, space).unwrap();
            for stage in [
                SampleStage::WhiteBalance,
                SampleStage::Curve,
                SampleStage::Mixer,
                SampleStage::PointColor,
                SampleStage::Optics,
            ] {
                let (result, sample) =
                    apply_with_sample(&source, &settings, &AtomicBool::new(false), stage).unwrap();
                assert_eq!(
                    result.to_rgba16(),
                    pixels,
                    "space {space:?}, stage {stage:?}"
                );
                assert_eq!(
                    sample.to_rgba16(),
                    pixels,
                    "space {space:?}, stage {stage:?}"
                );
            }
        }
    }
}
