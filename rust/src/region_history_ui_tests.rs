use super::*;
use gpui_kit::TestAppContext;

const CANVAS_EDGE: u32 = 520;

#[gpui_kit::test]
fn region_history_brush_frames_cancel_and_round_trip_with_retained_pixels(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let recovery_dir = temp.path().join("recovery");
    let (view, cx) = cx.add_window_view(|window, cx| {
        assert!((CANVAS_EDGE as usize) * (CANVAS_EDGE as usize) * 4 >= 1024 * 1024);
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(recovery_dir);
        let mut document = Document::new(CANVAS_EDGE, CANVAS_EDGE);
        document.layers[0].image = Some(
            image::RgbaImage::from_pixel(CANVAS_EDGE, CANVAS_EDGE, image::Rgba([12, 34, 56, 255]))
                .into(),
        );
        view.editor = Editor::new(document);
        view.editor.set_region_history_enabled(true);
        view.editor.brush.size = 11.;
        view.editor.brush.color = [220, 40, 90, 255];
        view.dialog = Dialog::None;
        view.refresh(cx);
        view
    });

    let (retained, retained_composite, initial_composite) = view.update(cx, |view, _| {
        // Model the immutable document owner held by recovery or an asynchronous save.
        let retained = view.editor.document.clone();
        let retained_composite = raster::composite(&retained);
        (retained, retained_composite.clone(), retained_composite)
    });

    view.update_in(cx, |view, window, cx| {
        assert!(view.editor.begin_stroke(80., 80., 1., PaintTool::Brush));
        assert!(view.editor.continue_stroke(104., 92., 1.));
        view.queue_stroke_frame(window, cx);
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    view.update(cx, |view, cx| {
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
        view.editor.cancel_stroke();
        view.changed(cx);
        assert_eq!(view.pixels, initial_composite);
        assert_eq!(view.editor.history_stats().raster_patch_entries, 0);
    });

    view.update_in(cx, |view, window, cx| {
        assert!(view.editor.begin_stroke(140., 180., 1., PaintTool::Brush));
        for point in [(168., 190.), (196., 208.), (224., 220.)] {
            assert!(view.editor.continue_stroke(point.0, point.1, 1.));
            view.queue_stroke_frame(window, cx);
        }
        assert!(view.pending_stroke_frame.is_some());
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
        assert!(view.editor.continue_stroke(252., 236., 1.));
        view.queue_stroke_frame(window, cx);
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));

    let painted = view.update(cx, |view, cx| {
        assert!(view.editor.finish_stroke());
        view.changed(cx);
        let painted = raster::composite(&view.editor.document);
        assert_eq!(view.pixels, painted);
        assert_ne!(painted, initial_composite);
        assert!(view.editor.history_stats().raster_patch_entries > 0);
        painted
    });

    view.update(cx, |view, cx| {
        assert!(view.editor.undo());
        view.changed(cx);
        assert_eq!(view.pixels, initial_composite);
        assert!(view.editor.redo());
        view.changed(cx);
        assert_eq!(view.pixels, painted);
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
    });

    assert_eq!(raster::composite(&retained), retained_composite);
}
