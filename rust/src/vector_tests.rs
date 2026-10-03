use super::*;
use gpui_kit::{Focusable, Modifiers, TestAppContext, VisualTestContext};

#[path = "vector_selection_tests.rs"]
mod selection;
#[path = "vector_style_tests.rs"]
mod styles;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        let mut doc = Document::new(40, 30);
        doc.layers[0].image =
            Some(image::RgbaImage::from_pixel(40, 30, image::Rgba([80, 90, 100, 255])).into());
        view.editor = Editor::new(doc);
        view.refresh(cx);
        view.focus.focus(window, cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    (view, cx)
}
fn scroll_to_id(cx: &mut VisualTestContext, id: &'static str, viewport_id: &'static str) {
    for _ in 0..20 {
        let viewport = cx.debug_bounds(viewport_id).unwrap();
        let bounds = cx.debug_bounds(id);
        if bounds.is_some_and(|bounds| {
            bounds.origin.y >= viewport.origin.y
                && bounds.bottom_right().y <= viewport.bottom_right().y
        }) {
            break;
        }
        let delta = if bounds.is_some_and(|bounds| bounds.origin.y < viewport.origin.y) {
            100.
        } else {
            -100.
        };
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
            modifiers: Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        draw(cx);
    }
    let bounds = cx.debug_bounds(id).unwrap();
    let viewport = cx.debug_bounds(viewport_id).unwrap();
    assert!(
        bounds.origin.y >= viewport.origin.y
            && bounds.bottom_right().y <= viewport.bottom_right().y,
        "{id} is not reachable in compact layout: {bounds:?}, viewport {viewport:?}"
    );
}

fn click_id(cx: &mut VisualTestContext, id: &'static str) {
    if !matches!(id, "confirm-dialog" | "cancel-dialog") && cx.debug_bounds("dialog-body").is_some()
    {
        scroll_to_id(cx, id, "dialog-body");
    }
    let b = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
}

fn click_inspector_id(cx: &mut VisualTestContext, id: &'static str) {
    scroll_to_id(cx, id, "inspector-content");
    let b = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
}

fn canvas_point(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    x: f32,
    y: f32,
) -> Point<Pixels> {
    view.update(cx, |view, _| {
        let bounds = view.viewport.get();
        point(
            bounds.origin.x
                + (bounds.size.width - px(view.editor.document.width as f32 * view.zoom)) / 2.
                + px(view.pan.0 + x * view.zoom),
            bounds.origin.y
                + (bounds.size.height - px(view.editor.document.height as f32 * view.zoom)) / 2.
                + px(view.pan.1 + y * view.zoom),
        )
    })
}

fn layer_canvas_point(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    layer: &str,
    x: f32,
    y: f32,
) -> Point<Pixels> {
    let (x, y) = view.update(cx, |view, _| {
        view.editor.layer_to_canvas(layer, x, y).unwrap()
    });
    canvas_point(view, cx, x, y)
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>, button: MouseButton) {
    drag_with_modifiers(cx, from, to, button, Modifiers::default());
}

fn drag_with_modifiers(
    cx: &mut VisualTestContext,
    from: Point<Pixels>,
    to: Point<Pixels>,
    button: MouseButton,
    modifiers: Modifiers,
) {
    cx.simulate_event(MouseDownEvent {
        position: from,
        button,
        modifiers,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(to, Some(button), modifiers);
    cx.simulate_event(MouseUpEvent {
        position: to,
        button,
        modifiers,
        click_count: 1,
    });
    draw(cx);
}

fn double_click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    let modifiers = Modifiers::default();
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers,
        click_count: 2,
    });
    draw(cx);
}

fn svg_fixture() -> &'static [u8] {
    br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">
      <path d="M8 4 L72 4 L72 36 L8 36 Z M24 12 L56 12 L56 28 L24 28 Z"
        fill="#D58049" fill-rule="evenodd"/>
    </svg>"##
}

