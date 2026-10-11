use super::*;
use image::Rgba;

fn fixture() -> (RgbaImage, Vec<u8>, Vec<u8>) {
    let image = RgbaImage::from_fn(96, 64, |x, y| {
        let grain = (((x.wrapping_mul(747796405) ^ y.wrapping_mul(2891336453)) >> 9) % 55) as u8;
        let value = if x < 50 { 98 + grain } else { 125 };
        Rgba([value, value.saturating_add(4), value.saturating_sub(3), 255])
    });
    let mut mask = vec![0; 96 * 64];
    for y in 24..38 {
        for x in 24..38 {
            mask[y * 96 + x] = 255;
        }
    }
    (image, mask, vec![255; 96 * 64])
}

fn render(image: &RgbaImage, target: &[u8], allowed: &[u8]) -> Result<RgbaImage> {
    let mut result = image.clone();
    visit_samples(
        image,
        target,
        allowed,
        48,
        2,
        0.0,
        &AtomicBool::new(false),
        |x, y, sx, sy, amount| {
            let p = result.get_pixel_mut(x, y);
            let donor = image.get_pixel(sx, sy);
            for c in 0..4 {
                p[c] = (f32::from(p[c]) + (f32::from(donor[c]) - f32::from(p[c])) * amount).round()
                    as u8;
            }
            Ok(())
        },
    )?;
    Ok(result)
}

#[test]
fn deterministic_texture_survives_with_protected_pixels_and_no_object_leakage() {
    let (mut image, mask, allowed) = fixture();
    for (i, p) in image.pixels_mut().enumerate() {
        if mask[i] > 0 {
            *p = Rgba([255, 0, 255, 255]);
        }
    }
    let before = image.clone();
    let a = render(&image, &mask, &allowed).unwrap();
    let b = render(&image, &mask, &allowed).unwrap();
    assert_eq!(a, b);
    assert_eq!(image, before);
    let mut mean = 0.0;
    let mut squares = 0.0;
    let mut count = 0.0;
    for (i, (p, q)) in a.pixels().zip(before.pixels()).enumerate() {
        if mask[i] == 0 {
            assert_eq!(p, q, "unselected pixel {i}");
        } else {
            assert_ne!(p.0, [255, 0, 255, 255]);
            mean += f64::from(p[0]);
            squares += f64::from(p[0]).powi(2);
            count += 1.0;
        }
    }
    let variance = squares / count - (mean / count).powi(2);
    assert!(
        variance > 50.0,
        "texture collapsed into a smooth region: variance {variance}"
    );
    for (i, p) in image.pixels_mut().enumerate() {
        if mask[i] > 0 {
            *p = Rgba([0, 255, 0, 1]);
        }
    }
    let changed_hidden = render(&image, &mask, &allowed).unwrap();
    for (i, (a, b)) in a.pixels().zip(changed_hidden.pixels()).enumerate() {
        if mask[i] > 0 {
            assert_eq!(a, b, "removed colours influenced output {i}");
        }
    }
}

