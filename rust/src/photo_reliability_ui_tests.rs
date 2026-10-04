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
        view.editor = Editor::new(Document::new(80, 60));
        view.editor.document.layers[0].image = Some(
            image::RgbaImage::from_fn(80, 60, |x, y| {
                image::Rgba([(x * 3) as u8, (y * 4) as u8, ((x + y) % 2 * 200) as u8, 255])
            })
            .into(),
        );
        view.refresh(cx);
        view.focus.focus(window, cx);
        view
    });
    cx.update(|window, _| window.activate_window());
    cx.simulate_resize(size(px(1200.), px(850.)));
    cx.run_until_parked();
    (view, cx, temp)
}

fn text_start(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, w, cx| {
        v.begin_inline_text(None, (5., 5.), None, w, cx)
    });
    cx.simulate_input("A😀éZ");
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        v.font_names = Some(vec![
            "sans-serif".into(),
            "serif".into(),
            "monospace".into(),
        ]);
        v.inline_text
            .as_ref()
            .unwrap()
            .input
            .update(cx, |i, cx| i.set_selected_range(1..7, cx));
    });
}

#[gpui_kit::test]
fn selected_unicode_font_hover_cancel_and_apply_keep_one_undo(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let before = view.update(cx, |v, _| v.pixels.clone());
    text_start(&view, cx);
    view.update_in(cx, |v, w, cx| {
        let original = v.inline_text.as_ref().unwrap().style.clone();
        v.begin_inline_font(w, cx);
        v.preview_inline_font("serif", cx);
        let draft = v.inline_text.as_ref().unwrap();
        assert_eq!(draft.style.content, "A😀éZ");
        assert_eq!((draft.style.runs[0].start, draft.style.runs[0].end), (1, 7));
        assert_eq!(v.editor.undo_depth(), 0);
        v.cancel_inline_font(w, cx);
        assert_eq!(v.inline_text.as_ref().unwrap().style, original);
        assert_eq!(
            v.inline_text
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .selected_range(),
            1..7
        );
        v.begin_inline_font(w, cx);
        v.choose_inline_font("monospace", w, cx);
        let draft = v.inline_text.as_ref().unwrap();
        assert!(draft.input.read(cx).focus_handle(cx).is_focused(w));
        assert_eq!(draft.style.runs[0].font_name.as_deref(), Some("monospace"));
        draft
            .input
            .update(cx, |i, cx| i.set_selected_range(0..8, cx));
        assert_eq!(v.inline_font_label(cx), "Mixed fonts");
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    view.update_in(cx, |v, w, _| {
        assert!(v.inline_text.is_none());
        assert_eq!(v.editor.undo_depth(), 1);
        assert!(v.focus.is_focused(w));
    });
    // Immediate tool shortcut and undo must work without a refocus click.
    cx.simulate_keystrokes("b");
    view.update(cx, |v, _| assert_eq!(v.tool, Tool::Brush));
    cx.simulate_keystrokes("ctrl-z");
    view.update(cx, |v, _| assert_eq!(v.pixels, before));
}

#[gpui_kit::test]
fn font_search_keyboard_commit_caret_typing_and_stale_hover_preserve_text(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    text_start(&view, cx);
    view.update_in(cx, |v, w, cx| v.begin_inline_font(w, cx));
    cx.simulate_input("mono");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    view.update_in(cx, |v, w, cx| {
        let d = v.inline_text.as_ref().unwrap();
        assert!(d.font_edit.is_none());
        assert_eq!(d.style.runs[0].font_name.as_deref(), Some("monospace"));
        d.input.update(cx, |i, cx| i.set_selected_range(8..8, cx));
        v.begin_inline_font(w, cx);
        v.choose_inline_font("serif", w, cx);
    });
    cx.simulate_input("ø");
    cx.run_until_parked();
    view.update_in(cx, |v, w, cx| {
        let d = v.inline_text.as_ref().unwrap();
        assert_eq!(d.style.content, "A😀éZø");
        assert!(
            d.style
                .runs
                .iter()
                .any(|r| r.start == 8 && r.end == 10 && r.font_name.as_deref() == Some("serif"))
        );
        v.begin_inline_font(w, cx);
        v.preview_inline_font("monospace", cx);
        v.inline_text.as_ref().unwrap().input.update(cx, |i, cx| {
            i.set_value("New typing", w, cx);
            i.set_selected_range(2..4, cx);
        });
        v.choose_inline_font("serif", w, cx);
        assert_eq!(v.inline_text.as_ref().unwrap().style.content, "New typing");
        assert_eq!(
            v.inline_text
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .selected_range(),
            2..4
        );
        assert!(v.inline_text.as_ref().unwrap().font_edit.is_none());
    });
}

#[gpui_kit::test]
fn long_stroke_release_duplicate_is_free_and_overflow_is_atomic(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |v, w, cx| {
        v.tool = Tool::BlurBrush;
        v.zoom = 1.;
        v.pan = (0., 0.);
        v.viewport
            .set(Bounds::new(point(px(0.), px(0.)), size(px(80.), px(60.))));
        let before = v.pixels.clone();
        v.drag_start = Some((30., 30.));
        v.lasso = (0..100_000)
            .map(|i| (30. + (i % 2) as f32 * 0.001, 30.))
            .collect();
        let end = *v.lasso.last().unwrap();
        v.up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: point(px(end.0), px(end.1)),
                ..Default::default()
            },
            w,
            cx,
        );
        assert!(!v.status.contains("too many"), "{}", v.status);
        assert_eq!(v.editor.undo_depth(), 1, "{}", v.status);
        if v.editor.undo_depth() > 0 {
            assert!(v.editor.undo());
            v.refresh(cx);
        }
        assert_eq!(v.pixels, before);
        v.drag_start = Some((30., 30.));
        v.lasso = (0..100_000)
            .map(|i| (30. + (i % 2) as f32 * 0.001, 30.))
            .collect();
        v.up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: point(px(40.), px(30.)),
                ..Default::default()
            },
            w,
            cx,
        );
        assert!(v.status.contains("too many"), "{}", v.status);
        assert_eq!(v.pixels, before);
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(v.drag_start.is_none());
        assert!(v.lasso.is_empty());
    });
}

