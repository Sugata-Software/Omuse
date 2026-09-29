use image::{Rgba, RgbaImage};
use omuse::{
    editor::Editor,
    model::{Document, Layer},
};
use serde_json::json;

fn photo() -> Document {
    let mut doc = Document::new(8, 6);
    doc.layers[0].image = Some(RgbaImage::from_pixel(8, 6, Rgba([210, 45, 23, 255])).into());
    doc
}

fn refuses_unchanged(mut editor: Editor) {
    editor.select_rect(2, 1, 3, 3);
    let before = format!("{:?}", editor.document);
    let selection = editor.selection.clone();
    let history = (
        editor.undo_depth(),
        editor.redo_depth(),
        editor.revision(),
        editor.is_dirty(),
    );
    assert!(editor.validate_pixel_cut().is_err());
    assert!(!editor.cut_selection());
    assert_eq!(format!("{:?}", editor.document), before);
    assert_eq!(editor.selection, selection);
    assert_eq!(
        (
            editor.undo_depth(),
            editor.redo_depth(),
            editor.revision(),
            editor.is_dirty()
        ),
        history
    );
}

#[test]
fn hidden_masked_and_reduced_opacity_pixels_are_not_silently_erased() {
    for case in 0..3 {
        let mut doc = photo();
        match case {
            0 => doc.layers[0].visible = false,
            1 => {
                doc.layers[0].mask = Some(RgbaImage::from_pixel(8, 6, Rgba([0, 0, 0, 255])).into())
            }
            _ => doc.layers[0].opacity = 0.3,
        }
        let mut e = Editor::new(doc);
        e.select_rect(2, 1, 3, 3);
        let copied = e
            .copy_selection()
            .expect("appearance Copy remains available");
        assert!(copied.get_pixel(1, 1)[3] < 255);
        refuses_unchanged(e);
    }
}

#[test]
fn appearance_metadata_and_fractional_or_scaled_transforms_refuse_pixel_cut() {
    for case in 0..7 {
        let mut doc = photo();
        let layer = &mut doc.layers[0];
        match case {
            0 => layer.metadata["maskSourceID"] = json!("another layer"),
            1 => layer.metadata["effects"] = json!({"colorOverlay": {"enabled": true}}),
            2 => layer.metadata["rustBlendIf"] = json!({}),
            3 => layer.scale_x = 0.5,
            4 => layer.rotation = 30.0,
            5 => layer.offset_x = 0.25,
            _ => layer.blend_mode = "Multiply".into(),
        }
        refuses_unchanged(Editor::new(doc));
    }
}

#[test]
fn locked_or_masked_ancestors_and_live_layers_cannot_be_cut_as_pixels() {
    // Plain child source pixels must remain intact even if an ancestor hides
    // them or disallows edits. The public clipboard is tested at the UI seam.
    for case in 0..5 {
        let mut doc = photo();
        let child = doc.layers.remove(0);
        let id = child.id.clone();
        let mut parent = Layer::group("Parent");
        match case {
            0 => parent.locked = true,
            1 => parent.mask = Some(RgbaImage::from_pixel(8, 6, Rgba([0, 0, 0, 255])).into()),
            2 => parent.offset_x = 2.0,
            3 => parent.opacity = 0.5,
            _ => parent.visible = false,
        }
        parent.children.push(child);
        doc.layers.push(parent);
        let mut e = Editor::new(doc);
        e.active_layer = id;
        refuses_unchanged(e);
    }
    let mut doc = photo();
    doc.layers[0].metadata["text"] = json!({"text": "Keep editable"});
    refuses_unchanged(Editor::new(doc));
}

#[test]
fn ordinary_integer_translated_cut_preserves_off_canvas_source_and_undo() {
    let mut doc = photo();
    doc.layers[0].offset_x = -2.0;
    let mut e = Editor::new(doc);
    let original = e.document.layers[0].image.clone();
    let copied = e.copy_selection().unwrap();
    assert_eq!(copied.get_pixel(0, 0).0, [210, 45, 23, 255]);
    assert!(e.validate_pixel_cut().is_ok());
    assert!(e.cut_selection());
    let source = e.document.layers[0].image.as_ref().unwrap();
    assert_eq!(source.get_pixel(0, 0).0, [210, 45, 23, 255]);
    assert_eq!(source.get_pixel(2, 0).0, [0; 4]);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(e.document.layers[0].image, original);
}
