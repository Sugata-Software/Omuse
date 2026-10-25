//! Experimental guided masks in the existing canvas/inspector. No new window.
use super::inspector_ui::{
    panel_button as button, panel_header, panel_note, panel_section, panel_width,
};
use super::studio_ui::InspectorTab;
use super::*;
use gpui_kit::WeakEntity;
use omuse::{
    guided_mask::{self, Cancellation, ManagedPack, Prompt, Worker},
    guided_mask_review::{Review, Tool as ReviewTool},
};

#[cfg(all(test, feature = "ui-test"))]
#[path = "guided_mask_ui_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct GuidedMaskUi {
    draft: Option<Draft>,
    miniatures: Vec<Arc<RenderImage>>,
    details: bool,
    release_registered: bool,
    worker: Option<Arc<Worker>>,
    active: Option<Arc<Cancellation>>,
    download: Option<Arc<std::sync::atomic::AtomicBool>>,
    opening: Option<Arc<std::sync::atomic::AtomicBool>>,
    preview_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    captured_preview_bytes: usize,
    has_pack: bool,
    has_runtime: bool,
    job: u64,
    preview_working: bool,
    preview_generation: u64,
    preview_seen: Option<(u64, u64)>,
    preview_presented: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Drop for GuidedMaskUi {
    fn drop(&mut self) {
        if let Some(token) = &self.active {
            token.cancel();
        }
        for token in [&self.opening, &self.preview_cancel].into_iter().flatten() {
            token.store(true, std::sync::atomic::Ordering::Release);
        }
        if let Some(token) = &self.download {
            token.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}
struct Draft {
    review: Review,
    source_name: String,
    gesture: Option<(f32, f32)>,
    last: Option<(f32, f32)>,
    original: bool,
    previous_tool: Tool,
    source_ready: bool,
    selection_revision: u64,
    prepared: Option<omuse::editor::PreparedSubjectMask>,
    prepared_pixels: Option<PreparedGuidedPixels>,
    original_display: DisplaySurface,
    original_proof: omuse::proofing::Settings,
    message: String,
}
struct PreparedGuidedPixels {
    pixels: image::RgbaImage,
    display: DisplaySurface,
    proof: omuse::proofing::Settings,
}
fn guided_surface_bytes(display: &DisplaySurface) -> u64 {
    display.snapshot().iter().fold(0_u64, |total, tile| {
        total.saturating_add(
            (u64::from(tile.rect.width) + 2)
                .saturating_mul(u64::from(tile.rect.height) + 2)
                .saturating_mul(4),
        )
    })
}
impl EditorView {
    // Explicit retained candidate/original surfaces plus the next worker's raw
    // and proof/display buffer reserve. No scan runs from render or pointer input.
    fn guided_preview_memory_bytes(&self) -> anyhow::Result<u64> {
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return Ok(0);
        };
        let mut bytes = guided_surface_bytes(&d.original_display);
        if let Some(p) = &d.prepared_pixels {
            bytes = bytes
                .checked_add(p.pixels.as_raw().len() as u64)
                .and_then(|n| n.checked_add(guided_surface_bytes(&p.display)))
                .ok_or_else(|| anyhow::anyhow!("Preview memory estimate overflow"))?;
        }
        let w = u64::from(self.editor.document.width);
        let h = u64::from(self.editor.document.height);
        let edge = u64::from(crate::display_surface::TILE_EDGE);
        let next_surface = w
            .checked_add(2 * w.div_ceil(edge))
            .and_then(|tw| {
                h.checked_add(2 * h.div_ceil(edge))
                    .and_then(|th| tw.checked_mul(th))
            })
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| anyhow::anyhow!("Preview surface estimate overflow"))?;
        // Raw canvas + temporary proof output + halo-bearing BGRA tiles.
        bytes
            .checked_add(
                w.checked_mul(h)
                    .and_then(|n| n.checked_mul(8))
                    .ok_or_else(|| anyhow::anyhow!("Preview memory estimate overflow"))?,
            )
            .and_then(|n| n.checked_add(next_surface))
            .ok_or_else(|| anyhow::anyhow!("Preview memory estimate overflow"))
    }
    fn release_guided_miniatures(&mut self, mut window: Option<&mut Window>, cx: &mut App) {
        for image in self.guided_mask.miniatures.drain(..) {
            cx.drop_image(image.clone(), window.as_deref_mut());
            // The active window is absent from App.windows inside update_in.
            // Retained external Arcs must not keep atlas entries alive.
            cx.defer(move |cx| cx.drop_image(image, None));
        }
    }
    fn replace_guided_miniatures(&mut self, pixels: Vec<image::RgbaImage>, cx: &mut App) {
        self.release_guided_miniatures(None, cx);
        self.guided_mask.miniatures = pixels.iter().map(render_image).collect();
    }
    pub(super) fn guided_mask_active(&self) -> bool {
        self.guided_mask.draft.is_some()
    }
    pub(super) fn guided_mask_has_work(&self) -> bool {
        self.guided_mask.active.is_some()
            || self.guided_mask.download.is_some()
            || self.guided_mask.opening.is_some()
            || self.guided_mask.preview_working
    }
    pub(super) fn guided_mask_command_allowed(name: &str) -> bool {
        matches!(
            name,
            "select-guided-mask"
                | "zoom-in"
                | "zoom-in-plus"
                | "zoom-out"
                | "fit"
                | "actual"
                | "toggle-panels"
                | "command-search"
                | "shortcuts"
                | "tool-hand"
                | "brush-smaller"
                | "brush-larger"
                | "quit"
        )
    }
    pub(super) fn guard_guided_mask(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.guided_mask_active() && !self.guided_mask_has_work() {
            return false;
        }
        self.status = if self.guided_mask_active() {
            "Keep or cancel the guided mask before editing the document"
        } else {
            "Finishing local work; wait a moment before starting another edit"
        }
        .into();
        cx.notify();
        true
    }
    fn guided_mask_context_valid(&self) -> bool {
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return false;
        };
        let key = &d.review.source;
        key.editor == self.editor.instance_id()
            && key.page == self.create.epoch
            && key.revision == self.editor.revision()
            && key.layer == self.editor.active_layer
            && d.selection_revision == self.editor.selection_revision()
            && self
                .editor
                .document
                .find_layer(&key.layer)
                .and_then(|l| l.image.as_ref())
                .is_some_and(|image| Arc::ptr_eq(&image.as_arc(), &d.review.pixels))
    }
    pub(super) fn validate_guided_mask_context(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.guided_mask_active() && !self.guided_mask_context_valid() {
            self.cancel_guided_mask(window, cx);
            self.status = "Guided-mask source changed; the draft was discarded".into();
        }
    }
    pub(super) fn open_guided_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guided_mask.release_registered {
            self.guided_mask.release_registered = true;
            cx.on_release(|this, cx| this.release_guided_miniatures(None, cx))
                .detach();
        }

        if self.guided_mask_active() {
            self.inspector_visible = true;
            cx.notify();
            return;
        }
        if self.guided_mask_has_work() {
            self.status = "The local model is still stopping; try again shortly".into();
            cx.notify();
            return;
        }
        let prepared = (|| -> anyhow::Result<_> {
            anyhow::ensure!(
                self.dialog == Dialog::None
                    && !self.busy
                    && !self.vector_scene_active()
                    && !self.image_trace_active(),
                "Finish the current edit before guided selection"
            );
            anyhow::ensure!(
                self.crop.is_none() && self.inline_text.is_none(),
                "Finish the current crop or text edit before guided selection"
            );
            anyhow::ensure!(
                u64::from(self.editor.document.width) * u64::from(self.editor.document.height)
                    <= guided_mask::MAX_SOURCE_PIXELS,
                "Guided-mask review supports canvases up to 16 megapixels"
            );
            anyhow::ensure!(
                self.editor.floating_selection_layer().is_none(),
                "Finish or cancel the floating selection first"
            );
            self.finish_interaction(cx);
            self.editor.mask_review_ready()?;
            anyhow::ensure!(
                self.numeric_preview_document().is_none(),
                "Finish the current property adjustment before guided selection"
            );
            let id = self.editor.active_layer.clone();
            anyhow::ensure!(
                editable_source(&self.editor.document.layers, &id, false),
                "Select a visible image layer and unlock it and its parent groups"
            );
            let layer = self
                .editor
                .document
                .find_layer(&id)
                .ok_or_else(|| anyhow::anyhow!("Select an image layer"))?;
            anyhow::ensure!(
                !layer.is_group()
                    && layer.vector_scene.is_none()
                    && objects::live_text(layer)?.is_none()
                    && objects::live_shape(layer)?.is_none(),
                "Choose a pixel image layer; keep live objects and retained adjustments editable"
            );
            let image = layer
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Choose an image layer"))?
                .as_arc();
            guided_mask::admit_working_set(
                usize::try_from(
                    self.editor
                        .resident_memory_estimate()?
                        .checked_add(self.create_context_resident_bytes()?)
                        .ok_or_else(|| anyhow::anyhow!("Project memory estimate overflow"))?,
                )?,
                image.width(),
                image.height(),
                self.editor.document.width,
                self.editor.document.height,
                count_layers(&self.editor.document.layers),
                count_image_pixels(&self.editor.document.layers),
                self.guided_mask.captured_preview_bytes,
            )?;
            let key = guided_mask::SourceKey {
                editor: self.editor.instance_id(),
                page: self.create.epoch,
                revision: self.editor.revision(),
                layer: id,
                width: image.width(),
                height: image.height(),
                pixels_sha256: String::new(),
            };
            let review = Review::new(key, image)?;
            let pack = ManagedPack::installed()?;
            let message = if pack.available() {
                "Experimental local model · Click the subject or draw a box, then Update mask. No image leaves this machine.".into()
            } else {
                "Download the optional local model pack to begin (~40 MB). Your photo stays on this machine.".into()
            };
            Ok((review, layer.name.clone(), pack, message))
        })();
        match prepared {
            Ok((review, source_name, pack, message)) => {
                self.guided_mask.has_pack = pack.weights_present();
                self.guided_mask.has_runtime = pack.runtime_available();
                if self.guided_mask.worker.is_none() {
                    self.guided_mask.worker = Some(Arc::new(Worker::new(pack)));
                }
                self.guided_mask.draft = Some(Draft {
                    review,
                    source_name,
                    gesture: None,
                    last: None,
                    original: false,
                    previous_tool: self.tool,
                    source_ready: false,
                    selection_revision: self.editor.selection_revision(),
                    prepared: None,
                    prepared_pixels: None,
                    original_display: self.display.clone(),
                    original_proof: self.proof_settings.clone(),
                    message,
                });
                self.tool = Tool::Move;
                self.mask_inspection.exit();
                self.mask_preview.borrow_mut().take();
                self.inspector_visible = true;
                self.inspector_tab = InspectorTab::Layers;
                self.focus.focus(window, cx);
                self.status = "Guided mask · Source pixels remain intact · Escape cancels".into();
                self.paint_guided_mask(cx);
                self.prepare_guided_source(cx);
            }
            Err(e) => {
                self.status = format!("Guided mask: {e:#}");
                cx.notify();
            }
        }
    }
    fn guided_local_point(&self, position: gpui_kit::Point<Pixels>) -> Option<(f32, f32)> {
        let d = self.guided_mask.draft.as_ref()?;
        let (x, y) = self.coordinates(position);
        let (x, y) = self
            .editor
            .image_local_point(&d.review.source.layer, x, y)?;
        (x.is_finite()
            && y.is_finite()
            && x >= 0.
            && y >= 0.
            && x < d.review.source.width as f32
            && y < d.review.source.height as f32)
            .then_some((x, y))
    }
    pub(super) fn guided_mask_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.guided_mask_active() {
            return false;
        }
        if event.button != MouseButton::Left {
            return true;
        }
        if self.guided_mask.active.is_some()
            || self.guided_mask.preview_working
            || self.guided_mask.opening.is_some()
        {
            return true;
        }
        let Some((x, y)) = self.guided_local_point(event.position) else {
            self.status = "Click inside the selected image".into();
            cx.notify();
            return true;
        };
        let d = self.guided_mask.draft.as_mut().unwrap();
        d.original = false;
        let result = match d.review.tool {
            ReviewTool::Positive => d.review.add_prompt(Prompt::Positive { x, y }),
            ReviewTool::Box => {
                d.gesture = Some((x, y));
                d.last = Some((x, y));
                Ok(())
            }
            ReviewTool::AddCoverage | ReviewTool::SubtractCoverage => {
                d.gesture = Some((x, y));
                d.last = Some((x, y));
                d.review
                    .dab(x, y, d.review.tool == ReviewTool::SubtractCoverage)
            }
        };
        if let Err(e) = result {
            d.message = e.to_string();
        } else {
            d.message="Draft updated · Update mask after changing model prompts; manual brush edits need no inference".into();
        }
        self.invalidate_guided_mask_preview();
        self.paint_guided_mask(cx);
        true
    }
    pub(super) fn guided_mask_moved(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.guided_mask_active() {
            return false;
        }
        if self.guided_mask.active.is_some()
            || self.guided_mask.preview_working
            || self.guided_mask.opening.is_some()
        {
            return true;
        }
        if event.pressed_button != Some(MouseButton::Left)
            && self
                .guided_mask
                .draft
                .as_ref()
                .is_some_and(|d| d.gesture.is_some())
        {
            self.finish_guided_mask_gesture(self.guided_local_point(event.position), cx);
            return true;
        }
        let Some((x, y)) = self.guided_local_point(event.position) else {
            return true;
        };
        let d = self.guided_mask.draft.as_mut().unwrap();
        if d.gesture.is_none() {
            if matches!(
                d.review.tool,
                ReviewTool::AddCoverage | ReviewTool::SubtractCoverage
            ) {
                d.last = Some((x, y));
                cx.notify();
            }
            return true;
        }
        if matches!(
            d.review.tool,
            ReviewTool::AddCoverage | ReviewTool::SubtractCoverage
        ) {
            let previous = d.last.unwrap_or((x, y));
            let distance = ((x - previous.0).powi(2) + (y - previous.1).powi(2)).sqrt();
            let steps = (distance / (d.review.brush_radius * 0.4).max(1.))
                .ceil()
                .clamp(1., 64.) as usize;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                if let Err(e) = d.review.dab(
                    previous.0 + (x - previous.0) * t,
                    previous.1 + (y - previous.1) * t,
                    d.review.tool == ReviewTool::SubtractCoverage,
                ) {
                    d.message = e.to_string();
                    break;
                }
            }
        }
        d.last = Some((x, y));
        self.invalidate_guided_mask_preview();
        // Repaint only native handles while dragging. Full document compositing
        // is coalesced on the background executor after gesture release.
        cx.notify();
        true
    }
    pub(super) fn guided_mask_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) -> bool {
        if !self.guided_mask_active() {
            return false;
        }
        // The canvas receives mouse-up-out for clicks on inspector buttons.
        // Only a real left-button canvas gesture owns a pending release; a
        // toolbar/footer release must not re-render or invalidate its review.
        if event.button != MouseButton::Left
            || self.guided_mask.active.is_some()
            || self
                .guided_mask
                .draft
                .as_ref()
                .is_none_or(|d| d.gesture.is_none())
        {
            return true;
        }
        self.finish_guided_mask_gesture(self.guided_local_point(event.position), cx);
        true
    }
    fn finish_guided_mask_gesture(&mut self, position: Option<(f32, f32)>, cx: &mut Context<Self>) {
        let d = self.guided_mask.draft.as_mut().unwrap();
        let Some(start) = d.gesture.take() else {
            return;
        };
        {
            if d.review.tool == ReviewTool::Box {
                if let Some(end) = position.or(d.last) {
                    let p = Prompt::Box {
                        left: start.0.min(end.0),
                        top: start.1.min(end.1),
                        right: start.0.max(end.0),
                        bottom: start.1.max(end.1),
                    };
                    d.message = match d.review.add_prompt(p) {
                        Ok(()) => "Box captured · Update mask to run the local model".into(),
                        Err(e) => e.to_string(),
                    };
                }
            }
        }
        d.last = None;
        self.paint_guided_mask(cx);
    }
    /// Walk retained owners only when admitting a new heavy job, never while painting.
    fn admit_guided_job(&mut self, cx: &mut Context<Self>) -> bool {
        let result = (|| -> anyhow::Result<()> {
            let d = self
                .guided_mask
                .draft
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Open guided selection first"))?;
            let retained = self
                .editor
                .resident_memory_estimate()?
                .checked_add(self.create_context_resident_bytes()?)
                .and_then(|n| n.checked_add(self.guided_preview_memory_bytes().unwrap_or(u64::MAX)))
                .ok_or_else(|| anyhow::anyhow!("Project memory estimate overflow"))?;
            guided_mask::admit_working_set(
                usize::try_from(retained)?,
                d.review.source.width,
                d.review.source.height,
                self.editor.document.width,
                self.editor.document.height,
                count_layers(&self.editor.document.layers),
                count_image_pixels(&self.editor.document.layers),
                self.guided_mask.captured_preview_bytes,
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            self.status = format!("Local selection: {e:#}");
            cx.notify();
            return false;
        }
        true
    }
    fn update_guided_mask(&mut self, cx: &mut Context<Self>) {
        if !self.admit_guided_job(cx) {
            return;
        }
        if self.guided_mask_has_work() || !self.guided_mask_context_valid() {
            return;
        }
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return;
        };
        if !d.source_ready {
            self.status = "Preparing the image for local selection…".into();
            cx.notify();
            return;
        }
        if d.review.manually_refined {
            self.status = "Reset manual refinements before running the model again".into();
            cx.notify();
            return;
        }
        if let Err(e) = guided_mask::validate_prompts(
            &d.review.prompts,
            d.review.source.width,
            d.review.source.height,
        ) {
            self.status = e.to_string();
            cx.notify();
            return;
        }
        let source = d.review.source.clone();
        let image = d.review.pixels.clone();
        let prompts = d.review.prompts.clone();
        let generation = d.review.generation;
        let Some(worker) = self.guided_mask.worker.clone() else {
            return;
        };
        let token = Cancellation::new();
        self.guided_mask.active = Some(token.clone());
        self.guided_mask.job = self.guided_mask.job.wrapping_add(1);
        let job = self.guided_mask.job;
        self.status = "Running the experimental local model · Escape cancels".into();
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let prediction = worker.predict(&source, &image, &prompts, &token)?;
            let miniatures =
                candidate_miniatures(&image, &prediction.alternatives, || token.is_cancelled())?;
            Ok::<_, anyhow::Error>((prediction, miniatures))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{
            if this.guided_mask.job!=job{return;}
            let cancelled=this.guided_mask.active.take().is_none_or(|t|t.is_cancelled());
            if cancelled { if let Some(worker)=&this.guided_mask.worker{worker.clear_idle_cache();} if let Some(d)=this.guided_mask.draft.as_mut(){d.message="Local inference stopped; document unchanged".into();this.status=d.message.clone();} cx.notify();return; }
            if !this.guided_mask_context_valid(){cx.notify();return;}
            let Some(d)=this.guided_mask.draft.as_mut()else{return};
            let mut miniature_pixels = None;
            match result {
                Ok((result,pixels))=>{match d.review.accept(generation,result){Ok(())=>{d.message="Mask ready · Compare options, then Keep or refine with a brush".into(); miniature_pixels=Some(pixels);},Err(e)=>d.message=e.to_string()}},
                Err(e)=>d.message=format!("Local model: {e:#}"),
            }
            if let Some(pixels)=miniature_pixels { this.replace_guided_miniatures(pixels,cx); }
            if let Some(d)=this.guided_mask.draft.as_ref(){this.status=d.message.clone();}
            this.paint_guided_mask(cx);
        });}).detach();
    }
    fn stop_guided_mask_job(&mut self, cx: &mut Context<Self>) {
        if let Some(token) = &self.guided_mask.download {
            token.store(true, std::sync::atomic::Ordering::Release);
            self.status = "Stopping model download; no partial pack will be installed".into();
            cx.notify();
        }
        if let Some(token) = &self.guided_mask.active {
            token.cancel();
            self.status = "Stopping local inference; the document is unchanged".into();
            cx.notify();
        }
    }
    fn prepare_guided_source(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return;
        };
        let image = d.review.pixels.clone();
        let key = d.review.source.clone();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.opening = Some(cancel.clone());
        let task = cx.background_executor().spawn(async move {
            anyhow::ensure!(
                !cancel.load(std::sync::atomic::Ordering::Acquire),
                "Image preparation cancelled"
            );
            let result = guided_mask::SourceKey::capture(
                key.editor,
                key.page,
                key.revision,
                key.layer,
                &image,
            )?;
            anyhow::ensure!(
                !cancel.load(std::sync::atomic::Ordering::Acquire),
                "Image preparation cancelled"
            );
            Ok::<_, anyhow::Error>(result)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{
            let cancelled=this.guided_mask.opening.take().is_none_or(|c|c.load(std::sync::atomic::Ordering::Acquire));
            if cancelled||!this.guided_mask_context_valid(){cx.notify();return;}
            let d=this.guided_mask.draft.as_mut().unwrap();match result{Ok(key)=>{d.review.source=key;d.source_ready=true;d.message="Click the subject or draw one box, then Update mask. Original pixels remain intact.".into();},Err(e)=>d.message=e.to_string()};this.paint_guided_mask(cx);
        });}).detach();
        cx.notify();
    }
    fn prepare_guided_alternative(
        &mut self,
        index: usize,
        reset_manual: bool,
        cx: &mut Context<Self>,
    ) {
        if self.guided_mask_has_work() || !self.guided_mask_context_valid() {
            return;
        }
        if !self.admit_guided_job(cx) {
            return;
        }
        let d = self.guided_mask.draft.as_ref().unwrap();
        let Some(alt) = d.review.alternatives.get(index) else {
            return;
        };
        let mask = alt.mask.clone();
        let (w, h) = (d.review.source.width, d.review.source.height);
        let visual = (d.review.generation, d.review.visual_revision);
        self.invalidate_guided_mask_preview();
        let generation = self.guided_mask.preview_generation;
        self.guided_mask.preview_working = true;
        self.guided_mask.captured_preview_bytes = self.editor.mask_review_memory_bytes();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.preview_cancel = Some(cancel.clone());
        let task = cx
            .background_executor()
            .spawn(async move { guided_mask::prepare_source_mask(&mask, w, h, &cancel) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ =
                view.update(cx, |this, cx| {
                    this.guided_mask.preview_working = false;
                    this.guided_mask.captured_preview_bytes = 0;
                    let cancelled = this
                        .guided_mask
                        .preview_cancel
                        .take()
                        .is_none_or(|c| c.load(std::sync::atomic::Ordering::Acquire));
                    if cancelled
                        || !this.guided_mask_context_valid()
                        || this.guided_mask.preview_generation != generation
                        || this.guided_mask.draft.as_ref().is_none_or(|d| {
                            (d.review.generation, d.review.visual_revision) != visual
                        })
                    {
                        cx.notify();
                        return;
                    }
                    let d = this.guided_mask.draft.as_mut().unwrap();
                    match result.and_then(|mask| {
                        if reset_manual {
                            d.review.reset_manual(mask)
                        } else {
                            d.review.select_prepared_alternative(index, mask)
                        }
                    }) {
                        Ok(()) => {}
                        Err(e) => d.message = e.to_string(),
                    };
                    this.paint_guided_mask(cx);
                });
        })
        .detach();
        cx.notify();
    }
    fn verify_repair_guided_pack(&mut self, cx: &mut Context<Self>) {
        if self.guided_mask_has_work() {
            return;
        }
        let pack = match ManagedPack::installed() {
            Ok(p) => p,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.download = Some(cancel.clone());
        if let Some(d) = self.guided_mask.draft.as_mut() {
            d.message="Verifying the local model pack; repair may download ~40 MB and preserves the previous pack.".into();
        }
        let task = cx
            .background_executor()
            .spawn(async move { omuse::guided_mask_pack::repair_verified(&pack, &cancel) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.guided_mask.download = None;
                if let Some(d) = this.guided_mask.draft.as_mut() {
                    d.message = match result {
                        Ok(report) => {
                            this.guided_mask.has_pack = true;
                            if let Some(warning) = report.outcome.warning() {
                                warning.to_owned()
                            } else {
                                format!(
                                    "Verified model pack ready{}",
                                    report.retained_previous.map_or(String::new(), |p| format!(
                                        " · Previous pack preserved at {}",
                                        p.display()
                                    ))
                                )
                            }
                        }
                        Err(e) => format!("Model pack: {e:#}"),
                    };
                }
                this.paint_guided_mask(cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn download_guided_mask_pack(&mut self, cx: &mut Context<Self>) {
        if self.guided_mask_has_work() {
            return;
        }
        let pack = match ManagedPack::installed() {
            Ok(p) => p,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.download = Some(token.clone());
        if let Some(d) = self.guided_mask.draft.as_mut() {
            d.message="Downloading the pinned local model pack (~40 MB) · Stop cancels · SHA-256 verification precedes publication".into();
        }
        let task = cx
            .background_executor()
            .spawn(async move { omuse::guided_mask_pack::download_verified(&pack, &token) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{
            this.guided_mask.download=None;
            if let Some(d)=this.guided_mask.draft.as_mut(){
                d.message=match result{Ok(outcome)=>{this.guided_mask.has_pack=true;if let Some(warning)=outcome.warning(){warning.to_string()}else if this.guided_mask.has_runtime{"Verified optional pack installed · Choose positive points or one box, then Update mask".into()}else{"Verified pack installed; the bundled ONNX Runtime library is still unavailable. Repair the application runtime separately before inference.".into()}},Err(e)=>format!("Model pack: {e:#}")};
            }
            this.paint_guided_mask(cx);
        });}).detach();
        cx.notify();
    }
    pub(super) fn cancel_guided_mask(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.guided_mask_active() {
            return false;
        }
        let restore_captured = self.guided_mask_context_valid()
            && self
                .guided_mask
                .draft
                .as_ref()
                .is_some_and(|d| d.original_proof == self.proof_settings);
        if let Some(token) = &self.guided_mask.active {
            token.cancel();
        }
        if let Some(token) = &self.guided_mask.download {
            token.store(true, std::sync::atomic::Ordering::Release);
        }
        for token in [&self.guided_mask.opening, &self.guided_mask.preview_cancel]
            .into_iter()
            .flatten()
        {
            token.store(true, std::sync::atomic::Ordering::Release);
        }
        self.guided_mask.preview_generation = self.guided_mask.preview_generation.wrapping_add(1);
        self.guided_mask.preview_seen = None;
        self.release_guided_miniatures(Some(window), cx);
        if let Some(draft) = self.guided_mask.draft.take() {
            self.tool = draft.previous_tool;
            if restore_captured {
                self.display = draft.original_display;
            }
        }
        if let Some(worker) = &self.guided_mask.worker {
            worker.clear_idle_cache();
        }
        self.focus.focus(window, cx);
        self.status = "Guided mask cancelled · Document unchanged".into();
        if !restore_captured {
            self.refresh(cx);
        }
        cx.notify();
        true
    }
    fn keep_guided_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guided_mask_preview_current() {
            return;
        }
        let d = self.guided_mask.draft.as_mut().unwrap();
        let previous_tool = d.previous_tool;
        let Some(prepared) = d.prepared.take() else {
            return;
        };
        let Some(pixels) = d.prepared_pixels.take() else {
            return;
        };
        match self.editor.keep_prepared_subject_mask(prepared) {
            Ok(true) => {
                self.guided_mask.draft = None;
                self.release_guided_miniatures(Some(window), cx);
                self.tool = previous_tool;
                if let Some(w) = &self.guided_mask.worker {
                    w.clear_idle_cache();
                }
                self.focus.focus(window, cx);
                self.status =
                    "Guided mask kept · Source pixels intact · Ctrl+Z restores the previous mask"
                        .into();
                self.changed_with_prepared_pixels(pixels.pixels, pixels.display, cx);
            }
            Ok(false) => {
                self.status = "The mask was not changed".into();
                cx.notify();
            }
            Err(e) => {
                self.status = format!("Keep guided mask: {e:#}");
                cx.notify();
            }
        }
    }
    fn use_guided_mask_as_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guided_mask_preview_current() {
            return;
        }
        if !self.admit_guided_job(cx) {
            return;
        }
        let d = self.guided_mask.draft.as_ref().unwrap();
        let id = d.review.source.layer.clone();
        let visual = (d.review.generation, d.review.visual_revision);
        let previous_tool = d.previous_tool;
        let request = match self
            .editor
            .prepare_subject_selection(&id, d.review.mask_arc().unwrap())
        {
            Ok(r) => r,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        self.guided_mask.preview_working = true;
        self.guided_mask.captured_preview_bytes = self.editor.mask_review_memory_bytes();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.preview_cancel = Some(cancel.clone());
        let task = cx
            .background_executor()
            .spawn(async move { request.prepare_selection(&cancel) });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{
            this.guided_mask.preview_working=false;this.guided_mask.captured_preview_bytes=0;
            let cancelled=this.guided_mask.preview_cancel.take().is_none_or(|c|c.load(std::sync::atomic::Ordering::Acquire));
            if cancelled||!this.guided_mask_context_valid()||this.guided_mask.draft.as_ref().is_none_or(|d|(d.review.generation,d.review.visual_revision)!=visual){cx.notify();return;}
            match result.and_then(|p|this.editor.use_prepared_subject_selection(p)){
                Ok(true)=>{let draft=this.guided_mask.draft.take().unwrap();let original_display=draft.original_display;let original_proof=draft.original_proof;this.release_guided_miniatures(Some(window),cx);this.tool=previous_tool;if let Some(w)=&this.guided_mask.worker{w.clear_idle_cache();}this.selection_box=this.editor.selection.as_ref().and_then(|s|s.bounds()).map(|(x,y,w,h)|(x as f32,y as f32,w as f32,h as f32));if original_proof==this.proof_settings{this.display=original_display;}else{this.present_pixels(cx);}this.focus.focus(window,cx);this.status="Guided selection ready for Controlled removal · Artwork unchanged · Ctrl+Z restores the previous selection · Ctrl+D clears".into();},
                Ok(false)=>this.status="The selection was unchanged".into(),Err(e)=>this.status=format!("Guided selection: {e:#}"),
            }
            cx.notify();
        });}).detach();
        cx.notify();
    }
    fn guided_mask_preview_current(&self) -> bool {
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return false;
        };
        !self.guided_mask_has_work()
            && d.gesture.is_none()
            && d.prepared.is_some()
            && d.prepared_pixels.as_ref().is_some_and(|p| {
                p.proof == self.proof_settings
                    && p.pixels.dimensions()
                        == (self.editor.document.width, self.editor.document.height)
                    && p.display.dimensions() == p.pixels.dimensions()
            })
            && self
                .guided_mask
                .preview_presented
                .as_ref()
                .is_some_and(|p| p.load(std::sync::atomic::Ordering::Acquire))
            && self.guided_mask.preview_seen
                == Some((d.review.generation, d.review.visual_revision))
            && self.guided_mask_context_valid()
    }
    fn invalidate_guided_mask_preview(&mut self) {
        self.guided_mask.preview_generation = self.guided_mask.preview_generation.wrapping_add(1);
        self.guided_mask.preview_seen = None;
        self.guided_mask.preview_presented = None;
        if let Some(d) = self.guided_mask.draft.as_mut() {
            d.prepared = None;
            d.prepared_pixels = None;
        }
    }
    fn change_guided_mask_tool(&mut self, tool: ReviewTool, cx: &mut Context<Self>) {
        if self
            .guided_mask
            .draft
            .as_ref()
            .is_some_and(|d| d.gesture.is_some())
        {
            self.finish_guided_mask_gesture(None, cx);
        }
        self.tool = Tool::Move;
        if let Some(d) = self.guided_mask.draft.as_mut() {
            d.review.tool = tool;
            d.last = None;
        }
        cx.notify();
    }
    fn set_guided_brush_diameter(&mut self, diameter: f32, cx: &mut Context<Self>) {
        if !diameter.is_finite() {
            return;
        }
        if let Some(d) = self.guided_mask.draft.as_mut() {
            if d.gesture.is_some() {
                self.status = "Release the brush before changing its size".into();
                cx.notify();
                return;
            }
            d.review.brush_radius = (diameter.clamp(1., 256.).round()) / 2.;
            self.status = format!(
                "Guided brush diameter · {:.0} px",
                d.review.brush_radius * 2.
            );
        }
        cx.notify();
    }
    pub(super) fn guided_mask_brush_command(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        if !self.guided_mask_active() || !matches!(name, "brush-smaller" | "brush-larger") {
            return false;
        }
        let diameter = self.guided_mask.draft.as_ref().unwrap().review.brush_radius * 2.;
        self.set_guided_brush_diameter(
            diameter + if name == "brush-smaller" { -1. } else { 1. },
            cx,
        );
        true
    }
    fn paint_guided_mask(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.guided_mask.draft.as_ref() else {
            return;
        };
        self.guided_mask.preview_generation = self.guided_mask.preview_generation.wrapping_add(1);
        if d.gesture.is_some() {
            cx.notify();
            return;
        }
        if d.original || d.review.mask().is_err() {
            self.display = d.original_display.clone();
            cx.notify();
            return;
        }
        if self.guided_mask_has_work() {
            cx.notify();
            return;
        }
        if !self.admit_guided_job(cx) {
            return;
        }
        let d = self.guided_mask.draft.as_ref().unwrap();
        let key = d.review.source.clone();
        let mask = d.review.mask_arc().unwrap();
        let request = match self.editor.subject_mask_request(&key.layer, mask) {
            Ok(r) => r,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let generation = self.guided_mask.preview_generation;
        let visual = (d.review.generation, d.review.visual_revision);
        self.guided_mask.preview_working = true;
        self.status = "Preparing mask preview…".into();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.guided_mask.preview_cancel = Some(cancel.clone());
        self.guided_mask.captured_preview_bytes = self.editor.mask_review_memory_bytes();
        let proof = self.proof_settings.clone();
        let task = cx.background_executor().spawn(async move {
            let (prepared, document) = request.prepare(&cancel)?;
            anyhow::ensure!(
                !cancel.load(std::sync::atomic::Ordering::Acquire),
                "Preview cancelled"
            );
            let pixels = raster::composite(&document);
            anyhow::ensure!(
                pixels.dimensions() == (document.width, document.height),
                "Preview could not render the complete canvas"
            );
            anyhow::ensure!(
                !cancel.load(std::sync::atomic::Ordering::Acquire),
                "Preview cancelled"
            );
            let proofed = if proof.enabled {
                Some(omuse::proofing::render(&pixels, &proof)?)
            } else {
                None
            };
            let display = DisplaySurface::new(proofed.as_ref().unwrap_or(&pixels));
            anyhow::ensure!(
                !cancel.load(std::sync::atomic::Ordering::Acquire),
                "Preview cancelled"
            );
            Ok::<_, anyhow::Error>((
                PreparedGuidedPixels {
                    pixels,
                    display,
                    proof,
                },
                prepared,
            ))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ =
                view.update(cx, |this, cx| {
                    this.guided_mask.preview_working = false;
                    this.guided_mask.captured_preview_bytes = 0;
                    let cancelled = this
                        .guided_mask
                        .preview_cancel
                        .take()
                        .is_none_or(|c| c.load(std::sync::atomic::Ordering::Acquire));
                    if cancelled || !this.guided_mask_context_valid() {
                        cx.notify();
                        return;
                    }
                    if this.guided_mask.preview_generation != generation
                        || this.guided_mask.draft.as_ref().is_none_or(|d| {
                            (d.review.generation, d.review.visual_revision) != visual
                        })
                    {
                        this.paint_guided_mask(cx);
                        return;
                    }
                    match result {
                        Ok((pixels, prepared)) => {
                            if pixels.proof != this.proof_settings {
                                this.paint_guided_mask(cx);
                                return;
                            }
                            this.display = pixels.display.clone();
                            this.guided_mask.draft.as_mut().unwrap().prepared = Some(prepared);
                            this.guided_mask.draft.as_mut().unwrap().prepared_pixels = Some(pixels);
                            this.status =
                                "Mask ready · Review the result, then Keep or refine with a brush"
                                    .into();
                            this.guided_mask.preview_seen = Some(visual);
                            this.guided_mask.preview_presented =
                                Some(Arc::new(std::sync::atomic::AtomicBool::new(false)));
                        }
                        Err(e) => this.status = format!("Guided-mask preview: {e:#}"),
                    }
                    cx.notify();
                });
        })
        .detach();
        cx.notify();
    }

    /// Capture only bounded overlay geometry; no image or retention scans.
    pub(super) fn guided_mask_overlay(&self, cx: &Context<Self>) -> Option<GuidedMaskOverlay> {
        let d = self.guided_mask.draft.as_ref()?;
        if d.original {
            return None;
        }
        let world = |x, y| self.editor.image_world_point(&d.review.source.layer, x, y);
        let mut lines = Vec::new();
        let mut points = Vec::new();
        let mut add_box = |a: (f32, f32), b: (f32, f32)| {
            let q = [
                world(a.0, a.1),
                world(b.0, a.1),
                world(b.0, b.1),
                world(a.0, b.1),
                world(a.0, a.1),
            ];
            if q.iter().all(Option::is_some) {
                lines.push(q.into_iter().flatten().collect::<Vec<_>>());
            }
        };
        for p in &d.review.prompts {
            match *p {
                Prompt::Box {
                    left,
                    top,
                    right,
                    bottom,
                } => add_box((left, top), (right, bottom)),
                Prompt::Positive { x, y } => {
                    if let Some(p) = world(x, y) {
                        points.push(p);
                    }
                }
            }
        }
        if d.review.tool == ReviewTool::Box {
            if let (Some(a), Some(b)) = (d.gesture, d.last) {
                add_box(a, b);
            }
        }
        drop(add_box);
        if matches!(
            d.review.tool,
            ReviewTool::AddCoverage | ReviewTool::SubtractCoverage
        ) {
            if let Some((x, y)) = d.last {
                let mut circle = Vec::with_capacity(33);
                for i in 0..=32 {
                    let a = i as f32 * std::f32::consts::TAU / 32.;
                    if let Some(p) = world(
                        x + a.cos() * d.review.brush_radius,
                        y + a.sin() * d.review.brush_radius,
                    ) {
                        circle.push(p);
                    }
                }
                lines.push(circle);
            }
        }
        Some(GuidedMaskOverlay {
            lines,
            points,
            width: self.editor.document.width,
            height: self.editor.document.height,
            presented: self.guided_mask.preview_presented.clone(),
            view: cx.entity().downgrade(),
        })
    }
    pub(super) fn guided_mask_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d = self.guided_mask.draft.as_ref().unwrap();
        let working = self.guided_mask.active.is_some();
        let downloading = self.guided_mask.download.is_some();
        let t = cx.omarchy().clone();
        let mut tools = div().flex().flex_wrap().gap_1();
        for (id, label, tool) in [
            ("positive", "Mark subject", ReviewTool::Positive),
            ("box", "Draw box", ReviewTool::Box),
            ("add", "Brush add", ReviewTool::AddCoverage),
            ("subtract", "Brush remove", ReviewTool::SubtractCoverage),
        ] {
            let selector = format!("guided-{id}");
            let debug_selector = selector.clone();
            tools = tools.child(
                button(
                    SharedString::from(selector),
                    label,
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(move || debug_selector.clone())
                .selected(d.review.tool == tool)
                .disabled(
                    working
                        || (matches!(tool, ReviewTool::AddCoverage | ReviewTool::SubtractCoverage)
                            && d.review.mask().is_err()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.change_guided_mask_tool(tool, cx);
                })),
            );
        }
        let mut alternatives = div().flex().flex_wrap().gap_1();
        for index in 0..d.review.alternatives.len().min(3) {
            let mut card = div()
                .id(SharedString::from(format!("guided-option-card-{index}")))
                .w(px(96.))
                .min_w_0()
                .p_1()
                .rounded(control_radius())
                .border_1()
                .border_color(if d.review.selected_alternative == index {
                    t.bright
                } else {
                    t.border
                })
                .flex()
                .flex_col()
                .gap_1();
            if let Some(image) = self.guided_mask.miniatures.get(index).cloned() {
                card = card.child(
                    div().h(px(60.)).w_full().overflow_hidden().child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                let _ = window.paint_image(
                                    bounds,
                                    bounds,
                                    Corners::default(),
                                    image.clone(),
                                    0,
                                    false,
                                );
                            },
                        )
                        .size_full(),
                    ),
                );
            }
            alternatives = alternatives.child(
                card.child(
                    button(
                        SharedString::from(format!("guided-alternative-{index}")),
                        SharedString::from(format!("Option {}", index + 1)),
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .selected(d.review.selected_alternative == index)
                    .disabled(self.guided_mask_has_work() || d.review.manually_refined)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.prepare_guided_alternative(index, false, cx)
                    })),
                ),
            );
        }
        let scores = d
            .review
            .alternatives
            .iter()
            .enumerate()
            .map(|(i, a)| format!("Option {}: {:.3}", i + 1, a.estimated_iou))
            .collect::<Vec<_>>()
            .join(" · ");
        let details = format!(
            "EfficientSAM-Ti · Apache 2.0 · Pinned author-hosted ONNX pair {}. Positive marks and one box, at most six prompt tokens. Scores ({scores}) rank model suggestions; they are not calibrated confidence or an editing-quality guarantee. Miniatures show source cutouts; the canvas shows the complete editable result. Model files are SHA-256 verified before inference. {}",
            guided_mask::MODEL_ID,
            if self.guided_mask.has_runtime {
                "Bundled ONNX Runtime available."
            } else {
                "ONNX Runtime unavailable: repair Omuse's application runtime separately; downloading weights cannot repair it."
            }
        );
        div().w(panel_width(window)).h_full().flex_shrink_0().border_l_1().border_color(t.divider()).bg(t.background).flex().flex_col()
            .child(panel_header("Guided mask","Experimental local selection","select",cx))
            .when(!self.guided_mask.has_runtime,|el|el.child(panel_note("The local engine is unavailable. Repair the Omuse installation before using this tool.",cx)))
            .child(button("guided-download",if downloading{"Stop model download"}else{"Download verified local model pack · ~40 MB"},ButtonVariant::Secondary,cx).debug_selector(||"guided-download".into()).disabled(working||(!downloading&&self.guided_mask.has_pack)).on_click(cx.listener(|this,_,_,cx|{if this.guided_mask.download.is_some(){this.stop_guided_mask_job(cx);}else{this.download_guided_mask_pack(cx);}})))
            .child(button("guided-repair","Verify / repair local pack · up to 40 MB",ButtonVariant::Secondary,cx).debug_selector(||"guided-repair".into()).disabled(self.guided_mask_has_work()).on_click(cx.listener(|this,_,_,cx|this.verify_repair_guided_pack(cx))))
            .child(div().id("guided-mask-scroll").flex_1().min_h_0().overflow_y_scroll().child(panel_section(d.source_name.clone(),cx).child(panel_note(d.message.clone(),cx)).child(tools).child(panel_note(format!("{} / 6 model points · box uses two",guided_mask::prompt_count(&d.review.prompts)),cx))
                .child(button("guided-undo-prompt","Remove last prompt",ButtonVariant::Secondary,cx).disabled(working||d.review.prompts.is_empty()||d.review.manually_refined).on_click(cx.listener(|this,_,_,cx|{if let Some(d)=this.guided_mask.draft.as_mut(){if let Err(e)=d.review.undo_prompt(){d.message=e.to_string();}}this.paint_guided_mask(cx);})))
                .child(button("guided-update","Update mask",ButtonVariant::Primary,cx).debug_selector(||"guided-update".into()).disabled(working||self.guided_mask.preview_working||self.guided_mask.opening.is_some()||downloading||!self.guided_mask.has_pack||!self.guided_mask.has_runtime||d.review.prompts.is_empty()||d.review.manually_refined).on_click(cx.listener(|this,_,_,cx|this.update_guided_mask(cx))))
                .child(alternatives)
                .child(button("guided-details",if self.guided_mask.details{"Hide details"}else{"Details"},ButtonVariant::Secondary,cx).debug_selector(||"guided-details".into()).on_click(cx.listener(|this,_,_,cx|{this.guided_mask.details=!this.guided_mask.details;cx.notify();})))
                .when(self.guided_mask.details,|el|el.child(panel_note(details,cx)))
                .child(panel_note(format!("Brush diameter · {:.0} px",d.review.brush_radius*2.),cx))
                .child(div().flex().gap_1()
                    .child(button("guided-brush-smaller","−",ButtonVariant::Secondary,cx).disabled(d.gesture.is_some()).on_click(cx.listener(|this,_,_,cx|{this.guided_mask_brush_command("brush-smaller",cx);})))
                    .child(button("guided-brush-1","1 px",ButtonVariant::Secondary,cx).disabled(d.gesture.is_some()).on_click(cx.listener(|this,_,_,cx|this.set_guided_brush_diameter(1.,cx))))
                    .child(button("guided-brush-8","8 px",ButtonVariant::Secondary,cx).disabled(d.gesture.is_some()).on_click(cx.listener(|this,_,_,cx|this.set_guided_brush_diameter(8.,cx))))
                    .child(button("guided-brush-32","32 px",ButtonVariant::Secondary,cx).disabled(d.gesture.is_some()).on_click(cx.listener(|this,_,_,cx|this.set_guided_brush_diameter(32.,cx))))
                    .child(button("guided-brush-larger","+",ButtonVariant::Secondary,cx).disabled(d.gesture.is_some()).on_click(cx.listener(|this,_,_,cx|{this.guided_mask_brush_command("brush-larger",cx);}))))
                .child(panel_note("Compare the options on your photo. Use Brush remove to tidy the selection manually. This local tool is experimental.",cx))
                .child(button("guided-reset-manual","Reset manual refinements",ButtonVariant::Secondary,cx).disabled(working||!d.review.manually_refined).on_click(cx.listener(|this,_,_,cx|{if let Some(d)=this.guided_mask.draft.as_mut(){let index=d.review.selected_alternative;let _=d;this.prepare_guided_alternative(index,true,cx);return;}this.paint_guided_mask(cx);})))
                .child(button("guided-original",if d.original{"Show mask preview"}else{"Show original"},ButtonVariant::Secondary,cx).disabled(self.guided_mask.preview_working).on_click(cx.listener(|this,_,_,cx|{if let Some(d)=this.guided_mask.draft.as_mut(){d.original=!d.original;}this.paint_guided_mask(cx);})))
                .child(panel_note("Brush size · 1–256 source pixels · [ and ] adjust diameter. Review adds an editable layer mask, preserving original pixels and earlier mask coverage.",cx))))
            .child(div().p_3().border_t_1().border_color(t.divider()).flex().gap_2()
                .child(button("guided-cancel","Cancel",ButtonVariant::Secondary,cx).debug_selector(||"guided-cancel".into()).flex_1().on_click(cx.listener(|this,_,w,cx|{this.cancel_guided_mask(w,cx);})))
                .child(button("guided-keep",if working{"Stop"}else{"Keep mask"},ButtonVariant::Primary,cx).debug_selector(||"guided-keep".into()).flex_1().disabled(!working&&(!self.guided_mask_preview_current())).on_click(cx.listener(|this,_,w,cx|{if this.guided_mask.active.is_some(){this.stop_guided_mask_job(cx);}else{this.keep_guided_mask(w,cx);}}))))
            .child(button("guided-use-selection","Use as selection",ButtonVariant::Secondary,cx).debug_selector(||"guided-use-selection".into()).disabled(working||!self.guided_mask_preview_current()).on_click(cx.listener(|this,_,w,cx|this.use_guided_mask_as_selection(w,cx))))
            .into_any_element()
    }
}
/// Source cutouts only, not full-document renders. Exactly bounded at three
/// 128×80 opaque RGBA buffers, authored on the inference worker before UI publication.
fn candidate_miniatures(
    source: &image::RgbaImage,
    alternatives: &[guided_mask::Alternative],
    cancelled: impl Fn() -> bool,
) -> anyhow::Result<Vec<image::RgbaImage>> {
    anyhow::ensure!(
        !source.is_empty() && alternatives.len() <= 3,
        "Invalid candidate miniature source"
    );
    let mut output = Vec::with_capacity(alternatives.len());
    for alternative in alternatives {
        anyhow::ensure!(
            !alternative.mask.is_empty()
                && alternative.mask.width() <= 1024
                && alternative.mask.height() <= 1024,
            "Invalid candidate miniature mask"
        );
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(128 * 80 * 4)?;
        let scale = (128. / source.width() as f64).min(80. / source.height() as f64);
        let width = source.width() as f64 * scale;
        let height = source.height() as f64 * scale;
        let left = (128. - width) * 0.5;
        let top = (80. - height) * 0.5;
        for y in 0..80 {
            anyhow::ensure!(!cancelled(), "Candidate previews cancelled");
            for x in 0..128 {
                let backdrop = if (x / 8 + y / 8) % 2 == 0 { 52_u32 } else { 72 };
                let dx = x as f64 + 0.5 - left;
                let dy = y as f64 + 0.5 - top;
                if dx >= 0. && dy >= 0. && dx < width && dy < height {
                    let sx = (dx / scale).floor().clamp(0., (source.width() - 1) as f64) as u32;
                    let sy = (dy / scale).floor().clamp(0., (source.height() - 1) as f64) as u32;
                    let mx = (u64::from(sx) * u64::from(alternative.mask.width())
                        / u64::from(source.width())) as u32;
                    let my = (u64::from(sy) * u64::from(alternative.mask.height())
                        / u64::from(source.height())) as u32;
                    let pixel = source.get_pixel(sx, sy);
                    let alpha = (u32::from(pixel[3])
                        * u32::from(alternative.mask.get_pixel(mx, my)[0])
                        + 127)
                        / 255;
                    for c in 0..3 {
                        bytes.push(
                            ((u32::from(pixel[c]) * alpha + backdrop * (255 - alpha) + 127) / 255)
                                as u8,
                        );
                    }
                    bytes.push(255);
                } else {
                    bytes.extend_from_slice(&[backdrop as u8, backdrop as u8, backdrop as u8, 255]);
                }
            }
        }
        output.push(
            image::RgbaImage::from_raw(128, 80, bytes)
                .ok_or_else(|| anyhow::anyhow!("Candidate miniature buffer mismatch"))?,
        );
    }
    anyhow::ensure!(!cancelled(), "Candidate previews cancelled");
    Ok(output)
}
fn editable_source(layers: &[Layer], id: &str, blocked: bool) -> bool {
    for layer in layers {
        let blocked = blocked || layer.locked || !layer.visible || layer.opacity <= 0.;
        if layer.id == id {
            return !blocked;
        }
        if editable_source(&layer.children, id, blocked) {
            return true;
        }
    }
    false
}
fn count_layers(layers: &[Layer]) -> usize {
    layers.iter().fold(0usize, |n, l| {
        n.saturating_add(1)
            .saturating_add(count_layers(&l.children))
    })
}
fn count_image_pixels(layers: &[Layer]) -> u64 {
    layers.iter().fold(0_u64, |n, l| {
        let own = [&l.image, &l.mask]
            .into_iter()
            .flatten()
            .fold(0_u64, |n, i| {
                n.saturating_add(u64::from(i.width()) * u64::from(i.height()))
            });
        n.saturating_add(own)
            .saturating_add(count_image_pixels(&l.children))
    })
}

