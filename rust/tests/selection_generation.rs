use image::{GrayImage, Luma, Rgba, RgbaImage};
use omuse::{editor::Editor, model::Document, selection_tools::SelectionMode};

fn editor() -> Editor {
    let mut document = Document::new(24, 20);
    document.layers[0].image = Some(RgbaImage::from_pixel(24, 20, Rgba([120, 90, 30, 255])).into());
    Editor::new(document)
}
fn changed(editor: &mut Editor, operation: impl FnOnce(&mut Editor)) {
    let generation = editor.selection_revision();
    let instance = editor.instance_id();
    operation(editor);
    assert!(
        editor.selection_revision() > generation,
        "Selection API must invalidate display work before GUI Undo recording"
    );
    assert_eq!(editor.instance_id(), instance);
}

#[test]
fn direct_selection_mutations_invalidate_async_display_without_creating_history() {
    let mut editor = editor();
    changed(&mut editor, |e| e.select_all());
    changed(&mut editor, |e| e.select_rectangle(3., 4., 12., 10.));
    changed(&mut editor, |e| e.invert_selection());
    changed(&mut editor, |e| {
        assert!(e.feather_selection(1.));
    });
    changed(&mut editor, |e| {
        assert!(e.resize_selection(1));
    });
    changed(&mut editor, |e| e.select_ellipse(2., 3., 14., 12.));
    changed(&mut editor, |e| {
        assert!(e.select_polygon(&[(3., 3.), (18., 3.), (10., 16.)]));
    });
    changed(&mut editor, |e| {
        e.combine_selection(None, SelectionMode::Replace)
    });
    changed(&mut editor, |e| {
        assert!(e.wand_select(8, 8, 0));
    });
    changed(&mut editor, |e| {
        let id = e.active_layer.clone();
        assert!(
            e.apply_subject_mask(&id, &GrayImage::from_pixel(24, 20, Luma([200])), true)
                .unwrap()
        );
    });
    changed(&mut editor, |e| e.clear_selection());
    assert_eq!(editor.undo_depth(), 0);
}

#[test]
fn floating_commit_and_undo_invalidate_contours_but_not_editor_identity() {
    let mut editor = editor();
    editor.select_rectangle(4., 4., 5., 5.);
    let generation = editor.selection_revision();
    let instance = editor.instance_id();
    let floating = editor.begin_floating_selection().unwrap().unwrap();
    assert!(editor.transform_layer(&floating, 10., 7., 0., 1., 1.));
    assert!(editor.commit_floating_selection().unwrap());
    assert!(editor.selection_revision() > generation);
    assert_eq!(
        editor.selection.as_ref().unwrap().bounds(),
        Some((10, 7, 5, 5))
    );
    let committed = editor.selection_revision();
    assert!(editor.undo());
    assert!(editor.selection_revision() > committed);
    assert_eq!(editor.instance_id(), instance);
    assert_eq!(
        editor.selection.as_ref().unwrap().bounds(),
        Some((4, 4, 5, 5))
    );
    assert_ne!(Editor::new(editor.document.clone()).instance_id(), instance);
}
