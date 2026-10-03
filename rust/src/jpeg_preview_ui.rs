//! Inspect the actual encoded JPEG at fit or native pixel scale.
use super::*;

#[derive(Default)]
pub(super) struct JpegInspection {
    view: omuse::image_inspection::ImageInspection,
    pointer: Option<Point<Pixels>>,
    viewport: Rc<Cell<[f32; 2]>>,
}

impl EditorView {
    pub(super) fn jpeg_export_form(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut form = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        inspector_ui::panel_input("path", &self.path_input, window, cx)
                            .flex_1()
                            .min_w_0()
                            .debug_selector(|| "jpeg-export-path".into()),
                    )
                    .child(
                        inspector_ui::panel_button("browse", "Browse…", ButtonVariant::Outline, cx)
                            .debug_selector(|| "jpeg-export-browse".into())
                            .disabled(self.photo_io.is_some())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.native_browse(window, cx)),
                            ),
                    ),
            )
            .child(self.jpeg_export_settings(window, cx));
        if let Some((preview, bytes, width, height)) = self.jpeg_preview.clone() {
            // Keep the complete preview above the fixed actions at 800×600,
            // and give larger windows more room for image inspection.
            let height_px = (f32::from(window.viewport_size().height) - 400.).clamp(200., 400.);
            form = form.child(self.jpeg_inspection_panel(
                preview,
                bytes,
                width,
                height,
                px(height_px),
                cx,
            ));
        }
        form
    }

    fn jpeg_export_settings(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let field = |index: usize, label: &str, window: &mut Window, cx: &mut Context<Self>| {
            let id = format!("export-option-{index}");
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(inspector_ui::panel_note(label, cx).whitespace_nowrap())
                .child(
                    inspector_ui::panel_input(
                        SharedString::from(id.clone()),
                        &self.detail_inputs[index],
                        window,
                        cx,
                    )
                    .flex_1()
                    .min_w(px(44.))
                    .debug_selector(move || id.clone()),
                )
        };
        let matte = (1..=3)
            .map(|index| {
                self.detail_inputs[index]
                    .read(cx)
                    .value()
                    .parse::<u8>()
                    .ok()
            })
            .collect::<Option<Vec<_>>>()
            .map(|rgb| [rgb[0], rgb[1], rgb[2], 255]);
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(field(0, "Quality · 1–100", window, cx))
                    .child(field(4, "DPI", window, cx))
                    .child(
                        inspector_ui::panel_button(
                            "jpeg-preview",
                            if self.jpeg_preview_task.is_some() {
                                "Encoding…"
                            } else {
                                "Preview JPEG"
                            },
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "jpeg-preview".into())
                        .disabled(
                            self.jpeg_preview_task.is_some()
                                || self.busy
                                || self.photo_io.is_some(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.start_jpeg_preview(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .w(px(74.))
                            .gap_2()
                            .child(
                                inspector_ui::panel_note("Matte", cx)
                                    .whitespace_nowrap()
                                    .debug_selector(|| "jpeg-matte-label".into()),
                            )
                            .child(inspector_ui::colour_swatch(matte, false, cx)),
                    )
                    .child(field(1, "Red", window, cx))
                    .child(field(2, "Green", window, cx))
                    .child(field(3, "Blue", window, cx)),
            )
            .child(inspector_ui::panel_note(
                "Matte replaces transparency · RGB 0–255",
                cx,
            ))
    }

    pub(super) fn jpeg_inspection_panel(
        &self,
        preview: Arc<RenderImage>,
        bytes: usize,
        width: u32,
        height: u32,
        preview_height: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let actual = self.jpeg_inspection.view.actual_size;
        let view = self.jpeg_inspection.view;
        let viewport = self.jpeg_inspection.viewport.clone();
        let mut controls = div()
            .flex()
            .items_center()
            .min_w_0()
            .flex_shrink_0()
            .gap_1()
            .child(
                inspector_ui::panel_note(
                    format!("{width} × {height} px · {:.1} KB", bytes as f64 / 1024.),
                    cx,
                )
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis(),
            )
            .child(
                button(
                    "jpeg-fit",
                    "Fit",
                    if actual {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Secondary
                    },
                    cx,
                )
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .h(px(28.))
                .px_2()
                .py_0()
                .text_size(px(12.))
                .line_height(px(16.))
                .accessibility_label("Fit the encoded JPEG inside the preview")
                .debug_selector(|| "jpeg-fit".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.jpeg_inspection.view.actual_size = false;
                    this.jpeg_inspection.pointer = None;
                    cx.notify();
                })),
            )
            .child(
                button(
                    "jpeg-actual",
                    "100%",
                    if actual {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .h(px(28.))
                .px_2()
                .py_0()
                .text_size(px(12.))
                .line_height(px(16.))
                .accessibility_label("100 percent: one image pixel per screen pixel")
                .debug_selector(|| "jpeg-actual".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.jpeg_inspection.view.actual_size = true;
                    this.jpeg_inspection.pointer = None;
                    cx.notify();
                })),
            );
        for (id, label, accessible, delta) in [
            ("jpeg-left", "←", "Pan JPEG preview left", [80., 0.]),
            ("jpeg-right", "→", "Pan JPEG preview right", [-80., 0.]),
            ("jpeg-up", "↑", "Pan JPEG preview up", [0., 60.]),
            ("jpeg-down", "↓", "Pan JPEG preview down", [0., -60.]),
        ] {
            controls = controls.child(
                button(id, label, ButtonVariant::Outline, cx)
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(px(28.))
                    .h(px(28.))
                    .px_0()
                    .py_0()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .accessibility_label(accessible)
                    .disabled(!actual)
                    .debug_selector(move || id.into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.jpeg_inspection.view.drag(
                            delta,
                            [width, height],
                            this.jpeg_inspection.viewport.get(),
                            window.scale_factor(),
                        );
                        cx.notify();
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_2()
            .child(controls)
            .child(
                div()
                    .id("jpeg-inspection")
                    .debug_selector(|| "jpeg-inspection".into())
                    .w_full()
                    .h(preview_height)
                    .flex_shrink_0()
                    .overflow_hidden()
                    .bg(cx.omarchy().inset)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.jpeg_inspection.pointer = this
                                .jpeg_inspection
                                .view
                                .actual_size
                                .then_some(event.position);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                            if event.pressed_button != Some(MouseButton::Left) {
                                this.jpeg_inspection.pointer = None;
                                return;
                            }
                            if let Some(previous) = this.jpeg_inspection.pointer {
                                let delta = event.position - previous;
                                this.jpeg_inspection.view.drag(
                                    [f32::from(delta.x), f32::from(delta.y)],
                                    [width, height],
                                    this.jpeg_inspection.viewport.get(),
                                    window.scale_factor(),
                                );
                                this.jpeg_inspection.pointer = Some(event.position);
                                cx.notify();
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.jpeg_inspection.pointer = None),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.jpeg_inspection.pointer = None),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                viewport.set([
                                    f32::from(bounds.size.width),
                                    f32::from(bounds.size.height),
                                ])
                            },
                            move |bounds, _, window, _| {
                                let device = window.scale_factor();
                                let layout = view.layout(
                                    [width, height],
                                    [f32::from(bounds.size.width), f32::from(bounds.size.height)],
                                    device,
                                );
                                let mut x = f32::from(bounds.origin.x) + layout.origin[0];
                                let mut y = f32::from(bounds.origin.y) + layout.origin[1];
                                if view.actual_size {
                                    x = (x * device).round() / device;
                                    y = (y * device).round() / device;
                                }
                                let target = Bounds::new(
                                    point(px(x), px(y)),
                                    size(px(layout.size[0]), px(layout.size[1])),
                                );
                                let _ = window.paint_image(
                                    bounds,
                                    target,
                                    Corners::default(),
                                    preview.clone(),
                                    0,
                                    false,
                                );
                            },
                        )
                        .w_full()
                        .h_full(),
                    ),
            )
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext};

    #[gpui_kit::test]
    fn compact_jpeg_export_keeps_image_settings_and_actions_visible(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("Inspect.jpg");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            // The GPUI test display is scaled: use enough source pixels to
            // exceed the viewport even at physical-pixel 100% on that display.
            let mut doc = Document::new(2048, 600);
            doc.layers[0].image = Some(
                image::RgbaImage::from_fn(2048, 600, |x, y| {
                    image::Rgba([(x % 255) as u8, (y % 255) as u8, 100, 255])
                })
                .into(),
            );
            view.editor = Editor::new(doc);
            view.save_dialog(true, window, cx);
            view.path_input.update(cx, |input, cx| {
                input.set_value(destination.to_string_lossy().to_string(), window, cx)
            });
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        view.update(cx, |view, cx| view.start_jpeg_preview(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let identity = view.update(cx, |view, _| {
            assert!(view.jpeg_preview.is_some(), "{}", view.status);
            (
                view.editor.revision(),
                view.editor.undo_depth(),
                view.editor.document.layers[0].image.clone(),
            )
        });
        let body = cx.debug_bounds("dialog-body").unwrap();
        for selector in [
            "jpeg-export-path",
            "jpeg-export-browse",
            "export-option-0",
            "export-option-1",
            "export-option-2",
            "export-option-3",
            "export-option-4",
            "jpeg-preview",
            "jpeg-actual",
            "jpeg-fit",
            "jpeg-left",
            "jpeg-right",
            "jpeg-up",
            "jpeg-down",
            "jpeg-inspection",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.origin.x >= body.origin.x
                    && bounds.bottom_right().x <= body.bottom_right().x
                    && bounds.origin.y >= body.origin.y
                    && bounds.bottom_right().y <= body.bottom_right().y,
                "{selector} must be fully visible without scrolling at 800×600: {bounds:?} in {body:?}"
            );
        }
        let matte = cx.debug_bounds("jpeg-matte-label").unwrap();
        assert!(
            matte.size.height <= px(16.),
            "Matte label wrapped: {matte:?}"
        );
        let inspection = cx.debug_bounds("jpeg-inspection").unwrap();
        assert!(
            inspection.size.height >= px(200.) && inspection.size.width >= px(480.),
            "The encoded-image viewport must have useful visible area: {inspection:?}"
        );
        let footer = cx.debug_bounds("dialog-footer").unwrap();
        assert!(inspection.bottom_right().y <= footer.origin.y);
        for selector in ["cancel-dialog", "confirm-dialog"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.origin.x >= px(0.)
                    && bounds.bottom_right().x <= px(800.)
                    && bounds.origin.y >= footer.origin.y
                    && bounds.bottom_right().y <= px(600.),
                "{selector} must remain visible below the preview: {bounds:?}"
            );
        }
        view.update_in(cx, |view, window, _| {
            let (_, bytes, width, height) = view.jpeg_preview.as_ref().unwrap();
            assert!(*bytes > 0);
            let viewport = view.jpeg_inspection.viewport.get();
            let fit = view.jpeg_inspection.view.layout(
                [*width, *height],
                viewport,
                window.scale_factor(),
            );
            assert!(fit.origin[0] >= 0. && fit.origin[1] >= 0.);
            assert!(
                fit.origin[0] + fit.size[0] <= viewport[0] + 0.01
                    && fit.origin[1] + fit.size[1] <= viewport[1] + 0.01,
                "Fit must show the whole encoded image, not a clipped strip"
            );
            assert!(
                fit.size[1] >= 100.,
                "The landscape fixture must be large enough to inspect"
            );
        });
        let point = cx.debug_bounds("jpeg-actual").unwrap().center();
        cx.simulate_click(point, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, _| assert!(view.jpeg_inspection.view.actual_size));
        let point = cx.debug_bounds("jpeg-right").unwrap().center();
        cx.simulate_click(point, Modifiers::default());
        view.update(cx, |view, _| assert!(view.jpeg_inspection.view.pan[0] > 0.));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let point = cx.debug_bounds("jpeg-fit").unwrap().center();
        cx.simulate_click(point, Modifiers::default());
        view.update(cx, |view, _| {
            assert!(!view.jpeg_inspection.view.actual_size);
            assert_eq!(
                (
                    view.editor.revision(),
                    view.editor.undo_depth(),
                    view.editor.document.layers[0].image.clone()
                ),
                identity
            );
        });
        // The compact fields still use native text editing and invalidate the
        // encoded preview without changing the document or exporting a file.
        let point = cx.debug_bounds("export-option-1").unwrap().center();
        cx.simulate_click(point, Modifiers::default());
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("238");
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.detail_inputs[1].read(cx).value(), "238");
            assert!(view.jpeg_preview.is_none());
            assert_eq!(view.editor.revision(), identity.0);
            assert_eq!(view.editor.undo_depth(), identity.1);
            assert_eq!(view.editor.document.layers[0].image, identity.2);
        });
        assert!(!destination.exists());
    }
}
