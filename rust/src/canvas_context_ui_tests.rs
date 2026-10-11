use super::*;
use gpui_kit::{Focusable, Modifiers, TestAppContext, VisualTestContext};

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let temp = tempfile::tempdir().unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.preferences.context_toolbar = true;
        view.tool = Tool::Move;
        let mut document = Document::new(320, 180);
        document.layers[0].image =
            Some(image::RgbaImage::from_pixel(320, 180, image::Rgba([48, 96, 160, 255])).into());
        view.editor = Editor::new(document);
        view.refresh(cx);
        view.focus.focus(window, cx);
        view
    });
    cx.simulate_resize(size(px(1200.), px(850.)));
    draw(cx);
    (view, cx, temp)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    draw(cx);
    let center = cx.debug_bounds(selector).expect(selector).center();
    cx.simulate_mouse_move(center, None, Modifiers::default());
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    draw(cx);
}

#[gpui_kit::test]
fn photo_button_click_duplicates_once_without_starting_canvas_interaction(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (original_id, original_pixels) = cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.canvas_context(cx), Some(CanvasContext::Photo));
        (view.editor.active_layer.clone(), view.pixels.clone())
    });
    let point = cx
        .debug_bounds("canvas-context-duplicate")
        .unwrap()
        .center();
    cx.simulate_mouse_move(point, None, Modifiers::default());
    cx.simulate_mouse_down(point, MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.drag_start.is_none());
        assert!(view.transform_drag.is_none());
        assert!(view.pan_pointer.is_none());
        assert_eq!(view.editor.active_layer, original_id);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.simulate_mouse_up(point, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor.document.layers.len(), 2);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.drag_start.is_none());
        assert!(view.transform_drag.is_none());
        assert_eq!(view.pixels, original_pixels);
        assert_ne!(view.editor.active_layer, original_id);
        view.command("undo", window, cx);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.document.layers[0].id, original_id);
        assert_eq!(view.pixels, original_pixels);
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn toolbar_rechecks_context_and_gesture_admission_before_dispatch(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |view, window, cx| {
        // Dispatching a formerly visible action must use the current state.
        view.busy = true;
        assert_eq!(view.canvas_context(cx), None);
        view.run_canvas_context("duplicate", window, cx);
        view.busy = false;
        view.dialog = Dialog::Text;
        assert_eq!(view.canvas_context(cx), None);
        view.run_canvas_context("duplicate", window, cx);
        view.dialog = Dialog::None;
        view.drag_start = Some((12., 16.));
        view.run_canvas_context("duplicate", window, cx);
        assert_eq!(view.drag_start, Some((12., 16.)));
        view.drag_start = None;
        view.paint_mask = true;
        view.run_canvas_context("duplicate", window, cx);
        view.paint_mask = false;
        view.space_down = true;
        view.run_canvas_context("duplicate", window, cx);
        view.space_down = false;
        view.tool = Tool::Brush;
        assert_eq!(view.canvas_context(cx), None);
        view.run_canvas_context("duplicate", window, cx);
        view.tool = Tool::Move;
        view.editor.select_rect(8, 9, 30, 20);
        assert_eq!(view.canvas_context(cx), Some(CanvasContext::Selection));
        view.run_canvas_context("duplicate", window, cx);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.undo_depth(), 0);
        view.run_canvas_context("deselect", window, cx);
        assert!(view.editor.selection.is_none());
        assert_eq!(view.canvas_context(cx), Some(CanvasContext::Photo));
    });
}

#[gpui_kit::test]
fn locked_and_hidden_ancestors_protect_toolbar_actions(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (group_id, child_id) = view.update(cx, |view, cx| {
        let child = view.editor.document.layers[0].clone();
        let child_id = child.id.clone();
        let mut group = Layer::group("Protected group");
        let group_id = group.id.clone();
        group.locked = true;
        group.children.push(child);
        let mut document = Document::new(320, 180);
        document.layers = vec![group];
        view.editor = Editor::new(document);
        view.select_layer_ids(vec![child_id.clone()]);
        view.refresh(cx);
        (group_id, child_id)
    });
    draw(cx);
    // Both the rendered disabled control and direct delayed dispatch are inert.
    click(cx, "canvas-context-duplicate");
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor.undo_depth(), 0);
        view.run_canvas_context("duplicate", window, cx);
        assert_eq!(view.editor.document.layers[0].children.len(), 1);
        let group = view.editor.document.find_layer_mut(&group_id).unwrap();
        group.locked = false;
        group.visible = false;
        assert_eq!(view.canvas_context(cx), None);
        view.run_canvas_context("duplicate", window, cx);
        assert_eq!(view.editor.undo_depth(), 0);
        view.editor
            .document
            .find_layer_mut(&group_id)
            .unwrap()
            .visible = true;
        view.select_layer_ids(vec![group_id.clone()]);
        assert_eq!(view.canvas_context(cx), Some(CanvasContext::Folder));
        view.run_canvas_context("ungroup", window, cx);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.editor.document.find_layer(&group_id).is_none());
        assert!(view.editor.document.find_layer(&child_id).is_some());
        view.command("undo", window, cx);
        assert!(view.editor.document.find_layer(&group_id).is_some());
    });
}

