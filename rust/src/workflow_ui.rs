//! Cancellable smart-source, colour, recipe, and multi-image workflows.
use super::*;
use anyhow::Context as _;
use omuse::{
    advanced::LayerState,
    model::Layer,
    multiframe::{CancellationToken, ExposureFrame, FocusFrame, MultiImageOptions},
    precision::WorkingSpace,
    raw_import::DevelopSettings,
    recipes::{Recipe, Step},
};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(all(test, feature = "ui-test"))]
#[path = "workflow_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkflowKind {
    Source,
    Raw,
    Colour,
    Automation,
    Merge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Import,
    Refresh,
    Duplicate,
    Redevelop,
    Convert(WorkingSpace),
    Proof,
    Export16,
    LoadRecipe,
    SaveRecipe,
    ApplyRecipe,
    Batch,
    Focus,
    Hdr,
    Panorama,
}

pub(super) struct WorkflowDraft {
    kind: WorkflowKind,
    layer: String,
    revision: u64,
    action: Action,
    cancel: Arc<AtomicBool>,
    multi_cancel: CancellationToken,
    job: u64,
    progress_done: Arc<AtomicUsize>,
    progress_total: Arc<AtomicUsize>,
    window: gpui_kit::AnyWindowHandle,
}
impl Drop for WorkflowDraft {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.multi_cancel.cancel();
    }
}

pub(super) enum Outcome {
    Replace(Vec<(String, LayerState)>),
    Insert(Layer, Option<(u32, u32)>),
    Proof(omuse::proofing::Settings),
    Recipe(Recipe),
    Saved,
    Batch(omuse::recipes::BatchReport),
    Exported,
    Derived(String, Layer),
}

