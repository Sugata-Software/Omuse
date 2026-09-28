use image::Rgba;
use omuse::editor::{Adjustment, Editor, GuideAxis, LayerPlacement, PaintTool};
use omuse::model::{Document, Layer};
use omuse::retouch_brush::RetouchMode;
use omuse::spot_heal::SpotHealingMode;

fn editor() -> Editor {
    Editor::new(Document::new(16, 16))
}
fn pixel(editor: &Editor, x: u32, y: u32) -> [u8; 4] {
    editor
        .document
        .find_layer(&editor.active_layer)
        .unwrap()
        .image
        .as_ref()
        .unwrap()
        .get_pixel(x, y)
        .0
}
fn dot(editor: &mut Editor, x: f32, y: f32, color: [u8; 4]) {
    editor.brush.size = 1.0;
    editor.brush.color = color;
    assert!(editor.begin_stroke(x, y, 1.0, PaintTool::Pencil));
    assert!(editor.finish_stroke());
}

#[test]
fn spot_healing_is_source_free_and_one_undo_step() {
    let mut e = editor();
    let id = e.active_layer.clone();
    let image = e
        .document
        .find_layer_mut(&id)
        .unwrap()
        .image
        .as_mut()
        .unwrap();
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([
            ((x * 17 + y * 3 + x * y % 19) % 256) as u8,
            ((x * 5 + y * 13 + x * y % 23) % 256) as u8,
            ((x * 11 + y * 7 + x * y % 29) % 256) as u8,
            255,
        ]);
    }
    let before = image.clone();
    e.brush.size = 5.0;
    e.brush.hardness = 0.8;
    e.brush.opacity = 0.75;
    assert!(
        e.spot_heal_stroke(&[(8.5, 8.5)], SpotHealingMode::CreateTexture, 0x1234_5678)
            .unwrap()
    );
    assert_ne!(
        e.document.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &before
    );
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &before
    );
}

#[test]
fn saved_revision_survives_undo_redo_and_branching() {
    let mut e = editor();
    assert!(!e.is_dirty());
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    assert!(e.is_dirty());
    e.mark_saved();
    assert!(!e.is_dirty());
    dot(&mut e, 3.5, 3.5, [0, 255, 0, 255]);
    assert!(e.is_dirty());
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert!(e.redo());
    assert!(e.is_dirty());
    assert!(e.undo());
    assert!(e.undo());
    assert!(e.is_dirty());
    dot(&mut e, 5.5, 5.5, [0, 0, 255, 255]);
    assert!(e.is_dirty());
    assert!(!e.can_redo());
}
#[test]
fn a_whole_interpolated_stroke_is_one_undo_step() {
    let mut e = editor();
    e.brush.size = 2.0;
    e.brush.color = [255, 0, 0, 255];
    assert!(e.begin_stroke(1.5, 4.5, 1.0, PaintTool::Pencil));
    assert!(e.continue_stroke(14.5, 4.5, 1.0));
    assert!(e.finish_stroke());
    assert_eq!(e.undo_depth(), 1);
    for x in 1..15 {
        assert_eq!(pixel(&e, x, 4), [255, 0, 0, 255]);
    }
    assert!(e.undo());
    assert_eq!(pixel(&e, 7, 4), [0; 4]);
}
#[test]
fn overlapping_dabs_share_one_stroke_opacity_cap() {
    let mut e = editor();
    e.brush.size = 1.0;
    e.brush.opacity = 0.5;
    e.brush.color = [240, 60, 20, 255];
    assert!(e.begin_stroke(3.5, 3.5, 1.0, PaintTool::Pencil));
    for _ in 0..20 {
        assert!(e.continue_stroke(3.5, 3.5, 1.0));
    }
    assert!(e.finish_stroke());
    assert_eq!(pixel(&e, 3, 3), [240, 60, 20, 128]);
    assert_eq!(e.undo_depth(), 1);

    assert!(e.begin_stroke(3.5, 3.5, 1.0, PaintTool::Pencil));
    assert!(e.finish_stroke());
    assert_eq!(pixel(&e, 3, 3), [240, 60, 20, 192]);
    assert!(e.undo());
    assert_eq!(pixel(&e, 3, 3), [240, 60, 20, 128]);
}
#[test]
fn pulled_string_smoothing_ignores_slack_and_finishes_at_pointer() {
    let mut e = editor();
    e.brush.size = 1.;
    e.brush.smoothing = 5.;
    e.brush.color = [20, 180, 40, 255];
    assert!(e.begin_stroke(1.5, 6.5, 1., PaintTool::Brush));
    assert!(e.continue_stroke_at_zoom(4.5, 6.5, 1., 1.));
    assert_eq!(
        pixel(&e, 4, 6),
        [0; 4],
        "pointer inside string must not paint"
    );
    assert!(e.continue_stroke_at_zoom(14.5, 6.5, 1., 1.));
    assert_eq!(pixel(&e, 8, 6), [20, 180, 40, 255]);
    assert_eq!(
        pixel(&e, 13, 6),
        [0; 4],
        "stroke must trail while pointer is down"
    );
    assert!(e.finish_stroke());
    assert_eq!(
        pixel(&e, 14, 6),
        [20, 180, 40, 255],
        "mouse-up must reach the hand position"
    );
}
#[test]
fn unified_mask_edit_respects_placement_source_and_undo() {
    let mut e = editor();
    let id = e.active_layer.clone();
    let source = e.document.find_layer(&id).unwrap().image.clone();
    {
        let layer = e.document.find_layer_mut(&id).unwrap();
        layer.mask = Some(image::RgbaImage::from_pixel(4, 4, Rgba([255; 4])).into());
        layer.metadata["maskLinked"] = serde_json::json!(false);
        layer.metadata["maskPlacement"] = serde_json::json!({"origin":[4,4],"size":[4,4],"rotation":0,"flipX":false,"flipY":false,"sampling":"High quality"});
    }
    e.select_rectangle(5., 5., 1., 1.);
    assert!(e.fill_mask_selection(&id, 0).unwrap());
    let layer = e.document.find_layer(&id).unwrap();
    assert_eq!(layer.image, source);
    assert_eq!(
        layer.mask.as_ref().unwrap().get_pixel(1, 1).0,
        [0, 0, 0, 255]
    );
    assert_eq!(layer.mask.as_ref().unwrap().get_pixel(0, 0).0, [255; 4]);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(1, 1)
            .0,
        [255; 4]
    );
}

#[test]
fn mask_adjustment_preserves_source_fractional_selection_and_undo() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.add_mask(&id, true));
    let source = e.document.find_layer(&id).unwrap().image.clone();
    e.select_rectangle(6., 6., 4., 4.);
    assert!(e.feather_selection(1.5));
    assert!(e.adjust_mask_selection(&id, Adjustment::Invert).unwrap());
    let mask = e.document.find_layer(&id).unwrap().mask.as_ref().unwrap();
    assert_eq!(mask.get_pixel(0, 0).0, [255; 4]);
    let center = mask.get_pixel(7, 7)[0];
    assert!(center < 255);
    let edge = mask.get_pixel(5, 7)[0];
    assert!(
        edge > center && edge < 255,
        "center {center}, fractional edge {edge}"
    );
    assert_eq!(e.document.find_layer(&id).unwrap().image, source);
    assert!(e.undo());
    assert!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .pixels()
            .all(|p| p.0 == [255; 4])
    );
}