fn scene_field(view: &Entity<EditorView>, cx: &mut VisualTestContext, index: usize, value: &str) {
    let id = match index {
        0 => "vector-style-0",
        1 => "vector-style-1",
        2 => "vector-style-2",
        3 => "vector-style-3",
        _ => unreachable!(),
    };
    click_inspector_id(cx, id);
    view.update_in(cx, |view, window, cx| {
        assert!(
            view.detail_inputs[index]
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "{id} did not focus through its visible click target"
        );
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(value);
    draw(cx);
    view.update(cx, |view, cx| {
        assert_eq!(view.detail_inputs[index].read(cx).value(), value)
    });
}

#[gpui_kit::test]
fn vector_artwork_shortcut_objects_style_move_apply_and_reopen(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let original = view.update(cx, |view, _| {
        view.editor.document.layers[0].image.clone().unwrap()
    });
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.as_ref().unwrap().is_scene());
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.inspector_visible);
    });
    assert!(cx.debug_bounds("artwork").is_some());
    assert!(cx.debug_bounds("vector-canvas-context").is_some());
    assert!(cx.debug_bounds("vector-canvas-inspector").is_some());
    assert!(cx.debug_bounds("layers").is_some());
    click_inspector_id(cx, "scene-rectangle");
    scene_field(&view, cx, 0, "#C83214FF");
    click_inspector_id(cx, "vector-style-preview");
    click_inspector_id(cx, "scene-ellipse");
    let live_revision = view.update(cx, |view, _| view.editor.revision());
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .artwork
                .objects
                .len(),
            1
        );
        assert_eq!(view.editor.revision(), live_revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("redo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .artwork
                .objects
                .len(),
            2
        );
        assert_eq!(view.editor.revision(), live_revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    scene_field(&view, cx, 0, "#143CC8FF");
    scene_field(&view, cx, 3, "50");
    click_inspector_id(cx, "vector-style-preview");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(scene.artwork.objects.len(), 2);
        assert_eq!(scene.active, 1);
        assert!(scene.display.is_some());
        assert!(view.vector_canvas_display().is_some());
        assert_eq!(view.dialog, Dialog::None);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    click_inspector_id(cx, "scene-previous");
    view.update(cx, |view, cx| {
        assert_eq!(view.detail_inputs[0].read(cx).value(), "#C83214FF");
        assert_eq!(view.detail_inputs[3].read(cx).value(), "100");
    });
    click_inspector_id(cx, "scene-next");
    view.update(cx, |view, cx| {
        assert_eq!(view.detail_inputs[0].read(cx).value(), "#143CC8FF");
        assert_eq!(view.detail_inputs[3].read(cx).value(), "50");
        assert!(
            view.vector_artwork(cx)
                .unwrap_err()
                .to_string()
                .contains("100% object opacity")
        );
    });
    click_inspector_id(cx, "scene-duplicate");
    click_inspector_id(cx, "scene-remove");
    click_inspector_id(cx, "scene-backward");
    click_inspector_id(cx, "scene-forward");
    click_inspector_id(cx, "scene-visibility");
    click_inspector_id(cx, "scene-visibility");

    // Canvas navigation remains live throughout an inline vector session.
    let (zoom_before, pan_before) = view.update(cx, |view, _| (view.zoom, view.pan));
    for _ in 0..6 {
        click_id(cx, "zoom-in");
    }
    let artwork = cx.debug_bounds("artwork").unwrap();
    drag(
        cx,
        artwork.center(),
        artwork.center() + point(px(24.), px(-16.)),
        MouseButton::Middle,
    );
    view.update(cx, |view, _| {
        assert!(view.zoom > zoom_before);
        assert_ne!(view.pan, pan_before);
        assert!(view.vector_scene_active());
        assert_eq!(view.dialog, Dialog::None);
    });

    // Pick the lower rectangle through its filled interior, outside the top
    // ellipse, then move it with a genuine routed canvas drag.
    let inside_rectangle = canvas_point(&view, cx, 8.5, 6.5);
    cx.simulate_click(inside_rectangle, Modifiers::default());
    draw(cx);
    let before = view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(draft.scene.as_ref().unwrap().active, 0);
        draft.path.subpaths[0].anchors[0].position
    });
    let moved_rectangle = canvas_point(&view, cx, 12.5, 9.5);
    drag(cx, inside_rectangle, moved_rectangle, MouseButton::Left);
    view.update(cx, |view, _| {
        let after = view.vector_draft.as_ref().unwrap().path.subpaths[0].anchors[0].position;
        assert!((after.x - before.x - 4.).abs() < 0.01);
        assert!((after.y - before.y - 3.).abs() < 0.01);
    });
    cx.run_until_parked();
    draw(cx);
    click_id(cx, "scene-done");
    cx.run_until_parked();
    draw(cx);
    let document = view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.editor.document.layers.len(), 2);
        assert_eq!(
            view.editor.document.layers[0].image.as_ref(),
            Some(&original)
        );
        let layer = view
            .editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap();
        assert!(layer.advanced.is_none());
        assert_eq!(layer.vector_scene.as_ref().unwrap().objects.len(), 2);
        // Independent source-over oracle for blue at 50% over opaque red.
        let center = layer.image.as_ref().unwrap().get_pixel(20, 15).0;
        assert_eq!(center, [110, 55, 110, 255]);
        view.editor.document.clone()
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scene.omuse");
    omuse::document::save(&document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(
        reopened.layers[1].vector_scene,
        document.layers[1].vector_scene
    );
    assert_eq!(reopened.layers[1].image, document.layers[1].image);
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(
            !view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .is_new
        );
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .mode
                == scene::SceneMode::Pen
        );
    });
    click_inspector_id(cx, "scene-object-1");
    click_id(cx, "scene-cancel");
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(
            view.editor.document.layers[0].image.as_ref(),
            Some(&original)
        );
    });
}

#[gpui_kit::test]
fn vector_artwork_cancel_and_stale_apply_preserve_newer_document(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let original = view.update(cx, |view, _| {
        view.editor.document.layers[0].image.clone().unwrap()
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    view.update(cx, |view, _| assert_eq!(view.dialog, Dialog::None));
    click_inspector_id(cx, "scene-rectangle");
    // Cancel while the coalesced preview is still eligible to complete. Its
    // late result must not resurrect the inline display or mutate history.
    click_id(cx, "scene-cancel");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.is_none());
        assert!(view.vector_canvas_display().is_none());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(
            view.editor.document.layers[0].image.as_ref(),
            Some(&original)
        );
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    click_inspector_id(cx, "scene-ellipse");
    view.update(cx, |view, _| {
        assert!(view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 0);
        view.editor.add_layer("Concurrent edit");
    });
    // Rendering validates the revision fence and discards the stale session;
    // its already queued preview may still finish afterward.
    draw(cx);
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.vector_draft.is_none());
        assert!(view.vector_canvas_display().is_none());
        assert!(view.status.contains("discarded"), "{}", view.status);
        assert_eq!(view.editor.document.layers.len(), 2);
        assert!(
            view.editor
                .document
                .layers
                .iter()
                .all(|layer| layer.vector_scene.is_none())
        );
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(
            view.editor.document.layers[0].image.as_ref(),
            Some(&original)
        );
    });
}

