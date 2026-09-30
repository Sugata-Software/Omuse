//! Deterministic, bounded retro image finishes. All methods preserve the exact
//! source alpha, including hidden RGB, and use alpha-weighted colour sampling.
//! ASCII uses a built-in pixel alphabet, so saved recipes do not depend on fonts.
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_PIXELS: u64 = 16_777_216;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    #[default]
    Atkinson,
    FloydSteinberg,
    Bayer2,
    Bayer4,
    Bayer8,
    Dots,
    Lines,
    Diamonds,
    Patterns,
    Ascii,
}
impl Style {
    pub const ALL: [Self; 10] = [
        Self::Atkinson,
        Self::FloydSteinberg,
        Self::Bayer2,
        Self::Bayer4,
        Self::Bayer8,
        Self::Dots,
        Self::Lines,
        Self::Diamonds,
        Self::Patterns,
        Self::Ascii,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Atkinson => "Atkinson",
            Self::FloydSteinberg => "Floyd–Steinberg",
            Self::Bayer2 => "Bayer 2 × 2",
            Self::Bayer4 => "Bayer 4 × 4",
            Self::Bayer8 => "Bayer 8 × 8",
            Self::Dots => "Halftone dots",
            Self::Lines => "Halftone lines",
            Self::Diamonds => "Halftone diamonds",
            Self::Patterns => "Patterns",
            Self::Ascii => "ASCII",
        }
    }
    fn diffuses(self) -> bool {
        matches!(self, Self::Atkinson | Self::FloydSteinberg)
    }
    fn ordered(self) -> bool {
        matches!(self, Self::Bayer2 | Self::Bayer4 | Self::Bayer8)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Palette {
    #[default]
    BlackWhite,
    TwoColors,
    Original,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PixelShape {
    #[default]
    Square,
    Dot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub style: Style,
    pub pixel_size: u32,
    pub cell_size: u32,
    pub levels: u8,
    pub angle: f32,
    pub diffusion: f32,
    pub density: f32,
    pub contrast: f32,
    pub palette: Palette,
    pub dark: [u8; 3],
    pub light: [u8; 3],
    pub pixel_shape: PixelShape,
    pub light_on_dark: bool,
    pub characters: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            style: Style::Atkinson,
            pixel_size: 2,
            cell_size: 8,
            levels: 2,
            angle: 45.,
            diffusion: 1.,
            density: 0.,
            contrast: 0.,
            palette: Palette::BlackWhite,
            dark: [27, 29, 31],
            light: [245, 211, 162],
            pixel_shape: PixelShape::Square,
            light_on_dark: true,
            characters: " .:-=+*#%@".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=32).contains(&self.pixel_size),
            "Pixel size must be 1–32"
        );
        ensure!((4..=64).contains(&self.cell_size), "Cell size must be 4–64");
        ensure!((2..=8).contains(&self.levels), "Tones must be 2–8");
        for (value, low, high, name) in [
            (self.angle, -90., 90., "Angle"),
            (self.diffusion, 0., 1., "Diffusion"),
            (self.density, -1., 1., "Density"),
            (self.contrast, -1., 1., "Contrast"),
        ] {
            ensure!(
                value.is_finite() && (low..=high).contains(&value),
                "{name} must be finite and between {low} and {high}"
            );
        }
        ensure!(
            !self.characters.is_empty()
                && self.characters.len() <= 64
                && self.characters.bytes().all(|c| (32..=126).contains(&c)),
            "ASCII uses 1–64 printable ASCII characters"
        );
        Ok(())
    }
}

