//! Native16V1 qualification. Analytic transfer and alpha expectations here do
//! not call production transfer, colour conversion or blur helpers.
use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::SoftMask,
    camera_raw::{self, Processing, SampleStage, Settings, ToneMapping, WhiteBalance, native},
    precision::{Rgba16Image, TiledImage16, WorkingSpace},
};
use std::sync::{Arc, atomic::AtomicBool};
fn cancel() -> AtomicBool {
    AtomicBool::new(false)
}
fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn source(space: WorkingSpace) -> TiledImage16 {
    let raw = Rgba16Image::from_fn(31, 19, |x, y| {
        Rgba([
            1000 + (x * 173 + y * 19) as u16,
            17000 + (x * 121 + y * 23) as u16,
            39000 + (x * 211 + y * 29) as u16,
            [0, 1, 2, 257, 32768, 65534, 65535][(x as usize + y as usize) % 7],
        ])
    });
    TiledImage16::from_rgba16_in(&raw, space).unwrap()
}
#[test]
fn missing_processing_remains_byte_compatible_and_unknown_versions_fail() {
    let old: Settings = serde_json::from_value(serde_json::json!({"shadows":18})).unwrap();
    assert_eq!(old.processing, Processing::ByteCompatible);
    assert_eq!(old.tone_mapping, ToneMapping::Legacy);
    assert!(
        serde_json::to_value(&old)
            .unwrap()
            .get("processing")
            .is_none()
    );
    let bytes = RgbaImage::from_fn(3, 1, |x, _| {
        Rgba(match x {
            0 => [0, 0, 0, 255],
            1 => [1, 0, 0, 255],
            _ => [0, 0, 1, 255],
        })
    });
    let output = camera_raw::apply(&bytes, &old).unwrap();
    assert_eq!(
        output.as_raw(),
        &[23, 23, 23, 255, 108, 0, 0, 255, 0, 0, 255, 255]
    );
    let native = Settings::for_native_edit();
    assert_eq!(
        serde_json::to_value(&native).unwrap()["processing"],
        "Native16V1"
    );
    assert!(
        serde_json::from_value::<Settings>(serde_json::json!({"processing":"Native16V99"}))
            .is_err()
    );
    assert!(
        camera_raw::apply(&bytes, &native).is_err(),
        "Native16 must never silently enter the byte kernel"
    );
}
#[test]
fn native_identity_is_exact_in_every_space_including_hidden_and_single_word_alpha() {
    for space in [
        WorkingSpace::Srgb,
        WorkingSpace::LinearSrgb,
        WorkingSpace::DisplayP3,
    ] {
        let source = source(space);
        let output = native::apply(&source, &Settings::for_native_edit(), &cancel()).unwrap();
        assert_eq!(output.to_rgba16(), source.to_rgba16());
        assert!(output.shares_tile_with(&source, 0, 0));
        assert_eq!(output.working_space(), space);
    }
}
#[test]
fn native_exposure_matches_independent_word_oracle_without_alpha_dependent_quantization() {
    for space in [
        WorkingSpace::Srgb,
        WorkingSpace::LinearSrgb,
        WorkingSpace::DisplayP3,
    ] {
        let source = source(space);
        let settings = Settings {
            exposure: 0.75,
            ..Settings::for_native_edit()
        };
        let result = native::apply(&source, &settings, &cancel()).unwrap();
        for y in 0..source.height() {
            for x in 0..source.width() {
                let input = source.get_pixel(x, y).0;
                let actual = result.get_pixel(x, y).0;
                assert_eq!(actual[3], input[3]);
                if input[3] == 0 {
                    assert_eq!(actual, input);
                    continue;
                }
                for c in 0..3 {
                    let v = f64::from(input[c]) / 65535.;
                    let expected = if space == WorkingSpace::LinearSrgb {
                        (v * 2f64.powf(0.75)).min(1.)
                    } else {
                        encode((decode(v) * 2f64.powf(0.75)).min(1.))
                    };
                    let expected = (expected * 65535.).round() as u16;
                    assert!(
                        actual[c].abs_diff(expected) <= 1,
                        "{space:?}: {input:?} -> {actual:?}, expected channel {expected}"
                    );
                }
            }
        }
    }
    let source = TiledImage16::from_rgba16(&Rgba16Image::from_fn(7, 1, |x, _| {
        Rgba([
            10001,
            20003,
            30007,
            [0, 1, 2, 257, 32768, 65534, 65535][x as usize],
        ])
    }))
    .unwrap();
    let output = native::apply(
        &source,
        &Settings {
            exposure: 0.7,
            ..Settings::for_native_edit()
        },
        &cancel(),
    )
    .unwrap();
    for x in 1..7 {
        assert_eq!(
            &output.get_pixel(x, 0).0[..3],
            &output.get_pixel(6, 0).0[..3]
        );
    }
    assert!(output.get_pixel(6, 0).0[..3].iter().any(|v| v % 257 != 0));
}
#[test]
fn native_draft_is_exact_at_original_sample_coordinates_and_rejects_global_or_spatial_controls() {
    let mut settings = Settings {
        exposure: 0.43,
        temperature: -11.,
        shadows: 20.,
        texture: 0.,
        ..Settings::for_native_edit()
    };
    settings
        .curve
        .red
        .insert(1, camera_raw::CurvePoint { x: 0.4, y: 0.51 });
    settings.mixer.hue[2] = 13.;
    settings.calibration.blue_hue = 7.;
    settings.grading.shadows.saturation = 13.;
    settings.grading.shadows.hue = 210.;
    for space in [
        WorkingSpace::Srgb,
        WorkingSpace::LinearSrgb,
        WorkingSpace::DisplayP3,
    ] {
        let source = source(space);
        let full = native::apply(&source, &settings, &cancel()).unwrap();
        let small = native::sampled_source(&source, (11, 7), &cancel()).unwrap();
        let reduced = native::apply(&small, &settings, &cancel()).unwrap();
        for y in 0..7 {
            for x in 0..11 {
                assert_eq!(
                    reduced.get_pixel(x, y),
                    full.get_pixel(
                        native::sample_coordinate(x, 11, 31),
                        native::sample_coordinate(y, 7, 19)
                    )
                );
            }
        }
    }
    assert!(native::pointwise(&settings));
    settings.white_balance = WhiteBalance::Auto;
    assert!(!native::pointwise(&settings));
    settings.white_balance = WhiteBalance::Custom;
    settings.clarity = 2.;
    assert!(!native::pointwise(&settings));
    settings.clarity = 0.;
    settings.grain_amount = 1.;
    assert!(!native::pointwise(&settings));
}
#[test]
fn all_native_spatial_families_are_real_word_edits_preserve_alpha_and_hidden_rgb() {
    let mut chart = source(WorkingSpace::Srgb).to_rgba16();
    // Include visible highlights above the glow threshold; the original blue
    // low-luminance ramp legitimately contains no glow-eligible pixels.
    // Retain the varied alpha/hidden-RGB chart outside this patch.
    for y in 6..13 {
        for x in 20..27 {
            let alpha = chart.get_pixel(x, y)[3];
            chart.put_pixel(x, y, Rgba([55001, 60003, 58007, alpha]));
        }
    }
    let src = TiledImage16::from_rgba16(&chart).unwrap();
    let mut cases = Vec::new();
    for (which, amount) in [
        (0, 52.),
        (1, 40.),
        (2, 32.),
        (3, 46.),
        (4, -52.),
        (5, 36.),
        (6, 45.),
        (7, 64.),
        (8, 75.),
    ] {
        let mut s = Settings::for_native_edit();
        match which {
            0 => s.texture = amount,
            1 => s.clarity = amount,
            2 => s.dehaze = amount,
            3 => s.glow = amount,
            4 => s.vignette_amount = amount,
            5 => s.grain_amount = amount,
            6 => s.detail.noise_luminance = amount,
            7 => s.detail.noise_color = amount,
            _ => s.detail.sharpen_amount = amount,
        };
        cases.push(s);
    }
    for settings in cases {
        let output = native::apply(&src, &settings, &cancel()).unwrap();
        assert_ne!(
            output.to_rgba16(),
            src.to_rgba16(),
            "settings were silently skipped: {settings:?}"
        );
        for y in 0..src.height() {
            for x in 0..src.width() {
                let a = src.get_pixel(x, y).0;
                let b = output.get_pixel(x, y).0;
                assert_eq!(a[3], b[3]);
                if a[3] == 0 {
                    assert_eq!(a, b);
                }
            }
        }
    }
}
#[test]
fn native_auto_white_balance_neutralizes_constant_midtone_and_has_explicit_failure() {
    let src = TiledImage16::from_rgba16(&Rgba16Image::from_pixel(
        9,
        7,
        Rgba([33001, 28003, 21007, 65535]),
    ))
    .unwrap();
    let settings = Settings {
        white_balance: WhiteBalance::Auto,
        ..Settings::for_native_edit()
    };
    let output = native::apply(&src, &settings, &cancel()).unwrap();
    let p = output.get_pixel(4, 3).0;
    assert!(
        p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1,
        "{p:?}"
    );
    let black =
        TiledImage16::from_rgba16(&Rgba16Image::from_pixel(4, 4, Rgba([0, 0, 0, 65535]))).unwrap();
    assert!(
        native::apply(&black, &settings, &cancel())
            .unwrap_err()
            .to_string()
            .contains("visible unclipped midtones")
    );
}
#[test]
fn native_stage_samples_are_real_words_and_range_view_never_changes_kept_pixels() {
    let src = source(WorkingSpace::Srgb);
    let mut s = Settings {
        exposure: 0.3,
        ..Settings::for_native_edit()
    };
    s.mixer.points.push(camera_raw::PointColor {
        hue: 220.,
        hue_range: 100.,
        visualize: true,
        luminance_shift: 44.,
        ..Default::default()
    });
    let (result, sample) =
        native::apply_with_sample(&src, &s, &cancel(), SampleStage::PointColor).unwrap();
    let preview = native::range_preview(&result, &sample, &s, &cancel()).unwrap();
    assert_ne!(preview.to_rgba16(), result.to_rgba16());
    s.mixer.points[0].visualize = false;
    assert_eq!(
        native::apply(&src, &s, &cancel()).unwrap().to_rgba16(),
        result.to_rgba16()
    );
    for stage in [
        SampleStage::WhiteBalance,
        SampleStage::Curve,
        SampleStage::Mixer,
        SampleStage::PointColor,
        SampleStage::Optics,
    ] {
        let (output, sample) = native::apply_with_sample(&src, &s, &cancel(), stage).unwrap();
        assert_eq!(output.to_rgba16(), result.to_rgba16());
        assert_eq!(sample.dimensions(), src.dimensions());
        assert_eq!(sample.working_space(), src.working_space());
    }
}
#[test]
fn native_recipe_mask_prepared_and_replayed_results_agree_and_undo_is_one_step() {
    let pixels = source(WorkingSpace::Srgb);
    let mut state = LayerState::from_rgba16(&pixels.to_rgba16(), "native fixture").unwrap();
    let mask = SoftMask::new(
        31,
        19,
        (0..31 * 19)
            .map(|i| {
                if i % 3 == 0 {
                    0
                } else if i % 3 == 1 {
                    127
                } else {
                    255
                }
            })
            .collect(),
    )
    .unwrap();
    let settings = Settings {
        exposure: 0.7,
        clarity: 24.,
        ..Settings::for_native_edit()
    };
    let (prepared, _) = state
        .with_native_camera_raw(settings, Some(mask.clone()), &cancel(), None)
        .unwrap();
    let replay = prepared.evaluate(&cancel()).unwrap();
    assert_eq!(replay.result.to_rgba16(), prepared.result.to_rgba16());
    for y in 0..19 {
        for x in 0..31 {
            let i = y as usize * 31 + x as usize;
            if mask.data[i] == 0 {
                assert_eq!(
                    prepared.result.get_pixel(x, y),
                    state.result.get_pixel(x, y)
                );
            }
        }
    }
    let mut doc = omuse::model::Document::new(31, 19);
    let id = doc.layers[0].id.clone();
    doc.layers[0].image = Some(state.proxy().unwrap().into());
    doc.layers[0].advanced = Some(Arc::new(state.clone()));
    let mut editor = omuse::editor::Editor::new(doc);
    editor.active_layer = id.clone();
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), prepared.clone())])
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16(),
        state.result.to_rgba16()
    );
    assert!(editor.redo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16(),
        prepared.result.to_rgba16()
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("native.omuse");
    omuse::document::save(&editor.document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    state = (**reopened.find_layer(&id).unwrap().advanced.as_ref().unwrap()).clone();
    assert_eq!(state.result.to_rgba16(), prepared.result.to_rgba16());
    assert_eq!(
        state.evaluate(&cancel()).unwrap().result.to_rgba16(),
        prepared.result.to_rgba16()
    );
    let json = serde_json::to_value(&state.recipe).unwrap();
    assert!(json.to_string().contains("Native16V1"));
}
#[test]
fn native_cancellation_budget_and_invalid_settings_publish_no_partial_result() {
    let source = source(WorkingSpace::DisplayP3);
    let before = source.to_rgba16();
    let cancelled = AtomicBool::new(true);
    assert!(native::apply(&source, &Settings::for_native_edit(), &cancelled).is_err());
    assert_eq!(source.to_rgba16(), before);
    assert!(native::work_bytes((0, 4)).is_err());
    assert!(native::work_bytes((4097, 4097)).is_err());
    assert_eq!(native::work_bytes((4096, 4096)).unwrap(), 512 * 1024 * 1024);
    assert!(
        native::apply(
            &source,
            &Settings {
                exposure: f32::NAN,
                ..Settings::for_native_edit()
            },
            &cancel()
        )
        .is_err()
    );
}

#[test]
fn native_multi_point_visualization_follows_the_actual_sequential_hue_stage() {
    let pixels = Rgba16Image::from_fn(3, 2, |x, _| {
        Rgba(match x {
            0 => [0, 65535, 0, 65535],     // green:120deg, shifted to150deg
            1 => [0, 0, 65535, 65535],     // blue:240deg, outside both point ranges
            _ => [0, 65535, 65535, 65535], // cyan:180deg, outside both ranges
        })
    });
    let src = TiledImage16::from_rgba16(&pixels).unwrap();
    let mut settings = Settings::for_native_edit();
    settings.mixer.points = vec![
        camera_raw::PointColor {
            hue: 120.,
            saturation: 1.,
            luminance: 0.5,
            hue_range: 5.,
            hue_shift: 100.,
            ..Default::default()
        },
        camera_raw::PointColor {
            hue: 150.,
            saturation: 1.,
            luminance: 0.5,
            hue_range: 5.,
            luminance_shift: 20.,
            visualize: true,
            ..Default::default()
        },
    ];
    let (result, sample) =
        native::apply_with_sample(&src, &settings, &cancel(), SampleStage::PointColor).unwrap();
    // Independent HSL oracle: green120 +30deg ->150. With valid disjoint
    // +/-5deg ranges, the next point has weight1 only after the first edit.
    // Raising L=.5 by .05 gives RGB(.1,1,.55).
    let expected = [6554u16, 65535, 36044, 65535];
    let pixel = result.get_pixel(0, 1).0;
    for c in 0..4 {
        assert!(pixel[c].abs_diff(expected[c]) <= 1, "{pixel:?}");
    }
    let review = native::range_preview(&result, &sample, &settings, &cancel()).unwrap();
    assert_eq!(
        review.get_pixel(0, 1),
        result.get_pixel(0, 1),
        "The second point includes pixels shifted into its hue range by the first point"
    );
    for x in [1, 2] {
        assert_eq!(result.get_pixel(x, 1), src.get_pixel(x, 1));
        let p = review.get_pixel(x, 1).0;
        assert_eq!(p[0], p[1], "unselected hue must be gray in range review");
        assert_eq!(p[1], p[2]);
        assert_eq!(p[3], 65535);
    }
    // Reversing the point order makes the lightness point miss green; the
    // final shifted colour is then RGB(0,1,.5), proving sequential semantics.
    settings.mixer.points.reverse();
    let reversed = native::apply(&src, &settings, &cancel()).unwrap();
    let expected_reversed = [0u16, 65535, 32768, 65535];
    for c in 0..4 {
        assert!(reversed.get_pixel(0, 1).0[c].abs_diff(expected_reversed[c]) <= 1);
    }
}
#[test]
fn native_glow_has_an_independent_constant_highlight_oracle_and_respects_threshold() {
    let settings = Settings {
        glow: 46.,
        ..Settings::for_native_edit()
    };
    for word in [30001u16, 40001] {
        let pixels = Rgba16Image::from_fn(13, 9, |x, y| {
            if x == 0 && y == 0 {
                Rgba([123, 456, 789, 0])
            } else {
                Rgba([word, word, word, 65535])
            }
        });
        let source = TiledImage16::from_rgba16(&pixels).unwrap();
        let output = native::apply(&source, &settings, &cancel()).unwrap();
        // A coverage-normalized blur of a constant visible field is constant,
        // even beside the transparent hidden colour. No production blur/luma
        // helper is used for this analytic expectation.
        let level = f64::from(word) / 65535.;
        let highlight = ((level - 0.55) / 0.45).clamp(0., 1.);
        let expected = [0.75, 0.6, 0.75]
            .map(|gain| ((level + highlight * 0.46 * gain).clamp(0., 1.) * 65535.).round() as u16);
        for y in 0..9 {
            for x in 0..13 {
                let p = output.get_pixel(x, y).0;
                if x == 0 && y == 0 {
                    assert_eq!(p, [123, 456, 789, 0]);
                } else {
                    for c in 0..3 {
                        assert!(
                            p[c].abs_diff(expected[c]) <= 1,
                            "word={word}, pixel={p:?}, expected={expected:?}"
                        );
                    }
                    assert_eq!(p[3], 65535);
                }
            }
        }
        if word < 36045 {
            assert_eq!(output.to_rgba16(), pixels);
        } else {
            assert!(output.get_pixel(6, 4).0[0] > word);
        }
    }
}
#[test]
fn native_admission_counts_history_layers_and_preview_reserves_before_allocation() {
    let mut document = omuse::model::Document::new(48, 32);
    document.layers[0].image = Some(RgbaImage::from_pixel(48, 32, Rgba([20, 30, 40, 255])).into());
    let plain = native::ui_admission(&document, 0, (48, 32), true, false, 0).unwrap();
    let visual = native::ui_admission(&document, 0, (48, 32), true, true, 48 * 32).unwrap();
    assert!(plain.estimated_bytes > plain.kernel_scratch_bytes);
    assert!(visual.estimated_bytes > plain.estimated_bytes);
    let history = native::ui_admission(&document, 12345, (48, 32), true, false, 0).unwrap();
    assert_eq!(history.estimated_bytes - plain.estimated_bytes, 12345);
    document.layers.push(document.layers[0].clone());
    let extra = native::ui_admission(&document, 0, (48, 32), true, false, 0).unwrap();
    assert!(extra.existing_document_bytes > plain.existing_document_bytes);
    assert!(extra.renderer_reserve_bytes > plain.renderer_reserve_bytes);
    assert!(
        native::ui_admission(
            &document,
            native::MAX_NATIVE_UI_ESTIMATE_BYTES,
            (48, 32),
            true,
            false,
            0
        )
        .is_err()
    );
}
