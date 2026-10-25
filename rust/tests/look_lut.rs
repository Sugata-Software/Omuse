use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode},
    filters::{self, Filter},
    look_lut::{self, Settings},
    precision::{Rgba16, TiledImage16, WorkingSpace},
};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};

fn cube(f: impl Fn(f32, f32, f32) -> [f32; 3]) -> String {
    let mut out = "TITLE \"Test look\"\nLUT_3D_SIZE 2\n".to_owned();
    for b in [0., 1.] {
        for g in [0., 1.] {
            for r in [0., 1.] {
                let p = f(r, g, b);
                out.push_str(&format!("{} {} {}\n", p[0], p[1], p[2]));
            }
        }
    }
    out
}
fn settings(f: impl Fn(f32, f32, f32) -> [f32; 3]) -> Settings {
    Settings::new(look_lut::parse(&cube(f), "Fallback").unwrap())
}
fn near(actual: [f32; 3], expected: [f32; 3]) {
    for c in 0..3 {
        assert!(
            (actual[c] - expected[c]).abs() < 1e-5,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn red_fastest_cube_and_trilinear_interpolation_have_independent_oracles() {
    let swap = settings(|r, g, b| [g, b, r]);
    near(swap.sample([0.2, 0.3, 0.7]), [0.3, 0.7, 0.2]);
    let product = settings(|r, g, b| [r * g * b; 3]);
    near(product.sample([0.25, 0.5, 0.75]), [0.09375; 3]);
    near(swap.sample([1., 0., 0.]), [0., 0., 1.]);
}

#[test]
fn domain_shaper_and_quoted_titles_are_explicit() {
    let one = look_lut::parse("TITLE \"Warm #1\" # comment\nDOMAIN_MIN -1 -2 0\nDOMAIN_MAX 1 2 2\nLUT_1D_SIZE 2\n0 1 0\n1 0 1\n", "fallback").unwrap();
    assert_eq!(one.title, "Warm #1");
    near(Settings::new(one).sample([0., 0., 1.]), [0.5; 3]);
    let mut text="LUT_1D_SIZE 2\nLUT_3D_SIZE 2\nLUT_1D_INPUT_RANGE 0 1\nLUT_3D_INPUT_RANGE 0 1\n0 1 0.2\n1 0 0.8\n".to_owned();
    for b in [0., 1.] {
        for g in [0., 1.] {
            for r in [0., 1.] {
                text.push_str(&format!("{g} {b} {r}\n"));
            }
        }
    }
    let look = Settings::new(look_lut::parse(&text, "combined").unwrap());
    near(look.sample([0.2, 0.3, 0.4]), [0.7, 0.44, 0.2]);
    assert!(
        look_lut::parse(
            &text.replace("LUT_1D_INPUT_RANGE 0 1", "DOMAIN_MIN 0 0 0"),
            "ambiguous"
        )
        .is_err()
    );
}

#[test]
fn amount_above_one_clips_after_the_encoded_srgb_blend() {
    let mut look = settings(|r, g, b| [r * 0.25, g, (b + 0.5).min(1.)]);
    look.amount = 0.;
    near(look.sample([0.8, 0.3, 0.2]), [0.8, 0.3, 0.2]);
    look.amount = 2.;
    near(look.sample([0.8, 0.3, 0.2]), [0., 0.3, 1.]);
    look.amount = f32::NAN;
    assert!(look.validate().is_err());
}

#[test]
fn malformed_and_oversized_tables_are_refused_before_evaluation() {
    for text in [
        "LUT_3D_SIZE 66\n",
        "LUT_1D_SIZE 4097\n",
        "LUT_1D_SIZE 2\n0 0 0\n",
        "LUT_1D_SIZE 2\n0 0 0\n1 1 NaN\n",
        "DOMAIN_MIN 0 0 0\nDOMAIN_MAX 0 1 1\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n",
        "LUT_1D_SIZE 2\nLUT_1D_SIZE 3\n",
        "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n2 2 2\n",
        "LUT_2D_SIZE 2\n",
    ] {
        assert!(look_lut::parse(text, "invalid").is_err(), "accepted {text}");
    }
    let mut table = settings(|r, g, b| [r, g, b]).table;
    table.values.pop();
    assert!(table.validate().is_err());
    assert!(look_lut::parse(&" ".repeat(look_lut::MAX_INPUT_BYTES + 1), "oversized").is_err());
}

#[test]
fn byte_kernel_is_transactional_and_preserves_alpha_and_hidden_rgb() {
    let filter = Filter::CubeLook(settings(|r, g, b| [g, b, r]));
    let mut image = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([31, 88, 192, 127])
        } else {
            Rgba([44, 23, 219, 0])
        }
    });
    let original = image.clone();
    assert!(filters::apply_cancellable(&mut image, &filter, &AtomicBool::new(true)).is_err());
    assert_eq!(image, original);
    filters::apply(&mut image, &filter).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [88, 192, 31, 127]);
    assert_eq!(image.get_pixel(1, 0), original.get_pixel(1, 0));
}

