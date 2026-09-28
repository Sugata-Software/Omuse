//! Bounded tonal and RGB range masks.
//!
//! The mask values are coverage values in the range `0..=255`. Tonal values
//! use display-referred sRGB luma; this kernel does not linearize channels and
//! makes no perceptual-color claim about its RGB distance.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};
use image::{GrayImage, RgbaImage};

const MAX_PIXELS: u64 = 16_777_216;
const INV_SQRT_3: f32 = 0.577_350_26;

/// The range to select from a source image.
///
/// All scalar parameters are normalized to `0..=1`. `low` and `high` are the
/// fully selected luminosity interval. Outside that interval, `feather`
/// specifies the width of a smoothstep falloff on each side. Luminosity is
/// display-referred sRGB luma, using `0.2126/0.7152/0.0722`; it is not linear
/// luminance.
///
/// `Color` measures ordinary Euclidean distance between normalized 8-bit RGB
/// channels, divided by `sqrt(3)`. It is a channel-space distance and carries
/// no perceptual color claim. `tolerance` is the fully selected radius and
/// `feather` is the smoothstep falloff width beyond that radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RangeKind {
    Luminosity {
        low: f32,
        high: f32,
        feather: f32,
    },
    Color {
        rgb: [u8; 3],
        tolerance: f32,
        feather: f32,
    },
}

/// Settings for producing a bounded range mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeSettings {
    pub kind: RangeKind,
    pub invert: bool,
}

/// Validate normalized range settings.
pub fn validate(settings: RangeSettings) -> Result<()> {
    match settings.kind {
        RangeKind::Luminosity { low, high, feather } => {
            ensure!(
                low.is_finite()
                    && high.is_finite()
                    && feather.is_finite()
                    && (0.0..=1.0).contains(&low)
                    && (0.0..=1.0).contains(&high)
                    && (0.0..=1.0).contains(&feather),
                "invalid luminosity range settings"
            );
            ensure!(low <= high, "luminosity lower bound exceeds upper bound");
        }
        RangeKind::Color {
            tolerance, feather, ..
        } => {
            ensure!(
                tolerance.is_finite()
                    && feather.is_finite()
                    && (0.0..=1.0).contains(&tolerance)
                    && (0.0..=1.0).contains(&feather),
                "invalid color range settings"
            );
        }
    }
    Ok(())
}

/// Build a range mask using a non-cancellable operation.
pub fn mask(source: &RgbaImage, settings: RangeSettings) -> Result<GrayImage> {
    let cancelled = AtomicBool::new(false);
    mask_cancellable(source, settings, &cancelled)
}

