use image::{GrayImage, Luma, Rgba, RgbaImage};
use omuse::editor::{Editor, Selection};
use omuse::model::{Document, Layer};
use omuse::{document, raster};

fn canvas_mask(width: u32, height: u32, value: impl Fn(u32, u32) -> u8) -> GrayImage {
    GrayImage::from_fn(width, height, |x, y| Luma([value(x, y)]))
}

#[test]
fn image_less_adjustment_uses_soft_range_mask_and_round_trips() {
    let mut document = Document::new(3, 1);
    document.layers[0].image = Some(RgbaImage::from_pixel(3, 1, Rgba([80, 80, 80, 255])).into());
    let mut editor = Editor::new(document);
    let id = editor
        .add_adjustment(
            omuse::effects::adjustment_for_filter(&omuse::filters::Filter::Invert).unwrap(),
        )
        .unwrap();
    let mask = canvas_mask(3, 1, |x, _| [0, 128, 255][x as usize]);
    assert!(editor.replace_canvas_mask(&id, &mask).unwrap());
    let output = raster::composite(&editor.document);
    assert_eq!(output.get_pixel(0, 0)[0], 80);
    assert!((127..=128).contains(&output.get_pixel(1, 0)[0]));
    assert_eq!(output.get_pixel(2, 0)[0], 175);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("MaskedAdjustment.comp");
    document::save(&editor.document, &path).unwrap();
    assert_eq!(raster::composite(&document::open(&path).unwrap()), output);
}

#[test]
fn fractional_canvas_projection_preserves_soft_coverage() {
    let mut document = Document::new(6, 6);
    let layer = &mut document.layers[0];
    layer.image = Some(RgbaImage::new(2, 2).into());
    layer.offset_x = 1.25;
    layer.offset_y = 1.0;
    let id = layer.id.clone();
    let mut editor = Editor::new(document);
    let source = canvas_mask(6, 6, |x, y| if x == 1 && y == 1 { 255 } else { 0 });

    assert!(editor.replace_canvas_mask(&id, &source).unwrap());
    let mask = editor
        .document
        .find_layer(&id)
        .unwrap()
        .mask
        .as_ref()
        .unwrap();
    assert_eq!(mask.dimensions(), (2, 2));
    let fractional = mask.get_pixel(0, 0)[0];
    assert!(
        fractional > 0 && fractional < 255,
        "expected soft coverage, got {fractional}"
    );
}

#[test]
fn translated_rotated_flipped_target_and_image_less_group_are_projected() {
    let mut document = Document::new(10, 8);
    let mut layer = Layer::paint("transformed", 4, 3);
    layer.offset_x = 3.0;
    layer.offset_y = 2.0;
    layer.rotation = 27.0;
    layer.scale_x = -1.0;
    layer.scale_y = 1.25;
    let transformed_id = layer.id.clone();
    let mut group = Layer::group("group");
    group.offset_x = 0.75;
    group.rotation = -11.0;
    let group_id = group.id.clone();
    group.children.push(Layer::paint("child", 10, 8));
    document.layers = vec![layer, group];
    let mut editor = Editor::new(document);
    let source = canvas_mask(10, 8, |x, y| if x == 5 && y == 4 { 255 } else { 0 });

    assert!(
        editor
            .replace_canvas_mask(&transformed_id, &source)
            .unwrap()
    );
    let transformed_mask = editor
        .document
        .find_layer(&transformed_id)
        .unwrap()
        .mask
        .as_ref()
        .unwrap();
    assert_eq!(transformed_mask.dimensions(), (4, 3));
    assert!(
        (140..=150).contains(&transformed_mask.get_pixel(1, 1)[0]),
        "rotation/flip projection at (1,1) was {}",
        transformed_mask.get_pixel(1, 1)[0]
    );
    assert!(
        (0..=5).contains(&transformed_mask.get_pixel(2, 1)[0]),
        "rotation/flip projection at (2,1) was {}",
        transformed_mask.get_pixel(2, 1)[0]
    );
    assert!(editor.replace_canvas_mask(&group_id, &source).unwrap());
    assert_eq!(
        editor
            .document
            .find_layer(&group_id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .dimensions(),
        (10, 8)
    );
}

#[test]
fn replacement_is_one_transaction_replaces_live_metadata_and_round_trips() {
    let mut document = Document::new(5, 4);
    let source_id = document.layers[0].id.clone();
    document.layers[0].image = Some(RgbaImage::from_pixel(5, 4, Rgba([20, 30, 40, 255])).into());
    let mut target = Layer::paint("target", 5, 4);
    target.image = Some(RgbaImage::from_pixel(5, 4, Rgba([200, 30, 10, 255])).into());
    target.metadata = serde_json::json!({
        "keep": {"value": 7},
        "maskEnabled": false,
        "maskLinked": false,
        "maskPlacement": {"origin":[1, 1], "size":[3, 2]},
        "maskSourceID": source_id,
    });
    target.mask = Some(RgbaImage::from_pixel(5, 4, Rgba([17; 4])).into());
    let target_id = target.id.clone();
    document.layers.push(target);
    let mut editor = Editor::new(document);
    let before = editor.document.clone();
    editor.selection = Some(Selection {
        width: 5,
        height: 4,
        mask: vec![128; 20],
    });
    let before_selection = editor.selection.clone();
    let source_pixels = before.layers[0].image.clone();
    let replacement = canvas_mask(5, 4, |x, y| ((x + 2 * y) * 23) as u8);

    assert!(
        editor
            .replace_canvas_mask(&target_id, &replacement)
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), 1);
    let changed = editor.document.find_layer(&target_id).unwrap();
    assert_eq!(changed.metadata["keep"], serde_json::json!({"value": 7}));
    assert_eq!(changed.metadata["maskEnabled"], true);
    assert_eq!(changed.metadata["maskLinked"], true);
    assert!(changed.metadata.get("maskPlacement").is_none());
    assert!(changed.metadata.get("maskSourceID").is_none());
    assert_eq!(editor.selection, before_selection);
    assert_eq!(editor.document.layers[0].image, source_pixels);
    let rendered = raster::composite(&editor.document);

    assert!(editor.undo());
    assert_eq!(editor.document.layers[1].mask, before.layers[1].mask);
    assert_eq!(
        editor.document.layers[1].metadata,
        before.layers[1].metadata
    );
    assert!(editor.redo());
    assert_eq!(raster::composite(&editor.document), rendered);
    let depth = editor.undo_depth();
    assert!(
        !editor
            .replace_canvas_mask(&target_id, &replacement)
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), depth);

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("range-mask.comp");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(raster::composite(&reopened), rendered);
}

