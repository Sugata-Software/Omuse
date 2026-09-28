//! High precision kernels for editable retouching, denoising and deformation.
use crate::{
    advanced_ops::{self, AdvancedOperation, DodgeBurnMode},
    filters::Filter,
    precision::{Rgba16, TiledImage16},
};
use anyhow::{Result, ensure};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn evaluate(
    source: &TiledImage16,
    operation: &AdvancedOperation,
    cancel: &AtomicBool,
) -> Result<Option<TiledImage16>> {
    advanced_ops::validate_operation_dimensions(source.dimensions(), operation)?;
    check(cancel)?;
    let mut output = source.clone();
    match operation {
        AdvancedOperation::CameraRaw(_) | AdvancedOperation::Filter(_) => return Ok(None),
        AdvancedOperation::Denoise { radius, strength } => {
            if *radius == 0 || *strength == 0. {
                return Ok(Some(output));
            }
            let soft = source.filtered_with_cancel(
                &Filter::GaussianBlur {
                    sigma: f32::from(*radius) / 2.,
                },
                || cancel.load(Ordering::Relaxed),
            )?;
            for y in 0..source.height() {
                check(cancel)?;
                for x in 0..source.width() {
                    let old = source.get_pixel(x, y).0;
                    let smooth = soft.get_pixel(x, y).0;
                    let amount =
                        *strength * (1. - ((luma(old) - luma(smooth)).abs() * 8.).clamp(0., 1.));
                    let mut p = old;
                    for c in 0..3 {
                        p[c] = word(
                            f64::from(old[c])
                                + (f64::from(smooth[c]) - f64::from(old[c])) * f64::from(amount),
                        );
                    }
                    output.set_pixel(x, y, Rgba16(p))?;
                }
            }
        }
        AdvancedOperation::DodgeBurn(settings) => {
            let soft = source.filtered_with_cancel(
                &Filter::GaussianBlur {
                    sigma: f32::from(settings.radius) / 2.,
                },
                || cancel.load(Ordering::Relaxed),
            )?;
            for y in 0..source.height() {
                check(cancel)?;
                for x in 0..source.width() {
                    let local = luma(soft.get_pixel(x, y).0);
                    let factor = match settings.mode {
                        DodgeBurnMode::Dodge => 1. + settings.amount * (1. - local),
                        DodgeBurnMode::Burn => 1. - settings.amount * local,
                    };
                    let mut p = source.get_pixel(x, y).0;
                    for c in 0..3 {
                        p[c] = word(f64::from(p[c]) * f64::from(factor));
                    }
                    output.set_pixel(x, y, Rgba16(p))?;
                }
            }
        }
        AdvancedOperation::FrequencySeparation(settings) => {
            let low = source.filtered_with_cancel(
                &Filter::GaussianBlur {
                    sigma: f32::from(settings.radius) / 2.,
                },
                || cancel.load(Ordering::Relaxed),
            )?;
            for y in 0..source.height() {
                check(cancel)?;
                for x in 0..source.width() {
                    let mut p = source.get_pixel(x, y).0;
                    let soft = low.get_pixel(x, y).0;
                    for c in 0..3 {
                        p[c] = word(
                            f64::from(soft[c]) * (1. + f64::from(settings.low_amount))
                                + (f64::from(p[c]) - f64::from(soft[c]))
                                    * f64::from(settings.high_amount),
                        );
                    }
                    output.set_pixel(x, y, Rgba16(p))?;
                }
            }
        }
        AdvancedOperation::Warp(warp) => {
            let (w, h) = source.dimensions();
            for y in 0..h {
                check(cancel)?;
                for x in 0..w {
                    let p = [
                        x as f32 / w.saturating_sub(1).max(1) as f32,
                        y as f32 / h.saturating_sub(1).max(1) as f32,
                    ];
                    let freeze = warp.freeze_mask.as_ref().map_or(0., |m| m.value(x, y));
                    let protect = warp.protect_mask.as_ref().map_or(0., |m| m.value(x, y));
                    if freeze == 1. || protect == 1. {
                        continue;
                    }
                    let mut at = p;
                    for _ in 0..8 {
                        let displacement = advanced_ops::warp_displacement(at, warp);
                        at = [
                            p[0] - displacement[0] * (1. - freeze),
                            p[1] - displacement[1] * (1. - freeze),
                        ];
                    }
                    let value = sample(
                        source,
                        at[0] * w.saturating_sub(1) as f32,
                        at[1] * h.saturating_sub(1) as f32,
                    );
                    output.set_pixel(
                        x,
                        y,
                        Rgba16(mix(source.get_pixel(x, y).0, value, 1. - protect)),
                    )?;
                }
            }
        }
        AdvancedOperation::ContentAwareReplace(settings) => {
            // An 8-bit preview selects coordinates. Pixel copying and feather
            // interpolation use the untouched 16-bit source at those positions.
            let matching = source.to_rgba8_in(crate::precision::WorkingSpace::Srgb)?;
            advanced_ops::visit_content_samples(
                &matching,
                settings,
                cancel,
                |x, y, sx, sy, amount| {
                    output.set_pixel(
                        x,
                        y,
                        Rgba16(mix(
                            source.get_pixel(x, y).0,
                            source.get_pixel(sx, sy).0,
                            amount,
                        )),
                    )
                },
            )?;
        }
        AdvancedOperation::BlendIf(settings) => {
            let proxy = source.to_rgba8_in(crate::precision::WorkingSpace::Srgb)?;
            for y in 0..source.height() {
                check(cancel)?;
                for x in 0..source.width() {
                    let byte = proxy.get_pixel(x, y).0;
                    let amount = advanced_ops::blend_if_coverage(byte, byte, settings);
                    let mut p = source.get_pixel(x, y).0;
                    p[3] = word(f64::from(p[3]) * f64::from(amount));
                    output.set_pixel(x, y, Rgba16(p))?;
                }
            }
        }
    }
    check(cancel)?;
    Ok(Some(output))
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Advanced edit cancelled");
    Ok(())
}
fn word(v: f64) -> u16 {
    v.round().clamp(0., 65535.) as u16
}
fn luma(p: [u16; 4]) -> f32 {
    (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 65535.
}

fn mix(a: [u16; 4], b: [u16; 4], t: f32) -> [u16; 4] {
    if t <= 0. {
        return a;
    }
    if t >= 1. {
        return b;
    }
    let t = f64::from(t);
    let aa = f64::from(a[3]) / 65535.;
    let ba = f64::from(b[3]) / 65535.;
    let alpha = aa * (1. - t) + ba * t;
    let mut out = [0; 4];
    if alpha > 0. {
        for c in 0..3 {
            out[c] = word((f64::from(a[c]) * aa * (1. - t) + f64::from(b[c]) * ba * t) / alpha);
        }
    }
    out[3] = word(alpha * 65535.);
    out
}
fn sample(source: &TiledImage16, x: f32, y: f32) -> [u16; 4] {
    let x = x.clamp(0., source.width().saturating_sub(1) as f32);
    let y = y.clamp(0., source.height().saturating_sub(1) as f32);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(source.width() - 1),
        (y0 + 1).min(source.height() - 1),
    );
    let a = mix(
        source.get_pixel(x0, y0).0,
        source.get_pixel(x1, y0).0,
        x.fract(),
    );
    let b = mix(
        source.get_pixel(x0, y1).0,
        source.get_pixel(x1, y1).0,
        x.fract(),
    );
    mix(a, b, y.fract())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advanced_ops::{ContentAwareReplace, SoftMask, WarpMesh};
    fn master() -> TiledImage16 {
        TiledImage16::from_rgba16(&image::ImageBuffer::from_fn(7, 5, |x, y| {
            image::Rgba([1001 + x as u16 * 17, 13001 + y as u16 * 23, 33003, 65535])
        }))
        .unwrap()
    }
    #[test]
    fn identity_and_frozen_warps_keep_exact_sub_byte_values() {
        let source = master();
        let mut warp = WarpMesh {
            columns: 2,
            rows: 2,
            points: vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]],
            pins: vec![],
            freeze_mask: None,
            protect_mask: None,
        };
        let result = evaluate(
            &source,
            &AdvancedOperation::Warp(warp.clone()),
            &AtomicBool::new(false),
        )
        .unwrap()
        .unwrap();
        assert_eq!(source.to_rgba16(), result.to_rgba16());
        warp.points[0] = [0.4, 0.2];
        warp.freeze_mask = Some(SoftMask::new(7, 5, vec![255; 35]).unwrap());
        let result = evaluate(
            &source,
            &AdvancedOperation::Warp(warp),
            &AtomicBool::new(false),
        )
        .unwrap()
        .unwrap();
        assert_eq!(source.to_rgba16(), result.to_rgba16());
    }
    #[test]
    fn denoise_constant_plane_keeps_sixteen_bit_detail_and_alpha() {
        let pixels =
            image::ImageBuffer::from_pixel(5, 5, image::Rgba([13001, 22002, 33003, 45555]));
        let source = TiledImage16::from_rgba16(&pixels).unwrap();
        let result = evaluate(
            &source,
            &AdvancedOperation::Denoise {
                radius: 4,
                strength: 1.,
            },
            &AtomicBool::new(false),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.to_rgba16(), pixels);
        assert!(
            evaluate(
                &source,
                &AdvancedOperation::Denoise {
                    radius: 4,
                    strength: 1.
                },
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }
    #[test]
    fn removal_copies_exact_source_even_at_far_edge_of_allowed_search() {
        let pixels = image::ImageBuffer::from_fn(70, 30, |x, y| {
            image::Rgba([1001 + x as u16 * 17, 13001 + y as u16 * 23, 33003, 65535])
        });
        let source = TiledImage16::from_rgba16(&pixels).unwrap();
        let mut target = vec![0; 2100];
        target[10 * 70 + 20] = 255;
        let mut allowed = vec![0; 2100];
        allowed[29 * 70 + 69] = 255;
        let settings = ContentAwareReplace {
            target_mask: SoftMask::new(70, 30, target).unwrap(),
            allowed_source_mask: SoftMask::new(70, 30, allowed).unwrap(),
            search_radius: 64,
            patch_radius: 1,
            feather: 0.,
        };
        let result = evaluate(
            &source,
            &AdvancedOperation::ContentAwareReplace(settings),
            &AtomicBool::new(false),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.get_pixel(20, 10), source.get_pixel(69, 29));
        for y in 0..30 {
            for x in 0..70 {
                if (x, y) != (20, 10) {
                    assert_eq!(result.get_pixel(x, y), source.get_pixel(x, y));
                }
            }
        }
    }
}
