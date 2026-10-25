use super::*;
use gpui_kit::{TestAppContext, VisualTestContext};

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
        let mut v = EditorView::new(None, window, cx);
        v.dialog = Dialog::None;
        v.recovery = Recovery::at(temp.path().join("recovery"));
        v.editor = Editor::new(Document::new(320, 180));
        v.refresh(cx);
        v
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    view.update_in(cx, |v, w, cx| {
        v.begin_inline_text(None, (8., 8.), None, w, cx)
    });
    cx.simulate_input("A😀éZ");
    cx.run_until_parked();
    view.update_in(cx, |v, w, cx| {
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
        v.begin_inline_font(w, cx);
    });
    (view, cx, temp)
}

#[gpui_kit::test]
fn audition_captures_unicode_selection_and_cancel_restores_without_history(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    let (original, history_before) = view.update(cx, |v, _| {
        let draft = v.inline_text.as_ref().unwrap();
        (draft.style.clone(), draft.history.len())
    });
    view.update(cx, |v, cx| {
        v.compare_inline_font("serif", cx);
        v.compare_inline_font("monospace", cx);
        let draft = v.inline_text.as_ref().unwrap();
        let edit = draft.font_edit.as_ref().unwrap();
        assert_eq!(edit.comparisons.len(), 2);
        assert!(
            edit.comparisons
                .iter()
                .all(|s| s.sample == "😀é" && !s.truncated && !s.families.is_empty())
        );
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(draft.history.len(), history_before);
    });
    view.update_in(cx, |v, w, cx| {
        assert!(v.cancel_inline_font(w, cx));
        let draft = v.inline_text.as_ref().unwrap();
        assert_eq!(draft.style, original);
        assert_eq!(draft.input.read(cx).selected_range(), 1..7);
        assert_eq!(draft.history.len(), history_before);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn audition_keep_uses_one_text_transaction_and_one_document_undo(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (before, history_before) = view.update(cx, |v, _| {
        (
            v.pixels.clone(),
            v.inline_text.as_ref().unwrap().history.len(),
        )
    });
    view.update_in(cx, |v, w, cx| {
        v.compare_inline_font("serif", cx);
        v.compare_inline_font("monospace", cx);
        v.choose_inline_font("serif", w, cx);
        let draft = v.inline_text.as_ref().unwrap();
        assert!(draft.font_edit.is_none());
        assert_eq!(draft.history.len(), history_before + 1);
        assert_eq!(draft.input.read(cx).selected_range(), 1..7);
        assert_eq!(draft.style.content, "A😀éZ");
        assert_eq!(draft.style.runs[0].font_name.as_deref(), Some("serif"));
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    view.update(cx, |v, _| {
        assert!(v.inline_text.is_none());
        assert_eq!(v.editor.undo_depth(), 1);
    });
    cx.simulate_keystrokes("ctrl-z");
    view.update(cx, |v, _| assert_eq!(v.pixels, before));
}

#[gpui_kit::test]
fn audition_is_bounded_installed_only_and_stale_content_cannot_commit(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let history_before = view.update(cx, |v, _| v.inline_text.as_ref().unwrap().history.len());
    view.update_in(cx, |v, w, cx| {
        v.compare_inline_font("Definitely not installed", cx);
        assert!(
            v.inline_text
                .as_ref()
                .unwrap()
                .font_edit
                .as_ref()
                .unwrap()
                .comparisons
                .is_empty()
        );
        for font in ["serif", "monospace", "sans-serif", "serif"] {
            v.compare_inline_font(font, cx);
        }
        assert_eq!(
            v.inline_text
                .as_ref()
                .unwrap()
                .font_edit
                .as_ref()
                .unwrap()
                .comparisons
                .len(),
            3
        );
        v.inline_text.as_ref().unwrap().input.update(cx, |i, cx| {
            i.set_value("New Ω text", w, cx);
            i.set_selected_range(4..6, cx);
        });
        v.choose_inline_font("serif", w, cx);
        let draft = v.inline_text.as_ref().unwrap();
        assert!(draft.font_edit.is_none());
        assert_eq!(draft.style.content, "New Ω text");
        assert_eq!(draft.input.read(cx).selected_range(), 4..6);
        assert_eq!(
            draft.history.len(),
            history_before + 1,
            "only the new text content adds history"
        );
    });
}

#[gpui_kit::test]
fn audition_refuses_changed_document_identity_and_labels_truncated_samples(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |v, w, cx| {
        v.cancel_inline_font(w, cx);
        let content = "é".repeat(200);
        v.inline_text.as_ref().unwrap().input.update(cx, |i, cx| {
            i.set_value(content, w, cx);
            i.set_selected_range(0..400, cx);
        });
        v.begin_inline_font(w, cx);
        v.compare_inline_font("serif", cx);
        let edit = v.inline_text.as_ref().unwrap().font_edit.as_ref().unwrap();
        assert!(edit.comparisons[0].truncated);
        assert_eq!(edit.comparisons[0].sample.chars().count(), 160);
        v.editor = Editor::new(Document::new(320, 180));
        v.compare_inline_font("monospace", cx);
        assert_eq!(
            v.inline_text
                .as_ref()
                .unwrap()
                .font_edit
                .as_ref()
                .unwrap()
                .comparisons
                .len(),
            1
        );
        v.choose_inline_font("serif", w, cx);
        assert!(v.inline_text.as_ref().unwrap().font_edit.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn typing_invalidates_audition_and_evicts_comparison_images_even_with_external_arcs(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    let image = view.update(cx, |v, cx| {
        v.compare_inline_font("serif", cx);
        v.inline_text
            .as_ref()
            .unwrap()
            .font_edit
            .as_ref()
            .unwrap()
            .comparisons[0]
            .image
            .clone()
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        assert!(
            window.has_image_atlas_entry(&image),
            "comparison must actually paint before eviction is tested"
        )
    });
    view.update_in(cx, |v, w, cx| {
        v.inline_text
            .as_ref()
            .unwrap()
            .input
            .update(cx, |input, cx| {
                input.set_value("New Ω content", w, cx);
                input.set_selected_range(4..6, cx);
            });
        v.sync_inline_text(cx).unwrap();
        let d = v.inline_text.as_ref().unwrap();
        assert!(d.font_edit.is_none());
        assert_eq!(d.style.content, "New Ω content");
        assert_eq!(d.input.read(cx).selected_range(), 4..6);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(!window.has_image_atlas_entry(&image));
    });
}
