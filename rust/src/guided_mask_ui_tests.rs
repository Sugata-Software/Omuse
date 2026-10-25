//! Synthetic review-control regressions only. These do not qualify model/photo quality.
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
    let temp = tempfile::tempdir().unwrap();
    let recovery = temp.path().join("recovery");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = EditorView::new(None, window, cx);
        v.dialog = Dialog::None;
        v.recovery = Recovery::at(recovery);
        let mut doc = Document::new(128, 96);
        doc.layers[0].image = Some(
            image::RgbaImage::from_fn(128, 96, |x, _| {
                image::Rgba(if x < 64 {
                    [180, 70, 30, 255]
                } else {
                    [30, 90, 180, 255]
                })
            })
            .into(),
        );
        v.editor = Editor::new(doc);
        v.refresh(cx);
        v.editor.select_rect(9, 11, 20, 15);
        v.open_guided_mask(window, cx);
        v
    });
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    (view, cx, temp)
}
fn settle_preview(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn seed_review(v: &mut EditorView, cx: &mut Context<EditorView>) {
    let d = v.guided_mask.draft.as_mut().unwrap();
    d.review
        .add_prompt(Prompt::Positive { x: 32., y: 48. })
        .unwrap();
    let prediction = guided_mask::Prediction {
        source: d.review.source.clone(),
        alternatives: vec![guided_mask::Alternative {
            mask: Arc::new(image::GrayImage::from_fn(128, 96, |x, _| {
                image::Luma([if x < 64 { 255 } else { 0 }])
            })),
            estimated_iou: 0.93,
        }],
        embedding_reused: false,
        initial_mask: Arc::new(image::GrayImage::from_fn(128, 96, |x, _| {
            image::Luma([if x < 64 { 255 } else { 0 }])
        })),
    };
    let pixels =
        candidate_miniatures(&d.review.pixels, &prediction.alternatives, || false).unwrap();
    d.review.accept(d.review.generation, prediction).unwrap();
    v.replace_guided_miniatures(pixels, cx);
    v.paint_guided_mask(cx);
}
#[gpui_kit::test]
fn guided_cancel_keeps_source_selection_and_history_unchanged(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let (original, selection_before) = view.update(cx, |v, cx| {
        seed_review(v, cx);
        (v.pixels.clone(), v.editor.selection.clone())
    });
    view.update(cx, |v, _| {
        assert!(v.guided_mask_active());
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(v.editor.selection, selection_before);
        assert_eq!(v.pixels, original);
    });
    cx.simulate_keystrokes("escape");
    settle_preview(cx);
    cx.update(|window, cx| {
        let v = view.read(cx);
        assert!(!v.guided_mask_active());
        assert_eq!(v.pixels, original);
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(v.editor.selection, selection_before);
        assert!(v.focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn use_as_selection_preserves_pixels_and_mask_without_document_history(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    let original = view.update(cx, |v, cx| {
        seed_review(v, cx);
        v.editor.document.layers[0].image.clone().unwrap()
    });
    settle_preview(cx);
    view.update_in(cx, |v, w, cx| {
        v.use_guided_mask_as_selection(w, cx);
    });
    settle_preview(cx);
    view.update(cx, |v, _| {
        assert!(!v.guided_mask_active());
        assert_eq!(
            v.editor.document.layers[0].image.as_ref().unwrap(),
            &original
        );
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 1);
        assert_eq!(
            v.editor.selection.as_ref().unwrap().bounds(),
            Some((0, 0, 64, 96))
        );
    });
    cx.simulate_keystrokes("ctrl-d");
    view.update(cx, |v, _| assert!(v.editor.selection.is_none()));
}
#[gpui_kit::test]
fn guided_keep_is_editable_one_keyboard_undo_and_saved_mask_needs_no_model(
    cx: &mut TestAppContext,
) {
    let (view, cx, temp) = setup(cx);
    let original = view.update(cx, |v, cx| {
        seed_review(v, cx);
        v.editor.document.layers[0].image.clone().unwrap()
    });
    settle_preview(cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let keep = cx.debug_bounds("guided-keep").unwrap().center();
    cx.simulate_click(keep, Modifiers::default());
    settle_preview(cx);
    let path = temp.path().join("guided.omuse");
    cx.update(|window, cx| {
        let v = view.read(cx);
        assert!(!v.guided_mask_active());
        assert_eq!(v.editor.undo_depth(), 1);
        assert_eq!(
            v.editor.document.layers[0].image.as_ref().unwrap(),
            &original
        );
        assert!(v.focus.is_focused(window));
        omuse::document::save(&v.editor.document, &path).unwrap();
    });
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(reopened.layers[0].image.as_ref().unwrap(), &original);
    let mask = reopened.layers[0].mask.as_ref().unwrap();
    assert_eq!(mask.get_pixel(20, 20)[0], 255);
    assert_eq!(mask.get_pixel(100, 20)[0], 0);
    cx.simulate_keystrokes("ctrl-z");
    view.update(cx, |v, _| {
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(
            v.editor.document.layers[0].image.as_ref().unwrap(),
            &original
        );
        assert_eq!(v.editor.undo_depth(), 0);
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    view.update(cx, |v, _| {
        assert!(v.editor.document.layers[0].mask.is_some())
    });
}
#[gpui_kit::test]
fn changed_editor_identity_cannot_keep_a_ready_mask(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update_in(cx, |v, w, cx| {
        seed_review(v, cx);
        v.editor = Editor::new(v.editor.document.clone());
        v.keep_guided_mask(w, cx);
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        v.validate_guided_mask_context(w, cx);
        assert!(!v.guided_mask_active());
    });
}

fn screen_point(v: &EditorView, x: f32, y: f32) -> Point<Pixels> {
    let d = v.guided_mask.draft.as_ref().unwrap();
    let (x, y) = v
        .editor
        .image_world_point(&d.review.source.layer, x, y)
        .unwrap();
    let b = v.viewport.get();
    point(
        px(f32::from(b.origin.x)
            + (f32::from(b.size.width) - v.editor.document.width as f32 * v.zoom) / 2.
            + v.pan.0
            + x * v.zoom),
        px(f32::from(b.origin.y)
            + (f32::from(b.size.height) - v.editor.document.height as f32 * v.zoom) / 2.
            + v.pan.1
            + y * v.zoom),
    )
}
#[gpui_kit::test]
fn lost_release_then_tool_change_never_keeps_an_unseen_manual_mask(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    view.update_in(cx, |v, w, cx| {
        assert!(v.guided_mask_preview_current());
        v.change_guided_mask_tool(ReviewTool::SubtractCoverage, cx);
        let p = screen_point(v, 20.5, 20.5);
        v.guided_mask_down(
            &MouseDownEvent {
                button: MouseButton::Left,
                position: p,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            },
            cx,
        );
        assert!(!v.guided_mask_preview_current());
        v.guided_mask_moved(
            &MouseMoveEvent {
                position: p,
                pressed_button: None,
                modifiers: Modifiers::default(),
            },
            cx,
        );
        assert!(v.guided_mask.draft.as_ref().unwrap().gesture.is_none());
        v.change_guided_mask_tool(ReviewTool::Positive, cx);
        v.keep_guided_mask(w, cx);
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        let before = v.editor.selection.clone();
        v.use_guided_mask_as_selection(w, cx);
        assert_eq!(v.editor.selection, before);
    });
    settle_preview(cx);
    view.update_in(cx, |v, w, cx| {
        assert!(v.guided_mask_preview_current());
        assert_eq!(
            v.guided_mask
                .draft
                .as_ref()
                .unwrap()
                .review
                .mask()
                .unwrap()
                .get_pixel(20, 20)[0],
            0
        );
        v.keep_guided_mask(w, cx);
        assert_eq!(
            v.editor.document.layers[0]
                .mask
                .as_ref()
                .unwrap()
                .get_pixel(20, 20)[0],
            0
        );
        assert_eq!(v.editor.undo_depth(), 1);
    });
}
#[gpui_kit::test]
fn review_brush_shortcuts_are_bounded_precise_and_do_not_change_document_brush(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| {
        seed_review(v, cx);
        v.set_guided_brush_diameter(1., cx);
        assert_eq!(
            v.guided_mask.draft.as_ref().unwrap().review.brush_radius,
            0.5
        );
        v.guided_mask_brush_command("brush-smaller", cx);
        assert_eq!(
            v.guided_mask.draft.as_ref().unwrap().review.brush_radius,
            0.5
        );
        v.set_guided_brush_diameter(999., cx);
        assert_eq!(
            v.guided_mask.draft.as_ref().unwrap().review.brush_radius,
            128.
        );
        v.set_guided_brush_diameter(f32::NAN, cx);
        assert_eq!(
            v.guided_mask.draft.as_ref().unwrap().review.brush_radius,
            128.
        );
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(v.editor.document.layers[0].mask.is_none());
    });
}

#[test]
fn miniature_pixels_are_bounded_alpha_correct_and_cancelled() {
    let source = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 0]));
    let alternatives = vec![guided_mask::Alternative {
        mask: Arc::new(image::GrayImage::from_pixel(4, 4, image::Luma([255]))),
        estimated_iou: 0.9,
    }];
    let pixels = candidate_miniatures(&source, &alternatives, || false).unwrap();
    assert_eq!(pixels.len(), 1);
    assert_eq!(pixels[0].dimensions(), (128, 80));
    assert_eq!(pixels[0].as_raw().len(), 128 * 80 * 4);
    assert!(
        pixels[0]
            .pixels()
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255),
        "transparent hidden red must not tint the checker"
    );
    assert!(candidate_miniatures(&source, &alternatives, || true).is_err());
    assert!(candidate_miniatures(&source, &vec![alternatives[0].clone(); 4], || false).is_err());
}

#[gpui_kit::test]
fn replaced_and_cancelled_miniatures_leave_no_atlas_entries_with_external_arcs(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    for _ in 0..3 {
        let old = view.update(cx, |v, _| v.guided_mask.miniatures[0].clone());
        cx.update(|window, _| {
            assert!(
                window.has_image_atlas_entry(&old),
                "visible option must first allocate an atlas entry"
            )
        });
        view.update_in(cx, |v, _window, cx| {
            let d = v.guided_mask.draft.as_ref().unwrap();
            let pixels =
                candidate_miniatures(&d.review.pixels, &d.review.alternatives, || false).unwrap();
            v.replace_guided_miniatures(pixels, cx);
            cx.notify();
        });
        settle_preview(cx);
        cx.update(|window, _| {
            assert!(
                !window.has_image_atlas_entry(&old),
                "replacement evicts even while this Arc survives"
            )
        });
    }
    let current = view.update(cx, |v, _| v.guided_mask.miniatures[0].clone());
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&current)));
    view.update_in(cx, |v, window, cx| {
        assert!(v.cancel_guided_mask(window, cx));
    });
    settle_preview(cx);
    cx.update(|window, _| {
        assert!(
            !window.has_image_atlas_entry(&current),
            "Cancel must evict the active window as well as other windows"
        )
    });
}

