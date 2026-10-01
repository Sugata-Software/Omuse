//! Cancellable image tracing in the ordinary canvas and right inspector.
use super::inspector_ui::{
    panel_button as button, panel_header, panel_input, panel_note, panel_section, panel_width,
};
use super::*;
use omuse::{
    image_trace::{TraceMode, TraceOptions},
    image_trace_layer::{self, PreparedTrace},
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub(super) struct ImageTraceUi {
    draft: Option<TraceDraft>,
    working: bool,
    worker_cancel: Option<Arc<AtomicBool>>,
}

struct TraceDraft {
    source: String,
    target: Option<String>,
    source_name: String,
    revision: u64,
    identity: (u64, u64),
    token: Arc<AtomicBool>,
    generation: u64,
    inputs: Vec<Entity<InputState>>,
    mode: TraceMode,
    omit_white: bool,
    omit_background: bool,
    requested: Option<(u64, TraceOptions)>,
    latest: Option<TraceOptions>,
    prepared: Option<PreparedTrace>,
    display: Option<DisplaySurface>,
    original: Option<DisplaySurface>,
    show_original: bool,
    message: String,
    invalid: bool,
}

const FIELDS: [(&str, &str); 8] = [
    ("colours", "Colours · 1–32"),
    ("detail", "Detail · %"),
    ("smoothing", "Smoothing · %"),
    ("corners", "Keep corners · %"),
    ("noise", "Noise · px²"),
    ("threshold", "Threshold · 0–255"),
    ("resolution", "Process edge · px"),
    ("points", "Point limit"),
];

fn field_values(options: &TraceOptions) -> [String; 8] {
    [
        options.colors.to_string(),
        format!("{:.0}", (1. - options.detail) * 100.),
        format!("{:.0}", options.smoothing * 100.),
        format!("{:.0}", options.corner_preservation * 100.),
        options.speckle_area.to_string(),
        options.monochrome_threshold.to_string(),
        options.max_dimension.to_string(),
        options.max_points.to_string(),
    ]
}

impl EditorView {
    pub(super) fn image_trace_active(&self) -> bool {
        self.image_trace.draft.is_some()
    }

    pub(super) fn trace_command_allowed(name: &str) -> bool {
        matches!(
            name,
            "zoom-in"
                | "zoom-in-plus"
                | "zoom-out"
                | "fit"
                | "actual"
                | "toggle-panels"
                | "grid"
                | "guides"
                | "rulers"
                | "command-search"
                | "shortcuts"
                | "tool-hand"
                | "image-trace"
                | "quit"
        )
    }

    pub(super) fn guard_image_trace(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.image_trace_active() {
            return false;
        }
        self.status = "Keep vectors or cancel Image trace before another edit.".into();
        cx.notify();
        true
    }

    pub(super) fn trace_unavailable(&self) -> Option<&'static str> {
        if self.editor.floating_selection_layer().is_some() {
            return Some("Finish the floating selection first");
        }
        let Some(layer) = self.editor.document.find_layer(&self.editor.active_layer) else {
            return Some("Select an image layer");
        };
        if image_trace_layer::validate_target(&self.editor.document, &layer.id).is_err() {
            return Some("Unlock this layer and its parent groups before tracing");
        }
        if let Some((source, options)) = image_trace_layer::retained_settings(layer) {
            if options.validate().is_err()
                || image_trace_layer::source(&self.editor.document, &source).is_err()
            {
                return Some("The original image or retained trace settings are unavailable");
            }
        } else if image_trace_layer::source(&self.editor.document, &layer.id).is_err() {
            return Some("Select a pixel image of up to 16 megapixels");
        }
        None
    }

    pub(super) fn open_image_trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.image_trace_active() {
            self.inspector_visible = true;
            cx.notify();
            return;
        }
        if let Some(reason) = self.trace_unavailable() {
            self.status = reason.into();
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        let selected = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .unwrap();
        let (source, target, options) =
            if let Some((source, options)) = image_trace_layer::retained_settings(selected) {
                (source, Some(selected.id.clone()), options)
            } else {
                (selected.id.clone(), None, TraceOptions::default())
            };
        let source_name = self
            .editor
            .document
            .find_layer(&source)
            .unwrap()
            .name
            .clone();
        let inputs: Vec<_> = field_values(&options)
            .into_iter()
            .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)))
            .collect();
        for input in &inputs {
            cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_image_trace(cx);
                }
            })
            .detach();
        }
        self.image_trace.draft = Some(TraceDraft {
            source,
            target,
            source_name,
            revision: self.editor.revision(),
            identity: (self.editor.instance_id(), self.create.epoch),
            token: Arc::new(AtomicBool::new(false)),
            generation: 0,
            inputs,
            mode: options.mode,
            omit_white: options.omit_white,
            omit_background: options.omit_background,
            requested: None,
            latest: None,
            prepared: None,
            display: None,
            original: None,
            show_original: false,
            message: "Preparing editable artwork…".into(),
            invalid: false,
        });
        self.inspector_tab = studio_ui::InspectorTab::Layers;
        self.inspector_visible = true;
        self.tool = Tool::Move;
        self.focus.focus(window, cx);
        self.schedule_image_trace(cx);
    }

    fn trace_options(&self, cx: &App) -> anyhow::Result<TraceOptions> {
        let draft = self
            .image_trace
            .draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Image trace is closed"))?;
        let value = |index: usize| draft.inputs[index].read(cx).value().to_string();
        let percent = |index: usize| -> anyhow::Result<f32> {
            let v: f32 = value(index)
                .parse()
                .map_err(|_| anyhow::anyhow!("Enter a number for {}", FIELDS[index].1))?;
            anyhow::ensure!(
                v.is_finite() && (0. ..=100.).contains(&v),
                "{} must be 0–100",
                FIELDS[index].1
            );
            Ok(v / 100.)
        };
        let options = TraceOptions {
            mode: draft.mode,
            colors: value(0)
                .parse()
                .map_err(|_| anyhow::anyhow!("Colours must be 1–32"))?,
            detail: 1. - percent(1)?,
            smoothing: percent(2)?,
            corner_preservation: percent(3)?,
            speckle_area: value(4)
                .parse()
                .map_err(|_| anyhow::anyhow!("Noise must be a whole pixel area"))?,
            monochrome_threshold: value(5)
                .parse()
                .map_err(|_| anyhow::anyhow!("Threshold must be 0–255"))?,
            max_dimension: value(6)
                .parse()
                .map_err(|_| anyhow::anyhow!("Process edge must be 16–4096"))?,
            max_points: value(7)
                .parse()
                .map_err(|_| anyhow::anyhow!("Point limit must be 4–100000"))?,
            omit_white: draft.omit_white,
            omit_background: draft.omit_background,
        };
        options.validate()?;
        Ok(options)
    }

    fn schedule_image_trace(&mut self, cx: &mut Context<Self>) {
        if !self.image_trace_active() {
            return;
        }
        let options = self.trace_options(cx);
        let draft = self.image_trace.draft.as_mut().unwrap();
        if let Ok(options) = &options {
            if !draft.invalid && draft.latest.as_ref() == Some(options) {
                return;
            }
        }
        draft.generation = draft.generation.wrapping_add(1);
        draft.prepared = None;
        if let Some(cancel) = &self.image_trace.worker_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        match options {
            Ok(options) => {
                draft.latest = Some(options.clone());
                draft.requested = Some((draft.generation, options));
                draft.invalid = false;
                draft.message = "Updating trace… previous preview remains visible.".into();
                self.status = "Tracing image · Cancel leaves the original unchanged".into();
            }
            Err(error) => {
                draft.requested = None;
                draft.invalid = true;
                draft.message = error.to_string();
                self.status = error.to_string();
            }
        }
        self.start_image_trace(cx);
        cx.notify();
    }

    fn start_image_trace(&mut self, cx: &mut Context<Self>) {
        if self.image_trace.working {
            return;
        }
        let Some(draft) = self.image_trace.draft.as_mut() else {
            return;
        };
        let Some((generation, options)) = draft.requested.take() else {
            return;
        };
        let (source, target, revision, identity, session) = (
            draft.source.clone(),
            draft.target.clone(),
            draft.revision,
            draft.identity,
            draft.token.clone(),
        );
        let document = self.editor.document.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.image_trace.worker_cancel = Some(cancel.clone());
        self.image_trace.working = true;
        let proof = self.proof_settings.clone();
        let job = cx.background_executor().spawn(async move {
            let pixels = image_trace_layer::source(&document, &source)?;
            let trace = omuse::image_trace::trace(&pixels, &options, &cancel)?;
            let mut prepared = image_trace_layer::prepare(
                &document,
                &source,
                target.as_deref(),
                trace,
                &options,
                revision,
                identity.0,
                &cancel,
            )?;
            // The immutable candidate is sufficient for Keep; move its display
            // buffers out so a ready draft does not retain two extra full frames.
            let mut preview = std::mem::take(&mut prepared.preview);
            let mut original = std::mem::take(&mut prepared.source_preview);
            if proof.enabled {
                preview = omuse::proofing::render(&preview, &proof)?;
                original = omuse::proofing::render(&original, &proof)?;
            }
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Image trace cancelled");
            Ok::<_, anyhow::Error>((prepared, preview, original))
        });
        cx.spawn(async move |view,cx| {
            let result=job.await;
            let _=view.update(cx,|this,cx| {
                this.image_trace.working=false;
                this.image_trace.worker_cancel=None;
                let current = this.image_trace.draft.as_ref().is_some_and(|d|
                    Arc::ptr_eq(&d.token,&session) && d.generation==generation && d.revision==this.editor.revision()
                        && d.identity==(this.editor.instance_id(),this.create.epoch));
                if current {
                    let draft=this.image_trace.draft.as_mut().unwrap();
                    match result {
                        Ok((prepared,preview,original)) => {
                            drop_surface(draft.display.take(),cx); drop_surface(draft.original.take(),cx);
                            draft.display=Some(DisplaySurface::new(&preview));
                            draft.original=Some(DisplaySurface::new(&original));
                            let s=&prepared.stats;
                            let reduction=100.-(s.anchors as f64/s.points_before.max(1) as f64*100.);
                            draft.message=format!("{} points · {} paths · {:.0}% fewer points",s.anchors,s.subpaths,reduction.max(0.));
                            draft.invalid=false;
                            this.status="Trace ready · Compare Source/Trace, then Keep vectors to edit points".into();
                            draft.prepared=Some(prepared);
                        }
                        Err(error) => { draft.invalid=true; draft.message=format!("{error:#}"); this.status=format!("Image trace: {error:#}"); }
                    }
                }
                this.validate_image_trace(cx);
                this.start_image_trace(cx);
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn validate_image_trace(&mut self, cx: &mut Context<Self>) {
        if self.image_trace.draft.as_ref().is_some_and(|d| {
            d.revision != self.editor.revision()
                || d.identity != (self.editor.instance_id(), self.create.epoch)
        }) {
            self.clear_image_trace(cx);
            self.status =
                "Image trace closed because the document changed; original artwork is preserved."
                    .into();
        }
    }

    pub(super) fn clear_image_trace(&mut self, cx: &mut App) {
        if let Some(cancel) = &self.image_trace.worker_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(draft) = self.image_trace.draft.take() {
            draft.token.store(true, Ordering::Relaxed);
            drop_surface(draft.display, cx);
            drop_surface(draft.original, cx);
        }
    }

    pub(super) fn cancel_image_trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_image_trace(cx);
        self.status = "Image trace cancelled; your original and document are unchanged.".into();
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn keep_image_trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.validate_image_trace(cx);
        if self.trace_options(cx).is_err() || self.image_trace.working {
            return;
        }
        let Some(prepared) = self
            .image_trace
            .draft
            .as_mut()
            .and_then(|d| d.prepared.take())
        else {
            return;
        };
        match self.editor.apply_image_trace(prepared) {
            Ok(id) => {
                self.clear_image_trace(cx);
                self.select_layer_ids(vec![id]);
                self.changed(cx);
                self.open_vector_scene(window, cx);
                self.vector_before_command("vector-nodes", window, cx);
                self.status="Editable trace kept · A edits points, P adds curves · Original image retained in Layers".into();
            }
            Err(error) => {
                self.status = format!("Cannot keep trace: {error:#}");
                if let Some(draft) = self.image_trace.draft.as_mut() {
                    draft.invalid = true;
                    draft.message = self.status.clone();
                }
                self.validate_image_trace(cx);
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn image_trace_display(&self) -> Option<&DisplaySurface> {
        let d = self.image_trace.draft.as_ref()?;
        if d.show_original {
            d.original.as_ref()
        } else {
            d.display.as_ref()
        }
    }

    pub(super) fn image_trace_ready(&self) -> bool {
        self.image_trace
            .draft
            .as_ref()
            .is_some_and(|d| d.prepared.is_some() && !d.invalid && !self.image_trace.working)
    }

    fn trace_preset(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut options = TraceOptions::default();
        match kind {
            "logo" => {
                options.mode = TraceMode::Monochrome;
                options.colors = 2;
                options.detail = 0.7;
                options.smoothing = 0.55;
                options.speckle_area = 8;
                options.omit_white = true;
                options.max_points = 20_000;
            }
            "photo" => {
                options.colors = 16;
                options.detail = 0.65;
                options.smoothing = 0.6;
                options.corner_preservation = 0.4;
                options.speckle_area = 24;
                options.max_dimension = 768;
            }
            _ => {
                options.speckle_area = 8;
            }
        }
        if let Some(d) = self.image_trace.draft.as_mut() {
            d.mode = options.mode;
            d.omit_white = options.omit_white;
            d.omit_background = options.omit_background;
            for (input, value) in d.inputs.iter().zip(field_values(&options)) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        self.schedule_image_trace(cx);
    }

    pub(super) fn image_trace_context(&self, cx: &mut Context<Self>) -> AnyElement {
        let d = self.image_trace.draft.as_ref().unwrap();
        let mut compare = div().flex().gap_1().flex_1().min_w_0();
        for (id, label, source) in [
            ("trace-source", "Source", true),
            ("trace-result", "Trace", false),
        ] {
            compare = compare.child(
                button(id, label, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .selected(d.show_original == source)
                    .disabled(d.original.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(d) = this.image_trace.draft.as_mut() {
                            d.show_original = source;
                        }
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            );
        }
        div()
            .id("image-trace-context")
            .debug_selector(|| "image-trace-context".into())
            .h(px(44.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.omarchy().divider())
            .bg(cx.omarchy().surface)
            .child(compare)
            .child(
                button("trace-cancel", "Cancel", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "trace-cancel".into())
                    .on_click(cx.listener(|this, _, w, cx| this.cancel_image_trace(w, cx))),
            )
            .child(
                button("trace-keep", "Keep vectors", ButtonVariant::Primary, cx)
                    .debug_selector(|| "trace-keep".into())
                    .disabled(!self.image_trace_ready())
                    .on_click(cx.listener(|this, _, w, cx| this.keep_image_trace(w, cx))),
            )
            .into_any_element()
    }

    pub(super) fn image_trace_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d = self.image_trace.draft.as_ref().unwrap();
        let t = cx.omarchy().clone();
        let mut presets = div().flex().gap_1();
        for (id, label) in [
            ("logo", "Logo"),
            ("art", "Illustration"),
            ("photo", "Photo art"),
        ] {
            let selector = format!("trace-preset-{id}");
            presets = presets.child(
                button(
                    SharedString::from(selector.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .flex_1()
                .min_w_0()
                .px_1()
                .on_click(cx.listener(move |this, _, w, cx| this.trace_preset(id, w, cx))),
            );
        }
        let mut modes = div().flex().gap_1();
        for (id, label, mode) in [
            ("colour", "Colour", TraceMode::Color),
            ("gray", "Gray", TraceMode::Grayscale),
            ("mono", "B&W", TraceMode::Monochrome),
        ] {
            let selector = format!("trace-mode-{id}");
            modes = modes.child(
                button(
                    SharedString::from(selector.clone()),
                    label,
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .selected(d.mode == mode)
                .flex_1()
                .min_w_0()
                .px_1()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(d) = this.image_trace.draft.as_mut() {
                        d.mode = mode;
                    }
                    this.schedule_image_trace(cx);
                })),
            );
        }
        let mut fields = div().flex().flex_wrap().gap_2();
        for (i, (id, label)) in FIELDS.iter().enumerate() {
            let selector = format!("trace-field-{id}");
            fields = fields.child(
                div()
                    .w(px(124.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_size(px(11.))
                    .child(*label)
                    .child(
                        panel_input(
                            SharedString::from(selector.clone()),
                            &d.inputs[i],
                            window,
                            cx,
                        )
                        .debug_selector(move || selector.clone()),
                    ),
            );
        }
        let mut omit = div().flex().flex_col().gap_1();
        for (id, label, background) in [
            ("trace-omit-white", "Ignore white", false),
            ("trace-omit-background", "Ignore border colour", true),
        ] {
            let selected = if background {
                d.omit_background
            } else {
                d.omit_white
            };
            omit = omit.child(
                button(id, label, ButtonVariant::Outline, cx)
                    .debug_selector(move || id.into())
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.image_trace.draft.as_mut() {
                            if background {
                                d.omit_background = !d.omit_background
                            } else {
                                d.omit_white = !d.omit_white
                            }
                        }
                        this.schedule_image_trace(cx);
                    })),
            );
        }
        let mut body=div().id("trace-inspector-content").debug_selector(||"trace-inspector-content".into())
            .flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap_3().p_3()
            .child(panel_header("Image trace","Pixels into editable artwork","pen-tool",cx)).child(panel_note(d.source_name.clone(),cx))
            .child(presets).child(modes)
            .child(div().id("trace-status").debug_selector(||"trace-status".into()).text_size(px(11.)).child(d.message.clone()))
            .child(panel_section("Shape & detail",cx).child(fields))
            .child(omit)
            .child(panel_note("More detail adds points. Smoothing softens curves; corners protect sharp turns. Noise removes small regions. Threshold affects B&W.",cx));
        if let Some(p) = &d.prepared {
            body = body.child(panel_note(
                format!(
                    "{} colours · {}×{} processing · {} contour points → {} editable points",
                    p.stats.objects,
                    p.stats.working_width,
                    p.stats.working_height,
                    p.stats.points_before,
                    p.stats.anchors
                ),
                cx,
            ));
        }
        body=body.child(panel_note(if d.target.is_some(){"Retracing replaces this vector layer’s point edits. Your original image stays available."}else{"Keep vectors adds an editable layer and hides the original image. Undo restores it. Photo art is a stylised approximation."},cx));
        div()
            .id("image-trace-inspector")
            .debug_selector(|| "image-trace-inspector".into())
            .w(panel_width(window))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(t.divider())
            .bg(t.surface)
            .child(body)
            .into_any_element()
    }
}

fn drop_surface(surface: Option<DisplaySurface>, cx: &mut App) {
    if let Some(surface) = surface {
        for tile in surface.snapshot().iter() {
            cx.drop_image(tile.image.clone(), None);
        }
    }
}

#[cfg(all(test, feature = "ui-test"))]
#[path = "image_trace_ui_tests.rs"]
mod tests;
