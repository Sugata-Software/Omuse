use image::{Rgba, RgbaImage};
use omuse::{
    color_match::{self, Settings, Statistics},
    precision::{Rgba16Image, TiledImage16, WorkingSpace},
};
use std::sync::atomic::AtomicBool;

fn cancel() -> AtomicBool {
    AtomicBool::new(false)
}
fn settings(reference: Statistics) -> Settings {
    Settings {
        version: 1,
        reference,
        amount: 1.,
        preserve_lightness: false,
    }
}
fn near(a: impl Into<f64>, b: f64, tolerance: f64) {
    let a = a.into();
    assert!((a - b).abs() <= tolerance, "{a} differs from {b}");
}

#[test]
fn primary_colour_coordinates_agree_with_independent_oklab_reference_values() {
    // Published Oklab linear-sRGB primary coordinates, independent of our
    // inverse transform and statistics aggregation.
    for (rgb, lab) in [
        ([255, 0, 0, 255], [0.6279553606, 0.2248630611, 0.1258462985]),
        (
            [0, 255, 0, 255],
            [0.8664396115, -0.2338875742, 0.1794984799],
        ),
        (
            [0, 0, 255, 255],
            [0.4520137184, -0.0324569842, -0.3115281477],
        ),
    ] {
        let stats =
            color_match::statistics8(&RgbaImage::from_pixel(1, 1, Rgba(rgb)), &cancel()).unwrap();
        for c in 0..3 {
            near(stats.mean[c], lab[c], 5e-8);
        }
        assert_eq!(stats.deviation, [0.; 3]);
    }
}

#[test]
fn statistics_use_alpha_weights_and_never_hidden_rgb() {
    let image =
        RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 255, 255, 255, 128, 0, 255, 0, 0]).unwrap();
    let stats = color_match::statistics8(&image, &cancel()).unwrap();
    let p: f64 = 128. / 383.;
    assert_eq!(stats.samples, 2);
    near(stats.mean[0], p, 1e-7);
    near(stats.deviation[0], (p * (1. - p)).sqrt(), 1e-7);
    near(stats.mean[1], 0., 1e-7);
    near(stats.mean[2], 0., 1e-7);
    let mut hidden_changed = image.clone();
    hidden_changed.put_pixel(2, 0, Rgba([255, 0, 255, 0]));
    assert_eq!(
        color_match::statistics8(&hidden_changed, &cancel()).unwrap(),
        stats
    );
}

#[test]
fn two_tone_full_transfer_matches_reference_endpoints_without_a_roundtrip_oracle() {
    let source = RgbaImage::from_fn(2, 1, |x, _| {
        Rgba(if x == 0 {
            [64, 64, 64, 255]
        } else {
            [192, 192, 192, 255]
        })
    });
    let reference = RgbaImage::from_fn(2, 1, |x, _| {
        Rgba(if x == 0 {
            [96, 96, 96, 255]
        } else {
            [160, 160, 160, 255]
        })
    });
    let adjusted = color_match::apply8(
        &source,
        &settings(color_match::statistics8(&reference, &cancel()).unwrap()),
        &cancel(),
    )
    .unwrap();
    assert_eq!(adjusted, reference);
}

#[test]
fn amount_interpolates_perceptual_lightness_and_flat_colour_fallback_is_defined() {
    let source = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 137]));
    let reference = RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 255]));
    let mut effect = settings(color_match::statistics8(&reference, &cancel()).unwrap());
    effect.amount = 0.5;
    // Half Oklab lightness = cbrt(linear)=0.5 -> linear=0.125 -> sRGB 99.
    assert_eq!(
        color_match::apply8(&source, &effect, &cancel())
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [99, 99, 99, 137]
    );
    let reference = RgbaImage::from_pixel(1, 1, Rgba([230, 160, 100, 1]));
    let effect = settings(color_match::statistics8(&reference, &cancel()).unwrap());
    assert_eq!(
        color_match::apply8(&source, &effect, &cancel())
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [230, 160, 100, 137]
    );
}

