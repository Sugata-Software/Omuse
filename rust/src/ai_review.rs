//! Local, bounded measurements of review previews; never a quality or full-image score.
use image::{Rgba, RgbaImage};

pub(super) const MAX_PREVIEW_EDGE: u32 = 640;
const VISIBLE_THRESHOLD: u16 = 8;

/// Fit both dimensions inside the review bound without upscaling. The raw
/// imageops thumbnail function resizes to the exact dimensions supplied, so
/// calculate the shared aspect-preserving dimensions before resampling.
pub(super) fn fit_preview(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= MAX_PREVIEW_EDGE || width == 0 || height == 0 {
        return image.clone();
    }
    let scaled = |dimension: u32| {
        ((u64::from(dimension) * u64::from(MAX_PREVIEW_EDGE) + u64::from(longest) / 2)
            / u64::from(longest))
        .max(1) as u32
    };
    image::imageops::thumbnail(image, scaled(width), scaled(height))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ReviewMode {
    #[default]
    Pair,
    Before,
    After,
    Changes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DifferenceSummary {
    pub changed_pixels: u32,
    pub total_pixels: u32,
    pub width: u32,
    pub height: u32,
    /// Inclusive bounds in preview pixels; absent for a visually unchanged preview.
    pub bounds: Option<[u32; 4]>,
}

impl DifferenceSummary {
    pub fn description(&self) -> String {
        if self.changed_pixels == 0 {
            return "No visible changes above the preview threshold.".into();
        }
        let percentage = 100. * f64::from(self.changed_pixels) / f64::from(self.total_pixels);
        let percentage = if percentage < 0.1 {
            "<0.1".to_owned()
        } else {
            format!("{percentage:.1}")
        };
        format!(
            "{percentage}% of this {} × {} preview changed.",
            self.width, self.height
        )
    }
}

pub(super) struct Difference {
    pub summary: DifferenceSummary,
    pub highlight: RgbaImage,
}

/// Do not align or stretch unrelated frames. Compare displayed appearance on both
/// black and white mattes so hidden RGB is ignored but alpha changes are counted.
/// This runs on the existing preparation worker, never in a GPUI render callback.
pub(super) fn preview_difference(
    before: &RgbaImage,
    after: &RgbaImage,
) -> Result<Difference, &'static str> {
    if before.dimensions() != after.dimensions() {
        return Err(
            "The preview framing changed. Compare the images side by side; a pixel difference would be misleading.",
        );
    }
    let (width, height) = before.dimensions();
    if width == 0 || height == 0 || width > MAX_PREVIEW_EDGE || height > MAX_PREVIEW_EDGE {
        return Err("A bounded preview is required for visual change highlighting.");
    }
    let mut highlight = RgbaImage::new(width, height);
    let mut changed_pixels = 0;
    let mut bounds: Option<[u32; 4]> = None;
    for (index, (left, right)) in before.pixels().zip(after.pixels()).enumerate() {
        let mut delta = 0u16;
        for channel in 0..3 {
            for matte in [0u32, 255u32] {
                let visible = |p: &Rgba<u8>| -> u16 {
                    ((u32::from(p[channel]) * u32::from(p[3])
                        + matte * (255 - u32::from(p[3]))
                        + 127)
                        / 255) as u16
                };
                delta = delta.max(visible(left).abs_diff(visible(right)));
            }
        }
        let x = index as u32 % width;
        let y = index as u32 / width;
        let value = if delta > VISIBLE_THRESHOLD {
            changed_pixels += 1;
            bounds = Some(match bounds {
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                None => [x, y, x, y],
            });
            (delta * 4).clamp(80, 255) as u8
        } else {
            0
        };
        highlight.put_pixel(x, y, Rgba([value, value, value, 255]));
    }
    Ok(Difference {
        summary: DifferenceSummary {
            changed_pixels,
            total_pixels: width * height,
            width,
            height,
            bounds,
        },
        highlight,
    })
}

pub(super) fn concise(text: &str, limit: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = normalized.chars();
    let head: String = characters.by_ref().take(limit).collect();
    if characters.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn landscape_and_portrait_previews_fit_without_distortion() {
        for (width, height, expected) in [
            (1280, 960, (640, 480)),
            (960, 1280, (480, 640)),
            (1920, 1080, (640, 360)),
            (1080, 1920, (360, 640)),
            (640, 480, (640, 480)),
        ] {
            let source = RgbaImage::from_pixel(width, height, Rgba([25, 50, 75, 255]));
            let preview = fit_preview(&source);
            assert_eq!(preview.dimensions(), expected);
            assert!(preview.pixels().all(|pixel| pixel.0 == [25, 50, 75, 255]));
        }
    }
    #[test]
    fn small_previews_keep_original_dimensions_and_exact_pixels() {
        let source = RgbaImage::from_fn(19, 37, |x, y| Rgba([x as u8, y as u8, 90, (x + y) as u8]));
        assert_eq!(fit_preview(&source), source);
    }
    #[test]
    fn rectangular_before_after_previews_and_change_locations_align() {
        let before = RgbaImage::from_pixel(1280, 960, Rgba([0, 0, 0, 255]));
        let mut after = before.clone();
        for y in 200..600 {
            for x in 400..800 {
                after.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let left = fit_preview(&before);
        let right = fit_preview(&after);
        assert_eq!(left.dimensions(), (640, 480));
        assert_eq!(left.dimensions(), right.dimensions());
        let difference = preview_difference(&left, &right).unwrap();
        assert_eq!(difference.summary.bounds, Some([200, 100, 399, 299]));
        assert_eq!(difference.summary.changed_pixels, 40_000);
        assert_eq!(difference.highlight.dimensions(), (640, 480));
    }
    #[test]
    fn hidden_rgb_is_not_a_visual_change() {
        let before = RgbaImage::from_pixel(3, 2, Rgba([255, 0, 0, 0]));
        let after = RgbaImage::from_pixel(3, 2, Rgba([0, 255, 255, 0]));
        let result = preview_difference(&before, &after).unwrap();
        assert_eq!(result.summary.changed_pixels, 0);
        assert_eq!(result.summary.bounds, None);
        assert!(result.highlight.pixels().all(|p| p.0 == [0, 0, 0, 255]));
    }
    #[test]
    fn alpha_and_colour_changes_have_exact_preview_bounds() {
        let before = RgbaImage::from_pixel(4, 3, Rgba([40, 50, 60, 255]));
        let mut after = before.clone();
        after.put_pixel(1, 1, Rgba([40, 50, 60, 0]));
        after.put_pixel(3, 2, Rgba([90, 50, 60, 255]));
        let result = preview_difference(&before, &after).unwrap();
        assert_eq!(result.summary.changed_pixels, 2);
        assert_eq!(result.summary.total_pixels, 12);
        assert_eq!(result.summary.bounds, Some([1, 1, 3, 2]));
        assert_eq!(result.highlight.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert!(result.highlight.get_pixel(1, 1)[0] >= 80);
    }
    #[test]
    fn threshold_is_explicit_and_dark_alpha_edges_are_measured() {
        let before = RgbaImage::from_pixel(2, 1, Rgba([0, 0, 0, 255]));
        let mut after = before.clone();
        after.put_pixel(0, 0, Rgba([8, 0, 0, 255]));
        after.put_pixel(1, 0, Rgba([0, 0, 0, 246]));
        let result = preview_difference(&before, &after).unwrap();
        assert_eq!(result.summary.changed_pixels, 1);
        assert_eq!(result.summary.bounds, Some([1, 0, 1, 0]));
    }
    #[test]
    fn different_framing_and_oversized_previews_are_not_resampled() {
        let small = RgbaImage::new(2, 1);
        assert!(preview_difference(&small, &RgbaImage::new(1, 2)).is_err());
        assert!(preview_difference(&RgbaImage::new(0, 0), &RgbaImage::new(0, 0)).is_err());
        let large = RgbaImage::new(MAX_PREVIEW_EDGE + 1, 1);
        assert!(preview_difference(&large, &large).is_err());
    }
    #[test]
    fn concise_summary_preserves_unicode_and_collapses_whitespace() {
        assert_eq!(concise("  Café\n猫   changed  ", 6), "Café 猫…");
        assert_eq!(concise("two   words", 30), "two words");
    }
}