#[gpui_kit::test]
fn vector_nodes_follow_rotated_flipped_nonuniform_layer_coordinates(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let layer = view.update(cx, |view, cx| {
        let object = omuse::vector_scene::VectorObject::rectangle(
            "Transformed rectangle",
            3.,
            3.,
            10.,
            8.,
            Some([190, 70, 35, 255]),
            None,
        )
        .unwrap();
        let artwork = omuse::vector_scene::VectorScene {
            version: omuse::vector_scene::VECTOR_SCENE_VERSION,
            width: 20,
            height: 16,
            objects: vec![object],
        };
        let cache = artwork
            .render(&std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        let layer = view
            .editor
            .insert_vector_scene("Transformed scene", view.editor.revision(), artwork, cache)
            .unwrap();
        assert!(view.editor.transform_layer(&layer, 6., 4., 37., -1.2, 0.7));
        view.select_layer_ids(vec![layer.clone()]);
        view.refresh(cx);
        layer
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    click_id(cx, "scene-nodes");
    for _ in 0..5 {
        click_id(cx, "zoom-in");
    }

    let original = VectorPoint { x: 3., y: 3. };
    let target = VectorPoint { x: 5.25, y: 1.5 };
    let from = layer_canvas_point(&view, cx, &layer, original.x, original.y);
    let to = layer_canvas_point(&view, cx, &layer, target.x, target.y);
    drag(cx, from, to, MouseButton::Left);
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(draft.selected, Some((0, 0)));
        let moved = draft.path.subpaths[0].anchors[0].position;
        assert!((moved.x - target.x).abs() < 0.001, "{moved:?}");
        assert!((moved.y - target.y).abs() < 0.001, "{moved:?}");
        assert_eq!(view.dialog, Dialog::None);
    });
    let before_move = view.update(cx, |view, _| {
        view.vector_draft.as_ref().unwrap().path.clone()
    });
    cx.simulate_keystrokes("v right");
    draw(cx);
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert!(draft.selected.is_none());
        assert!(draft.drag.is_none());
        for (before, after) in before_move.subpaths[0]
            .anchors
            .iter()
            .zip(&draft.path.subpaths[0].anchors)
        {
            assert_eq!(after.position.x, before.position.x + 1.);
            assert_eq!(after.position.y, before.position.y);
        }
    });
    cx.simulate_keystrokes("backspace");
    draw(cx);
    view.update(cx, |view, _| {
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .path
                .subpaths
                .iter()
                .all(|s| s.anchors.is_empty())
        );
    });
    click_id(cx, "scene-cancel");
}

#[gpui_kit::test]
fn pen_click_drag_builds_smooth_nodes_and_first_anchor_closes_one_undo_step(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    // Node and handle hit targets stay nine screen pixels wide. Zoom this
    // compact 40 px fixture before placing deliberately nearby anchors so a
    // Pen press does not correctly resolve to the preceding anchor instead.
    for _ in 0..5 {
        click_id(cx, "zoom-in");
    }
    let revision = view.update(cx, |view, _| view.editor.revision());
    let first = canvas_point(&view, cx, 7., 7.);
    let smooth = canvas_point(&view, cx, 20., 7.);
    let smooth_handle = canvas_point(&view, cx, 24., 11.);
    let third = canvas_point(&view, cx, 20., 21.);
    cx.simulate_click(first, Modifiers::default());
    drag(cx, smooth, smooth_handle, MouseButton::Left);
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft.as_ref().unwrap().path.subpaths[0]
                .anchors
                .len(),
            1,
            "one Undo must remove the complete click-drag gesture"
        );
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    draw(cx);
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths[0].anchors.len(), 2);
        assert!(matches!(
            draft.scene.as_ref().unwrap().mode,
            scene::SceneMode::Pen
        ));
        assert_eq!(view.status, "Vector edit redone");
    });
    cx.simulate_click(third, Modifiers::default());
    draw(cx);
    view.update(cx, |view, _| {
        let path = &view.vector_draft.as_ref().unwrap().path;
        assert_eq!(path.subpaths[0].anchors.len(), 3);
        assert!(!path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].anchors[0].incoming, None);
        assert_eq!(path.subpaths[0].anchors[0].outgoing, None);
        let node = &path.subpaths[0].anchors[1];
        assert!((node.position.x - 20.).abs() < 0.001, "{node:?}");
        assert!((node.position.y - 7.).abs() < 0.001, "{node:?}");
        let incoming = node.incoming.unwrap();
        let outgoing = node.outgoing.unwrap();
        assert!((incoming.x - 16.).abs() < 0.001, "{node:?}");
        assert!((incoming.y - 3.).abs() < 0.001, "{node:?}");
        assert!((outgoing.x - 24.).abs() < 0.001, "{node:?}");
        assert!((outgoing.y - 11.).abs() < 0.001, "{node:?}");
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.simulate_click(first, Modifiers::default());
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.as_ref().unwrap().path.subpaths[0].closed);
        assert!(view.status.contains("Path closed"), "{}", view.status);
    });
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.vector_draft.as_ref().unwrap().path.subpaths[0].closed);
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.as_ref().unwrap().path.subpaths[0].closed);
    });
    scroll_to_id(cx, "vector-anchor-status", "inspector-content");
    assert!(cx.debug_bounds("vector-anchor-status").is_some());
    click_id(cx, "scene-cancel");
}

