use omuse::{
    document,
    editor::Editor,
    model::Document,
    vector_clip,
    vector_scene::{VectorObject, VectorScene},
};
use std::sync::atomic::AtomicBool;
fn scene() -> VectorScene {
    VectorScene {
        version: 1,
        width: 32,
        height: 32,
        objects: vec![
            VectorObject::rectangle("Artwork", 0., 0., 32., 32., Some([240, 80, 50, 255]), None)
                .unwrap(),
            VectorObject::ellipse("Mask", 8., 8., 16., 16., Some([0, 0, 200, 255]), None).unwrap(),
        ],
    }
}
#[test]
fn clipped_project_roundtrip_undo_redo_and_release_preserve_sources() {
    let mut editor = Editor::new(Document::new(32, 32));
    let source = scene();
    let cancel = AtomicBool::new(false);
    let id = editor
        .insert_vector_scene(
            "Artwork",
            editor.revision(),
            source.clone(),
            source.render(&cancel).unwrap(),
        )
        .unwrap();
    let (clipped, _) = vector_clip::make(&source, &[0, 1]).unwrap();
    let pixels = clipped.render(&cancel).unwrap();
    editor
        .replace_vector_scene(&id, editor.revision(), clipped.clone(), pixels.clone())
        .unwrap();
    assert!(editor.undo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(&source)
    );
    assert!(editor.redo());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Clipped.omuse");
    document::save(&editor.document, &path).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], document::PROJECT_CLIP_VERSION);
    let reopened = document::open(&path).unwrap();
    let layer = reopened.find_layer(&id).unwrap();
    assert_eq!(layer.vector_scene.as_deref(), Some(&clipped));
    assert_eq!(layer.image.as_deref(), Some(&pixels));
    let (released, _) = vector_clip::release(layer.vector_scene.as_ref().unwrap(), &[1]).unwrap();
    assert_eq!(released.objects, source.objects);
    // A manifest downgrade must not disguise editable clipping as old content.
    let mut downgrade = manifest;
    downgrade["version"] = serde_json::json!(14);
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&downgrade).unwrap(),
    )
    .unwrap();
    assert!(document::open(&path).is_err());
}
#[test]
fn svg_clipping_roundtrip_preserves_visible_pixels_and_pdf_is_vector() {
    let (mut clipped, _) = vector_clip::make(&scene(), &[0, 1]).unwrap();
    clipped.objects[1].transform[4] = 3.;
    let svg = omuse::vector_svg_scene::encode_scene(&clipped).unwrap();
    assert!(svg.contains("clipPathUnits=\"userSpaceOnUse\""));
    let imported = omuse::vector_svg_scene::decode_scene(svg.as_bytes()).unwrap();
    assert!(imported.objects.iter().any(|o| o.is_clip));
    let cancel = AtomicBool::new(false);
    assert_eq!(
        clipped.render(&cancel).unwrap(),
        imported.render(&cancel).unwrap()
    );
    let pdf = omuse::vector_pdf::encode_scene(&clipped, 72.).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
}
#[test]
fn svg_external_nested_and_object_bounds_clips_are_rejected() {
    for svg in [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><path d="M0 0H32V32Z" clip-path="url(https://evil.test/a.svg#clip)"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="a" clipPathUnits="objectBoundingBox"><path d="M0 0H1V1Z"/></clipPath></defs><path d="M0 0H32V32Z" clip-path="url(#a)"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="a"><path d=""/></clipPath></defs><path d="M0 0H32V32Z" clip-path="url(#a)"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="a"><path visibility="hidden" d="M0 0H20V20Z"/></clipPath></defs><path d="M0 0H32V32Z" clip-path="url(#a)"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="a"><path d="M0 0H20V20Z"/></clipPath><clipPath id="b"><path d="M0 0H10V10Z"/></clipPath></defs><g clip-path="url(#a)"><path d="M0 0H32V32Z" clip-path="url(#b)"/></g></svg>"##,
    ] {
        assert!(omuse::vector_svg_scene::decode_scene(svg.as_bytes()).is_err());
    }
}

#[test]
fn compound_holes_clips_multiple_styled_objects_and_scaled_preview_are_retained() {
    let mut source = scene();
    let inner =
        VectorObject::rectangle("Hole", 12., 12., 8., 8., Some([0, 0, 0, 255]), None).unwrap();
    source.objects[1].path.subpaths.extend(inner.path.subpaths);
    source.objects[1].path.fill_rule = omuse::vector_path::FillRule::EvenOdd;
    source.objects.insert(
        1,
        VectorObject::rectangle("Second", 0., 0., 16., 32., Some([0, 200, 80, 190]), None).unwrap(),
    );
    let (clipped, _) = vector_clip::make(&source, &[0, 1, 2]).unwrap();
    let cancel = AtomicBool::new(false);
    let full = clipped.render(&cancel).unwrap();
    assert_eq!(full.get_pixel(16, 16)[3], 0);
    assert!(full.get_pixel(10, 16)[3] > 0);
    let svg = omuse::vector_svg_scene::encode_scene(&clipped).unwrap();
    let imported = omuse::vector_svg_scene::decode_scene(svg.as_bytes()).unwrap();
    assert_eq!(full, imported.render(&cancel).unwrap());
    let preview = clipped.render_preview(16, &cancel).unwrap();
    assert_eq!(preview.dimensions(), (16, 16));
    assert_eq!(preview.get_pixel(8, 8)[3], 0);
    assert!(clipped.render(&AtomicBool::new(true)).is_err());
    // Geometry conversion cannot discard a retained reference.
    assert!(
        omuse::vector_boolean::combine(
            &clipped.objects[..2],
            omuse::vector_boolean::BooleanOperation::Union,
            &cancel
        )
        .is_err()
    );
}

#[test]
fn svg_group_clip_and_mask_child_transforms_are_composed_once() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <defs><clipPath id="mask" clipPathUnits="userSpaceOnUse" transform="translate(3 2)">
        <path transform="scale(2)" d="M2 2H8V8H2Z"/></clipPath></defs>
        <g transform="translate(4 3)" clip-path="url(#mask)"><path fill="#e05030" d="M0 0H32V32H0Z"/></g>
        </svg>"##;
    let imported = omuse::vector_svg_scene::decode_scene(svg).unwrap();
    let cancel = AtomicBool::new(false);
    let pixels = imported.render(&cancel).unwrap();
    assert_eq!(pixels.get_pixel(10, 10)[3], 0);
    assert_eq!(pixels.get_pixel(12, 10).0, [224, 80, 48, 255]);
    assert_eq!(pixels.get_pixel(24, 14)[3], 0);
    let exported = omuse::vector_svg_scene::encode_scene(&imported).unwrap();
    let reimported = omuse::vector_svg_scene::decode_scene(exported.as_bytes()).unwrap();
    assert_eq!(reimported.render(&cancel).unwrap(), pixels);
}
