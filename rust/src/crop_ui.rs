use super::*;
use omuse::crop::{CropFrame, PRESETS};

impl EditorView {
    pub(super) fn begin_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.floating_selection_layer().is_some() {
            self.status = "Commit or cancel the floating selection before cropping".into();
            return;
        }
        self.crop = Some(CropFrame::new(
            self.editor.document.width,
            self.editor.document.height,
            self.editor.selection.as_ref().and_then(Selection::bounds),
        ));
        self.status =
            "Crop · Drag corners to resize or inside to move · Enter applies · Escape cancels"
                .into();
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn apply_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(crop) = self.crop.take() else {
            return;
        };
        let (x, y, w, h) = crop.pixels();
        if self.editor.crop_canvas(x, y, w, h) {
            self.selection_box = None;
            // Keep the retained canvas pixels in the same screen location.
            self.pan.0 += (x as f32 + w as f32 * 0.5 - crop.canvas_size().0 * 0.5) * self.zoom;
            self.pan.1 += (y as f32 + h as f32 * 0.5 - crop.canvas_size().1 * 0.5) * self.zoom;
            self.changed(cx);
            self.status =
                format!("Cropped to {w} × {h} · Outside pixels retained · Ctrl+Z to undo");
        } else {
            self.status = "Crop left the canvas unchanged".into();
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn cancel_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.crop = None;
        self.status = "Crop cancelled · Artwork unchanged".into();
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn crop_context(&self, cx: &mut Context<Self>) -> AnyElement {
        let crop = self.crop.as_ref().unwrap();
        let t = cx.omarchy().clone();
        let mut presets = div()
            .id("crop-presets")
            .flex()
            .gap_1()
            .items_center()
            .min_w_0()
            .flex_1()
            .overflow_x_scroll();
        for (index, &label) in PRESETS.iter().enumerate() {
            let label = if crop.swapped {
                match index {
                    3 => "5:4",
                    4 => "2:3",
                    5 => "9:16",
                    _ => label,
                }
            } else {
                label
            };
            presets = presets.child(
                button(
                    ("crop-ratio", index),
                    label,
                    if crop.preset == index {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    cx,
                )
                .debug_selector(move || format!("crop-ratio-{index}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(crop) = &mut this.crop {
                        crop.set_preset(index);
                    }
                    this.focus.focus(window, cx);
                    cx.notify();
                })),
            );
        }
        div()
            .id("crop-context")
            .debug_selector(|| "crop-context".into())
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(44.))
            .flex_shrink_0()
            .bg(t.background)
            .border_b_1()
            .border_color(t.divider())
            .child(div().text_sm().child("Crop"))
            .child(presets)
            .child(
                button("crop-swap", "Swap", ButtonVariant::Outline, cx)
                    .disabled(crop.ratio.is_none())
                    .debug_selector(|| "crop-swap".into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(crop) = &mut this.crop {
                            crop.swap();
                        }
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            )
            .child(
                button("crop-cancel", "Cancel", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "crop-cancel".into())
                    .on_click(cx.listener(|this, _, window, cx| this.cancel_crop(window, cx))),
            )
            .child(
                button("crop-apply", "Apply", ButtonVariant::Primary, cx)
                    .debug_selector(|| "crop-apply".into())
                    .on_click(cx.listener(|this, _, window, cx| this.apply_crop(window, cx))),
            )
            .into_any_element()
    }
    pub(super) fn set_zoom(&mut self, next: f32, anchor: Option<Point<Pixels>>) {
        let bounds = self.viewport.get();
        let anchor = anchor
            .map(|p| {
                (
                    f32::from(p.x - bounds.center().x),
                    f32::from(p.y - bounds.center().y),
                )
            })
            .unwrap_or((0., 0.));
        (self.zoom, self.pan) =
            omuse::canvas_navigation::zoom_about(self.zoom, self.pan, next, anchor);
    }
}

pub(super) fn paint_crop(
    crop: omuse::crop::CropRect,
    rect: Bounds<Pixels>,
    clipped: Bounds<Pixels>,
    zoom: f32,
    window: &mut Window,
) {
    let x = rect.origin.x + px(crop.x * zoom);
    let y = rect.origin.y + px(crop.y * zoom);
    let w = px(crop.width * zoom);
    let h = px(crop.height * zoom);
    for band in [
        Bounds::new(rect.origin, size(rect.size.width, y - rect.origin.y)),
        Bounds::new(
            point(rect.origin.x, y + h),
            size(rect.size.width, rect.bottom() - y - h),
        ),
        Bounds::new(point(rect.origin.x, y), size(x - rect.origin.x, h)),
        Bounds::new(point(x + w, y), size(rect.right() - x - w, h)),
    ] {
        if band.size.width > px(0.) && band.size.height > px(0.) {
            window.paint_quad(fill(band.intersect(&clipped), rgba(0x00000099)));
        }
    }
    let frame = Bounds::new(point(x, y), size(w, h));
    window.paint_quad(outline(frame, rgba(0xffffffee), BorderStyle::Solid));
    for fraction in [1. / 3., 2. / 3.] {
        window.paint_quad(fill(
            Bounds::new(point(x + w * fraction, y), size(px(1.), h)).intersect(&clipped),
            rgba(0xffffff77),
        ));
        window.paint_quad(fill(
            Bounds::new(point(x, y + h * fraction), size(w, px(1.))).intersect(&clipped),
            rgba(0xffffff77),
        ));
    }
    for (cx, cy) in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
        window.paint_quad(fill(
            Bounds::new(point(cx - px(4.), cy - px(4.)), size(px(8.), px(8.))).intersect(&clipped),
            rgb(0xffffff),
        ));
    }
}
