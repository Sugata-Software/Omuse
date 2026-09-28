use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

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
    let recovery = tempfile::tempdir().unwrap();
    let recovery_dir = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        view.recovery = Recovery::at(recovery_dir);
        let mut document = Document::new(128, 64);
        document.layers[0].image = Some(
            image::RgbaImage::from_fn(128, 64, |x, _| {
                let value = [0, 128, 200, 255][x as usize / 32];
                image::Rgba([value, value, value, 255])
            })
            .into(),
        );
        view.editor = Editor::new(document);
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    click(cx, "inspector-tab-selection");
    (view, cx, recovery)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("Missing control {id}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}
fn complete_preview(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.run_range(false, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.busy, "{}", view.status);
        assert!(
            view.range_draft.as_ref().unwrap().computed.is_some(),
            "{}",
            view.status
        );
    });
}

#[gpui_kit::test]
fn range_minimum_window_has_visible_preview_presets_outputs_and_footer(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    click(cx, "luminosity-range");
    complete_preview(&view, cx);
    for id in [
        "range-source-preview",
        "range-mask-preview",
        "range-shadows",
        "range-value-0",
        "range-value-2",
        "range-invert",
        "range-layer-mask",
        "cancel-dialog",
        "confirm-dialog",
    ] {
        let b = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("Missing {id}"));
        assert!(
            b.origin.x >= px(0.)
                && b.origin.y >= px(0.)
                && b.origin.x + b.size.width <= px(800.)
                && b.origin.y + b.size.height <= px(600.),
            "{id} outside viewport: {b:?}"
        );
    }
    click(cx, "range-shadows");
    complete_preview(&view, cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx)
                .range_draft
                .as_ref()
                .unwrap()
                .computed
                .as_ref()
                .unwrap()
                .1
                .get_pixel(0, 0)[0],
            255
        )
    });
    click(cx, "range-invert");
    complete_preview(&view, cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx)
                .range_draft
                .as_ref()
                .unwrap()
                .computed
                .as_ref()
                .unwrap()
                .1
                .get_pixel(0, 0)[0],
            0
        )
    });
    click(cx, "cancel-dialog");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn range_selection_retains_soft_values_and_keyboard_undo_without_dirtying_pixels(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    let original = cx.update(|_, cx| view.read(cx).pixels.clone());
    click(cx, "luminosity-range");
    complete_preview(&view, cx);
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.focus.is_focused(window));
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(!view.editor.is_dirty());
        assert_eq!(view.pixels, original);
        let mask = &view.editor.selection.as_ref().unwrap().mask;
        assert_eq!(mask[0], 0);
        assert!(mask[40] > 0 && mask[40] < 255);
        assert_eq!(mask[80], 255);
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.update(|_, cx| assert!(view.read(cx).editor.selection.is_none()));
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.update(|_, cx| assert!(view.read(cx).editor.selection.is_some()));
}

#[gpui_kit::test]
fn range_color_sampling_and_add_subtract_affect_only_selection(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| view.editor.select_rectangle(0., 0., 32., 64.));
    click(cx, "color-range");
    let b = cx.update(|_, cx| {
        view.read(cx)
            .range_draft
            .as_ref()
            .unwrap()
            .sample_bounds
            .get()
    });
    cx.simulate_click(
        point(b.origin.x + b.size.width * 0.875, b.center().y),
        Modifiers::default(),
    );
    draw(cx);
    complete_preview(&view, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog_color, [255, 255, 255, 255]));
    click(cx, "range-add");
    click(cx, "confirm-dialog");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let mask = &view.editor.selection.as_ref().unwrap().mask;
        assert_eq!(mask[0], 255);
        assert_eq!(mask[127], 255);
        assert_eq!(mask[40], 0);
    });
    // A fresh white range removes the white end while preserving the black selection.
    view.update(cx, |view, _| view.editor.brush.color = [255, 255, 255, 255]);
    click(cx, "color-range");
    complete_preview(&view, cx);
    click(cx, "range-subtract");
    click(cx, "confirm-dialog");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let mask = &view.editor.selection.as_ref().unwrap().mask;
        assert_eq!(mask[0], 255);
        assert_eq!(mask[127], 0);
        assert_eq!(view.editor.undo_depth(), 2);
        assert!(!view.editor.is_dirty());
    });
}

#[gpui_kit::test]
fn range_layer_mask_apply_is_one_undo_and_keeps_source_pixels(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    let before = cx.update(|_, cx| view.read(cx).editor.document.layers[0].image.clone());
    click(cx, "luminosity-range");
    complete_preview(&view, cx);
    click(cx, "range-layer-mask");
    click(cx, "confirm-dialog");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let layer = &view.editor.document.layers[0];
        assert_eq!(layer.image, before);
        let mask = layer.mask.as_ref().unwrap();
        assert_eq!(mask.get_pixel(0, 0)[0], 0);
        assert!(mask.get_pixel(40, 0)[0] > 0 && mask.get_pixel(40, 0)[0] < 255);
        assert_eq!(mask.get_pixel(127, 0)[0], 255);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.editor.selection.is_none());
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.editor.document.layers[0].mask.is_none());
        assert_eq!(view.editor.document.layers[0].image, before);
        assert!(!view.editor.is_dirty());
    });
}

#[gpui_kit::test]
fn range_cancel_stops_pending_apply_and_late_work_cannot_own_next_dialog(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    click(cx, "luminosity-range");
    let token = view.update(cx, |view, cx| {
        view.run_range(true, cx);
        view.range_draft.as_ref().unwrap().cancel.clone()
    });
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        assert!(view.range_draft.is_none());
        view.dialog = Dialog::CameraRaw;
        view.busy = true;
        view.status = "Newer job owns this state".into();
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(token.load(Ordering::Relaxed));
        assert!(view.busy);
        assert_eq!(view.status, "Newer job owns this state");
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.selection.is_none());
    });
}

#[gpui_kit::test]
fn range_invalid_or_changed_canvas_is_never_applied(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    click(cx, "luminosity-range");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.detail_inputs[0].update(cx, |input, cx| input.set_value("NaN", window, cx));
        })
    });
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::RangeMask);
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.selection.is_none());
    });
    click(cx, "range-highlights");
    complete_preview(&view, cx);
    view.update(cx, |view, _| {
        view.pixels.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
    });
    click(cx, "confirm-dialog");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::RangeMask);
        assert!(view.status.contains("canvas changed"));
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn range_dirty_window_close_cancels_work_and_declining_close_keeps_editor_usable(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| view.editor.mark_unsaved());
    click(cx, "luminosity-range");
    let token = view.update(cx, |view, cx| {
        view.run_range(true, cx);
        assert!(view.busy);
        view.range_draft.as_ref().unwrap().cancel.clone()
    });
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::Unsaved);
        assert!(view.range_draft.is_none());
        assert!(!view.busy);
        assert!(token.load(Ordering::Relaxed));
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.selection.is_none());
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.focus.is_focused(window));
        assert!(!view.busy);
    });
    click(cx, "luminosity-range");
    complete_preview(&view, cx);
    cx.simulate_keystrokes("escape");
}
