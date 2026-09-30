use super::*;
use gpui_kit::{Focusable, TestAppContext, VisualTestContext};

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(crate::init_test_theme);
    cx.update(|cx| install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx));
    let temp = tempfile::tempdir().unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.editor = Editor::new(Document::new(320, 180));
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    (view, cx, temp)
}
fn displayed(view: &EditorView) -> Vec<Vec<u8>> {
    view.display
        .snapshot()
        .iter()
        .map(|tile| tile.image.as_bytes(0).unwrap().to_vec())
        .collect()
}
fn begin(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.begin_inline_text(None, (12., 20.), None, window, cx)
    });
    cx.simulate_input("A😀éZ");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn live_text_upgrade_preview_matches_apply_without_early_history(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let original = cx.update(|_, cx| view.read(cx).pixels.clone());
    begin(&view, cx);
    let preview = cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(v.pixels, original);
        assert!(!v.editor.is_dirty());
        displayed(v)
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(v.inline_text.is_none());
        assert_eq!(v.editor.undo_depth(), 1);
        assert_eq!(displayed(v), preview);
        assert_ne!(v.pixels, original);
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.update(|_, cx| assert_eq!(view.read(cx).pixels, original));
}

#[gpui_kit::test]
fn live_text_upgrade_picker_cancel_restores_exact_ranges_and_commit_keeps_selection(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    begin(&view, cx);
    let original = view.update_in(cx, |view, window, cx| {
        let draft = view.inline_text.as_mut().unwrap();
        draft
            .input
            .update(cx, |input, cx| input.set_selected_range(1..5, cx));
        let original = draft.style.clone();
        draft
            .color
            .update(cx, |picker, cx| picker.set_open(true, cx));
        view.inline_color_changed(window, cx);
        original
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        let color = view.inline_text.as_ref().unwrap().color.clone();
        color.update(cx, |picker, cx| {
            assert!(picker.preview_hex("#ef703a", window, cx));
        });
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_ne!(view.inline_text.as_ref().unwrap().style, original);
        view.inline_text
            .as_ref()
            .unwrap()
            .color
            .update(cx, |picker, cx| picker.set_open(false, cx));
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        let draft = view.inline_text.as_ref().unwrap();
        assert_eq!(draft.style, original);
        assert_eq!(draft.input.read(cx).selected_range(), 1..5);
        assert!(draft.input.read(cx).focus_handle(cx).is_focused(window));
        draft
            .color
            .update(cx, |picker, cx| picker.set_open(true, cx));
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.inline_text
            .as_ref()
            .unwrap()
            .color
            .update(cx, |picker, cx| {
                picker.commit_hex("#ef703a", window, cx).unwrap();
            });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let v = view.read(cx);
        let draft = v.inline_text.as_ref().unwrap();
        assert_eq!(draft.style.runs.len(), 1);
        assert_eq!((draft.style.runs[0].start, draft.style.runs[0].end), (1, 5));
        assert_eq!(draft.input.read(cx).selected_range(), 1..5);
        assert!(draft.input.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(v.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.update(|_, cx| {
        let v = view.read(cx);
        let style = objects::live_text(
            v.editor
                .document
                .find_layer(&v.editor.active_layer)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(style.runs.len(), 1);
        assert_eq!(v.editor.undo_depth(), 1);
    });
}

#[gpui_kit::test]
fn live_text_upgrade_cancel_rejects_stale_worker_and_quit_commits_before_prompt(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    let original = cx.update(|_, cx| displayed(view.read(cx)));
    view.update_in(cx, |view, window, cx| {
        view.begin_inline_text(None, (8., 8.), None, window, cx);
        let input = view.inline_text.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| {
            input.set_value("Cancelled artwork", window, cx)
        });
        view.sync_inline_text(cx).unwrap();
        view.schedule_inline_preview(cx);
        assert!(view.finish_inline_text(false, window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(displayed(v), original);
        assert!(!v.editor.is_dirty());
    });
    begin(&view, cx);
    cx.simulate_keystrokes("ctrl-q");
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(v.inline_text.is_none());
        assert_eq!(v.dialog, Dialog::Unsaved);
        assert!(matches!(v.pending, Some(Pending::Quit)));
        assert_eq!(v.editor.undo_depth(), 1);
    });
}

#[gpui_kit::test]
fn live_text_upgrade_typing_and_native_undo_preserve_unicode_styles(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    begin(&view, cx);
    view.update_in(cx, |view, window, cx| {
        let draft = view.inline_text.as_mut().unwrap();
        objects::apply_rich_text_patch(
            &mut draft.style,
            1..7,
            objects::RichTextPatch {
                color: Some([0.8, 0.2, 0.3, 1.]),
                ..Default::default()
            },
        )
        .unwrap();
        draft.input.update(cx, |input, cx| {
            input.set_selected_range(5..5, cx);
            input.focus(window, cx);
        });
    });
    cx.run_until_parked();
    cx.simulate_input("東京");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let draft = view.read(cx).inline_text.as_ref().unwrap();
        assert_eq!(draft.style.content, "A😀東京éZ");
        assert_eq!(
            (draft.style.runs[0].start, draft.style.runs[0].end),
            (1, 13)
        );
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let draft = view.read(cx).inline_text.as_ref().unwrap();
        assert_eq!(draft.style.content, "A😀éZ");
        assert_eq!((draft.style.runs[0].start, draft.style.runs[0].end), (1, 7));
    });
}

#[gpui_kit::test]
fn live_text_upgrade_placed_mask_rotation_and_blend_match_committed_pixels(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    let original = view.update_in(cx, |view, window, cx| {
        let mut document = Document::new(320, 180);
        document.layers[0].image =
            Some(image::RgbaImage::from_pixel(320, 180, image::Rgba([40, 80, 120, 255])).into());
        let mut layer = objects::live_text_layer(
            "Placed text",
            objects::ObjectPoint { x: 24., y: 28. },
            objects::LiveTextStyle {
                content: "Styled original".into(),
                font_size: 28.,
                red: 1.,
                green: 0.5,
                blue: 0.2,
                ..Default::default()
            },
        )
        .unwrap();
        layer.rotation = 11.;
        layer.opacity = 0.65;
        layer.blend_mode = "Screen".into();
        layer.mask = Some(
            image::RgbaImage::from_fn(8, 4, |x, _| {
                image::Rgba([
                    if x < 4 { 90 } else { 255 },
                    if x < 4 { 90 } else { 255 },
                    if x < 4 { 90 } else { 255 },
                    255,
                ])
            })
            .into(),
        );
        layer.metadata["maskLinked"] = serde_json::json!(false);
        layer.metadata["maskPlacement"] =
            serde_json::json!({"origin":[20,24],"size":[150,85],"rotation":8,"sampling":"Smooth"});
        let id = layer.id.clone();
        document.layers.push(layer);
        view.editor = Editor::new(document);
        view.editor.active_layer = id.clone();
        view.refresh(cx);
        let original = view.pixels.clone();
        view.begin_inline_text(Some(id), (24., 28.), None, window, cx);
        let input = view.inline_text.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| {
            input.set_value("Styled replacement", window, cx)
        });
        original
    });
    cx.run_until_parked();
    let preview = cx.update(|_, cx| displayed(view.read(cx)));
    view.update_in(cx, |view, window, cx| {
        assert!(view.finish_inline_text(true, window, cx))
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(displayed(view), preview);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.editor.undo());
        view.refresh(cx);
        assert_eq!(view.pixels, original);
    });
}

#[gpui_kit::test]
fn live_text_upgrade_retyping_old_content_cannot_resurrect_old_colours(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.begin_inline_text(None, (8., 8.), None, window, cx)
    });
    cx.simulate_input("A");
    cx.run_until_parked();
    cx.simulate_input("B");
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| {
        let draft = v.inline_text.as_mut().unwrap();
        objects::apply_rich_text_patch(
            &mut draft.style,
            0..1,
            objects::RichTextPatch {
                color: Some([1., 0., 0., 1.]),
                ..Default::default()
            },
        )
        .unwrap();
        draft.input.update(cx, |input, cx| {
            input.set_selected_range(2..2, cx);
            input.focus(window, cx);
        });
    });
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let draft = view.read(cx).inline_text.as_ref().unwrap();
        assert_eq!(draft.style.content, "A");
        assert_eq!(draft.style.runs[0].color, Some([1., 0., 0., 1.]));
    });
}
