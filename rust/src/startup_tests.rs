use super::*;
use gpui_kit::{TestAppContext, VisualTestContext, size};

fn setup(cx: &mut TestAppContext, reduced: bool) -> (Entity<StartupView>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        cx.set_reduce_motion(reduced);
    });
    let (view, cx) = cx.add_window_view(|window, cx| StartupView::new(None, None, window, cx));
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, cx)
}

fn advance_first_frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn splash_paints_before_preparing_then_releases_artwork_and_accepts_editor_input(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx, false);
    let artwork = cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.phase, Phase::FirstFrame);
        assert!(view.editor.is_none());
        assert!(view.load_task.is_none());
        let artwork = view.artwork.as_ref().unwrap();
        let images = [
            artwork.muse.clone().unwrap(),
            artwork.wordmark.clone().unwrap(),
        ];
        assert!(
            images
                .iter()
                .all(|image| window.has_image_atlas_entry(image))
        );
        images
    });
    let status = cx.debug_bounds("startup-status").unwrap();
    assert!(status.bottom() <= px(600.));
    assert!(cx.debug_bounds("startup-surface").is_some());
    assert!(cx.debug_bounds("studio-header").is_none());
    advance_first_frame(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.phase, Phase::Handoff);
        assert!(view.editor.is_some());
        assert!(view.transition_task.is_none());
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(HANDOFF + Duration::from_millis(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let view = view.read(cx);
        assert_eq!(view.phase, Phase::Ready);
        assert!(view.artwork.is_none());
        assert!(
            artwork
                .iter()
                .all(|image| !window.has_image_atlas_entry(image))
        );
    });
    assert!(cx.debug_bounds("startup-surface").is_none());
    assert!(cx.debug_bounds("studio-header").is_some());
    // A real pointer action proves the removed splash no longer intercepts input.
    let toggle = cx.debug_bounds("inspector-toggle").unwrap();
    assert!(cx.debug_bounds("inspector").is_some());
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("inspector").is_none());
}

#[gpui_kit::test]
fn reduced_motion_hands_off_immediately_without_an_invisible_overlay(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, true);
    advance_first_frame(cx);
    view.update(cx, |view, _| {
        assert_eq!(view.phase, Phase::Ready);
        assert!(view.transition_task.is_none());
        assert!(view.artwork.is_none());
    });
    assert!(cx.debug_bounds("startup-surface").is_none());
    assert!(cx.debug_bounds("studio-header").is_some());
}

#[gpui_kit::test]
fn enabling_reduced_motion_during_handoff_dismisses_the_overlay(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, false);
    advance_first_frame(cx);
    assert_eq!(view.update(cx, |view, _| view.phase), Phase::Handoff);
    cx.update(|window, cx| {
        cx.set_reduce_motion(true);
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert_eq!(view.update(cx, |view, _| view.phase), Phase::Ready);
    assert!(cx.debug_bounds("startup-surface").is_none());
}

#[gpui_kit::test]
fn closing_during_preparation_cancels_and_cannot_recreate_the_editor(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let (view, cx) = setup(cx, false);
    let cancelled = view.update(cx, |view, _| {
        // A deterministic pending job uses the same explicit native-test gate.
        view.native_journey = Some(tmp.path().to_path_buf());
        view.capture_gate = true;
        view.cancelled.clone()
    });
    advance_first_frame(cx);
    assert_eq!(view.update(cx, |view, _| view.phase), Phase::Loading);
    assert!(view.update(cx, |view, _| view.editor.is_none()));
    let weak = view.downgrade();
    drop(view);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(weak.upgrade().is_none());
    assert!(cx.windows().is_empty());
    std::fs::write(tmp.path().join("continue-startup"), b"continue").unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert!(!tmp.path().join("startup-editor-ready.json").exists());
}

#[gpui_kit::test]
fn duplicate_completion_cannot_replace_an_existing_editor(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, true);
    advance_first_frame(cx);
    let editor = view.update(cx, |view, _| view.editor.as_ref().unwrap().entity_id());
    let prepared = PreparedEditor::load(None, &AtomicBool::new(false)).unwrap();
    view.update_in(cx, |view, window, cx| {
        view.finish_load(prepared, window, cx)
    });
    assert_eq!(
        view.update(cx, |view, _| view.editor.as_ref().unwrap().entity_id()),
        editor
    );
    assert_eq!(view.update(cx, |view, _| view.phase), Phase::Ready);
}