#[gpui_kit::test]
fn selected_pixel_and_layer_contexts_have_distinct_actions(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |view, cx| {
        view.editor.select_rect(10, 12, 20, 18);
        view.refresh(cx);
    });
    draw(cx);
    assert!(
        cx.debug_bounds("canvas-context-controlled-removal")
            .is_some()
    );
    assert!(cx.debug_bounds("canvas-context-camera-raw").is_none());
    click(cx, "canvas-context-deselect");
    assert!(cx.debug_bounds("canvas-context-camera-raw").is_some());
    view.update(cx, |view, cx| {
        let first = view.editor.active_layer.clone();
        view.editor.add_layer("Second");
        let second = view.editor.active_layer.clone();
        view.select_layer_ids(vec![first, second]);
        assert_eq!(view.canvas_context(cx), Some(CanvasContext::Layers));
        view.refresh(cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("canvas-context-group").is_some());
    assert!(cx.debug_bounds("canvas-context-auto-layout").is_some());
    assert!(cx.debug_bounds("canvas-context-camera-raw").is_none());
}

#[gpui_kit::test]
fn pointer_ungroup_returns_focus_for_immediate_keyboard_undo_and_redo(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (frame, history, original) = view.update(cx, |view, cx| {
        let child = view.editor.active_layer.clone();
        let frame = view
            .editor
            .create_layout_frame(
                std::slice::from_ref(&child),
                omuse::auto_layout::FrameRule::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        // Mirror selecting a frame from the inspector before using its dock.
        view.select_layer_ids(vec![child]);
        view.refresh(cx);
        (frame, view.editor.undo_depth(), view.pixels.clone())
    });
    draw(cx);
    let row = Box::leak(format!("layer-{frame}").into_boxed_str());
    let bounds = cx.debug_bounds(row).expect("retained frame row");
    cx.simulate_click(
        point(bounds.origin.x + px(80.), bounds.center().y),
        Modifiers::default(),
    );
    draw(cx);
    click(cx, "canvas-context-ungroup");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.editor.document.find_layer(&frame).is_none());
        assert_eq!(view.editor.undo_depth(), history + 1);
        assert!(view.focus.is_focused(window));
    });
    assert!(cx.debug_bounds("canvas-context-ungroup").is_none());
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let restored = view
            .editor
            .document
            .find_layer(&frame)
            .expect("keyboard Undo restores frame");
        assert!(omuse::auto_layout::frame_rule(restored).unwrap().is_some());
        assert_eq!(view.editor.undo_depth(), history);
        assert_eq!(view.pixels, original);
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.editor.document.find_layer(&frame).is_none());
        assert_eq!(view.editor.undo_depth(), history + 1);
    });
}

#[gpui_kit::test]
fn hiding_the_toolbar_keeps_immediate_document_shortcuts_working(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    click(cx, "canvas-context-duplicate");
    click(cx, "canvas-context-hide");
    assert!(cx.debug_bounds("canvas-context-toolbar").is_none());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(!view.preferences.context_toolbar);
        assert!(view.focus.is_focused(window));
        assert_eq!(view.editor.undo_depth(), 1);
    });
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.document.layers.len(), 2));
}

