use image::{Rgba, RgbaImage};
use omuse::{
    auto_layout::{self, Align, Flow, FrameRule, FrameSize, ItemSize},
    document,
    editor::{Editor, LayerPlacement},
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint},
    raster,
};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn pixels(name: &str, w: u32, h: u32) -> Layer {
    let mut l = Layer::paint(name, w, h);
    l.image =
        Some(RgbaImage::from_fn(w, h, |x, y| Rgba([x as u8 * 7, y as u8 * 9, 72, 255])).into());
    l
}
fn pair() -> (Editor, Vec<String>) {
    let mut d = Document::new(400, 300);
    d.layers = vec![pixels("A", 10, 8), pixels("B", 20, 12)];
    let ids = d.layers.iter().map(|l| l.id.clone()).collect();
    (Editor::new(d), ids)
}
fn rule() -> FrameRule {
    FrameRule {
        padding: [2., 3., 4., 5.],
        gap: 7.,
        ..Default::default()
    }
}
fn describe(d: &Document) -> Value {
    fn layers(ls: &[Layer]) -> Vec<Value> {
        ls.iter().map(|l|json!({"id":l.id,"name":l.name,
        "placement":[l.offset_x,l.offset_y,l.scale_x,l.scale_y,l.rotation],"visible":l.visible,"locked":l.locked,
        "metadata":l.metadata,"image":l.image.as_ref().map(|i|omuse::asset_library::sha256_hex(i.as_raw())),
        "children":layers(&l.children)})).collect()
    }
    json!({"size":[d.width,d.height],"layers":layers(&d.layers),"metadata":d.metadata})
}
fn place(e: &Editor, id: &str) -> LayerPlacement {
    e.layer_placement(id).unwrap()
}

#[test]
fn flow_padding_gap_and_undo_are_retained_without_touching_pixels() {
    let (mut e, ids) = pair();
    let original = describe(&e.document);
    let allocations: Vec<_> = e
        .document
        .layers
        .iter()
        .map(|l| l.image.as_ref().unwrap().as_raw().as_ptr())
        .collect();
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    assert_eq!(e.undo_depth(), 1);
    assert_eq!(
        auto_layout::extent(e.document.find_layer(&frame).unwrap()),
        Some((45., 18.))
    );
    assert_eq!((place(&e, &ids[0]).x, place(&e, &ids[0]).y), (5., 2.));
    assert_eq!((place(&e, &ids[1]).x, place(&e, &ids[1]).y), (22., 2.));
    for (id, allocation) in ids.iter().zip(allocations) {
        assert_eq!(
            e.document
                .find_layer(id)
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .as_raw()
                .as_ptr(),
            allocation
        );
    }
    assert!(!auto_layout::reflow(&mut e.document, &idle()).unwrap());
    let saved = describe(&e.document);
    assert!(e.undo());
    assert_eq!(describe(&e.document), original);
    assert!(e.redo());
    assert_eq!(describe(&e.document), saved);
    let mut vertical = rule();
    vertical.flow = Flow::Vertical;
    assert!(e.set_layout_frame(&frame, vertical, &idle()).unwrap());
    assert_eq!((place(&e, &ids[1]).x, place(&e, &ids[1]).y), (5., 17.));
    assert_eq!(
        auto_layout::extent(e.document.find_layer(&frame).unwrap()),
        Some((28., 33.))
    );
}

