//! Deterministic regression coverage for the compact Studio workspace.
//!
//! This module is included from `ui.rs` after the Studio layout is assembled.
//! It deliberately exercises the rendered controls through GPUI's test input
//! path, rather than calling `EditorView` methods directly.

use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn studio_document() -> Document {
    let mut document = Document::new(64, 48);
    document.layers[0].image =
        Some(image::RgbaImage::from_pixel(64, 48, image::Rgba([42, 96, 180, 255])).into());
    document
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be visible"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

fn assert_in_window(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be visible"));
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    assert!(
        left >= 0. && top >= 0.,
        "{selector} starts outside the window: {bounds:?}"
    );
    assert!(
        right <= 800. && bottom <= 600.,
        "{selector} exceeds 800x600: {bounds:?}"
    );
    assert!(
        bounds.size.width > px(0.) && bounds.size.height > px(0.),
        "{selector} is empty"
    );
}

fn tool_catalog() -> [Tool; 23] {
    [
        Tool::Brush,
        Tool::Pencil,
        Tool::Eraser,
        Tool::Fill,
        Tool::Gradient,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Move,
        Tool::Picker,
        Tool::Clone,
        Tool::Heal,
        Tool::SpotHeal,
        Tool::Wand,
        Tool::Object,
        Tool::ShapeRect,
        Tool::ShapeEllipse,
        Tool::Line,
        Tool::Lasso,
        Tool::Hand,
        Tool::BlurBrush,
        Tool::Smudge,
        Tool::Liquify,
        Tool::Text,
    ]
}

#[gpui_kit::test]
fn studio_minimum_window_keeps_every_workspace_component_usable(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let recovery = tempfile::tempdir().unwrap();
    let recovery_dir = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(recovery_dir);
        view.dialog = Dialog::None;
        view.editor = Editor::new(studio_document());
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);

    for selector in [
        "studio-header",
        "studio-context",
        "tools",
        "artwork",
        "inspector",
        "studio-footer",
        "inspector-toggle",
        "inspector-tab-layers",
        "inspector-tab-develop",
        "inspector-tab-selection",
        "inspector-tab-canvas",
        "tool-settings",
        "color-picker-trigger",
        "export",
        "save",
        "zoom-in",
    ] {
        assert_in_window(selector, cx);
    }
    let canvas = cx.debug_bounds("artwork").unwrap();
    assert!(
        canvas.size.width > px(350.),
        "minimum workspace leaves too little canvas width: {canvas:?}"
    );
    cx.update(|_, cx| assert!(view.read(cx).inspector_visible));
}

#[gpui_kit::test]
fn studio_tool_dock_selects_all_tools_at_minimum_size(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let recovery = tempfile::tempdir().unwrap();
    let recovery_dir = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(recovery_dir);
        view.dialog = Dialog::None;
        view.editor = Editor::new(studio_document());
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);

    for tool in tool_catalog() {
        assert_in_window(tool.studio_id(), cx);
        click(cx, tool.studio_id());
        cx.update(|_, cx| assert_eq!(view.read(cx).tool, tool, "{} did not select", tool.name()));
    }
}

#[gpui_kit::test]
fn studio_inspector_tabs_keep_representative_actions_reachable(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let recovery = tempfile::tempdir().unwrap();
    let recovery_dir = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(recovery_dir);
        view.dialog = Dialog::None;
        view.editor = Editor::new(studio_document());
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);

    click(cx, "inspector-tab-layers");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    cx.update(|_, cx| assert_eq!(view.read(cx).inspector_tab, studio_ui::InspectorTab::Layers));
    assert_in_window("add", cx);

    click(cx, "inspector-tab-develop");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).inspector_tab,
            studio_ui::InspectorTab::Develop
        )
    });
    assert_in_window("camera-raw", cx);
    click(cx, "camera-raw");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::CameraRaw));
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::None));
    draw(cx);

    click(cx, "inspector-tab-selection");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).inspector_tab,
            studio_ui::InspectorTab::Selection
        )
    });
    assert_in_window("select-subject", cx);
    click(cx, "select-all");
    assert_in_window("feather-selection", cx);
    click(cx, "feather-selection");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Selection));
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| {
        assert!(!view.read(cx).busy);
        assert_eq!(view.read(cx).dialog, Dialog::None);
    });
    draw(cx);

    click(cx, "inspector-tab-canvas");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    cx.update(|_, cx| assert_eq!(view.read(cx).inspector_tab, studio_ui::InspectorTab::Canvas));
    assert_in_window("resize", cx);
    click(cx, "resize");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Resize));
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::None));
}

#[gpui_kit::test]
fn studio_themes_preserve_geometry_pixels_and_inspector_toggle_expands_canvas(
    cx: &mut TestAppContext,
) {
    cx.update(crate::init_test_theme);
    let recovery = tempfile::tempdir().unwrap();
    let recovery_dir = recovery.path().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(recovery_dir);
        view.dialog = Dialog::None;
        view.editor = Editor::new(studio_document());
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    let before = cx.debug_bounds("artwork").unwrap();
    let pixels = cx.update(|_, cx| view.read(cx).pixels.clone());

    for theme in [
        gpui_omarchy::Theme::flexoki_light(),
        gpui_omarchy::Theme::tokyo_night(),
    ] {
        cx.update(|window, cx| {
            theme.apply(cx);
            window.draw(cx).clear(cx);
        });
        let after = cx.debug_bounds("artwork").unwrap();
        assert_eq!(before.origin, after.origin, "theme moved the canvas");
        assert_eq!(before.size, after.size, "theme changed workspace geometry");
        assert_eq!(cx.update(|_, cx| view.read(cx).pixels.clone()), pixels);
        assert_in_window("studio-header", cx);
        assert_in_window("studio-context", cx);
        assert_in_window("studio-footer", cx);
    }

    let before_toggle = cx.debug_bounds("artwork").unwrap();
    click(cx, "inspector-toggle");
    cx.update(|_, cx| assert!(!view.read(cx).inspector_visible));
    assert!(
        cx.debug_bounds("inspector").is_none(),
        "hidden inspector still occupies bounds"
    );
    let expanded = cx.debug_bounds("artwork").unwrap();
    assert!(expanded.size.width > before_toggle.size.width);
    click(cx, "inspector-toggle");
    cx.update(|_, cx| assert!(view.read(cx).inspector_visible));
    assert!(cx.debug_bounds("inspector").is_some());
}
