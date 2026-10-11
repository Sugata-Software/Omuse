//! Native-pixel Blur, Smudge and Liquify, with bounded, fallible working storage.
//! Pointer spacing and circular footprints are measured in canvas pixels. The
//! source is never projected to a canvas raster: transforms map dabs to native
//! pixels, and Liquify samples the untouched source once at the end of a stroke.
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_WORKING_BYTES: usize = 256 * 1024 * 1024;
const MAX_WORK: u64 = 200_000_000;

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

/// Sampling outside a native raster is explicit. Masks supply their saved
/// outside coverage; compatibility callers retain the former clamped boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMode {
    Clamp,
    Constant([u8; 4]),
}

#[derive(Clone, Copy, Debug)]
pub struct NativeOptions {
    /// Layer to canvas: [a, b, c, d, tx, ty], x'=a*x+c*y+tx, y'=b*x+d*y+ty.
    /// Rotation, anisotropic scaling and flips are supported; shear is rejected.
    pub transform: [f32; 6],
    /// Gaussian sigma in canvas pixels, independent of the brush footprint.
    pub blur_radius: f32,
    pub edge: EdgeMode,
    /// Other stroke-owned buffers retained by a caller (points, mask copy,
    /// selection snapshot). Included in the same 256 MiB working-memory cap.
    pub reserved_bytes: usize,
}
impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            transform: [1., 0., 0., 1., 0., 0.],
            blur_radius: 2.,
            edge: EdgeMode::Clamp,
            reserved_bytes: 0,
        }
    }
}

/// Compatibility entry point: retain the previous diameter-derived blur sigma.
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
    apply_native_cancellable(
        source,
        points,
        diameter,
        hardness,
        strength,
        mode,
        NativeOptions {
            blur_radius: (diameter / 10.).clamp(1.5, 30.),
            ..Default::default()
        },
        cancelled,
    )
}

/// Input/output are native, straight-alpha RGBA. Selection is deliberately a
/// caller-side write constraint: unselected source pixels remain sampleable.
/// Every large working allocation, including image copies and blur passes, is
/// fallible; cancellation/failure never changes the borrowed source.
pub fn apply_native_cancellable(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    mode: RetouchMode,
    options: NativeOptions,
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
        options.blur_radius.is_finite() && (0.25..=128.).contains(&options.blur_radius),
        "Blur radius must be 0.25-128 canvas pixels"
    );
    ensure!(
        points.len() <= 100_000
            && points.iter().all(|p| p.x.is_finite()
                && p.y.is_finite()
                && p.x.abs() <= 1_000_000.
                && p.y.abs() <= 1_000_000.),
        "invalid stroke points"
    );
    ensure!(
        u64::from(source.width()) * u64::from(source.height()) <= 16_777_216,
        "Retouch supports images up to 16 million pixels"
    );
    let space = Space::new(options.transform)?;
    let mut memory = Memory {
        bytes: options.reserved_bytes,
        work: 0,
    };
    // Account for the final native result before any mode-specific scratch.
    memory.admit(source.as_raw().len())?;
    if points.is_empty() || (points.len() == 1 && mode != RetouchMode::Blur) {
        return copy_image(source);
    }
    let spacing = match mode {
        RetouchMode::Blur => (diameter * 0.025).max(0.25),
        RetouchMode::Smudge => (diameter * 0.08).max(1.),
        RetouchMode::Liquify => (diameter * 0.025).max(1.),
    };
    let distance: f64 = points
        .windows(2)
        .map(|p| {
            (f64::from(p[1].x) - f64::from(p[0].x)).hypot(f64::from(p[1].y) - f64::from(p[0].y))
        })
        .sum();
    let dabs = (distance / f64::from(spacing)).ceil() as u64 + 2;
    let radii = space.radii(diameter / 2.);
    ensure!(
        radii.iter().all(|r| r.is_finite() && *r <= 1_000_000.),
        "Brush footprint is too large at this layer scale"
    );
    let side = [
        (radii[0] * 2.).ceil() as u64 + 3,
        (radii[1] * 2.).ceil() as u64 + 3,
    ];
    let samples = if mode == RetouchMode::Smudge {
        side[0].saturating_mul(side[1]).saturating_mul(2)
    } else {
        side[0]
            .min(u64::from(source.width()))
            .saturating_mul(side[1].min(u64::from(source.height())))
    };
    memory.admit_work(dabs.saturating_mul(samples))?;
    let Some(area) = Area::for_points(source, points, space, radii) else {
        return copy_image(source);
    };
    let result = match mode {
        RetouchMode::Blur => blur_stroke(
            source,
            points,
            diameter,
            hardness,
            strength,
            spacing,
            area,
            space,
            options,
            &mut memory,
            cancelled,
        ),
        RetouchMode::Smudge => smudge_stroke(
            source,
            points,
            diameter,
            hardness,
            strength,
            spacing,
            space,
            options.edge,
            &mut memory,
            cancelled,
        ),
        RetouchMode::Liquify => liquify_stroke(
            source,
            points,
            diameter,
            hardness,
            strength,
            spacing,
            area,
            space,
            options.edge,
            &mut memory,
            cancelled,
        ),
    }?;
    check_cancelled(cancelled)?;
    Ok(result)
}

