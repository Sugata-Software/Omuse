use super::*;
use omuse::vector_scene::{VectorObject, VectorScene};

fn snapshot(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> VectorScene {
    view.update(cx, |view, _| {
        view.vector_draft
            .as_ref()
            .unwrap()
            .scene_snapshot()
            .unwrap()
    })
}
fn command(view: &Entity<EditorView>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(name, window, cx)));
    cx.run_until_parked();
    draw(cx);
}

fn clipped_artwork() -> VectorScene {
    let source = VectorScene {
        version: 1,
        width: 40,
        height: 30,
        objects: vec![
            VectorObject::rectangle(
                "Wide contents",
                -500.,
                -500.,
                1040.,
                1030.,
                Some([220, 50, 30, 255]),
                None,
            )
            .unwrap(),
            VectorObject::rectangle("Mask", 6., 6., 22., 16., Some([40, 80, 220, 255]), None)
                .unwrap(),
        ],
    };
    omuse::vector_clip::make(&source, &[0, 1]).unwrap().0
}

fn open_clipped(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zoom = 8.;
            view.preferences.snapping = false;
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(clipped_artwork(), [0, 1].into_iter().collect(), window, cx)
                .unwrap();
            view.set_scene_mode(super::super::scene::SceneMode::Select, window, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    (view, cx)
}

#[gpui_kit::test]
fn hidden_clipping_source_still_has_selection_overlay_and_editable_nodes(cx: &mut TestAppContext) {
    let (view, cx) = open_clipped(cx);
    let mut source = clipped_artwork();
    source.objects[1].visible = false;
    let pixels = source
        .render(&std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_scene_artwork(source.clone(), [0, 1].into_iter().collect(), window, cx)
                .unwrap();
            assert!(view.vector_canvas_overlay().is_some());
            view.scene_clip_edit(true, window, cx);
            assert!(
                view.vector_canvas_overlay().is_some(),
                "hidden active mask must expose node handles"
            );
        })
    });
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        snapshot(&view, cx)
            .render(&std::sync::atomic::AtomicBool::new(false))
            .unwrap(),
        pixels
    );
    let from = canvas_point(&view, cx, 6., 6.);
    let to = canvas_point(&view, cx, 8., 8.);
    drag(cx, from, to, MouseButton::Left);
    let edited = snapshot(&view, cx);
    assert_eq!(
        edited.objects[1].path.subpaths[0].anchors[0].position,
        VectorPoint { x: 8., y: 8. }
    );
    assert!(
        !edited.objects[1].visible,
        "selection visibility must not change artwork state"
    );
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), source);
}

#[test]
fn clip_targets_refuse_mixed_or_empty_selections_and_bounds_exclude_hidden_geometry() {
    use super::super::{vector_clip_ui::edit_targets, vector_selection::clipped_object_bounds};
    let mut source = clipped_artwork();
    assert!(edit_targets(&source, &Default::default()).is_none());
    assert_eq!(
        edit_targets(&source, &[0].into_iter().collect()),
        Some((1, [0].into_iter().collect()))
    );
    assert_eq!(
        edit_targets(&source, &[1].into_iter().collect()),
        Some((1, [0].into_iter().collect()))
    );
    assert_eq!(
        clipped_object_bounds(&source, 0),
        Some((VectorPoint { x: 6., y: 6. }, VectorPoint { x: 28., y: 22. }))
    );
    assert!(clipped_object_bounds(&source, 1).is_none());
    source.objects.push(
        VectorObject::rectangle("Unrelated", 1., 1., 2., 2., Some([0, 0, 0, 255]), None).unwrap(),
    );
    assert!(edit_targets(&source, &[0, 2].into_iter().collect()).is_none());
    // UI drafts bake supported transforms before selection; bounds follow the
    // transformed silhouette without altering the retained content path.
    source.objects[1].transform = [1., 0., 0., 1., 3., 2.];
    super::super::scene::bake_transforms(&mut source).unwrap();
    assert_eq!(
        clipped_object_bounds(&source, 0),
        Some((VectorPoint { x: 9., y: 8. }, VectorPoint { x: 31., y: 24. }))
    );
}