#[test]
fn miniatures_show_each_candidates_actual_coverage_on_the_source_photo() {
    let source = image::RgbaImage::from_fn(4, 2, |x, _| {
        image::Rgba(if x < 2 {
            [230, 40, 20, 255]
        } else {
            [20, 40, 230, 255]
        })
    });
    let half = guided_mask::Alternative {
        mask: Arc::new(image::GrayImage::from_fn(4, 2, |x, _| {
            image::Luma([if x < 2 { 255 } else { 0 }])
        })),
        estimated_iou: 0.9,
    };
    let full = guided_mask::Alternative {
        mask: Arc::new(image::GrayImage::from_pixel(4, 2, image::Luma([255]))),
        estimated_iou: 0.8,
    };
    let pixels = candidate_miniatures(&source, &[half, full], || false).unwrap();
    assert_eq!(pixels[0].get_pixel(40, 40).0, [230, 40, 20, 255]);
    let removed = pixels[0].get_pixel(100, 40);
    assert_eq!(removed[0], removed[1]);
    assert_eq!(removed[1], removed[2]);
    assert_eq!(pixels[1].get_pixel(100, 40).0, [20, 40, 230, 255]);
}

#[gpui_kit::test]
fn keep_consumes_the_exact_reviewed_pixels_and_surface_even_while_original_is_shown(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    view.update_in(cx, |v, window, cx| {
        assert!(v.guided_mask_preview_current());
        let d = v.guided_mask.draft.as_ref().unwrap();
        let ready = d.prepared_pixels.as_ref().unwrap();
        let pixels = ready.pixels.as_raw().as_ptr();
        let surface = ready.display.snapshot();
        let expected = ready.pixels.clone();
        let actual_retained = ready.pixels.as_raw().len() as u64
            + guided_surface_bytes(&ready.display)
            + guided_surface_bytes(&d.original_display);
        assert!(
            v.guided_preview_memory_bytes().unwrap()
                >= actual_retained
                    + u64::from(v.editor.document.width) * u64::from(v.editor.document.height) * 8,
            "admission charges retained candidates and next worker buffers"
        );
        v.guided_mask.draft.as_mut().unwrap().original = true;
        v.paint_guided_mask(cx);
        assert!(
            v.guided_mask_preview_current(),
            "viewing Original must not invalidate an already viewed exact candidate"
        );
        v.keep_guided_mask(window, cx);
        assert!(!v.guided_mask_active());
        assert_eq!(v.editor.undo_depth(), 1);
        assert_eq!(v.pixels, expected);
        assert_eq!(
            v.pixels.as_raw().as_ptr(),
            pixels,
            "Keep must consume the worker pixel allocation, not re-composite it"
        );
        assert!(
            Arc::ptr_eq(&v.display.snapshot(), &surface),
            "Keep must consume the worker-created tiles without converting them again"
        );
    });
}

