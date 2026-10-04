//! Focused UI coverage for non-destructive mask inspection.
use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.focus.focus(window, cx);
        view.dialog = Dialog::None;
        view.editor = Editor::new(Document::new(16, 12));
        let id = view.editor.active_layer.clone();
        assert!(view.editor.add_mask(&id, true));
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(1200.), px(800.)));
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn alt_click_mask_badge_toggles_inspection_without_undo(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    draw(cx);
    let selector: &'static str = Box::leak(
        format!(
            "mask-badge-{}",
            cx.update(|_, cx| view.read(cx).editor.active_layer.clone())
        )
        .into_boxed_str(),
    );
    let badge = cx.debug_bounds(selector);
    let badge = badge.expect("mask badge should be exposed in the layers inspector");
    let depth = cx.update(|_, cx| view.read(cx).editor.undo_depth());
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    cx.simulate_mouse_down(badge.center(), MouseButton::Left, alt);
    cx.update(|_, cx| {
        let current = view.read(cx);
        assert!(current.mask_inspection.active());
        assert_eq!(current.editor.undo_depth(), depth);
    });
    cx.simulate_mouse_down(badge.center(), MouseButton::Left, alt);
    cx.update(|_, cx| assert!(!view.read(cx).mask_inspection.active()));
}

#[gpui_kit::test]
fn mask_preview_reuses_tiles_but_editor_replacement_invalidates_cache(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    view.update_in(cx, |view, _, cx| {
        view.mask_inspection.enter(view.editor.active_layer.clone());
        cx.notify();
    });
    draw(cx);
    let first = view.update_in(cx, |view, _, _| {
        view.mask_preview.borrow().as_ref().unwrap().5.snapshot()[0]
            .image
            .clone()
    });
    draw(cx);
    let second = view.update_in(cx, |view, _, _| {
        view.mask_preview.borrow().as_ref().unwrap().5.snapshot()[0]
            .image
            .clone()
    });
    assert!(Arc::ptr_eq(&first, &second));
    view.update_in(cx, |view, _, cx| {
        let document = view.editor.document.clone();
        view.editor = Editor::new(document);
        view.refresh(cx);
    });
    draw(cx);
    let replacement = view.update_in(cx, |view, _, _| {
        view.mask_preview.borrow().as_ref().unwrap().5.snapshot()[0]
            .image
            .clone()
    });
    assert!(!Arc::ptr_eq(&first, &replacement));
}

#[gpui_kit::test]
fn escape_and_mask_removal_leave_inspection_read_only_state(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    view.update_in(cx, |view, _, cx| {
        view.mask_inspection.enter(view.editor.active_layer.clone());
        cx.notify();
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(!view.read(cx).mask_inspection.active()));
    view.update_in(cx, |view, _, cx| {
        let id = view.editor.active_layer.clone();
        view.mask_inspection.enter(id.clone());
        assert!(view.editor.remove_mask(&id, false));
        view.refresh(cx);
    });
    cx.update(|_, cx| assert!(!view.read(cx).mask_inspection.active()));
}

#[gpui_kit::test]
fn transformed_rgba_mask_preview_uses_weighted_alpha_and_outside_coverage(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    view.update_in(cx, |view, _, cx| {
        let id = view.editor.active_layer.clone();
        let layer = view.editor.document.find_layer_mut(&id).unwrap();
        layer.mask = Some(
            image::RgbaImage::from_fn(2, 2, |x, _| {
                if x == 0 {
                    image::Rgba([255, 0, 0, 128])
                } else {
                    image::Rgba([0, 255, 0, 255])
                }
            })
            .into(),
        );
        layer.metadata["maskOutsideCoverage"] = serde_json::json!(255);
        layer.metadata["maskLinked"] = serde_json::json!(false);
        layer.metadata["maskPlacement"] = serde_json::json!({
            "origin": [4, 3], "size": [8, 6], "rotation": 0,
            "flipX": false, "flipY": false
        });
        view.mask_inspection.enter(id);
        view.refresh(cx);
    });
    draw(cx);
    let bytes = view.update_in(cx, |view, _, _| {
        view.mask_preview.borrow().as_ref().unwrap().5.snapshot()[0]
            .image
            .as_bytes(0)
            .unwrap()
            .to_vec()
    });
    // Display tiles have one native pixel of halo on each edge.
    let at =
        |x: usize, y: usize| &bytes[((y + 1) * 18 + x + 1) * 4..((y + 1) * 18 + x + 1) * 4 + 4];
    assert_eq!(at(0, 0), [255, 255, 255, 255]);
    assert_eq!(at(4, 3), [27, 27, 27, 255]);
    assert_eq!(at(11, 8), [182, 182, 182, 255]);
}
