use super::*;
use gpui_kit::{Focusable, Modifiers, TestAppContext, VisualTestContext};
use image::{Rgba, RgbaImage};
use std::sync::Arc;

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
        let mut document = Document::new(48, 36);
        let pixels = RgbaImage::from_fn(48, 36, |x, y| {
            let color = if x < 16 {
                [215, 55, 45, 255]
            } else if y < 18 {
                [35, 175, 85, 255]
            } else {
                [45, 85, 215, 255]
            };
            Rgba(color)
        });
        document.layers[0].name = "Trace source".into();
        document.layers[0].image = Some(pixels.into());
        view.editor = Editor::new(document);
        view.refresh(cx);
        view.focus.focus(window, cx);
        view
    });
    cx.simulate_resize(size(px(820.), px(620.)));
    draw(cx);
    (view, cx)
}

fn open_trace(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_image_trace(window, cx));
    });
    draw(cx);
}

fn wait_for_trace(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.image_trace_ready(), "{}", view.status);
        assert!(view.image_trace_display().is_some());
    });
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

fn reveal_trace_control(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    let viewport = cx
        .debug_bounds("trace-inspector-content")
        .expect("trace inspector viewport");
    let visible = |bounds: Bounds<Pixels>| {
        bounds.origin.y >= viewport.origin.y && bounds.bottom_right().y <= viewport.bottom_right().y
    };
    if let Some(bounds) = cx.debug_bounds(id).filter(|bounds| visible(*bounds)) {
        return bounds;
    }
    for delta in std::iter::once(10_000.).chain(std::iter::repeat_n(-90., 40)) {
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
            modifiers: Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        draw(cx);
        if let Some(bounds) = cx.debug_bounds(id).filter(|bounds| visible(*bounds)) {
            return bounds;
        }
    }
    panic!("trace control {id} is not reachable by scrolling");
}

fn set_trace_field(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    index: usize,
    value: &str,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let input = view.image_trace.draft.as_ref().unwrap().inputs[index].clone();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
            view.schedule_image_trace(cx);
        });
    });
    draw(cx);
}

#[gpui_kit::test]
fn trace_preview_controls_latest_request_and_invalid_keep_are_non_destructive(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let (revision, source, source_id) = view.update(cx, |view, _| {
        (
            view.editor.revision(),
            view.editor.document.layers[0].image.clone().unwrap(),
            view.editor.document.layers[0].id.clone(),
        )
    });
    open_trace(&view, cx);
    view.update_in(cx, |view, window, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.inspector_visible);
        assert!(view.focus.is_focused(window));
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    wait_for_trace(&view, cx);
    view.update(cx, |view, _| {
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].image.as_ref(), Some(&source));
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });

    click(cx, "trace-source");
    view.update(cx, |view, _| {
        assert!(view.image_trace.draft.as_ref().unwrap().show_original);
    });
    click(cx, "trace-result");
    view.update(cx, |view, _| {
        assert!(!view.image_trace.draft.as_ref().unwrap().show_original);
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });

    // Presets enqueue while the first replacement is still eligible to finish.
    // Only the newest generation may publish its prepared scene.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.trace_preset("logo", window, cx);
            view.trace_preset("photo", window, cx);
        });
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let draft = view.image_trace.draft.as_ref().unwrap();
        assert_eq!(draft.latest.as_ref().unwrap().mode, TraceMode::Color);
        assert_eq!(draft.latest.as_ref().unwrap().colors, 16);
        assert!(draft.prepared.is_some(), "{}", draft.message);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let input = view.image_trace.draft.as_ref().unwrap().inputs[0].clone();
            input.update(cx, |input, cx| input.set_value("3", window, cx));
            view.schedule_image_trace(cx);
            input.update(cx, |input, cx| input.set_value("5", window, cx));
            view.schedule_image_trace(cx);
        });
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let draft = view.image_trace.draft.as_ref().unwrap();
        assert_eq!(draft.latest.as_ref().unwrap().colors, 5);
        assert!(draft.prepared.is_some(), "{}", draft.message);
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });

    // The full inspector remains reachable on a compact window and the native
    // input receives focus through its visible click target.
    cx.simulate_resize(size(px(640.), px(460.)));
    draw(cx);
    let points = reveal_trace_control(cx, "trace-field-points");
    cx.simulate_click(points.center(), Modifiers::default());
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        let input = &view.image_trace.draft.as_ref().unwrap().inputs[7];
        assert!(input.read(cx).focus_handle(cx).is_focused(window));
    });

    set_trace_field(&view, cx, 0, "99");
    view.update(cx, |view, _| {
        let draft = view.image_trace.draft.as_ref().unwrap();
        assert!(draft.invalid);
        assert!(draft.prepared.is_none());
        assert!(!view.image_trace_ready());
    });
    click(cx, "trace-keep");
    view.update(cx, |view, _| {
        assert!(view.image_trace_active());
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].id, source_id);
        assert_eq!(view.editor.document.layers[0].image.as_ref(), Some(&source));
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });

    // Ordinary modal navigation is allowed and closing it returns to the same
    // trace draft. Editing commands remain guarded.
    let token = view.update(cx, |view, _| {
        view.image_trace.draft.as_ref().unwrap().token.clone()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command("command-search", window, cx));
    });
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::CommandSearch);
        assert!(Arc::ptr_eq(
            &token,
            &view.image_trace.draft.as_ref().unwrap().token
        ));
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command("add", window, cx));
    });
    view.update(cx, |view, _| {
        assert!(view.image_trace_active());
        assert_eq!(view.editor.document.layers.len(), 1);
        assert!(
            view.status.contains("Keep vectors or cancel"),
            "{}",
            view.status
        );
    });
}