#[test]
fn clone_from_canvas_freezes_all_layers_caps_opacity_and_undoes_once() {
    let mut e = editor();
    let target_id = e.active_layer.clone();
    let mut source = Layer::paint("Composite source", 2, 2);
    source.image = Some(image::RgbaImage::from_pixel(2, 2, Rgba([20, 220, 40, 255])).into());
    let source_id = source.id.clone();
    e.document.layers.insert(0, source);
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(0, 0).0,
        [20, 220, 40, 255]
    );
    e.brush.size = 1.;
    e.brush.hardness = 1.;
    e.brush.opacity = 0.5;
    assert!(e.begin_clone_stroke_from_canvas((0.5, 0.5), (8.5, 8.5), false));
    for _ in 0..12 {
        assert!(e.continue_clone_stroke((8.5, 8.5)));
    }
    assert!(e.finish_clone_stroke());
    assert_eq!(pixel(&e, 8, 8), [20, 220, 40, 128]);
    assert_eq!(
        e.document
            .find_layer(&source_id)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [20, 220, 40, 255]
    );
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(
        e.document
            .find_layer(&target_id)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .get_pixel(8, 8)
            .0,
        [0; 4]
    );
}
#[test]
fn unified_mask_edit_supports_group_masks() {
    let mut e = editor();
    let group = e.add_group("Masked group");
    e.document.find_layer_mut(&group).unwrap().mask =
        Some(image::RgbaImage::from_pixel(16, 16, Rgba([255; 4])).into());
    e.select_rectangle(3., 3., 2., 2.);
    assert!(e.gradient_mask(&group, (3., 3.), (5., 3.), 0, 255).unwrap());
    let mask = e
        .document
        .find_layer(&group)
        .unwrap()
        .mask
        .as_ref()
        .unwrap();
    assert_eq!(mask.get_pixel(3, 3)[3], 255);
    assert!(mask.get_pixel(3, 3)[0] < mask.get_pixel(4, 3)[0]);
}
#[test]
fn cancellation_restores_pixels_and_dirty_state() {
    let mut e = editor();
    assert!(e.begin_stroke(3.5, 3.5, 1.0, PaintTool::Brush));
    assert!(e.is_dirty());
    e.cancel_stroke();
    assert_eq!(pixel(&e, 3, 3), [0; 4]);
    assert!(!e.is_dirty());
    assert_eq!(e.undo_depth(), 0);
}
#[test]
fn no_op_strokes_do_not_pollute_history() {
    let mut e = editor();
    assert!(e.begin_stroke(-1000.0, -1000.0, 1.0, PaintTool::Brush));
    assert!(!e.finish_stroke());
    assert!(!e.is_dirty());
    assert!(e.begin_stroke(4.0, 4.0, 0.0, PaintTool::Brush));
    assert!(!e.finish_stroke());
    assert!(!e.begin_stroke(f32::NAN, 4.0, 1.0, PaintTool::Brush));
}
#[test]
fn pressure_and_eraser_change_alpha_without_corrupting_straight_color() {
    let mut e = editor();
    e.brush.color = [240, 60, 20, 255];
    e.brush.size = 1.0;
    e.begin_stroke(3.5, 3.5, 0.5, PaintTool::Pencil);
    e.finish_stroke();
    assert_eq!(pixel(&e, 3, 3), [240, 60, 20, 128]);
    e.begin_stroke(3.5, 3.5, 0.5, PaintTool::Eraser);
    e.finish_stroke();
    assert_eq!(pixel(&e, 3, 3), [240, 60, 20, 64]);
}
#[test]
fn strokes_are_restricted_by_canvas_selection() {
    let mut e = editor();
    e.select_rectangle(2.0, 2.0, 2.0, 2.0);
    e.brush.size = 16.0;
    e.begin_stroke(3.0, 3.0, 1.0, PaintTool::Pencil);
    e.finish_stroke();
    assert_eq!(pixel(&e, 2, 2)[3], 255);
    assert_eq!(pixel(&e, 4, 2)[3], 0);
    e.invert_selection();
    assert!(!e.selection.as_ref().unwrap().contains(2, 2));
    assert!(e.selection.as_ref().unwrap().contains(4, 2));
}
#[test]
fn reversed_rectangle_and_ellipse_have_correct_bounds() {
    let mut e = editor();
    e.select_rectangle(6.0, 7.0, -4.0, -3.0);
    assert_eq!(e.selection.as_ref().unwrap().bounds(), Some((2, 4, 4, 3)));
    e.select_ellipse(2.0, 2.0, 8.0, 8.0);
    let s = e.selection.as_ref().unwrap();
    assert!(!s.contains(2, 2));
    assert!(s.contains(5, 5));
    assert_eq!(s.bounds(), Some((2, 2, 8, 8)));
}
#[test]
fn selection_can_expand_contract_and_feather_without_document_history() {
    let mut e = editor();
    e.select_rectangle(6., 6., 2., 2.);
    let depth = e.undo_depth();
    assert!(e.resize_selection(2));
    assert!(e.selection.as_ref().unwrap().contains(4, 6));
    assert!(e.resize_selection(-2));
    assert!(e.feather_selection(1.5));
    let selection = e.selection.as_ref().unwrap();
    assert!(selection.mask.iter().any(|&value| value > 0 && value < 255));
    assert_eq!(e.undo_depth(), depth);
}
#[test]
fn a_locked_ancestor_protects_child_pixels_and_structure() {
    let mut e = editor();
    let child = e.active_layer.clone();
    let group = e.add_group("Group");
    assert!(e.reorder_layer(&child, Some(&group), 0));
    assert!(e.select_layer(&child));
    e.set_locked(&group, true);
    assert!(!e.begin_stroke(4.0, 4.0, 1.0, PaintTool::Brush));
    assert!(!e.fill_selection([0, 0, 0, 255]));
    assert!(!e.delete_layer(&child));
    assert!(!e.reorder_layer(&child, None, 0));
    e.set_locked(&group, false);
    assert!(e.fill_selection([0, 0, 0, 255]));
}
#[test]
fn nesting_rejects_cycles_and_image_parents() {
    let mut e = editor();
    let paint = e.active_layer.clone();
    let outer = e.add_group("Outer");
    let inner = e.add_group("Inner");
    assert!(e.reorder_layer(&inner, Some(&outer), 0));
    assert!(!e.reorder_layer(&outer, Some(&inner), 0));
    assert!(!e.reorder_layer(&outer, Some(&outer), 0));
    assert!(!e.reorder_layer(&outer, Some(&paint), 0));
    assert!(e.document.find_layer(&inner).is_some());
    assert!(e.document.find_layer(&outer).is_some());
}
#[test]
fn duplicating_groups_assigns_fresh_descendant_ids_and_independent_pixels() {
    let mut e = editor();
    let paint = e.active_layer.clone();
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    let group = e.add_group("Group");
    e.reorder_layer(&paint, Some(&group), 0);
    let copy = e.duplicate_layer(&group).unwrap();
    let copied = e.document.find_layer(&copy).unwrap().children[0].id.clone();
    assert_ne!(copied, paint);
    e.select_layer(&copied);
    e.fill_selection([0, 0, 255, 255]);
    assert_eq!(
        e.document
            .find_layer(&paint)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .get_pixel(2, 2)
            .0,
        [255, 0, 0, 255]
    );
}
#[test]
fn duplicating_group_rewrites_internal_live_mask_sources() {
    let mut e = editor();
    let source = e.active_layer.clone();
    let target = e.add_layer("Masked");
    let group = e.add_group("Pair");
    assert!(e.reorder_layer(&source, Some(&group), 0));
    assert!(e.reorder_layer(&target, Some(&group), 1));
    assert!(e.set_live_mask_source(&target, Some(&source)).unwrap());
    let copy = e.duplicate_layer(&group).unwrap();
    let children = &e.document.find_layer(&copy).unwrap().children;
    assert_ne!(children[0].id, source);
    assert_eq!(children[1].metadata["maskSourceID"], children[0].id);
    assert!(e.undo());
    assert!(e.document.find_layer(&copy).is_none());
}
#[test]
fn flood_fill_is_contiguous_and_obeys_selection_barriers() {
    let mut e = editor();
    for y in 0..16 {
        e.document.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(8, y, Rgba([0, 0, 0, 255]));
    }
    assert!(e.fill_at(1, 1, [255, 0, 0, 255], 0));
    assert_eq!(pixel(&e, 7, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 9, 1), [0; 4]);
    e.select_rectangle(0.0, 0.0, 3.0, 3.0);
    assert!(e.fill_at(1, 1, [0, 255, 0, 255], 0));
    assert_eq!(pixel(&e, 2, 2), [0, 255, 0, 255]);
    assert_eq!(pixel(&e, 3, 2), [255, 0, 0, 255]);
    assert!(!e.fill_at(15, 15, [255, 255, 255, 255], 0));
}
#[test]
fn transparent_hidden_rgb_does_not_split_flood_region() {
    let mut e = editor();
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(3, 3, Rgba([255, 0, 0, 0]));
    assert!(e.fill_at(0, 0, [20, 30, 40, 255], 0));
    assert_eq!(pixel(&e, 3, 3), [20, 30, 40, 255]);
}
#[test]
fn wand_maps_scaled_source_pixels_to_canvas() {
    let mut e = Editor::new(Document::new(8, 8));
    e.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])).into());
    let id = e.active_layer.clone();
    e.transform_layer(&id, 1.0, 2.0, 0.0, 2.0, 2.0);
    assert!(e.wand_select(2, 3, 0));
    assert_eq!(e.selection.as_ref().unwrap().bounds(), Some((1, 2, 4, 4)));
}
#[test]
fn painting_inverse_transform_matches_composited_pixel() {
    let mut e = Editor::new(Document::new(16, 16));
    e.document.layers[0].image = Some(image::RgbaImage::new(4, 4).into());
    let id = e.active_layer.clone();
    e.transform_layer(&id, 4.0, 5.0, 90.0, 1.0, 1.0);
    dot(&mut e, 7.5, 5.5, [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 0, 0), [255, 0, 0, 255]);
    let rendered = omuse::raster::composite(&e.document);
    assert_eq!(rendered.get_pixel(7, 5).0, [255, 0, 0, 255]);
}
#[test]
fn mirrored_layer_painting_maps_back_to_source() {
    let mut e = editor();
    e.document.layers[0].image = Some(image::RgbaImage::new(4, 4).into());
    let id = e.active_layer.clone();
    e.transform_layer(&id, 3.0, 2.0, 0.0, -1.0, 1.0);
    dot(&mut e, 3.5, 2.5, [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 3, 0), [255, 0, 0, 255]);
}
#[test]
fn gradient_is_linear_and_keeps_outside_selection_unchanged() {
    let mut e = editor();
    e.select_rectangle(0.0, 0.0, 4.0, 1.0);
    assert!(e.gradient((0.5, 0.5), (3.5, 0.5), [0, 0, 0, 255], [255, 255, 255, 255]));
    assert_eq!(pixel(&e, 0, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&e, 1, 0), [85, 85, 85, 255]);
    assert_eq!(pixel(&e, 3, 0), [255; 4]);
    assert_eq!(pixel(&e, 4, 0), [0; 4]);
}
#[test]
fn adjustments_keep_alpha_and_apply_only_inside_selection() {
    let mut e = editor();
    e.fill_selection([100, 50, 20, 128]);
    e.mark_saved();
    e.select_rectangle(0.0, 0.0, 1.0, 1.0);
    assert!(e.adjust(Adjustment::Invert));
    assert_eq!(pixel(&e, 0, 0), [155, 205, 235, 128]);
    assert_eq!(pixel(&e, 1, 0), [100, 50, 20, 128]);
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert!(e.adjust(Adjustment::Grayscale));
    let p = pixel(&e, 0, 0);
    assert_eq!(p[0], p[1]);
    assert_eq!(p[1], p[2]);
    assert_eq!(p[3], 128);
}
#[test]
fn blur_avoids_black_fringes_around_transparent_red() {
    let mut e = editor();
    dot(&mut e, 8.5, 8.5, [255, 0, 0, 255]);
    assert!(e.adjust(Adjustment::Blur(1.0)));
    let p = pixel(&e, 8, 8);
    assert_eq!(p[0], 255);
    assert!(p[3] > 0 && p[3] < 255);
    assert_eq!(p[1], 0);
}
#[test]
fn crop_is_nondestructive_and_shifts_nested_leaf_coordinates() {
    let mut e = editor();
    let leaf = e.active_layer.clone();
    dot(&mut e, 6.5, 6.5, [255, 0, 0, 255]);
    let group = e.add_group("Group");
    e.reorder_layer(&leaf, Some(&group), 0);
    assert!(e.crop_canvas(4, 4, 8, 8));
    let layer = e.document.find_layer(&leaf).unwrap();
    assert_eq!((layer.offset_x, layer.offset_y), (-4.0, -4.0));
    assert_eq!(layer.image.as_ref().unwrap().dimensions(), (16, 16));
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(2, 2).0,
        [255, 0, 0, 255]
    );
    assert!(e.undo());
    assert_eq!((e.document.width, e.document.height), (16, 16));
}
#[test]
fn selection_is_dropped_when_undo_changes_canvas_size() {
    let mut e = editor();
    e.resize_canvas(8, 8);
    e.select_all();
    assert!(e.undo());
    assert!(e.selection.is_none());
}
#[test]
fn history_budget_is_strict_and_redo_does_not_escape_it() {
    let mut e = editor();
    dot(&mut e, 1.5, 1.5, [255, 0, 0, 255]);
    let one = e.history_bytes();
    e.set_history_limit(one * 2);
    for x in 2..10 {
        dot(&mut e, x as f32 + 0.5, 1.5, [255, 0, 0, 255]);
        assert!(e.history_bytes() <= one * 2);
    }
    while e.undo() {
        assert!(e.history_bytes() <= one * 2);
    }
    while e.redo() {
        assert!(e.history_bytes() <= one * 2);
    }
    e.set_history_limit(0);
    assert_eq!(e.history_bytes(), 0);
    assert!(!e.can_undo());
}
#[test]
fn invalid_transform_dimensions_and_adjustments_leave_document_unchanged() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(!e.resize_canvas(0, 1));
    assert!(!e.resize_canvas(u32::MAX, u32::MAX));
    assert!(!e.transform_layer(&id, 0.0, 0.0, 0.0, 0.0, 1.0));
    assert!(!e.transform_layer(&id, f32::NAN, 0.0, 0.0, 1.0, 1.0));
    assert!(!e.adjust(Adjustment::Brightness(f32::NAN)));
    assert!(!e.adjust(Adjustment::Blur(f32::INFINITY)));
    assert!(!e.is_dirty());
}
#[test]
fn imported_layer_gets_unique_identity_and_supports_undo() {
    let mut e = editor();
    let mut layer = Layer::paint("Imported", 2, 2);
    layer.id = e.active_layer.clone();
    let old = e.active_layer.clone();
    let imported = e.import_layer(layer);
    assert_ne!(old, imported);
    assert_eq!(e.document.layers.len(), 2);
    assert!(e.undo());
    assert_eq!(e.active_layer, old);
    assert!(e.document.find_layer(&imported).is_none());
}

