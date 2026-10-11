use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use omuse::editor::PreparedRetouch;

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
        view.editor = Editor::new(Document::new(96, 64));
        let pixels = image::RgbaImage::from_fn(96, 64, |x, y| {
            image::Rgba([
                (x * 47 % 256) as u8,
                (y * 71 % 256) as u8,
                ((x + y) * 97 % 256) as u8,
                255,
            ])
        });
        view.editor.document.layers[0].image = Some(pixels.into());
        view.editor.brush.size = 18.;
        view.editor.brush.blur_radius = 3.;
        view.editor.brush.opacity = 0.8;
        view.refresh(cx);
        view.focus.focus(window, cx);
        view
    });
    cx.update(|window, _| window.activate_window());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    (view, cx, temp)
}

fn path() -> Vec<(f32, f32)> {
    vec![(30., 25.), (37., 26.), (44., 28.)]
}

// Hold a completed worker result at the publication boundary. This makes late
// completion tests independent of thread speed and exercises the same handler
// as an actual background job.
fn pending(
    view: &mut EditorView,
    cx: &mut Context<EditorView>,
) -> (Arc<AtomicBool>, (Dialog, u64, u64, u64), PreparedRetouch) {
    let request = view
        .editor
        .prepare_retouch(path(), RetouchMode::Blur, false)
        .unwrap();
    let cancel = view.begin_photo_io(cx).unwrap();
    let identity = (
        view.dialog,
        view.dialog_generation,
        view.create.epoch,
        view.editor.revision(),
    );
    let result = request.compute(&cancel).unwrap();
    view.status = "Blurring pixels… Esc to cancel".into();
    cx.notify();
    (cancel, identity, result)
}

#[gpui_kit::test]
fn mouse_up_uses_worker_and_keeps_exact_raster_and_mask_undo(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    for mask in [false, true] {
        for tool in [Tool::BlurBrush, Tool::Smudge, Tool::Liquify] {
            let (before, expected) = view.update_in(cx, |v, w, cx| {
                if mask && v.editor.document.layers[0].mask.is_none() {
                    v.editor.document.layers[0].mask = Some(
                        image::RgbaImage::from_fn(96, 64, |x, y| {
                            let g = ((x * 37 + y * 53) % 256) as u8;
                            image::Rgba([g, g, g, 255])
                        })
                        .into(),
                    );
                    v.refresh(cx);
                }
                v.paint_mask = mask;
                v.tool = tool;
                v.zoom = 1.;
                v.pan = (0., 0.);
                v.viewport
                    .set(Bounds::new(point(px(0.), px(0.)), size(px(96.), px(64.))));
                let before = v.pixels.clone();
                let mode = match tool {
                    Tool::BlurBrush => RetouchMode::Blur,
                    Tool::Smudge => RetouchMode::Smudge,
                    _ => RetouchMode::Liquify,
                };
                let mut reference = Editor::new(v.editor.document.clone());
                reference.brush = v.editor.brush.clone();
                reference.active_layer = v.editor.active_layer.clone();
                let changed = if mask {
                    reference.retouch_mask_stroke(&reference.active_layer.clone(), &path(), mode)
                } else {
                    reference.retouch_stroke(&path(), mode)
                }
                .unwrap();
                assert!(changed);
                let expected = raster::composite(&reference.document);
                v.drag_start = Some(path()[0]);
                v.lasso = path();
                v.up(
                    &MouseUpEvent {
                        button: MouseButton::Left,
                        position: point(px(44.), px(28.)),
                        ..Default::default()
                    },
                    w,
                    cx,
                );
                assert!(v.busy && v.photo_io.is_some());
                assert_eq!(v.editor.undo_depth(), 0);
                assert_eq!(v.pixels, before);
                assert!(v.lasso.is_empty() && v.drag_start.is_none());
                (before, expected)
            });
            cx.run_until_parked();
            view.update(cx, |v, cx| {
                assert!(!v.busy && v.photo_io.is_none(), "{}", v.status);
                assert_eq!(v.pixels, expected);
                assert_eq!(v.editor.undo_depth(), 1);
                assert!(v.editor.undo());
                v.refresh(cx);
                assert_eq!(v.pixels, before);
                assert!(v.editor.redo());
                v.refresh(cx);
                assert_eq!(v.pixels, expected);
                assert!(v.editor.undo());
                v.refresh(cx);
            });
        }
    }
}

