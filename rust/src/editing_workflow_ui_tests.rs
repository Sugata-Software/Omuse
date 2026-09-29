use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn photo() -> Document {
    let mut d = Document::new(120, 80);
    d.layers[0].image = Some(
        image::RgbaImage::from_fn(120, 80, |x, y| image::Rgba([x as u8, y as u8, 71, 255])).into(),
    );
    d
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| w.draw(cx).clear(cx));
}
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("Missing {selector}"));
    assert!(
        b.right() <= px(800.) && b.bottom() <= px(600.),
        "{selector} offscreen: {b:?}"
    );
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
}
fn screen(v: &EditorView, p: (f32, f32)) -> Point<Pixels> {
    let b = v.viewport.get();
    point(
        b.center().x + px(v.pan.0 + (p.0 - v.editor.document.width as f32 * 0.5) * v.zoom),
        b.center().y + px(v.pan.1 + (p.1 - v.editor.document.height as f32 * 0.5) * v.zoom),
    )
}

#[gpui_kit::test]
fn crop_controls_pointer_keyboard_apply_and_undo_preserve_source(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        v.editor = Editor::new(photo());
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    view.update_in(cx, |v, w, cx| v.command("fit", w, cx));
    draw(cx);
    let original = view.read_with(cx, |v, _| v.pixels.clone());
    cx.simulate_keystrokes("h c");
    draw(cx);
    click(cx, "crop-ratio-3");
    let (from, to) = view.read_with(cx, |v, _| {
        let c = v.crop.as_ref().unwrap();
        assert_eq!(c.preset, 3);
        assert_eq!(v.pixels, original);
        assert!(!v.editor.is_dirty());
        (
            screen(v, (c.rect.x + c.rect.width, c.rect.y + c.rect.height)),
            screen(v, (c.rect.x + 48., c.rect.y + 60.)),
        )
    });
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_keystrokes("right shift-down");
    draw(cx);
    let rect = view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document.width, 120);
        let rect = v.crop.as_ref().unwrap().pixels();
        assert_eq!(rect, (29, 10, 48, 60));
        rect
    });
    click(cx, "crop-apply");
    view.read_with(cx, |v, _| {
        assert!(v.crop.is_none());
        assert_eq!(
            v.pixels,
            image::imageops::crop_imm(&original, rect.0 as u32, rect.1 as u32, rect.2, rect.3)
                .to_image()
        );
        assert_eq!(
            v.editor.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .dimensions(),
            (120, 80)
        );
    });
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    view.read_with(cx, |v, _| assert_eq!(v.pixels, original));
    cx.simulate_keystrokes("ctrl-shift-z");
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            (v.editor.document.width, v.editor.document.height),
            (48, 60)
        )
    });
}

#[gpui_kit::test]
fn crop_cancel_and_selection_seed_have_no_artwork_or_history_side_effect(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        v.editor = Editor::new(photo());
        v.editor.select_rect(20, 10, 60, 40);
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    let before = view.read_with(cx, |v, _| {
        (
            v.pixels.clone(),
            v.editor.selection.clone(),
            v.editor.revision(),
        )
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("crop");
    cx.simulate_keystrokes("enter");
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.crop.as_ref().unwrap().pixels(), (20, 10, 60, 40))
    });
    click(cx, "crop-ratio-5");
    click(cx, "crop-swap");
    cx.simulate_keystrokes("ctrl-j ctrl-z ctrl-s ctrl-alt-shift-s");
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.crop.is_some()));
    cx.simulate_keystrokes("escape");
    draw(cx);
    view.read_with(cx, |v, _| {
        assert!(v.crop.is_none());
        assert_eq!(
            (
                v.pixels.clone(),
                v.editor.selection.clone(),
                v.editor.revision()
            ),
            before
        );
    });
    cx.simulate_keystrokes("c");
    draw(cx);
    click(cx, "crop-ratio-2");
    click(cx, "crop-cancel");
    view.read_with(cx, |v, _| {
        assert_eq!(
            (
                v.pixels.clone(),
                v.editor.selection.clone(),
                v.editor.revision()
            ),
            before
        )
    });
}

