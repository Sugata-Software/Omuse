use image::{ImageBuffer, Rgba, RgbaImage};
use omuse::{
    color_management, document,
    editor::Editor,
    precision::WorkingSpace,
    proofing, raw_import,
    recipes::{Recipe, Step},
    smart_source,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn oriented_tiff16_source_rotates_without_quantizing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("portrait.tiff");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut image = encoder
        .new_image::<tiff::encoder::colortype::RGBA16>(2, 3)
        .unwrap();
    image
        .encoder()
        .write_tag(tiff::tags::Tag::Orientation, 6u16)
        .unwrap();
    let data = (0..6)
        .flat_map(|index| [1001 + index * 100, 12003, 45005, 65535])
        .collect::<Vec<u16>>();
    image.write_data(&data).unwrap();
    drop(encoder);
    let state = smart_source::import(&path, Default::default(), &AtomicBool::new(false)).unwrap();
    let pixels = state.source.to_rgba16();
    assert_eq!(pixels.dimensions(), (3, 2));
    assert_eq!(pixels.get_pixel(2, 0).0, [1001, 12003, 45005, 65535]);
    assert_eq!(pixels.get_pixel(0, 1).0, [1501, 12003, 45005, 65535]);
}

#[test]
fn embedded_png16_import_and_working_space_keep_sub_byte_values() {
    let tmp = tempfile::tempdir().unwrap();
    let pixels = ImageBuffer::<Rgba<u16>, Vec<u16>>::from_fn(600, 1, |x, _| {
        Rgba([1000 + x as u16 * 17, 13001, 33003, 65535])
    });
    let path = tmp.path().join("master.png");
    image::DynamicImage::ImageRgba16(pixels.clone())
        .save(&path)
        .unwrap();
    let mut state =
        smart_source::import(&path, Default::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(state.source.to_rgba16(), pixels);
    let profile = color_management::srgb_profile().unwrap();
    let converted = color_management::to_srgb16(&pixels, Some(&profile)).unwrap();
    assert!(
        converted.pixels().zip(pixels.pixels()).all(|(a, b)| a
            .0
            .iter()
            .zip(b.0)
            .all(|(a, b)| a.abs_diff(b) <= 2))
    );
    let source = std::sync::Arc::make_mut(&mut state.source);
    source
        .convert_working_space(WorkingSpace::DisplayP3)
        .unwrap();
    source.convert_working_space(WorkingSpace::Srgb).unwrap();
    assert!(
        source
            .to_rgba16()
            .pixels()
            .zip(pixels.pixels())
            .all(|(a, b)| a.0.iter().zip(b.0).all(|(a, b)| a.abs_diff(b) <= 8))
    );
}

#[test]
fn recipe_adjustment_matches_editor_and_resize_does_not_leak_hidden_rgb() {
    let pixels = RgbaImage::from_fn(16, 12, |x, y| {
        Rgba([(x * 13) as u8, (y * 19) as u8, 80, 255])
    });
    let mut doc = omuse::model::Document::new(16, 12);
    doc.layers[0].image = Some(pixels.clone().into());
    let mut editor = Editor::new(doc);
    let adjust = omuse::editor::Adjustment::Brightness(0.1);
    assert!(editor.adjust(adjust));
    let recipe = Recipe {
        steps: vec![Step::Adjustment { adjustment: adjust }],
        ..Default::default()
    };
    assert_eq!(
        recipe.apply(&pixels, &AtomicBool::new(false)).unwrap(),
        **editor.document.layers[0].image.as_ref().unwrap()
    );
    let edge = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 0])
        }
    });
    let recipe = Recipe {
        steps: vec![Step::Resize {
            width: 1,
            height: 1,
        }],
        ..Default::default()
    };
    let out = recipe.apply(&edge, &AtomicBool::new(false)).unwrap();
    assert_eq!(&out.get_pixel(0, 0).0[..3], &[255, 0, 0]);
    assert!((100..=155).contains(&out.get_pixel(0, 0)[3]));
}

#[test]
fn batch_cancellation_preserves_completed_output_and_originals() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input");
    let output = tmp.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for name in ["a.png", "b.png"] {
        RgbaImage::from_pixel(4, 3, Rgba([30, 80, 140, 255]))
            .save(input.join(name))
            .unwrap();
    }
    let before = std::fs::read(input.join("a.png")).unwrap();
    let cancel = AtomicBool::new(false);
    let report = omuse::recipes::batch(
        &Recipe::default(),
        &input,
        &output,
        "png",
        &cancel,
        |done, _| {
            if done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.items.len(), 1);
    assert!(report.items[0].error.is_none());
    assert!(output.join("a.png.png").is_file());
    assert!(!output.join("b.png.png").exists());
    assert_eq!(before, std::fs::read(input.join("a.png")).unwrap());
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
}

#[test]
fn raw_original_redevelops_after_external_file_is_removed() {
    let Some(fixture) = omuse::identity::env_var_os("OMUSE_RAW_FIXTURE") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let raw = tmp.path().join("source.dng");
    std::fs::copy(fixture, &raw).unwrap();
    let cancel = AtomicBool::new(false);
    let state = smart_source::import(&raw, Default::default(), &cancel).unwrap();
    assert_eq!(
        state.raw_bytes.as_ref().unwrap().as_slice(),
        std::fs::read(&raw).unwrap()
    );
    assert!(
        state
            .source
            .to_rgba16()
            .pixels()
            .take(100_000)
            .any(|p| p.0[..3].iter().any(|v| v % 257 != 0)),
        "RAW must retain genuine sub-byte precision"
    );
    let (w, h) = state.source.dimensions();
    let mut doc = omuse::model::Document::new(w, h);
    doc.layers[0].image = Some(state.proxy().unwrap().into());
    doc.layers[0].advanced = Some(std::sync::Arc::new(state));
    let path = tmp.path().join("raw.comp");
    document::save(&doc, &path).unwrap();
    std::fs::remove_file(&raw).unwrap();
    let loaded = document::open(&path).unwrap();
    let state = loaded.layers[0].advanced.as_ref().unwrap();
    let settings = raw_import::DevelopSettings {
        exposure: 0.75,
        ..Default::default()
    };
    let result = smart_source::redevelop(state, settings, &cancel).unwrap();
    assert_ne!(result.source.to_rgba16(), state.source.to_rgba16());
    assert_eq!(result.raw_bytes, state.raw_bytes);
}

#[test]
fn display_proof_conversion_does_not_mutate_document_pixels() {
    let pixels = RgbaImage::from_pixel(4, 3, Rgba([30, 140, 210, 128]));
    let original = pixels.clone();
    let settings = proofing::Settings::default();
    let _ = proofing::render(&pixels, &settings).unwrap();
    assert_eq!(pixels, original);
}

#[test]
fn oversized_recipe_is_rejected_before_publishing_any_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("too-large.json");
    let points = (0..256)
        .map(|i| (i as f32 / 255., i as f32 / 255.))
        .collect::<Vec<_>>();
    let recipe = Recipe {
        steps: (0..256)
            .map(|_| Step::Filter {
                filter: omuse::filters::Filter::Curves {
                    points: points.clone(),
                },
            })
            .collect(),
        ..Default::default()
    };
    assert!(
        recipe
            .save(&path)
            .unwrap_err()
            .to_string()
            .contains("1 MiB")
    );
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}