impl EditorView {
    pub(super) fn workflow_title(&self) -> &'static str {
        match self.workflow_draft.as_ref().map(|d| d.kind) {
            Some(WorkflowKind::Source) => "Smart source",
            Some(WorkflowKind::Raw) => "Develop embedded RAW",
            Some(WorkflowKind::Colour) => "Precision & colour",
            Some(WorkflowKind::Automation) => "Automation",
            Some(WorkflowKind::Merge) => "Multi-image merge",
            None => "Workflow",
        }
    }
    pub(super) fn open_workflow(
        &mut self,
        kind: WorkflowKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_workflow(cx);
        let layer = self.editor.active_layer.clone();
        let revision = self.editor.revision();
        let action = match kind {
            WorkflowKind::Source => Action::Import,
            WorkflowKind::Raw => Action::Redevelop,
            WorkflowKind::Colour => Action::Convert(WorkingSpace::Srgb),
            WorkflowKind::Automation => Action::ApplyRecipe,
            WorkflowKind::Merge => Action::Focus,
        };
        self.workflow_draft = Some(WorkflowDraft {
            kind,
            layer,
            revision,
            action,
            cancel: Arc::new(AtomicBool::new(false)),
            multi_cancel: CancellationToken::new(),
            job: 0,
            progress_done: Arc::new(AtomicUsize::new(0)),
            progress_total: Arc::new(AtomicUsize::new(0)),
            window: window.window_handle(),
        });
        let defaults: &[&str] = match kind {
            WorkflowKind::Raw => &["0", "5000", "0", "1"],
            WorkflowKind::Colour => &["", "", "1", "0"],
            WorkflowKind::Automation => &["", "png"],
            WorkflowKind::Merge => &["0,0", "64"],
            _ => &[],
        };
        for (input, value) in self.detail_inputs.iter().zip(defaults) {
            input.update(cx, |s, cx| s.set_value(*value, window, cx));
        }
        if kind == WorkflowKind::Colour {
            let intent = self.proof_settings.intent.to_string();
            let bpc = if self.proof_settings.black_point_compensation {
                "1"
            } else {
                "0"
            };
            for (input, value) in self.detail_inputs.iter().zip([
                self.proof_settings.proof_profile.as_deref().unwrap_or(""),
                intent.as_str(),
                bpc,
            ]) {
                input.update(cx, |s, cx| s.set_value(value, window, cx));
            }
        }
        if kind == WorkflowKind::Automation {
            let values = [
                self.editor.document.width.to_string(),
                self.editor.document.height.to_string(),
                "0".into(),
                "0".into(),
                "1".into(),
                "horizontal".into(),
            ];
            for (input, value) in self.detail_inputs[2..8].iter().zip(values) {
                input.update(cx, |s, cx| s.set_value(value, window, cx));
            }
        }
        self.path_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        if kind == WorkflowKind::Colour {
            let monitor = self
                .proof_settings
                .monitor_profile
                .clone()
                .unwrap_or_default();
            self.path_input
                .update(cx, |s, cx| s.set_value(monitor, window, cx));
        }
        self.dialog = Dialog::Workflow;
        self.modal_focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn clear_workflow(&mut self, _cx: &mut App) {
        if let Some(d) = self.workflow_draft.take() {
            d.cancel.store(true, Ordering::Relaxed);
            d.multi_cancel.cancel();
        }
        self.busy = false;
    }
    fn raw_settings(&self, cx: &Context<Self>) -> anyhow::Result<DevelopSettings> {
        let v = (0..4)
            .map(|i| self.detail_inputs[i].read(cx).value().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()?;
        let mut s = DevelopSettings::default();
        s.exposure = v[0];
        s.temperature = v[1];
        s.tint = v[2];
        s.boost = v[3];
        s.validate()?;
        Ok(s)
    }
    pub(super) fn run_workflow(
        &mut self,
        _apply: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(d) = self.workflow_draft.as_mut() else {
            return;
        };
        d.cancel.store(true, Ordering::Relaxed);
        d.multi_cancel.cancel();
        d.cancel = Arc::new(AtomicBool::new(false));
        d.multi_cancel = CancellationToken::new();
        d.job = d.job.wrapping_add(1);
        d.progress_done = Arc::new(AtomicUsize::new(0));
        d.progress_total = Arc::new(AtomicUsize::new(0));
        let job = d.job;
        let generation = self.dialog_generation;
        let layer = d.layer.clone();
        let revision = d.revision.clone();
        let action = d.action;
        let cancel = d.cancel.clone();
        let multi_cancel = d.multi_cancel.clone();
        let progress_done = d.progress_done.clone();
        let progress_total = d.progress_total.clone();
        let path = PathBuf::from(self.path_input.read(cx).value().as_ref());
        let second = PathBuf::from(self.detail_inputs[0].read(cx).value().as_ref());
        let option = self.detail_inputs[1].read(cx).value().to_string();
        let raw = if action == Action::Redevelop {
            match self.raw_settings(cx) {
                Ok(v) => v,
                Err(e) => {
                    self.status = e.to_string();
                    cx.notify();
                    return;
                }
            }
        } else {
            DevelopSettings::default()
        };
        let active = self.editor.editable_state(&layer).ok();
        let mut all_states = Vec::new();
        collect_states(&self.editor.document.layers, &mut all_states);
        let recipe = self.recorded_recipe.clone();
        let mut proof = self.proof_settings.clone();
        if action == Action::Proof {
            proof.monitor_profile =
                (!path.as_os_str().is_empty()).then(|| path.to_string_lossy().into_owned());
            proof.proof_profile =
                (!second.as_os_str().is_empty()).then(|| second.to_string_lossy().into_owned());
            proof.intent = match option.parse() {
                Ok(intent @ 0..=3) => intent,
                _ => {
                    self.status = "Rendering intent must be an integer from 0 to 3".into();
                    cx.notify();
                    return;
                }
            };
            proof.black_point_compensation = match self.detail_inputs[2].read(cx).value().as_ref() {
                "0" => false,
                "1" => true,
                _ => {
                    self.status = "Black point compensation must be 0 or 1".into();
                    cx.notify();
                    return;
                }
            };
            proof.enabled = true;
        }
        let target_id = layer.clone();
        let export_document = (action == Action::Export16).then(|| self.editor.document.clone());
        let canvas_dimensions = (self.editor.document.width, self.editor.document.height);
        self.busy = true;
        self.status = "Running workflow…".into();
        let task = cx.background_executor().spawn(async move {
            run_action(
                action,
                path,
                second,
                option,
                raw,
                &target_id,
                active,
                all_states,
                recipe,
                proof,
                cancel,
                multi_cancel,
                progress_done,
                progress_total,
                export_document,
                canvas_dimensions,
            )
        });
        self.poll_workflow_progress(job, generation, cx);
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::Workflow
                    || this.dialog_generation != generation
                    || !this
                        .workflow_draft
                        .as_ref()
                        .is_some_and(|d| d.job == job && d.layer == layer && d.revision == revision)
                    || this.editor.revision() != revision
                {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(out) => match this.commit_workflow(out) {
                        Ok((msg, document_changed)) => {
                            let window = this.workflow_draft.as_ref().map(|d| d.window);
                            this.status = msg;
                            if document_changed {
                                this.changed(cx);
                            } else {
                                this.refresh(cx);
                            }
                            this.clear_workflow(cx);
                            this.dialog = Dialog::None;
                            this.dialog_generation = this.dialog_generation.wrapping_add(1);
                            let focus = this.focus.clone();
                            if let Some(window) = window {
                                cx.defer(move |cx| {
                                    let _ = cx.update_window(window, |_, window, cx| {
                                        focus.focus(window, cx)
                                    });
                                });
                            }
                        }
                        Err(e) => this.status = format!("Workflow: {e:#}"),
                    },
                    Err(e) => this.status = format!("Workflow: {e:#}"),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn poll_workflow_progress(&mut self, job: u64, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let _ = view.update(cx, |this, cx| {
                let Some(draft) = this.workflow_draft.as_ref() else {
                    return;
                };
                if this.dialog != Dialog::Workflow
                    || this.dialog_generation != generation
                    || draft.job != job
                    || !this.busy
                {
                    return;
                }
                let done = draft.progress_done.load(Ordering::Relaxed);
                let total = draft.progress_total.load(Ordering::Relaxed);
                if total > 0 {
                    this.status = format!("Batch processing {done} of {total}…");
                    cx.notify();
                }
                this.poll_workflow_progress(job, generation, cx);
            });
        })
        .detach();
    }
    pub(super) fn commit_workflow(&mut self, out: Outcome) -> anyhow::Result<(String, bool)> {
        match out {
            Outcome::Replace(states) => {
                self.editor.replace_editable_states(states)?;
                Ok(("Editable source updated".into(), true))
            }
            Outcome::Insert(layer, size) => {
                self.editor
                    .insert_prepared_layers_sized(vec![layer], size)?;
                Ok(("Merged image added as a new editable layer".into(), true))
            }
            Outcome::Derived(source_id, mut layer) => {
                let source = self
                    .editor
                    .document
                    .find_layer(&source_id)
                    .context("Recipe source changed")?;
                let old = source
                    .image
                    .as_ref()
                    .context("Recipe source has no pixels")?
                    .dimensions();
                let new = layer
                    .image
                    .as_ref()
                    .context("Recipe result has no pixels")?
                    .dimensions();
                layer.offset_x = source.offset_x;
                layer.offset_y = source.offset_y;
                layer.rotation = source.rotation;
                layer.scale_x = source.scale_x * old.0 as f32 / new.0 as f32;
                layer.scale_y = source.scale_y * old.1 as f32 / new.1 as f32;
                self.editor.insert_derived_layers(&source_id, vec![layer])?;
                Ok(("Recipe result added above its hidden source".into(), true))
            }
            Outcome::Proof(settings) => {
                self.proof_settings = settings;
                Ok((
                    "Display proof settings updated; document pixels unchanged".into(),
                    false,
                ))
            }
            Outcome::Recipe(recipe) => {
                self.recorded_recipe = recipe;
                Ok(("Recipe loaded".into(), false))
            }
            Outcome::Saved => Ok((
                "Recipe saved without replacing an existing file".into(),
                false,
            )),
            Outcome::Batch(report) => {
                let succeeded = report
                    .items
                    .iter()
                    .filter(|item| item.error.is_none())
                    .count();
                let failed = report.items.len() - succeeded;
                let cancelled = usize::from(report.cancelled);
                self.last_batch_report = Some(report);
                Ok((
                    format!(
                        "Batch finished: {succeeded} succeeded, {failed} failed, {cancelled} cancelled"
                    ),
                    false,
                ))
            }
            Outcome::Exported => Ok(("Full-precision export saved".into(), false)),
        }
    }
    fn choose_workflow_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Some(d) = self.workflow_draft.as_mut() {
            d.action = action;
        }
        cx.notify();
    }
    fn add_recipe_step(&mut self, kind: &str, cx: &mut Context<Self>) {
        let parse = |index: usize| {
            self.detail_inputs[index]
                .read(cx)
                .value()
                .parse::<u32>()
                .map_err(|_| anyhow::anyhow!("Enter whole-number recipe dimensions"))
        };
        let step = match kind {
            "resize" => parse(2).and_then(|width| {
                Ok(Step::Resize {
                    width,
                    height: parse(3)?,
                })
            }),
            "crop" => parse(2).and_then(|width| {
                Ok(Step::Crop {
                    x: parse(4)?,
                    y: parse(5)?,
                    width,
                    height: parse(3)?,
                })
            }),
            "rotate" => self.detail_inputs[6]
                .read(cx)
                .value()
                .parse::<u8>()
                .map(|clockwise_quarters| Step::Rotate { clockwise_quarters })
                .map_err(|_| anyhow::anyhow!("Quarter turns must be 0 through 3")),
            "flip" => Ok(Step::Flip {
                horizontal: self.detail_inputs[7].read(cx).value().as_ref() != "vertical",
            }),
            _ => return,
        };
        match step.and_then(|step| {
            let mut recipe = self.recorded_recipe.clone();
            recipe.steps.push(step);
            recipe.validate()?;
            self.recorded_recipe = recipe;
            Ok(())
        }) {
            Ok(()) => {
                self.status = format!("Recipe now has {} steps", self.recorded_recipe.steps.len())
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }
    pub(super) fn workflow_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(kind) = self.workflow_draft.as_ref().map(|d| d.kind) else {
            return div().into_any_element();
        };
        let mut body = div()
            .id("workflow-controls")
            .debug_selector(|| "workflow-controls".into())
            .flex()
            .flex_col()
            .gap_2()
            .text_sm();
        body=body.child(match kind {WorkflowKind::Source=>"Embed or refresh a linked file. Refresh Same Source updates every layer sharing the source identity atomically.",WorkflowKind::Raw=>"Redevelops the embedded RAW original; external files are not substituted.",WorkflowKind::Colour=>"Working-space conversion keeps appearance. ICC proofing affects display only.",WorkflowKind::Automation=>"Recipes create new results and batch never overwrites inputs or existing outputs.",WorkflowKind::Merge=>"Sorted folder inputs; registration is translation-only and bounded."});
        if !matches!(kind, WorkflowKind::Raw | WorkflowKind::Colour) {
            body = body.child(
                input("workflow-path", &self.path_input, window, cx)
                    .debug_selector(|| "workflow-path".into()),
            );
        }
        match kind {
            WorkflowKind::Source => {
                body = body.child(actions(
                    self,
                    cx,
                    &[
                        ("workflow-import", "Import / replace", Action::Import),
                        ("workflow-refresh", "Refresh same source", Action::Refresh),
                        (
                            "workflow-duplicate",
                            "Duplicate linked instance",
                            Action::Duplicate,
                        ),
                    ],
                ));
            }
            WorkflowKind::Raw => {
                body = body
                    .child(fields(
                        self,
                        window,
                        cx,
                        &[
                            "Exposure (-3…3)",
                            "Temperature (2000…12000)",
                            "Tint (-150…150)",
                            "Boost (0…1)",
                        ],
                    ))
                    .child(actions(
                        self,
                        cx,
                        &[("workflow-raw", "Redevelop", Action::Redevelop)],
                    ));
            }
            WorkflowKind::Colour => {
                body = body
                    .child(
                        if self
                            .workflow_draft
                            .as_ref()
                            .is_some_and(|d| d.action == Action::Export16)
                        {
                            "Export destination · .png or .tiff"
                        } else {
                            "Monitor ICC path · blank uses sRGB"
                        },
                    )
                    .child(
                        input("workflow-monitor-icc", &self.path_input, window, cx)
                            .debug_selector(|| "workflow-monitor-icc".into()),
                    )
                    .child(fields(
                        self,
                        window,
                        cx,
                        &[
                            "Proof ICC path",
                            "Intent (0…3)",
                            "Black point compensation (0/1)",
                        ],
                    ))
                    .child(actions(
                        self,
                        cx,
                        &[
                            ("workflow-srgb", "sRGB", Action::Convert(WorkingSpace::Srgb)),
                            (
                                "workflow-linear",
                                "Linear sRGB",
                                Action::Convert(WorkingSpace::LinearSrgb),
                            ),
                            (
                                "workflow-p3",
                                "Display P3",
                                Action::Convert(WorkingSpace::DisplayP3),
                            ),
                            (
                                "workflow-proof",
                                "Apply display proof settings",
                                Action::Proof,
                            ),
                            (
                                "workflow-export16",
                                "Export 16-bit PNG/TIFF",
                                Action::Export16,
                            ),
                        ],
                    ))
                    .child(
                        button(
                            "workflow-proof-off",
                            "Disable display proof",
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "workflow-proof-off".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.proof_settings.enabled = false;
                            this.refresh(cx);
                            this.status = "Display proof disabled; artwork is unchanged".into();
                            cx.notify();
                        })),
                    );
            }
            WorkflowKind::Automation => {
                body = body
                    .child(
                        button(
                            "workflow-record",
                            if self.macro_recording {
                                "Stop recording"
                            } else {
                                "Record supported edits"
                            },
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "workflow-record".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.macro_recording = !this.macro_recording;
                            if this.macro_recording {
                                this.macro_revision = this.editor.revision();
                            }
                            this.status = if this.macro_recording { "Recording supported filter and quick-adjust operations. Reopen Automation and press Stop recording when finished." } else { "Recipe recording stopped" }.into();
                            this.clear_workflow(cx); this.dialog=Dialog::None;
                            this.dialog_generation=this.dialog_generation.wrapping_add(1);
                            this.focus.focus(window,cx);
                            cx.notify();
                        })),
                    )
                    .child(indexed_fields(self, window, cx, &[(2,"Width"),(3,"Height"),(4,"Crop X"),(5,"Crop Y"),(6,"Quarter turns"),(7,"Flip axis")]))
                    .child(
                        div().flex().flex_wrap().gap_2()
                            .child(button("workflow-add-resize","Add Resize",ButtonVariant::Outline,cx).debug_selector(||"workflow-add-resize".into()).on_click(cx.listener(|this,_,_,cx|this.add_recipe_step("resize",cx))))
                            .child(button("workflow-add-crop","Add Crop",ButtonVariant::Outline,cx).debug_selector(||"workflow-add-crop".into()).on_click(cx.listener(|this,_,_,cx|this.add_recipe_step("crop",cx))))
                            .child(button("workflow-add-rotate","Add Rotate",ButtonVariant::Outline,cx).debug_selector(||"workflow-add-rotate".into()).on_click(cx.listener(|this,_,_,cx|this.add_recipe_step("rotate",cx))))
                            .child(button("workflow-add-flip","Add Flip",ButtonVariant::Outline,cx).debug_selector(||"workflow-add-flip".into()).on_click(cx.listener(|this,_,_,cx|this.add_recipe_step("flip",cx))))
                            .child(button("workflow-clear-recipe","New / clear recipe",ButtonVariant::Outline,cx).debug_selector(||"workflow-clear-recipe".into()).on_click(cx.listener(|this,_,_,cx|{this.recorded_recipe=Recipe::default();this.status="New empty recipe".into();cx.notify();})))
                            .child(format!("{} steps",self.recorded_recipe.steps.len())),
                    )
                    .child(fields(
                        self,
                        window,
                        cx,
                        &["Output folder", "Format (png/jpg/webp/tiff)"],
                    ))
                    .child(actions(
                        self,
                        cx,
                        &[
                            ("workflow-load-recipe", "Load recipe", Action::LoadRecipe),
                            ("workflow-save-recipe", "Save recipe", Action::SaveRecipe),
                            (
                                "workflow-apply-recipe",
                                "Apply as new layer",
                                Action::ApplyRecipe,
                            ),
                            ("workflow-batch", "Run folder batch", Action::Batch),
                        ],
                    ));
                if let Some(report) = &self.last_batch_report {
                    let succeeded = report
                        .items
                        .iter()
                        .filter(|item| item.error.is_none())
                        .count();
                    let failed = report.items.len() - succeeded;
                    let cancelled = usize::from(report.cancelled);
                    let mut report_view = div()
                        .id("workflow-batch-report")
                        .debug_selector(|| "workflow-batch-report".into())
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(format!(
                            "Last batch: {succeeded} succeeded, {failed} failed, {cancelled} cancelled"
                        ));
                    for (index, item) in report.items.iter().take(20).enumerate() {
                        let filename = item
                            .input
                            .file_name()
                            .map(|name| name.to_string_lossy())
                            .unwrap_or_else(|| item.input.to_string_lossy());
                        let result = item.error.as_deref().unwrap_or("Succeeded");
                        let row_id =
                            SharedString::from(format!("workflow-batch-report-row-{index}"));
                        let selector = row_id.clone();
                        report_view = report_view.child(
                            div()
                                .id(row_id)
                                .debug_selector(move || selector.to_string())
                                .child(format!("{filename}: {result}")),
                        );
                    }
                    if report.items.len() > 20 {
                        report_view = report_view.child(format!(
                            "{} more results retained in this session",
                            report.items.len() - 20
                        ));
                    }
                    body = body.child(report_view);
                }
            }
            WorkflowKind::Merge => {
                body = body
                    .child(fields(
                        self,
                        window,
                        cx,
                        &[
                            "Exposure stops, comma-separated (HDR)",
                            "Alignment shift limit",
                        ],
                    ))
                    .child(actions(
                        self,
                        cx,
                        &[
                            ("workflow-focus", "Focus stack", Action::Focus),
                            ("workflow-hdr", "HDR merge", Action::Hdr),
                            ("workflow-panorama", "Panorama", Action::Panorama),
                        ],
                    ));
            }
        }
        body.into_any_element()
    }
}
fn actions(
    this: &EditorView,
    cx: &mut Context<EditorView>,
    items: &[(&'static str, &'static str, Action)],
) -> AnyElement {
    let mut row = div().flex().flex_wrap().gap_2();
    for &(id, label, action) in items {
        let active = this
            .workflow_draft
            .as_ref()
            .is_some_and(|d| d.action == action);
        row = row.child(
            button(
                id,
                label,
                if active {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                },
                cx,
            )
            .debug_selector(move || id.into())
            .on_click(cx.listener(move |this, _, _, cx| this.choose_workflow_action(action, cx))),
        );
    }
    row.into_any_element()
}
fn fields(
    this: &EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    labels: &[&'static str],
) -> AnyElement {
    let mut out = div().flex().gap_2();
    for (i, label) in labels.iter().enumerate() {
        let id = SharedString::from(format!("workflow-{i}"));
        let selector = id.clone();
        out = out.child(
            div().flex_1().min_w_0().child(*label).child(
                input(id, &this.detail_inputs[i], window, cx)
                    .debug_selector(move || selector.to_string()),
            ),
        );
    }
    out.into_any_element()
}
fn indexed_fields(
    this: &EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    items: &[(usize, &'static str)],
) -> AnyElement {
    let mut out = div().flex().flex_wrap().gap_2();
    for &(index, label) in items {
        let id = SharedString::from(format!("workflow-step-{index}"));
        let selector = id.clone();
        out = out.child(
            div().w(px(120.)).child(label).child(
                input(id, &this.detail_inputs[index], window, cx)
                    .debug_selector(move || selector.to_string()),
            ),
        );
    }
    out.into_any_element()
}
fn run_action(
    action: Action,
    path: PathBuf,
    second: PathBuf,
    option: String,
    raw: DevelopSettings,
    target_id: &str,
    active: Option<LayerState>,
    states: Vec<(String, LayerState)>,
    recipe: Recipe,
    proof: omuse::proofing::Settings,
    cancel: Arc<AtomicBool>,
    multi_cancel: CancellationToken,
    progress_done: Arc<AtomicUsize>,
    progress_total: Arc<AtomicUsize>,
    export_document: Option<Document>,
    canvas_dimensions: (u32, u32),
) -> anyhow::Result<Outcome> {
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Workflow cancelled");
    match action {
        Action::Import => Ok(Outcome::Replace(vec![(
            target_id.to_owned(),
            omuse::smart_source::import(&path, raw, &cancel)?,
        )])),
        Action::Refresh => {
            let current = active.ok_or_else(|| anyhow::anyhow!("Choose an editable source"))?;
            let linked = current
                .recipe
                .linked_path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Source has no linked path"))?;
            let mut replacements = vec![];
            for (id, old) in states
                .into_iter()
                .filter(|(_, s)| s.recipe.source_id == current.recipe.source_id)
            {
                let imported = omuse::smart_source::import(
                    Path::new(&linked),
                    old.recipe.raw_settings.unwrap_or_default(),
                    &cancel,
                )?;
                let mut next = old.clone();
                next.source = imported.source;
                next.result = next.source.clone();
                next.raw_bytes = imported.raw_bytes;
                next.recipe.raw_extension = imported.recipe.raw_extension;
                next.recipe.raw_settings = imported.recipe.raw_settings;
                next = next.evaluate(&cancel)?;
                replacements.push((id, next));
            }
            anyhow::ensure!(!replacements.is_empty(), "No matching smart sources");
            Ok(Outcome::Replace(replacements))
        }
        Action::Duplicate => {
            let state = active.ok_or_else(|| anyhow::anyhow!("Choose an editable source"))?;
            let pixels = state.proxy()?;
            let mut layer = Layer::paint(
                format!("{} linked copy", state.recipe.source_name),
                pixels.width(),
                pixels.height(),
            );
            layer.image = Some(pixels.into());
            layer.advanced = Some(Arc::new(state));
            Ok(Outcome::Insert(layer, None))
        }
        Action::Redevelop => {
            let state = active.ok_or_else(|| anyhow::anyhow!("Choose an editable RAW source"))?;
            Ok(Outcome::Replace(vec![(
                target_id.to_owned(),
                omuse::smart_source::redevelop(&state, raw, &cancel)?,
            )]))
        }
        Action::Convert(space) => {
            let mut state = active.ok_or_else(|| anyhow::anyhow!("Choose an editable source"))?;
            let mut source = (*state.source).clone();
            source.convert_working_space(space)?;
            state.source = Arc::new(source);
            state.recipe.working_space = space;
            state = state.evaluate(&cancel)?;
            Ok(Outcome::Replace(vec![(target_id.to_owned(), state)]))
        }
        Action::Proof => {
            omuse::proofing::render(
                &image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 128, 255])),
                &proof,
            )?;
            Ok(Outcome::Proof(proof))
        }
        Action::Export16 => {
            omuse::raster::export16_cancellable(
                export_document
                    .as_ref()
                    .context("Missing export document")?,
                &path,
                &cancel,
            )?;
            Ok(Outcome::Exported)
        }
        Action::LoadRecipe => Ok(Outcome::Recipe(Recipe::load(&path)?)),
        Action::SaveRecipe => {
            recipe.save(&path)?;
            Ok(Outcome::Saved)
        }
        Action::ApplyRecipe => {
            let state = active.ok_or_else(|| anyhow::anyhow!("Choose an image layer"))?;
            let pixels = recipe.apply(&state.proxy()?, &cancel)?;
            Ok(Outcome::Derived(
                target_id.to_owned(),
                layer_from_pixels("Recipe result", pixels)?,
            ))
        }
        Action::Batch => Ok(Outcome::Batch(omuse::recipes::batch(
            &recipe,
            &path,
            &second,
            option.trim(),
            &cancel,
            |done, total| {
                progress_done.store(done, Ordering::Relaxed);
                progress_total.store(total, Ordering::Relaxed);
            },
        )?)),
        Action::Focus | Action::Hdr | Action::Panorama => merge_folder(
            action,
            &path,
            &second,
            &option,
            &cancel,
            multi_cancel,
            canvas_dimensions,
        ),
    }
}
fn merge_folder(
    action: Action,
    folder: &Path,
    exposures: &Path,
    max_shift: &str,
    cancel: &AtomicBool,
    multi_cancel: CancellationToken,
    canvas_dimensions: (u32, u32),
) -> anyhow::Result<Outcome> {
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Merge cancelled");
    let max_shift = parse_max_shift(max_shift)?;
    anyhow::ensure!(folder.is_dir(), "Choose an input folder");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Merge cancelled");
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
            anyhow::ensure!(paths.len() <= 64, "Merge accepts at most 64 images");
        }
    }
    paths.sort();
    anyhow::ensure!(
        (2..=64).contains(&paths.len()),
        "Merge needs 2 through 64 regular image files"
    );
    let mut images = vec![];
    let mut source_bytes = 0usize;
    for path in paths {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Merge cancelled");
        let state = omuse::smart_source::import(&path, DevelopSettings::default(), cancel)?;
        source_bytes = source_bytes.saturating_add(state.source.memory_bytes());
        anyhow::ensure!(
            source_bytes <= 256 * 1024 * 1024,
            "Merge sources exceed 256 MiB; use fewer or smaller images"
        );
        images.push((*state.source).clone());
    }
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Merge cancelled");
    let options = MultiImageOptions {
        max_shift,
        cancellation: Some(multi_cancel),
        ..Default::default()
    };
    let (image, name) = match action {
        Action::Focus => (
            omuse::multiframe::focus_stack(
                &images
                    .into_iter()
                    .map(|image| FocusFrame { image })
                    .collect::<Vec<_>>(),
                options,
            )?
            .image,
            "Focus stack",
        ),
        Action::Panorama => (
            omuse::multiframe::panorama(&images, options)?.image,
            "Panorama",
        ),
        Action::Hdr => {
            let values = exposures
                .to_string_lossy()
                .split(',')
                .map(|v| v.trim().parse::<f32>())
                .collect::<Result<Vec<_>, _>>()?;
            anyhow::ensure!(
                values.len() == images.len(),
                "Provide one exposure stop per HDR image"
            );
            let frames = images
                .into_iter()
                .zip(values)
                .map(|(image, exposure_stops)| ExposureFrame {
                    image,
                    exposure_stops,
                })
                .collect::<Vec<_>>();
            (
                omuse::multiframe::merge_exposure(&frames, options)?
                    .0
                    .to_linear_tiled(1.)?,
                "HDR merge",
            )
        }
        _ => unreachable!(),
    };
    let dimensions = image.dimensions();
    let mut state = LayerState::from_image(&image.to_rgba8_in(WorkingSpace::Srgb)?, name)?;
    state.source = Arc::new(image);
    state.result = state.source.clone();
    state.recipe.working_space = state.source.working_space();
    let mut layer = Layer::paint(name, dimensions.0, dimensions.1);
    layer.image = Some(state.proxy()?.into());
    layer.advanced = Some(Arc::new(state));
    Ok(Outcome::Insert(
        layer,
        Some(expanded_canvas(canvas_dimensions, dimensions)),
    ))
}
fn parse_max_shift(value: &str) -> anyhow::Result<i32> {
    value
        .trim()
        .parse::<i32>()
        .context("Alignment shift must be a whole number")
}
fn expanded_canvas(current: (u32, u32), required: (u32, u32)) -> (u32, u32) {
    (current.0.max(required.0), current.1.max(required.1))
}
fn layer_from_pixels(name: &str, pixels: image::RgbaImage) -> anyhow::Result<Layer> {
    let state = LayerState::from_image(&pixels, name)?;
    let mut layer = Layer::paint(name, pixels.width(), pixels.height());
    layer.image = Some(pixels.into());
    layer.advanced = Some(Arc::new(state));
    Ok(layer)
}

fn collect_states(layers: &[Layer], output: &mut Vec<(String, LayerState)>) {
    for layer in layers {
        if let Some(state) = &layer.advanced {
            output.push((layer.id.clone(), (**state).clone()));
        }
        collect_states(&layer.children, output);
    }
}
