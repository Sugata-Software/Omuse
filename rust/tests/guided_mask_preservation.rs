//! Mask-only Keep must preserve a retained photograph's exact source/recipe.
use image::{GrayImage, ImageBuffer, Luma, Rgba};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode},
    editor::Editor,
    filters::Filter,
    model::Document,
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn guided_mask_keep_preserves_retained_words_recipe_and_one_undo_through_reopen() {
    let source = ImageBuffer::from_fn(8, 6, |x, y| {
        Rgba([12345u16 + x as u16, 23456 + y as u16, 34567, 45678])
    });
    let mut state = LayerState::from_rgba16(&source, "Retained photograph").unwrap();
    state.recipe.nodes.push(FilterNode {
        id: "mask-preserve-invert".into(),
        name: "Editable invert".into(),
        enabled: true,
        opacity: 0.75,
        operation: AdvancedOperation::Filter(Filter::Invert),
        soft_mask: None,
    });
    let state = Arc::new(state.evaluate(&AtomicBool::new(false)).unwrap());
    let original_source = state.source.to_rgba16();
    let original_result = state.result.to_rgba16();
    let original_recipe = serde_json::to_value(&state.recipe).unwrap();
    let mut doc = Document::new(8, 6);
    doc.layers[0].image = Some(state.proxy().unwrap().into());
    doc.layers[0].advanced = Some(state.clone());
    let mut editor = Editor::new(doc);
    let id = editor.active_layer.clone();
    let derived = editor.document.layers[0].image.clone().unwrap();
    let mask = GrayImage::from_fn(8, 6, |x, _| Luma([if x < 4 { 255 } else { 0 }]));
    assert!(editor.apply_subject_mask(&id, &mask, false).unwrap());
    assert_eq!(editor.undo_depth(), 1);
    let layer = editor.document.find_layer(&id).unwrap();
    assert!(Arc::ptr_eq(layer.advanced.as_ref().unwrap(), &state));
    assert!(layer.image.as_ref().unwrap().shares_pixels_with(&derived));
    assert!(editor.undo());
    let layer = editor.document.find_layer(&id).unwrap();
    assert!(layer.mask.is_none());
    assert!(Arc::ptr_eq(layer.advanced.as_ref().unwrap(), &state));
    assert!(editor.redo());
    assert_eq!(editor.undo_depth(), 1);
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("guided-retained.omuse");
    omuse::document::save(&editor.document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    let layer = reopened.find_layer(&id).unwrap();
    let retained = layer.advanced.as_ref().unwrap();
    assert_eq!(retained.source.to_rgba16(), original_source);
    assert_eq!(retained.result.to_rgba16(), original_result);
    assert_eq!(
        serde_json::to_value(&retained.recipe).unwrap(),
        original_recipe
    );
    assert_eq!(layer.image.as_ref().unwrap(), &derived);
    assert_eq!(layer.mask.as_ref().unwrap().get_pixel(1, 1)[0], 255);
    assert_eq!(layer.mask.as_ref().unwrap().get_pixel(6, 1)[0], 0);
}

#[test]
fn transformed_guided_selection_has_one_selection_undo_without_mutating_source_or_mask() {
    let image = ImageBuffer::from_pixel(8, 6, Rgba([12345u16, 23456, 34567, 45678]));
    let retained = Arc::new(LayerState::from_rgba16(&image, "Retained selection source").unwrap());
    let mut doc = Document::new(32, 24);
    let layer = &mut doc.layers[0];
    layer.image = Some(retained.proxy().unwrap().into());
    layer.advanced = Some(retained.clone());
    layer.offset_x = 4.;
    layer.offset_y = 3.;
    layer.scale_x = 2.;
    layer.scale_y = 1.5;
    let old_mask = image::RgbaImage::from_pixel(8, 6, Rgba([180, 180, 180, 255]));
    layer.mask = Some(old_mask.clone().into());
    let source = layer.image.clone().unwrap();
    let id = layer.id.clone();
    let mut editor = Editor::new(doc);
    let incoming = GrayImage::from_fn(8, 6, |x, _| Luma([if x < 2 { 255 } else { 0 }]));
    let previous = editor.selection.clone();
    assert!(editor.apply_subject_mask(&id, &incoming, true).unwrap());
    editor.record_selection_change(previous);
    assert_eq!(
        editor.undo_depth(),
        1,
        "one selection history entry preserves artwork"
    );
    let selection = editor.selection.as_ref().unwrap();
    assert_eq!((selection.width, selection.height), (32, 24));
    assert_eq!(selection.bounds(), Some((4, 3, 4, 9)));
    assert_eq!(selection.mask.iter().filter(|&&v| v == 255).count(), 36);
    assert_eq!(selection.mask[3 * 32 + 4], 255);
    assert_eq!(selection.mask[3 * 32 + 8], 0);
    let layer = editor.document.find_layer(&id).unwrap();
    assert!(layer.image.as_ref().unwrap().shares_pixels_with(&source));
    assert!(Arc::ptr_eq(layer.advanced.as_ref().unwrap(), &retained));
    assert_eq!(layer.mask.as_deref().unwrap(), &old_mask);
    assert_eq!(retained.source.to_rgba16(), image);
    assert!(editor.undo());
    assert!(editor.selection.is_none());
    assert_eq!(editor.undo_depth(), 0);
    assert!(editor.redo());
    editor.clear_selection();
    assert!(editor.selection.is_none());
}