#[test]
fn preserve_lightness_survives_out_of_gamut_reference_chroma() {
    let source = Rgba16Image::from_fn(3, 1, |x, _| {
        Rgba([
            5000 + x as u16 * 25000,
            5000 + x as u16 * 25000,
            5000 + x as u16 * 25000,
            31999,
        ])
    });
    let source = TiledImage16::from_rgba16(&source).unwrap();
    let reference = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 255]));
    let mut effect = settings(color_match::statistics8(&reference, &cancel()).unwrap());
    effect.preserve_lightness = true;
    let out = color_match::apply16(&source, &effect, &cancel()).unwrap();
    assert_ne!(out.to_rgba16(), source.to_rgba16());
    for x in 0..3 {
        let before = source.get_pixel(x, 0).0;
        let after = out.get_pixel(x, 0).0;
        assert_eq!(after[3], before[3]);
        let single =
            TiledImage16::from_rgba16(&Rgba16Image::from_pixel(1, 1, Rgba(after))).unwrap();
        let encoded = f64::from(before[0]) / 65535.;
        let linear = if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
        near(
            color_match::statistics16(&single, &cancel()).unwrap().mean[0],
            linear.cbrt(),
            0.00002,
        );
    }
}

#[test]
fn high_precision_and_wide_gamut_identity_and_partial_alpha_are_retained() {
    let reference = color_match::statistics8(
        &RgbaImage::from_pixel(1, 1, Rgba([219, 151, 90, 255])),
        &cancel(),
    )
    .unwrap();
    for space in [
        WorkingSpace::Srgb,
        WorkingSpace::LinearSrgb,
        WorkingSpace::DisplayP3,
    ] {
        let pixels = Rgba16Image::from_fn(7, 1, |x, _| {
            Rgba([
                38001 + x as u16,
                22003 + x as u16 * 91,
                11999,
                if x == 0 { 0 } else { 32003 },
            ])
        });
        let source = TiledImage16::from_rgba16_in(&pixels, space).unwrap();
        let mut effect = settings(reference.clone());
        effect.amount = 0.;
        assert_eq!(
            color_match::apply16(&source, &effect, &cancel())
                .unwrap()
                .to_rgba16(),
            pixels
        );
        effect.amount = 0.35;
        effect.preserve_lightness = true;
        let output = color_match::apply16(&source, &effect, &cancel()).unwrap();
        assert_eq!(output.working_space(), space);
        assert_eq!(output.get_pixel(0, 0).0, pixels.get_pixel(0, 0).0);
        assert!(output.to_rgba16().pixels().skip(1).any(|p| p[0] % 257 != 0));
        assert_ne!(output.get_pixel(1, 0).0, output.get_pixel(2, 0).0);
        for x in 0..7 {
            assert_eq!(output.get_pixel(x, 0).0[3], pixels.get_pixel(x, 0)[3]);
        }
        assert_eq!(source.to_rgba16(), pixels);
        let identity = settings(color_match::statistics16(&source, &cancel()).unwrap());
        assert_eq!(
            color_match::apply16(&source, &identity, &cancel())
                .unwrap()
                .to_rgba16(),
            pixels
        );
    }
    // Saturated P3 red cannot survive a conversion to a clamped sRGB buffer.
    let source = TiledImage16::from_rgba16_in(
        &Rgba16Image::from_pixel(1, 1, Rgba([65535, 0, 0, 12345])),
        WorkingSpace::DisplayP3,
    )
    .unwrap();
    let identity = settings(color_match::statistics16(&source, &cancel()).unwrap());
    assert_eq!(
        color_match::apply16(&source, &identity, &cancel())
            .unwrap()
            .to_rgba16(),
        source.to_rgba16()
    );
}

