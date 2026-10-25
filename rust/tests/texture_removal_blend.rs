//! Independent retained-word/alpha oracle for TextureV2 soft coverage. Donor
//! fields are constant, so expected samples do not depend on search tie order.
use image::Rgba;
use omuse::{
    advanced_ops::{AdvancedOperation, ContentAwareAlgorithm, ContentAwareReplace, SoftMask},
    advanced16,
    precision::{Rgba16Image, TiledImage16, WorkingSpace},
    retouch::texture,
};
use std::sync::atomic::AtomicBool;

/// Straight retained words represent colour plus coverage. Interpolate the
/// associated colour numerators and alpha weights, then unassociate once.
/// This is deliberately independent of advanced16's private mixing helpers.
fn expected_blend(old: [u16; 4], donor: [u16; 4], coverage: f64) -> [u16; 4] {
    let old_weight = f64::from(old[3]) * (1. - coverage);
    let donor_weight = f64::from(donor[3]) * coverage;
    let weight = old_weight + donor_weight;
    let mut result = [0; 4];
    if weight > 0. {
        for channel in 0..3 {
            result[channel] = ((f64::from(old[channel]) * old_weight
                + f64::from(donor[channel]) * donor_weight)
                / weight)
                .round() as u16;
        }
    }
    result[3] = weight.round() as u16;
    result
}
fn assert_word_close(actual: [u16; 4], expected: [u16; 4], context: &str) {
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 1,
            "{context}: {actual:?} != {expected:?} in channel{channel}"
        );
    }
}
fn settings(
    width: u32,
    height: u32,
    target: Vec<u8>,
    allowed: Vec<u8>,
    feather: f32,
) -> ContentAwareReplace {
    ContentAwareReplace {
        algorithm: ContentAwareAlgorithm::TextureV2,
        target_mask: SoftMask::new(width, height, target).unwrap(),
        allowed_source_mask: SoftMask::new(width, height, allowed).unwrap(),
        search_radius: 48,
        patch_radius: 1,
        feather,
    }
}
#[test]
fn constant_donors_soft_masks_and_partial_alpha_follow_the_retained_word_oracle() {
    const W: u32 = 48;
    const H: u32 = 40;
    for donor_alpha in [12345u16, 65535] {
        let donor = [10007, 43003, 22001, donor_alpha];
        let pixels = Rgba16Image::from_fn(W, H, |x, y| {
            if (20..25).contains(&x) && (16..21).contains(&y) {
                Rgba([
                    59003,
                    12007 + y as u16,
                    50003,
                    [0, 1, 257, 12345, 65535][(x - 20) as usize],
                ])
            } else if (x, y) == (0, 0) {
                Rgba([333, 444, 555, 0])
            } else {
                Rgba(donor)
            }
        });
        for space in [
            WorkingSpace::Srgb,
            WorkingSpace::LinearSrgb,
            WorkingSpace::DisplayP3,
        ] {
            let source = TiledImage16::from_rgba16_in(&pixels, space).unwrap();
            for allowed_coverage in [128u8, 255] {
                let mut target = vec![0; (W * H) as usize];
                let mut allowed = vec![allowed_coverage; target.len()];
                for y in 16..21 {
                    for x in 20..25 {
                        let i = (y * W + x) as usize;
                        target[i] = [1, 64, 128, 192, 255][(x - 20) as usize];
                        allowed[i] = 0;
                    }
                }
                for feather in [0., 0.35, 1.] {
                    let requested = settings(W, H, target.clone(), allowed.clone(), feather);
                    // All legitimate context/donor patches have the same
                    // preview colour/alpha and zero feature differences. Cost
                    // is zero, confidence is1, so feather must be neutral.
                    let result = advanced16::evaluate(
                        &source,
                        &AdvancedOperation::ContentAwareReplace(requested),
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(result.working_space(), space);
                    assert_eq!(source.to_rgba16(), pixels, "immutable source changed");
                    let mut seen_retained_residue = false;
                    for y in 0..H {
                        for x in 0..W {
                            let i = (y * W + x) as usize;
                            let old = pixels.get_pixel(x, y).0;
                            let actual = result.get_pixel(x, y).0;
                            if target[i] == 0 {
                                assert_eq!(actual, old, "protected {x},{y}");
                                continue;
                            }
                            let coverage =
                                f64::from(target[i]) / 255. * f64::from(allowed_coverage) / 255.;
                            let expected = expected_blend(old, donor, coverage);
                            assert_word_close(
                                actual,
                                expected,
                                &format!(
                                    "{space:?}, feather{feather}, target{} allowed{allowed_coverage} at{x},{y}",
                                    target[i]
                                ),
                            );
                            seen_retained_residue |= actual[..3].iter().any(|v| v % 257 != 0);
                        }
                    }
                    assert!(
                        seen_retained_residue,
                        "retained values collapsed to expanded bytes"
                    );
                }
            }
        }
    }
}
#[test]
fn nonunit_feather_confidence_precedes_partial_alpha_blending_without_byte_expansion() {
    const W: u32 = 64;
    const H: u32 = 40;
    const X: u32 = 45;
    const Y: u32 = 20;
    let donor = [10007u16, 43003, 22001, 12345];
    let boundary = [60001u16, 9007, 55003, 65535];
    // For one selected pixel, the patch has eight unchanged boundary samples
    // and its newly assigned donor at the centre. All eligible donor patches
    // are the same colour with zero spatial features. No neighbour target has
    // a cheaper covering patch, so cost is8/9 of the RGBA matching distance.
    let byte = |v: u16| (f64::from(v) / 257.).round();
    let da = byte(donor[3]);
    let ba = byte(boundary[3]);
    let mut difference = (ba - da).powi(2);
    for c in 0..3 {
        difference += ((byte(boundary[c]) * ba - byte(donor[c]) * da) / 255.).powi(2);
    }
    let confidence = (1. - (8. / 9.) * difference / (4. * 255. * 255.)).clamp(0., 1.);
    assert!(
        (0.1..0.9).contains(&confidence),
        "fixture must exercise actual feather attenuation"
    );
    for old_alpha in [0u16, 1, 257, 12345, 49001, 65535] {
        let old = [59003, 12007, 50003, old_alpha];
        let pixels = Rgba16Image::from_fn(W, H, |x, y| {
            if (x, y) == (X, Y) {
                Rgba(old)
            } else if (36..56).contains(&x) && (10..31).contains(&y) {
                Rgba(boundary)
            } else {
                Rgba(donor)
            }
        });
        let source = TiledImage16::from_rgba16(&pixels).unwrap();
        for selected in [64u8, 128, 255] {
            for allowed_coverage in [128u8, 255] {
                for feather in [0f32, 0.5, 1.] {
                    let mut target = vec![0; (W * H) as usize];
                    target[(Y * W + X) as usize] = selected;
                    let mut allowed = vec![0; target.len()];
                    for y in 0..H {
                        for x in 0..29 {
                            allowed[(y * W + x) as usize] = allowed_coverage;
                        }
                    }
                    let request = settings(W, H, target.clone(), allowed.clone(), feather);
                    let expected_coverage =
                        f64::from(selected) / 255. * f64::from(allowed_coverage) / 255.
                            * (1. - f64::from(feather) + f64::from(feather) * confidence);
                    // Observe donor selection/coefficient solely to diagnose a failure;
                    // it supplies neither the expected colour nor the blend oracle.
                    let matching = source.to_rgba8_in(WorkingSpace::Srgb).unwrap();
                    let mut samples = 0;
                    texture::visit_samples(
                        &matching,
                        &target,
                        &allowed,
                        48,
                        1,
                        feather,
                        &AtomicBool::new(false),
                        |x, y, sx, sy, amount| {
                            assert_eq!((x, y), (X, Y));
                            assert_eq!(pixels.get_pixel(sx, sy).0, donor);
                            assert!(
                                (f64::from(amount) - expected_coverage).abs() < 2e-7,
                                "coverage{} expected{expected_coverage}",
                                amount
                            );
                            samples += 1;
                            Ok(())
                        },
                    )
                    .unwrap();
                    assert_eq!(samples, 1);
                    let result = advanced16::evaluate(
                        &source,
                        &AdvancedOperation::ContentAwareReplace(request),
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                    .unwrap();
                    assert_word_close(
                        result.get_pixel(X, Y).0,
                        expected_blend(old, donor, expected_coverage),
                        &format!(
                            "alpha{old_alpha} target{selected} allowed{allowed_coverage} feather{feather}"
                        ),
                    );
                    for y in 0..H {
                        for x in 0..W {
                            if (x, y) != (X, Y) {
                                assert_eq!(
                                    result.get_pixel(x, y).0,
                                    pixels.get_pixel(x, y).0,
                                    "protected{x},{y}"
                                );
                            }
                        }
                    }
                    assert_eq!(source.to_rgba16(), pixels);
                }
            }
        }
    }
}
