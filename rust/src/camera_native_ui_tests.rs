use super::*;
use gpui_kit::{TestAppContext, VisualTestContext};
use omuse::{
    advanced::LayerState,
    camera_raw::{Processing, native},
    precision::Rgba16Image,
};
fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(crate::init_test_theme);
    let recovery = tempfile::tempdir().unwrap();
    let path = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(path);
        view.dialog = Dialog::None;
        let words = Rgba16Image::from_fn(48, 32, |x, y| {
            image::Rgba([
                24001 + x as u16 * 31,
                21003 + y as u16 * 37,
                18007,
                [0, 1, 65535][((x + y) % 3) as usize],
            ])
        });
        let state = LayerState::from_rgba16(&words, "Native photo").unwrap();
        let mut doc = Document::new(48, 32);
        doc.background = [0; 4];
        doc.layers[0].image = Some(state.proxy().unwrap().into());
        doc.layers[0].advanced = Some(Arc::new(state));
        view.editor = Editor::new(doc);
        view.refresh(cx);
        view.open_camera_raw(window, cx);
        view
    });
    (view, cx, recovery)
}
#[gpui_kit::test]
fn retained_camera_dialog_previews_applies_one_node_and_undoes_exact_words(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    let (before, expected) = view.update(cx, |view, cx| {
        assert_eq!(view.dialog, Dialog::CameraRaw, "{}", view.status);
        let mut settings: Settings = serde_json::from_value(view.camera_draft.clone()).unwrap();
        assert_eq!(settings.processing, Processing::Native16V1);
        assert!(
            crate::camera_controls::fields(&view.camera_draft, 0)
                .iter()
                .all(|field| field.path != "/processing")
        );
        settings.exposure = 0.65;
        settings.clarity = 17.;
        let state = view
            .editor
            .editable_state(&view.editor.active_layer)
            .unwrap();
        let expected = native::apply(&state.result, &settings, &AtomicBool::new(false))
            .unwrap()
            .to_rgba16();
        let before = state.result.to_rgba16();
        view.camera_draft = serde_json::to_value(&settings).unwrap();
        view.start_camera_raw(settings, true, cx);
        (before, expected)
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert!(!view.busy, "{}", view.status);
        assert!(view.status.contains("16-bit preview"), "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 0);
        let state = view
            .editor
            .editable_state(&view.editor.active_layer)
            .unwrap();
        assert_eq!(state.result.to_rgba16(), before);
        let settings = serde_json::from_value(view.camera_draft.clone()).unwrap();
        view.start_camera_raw(settings, false, cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        let state = view
            .editor
            .editable_state(&view.editor.active_layer)
            .unwrap();
        assert_eq!(state.recipe.nodes.len(), 1);
        assert_eq!(state.source.to_rgba16(), before);
        assert_eq!(state.result.to_rgba16(), expected);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.editor.undo());
        assert_eq!(
            view.editor
                .editable_state(&view.editor.active_layer)
                .unwrap()
                .result
                .to_rgba16(),
            before
        );
        assert!(view.editor.redo());
        assert_eq!(
            view.editor
                .editable_state(&view.editor.active_layer)
                .unwrap()
                .result
                .to_rgba16(),
            expected
        );
    });
}
#[gpui_kit::test]
fn native_apply_cannot_commit_after_same_revision_editor_replacement_or_selection_change(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    for change in 0..2 {
        let before = view.update(cx, |view, cx| {
            let state = view
                .editor
                .editable_state(&view.editor.active_layer)
                .unwrap();
            view.start_camera_raw(
                Settings {
                    exposure: 0.5,
                    ..Settings::for_native_edit()
                },
                false,
                cx,
            );
            if change == 0 {
                view.editor = Editor::new(view.editor.document.clone());
            } else {
                view.editor.select_all();
            }
            state.result.to_rgba16()
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert_eq!(view.dialog, Dialog::CameraRaw);
            assert!(view.status.contains("changed"), "{}", view.status);
            assert_eq!(view.editor.undo_depth(), 0);
            assert_eq!(
                view.editor
                    .editable_state(&view.editor.active_layer)
                    .unwrap()
                    .result
                    .to_rgba16(),
                before
            );
        });
    }
}
#[gpui_kit::test]
fn native_sampler_receives_low_alpha_words_and_cancel_never_changes_history(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, cx| {
        view.camera_section = 0;
        view.start_camera_raw(Settings::for_native_edit(), true, cx);
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        let sample = omuse::camera_gestures::Sample::Native {
            pixel: [24001, 21003, 18007, 1],
            space: omuse::precision::WorkingSpace::Srgb,
        };
        let expected =
            omuse::camera_gestures::white_balance_sample(&Settings::for_native_edit(), sample)
                .unwrap()
                .0;
        view.handle_camera_canvas(
            &crate::camera_canvas::CameraCanvasEvent::SampledColour(sample),
            window,
            cx,
        );
        let actual: Settings = serde_json::from_value(view.camera_draft.clone()).unwrap();
        assert_eq!(actual.temperature, expected.temperature);
        assert_eq!(actual.tint, expected.tint);
        assert_eq!(view.editor.undo_depth(), 0);
        view.cancel_camera_raw();
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(
            view.editor
                .editable_state(&view.editor.active_layer)
                .unwrap()
                .recipe
                .nodes
                .len(),
            0
        );
    });
}