#[test]
fn reference_recipe_is_small_validated_versioned_and_source_independent() {
    let reference = RgbaImage::from_pixel(1024, 1024, Rgba([120, 100, 80, 255]));
    let stats = color_match::statistics8(&reference, &cancel()).unwrap();
    assert_eq!(stats.samples, color_match::MAX_SAMPLES);
    let mut effect = settings(stats);
    let bytes = serde_json::to_vec(&effect).unwrap();
    assert!(bytes.len() < 512);
    assert_eq!(serde_json::from_slice::<Settings>(&bytes).unwrap(), effect);
    effect.amount = f32::NAN;
    assert!(effect.validate().is_err());
    effect.amount = 1.;
    effect.version = 2;
    assert!(effect.validate().is_err());
    effect.version = 1;
    effect.reference.mean[1] = 99.;
    assert!(effect.validate().is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["reference"]["path"] = "/private/reference.png".into();
    assert!(serde_json::from_value::<Settings>(json).is_err());
    assert!(color_match::statistics8(&RgbaImage::new(4, 4), &cancel()).is_err());
}

#[test]
fn file_picker_loader_reads_real_sixteen_bit_png_and_keeps_only_statistics() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reference.png");
    let pixels = Rgba16Image::from_pixel(3, 2, Rgba([21001, 22007, 36011, 45001]));
    pixels.save(&path).unwrap();
    let loaded = color_match::load_reference(&path, &cancel()).unwrap();
    assert_eq!(loaded.dimensions, (3, 2));
    assert!(!loaded.profile_applied);
    assert_eq!(
        loaded.statistics,
        color_match::statistics16(&TiledImage16::from_rgba16(&pixels).unwrap(), &cancel()).unwrap()
    );
    assert!(loaded.thumbnail.width() <= 320 && loaded.thumbnail.height() <= 160);
    std::fs::remove_file(&path).unwrap();
    let effect = settings(loaded.statistics);
    let source = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    assert!(color_match::apply8(&source, &effect, &cancel()).is_ok());
    assert!(color_match::load_reference(directory.path(), &cancel()).is_err());
}

#[test]
fn pre_cancelled_operations_do_not_mutate_or_read_missing_reference_files() {
    let cancelled = AtomicBool::new(true);
    let source = RgbaImage::from_pixel(2, 2, Rgba([55, 70, 90, 200]));
    let original = source.clone();
    let effect = settings(color_match::statistics8(&source, &cancel()).unwrap());
    assert!(color_match::apply8(&source, &effect, &cancelled).is_err());
    assert_eq!(source, original);
    let error =
        color_match::load_reference(std::path::Path::new("missing-reference.png"), &cancelled)
            .err()
            .unwrap()
            .to_string();
    assert!(error.contains("cancelled"));
}

#[test]
fn sparse_cutouts_use_visible_fallback_and_thin_images_sample_their_extent() {
    let mut image = RgbaImage::new(512, 512);
    image.put_pixel(0, 0, Rgba([210, 120, 85, 255]));
    image.put_pixel(510, 510, Rgba([90, 170, 200, 128]));
    let sparse = color_match::statistics8(&image, &cancel()).unwrap();
    let dense = RgbaImage::from_raw(2, 1, vec![210, 120, 85, 255, 90, 170, 200, 128]).unwrap();
    assert_eq!(sparse, color_match::statistics8(&dense, &cancel()).unwrap());
    let thin = RgbaImage::from_pixel(30000, 1, Rgba([100, 150, 200, 255]));
    assert_eq!(
        color_match::statistics8(&thin, &cancel()).unwrap().samples,
        30000
    );
}

#[test]
fn reference_loader_honours_icc_and_rejects_oversize_or_invalid_files() {
    use image::{ImageEncoder, codecs::png::PngEncoder};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tagged.png");
    let mut file = std::fs::File::create(&path).unwrap();
    let mut encoder = PngEncoder::new(&mut file);
    encoder
        .set_icc_profile(omuse::color_management::srgb_profile().unwrap())
        .unwrap();
    encoder
        .write_image(&[128, 95, 70, 181], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    drop(file);
    let loaded = color_match::load_reference(&path, &cancel()).unwrap();
    assert!(loaded.profile_applied);
    assert_eq!(loaded.dimensions, (1, 1));
    assert_eq!(loaded.thumbnail.get_pixel(0, 0).0, [128, 95, 70, 181]);
    let mut file = std::fs::File::create(&path).unwrap();
    let mut encoder = PngEncoder::new(&mut file);
    encoder
        .set_icc_profile(b"not a valid ICC profile".to_vec())
        .unwrap();
    encoder
        .write_image(&[128, 95, 70, 181], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    drop(file);
    assert!(color_match::load_reference(&path, &cancel()).is_err());
    std::fs::File::create(&path)
        .unwrap()
        .set_len(color_match::MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(
        color_match::load_reference(&path, &cancel())
            .err()
            .unwrap()
            .to_string()
            .contains("128 MiB")
    );
    RgbaImage::new(30001, 1).save(&path).unwrap();
    assert!(color_match::load_reference(&path, &cancel()).is_err());
}