#[test]
fn rejected_imports_preserve_selection_history_and_floating_transaction() {
    let mut e = editor();
    e.select_rectangle(2.0, 2.0, 3.0, 3.0);
    let selection = e.selection.clone();
    let before = omuse::raster::composite(&e.document);
    let depth = e.undo_depth();

    let mut invalid = Layer::group("Invalid");
    invalid.image = Some(image::RgbaImage::new(1, 1).into());
    assert!(e.import_layer(invalid).is_empty());
    assert_eq!(omuse::raster::composite(&e.document), before);
    assert_eq!(e.selection, selection);
    assert_eq!(e.undo_depth(), depth);

    let mut too_many = Layer::group("Too many");
    too_many.children = (0..omuse::model::MAX_LAYERS)
        .map(|index| Layer::group(format!("Layer {index}")))
        .collect();
    assert!(e.insert_layer(too_many).is_empty());
    assert_eq!(omuse::raster::composite(&e.document), before);
    assert_eq!(e.selection, selection);
    assert_eq!(e.undo_depth(), depth);

    e.fill_selection([255, 0, 0, 255]);
    e.select_rectangle(2.0, 2.0, 2.0, 2.0);
    let floating = e.begin_floating_selection().unwrap().unwrap();
    let floating_pixels = omuse::raster::composite(&e.document);
    let floating_selection = e.selection.clone();
    let floating_depth = e.undo_depth();
    assert!(e.add_layer("Blocked while floating").is_empty());
    assert!(e.add_group("Blocked while floating").is_empty());
    assert!(e.import_layer(Layer::paint("Imported", 1, 1)).is_empty());
    assert_eq!(e.floating_selection_layer(), Some(floating.as_str()));
    assert_eq!(omuse::raster::composite(&e.document), floating_pixels);
    assert_eq!(e.selection, floating_selection);
    assert_eq!(e.undo_depth(), floating_depth);
    assert!(e.cancel_floating_selection());
}