#[gpui_kit::test]
fn direct_selection_couples_smooth_handles_alt_breaks_and_shift_constrains(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_mut().unwrap();
        draft.scene.as_mut().unwrap().mode = scene::SceneMode::Nodes;
        draft.path = VectorPath {
            subpaths: vec![Subpath {
                anchors: vec![
                    Anchor {
                        position: VectorPoint { x: 4., y: 10. },
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: VectorPoint { x: 10., y: 10. },
                        incoming: Some(VectorPoint { x: 6., y: 10. }),
                        outgoing: Some(VectorPoint { x: 14., y: 10. }),
                    },
                    Anchor {
                        position: VectorPoint { x: 25., y: 10. },
                        incoming: None,
                        outgoing: None,
                    },
                ],
                closed: false,
            }],
            fill_rule: Default::default(),
        };
    });
    let handle = canvas_point(&view, cx, 14., 10.);
    let diagonal = canvas_point(&view, cx, 14., 14.);
    drag(cx, handle, diagonal, MouseButton::Left);
    let coupled_incoming = view.update(cx, |view, _| {
        let anchor = &view.vector_draft.as_ref().unwrap().path.subpaths[0].anchors[1];
        let incoming = anchor.incoming.unwrap();
        let outgoing = anchor.outgoing.unwrap();
        let a = VectorPoint {
            x: incoming.x - anchor.position.x,
            y: incoming.y - anchor.position.y,
        };
        let b = VectorPoint {
            x: outgoing.x - anchor.position.x,
            y: outgoing.y - anchor.position.y,
        };
        assert!((a.x * b.y - a.y * b.x).abs() < 0.001);
        assert!(a.x * b.x + a.y * b.y < 0.);
        incoming
    });
    let moved_handle = canvas_point(&view, cx, 14., 14.);
    let independent = canvas_point(&view, cx, 16., 8.);
    drag_with_modifiers(
        cx,
        moved_handle,
        independent,
        MouseButton::Left,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    view.update(cx, |view, _| {
        let anchor = &view.vector_draft.as_ref().unwrap().path.subpaths[0].anchors[1];
        assert_eq!(anchor.incoming, Some(coupled_incoming));
        let outgoing = anchor.outgoing.unwrap();
        assert!((outgoing.x - 16.).abs() < 0.001, "{anchor:?}");
        assert!((outgoing.y - 8.).abs() < 0.001, "{anchor:?}");
    });
    let anchor = canvas_point(&view, cx, 10., 10.);
    let off_axis = canvas_point(&view, cx, 15., 12.);
    drag_with_modifiers(
        cx,
        anchor,
        off_axis,
        MouseButton::Left,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    view.update(cx, |view, _| {
        let anchor = &view.vector_draft.as_ref().unwrap().path.subpaths[0].anchors[1];
        assert!((anchor.position.y - 10.).abs() < 0.001, "{anchor:?}");
        assert!(anchor.position.x > 15.);
    });
    click_id(cx, "scene-cancel");
}

#[gpui_kit::test]
fn direct_selection_double_click_inserts_on_the_existing_cubic(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    // Keep the cubic midpoint outside the fixed nine-pixel handle targets;
    // otherwise this intentionally tiny fixture tests handle dragging rather
    // than the segment double-click gesture.
    for _ in 0..5 {
        click_id(cx, "zoom-in");
    }
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_mut().unwrap();
        draft.scene.as_mut().unwrap().mode = scene::SceneMode::Nodes;
        draft.path = VectorPath {
            subpaths: vec![Subpath {
                anchors: vec![
                    Anchor {
                        position: VectorPoint { x: 5., y: 15. },
                        incoming: None,
                        outgoing: Some(VectorPoint { x: 5., y: 0. }),
                    },
                    Anchor {
                        position: VectorPoint { x: 30., y: 15. },
                        incoming: Some(VectorPoint { x: 30., y: 0. }),
                        outgoing: None,
                    },
                ],
                closed: false,
            }],
            fill_rule: Default::default(),
        };
    });
    let midpoint = canvas_point(&view, cx, 17.5, 3.75);
    double_click(cx, midpoint);
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths[0].anchors.len(), 3);
        assert_eq!(draft.selected, Some((0, 1)));
        let inserted = draft.path.subpaths[0].anchors[1].position;
        assert!((inserted.x - 17.5).abs() < 0.1, "{inserted:?}");
        assert!((inserted.y - 3.75).abs() < 0.1, "{inserted:?}");
        assert!(
            view.status.contains("without changing the curve"),
            "{}",
            view.status
        );
        assert_eq!(view.editor.undo_depth(), 0);
    });
    click_id(cx, "scene-cancel");
}

