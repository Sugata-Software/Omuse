use omuse::{
    compatibility_report::{CompatibilityReport, from_metadata},
    document,
    editor::Editor,
    import_report::conversion_notes,
    model::Document,
    vector_svg_scene,
};
use std::{path::Path, sync::atomic::AtomicBool};

fn imported() -> vector_svg_scene::ImportedScene {
    vector_svg_scene::decode_scene_with_report(br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><path fill="#cc5500" d="M2 2L18 2L10 18Z"/></svg>"##).unwrap()
}

#[test]
fn import_compatibility_survives_project_save_and_tracks_geometry_undo() {
    let source = imported();
    let report = CompatibilityReport::svg_import(
        Path::new("brand.svg"),
        Some(&source.scene),
        &source.warnings,
    );
    let pixels = source.scene.render(&AtomicBool::new(false)).unwrap();
    let mut editor = Editor::new(Document::new(24, 24));
    let depth = editor.undo_depth();
    let id = editor
        .insert_vector_scene_with_report(
            "Brand",
            editor.revision(),
            source.scene.clone(),
            pixels,
            Some(&report),
        )
        .unwrap();
    assert_eq!(editor.undo_depth(), depth + 1);
    assert!(
        conversion_notes(&editor.document)
            .iter()
            .any(|note| note.contains("brand.svg"))
    );
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("brand.omuse");
    document::save(&editor.document, &project).unwrap();
    let reopened = document::open(&project).unwrap();
    assert_eq!(
        from_metadata(&reopened.find_layer(&id).unwrap().metadata),
        Some(report.clone())
    );
    assert!(editor.undo());
    assert!(
        !conversion_notes(&editor.document)
            .iter()
            .any(|note| note.contains("brand.svg"))
    );
    assert!(editor.redo());

    let mut changed = source.scene;
    changed.objects[0].name = "Changed".into();
    let next = CompatibilityReport::svg_import(
        Path::new("revision.svg"),
        Some(&changed),
        &["Example conversion recorded".into()],
    );
    let pixels = changed.render(&AtomicBool::new(false)).unwrap();
    assert!(
        editor
            .replace_vector_scene_with_report(&id, editor.revision(), changed, pixels, Some(&next))
            .unwrap()
    );
    assert_eq!(
        from_metadata(&editor.document.find_layer(&id).unwrap().metadata),
        Some(next)
    );
    assert!(editor.undo());
    assert_eq!(
        from_metadata(&editor.document.find_layer(&id).unwrap().metadata),
        Some(report)
    );
}

#[test]
fn rejected_report_cannot_partially_change_document_or_history() {
    let source = imported();
    let pixels = source.scene.render(&AtomicBool::new(false)).unwrap();
    let mut editor = Editor::new(Document::new(24, 24));
    let before = editor.document.layers.len();
    let depth = editor.undo_depth();
    let mut report =
        CompatibilityReport::svg_import(Path::new("brand.svg"), Some(&source.scene), &[]);
    report.converted.push("x".repeat(1000));
    assert!(
        editor
            .insert_vector_scene_with_report(
                "Brand",
                editor.revision(),
                source.scene,
                pixels,
                Some(&report)
            )
            .is_err()
    );
    assert_eq!(editor.document.layers.len(), before);
    assert_eq!(editor.undo_depth(), depth);
}

#[test]
fn svg_and_pdf_reports_explain_artwork_only_scope_without_claiming_ai_interchange() {
    let source = imported();
    for (pdf, path) in [(false, "brand.svg"), (true, "brand.pdf")] {
        let report = CompatibilityReport::vector_export(Path::new(path), Some(&source.scene), pdf);
        let lines = report.lines().join("\n");
        assert!(lines.contains("Other document layers"));
        assert!(lines.contains("round trip is not guaranteed"));
        assert!(lines.contains("Save .omuse"));
        assert!(
            !report
                .preserved
                .iter()
                .any(|line| line.contains("Illustrator"))
        );
    }
}

#[test]
fn rasterized_import_report_keeps_damage_composition_fast_and_pixel_exact() {
    use omuse::{compatibility_report::METADATA_KEY, model::PixelRect, raster};
    let source = imported();
    let report = CompatibilityReport::svg_import(Path::new("brand.svg"), Some(&source.scene), &[]);
    let pixels = source.scene.render(&AtomicBool::new(false)).unwrap();
    let mut editor = Editor::new(Document::new(24, 24));
    let id = editor
        .insert_vector_scene_with_report(
            "Brand",
            editor.revision(),
            source.scene,
            pixels,
            Some(&report),
        )
        .unwrap();
    assert!(editor.rasterize_layer(&id));
    let expected = raster::composite(&editor.document);
    let sentinel = image::Rgba([7, 13, 19, 31]);
    let mut target = image::RgbaImage::from_pixel(24, 24, sentinel);
    let region = PixelRect {
        x: 3,
        y: 2,
        width: 8,
        height: 9,
    };
    assert!(raster::composite_region(
        &editor.document,
        &mut target,
        region
    ));
    for (x, y, pixel) in target.enumerate_pixels() {
        assert_eq!(
            *pixel,
            if (3..11).contains(&x) && (2..11).contains(&y) {
                *expected.get_pixel(x, y)
            } else {
                sentinel
            }
        );
    }
    for invalid in [
        serde_json::json!({"unrecognized": true}),
        serde_json::json!({
            "operation": "Import", "file": "brand.svg", "preserved": [], "converted": [],
            "not_included": [], "blocked": ["bad\ncontrol"]
        }),
    ] {
        editor.document.find_layer_mut(&id).unwrap().metadata[METADATA_KEY] = invalid;
        let unchanged = target.clone();
        assert!(!raster::composite_region(
            &editor.document,
            &mut target,
            region
        ));
        assert_eq!(target, unchanged);
    }
    let metadata = &mut editor.document.find_layer_mut(&id).unwrap().metadata;
    metadata[METADATA_KEY] = serde_json::to_value(report).unwrap();
    metadata["unrecognized-rendering-extension"] = serde_json::json!({});
    let unchanged = target.clone();
    assert!(!raster::composite_region(
        &editor.document,
        &mut target,
        region
    ));
    assert_eq!(target, unchanged);
}