#[gpui_kit::test]
fn changed_proof_or_prepared_dimensions_refuse_keep_without_document_mutation(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    view.update_in(cx, |v, window, cx| {
        assert!(v.guided_mask_preview_current());
        let before = v.pixels.clone();
        v.proof_settings.intent = v.proof_settings.intent.wrapping_add(1);
        assert!(!v.guided_mask_preview_current());
        v.keep_guided_mask(window, cx);
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(v.pixels, before);
        v.proof_settings.intent = v.proof_settings.intent.wrapping_sub(1);
        assert!(v.guided_mask_preview_current());
        v.guided_mask
            .draft
            .as_mut()
            .unwrap()
            .prepared_pixels
            .as_mut()
            .unwrap()
            .pixels = image::RgbaImage::new(1, 1);
        assert!(!v.guided_mask_preview_current());
        v.keep_guided_mask(window, cx);
        assert!(v.editor.document.layers[0].mask.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
        assert_eq!(v.pixels, before);
        assert!(v.cancel_guided_mask(window, cx));
    });
}

#[gpui_kit::test]
fn cancel_after_external_document_replacement_never_restores_the_old_display(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    view.update_in(cx, |v, window, cx| {
        let mut doc = Document::new(32, 24);
        doc.layers[0].image =
            Some(image::RgbaImage::from_pixel(32, 24, image::Rgba([20, 220, 30, 255])).into());
        v.editor = Editor::new(doc);
        assert!(v.cancel_guided_mask(window, cx));
        assert_eq!(v.display.dimensions(), (32, 24));
        assert_eq!(v.pixels.dimensions(), (32, 24));
        assert!(v.pixels.pixels().all(|p| p.0 == [20, 220, 30, 255]));
        assert_eq!(v.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn cancel_after_proof_change_rebuilds_current_display_instead_of_old_proof_tiles(
    cx: &mut TestAppContext,
) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    view.update_in(cx, |v, window, cx| {
        let raw = v.pixels.clone();
        let captured = v
            .guided_mask
            .draft
            .as_ref()
            .unwrap()
            .original_display
            .snapshot();
        v.proof_settings.enabled = true;
        v.proof_settings.monitor_profile = Some("/unavailable/omuse-guided-test.icc".into());
        assert!(v.cancel_guided_mask(window, cx));
        assert_eq!(v.pixels, raw);
        assert_eq!(v.display.dimensions(), raw.dimensions());
        assert!(
            !Arc::ptr_eq(&captured, &v.display.snapshot()),
            "A changed proof setting cannot reuse the captured old display"
        );
        assert!(
            v.status.contains("Display profile unavailable"),
            "Current proof failure must remain visible: {}",
            v.status
        );
        assert_eq!(v.editor.undo_depth(), 0);
        assert!(v.editor.document.layers[0].mask.is_none());
    });
}

#[gpui_kit::test]
fn inspector_mouse_up_out_does_not_invalidate_a_current_guided_review(cx: &mut TestAppContext) {
    let (view, cx, _temp) = setup(cx);
    view.update(cx, |v, cx| seed_review(v, cx));
    settle_preview(cx);
    let footer = cx.debug_bounds("guided-keep").unwrap().center();
    view.update(cx, |v, cx| {
        assert!(v.guided_mask_preview_current());
        let generation = v.guided_mask.preview_generation;
        let painted = v.guided_mask.preview_presented.as_ref().unwrap().clone();
        let pixels = v
            .guided_mask
            .draft
            .as_ref()
            .unwrap()
            .prepared_pixels
            .as_ref()
            .unwrap()
            .pixels
            .as_raw()
            .as_ptr();
        assert!(v.guided_mask_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: footer,
                modifiers: Modifiers::default(),
                click_count: 1
            },
            cx
        ));
        assert!(
            v.guided_mask_preview_current(),
            "Inspector release must leave the displayed candidate available to its button handler"
        );
        assert_eq!(v.guided_mask.preview_generation, generation);
        assert!(Arc::ptr_eq(
            v.guided_mask.preview_presented.as_ref().unwrap(),
            &painted
        ));
        assert_eq!(
            v.guided_mask
                .draft
                .as_ref()
                .unwrap()
                .prepared_pixels
                .as_ref()
                .unwrap()
                .pixels
                .as_raw()
                .as_ptr(),
            pixels
        );
        assert!(
            !v.guided_mask_has_work(),
            "A non-gesture release must not queue a compositor"
        );
    });
    // Real dispatch traverses the canvas's mouse-up-out before button activation.
    cx.simulate_click(footer, Modifiers::default());
    settle_preview(cx);
    view.update(cx, |v, _| {
        assert!(!v.guided_mask_active());
        assert_eq!(v.editor.undo_depth(), 1);
        assert!(v.editor.document.layers[0].mask.is_some());
    });
}