#[gpui_kit::test]
fn vector_tool_and_layer_switches_retain_once_then_resume(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let photo = view.update(cx, |view, _| view.editor.active_layer.clone());

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    click_inspector_id(cx, "scene-rectangle");
    click_id(cx, "tool-brush");
    cx.run_until_parked();
    draw(cx);
    let (vector, original_x) = view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.tool, Tool::Brush);
        assert_eq!(view.editor.undo_depth(), 1);
        let vector = view.editor.active_layer.clone();
        let x = view
            .editor
            .document
            .find_layer(&vector)
            .unwrap()
            .vector_scene
            .as_ref()
            .unwrap()
            .objects[0]
            .path
            .subpaths[0]
            .anchors[0]
            .position
            .x;
        (vector, x)
    });

    // Exercise the command router: P reopens an existing scene in Pen mode,
    // and a nudge remains local until the subsequent layer-row event exits.
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("vector-path", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_scene_active());
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .mode
                == scene::SceneMode::Pen
        );
    });
    let (live_revision, live_token) = view.update(cx, |view, _| {
        (
            view.editor.revision(),
            view.vector_draft.as_ref().unwrap().cancel.clone(),
        )
    });

    // These ordinary navigation surfaces are passthroughs. They must not
    // retain, discard or replace an in-progress draft.
    let vector_row: &'static str = Box::leak(format!("layer-{vector}").into_boxed_str());
    click_inspector_id(cx, vector_row);
    view.update(cx, |view, _| {
        assert!(std::sync::Arc::ptr_eq(
            &view.vector_draft.as_ref().unwrap().cancel,
            &live_token
        ));
        assert_eq!(view.editor.revision(), live_revision);
    });

    cx.simulate_keystrokes("ctrl-k");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::CommandSearch);
        assert!(std::sync::Arc::ptr_eq(
            &view.vector_draft.as_ref().unwrap().cancel,
            &live_token
        ));
        assert_eq!(view.editor.revision(), live_revision);
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(std::sync::Arc::ptr_eq(
            &view.vector_draft.as_ref().unwrap().cancel,
            &live_token
        ));
        assert_eq!(view.editor.revision(), live_revision);
    });

    click_id(cx, "shortcuts");
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::Shortcuts);
        assert!(std::sync::Arc::ptr_eq(
            &view.vector_draft.as_ref().unwrap().cancel,
            &live_token
        ));
        assert_eq!(view.editor.revision(), live_revision);
    });
    click_id(cx, "cancel-dialog");
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(std::sync::Arc::ptr_eq(
            &view.vector_draft.as_ref().unwrap().cancel,
            &live_token
        ));
        assert_eq!(view.editor.revision(), live_revision);
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.command("nudge-right", window, cx)));
    draw(cx);
    let photo_row: &'static str = Box::leak(format!("layer-{photo}").into_boxed_str());
    click_inspector_id(cx, photo_row);
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.active_layer, photo);
        assert_eq!(view.editor.undo_depth(), 2);
        let moved_x = view
            .editor
            .document
            .find_layer(&vector)
            .unwrap()
            .vector_scene
            .as_ref()
            .unwrap()
            .objects[0]
            .path
            .subpaths[0]
            .anchors[0]
            .position
            .x;
        assert!((moved_x - original_x - 1.).abs() < 0.001);
    });

    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.editor.undo_depth(), 1);
        let restored_x = view
            .editor
            .document
            .find_layer(&vector)
            .unwrap()
            .vector_scene
            .as_ref()
            .unwrap()
            .objects[0]
            .path
            .subpaths[0]
            .anchors[0]
            .position
            .x;
        assert!((restored_x - original_x).abs() < 0.001);
    });
}

#[gpui_kit::test]
fn empty_vector_scene_undo_redo_leave_document_history_untouched(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let (revision, depth, active, layer_ids) = view.update(cx, |view, _| {
        view.editor.add_layer("Existing history");
        (
            view.editor.revision(),
            view.editor.undo_depth(),
            view.editor.active_layer.clone(),
            view.editor
                .document
                .layers
                .iter()
                .map(|layer| layer.id.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert!(depth > 0, "fixture must contain document history");
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_scene_active());
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .path
                .subpaths
                .iter()
                .all(|subpath| subpath.anchors.is_empty())
        );
    });

    for command in ["undo", "redo"] {
        cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
        draw(cx);
        view.update(cx, |view, _| {
            assert!(view.vector_scene_active());
            assert_eq!(view.editor.revision(), revision);
            assert_eq!(view.editor.undo_depth(), depth);
            assert_eq!(view.editor.active_layer, active);
            assert_eq!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .map(|layer| layer.id.clone())
                    .collect::<Vec<_>>(),
                layer_ids
            );
        });
    }
    click_id(cx, "scene-cancel");
}

#[gpui_kit::test]
fn inline_scene_svg_choosers_restore_canvas_preview_focus_and_export_truth(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("scene-source.svg");
    let output = temporary.path().join("temp.svg");
    std::fs::write(&input, svg_fixture()).unwrap();
    let (revision, depth) = view.update(cx, |view, _| {
        (view.editor.revision(), view.editor.undo_depth())
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_vector_scene(window, cx)));
    draw(cx);
    click_inspector_id(cx, "vector-svg-import");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![input.clone()])
    });
    cx.run_until_parked();
    draw(cx);
    view.update_in(cx, |view, window, _| {
        assert!(view.focus.is_focused(window));
        assert!(!view.busy, "{}", view.status);
        assert!(view.vector_scene_active());
        assert!(view.vector_canvas_display().is_some());
        assert!(
            !view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .running
        );
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), depth);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert!(view.editor.document.layers[0].vector_scene.is_none());
        let draft = view.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths.len(), 2);
        assert_eq!(draft.path.fill_rule, omuse::vector_path::FillRule::EvenOdd);
        assert!(
            view.status.contains("Imported 1 editable SVG objects"),
            "{}",
            view.status
        );
    });

    click_inspector_id(cx, "vector-svg-export");
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    draw(cx);
    let exported_bytes = std::fs::read(&output).unwrap();
    assert!(!exported_bytes.is_empty());
    view.update_in(cx, |view, window, _| {
        assert!(view.focus.is_focused(window));
        assert!(!view.busy, "{}", view.status);
        assert!(view.vector_scene_active());
        assert!(view.vector_canvas_display().is_some());
        assert!(
            !view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .running
        );
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), depth);
        assert!(
            view.status.contains("Exported editable SVG"),
            "{}",
            view.status
        );
        assert!(view.status.contains("temp.svg"), "{}", view.status);
    });

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(std::fs::read(&output).unwrap(), exported_bytes);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.is_none());
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), depth);
        assert!(
            view.status.contains("exported SVG file is already saved"),
            "{}",
            view.status
        );
    });
}

