use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, ContentAwareReplace},
};
use std::sync::{Arc, atomic::Ordering};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = reveal_control(cx, id);
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

fn reveal_control(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    let in_settings = id.starts_with("pro-")
        && !matches!(
            id,
            "pro-source"
                | "pro-preview"
                | "pro-preview-button"
                | "pro-add-effect"
                | "pro-update-effect"
        );
    if !in_settings {
        return cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"));
    }
    let viewport = cx.debug_bounds("pro-settings").expect("settings viewport");
    let visible = |b: Bounds<Pixels>| {
        b.origin.y >= viewport.origin.y
            && b.origin.y + b.size.height <= viewport.origin.y + viewport.size.height
    };
    if let Some(bounds) = cx.debug_bounds(id).filter(|b| visible(*b)) {
        return bounds;
    }
    // Exercise the same scroll path as a user: rewind then find the control.
    for delta in std::iter::once(10_000.).chain(std::iter::repeat_n(-100., 30)) {
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
            modifiers: Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        draw(cx);
        if let Some(bounds) = cx.debug_bounds(id).filter(|b| visible(*b)) {
            return bounds;
        }
    }
    panic!("advanced control {id} is not reachable by scrolling");
}

fn set_input(view: &Entity<EditorView>, cx: &mut VisualTestContext, index: usize, value: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.detail_inputs[index].update(cx, |input, cx| input.set_value(value, window, cx));
        });
    });
    draw(cx);
}

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
    let recovery_dir = recovery.path().join("recovery");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        let mut document = Document::new(32, 24);
        document.background = [12, 16, 22, 255];
        let pixels = RgbaImage::from_fn(32, 24, |x, y| {
            Rgba([
                (20 + x * 5) as u8,
                (30 + y * 7) as u8,
                (80 + (x + y) * 2) as u8,
                255,
            ])
        });
        let state = LayerState::from_image(&pixels, "Base").unwrap();
        document.layers[0].image = Some(pixels.into());
        document.layers[0].advanced = Some(Arc::new(state));
        view.recovery = Recovery::at(recovery_dir);
        view.editor = Editor::new(document);
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    (view, cx, recovery)
}

fn open(view: &Entity<EditorView>, cx: &mut VisualTestContext, kind: Kind) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_pro(kind, window, cx));
    });
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Pro));
}

fn wait_for_preview(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.run_pro(false, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(
            !view.busy,
            "advanced preview did not finish: {}",
            view.status
        );
        assert!(
            view.pro_draft.as_ref().unwrap().preview.is_some(),
            "{}",
            view.status
        );
    });
}

fn apply(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        assert!(!view.busy, "{}", view.status);
    });
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
}

fn source_pixels(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> RgbaImage {
    view.update(cx, |view, _| {
        view.editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .to_image()
    })
}

#[gpui_kit::test]
fn advanced_footer_and_preview_fit_the_minimum_viewport(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    open(&view, cx, Kind::Stack);
    click(cx, "pro-preview-button");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(
            view.pro_draft.as_ref().unwrap().preview_dimensions,
            (32, 24)
        );
    });
    for id in [
        "pro-source",
        "pro-preview",
        "pro-add-effect",
        "pro-preview-button",
        "cancel-dialog",
        "confirm-dialog",
        "dialog-footer",
    ] {
        let bounds = reveal_control(cx, id);
        assert!(
            bounds.origin.x >= px(0.)
                && bounds.origin.y >= px(0.)
                && bounds.origin.x + bounds.size.width <= px(800.)
                && bounds.origin.y + bounds.size.height <= px(600.),
            "{id} outside viewport: {bounds:?}"
        );
    }
    click(cx, "cancel-dialog");
}

#[gpui_kit::test]
fn filter_stack_edit_mask_reorder_apply_and_undo_are_one_transaction(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| view.editor.select_rectangle(0., 0., 12., 24.));
    let original = source_pixels(&view, cx);
    open(&view, cx, Kind::Stack);
    set_input(&view, cx, 30, "42");
    click(cx, "pro-add-effect");
    cx.run_until_parked();
    draw(cx);
    click(cx, "pro-effect-11");
    click(cx, "pro-add-effect");
    cx.run_until_parked();
    draw(cx);
    click(cx, "pro-node-toggle");
    click(cx, "pro-node-up");
    click(cx, "pro-node-mask");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.pro_draft.as_ref().unwrap().state.recipe.nodes.len(), 2);
        assert_eq!(view.pro_draft.as_ref().unwrap().selected, Some(0));
        assert!(
            view.pro_draft.as_ref().unwrap().state.recipe.nodes[0]
                .soft_mask
                .is_some()
        );
        assert!(!view.pro_draft.as_ref().unwrap().state.recipe.nodes[0].enabled);
    });
    apply(&view, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let state = view
            .editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap();
        assert_eq!(state.recipe.nodes.len(), 2);
        assert!(
            state
                .recipe
                .nodes
                .iter()
                .any(|node| node.soft_mask.is_some())
        );
        assert_eq!(view.editor.undo_depth(), 1);
    });
    view.update(cx, |view, _| assert!(view.editor.undo()));
    assert_eq!(source_pixels(&view, cx), original);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(
            view.editor.document.layers[0]
                .advanced
                .as_ref()
                .unwrap()
                .recipe
                .nodes
                .is_empty()
        );
    });
}