#[gpui_kit::test]
fn clipping_target_actions_and_marquee_keep_full_contents_editable(cx: &mut TestAppContext) {
    let (view, cx) = open_clipped(cx);
    let original = snapshot(&view, cx);
    click_inspector_id(cx, "vector-clip-edit-mask");
    view.update_in(cx, |view, window, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(scene.selected_objects, [1].into_iter().collect());
        assert!(scene.mode == super::super::scene::SceneMode::Nodes);
        assert!(view.focus.is_focused(window));
    });
    assert_eq!(snapshot(&view, cx), original);
    click_inspector_id(cx, "vector-clip-edit-contents");
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(
            draft.scene.as_ref().unwrap().selected_objects,
            [0].into_iter().collect()
        );
        assert_eq!(
            draft.path.bounds().unwrap().0,
            VectorPoint { x: -500., y: -500. }
        );
    });
    let from = canvas_point(&view, cx, 4., 4.);
    let to = canvas_point(&view, cx, 30., 24.);
    drag(cx, from, to, MouseButton::Left);
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .selected_objects,
            [0, 1].into_iter().collect()
        )
    });
    assert_eq!(snapshot(&view, cx), original);
    // Normal picking selects the relationship; Ctrl-click accesses one content.
    let inside = canvas_point(&view, cx, 14., 14.);
    cx.simulate_click(
        inside,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .selected_objects,
            [0].into_iter().collect()
        )
    });
    cx.simulate_click(inside, Modifiers::default());
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .selected_objects,
            [0, 1].into_iter().collect()
        )
    });
}

