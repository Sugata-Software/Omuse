//! Pinned Omuse appearance regressions, not an external colour-accuracy oracle.
//! See reference/photo-color/README.md for scope, provenance and review policy.
#[path = "reference/photo-color/color_metrics.rs"]
mod color_metrics;
#[path = "reference/photo-color/corpus.rs"]
mod corpus;

use image::RgbaImage;
use omuse::{asset_library::sha256_hex, camera_raw};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf};

const MATTES: [u8; 3] = [0, 127, 255];
const MEAN_LIMIT: f64 = 0.03;
const P99_LIMIT: f64 = 0.35;
const MAX_LIMIT: f64 = 0.8;
const PREMULTIPLIED_BYTE_LIMIT: u16 = 1;

fn references() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/reference/photo-color/baseline")
}

fn premultiplied(channel: u8, alpha: u8) -> u16 {
    (u16::from(channel) * u16::from(alpha) + 127) / 255
}

/// Check each chart row separately so a changed skin/shadow/highlight band
/// cannot disappear in a full-image mean. Quantiles include all three mattes.
fn compare_images(label: &str, actual: &RgbaImage, expected: &RgbaImage) -> Result<(), String> {
    if actual.dimensions() != expected.dimensions() {
        return Err(format!(
            "{label}: dimensions {:?}, expected {:?}",
            actual.dimensions(),
            expected.dimensions()
        ));
    }
    let mut failures = Vec::new();
    for y in 0..expected.height() {
        let row = corpus::ROWS.get(y as usize).copied().unwrap_or("extra row");
        let mut differences = Vec::with_capacity(actual.width() as usize * MATTES.len());
        let mut worst = (0_f64, 0_u32, 0_u8);
        let mut max_byte = (0_u16, 0_u32, 0_usize);
        let mut first_alpha = None;
        for x in 0..expected.width() {
            let a = actual.get_pixel(x, y).0;
            let b = expected.get_pixel(x, y).0;
            if a[3] != b[3] && first_alpha.is_none() {
                first_alpha = Some((x, a[3], b[3]));
            }
            for channel in 0..3 {
                let delta =
                    premultiplied(a[channel], a[3]).abs_diff(premultiplied(b[channel], b[3]));
                if delta > max_byte.0 {
                    max_byte = (delta, x, channel);
                }
            }
            for matte in MATTES {
                let delta = color_metrics::delta_e_2000(
                    color_metrics::composite_lab(a, matte),
                    color_metrics::composite_lab(b, matte),
                );
                if !delta.is_finite() {
                    return Err(format!("{label}: non-finite colour metric at ({x},{y})"));
                }
                if delta > worst.0 {
                    worst = (delta, x, matte);
                }
                differences.push(delta);
            }
        }
        if let Some((x, actual_alpha, expected_alpha)) = first_alpha {
            failures.push(format!(
                "{label}, {row}: alpha at ({x},{y})={actual_alpha}, expected {expected_alpha} (exact)"
            ));
        }
        let mean = differences.iter().sum::<f64>() / differences.len() as f64;
        differences.sort_unstable_by(f64::total_cmp);
        let p99 = differences[(differences.len() * 99).div_ceil(100) - 1];
        if mean > MEAN_LIMIT || p99 > P99_LIMIT || worst.0 > MAX_LIMIT {
            failures.push(format!(
                "{label}, {row}: DeltaE00 mean={mean:.6} (limit {MEAN_LIMIT}), \
                 p99={p99:.6} (limit {P99_LIMIT}), max={:.6} (limit {MAX_LIMIT}); \
                 worst ({},{y}) matte={} actual={:?} expected={:?}",
                worst.0,
                worst.1,
                worst.2,
                actual.get_pixel(worst.1, y).0,
                expected.get_pixel(worst.1, y).0,
            ));
        }
        if max_byte.0 > PREMULTIPLIED_BYTE_LIMIT {
            failures.push(format!(
                "{label}, {row}: premultiplied-byte difference={} (limit {}) \
                 at ({},{y}) channel={} actual={:?} expected={:?}",
                max_byte.0,
                PREMULTIPLIED_BYTE_LIMIT,
                max_byte.1,
                max_byte.2,
                actual.get_pixel(max_byte.1, y).0,
                expected.get_pixel(max_byte.1, y).0,
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn ciede2000_matches_all_34_published_supplementary_pairs() {
    let source = include_str!("reference/photo-color/ciede2000-pairs.tsv");
    let mut count = 0;
    for line in source
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let values: Vec<f64> = line
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(
            values.len(),
            7,
            "reference row must contain two Lab triples and DeltaE00"
        );
        count += 1;
        let a = [values[0], values[1], values[2]];
        let b = [values[3], values[4], values[5]];
        let actual = color_metrics::delta_e_2000(a, b);
        assert!(
            (actual - values[6]).abs() <= 0.000051,
            "published pair {count}: DeltaE00={actual:.10}, expected {:.4} (rounded reference)",
            values[6]
        );
        assert!(
            (actual - color_metrics::delta_e_2000(b, a)).abs() < 1e-10,
            "published pair {count}: metric must be symmetric"
        );
        assert_eq!(
            color_metrics::delta_e_2000(a, a),
            0.,
            "pair {count}: identity"
        );
    }
    assert_eq!(
        count, 34,
        "all published supplementary pairs must remain present"
    );
}

#[test]
fn srgb_d65_conversion_matches_analytic_neutral_anchors_and_alpha() {
    for (byte, expected_l) in [
        (0, 0.),
        (10, 2.7417480006565176),
        (11, 3.0229133609532065),
        (128, 53.58501345216902),
        (255, 100.),
    ] {
        let lab = color_metrics::srgb_to_lab([byte; 3]);
        assert!(
            (lab[0] - expected_l).abs() < 1e-10,
            "neutral {byte}: {lab:?}"
        );
        assert!(
            lab[1].abs() < 1e-10 && lab[2].abs() < 1e-10,
            "neutral {byte}: {lab:?}"
        );
    }
    // Calculated separately with 60-digit decimal arithmetic from the W3C
    // rational matrix and D65 white, avoiding a self-generated runtime oracle.
    for (rgb, expected_lab) in [
        (
            [255, 0, 0],
            [53.23711559542936, 80.09011352310383, 67.20326351172213],
        ),
        (
            [0, 255, 0],
            [87.73551910966001, -86.18159689039895, 83.18662027363],
        ),
        (
            [0, 0, 255],
            [32.30087290398018, 79.19527030740421, -107.85546553974265],
        ),
    ] {
        let actual = color_metrics::srgb_to_lab(rgb);
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected_lab[channel]).abs() < 1e-10,
                "primary {rgb:?}: {actual:?} expected {expected_lab:?}"
            );
        }
    }
    let mut previous_l = -1.;
    for byte in 0..=255 {
        let lab = color_metrics::srgb_to_lab([byte; 3]);
        assert!(lab[0] > previous_l, "non-monotonic neutral ramp at {byte}");
        assert!(lab[1].abs() < 1e-10 && lab[2].abs() < 1e-10);
        previous_l = lab[0];
    }
    for rgb in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [196, 131, 87]] {
        for matte in MATTES {
            assert_eq!(
                color_metrics::composite_lab([rgb[0], rgb[1], rgb[2], 0], matte),
                color_metrics::srgb_to_lab([matte; 3]),
                "transparent pixel must match its matte"
            );
            assert_eq!(
                color_metrics::composite_lab([rgb[0], rgb[1], rgb[2], 255], matte),
                color_metrics::srgb_to_lab(rgb),
                "opaque pixel must be independent of its matte"
            );
        }
    }
}