#[gpui_kit::test]
fn blend_if_commit_keeps_split_ranges_for_an_actual_backdrop(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    open(&view, cx, Kind::Blend);
    click(cx, "pro-blend-source-1");
    click(cx, "pro-blend-backdrop-3");
    for (index, value) in [(4, "25"), (5, "76"), (6, "178"), (7, "230")] {
        set_input(&view, cx, index, value);
    }
    apply(&view, cx);
    let (source, settings) = view.update(cx, |view, _| {
        let layer = view
            .editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap();
        (
            layer.image.as_ref().unwrap().to_image(),
            layer.advanced.as_ref().unwrap().recipe.blend_if.unwrap(),
        )
    });
    let dark = RgbaImage::from_pixel(32, 24, Rgba([8, 8, 8, 255]));
    assert_eq!(settings.source_channel, BlendIfChannel::Red);
    assert_eq!(settings.backdrop_channel, BlendIfChannel::Blue);
    let middle = RgbaImage::from_pixel(32, 24, Rgba([128, 128, 128, 255]));
    let dark_result = apply_blend_if(&source, &dark, &settings).unwrap();
    let middle_result = apply_blend_if(&source, &middle, &settings).unwrap();
    assert_ne!(
        dark_result.get_pixel(16, 12)[3],
        middle_result.get_pixel(16, 12)[3]
    );
    assert!(validate_blend_if(&settings).is_ok());
}

#[gpui_kit::test]
fn warp_reopens_with_freeze_and_pins_then_cancel_preserves_the_commit(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| view.editor.select_rectangle(0., 0., 10., 24.));
    open(&view, cx, Kind::Warp);
    click(cx, "pro-freeze");
    let bounds = cx.debug_bounds("pro-source").unwrap();
    cx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width * 0.25,
            bounds.origin.y + bounds.size.height * 0.5,
        ),
        Modifiers::default(),
    );
    draw(cx);
    cx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width * 0.75,
            bounds.origin.y + bounds.size.height * 0.5,
        ),
        Modifiers::default(),
    );
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.pro_draft.as_ref().unwrap().freeze);
        assert_eq!(view.pro_draft.as_ref().unwrap().pins.len(), 1);
    });
    apply(&view, cx);
    let before_undo = view.update(cx, |view, _| {
        let state = view
            .editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap();
        match &state.recipe.nodes[0].operation {
            AdvancedOperation::Warp(mesh) => (mesh.pins.len(), mesh.freeze_mask.is_some()),
            _ => panic!("expected warp node"),
        }
    });
    assert_eq!(before_undo, (1, true));
    open(&view, cx, Kind::Warp);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.pro_draft.as_ref().unwrap().pins.len(), 1);
        assert!(view.pro_draft.as_ref().unwrap().freeze);
    });
    click(cx, "cancel-dialog");
    assert_eq!(view.update(cx, |view, _| view.editor.undo_depth()), 1);
}

#[gpui_kit::test]
fn refine_foreground_background_apply_inserts_layer_without_mutating_source(
    cx: &mut TestAppContext,
) {
    let (view, cx, _recovery) = setup(cx);
    let original = source_pixels(&view, cx);
    let source_id = view.update(cx, |view, _| view.editor.active_layer.clone());
    open(&view, cx, Kind::Refine);
    click(cx, "pro-correct-foreground");
    let bounds = cx.debug_bounds("pro-source").unwrap();
    cx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width * 0.25,
            bounds.origin.y + bounds.size.height * 0.5,
        ),
        Modifiers::default(),
    );
    draw(cx);
    click(cx, "pro-correct-background");
    cx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width * 0.75,
            bounds.origin.y + bounds.size.height * 0.5,
        ),
        Modifiers::default(),
    );
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).pro_draft.as_ref().unwrap().corrections.len(),
            2
        )
    });
    wait_for_preview(&view, cx);
    click(cx, "pro-background-4");
    cx.run_until_parked();
    draw(cx);
    apply(&view, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.document.layers.len(), 2);
        let source = view.editor.document.find_layer(&source_id).unwrap();
        assert!(!source.visible);
        assert_eq!(source.image.as_ref().unwrap().to_image(), original);
        assert!(
            view.editor
                .document
                .layers
                .iter()
                .any(|layer| layer.name.contains("refined"))
        );
    });
}