#[gpui_kit::test]
fn escape_discards_completed_retouch_and_keeps_admission_until_exit(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let before = view.update(cx, |v, _| v.pixels.clone());
    let (cancel, identity, result) = view.update(cx, pending);
    cx.simulate_keystrokes("escape");
    view.update_in(cx, |v, w, cx| {
        assert!(!v.busy && v.photo_io.is_some());
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        v.start_background_retouch(path(), RetouchMode::Blur, w, cx);
        assert!(v.status.contains("finishing"));
        assert!(!v.busy);
        let status = v.status.clone();
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert!(v.photo_io.is_none());
        assert_eq!(v.status, status);
        assert_eq!(v.pixels, before);
        assert_eq!(v.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("b");
    view.update(cx, |v, _| assert_eq!(v.tool, Tool::Brush));
}

#[gpui_kit::test]
fn compact_footer_cancel_returns_focus_and_preserves_artwork(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (cancel, identity, result) = view.update(cx, pending);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let bounds = cx
        .debug_bounds("cancel-image-operation")
        .expect("visible cancel action");
    assert!(bounds.right() <= px(800.) && bounds.bottom() <= px(600.));
    cx.simulate_click(bounds.center(), Modifiers::default());
    view.update_in(cx, |v, w, cx| {
        assert!(!v.busy && v.photo_io.is_some());
        assert!(v.focus.is_focused(w));
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(!v.editor.is_dirty());
    });
}

#[gpui_kit::test]
fn escape_cancels_retouch_when_a_numeric_field_has_focus(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| {
        v.tool = Tool::BlurBrush;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let field = cx
        .debug_bounds("numeric-value-blur-radius")
        .unwrap()
        .center();
    cx.simulate_click(field, Modifiers::default());
    view.update_in(cx, |v, w, _| assert!(!v.focus.is_focused(w)));
    let (cancel, identity, result) = view.update(cx, pending);
    cx.simulate_keystrokes("escape");
    view.update_in(cx, |v, w, cx| {
        assert!(!v.busy && v.photo_io.is_some());
        assert!(v.focus.is_focused(w));
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn escape_in_another_window_does_not_cancel_pending_retouch(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (cancel, identity, result) = view.update(cx, pending);
    let mut other_app = cx.cx.clone();
    let (_other, other_cx, _other_temp) = setup(&mut other_app);
    other_cx.simulate_keystrokes("escape");
    view.update(cx, |v, _| {
        assert!(v.busy && v.photo_io.is_some());
        assert!(!cancel.load(std::sync::atomic::Ordering::Relaxed));
    });
    cx.simulate_keystrokes("escape");
    view.update(cx, |v, cx| {
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert!(!v.busy && v.photo_io.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn changed_target_selection_or_replaced_editor_discards_retouch(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    for change in 0..4 {
        let (cancel, identity, result) = view.update(cx, pending);
        view.update(cx, |v, cx| {
            match change {
                0 => v.paint_mask = true,
                1 => v.editor.select_rectangle(0., 0., 10., 10.),
                2 => v.editor = Editor::new(v.editor.document.clone()),
                _ => {
                    v.create.epoch += 1;
                    v.editor = Editor::new(Document::new(12, 9));
                }
            }
            v.refresh(cx);
            let before = v.pixels.clone();
            v.status = "Current page".into();
            v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
            assert!(!v.busy && v.photo_io.is_none());
            assert_eq!(v.pixels, before);
            assert_eq!(v.editor.undo_depth(), 0);
            if change == 3 {
                assert_eq!(v.status, "Current page");
            }
            v.paint_mask = false;
            v.editor.selection = None;
        });
    }
}

#[gpui_kit::test]
fn switching_create_pages_away_and_back_invalidates_pending_retouch(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let ids = view.update(cx, |v, cx| {
        let mut project =
            omuse::create_project::Project::new("Retouch pages", v.editor.document.clone());
        project.add_blank_page("Other page", 96, 64).unwrap();
        let ids = project.page_ids();
        project.set_active_page(&ids[0]).unwrap();
        let doc = project.active_document().unwrap().clone();
        v.install_opened_content(doc, Some(project));
        v.refresh(cx);
        ids
    });
    let (cancel, identity, result) = view.update(cx, pending);
    view.update(cx, |v, cx| {
        let instance = v.editor.instance_id();
        let before = v.pixels.clone();
        v.activate_page(&ids[1], cx).unwrap();
        v.activate_page(&ids[0], cx).unwrap();
        assert_eq!(
            v.editor.instance_id(),
            instance,
            "returned to the cached original editor"
        );
        assert_eq!(v.editor.revision(), identity.3);
        let status = v.status.clone();
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert!(!v.busy && v.photo_io.is_none());
        assert_eq!(v.pixels, before);
        assert_eq!(v.status, status);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn disposing_the_window_cancels_unfinished_retouch(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (cancel, _, result) = view.update(cx, pending);
    let weak = view.downgrade();
    cx.update(|window, _| window.remove_window());
    // Entity destruction is flushed by an app update, not by draining only
    // background tasks. Drop the final test-owned handle in that effect cycle.
    cx.cx.update(|_| drop(view));
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
    drop(result);
}

#[gpui_kit::test]
fn stale_retouch_completion_cannot_clear_newer_busy_operation(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (cancel, identity, result) = view.update(cx, pending);
    view.update(cx, |v, cx| {
        assert!(v.cancel_photo_io());
        v.dialog_generation += 1;
        assert!(!v.finish_photo_io(&cancel, identity));
        let newer = v.begin_photo_io(cx).unwrap();
        v.status = "New image operation".into();
        v.finish_background_retouch(cancel, identity, false, Ok(result), cx);
        assert!(v.busy && v.photo_io.is_some());
        assert_eq!(v.status, "New image operation");
        assert!(!newer.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn failed_and_no_op_workers_leave_history_clean_and_release_slot(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    for points in [vec![(f32::NAN, 10.)], vec![(30., 25.)]] {
        view.update_in(cx, |v, w, cx| {
            v.start_background_retouch(points, RetouchMode::Smudge, w, cx)
        });
        cx.run_until_parked();
        view.update(cx, |v, _| {
            assert!(!v.busy && v.photo_io.is_none());
            assert_eq!(v.editor.undo_depth(), 0);
            assert!(!v.editor.is_dirty());
        });
    }
}