#[test]
fn retained_precision_never_passes_through_bytes_and_cancel_keeps_source() {
    let mut source = TiledImage16::new(2, 2, WorkingSpace::Srgb).unwrap();
    source
        .set_pixel(0, 0, Rgba16([12345, 23456, 34567, 45678]))
        .unwrap();
    source.set_pixel(1, 0, Rgba16([9, 17, 29, 0])).unwrap();
    let filter = Filter::CubeLook(settings(|r, g, b| [g, b, r]));
    let result = source.filtered_with_cancel(&filter, || false).unwrap();
    assert_eq!(result.get_pixel(0, 0).0, [23456, 34567, 12345, 45678]);
    assert_eq!(result.get_pixel(1, 0), source.get_pixel(1, 0));
    let mut calls = 0;
    assert!(
        source
            .filtered_with_cancel(&filter, || {
                calls += 1;
                calls > 2
            })
            .is_err()
    );
    assert_eq!(source.get_pixel(0, 0).0, [12345, 23456, 34567, 45678]);
    for space in [WorkingSpace::LinearSrgb, WorkingSpace::DisplayP3] {
        let mut image = TiledImage16::new(1, 1, space).unwrap();
        image
            .set_pixel(0, 0, Rgba16([19043, 32432, 12321, 12345]))
            .unwrap();
        let mut zero = settings(|_, _, _| [0.; 3]);
        zero.amount = 0.;
        assert_eq!(
            image
                .filtered(&Filter::CubeLook(zero))
                .unwrap()
                .get_pixel(0, 0),
            image.get_pixel(0, 0)
        );
    }
}

#[test]
fn embedded_look_serialization_survives_source_file_removal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.cube");
    fs::write(&path, cube(|r, g, b| [g, b, r])).unwrap();
    let imported = look_lut::import(&path).unwrap();
    let fingerprint = imported.fingerprint();
    let filter = Filter::CubeLook(Settings::new(imported));
    let encoded = serde_json::to_vec(&filter).unwrap();
    fs::remove_file(path).unwrap();
    let restored: Filter = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(filter, restored);
    let Filter::CubeLook(look) = restored else {
        panic!("wrong filter")
    };
    assert_eq!(look.table.fingerprint(), fingerprint);
    near(look.sample([0.2, 0.4, 0.8]), [0.4, 0.8, 0.2]);
    let mut changed = look.table.clone();
    changed.values[0][0] = 0.125;
    assert_ne!(changed.fingerprint(), fingerprint);
}

#[test]
fn source_admission_refuses_symlinks_and_special_files() {
    let temp = tempfile::tempdir().unwrap();
    assert!(look_lut::import(temp.path()).is_err());
    #[cfg(unix)]
    {
        let real = temp.path().join("look.cube");
        fs::write(&real, cube(|r, g, b| [r, g, b])).unwrap();
        let link = temp.path().join("link.cube");
        std::os::unix::fs::symlink(real, &link).unwrap();
        assert!(look_lut::import(&link).is_err());
    }
}

#[test]
fn embedded_look_survives_native_project_reopen_with_exact_words() {
    let temp = tempfile::tempdir().unwrap();
    let cube_path = temp.path().join("look.cube");
    fs::write(&cube_path, cube(|r, g, b| [g, b, r])).unwrap();
    let mut state = LayerState::from_rgba16(
        &image::ImageBuffer::from_pixel(2, 2, Rgba([12345u16, 23456, 34567, 45678])),
        "Precise source",
    )
    .unwrap();
    let look = Settings::new(look_lut::import(&cube_path).unwrap());
    state.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Portable look".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::Filter(Filter::CubeLook(look)),
        soft_mask: None,
    });
    let evaluated = state.evaluate(&AtomicBool::new(false)).unwrap();
    assert_eq!(
        evaluated.result.get_pixel(0, 0).0,
        [23456, 34567, 12345, 45678]
    );
    let retained = evaluated.retained_bytes();
    assert!(retained > evaluated.source.memory_bytes() + evaluated.result.memory_bytes());
    let mut document = omuse::model::Document::new(2, 2);
    document.layers[0].image = Some(evaluated.proxy().unwrap().into());
    document.layers[0].advanced = Some(Arc::new(evaluated));
    let project = temp.path().join("Look.omuse");
    omuse::document::save(&document, &project).unwrap();
    fs::remove_file(cube_path).unwrap();
    let loaded = omuse::document::open(&project).unwrap();
    let loaded = loaded.layers[0].advanced.as_ref().unwrap();
    assert_eq!(
        loaded.result.get_pixel(0, 0).0,
        [23456, 34567, 12345, 45678]
    );
    assert_eq!(
        loaded
            .evaluate(&AtomicBool::new(false))
            .unwrap()
            .result
            .get_pixel(0, 0),
        loaded.result.get_pixel(0, 0)
    );
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("manifest.json")).unwrap()).unwrap();
    assert!(
        manifest["version"].as_u64().unwrap() >= 17,
        "Look documents need an explicit old-reader guard"
    );
    manifest["version"] = serde_json::json!(16);
    fs::write(
        project.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        omuse::document::open(&project)
            .unwrap_err()
            .to_string()
            .contains("format 17")
    );
}

#[test]
fn maximum_grid_is_retained_without_silent_resampling() {
    let size = look_lut::MAX_CUBE_SIZE;
    let text = format!(
        "LUT_3D_SIZE {size}\n{}",
        "0.2 0.3 0.4\n".repeat(size.pow(3))
    );
    let table = look_lut::parse(&text, "65-grid").unwrap();
    assert_eq!(usize::from(table.cube_size), size);
    assert_eq!(table.values.len(), size.pow(3));
    near(Settings::new(table).sample([1., 1., 1.]), [0.2, 0.3, 0.4]);
}
