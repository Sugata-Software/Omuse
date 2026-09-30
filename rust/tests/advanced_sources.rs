use image::{ImageBuffer, Rgba, RgbaImage};
use omuse::{
    color_management,
    create_project::Project,
    document,
    editor::Editor,
    model::Document,
    precision::WorkingSpace,
    proofing, raster, raw_import,
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

fn batch_document(width: u32, height: u32, color: [u8; 4]) -> Document {
    let mut document = Document::new(width, height);
    document.layers[0].image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
    document
}

#[test]
fn batch_processes_omuse_and_legacy_packages_in_order_without_changing_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input");
    let output = tmp.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    let fixtures = [
        ("a.omuse", [30, 80, 140, 255]),
        ("b.comp", [90, 150, 20, 255]),
        ("c.OMUSE", [200, 30, 110, 255]),
    ];
    let mut manifests = Vec::new();
    for (name, color) in fixtures {
        let path = input.join(name);
        document::save(&batch_document(4, 3, color), &path).unwrap();
        let manifest_path = path.join("manifest.json");
        if name.ends_with(".comp") {
            // Preserve coverage of the actual legacy manifest, independent of
            // the format emitted by new Omuse saves.
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
            manifest["format"] = serde_json::json!("com.compositor.project");
            std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        }
        manifests.push(std::fs::read(manifest_path).unwrap());
    }
    let recipe = Recipe {
        steps: vec![Step::Filter {
            filter: omuse::filters::Filter::Invert,
        }],
        ..Default::default()
    };
    let cancel = AtomicBool::new(false);
    let mut progress = Vec::new();
    let report = omuse::recipes::batch(&recipe, &input, &output, "png", &cancel, |done, total| {
        progress.push((done, total));
    })
    .unwrap();
    assert!(!report.cancelled);
    assert_eq!(report.items.len(), fixtures.len());
    assert_eq!(progress, vec![(1, 3), (2, 3), (3, 3)]);
    for ((item, (name, color)), manifest) in report.items.iter().zip(fixtures).zip(manifests) {
        assert_eq!(item.input, input.join(name));
        assert_eq!(item.output, output.join(format!("{name}.png")));
        assert!(item.error.is_none(), "{name}: {:?}", item.error);
        let source = RgbaImage::from_pixel(4, 3, Rgba(color));
        assert_eq!(
            image::open(&item.output).unwrap().to_rgba8(),
            recipe.apply(&source, &cancel).unwrap()
        );
        assert_eq!(
            raster::composite(&document::open(&item.input).unwrap()),
            source
        );
        assert_eq!(
            std::fs::read(item.input.join("manifest.json")).unwrap(),
            manifest
        );
    }
}

#[test]
fn batch_exports_only_the_collections_saved_active_page() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input");
    let output = tmp.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    let mut project = Project::new("Collection", batch_document(4, 3, [210, 30, 40, 255]));
    let active_pixels = RgbaImage::from_pixel(2, 5, Rgba([20, 130, 210, 255]));
    let active_id = project
        .add_page("Selected page", batch_document(2, 5, [20, 130, 210, 255]))
        .unwrap();
    project.set_active_page(&active_id).unwrap();
    let path = input.join("collection.omuse");
    project.save(&path).unwrap();
    let before = std::fs::read(path.join("project.json")).unwrap();
    let report = omuse::recipes::batch(
        &Recipe::default(),
        &input,
        &output,
        "png",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(report.items.len(), 1, "nested pages must not be enumerated");
    assert!(
        report.items[0].error.is_none(),
        "{:?}",
        report.items[0].error
    );
    assert_eq!(
        image::open(output.join("collection.omuse.png"))
            .unwrap()
            .to_rgba8(),
        active_pixels
    );
    assert_eq!(std::fs::read(path.join("project.json")).unwrap(), before);
    let mut reopened = Project::open(&path).unwrap();
    assert_eq!(reopened.active_page_id(), active_id);
    assert_eq!(reopened.page_summaries().len(), 2);
    assert_eq!(
        raster::composite(reopened.active_document().unwrap()),
        active_pixels
    );
}

#[test]
fn batch_package_errors_and_collisions_preserve_outputs_and_continue() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input");
    let output = tmp.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    let invalid = input.join("a-invalid.omuse");
    std::fs::create_dir(&invalid).unwrap();
    std::fs::write(invalid.join("manifest.json"), b"invalid manifest").unwrap();
    let source = batch_document(4, 3, [30, 80, 140, 255]);
    document::save(&source, &input.join("b-existing.omuse")).unwrap();
    document::save(&source, &input.join("c-valid.omuse")).unwrap();
    let preserved = output.join("b-existing.omuse.png");
    std::fs::write(&preserved, b"preserved output").unwrap();
    let report = omuse::recipes::batch(
        &Recipe::default(),
        &input,
        &output,
        "png",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(report.items.len(), 3);
    assert!(report.items[0].error.is_some());
    assert_eq!(
        report.items[1].error.as_deref(),
        Some("Output already exists")
    );
    assert!(report.items[2].error.is_none());
    assert!(!output.join("a-invalid.omuse.png").exists());
    assert_eq!(std::fs::read(preserved).unwrap(), b"preserved output");
    assert_eq!(
        image::open(output.join("c-valid.omuse.png"))
            .unwrap()
            .to_rgba8(),
        raster::composite(&source)
    );
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 2);
}

#[test]
fn batch_rejects_an_oversized_collection_page_before_decoding_it() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input");
    let output = tmp.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    let path = input.join("oversized.omuse");
    Project::new("Collection", batch_document(4, 3, [30, 80, 140, 255]))
        .save(&path)
        .unwrap();
    let manifest_path = path.join("project.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    // The lazy page is deliberately much smaller. The recipe limit must be
    // reported from the declared dimensions before its pixels are decoded.
    manifest["pages"][0]["width"] = serde_json::json!(8193);
    manifest["pages"][0]["height"] = serde_json::json!(2048);
    std::fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let report = omuse::recipes::batch(
        &Recipe::default(),
        &input,
        &output,
        "png",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(report.items.len(), 1);
    assert_eq!(
        report.items[0].error.as_deref(),
        Some("Recipe source exceeds 16 million pixels")
    );
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 0);
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