#[gpui_kit::test]
fn escape_cancels_clip_drag_preserving_prior_edits_redo_and_late_release(cx: &mut TestAppContext) {
    let (view, cx) = open_clipped(cx);
    cx.simulate_keystrokes("right");
    let before = snapshot(&view, cx);
    cx.simulate_keystrokes("right");
    let redone = snapshot(&view, cx);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), before);
    let from = canvas_point(&view, cx, 14., 14.);
    let to = canvas_point(&view, cx, 17., 16.);
    cx.simulate_event(MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    assert_ne!(snapshot(&view, cx), before);
    cx.simulate_keystrokes("escape");
    assert_eq!(snapshot(&view, cx), before);
    view.update(cx, |view, _| assert!(view.vector_scene_active()));
    cx.simulate_event(MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    assert_eq!(snapshot(&view, cx), before);
    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(snapshot(&view, cx), redone);
}

#[gpui_kit::test]
fn escape_cancels_mask_node_and_marquee_without_losing_the_draft(cx: &mut TestAppContext) {
    let (view, cx) = open_clipped(cx);
    click_inspector_id(cx, "vector-clip-edit-mask");
    let before = snapshot(&view, cx);
    let from = canvas_point(&view, cx, 6., 6.);
    let to = canvas_point(&view, cx, 8., 8.);
    cx.simulate_event(MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    assert_ne!(snapshot(&view, cx), before);
    cx.simulate_keystrokes("escape");
    assert_eq!(snapshot(&view, cx), before);
    cx.simulate_event(MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    assert_eq!(snapshot(&view, cx), before);
    command(&view, cx, "vector-clip-edit-contents");
    let selected = view.update(cx, |view, _| {
        view.vector_draft
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .selected_objects
            .clone()
    });
    let from = canvas_point(&view, cx, 1., 1.);
    let to = canvas_point(&view, cx, 30., 25.);
    cx.simulate_event(MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .selected_objects,
            selected
        )
    });
    cx.simulate_event(MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    assert_eq!(snapshot(&view, cx), before);
}

#[gpui_kit::test]
fn clipping_commands_mask_row_node_edit_delete_guard_and_project_roundtrip(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    let source = VectorScene {
        version: 1,
        width: 40,
        height: 30,
        objects: vec![
            VectorObject::rectangle("Artwork", 0., 0., 40., 30., Some([240, 80, 50, 255]), None)
                .unwrap(),
            VectorObject::rectangle(
                "Editable mask",
                6.,
                6.,
                22.,
                16.,
                Some([40, 80, 220, 255]),
                None,
            )
            .unwrap(),
        ],
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zoom = 8.;
            view.preferences.snapping = false;
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(source.clone(), [0, 1].into_iter().collect(), window, cx)
                .unwrap();
        })
    });
    cx.run_until_parked();
    draw(cx);
    command(&view, cx, "vector-clip-make");
    let clipped = snapshot(&view, cx);
    assert!(clipped.objects[1].is_clip);
    assert_eq!(
        clipped.objects[0].clip_to.as_deref(),
        Some(clipped.objects[1].id.as_str())
    );
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), source);
    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(snapshot(&view, cx), clipped);
    command(&view, cx, "vector-nodes");
    click_inspector_id(cx, "scene-object-0");
    click_inspector_id(cx, "scene-object-1");
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .active,
            1
        )
    });
    let from = canvas_point(&view, cx, 6., 6.);
    let to = canvas_point(&view, cx, 8., 8.);
    drag(cx, from, to, MouseButton::Left);
    let edited = snapshot(&view, cx);
    assert_eq!(
        edited.objects[1].path.subpaths[0].anchors[0].position,
        VectorPoint { x: 8., y: 8. }
    );
    assert_eq!(edited.objects[0], clipped.objects[0]);
    // A valid triangle is editable, but its last required point and closed
    // topology cannot be destroyed while other objects depend on it.
    view.update(cx, |view, _| {
        view.vector_draft.as_mut().unwrap().selected = Some((0, 0))
    });
    command(&view, cx, "delete-content");
    let triangle = snapshot(&view, cx);
    assert_eq!(triangle.objects[1].path.subpaths[0].anchors.len(), 3);
    view.update(cx, |view, _| {
        view.vector_draft.as_mut().unwrap().selected = Some((0, 0))
    });
    command(&view, cx, "delete-content");
    assert_eq!(snapshot(&view, cx), triangle);
    command(&view, cx, "vector-split-node");
    assert_eq!(snapshot(&view, cx), triangle);
    click_inspector_id(cx, "vector-close");
    assert_eq!(snapshot(&view, cx), triangle);
    command(&view, cx, "vector-path");
    let outside = canvas_point(&view, cx, 38., 28.);
    cx.simulate_click(outside, Modifiers::default());
    assert_eq!(snapshot(&view, cx), triangle);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), edited);
    // Removing a retained mask must not silently expose the clipped contents.
    // Its labelled inspector row targets just the mask; canvas unit selection
    // must not turn this deliberate mask edit into deletion of the whole clip.
    click_inspector_id(cx, "scene-object-1");
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .selected_objects,
            [1].into_iter().collect()
        );
    });
    cx.simulate_keystrokes("backspace");
    assert_eq!(snapshot(&view, cx), edited);
    view.update(cx, |view, _| {
        assert!(
            view.status.to_ascii_lowercase().contains("release"),
            "{}",
            view.status
        )
    });
    command(&view, cx, "vector-clip-release");
    let released = snapshot(&view, cx);
    assert!(!released.objects[1].is_clip);
    assert!(released.objects[0].clip_to.is_none());
    assert_eq!(released.objects[1].path, edited.objects[1].path);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(snapshot(&view, cx), edited);
    cx.run_until_parked();
    click_id(cx, "scene-done");
    cx.run_until_parked();
    let document = view.update(cx, |view, _| {
        assert!(!view.vector_scene_active(), "{}", view.status);
        assert_eq!(view.editor.document.layers.len(), 2);
        view.editor.document.clone()
    });
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Mask edited.omuse");
    omuse::document::save(&document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(reopened.layers[1].vector_scene.as_deref(), Some(&edited));
    assert_eq!(reopened.layers[1].image, document.layers[1].image);
}