#[test]
fn locks_invalid_inputs_and_floating_selection_do_not_mutate_history() {
    let mut document = Document::new(6, 6);
    let mut group = Layer::group("locked group");
    group.locked = true;
    let id = group.id.clone();
    group.children.push(Layer::paint("child", 6, 6));
    document.layers = vec![group];
    let mut editor = Editor::new(document);
    let source = canvas_mask(6, 6, |_, _| 255);
    let before = editor.document.clone();
    assert!(!editor.replace_canvas_mask(&id, &source).unwrap());
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(editor.document.layers[0].mask, before.layers[0].mask);

    let mut invalid_transform = Editor::new(Document::new(6, 6));
    let invalid_id = invalid_transform.active_layer.clone();
    invalid_transform
        .document
        .find_layer_mut(&invalid_id)
        .unwrap()
        .scale_x = 0.0;
    let invalid_before = invalid_transform.document.clone();
    let error = invalid_transform
        .replace_canvas_mask(&invalid_id, &source)
        .unwrap_err();
    assert!(error.to_string().contains("transform"));
    assert_eq!(invalid_transform.undo_depth(), 0);
    assert_eq!(
        invalid_transform.document.layers[0].image,
        invalid_before.layers[0].image
    );

    let wrong_size = GrayImage::new(5, 6);
    let error = editor.replace_canvas_mask(&id, &wrong_size).unwrap_err();
    assert!(error.to_string().contains("dimensions"));
    assert_eq!(editor.undo_depth(), 0);

    let mut floating_editor = Editor::new(Document::new(6, 6));
    let child_id = floating_editor.active_layer.clone();
    floating_editor.select_rectangle(0.0, 0.0, 2.0, 2.0);
    assert!(
        floating_editor
            .begin_floating_selection()
            .unwrap()
            .is_some()
    );
    let error = floating_editor
        .replace_canvas_mask(&child_id, &source)
        .unwrap_err();
    assert!(error.to_string().contains("floating selection"));
    assert_eq!(floating_editor.undo_depth(), 0);
}

#[test]
fn oversize_canvas_source_is_rejected_before_projection() {
    let (width, height) = (30_000, 560);
    let document = Document {
        width,
        height,
        name: "oversize source".into(),
        background: [0; 4],
        layers: vec![Layer::group("group")],
        metadata: serde_json::json!({}),
    };
    let mut editor = Editor::new(document);
    let source = GrayImage::new(width, height);
    let id = editor.document.layers[0].id.clone();
    let error = editor.replace_canvas_mask(&id, &source).unwrap_err();
    assert!(error.to_string().contains("16 million"));
    assert_eq!(editor.undo_depth(), 0);
}
