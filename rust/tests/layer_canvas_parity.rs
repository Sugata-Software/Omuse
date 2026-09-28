use image::Rgba;
use omuse::{
    editor::{Editor, GuideAxis, TrimBasedOn, TrimOptions},
    model::{Document, Layer},
    raster,
};

fn editor() -> Editor {
    Editor::new(Document::new(8, 6))
}
fn names(layers: &[Layer]) -> Vec<String> {
    layers.iter().map(|l| l.name.clone()).collect()
}
fn ids(layers: &[Layer]) -> Vec<String> {
    layers.iter().map(|l| l.id.clone()).collect()
}
fn paint(e: &mut Editor, x: u32, y: u32, color: [u8; 4]) {
    e.document
        .find_layer_mut(&e.active_layer)
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .put_pixel(x, y, Rgba(color));
}

#[test]
fn grouping_noncontiguous_roots_uses_topmost_selected_branch_and_one_undo() {
    let mut e = editor();
    let a = e.active_layer.clone();
    e.rename_layer(&a, "A");
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    let d = e.add_layer("D");
    let before = ids(&e.document.layers);
    let depth = e.undo_depth();
    let group = e
        .group_layers(&[c.clone(), a.clone(), c.clone()], "Pair")
        .unwrap();
    assert_eq!(names(&e.document.layers), vec!["B", "Pair", "D"]);
    assert_eq!(
        ids(&e.document.find_layer(&group).unwrap().children),
        vec![a, c]
    );
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(e.active_layer, group);
    assert!(e.undo());
    assert_eq!(ids(&e.document.layers), before);
    assert_eq!(e.active_layer, d);
    assert!(e.document.find_layer(&b).is_some());
}
#[test]
fn grouping_inside_common_ancestor_preserves_selected_folders_and_descendants() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let outer = e.add_group("Outer");
    let inner = e.add_group("Inner");
    e.reparent_layers(&[a.clone()], Some(&inner), 0);
    e.reparent_layers(&[inner.clone(), b.clone()], Some(&outer), 0);
    let group = e
        .group_layers(&[inner.clone(), a.clone(), b.clone()], "")
        .unwrap();
    assert_eq!(e.document.layers.len(), 1);
    assert_eq!(e.document.layers[0].id, outer);
    assert_eq!(e.document.find_layer(&outer).unwrap().children[0].id, group);
    assert_eq!(e.document.find_layer(&group).unwrap().name, "Folder 1");
    assert_eq!(
        ids(&e.document.find_layer(&group).unwrap().children),
        vec![b, inner.clone()]
    );
    assert_eq!(e.document.find_layer(&inner).unwrap().children[0].id, a);
}
#[test]
fn grouping_across_folders_uses_common_parent_without_pulling_selected_descendants_twice() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let g1 = e.add_group("G1");
    e.reparent_layers(&[a.clone()], Some(&g1), 0);
    let b = e.add_layer("B");
    let g2 = e.add_group("G2");
    e.reparent_layers(&[b.clone()], Some(&g2), 0);
    let wrapped = e.group_layers(&[a.clone(), b.clone()], "Together").unwrap();
    assert_eq!(names(&e.document.layers), vec!["G1", "G2", "Together"]);
    assert_eq!(
        ids(&e.document.find_layer(&wrapped).unwrap().children),
        vec![a, b]
    );
}
#[test]
fn locked_descendants_and_invalid_ids_reject_whole_structural_batches() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let group = e.add_group("Group");
    e.reparent_layers(&[a.clone()], Some(&group), 0);
    let other = e.add_layer("Other");
    e.set_locked(&a, true);
    let before = e.undo_depth();
    assert!(
        e.group_layers(&[group.clone(), other.clone()], "No")
            .is_none()
    );
    assert!(!e.delete_layers(&[group.clone(), other.clone()]));
    assert!(!e.reparent_layers(&[group.clone()], None, 2));
    assert!(e.duplicate_layers(&[group.clone()]).is_empty());
    assert!(!e.move_layers(&[group], 1., 1.));
    assert!(
        e.duplicate_layers(&[other.clone(), "missing".into()])
            .is_empty()
    );
    assert!(!e.delete_layers(&[other, "missing".into()]));
    assert_eq!(e.undo_depth(), before);
}
#[test]
fn duplicate_batch_remaps_cross_tree_live_masks_and_keeps_originals_independent() {
    let mut e = editor();
    let a = e.active_layer.clone();
    e.rename_layer(&a, "A");
    let b = e.add_layer("B");
    e.document.find_layer_mut(&b).unwrap().metadata["maskSourceID"] = serde_json::json!(a);
    let depth = e.undo_depth();
    let copies = e.duplicate_layers(&[b.clone(), a.clone()]);
    assert_eq!(copies.len(), 2);
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(
        names(&e.document.layers),
        vec!["A", "A copy", "B", "B copy"]
    );
    assert_eq!(
        e.document.find_layer(&copies[1]).unwrap().metadata["maskSourceID"],
        copies[0]
    );
    assert_eq!(
        e.document.find_layer(&b).unwrap().metadata["maskSourceID"],
        a
    );
    e.select_layer(&copies[0]);
    e.fill_selection([255, 0, 0, 255]);
    assert_eq!(
        e.document
            .find_layer(&a)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)[3],
        0
    );
}
#[test]
fn duplicate_selected_folder_does_not_duplicate_selected_child_twice() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let g = e.add_group("Group");
    e.reparent_layers(&[a.clone()], Some(&g), 0);
    let copies = e.duplicate_layers(&[g.clone(), a.clone()]);
    assert_eq!(copies.len(), 1);
    assert_eq!(e.document.find_layer(&copies[0]).unwrap().children.len(), 1);
    assert_ne!(e.document.find_layer(&copies[0]).unwrap().children[0].id, a);
}
#[test]
fn bulk_delete_is_single_undo_and_preserves_an_editable_layer() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let original = ids(&e.document.layers);
    let depth = e.undo_depth();
    assert!(e.delete_layers(&[a, b]));
    assert_eq!(e.document.layers.len(), 1);
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.document.layers[0].image.is_some());
    assert!(e.undo());
    assert_eq!(ids(&e.document.layers), original);
}
#[test]
fn live_mask_delete_requires_explicit_unlink_and_respects_locked_dependents() {
    let mut e = editor();
    let source = e.active_layer.clone();
    let target = e.add_layer("Target");
    e.document.find_layer_mut(&target).unwrap().metadata["maskSourceID"] =
        serde_json::json!(source);
    let depth = e.undo_depth();
    assert!(!e.delete_layers(&[source.clone()]));
    assert_eq!(e.undo_depth(), depth);
    e.set_locked(&target, true);
    assert!(!e.delete_layers_with_unlink(&[source.clone()], true));
    e.set_locked(&target, false);
    assert!(e.delete_layers_with_unlink(&[source.clone()], true));
    assert!(
        e.document
            .find_layer(&target)
            .unwrap()
            .metadata
            .get("maskSourceID")
            .is_none()
    );
    assert!(e.undo());
    assert!(e.document.find_layer(&source).is_some());
    assert_eq!(
        e.document.find_layer(&target).unwrap().metadata["maskSourceID"],
        source
    );
}
#[test]
fn multi_drop_adjusts_indices_preserves_order_and_undoes_atomically() {
    let mut e = editor();
    let a = e.active_layer.clone();
    e.rename_layer(&a, "A");
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    let d = e.add_layer("D");
    let depth = e.undo_depth();
    assert_eq!(
        e.drop_layers(&[c.clone(), a.clone()], Some(&d), 1, false),
        vec![a.clone(), c.clone()]
    );
    assert_eq!(names(&e.document.layers), vec!["B", "D", "A", "C"]);
    assert_eq!(e.undo_depth(), depth + 1);
    e.undo();
    assert_eq!(names(&e.document.layers), vec!["A", "B", "C", "D"]);
    assert_eq!(
        e.drop_layers(&[c.clone(), d.clone()], Some(&b), -1, false),
        vec![c, d]
    );
    assert_eq!(names(&e.document.layers), vec!["A", "C", "D", "B"]);
}
#[test]
fn drop_rejects_cycles_and_supports_copy_into_group_as_single_transaction() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let g = e.add_group("G");
    let inner = e.add_group("Inner");
    e.reparent_layers(&[inner.clone()], Some(&g), 0);
    let before = e.undo_depth();
    assert!(
        e.drop_layers(&[g.clone()], Some(&inner), 0, false)
            .is_empty()
    );
    assert_eq!(e.undo_depth(), before);
    let copies = e.drop_layers(&[a], Some(&inner), 0, true);
    assert_eq!(copies.len(), 1);
    assert_eq!(
        e.document.find_layer(&inner).unwrap().children[0].id,
        copies[0]
    );
    assert_eq!(e.undo_depth(), before + 1);
}
#[test]
fn no_op_reparent_does_not_pollute_history() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let before = e.undo_depth();
    assert!(!e.reparent_layers(&[a, b], None, 0));
    assert_eq!(e.undo_depth(), before);
}
#[test]
fn move_selected_folder_and_child_only_translates_descendant_once() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let group = e.add_group("G");
    e.reparent_layers(&[a.clone()], Some(&group), 0);
    let before = e.undo_depth();
    assert!(e.move_layers(&[a.clone(), group], 3., 4.));
    let l = e.document.find_layer(&a).unwrap();
    assert_eq!((l.offset_x, l.offset_y), (3., 4.));
    assert_eq!(e.undo_depth(), before + 1);
    e.undo();
    assert_eq!(e.document.find_layer(&a).unwrap().offset_x, 0.);
}
#[test]
fn all_nine_anchors_match_floor_rule_for_growth_and_shrink() {
    for anchor in 0..9u8 {
        let mut e = editor();
        let id = e.active_layer.clone();
        assert!(e.resize_canvas_anchored(11, 9, anchor, None));
        let l = e.document.find_layer(&id).unwrap();
        assert_eq!(l.offset_x, (3. * f32::from(anchor % 3) / 2.).floor());
        assert_eq!(l.offset_y, (3. * f32::from(anchor / 3) / 2.).floor());
        assert!(e.undo());
        assert!(e.resize_canvas_anchored(5, 3, anchor, None));
        let l = e.document.find_layer(&id).unwrap();
        assert_eq!(l.offset_x, (-3. * f32::from(anchor % 3) / 2.).floor());
        assert_eq!(l.offset_y, (-3. * f32::from(anchor / 3) / 2.).floor());
    }
}
#[test]
fn colored_extension_preserves_transparent_holes_and_forces_opaque_fill() {
    let mut e = editor();
    let original = e.active_layer.clone();
    paint(&mut e, 1, 1, [255, 0, 0, 255]);
    let depth = e.undo_depth();
    assert!(e.resize_canvas_anchored(12, 10, 4, Some([20, 40, 60, 1])));
    assert_eq!(e.document.layers[0].name, "Canvas Extension");
    assert_eq!(e.active_layer, original);
    assert_eq!(e.undo_depth(), depth + 1);
    let rendered = raster::composite(&e.document);
    assert_eq!(rendered.get_pixel(0, 0).0, [20, 40, 60, 255]);
    assert_eq!(rendered.get_pixel(2, 2).0, [0; 4]);
    assert_eq!(rendered.get_pixel(3, 3).0, [255, 0, 0, 255]);
    assert!(e.undo());
    assert_eq!(e.document.layers.len(), 1);
    assert_eq!((e.document.width, e.document.height), (8, 6));
}
#[test]
fn extension_fill_handles_grow_one_axis_shrink_other_and_omits_shrink_only_fill() {
    let mut e = editor();
    assert!(e.resize_canvas_anchored(10, 4, 4, Some([255, 255, 255, 255])));
    let img = raster::composite(&e.document);
    assert_eq!(img.get_pixel(0, 0)[3], 255);
    assert_eq!(img.get_pixel(1, 0)[3], 0);
    assert_eq!(img.get_pixel(9, 0)[3], 255);
    e.undo();
    assert!(e.resize_canvas_anchored(6, 4, 4, Some([255; 4])));
    assert_eq!(e.document.layers.len(), 1);
}
#[test]
fn anchor_resize_moves_guides_and_detached_mask_placement() {
    let mut e = editor();
    let id = e.active_layer.clone();
    e.add_guide(GuideAxis::Vertical, 3.);
    e.add_guide(GuideAxis::Horizontal, 2.);
    let layer = e.document.find_layer_mut(&id).unwrap();
    layer.mask = Some(image::RgbaImage::from_pixel(8, 6, Rgba([255; 4])).into());
    layer.metadata["maskLinked"] = serde_json::json!(false);
    layer.metadata["maskPlacement"] = serde_json::json!({"origin":[1.,2.],"size":[8.,6.],"rotation":0.,"flipX":false,"flipY":false});
    assert!(e.resize_canvas_anchored(12, 10, 4, None));
    assert_eq!(e.guides()[0].position, 5.);
    assert_eq!(e.guides()[1].position, 4.);
    let mask = e.mask_placement(&id).unwrap();
    assert_eq!((mask.x, mask.y), (3., 4.));
}
#[test]
fn trim_transparency_respects_individual_edges_and_is_nondestructive() {
    let mut e = editor();
    paint(&mut e, 2, 1, [255, 0, 0, 255]);
    paint(&mut e, 5, 4, [0, 255, 0, 1]);
    assert_eq!(e.trim_bounds(TrimOptions::default()), Some((2, 1, 4, 4)));
    let options = TrimOptions {
        left: false,
        bottom: false,
        ..Default::default()
    };
    assert_eq!(e.trim_bounds(options), Some((0, 1, 6, 5)));
    let depth = e.undo_depth();
    assert!(e.trim_canvas(TrimOptions::default()));
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!((e.document.width, e.document.height), (4, 4));
    assert_eq!(
        e.document.layers[0].image.as_ref().unwrap().dimensions(),
        (8, 6)
    );
    assert_eq!(
        raster::composite(&e.document).get_pixel(0, 0).0,
        [255, 0, 0, 255]
    );
    assert!(e.undo());
    assert_eq!((e.document.width, e.document.height), (8, 6));
}
#[test]
fn color_trim_samples_both_corners_and_applies_tolerance() {
    let mut e = editor();
    e.fill_selection([240, 240, 240, 255]);
    paint(&mut e, 2, 1, [10, 20, 30, 255]);
    paint(&mut e, 5, 4, [235, 240, 240, 255]);
    let options = TrimOptions {
        based_on: TrimBasedOn::TopLeft,
        tolerance: 5,
        ..Default::default()
    };
    assert_eq!(e.trim_bounds(options), Some((2, 1, 1, 1)));
    assert_eq!(
        e.trim_bounds(TrimOptions {
            based_on: TrimBasedOn::BottomRight,
            ..options
        }),
        Some((2, 1, 1, 1))
    );
    assert_eq!(
        e.trim_bounds(TrimOptions {
            tolerance: 0,
            ..options
        }),
        Some((2, 1, 4, 4))
    );
}
#[test]
fn empty_uniform_and_no_edge_trim_leave_history_and_document_intact() {
    let mut e = editor();
    assert!(!e.trim_canvas(TrimOptions::default()));
    assert!(!e.is_dirty());
    e.fill_selection([12, 34, 56, 255]);
    e.mark_saved();
    let depth = e.undo_depth();
    assert!(!e.trim_canvas(TrimOptions {
        based_on: TrimBasedOn::TopLeft,
        ..Default::default()
    }));
    assert!(!e.trim_canvas(TrimOptions {
        top: false,
        bottom: false,
        left: false,
        right: false,
        ..Default::default()
    }));
    assert_eq!(e.undo_depth(), depth);
    assert!(!e.is_dirty());
}
#[test]
fn invalid_anchor_size_and_offsets_are_atomic() {
    let mut e = editor();
    assert!(!e.resize_canvas_anchored(12, 10, 9, None));
    assert!(!e.resize_canvas_anchored(0, 10, 4, None));
    assert!(!e.crop_canvas(i32::MIN, 0, 8, 6));
    assert!(!e.resize_canvas_anchored(8, 6, 4, Some([255; 4])));
    assert!(!e.is_dirty());
}

