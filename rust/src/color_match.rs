//! Reference colour matching with a small, self-contained editable recipe.
//!
//! The per-axis mean/standard-deviation transfer follows Reinhard et al.,
//! https://home.cis.rit.edu/~cnspci/references/dip/color_transfer/reinhard2001.pdf
//! (equations 10–11), adapted to Oklab rather than the paper's l-alpha-beta.
//! Oklab matrices are Björn Ottosson's public-domain 2021 matrices:
//! https://bottosson.github.io/posts/oklab/ . Flat channels use mean shift;
//! variance expansion is capped at 4× and gamut mapping reduces chroma at
//! fixed lightness/hue. This is a global colour grade, not semantic matching.
use crate::precision::{Rgba16, TiledImage16, WorkingSpace};
use anyhow::{Context, Result, ensure};
use image::{ImageDecoder, ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_SAMPLES: u32 = 65_536;
pub const MAX_PIXELS: u64 = 16_777_216;
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;

/// Alpha-weighted population statistics in Oklab. No reference pixels, paths,
/// profiles, or filenames are retained by an editable node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Statistics {
    // f32 persists exactly through the existing JSON reader; accumulation and
    // transfer use f64. Its precision exceeds the retained 16-bit source.
    pub mean: [f32; 3],
    pub deviation: [f32; 3],
    pub samples: u32,
}

impl Statistics {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_SAMPLES).contains(&self.samples),
            "Invalid reference sample count"
        );
        ensure!(
            self.mean[0].is_finite() && (0.0..=1.000001).contains(&self.mean[0]),
            "Invalid reference lightness"
        );
        ensure!(
            self.mean[1..]
                .iter()
                .all(|v| v.is_finite() && (-0.6..=0.6).contains(v)),
            "Invalid reference chroma"
        );
        ensure!(
            self.deviation
                .iter()
                .all(|v| v.is_finite() && (0.0..=0.6).contains(v)),
            "Invalid reference colour deviation"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    /// Version fixes the colour space, sampling rule, and transfer behaviour.
    pub version: u32,
    pub reference: Statistics,
    pub amount: f32,
    pub preserve_lightness: bool,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "Unsupported reference colour match version"
        );
        self.reference.validate()?;
        ensure!(
            self.amount.is_finite() && (0.0..=1.0).contains(&self.amount),
            "Colour match amount must be 0–100%"
        );
        Ok(())
    }
}

pub struct LoadedReference {
    pub statistics: Statistics,
    pub thumbnail: RgbaImage,
    pub dimensions: (u32, u32),
    pub profile_applied: bool,
}

