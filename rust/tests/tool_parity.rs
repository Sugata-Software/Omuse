use image::Rgba;
use omuse::{
    editor::{Editor, Selection},
    gradient_tools::{GradientKind, GradientSettings},
    model::Document,
    selection_tools::{SelectionMode, WandSettings},
};

#[test]
fn gradient_composites_transparency_and_fractional_selection_in_one_edit() {
    let mut e = Editor::new(Document::new(4, 4));
    e.document.layers[0].image.as_mut().unwrap().fill(255);
    e.selection = Some(Selection {
        width: 4,
        height: 4,
        mask: vec![128; 16],
    });
    let before = e.document.layers[0].image.clone();
    let settings = GradientSettings {
        kind: GradientKind::Radial,
        transparent: true,
        ..Default::default()
    };
    assert!(
        e.gradient_with((0.5, 0.5), (2.5, 0.5), [0, 0, 0, 255], [255; 4], &settings)
            .unwrap()
    );
    let img = e.document.layers[0].image.as_ref().unwrap();
    assert_eq!(img.get_pixel(0, 0).0, [127, 127, 127, 255]);
    assert_eq!(img.get_pixel(3, 0).0, [255; 4]);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(e.document.layers[0].image, before);
}

#[test]
fn configured_wand_samples_composite_and_combines_without_pixel_edits() {
    let mut e = Editor::new(Document::new(4, 4));
    e.document.background = [0; 4];
    let bottom = e.active_layer.clone();
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    e.add_layer("Empty active layer");
    let depth = e.undo_depth();
    let settings = WandSettings {
        tolerance: 0,
        all_layers: true,
        ..Default::default()
    };
    assert!(
        e.wand_select_with(0, 0, &settings, SelectionMode::Replace)
            .unwrap()
    );
    assert_eq!(
        e.selection
            .as_ref()
            .unwrap()
            .mask
            .iter()
            .filter(|&&v| v != 0)
            .count(),
        1
    );
    e.select_rectangle(2., 2., 1., 1.);
    assert!(
        e.wand_select_with(0, 0, &settings, SelectionMode::Add)
            .unwrap()
    );
    assert_eq!(
        e.selection
            .as_ref()
            .unwrap()
            .mask
            .iter()
            .filter(|&&v| v != 0)
            .count(),
        2
    );
    assert!(
        e.wand_select_with(0, 0, &settings, SelectionMode::Subtract)
            .unwrap()
    );
    assert_eq!(e.selection.as_ref().unwrap().mask[0], 0);
    assert_eq!(e.selection.as_ref().unwrap().mask[10], 255);
    assert_eq!(e.undo_depth(), depth);
    assert_eq!(
        e.document
            .find_layer(&bottom)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [255, 0, 0, 255]
    );
}

#[test]
fn radial_mask_gradient_preserves_source_and_locked_targets() {
    let mut e = Editor::new(Document::new(4, 4));
    let id = e.active_layer.clone();
    e.add_mask(&id, true);
    let source = e.document.layers[0].image.clone();
    let depth = e.undo_depth();
    let settings = GradientSettings {
        kind: GradientKind::Radial,
        ..Default::default()
    };
    assert!(
        e.gradient_mask_with(&id, (0.5, 0.5), (3.5, 0.5), 0, 255, &settings)
            .unwrap()
    );
    assert_eq!(e.document.layers[0].image, source);
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(
        e.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [0, 0, 0, 255]
    );
    e.set_locked(&id, true);
    let mask = e.document.layers[0].mask.clone();
    assert!(
        e.gradient_mask_with(&id, (0., 0.), (1., 1.), 255, 0, &settings)
            .is_err()
    );
    assert_eq!(e.document.layers[0].mask, mask);
}

#[test]
fn wand_uses_premultiplied_canvas_sample_and_ignores_active_mask() {
    let mut e = Editor::new(Document::new(4, 4));
    let id = e.active_layer.clone();
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(0, 0, Rgba([255, 0, 0, 0]));
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(1, 0, Rgba([0, 255, 0, 0]));
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(2, 0, Rgba([255, 0, 0, 128]));
    e.add_mask(&id, false);
    let settings = WandSettings {
        tolerance: 0,
        ..Default::default()
    };
    e.wand_select_with(0, 0, &settings, SelectionMode::Replace)
        .unwrap();
    assert_eq!(e.selection.as_ref().unwrap().mask[0], 255);
    assert_eq!(e.selection.as_ref().unwrap().mask[1], 255);
    assert_eq!(e.selection.as_ref().unwrap().mask[2], 0);
    e.wand_select_with(2, 0, &settings, SelectionMode::Replace)
        .unwrap();
    assert_eq!(e.selection.as_ref().unwrap().mask[2], 255);
    assert_eq!(e.selection.as_ref().unwrap().mask[0], 0);
    let sample = e.selection_sample(false).unwrap();
    assert_eq!(sample.get_pixel(2, 0).0, [255, 0, 0, 128]);
}

#[test]
fn selection_history_interleaves_with_artwork_without_dirtying_saved_pixels() {
    let mut e = Editor::new(Document::new(4, 4));
    e.select_rectangle(1., 1., 2., 2.);
    assert!(e.record_selection_change(None));
    assert!(!e.is_dirty());
    let selected = e.selection.clone();
    e.fill_selection([255, 0, 0, 255]);
    assert!(e.is_dirty());
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert_eq!(e.selection, selected);
    assert!(e.undo());
    assert!(e.selection.is_none());
    assert!(e.redo());
    assert_eq!(e.selection, selected);
    assert!(!e.is_dirty());
    assert!(e.redo());
    assert!(e.is_dirty());
}
