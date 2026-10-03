//! Inspect the actual encoded JPEG at fit or native pixel scale.
use super::*;

#[derive(Default)]
pub(super) struct JpegInspection {
    view: omuse::image_inspection::ImageInspection,
    pointer: Option<Point<Pixels>>,
    viewport: Rc<Cell<[f32; 2]>>,
}

impl EditorView {
    pub(super) fn jpeg_export_settings(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let field = |index: usize, label: &str, window: &mut Window, cx: &mut Context<Self>| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(inspector_ui::panel_note(label, cx))
                .child(input(
                    SharedString::from(format!("export-option-{index}")),
                    &self.detail_inputs[index],
                    window,
                    cx,
                ))
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
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(field(0, "JPEG quality · 1–100", window, cx))
                    .child(field(4, "Resolution · DPI", window, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(inspector_ui::panel_note("Matte", cx))
                            .child(
                                div()
                                    .h(px(32.))
                                    .flex()
                                    .items_center()
                                    .child(inspector_ui::colour_swatch(matte, false, cx)),
                            ),
                    )
                    .child(field(1, "Red", window, cx))
                    .child(field(2, "Green", window, cx))
                    .child(field(3, "Blue", window, cx)),
            )
            .child(inspector_ui::panel_note(
                "Transparent areas use the matte colour · RGB 0–255",
                cx,
            ))
    }

    pub(super) fn jpeg_inspection_panel(
        &self,
        preview: Arc<RenderImage>,
        width: u32,
        height: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let actual = self.jpeg_inspection.view.actual_size;
        let view = self.jpeg_inspection.view;
        let viewport = self.jpeg_inspection.viewport.clone();
        let mut controls = div()
            .flex()
            .items_center()
            .gap_2()
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
                .debug_selector(|| "jpeg-actual".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.jpeg_inspection.view.actual_size = true;
                    this.jpeg_inspection.pointer = None;
                    cx.notify();
                })),
            );
        for (id, label, delta) in [
            ("jpeg-left", "←", [80., 0.]),
            ("jpeg-right", "→", [-80., 0.]),
            ("jpeg-up", "↑", [0., 60.]),
            ("jpeg-down", "↓", [0., -60.]),
        ] {
            controls = controls.child(
                button(id, label, ButtonVariant::Outline, cx)
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
            .gap_2()
            .child(controls)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.omarchy().secondary)
                    .child(if actual {
                        "100% · one image pixel per screen pixel · drag or use arrows"
                    } else {
                        "Fit · preview of the encoded file"
                    }),
            )
            .child(
                div()
                    .id("jpeg-inspection")
                    .debug_selector(|| "jpeg-inspection".into())
                    .w_full()
                    .h(px(200.))
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
    fn jpeg_inspection_controls_never_edit_or_export_the_document(cx: &mut TestAppContext) {
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
        cx.simulate_resize(size(px(1000.), px(800.)));
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
        for selector in ["jpeg-actual", "jpeg-fit", "jpeg-right"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.origin.y >= body.origin.y
                    && bounds.bottom_right().y <= body.bottom_right().y,
                "JPEG navigation must be visible without scrolling"
            );
        }
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
        assert!(!destination.exists());
    }
}
