use image::Rgba;
use omuse::editor::Editor;
use omuse::model::Document;

fn editor_with_block() -> Editor {
    let mut editor = Editor::new(Document::new(8, 8));
    let image = editor
        .document
        .find_layer_mut(&editor.active_layer)
        .unwrap()
        .image
        .as_mut()
        .unwrap();
    for y in 2..4 {
        for x in 2..4 {
            image.put_pixel(x, y, Rgba([200, 30, 10, 255]));
        }
    }
    editor.select_rectangle(2., 2., 2., 2.);
    editor
}

#[test]
fn floating_move_commits_as_one_undo_and_cancel_is_exact() {
    let mut editor = editor_with_block();
    let original = editor.document.clone();
    let floating = editor.begin_floating_selection().unwrap().unwrap();
    assert_eq!(editor.floating_selection_layer(), Some(floating.as_str()));
    let mut placement = editor.layer_placement(&floating).unwrap();
    placement.x += 2.;
    assert!(editor.set_layer_placement(&floating, placement));
    assert_eq!(
        editor.undo_depth(),
        0,
        "intermediate transform must not enter history"
    );
    assert!(editor.commit_floating_selection().unwrap());
    assert_eq!(editor.undo_depth(), 1);
    assert_eq!(editor.document.layers.len(), 1);
    assert_eq!(
        editor.document.layers[0]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(2, 2)
            .0,
        [0; 4]
    );
    assert_eq!(
        editor.document.layers[0]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(4, 2)
            .0,
        [200, 30, 10, 255]
    );
    assert!(editor.undo());
    assert_eq!(editor.document.layers[0].image, original.layers[0].image);

    editor.select_rectangle(2., 2., 2., 2.);
    editor.begin_floating_selection().unwrap().unwrap();
    assert!(editor.cancel_floating_selection());
    assert_eq!(editor.document.layers[0].image, original.layers[0].image);
    assert_eq!(editor.undo_depth(), 0);
}

#[test]
fn unchanged_soft_selection_restores_exact_pixels_without_history() {
    let mut editor = editor_with_block();
    editor.selection.as_mut().unwrap().mask[2 * 8 + 2] = 96;
    let original = editor.document.clone();
    editor.begin_floating_selection().unwrap().unwrap();
    assert!(!editor.commit_floating_selection().unwrap());
    assert_eq!(editor.document.layers[0].image, original.layers[0].image);
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(editor.selection.as_ref().unwrap().mask[2 * 8 + 2], 96);
}