#[gpui_kit::test]
fn editable_svg_chooser_fit_export_apply_and_reopen_preserve_geometry(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("source.svg");
    let output = temporary.path().join("exported.svg");
    std::fs::write(&input, svg_fixture()).unwrap();
    let initial = view.update(cx, |v, _| v.editor.revision());
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    draw(cx);
    click_id(cx, "vector-svg-import");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![input.clone()])
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(!v.busy, "{}", v.status);
        assert_eq!(v.editor.revision(), initial);
        let draft = v.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths.len(), 2);
        assert_eq!(draft.path.fill_rule, omuse::vector_path::FillRule::EvenOdd);
        assert_eq!(draft.fill, [213, 128, 73, 255]);
        let first = draft.path.subpaths[0].anchors[0].position;
        assert_eq!((first.x, first.y), (4., 7.));
    });
    click_id(cx, "vector-svg-export");
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    draw(cx);
    let exported = omuse::vector_svg::import(&output).unwrap();
    assert_eq!((exported.width, exported.height), (40, 30));
    assert_eq!(exported.path.subpaths.len(), 2);
    let output_bytes = std::fs::read(&output).unwrap();
    click_id(cx, "vector-svg-export");
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(std::fs::read(&output).unwrap(), output_bytes);
    view.update(cx, |v, _| {
        assert!(v.status.contains("existing file"), "{}", v.status);
        assert_eq!(v.editor.revision(), initial);
    });
    let wrong_extension = temporary.path().join("not-an-image.png");
    click_id(cx, "vector-svg-export");
    cx.simulate_new_path_selection(|_| Some(wrong_extension.clone()));
    cx.run_until_parked();
    draw(cx);
    assert!(!wrong_extension.exists());
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert!(v.status.contains("ending in .svg"), "{}", v.status);
    });
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let document = view.update(cx, |v, _| {
        assert_eq!(v.dialog, Dialog::None, "{}", v.status);
        assert_eq!(v.editor.undo_depth(), 1);
        let state = v.editor.document.layers[0].advanced.as_ref().unwrap();
        let pixels = state.proxy().unwrap();
        assert_eq!(pixels.get_pixel(8, 10).0, [213, 128, 73, 255]);
        assert_eq!(
            pixels.get_pixel(20, 15)[3],
            0,
            "compound-path hole must remain transparent"
        );
        v.editor.document.clone()
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("edit-object", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::VectorPath);
        assert!(
            view.vector_draft
                .as_ref()
                .is_some_and(|draft| !draft.is_scene())
        );
    });
    click_id(cx, "cancel-dialog");
    let package = temporary.path().join("editable.omuse");
    omuse::document::save(&document, &package).unwrap();
    let reopened = omuse::document::open(&package).unwrap();
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .recipe
            .vector
            .as_ref(),
        Some(&exported.path)
    );
    assert_eq!(std::fs::read(&input).unwrap(), svg_fixture());
    view.update(cx, |v, _| {
        assert!(v.editor.undo());
        assert_eq!(v.editor.revision(), initial);
        assert!(v.editor.document.layers[0].advanced.is_none());
    });
}

#[gpui_kit::test]
fn svg_stale_document_refuses_work_and_releases_pending_chooser(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("source.svg");
    let output = temporary.path().join("stale.svg");
    std::fs::write(&input, svg_fixture()).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
    });
    let original = view.update(cx, |v, _| v.vector_draft.as_ref().unwrap().path.clone());
    draw(cx);
    click_id(cx, "vector-svg-import");
    assert!(cx.did_prompt_for_paths());
    view.update(cx, |v, _| {
        let id = v.editor.active_layer.clone();
        assert!(v.editor.rename_layer(&id, "Changed while choosing"));
    });
    cx.simulate_path_prompt_response(|_| Some(vec![input]));
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert!(v.status.contains("Document changed"), "{}", v.status);
        assert_eq!(v.vector_draft.as_ref().unwrap().path, original);
        assert_eq!(v.editor.undo_depth(), 1);
    });
    // A stale draft cannot launch more work or become stranded in Apply.
    click_id(cx, "vector-svg-import");
    assert!(!cx.did_prompt_for_paths());
    click_id(cx, "confirm-dialog");
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert_eq!(v.editor.undo_depth(), 1);
        assert_eq!(v.dialog, Dialog::VectorPath);
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    click_id(cx, "vector-svg-export");
    view.update(cx, |v, _| {
        let id = v.editor.active_layer.clone();
        assert!(v.editor.rename_layer(&id, "Changed again"));
    });
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    draw(cx);
    assert!(!output.exists(), "A stale chooser must not publish an SVG");
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert!(v.status.contains("Document changed"), "{}", v.status);
        assert_eq!(v.editor.undo_depth(), 2);
    });
}

