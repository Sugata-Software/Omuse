use super::*;
use omuse::vector_scene::{VectorObject, VectorScene};

fn artwork() -> VectorScene {
    VectorScene {
        version: 1,
        width: 40,
        height: 30,
        objects: vec![
            VectorObject::rectangle("Red", 2., 3., 8., 6., Some([220, 50, 30, 255]), None).unwrap(),
            VectorObject::rectangle("Blue", 18., 5., 8., 6., Some([30, 70, 230, 255]), None)
                .unwrap(),
            VectorObject::rectangle(
                "Red small",
                30.,
                20.,
                6.,
                6.,
                Some([220, 50, 30, 255]),
                None,
            )
            .unwrap(),
        ],
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    let (view, cx) = setup(cx);
    view.update(cx, |view, _| view.zoom = 8.);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(artwork(), [0].into_iter().collect(), window, cx)
                .unwrap();
            view.command("tool-move", window, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    (view, cx)
}

fn snapshot(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> VectorScene {
    view.update(cx, |view, _| {
        view.vector_draft
            .as_ref()
            .unwrap()
            .scene_snapshot()
            .unwrap()
    })
}

fn chosen(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<usize> {
    view.update(cx, |view, _| {
        view.vector_draft
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .selected_objects
            .iter()
            .copied()
            .collect()
    })
}

#[gpui_kit::test]
fn shift_selection_drag_and_keyboard_undo_move_all_objects_once(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let red = canvas_point(&view, cx, 5., 6.);
    let blue = canvas_point(&view, cx, 21., 8.);
    cx.simulate_click(red, Modifiers::default());
    cx.simulate_click(
        blue,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(chosen(&view, cx), vec![0, 1]);
    let before = snapshot(&view, cx);
    let to = canvas_point(&view, cx, 23., 10.);
    drag(cx, blue, to, MouseButton::Left);
    let moved = snapshot(&view, cx);
    for i in 0..2 {
        let a = before.objects[i].path.subpaths[0].anchors[0].position;
        let b = moved.objects[i].path.subpaths[0].anchors[0].position;
        assert!((b.x - a.x - 2.).abs() < 0.001);
        assert!((b.y - a.y - 2.).abs() < 0.001);
    }
    assert_eq!(moved.objects[2], before.objects[2]);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), before);
    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(snapshot(&view, cx), moved);
    cx.simulate_keystrokes("right");
    let nudged = snapshot(&view, cx);
    assert_eq!(
        nudged.objects[0].path.subpaths[0].anchors[0].position.x,
        moved.objects[0].path.subpaths[0].anchors[0].position.x + 1.
    );
    assert_eq!(
        nudged.objects[1].path.subpaths[0].anchors[0].position.x,
        moved.objects[1].path.subpaths[0].anchors[0].position.x + 1.
    );
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.document.layers.len(), 1);
    });
}

#[gpui_kit::test]
fn marquee_group_pick_individual_ungroup_and_save_round_trip(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let from = canvas_point(&view, cx, 0., 1.);
    let to = canvas_point(&view, cx, 28., 13.);
    drag(cx, from, to, MouseButton::Left);
    assert_eq!(chosen(&view, cx), vec![0, 1]);
    cx.simulate_keystrokes("ctrl-g");
    let grouped = snapshot(&view, cx);
    assert_eq!(grouped.version, 2);
    assert_eq!(grouped.objects[0].groups, grouped.objects[1].groups);
    assert_eq!(grouped.objects[0].groups.len(), 1);
    let red = canvas_point(&view, cx, 5., 6.);
    cx.simulate_keystrokes("ctrl-d");
    assert!(chosen(&view, cx).is_empty());
    cx.simulate_click(red, Modifiers::default());
    assert_eq!(chosen(&view, cx), vec![0, 1]);
    cx.simulate_click(
        red,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(chosen(&view, cx), vec![0]);
    cx.simulate_keystrokes("ctrl-shift-g");
    assert!(
        snapshot(&view, cx)
            .objects
            .iter()
            .all(|o| o.groups.is_empty())
    );
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), grouped);
    cx.run_until_parked();
    click_id(cx, "scene-done");
    cx.run_until_parked();
    let doc = view.update(cx, |view, _| {
        assert!(!view.vector_scene_active(), "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 1);
        view.editor.document.clone()
    });
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("grouped.omuse");
    omuse::document::save(&doc, &file).unwrap();
    let reopened = omuse::document::open(&file).unwrap();
    assert_eq!(reopened.layers[1].vector_scene, doc.layers[1].vector_scene);
}