/// Decode one bounded raster reference. Embedded ICC colour and orientation
/// are honoured; an untagged image explicitly uses sRGB. Animated inputs use
/// their first frame. Unsupported/broken profiles fail rather than being ignored.
pub fn load_reference(path: &Path, cancel: &AtomicBool) -> Result<LoadedReference> {
    check(cancel)?;
    let mut file = std::fs::File::open(path).context("Open reference image")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "Choose a raster image file for the reference"
    );
    ensure!(
        metadata.len() <= MAX_FILE_BYTES,
        "Reference file exceeds 128 MiB; export a smaller PNG or JPEG"
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = [0u8; 65_536];
    loop {
        check(cancel)?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() as u64 + count as u64 <= MAX_FILE_BYTES,
            "Reference file exceeds 128 MiB"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
    ensure!(
        reader.format().is_some(),
        "Use PNG, JPEG, TIFF, WebP, BMP, or GIF as the reference"
    );
    let is_tiff = reader.format() == Some(image::ImageFormat::Tiff);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(192 * 1024 * 1024);
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    dimensions(decoder.dimensions())?;
    let mut profile = decoder.icc_profile()?;
    if profile.is_none() && is_tiff {
        profile = crate::color_management::tiff_icc_profile(&bytes)?;
    }
    let orientation = decoder.orientation()?;
    check(cancel)?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    drop(bytes);
    decoded.apply_orientation(orientation);
    check(cancel)?;
    let decoded = decoded.into_rgba16();
    let pixels = if profile.is_some() {
        crate::color_management::to_srgb16(&decoded, profile.as_deref())?
    } else {
        decoded
    };
    check(cancel)?;
    let statistics = statistics(
        pixels.dimensions(),
        |x, y| {
            let p = pixels.get_pixel(x, y).0;
            (
                rgb_to_lab(
                    [p[0], p[1], p[2]].map(|v| f64::from(v) / 65_535.),
                    WorkingSpace::Srgb,
                ),
                f64::from(p[3]) / 65_535.,
            )
        },
        cancel,
    )?;
    let thumbnail =
        image::DynamicImage::ImageRgba16(image::imageops::thumbnail(&pixels, 320, 160)).to_rgba8();
    check(cancel)?;
    Ok(LoadedReference {
        statistics,
        thumbnail,
        dimensions: pixels.dimensions(),
        profile_applied: profile.is_some(),
    })
}

pub fn statistics16(source: &TiledImage16, cancel: &AtomicBool) -> Result<Statistics> {
    statistics(
        source.dimensions(),
        |x, y| {
            let p = source.get_pixel(x, y).0;
            (
                rgb_to_lab(
                    [p[0], p[1], p[2]].map(|v| f64::from(v) / 65_535.),
                    source.working_space(),
                ),
                f64::from(p[3]) / 65_535.,
            )
        },
        cancel,
    )
}

pub fn statistics8(source: &RgbaImage, cancel: &AtomicBool) -> Result<Statistics> {
    statistics(
        source.dimensions(),
        |x, y| {
            let p = source.get_pixel(x, y).0;
            (
                rgb_to_lab(
                    [p[0], p[1], p[2]].map(|v| f64::from(v) / 255.),
                    WorkingSpace::Srgb,
                ),
                f64::from(p[3]) / 255.,
            )
        },
        cancel,
    )
}

fn dimensions((width, height): (u32, u32)) -> Result<()> {
    ensure!(
        crate::model::valid_dimensions(width, height)
            && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Colour matching supports images up to 16 megapixels; resize the reference or layer first"
    );
    Ok(())
}

fn statistics(
    size: (u32, u32),
    pixel: impl Fn(u32, u32) -> ([f64; 3], f64),
    cancel: &AtomicBool,
) -> Result<Statistics> {
    dimensions(size)?;
    check(cancel)?;
    let (width, height) = size;
    // Aspect-correct stratified grid; thin images still use the full budget.
    let nx = ((f64::from(MAX_SAMPLES) * f64::from(width) / f64::from(height))
        .sqrt()
        .ceil() as u32)
        .clamp(1, width.min(MAX_SAMPLES));
    let ny = (MAX_SAMPLES / nx).clamp(1, height);
    let mut mean = [0.; 3];
    let mut squared = [0.; 3];
    let mut weight = 0.;
    let mut samples = 0;
    for gy in 0..ny {
        check(cancel)?;
        let y = ((u64::from(2 * gy + 1) * u64::from(height)) / u64::from(2 * ny)) as u32;
        for gx in 0..nx {
            let x = ((u64::from(2 * gx + 1) * u64::from(width)) / u64::from(2 * nx)) as u32;
            let (lab, alpha) = pixel(x, y);
            if alpha == 0. {
                continue;
            }
            samples += 1;
            let next_weight = weight + alpha;
            for c in 0..3 {
                let delta = lab[c] - mean[c];
                mean[c] += delta * alpha / next_weight;
                squared[c] += alpha * delta * (lab[c] - mean[c]);
            }
            weight = next_weight;
        }
    }
    // Small isolated cut-outs can fall between every grid sample. In that
    // case scan the bounded raster and use up to MAX_SAMPLES visible pixels,
    // rather than incorrectly rejecting content as fully transparent.
    if samples == 0 {
        'visible: for y in 0..height {
            check(cancel)?;
            for x in 0..width {
                let (lab, alpha) = pixel(x, y);
                if alpha == 0. {
                    continue;
                }
                samples += 1;
                let next_weight = weight + alpha;
                for c in 0..3 {
                    let delta = lab[c] - mean[c];
                    mean[c] += delta * alpha / next_weight;
                    squared[c] += alpha * delta * (lab[c] - mean[c]);
                }
                weight = next_weight;
                if samples == MAX_SAMPLES {
                    break 'visible;
                }
            }
        }
    }
    ensure!(
        samples > 0,
        "No visible pixels in the colour sample; use a reference or layer with visible content"
    );
    let result = Statistics {
        mean: mean.map(|v| v as f32),
        deviation: squared.map(|v: f64| (v.max(0.) / weight).sqrt() as f32),
        samples,
    };
    result.validate()?;
    check(cancel)?;
    Ok(result)
}

struct Transfer {
    scale: [f64; 3],
    offset: [f64; 3],
}

impl Transfer {
    fn new(source: &Statistics, settings: &Settings) -> Self {
        let mut scale = [1.; 3];
        let mut offset = [0.; 3];
        let t = f64::from(settings.amount);
        for c in 0..3 {
            if c == 0 && settings.preserve_lightness {
                continue;
            }
            let ratio = if source.deviation[c] < 0.0001 {
                1.
            } else {
                (f64::from(settings.reference.deviation[c]) / f64::from(source.deviation[c]))
                    .min(4.)
            };
            scale[c] = 1. + (ratio - 1.) * t;
            offset[c] =
                (f64::from(settings.reference.mean[c]) - ratio * f64::from(source.mean[c])) * t;
        }
        Self { scale, offset }
    }

