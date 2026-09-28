//! Subject-mask refinement, following SubjectRemoval.swift and GuidedMatte.swift.
use anyhow::{Result, ensure};
use image::{GrayImage, Luma, RgbaImage, imageops};
#[derive(Clone, Copy, Debug, Default)]
pub struct Settings {
    pub refine: f32,
    pub contrast: f32,
    pub shift: f32,
}
fn box_mean(source: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut pass = vec![0.; w * h];
    let mut out = vec![0.; w * h];
    let span = (r * 2 + 1) as f32;
    for y in 0..h {
        let mut sum = 0.;
        for k in -(r as isize)..=r as isize {
            sum += source[y * w + k.clamp(0, w as isize - 1) as usize];
        }
        for x in 0..w {
            pass[y * w + x] = sum / span;
            sum -= source[y * w + (x as isize - r as isize).clamp(0, w as isize - 1) as usize];
            sum += source[y * w + (x + r + 1).min(w - 1)];
        }
    }
    for x in 0..w {
        let mut sum = 0.;
        for k in -(r as isize)..=r as isize {
            sum += pass[k.clamp(0, h as isize - 1) as usize * w + x];
        }
        for y in 0..h {
            out[y * w + x] = sum / span;
            sum -= pass[(y as isize - r as isize).clamp(0, h as isize - 1) as usize * w + x];
            sum += pass[(y + r + 1).min(h - 1) * w + x];
        }
    }
    out
}
fn guided(mask: &GrayImage, guide: &RgbaImage, radius: f32) -> GrayImage {
    let factor = (1400. / mask.width().max(mask.height()) as f32).min(1.);
    let w = (mask.width() as f32 * factor).round().max(1.) as u32;
    let h = (mask.height() as f32 * factor).round().max(1.) as u32;
    let small = imageops::resize(mask, w, h, imageops::FilterType::Triangle);
    let color = imageops::resize(guide, w, h, imageops::FilterType::Triangle);
    let guide: Vec<f32> = color
        .pixels()
        .map(|p| (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.)
        .collect();
    let p: Vec<f32> = small.pixels().map(|p| p[0] as f32 / 255.).collect();
    let (w, h) = (w as usize, h as usize);
    let r = (radius * factor).round().max(1.) as usize;
    let mean_i = box_mean(&guide, w, h, r);
    let mean_p = box_mean(&p, w, h, r);
    let corr_i = box_mean(&guide.iter().map(|v| v * v).collect::<Vec<_>>(), w, h, r);
    let corr_ip = box_mean(
        &guide.iter().zip(&p).map(|(i, p)| i * p).collect::<Vec<_>>(),
        w,
        h,
        r,
    );
    let a: Vec<f32> = (0..w * h)
        .map(|n| (corr_ip[n] - mean_i[n] * mean_p[n]) / (corr_i[n] - mean_i[n] * mean_i[n] + 1e-4))
        .collect();
    let b: Vec<f32> = (0..w * h).map(|n| mean_p[n] - a[n] * mean_i[n]).collect();
    let a = box_mean(&a, w, h, r);
    let b = box_mean(&b, w, h, r);
    let out = GrayImage::from_fn(w as u32, h as u32, |x, y| {
        let n = y as usize * w + x as usize;
        Luma([((a[n] * guide[n] + b[n]).clamp(0., 1.) * 255.).round() as u8])
    });
    imageops::resize(
        &out,
        mask.width(),
        mask.height(),
        imageops::FilterType::Triangle,
    )
}
pub fn refine(mask: &GrayImage, guide: &RgbaImage, settings: Settings) -> Result<GrayImage> {
    ensure!(
        mask.dimensions() == guide.dimensions()
            && crate::model::valid_dimensions(mask.width(), mask.height()),
        "Matte dimensions do not match the image"
    );
    ensure!(
        u64::from(mask.width()) * u64::from(mask.height()) <= 16_777_216,
        "Matte refinement supports images up to 16 million pixels"
    );
    ensure!(
        settings.refine.is_finite()
            && (0.0..=100.).contains(&settings.refine)
            && settings.contrast.is_finite()
            && (0.0..=100.).contains(&settings.contrast)
            && settings.shift.is_finite()
            && (-100.0..=100.).contains(&settings.shift),
        "Invalid matte settings"
    );
    let mut result = if settings.refine > 0. {
        guided(mask, guide, settings.refine)
    } else {
        mask.clone()
    };
    if settings.shift != 0. {
        result = imageops::blur(&result, settings.shift.abs() / 2.);
        let level = if settings.shift < 0. { 0.75 } else { 0.25 };
        for p in result.pixels_mut() {
            p[0] = ((p[0] as f32 / 255. - level) * 1000.)
                .clamp(0., 1.)
                .mul_add(255., 0.)
                .round() as u8;
        }
    }
    if settings.contrast > 0. {
        let slope = 1. / (1. - settings.contrast / 100. * 0.98).max(0.02);
        for p in result.pixels_mut() {
            p[0] = ((p[0] as f32 / 255. - 0.5) * slope + 0.5)
                .clamp(0., 1.)
                .mul_add(255., 0.)
                .round() as u8;
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn box_matches_bruteforce_edge_extension() {
        let p: Vec<f32> = (0..35).map(|i| i as f32 / 35.).collect();
        let out = box_mean(&p, 7, 5, 3);
        for y in 0..5 {
            for x in 0..7 {
                let mut sum = 0.;
                for dy in -3..=3 {
                    for dx in -3..=3 {
                        sum += p[(y + dy).clamp(0, 4) as usize * 7 + (x + dx).clamp(0, 6) as usize];
                    }
                }
                assert!((out[y as usize * 7 + x as usize] - sum / 49.).abs() < 1e-6);
            }
        }
    }
    #[test]
    fn refinement_preserves_flat_matte_and_dimensions() {
        let m = GrayImage::from_pixel(32, 24, Luma([128]));
        let g = RgbaImage::from_fn(32, 24, |x, _| {
            image::Rgba([if x < 16 { 0 } else { 255 }, 0, 0, 255])
        });
        let output = refine(
            &m,
            &g,
            Settings {
                refine: 8.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(output, m);
        assert!(
            refine(
                &m,
                &g,
                Settings {
                    refine: f32::NAN,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn shift_grows_and_shrinks() {
        let m = GrayImage::from_fn(40, 40, |x, y| {
            Luma([if (12..28).contains(&x) && (12..28).contains(&y) {
                255
            } else {
                0
            }])
        });
        let g = RgbaImage::new(40, 40);
        let count = |m: &GrayImage| m.pixels().filter(|p| p[0] > 128).count();
        assert!(
            count(
                &refine(
                    &m,
                    &g,
                    Settings {
                        shift: 4.,
                        ..Default::default()
                    }
                )
                .unwrap()
            ) > count(&m)
        );
        assert!(
            count(
                &refine(
                    &m,
                    &g,
                    Settings {
                        shift: -4.,
                        ..Default::default()
                    }
                )
                .unwrap()
            ) < count(&m)
        );
    }
}