#[gpui_kit::test]
fn toolbar_auto_layout_keeps_its_modal_destination_focus(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |view, cx| {
        let child = view.editor.active_layer.clone();
        let group = view.editor.group_layers(&[child], "Group").unwrap();
        view.select_layer_ids(vec![group]);
        view.refresh(cx);
    });
    draw(cx);
    click(cx, "canvas-context-auto-layout");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::AutoLayout);
        assert!(view.modal_focus.contains_focused(window, cx));
        assert!(!view.focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn toolbar_stays_inside_the_canvas_after_resize_zoom_and_pan(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    for (width, height) in [(1200., 850.), (800., 700.), (1000., 750.)] {
        cx.simulate_resize(size(px(width), px(height)));
        draw(cx);
        let artwork = cx.debug_bounds("artwork").unwrap();
        let toolbar = cx.debug_bounds("canvas-context-toolbar").unwrap();
        assert!(toolbar.origin.x >= artwork.origin.x);
        assert!(toolbar.origin.y >= artwork.origin.y);
        assert!(toolbar.bottom_right().x <= artwork.bottom_right().x);
        assert!(toolbar.bottom_right().y <= artwork.bottom_right().y);
        for selector in [
            "canvas-context-camera-raw",
            "canvas-context-remove-background",
            "canvas-context-add-mask",
            "canvas-context-duplicate",
            "canvas-context-ask-omuse",
            "canvas-context-hide",
        ] {
            let button = cx.debug_bounds(selector).expect(selector);
            assert!(button.origin.x >= toolbar.origin.x, "{selector}");
            assert!(button.origin.y >= toolbar.origin.y, "{selector}");
            assert!(
                button.bottom_right().x <= toolbar.bottom_right().x,
                "{selector}"
            );
            assert!(
                button.bottom_right().y <= toolbar.bottom_right().y,
                "{selector}"
            );
        }
        view.update(cx, |view, cx| {
            view.zoom = 3.25;
            view.pan = (-500., 200.);
            cx.notify();
        });
        draw(cx);
        assert_eq!(cx.debug_bounds("canvas-context-toolbar").unwrap(), toolbar);
    }
}

#[gpui_kit::test]
fn edit_text_button_preserves_modal_input_focus_and_original_until_apply(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let id = view.update(cx, |view, cx| {
        let layer = objects::live_text_layer(
            "Title",
            objects::ObjectPoint { x: 16., y: 24. },
            objects::LiveTextStyle {
                content: "Original title".into(),
                font_size: 24.,
                ..Default::default()
            },
        )
        .unwrap();
        let id = layer.id.clone();
        let mut document = Document::new(320, 180);
        document.layers = vec![layer];
        view.editor = Editor::new(document);
        view.select_layer_ids(vec![id.clone()]);
        view.refresh(cx);
        id
    });
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).canvas_context(cx), Some(CanvasContext::Text)));
    click(cx, "canvas-context-edit-object");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::Text);
        assert!(view.path_input.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(view.canvas_context(cx), None);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("Revised title");
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        assert!(view.path_input.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(view.path_input.read(cx).value().as_ref(), "Revised title");
        assert_eq!(
            objects::live_text(view.editor.document.find_layer(&id).unwrap())
                .unwrap()
                .unwrap()
                .content,
            "Original title"
        );
        view.confirm_dialog(window, cx);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(
            objects::live_text(view.editor.document.find_layer(&id).unwrap())
                .unwrap()
                .unwrap()
                .content,
            "Revised title"
        );
    });
}

#[gpui_kit::test]
fn vector_toolbar_duplicate_uses_draft_history_until_keep(cx: &mut TestAppContext) {
    use omuse::vector_scene::{VectorObject, VectorScene};
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |view, window, cx| {
        let scene = VectorScene {
            version: 1,
            width: 320,
            height: 180,
            objects: vec![
                VectorObject::rectangle(
                    "Rectangle",
                    20.,
                    20.,
                    80.,
                    60.,
                    Some([200, 60, 30, 255]),
                    None,
                )
                .unwrap(),
            ],
        };
        let mut document = Document::new(320, 180);
        document.layers[0].image = Some(
            scene
                .render(&std::sync::atomic::AtomicBool::new(false))
                .unwrap()
                .into(),
        );
        document.layers[0].vector_scene = Some(Arc::new(scene));
        view.editor = Editor::new(document);
        view.refresh(cx);
        view.command("edit-object", window, cx);
    });
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).canvas_context(cx),
            Some(CanvasContext::VectorDraft)
        )
    });
    click(cx, "canvas-context-duplicate");
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(
            view.editor.document.layers[0]
                .vector_scene
                .as_ref()
                .unwrap()
                .objects
                .len(),
            1
        );
        view.command("undo", window, cx);
        assert!(view.vector_has_redo());
        assert_eq!(view.editor.undo_depth(), 0);
        view.command("redo", window, cx);
    });
    draw(cx);
    view.update(cx, |view, cx| view.apply_vector(cx));
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        assert!(!view.vector_scene_active());
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(
            view.editor.document.layers[0]
                .vector_scene
                .as_ref()
                .unwrap()
                .objects
                .len(),
            2
        );
        view.command("undo", window, cx);
        assert_eq!(
            view.editor.document.layers[0]
                .vector_scene
                .as_ref()
                .unwrap()
                .objects
                .len(),
            1
        );
    });
}
