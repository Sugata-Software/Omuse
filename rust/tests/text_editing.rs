use omuse::{
    document,
    editor::{Editor, LayerPlacement},
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint},
};

fn text_layer(content: &str) -> Layer {
    let mut style = LiveTextStyle::default();
    style.content = content.into();
    objects::live_text_layer("Editable", ObjectPoint { x: 0.0, y: 0.0 }, style).unwrap()
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}

#[test]
fn point_text_edit_keeps_anchor_transform_metadata_and_is_one_undo() {
    let mut layer = text_layer("Hi");
    layer.opacity = 0.42;
    layer.blend_mode = "Multiply".into();
    layer.mask = Some(image::RgbaImage::from_pixel(24, 18, image::Rgba([255; 4])).into());
    layer.metadata["effects"] = serde_json::json!({"stroke": {"size": 2.0}});
    layer.metadata["applicationNote"] = serde_json::json!({"keep": true});
    let id = layer.id.clone();
    let mut doc = Document::new(800, 600);
    doc.layers = vec![layer];
    let mut editor = Editor::new(doc);
    let placed = LayerPlacement {
        x: 121.0,
        y: 83.0,
        width: 96.0,
        height: 57.0,
        rotation: 31.0,
        flip_x: true,
        flip_y: false,
    };
    assert!(editor.set_layer_placement(&id, placed));
    let before_layer = editor.document.find_layer(&id).unwrap().clone();
    let before_anchor = editor.layer_placement(&id).unwrap().point(0.0, 0.0);
    let before_scale = (before_layer.scale_x, before_layer.scale_y);
    let depth = editor.undo_depth();

    let mut edited = objects::live_text(&before_layer).unwrap().unwrap();
    edited.content = "A substantially longer line of editable text".into();
    assert!(editor.set_live_text(&id, edited.clone()).unwrap());

    let after = editor.document.find_layer(&id).unwrap();
    let after_place = editor.layer_placement(&id).unwrap();
    let after_anchor = after_place.point(0.0, 0.0);
    close(after_anchor.0, before_anchor.0);
    close(after_anchor.1, before_anchor.1);
    close(after.scale_x, before_scale.0);
    close(after.scale_y, before_scale.1);
    assert_eq!(after_place.rotation, placed.rotation);
    assert!(after_place.flip_x);
    assert!(!after_place.flip_y);
    assert!(after_place.width > placed.width);
    assert_eq!(after.opacity, 0.42);
    assert_eq!(after.blend_mode, "Multiply");
    assert_eq!(after.metadata["effects"], before_layer.metadata["effects"]);
    assert_eq!(
        after.metadata["applicationNote"],
        serde_json::json!({"keep": true})
    );
    assert_eq!(objects::live_text(after).unwrap(), Some(edited));
    assert_eq!(editor.mask_placement(&id), Some(placed));
    assert!(after.metadata.get("maskPlacement").is_some());
    assert_eq!(editor.undo_depth(), depth + 1);
    assert!(editor.undo());
    assert_eq!(
        editor.document.find_layer(&id).unwrap().image,
        before_layer.image
    );
    assert_eq!(
        editor.document.find_layer(&id).unwrap().metadata,
        before_layer.metadata
    );
    assert_eq!(editor.layer_placement(&id), Some(placed));
}

#[test]
fn no_op_and_render_error_do_not_mutate_or_add_history() {
    let layer = text_layer("Stable");
    let id = layer.id.clone();
    let mut doc = Document::new(200, 100);
    doc.layers = vec![layer];
    let mut editor = Editor::new(doc);
    let original = editor.document.find_layer(&id).unwrap().clone();
    let style = objects::live_text(&original).unwrap().unwrap();
    assert!(!editor.set_live_text(&id, style.clone()).unwrap());
    assert_eq!(editor.undo_depth(), 0);

    let mut too_wide = style;
    too_wide.content = "W".repeat(10_000);
    too_wide.font_size = 100.0;
    assert!(editor.set_live_text(&id, too_wide).is_err());
    let after = editor.document.find_layer(&id).unwrap();
    assert_eq!(after.image, original.image);
    assert_eq!(after.metadata, original.metadata);
    assert_eq!(editor.undo_depth(), 0);
    assert!(!editor.is_dirty());
}

#[test]
fn ancestor_lock_rejects_edit_and_mask_relationship_survives_edit_and_reopen() {
    let source = text_layer("Mask source");
    let source_id = source.id.clone();
    let mut dependent = Layer::paint("Dependent", 40, 30);
    dependent.locked = true;
    dependent.metadata["maskSourceID"] = serde_json::json!(source_id);
    let dependent_id = dependent.id.clone();
    let mut group = Layer::group("Locked group");
    group.locked = true;
    group.children.push(source.clone());
    let mut locked_doc = Document::new(200, 100);
    locked_doc.layers = vec![group];
    let mut locked_editor = Editor::new(locked_doc);
    let mut style = objects::live_text(&source).unwrap().unwrap();
    style.content = "Blocked".into();
    assert!(!locked_editor.set_live_text(&source_id, style).unwrap());
    assert_eq!(locked_editor.undo_depth(), 0);

    let mut doc = Document::new(200, 100);
    doc.layers = vec![source, dependent];
    let mut editor = Editor::new(doc);
    let mut style = objects::live_text(editor.document.find_layer(&source_id).unwrap())
        .unwrap()
        .unwrap();
    style.content = "Updated mask source".into();
    assert!(editor.set_live_text(&source_id, style.clone()).unwrap());
    assert_eq!(
        editor.document.find_layer(&dependent_id).unwrap().metadata["maskSourceID"],
        serde_json::json!(source_id)
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Edited.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(
        objects::live_text(reopened.find_layer(&source_id).unwrap()).unwrap(),
        Some(style)
    );
    assert_eq!(
        reopened.find_layer(&dependent_id).unwrap().metadata["maskSourceID"],
        serde_json::json!(source_id)
    );
}