#[gpui_kit::test]
fn remove_uses_only_the_allowed_sampling_rectangle(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| view.editor.select_rectangle(0., 0., 8., 8.));
    let source_id = view.update(cx, |view, _| view.editor.active_layer.clone());
    open(&view, cx, Kind::Remove);
    for (index, value) in [(0, "0"), (1, "0"), (2, "16"), (3, "24")] {
        set_input(&view, cx, index, value);
    }
    apply(&view, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let source = view.editor.document.find_layer(&source_id).unwrap();
        assert!(!source.visible);
        let derived = view.editor.document.layers.last().unwrap();
        let state = derived.advanced.as_ref().unwrap();
        let node = state.recipe.nodes.last().unwrap();
        let AdvancedOperation::ContentAwareReplace(ContentAwareReplace {
            allowed_source_mask,
            target_mask,
            ..
        }) = &node.operation
        else {
            panic!("expected content-aware operation")
        };
        assert_eq!(target_mask.data[(2 * target_mask.width + 2) as usize], 255);
        assert_eq!(
            allowed_source_mask.data[(2 * allowed_source_mask.width + 2) as usize],
            0
        );
        assert_eq!(
            allowed_source_mask.data[(12 * allowed_source_mask.width + 12) as usize],
            255
        );
        assert_eq!(
            allowed_source_mask.data[(12 * allowed_source_mask.width + 20) as usize],
            0
        );
    });
}

#[gpui_kit::test]
fn frequency_layers_preserve_masked_nonunit_source_and_undo_visibility(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    view.update(cx, |view, _| {
        let layer = view
            .editor
            .document
            .find_layer_mut(&view.editor.active_layer)
            .unwrap();
        layer.opacity = 0.63;
        layer.mask = Some(RgbaImage::from_pixel(32, 24, Rgba([128, 128, 128, 255])).into());
        layer.metadata["maskEnabled"] = serde_json::Value::Bool(true);
    });
    let before = view.update(cx, |view, _| raster::composite(&view.editor.document));
    open(&view, cx, Kind::Retouch);
    click(cx, "pro-retouch-0");
    apply(&view, cx);
    let after = view.update(cx, |view, _| {
        let source = &view.editor.document.layers[0];
        assert!(!source.visible);
        assert_eq!(view.editor.document.layers.len(), 2);
        assert_eq!(view.editor.document.layers[1].children.len(), 2);
        assert!((view.editor.document.layers[1].opacity - 0.63).abs() < f32::EPSILON);
        raster::composite(&view.editor.document)
    });
    for (a, b) in before.pixels().zip(after.pixels()) {
        for channel in 0..4 {
            assert!((i16::from(a[channel]) - i16::from(b[channel])).abs() <= 1);
        }
    }
    view.update(cx, |view, _| assert!(view.editor.undo()));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.document.layers.len(), 1);
        assert!(view.editor.document.layers[0].visible);
        assert_eq!(raster::composite(&view.editor.document), before);
    });
}

#[gpui_kit::test]
fn brush_apply_cancel_and_dirty_close_cancel_pending_job(cx: &mut TestAppContext) {
    let (view, cx, _recovery) = setup(cx);
    open(&view, cx, Kind::Brush);
    set_input(&view, cx, 0, "12");
    apply(&view, cx);
    let applied = view.update(cx, |view, _| view.editor.brush_dynamics.clone().unwrap());
    assert!((applied.size - 12.).abs() < f32::EPSILON);
    open(&view, cx, Kind::Brush);
    click(cx, "cancel-dialog");
    assert_eq!(
        view.update(cx, |view, _| view.editor.brush_dynamics.clone().unwrap()),
        applied
    );

    view.update(cx, |view, _| view.editor.mark_unsaved());
    open(&view, cx, Kind::Stack);
    let cancel = view.update(cx, |view, cx| {
        view.run_pro(true, cx);
        view.pro_draft.as_ref().unwrap().cancel.clone()
    });
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::Unsaved);
        assert!(view.pro_draft.is_none());
        assert!(!view.busy);
        assert!(cancel.load(Ordering::Relaxed));
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.dialog, Dialog::None);
        assert!(view.focus.is_focused(window));
    });
}