pub(super) struct GuidedMaskOverlay {
    lines: Vec<Vec<(f32, f32)>>,
    points: Vec<(f32, f32)>,
    width: u32,
    height: u32,
    presented: Option<Arc<std::sync::atomic::AtomicBool>>,
    view: WeakEntity<EditorView>,
}
impl GuidedMaskOverlay {
    pub(super) fn paint(
        self,
        rect: Bounds<Pixels>,
        viewport: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.width == 0
            || self.height == 0
            || rect.size.width <= px(0.)
            || rect.size.height <= px(0.)
            || viewport.size.width <= px(0.)
            || viewport.size.height <= px(0.)
            || rect.origin.x >= viewport.origin.x + viewport.size.width
            || rect.origin.y >= viewport.origin.y + viewport.size.height
            || rect.origin.x + rect.size.width <= viewport.origin.x
            || rect.origin.y + rect.size.height <= viewport.origin.y
        {
            return;
        }
        let screen = |p: (f32, f32)| {
            point(
                rect.origin.x + px(p.0 * f32::from(rect.size.width) / self.width as f32),
                rect.origin.y + px(p.1 * f32::from(rect.size.height) / self.height as f32),
            )
        };
        window.with_content_mask(Some(gpui_kit::ContentMask { bounds: viewport }), |window| {
            let mut path = gpui_kit::PathBuilder::stroke(px(1.5));
            for line in self.lines {
                let mut it = line.into_iter();
                if let Some(p) = it.next() {
                    path.move_to(screen(p));
                    for p in it {
                        path.line_to(screen(p));
                    }
                }
            }
            for p in self.points {
                let p = screen(p);
                path.move_to(p - point(px(5.), px(0.)));
                path.line_to(p + point(px(5.), px(0.)));
                path.move_to(p - point(px(0.), px(5.)));
                path.line_to(p + point(px(0.), px(5.)));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(0xf5dc64));
            }
        });
        if let Some(presented) = self.presented {
            if !presented.swap(true, std::sync::atomic::Ordering::AcqRel) {
                let view = self.view;
                cx.defer(move |cx| {
                    let _ = view.update(cx, |_, cx| cx.notify());
                });
            }
        }
    }
}

#[path = "guided_mask_native.rs"]
mod native;