#[gpui_kit::test]
fn pending_cancel_reopen_and_stale_identity_cannot_publish_late_results(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let original = view.update(cx, |view, _| {
        view.editor.document.layers[0].image.clone().unwrap()
    });
    let (old_token, new_token) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_image_trace(window, cx);
            assert!(view.image_trace.working);
            let old = view.image_trace.draft.as_ref().unwrap().token.clone();
            view.cancel_image_trace(window, cx);
            assert!(!view.image_trace_active());
            view.open_image_trace(window, cx);
            let new = view.image_trace.draft.as_ref().unwrap().token.clone();
            (old, new)
        })
    });
    assert!(!Arc::ptr_eq(&old_token, &new_token));
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.image_trace_ready(), "{}", view.status);
        assert!(Arc::ptr_eq(
            &new_token,
            &view.image_trace.draft.as_ref().unwrap().token
        ));
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(
            view.editor.document.layers[0].image.as_ref(),
            Some(&original)
        );
        assert_eq!(view.editor.undo_depth(), 0);
    });

    // Start another worker, then replace the editor with a distinct document
    // identity before it can complete. Neither the old preview nor its prepared
    // document may reappear in the replacement editor.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.trace_preset("logo", window, cx);
            assert!(view.image_trace.working);
            let mut replacement = Document::new(24, 20);
            replacement.name = "Replacement".into();
            replacement.layers[0].name = "New document pixels".into();
            replacement.layers[0].image =
                Some(RgbaImage::from_pixel(24, 20, Rgba([9, 18, 27, 255])).into());
            view.editor = Editor::new(replacement);
            view.refresh(cx);
        });
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.image_trace_active());
        assert!(view.image_trace_display().is_none());
        assert_eq!(view.editor.document.name, "Replacement");
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].name, "New document pixels");
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.status.contains("document changed"), "{}", view.status);
    });
}