#[test]
fn deleting_last_layer_retains_a_usable_blank_paint_layer() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.delete_layer(&id));
    assert_eq!(e.document.layers.len(), 1);
    assert!(
        e.document
            .find_layer(&e.active_layer)
            .unwrap()
            .image
            .is_some()
    );
    assert!(e.undo());
    assert_eq!(e.active_layer, id);
}

#[test]
fn recovery_is_dirty_until_saved_even_without_new_edits() {
    let mut e = editor();
    e.mark_unsaved();
    assert!(e.is_dirty());
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    e.undo();
    assert!(e.is_dirty());
    e.mark_saved();
    assert!(!e.is_dirty());
}
#[test]
fn selection_mask_can_be_inverted_disabled_and_baked_with_undo() {
    let mut e = editor();
    e.fill_selection([255, 0, 0, 255]);
    let id = e.active_layer.clone();
    e.select_rectangle(0.0, 0.0, 4.0, 4.0);
    assert!(e.add_mask(&id, true));
    let result = omuse::raster::composite(&e.document);
    assert_eq!(result.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(result.get_pixel(5, 5)[3], 0);
    assert!(e.invert_mask(&id));
    let result = omuse::raster::composite(&e.document);
    assert_eq!(result.get_pixel(0, 0)[3], 0);
    assert_eq!(result.get_pixel(5, 5)[3], 255);
    assert!(e.set_mask_enabled(&id, false));
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(0, 0)[3],
        255
    );
    e.set_mask_enabled(&id, true);
    assert!(e.remove_mask(&id, true));
    assert!(e.document.find_layer(&id).unwrap().mask.is_none());
    assert_eq!(pixel(&e, 0, 0)[3], 0);
    assert!(e.undo());
    assert!(e.document.find_layer(&id).unwrap().mask.is_some());
    assert_eq!(pixel(&e, 0, 0)[3], 255);
}
#[test]
fn merge_down_preserves_normal_composite_and_undo() {
    let mut e = editor();
    e.fill_selection([0, 0, 255, 255]);
    e.add_layer("Red");
    e.fill_selection([255, 0, 0, 128]);
    let before = omuse::raster::composite(&e.document);
    assert!(e.merge_down());
    assert_eq!(e.document.layers.len(), 1);
    assert_eq!(omuse::raster::composite(&e.document), before);
    assert!(e.undo());
    assert_eq!(e.document.layers.len(), 2);
}
#[test]
fn unsafe_merge_interactions_are_rejected() {
    let mut e = editor();
    let lower = e.active_layer.clone();
    let upper = e.add_layer("Upper");
    e.set_blend_mode(&upper, "multiply");
    assert!(!e.merge_down());
    e.set_blend_mode(&upper, "normal");
    e.set_locked(&lower, true);
    assert!(!e.merge_down());
}
#[test]
fn flatten_preserves_artwork_background_and_recovers_structure_on_undo() {
    let mut e = editor();
    e.document.background = [220, 220, 220, 255];
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    e.add_layer("Second");
    dot(&mut e, 5.5, 5.5, [0, 255, 0, 255]);
    let before = omuse::raster::composite(&e.document);
    assert!(e.flatten());
    assert_eq!(e.document.background, [0; 4]);
    assert_eq!(e.document.layers.len(), 1);
    assert_eq!(omuse::raster::composite(&e.document), before);
    assert!(e.undo());
    assert_eq!(e.document.background, [220, 220, 220, 255]);
    assert_eq!(e.document.layers.len(), 2);
}
#[test]
fn clone_stamp_samples_original_pixels_and_respects_selection() {
    let mut e = editor();
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    e.select_rectangle(8.0, 8.0, 1.0, 1.0);
    assert!(e.clone_stamp((2.5, 2.5), (8.5, 8.5), 2.0, 1.0, false));
    assert_eq!(pixel(&e, 8, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 9, 8), [0; 4]);
    assert!(e.undo());
    assert_eq!(pixel(&e, 8, 8), [0; 4]);
}
#[test]
fn healing_matches_local_tone() {
    let mut e = editor();
    e.fill_selection([100, 100, 100, 255]);
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(2, 2, Rgba([200, 150, 100, 255]));
    // A single sample on a flat destination has its tone matched exactly, so no edit is needed.
    assert!(!e.clone_stamp((2.5, 2.5), (8.5, 8.5), 0.4, 1.0, true));
    assert_eq!(pixel(&e, 8, 8), [100, 100, 100, 255]);
}
#[test]
fn advanced_filter_is_selected_undoable_and_rejects_invalid_settings() {
    use omuse::filters::Filter;
    let mut e = editor();
    e.fill_selection([40, 80, 120, 128]);
    e.select_rectangle(0.0, 0.0, 1.0, 1.0);
    e.mark_saved();
    assert!(e.apply_filter(&Filter::Exposure { stops: 1.0 }));
    assert_ne!(pixel(&e, 0, 0), [40, 80, 120, 128]);
    assert_eq!(pixel(&e, 1, 0), [40, 80, 120, 128]);
    assert_eq!(pixel(&e, 0, 0)[3], 128);
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert!(!e.apply_filter(&Filter::Levels {
        black: 0.9,
        white: 0.1,
        gamma: 1.0
    }));
    assert!(!e.is_dirty());
}

#[test]
fn advanced_filter_blends_a_feathered_selection_and_undo_restores_pixels() {
    use omuse::filters::Filter;

    let mut e = editor();
    e.fill_selection([40, 80, 120, 255]);
    let before = e
        .document
        .find_layer(&e.active_layer)
        .unwrap()
        .image
        .clone();
    e.selection = Some(omuse::editor::Selection {
        width: 16,
        height: 16,
        mask: vec![128; 256],
    });

    assert!(e.apply_filter(&Filter::Invert));
    assert_eq!(pixel(&e, 5, 5), [128, 128, 128, 255]);
    assert_eq!(e.undo_depth(), 2);
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&e.active_layer).unwrap().image,
        before
    );
}

#[test]
fn blur_blends_a_feathered_selection_and_undo_restores_pixels() {
    fn source() -> image::RgbaImage {
        image::RgbaImage::from_fn(16, 16, |x, y| {
            if (x, y) == (8, 8) {
                Rgba([230, 20, 10, 255])
            } else {
                Rgba([30, 40, 50, 255])
            }
        })
    }
    fn selected(coverage: u8) -> omuse::editor::Selection {
        let mut mask = vec![0; 16 * 16];
        mask[8 * 16 + 8] = coverage;
        omuse::editor::Selection {
            width: 16,
            height: 16,
            mask,
        }
    }

    let mut fully_selected = editor();
    fully_selected.document.layers[0].image = Some(source().into());
    fully_selected.selection = Some(selected(255));
    assert!(fully_selected.adjust(Adjustment::Blur(1.0)));
    let blurred = pixel(&fully_selected, 8, 8);

    let mut e = editor();
    e.document.layers[0].image = Some(source().into());
    let before = e
        .document
        .find_layer(&e.active_layer)
        .unwrap()
        .image
        .clone();
    e.selection = Some(selected(128));
    assert!(e.adjust(Adjustment::Blur(1.0)));
    let expected = std::array::from_fn(|channel| {
        (f32::from(before.as_ref().unwrap().get_pixel(8, 8)[channel]) * (127.0 / 255.0)
            + f32::from(blurred[channel]) * (128.0 / 255.0))
            .round() as u8
    });
    assert_eq!(pixel(&e, 8, 8), expected);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&e.active_layer).unwrap().image,
        before
    );
}

