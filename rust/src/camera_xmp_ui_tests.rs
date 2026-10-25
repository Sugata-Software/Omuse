use super::*;
use gpui_kit::{Focusable, TestAppContext, VisualTestContext};
fn packet() -> Vec<u8> {
    br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:c="http://ns.adobe.com/camera-raw-settings/1.0/" c:Exposure2012="0.5" c:Saturation="17" c:Temperature="6500" c:CameraProfile="Not an installed profile"/></rdf:RDF>"#.to_vec()
}
fn setup(cx: &mut TestAppContext, retained: bool) -> (Entity<EditorView>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        let mut document = Document::new(12, 8);
        let pixels = image::RgbaImage::from_fn(12, 8, |x, y| {
            image::Rgba([
                (11 + x * 13 + y * 5) as u8,
                (19 + x * 9 + y * 7) as u8,
                (221 - x * 11 - y * 9) as u8,
                [0, 1, 63, 127, 254, 255][x as usize % 6],
            ])
        });
        if retained {
            document.layers[0].advanced = Some(Arc::new(
                omuse::advanced::LayerState::from_image(&pixels, "retained original").unwrap(),
            ));
        }
        document.layers[0].image = Some(pixels.into());
        let target_id=document.layers[0].id.clone();
        let mut clip=Layer::paint("Actual hidden half-image clip",12,8);
        clip.image=Some(image::RgbaImage::from_fn(12,8,|x,_|image::Rgba([255,255,255,if x<6{255}else{0}])).into());clip.visible=false;
        document.layers[0].metadata["maskSourceID"]=serde_json::json!(clip.id);
        document.layers[0].mask=Some(image::RgbaImage::from_pixel(12,8,image::Rgba([0,0,0,255])).into());
        document.layers[0].metadata["maskEnabled"]=false.into();
        document.layers[0].metadata["maskPlacement"]=serde_json::json!({"origin":[1,1],"size":[10,6],"rotation":0,"flipX":false,"flipY":false});
        document.layers.push(clip);
        view.editor = Editor::new(document);
        view.editor.active_layer=target_id;
        view.dialog = Dialog::None;
        view.open_camera_raw(window, cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    (view, cx)
}
fn load(view: &Entity<EditorView>, cx: &mut VisualTestContext, path: PathBuf) {
    view.update_in(cx, |view, window, cx| {
        let admission = view.camera_xmp_admit(cx).unwrap();
        view.load_camera_xmp(path, admission, window, cx);
    });
    cx.run_until_parked();
}
#[gpui_kit::test]
fn native_xmp_review_keeps_pixels_until_apply_then_grade_report_undo_and_reopen_together(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, true);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("user.xmp");
    std::fs::write(&path, packet()).unwrap();
    let before = view.update(cx, |view, _| view.editor.document.clone());
    load(&view, cx, path.clone());
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.review.is_some());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.document.layers[0].image, before.layers[0].image);
        assert!(
            view.camera_preview
                .xmp
                .review
                .as_ref()
                .unwrap()
                .plan
                .report()
                .rows
                .iter()
                .any(|row| row.property == "Temperature"
                    && row.disposition == omuse::xmp_preset::Disposition::Refused)
        );
    });
    view.update_in(cx, |view, window, cx| {
        view.camera_xmp_use(window, cx).unwrap()
    });
    cx.run_until_parked();
    let preview_frame = view.update(cx, |view, cx| {
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.document.layers[0].image, before.layers[0].image);
        let settings: Settings = serde_json::from_value(view.camera_draft.clone()).unwrap();
        assert_eq!(
            settings.processing,
            omuse::camera_raw::Processing::Native16V1
        );
        assert_eq!(settings.exposure, 0.5);
        let id = view.editor.active_layer.clone();
        let mut expected = Editor::new(before.clone());
        expected.active_layer = id.clone();
        let state = expected.editable_state(&id).unwrap();
        let (prepared, _) = state
            .with_native_camera_raw(
                settings.clone(),
                expected.selection_for_layer(&id).unwrap(),
                &AtomicBool::new(false),
                None,
            )
            .unwrap();
        expected
            .replace_editable_states(vec![(id, prepared)])
            .unwrap();
        let pixels = raster::composite(&expected.document);
        assert!(pixels.get_pixel(2, 4)[3] > 0);
        assert_eq!(pixels.get_pixel(9, 4)[3], 0);
        assert_eq!(
            view.display.replace(&pixels).replaced_tiles,
            0,
            "XMP preview must show the mapped grade with the actual live clip"
        );
        view.start_camera_raw(settings, false, cx);
        pixels
    });
    cx.run_until_parked();
    let committed = view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(
            view.display.replace(&preview_frame).replaced_tiles,
            0,
            "kept pixels must agree with the actual reviewed preview"
        );
        let layer = &view.editor.document.layers[0];
        for key in ["maskSourceID", "maskEnabled", "maskPlacement"] {
            assert_eq!(layer.metadata[key], before.layers[0].metadata[key], "{key}");
        }
        assert_eq!(layer.mask, before.layers[0].mask);
        assert!(
            layer
                .metadata
                .get(omuse::xmp_preset::METADATA_KEY)
                .is_some()
        );
        view.editor.document.clone()
    });
    view.update(cx, |view, _| {
        assert!(view.editor.undo());
        assert_eq!(view.editor.document.layers[0].image, before.layers[0].image);
        assert!(
            view.editor.document.layers[0]
                .metadata
                .get(omuse::xmp_preset::METADATA_KEY)
                .is_none()
        );
        assert!(view.editor.redo());
        assert_eq!(
            view.editor.document.layers[0].metadata[omuse::xmp_preset::METADATA_KEY],
            committed.layers[0].metadata[omuse::xmp_preset::METADATA_KEY]
        );
    });
    std::fs::remove_file(&path).unwrap();
    let saved = temp.path().join("native-preset.omuse");
    omuse::document::save(&committed, &saved).unwrap();
    let reopened = omuse::document::open(&saved).unwrap();
    assert_eq!(
        reopened.layers[0].metadata[omuse::xmp_preset::METADATA_KEY],
        committed.layers[0].metadata[omuse::xmp_preset::METADATA_KEY]
    );
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16(),
        committed.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16()
    );
}
#[gpui_kit::test]
fn ordinary_photo_promotion_is_explicit_separate_undo_and_rebuilds_same_xml_report(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("user.xmp");
    std::fs::write(&path, packet()).unwrap();
    load(&view, cx, path);
    let (before, hash) = view.update(cx, |view, _| {
        (
            view.editor.document.clone(),
            view.camera_preview
                .xmp
                .review
                .as_ref()
                .unwrap()
                .plan
                .report()
                .source_sha256
                .clone(),
        )
    });
    view.update_in(cx, |view, window, cx| {
        assert!(view.camera_xmp_use(window, cx).is_err());
        assert_eq!(view.editor.undo_depth(), 0);
        view.camera_xmp_promote(window, cx).unwrap();
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.editor.document.layers[0].image, before.layers[0].image);
        assert!(
            view.editor.document.layers[0]
                .advanced
                .as_ref()
                .unwrap()
                .recipe
                .nodes
                .is_empty()
        );
        let review = view.camera_preview.xmp.review.as_ref().unwrap();
        assert_eq!(review.plan.report().source_sha256, hash);
        assert_eq!(review.plan.report().target.processing, "Native16V1");
        assert_eq!(review.plan.report().target.revision, view.editor.revision());
    });
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::CameraRaw);
        assert!(view.camera_preview.xmp.review.is_none());
    });
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::None);
        assert_eq!(view.editor.undo_depth(), 1);
        assert!(view.editor.undo());
        assert!(view.editor.document.layers[0].advanced.is_none());
        assert_eq!(view.editor.document.layers[0].image, before.layers[0].image);
    });
}
#[gpui_kit::test]
fn malformed_unsupported_cancelled_and_stale_imports_never_replace_the_draft_or_pixels(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, true);
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("bad.xmp");
    std::fs::write(&bad, b"<bad").unwrap();
    let original = view.update(cx, |view, _| {
        (view.camera_draft.clone(), view.editor.document.clone())
    });
    load(&view, cx, bad);
    view.update(cx, |view, _| {
        assert!(
            view.camera_preview
                .xmp
                .message
                .as_ref()
                .unwrap()
                .contains("not imported")
        );
        assert!(view.camera_preview.xmp.review.is_none());
        assert_eq!(view.camera_draft, original.0);
        assert_eq!(view.editor.undo_depth(), 0);
    });
    let unsupported = temp.path().join("unsupported.xmp");
    std::fs::write(&unsupported,br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:c="http://ns.adobe.com/camera-raw-settings/1.0/" c:Temperature="6500"/></rdf:RDF>"#).unwrap();
    load(&view, cx, unsupported);
    view.update_in(cx, |view, window, cx| {
        assert_eq!(
            view.camera_preview
                .xmp
                .review
                .as_ref()
                .unwrap()
                .plan
                .mapped_count(),
            0
        );
        assert!(view.camera_xmp_use(window, cx).is_err());
        assert_eq!(view.camera_draft, original.0);
    });
    let valid = temp.path().join("valid.xmp");
    std::fs::write(&valid, packet()).unwrap();
    view.update_in(cx, |view, window, cx| {
        let admission = view.camera_xmp_admit(cx).unwrap();
        view.load_camera_xmp(valid.clone(), admission, window, cx);
        view.camera_xmp_cancel_review(cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.review.is_none())
    });
    view.update_in(cx, |view, window, cx| {
        let admission = view.camera_xmp_admit(cx).unwrap();
        view.load_camera_xmp(valid, admission, window, cx);
        view.cancel_camera_raw();
        view.open_camera_raw(window, cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.review.is_none());
        assert!(view.camera_preview.xmp.active.is_none());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(
            view.editor.document.layers[0].image,
            original.1.layers[0].image
        );
    });
}