#[gpui_kit::test]
fn keep_enters_node_editing_and_one_document_undo_restores_the_bitmap(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    let (source_id, source_pixels) = view.update(cx, |view, _| {
        (
            view.editor.document.layers[0].id.clone(),
            view.editor.document.layers[0].image.clone().unwrap(),
        )
    });
    open_trace(&view, cx);
    wait_for_trace(&view, cx);
    click(cx, "trace-keep");
    cx.run_until_parked();
    draw(cx);
    let traced_id = view.update(cx, |view, _| {
        assert!(!view.image_trace_active());
        assert!(view.vector_scene_active());
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.status.contains("A edits points"), "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.editor.document.layers.len(), 2);
        let source = view.editor.document.find_layer(&source_id).unwrap();
        assert!(!source.visible);
        assert_eq!(source.image.as_ref(), Some(&source_pixels));
        let traced = view
            .editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap();
        assert!(traced.visible);
        assert!(traced.vector_scene.is_some());
        assert!(traced.image.is_some());
        traced.id.clone()
    });

    cx.simulate_keystrokes("escape");
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.vector_scene_active());
        assert!(view.editor.document.find_layer(&traced_id).is_some());
        assert_eq!(view.editor.undo_depth(), 1);
    });
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.editor.document.layers.len(), 1);
        assert!(view.editor.document.find_layer(&traced_id).is_none());
        let source = view.editor.document.find_layer(&source_id).unwrap();
        assert!(source.visible);
        assert_eq!(source.image.as_ref(), Some(&source_pixels));
        assert!(source.vector_scene.is_none());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.redo_depth(), 1);
    });
}

#[gpui_kit::test]
fn trace_closes_on_revision_change_without_mutating_the_newer_document(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let newer = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_image_trace(window, cx);
            assert!(view.image_trace.working);
            view.editor.add_layer("Concurrent edit")
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.image_trace_active());
        assert!(view.image_trace_display().is_none());
        assert!(view.editor.document.find_layer(&newer).is_some());
        assert_eq!(view.editor.document.layers.len(), 2);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.status.contains("document changed"), "{}", view.status);
    });
}

#[gpui_kit::test]
fn cancelled_quit_prompt_preserves_ready_trace_and_settings(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, _| view.editor.mark_unsaved());
    open_trace(&view, cx);
    wait_for_trace(&view, cx);
    let (token, generation, options, revision) = view.update(cx, |view, _| {
        let draft = view.image_trace.draft.as_ref().unwrap();
        (
            draft.token.clone(),
            draft.generation,
            draft.latest.clone().unwrap(),
            view.editor.revision(),
        )
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command("quit", window, cx));
    });
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::Unsaved);
        assert!(matches!(view.pending.as_ref(), Some(Pending::Quit)));
        let draft = view.image_trace.draft.as_ref().unwrap();
        assert!(Arc::ptr_eq(&token, &draft.token));
        assert_eq!(draft.generation, generation);
        assert_eq!(draft.latest.as_ref(), Some(&options));
        assert!(draft.prepared.is_some());
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.pending.is_none());
        assert!(view.image_trace_ready());
        let draft = view.image_trace.draft.as_ref().unwrap();
        assert!(Arc::ptr_eq(&token, &draft.token));
        assert_eq!(draft.generation, generation);
        assert_eq!(draft.latest.as_ref(), Some(&options));
        assert_eq!(view.editor.revision(), revision);
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn locked_source_or_parent_refuses_trace_before_starting_a_worker(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let source_id = view.update(cx, |view, _| {
        let source = &mut view.editor.document.layers[0];
        source.locked = true;
        source.id.clone()
    });
    open_trace(&view, cx);
    view.update(cx, |view, _| {
        assert!(!view.image_trace_active());
        assert!(!view.image_trace.working);
        assert!(view.image_trace.worker_cancel.is_none());
        assert!(view.status.contains("Unlock"), "{}", view.status);
    });

    view.update(cx, |view, _| {
        let mut source = view.editor.document.layers.remove(0);
        source.locked = false;
        let mut folder = Layer::group("Locked folder");
        folder.locked = true;
        folder.children.push(source);
        view.editor.document.layers.push(folder);
        view.editor.active_layer = source_id.clone();
    });
    open_trace(&view, cx);
    view.update(cx, |view, _| {
        assert!(!view.image_trace_active());
        assert!(!view.image_trace.working);
        assert!(view.image_trace.worker_cancel.is_none());
        assert!(view.status.contains("parent groups"), "{}", view.status);
        assert!(view.editor.document.find_layer(&source_id).is_some());
        assert_eq!(view.editor.undo_depth(), 0);
    });
}