#[test]
fn reference_integrity_and_case_coverage_are_explicit() {
    let source = corpus::fixture();
    assert_eq!(source.dimensions(), (corpus::WIDTH, corpus::HEIGHT));
    let root = references();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("manifest.json")).expect("committed baseline manifest"),
    )
    .unwrap();
    assert_eq!(manifest["schema"], 1);
    assert_eq!(manifest["width"], corpus::WIDTH);
    assert_eq!(manifest["height"], corpus::HEIGHT);
    assert_eq!(manifest["rows"], serde_json::json!(corpus::ROWS));
    assert_eq!(manifest["revisionIsContextOnly"], true);
    assert!(
        manifest["origin"]
            .as_str()
            .unwrap()
            .contains("not Adobe parity")
    );
    for field in [
        "cameraRawSourceSha256",
        "generatorSourceSha256",
        "fixtureDefinitionSha256",
    ] {
        let digest = manifest[field].as_str().unwrap();
        assert!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "missing source provenance: {field}"
        );
    }
    assert_eq!(
        manifest["caseDefinitionsSha256"],
        sha256_hex(include_bytes!("reference/photo-color/cases.json")),
        "case settings changed: references require explicit review"
    );
    assert_eq!(
        manifest["inputPixelsSha256"],
        sha256_hex(source.as_raw()),
        "synthetic fixture changed: references require explicit review"
    );
    let input = fs::read(root.join("input.png")).unwrap();
    assert_eq!(manifest["inputPngSha256"], sha256_hex(&input));
    assert_eq!(image::load_from_memory(&input).unwrap().to_rgba8(), source);
    let cases = corpus::cases();
    let entries = manifest["references"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    assert_eq!(entries.len(), cases.len());
    let mut names = BTreeSet::new();
    for (case, entry) in cases.iter().zip(entries) {
        assert!(names.insert(&case.name), "duplicate case {}", case.name);
        assert!(!case.purpose.is_empty());
        assert_eq!(entry["name"], case.name);
        assert_eq!(entry["purpose"], case.purpose);
        let file = format!("{}.png", case.name);
        assert_eq!(entry["file"], file);
        let bytes = fs::read(root.join(file)).unwrap();
        assert_eq!(
            entry["pngSha256"],
            sha256_hex(&bytes),
            "{} reference PNG integrity",
            case.name
        );
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(
            image.dimensions(),
            source.dimensions(),
            "{} reference dimensions",
            case.name
        );
        assert_eq!(
            entry["pixelsSha256"],
            sha256_hex(image.as_raw()),
            "{} reference pixels integrity",
            case.name
        );
        if case.name == "identity" {
            assert_eq!(image, source, "default must retain every source byte");
        } else {
            assert!(
                image
                    .pixels()
                    .zip(source.pixels())
                    .any(|(a, b)| { b[3] == 255 && a.0[..3] != b.0[..3] }),
                "{} must change visible opaque colour, not only canonicalize transparency",
                case.name
            );
        }
    }
}