#[gpui_kit::test]
fn prepared_svg_export_rechecks_staleness_at_publish_and_cancel_after_commit_is_complete(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    let temporary = tempfile::tempdir().unwrap();
    let stale_output = temporary.path().join("stale-at-publish.svg");
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
    });
    let (guard, artwork) = view.update(cx, |v, cx| {
        let guard = v.vector_io_guard(cx).unwrap();
        let artwork = v.vector_artwork(cx).unwrap();
        v.busy = true;
        (guard, artwork)
    });
    let prepared = omuse::vector_svg::prepare_export(&stale_output, &artwork).unwrap();
    assert!(!stale_output.exists());
    view.update(cx, |v, _| {
        let id = v.editor.active_layer.clone();
        assert!(v.editor.rename_layer(&id, "Changed after preparation"));
    });
    view.update(cx, |v, cx| {
        assert!(
            v.publish_prepared_vector_export(&guard, prepared, cx)
                .is_none()
        );
        assert!(!v.busy);
        assert!(v.status.contains("Document changed"), "{}", v.status);
    });
    assert!(!stale_output.exists());
    assert!(std::fs::read_dir(temporary.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omuse-vector-svg-")
    }));

    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
    });
    let committed_output = temporary.path().join("committed.svg");
    let (guard, artwork) = view.update(cx, |v, cx| {
        let guard = v.vector_io_guard(cx).unwrap();
        let artwork = v.vector_artwork(cx).unwrap();
        v.busy = true;
        (guard, artwork)
    });
    let prepared = omuse::vector_svg::prepare_export(&committed_output, &artwork).unwrap();
    let published = view.update(cx, |v, cx| {
        v.publish_prepared_vector_export(&guard, prepared, cx)
            .unwrap()
    });
    assert!(committed_output.exists());
    view.update(cx, |v, _| {
        assert!(v.busy, "finalization keeps the draft fenced");
        assert!(v.status.contains("Exported editable SVG"), "{}", v.status);
    });
    draw(cx);
    click_id(cx, "cancel-dialog");
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert!(v.vector_draft.is_none());
        assert!(v.status.contains("Exported editable SVG"), "{}", v.status);
    });
    published.finish().unwrap();
    assert!(committed_output.exists());
    assert!(std::fs::read_dir(temporary.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omuse-vector-svg-")
    }));
}

#[gpui_kit::test]
fn svg_chooser_cancellation_and_late_results_preserve_reopened_draft(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("source.svg");
    std::fs::write(&input, svg_fixture()).unwrap();
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    draw(cx);
    click_id(cx, "vector-svg-import");
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(
            v.vector_draft.as_ref().unwrap().path.subpaths[0]
                .anchors
                .is_empty()
        );
    });
    click_id(cx, "vector-svg-import");
    // Close and reopen at the same document revision while the native chooser
    // is pending. Its result belongs to the old draft and must be discarded.
    click_id(cx, "cancel-dialog");
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    cx.simulate_path_prompt_response(|_| Some(vec![input]));
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(
            v.vector_draft.as_ref().unwrap().path.subpaths[0]
                .anchors
                .is_empty()
        );
    });
}

#[gpui_kit::test]
fn svg_refusal_keeps_existing_path_and_source_untouched(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("unsupported.svg");
    std::fs::write(&input, br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><text y="20">Keep editable</text></svg>"#).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
    });
    let original = view.update(cx, |v, _| v.vector_draft.as_ref().unwrap().path.clone());
    draw(cx);
    click_id(cx, "vector-svg-import");
    cx.simulate_path_prompt_response(|_| Some(vec![input]));
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(!v.busy);
        assert!(v.status.contains("text is unsupported"), "{}", v.status);
        assert_eq!(v.vector_draft.as_ref().unwrap().path, original);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn compact_node_controls_edit_compound_paths_and_commit_once(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_vector(false, window, cx);
            v.import_vector_artwork(
                omuse::vector_svg::decode(svg_fixture()).unwrap(),
                window,
                cx,
            )
            .unwrap();
            v.vector_draft.as_mut().unwrap().selected = Some((0, 0));
        })
    });
    draw(cx);
    assert_eq!(
        cx.debug_bounds("vector-path-preview").unwrap().size.height,
        px(160.)
    );
    click_id(cx, "vector-insert");
    view.update(cx, |v, _| {
        let draft = v.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths[0].anchors.len(), 5);
        assert_eq!(draft.selected, Some((0, 1)));
        let p = draft.path.subpaths[0].anchors[1].position;
        assert_eq!((p.x, p.y), (20., 7.));
    });
    click_id(cx, "vector-smooth");
    click_id(cx, "vector-corner");
    view.update(cx, |v, _| {
        let node = &v.vector_draft.as_ref().unwrap().path.subpaths[0].anchors[1];
        assert!(node.incoming.is_none() && node.outgoing.is_none());
    });
    click_id(cx, "vector-reverse");
    view.update(cx, |v, _| {
        assert_eq!(v.vector_draft.as_ref().unwrap().selected, Some((0, 3)))
    });
    // Open/close acts on the selected subpath, not always on the last one.
    click_id(cx, "vector-close");
    view.update(cx, |v, _| {
        let draft = v.vector_draft.as_ref().unwrap();
        assert!(!draft.path.subpaths[0].closed);
        assert!(draft.path.subpaths[1].closed);
    });
    click_id(cx, "vector-close");
    click_id(cx, "vector-fill-rule");
    view.update(cx, |v, _| {
        assert_eq!(
            v.vector_draft.as_ref().unwrap().path.fill_rule,
            omuse::vector_path::FillRule::NonZero
        )
    });
    click_id(cx, "vector-fill-rule");
    click_id(cx, "vector-new-subpath");
    view.update(cx, |v, _| {
        let draft = v.vector_draft.as_ref().unwrap();
        assert_eq!(draft.path.subpaths.len(), 3);
        assert!(draft.selected.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
    });
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert_eq!(v.dialog, Dialog::None, "{}", v.status);
        assert_eq!(v.editor.undo_depth(), 1);
        assert!(v.editor.undo());
        assert!(v.editor.document.layers[0].advanced.is_none());
    });
}