#[gpui_kit::test]
fn matching_style_and_transform_apply_only_to_selected_objects(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = snapshot(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.scene_select_matching("fill", window, cx);
            assert_eq!(
                view.vector_draft
                    .as_ref()
                    .unwrap()
                    .scene
                    .as_ref()
                    .unwrap()
                    .selected_objects
                    .len(),
                2
            );
            view.detail_inputs[0].update(cx, |input, cx| input.set_value("#00AA88", window, cx));
            view.update_vector_style(cx).unwrap();
            view.detail_inputs[4].update(cx, |input, cx| input.set_value("2", window, cx));
            view.detail_inputs[5].update(cx, |input, cx| input.set_value("-1", window, cx));
            view.scene_arrange_action("transform", window, cx);
        })
    });
    let result = snapshot(&view, cx);
    assert_eq!(result.objects[0].fill, Some([0, 170, 136, 255]));
    assert_eq!(result.objects[2].fill, Some([0, 170, 136, 255]));
    assert_eq!(result.objects[1], before.objects[1]);
    assert_eq!(
        result.objects[0].path.subpaths[0].anchors[0].position,
        VectorPoint { x: 4., y: 2. }
    );
    assert_eq!(
        result.objects[2].path.subpaths[0].anchors[0].position,
        VectorPoint { x: 32., y: 19. }
    );
}

#[gpui_kit::test]
fn boolean_command_is_async_reversible_and_keeps_unselected_artwork(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([0, 1].into_iter().collect(), Some(0), window, cx);
            view.scene_arrange_action("align-left", window, cx);
            view.scene_arrange_action("align-top", window, cx);
        })
    });
    let before = snapshot(&view, cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("vector-union", window, cx)));
    cx.run_until_parked();
    let result = snapshot(&view, cx);
    assert_eq!(result.objects.len(), 2);
    assert_eq!(result.objects[1], before.objects[2]);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), before);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command("vector-union", window, cx);
            view.cancel_vector_canvas(window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn scene_svg_import_appends_objects_and_undo_restores_previous_artwork(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = snapshot(&view, cx);
    let imported = omuse::vector_svg_scene::decode_scene(br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><g id="Mark"><rect x="1" y="1" width="4" height="4" fill="#FF0000"/><circle cx="15" cy="5" r="3" fill="#00FF00"/></g></svg>"##).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.import_vector_scene_artwork(imported, window, cx)
                .unwrap()
        })
    });
    let after = snapshot(&view, cx);
    assert_eq!(after.objects.len(), 5);
    assert_eq!(&after.objects[..3], &before.objects[..]);
    assert_eq!(chosen(&view, cx), vec![3, 4]);
    let encoded = omuse::vector_svg_scene::encode_scene(&after).unwrap();
    assert_eq!(
        omuse::vector_svg_scene::decode_scene(encoded.as_bytes())
            .unwrap()
            .objects
            .len(),
        5
    );
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), before);
}

#[gpui_kit::test]
fn outline_view_does_not_change_artwork_or_history(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = snapshot(&view, cx);
    cx.simulate_keystrokes("ctrl-y");
    view.update(cx, |view, _| assert!(view.vector_outline_active()));
    assert_eq!(snapshot(&view, cx), before);
    cx.simulate_keystrokes("ctrl-y");
    view.update(cx, |view, _| assert!(!view.vector_outline_active()));
}

#[gpui_kit::test]
fn stale_async_operations_release_busy_and_leave_newer_document_intact(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([0, 1].into_iter().collect(), Some(0), window, cx);
            view.command("vector-union", window, cx);
            let id = view.editor.active_layer.clone();
            assert!(view.editor.rename_layer(&id, "Newer document"));
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].name, "Newer document");
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(artwork(), [0].into_iter().collect(), window, cx)
                .unwrap();
            view.apply_vector_scene(cx);
            let id = view.editor.active_layer.clone();
            assert!(view.editor.rename_layer(&id, "Changed during apply"));
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert_eq!(view.editor.undo_depth(), 2);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].name, "Changed during apply");
    });
}