#[gpui_kit::test]
fn keyboard_zoom_and_actual_keep_the_same_canvas_point(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        v.editor = Editor::new(photo());
        v.zoom = 0.5;
        v.pan = (37., -29.);
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    let anchor = view.read_with(cx, |v, _| v.coordinates(v.viewport.get().center()));
    for key in ["ctrl-=", "ctrl--", "ctrl-1"] {
        cx.simulate_keystrokes(key);
        draw(cx);
        view.read_with(cx, |v, _| {
            let p = v.coordinates(v.viewport.get().center());
            assert!((p.0 - anchor.0).abs() < 0.001 && (p.1 - anchor.1).abs() < 0.001);
            assert!(!v.editor.is_dirty());
        });
    }
    view.update(cx, |v, _| {
        let p = v.viewport.get().center() + point(px(35.), px(-28.));
        let before = v.coordinates(p);
        v.set_zoom(3.7, Some(p));
        let after = v.coordinates(p);
        assert!((before.0 - after.0).abs() < 0.001 && (before.1 - after.1).abs() < 0.001);
    });
}

#[gpui_kit::test]
fn clipboard_keeps_groups_in_session_but_external_identical_png_is_pixels(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        let mut doc = photo();
        let mut group = Layer::group("Product");
        group.children = doc.layers;
        doc.layers = vec![group];
        v.editor = Editor::new(doc);
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    cx.simulate_keystrokes("ctrl-c");
    cx.run_until_parked();
    let item = cx.update(|_, cx| cx.read_from_clipboard().unwrap());
    assert!(
        item.entries
            .iter()
            .any(|e| matches!(e, ClipboardEntry::Image(_)))
    );
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 2);
        assert_eq!(v.editor.document.layers[1].name, "Product");
        assert_eq!(v.editor.document.layers[1].children.len(), 1);
        assert_ne!(
            v.editor.document.layers[0].children[0].id,
            v.editor.document.layers[1].children[0].id
        );
    });
    cx.simulate_keystrokes("ctrl-z");
    draw(cx);
    let external = ClipboardItem {
        entries: item
            .entries
            .into_iter()
            .filter(|e| matches!(e, ClipboardEntry::Image(_)))
            .collect(),
    };
    cx.update(|_, cx| cx.write_to_clipboard(external));
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 2);
        assert!(v.editor.document.layers[1].children.is_empty());
        assert!(v.editor.document.layers[1].image.is_some());
    });
}

#[gpui_kit::test]
fn locked_cut_does_not_replace_clipboard_or_remove_artwork(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        let mut doc = photo();
        doc.layers[0].locked = true;
        v.editor = Editor::new(doc);
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    let prior = ClipboardItem::new_string("Keep my clipboard".into());
    cx.update(|_, cx| cx.write_to_clipboard(prior.clone()));
    cx.simulate_keystrokes("ctrl-x");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard(), Some(prior)));
    view.read_with(cx, |v, _| {
        assert!(!v.editor.is_dirty());
        assert_eq!(v.editor.document.layers.len(), 1);
    });
}

#[gpui_kit::test]
fn delayed_paste_does_not_interrupt_gestures_or_erase_a_newer_rich_copy(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let mut v = EditorView::new(None, w, cx);
        v.dialog = Dialog::None;
        let mut doc = photo();
        let mut g = Layer::group("Editable group");
        g.children = doc.layers;
        doc.layers = vec![g];
        v.editor = Editor::new(doc);
        v.refresh(cx);
        v.focus.focus(w, cx);
        v
    });
    view.update(cx, |v, cx| {
        v.copy(false, cx);
        v.paste(cx);
        // A gesture has begun but has not committed a document revision yet.
        v.drag_start = Some((10., 10.));
    });
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        assert_eq!(v.editor.document.layers.len(), 1);
        assert!(!v.editor.is_dirty());
        v.drag_start = None;
        v.paste(cx);
        // Copy B replaces copy A while A's clipboard read is still pending.
        v.copy(false, cx);
    });
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        assert_eq!(v.editor.document.layers.len(), 1);
        v.paste(cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 2);
        assert_eq!(v.editor.document.layers[1].children.len(), 1);
    });
}
