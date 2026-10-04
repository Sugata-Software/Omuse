use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn setup(
    cx: &mut TestAppContext,
    as_selection: bool,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let recovery = tempfile::tempdir().unwrap();
    let directory = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(directory);
        let mut document = Document::new(48, 32);
        document.layers[0].image =
            Some(image::RgbaImage::from_pixel(48, 32, image::Rgba([80, 120, 160, 255])).into());
        view.editor = Editor::new(document);
        view.refresh(cx);
        // Start at the result of local segmentation: this regression exercises
        // the actual refinement worker, modal Apply and keyboard history.
        view.subject_mask = Some(image::GrayImage::from_fn(48, 32, |x, _| {
            image::Luma([[0, 128, 255][x as usize / 16]])
        }));
        view.subject_guide = Some(view.pixels.clone());
        view.subject_layer = Some(view.editor.active_layer.clone());
        view.subject_as_selection = as_selection;
        for input in &view.detail_inputs[..3] {
            input.update(cx, |input, cx| input.set_value("0", window, cx));
        }
        view.dialog = Dialog::SubjectRefine;
        view.modal_focus.focus(window, cx);
        view
    });
    cx.simulate_resize(size(px(1000.), px(800.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, cx, recovery)
}

fn exercise_apply_keyboard_history(cx: &mut TestAppContext, as_selection: bool) {
    let (view, cx, _recovery) = setup(cx, as_selection);
    let (original, expected_mask) = cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.modal_focus.contains_focused(window, cx));
        assert!(!view.focus.is_focused(window));
        (
            view.pixels.clone(),
            view.subject_mask.as_ref().unwrap().as_raw().clone(),
        )
    });
    let apply = cx.debug_bounds("confirm-dialog").unwrap().center();
    cx.simulate_click(apply, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let applied = cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(!view.busy);
        assert!(view.focus.is_focused(window));
        assert_eq!(view.editor.undo_depth(), 1);
        if as_selection {
            assert_eq!(view.editor.selection.as_ref().unwrap().mask, expected_mask);
            assert_eq!(view.pixels, original);
        } else {
            let mask = view.editor.document.layers[0].mask.as_ref().unwrap();
            assert_eq!(
                mask.pixels().map(|p| p[0]).collect::<Vec<_>>(),
                expected_mask
            );
            assert_ne!(view.pixels, original);
        }
        view.pixels.clone()
    });
    // The next interaction is a key chord, with no direct history call,
    // explicit refocus or canvas click to hide lost-focus regressions.
    cx.simulate_keystrokes("ctrl-z");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.pixels, original);
        assert!(view.editor.selection.is_none());
        assert!(view.editor.document.layers[0].mask.is_none());
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.pixels, applied);
        if as_selection {
            assert_eq!(view.editor.selection.as_ref().unwrap().mask, expected_mask);
        } else {
            assert!(view.editor.document.layers[0].mask.is_some());
        }
    });
}

#[gpui_kit::test]
fn refined_subject_selection_apply_allows_immediate_keyboard_undo_redo(cx: &mut TestAppContext) {
    exercise_apply_keyboard_history(cx, true);
}

#[gpui_kit::test]
fn refined_subject_layer_mask_apply_allows_immediate_keyboard_undo_redo(cx: &mut TestAppContext) {
    exercise_apply_keyboard_history(cx, false);
}

#[gpui_kit::test]
fn failed_or_cancelled_refinement_never_steals_focus_from_a_modal(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx, false);
    view.update_in(cx, |view, window, cx| {
        view.detail_inputs[0].update(cx, |input, cx| input.set_value("NaN", window, cx));
        view.run_subject_refine(true, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::SubjectRefine);
        assert!(view.status.contains("failed"), "{}", view.status);
        assert!(view.modal_focus.contains_focused(window, cx));
        assert!(!view.focus.is_focused(window));
        assert_eq!(view.editor.undo_depth(), 0);
    });
    view.update_in(cx, |view, window, cx| {
        view.detail_inputs[0].update(cx, |input, cx| input.set_value("0", window, cx));
        view.run_subject_refine(true, window, cx);
    });
    cx.simulate_keystrokes("escape");
    view.update_in(cx, |view, window, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.focus.is_focused(window));
    });
    view.update_in(cx, |view, window, cx| {
        view.dialog = Dialog::RawImport;
        view.busy = true;
        view.status = "New import owns focus".into();
        view.modal_focus.focus(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::RawImport);
        assert_eq!(view.status, "New import owns focus");
        assert!(view.busy);
        assert!(view.modal_focus.is_focused(window));
        assert!(!view.focus.is_focused(window));
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.document.layers[0].mask.is_none());
    });
}
