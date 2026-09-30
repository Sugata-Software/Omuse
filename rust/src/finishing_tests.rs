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
    let recovery = tempfile::tempdir().unwrap();
    let path = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        view.recovery = Recovery::at(path);
        let mut document = Document::new(48, 32);
        document.layers[0].image = Some(
            image::RgbaImage::from_fn(48, 32, |x, y| {
                image::Rgba([
                    (x * 5) as u8,
                    (y * 7) as u8,
                    104,
                    if x % 7 == 0 { 128 } else { 255 },
                ])
            })
            .into(),
        );
        view.editor = Editor::new(document);
        view.focus.focus(window, cx);
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    (view, cx, recovery)
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn complete(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.run_finishing(false, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.busy, "{}", view.status);
        assert!(
            view.finishing_draft.as_ref().unwrap().computed.is_some(),
            "{}",
            view.status
        );
    });
}

#[gpui_kit::test]
fn finishing_search_opens_cancel_discards_preview_and_footer_fits(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    let before = cx.update(|_, cx| view.read(cx).pixels.clone());
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("dither");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Finishing));
    complete(&view, cx);
    for id in [
        "dialog",
        "finishing-artwork-preview",
        "cancel-dialog",
        "confirm-dialog",
    ] {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("Missing {id}"));
        assert!(
            bounds.origin.x >= px(0.)
                && bounds.origin.y >= px(0.)
                && bounds.bottom_right().x <= px(800.)
                && bounds.bottom_right().y <= px(600.),
            "{id}: {bounds:?}"
        );
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.finishing_draft.is_none());
        assert_eq!(view.pixels, before);
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn finishing_apply_matches_preview_pixels_respects_selection_and_is_one_undo(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    let before = cx.update(|_, cx| view.read(cx).pixels.clone());
    view.update_in(cx, |view, window, cx| {
        view.editor.selection = Some(Selection {
            width: 48,
            height: 32,
            mask: (0..48 * 32)
                .map(|i| {
                    if i % 48 < 16 {
                        0
                    } else if i % 48 < 32 {
                        128
                    } else {
                        255
                    }
                })
                .collect(),
        });
        view.command("dither", window, cx);
    });
    complete(&view, cx);
    let expected = view.update(cx, |view, _| {
        let draft = view.finishing_draft.as_ref().unwrap();
        let mut editor = Editor::new(view.editor.document.clone());
        editor.active_layer = draft.layer.clone();
        editor.selection = draft.selection.clone();
        editor
            .apply_image_operation(|_| Ok((*draft.computed.as_ref().unwrap().1).clone()))
            .unwrap();
        raster::composite(&editor.document)
    });
    view.update_in(cx, |view, window, cx| view.confirm_dialog(window, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert_eq!(view.pixels, expected);
        assert_eq!(view.editor.undo_depth(), 1);
        for y in 0..32 {
            for x in 0..16 {
                assert_eq!(view.pixels.get_pixel(x, y), before.get_pixel(x, y));
            }
        }
    });
    view.update(cx, |view, cx| {
        assert!(view.editor.undo());
        view.refresh(cx);
    });
    cx.update(|_, cx| assert_eq!(view.read(cx).pixels, before));
}

#[gpui_kit::test]
fn finishing_rejects_changed_selection_and_document_revision(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update_in(cx, |view, window, cx| view.open_finishing(1, window, cx));
    complete(&view, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor.selection = Some(Selection {
            width: 48,
            height: 32,
            mask: vec![255; 48 * 32],
        });
        view.confirm_dialog(window, cx);
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.dialog, Dialog::Finishing);
        assert!(view.status.contains("changed"));
        view.cancel_finishing(cx);
        view.dialog = Dialog::None;
        view.open_finishing(1, window, cx);
    });
    complete(&view, cx);
    view.update_in(cx, |view, window, cx| {
        let id = view.editor.active_layer.clone();
        assert!(view.editor.set_locked(&id, true));
        let undo = view.editor.undo_depth();
        view.confirm_dialog(window, cx);
        assert_eq!(view.editor.undo_depth(), undo);
        assert_eq!(view.dialog, Dialog::Finishing);
        assert!(view.status.contains("changed"));
    });
}

#[gpui_kit::test]
fn finishing_invalid_input_cannot_apply_old_preview_and_modes_remain_available(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    view.update_in(cx, |view, window, cx| view.open_finishing(0, window, cx));
    complete(&view, cx);
    view.update_in(cx, |view, window, cx| {
        let input = view.finishing_draft.as_ref().unwrap().inputs[0].clone();
        input.update(cx, |input, cx| input.set_value("1.5", window, cx));
        view.confirm_dialog(window, cx);
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.dialog, Dialog::Finishing);
        assert!(view.status.contains("whole numbers"));
    });
    for kind in 1..4 {
        view.update_in(cx, |view, window, cx| view.open_finishing(kind, window, cx));
        complete(&view, cx);
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.undo_depth(), 0));
    }
}

#[gpui_kit::test]
fn finishing_cancel_inflight_cannot_resurrect_a_draft(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update_in(cx, |view, window, cx| {
        view.open_finishing(0, window, cx);
        view.run_finishing(true, cx);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.finishing_draft.is_none());
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(!view.busy);
    });
}