#[gpui_kit::test]
fn copying_a_member_and_adding_shapes_preserve_existing_groups(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([0, 1].into_iter().collect(), Some(0), window, cx);
            view.scene_arrange_action("group", window, cx);
            view.set_scene_selection([0].into_iter().collect(), Some(0), window, cx);
        })
    });
    let before = snapshot(&view, cx);
    cx.simulate_keystrokes("ctrl-j");
    let copied = snapshot(&view, cx);
    assert_eq!(copied.objects.len(), 4);
    assert_eq!(&copied.objects[..2], &before.objects[..2]);
    assert_ne!(copied.objects[2].groups, before.objects[0].groups);
    copied.validate().unwrap();
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), before);
    cx.update(|window, cx| view.update(cx, |view, cx| view.scene_action("rectangle", window, cx)));
    let added = snapshot(&view, cx);
    added.validate().unwrap();
    assert_eq!(&added.objects[..2], &before.objects[..2]);
    assert_eq!(added.objects.len(), 4);
}

#[gpui_kit::test]
fn zoom_preview_changes_resolution_without_editing_geometry(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = snapshot(&view, cx);
    view.update(cx, |view, cx| {
        view.zoom = 1.;
        view.validate_vector_canvas(cx);
    });
    cx.run_until_parked();
    let first = view.update(cx, |view, _| {
        view.vector_canvas_display().unwrap().dimensions()
    });
    view.update(cx, |view, cx| {
        view.zoom = 2.;
        view.validate_vector_canvas(cx);
        view.zoom = 3.;
        view.validate_vector_canvas(cx);
    });
    cx.run_until_parked();
    let high = view.update(cx, |view, _| {
        view.vector_canvas_display().unwrap().dimensions()
    });
    assert_eq!(high, (first.0 * 3, first.1 * 3));
    assert_eq!(snapshot(&view, cx), before);
    view.update(cx, |view, _| assert_eq!(view.editor.undo_depth(), 0));
}

#[gpui_kit::test]
fn compact_inspector_exposes_style_and_selection_controls_in_context(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for id in [
        "vector-style-0",
        "vector-group",
        "vector-transform-apply",
        "vector-align-left",
        "vector-union",
        "vector-same-fill",
        "vector-svg-export",
    ] {
        scroll_to_id(cx, id, "inspector-content");
    }
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("vector-path", window, cx)));
    draw(cx);
    assert!(cx.debug_bounds("vector-transform-apply").is_none());
    scroll_to_id(cx, "vector-style-0", "inspector-content");
    scroll_to_id(cx, "vector-new-subpath", "inspector-content");
    cx.update(|window, cx| view.update(cx, |view, cx| view.focus.focus(window, cx)));
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    draw(cx);
    let field = cx.debug_bounds("vector-transform-0").unwrap();
    let viewport = cx.debug_bounds("inspector-content").unwrap();
    assert!(
        field.origin.y >= viewport.origin.y && field.bottom_right().y <= viewport.bottom_right().y,
        "Ctrl+T must reveal its field: {field:?} inside {viewport:?}"
    );
}

#[gpui_kit::test]
fn adding_a_shape_restores_canvas_shortcuts(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click_inspector_id(cx, "scene-ellipse");
    view.update_in(cx, |view, window, _| assert!(view.focus.is_focused(window)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn matching_and_outline_buttons_keep_canvas_keyboard_control(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click_inspector_id(cx, "vector-same-fill");
    assert_eq!(chosen(&view, cx), vec![0, 2]);
    view.update_in(cx, |view, window, _| assert!(view.focus.is_focused(window)));
    let before = snapshot(&view, cx);
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    let moved = snapshot(&view, cx);
    for index in [0, 2] {
        let x = |scene: &VectorScene| scene.objects[index].path.subpaths[0].anchors[0].position.x;
        assert_eq!(x(&moved), x(&before) + 1.);
    }
    assert_eq!(moved.objects[1], before.objects[1]);
    click_inspector_id(cx, "vector-outline");
    view.update_in(cx, |view, window, _| {
        assert!(view.vector_outline_active());
        assert!(view.focus.is_focused(window));
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn raise_can_move_a_lower_selection_when_the_active_object_is_already_on_top(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([0, 2].into_iter().collect(), Some(2), window, cx);
        })
    });
    let before = snapshot(&view, cx);
    click_inspector_id(cx, "scene-forward");
    let scene = snapshot(&view, cx);
    let names: Vec<_> = scene
        .objects
        .iter()
        .map(|object| object.name.as_str())
        .collect();
    assert_eq!(names, ["Blue", "Red", "Red small"]);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(snapshot(&view, cx), before);
}
