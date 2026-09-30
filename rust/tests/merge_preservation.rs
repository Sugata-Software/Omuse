//! Merging must not destroy source pixels retained outside a cropped canvas.
use image::{Rgba, RgbaImage};
use omuse::{
    editor::Editor,
    model::{Document, Layer},
    raster,
};

#[test]
fn off_canvas_merge_refusal_preserves_source_geometry_and_existing_undo() {
    for (width, height, x, y, rotation, scale_x) in [
        (20, 8, -2., 3., 0., 1.),
        (8, 8, 14., 3., 0., 1.),
        (8, 8, 3., -1., 0., 1.),
        (16, 16, 0., 0., 45., 1.),
        (20, 8, -2., 3., 0., -1.),
    ] {
        let mut document = Document::new(16, 16);
        let mut source = Layer::paint("Original source", width, height);
        let pixels = RgbaImage::from_fn(width, height, |x, y| Rgba([x as u8, y as u8, 91, 255]));
        source.image = Some(pixels.clone().into());
        source.offset_x = x;
        source.offset_y = y;
        source.rotation = rotation;
        source.scale_x = scale_x;
        let id = source.id.clone();
        document.layers.push(source);
        let mut editor = Editor::new(document);
        editor.select_layer(&id);
        assert!(editor.rename_layer(&id, "Keep this edit"));
        let revision = editor.revision();
        let preview = raster::composite(&editor.document);
        assert!(
            editor
                .merge_down_preservation_reason()
                .unwrap()
                .contains("outside the canvas")
        );
        assert!(!editor.merge_down());
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.document.layers.len(), 2);
        assert_eq!(raster::composite(&editor.document), preview);
        let retained = editor.document.find_layer(&id).unwrap();
        assert_eq!(retained.image.as_deref(), Some(&pixels));
        assert_eq!(
            (
                retained.offset_x,
                retained.offset_y,
                retained.rotation,
                retained.scale_x
            ),
            (x, y, rotation, scale_x)
        );
        assert!(editor.undo());
        let retained = editor.document.find_layer(&id).unwrap();
        assert_eq!(retained.name, "Original source");
        assert_eq!(retained.image.as_deref(), Some(&pixels));
        assert!(!editor.can_undo());
        assert!(editor.redo());
        assert_eq!(
            editor.document.find_layer(&id).unwrap().name,
            "Keep this edit"
        );
    }
}

#[test]
fn off_canvas_lower_layer_and_external_effect_bounds_are_also_preserved() {
    let mut document = Document::new(16, 16);
    document.layers[0].offset_x = -1.;
    let upper = Layer::paint("Upper", 8, 8);
    let id = upper.id.clone();
    document.layers.push(upper);
    let mut editor = Editor::new(document);
    editor.select_layer(&id);
    assert!(!editor.merge_down());
    editor.document.layers[0].offset_x = 0.;
    editor.document.layers[1].metadata["effects"] = serde_json::json!({"outerGlow":{"size":2.0}});
    assert!(editor.merge_down_preservation_reason().is_some());
    assert!(!editor.merge_down());
    assert!(!editor.can_undo());
    assert_eq!(editor.document.layers.len(), 2);
}

#[test]
fn fully_contained_merge_keeps_preview_and_remains_one_undo_step() {
    let mut document = Document::new(16, 16);
    let mut upper = Layer::paint("Upper", 8, 8);
    upper.image = Some(RgbaImage::from_pixel(8, 8, Rgba([255, 30, 80, 220])).into());
    upper.offset_x = 4.;
    upper.offset_y = 4.;
    upper.rotation = 90.;
    let id = upper.id.clone();
    document.layers.push(upper);
    let mut editor = Editor::new(document);
    editor.select_layer(&id);
    let preview = raster::composite(&editor.document);
    assert!(editor.merge_down_preservation_reason().is_none());
    assert!(editor.merge_down());
    assert_eq!(raster::composite(&editor.document), preview);
    assert!(editor.undo());
    assert_eq!(editor.document.layers.len(), 2);
    assert!(editor.document.find_layer(&id).is_some());
    assert!(!editor.can_undo());
}

#[test]
fn merge_refuses_external_live_mask_dependents_without_corrupting_history() {
    for source_is_upper in [false, true] {
        let mut document = Document::new(16, 16);
        document.layers[0].image =
            Some(RgbaImage::from_pixel(16, 16, Rgba([20, 40, 60, 255])).into());
        let lower = document.layers[0].id.clone();
        let mut upper = Layer::paint("Upper", 16, 16);
        upper.image = Some(RgbaImage::from_pixel(16, 16, Rgba([200, 90, 30, 128])).into());
        let upper_id = upper.id.clone();
        document.layers.push(upper);
        let mut dependent = Layer::paint("Dependent", 16, 16);
        dependent.image = Some(RgbaImage::from_pixel(16, 16, Rgba([0, 255, 0, 255])).into());
        let source = if source_is_upper { &upper_id } else { &lower };
        dependent.metadata["maskSourceID"] = serde_json::json!(source);
        let dependent_id = dependent.id.clone();
        // Exercise a dependency outside the pair's sibling list as well.
        let mut group = Layer::group("Other group");
        group.children.push(dependent);
        document.layers.push(group);
        let mut editor = Editor::new(document);
        editor.select_layer(&upper_id);
        assert!(raster::validate(&editor.document).is_empty());
        assert!(editor.rename_layer(&upper_id, "Prior edit"));
        let revision = editor.revision();
        let preview = raster::composite(&editor.document);
        assert!(
            editor
                .merge_down_preservation_reason()
                .unwrap()
                .contains("live-mask source")
        );
        assert!(!editor.merge_down());
        assert_eq!(editor.revision(), revision);
        assert_eq!(raster::composite(&editor.document), preview);
        assert!(raster::validate(&editor.document).is_empty());
        assert_eq!(
            editor.document.find_layer(&dependent_id).unwrap().metadata["maskSourceID"],
            serde_json::json!(source)
        );
        assert!(editor.undo());
        assert_eq!(editor.document.find_layer(&upper_id).unwrap().name, "Upper");
        assert!(!editor.can_undo());
        assert!(editor.set_live_mask_source(&dependent_id, None).unwrap());
        assert!(editor.merge_down_preservation_reason().is_none());
        assert!(editor.merge_down());
        assert!(raster::validate(&editor.document).is_empty());
    }
}