#[test]
fn copied_mask_keeps_document_position_enable_link_flags_and_clipping_links() {
    let mut e = editor();
    let source = e.active_layer.clone();
    e.add_mask(&source, true);
    e.set_mask_enabled(&source, false);
    e.set_mask_linked(&source, false);
    e.transform_layer(&source, 3., 2., 0., 1., 1.);
    // Detached mask stayed at the original position when its source image moved.
    let placement = e.mask_placement(&source).unwrap();
    let target = e.add_layer("Target");
    e.transform_layer(&target, 10., 20., 0., 1., 1.);
    e.document.find_layer_mut(&target).unwrap().metadata["maskSourceID"] =
        serde_json::json!(source);
    let depth = e.undo_depth();
    assert!(e.copy_layer_mask(&source, &target, true).unwrap());
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.document.find_layer(&source).unwrap().mask.is_some());
    assert_eq!(e.mask_placement(&target), Some(placement));
    let layer = e.document.find_layer(&target).unwrap();
    assert_eq!(layer.metadata["maskEnabled"], false);
    assert_eq!(layer.metadata["maskLinked"], false);
    assert_eq!(layer.metadata["maskSourceID"], source);
    assert!(e.undo());
    assert!(e.document.find_layer(&target).unwrap().mask.is_none());
}
#[test]
fn moved_mask_is_atomic_and_rejects_locked_source_or_folder_target() {
    let mut e = editor();
    let source = e.active_layer.clone();
    e.add_mask(&source, true);
    let target = e.add_layer("Target");
    e.set_locked(&source, true);
    assert!(!e.copy_layer_mask(&source, &target, false).unwrap());
    assert!(e.copy_layer_mask(&source, &target, true).unwrap());
    e.set_locked(&source, false);
    let group = e.add_group("Group");
    assert!(!e.copy_layer_mask(&source, &group, true).unwrap());
    let depth = e.undo_depth();
    assert!(e.copy_layer_mask(&source, &target, false).unwrap());
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.document.find_layer(&source).unwrap().mask.is_none());
    assert!(e.undo());
    assert!(e.document.find_layer(&source).unwrap().mask.is_some());
}
#[test]
fn effect_row_copy_preserves_other_effects_and_stack_move_is_one_undo() {
    let mut e = editor();
    let source = e.active_layer.clone();
    e.document.find_layer_mut(&source).unwrap().metadata["effects"] =
        serde_json::json!({"stroke":{"size":2.},"colorOverlay":{"red":1.}});
    let target = e.add_layer("Target");
    e.document.find_layer_mut(&target).unwrap().metadata["effects"] =
        serde_json::json!({"shadow":{"distance":3.}});
    e.document.find_layer_mut(&target).unwrap().metadata["customField"] = serde_json::json!("keep");
    let depth = e.undo_depth();
    assert!(
        e.copy_layer_effect(&source, &target, "stroke", true)
            .unwrap()
    );
    let effects = &e.document.find_layer(&target).unwrap().metadata["effects"];
    assert!(effects.get("shadow").is_some());
    assert!(effects.get("stroke").is_some());
    assert!(effects.get("colorOverlay").is_none());
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.copy_layer_effects(&source, &target, false).unwrap());
    assert!(
        e.document
            .find_layer(&source)
            .unwrap()
            .metadata
            .get("effects")
            .is_none()
    );
    assert_eq!(
        e.document.find_layer(&target).unwrap().metadata["customField"],
        "keep"
    );
    assert!(e.undo());
    assert!(
        e.document
            .find_layer(&source)
            .unwrap()
            .metadata
            .get("effects")
            .is_some()
    );
}
#[test]
fn invalid_effect_and_mask_metadata_do_not_partially_replace_targets() {
    let mut e = editor();
    let source = e.active_layer.clone();
    e.add_mask(&source, true);
    let target = e.add_layer("Target");
    e.document.find_layer_mut(&source).unwrap().metadata["effects"] =
        serde_json::json!({"stroke":{"size":-1.}});
    let depth = e.undo_depth();
    assert!(e.copy_layer_effects(&source, &target, true).is_err());
    assert!(
        e.document
            .find_layer(&target)
            .unwrap()
            .metadata
            .get("effects")
            .is_none()
    );
    assert_eq!(e.undo_depth(), depth);
    e.document.find_layer_mut(&source).unwrap().metadata["maskPlacement"] =
        serde_json::json!({"origin":[0.,0.],"size":[-1.,2.]});
    assert!(e.copy_layer_mask(&source, &target, true).is_err());
    assert!(e.document.find_layer(&target).unwrap().mask.is_none());
    assert_eq!(e.undo_depth(), depth);
}