#[test]
fn content_fill_blends_a_feathered_selection_and_undo_restores_pixels() {
    fn configured(coverage: u8) -> Editor {
        let mut e = editor();
        let id = e.active_layer.clone();
        e.document.find_layer_mut(&id).unwrap().image = Some(
            image::RgbaImage::from_fn(16, 16, |x, y| {
                Rgba([(x * 10) as u8, (y * 10) as u8, 70, 255])
            })
            .into(),
        );
        e.document
            .find_layer_mut(&id)
            .unwrap()
            .image
            .as_mut()
            .unwrap()
            .put_pixel(8, 8, Rgba([255, 0, 255, 255]));
        let mut mask = vec![0; 16 * 16];
        mask[8 * 16 + 8] = coverage;
        e.selection = Some(omuse::editor::Selection {
            width: 16,
            height: 16,
            mask,
        });
        e
    }

    let mut fully_selected = configured(255);
    assert!(fully_selected.content_aware_fill().unwrap());
    let filled = pixel(&fully_selected, 8, 8);

    let mut e = configured(128);
    let before = e
        .document
        .find_layer(&e.active_layer)
        .unwrap()
        .image
        .clone();
    assert!(e.content_aware_fill().unwrap());
    let expected = std::array::from_fn(|channel| {
        (f32::from(before.as_ref().unwrap().get_pixel(8, 8)[channel]) * (127.0 / 255.0)
            + f32::from(filled[channel]) * (128.0 / 255.0))
            .round() as u8
    });
    assert_eq!(pixel(&e, 8, 8), expected);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&e.active_layer).unwrap().image,
        before
    );
}

#[test]
fn continuous_clone_uses_frozen_source_and_one_undo_snapshot() {
    let mut e = editor();
    let image = e.document.layers[0].image.as_mut().unwrap();
    for x in 0..16 {
        image.put_pixel(x, 4, Rgba([(x * 10) as u8, 0, 0, 255]));
    }
    let original = image.clone();
    e.brush.size = 1.0;
    e.brush.hardness = 1.0;
    assert!(e.begin_clone_stroke((1.5, 4.5), (3.5, 4.5), false));
    assert!(e.continue_clone_stroke((9.5, 4.5)));
    assert!(e.finish_clone_stroke());
    assert_eq!(e.undo_depth(), 1);
    assert_eq!(pixel(&e, 9, 4), [70, 0, 0, 255]);
    assert!(e.undo());
    assert_eq!(*e.document.layers[0].image.as_ref().unwrap(), original);
    assert!(!e.is_dirty());
}
#[test]
fn clone_alpha_is_source_over_and_cancellation_restores_state() {
    let mut e = editor();
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(1, 1, Rgba([200, 50, 20, 128]));
    e.brush.size = 1.0;
    e.brush.hardness = 1.0;
    e.brush.opacity = 0.5;
    assert!(e.begin_clone_stroke((1.5, 1.5), (8.5, 8.5), false));
    assert_eq!(pixel(&e, 8, 8), [200, 50, 20, 64]);
    assert!(e.is_dirty());
    e.cancel_clone_stroke();
    assert_eq!(pixel(&e, 8, 8), [0; 4]);
    assert_eq!(e.undo_depth(), 0);
    assert!(!e.is_dirty());
}
#[test]
fn continuous_clone_rejects_locked_ancestor_and_preserves_selection() {
    let mut e = editor();
    let child = e.active_layer.clone();
    let group = e.add_group("Group");
    e.reorder_layer(&child, Some(&group), 0);
    e.select_layer(&child);
    e.set_locked(&group, true);
    assert!(!e.begin_clone_stroke((1.0, 1.0), (5.0, 5.0), false));
    e.set_locked(&group, false);
    e.document
        .find_layer_mut(&child)
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .put_pixel(1, 1, Rgba([255, 0, 0, 255]));
    e.brush.size = 4.0;
    e.select_rectangle(8.0, 8.0, 1.0, 1.0);
    e.begin_clone_stroke((1.5, 1.5), (8.5, 8.5), false);
    e.finish_clone_stroke();
    assert_eq!(pixel(&e, 8, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 7, 8), [0; 4]);
    assert_eq!(e.selection.as_ref().unwrap().bounds(), Some((8, 8, 1, 1)));
}
#[test]
fn continuous_heal_matches_flat_destination_without_creating_undo_noise() {
    let mut e = editor();
    e.fill_selection([100, 100, 100, 255]);
    for x in 0..5 {
        e.document.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(x, 1, Rgba([200, 150, 50, 255]));
    }
    e.mark_saved();
    let before = e.undo_depth();
    e.brush.size = 1.0;
    assert!(e.begin_clone_stroke((0.5, 1.5), (8.5, 8.5), true));
    e.continue_clone_stroke((12.5, 8.5));
    assert!(!e.finish_clone_stroke());
    assert_eq!(e.undo_depth(), before);
    assert!(!e.is_dirty());
    assert_eq!(pixel(&e, 10, 8), [100, 100, 100, 255]);
}
#[test]
fn clipboard_copy_crops_ellipse_and_cut_is_undoable() {
    let mut e = editor();
    e.fill_selection([200, 50, 30, 128]);
    e.select_ellipse(2.0, 2.0, 8.0, 8.0);
    let copied = e.copy_selection().unwrap();
    assert_eq!(copied.dimensions(), (8, 8));
    assert_eq!(copied.get_pixel(0, 0)[3], 0);
    assert_eq!(copied.get_pixel(4, 4).0, [200, 50, 30, 128]);
    assert!(e.cut_selection());
    assert_eq!(pixel(&e, 6, 6), [0; 4]);
    assert_eq!(pixel(&e, 2, 2), [200, 50, 30, 128]);
    assert!(e.undo());
    assert_eq!(pixel(&e, 6, 6), [200, 50, 30, 128]);
}
#[test]
fn clipboard_copy_handles_layer_transform_and_cut_retains_offcanvas_pixels() {
    let mut e = editor();
    e.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(4, 4, Rgba([200, 50, 30, 255])).into());
    let id = e.active_layer.clone();
    e.transform_layer(&id, -2.0, 0.0, 0.0, 1.0, 1.0);
    let copied = e.copy_selection().unwrap();
    assert_eq!(copied.get_pixel(0, 0).0, [200, 50, 30, 255]);
    assert_eq!(copied.get_pixel(2, 0)[3], 0);
    assert!(e.cut_selection());
    assert_eq!(pixel(&e, 0, 0), [200, 50, 30, 255]);
    assert_eq!(pixel(&e, 2, 0), [0; 4]);
}
#[test]
fn empty_selection_cannot_copy_or_cut_and_locked_layer_can_only_copy() {
    let mut e = editor();
    e.fill_selection([20, 30, 40, 255]);
    e.select_rectangle(0.0, 0.0, 0.0, 0.0);
    assert!(e.copy_selection().is_none());
    assert!(!e.cut_selection());
    e.clear_selection();
    let id = e.active_layer.clone();
    e.set_locked(&id, true);
    assert!(e.copy_selection().is_some());
    assert!(!e.cut_selection());
}
#[test]
fn unsupported_blend_mode_is_rejected_before_history_mutation() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(!e.set_blend_mode(&id, "invented"));
    assert!(!e.is_dirty());
    assert!(e.set_blend_mode(&id, "Linear Dodge (Add)"));
    assert!(omuse::raster::validate(&e.document).is_empty());
}

#[test]
fn merge_rejects_nonassociative_passthrough_group_coverage() {
    let mut e = editor();
    let lower = e.active_layer.clone();
    let upper = e.add_layer("Upper");
    let group = e.add_group("Group");
    e.reorder_layer(&lower, Some(&group), 0);
    e.reorder_layer(&upper, Some(&group), 1);
    e.select_layer(&upper);
    e.set_opacity(&group, 0.5);
    assert!(!e.merge_down());
    e.set_opacity(&group, 1.0);
    assert!(e.add_mask(&group, true));
    assert!(!e.merge_down());
    e.set_mask_enabled(&group, false);
    assert!(e.merge_down());
}
#[test]
fn nesting_cannot_exceed_renderer_depth_limit() {
    let mut e = editor();
    let leaf = e.active_layer.clone();
    let mut parent: Option<String> = None;
    for depth in 0..64 {
        let group = e.add_group(&format!("Group {depth}"));
        if let Some(previous) = parent {
            assert!(e.reorder_layer(&group, Some(&previous), 0));
        }
        parent = Some(group);
    }
    assert!(!e.reorder_layer(&leaf, parent.as_deref(), 0));
    assert!(omuse::raster::validate(&e.document).is_empty());
}