/// Work is transactional: invalid settings, an oversized image or cancellation
/// return without replacing any source pixels.
pub fn apply(image: &mut RgbaImage, settings: &Settings, cancel: &AtomicBool) -> Result<()> {
    settings.validate()?;
    validate_dimensions(image.width(), image.height())?;
    check(cancel)?;
    let block = settings.pixel_size;
    let (w, h) = (
        image.width().div_ceil(block),
        image.height().div_ceil(block),
    );
    // Only the downsampled grid and three diffusion rows are allocated. No
    // image-sized float planes, summed-area tables or glyph surfaces are needed.
    let mut grid = RgbaImage::new(w, h);
    for y in 0..h {
        check(cancel)?;
        for x in 0..w {
            grid.put_pixel(x, y, sample(image, x * block, y * block, block, block));
        }
    }
    if settings.style.diffuses() {
        diffusion(&mut grid, settings, cancel)?;
    } else if settings.style.ordered() {
        for y in 0..h {
            check(cancel)?;
            for x in 0..w {
                let p = grid.get_pixel_mut(x, y);
                if p[3] == 0 {
                    continue;
                }
                let tones = tone(*p, settings);
                let threshold = bayer(settings.style, x, y);
                let steps = f32::from(settings.levels - 1);
                let quantized = tones.map(|v| (v * steps + threshold).floor().min(steps) / steps);
                write_color(p, palette(quantized, settings));
            }
        }
    } else {
        marks(&mut grid, settings, cancel)?;
    }
    check(cancel)?;
    let mut result = image.clone();
    for y in 0..image.height() {
        check(cancel)?;
        for x in 0..image.width() {
            let out = result.get_pixel_mut(x, y);
            if out[3] == 0 {
                continue;
            }
            let mut rgb = grid.get_pixel(x / block, y / block).0;
            if settings.pixel_shape == PixelShape::Dot && block > 1 {
                let dx = (x % block) as f32 + 0.5 - block as f32 / 2.;
                let dy = (y % block) as f32 + 0.5 - block as f32 / 2.;
                if dx * dx + dy * dy > (block as f32 / 2.).powi(2) {
                    let gap = if settings.palette == Palette::TwoColors {
                        settings.dark
                    } else {
                        [0; 3]
                    };
                    rgb[..3].copy_from_slice(&gap);
                }
            }
            out.0[..3].copy_from_slice(&rgb[..3]);
        }
    }
    check(cancel)?;
    *image = result;
    Ok(())
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        crate::model::valid_dimensions(width, height)
            && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Dither supports images up to 16 megapixels"
    );
    Ok(())
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Dither cancelled");
    Ok(())
}
fn sample(image: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Rgba<u8> {
    let (right, bottom) = ((x + w).min(image.width()), (y + h).min(image.height()));
    let mut sum = [0u64; 4];
    for py in y..bottom {
        for px in x..right {
            let p = image.get_pixel(px, py);
            for c in 0..3 {
                sum[c] += u64::from(p[c]) * u64::from(p[3]);
            }
            sum[3] += u64::from(p[3]);
        }
    }
    if sum[3] == 0 {
        return Rgba([0; 4]);
    }
    let mut p = [0; 4];
    for c in 0..3 {
        p[c] = ((sum[c] + sum[3] / 2) / sum[3]) as u8;
    }
    let area = u64::from(right - x) * u64::from(bottom - y);
    // Keep very thin, low-alpha details eligible in the grid. The original
    // per-pixel alpha is restored exactly on output, even at partial edge blocks.
    p[3] = ((sum[3] + area / 2) / area).max(1) as u8;
    Rgba(p)
}
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
fn rgb(p: Rgba<u8>) -> [f32; 3] {
    [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.)
}
fn tone(p: Rgba<u8>, settings: &Settings) -> [f32; 3] {
    let color = rgb(p);
    let color = if settings.palette == Palette::Original {
        color
    } else {
        [luma(color); 3]
    };
    let gain = if settings.contrast >= 0. {
        1. / (1. - settings.contrast * 0.95)
    } else {
        1. + settings.contrast
    };
    color.map(|v| ((v.powf(2f32.powf(settings.density * 1.5)) - 0.5) * gain + 0.5).clamp(0., 1.))
}
fn palette(tone: [f32; 3], settings: &Settings) -> [f32; 3] {
    if settings.palette != Palette::TwoColors {
        return tone;
    }
    std::array::from_fn(|c| {
        (f32::from(settings.dark[c]) * (1. - tone[0]) + f32::from(settings.light[c]) * tone[0])
            / 255.
    })
}
fn write_color(pixel: &mut Rgba<u8>, color: [f32; 3]) {
    for c in 0..3 {
        pixel[c] = (color[c].clamp(0., 1.) * 255.).round() as u8;
    }
}

fn diffusion(grid: &mut RgbaImage, s: &Settings, cancel: &AtomicBool) -> Result<()> {
    let width = grid.width() as usize;
    let mut errors = vec![[0f32; 3]; width * 3];
    let taps: &[(i32, u32, f32)] = if s.style == Style::Atkinson {
        &[
            (1, 0, 0.125),
            (2, 0, 0.125),
            (-1, 1, 0.125),
            (0, 1, 0.125),
            (1, 1, 0.125),
            (0, 2, 0.125),
        ]
    } else {
        &[
            (1, 0, 7. / 16.),
            (-1, 1, 3. / 16.),
            (0, 1, 5. / 16.),
            (1, 1, 1. / 16.),
        ]
    };
    let steps = f32::from(s.levels - 1);
    for y in 0..grid.height() {
        check(cancel)?;
        let reverse = y % 2 == 1;
        let row = y as usize % 3;
        for column in 0..width {
            let x = if reverse { width - 1 - column } else { column };
            let p = *grid.get_pixel(x as u32, y);
            let incoming = std::mem::take(&mut errors[row * width + x]);
            if p[3] == 0 {
                continue;
            }
            let original = tone(p, s);
            let old: [f32; 3] = std::array::from_fn(|c| original[c] + incoming[c]);
            let quantized = old.map(|v| (v.clamp(0., 1.) * steps).round() / steps);
            write_color(grid.get_pixel_mut(x as u32, y), palette(quantized, s));
            for &(dx, dy, weight) in taps {
                let nx = x as i32 + if reverse { -dx } else { dx };
                let ny = y + dy;
                if nx < 0 || nx >= width as i32 || ny >= grid.height() {
                    continue;
                }
                let target_alpha = grid.get_pixel(nx as u32, ny)[3];
                if target_alpha == 0 {
                    continue;
                }
                let visibility = (f32::from(p[3]) / f32::from(target_alpha)).min(1.);
                let at = ny as usize % 3 * width + nx as usize;
                for c in 0..3 {
                    errors[at][c] += (old[c] - quantized[c]) * weight * s.diffusion * visibility;
                }
            }
        }
    }
    Ok(())
}
fn bayer(style: Style, x: u32, y: u32) -> f32 {
    // Build the recursive matrix from its two-bit quadrants without a table.
    let size = match style {
        Style::Bayer2 => 2,
        Style::Bayer4 => 4,
        _ => 8,
    };
    let mut value = 0;
    let mut scale = 1;
    while scale < size {
        let xb = (x / scale) & 1;
        let yb = (y / scale) & 1;
        value = value * 4 + ((xb ^ yb) * 2 + yb);
        scale *= 2;
    }
    (value as f32 + 0.5) / (size * size) as f32
}

fn marks(grid: &mut RgbaImage, s: &Settings, cancel: &AtomicBool) -> Result<()> {
    let cell = s.cell_size;
    let glyphs = if s.style == Style::Ascii {
        ascii_glyphs(&s.characters)
    } else {
        vec![]
    };
    let (sin, cos) = s.angle.to_radians().sin_cos();
    for top in (0..grid.height()).step_by(cell as usize) {
        check(cancel)?;
        for left in (0..grid.width()).step_by(cell as usize) {
            let average = sample(grid, left, top, cell, cell);
            if average[3] == 0 {
                continue;
            }
            let tone = luma(tone(average, s));
            let amount = if s.light_on_dark { tone } else { 1. - tone };
            let glyph = if glyphs.is_empty() {
                None
            } else {
                Some(&glyphs[(amount * (glyphs.len() - 1) as f32).round() as usize].1)
            };
            for y in top..(top + cell).min(grid.height()) {
                for x in left..(left + cell).min(grid.width()) {
                    let p = grid.get_pixel_mut(x, y);
                    if p[3] == 0 {
                        continue;
                    }
                    let marked = match s.style {
                        Style::Ascii => {
                            let gx = ((x - left) * 6 / cell).min(5);
                            let gy = ((y - top) * 8 / cell).min(7);
                            gx < 5 && gy < 7 && glyph.unwrap()[gy as usize] & (1 << (4 - gx)) != 0
                        }
                        Style::Patterns => pattern(amount, x - left, y - top),
                        _ => {
                            // Anchor the screen to the layer, not the preview or
                            // selection rectangle, so repeated crops stay coherent.
                            let u = ((x as f32 + 0.5) * cos + (y as f32 + 0.5) * sin) / cell as f32;
                            let v =
                                (-(x as f32 + 0.5) * sin + (y as f32 + 0.5) * cos) / cell as f32;
                            let (u, v) = (u.rem_euclid(1.) - 0.5, v.rem_euclid(1.) - 0.5);
                            let distance = match s.style {
                                Style::Dots => (u * u + v * v) * std::f32::consts::PI,
                                Style::Lines => v.abs() * 2.,
                                _ => u.abs() + v.abs(),
                            };
                            amount >= 1. || (amount > 0. && distance < amount)
                        }
                    };
                    let light = if s.light_on_dark { marked } else { !marked };
                    let color = if s.palette == Palette::Original {
                        if s.light_on_dark {
                            if marked { rgb(average) } else { [0.; 3] }
                        } else if marked {
                            rgb(average)
                        } else {
                            [1.; 3]
                        }
                    } else {
                        palette([if light { 1. } else { 0. }; 3], s)
                    };
                    write_color(p, color);
                }
            }
        }
    }
    Ok(())
}

fn pattern(amount: f32, x: u32, y: u32) -> bool {
    // A monotone 8×8 hatch: sparse points develop into diagonal weave, then
    // denser lines. Coverage cannot regress as density increases.
    let rank = ((x + y * 3) % 8) * 8 + ((y + x * 2) % 8);
    (rank as f32 + 0.5) / 64. < amount
}
fn ascii_glyphs(characters: &str) -> Vec<(u32, [u8; 7])> {
    let mut glyphs: Vec<_> = characters
        .bytes()
        .map(|c| {
            let rows = glyph(c);
            (rows.iter().map(|r| r.count_ones()).sum::<u32>(), rows)
        })
        .collect();
    glyphs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    glyphs.dedup_by_key(|g| g.1);
    glyphs
}

// Five-bit rows, centered in a six-by-eight character cell. The compact
// built-in alphabet intentionally retains a crisp terminal / print aesthetic.
fn glyph(c: u8) -> [u8; 7] {
    match c {
        b' ' => [0; 7],
        b'!' => [4, 4, 4, 4, 4, 0, 4],
        b'"' => [10, 10, 10, 0, 0, 0, 0],
        b'#' => [10, 31, 10, 10, 31, 10, 0],
        b'$' => [4, 15, 20, 14, 5, 30, 4],
        b'%' => [24, 25, 2, 4, 8, 19, 3],
        b'&' => [12, 18, 20, 8, 21, 18, 13],
        b'\'' => [4, 4, 8, 0, 0, 0, 0],
        b'(' => [2, 4, 8, 8, 8, 4, 2],
        b')' => [8, 4, 2, 2, 2, 4, 8],
        b'*' => [0, 21, 14, 31, 14, 21, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b',' => [0, 0, 0, 0, 0, 4, 8],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 0, 4],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [6, 8, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 2, 12],
        b':' => [0, 0, 4, 0, 4, 0, 0],
        b';' => [0, 0, 4, 0, 4, 4, 8],
        b'<' => [2, 4, 8, 16, 8, 4, 2],
        b'=' => [0, 0, 31, 0, 31, 0, 0],
        b'>' => [8, 4, 2, 1, 2, 4, 8],
        b'?' => [14, 17, 1, 2, 4, 0, 4],
        b'@' => [14, 17, 23, 21, 23, 16, 14],
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 2, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'[' => [14, 8, 8, 8, 8, 8, 14],
        b'\\' => [16, 8, 8, 4, 2, 2, 1],
        b']' => [14, 2, 2, 2, 2, 2, 14],
        b'^' => [4, 10, 17, 0, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 31],
        b'`' => [8, 4, 2, 0, 0, 0, 0],
        b'a' => [0, 0, 14, 1, 15, 17, 15],
        b'b' => [16, 16, 30, 17, 17, 17, 30],
        b'c' => [0, 0, 14, 17, 16, 17, 14],
        b'd' => [1, 1, 15, 17, 17, 17, 15],
        b'e' => [0, 0, 14, 17, 31, 16, 14],
        b'f' => [6, 9, 8, 28, 8, 8, 8],
        b'g' => [0, 15, 17, 17, 15, 1, 14],
        b'h' => [16, 16, 30, 17, 17, 17, 17],
        b'i' => [4, 0, 12, 4, 4, 4, 14],
        b'j' => [2, 0, 6, 2, 2, 18, 12],
        b'k' => [16, 16, 18, 20, 24, 20, 18],
        b'l' => [12, 4, 4, 4, 4, 4, 14],
        b'm' => [0, 0, 26, 21, 21, 21, 21],
        b'n' => [0, 0, 30, 17, 17, 17, 17],
        b'o' => [0, 0, 14, 17, 17, 17, 14],
        b'p' => [0, 0, 30, 17, 30, 16, 16],
        b'q' => [0, 0, 15, 17, 15, 1, 1],
        b'r' => [0, 0, 22, 25, 16, 16, 16],
        b's' => [0, 0, 15, 16, 14, 1, 30],
        b't' => [8, 8, 28, 8, 8, 9, 6],
        b'u' => [0, 0, 17, 17, 17, 19, 13],
        b'v' => [0, 0, 17, 17, 17, 10, 4],
        b'w' => [0, 0, 17, 17, 21, 21, 10],
        b'x' => [0, 0, 17, 10, 4, 10, 17],
        b'y' => [0, 0, 17, 17, 15, 1, 14],
        b'z' => [0, 0, 31, 2, 4, 8, 31],
        b'{' => [2, 4, 4, 8, 4, 4, 2],
        b'|' => [4, 4, 4, 4, 4, 4, 4],
        b'}' => [8, 4, 4, 2, 4, 4, 8],
        b'~' => [0, 0, 9, 22, 0, 0, 0],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn processing_budget_is_checked_before_allocating_a_work_grid() {
        for (width, height) in [
            (0, 1),
            (1, 0),
            (4097, 4096),
            (u32::MAX, u32::MAX),
            (30_001, 1),
        ] {
            assert!(validate_dimensions(width, height).is_err());
        }
        assert!(validate_dimensions(4096, 4096).is_ok());
        assert!(validate_dimensions(30_000, 1).is_ok());
    }
    #[test]
    fn bayer_matrices_have_every_threshold_once_with_the_expected_quadrants() {
        for (style, size) in [(Style::Bayer2, 2), (Style::Bayer4, 4), (Style::Bayer8, 8)] {
            let mut thresholds: Vec<_> = (0..size)
                .flat_map(|y| (0..size).map(move |x| bayer(style, x, y)))
                .collect();
            thresholds.sort_by(f32::total_cmp);
            for (index, threshold) in thresholds.iter().enumerate() {
                assert_eq!(*threshold, (index as f32 + 0.5) / (size * size) as f32);
            }
        }
        assert_eq!((bayer(Style::Bayer4, 1, 0) * 16.).floor(), 8.);
        assert_eq!((bayer(Style::Bayer4, 0, 1) * 16.).floor(), 12.);
    }
    #[test]
    fn hatch_coverage_grows_monotonically_and_all_ascii_glyphs_are_bounded() {
        let mut previous = 0;
        for step in 0..=64 {
            let count = (0..8)
                .flat_map(|y| (0..8).map(move |x| pattern(step as f32 / 64., x, y)))
                .filter(|v| *v)
                .count();
            assert_eq!(count, step);
            assert!(count >= previous);
            previous = count;
        }
        for c in 32..=126 {
            assert!(glyph(c).iter().all(|row| *row < 32));
        }
    }
}