#[test]
fn copying_folder_into_its_own_original_subtree_is_rejected_like_mac() {
    let mut e = editor();
    let folder = e.add_group("Folder");
    let child = e.add_group("Child");
    assert!(e.reparent_layers(&[child.clone()], Some(&folder), 0));
    let depth = e.undo_depth();
    assert!(
        e.duplicate_layers_to(&[folder.clone()], Some(&folder), 0)
            .is_empty()
    );
    assert!(
        e.duplicate_layers_to(&[folder.clone()], Some(&child), 0)
            .is_empty()
    );
    assert!(e.drop_layers(&[folder], Some(&child), 0, true).is_empty());
    assert_eq!(e.undo_depth(), depth);
}
#[test]
fn copied_group_mask_preserves_rendered_extent_rotation_flip_and_sampling() {
    let mut e = Editor::new(Document::new(40, 30));
    let paint = e.active_layer.clone();
    e.fill_selection([255, 0, 0, 255]);
    let folder = e.add_group("Folder");
    e.reparent_layers(&[paint], Some(&folder), 0);
    let target = e.add_layer("Target");
    e.fill_selection([255, 0, 0, 255]);
    e.set_visibility(&target, false);
    let group = e.document.find_layer_mut(&folder).unwrap();
    group.offset_x = 5.;
    group.offset_y = 8.;
    group.rotation = 90.;
    group.scale_x = -1.5;
    group.scale_y = 0.5;
    group.metadata["transform"] = serde_json::json!({"size":[12.,8.],"sampling":"Nearest"});
    // A folder ignores the image-only independent placement entirely.
    group.metadata["maskPlacement"] =
        serde_json::json!({"origin":[100.,100.],"size":[1.,1.],"sampling":"Smooth"});
    group.mask = Some(
        image::RgbaImage::from_fn(4, 4, |x, y| {
            if x > 0 && x < 3 && y > 0 && y < 3 {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        })
        .into(),
    );
    let before = raster::composite(&e.document);
    assert!(before.pixels().any(|p| p[3] > 0));
    assert!(e.copy_layer_mask(&folder, &target, true).unwrap());
    let placement = e.mask_placement(&target).unwrap();
    assert_eq!(
        (placement.x, placement.y, placement.width, placement.height),
        (5., 8., 18., 4.)
    );
    assert_eq!(placement.rotation, 90.);
    assert!(placement.flip_x);
    assert_eq!(
        e.document.find_layer(&target).unwrap().metadata["maskPlacement"]["sampling"],
        "Nearest"
    );
    e.set_visibility(&folder, false);
    e.set_visibility(&target, true);
    assert_eq!(raster::composite(&e.document), before);
}
#[test]
fn null_mask_placement_is_materialized_and_preserves_implicit_sampling() {
    let mut e = Editor::new(Document::new(20, 12));
    let source = e.active_layer.clone();
    let layer = e.document.find_layer_mut(&source).unwrap();
    layer.image = Some(image::RgbaImage::from_pixel(8, 6, Rgba([255, 0, 0, 255])).into());
    layer.offset_x = 3.;
    layer.offset_y = 2.;
    layer.mask = Some(
        image::RgbaImage::from_fn(8, 6, |x, y| {
            if x > 1 && x < 6 && y > 1 && y < 4 {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        })
        .into(),
    );
    layer.metadata["maskPlacement"] = serde_json::Value::Null;
    layer.metadata["transform"] = serde_json::json!({"sampling":"Nearest"});
    let target = e.add_layer("Target");
    e.fill_selection([255, 0, 0, 255]);
    e.set_visibility(&target, false);
    e.document.find_layer_mut(&target).unwrap().metadata["transform"] =
        serde_json::json!({"sampling":"Smooth"});
    let before = raster::composite(&e.document);
    assert!(e.copy_layer_mask(&source, &target, true).unwrap());
    let place = e.mask_placement(&target).unwrap();
    assert_eq!(
        (place.x, place.y, place.width, place.height),
        (3., 2., 8., 6.)
    );
    let metadata = &e.document.find_layer(&target).unwrap().metadata;
    assert!(metadata["maskPlacement"].is_object());
    assert_eq!(metadata["maskPlacement"]["sampling"], "Nearest");
    e.set_visibility(&source, false);
    e.set_visibility(&target, true);
    assert_eq!(raster::composite(&e.document), before);
}

fn clip(e: &mut Editor, target: &str, source: &str) {
    e.document.find_layer_mut(target).unwrap().metadata["maskSourceID"] = serde_json::json!(source);
}
fn source(e: &Editor, id: &str) -> Option<String> {
    e.document
        .find_layer(id)
        .unwrap()
        .metadata
        .get("maskSourceID")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}
#[test]
fn dropped_layer_between_base_and_clipped_layer_adopts_stack_in_one_undo() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    clip(&mut e, &b, &a);
    let depth = e.undo_depth();
    assert!(e.reparent_layers(&[c.clone()], None, 1));
    assert_eq!(source(&e, &c), Some(a.clone()));
    assert_eq!(source(&e, &b), Some(a.clone()));
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.undo());
    assert_eq!(
        ids(&e.document.layers),
        vec![a.clone(), b.clone(), c.clone()]
    );
    assert_eq!(source(&e, &c), None);
    assert_eq!(source(&e, &b), Some(a));
}
#[test]
fn inserted_multilayer_block_and_drag_copies_join_clipping_stack() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    let d = e.add_layer("D");
    clip(&mut e, &b, &a);
    assert!(e.reparent_layers(&[d.clone(), c.clone()], None, 1));
    assert_eq!(source(&e, &c), Some(a.clone()));
    assert_eq!(source(&e, &d), Some(a.clone()));
    assert_eq!(source(&e, &b), Some(a.clone()));
    e.undo();
    let copied = e.drop_layers(&[c, d], Some(&b), -1, true);
    assert_eq!(copied.len(), 2);
    assert!(copied.iter().all(|id| source(&e, id) == Some(a.clone())));
    assert_eq!(source(&e, &b), Some(a));
}
#[test]
fn moving_clip_away_releases_it_and_moving_base_with_clip_retains_link() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let gap = e.add_layer("Gap");
    clip(&mut e, &b, &a);
    assert!(e.reparent_layers(&[b.clone()], None, 2));
    assert_eq!(source(&e, &b), None);
    e.undo();
    assert!(e.reparent_layers(&[a.clone(), b.clone()], None, 1));
    assert_eq!(ids(&e.document.layers), vec![gap, a.clone(), b.clone()]);
    assert_eq!(source(&e, &b), Some(a));
}
#[test]
fn unrelated_noncontiguous_live_mask_links_survive_reordering() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let gap = e.add_layer("Gap");
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    let d = e.add_layer("D");
    clip(&mut e, &b, &a);
    assert!(e.reparent_layers(&[d.clone()], None, 3));
    assert_eq!(source(&e, &b), Some(a.clone()));
    assert_eq!(ids(&e.document.layers), vec![a, gap, b, d, c]);
}
#[test]
fn moving_base_cannot_silently_modify_a_locked_clipped_dependent() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    clip(&mut e, &b, &a);
    e.set_locked(&b, true);
    let depth = e.undo_depth();
    assert!(!e.reparent_layers(&[a.clone()], None, 2));
    assert_eq!(ids(&e.document.layers), vec![a.clone(), b.clone(), c]);
    assert_eq!(source(&e, &b), Some(a));
    assert_eq!(e.undo_depth(), depth);
}
#[test]
fn group_and_plain_duplicate_keep_explicit_live_links_without_drag_cleanup() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    clip(&mut e, &b, &a);
    let group = e.group_layers(&[b.clone()], "Group").unwrap();
    assert_eq!(source(&e, &b), Some(a.clone()));
    let copies = e.duplicate_layers(&[group]);
    assert_eq!(copies.len(), 1);
    let child = e.document.find_layer(&copies[0]).unwrap().children[0]
        .id
        .clone();
    assert_eq!(source(&e, &child), Some(a));
}
#[test]
fn drag_copy_of_previously_contiguous_child_detaches_outside_its_stack() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    e.add_layer("Gap");
    clip(&mut e, &b, &a);
    let copies = e.duplicate_layers_to(&[b.clone()], None, 3);
    assert_eq!(copies.len(), 1);
    assert_eq!(source(&e, &copies[0]), None);
    assert_eq!(source(&e, &b), Some(a));
}