#[test]
fn polygon_lasso_uses_even_odd_fill_and_rejects_invalid_vertices() {
    let mut e = editor();
    assert!(e.select_polygon(&[(1.0, 1.0), (10.0, 1.0), (1.0, 10.0)]));
    let selection = e.selection.clone().unwrap();
    assert!(selection.contains(2, 2));
    assert!(!selection.contains(8, 8));
    assert!(!selection.contains(0, 0));
    assert!(!e.select_polygon(&[(0.0, 0.0), (1.0, f32::NAN), (2.0, 2.0)]));
    assert_eq!(e.selection, Some(selection));
    assert!(e.select_polygon(&[(2.0, 2.0), (10.0, 2.0), (10.0, 10.0), (2.0, 10.0)]));
    assert_eq!(e.selection.as_ref().unwrap().bounds(), Some((2, 2, 8, 8)));
}
#[test]
fn group_move_is_single_transaction_preserving_passthrough_coordinates() {
    let mut e = editor();
    let leaf = e.active_layer.clone();
    dot(&mut e, 2.5, 2.5, [255, 0, 0, 255]);
    let group = e.add_group("Group");
    e.reorder_layer(&leaf, Some(&group), 0);
    e.mark_saved();
    let depth = e.undo_depth();
    assert!(e.move_group(&group, 3.0, 4.0));
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(5, 6).0,
        [255, 0, 0, 255]
    );
    assert!(e.undo());
    assert!(!e.is_dirty());
    e.set_locked(&leaf, true);
    assert!(!e.move_group(&group, 1.0, 1.0));
}
#[test]
fn image_resize_preserves_structure_opacity_and_undo_but_bakes_transforms() {
    let mut e = editor();
    e.fill_selection([255, 0, 0, 255]);
    let leaf = e.active_layer.clone();
    e.set_opacity(&leaf, 0.5);
    let group = e.add_group("Group");
    e.reorder_layer(&leaf, Some(&group), 0);
    e.set_opacity(&group, 0.5);
    e.mark_saved();
    assert!(e.resize_image(32, 24));
    assert_eq!((e.document.width, e.document.height), (32, 24));
    assert_eq!(e.document.layers[0].id, group);
    let layer = e.document.find_layer(&leaf).unwrap();
    assert_eq!(layer.image.as_ref().unwrap().dimensions(), (32, 24));
    assert_eq!(layer.opacity, 0.5);
    assert_eq!(
        (layer.offset_x, layer.rotation, layer.scale_x),
        (0.0, 0.0, 1.0)
    );
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(16, 12).0,
        [255, 0, 0, 64]
    );
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert_eq!((e.document.width, e.document.height), (16, 16));
}
#[test]
fn nonuniform_image_resize_preserves_rotated_shape_position() {
    let mut e = editor();
    e.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(4, 2, Rgba([255, 0, 0, 255])).into());
    let id = e.active_layer.clone();
    e.transform_layer(&id, 4.0, 4.0, 90.0, 1.0, 1.0);
    assert!(e.resize_image(32, 16));
    let rendered = omuse::raster::composite(&e.document);
    assert!(rendered.get_pixel(12, 4)[3] > 200);
    assert_eq!(rendered.get_pixel(0, 0)[3], 0);
    assert_eq!(e.document.layers[0].rotation, 0.0);
}
#[test]
fn image_resize_preserves_group_mask_as_an_editable_mask() {
    let mut e = editor();
    e.fill_selection([255, 0, 0, 255]);
    let leaf = e.active_layer.clone();
    let group = e.add_group("Group");
    e.reorder_layer(&leaf, Some(&group), 0);
    e.select_rectangle(0.0, 0.0, 8.0, 16.0);
    assert!(e.add_mask(&group, true));
    e.clear_selection();
    assert!(e.resize_image(32, 32));
    let mask = e
        .document
        .find_layer(&group)
        .unwrap()
        .mask
        .as_ref()
        .unwrap();
    assert_eq!(mask.dimensions(), (32, 32));
    let rendered = omuse::raster::composite(&e.document);
    assert_eq!(rendered.get_pixel(4, 16).0, [255, 0, 0, 255]);
    assert_eq!(rendered.get_pixel(28, 16)[3], 0);
}
#[test]
fn erasing_mask_changes_coverage_not_artwork_and_is_undoable() {
    let mut e = editor();
    e.fill_selection([255, 0, 0, 255]);
    let id = e.active_layer.clone();
    e.add_mask(&id, true);
    e.mark_saved();
    assert!(e.erase_mask(&id, 4.5, 4.5, 0.5, 1.0));
    assert_eq!(pixel(&e, 4, 4), [255, 0, 0, 255]);
    assert_eq!(omuse::raster::composite(&e.document).get_pixel(4, 4)[3], 0);
    assert!(e.undo());
    assert!(!e.is_dirty());
    assert_eq!(
        omuse::raster::composite(&e.document).get_pixel(4, 4)[3],
        255
    );
}

#[test]
fn erasing_detached_mask_uses_its_canvas_placement_and_undoes() {
    let mut e = editor();
    let id = e.active_layer.clone();
    let source = e.document.find_layer(&id).unwrap().image.clone();
    {
        let layer = e.document.find_layer_mut(&id).unwrap();
        layer.mask = Some(image::RgbaImage::from_pixel(4, 4, Rgba([255; 4])).into());
        layer.metadata["maskLinked"] = serde_json::json!(false);
        layer.metadata["maskPlacement"] = serde_json::json!({
            "origin":[4, 4], "size":[4, 4], "rotation":0,
            "flipX":false, "flipY":false, "sampling":"High quality"
        });
    }
    assert!(!e.erase_mask(&id, 5.5, 5.5, 0.5, -0.1));
    assert!(e.erase_mask(&id, 5.5, 5.5, 0.5, 1.0));
    let layer = e.document.find_layer(&id).unwrap();
    assert_eq!(layer.image, source);
    assert_eq!(
        layer.mask.as_ref().unwrap().get_pixel(1, 1).0,
        [255, 255, 255, 0]
    );
    assert_eq!(layer.mask.as_ref().unwrap().get_pixel(0, 0).0, [255; 4]);
    assert_eq!(e.undo_depth(), 1);
    assert!(e.undo());
    assert!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .pixels()
            .all(|pixel| pixel.0 == [255; 4])
    );
}

#[test]
fn coordinate_api_round_trips_rotated_flipped_layers() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.transform_layer(&id, 7.0, 9.0, 37.0, -1.5, 0.75));
    let canvas = e.layer_to_canvas(&id, 3.25, 11.5).unwrap();
    let local = e.canvas_to_layer(&id, canvas.0, canvas.1).unwrap();
    assert!((local.0 - 3.25).abs() < 0.001 && (local.1 - 11.5).abs() < 0.001);
    let p = e.layer_placement(&id).unwrap();
    assert_eq!(
        (p.x, p.y, p.width, p.height, p.flip_x, p.flip_y),
        (7.0, 9.0, 24.0, 12.0, true, false)
    );
}

#[test]
fn group_box_transform_is_atomic_and_undoable() {
    let mut e = editor();
    let first = e.active_layer.clone();
    let second = e.add_layer("Second");
    e.set_layer_placement(
        &first,
        LayerPlacement {
            x: 0.,
            y: 0.,
            width: 16.,
            height: 16.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        },
    );
    e.set_layer_placement(
        &second,
        LayerPlacement {
            x: 20.,
            y: 0.,
            width: 16.,
            height: 16.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        },
    );
    let depth = e.undo_depth();
    let from = LayerPlacement {
        x: 0.,
        y: 0.,
        width: 36.,
        height: 16.,
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    let to = LayerPlacement {
        x: 10.,
        y: 5.,
        width: 72.,
        height: 32.,
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    assert!(e.transform_layers(&[first.clone(), second.clone()], from, to));
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(e.layer_placement(&first).unwrap().x, 10.0);
    assert_eq!(e.layer_placement(&second).unwrap().x, 50.0);
    assert!(e.undo());
    assert_eq!(e.layer_placement(&second).unwrap().x, 20.0);
}

#[test]
fn unlinked_mask_stays_on_canvas_while_layer_moves() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.add_mask(&id, true));
    assert!(e.set_mask_linked(&id, false));
    let before = e.mask_placement(&id).unwrap();
    assert!(e.set_layer_placement(
        &id,
        LayerPlacement {
            x: 5.,
            y: 3.,
            ..before
        }
    ));
    assert_eq!(e.mask_placement(&id), Some(before));
    assert!(e.undo());
    assert_eq!(e.mask_placement(&id), Some(before));
}