#[gpui_kit::test]
fn blur_radius_native_field_blur_is_independent_of_brush_size(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| {
        v.tool = Tool::BlurBrush;
        v.editor.brush.size = 70.;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let radius = cx
        .debug_bounds("numeric-value-blur-radius")
        .unwrap()
        .center();
    let size = cx
        .debug_bounds("numeric-value-brush-size")
        .unwrap()
        .center();
    cx.simulate_click(radius, Modifiers::default());
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("7.25");
    cx.simulate_click(size, Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |v, _| {
        assert_eq!(v.editor.brush.blur_radius, 7.25);
        assert_eq!(v.editor.brush.size, 70.);
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("100");
    cx.simulate_keystrokes("enter");
    view.update(cx, |v, _| {
        assert_eq!(v.editor.brush.blur_radius, 7.25);
        assert_eq!(v.editor.brush.size, 100.);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn adjustment_apply_then_tool_layer_and_history_shortcuts_need_no_refocus(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let before = view.update_in(cx, |v, w, cx| {
        let before = v.pixels.clone();
        v.dialog = Dialog::Adjustment;
        v.open_adjustment(0, None, w, cx);
        v.detail_inputs[0].update(cx, |i, cx| i.set_value("20", w, cx));
        v.modal_focus.focus(w, cx);
        before
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let apply = cx.debug_bounds("confirm-dialog").unwrap().center();
    cx.simulate_click(apply, Modifiers::default());
    cx.run_until_parked();
    let after = view.update_in(cx, |v, w, _| {
        assert_eq!(v.dialog, Dialog::None);
        assert!(v.focus.is_focused(w));
        assert_eq!(v.editor.undo_depth(), 1);
        assert_ne!(v.pixels, before);
        v.pixels.clone()
    });
    cx.simulate_keystrokes("b");
    view.update(cx, |v, _| assert_eq!(v.tool, Tool::Brush));
    cx.simulate_keystrokes("ctrl-j");
    view.update(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 2);
        assert_eq!(v.editor.undo_depth(), 2);
    });
    cx.simulate_keystrokes("ctrl-z");
    view.update(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 1);
        assert_eq!(v.pixels, after);
    });
    cx.simulate_keystrokes("ctrl-z");
    view.update(cx, |v, _| assert_eq!(v.pixels, before));
}