#[test]
fn fill_fixed_hug_and_cycle_refusal_do_not_accumulate_scale() {
    let (mut e, ids) = pair();
    let mut spec = rule();
    spec.width = FrameSize::Fixed(100.);
    spec.height = FrameSize::Fixed(40.);
    spec.padding = [5.; 4];
    spec.gap = 10.;
    let frame = e.create_layout_frame(&ids, spec.clone(), &idle()).unwrap();
    e.set_layout_item(&ids[1], ItemSize::Fill, ItemSize::Fill, &idle())
        .unwrap();
    assert_eq!(
        (place(&e, &ids[1]).width, place(&e, &ids[1]).height),
        (70., 30.)
    );
    let snapshot = describe(&e.document);
    let history = e.undo_depth();
    let mut cyclic = spec.clone();
    cyclic.width = FrameSize::Hug;
    assert!(
        e.set_layout_frame(&frame, cyclic, &idle())
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    assert_eq!(describe(&e.document), snapshot);
    assert_eq!(e.undo_depth(), history);
    assert!(
        e.set_layout_item(&ids[0], ItemSize::Fixed(200.), ItemSize::Hug, &idle())
            .is_err()
    );
    assert_eq!(describe(&e.document), snapshot);
    e.set_layout_item(&ids[1], ItemSize::Hug, ItemSize::Hug, &idle())
        .unwrap();
    assert_eq!(
        (place(&e, &ids[1]).width, place(&e, &ids[1]).height),
        (20., 12.)
    );
    assert!(!e.set_layout_frame(&frame, spec, &idle()).unwrap());
    assert_eq!(e.undo_depth(), history + 1);
}

#[test]
fn wrapping_uses_fixed_main_size_and_responds_to_visibility_and_reordering() {
    let (mut e, ids) = pair();
    let mut spec = rule();
    spec.width = FrameSize::Fixed(23.);
    spec.padding = [0.; 4];
    spec.gap = 2.;
    spec.wrap = true;
    let frame = e.create_layout_frame(&ids, spec.clone(), &idle()).unwrap();
    assert_eq!((place(&e, &ids[1]).x, place(&e, &ids[1]).y), (0., 10.));
    assert_eq!(
        auto_layout::extent(e.document.find_layer(&frame).unwrap()),
        Some((23., 22.))
    );
    assert!(e.set_visibility(&ids[0], false));
    assert_eq!(place(&e, &ids[1]).y, 0.);
    assert_eq!(
        auto_layout::extent(e.document.find_layer(&frame).unwrap()),
        Some((23., 12.))
    );
    assert!(e.undo());
    assert!(e.reorder_layer(&ids[1], Some(&frame), 0));
    assert_eq!(place(&e, &ids[0]).y, 14.);
    let before = describe(&e.document);
    assert!(
        e.set_layout_item(&ids[0], ItemSize::Fill, ItemSize::Hug, &idle())
            .is_err()
    );
    assert_eq!(describe(&e.document), before);
    assert!(e.delete_layers(&[ids[1].clone()]));
    assert_eq!(place(&e, &ids[0]).y, 0.);
}

#[test]
fn mixed_live_text_photo_vector_sources_reflow_after_text_edit_in_one_undo() {
    let (mut e, _) = pair();
    let text = objects::live_text_layer(
        "Title",
        ObjectPoint { x: 0., y: 0. },
        LiveTextStyle {
            content: "Hi".into(),
            font_size: 14.,
            ..Default::default()
        },
    )
    .unwrap();
    let text_id = text.id.clone();
    let scene = omuse::vector_scene::VectorScene {
        version: 1,
        width: 20,
        height: 20,
        objects: vec![
            omuse::vector_scene::VectorObject::ellipse(
                "Dot",
                1.,
                1.,
                18.,
                18.,
                Some([211, 91, 53, 255]),
                None,
            )
            .unwrap(),
        ],
    };
    let mut vector = pixels("Vector", 20, 20);
    vector.image = Some(scene.render(&idle()).unwrap().into());
    vector.vector_scene = Some(Arc::new(scene.clone()));
    let vector_id = vector.id.clone();
    e.document.layers = vec![e.document.layers[0].clone(), text, vector];
    let ids = e
        .document
        .layers
        .iter()
        .map(|l| l.id.clone())
        .collect::<Vec<_>>();
    let photo = e.document.layers[0].image.clone();
    e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let before = describe(&e.document);
    let history = e.undo_depth();
    let x = place(&e, &vector_id).x;
    e.set_live_text(
        &text_id,
        LiveTextStyle {
            content: "A longer live headline".into(),
            font_size: 14.,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(place(&e, &vector_id).x > x);
    assert_eq!(e.undo_depth(), history + 1);
    assert_eq!(e.document.find_layer(&ids[0]).unwrap().image, photo);
    assert_eq!(
        e.document
            .find_layer(&vector_id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(&scene)
    );
    assert!(e.undo());
    assert_eq!(describe(&e.document), before);
    assert!(e.redo());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Mixed.omuse");
    document::save(&e.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(raster::composite(&reopened), raster::composite(&e.document));
    assert_eq!(
        auto_layout::frame_rule(&reopened.layers[0]).unwrap(),
        auto_layout::frame_rule(&e.document.layers[0]).unwrap()
    );
    let mut opened = Editor::new(reopened);
    opened
        .set_live_text(
            &text_id,
            LiveTextStyle {
                content: "Short".into(),
                font_size: 14.,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(place(&opened, &vector_id).x < place(&e, &vector_id).x);
}

#[test]
fn locked_neighbor_refuses_text_and_replacement_transactions_without_history() {
    let (mut e, _) = pair();
    e.document.layers[0] = objects::live_text_layer(
        "Title",
        ObjectPoint { x: 0., y: 0. },
        LiveTextStyle {
            content: "Hi".into(),
            font_size: 14.,
            ..Default::default()
        },
    )
    .unwrap();
    let ids = e
        .document
        .layers
        .iter()
        .map(|l| l.id.clone())
        .collect::<Vec<_>>();
    e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    assert!(e.set_locked(&ids[1], true));
    let before = describe(&e.document);
    let revision = e.revision();
    let history = e.undo_depth();
    assert!(
        e.set_live_text(
            &ids[0],
            LiveTextStyle {
                content: "Longer changes the neighbor".into(),
                font_size: 14.,
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("locked")
    );
    assert_eq!(describe(&e.document), before);
    assert_eq!(e.revision(), revision);
    assert_eq!(e.undo_depth(), history);
    let mut changed = e.document.clone();
    changed.find_layer_mut(&ids[0]).unwrap().image = Some(RgbaImage::new(300, 30).into());
    assert!(e.replace_document_transaction(changed).is_err());
    assert_eq!(describe(&e.document), before);
}

#[test]
fn cancel_invalid_inputs_nested_and_budget_refusals_are_atomic() {
    let (mut e, ids) = pair();
    let before = describe(&e.document);
    assert!(
        e.create_layout_frame(&ids, rule(), &AtomicBool::new(true))
            .is_err()
    );
    for spec in [
        FrameRule {
            gap: f32::NAN,
            ..rule()
        },
        FrameRule {
            version: 99,
            ..rule()
        },
        FrameRule {
            wrap: true,
            ..rule()
        },
    ] {
        assert!(e.create_layout_frame(&ids, spec, &idle()).is_err());
    }
    assert_eq!(describe(&e.document), before);
    assert_eq!(e.undo_depth(), 0);
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let framed = describe(&e.document);
    assert!(e.create_layout_frame(&ids, rule(), &idle()).is_err());
    let extra = Layer::group("Nested");
    assert!(e.insert_layer(extra.clone()).len() > 0);
    assert!(!e.reparent_layers(&[e.active_layer.clone()], Some(&frame), 0));
    assert!(e.undo());
    assert_eq!(describe(&e.document), framed);
    let mut candidate = e.document.clone();
    let group = candidate.find_layer_mut(&frame).unwrap();
    group.children = (0..257)
        .map(|i| pixels(&format!("Member{i}"), 1, 1))
        .collect();
    assert!(
        auto_layout::plan(&candidate, &idle())
            .unwrap_err()
            .to_string()
            .contains("256")
    );
    let mut duplicate = e.document.clone();
    duplicate.layers[0].children[1].id = duplicate.layers[0].children[0].id.clone();
    assert!(auto_layout::plan(&duplicate, &idle()).is_err());
}

#[test]
fn native_format_is_conditional_and_stale_or_unknown_rules_refuse_open() {
    let dir = tempfile::tempdir().unwrap();
    let (mut e, ids) = pair();
    let old = dir.path().join("Old.omuse");
    document::save(&e.document, &old).unwrap();
    let manifest = |p: &std::path::Path| -> Value {
        serde_json::from_slice(&std::fs::read(p.join("manifest.json")).unwrap()).unwrap()
    };
    assert_eq!(manifest(&old)["version"], 10);
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let file = dir.path().join("Frame.omuse");
    document::save(&e.document, &file).unwrap();
    let saved = manifest(&file);
    assert_eq!(saved["version"], 15);
    assert!(
        saved.get("rustVectorSceneCount").is_none(),
        "A photo-only frame must not invent vector inventory"
    );
    document::open(&file).unwrap();
    for mode in 0..4 {
        let mut bad = saved.clone();
        match mode {
            0 => bad["version"] = json!(14),
            1 => bad["layers"][1]["transform"]["origin"][0] = json!(987.),
            2 => bad["layers"][0]["omuseLayoutExtent"][0] = json!(987.),
            _ => bad["layers"][0][auto_layout::FRAME_KEY]["version"] = json!(999),
        }
        std::fs::write(
            file.join("manifest.json"),
            serde_json::to_vec(&bad).unwrap(),
        )
        .unwrap();
        assert!(
            document::open(&file).is_err(),
            "tamper mode{mode} was accepted"
        );
    }
    std::fs::write(
        file.join("manifest.json"),
        serde_json::to_vec(&saved).unwrap(),
    )
    .unwrap();
    let rendered = raster::composite(&e.document);
    e.remove_layout_frame(&frame, &idle()).unwrap();
    assert_eq!(raster::composite(&e.document), rendered);
    document::save(&e.document, &file).unwrap();
    assert_eq!(manifest(&file)["version"], 10);
    let mut stale = document::open(&old).unwrap();
    auto_layout::set_frame_rule(&mut stale.layers[0], &rule()).unwrap();
    assert!(document::save(&stale, &old).is_err());
    assert_eq!(manifest(&old)["version"], 10);
}

#[test]
fn flips_are_real_edits_and_fixed_resizing_changes_retained_member_rule() {
    let (mut e, ids) = pair();
    e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    e.select_layer(&ids[0]);
    let history = e.undo_depth();
    assert!(e.flip_horizontal());
    assert!(place(&e, &ids[0]).flip_x);
    assert_eq!(e.undo_depth(), history + 1);
    assert!(e.flip_vertical());
    assert!(place(&e, &ids[0]).flip_y);
    assert!(e.undo());
    assert!(!place(&e, &ids[0]).flip_y);
    assert!(place(&e, &ids[0]).flip_x);
    let mut placement = place(&e, &ids[0]);
    placement.width = 30.;
    assert!(e.set_layer_placement(&ids[0], placement));
    assert_eq!(
        auto_layout::item_rule(e.document.find_layer(&ids[0]).unwrap())
            .unwrap()
            .unwrap()
            .width,
        ItemSize::Fixed(30.)
    );
    assert_eq!(place(&e, &ids[1]).x, 42.);
    let state = describe(&e.document);
    let history = e.undo_depth();
    assert!(!e.move_layers(&[ids[0].clone()], 5., 0.));
    assert_eq!(describe(&e.document), state);
    assert_eq!(e.undo_depth(), history);
    let from = place(&e, &ids[0]);
    let mut to = from;
    to.width += 20.;
    assert!(!e.transform_layers(&ids, from, to));
    assert_eq!(describe(&e.document), state);
    assert_eq!(e.undo_depth(), history);
}

#[test]
fn frame_move_carries_linked_explicit_masks_and_preserves_unlinked_masks() {
    for linked in [false, true] {
        let (mut e, ids) = pair();
        let layer = e.document.find_layer_mut(&ids[0]).unwrap();
        layer.mask = Some(RgbaImage::from_pixel(6, 6, Rgba([180, 180, 180, 255])).into());
        layer.metadata["maskLinked"] = json!(linked);
        layer.metadata["maskPlacement"] = json!({"origin":[3.,4.],"size":[6.,6.],"rotation":0.,"flipX":false,"flipY":false,"sampling":"High quality"});
        let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
        let mask = e.mask_placement(&ids[0]).unwrap();
        let before = describe(&e.document);
        assert!(e.move_group(&frame, 11., 13.));
        let moved = e.mask_placement(&ids[0]).unwrap();
        assert_eq!(
            (moved.x, moved.y),
            if linked {
                (mask.x + 11., mask.y + 13.)
            } else {
                (mask.x, mask.y)
            }
        );
        assert!(e.undo());
        assert_eq!(describe(&e.document), before);
    }
}

#[test]
fn create_resize_scales_rules_once_and_preserves_sources_with_atomic_refusal() {
    let (mut e, ids) = pair();
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let source = e.document.find_layer(&ids[0]).unwrap().image.clone();
    let enlarged = omuse::create::resize_layout(
        &e.document,
        800,
        600,
        omuse::create::ResizeStrategy::ScaleToFit,
    )
    .unwrap();
    auto_layout::validate_settled(&enlarged).unwrap();
    let spec = auto_layout::frame_rule(enlarged.find_layer(&frame).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(spec.gap, 14.);
    assert_eq!(spec.padding, [4., 6., 8., 10.]);
    assert_eq!(enlarged.find_layer(&ids[0]).unwrap().image, source);
    assert_eq!(enlarged.find_layer(&ids[0]).unwrap().scale_x, 2.);
    let prior = e.undo_depth();
    e.replace_document_transaction(enlarged).unwrap();
    assert_eq!(e.undo_depth(), prior + 1);
    e.set_locked(&ids[0], true);
    let snapshot = describe(&e.document);
    assert!(
        omuse::create::resize_layout_in_place(
            &mut e.document,
            1000,
            750,
            omuse::create::ResizeStrategy::Adapt
        )
        .is_err()
    );
    assert_eq!(describe(&e.document), snapshot);
}

#[test]
fn reparent_out_releases_item_and_duplicate_frame_retains_rules_with_new_ids() {
    let (mut e, ids) = pair();
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let copy = e.duplicate_layer(&frame).unwrap();
    let group = e.document.find_layer(&copy).unwrap();
    assert_eq!(auto_layout::frame_rule(group).unwrap(), Some(rule()));
    assert_ne!(group.children[0].id, ids[0]);
    let before = place(&e, &ids[0]);
    assert!(e.reparent_layers(&[ids[0].clone()], None, 0));
    assert!(
        auto_layout::item_rule(e.document.find_layer(&ids[0]).unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(place(&e, &ids[0]), before);
    assert_eq!(place(&e, &ids[1]).x, 5.);
    assert!(e.undo());
    assert_eq!(place(&e, &ids[1]).x, 22.);
}

#[test]
fn cross_alignment_and_noncontiguous_or_clipping_selection_refuse_cleanly() {
    let (mut e, ids) = pair();
    let mut spec = rule();
    spec.height = FrameSize::Fixed(50.);
    spec.align = Align::End;
    e.create_layout_frame(&ids, spec, &idle()).unwrap();
    assert_eq!(place(&e, &ids[0]).y, 38.);
    assert_eq!(place(&e, &ids[1]).y, 34.);
    let (mut other, ids) = pair();
    other.document.layers.insert(1, pixels("Between", 2, 2));
    let before = describe(&other.document);
    assert!(other.create_layout_frame(&ids, rule(), &idle()).is_err());
    assert_eq!(describe(&other.document), before);
    other.document.layers.remove(1);
    other.document.layers[1].metadata["maskSourceID"] = json!(ids[0]);
    assert!(other.create_layout_frame(&ids, rule(), &idle()).is_err());
    assert_eq!(other.undo_depth(), 0);
}

#[test]
fn canvas_bounds_translate_rules_and_masks_but_image_resampling_requires_release() {
    let (mut e, ids) = pair();
    let layer = e.document.find_layer_mut(&ids[0]).unwrap();
    layer.mask = Some(RgbaImage::from_pixel(6, 6, Rgba([180, 180, 180, 255])).into());
    layer.metadata["maskLinked"] = json!(false);
    layer.metadata["maskPlacement"] = json!({"origin":[3.,4.],"size":[6.,6.],"rotation":0.,"flipX":false,"flipY":false,"sampling":"High quality"});
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    let original = describe(&e.document);
    let history = e.undo_depth();
    assert!(!e.resize_image_with_options(800, 600, 72., "High quality"));
    assert!(e.take_layout_error().unwrap().contains("Release"));
    assert_eq!(e.undo_depth(), history);
    assert_eq!(describe(&e.document), original);
    // Changing print resolution alone does not resample or alter the layout.
    assert!(e.resize_image_with_options(400, 300, 144., "High quality"));
    assert!(e.undo());
    assert_eq!(describe(&e.document), original);
    let before = place(&e, &ids[0]);
    let mask = e.mask_placement(&ids[0]).unwrap();
    assert!(e.crop_canvas(11, 13, 200, 120));
    assert_eq!(
        (place(&e, &ids[0]).x, place(&e, &ids[0]).y),
        (before.x - 11., before.y - 13.)
    );
    assert_eq!(
        (
            e.mask_placement(&ids[0]).unwrap().x,
            e.mask_placement(&ids[0]).unwrap().y
        ),
        (mask.x - 11., mask.y - 13.)
    );
    assert_eq!(
        auto_layout::frame_rule(e.document.find_layer(&frame).unwrap()).unwrap(),
        Some(rule())
    );
    auto_layout::validate_settled(&e.document).unwrap();
    assert!(e.undo());
    assert_eq!(describe(&e.document), original);
    assert!(e.resize_canvas(500, 400));
    assert_eq!(place(&e, &ids[0]), before);
    auto_layout::validate_settled(&e.document).unwrap();
    assert!(e.undo());
    assert!(e.resize_canvas_anchored(500, 400, 4, None));
    assert_eq!(
        (place(&e, &ids[0]).x, place(&e, &ids[0]).y),
        (before.x + 50., before.y + 50.)
    );
    assert!(e.undo());
    assert!(e.set_locked(&ids[0], true));
    let locked = describe(&e.document);
    let depth = e.undo_depth();
    assert!(!e.crop_canvas(2, 3, 200, 120));
    assert_eq!(describe(&e.document), locked);
    assert_eq!(e.undo_depth(), depth);
    assert!(e.resize_canvas(500, 400)); // Bounds only, no managed placement changes.
    auto_layout::validate_settled(&e.document).unwrap();
}

#[test]
fn create_resize_refuses_effectively_locked_empty_and_hidden_frames() {
    use omuse::create::{
        HorizontalAnchor, LayerLayoutRule, LayoutScale, ResizeStrategy, VerticalAnchor,
    };
    for empty in [false, true] {
        let (mut e, ids) = pair();
        let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
        let layer = e.document.find_layer_mut(&frame).unwrap();
        layer.offset_x = 40.;
        layer.offset_y = 30.;
        if empty {
            layer.children.clear();
        } else {
            for child in &mut layer.children {
                child.visible = false;
            }
        }
        omuse::create::set_layout_rule(
            layer,
            LayerLayoutRule {
                horizontal: HorizontalAnchor::Center,
                vertical: VerticalAnchor::Center,
                scale: LayoutScale::Preserve,
            },
        )
        .unwrap();
        auto_layout::reflow(&mut e.document, &idle()).unwrap();
        let mut parent = Layer::group("Locked containing group");
        parent.locked = true;
        parent.children = std::mem::take(&mut e.document.layers);
        e.document.layers = vec![parent];
        auto_layout::validate_settled(&e.document).unwrap();
        let before = describe(&e.document);
        let error =
            omuse::create::resize_layout_in_place(&mut e.document, 800, 600, ResizeStrategy::Adapt)
                .unwrap_err();
        assert!(error.to_string().contains("ancestors"), "{error}");
        assert_eq!(
            describe(&e.document),
            before,
            "empty={empty}: refused resize changed locked frame"
        );
        omuse::create::resize_layout_in_place(&mut e.document, 400, 300, ResizeStrategy::Adapt)
            .unwrap();
        assert_eq!(
            describe(&e.document),
            before,
            "same-size operation should remain a no-op"
        );
        e.document.layers[0].locked = false;
        omuse::create::resize_layout_in_place(&mut e.document, 800, 600, ResizeStrategy::Adapt)
            .unwrap();
        let moved = e.document.find_layer(&frame).unwrap();
        assert_eq!((moved.offset_x, moved.offset_y), (240., 180.));
        auto_layout::validate_settled(&e.document).unwrap();
    }
}

#[test]
fn floating_member_pixels_defer_reflow_and_commit_once_or_cancel_exactly() {
    let (mut e, ids) = pair();
    e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    e.select_layer(&ids[0]);
    e.select_rectangle(6., 3., 2., 2.);
    let before = describe(&e.document);
    let history = e.undo_depth();
    let revision = e.revision();
    let floating = e.begin_floating_selection().unwrap().unwrap();
    let mut placement = place(&e, &floating);
    placement.x += 1.;
    assert!(e.set_layer_placement(&floating, placement));
    assert_eq!(
        e.undo_depth(),
        history,
        "Deferred movement must not add an Undo entry"
    );
    assert_eq!(
        e.revision(),
        revision,
        "Deferred movement must not publish a revision"
    );
    assert_eq!(
        place(&e, &floating),
        placement,
        "Temporary layer must not be placed by frame rules"
    );
    assert!(e.cancel_floating_selection());
    assert_eq!(describe(&e.document), before);
    let floating = e.begin_floating_selection().unwrap().unwrap();
    let mut placement = place(&e, &floating);
    placement.x += 1.;
    assert!(e.set_layer_placement(&floating, placement));
    assert!(e.commit_floating_selection().unwrap());
    assert_eq!(e.undo_depth(), history + 1);
    auto_layout::validate_settled(&e.document).unwrap();
    assert_ne!(describe(&e.document), before);
    assert!(e.undo());
    assert_eq!(describe(&e.document), before);
    assert_eq!(e.undo_depth(), history);
    // Growth that would displace a locked neighbour refuses the final commit
    // and restores the original source pixels and hierarchy atomically.
    assert!(e.set_locked(&ids[1], true));
    let protected = describe(&e.document);
    let history = e.undo_depth();
    let floating = e.begin_floating_selection().unwrap().unwrap();
    let mut placement = place(&e, &floating);
    placement.x += 30.;
    assert!(e.set_layer_placement(&floating, placement));
    assert!(e.commit_floating_selection().is_err());
    assert_eq!(describe(&e.document), protected);
    assert_eq!(e.undo_depth(), history);
    assert!(e.floating_selection_layer().is_none());
}

#[test]
fn frame_placement_uses_fractional_retained_extent_before_and_after_reopen() {
    let (mut e, ids) = pair();
    let mut spec = rule();
    spec.width = FrameSize::Fixed(60.5);
    spec.height = FrameSize::Fixed(26.25);
    let frame = e.create_layout_frame(&ids, spec, &idle()).unwrap();
    let bounds = place(&e, &frame);
    assert_eq!((bounds.width, bounds.height), (60.5, 26.25));
    assert_eq!((bounds.x, bounds.y), (0., 0.));
    assert!(e.move_group(&frame, 11.5, 13.25));
    let moved = place(&e, &frame);
    assert_eq!(
        (moved.x, moved.y, moved.width, moved.height),
        (11.5, 13.25, 60.5, 26.25)
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Fractional-frame.omuse");
    document::save(&e.document, &path).unwrap();
    let reopened = Editor::new(document::open(&path).unwrap());
    assert_eq!(place(&reopened, &frame), moved);
    let mut invalid = e.document.clone();
    invalid.find_layer_mut(&frame).unwrap().metadata["omuseLayoutExtent"] = Value::Null;
    assert!(Editor::new(invalid).layer_placement(&frame).is_none());
    e.remove_layout_frame(&frame, &idle()).unwrap();
    // Existing ordinary folder semantics remain unchanged by this frame-only fix.
    assert_eq!(
        (place(&e, &frame).width, place(&e, &frame).height),
        (400., 300.)
    );
}

#[test]
fn ungroup_nonzero_frame_releases_rules_preserves_composition_and_undo() {
    let (mut e, ids) = pair();
    let frame = e.create_layout_frame(&ids, rule(), &idle()).unwrap();
    assert!(e.move_group(&frame, 11.5, 13.25));
    let before = describe(&e.document);
    let image = raster::composite(&e.document);
    let placements = ids.iter().map(|id| place(&e, id)).collect::<Vec<_>>();
    let history = e.undo_depth();
    assert!(e.ungroup_layer_unavailable(&frame).is_none());
    assert_eq!(e.ungroup_layer(&frame).unwrap(), ids);
    assert_eq!(e.undo_depth(), history + 1);
    assert_eq!(raster::composite(&e.document), image);
    assert!(!auto_layout::has_layout(&e.document.layers));
    for (id, placement) in ids.iter().zip(placements) {
        assert_eq!(place(&e, id), placement);
    }
    assert!(e.undo());
    assert_eq!(describe(&e.document), before);
    auto_layout::validate_settled(&e.document).unwrap();
    for mode in 0..8 {
        let mut candidate = e.document.clone();
        let group = candidate.find_layer_mut(&frame).unwrap();
        match mode {
            0 => group.rotation = 10.,
            1 => group.scale_x = 2.,
            2 => group.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255; 4])).into()),
            3 => group.metadata["effects"] = json!({"shadow":{"enabled":true}}),
            4 => group.children[0].locked = true,
            5 => group.metadata["maskSourceID"] = json!(ids[0]),
            6 => group.metadata[auto_layout::FRAME_KEY]["version"] = json!(999),
            _ => group.children[0].offset_x += 0.5,
        }
        let state = describe(&candidate);
        let mut invalid = Editor::new(candidate);
        assert!(
            invalid.ungroup_layer(&frame).is_err(),
            "unsafe ungroup mode{mode}"
        );
        assert_eq!(
            describe(&invalid.document),
            state,
            "refusal mode{mode} mutated artwork"
        );
        assert_eq!(invalid.undo_depth(), 0);
    }
}

#[test]
fn floating_growth_keeps_selection_on_pixels_after_hug_fixed_and_fill_reflow() {
    for mode in [ItemSize::Hug, ItemSize::Fixed(8.), ItemSize::Fill] {
        for (dx, dy) in [(-1., 0.), (0., -1.), (-1., -1.), (2., 0.), (0., 2.)] {
            let mut doc = Document::new(50, 40);
            let mut source = Layer::paint("Selected red, retained blue", 4, 4);
            source.offset_x = 10.;
            source.offset_y = 10.;
            source
                .image
                .as_mut()
                .unwrap()
                .put_pixel(0, 0, Rgba([255, 0, 0, 255]));
            source
                .image
                .as_mut()
                .unwrap()
                .put_pixel(3, 3, Rgba([0, 0, 255, 255]));
            source.metadata["transform"] = json!({"sampling":"Nearest"});
            let id = source.id.clone();
            doc.layers = vec![source];
            let mut e = Editor::new(doc);
            e.create_layout_frame(
                std::slice::from_ref(&id),
                FrameRule {
                    width: FrameSize::Fixed(8.),
                    height: FrameSize::Fixed(8.),
                    padding: [0.; 4],
                    gap: 0.,
                    ..Default::default()
                },
                &idle(),
            )
            .unwrap();
            e.set_layout_item(&id, mode, mode, &idle()).unwrap();
            e.select_layer(&id);
            let pixel_scale = place(&e, &id).width / 4.;
            e.select_rectangle(10., 10., pixel_scale, pixel_scale);
            let before = describe(&e.document);
            let prior_selection = e.selection.clone();
            let history = e.undo_depth();
            let floating = e.begin_floating_selection().unwrap().unwrap();
            let mut p = place(&e, &floating);
            p.x += dx * pixel_scale * 2.;
            p.y += dy * pixel_scale * 2.;
            assert!(e.set_layer_placement(&floating, p));
            assert!(
                e.commit_floating_selection().unwrap(),
                "mode{mode:?} delta{dx},{dy}"
            );
            assert_eq!(e.undo_depth(), history + 1);
            auto_layout::validate_settled(&e.document).unwrap();
            let image = raster::composite(&e.document);
            let selection = e.selection.as_ref().unwrap();
            let mut selected_count = 0;
            for (x, y, pixel) in image.enumerate_pixels() {
                let red = pixel.0 == [255, 0, 0, 255];
                let coverage = selection.mask[(y * e.document.width + x) as usize];
                assert_eq!(
                    coverage,
                    if red { 255 } else { 0 },
                    "mode{mode:?} delta{dx},{dy}: selection diverged at{x},{y}, rendered {:?}",
                    pixel.0
                );
                selected_count += usize::from(coverage > 0);
            }
            assert!(
                selected_count > 0,
                "fixture must retain visible selected pixels"
            );
            let moved_selection = e.selection.clone();
            let moved_pixels = image;
            assert!(e.undo());
            assert_eq!(describe(&e.document), before);
            assert_eq!(e.selection, prior_selection);
            assert!(e.redo());
            assert_eq!(e.selection, moved_selection);
            assert_eq!(raster::composite(&e.document), moved_pixels);
        }
    }
}