#[test]
fn guides_follow_crop_and_every_change_undoes() {
    let mut e = editor();
    let horizontal = e.add_guide(GuideAxis::Horizontal, 9.0).unwrap();
    let vertical = e.add_guide(GuideAxis::Vertical, 7.0).unwrap();
    assert!(e.crop_canvas(2, 3, 10, 10));
    let guides = e.guides();
    assert_eq!(
        guides.iter().find(|g| g.id == horizontal).unwrap().position,
        6.0
    );
    assert_eq!(
        guides.iter().find(|g| g.id == vertical).unwrap().position,
        5.0
    );
    assert!(e.undo());
    assert_eq!(e.guides().len(), 2);
    assert!(e.remove_guide(&horizontal));
    assert!(e.undo());
    assert_eq!(e.guides().len(), 2);
}

#[test]
fn live_objects_refuse_every_destructive_pixel_entry_until_rasterized() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(
        e.set_live_text(&id, omuse::objects::LiveTextStyle::default())
            .unwrap()
    );
    assert!(!e.fill_at(0, 0, [255; 4], 0));
    assert!(!e.fill_selection([255; 4]));
    assert!(!e.gradient((0., 0.), (5., 0.), [0; 4], [255; 4]));
    assert!(!e.adjust(Adjustment::Invert));
    assert!(!e.apply_filter(&omuse::filters::Filter::Invert));
    assert!(!e.clone_stamp((0., 0.), (2., 2.), 1., 1., false));
    assert!(!e.cut_selection());
    assert!(e.rasterize_layer(&id));
    assert!(e.fill_selection([20, 30, 40, 255]));
    assert!(e.undo());
    assert!(e.undo());
    assert!(
        omuse::objects::live_text(e.document.find_layer(&id).unwrap())
            .unwrap()
            .is_some()
    );
}

#[test]
fn distortion_warps_pixels_and_linked_mask_in_one_undo_step() {
    let mut e = Editor::new(Document::new(8, 8));
    let id = e.active_layer.clone();
    e.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])).into());
    assert!(e.add_mask(&id, true));
    let depth = e.undo_depth();
    assert!(
        e.distort_layer(&id, [(1., 1.), (6., 0.), (7., 7.), (0., 6.)])
            .unwrap()
    );
    let layer = e.document.find_layer(&id).unwrap();
    assert_eq!(
        layer.image.as_ref().unwrap().dimensions(),
        layer.mask.as_ref().unwrap().dimensions()
    );
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.undo());
    assert_eq!(
        e.document
            .find_layer(&id)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .dimensions(),
        (2, 2)
    );
}

#[test]
fn semantic_effect_and_adjustment_edits_validate_and_undo() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(
        e.set_layer_effects(&id, serde_json::json!({"stroke":{"size":2.0}}))
            .unwrap()
    );
    assert!(
        e.set_layer_effects(&id, serde_json::json!({"future":true}))
            .is_err()
    );
    let adjustment = e
        .add_adjustment(serde_json::json!({"kind":"Invert"}))
        .unwrap();
    assert!(
        e.set_adjustment(
            &adjustment,
            serde_json::json!({"kind":"Gaussian Blur","blurRadius":2.0})
        )
        .unwrap()
    );
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&adjustment).unwrap().metadata["adjustment"]["kind"],
        "Invert"
    );
}

#[test]
fn retouch_stroke_maps_canvas_to_source_and_is_one_undo_step() {
    let mut e = Editor::new(Document::new(12, 10));
    let id = e.active_layer.clone();
    e.document.layers[0].image = Some(image::RgbaImage::new(6, 5).into());
    e.transform_layer(&id, 1.0, 1.0, 0.0, 2.0, 2.0);
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(1, 2, Rgba([255; 4]));
    let original = e.document.layers[0].image.clone().unwrap();
    e.brush.size = 8.0;
    e.brush.hardness = 0.5;
    e.brush.opacity = 1.0;
    let depth = e.undo_depth();
    assert!(
        e.retouch_stroke(&[(3.0, 5.0), (9.0, 5.0)], RetouchMode::Liquify)
            .unwrap()
    );
    assert_eq!(e.undo_depth(), depth + 1);
    assert_ne!(
        e.document.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &original
    );
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &original
    );
}

#[test]
fn image_operation_honors_soft_selection_and_rejects_resize() {
    let mut e = editor();
    e.fill_selection([0, 0, 0, 255]);
    e.selection = Some(omuse::editor::Selection {
        width: 16,
        height: 16,
        mask: vec![128; 256],
    });
    assert!(
        e.apply_image_operation(|image| Ok(image::RgbaImage::from_pixel(
            image.width(),
            image.height(),
            Rgba([255, 0, 0, 255])
        )))
        .unwrap()
    );
    let changed = pixel(&e, 4, 4);
    assert!(changed[0] >= 127 && changed[0] <= 129);
    assert!(
        e.apply_image_operation(|_| Ok(image::RgbaImage::new(1, 1)))
            .is_err()
    );
}

#[test]
fn retouch_refuses_locked_and_live_objects_without_history() {
    let mut e = editor();
    let id = e.active_layer.clone();
    e.set_locked(&id, true);
    let depth = e.undo_depth();
    assert!(
        !e.retouch_stroke(&[(2., 2.), (8., 2.)], RetouchMode::Smudge)
            .unwrap()
    );
    assert_eq!(e.undo_depth(), depth);
    e.set_locked(&id, false);
    e.set_live_text(&id, omuse::objects::LiveTextStyle::default())
        .unwrap();
    let depth = e.undo_depth();
    assert!(
        !e.retouch_stroke(&[(2., 2.), (8., 2.)], RetouchMode::Smudge)
            .unwrap()
    );
    assert_eq!(e.undo_depth(), depth);
}

#[test]
fn subject_mask_multiplies_existing_mask_and_undo_restores_source() {
    let mut e = editor();
    let id = e.active_layer.clone();
    e.document.find_layer_mut(&id).unwrap().image =
        Some(image::RgbaImage::from_pixel(16, 16, Rgba([40, 80, 120, 255])).into());
    assert!(e.add_mask(&id, true));
    e.document.find_layer_mut(&id).unwrap().mask =
        Some(image::RgbaImage::from_pixel(16, 16, Rgba([128, 128, 128, 255])).into());
    let before = e.document.clone();
    let depth = e.undo_depth();
    let subject = image::GrayImage::from_pixel(16, 16, image::Luma([128]));
    assert!(e.apply_subject_mask(&id, &subject, false).unwrap());
    let layer = e.document.find_layer(&id).unwrap();
    assert_eq!(layer.mask.as_ref().unwrap().get_pixel(5, 5)[0], 64);
    assert_eq!(layer.image, before.find_layer(&id).unwrap().image);
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&id).unwrap().mask,
        before.find_layer(&id).unwrap().mask
    );
    assert!(e.redo());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cutout.comp");
    omuse::document::save(&e.document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(
        omuse::raster::composite(&reopened),
        omuse::raster::composite(&e.document)
    );
}
#[test]
fn subject_selection_projects_transformed_layer_without_changing_pixels() {
    let mut e = editor();
    let id = e.active_layer.clone();
    let mut placement = e.layer_placement(&id).unwrap();
    placement.x = 4.;
    placement.y = 3.;
    placement.width = 8.;
    placement.height = 8.;
    assert!(e.set_layer_placement(&id, placement));
    let depth = e.undo_depth();
    assert!(
        e.apply_subject_mask(
            &id,
            &image::GrayImage::from_pixel(16, 16, image::Luma([128])),
            true
        )
        .unwrap()
    );
    let selection = e.selection.as_ref().unwrap();
    assert_eq!(selection.mask[4 * 16 + 5], 128);
    assert_eq!(selection.mask[0], 0);
    assert_eq!(e.undo_depth(), depth);
}
#[test]
fn content_fill_is_one_transaction_and_preserves_unselected_pixels() {
    let mut e = editor();
    let id = e.active_layer.clone();
    e.document.find_layer_mut(&id).unwrap().image = Some(
        image::RgbaImage::from_fn(16, 16, |x, y| {
            Rgba([(x * 10) as u8, (y * 10) as u8, 70, 255])
        })
        .into(),
    );
    e.document
        .find_layer_mut(&id)
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .put_pixel(8, 8, Rgba([255, 0, 255, 255]));
    let before = e.document.find_layer(&id).unwrap().image.clone().unwrap();
    e.select_rectangle(8., 8., 1., 1.);
    let depth = e.undo_depth();
    assert!(e.content_aware_fill().unwrap());
    assert_eq!(e.undo_depth(), depth + 1);
    let after = e.document.find_layer(&id).unwrap().image.as_ref().unwrap();
    assert_ne!(after.get_pixel(8, 8), before.get_pixel(8, 8));
    for (x, y, p) in before.enumerate_pixels() {
        if (x, y) != (8, 8) {
            assert_eq!(after.get_pixel(x, y), p)
        }
    }
    assert!(e.undo());
    assert_eq!(
        e.document.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &before
    );
}
#[test]
fn mask_painting_respects_placement_soft_selection_and_single_undo() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.add_mask(&id, true));
    let pixels = e.document.find_layer(&id).unwrap().image.clone();
    e.selection = Some(omuse::editor::Selection {
        width: 16,
        height: 16,
        mask: vec![128; 256],
    });
    e.brush.size = 1.;
    e.brush.color = [0, 0, 0, 255];
    e.brush.opacity = 1.;
    let before = e.undo_depth();
    assert!(e.begin_mask_stroke(4.5, 4.5, 1., PaintTool::Pencil));
    assert!(e.finish_stroke());
    assert_eq!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(4, 4)[0],
        127
    );
    assert_eq!(e.document.find_layer(&id).unwrap().image, pixels);
    assert_eq!(e.undo_depth(), before + 1);
    assert!(e.undo());
    assert_eq!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(4, 4)[0],
        255
    );
    e.selection = None;
    assert!(e.set_mask_linked(&id, false));
    let mut place = e.mask_placement(&id).unwrap();
    place.x = 4.;
    place.y = 2.;
    assert!(e.set_mask_placement(&id, place));
    assert!(e.begin_mask_stroke(5.5, 3.5, 1., PaintTool::Pencil));
    e.finish_stroke();
    assert_eq!(
        e.document
            .find_layer(&id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(1, 1)[0],
        0
    );
}