    fn apply(&self, rgb: [f64; 3], space: WorkingSpace) -> [f64; 3] {
        let old = rgb_to_lab(rgb, space);
        let mut lab = std::array::from_fn(|c| old[c] * self.scale[c] + self.offset[c]);
        lab[0] = lab[0].clamp(0., 1.);
        if (0..3).all(|c| (lab[c] - old[c]).abs() < 1e-12) {
            return rgb;
        }
        let mut linear = lab_to_linear(lab, space);
        if !in_gamut(linear) {
            // A fixed 16 iterations bound the work, preserve hue/lightness,
            // and avoid clipping individual channels into a different hue.
            let mut low = 0.;
            let mut high = 1.;
            for _ in 0..16 {
                let chroma = (low + high) / 2.;
                if in_gamut(lab_to_linear(
                    [lab[0], lab[1] * chroma, lab[2] * chroma],
                    space,
                )) {
                    low = chroma;
                } else {
                    high = chroma;
                }
            }
            linear = lab_to_linear([lab[0], lab[1] * low, lab[2] * low], space);
        }
        linear.map(|v| {
            if space == WorkingSpace::LinearSrgb {
                v.clamp(0., 1.)
            } else {
                encode(v.clamp(0., 1.))
            }
        })
    }
}

pub fn apply16(
    source: &TiledImage16,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<TiledImage16> {
    settings.validate()?;
    dimensions(source.dimensions())?;
    check(cancel)?;
    let mut output = source.clone();
    if settings.amount == 0. {
        return Ok(output);
    }
    let transfer = Transfer::new(&statistics16(source, cancel)?, settings);
    for y in 0..source.height() {
        check(cancel)?;
        for x in 0..source.width() {
            let mut p = source.get_pixel(x, y).0;
            if p[3] == 0 {
                continue;
            }
            let rgb = transfer.apply(
                [p[0], p[1], p[2]].map(|v| f64::from(v) / 65_535.),
                source.working_space(),
            );
            for c in 0..3 {
                p[c] = (rgb[c] * 65_535.).round().clamp(0., 65_535.) as u16;
            }
            output.set_pixel(x, y, Rgba16(p))?;
        }
    }
    check(cancel)?;
    Ok(output)
}

pub fn apply8(source: &RgbaImage, settings: &Settings, cancel: &AtomicBool) -> Result<RgbaImage> {
    settings.validate()?;
    dimensions(source.dimensions())?;
    check(cancel)?;
    let mut output = source.clone();
    if settings.amount == 0. {
        return Ok(output);
    }
    let transfer = Transfer::new(&statistics8(source, cancel)?, settings);
    for (y, row) in output.rows_mut().enumerate() {
        check(cancel)?;
        for (x, pixel) in row.enumerate() {
            let p = source.get_pixel(x as u32, y as u32).0;
            if p[3] == 0 {
                continue;
            }
            let rgb = transfer.apply(
                [p[0], p[1], p[2]].map(|v| f64::from(v) / 255.),
                WorkingSpace::Srgb,
            );
            for c in 0..3 {
                pixel[c] = (rgb[c] * 255.).round().clamp(0., 255.) as u8;
            }
        }
    }
    check(cancel)?;
    Ok(output)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference colour match cancelled"
    );
    Ok(())
}
fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.into_iter()
        .all(|v| (-0.0000002..=1.0000002).contains(&v))
}

fn rgb_to_lab(rgb: [f64; 3], space: WorkingSpace) -> [f64; 3] {
    let mut rgb = if space == WorkingSpace::LinearSrgb {
        rgb
    } else {
        rgb.map(decode)
    };
    if space == WorkingSpace::DisplayP3 {
        // D65 Display P3 -> linear sRGB. Retain out-of-sRGB values; clamping
        // here would discard wide-gamut source colours before the operation.
        rgb = [
            1.2249401763 * rgb[0] - 0.2249401763 * rgb[1],
            -0.0420569547 * rgb[0] + 1.0420569547 * rgb[1],
            -0.0196375546 * rgb[0] - 0.0786360456 * rgb[1] + 1.0982736002 * rgb[2],
        ];
    }
    let l = (0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2]).cbrt();
    let m = (0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2]).cbrt();
    let s = (0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2]).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn lab_to_linear(lab: [f64; 3], space: WorkingSpace) -> [f64; 3] {
    let l = (lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2]).powi(3);
    let m = (lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2]).powi(3);
    let s = (lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2]).powi(3);
    let rgb = [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ];
    if space == WorkingSpace::DisplayP3 {
        [
            0.8224619687 * rgb[0] + 0.1775380313 * rgb[1],
            0.0331941989 * rgb[0] + 0.9668058011 * rgb[1],
            0.0170826307 * rgb[0] + 0.0723974407 * rgb[1] + 0.9105199286 * rgb[2],
        ]
    } else {
        rgb
    }
}