#[gpui_kit::test]
fn native_clipping_warnings_reach_display_and_never_enter_kept_recipe_pixels(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    view.update_in(cx, |view, window, cx| {
        view.cancel_camera_raw();
        view.dialog = Dialog::None;
        let state = LayerState::from_rgba16(
            &Rgba16Image::from_pixel(48, 32, image::Rgba([65535; 4])),
            "White precision fixture",
        )
        .unwrap();
        let mut document = Document::new(48, 32);
        document.background = [0; 4];
        document.layers[0].image = Some(state.proxy().unwrap().into());
        document.layers[0].advanced = Some(Arc::new(state));
        view.editor = Editor::new(document);
        view.refresh(cx);
        view.open_camera_raw(window, cx);
        view.camera_clip_highlights = true;
        view.start_camera_raw(Settings::for_native_edit(), true, cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert!(!view.busy, "{}", view.status);
        let white = image::RgbaImage::from_pixel(48, 32, image::Rgba([255; 4]));
        let expected = omuse::camera_raw::clipping_preview(&white, false, true);
        assert_ne!(expected, white);
        assert_eq!(
            view.display.replace(&expected).replaced_tiles,
            0,
            "Warning pixels were ignored by the retained compositor"
        );
        assert_eq!(
            view.camera_scopes.as_deref(),
            Some(&omuse::photo_scopes::PhotoScopes::analyze(&white))
        );
        assert_eq!(view.editor.undo_depth(), 0);
        view.start_camera_raw(Settings::for_native_edit(), false, cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 1);
        let state = view
            .editor
            .editable_state(&view.editor.active_layer)
            .unwrap();
        assert!(state.result.to_rgba16().pixels().all(|p| p.0 == [65535; 4]));
    });
}

#[gpui_kit::test]
fn native_sampler_cache_is_released_on_cancel_even_while_canvas_remains_alive(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, cx| {
        view.start_camera_raw(Settings::for_native_edit(), true, cx)
    });
    cx.run_until_parked();
    let weak = view.update(cx, |view, _| {
        assert!(!view.busy);
        let sample = view
            .camera_preview
            .native_sample
            .as_ref()
            .expect("native stage reference");
        Arc::downgrade(sample)
    });
    assert!(weak.upgrade().is_some());
    view.update(cx, |view, _| view.cancel_camera_raw());
    assert!(
        weak.upgrade().is_none(),
        "The canvas must hold only a weak reference after Cancel"
    );
}