pub(crate) fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    #[cfg(test)]
    let injected = failpoint::cancelled();
    #[cfg(not(test))]
    let injected = false;
    ensure!(
        !cancelled.load(Ordering::Relaxed) && !injected,
        "retouch stroke cancelled"
    );
    Ok(())
}

struct Memory {
    bytes: usize,
    work: u64,
}
impl Memory {
    fn admit_work(&mut self, work: u64) -> Result<()> {
        self.work = self.work.saturating_add(work);
        ensure!(
            self.work <= MAX_WORK,
            "Stroke or Blur radius is too large at this layer scale; use a smaller brush, radius or shorter strokes"
        );
        Ok(())
    }
    fn admit(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .context("Retouch working-memory size overflow")?;
        ensure!(
            self.bytes <= MAX_WORKING_BYTES,
            "Retouch needs more than 256 MiB of working memory; use a smaller brush, Blur radius or stroke"
        );
        Ok(())
    }
    fn filled<T: Copy>(&mut self, len: usize, value: T, label: &'static str) -> Result<Vec<T>> {
        self.admit(
            len.checked_mul(std::mem::size_of::<T>())
                .context("Retouch allocation size overflow")?,
        )?;
        filled(len, value, label)
    }
}
fn filled<T: Copy>(len: usize, value: T, label: &'static str) -> Result<Vec<T>> {
    let mut out = Vec::new();
    reserve(&mut out, len, label)?;
    out.resize(len, value);
    Ok(out)
}
pub(crate) fn reserve<T>(out: &mut Vec<T>, additional: usize, label: &'static str) -> Result<()> {
    #[cfg(test)]
    failpoint::allocation()?;
    out.try_reserve_exact(additional).with_context(|| {
        format!("Not enough memory for retouch {label}; try a smaller brush or stroke")
    })
}
pub(crate) fn copy_image(source: &RgbaImage) -> Result<RgbaImage> {
    let mut bytes = Vec::new();
    reserve(&mut bytes, source.as_raw().len(), "source copy")?;
    bytes.extend_from_slice(source.as_raw());
    RgbaImage::from_raw(source.width(), source.height(), bytes)
        .context("Invalid retouch image storage")
}