#[test]
fn camera_raw_matches_reviewed_omuse_colour_references() {
    let source = corpus::fixture();
    let root = references();
    let mut failures = Vec::new();
    for case in corpus::cases() {
        let label = format!("{} ({})", case.name, case.purpose);
        let actual = camera_raw::apply(&source, &case.settings)
            .unwrap_or_else(|error| panic!("{label}: pipeline failed: {error:#}"));
        let expected = image::open(root.join(format!("{}.png", case.name)))
            .unwrap()
            .to_rgba8();
        if let Err(error) = compare_images(&label, &actual, &expected) {
            failures.push(error);
        }
        for (x, y, pixel) in actual.enumerate_pixels() {
            let input = source.get_pixel(x, y);
            assert_eq!(
                pixel[3], input[3],
                "{label}: changed source alpha at ({x},{y})"
            );
            if case.name == "identity" {
                assert_eq!(
                    pixel, input,
                    "{label}: default changed source pixel at ({x},{y})"
                );
            } else if input[3] == 0 {
                assert_eq!(
                    pixel.0, [0; 4],
                    "{label}: transparent canonicalization at ({x},{y})"
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Appearance regression; inspect before any reference update:\n{}",
        failures.join("\n")
    );
}

#[test]
fn tonal_version_preserves_legacy_recipes_and_exact_new_identity() {
    use camera_raw::{Settings, ToneMapping};
    let legacy: Settings = serde_json::from_value(serde_json::json!({"shadows":18})).unwrap();
    assert_eq!(legacy.tone_mapping, ToneMapping::Legacy);
    assert_eq!(Settings::default().tone_mapping, ToneMapping::Legacy);
    let saved = serde_json::to_value(&legacy).unwrap();
    assert!(
        saved.get("toneMapping").is_none(),
        "legacy serialization must not acquire a new recipe field"
    );
    let reopened: Settings = serde_json::from_value(saved).unwrap();
    assert_eq!(reopened, legacy);
    let small = RgbaImage::from_fn(3, 1, |x, _| {
        image::Rgba(match x {
            0 => [0, 0, 0, 255],
            1 => [1, 0, 0, 255],
            _ => [0, 0, 1, 255],
        })
    });
    // Recorded from production before SmoothV1. These strong near-black spikes
    // are known legacy defects, retained ONLY as a saved-recipe compatibility
    // contract. They are not approved appearance targets for new edits.
    let legacy_output = camera_raw::apply(&small, &legacy).unwrap();
    assert_eq!(legacy_output.get_pixel(0, 0).0, [23, 23, 23, 255]);
    assert_eq!(legacy_output.get_pixel(1, 0).0, [108, 0, 0, 255]);
    assert_eq!(legacy_output.get_pixel(2, 0).0, [0, 0, 255, 255]);
    assert_eq!(camera_raw::apply(&small, &reopened).unwrap(), legacy_output);
    let fresh = Settings::for_new_edit();
    assert_eq!(fresh.tone_mapping, ToneMapping::SmoothV1);
    assert_eq!(
        serde_json::to_value(&fresh).unwrap()["toneMapping"],
        "SmoothV1"
    );
    let source = corpus::fixture();
    assert_eq!(
        camera_raw::apply(&source, &fresh).unwrap(),
        source,
        "new version alone must preserve every RGBA byte"
    );
    assert!(
        serde_json::from_value::<Settings>(serde_json::json!({"toneMapping":"FutureUnknown"}))
            .is_err()
    );
}

#[test]
fn smooth_tonal_lifts_do_not_amplify_one_byte_dark_noise_into_saturated_spikes() {
    let source = RgbaImage::from_fn(9, 4, |x, y| {
        image::Rgba(match y {
            0 => [x as u8, x as u8, x as u8, 255],
            1 => [x as u8, 0, 0, 255],
            2 => [0, x as u8, 0, 255],
            _ => [0, 0, x as u8, 255],
        })
    });
    for (name, settings) in [
        (
            "shadow lift",
            serde_json::json!({"toneMapping":"SmoothV1","shadows":18}),
        ),
        (
            "black lift",
            serde_json::json!({"toneMapping":"SmoothV1","blacks":12}),
        ),
        (
            "master curve lift",
            serde_json::json!({"toneMapping":"SmoothV1","curve":{"rgb":[{"x":0,"y":0.02},{"x":1,"y":1}]}}),
        ),
    ] {
        let settings = serde_json::from_value(settings).unwrap();
        let output = camera_raw::apply(&source, &settings).unwrap();
        for y in 0..4 {
            for x in 1..source.width() {
                let previous = output.get_pixel(x - 1, y);
                let current = output.get_pixel(x, y);
                for channel in 0..3 {
                    assert!(
                        previous[channel].abs_diff(current[channel]) <= 3,
                        "{name}: near-black input step at ({x},{y}) caused channel {channel} jump {:?} -> {:?}",
                        previous.0,
                        current.0
                    );
                }
                assert_eq!(current[3], 255);
            }
        }
    }
}

#[test]
fn smooth_shadow_and_highlight_controls_preserve_neutral_ramp_order() {
    let source = RgbaImage::from_fn(256, 1, |x, _| image::Rgba([x as u8, x as u8, x as u8, 255]));
    for field in ["shadows", "highlights"] {
        for amount in [-100, -65, -45, -18, 18, 45, 65, 100] {
            let mut value = serde_json::json!({"toneMapping":"SmoothV1"});
            value[field] = serde_json::json!(amount);
            let settings = serde_json::from_value(value).unwrap();
            let output = camera_raw::apply(&source, &settings).unwrap();
            let mut previous = 0;
            for x in 0..256 {
                let pixel = output.get_pixel(x, 0).0;
                assert_eq!(
                    pixel,
                    [pixel[0], pixel[0], pixel[0], 255],
                    "{field}={amount}: tinted neutral at x={x}"
                );
                assert!(
                    pixel[0] >= previous,
                    "{field}={amount}: brighter neutral became darker at x={x}, {previous} -> {}",
                    pixel[0]
                );
                previous = pixel[0];
            }
        }
    }
}

#[test]
fn exposure_has_expected_direction_without_introducing_a_neutral_cast() {
    let source = corpus::fixture();
    for (name, positive) in [("exposure_plus", true), ("exposure_minus", false)] {
        let case = corpus::cases()
            .into_iter()
            .find(|case| case.name == name)
            .unwrap();
        let output = camera_raw::apply(&source, &case.settings).unwrap();
        let mut changed = 0;
        let mut previous = 0;
        for x in 0..corpus::WIDTH {
            let pixel = output.get_pixel(x, 0).0;
            assert_eq!(
                pixel,
                [pixel[0], pixel[0], pixel[0], 255],
                "{name}: tinted neutral at x={x}: {pixel:?}"
            );
            assert!(
                pixel[0] >= previous,
                "{name}: non-monotonic neutral ramp at x={x}"
            );
            assert!(
                if positive {
                    pixel[0] >= x as u8
                } else {
                    pixel[0] <= x as u8
                },
                "{name}: exposure moved neutral in wrong direction at x={x}: {pixel:?}"
            );
            changed += usize::from(pixel[0] != x as u8);
            previous = pixel[0];
        }
        assert!(
            changed > 200,
            "{name}: exposure should affect most interior neutral levels, changed only {changed}"
        );
        assert_eq!(output.get_pixel(0, 0).0, [0, 0, 0, 255]);
    }
}

#[test]
fn comparison_guards_detect_colour_tone_channel_and_alpha_regressions() {
    let expected = corpus::fixture();
    assert!(compare_images("identical", &expected, &expected).is_ok());
    let mutations: [(&str, fn(u32, u32, &mut image::Rgba<u8>)); 6] = [
        ("channel swap", |_, _, p| p.0.swap(0, 2)),
        ("green cast", |_, _, p| p[1] = p[1].saturating_add(5)),
        ("shadow lift", |_, y, p| {
            if y == 1 {
                for c in 0..3 {
                    p[c] = p[c].saturating_add(8);
                }
            }
        }),
        ("highlight clipping", |_, y, p| {
            if y == 2 {
                for c in 0..3 {
                    p[c] = p[c].min(220);
                }
            }
        }),
        ("single colour patch", |x, y, p| {
            if x == 100 && y == 10 {
                p[0] = p[0].saturating_add(3);
            }
        }),
        ("alpha loss", |_, _, p| p[3] = p[3].saturating_sub(1)),
    ];
    for (label, mutation) in mutations {
        let mut actual = expected.clone();
        for (x, y, pixel) in actual.enumerate_pixels_mut() {
            mutation(x, y, pixel);
        }
        let error = compare_images(label, &actual, &expected).expect_err(label);
        assert!(
            error.contains(label) && (error.contains(" at (") || error.contains("worst (")),
            "failure must locate the regression: {error}"
        );
    }
    // Hidden zero-alpha colour has no visible contribution on any matte. The
    // pipeline test above separately enforces its exact identity/canonical form.
    let mut hidden_only = expected.clone();
    for pixel in hidden_only.pixels_mut().filter(|p| p[3] == 0) {
        pixel.0[..3].fill(255);
    }
    assert!(compare_images("hidden RGB", &hidden_only, &expected).is_ok());
}
