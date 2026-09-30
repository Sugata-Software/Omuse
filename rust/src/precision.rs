//! Bounded tiled 16-bit RGBA storage for high precision editing.
//!
//! TiledRgba16 is independent from the u8 document model. Tiles are reference
//! counted and mutable access detaches only the tile being edited, making
//! snapshots cheap and preserving source pixels for non-destructive editing.

use anyhow::{Result, ensure};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;

pub const DEFAULT_TILE_SIZE: u32 = 64;
pub const MAX_PRECISION_PIXELS: u64 = 100_000_000;
pub const MAX_PRECISION_BYTES: usize = 768 * 1024 * 1024;
pub type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkingSpace {
    Srgb,
    LinearSrgb,
    DisplayP3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgba16(pub [u16; 4]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba8(pub [u8; 4]);

#[derive(Clone, Debug)]
struct Tile {
    width: u32,
    pixels: Vec<u16>,
}

/// A tiled, straight-alpha, 16-bit RGBA image.
#[derive(Clone, Debug)]
pub struct TiledRgba16 {
    width: u32,
    height: u32,
    tile_size: u32,
    working_space: WorkingSpace,
    tiles: Vec<Arc<Tile>>,
}

/// Integration-facing name for a retained 16-bit source/master image.
pub type TiledImage16 = TiledRgba16;

impl TiledRgba16 {
    pub fn new(width: u32, height: u32, working_space: WorkingSpace) -> Result<Self> {
        Self::with_tile_size(width, height, DEFAULT_TILE_SIZE, working_space)
    }

    pub fn with_tile_size(
        width: u32,
        height: u32,
        tile_size: u32,
        working_space: WorkingSpace,
    ) -> Result<Self> {
        ensure!(
            width > 0 && height > 0,
            "precision image dimensions must be nonzero"
        );
        ensure!(
            tile_size.is_power_of_two() && (16..=256).contains(&tile_size),
            "tile size must be a power of two between 16 and 256"
        );
        let pixels = u64::from(width) * u64::from(height);
        ensure!(
            pixels <= MAX_PRECISION_PIXELS,
            "precision image exceeds pixel limit"
        );
        let cols = width.div_ceil(tile_size);
        let rows = height.div_ceil(tile_size);
        let tile_count = u64::from(cols) * u64::from(rows);
        let bytes = pixels
            .checked_mul(8)
            .and_then(|n| n.checked_add(tile_count.saturating_mul(64)))
            .ok_or_else(|| anyhow::anyhow!("precision image memory overflow"))?;
        ensure!(
            bytes <= MAX_PRECISION_BYTES as u64,
            "precision image exceeds memory limit"
        );
        let mut tiles = Vec::with_capacity(tile_count as usize);
        for ty in 0..rows {
            for tx in 0..cols {
                let tw = (width - tx * tile_size).min(tile_size);
                let th = (height - ty * tile_size).min(tile_size);
                let len = usize::try_from(u64::from(tw) * u64::from(th) * 4)
                    .map_err(|_| anyhow::anyhow!("precision tile size overflow"))?;
                tiles.push(Arc::new(Tile {
                    width: tw,
                    pixels: vec![0; len],
                }));
            }
        }
        Ok(Self {
            width,
            height,
            tile_size,
            working_space,
            tiles,
        })
    }

    pub fn from_rgba16(image: &Rgba16Image) -> Result<Self> {
        Self::from_rgba16_in(image, WorkingSpace::Srgb)
    }

    pub fn from_rgba16_in(image: &Rgba16Image, working_space: WorkingSpace) -> Result<Self> {
        let mut output = Self::new(image.width(), image.height(), working_space)?;
        for (x, y, pixel) in image.enumerate_pixels() {
            output.set_pixel(x, y, Rgba16(pixel.0))?;
        }
        Ok(output)
    }

    pub fn from_rgba8(image: &RgbaImage) -> Result<Self> {
        Self::from_rgba8_in(image, WorkingSpace::Srgb)
    }

    pub fn from_rgba8_in(image: &RgbaImage, working_space: WorkingSpace) -> Result<Self> {
        let mut output = Self::new(image.width(), image.height(), working_space)?;
        for (x, y, pixel) in image.enumerate_pixels() {
            output.set_pixel(x, y, Rgba16(pixel.0.map(|v| u16::from(v) * 257)))?;
        }
        Ok(output)
    }

    pub fn from_png16(bytes: &[u8], working_space: WorkingSpace) -> Result<Self> {
        let decoded = image::load_from_memory_with_format(bytes, ImageFormat::Png)
            .map_err(|error| anyhow::anyhow!("invalid PNG16 asset: {error}"))?;
        ensure!(
            decoded.color() == image::ColorType::Rgba16,
            "PNG16 asset must contain RGBA16 pixels"
        );
        let decoded = decoded.into_rgba16();
        Self::from_rgba16_in(&decoded, working_space)
    }

    pub fn to_rgba16(&self) -> ImageBuffer<Rgba<u16>, Vec<u16>> {
        ImageBuffer::from_fn(self.width, self.height, |x, y| Rgba(self.get_pixel(x, y).0))
    }

    pub fn to_rgba8(&self) -> Result<RgbaImage> {
        self.to_rgba8_in(self.working_space)
    }

    pub fn to_rgba8_in(&self, output_space: WorkingSpace) -> Result<RgbaImage> {
        let mut copy = self.clone();
        copy.convert_working_space(output_space)?;
        Ok(RgbaImage::from_fn(copy.width, copy.height, |x, y| {
            let pixel = copy.get_pixel(x, y);
            Rgba(
                pixel
                    .0
                    .map(|v| ((u32::from(v) * 255 + 32_767) / 65_535) as u8),
            )
        }))
    }

    pub fn to_png16(&self) -> Result<Vec<u8>> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba16(self.to_rgba16())
            .write_to(&mut bytes, ImageFormat::Png)
            .map_err(|error| anyhow::anyhow!("PNG16 encoding failed: {error}"))?;
        Ok(bytes.into_inner())
    }

    pub fn proxy_rgba8(&self, max_dimension: u32) -> Result<RgbaImage> {
        ensure!(max_dimension > 0, "proxy dimension must be positive");
        let scale = (f64::from(max_dimension) / f64::from(self.width.max(self.height))).min(1.0);
        let width = ((f64::from(self.width) * scale).round() as u32).max(1);
        let height = ((f64::from(self.height) * scale).round() as u32).max(1);
        let source = self.to_rgba8()?;
        Ok(image::imageops::resize(
            &source,
            width,
            height,
            image::imageops::FilterType::Triangle,
        ))
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }
    pub fn working_space(&self) -> WorkingSpace {
        self.working_space
    }
    pub fn space(&self) -> WorkingSpace {
        self.working_space
    }
    pub fn logical_bytes(&self) -> usize {
        (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(8)
    }
    pub fn memory_bytes(&self) -> usize {
        let mut seen = HashSet::new();
        self.tiles
            .iter()
            .filter(|tile| seen.insert(Arc::as_ptr(tile) as usize))
            .map(|tile| tile.pixels.capacity() * 2 + 16)
            .sum()
    }
    pub fn tile_memory_identity(&self, x: u32, y: u32) -> usize {
        let (index, _) = self.tile_position(x.min(self.width - 1), y.min(self.height - 1));
        Arc::as_ptr(&self.tiles[index]) as usize
    }
    pub fn shares_tile_with(&self, other: &Self, x: u32, y: u32) -> bool {
        self.tile_memory_identity(x, y) == other.tile_memory_identity(x, y)
    }
    pub fn snapshot(&self) -> Self {
        self.clone()
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba16 {
        assert!(
            x < self.width && y < self.height,
            "pixel outside precision image"
        );
        let (index, offset) = self.tile_position(x, y);
        let tile = &self.tiles[index];
        let offset = offset * 4;
        Rgba16([
            tile.pixels[offset],
            tile.pixels[offset + 1],
            tile.pixels[offset + 2],
            tile.pixels[offset + 3],
        ])
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: Rgba16) -> Result<()> {
        ensure!(
            x < self.width && y < self.height,
            "pixel outside precision image"
        );
        let (index, offset) = self.tile_position(x, y);
        let tile = Arc::make_mut(&mut self.tiles[index]);
        let offset = offset * 4;
        tile.pixels[offset..offset + 4].copy_from_slice(&pixel.0);
        Ok(())
    }

    pub fn edit_region(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        mut edit: impl FnMut(u32, u32, &mut Rgba16),
    ) -> Result<usize> {
        let right = x
            .checked_add(width)
            .ok_or_else(|| anyhow::anyhow!("edit region overflow"))?;
        let bottom = y
            .checked_add(height)
            .ok_or_else(|| anyhow::anyhow!("edit region overflow"))?;
        ensure!(
            right <= self.width && bottom <= self.height,
            "edit region outside image"
        );
        let mut changed = 0;
        for py in y..bottom {
            for px in x..right {
                let old = self.get_pixel(px, py);
                let mut next = old;
                edit(px, py, &mut next);
                if next != old {
                    self.set_pixel(px, py, next)?;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    pub fn convert_working_space(&mut self, target: WorkingSpace) -> Result<()> {
        if self.working_space == target {
            return Ok(());
        }
        let source_space = self.working_space;
        for y in 0..self.height {
            for x in 0..self.width {
                let mut p = self.get_pixel(x, y);
                let rgb = convert_rgb(
                    [
                        f32::from(p.0[0]) / 65_535.,
                        f32::from(p.0[1]) / 65_535.,
                        f32::from(p.0[2]) / 65_535.,
                    ],
                    source_space,
                    target,
                );
                for c in 0..3 {
                    p.0[c] = (rgb[c].clamp(0., 1.) * 65_535.).round() as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        self.working_space = target;
        Ok(())
    }

    /// Apply the repository filter vocabulary directly in 16-bit storage.
    /// Alpha is preserved exactly by every filter.
    pub fn apply_filter(&mut self, filter: &crate::filters::Filter) -> Result<()> {
        crate::filters::validate(filter)?;
        match filter {
            crate::filters::Filter::Exposure { stops } => self.exposure(*stops),
            crate::filters::Filter::Levels {
                black,
                white,
                gamma,
            } => {
                ensure!(*white > *black, "white point must exceed black point");
                self.edit_rgb(|v| (((v - black) / (white - black)).clamp(0., 1.)).powf(1. / gamma))
            }
            crate::filters::Filter::Curves { points } => self.edit_rgb(|v| curve(v, points)),
            crate::filters::Filter::GaussianBlur { sigma } => self.blur(*sigma),
            crate::filters::Filter::UnsharpMask {
                sigma,
                amount,
                threshold,
            } => self.unsharp(*sigma, *amount, *threshold),
            crate::filters::Filter::Invert => self.edit_rgb(|v| 1. - v),
            crate::filters::Filter::Grayscale => self.grayscale(),
            crate::filters::Filter::Hsl {
                hue_degrees,
                saturation,
                lightness,
            } => self.edit_hsl(*hue_degrees, *saturation, *lightness),
            crate::filters::Filter::ColorBalance { red, green, blue } => self.edit_rgb3(|rgb| {
                [
                    (rgb[0] + red).clamp(0., 1.),
                    (rgb[1] + green).clamp(0., 1.),
                    (rgb[2] + blue).clamp(0., 1.),
                ]
            }),
            crate::filters::Filter::Noise {
                amount,
                seed,
                monochrome,
            } => self.noise(*amount, *seed, *monochrome),
            crate::filters::Filter::Vignette {
                amount,
                midpoint,
                feather,
            } => self.vignette(*amount, *midpoint, *feather),
            crate::filters::Filter::Bloom {
                sigma,
                amount,
                threshold,
            } => self.bloom(*sigma, *amount, *threshold),
            crate::filters::Filter::TonalContrast {
                shadows,
                midtones,
                highlights,
            } => self.tonal_contrast(*shadows, *midtones, *highlights),
            crate::filters::Filter::Dither(_)
            | crate::filters::Filter::BloomGlow { .. }
            | crate::filters::Filter::VignetteOverlay { .. }
            | crate::filters::Filter::LocalContrast { .. } => anyhow::bail!(
                "This finishing effect requires an 8-bit paint layer; rasterize a copy to preserve the 16-bit source"
            ),
        }
    }

    pub fn filtered(&self, filter: &crate::filters::Filter) -> Result<Self> {
        let mut output = self.clone();
        output.apply_filter(filter)?;
        Ok(output)
    }

    /// Clone and apply a filter while polling a caller-owned cancellation
    /// callback between rows. The source image remains unchanged on failure or
    /// cancellation.
    pub fn filtered_with_cancel<F>(
        &self,
        filter: &crate::filters::Filter,
        mut cancelled: F,
    ) -> Result<Self>
    where
        F: FnMut() -> bool,
    {
        ensure!(!cancelled(), "precision filter cancelled");
        crate::filters::validate(filter)?;
        let mut output = self.clone();
        match filter {
            crate::filters::Filter::GaussianBlur { sigma } => {
                output.blur_with_cancel(*sigma, &mut cancelled)?
            }
            crate::filters::Filter::UnsharpMask {
                sigma,
                amount,
                threshold,
            } => output.unsharp_with_cancel(*sigma, *amount, *threshold, &mut cancelled)?,
            crate::filters::Filter::Bloom {
                sigma,
                amount,
                threshold,
            } => {
                output.bloom_with_cancel(*sigma, *amount, *threshold, &mut cancelled)?;
            }
            crate::filters::Filter::TonalContrast {
                shadows,
                midtones,
                highlights,
            } => output.tonal_contrast_with_cancel(
                *shadows,
                *midtones,
                *highlights,
                &mut cancelled,
            )?,
            _ => output.apply_filter(filter)?,
        }
        Ok(output)
    }

    pub fn exposure(&mut self, stops: f32) -> Result<()> {
        ensure!(
            stops.is_finite() && (-20. ..=20.).contains(&stops),
            "invalid exposure"
        );
        let factor = 2.0f32.powf(stops);
        let space = self.working_space;
        self.edit_rgb3(|rgb| {
            let linear = convert_rgb(rgb, space, WorkingSpace::LinearSrgb);
            convert_rgb(
                linear.map(|v| (v * factor).clamp(0., 1.)),
                WorkingSpace::LinearSrgb,
                space,
            )
        })
    }

    pub fn blur(&mut self, sigma: f32) -> Result<()> {
        self.blur_with_cancel(sigma, &mut || false)
    }

    fn blur_with_cancel(&mut self, sigma: f32, cancelled: &mut impl FnMut() -> bool) -> Result<()> {
        ensure!(
            sigma.is_finite() && (0. ..=128.).contains(&sigma),
            "invalid blur sigma"
        );
        if sigma == 0. {
            return Ok(());
        }
        ensure!(
            u64::from(self.width) * u64::from(self.height) <= 16_777_216,
            "blur exceeds 16 megapixel limit"
        );
        // Three separable box passes approximate a Gaussian with matched
        // variance, in O(pixels) time independent of radius. Work in
        // premultiplied values so hidden RGB cannot contaminate visible edges.
        // One float plane plus a single row/column scratch bounds temporary
        // memory; immutable source tiles remain available to undo/cancellation.
        let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
        let mut lower = ideal.floor() as i32;
        if lower % 2 == 0 {
            lower -= 1;
        }
        lower = lower.max(1);
        let upper = lower + 2;
        let lower_count =
            ((12.0 * sigma * sigma - 3.0 * (lower * lower) as f32 - 12.0 * lower as f32 - 9.0)
                / (-4.0 * lower as f32 - 4.0))
                .round()
                .clamp(0., 3.) as usize;
        let mut plane = Vec::<[f32; 4]>::with_capacity(self.width as usize * self.height as usize);
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let p = self.get_pixel(x, y).0;
                let alpha = f32::from(p[3]) / 65535.;
                plane.push([
                    f32::from(p[0]) * alpha,
                    f32::from(p[1]) * alpha,
                    f32::from(p[2]) * alpha,
                    alpha,
                ]);
            }
        }
        fn line(
            plane: &mut [[f32; 4]],
            start: usize,
            stride: usize,
            length: usize,
            radius: usize,
            scratch: &mut Vec<[f32; 4]>,
        ) {
            if radius == 0 {
                return;
            }
            scratch.clear();
            scratch.extend((0..length).map(|i| plane[start + i * stride]));
            let mut sum = [0f64; 4];
            let edge = |position: i64| -> usize { position.clamp(0, length as i64 - 1) as usize };
            for offset in -(radius as i64)..=radius as i64 {
                let p = scratch[edge(offset)];
                for c in 0..4 {
                    sum[c] += f64::from(p[c]);
                }
            }
            let denominator = (radius * 2 + 1) as f64;
            for i in 0..length {
                plane[start + i * stride] = sum.map(|v| (v / denominator) as f32);
                let removed = scratch[edge(i as i64 - radius as i64)];
                let added = scratch[edge(i as i64 + radius as i64 + 1)];
                for c in 0..4 {
                    sum[c] += f64::from(added[c]) - f64::from(removed[c]);
                }
            }
        }
        let mut scratch = Vec::with_capacity(self.width.max(self.height) as usize);
        for pass in 0..3 {
            let radius = ((if pass < lower_count { lower } else { upper }) / 2) as usize;
            for y in 0..self.height as usize {
                ensure!(!cancelled(), "precision filter cancelled");
                line(
                    &mut plane,
                    y * self.width as usize,
                    1,
                    self.width as usize,
                    radius,
                    &mut scratch,
                );
            }
            for x in 0..self.width as usize {
                ensure!(!cancelled(), "precision filter cancelled");
                line(
                    &mut plane,
                    x,
                    self.width as usize,
                    self.height as usize,
                    radius,
                    &mut scratch,
                );
            }
        }
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let soft = plane[(y * self.width + x) as usize];
                let mut p = self.get_pixel(x, y);
                for c in 0..3 {
                    p.0[c] = if soft[3] > 0. {
                        (soft[c] / soft[3]).round().clamp(0., 65535.) as u16
                    } else {
                        0
                    };
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }

    fn bloom(&mut self, sigma: f32, amount: f32, threshold: f32) -> Result<()> {
        self.bloom_with_cancel(sigma, amount, threshold, &mut || false)
    }

    /// Extract thresholded highlights first, blur that highlight image, then
    /// screen-composite the glow over the source.  This mirrors the byte
    /// filter's ordering and keeps alpha as a source property.
    fn bloom_with_cancel(
        &mut self,
        sigma: f32,
        amount: f32,
        threshold: f32,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<()> {
        let mut bright = self.clone();
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let mut p = bright.get_pixel(x, y);
                let rgb = [
                    f32::from(p.0[0]) / 65_535.,
                    f32::from(p.0[1]) / 65_535.,
                    f32::from(p.0[2]) / 65_535.,
                ];
                let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let weight = if threshold >= 1. {
                    0.
                } else {
                    ((lum - threshold) / (1. - threshold)).clamp(0., 1.)
                };
                for c in 0..3 {
                    p.0[c] = (rgb[c] * weight * 65_535.).round() as u16;
                }
                bright.set_pixel(x, y, p)?;
            }
        }
        let mut bloom = bright;
        bloom.blur_with_cancel(sigma, cancelled)?;
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let mut p = self.get_pixel(x, y);
                if p.0[3] == 0 {
                    continue;
                }
                let glow_pixel = bloom.get_pixel(x, y);
                for c in 0..3 {
                    let glow = (f32::from(glow_pixel.0[c]) / 65_535. * amount).clamp(0., 1.);
                    let source = f32::from(p.0[c]) / 65_535.;
                    p.0[c] = ((1. - (1. - source) * (1. - glow)) * 65_535.)
                        .round()
                        .clamp(0., 65_535.) as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }

    fn tonal_contrast(&mut self, shadows: f32, midtones: f32, highlights: f32) -> Result<()> {
        self.tonal_contrast_with_cancel(shadows, midtones, highlights, &mut || false)
    }

    fn tonal_contrast_with_cancel(
        &mut self,
        shadows: f32,
        midtones: f32,
        highlights: f32,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<()> {
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let mut p = self.get_pixel(x, y);
                if p.0[3] == 0 {
                    continue;
                }
                let rgb = [
                    f32::from(p.0[0]) / 65_535.,
                    f32::from(p.0[1]) / 65_535.,
                    f32::from(p.0[2]) / 65_535.,
                ];
                let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let shadow = (1. - 2. * lum).max(0.);
                let highlight = (2. * lum - 1.).max(0.);
                let middle = 1. - shadow - highlight;
                let strength = shadow * shadows + middle * midtones + highlight * highlights;
                for c in 0..3 {
                    p.0[c] = (0.5 + (rgb[c] - 0.5) * (1. + strength))
                        .clamp(0., 1.)
                        .mul_add(65_535., 0.)
                        .round() as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }

    pub fn unsharp(&mut self, sigma: f32, amount: f32, threshold: f32) -> Result<()> {
        self.unsharp_with_cancel(sigma, amount, threshold, &mut || false)
    }

    fn unsharp_with_cancel(
        &mut self,
        sigma: f32,
        amount: f32,
        threshold: f32,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<()> {
        ensure!(
            amount.is_finite() && (0. ..=10.).contains(&amount),
            "invalid unsharp amount"
        );
        ensure!(
            threshold.is_finite() && (0. ..=1.).contains(&threshold),
            "invalid unsharp threshold"
        );
        let source = self.clone();
        let mut blurred = source.clone();
        blurred.blur_with_cancel(sigma, cancelled)?;
        for y in 0..self.height {
            ensure!(!cancelled(), "precision filter cancelled");
            for x in 0..self.width {
                let original = source.get_pixel(x, y);
                let soft = blurred.get_pixel(x, y);
                let mut p = original;
                for c in 0..3 {
                    let value = f32::from(original.0[c]) / 65_535.;
                    let delta = (f32::from(original.0[c]) - f32::from(soft.0[c])) / 65_535.;
                    if delta.abs() >= threshold {
                        p.0[c] = ((value + amount * delta).clamp(0., 1.) * 65_535.).round() as u16;
                    }
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }

    fn grayscale(&mut self) -> Result<()> {
        self.edit_rgb3(|rgb| {
            let v = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            [v; 3]
        })
    }
    fn tile_position(&self, x: u32, y: u32) -> (usize, usize) {
        let cols = self.width.div_ceil(self.tile_size);
        let tx = x / self.tile_size;
        let ty = y / self.tile_size;
        let index = (ty * cols + tx) as usize;
        let tile = &self.tiles[index];
        (
            index,
            ((y % self.tile_size) * tile.width + (x % self.tile_size)) as usize,
        )
    }
    fn edit_rgb(&mut self, mut f: impl FnMut(f32) -> f32) -> Result<()> {
        self.edit_rgb3(|rgb| [f(rgb[0]), f(rgb[1]), f(rgb[2])])
    }
    fn edit_rgb3(&mut self, mut f: impl FnMut([f32; 3]) -> [f32; 3]) -> Result<()> {
        for y in 0..self.height {
            for x in 0..self.width {
                let mut p = self.get_pixel(x, y);
                let rgb = [
                    f32::from(p.0[0]) / 65_535.,
                    f32::from(p.0[1]) / 65_535.,
                    f32::from(p.0[2]) / 65_535.,
                ];
                for (c, value) in f(rgb).into_iter().enumerate() {
                    p.0[c] = (value.clamp(0., 1.) * 65_535.).round() as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }
    fn edit_hsl(&mut self, hue: f32, saturation: f32, lightness: f32) -> Result<()> {
        self.edit_rgb3(|rgb| {
            let (mut h, mut s, mut l) = rgb_to_hsl(rgb);
            h = (h + hue / 360.).rem_euclid(1.);
            s = (s + saturation).clamp(0., 1.);
            l = (l + lightness).clamp(0., 1.);
            hsl_to_rgb([h, s, l])
        })
    }
    fn noise(&mut self, amount: f32, seed: u64, monochrome: bool) -> Result<()> {
        let mut state = seed ^ 0x9e3779b97f4a7c15;
        if state == 0 {
            state = 1;
        }
        for y in 0..self.height {
            for x in 0..self.width {
                let mut p = self.get_pixel(x, y);
                if p.0[3] == 0 {
                    continue;
                }
                let n = precision_random(&mut state) * amount;
                for c in 0..3 {
                    let delta = if monochrome {
                        n
                    } else {
                        precision_random(&mut state) * amount
                    };
                    p.0[c] = ((f32::from(p.0[c]) / 65_535. + delta).clamp(0., 1.) * 65_535.).round()
                        as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }
    fn vignette(&mut self, amount: f32, midpoint: f32, feather: f32) -> Result<()> {
        let cx = (self.width as f32 - 1.) * 0.5;
        let cy = (self.height as f32 - 1.) * 0.5;
        let radius = (cx * cx + cy * cy).sqrt().max(1.);
        for y in 0..self.height {
            for x in 0..self.width {
                let distance = (((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt()
                    / radius)
                    .clamp(0., 1.);
                let factor =
                    (1. - amount * ((distance - midpoint) / feather).clamp(0., 1.)).clamp(0., 2.);
                let mut p = self.get_pixel(x, y);
                for c in 0..3 {
                    p.0[c] = (f32::from(p.0[c]) * factor).clamp(0., 65_535.) as u16;
                }
                self.set_pixel(x, y, p)?;
            }
        }
        Ok(())
    }
}

fn curve(v: f32, points: &[(f32, f32)]) -> f32 {
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if v <= x1 {
            let t = ((v - x0) / (x1 - x0)).clamp(0., 1.);
            return y0 + (y1 - y0) * t;
        }
    }
    points.last().map_or(v, |point| point.1)
}
fn precision_random(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    ((*state >> 40) as f32 / 16_777_215.0) * 2.0 - 1.0
}
fn transfer_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn transfer_from_linear(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn decode_rgb(rgb: [f32; 3], space: WorkingSpace) -> [f32; 3] {
    match space {
        WorkingSpace::LinearSrgb => rgb,
        WorkingSpace::Srgb | WorkingSpace::DisplayP3 => rgb.map(transfer_to_linear),
    }
}
fn encode_rgb(rgb: [f32; 3], space: WorkingSpace) -> [f32; 3] {
    match space {
        WorkingSpace::LinearSrgb => rgb,
        WorkingSpace::Srgb | WorkingSpace::DisplayP3 => rgb.map(transfer_from_linear),
    }
}
fn convert_rgb(rgb: [f32; 3], from: WorkingSpace, to: WorkingSpace) -> [f32; 3] {
    if from == to {
        return rgb;
    }
    let linear = decode_rgb(rgb, from);
    let xyz = match from {
        WorkingSpace::DisplayP3 => [
            0.48657095 * linear[0] + 0.26566769 * linear[1] + 0.19821729 * linear[2],
            0.22897456 * linear[0] + 0.69173852 * linear[1] + 0.07928691 * linear[2],
            0.04511338 * linear[1] + 1.04394437 * linear[2],
        ],
        _ => [
            0.4123908 * linear[0] + 0.35758434 * linear[1] + 0.18048079 * linear[2],
            0.21263901 * linear[0] + 0.71516868 * linear[1] + 0.07219232 * linear[2],
            0.01933082 * linear[0] + 0.11919478 * linear[1] + 0.95053215 * linear[2],
        ],
    };
    let destination_linear = match to {
        WorkingSpace::DisplayP3 => [
            2.4934969 * xyz[0] - 0.9313836 * xyz[1] - 0.4027108 * xyz[2],
            -0.829489 * xyz[0] + 1.762664 * xyz[1] + 0.0236247 * xyz[2],
            0.0358458 * xyz[0] - 0.0761724 * xyz[1] + 0.9568845 * xyz[2],
        ],
        _ => [
            3.24096994 * xyz[0] - 1.53738318 * xyz[1] - 0.49861076 * xyz[2],
            -0.96924364 * xyz[0] + 1.8759675 * xyz[1] + 0.04155506 * xyz[2],
            0.05563008 * xyz[0] - 0.20397696 * xyz[1] + 1.0569715 * xyz[2],
        ],
    };
    encode_rgb(destination_linear.map(|v| v.clamp(0., 1.)), to)
}
fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let l = (max + min) * 0.5;
    if (max - min).abs() < f32::EPSILON {
        return (0., 0., l);
    }
    let d = max - min;
    let s = d / (1. - (2. * l - 1.).abs());
    let mut h = if (max - rgb[0]).abs() < f32::EPSILON {
        (rgb[1] - rgb[2]) / d
    } else if (max - rgb[1]).abs() < f32::EPSILON {
        (rgb[2] - rgb[0]) / d + 2.
    } else {
        (rgb[0] - rgb[1]) / d + 4.
    };
    h /= 6.;
    (h.rem_euclid(1.), s, l)
}
fn hsl_to_rgb(hsl: [f32; 3]) -> [f32; 3] {
    let (h, s, l) = (hsl[0], hsl[1], hsl[2]);
    if s == 0. {
        return [l; 3];
    }
    let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
    let p = 2. * l - q;
    [h + 1. / 3., h, h - 1. / 3.].map(|mut t| {
        if t < 0. {
            t += 1.;
        }
        if t > 1. {
            t -= 1.;
        }
        if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 0.5 {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::{self, Filter};
    use image::Rgba;

    fn oracle_image() -> RgbaImage {
        RgbaImage::from_fn(9, 7, |x, y| {
            Rgba([
                (x * 23 + y * 7) as u8,
                (y * 31 + x * 5) as u8,
                (x * 13 + y * 19 + 17) as u8,
                255,
            ])
        })
    }

    fn assert_u8_oracle(filter: Filter, tolerance: u8) {
        let source = oracle_image();
        let mut low = source.clone();
        filters::apply(&mut low, &filter).unwrap();
        let mut high = TiledRgba16::from_rgba8(&source).unwrap();
        high.apply_filter(&filter).unwrap();
        let converted = high.to_rgba8().unwrap();
        for (expected, actual) in low.pixels().zip(converted.pixels()) {
            for c in 0..4 {
                assert!(
                    expected[c].abs_diff(actual[c]) <= tolerance,
                    "channel {c}: byte={} precision={} tolerance={tolerance}",
                    expected[c],
                    actual[c]
                );
            }
        }
    }
    #[test]
    fn u16_round_trip_and_cow_tile_snapshot_preserve_precision() {
        let mut image = TiledRgba16::with_tile_size(80, 2, 64, WorkingSpace::Srgb).unwrap();
        image
            .set_pixel(1, 0, Rgba16([257, 32_769, 60_001, 40_000]))
            .unwrap();
        let snapshot = image.snapshot();
        image.set_pixel(1, 0, Rgba16([65_534, 2, 3, 4])).unwrap();
        assert_eq!(
            snapshot.get_pixel(1, 0),
            Rgba16([257, 32_769, 60_001, 40_000])
        );
        assert!(snapshot.memory_bytes() > 0);
        assert_eq!(image.to_rgba16().get_pixel(1, 0).0, [65_534, 2, 3, 4]);
    }
    #[test]
    fn exposure_changes_16bit_levels_and_keeps_alpha() {
        let mut image = TiledRgba16::new(2, 1, WorkingSpace::LinearSrgb).unwrap();
        image
            .set_pixel(0, 0, Rgba16([12_345, 23_456, 34_567, 48_000]))
            .unwrap();
        image.exposure(1.).unwrap();
        let pixel = image.get_pixel(0, 0);
        assert!(pixel.0[0] > 24_000 && pixel.0[0] < 25_000);
        assert_eq!(pixel.0[3], 48_000);
    }
    #[test]
    fn color_conversion_and_png16_retain_alpha() {
        let mut image = TiledRgba16::new(1, 1, WorkingSpace::Srgb).unwrap();
        image
            .set_pixel(0, 0, Rgba16([40_001, 12_345, 55_555, 31_337]))
            .unwrap();
        image
            .convert_working_space(WorkingSpace::DisplayP3)
            .unwrap();
        image.convert_working_space(WorkingSpace::Srgb).unwrap();
        assert!(image.get_pixel(0, 0).0[0].abs_diff(40_001) < 8);
        assert_eq!(image.get_pixel(0, 0).0[3], 31_337);
        let decoded =
            TiledRgba16::from_png16(&image.to_png16().unwrap(), WorkingSpace::Srgb).unwrap();
        assert_eq!(decoded.get_pixel(0, 0).0[3], 31_337);
    }

    #[test]
    fn promoted_16bit_point_filters_track_byte_oracles() {
        assert_u8_oracle(Filter::Exposure { stops: 0.65 }, 1);
        assert_u8_oracle(
            Filter::Noise {
                amount: 0.12,
                seed: 0x1234_5678_9abc_def0,
                monochrome: false,
            },
            1,
        );
        assert_u8_oracle(
            Filter::TonalContrast {
                shadows: 0.35,
                midtones: -0.2,
                highlights: 0.55,
            },
            1,
        );
    }

    #[test]
    fn promoted_16bit_spatial_filters_track_byte_oracles() {
        assert_u8_oracle(Filter::GaussianBlur { sigma: 1.4 }, 3);
        assert_u8_oracle(
            Filter::Bloom {
                sigma: 1.1,
                amount: 0.7,
                threshold: 0.42,
            },
            3,
        );
        assert_u8_oracle(
            Filter::UnsharpMask {
                sigma: 1.2,
                amount: 0.8,
                // Keep the oracle away from the byte quantisation boundary:
                // byte blur rounds this sample below 0.02 while 16-bit blur
                // correctly sees the unrounded delta just above it.
                threshold: 0.03,
            },
            3,
        );
    }

    #[test]
    fn unsharp_threshold_uses_unrounded_precision_delta() {
        let source = oracle_image();
        let filter = Filter::UnsharpMask {
            sigma: 1.2,
            amount: 0.8,
            threshold: 0.02,
        };
        let mut low = source.clone();
        filters::apply(&mut low, &filter).unwrap();
        let mut high = TiledRgba16::from_rgba8(&source).unwrap();
        high.apply_filter(&filter).unwrap();
        let converted = high.to_rgba8().unwrap();
        assert_eq!(low.get_pixel(8, 1).0[2], 140);
        assert_eq!(converted.get_pixel(8, 1).0[2], 144);
    }

    #[test]
    fn blur_is_deterministic_and_preserves_alpha_edges_on_cancel() {
        let mut source = TiledRgba16::new(13, 9, WorkingSpace::Srgb).unwrap();
        for y in 0..source.height() {
            for x in 0..source.width() {
                let alpha = if x == 0 || y == 0 { 0 } else { 65_535 };
                source
                    .set_pixel(
                        x,
                        y,
                        Rgba16([65_535, (x * 3_000) as u16, (y * 5_000) as u16, alpha]),
                    )
                    .unwrap();
            }
        }
        let first = source
            .filtered_with_cancel(&Filter::GaussianBlur { sigma: 2. }, || false)
            .unwrap();
        let second = source
            .filtered_with_cancel(&Filter::GaussianBlur { sigma: 2. }, || false)
            .unwrap();
        assert_eq!(first.to_rgba16(), second.to_rgba16());
        for y in 0..source.height() {
            for x in 0..source.width() {
                assert_eq!(first.get_pixel(x, y).0[3], source.get_pixel(x, y).0[3]);
            }
        }
        let unchanged = source.to_rgba16();
        assert!(
            source
                .filtered_with_cancel(&Filter::GaussianBlur { sigma: 2. }, || true)
                .is_err()
        );
        assert_eq!(source.to_rgba16(), unchanged);
    }
}