#[test]
fn clipping_adoption_cannot_create_a_cycle_through_an_arbitrary_live_link() {
    let mut e = editor();
    let a = e.active_layer.clone();
    let b = e.add_layer("B");
    let c = e.add_layer("C");
    clip(&mut e, &a, &c);
    clip(&mut e, &b, &a);
    assert!(raster::validate(&e.document).is_empty());
    let depth = e.undo_depth();
    assert!(!e.reparent_layers(&[c.clone()], None, 1));
    assert_eq!(ids(&e.document.layers), vec![a.clone(), b, c.clone()]);
    assert_eq!(source(&e, &c), None);
    assert_eq!(source(&e, &a), Some(c));
    assert_eq!(e.undo_depth(), depth);
}

#[test]
fn copying_mask_to_adjustment_preserves_document_coverage_and_undo() {
    let mut e = Editor::new(Document::new(20, 16));
    e.fill_selection([200, 40, 20, 255]);
    let source = e.add_layer("Mask source");
    let layer = e.document.find_layer_mut(&source).unwrap();
    layer.image = Some(image::RgbaImage::from_pixel(5, 4, Rgba([255; 4])).into());
    layer.mask = Some(
        image::RgbaImage::from_fn(5, 4, |x, y| {
            if (x == 1 || x == 2) && y == 1 {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        })
        .into(),
    );
    layer.offset_x = 5.;
    layer.offset_y = 6.;
    layer.rotation = 90.;
    layer.scale_x = -2.;
    layer.scale_y = 1.;
    layer.metadata["transform"] = serde_json::json!({"sampling":"Nearest"});
    // Derive intended coverage independently from the source's rendered alpha.
    let mut coverage_document = Document::new(20, 16);
    coverage_document.layers = vec![layer.clone()];
    let coverage = raster::composite(&coverage_document);
    assert!(coverage.pixels().any(|p| p[3] == 255));
    layer.visible = false;
    let adjustment = e
        .add_adjustment(serde_json::json!({"kind":"Invert"}))
        .unwrap();
    let layer = e.document.find_layer_mut(&adjustment).unwrap();
    layer.offset_x = 12.;
    layer.offset_y = 10.;
    layer.scale_x = 3.;
    layer.rotation = 17.;
    layer.metadata["sentinel"] = serde_json::json!({"preserve":true});
    layer.metadata["transform"] = serde_json::json!({"sampling":"Smooth","custom":"retain"});
    let depth = e.undo_depth();
    assert!(e.copy_layer_mask(&source, &adjustment, true).unwrap());
    let actual = raster::composite(&e.document);
    for (pixel, mask) in actual.pixels().zip(coverage.pixels()) {
        assert_eq!(
            *pixel,
            if mask[3] == 255 {
                Rgba([55, 215, 235, 255])
            } else {
                Rgba([200, 40, 20, 255])
            }
        );
    }
    let layer = e.document.find_layer(&adjustment).unwrap();
    assert_eq!(
        (layer.offset_x, layer.offset_y, layer.rotation),
        (5., 6., 90.)
    );
    assert_eq!((layer.scale_x, layer.scale_y), (-1., 1.));
    assert_eq!(
        layer.metadata["transform"]["size"],
        serde_json::json!([10., 4.])
    );
    assert_eq!(layer.metadata["transform"]["custom"], "retain");
    assert_eq!(layer.metadata["sentinel"]["preserve"], true);
    assert_eq!(layer.metadata["adjustment"]["kind"], "Invert");
    assert!(e.document.find_layer(&source).unwrap().mask.is_some());
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(!e.copy_layer_mask(&source, &adjustment, true).unwrap());
    assert_eq!(e.undo_depth(), depth + 1);
    assert!(e.undo());
    let layer = e.document.find_layer(&adjustment).unwrap();
    assert!(layer.mask.is_none());
    assert_eq!(
        (
            layer.offset_x,
            layer.offset_y,
            layer.scale_x,
            layer.rotation
        ),
        (12., 10., 3., 17.)
    );
    assert_eq!(layer.metadata["transform"]["sampling"], "Smooth");
}