#[gpui_kit::test]
fn vector_mask_add_smooth_apply_undo_and_cancel(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let initial = view.update(cx, |v, _| v.editor.revision());
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(true, window, cx)));
    draw(cx);
    let source_bounds = view.update(cx, |v, _| {
        v.vector_draft.as_ref().unwrap().preview_bounds.get()
    });
    for (x, y) in [(0.2, 0.2), (0.8, 0.2), (0.5, 0.8)] {
        let p = point(
            source_bounds.origin.x + px(f32::from(source_bounds.size.width) * x),
            source_bounds.origin.y + px(f32::from(source_bounds.size.height) * y),
        );
        cx.simulate_click(p, Modifiers::default());
        draw(cx);
    }
    click_id(cx, "vector-smooth");
    click_id(cx, "vector-close");
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let cancel_revision = view.update(cx, |v, _| {
        let layer = v
            .editor
            .document
            .find_layer(&v.editor.active_layer)
            .unwrap();
        let state = layer.advanced.as_ref().unwrap();
        assert!(state.recipe.vector_is_mask);
        assert_eq!(
            state.recipe.vector.as_ref().unwrap().subpaths[0]
                .anchors
                .len(),
            3
        );
        assert!(layer.mask.is_some());
        assert_eq!(v.editor.undo_depth(), 1);
        assert!(v.editor.undo());
        // Undo restores the document revision stored with the snapshot.
        assert_eq!(v.editor.revision(), initial);
        assert!(v.editor.document.layers[0].advanced.is_none());
        assert!(v.editor.document.layers[0].mask.is_none());
        v.editor.revision()
    });
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    draw(cx);
    let preview = cx.debug_bounds("vector-path-preview").unwrap();
    cx.simulate_click(preview.center(), Modifiers::default());
    draw(cx);
    click_id(cx, "cancel-dialog");
    view.update(cx, |v, _| assert_eq!(v.editor.revision(), cancel_revision));
}

#[gpui_kit::test]
fn path_styling_recomputes_retained_filters_and_survives_reopen(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    view.update(cx, |v, _| {
        let draft = v.vector_draft.as_mut().unwrap();
        draft.path = VectorPath {
            subpaths: vec![Subpath {
                closed: true,
                anchors: [(5., 5.), (35., 5.), (35., 25.), (5., 25.)]
                    .into_iter()
                    .map(|(x, y)| Anchor {
                        position: VectorPoint { x, y },
                        incoming: None,
                        outgoing: None,
                    })
                    .collect(),
            }],
            fill_rule: Default::default(),
        };
        draft
            .state
            .as_mut()
            .unwrap()
            .recipe
            .nodes
            .push(omuse::advanced_ops::FilterNode {
                id: "inversion".into(),
                name: "Invert".into(),
                enabled: true,
                opacity: 1.,
                soft_mask: None,
                operation: omuse::advanced_ops::AdvancedOperation::Filter(
                    omuse::filters::Filter::Invert,
                ),
            });
    });
    for (index, value) in ["#102040FF", "#80A020FF", "2"].into_iter().enumerate() {
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.detail_inputs[index].update(cx, |input, cx| input.set_value(value, window, cx))
            })
        });
    }
    draw(cx);
    click_id(cx, "vector-style-preview");
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let document = view.update(cx, |v, _| {
        assert_eq!(v.dialog, Dialog::None, "{}", v.status);
        let state = v.editor.document.layers[0].advanced.as_ref().unwrap();
        assert_eq!(state.recipe.vector_fill, [16, 32, 64, 255]);
        assert_eq!(state.recipe.vector_stroke.unwrap().width, 2.);
        assert_eq!(
            state.proxy().unwrap().get_pixel(20, 15).0,
            [239, 223, 191, 255]
        );
        assert_eq!(v.editor.undo_depth(), 1);
        v.editor.document.clone()
    });
    let tmp = tempfile::tempdir().unwrap();
    let package = tmp.path().join("styled.omuse");
    omuse::document::save(&document, &package).unwrap();
    let reopened = omuse::document::open(&package).unwrap();
    let state = reopened.layers[0].advanced.as_ref().unwrap();
    assert_eq!(
        state.recipe.vector_stroke.unwrap().color,
        [128, 160, 32, 255]
    );
    assert_eq!(
        state.proxy().unwrap(),
        document.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .proxy()
            .unwrap()
    );
    view.update(cx, |v, _| {
        assert!(v.editor.undo());
        assert!(v.editor.document.layers[0].advanced.is_none());
    });
}