#[derive(Clone, Copy)]
struct Space {
    forward: [f32; 6],
    inverse: [f32; 4],
    scale: [f32; 2],
}
impl Space {
    fn new(m: [f32; 6]) -> Result<Self> {
        ensure!(
            m.iter().all(|v| v.is_finite()),
            "Invalid retouch layer transform"
        );
        let det = m[0] * m[3] - m[1] * m[2];
        let scale = [m[0].hypot(m[1]), m[2].hypot(m[3])];
        ensure!(
            det.is_finite() && det.abs() >= 1e-9 && scale.iter().all(|s| *s > 0.),
            "Invalid retouch layer transform"
        );
        ensure!(
            (m[0] * m[2] + m[1] * m[3]).abs() <= scale[0] * scale[1] * 1e-5,
            "Retouch does not support sheared layers"
        );
        ensure!(
            [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det]
                .iter()
                .all(|v| v.is_finite()),
            "Invalid retouch layer transform"
        );
        Ok(Self {
            forward: m,
            inverse: [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det],
            scale,
        })
    }
    fn local(self, p: StrokePoint) -> StrokePoint {
        let (x, y) = (p.x - self.forward[4], p.y - self.forward[5]);
        StrokePoint {
            x: self.inverse[0] * x + self.inverse[2] * y,
            y: self.inverse[1] * x + self.inverse[3] * y,
        }
    }
    fn radii(self, radius: f32) -> [f32; 2] {
        [
            radius * self.inverse[0].hypot(self.inverse[2]),
            radius * self.inverse[1].hypot(self.inverse[3]),
        ]
    }
    fn distance(self, x: f32, y: f32) -> f32 {
        (self.forward[0] * x + self.forward[2] * y).hypot(self.forward[1] * x + self.forward[3] * y)
    }
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
    fn for_points(
        image: &RgbaImage,
        points: &[StrokePoint],
        space: Space,
        radii: [f32; 2],
    ) -> Option<Self> {
        let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for p in points {
            let p = space.local(*p);
            min[0] = min[0].min(p.x);
            min[1] = min[1].min(p.y);
            max[0] = max[0].max(p.x);
            max[1] = max[1].max(p.y);
        }
        let area = Self {
            left: (min[0] - radii[0]).floor().clamp(0., image.width() as f32) as u32,
            top: (min[1] - radii[1]).floor().clamp(0., image.height() as f32) as u32,
            right: (max[0] + radii[0]).ceil().clamp(0., image.width() as f32) as u32,
            bottom: (max[1] + radii[1]).ceil().clamp(0., image.height() as f32) as u32,
        };
        (area.left < area.right && area.top < area.bottom).then_some(area)
    }
    fn expand(self, margin: [u32; 2], image: &RgbaImage) -> Self {
        Self {
            left: self.left.saturating_sub(margin[0]),
            top: self.top.saturating_sub(margin[1]),
            right: self.right.saturating_add(margin[0]).min(image.width()),
            bottom: self.bottom.saturating_add(margin[1]).min(image.height()),
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

/// Carry residual distance between pointer events, including a fractional final dab.
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
    let a = f32::from(pixel[3]);
    [
        f32::from(pixel[0]) * a / 255.,
        f32::from(pixel[1]) * a / 255.,
        f32::from(pixel[2]) * a / 255.,
        a,
    ]
}
fn straight(pixel: [f32; 4]) -> Rgba<u8> {
    let a = pixel[3].clamp(0., 255.);
    let rounded = a.round() as u8;
    if rounded == 0 {
        return Rgba([0; 4]);
    }
    Rgba([
        (pixel[0] * 255. / a).round().clamp(0., 255.) as u8,
        (pixel[1] * 255. / a).round().clamp(0., 255.) as u8,
        (pixel[2] * 255. / a).round().clamp(0., 255.) as u8,
        rounded,
    ])
}
fn mix<const N: usize>(a: [f32; N], b: [f32; N], amount: f32) -> [f32; N] {
    std::array::from_fn(|c| a[c] + (b[c] - a[c]) * amount)
}
fn at(image: &RgbaImage, x: i64, y: i64, edge: EdgeMode) -> Rgba<u8> {
    if x >= 0 && y >= 0 && x < i64::from(image.width()) && y < i64::from(image.height()) {
        return *image.get_pixel(x as u32, y as u32);
    }
    match edge {
        EdgeMode::Constant(p) => Rgba(p),
        EdgeMode::Clamp => *image.get_pixel(
            x.clamp(0, i64::from(image.width()) - 1) as u32,
            y.clamp(0, i64::from(image.height()) - 1) as u32,
        ),
    }
}
fn sample_image(image: &RgbaImage, x: f32, y: f32, edge: EdgeMode) -> [f32; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (left, top) = (x.floor() as i64, y.floor() as i64);
    mix(
        mix(
            premultiplied(at(image, left, top, edge)),
            premultiplied(at(image, left + 1, top, edge)),
            x - left as f32,
        ),
        mix(
            premultiplied(at(image, left, top + 1, edge)),
            premultiplied(at(image, left + 1, top + 1, edge)),
            x - left as f32,
        ),
        y - top as f32,
    )
}

fn box_radii(sigma: f32) -> [u32; 3] {
    let ideal = (4. * sigma * sigma + 1.).sqrt();
    let mut lower = ideal.floor() as u32;
    if lower % 2 == 0 {
        lower = lower.saturating_sub(1);
    }
    lower = lower.max(1);
    let lo = lower as f32;
    let count = ((12. * sigma * sigma - 3. * lo * lo - 12. * lo - 9.) / (-4. * lo - 4.))
        .round()
        .clamp(0., 3.) as usize;
    std::array::from_fn(|i| (if i < count { lower } else { lower + 2 }) / 2)
}
/// Same three-box reference approximation as the ordinary Gaussian filter,
/// but independent axes and caller-owned, fallibly allocated storage. Each
/// pass is allocation-free. A complete halo also handles mask outside ground.
fn box_pass(
    input: &RgbaImage,
    output: &mut RgbaImage,
    radius: u32,
    horizontal: bool,
    cancelled: &AtomicBool,
) -> Result<()> {
    if radius == 0 {
        let pixels: &mut [u8] = output.as_mut();
        pixels.copy_from_slice(input.as_raw());
        return Ok(());
    }
    let (len, lines) = if horizontal {
        (input.width(), input.height())
    } else {
        (input.height(), input.width())
    };
    let get = |i: i64, line: u32| -> Rgba<u8> {
        let i = i.clamp(0, i64::from(len) - 1) as u32;
        *if horizontal {
            input.get_pixel(i, line)
        } else {
            input.get_pixel(line, i)
        }
    };
    let count = u64::from(radius) * 2 + 1;
    for line in 0..lines {
        check_cancelled(cancelled)?;
        let mut sum = [0u64; 4];
        let accumulate = |sum: &mut [u64; 4], p: Rgba<u8>, add: bool| {
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
                check_cancelled(cancelled)?;
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
fn blur_stroke(
    source: &RgbaImage,
    points: &[StrokePoint],
    diameter: f32,
    hardness: f32,
    strength: f32,
    spacing: f32,
    area: Area,
    space: Space,
    options: NativeOptions,
    memory: &mut Memory,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let rx = box_radii(options.blur_radius / space.scale[0]);
    let ry = box_radii(options.blur_radius / space.scale[1]);
    let halo = [rx.iter().sum::<u32>(), ry.iter().sum::<u32>()];
    let mut left = i64::from(area.left) - i64::from(halo[0]);
    let mut top = i64::from(area.top) - i64::from(halo[1]);
    let mut right = i64::from(area.right) + i64::from(halo[0]);
    let mut bottom = i64::from(area.bottom) + i64::from(halo[1]);
    if options.edge == EdgeMode::Clamp {
        left = left.max(0);
        top = top.max(0);
        right = right.min(i64::from(source.width()));
        bottom = bottom.min(i64::from(source.height()));
    }
    let (width, height) = (
        u32::try_from(right - left).context("Blur patch too wide")?,
        u32::try_from(bottom - top).context("Blur patch too high")?,
    );
    let pixels = u64::from(width) * u64::from(height);
    let work = pixels.saturating_mul(6).saturating_add(
        u64::from(halo[0]) * 2 * u64::from(height) + u64::from(halo[1]) * 2 * u64::from(width),
    );
    memory.admit_work(work)?;
    let bytes = usize::try_from(pixels.checked_mul(4).context("Blur patch size overflow")?)
        .context("Blur patch too large")?;
    // Admit all buffers before attempting the first allocation.
    memory.admit(bytes.checked_mul(2).context("Blur scratch size overflow")?)?;
    memory.admit(area.len() * std::mem::size_of::<f32>())?;
    let mut softened =
        RgbaImage::from_raw(width, height, filled(bytes, 0u8, "blur source patch")?).unwrap();
    for y in 0..height {
        check_cancelled(cancelled)?;
        for x in 0..width {
            softened.put_pixel(
                x,
                y,
                at(
                    source,
                    left + i64::from(x),
                    top + i64::from(y),
                    options.edge,
                ),
            );
        }
    }
    let mut scratch =
        RgbaImage::from_raw(width, height, filled(bytes, 0u8, "blur scratch")?).unwrap();
    for pass in 0..3 {
        box_pass(&softened, &mut scratch, rx[pass], true, cancelled)?;
        box_pass(&scratch, &mut softened, ry[pass], false, cancelled)?;
    }
    let mut coverage = filled(area.len(), 0f32, "blur coverage")?;
    let radii = space.radii(diameter / 2.);
    let mut dab = |center: StrokePoint| -> Result<()> {
        check_cancelled(cancelled)?;
        let Some(bounds) = Area::for_points(source, &[center], space, radii) else {
            return Ok(());
        };
        let center = space.local(center);
        for y in bounds.top..bounds.bottom {
            check_cancelled(cancelled)?;
            for x in bounds.left..bounds.right {
                let w = weight(
                    space.distance(x as f32 + 0.5 - center.x, y as f32 + 0.5 - center.y),
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
    let mut output = copy_image(source)?;
    for y in area.top..area.bottom {
        check_cancelled(cancelled)?;
        for x in area.left..area.right {
            let amount = coverage[area.index(x, y)] * strength;
            if amount == 0. {
                continue;
            }
            let original = *source.get_pixel(x, y);
            let mut blurred =
                *softened.get_pixel((i64::from(x) - left) as u32, (i64::from(y) - top) as u32);
            // Blur colours, preserving the source's alpha as the existing tool does.
            blurred[3] = original[3];
            output.put_pixel(
                x,
                y,
                straight(mix(premultiplied(original), premultiplied(blurred), amount)),
            );
        }
    }
    Ok(output)
}
fn pick_up(
    image: &RgbaImage,
    center: StrokePoint,
    radius: [usize; 2],
    carried: &mut [[f32; 4]],
    edge: EdgeMode,
    cancelled: &AtomicBool,
) -> Result<()> {
    let side = [radius[0] * 2 + 1, radius[1] * 2 + 1];
    for y in 0..side[1] {
        check_cancelled(cancelled)?;
        for x in 0..side[0] {
            carried[y * side[0] + x] = sample_image(
                image,
                center.x + x as f32 - radius[0] as f32,
                center.y + y as f32 - radius[1] as f32,
                edge,
            );
        }
    }
    Ok(())
}
fn sample_carried(carried: &[[f32; 4]], side: [usize; 2], x: f32, y: f32) -> [f32; 4] {
    let (x, y) = (
        x.clamp(0., (side[0] - 1) as f32),
        y.clamp(0., (side[1] - 1) as f32),
    );
    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (right, bottom) = ((left + 1).min(side[0] - 1), (top + 1).min(side[1] - 1));
    mix(
        mix(
            carried[top * side[0] + left],
            carried[top * side[0] + right],
            x - left as f32,
        ),
        mix(
            carried[bottom * side[0] + left],
            carried[bottom * side[0] + right],
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
    space: Space,
    edge: EdgeMode,
    memory: &mut Memory,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let radii = space.radii(diameter / 2.);
    let radius = [radii[0].ceil() as usize, radii[1].ceil() as usize];
    let side = [radius[0] * 2 + 1, radius[1] * 2 + 1];
    let mut carried = memory.filled(
        side[0]
            .checked_mul(side[1])
            .context("Smudge carry size overflow")?,
        [0.; 4],
        "Smudge carry",
    )?;
    pick_up(
        source,
        space.local(points[0]),
        radius,
        &mut carried,
        edge,
        cancelled,
    )?;
    let mut output = copy_image(source)?;
    walk_path(points, spacing, |from, to| {
        check_cancelled(cancelled)?;
        let amount = strength * ((to.x - from.x).hypot(to.y - from.y) / spacing).min(1.);
        let center = space.local(to);
        if let Some(bounds) = Area::for_points(source, &[to], space, radii) {
            for y in bounds.top..bounds.bottom {
                check_cancelled(cancelled)?;
                for x in bounds.left..bounds.right {
                    let (dx, dy) = (x as f32 + 0.5 - center.x, y as f32 + 0.5 - center.y);
                    let w = weight(space.distance(dx, dy), diameter / 2., hardness) * amount;
                    if w == 0. {
                        continue;
                    }
                    let ink = sample_carried(
                        &carried,
                        side,
                        dx + radius[0] as f32,
                        dy + radius[1] as f32,
                    );
                    let under = premultiplied(*output.get_pixel(x, y));
                    output.put_pixel(x, y, straight(mix(under, ink, w)));
                }
            }
        }
        pick_up(&output, center, radius, &mut carried, edge, cancelled)
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
    space: Space,
    edge: EdgeMode,
    memory: &mut Memory,
    cancelled: &AtomicBool,
) -> Result<RgbaImage> {
    let radii = space.radii(diameter / 2.);
    // A fixed admitted scratch rectangle avoids reallocating during a stroke.
    let movement = space.radii(spacing * strength);
    let scratch_width = area
        .width()
        .min((radii[0] * 2.).ceil() as usize + 4 + (movement[0].ceil() as usize + 1) * 2);
    let scratch_height = area
        .height()
        .min((radii[1] * 2.).ceil() as usize + 4 + (movement[1].ceil() as usize + 1) * 2);
    memory.admit(area.len() * std::mem::size_of::<[f32; 2]>())?;
    memory.admit(scratch_width * scratch_height * std::mem::size_of::<[f32; 2]>())?;
    let mut offsets = filled(area.len(), [0f32; 2], "Liquify offsets")?;
    let mut scratch = filled(scratch_width * scratch_height, [0f32; 2], "Liquify scratch")?;
    walk_path(points, spacing, |from, to| {
        check_cancelled(cancelled)?;
        let Some(bounds) = Area::for_points(source, &[to], space, radii) else {
            return Ok(());
        };
        let (from, to) = (space.local(from), space.local(to));
        let movement = [(to.x - from.x) * strength, (to.y - from.y) * strength];
        let sampled = bounds
            .expand(
                [
                    movement[0].abs().ceil() as u32 + 1,
                    movement[1].abs().ceil() as u32 + 1,
                ],
                source,
            )
            .intersect(area);
        ensure!(
            sampled.len() <= scratch.len(),
            "Retouch displacement exceeds admitted scratch bounds"
        );
        for y in sampled.top..sampled.bottom {
            check_cancelled(cancelled)?;
            let start = area.index(sampled.left, y);
            let target = (y - sampled.top) as usize * sampled.width();
            scratch[target..target + sampled.width()]
                .copy_from_slice(&offsets[start..start + sampled.width()]);
        }
        for y in bounds.top..bounds.bottom {
            check_cancelled(cancelled)?;
            for x in bounds.left..bounds.right {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w = weight(
                    space.distance(px - to.x, py - to.y),
                    diameter / 2.,
                    hardness,
                );
                if w == 0. {
                    continue;
                }
                let step = [movement[0] * w, movement[1] * w];
                let (mut sx, mut sy) = (px - step[0], py - step[1]);
                if edge == EdgeMode::Clamp {
                    sx = sx.clamp(0.5, source.width() as f32 - 0.5);
                    sy = sy.clamp(0.5, source.height() as f32 - 0.5);
                }
                let old = sample_offset(&scratch, sampled, sx, sy);
                offsets[area.index(x, y)] = [old[0] - step[0], old[1] - step[1]];
            }
        }
        Ok(())
    })?;
    let mut output = copy_image(source)?;
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
                        edge,
                    )),
                );
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) mod failpoint {
    use std::cell::Cell;
    thread_local! {static ALLOCATIONS:Cell<Option<usize>>=const{Cell::new(None)};static CANCEL:Cell<Option<usize>>=const{Cell::new(None)};}
    pub(super) fn allocation() -> anyhow::Result<()> {
        let fail = ALLOCATIONS.with(|c| match c.get() {
            Some(0) => true,
            Some(n) => {
                c.set(Some(n - 1));
                false
            }
            None => false,
        });
        anyhow::ensure!(
            !fail,
            "Not enough memory for retouch (injected allocation failure)"
        );
        Ok(())
    }
    pub(super) fn cancelled() -> bool {
        CANCEL.with(|c| match c.get() {
            Some(0) => true,
            Some(n) => {
                c.set(Some(n - 1));
                false
            }
            None => false,
        })
    }
    pub(crate) struct Guard;
    impl Guard {
        pub(crate) fn allocation_after(n: usize) -> Self {
            ALLOCATIONS.with(|c| c.set(Some(n)));
            Self
        }
        pub(crate) fn cancel_after(n: usize) -> Self {
            CANCEL.with(|c| c.set(Some(n)));
            Self
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ALLOCATIONS.with(|c| c.set(None));
            CANCEL.with(|c| c.set(None));
        }
    }
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
    fn native(
        source: &RgbaImage,
        points: &[StrokePoint],
        diameter: f32,
        mode: RetouchMode,
        options: NativeOptions,
    ) -> RgbaImage {
        apply_native_cancellable(
            source,
            points,
            diameter,
            0.98,
            1.,
            mode,
            options,
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn rotated_anisotropic_and_flipped_liquify_retains_native_stripes() {
        let source = RgbaImage::from_fn(80, 24, |x, _| {
            Rgba([if x % 2 == 0 { 0 } else { 220 }, 31, 87, 128])
        });
        for flip in [1., -1.] {
            // A four-native-pixel move is one canvas pixel on the quarter-size
            // horizontal source axis, even after rotation and reflection.
            let transform = [0., 0.25 * flip, -0.75, 0., 40., 25.];
            let path = [30.5, 34.5].map(|x| StrokePoint {
                x: 40. - 0.75 * 12.5,
                y: 25. + 0.25 * flip * x,
            });
            let result = apply_native_cancellable(
                &source,
                &path,
                4.,
                0.98,
                0.25,
                RetouchMode::Liquify,
                NativeOptions {
                    transform,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            )
            .unwrap();
            for x in 31..38 {
                assert_eq!(
                    result.get_pixel(x, 12),
                    source.get_pixel(x - 1, 12),
                    "flip {flip}, native stripe {x}"
                );
            }
            assert_eq!(result.get_pixel(70, 12), source.get_pixel(70, 12));
            // On the compressed y footprint, this nearby source row is outside.
            assert_eq!(result.get_pixel(34, 16), source.get_pixel(34, 16));
        }
    }

    /// Deliberately slow direct sums, independently of the production rolling
    /// sums, check the anisotropic blur kernel and its integer rounding.
    fn direct_box(input: &RgbaImage, radius: i64, horizontal: bool) -> RgbaImage {
        RgbaImage::from_fn(input.width(), input.height(), |x, y| {
            let mut sums = [0u64; 4];
            for delta in -radius..=radius {
                let px = (i64::from(x) + if horizontal { delta } else { 0 })
                    .clamp(0, i64::from(input.width()) - 1) as u32;
                let py = (i64::from(y) + if horizontal { 0 } else { delta })
                    .clamp(0, i64::from(input.height()) - 1) as u32;
                let p = input.get_pixel(px, py);
                for c in 0..3 {
                    sums[c] += u64::from(p[c]) * u64::from(p[3]);
                }
                sums[3] += u64::from(p[3]);
            }
            if sums[3] == 0 {
                return Rgba([0; 4]);
            }
            let count = (radius * 2 + 1) as u64;
            Rgba([
                ((sums[0] + sums[3] / 2) / sums[3]) as u8,
                ((sums[1] + sums[3] / 2) / sums[3]) as u8,
                ((sums[2] + sums[3] / 2) / sums[3]) as u8,
                ((sums[3] + count / 2) / count) as u8,
            ])
        })
    }

    #[test]
    fn native_blur_uses_independent_axis_radii_and_preserves_source_alpha() {
        let source = RgbaImage::from_fn(49, 25, |x, y| {
            Rgba([
                (x * 41 + y * 13) as u8,
                (y * 47) as u8,
                (x * 19) as u8,
                if x % 3 == 0 { 97 } else { 211 },
            ])
        });
        let mut expected = source.clone();
        // sigma_x=8 and sigma_y=2: independently calculated box radii.
        for (rx, ry) in [(7, 1), (7, 1), (8, 2)] {
            expected = direct_box(&direct_box(&expected, rx, true), ry, false);
        }
        let transform = [0., 0.25, -1., 0., 25., 0.];
        let point = StrokePoint {
            x: 25. - 12.5,
            y: 24.5 * 0.25,
        };
        let result = native(
            &source,
            &[point],
            8.,
            RetouchMode::Blur,
            NativeOptions {
                transform,
                blur_radius: 2.,
                ..Default::default()
            },
        );
        for x in 20..29 {
            assert_eq!(
                &result.get_pixel(x, 12).0[..3],
                &expected.get_pixel(x, 12).0[..3]
            );
        }
        for (a, b) in source.pixels().zip(result.pixels()) {
            assert_eq!(a[3], b[3]);
        }
        assert_eq!(result.get_pixel(24, 20), source.get_pixel(24, 20));
    }

    #[test]
    fn blur_radius_changes_filtering_without_changing_the_brush_footprint() {
        let source = RgbaImage::from_fn(41, 21, |x, _| {
            Rgba([if x < 20 { 0 } else { 240 }, 0, 0, 255])
        });
        let point = StrokePoint { x: 18.5, y: 10.5 };
        let small = native(
            &source,
            &[point],
            12.,
            RetouchMode::Blur,
            NativeOptions {
                blur_radius: 1.,
                ..Default::default()
            },
        );
        let large = native(
            &source,
            &[point],
            12.,
            RetouchMode::Blur,
            NativeOptions {
                blur_radius: 6.,
                ..Default::default()
            },
        );
        let wide = native(
            &source,
            &[point],
            24.,
            RetouchMode::Blur,
            NativeOptions {
                blur_radius: 6.,
                ..Default::default()
            },
        );
        assert!(large.get_pixel(18, 10)[0] > small.get_pixel(18, 10)[0] + 20);
        assert_eq!(large.get_pixel(18, 10), wide.get_pixel(18, 10));
        for y in 0..21 {
            for x in 0..41 {
                if (x as f32 + 0.5 - point.x).hypot(y as f32 + 0.5 - point.y) >= 6. {
                    assert_eq!(large.get_pixel(x, y), source.get_pixel(x, y));
                }
            }
        }
    }

    #[test]
    fn mask_sampling_uses_explicit_opaque_outside_ground_in_every_mode() {
        let source = RgbaImage::from_pixel(9, 9, Rgba([0, 0, 0, 255]));
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let path: &[StrokePoint] = if mode == RetouchMode::Blur {
                &[StrokePoint { x: 0.5, y: 4.5 }]
            } else {
                &[
                    StrokePoint { x: -1.5, y: 4.5 },
                    StrokePoint { x: 0.5, y: 4.5 },
                ]
            };
            let black = native(
                &source,
                path,
                8.,
                mode,
                NativeOptions {
                    edge: EdgeMode::Constant([0, 0, 0, 255]),
                    ..Default::default()
                },
            );
            let white = native(
                &source,
                path,
                8.,
                mode,
                NativeOptions {
                    edge: EdgeMode::Constant([255; 4]),
                    ..Default::default()
                },
            );
            assert_eq!(black, source, "{mode:?} changed black on black ground");
            assert!(
                white.get_pixel(0, 4)[0] > 0,
                "{mode:?} did not sample white ground"
            );
            assert!(
                white
                    .pixels()
                    .all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2])
            );
            if mode != RetouchMode::Blur {
                assert_eq!(white.get_pixel(0, 4)[0], 255);
            }
        }
    }

    #[test]
    fn each_large_allocation_can_fail_without_mutation_and_retry_exactly() {
        let source = RgbaImage::from_fn(32, 24, |x, y| {
            Rgba([(x * 17) as u8, (y * 29) as u8, 51, 127])
        });
        let unchanged = source.clone();
        let path = [
            StrokePoint { x: 9.5, y: 12.5 },
            StrokePoint { x: 17.5, y: 12.5 },
        ];
        for (mode, expected_allocations) in [
            (RetouchMode::Blur, 4),
            (RetouchMode::Smudge, 2),
            (RetouchMode::Liquify, 3),
        ] {
            let expected = native(&source, &path, 8., mode, NativeOptions::default());
            for fail_at in 0..expected_allocations {
                let guard = failpoint::Guard::allocation_after(fail_at);
                let error = apply_native_cancellable(
                    &source,
                    &path,
                    8.,
                    0.98,
                    1.,
                    mode,
                    NativeOptions::default(),
                    &AtomicBool::new(false),
                )
                .unwrap_err();
                assert!(error.to_string().contains("memory"));
                drop(guard);
                assert_eq!(source, unchanged);
                assert_eq!(
                    native(&source, &path, 8., mode, NativeOptions::default()),
                    expected
                );
            }
            let _guard = failpoint::Guard::allocation_after(expected_allocations);
            assert_eq!(
                native(&source, &path, 8., mode, NativeOptions::default()),
                expected
            );
        }
    }

    #[test]
    fn cancellation_after_work_has_started_is_atomic_and_retryable() {
        let source = RgbaImage::from_fn(32, 24, |x, y| {
            Rgba([(x * 17) as u8, (y * 29) as u8, 51, 127])
        });
        let unchanged = source.clone();
        let path = [
            StrokePoint { x: 9.5, y: 12.5 },
            StrokePoint { x: 17.5, y: 12.5 },
        ];
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let expected = native(&source, &path, 8., mode, NativeOptions::default());
            for checkpoint in [3, 20] {
                let guard = failpoint::Guard::cancel_after(checkpoint);
                let error = apply_native_cancellable(
                    &source,
                    &path,
                    8.,
                    0.98,
                    1.,
                    mode,
                    NativeOptions::default(),
                    &AtomicBool::new(false),
                )
                .unwrap_err();
                assert!(error.to_string().contains("cancelled"));
                drop(guard);
                assert_eq!(source, unchanged);
                assert_eq!(
                    native(&source, &path, 8., mode, NativeOptions::default()),
                    expected
                );
            }
        }
    }

    #[test]
    fn memory_and_extreme_transform_admission_are_recoverable() {
        let source = RgbaImage::from_pixel(9, 9, Rgba([10, 20, 30, 255]));
        let path = [
            StrokePoint { x: 4.5, y: 4.5 },
            StrokePoint { x: 5.5, y: 4.5 },
        ];
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let error = apply_native_cancellable(
                &source,
                &path,
                8.,
                0.5,
                1.,
                mode,
                NativeOptions {
                    reserved_bytes: MAX_WORKING_BYTES - 400,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert!(error.to_string().contains("working memory"));
            assert_eq!(
                native(&source, &path, 8., mode, NativeOptions::default()),
                source
            );
        }
        for transform in [
            [1., 0., 0.5, 1., 0., 0.],
            [1e-30, 0., 0., 1e30, 0., 0.],
            [0.; 6],
        ] {
            assert!(
                apply_native_cancellable(
                    &source,
                    &path,
                    8.,
                    0.5,
                    1.,
                    RetouchMode::Smudge,
                    NativeOptions {
                        transform,
                        ..Default::default()
                    },
                    &AtomicBool::new(false)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn path_spacing_keeps_exact_boundaries_corners_and_final_fraction() {
        let path = [
            StrokePoint { x: 0., y: 0. },
            StrokePoint { x: 1., y: 0. },
            StrokePoint { x: 1., y: 0. },
            StrokePoint { x: 1., y: 1. },
            StrokePoint { x: 1.25, y: 1. },
        ];
        let mut output = Vec::new();
        walk_path(&path, 1., |_, to| {
            output.push(to);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            output,
            vec![
                StrokePoint { x: 1., y: 0. },
                StrokePoint { x: 1., y: 1. },
                StrokePoint { x: 1.25, y: 1. }
            ]
        );
    }
}