#[gpui_kit::test]
fn manual_xmp_path_enter_imports_for_review_without_applying_the_grade(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, true);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("manual.xmp");
    std::fs::write(&path, packet()).unwrap();
    view.update_in(cx, |view, window, cx| {
        let input = view.camera_preview.xmp.path.as_ref().unwrap().clone();
        input.update(cx, |state, cx| {
            state.set_value(path.to_string_lossy().to_string(), window, cx)
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.dialog, Dialog::CameraRaw);
        assert!(view.camera_preview.xmp.review.is_some());
        assert!(view.camera_preview.xmp.accepted.is_none());
        assert_eq!(view.editor.undo_depth(), 0);
    });
}

#[gpui_kit::test]
fn replaced_editor_discards_late_import_without_stuck_loading_or_hidden_promotion(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, true);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.xmp");
    std::fs::write(&path, packet()).unwrap();
    view.update_in(cx, |view, window, cx| {
        let admission = view.camera_xmp_admit(cx).unwrap();
        view.load_camera_xmp(path, admission, window, cx);
        view.editor = Editor::new(Document::new(12, 8));
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.review.is_none());
        assert!(view.camera_preview.xmp.active.is_none());
        assert!(
            view.camera_preview
                .xmp
                .message
                .as_ref()
                .unwrap()
                .contains("changed")
        );
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.document.layers[0].advanced.is_none());
    });
}