#[test]
fn sampling_changes_are_undoable_and_roundtrip_canonical_metadata() {
    let mut editor = Editor::new(Document::new(8, 8));
    let id = editor.active_layer.clone();
    assert!(editor.set_sampling(&id, "Nearest", false).unwrap());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .metadata
            .pointer("/transform/sampling")
            .unwrap(),
        "Nearest"
    );
    let depth = editor.undo_depth();
    assert!(!editor.set_sampling(&id, "Nearest", false).unwrap());
    assert_eq!(editor.undo_depth(), depth);
    assert!(editor.set_sampling(&id, "Smooth", false).unwrap());
    assert!(editor.undo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .metadata
            .pointer("/transform/sampling")
            .unwrap(),
        "Nearest"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Sampling.comp");
    omuse::document::save(&editor.document, &path).unwrap();
    let opened = omuse::document::open(&path).unwrap();
    assert_eq!(
        opened.layers[0]
            .metadata
            .pointer("/transform/sampling")
            .unwrap(),
        "Nearest"
    );
    assert!(editor.set_sampling(&id, "High Quality", false).is_err());
}

#[test]
fn folder_mask_sampling_changes_the_folder_transform_and_undoes() {
    let mut editor = Editor::new(Document::new(8, 8));
    let id = editor.add_group("Masked group");
    assert!(editor.add_mask(&id, true));
    let depth = editor.undo_depth();
    assert!(editor.set_sampling(&id, "Nearest", true).unwrap());
    let layer = editor.document.find_layer(&id).unwrap();
    assert_eq!(
        layer.metadata.pointer("/transform/sampling").unwrap(),
        "Nearest"
    );
    assert!(layer.metadata.get("maskPlacement").is_none());
    assert_eq!(editor.undo_depth(), depth + 1);
    assert!(editor.undo());
    assert!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .metadata
            .pointer("/transform/sampling")
            .is_none()
    );
}

#[test]
fn image_resize_preserves_off_canvas_pixels_and_scales_guides() {
    let mut editor = Editor::new(Document::new(16, 16));
    let layer = &mut editor.document.layers[0];
    layer.image = Some(image::RgbaImage::from_pixel(4, 2, Rgba([240, 30, 10, 255])).into());
    layer.offset_x = -3.;
    layer.offset_y = 4.;
    editor.add_guide(GuideAxis::Vertical, 6.);
    let original = editor.document.clone();
    let depth = editor.undo_depth();
    assert!(editor.resize_image(32, 32));
    let layer = &editor.document.layers[0];
    assert_eq!((layer.offset_x, layer.offset_y), (-6., 8.));
    assert_eq!(layer.image.as_ref().unwrap().dimensions(), (8, 4));
    assert!(
        layer
            .image
            .as_ref()
            .unwrap()
            .pixels()
            .all(|p| p[0] == 240 && p[3] == 255)
    );
    assert_eq!(editor.guides()[0].position, 12.);
    assert_eq!(editor.undo_depth(), depth + 1);
    assert!(editor.undo());
    assert_eq!(editor.document.layers[0].image, original.layers[0].image);
    assert_eq!(editor.document.layers[0].offset_x, -3.);
}

#[test]
fn image_resize_bakes_live_shape_and_scales_independent_mask() {
    let mut editor = Editor::new(Document::new(16, 16));
    let id = editor.active_layer.clone();
    let style = omuse::objects::LiveShapeStyle {
        kind: omuse::objects::LiveShapeKind::Rectangle,
        red: 1.,
        green: 0.,
        blue: 0.,
        corner_radius: 0.,
        line_width: None,
        start: None,
        end: None,
    };
    omuse::objects::set_live_shape(&mut editor.document.layers[0], style, 8, 8).unwrap();
    assert!(editor.add_mask(&id, true));
    assert!(editor.set_mask_linked(&id, false));
    assert!(editor.set_mask_placement(
        &id,
        LayerPlacement {
            x: 3.,
            y: 2.,
            width: 4.,
            height: 6.,
            rotation: 0.,
            flip_x: false,
            flip_y: false
        }
    ));
    let original_mask = editor.document.layers[0].mask.clone();
    assert!(editor.resize_image(32, 32));
    let layer = &editor.document.layers[0];
    assert!(layer.metadata.get("shape").is_none());
    assert!(layer.metadata.get("compositorRustRasterSource").is_none());
    assert_eq!(layer.mask, original_mask);
    let placement = editor.mask_placement(&id).unwrap();
    assert_eq!(
        (placement.x, placement.y, placement.width, placement.height),
        (6., 4., 8., 12.)
    );
    assert!(editor.undo());
    assert!(editor.document.layers[0].metadata.get("shape").is_some());
}

#[test]
fn image_resize_options_commit_dpi_only_and_validate_sampling() {
    let mut editor = editor();
    let image = editor.document.layers[0].image.clone();
    assert!(editor.resize_image_with_options(16, 16, 300., "Nearest"));
    assert_eq!(
        editor.document.metadata["resolution"],
        serde_json::json!(300.)
    );
    assert_eq!(editor.document.layers[0].image, image);
    assert_eq!(editor.undo_depth(), 1);
    assert!(!editor.resize_image_with_options(16, 16, 300., "Nearest"));
    assert!(!editor.resize_image_with_options(32, 32, 300., "Invalid"));
    assert!(editor.undo());
    assert!(editor.document.metadata.get("resolution").is_none());
}

#[test]
fn image_resize_covering_rotated_mask_uses_black_outside() {
    let mut editor = Editor::new(Document::new(16, 16));
    let id = editor.active_layer.clone();
    editor.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255])).into());
    editor.document.layers[0].offset_x = 4.;
    editor.document.layers[0].offset_y = 4.;
    editor.document.layers[0].rotation = 45.;
    assert!(editor.add_mask(&id, true));
    assert!(editor.resize_image_with_options(32, 16, 72., "High quality"));
    let mask = editor
        .document
        .find_layer(&id)
        .unwrap()
        .mask
        .as_ref()
        .unwrap();
    assert_eq!(mask.get_pixel(0, 0).0, [0, 0, 0, 255]);
}