#[test]
fn only_allowed_original_donors_are_reported_and_soft_coverage_is_retained() {
    let (image, mut mask, mut allowed) = fixture();
    for (i, v) in allowed.iter_mut().enumerate() {
        *v = if i % 96 < 50 { 128 } else { 0 };
    }
    for v in &mut mask {
        if *v > 0 {
            *v = 128;
        }
    }
    let mut seen = 0;
    visit_samples(
        &image,
        &mask,
        &allowed,
        48,
        2,
        0.0,
        &AtomicBool::new(false),
        |x, y, sx, sy, amount| {
            let (t, s) = ((y * 96 + x) as usize, (sy * 96 + sx) as usize);
            assert!(mask[t] > 0 && mask[s] == 0 && allowed[s] > 0);
            assert!(x.abs_diff(sx) <= 48 && y.abs_diff(sy) <= 48);
            assert!((amount - (128.0 / 255.0) * (128.0 / 255.0)).abs() < 1e-7);
            seen += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(seen, 196);
}

#[test]
fn failure_and_cancellation_emit_no_unfinished_synthesis() {
    let (image, mask, allowed) = fixture();
    let before = image.clone();
    let mut callbacks = 0;
    let cancelled = AtomicBool::new(true);
    assert!(
        visit_samples(
            &image,
            &mask,
            &allowed,
            48,
            2,
            0.0,
            &cancelled,
            |_, _, _, _, _| {
                callbacks += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(callbacks, 0);
    assert!(
        visit_samples(
            &image,
            &mask,
            &vec![0; allowed.len()],
            48,
            2,
            0.0,
            &AtomicBool::new(false),
            |_, _, _, _, _| {
                callbacks += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(callbacks, 0);
    assert_eq!(image, before);
    let cancel = AtomicBool::new(false);
    let result = visit_samples(
        &image,
        &mask,
        &allowed,
        48,
        2,
        0.0,
        &cancel,
        |_, _, _, _, _| {
            callbacks += 1;
            cancel.store(true, Ordering::Relaxed);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(callbacks, 1, "final copy must also observe cancellation");
    assert_eq!(image, before);
}

#[test]
fn invalid_inputs_and_over_budget_selections_are_refused() {
    let (image, mask, allowed) = fixture();
    for (search, patch, feather) in [(0, 2, 0.0), (65, 2, 0.0), (48, 5, 0.0), (48, 2, f32::NAN)] {
        assert!(validate_inputs(96, 64, &mask, &allowed, search, patch, feather).is_err());
    }
    assert!(validate_inputs(96, 64, &mask[..10], &allowed, 48, 2, 0.0).is_err());
    let all = vec![255; 512 * 512];
    assert!(validate_inputs(512, 512, &all, &all, 48, 2, 0.0).is_err());
    let mut large = vec![0; 1024 * 1024];
    for y in 100..590 {
        for x in 100..590 {
            large[y * 1024 + x] = 255;
        }
    }
    assert!(validate_inputs(1024, 1024, &large, &vec![255; large.len()], 48, 4, 0.0).is_err());
    let mut called = false;
    visit_samples(
        &image,
        &vec![0; mask.len()],
        &allowed,
        48,
        2,
        0.0,
        &AtomicBool::new(false),
        |_, _, _, _, _| {
            called = true;
            Ok(())
        },
    )
    .unwrap();
    assert!(!called);
}

#[test]
fn large_source_uses_small_roi_and_keeps_full_image_coordinates() {
    let image = RgbaImage::from_pixel(2400, 1800, Rgba([33, 61, 79, 255]));
    let mut target = vec![0; 2400 * 1800];
    target[1400 * 2400 + 1900] = 255;
    let allowed = vec![255; target.len()];
    let roi = region(
        2400,
        1800,
        &target,
        &allowed,
        16,
        2,
        0.0,
        &AtomicBool::new(false),
    )
    .unwrap()
    .unwrap();
    assert!(roi.w * roi.h < 2000);
    let mut count = 0;
    visit_samples(
        &image,
        &target,
        &allowed,
        16,
        2,
        0.0,
        &AtomicBool::new(false),
        |x, y, sx, sy, amount| {
            assert_eq!((x, y), (1900, 1400));
            assert!(sx >= 1884 && sx <= 1916 && sy >= 1384 && sy <= 1416);
            assert_ne!((x, y), (sx, sy));
            assert_eq!(amount, 1.0);
            count += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn border_target_and_disconnected_targets_are_reached() {
    let image = RgbaImage::from_pixel(64, 64, Rgba([41, 53, 67, 255]));
    let mut target = vec![0; 4096];
    for y in 0..4 {
        for x in 0..4 {
            target[y * 64 + x] = 255;
        }
    }
    target[50 * 64 + 50] = 255;
    let output = render(&image, &target, &vec![255; 4096]).unwrap();
    assert_eq!(output, image);
}
