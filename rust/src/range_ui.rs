//! A disposable colour/tonal selection draft. Only Apply mutates the editor.
use super::inspector_ui::panel_button as button;
use super::*;
use omuse::range_mask::{RangeKind, RangeSettings};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(test, feature = "ui-test"))]
#[path = "range_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RangeOutput {
    Selection(SelectionMode),
    LayerMask,
}

pub(super) struct RangeDraft {
    pub color: bool,
    hue: bool,
    source: Arc<image::RgbaImage>,
    layer: String,
    source_preview: Arc<RenderImage>,
    preview: Option<Arc<RenderImage>>,
    sample_bounds: Rc<Cell<Bounds<Pixels>>>,
    invert: bool,
    output: RangeOutput,
    computed: Option<(RangeSettings, Arc<image::GrayImage>)>,
    cancel: Arc<AtomicBool>,
    job: u64,
    window: gpui_kit::AnyWindowHandle,
}

impl Drop for RangeDraft {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl EditorView {
    pub(super) fn set_range_hue_mode(
        &mut self,
        hue: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.range_draft.as_mut() else {
            return;
        };
        if !draft.color || draft.hue == hue {
            return;
        }
        draft.hue = hue;
        for (input, value) in self.detail_inputs.iter().zip(if hue {
            ["20", "15", "10"]
        } else {
            ["10", "15", "0"]
        }) {
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        self.schedule_range_preview(cx);
    }

    pub(super) fn cancel_range(&mut self, cx: &mut App) {
        if self.range_draft.is_some() {
            self.clear_range(cx);
            self.status = "Range preview cancelled".into();
        }
    }

    pub(super) fn range_preview_ready(&self) -> bool {
        !self.busy
            && self
                .range_draft
                .as_ref()
                .is_some_and(|draft| draft.computed.is_some())
    }

    pub(super) fn set_range_output(&mut self, output: RangeOutput, cx: &mut Context<Self>) {
        if let Some(draft) = self.range_draft.as_mut() {
            draft.output = output;
        }
        // Changing the destination while Apply is running invalidates it.
        if self.busy {
            self.schedule_range_preview(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn clear_range(&mut self, cx: &mut App) {
        if let Some(mut draft) = self.range_draft.take() {
            draft.cancel.store(true, Ordering::Relaxed);
            cx.drop_image(draft.source_preview.clone(), None);
            if let Some(image) = draft.preview.take() {
                cx.drop_image(image, None);
            }
            self.busy = false;
        }
    }

    pub(super) fn open_range(&mut self, color: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.floating_selection_layer().is_some() {
            self.status = "Commit or cancel the floating selection first".into();
            cx.notify();
            return;
        }
        if u64::from(self.pixels.width()) * u64::from(self.pixels.height()) > 16_777_216 {
            self.status = "Range selections support canvases up to 16 million pixels".into();
            cx.notify();
            return;
        }
        self.clear_range(cx);
        let values = if color {
            ["10", "15", "0"]
        } else {
            ["170", "255", "48"]
        };
        for (input, value) in self.detail_inputs.iter().zip(values) {
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        let [r, g, b, _] = self.editor.brush.color;
        self.set_dialog_rgb(
            r as f32 / 255.,
            g as f32 / 255.,
            b as f32 / 255.,
            window,
            cx,
        );
        let source = Arc::new(self.pixels.clone());
        // Nearest-neighbour thumbnail generation samples only the output pixels.
        let (width, height) = thumbnail_size(source.width(), source.height());
        let small = image::imageops::resize(
            &*source,
            width,
            height,
            image::imageops::FilterType::Nearest,
        );
        self.range_draft = Some(RangeDraft {
            color,
            hue: false,
            source,
            layer: self.editor.active_layer.clone(),
            source_preview: render_image(&small),
            preview: None,
            sample_bounds: Rc::new(Cell::new(Bounds::default())),
            invert: false,
            output: RangeOutput::Selection(SelectionMode::Replace),
            computed: None,
            cancel: Arc::new(AtomicBool::new(false)),
            job: 0,
            window: window.window_handle(),
        });
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.dialog = Dialog::RangeMask;
        self.modal_focus.focus(window, cx);
        self.schedule_range_preview(cx);
    }

    fn range_settings(&self, cx: &Context<Self>) -> anyhow::Result<RangeSettings> {
        let draft = self
            .range_draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Range draft closed"))?;
        let value = |i: usize| -> anyhow::Result<f32> {
            self.detail_inputs[i]
                .read(cx)
                .value()
                .parse::<f32>()
                .map_err(|_| anyhow::anyhow!("Enter a number for each range setting"))
        };
        let kind = if draft.color && draft.hue {
            RangeKind::Hue {
                rgb: [
                    self.dialog_color[0],
                    self.dialog_color[1],
                    self.dialog_color[2],
                ],
                tolerance_degrees: value(0)?,
                feather_degrees: value(1)?,
                minimum_saturation: value(2)? / 100.,
            }
        } else if draft.color {
            RangeKind::Color {
                rgb: [
                    self.dialog_color[0],
                    self.dialog_color[1],
                    self.dialog_color[2],
                ],
                tolerance: value(0)? / 100.,
                feather: value(1)? / 100.,
            }
        } else {
            RangeKind::Luminosity {
                low: value(0)? / 255.,
                high: value(1)? / 255.,
                feather: value(2)? / 255.,
            }
        };
        let settings = RangeSettings {
            kind,
            invert: draft.invert,
        };
        omuse::range_mask::validate(settings)?;
        Ok(settings)
    }

    /// Coalesce text edits and stop superseded row work; an older completion can
    /// never clear the busy state or apply a mask belonging to a newer request.
    pub(super) fn schedule_range_preview(&mut self, cx: &mut Context<Self>) {
        if self.dialog != Dialog::RangeMask {
            return;
        }
        let Some(draft) = self.range_draft.as_mut() else {
            return;
        };
        draft.cancel.store(true, Ordering::Relaxed);
        draft.cancel = Arc::new(AtomicBool::new(false));
        draft.job = draft.job.wrapping_add(1);
        draft.computed = None;
        if let Some(image) = draft.preview.take() {
            cx.drop_image(image, None);
        }
        let job = draft.job;
        let generation = self.dialog_generation;
        self.busy = false;
        self.status = "Updating range preview…".into();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog == Dialog::RangeMask
                    && this.dialog_generation == generation
                    && this
                        .range_draft
                        .as_ref()
                        .is_some_and(|draft| draft.job == job)
                {
                    this.run_range(false, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn run_range(&mut self, apply: bool, cx: &mut Context<Self>) {
        if self.dialog != Dialog::RangeMask {
            return;
        }
        let settings = match self.range_settings(cx) {
            Ok(settings) => settings,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let Some(draft) = self.range_draft.as_mut() else {
            return;
        };
        if apply
            && let Some((previous, mask)) = &draft.computed
            && *previous == settings
        {
            let mask = mask.clone();
            self.commit_range(&mask, cx);
            return;
        }
        draft.cancel.store(true, Ordering::Relaxed);
        draft.cancel = Arc::new(AtomicBool::new(false));
        draft.job = draft.job.wrapping_add(1);
        let job = draft.job;
        let generation = self.dialog_generation;
        let cancel = draft.cancel.clone();
        let source = draft.source.clone();
        let output = draft.output;
        self.busy = true;
        self.status = if apply {
            "Preparing range mask…"
        } else {
            "Updating range preview…"
        }
        .into();
        let task = cx.background_executor().spawn(async move {
            let mask = omuse::range_mask::mask_cancellable(&source, settings, &cancel)?;
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Range preview cancelled");
            let (w, h) = thumbnail_size(mask.width(), mask.height());
            let small = image::imageops::resize(&mask, w, h, image::imageops::FilterType::Triangle);
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Range preview cancelled");
            let preview = image::RgbaImage::from_fn(w, h, |x, y| {
                let v = small.get_pixel(x, y)[0];
                image::Rgba([v, v, v, 255])
            });
            Ok::<_, anyhow::Error>((Arc::new(mask), preview))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::RangeMask
                    || this.dialog_generation != generation
                    || !this
                        .range_draft
                        .as_ref()
                        .is_some_and(|draft| draft.job == job)
                {
                    return;
                }
                this.busy = false;
                // Covers direct form edits as well as normal input notifications.
                if this.range_settings(cx).ok() != Some(settings) {
                    this.schedule_range_preview(cx);
                    return;
                }
                match result {
                    Ok((mask, preview)) => {
                        let draft = this.range_draft.as_mut().unwrap();
                        if let Some(old) = draft.preview.replace(render_image(&preview)) {
                            cx.drop_image(old, None);
                        }
                        draft.computed = Some((settings, mask.clone()));
                        if apply && output == draft.output {
                            this.commit_range(&mask, cx);
                        } else {
                            this.status =
                                "White is selected · grey is partial · black is excluded".into();
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        this.status = format!("Range mask: {error:#}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn commit_range(&mut self, mask: &image::GrayImage, cx: &mut Context<Self>) {
        let Some(draft) = self.range_draft.as_ref() else {
            return;
        };
        if mask.dimensions() != self.pixels.dimensions()
            || *draft.source != self.pixels
            || self.editor.floating_selection_layer().is_some()
        {
            self.status = "The canvas changed; close this draft and open the range again".into();
            cx.notify();
            return;
        }
        let output = draft.output;
        let window = draft.window;
        match output {
            RangeOutput::Selection(mode) => {
                let previous = self.editor.selection.clone();
                let incoming = Selection {
                    width: mask.width(),
                    height: mask.height(),
                    mask: mask.as_raw().clone(),
                };
                self.editor.selection = Some(omuse::selection_tools::combine(
                    previous.as_ref(),
                    &incoming,
                    mode,
                ));
                self.editor.record_selection_change(previous);
                self.selection_box = self
                    .editor
                    .selection
                    .as_ref()
                    .and_then(Selection::bounds)
                    .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
                self.status = "Range selection applied".into();
            }
            RangeOutput::LayerMask => match self.editor.replace_canvas_mask(&draft.layer, mask) {
                Ok(true) => self.status = "Range applied as an editable layer mask".into(),
                Ok(false) => {
                    self.status =
                        "Mask unchanged; check whether the layer or its group is locked".into();
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.status = format!("Layer mask: {error:#}");
                    cx.notify();
                    return;
                }
            },
        }
        self.clear_range(cx);
        self.dialog = Dialog::None;
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        if output == RangeOutput::LayerMask {
            self.changed(cx);
        } else {
            self.refresh(cx);
        }
        let focus = self.focus.clone();
        cx.defer(move |cx| {
            let _ = cx.update_window(window, |_, window, cx| focus.focus(window, cx));
        });
    }

    fn sample_range_color(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.range_draft.as_ref().filter(|draft| draft.color) else {
            return;
        };
        let bounds = draft.sample_bounds.get();
        let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width);
        let y = f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height);
        if !(0. ..1.).contains(&x) || !(0. ..1.).contains(&y) {
            return;
        }
        let pixel = draft.source.get_pixel(
            (x * draft.source.width() as f32) as u32,
            (y * draft.source.height() as f32) as u32,
        );
        if pixel[3] == 0 {
            self.status = "Choose a visible pixel to sample its colour".into();
            cx.notify();
            return;
        }
        self.set_dialog_rgb(
            pixel[0] as f32 / 255.,
            pixel[1] as f32 / 255.,
            pixel[2] as f32 / 255.,
            window,
            cx,
        );
        self.schedule_range_preview(cx);
    }

    pub(super) fn range_controls(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.range_draft.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let mut controls = div().flex().flex_col().gap_2().text_sm();
        let source = range_image(
            "range-source-preview",
            Some(draft.source_preview.clone()),
            draft.source.dimensions(),
            Some(draft.sample_bounds.clone()),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                this.sample_range_color(event.position, window, cx);
                cx.stop_propagation();
            }),
        );
        controls = controls.child(
            div()
                .flex()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(if draft.color {
                            "Visible canvas · click to sample"
                        } else {
                            "Visible canvas"
                        })
                        .child(source),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child("Selection mask")
                        .child(range_image(
                            "range-mask-preview",
                            draft.preview.clone(),
                            draft.source.dimensions(),
                            None,
                        )),
                ),
        );
        if draft.color {
            let mut methods = div().flex().items_center().gap_2();
            for (id, label, hue) in [
                ("range-rgb-mode", "RGB distance", false),
                ("range-hue-mode", "Hue range", true),
            ] {
                methods = methods.child(
                    button(id, label, ButtonVariant::Secondary, cx)
                        .selected(draft.hue == hue)
                        .debug_selector(move || id.into())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_range_hue_mode(hue, window, cx);
                        })),
                );
            }
            controls = controls.child(methods.child(div().flex_1()).child("Sample").child(
                color_picker("range-color", &self.dialog_color_picker, window, cx),
            ));
        } else {
            let mut presets = div().flex().gap_2();
            for (id, label, values) in [
                ("range-shadows", "Shadows", ["0", "85", "48"]),
                ("range-midtones", "Midtones", ["85", "170", "48"]),
                ("range-highlights", "Highlights", ["170", "255", "48"]),
            ] {
                presets = presets.child(
                    button(id, label, ButtonVariant::Outline, cx)
                        .debug_selector(move || id.into())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            for (input, value) in this.detail_inputs.iter().zip(values) {
                                input.update(cx, |state, cx| state.set_value(value, window, cx));
                            }
                            this.schedule_range_preview(cx);
                        })),
                );
            }
            controls = controls.child(presets);
        }
        let labels: &[&str] = if draft.color && draft.hue {
            &[
                "Hue tolerance (0–180°)",
                "Softness (0–180°)",
                "Min. saturation (%)",
            ]
        } else if draft.color {
            &["Tolerance (0–100%)", "Softness (0–100%)"]
        } else {
            &["From (0–255)", "To (0–255)", "Softness (0–255)"]
        };
        let mut fields = div().flex().gap_2();
        for (i, label) in labels.iter().enumerate() {
            fields = fields.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .debug_selector(move || format!("range-value-{i}").into())
                    .child(*label)
                    .child(input(
                        SharedString::from(format!("range-value-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )),
            );
        }
        controls = controls.child(fields).child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    button(
                        "range-invert",
                        if draft.invert {
                            "Invert: on"
                        } else {
                            "Invert: off"
                        },
                        ButtonVariant::Outline,
                        cx,
                    )
                    .debug_selector(|| "range-invert".into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(draft) = this.range_draft.as_mut() {
                            draft.invert = !draft.invert;
                        }
                        this.schedule_range_preview(cx);
                    })),
                )
                .child(
                    button(
                        "range-preview",
                        "Refresh preview",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .debug_selector(|| "range-preview".into())
                    .on_click(cx.listener(|this, _, _, cx| this.run_range(false, cx))),
                ),
        );
        controls = controls.child(div().text_color(t.secondary).child("OUTPUT"));
        let mut outputs = div().flex().gap_2();
        for (id, label, output) in [
            (
                "range-replace",
                "Selection",
                RangeOutput::Selection(SelectionMode::Replace),
            ),
            (
                "range-add",
                "Add",
                RangeOutput::Selection(SelectionMode::Add),
            ),
            (
                "range-subtract",
                "Subtract",
                RangeOutput::Selection(SelectionMode::Subtract),
            ),
            (
                "range-intersect",
                "Intersect",
                RangeOutput::Selection(SelectionMode::Intersect),
            ),
            ("range-layer-mask", "Layer mask", RangeOutput::LayerMask),
        ] {
            outputs = outputs.child(
                button(
                    id,
                    label,
                    if draft.output == output {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .debug_selector(move || id.into())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_range_output(output, cx);
                })),
            );
        }
        let note = if draft.output == RangeOutput::LayerMask {
            let name = self
                .editor
                .document
                .find_layer(&draft.layer)
                .map(|layer| layer.name.as_str())
                .unwrap_or("Missing layer");
            format!("Replace mask · undo supported · {name}")
        } else {
            "Visible canvas · transparent pixels excluded · undo supported".into()
        };
        controls
            .child(outputs)
            .child(div().text_color(t.secondary).truncate().child(note))
            .into_any_element()
    }
}

fn thumbnail_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (240. / width as f64).min(128. / height as f64).min(1.);
    (
        (width as f64 * scale).round().max(1.) as u32,
        (height as f64 * scale).round().max(1.) as u32,
    )
}

pub(super) fn range_image(
    id: &'static str,
    image: Option<Arc<RenderImage>>,
    dimensions: (u32, u32),
    sample_bounds: Option<Rc<Cell<Bounds<Pixels>>>>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(128.))
        .w_full()
        .overflow_hidden()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _cx| {
                    let scale = (f32::from(bounds.size.width) / dimensions.0 as f32)
                        .min(f32::from(bounds.size.height) / dimensions.1 as f32);
                    let target_size = size(
                        px(dimensions.0 as f32 * scale),
                        px(dimensions.1 as f32 * scale),
                    );
                    let target = Bounds::new(
                        point(
                            bounds.origin.x + (bounds.size.width - target_size.width) / 2.,
                            bounds.origin.y + (bounds.size.height - target_size.height) / 2.,
                        ),
                        target_size,
                    );
                    if let Some(sample) = &sample_bounds {
                        sample.set(target);
                    }
                    window.paint_quad(fill(bounds, rgb(0x111318)));
                    if let Some(image) = &image {
                        let _ = window.paint_image(
                            bounds,
                            target,
                            Corners::default(),
                            image.clone(),
                            0,
                            false,
                        );
                    }
                },
            )
            .size_full(),
        )
}
