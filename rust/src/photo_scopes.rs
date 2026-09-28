//! Bounded, alpha-weighted display-referred scopes for the Camera Raw preview.
//!
//! Samples the centre of at most 512 × 512 image cells. This is an overview of
//! the selected layer's 8-bit RGB output, not a sensor-RAW or HDR measurement.
use image::RgbaImage;

pub const HISTOGRAM_BINS: usize = 256;
pub const VECTOR_SIZE: usize = 64;
pub const SAMPLE_SIDE: u32 = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhotoScopes {
    pub rgb: [[u32; HISTOGRAM_BINS]; 3],
    /// Hue runs counterclockwise from red at the right; saturation is radius.
    pub vectors: [u32; VECTOR_SIZE * VECTOR_SIZE],
    pub sampled_pixels: u32,
    pub visible_pixels: u32,
}

impl PhotoScopes {
    pub fn analyze(image: &RgbaImage) -> Self {
        let mut scopes = Self {
            rgb: [[0; HISTOGRAM_BINS]; 3],
            vectors: [0; VECTOR_SIZE * VECTOR_SIZE],
            sampled_pixels: 0,
            visible_pixels: 0,
        };
        let columns = image.width().min(SAMPLE_SIDE);
        let rows = image.height().min(SAMPLE_SIDE);
        for row in 0..rows {
            let y = sample_coordinate(row, rows, image.height());
            for column in 0..columns {
                let x = sample_coordinate(column, columns, image.width());
                scopes.sampled_pixels += 1;
                let pixel = image.get_pixel(x, y).0;
                let weight = u32::from(pixel[3]);
                if weight == 0 {
                    continue;
                }
                scopes.visible_pixels += 1;
                for (channel, &level) in pixel[..3].iter().enumerate() {
                    scopes.rgb[channel][usize::from(level)] += weight;
                }
                scopes.vectors[vector_bin(pixel[0], pixel[1], pixel[2])] += weight;
            }
        }
        scopes
    }

    /// Common scale across RGB channels, so their relative counts stay honest.
    pub fn histogram_peak(&self) -> u32 {
        self.rgb.iter().flatten().copied().max().unwrap_or(0)
    }

    pub fn vector_peak(&self) -> u32 {
        self.vectors.iter().copied().max().unwrap_or(0)
    }
}

fn sample_coordinate(index: u32, count: u32, extent: u32) -> u32 {
    // 64-bit arithmetic also keeps synthetic images near the u32 limit safe.
    (((2 * u64::from(index) + 1) * u64::from(extent)) / (2 * u64::from(count))) as u32
}

fn vector_bin(r: u8, g: u8, b: u8) -> usize {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let saturation = if max == 0. { 0. } else { delta / max };
    let hue = if delta == 0. {
        0.
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    } * std::f32::consts::PI
        / 3.;
    let centre = (VECTOR_SIZE - 1) as f32 / 2.;
    let x = (centre + centre * saturation * hue.cos())
        .round()
        .clamp(0., (VECTOR_SIZE - 1) as f32) as usize;
    let y = (centre - centre * saturation * hue.sin())
        .round()
        .clamp(0., (VECTOR_SIZE - 1) as f32) as usize;
    y * VECTOR_SIZE + x
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn scopes_ignore_transparent_colour_and_weight_partial_coverage() {
        let image =
            RgbaImage::from_raw(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 128, 25, 50, 75, 0]).unwrap();
        let before = image.clone();
        let scopes = PhotoScopes::analyze(&image);
        assert_eq!(image, before);
        assert_eq!((scopes.sampled_pixels, scopes.visible_pixels), (3, 2));
        assert_eq!(scopes.rgb[0][255], 255);
        assert_eq!(scopes.rgb[0][0], 128);
        assert_eq!(scopes.rgb[1][255], 128);
        assert_eq!(scopes.rgb[2][0], 383);
        for histogram in scopes.rgb {
            assert_eq!(histogram.iter().sum::<u32>(), 383);
        }
        assert_eq!(scopes.vectors.iter().sum::<u32>(), 383);
    }

    #[test]
    fn primary_colours_surround_neutral_in_hue_saturation_scope() {
        let neutral = vector_bin(128, 128, 128);
        assert_eq!(neutral, 32 * VECTOR_SIZE + 32);
        assert_eq!(vector_bin(0, 0, 0), neutral);
        assert_eq!(vector_bin(255, 255, 255), neutral);
        let red = vector_bin(255, 0, 0);
        let green = vector_bin(0, 255, 0);
        let blue = vector_bin(0, 0, 255);
        assert_eq!(red % VECTOR_SIZE, VECTOR_SIZE - 1);
        assert!(green / VECTOR_SIZE < 8 && green % VECTOR_SIZE < 20);
        assert!(blue / VECTOR_SIZE > 55 && blue % VECTOR_SIZE < 20);
        let muted = vector_bin(255, 128, 128);
        assert!(muted % VECTOR_SIZE > 32 && muted % VECTOR_SIZE < red % VECTOR_SIZE);
    }

    #[test]
    fn scope_work_is_bounded_and_samples_across_the_whole_image() {
        let image = RgbaImage::from_fn(1025, 1025, |x, _| {
            if x < 512 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let scopes = PhotoScopes::analyze(&image);
        assert_eq!(scopes.sampled_pixels, SAMPLE_SIDE * SAMPLE_SIDE);
        assert_eq!(scopes.visible_pixels, scopes.sampled_pixels);
        assert_eq!(scopes.rgb[0][0], 256 * 512 * 255);
        assert_eq!(scopes.rgb[0][255], 256 * 512 * 255);
        assert!(sample_coordinate(511, 512, u32::MAX) < u32::MAX);
    }

    #[test]
    fn empty_and_fully_transparent_images_have_no_false_signal() {
        for image in [RgbaImage::new(0, 0), RgbaImage::new(8, 8)] {
            let scopes = PhotoScopes::analyze(&image);
            assert_eq!(scopes.histogram_peak(), 0);
            assert_eq!(scopes.vector_peak(), 0);
            assert_eq!(scopes.visible_pixels, 0);
        }
    }
}