/// Build a range mask, checking `cancelled` before processing each row.
///
/// Cancellation returns an error and discards the private output, so callers
/// never observe a partially written mask. Source alpha is applied once after
/// inversion; therefore transparent pixels remain unselected even for an
/// inverted range.
pub fn mask_cancellable(
    source: &RgbaImage,
    settings: RangeSettings,
    cancelled: &AtomicBool,
) -> Result<GrayImage> {
    validate(settings)?;
    let (width, height) = source.dimensions();
    ensure!(
        width != 0 && height != 0,
        "range mask requires nonzero dimensions"
    );
    ensure!(
        crate::model::valid_dimensions(width, height),
        "range mask dimensions exceed the Omuse limits"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "range mask supports at most 16,777,216 pixels"
    );
    ensure!(!cancelled.load(Ordering::Relaxed), "range mask cancelled");

    let width_usize = width as usize;
    let row_bytes = width_usize * 4;
    let mut output = GrayImage::new(width, height);
    let source_rows = source.as_raw().chunks_exact(row_bytes);
    let output_bytes: &mut [u8] = output.as_mut();
    let output_rows = output_bytes.chunks_exact_mut(width_usize);

    for (source_row, output_row) in source_rows.zip(output_rows) {
        ensure!(!cancelled.load(Ordering::Relaxed), "range mask cancelled");
        for (source_pixel, output_pixel) in source_row.chunks_exact(4).zip(output_row.iter_mut()) {
            let [red, green, blue, alpha] = [
                source_pixel[0],
                source_pixel[1],
                source_pixel[2],
                source_pixel[3],
            ];
            let mut coverage = match settings.kind {
                RangeKind::Luminosity { low, high, feather } => {
                    luminosity_coverage(red, green, blue, low, high, feather)
                }
                RangeKind::Color {
                    rgb,
                    tolerance,
                    feather,
                } => color_coverage(red, green, blue, rgb, tolerance, feather),
            };
            if settings.invert {
                coverage = 1.0 - coverage;
            }
            *output_pixel = (coverage.clamp(0.0, 1.0) * f32::from(alpha)).round() as u8;
        }
    }
    ensure!(!cancelled.load(Ordering::Relaxed), "range mask cancelled");
    Ok(output)
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn luminosity_coverage(red: u8, green: u8, blue: u8, low: f32, high: f32, feather: f32) -> f32 {
    // Express the weighted sum around green so an equal-channel pixel is
    // exactly `channel / 255`, including at a zero-width equal-tone range.
    let green = f32::from(green);
    let luminosity =
        (green + 0.2126 * (f32::from(red) - green) + 0.0722 * (f32::from(blue) - green)) / 255.0;
    if luminosity < low {
        if feather == 0.0 {
            0.0
        } else {
            smoothstep((luminosity - (low - feather)) / feather)
        }
    } else if luminosity > high {
        if feather == 0.0 {
            0.0
        } else {
            smoothstep(((high + feather) - luminosity) / feather)
        }
    } else {
        1.0
    }
}

fn color_coverage(
    red: u8,
    green: u8,
    blue: u8,
    target: [u8; 3],
    tolerance: f32,
    feather: f32,
) -> f32 {
    let red_delta = (f32::from(red) - f32::from(target[0])) / 255.0;
    let green_delta = (f32::from(green) - f32::from(target[1])) / 255.0;
    let blue_delta = (f32::from(blue) - f32::from(target[2])) / 255.0;
    let distance = (red_delta.mul_add(
        red_delta,
        green_delta.mul_add(green_delta, blue_delta * blue_delta),
    ))
    .sqrt()
        * INV_SQRT_3;
    if distance <= tolerance {
        1.0
    } else if feather == 0.0 {
        0.0
    } else {
        smoothstep((tolerance + feather - distance) / feather)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn gray_ramp(values: &[u8]) -> RgbaImage {
        RgbaImage::from_fn(values.len() as u32, 1, |x, _| {
            let value = values[x as usize];
            Rgba([value, value, value, 255])
        })
    }

    fn mask_values(mask: &GrayImage) -> Vec<u8> {
        mask.as_raw().clone()
    }

    #[test]
    fn luminosity_ramp_has_smooth_analytic_transitions() {
        let source = gray_ramp(&[0, 32, 64, 96, 128, 160, 192, 224, 255]);
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: 0.4,
                    high: 0.7,
                    feather: 0.2,
                },
                invert: false,
            },
        )
        .unwrap();
        let values = mask_values(&output);
        assert_eq!(values[4], 255);
        assert_eq!(values[5], 255);
        assert!(values[2] < values[3]);
        assert!(values[3] < values[4]);
        assert!(values[5] > values[6]);
        assert!(values[6] > values[7]);
        assert_eq!(values[0], 0);
        assert_eq!(values[8], 0);
    }

    #[test]
    fn equal_tone_bounds_select_only_the_equal_tone() {
        let source = gray_ramp(&[127, 128, 129]);
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: 128.0 / 255.0,
                    high: 128.0 / 255.0,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&output), vec![0, 255, 0]);
    }

    #[test]
    fn equal_tone_bounds_cover_every_grayscale_byte() {
        for value in 0..=255u8 {
            let source = gray_ramp(&[value]);
            let output = mask(
                &source,
                RangeSettings {
                    kind: RangeKind::Luminosity {
                        low: f32::from(value) / 255.0,
                        high: f32::from(value) / 255.0,
                        feather: 0.0,
                    },
                    invert: false,
                },
            )
            .unwrap();
            assert_eq!(output.as_raw(), &[255], "grayscale byte {value}");
        }
    }

    #[test]
    fn zero_feather_is_a_sharp_luminosity_boundary() {
        let source = gray_ramp(&[100, 128, 150]);
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: 100.0 / 255.0,
                    high: 150.0 / 255.0,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&output), vec![255, 255, 255]);
        let outside = gray_ramp(&[99, 151]);
        let output = mask(
            &outside,
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: 100.0 / 255.0,
                    high: 150.0 / 255.0,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&output), vec![0, 0]);
    }

    #[test]
    fn color_distance_is_normalized_and_has_inner_outer_values() {
        let source =
            RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 128, 0, 0, 255, 255, 0, 0, 255]).unwrap();
        let inside = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [0, 0, 0],
                    tolerance: 0.4,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&inside), vec![255, 255, 0]);
        let outside = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [0, 0, 0],
                    tolerance: 0.2,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&outside), vec![255, 0, 0]);
    }

    #[test]
    fn zero_tolerance_selects_only_an_exact_color_without_feather() {
        let source =
            RgbaImage::from_raw(3, 1, vec![10, 20, 30, 255, 11, 20, 30, 255, 0, 0, 0, 255])
                .unwrap();
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [10, 20, 30],
                    tolerance: 0.0,
                    feather: 0.0,
                },
                invert: false,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&output), vec![255, 0, 0]);
    }

    #[test]
    fn color_falloff_is_monotonic() {
        let source = RgbaImage::from_fn(6, 1, |x, _| {
            let value = [0, 32, 64, 96, 128, 255][x as usize];
            Rgba([value, 0, 0, 255])
        });
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [0, 0, 0],
                    tolerance: 0.05,
                    feather: 0.5,
                },
                invert: false,
            },
        )
        .unwrap();
        let values = mask_values(&output);
        assert!(values.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(values[0], 255);
        assert_eq!(values[5], 0);
    }

    #[test]
    fn inversion_and_alpha_gate_coverage_once() {
        let source = RgbaImage::from_raw(
            4,
            1,
            vec![10, 20, 30, 255, 10, 20, 30, 128, 0, 0, 0, 255, 0, 0, 0, 0],
        )
        .unwrap();
        let output = mask(
            &source,
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [10, 20, 30],
                    tolerance: 0.0,
                    feather: 0.0,
                },
                invert: true,
            },
        )
        .unwrap();
        assert_eq!(mask_values(&output), vec![0, 0, 255, 0]);
    }

    #[test]
    fn malformed_settings_are_rejected_without_touching_input() {
        let source = gray_ramp(&[10, 20, 30]);
        let original = source.clone();
        let invalid = [
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: f32::NAN,
                    high: 0.5,
                    feather: 0.0,
                },
                invert: false,
            },
            RangeSettings {
                kind: RangeKind::Luminosity {
                    low: 0.8,
                    high: 0.2,
                    feather: 0.0,
                },
                invert: false,
            },
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [0, 0, 0],
                    tolerance: 1.1,
                    feather: 0.0,
                },
                invert: false,
            },
            RangeSettings {
                kind: RangeKind::Color {
                    rgb: [0, 0, 0],
                    tolerance: 0.0,
                    feather: f32::INFINITY,
                },
                invert: false,
            },
        ];
        for settings in invalid {
            assert!(mask(&source, settings).is_err());
            assert_eq!(source, original);
        }
    }

    #[test]
    fn invalid_dimensions_and_cancellation_are_rejected() {
        let empty = RgbaImage::new(0, 1);
        assert!(
            mask(
                &empty,
                RangeSettings {
                    kind: RangeKind::Color {
                        rgb: [0, 0, 0],
                        tolerance: 0.0,
                        feather: 0.0,
                    },
                    invert: false,
                }
            )
            .is_err()
        );

        let oversized_axis = RgbaImage::new(30_001, 1);
        assert!(
            mask(
                &oversized_axis,
                RangeSettings {
                    kind: RangeKind::Color {
                        rgb: [0, 0, 0],
                        tolerance: 0.0,
                        feather: 0.0,
                    },
                    invert: false,
                }
            )
            .is_err()
        );

        let source = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        let cancelled = AtomicBool::new(true);
        assert!(
            mask_cancellable(
                &source,
                RangeSettings {
                    kind: RangeKind::Color {
                        rgb: [1, 2, 3],
                        tolerance: 0.0,
                        feather: 0.0,
                    },
                    invert: false,
                },
                &cancelled,
            )
            .is_err()
        );
    }
}