#[gpui_kit::test]
fn manual_setting_change_while_reading_keeps_new_value_and_releases_loading(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, true);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.xmp");
    std::fs::write(&path, packet()).unwrap();
    let original = view.update(cx, |view, _| view.editor.document.layers[0].image.clone());
    view.update_in(cx, |view, window, cx| {
        let admission = view.camera_xmp_admit(cx).unwrap();
        view.load_camera_xmp(path, admission, window, cx);
        let index = crate::camera_controls::fields(&view.camera_draft, view.camera_section)
            .iter()
            .position(|field| field.path == "/exposure")
            .unwrap();
        view.detail_inputs[index].update(cx, |input, cx| input.set_value("1.25", window, cx));
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.active.is_none());
        assert!(view.camera_preview.xmp.review.is_none());
        assert_eq!(view.camera_draft["exposure"], 1.25);
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.editor.document.layers[0].image, original);
    });
}

fn reveal(cx: &mut VisualTestContext, id: &'static str) {
    for _ in 0..25 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let viewport = cx.debug_bounds("dialog-body").unwrap();
        let bounds = cx.debug_bounds(id).expect("XMP control exists");
        if bounds.origin.y >= viewport.origin.y
            && bounds.bottom_right().y <= viewport.bottom_right().y
        {
            return;
        }
        let delta = if bounds.origin.y < viewport.origin.y {
            (f32::from(viewport.origin.y - bounds.origin.y) + 12.).clamp(100., 600.)
        } else {
            -(f32::from(bounds.bottom_right().y - viewport.bottom_right().y) + 12.)
                .clamp(100., 600.)
        };
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: point(viewport.origin.x + px(2.), viewport.origin.y + px(2.)),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
    }
    panic!("{id} is not reachable by actual scrolling in the minimum viewport");
}
#[gpui_kit::test]
fn xmp_import_and_review_buttons_are_reachable_in_the_minimum_camera_viewport(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, true);
    reveal(cx, "camera-xmp-choose");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.xmp");
    std::fs::write(&path, packet()).unwrap();
    load(&view, cx, path);
    reveal(cx, "camera-xmp-use");
    let bounds = cx.debug_bounds("camera-xmp-use").unwrap();
    cx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.camera_preview.xmp.accepted.is_some());
        assert_eq!(view.editor.undo_depth(), 0);
        assert_eq!(view.dialog, Dialog::CameraRaw);
    });
}

#[gpui_kit::test]
fn cancelled_or_replaced_background_promotion_cannot_commit_late(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.xmp");
    std::fs::write(&path, packet()).unwrap();
    load(&view, cx, path.clone());
    let pixels = view.update(cx, |view, _| view.editor.document.layers[0].image.clone());
    view.update_in(cx, |view, window, cx| {
        view.camera_xmp_promote(window, cx).unwrap();
        assert!(view.camera_preview.xmp.promotion.is_some());
        view.camera_xmp_cancel_review(cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.document.layers[0].advanced.is_none());
        assert_eq!(view.editor.document.layers[0].image, pixels);
    });
    load(&view, cx, path);
    view.update_in(cx, |view, window, cx| {
        view.camera_xmp_promote(window, cx).unwrap();
        view.editor = Editor::new(Document::new(12, 8));
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.editor.undo_depth(), 0);
        assert!(view.editor.document.layers[0].advanced.is_none());
        assert!(view.camera_preview.xmp.promotion.is_none());
        assert!(view.camera_preview.xmp.active.is_none());
    });
}
