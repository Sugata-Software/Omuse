//! Shared Camera Raw import review. Source claims remain inert and every
//! accepted grade uses the existing native Preview/Apply/Undo transaction.
use super::*;
use anyhow::{Result, ensure};
use gpui_kit::Focusable;
use omuse::xmp_preset::{MappingReport, PresetPlan, SourceKind, TargetIdentity};

#[derive(Clone)]
struct Admission {
    id: u64,
    key: SourceKey,
    target: TargetIdentity,
    base: serde_json::Value,
}
#[derive(Clone)]
struct Review {
    admission: Admission,
    plan: PresetPlan,
}
#[derive(Clone)]
struct Accepted {
    key: SourceKey,
    report: MappingReport,
    settings: serde_json::Value,
}
#[derive(Default)]
pub(super) struct XmpImportState {
    next_id: u64,
    active: Option<u64>,
    pending_key: Option<SourceKey>,
    pub(super) promotion: Option<Arc<AtomicBool>>,
    path: Option<Entity<InputState>>,
    review: Option<Review>,
    accepted: Option<Accepted>,
    message: Option<String>,
}
impl Drop for XmpImportState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.promotion {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl EditorView {
    pub(super) fn camera_xmp_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut state = XmpImportState::default();
        state.path = Some(cx.new(|cx| InputState::new(window, cx).placeholder("Local XMP path")));
        self.camera_preview.xmp = state;
    }
    fn camera_xmp_target(&self, settings: &Settings) -> Result<TargetIdentity> {
        let layer = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .context("Select a photo layer")?;
        let state = layer.advanced.as_ref();
        let json = serde_json::to_value(settings)?;
        Ok(TargetIdentity {
            kind: if state.is_some_and(|state| {
                state.raw_bytes.is_some() && state.recipe.raw_extension.is_some()
            }) {
                SourceKind::DecodedRaw
            } else {
                SourceKind::Rendered
            },
            layer_id: layer.id.clone(),
            source_id: state
                .map(|state| state.recipe.source_id.clone())
                .unwrap_or_else(|| {
                    format!(
                        "paint:{}:{}",
                        self.editor.instance_id(),
                        self.editor.revision()
                    )
                }),
            editor_instance: self.editor.instance_id(),
            revision: self.editor.revision(),
            working_space: state
                .map(|state| format!("{:?}", state.recipe.working_space))
                .unwrap_or_else(|| "Srgb".into()),
            processing: json
                .get("processing")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ByteCompatible")
                .into(),
            tone_mapping: json
                .get("toneMapping")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Legacy")
                .into(),
            // These are unknown unless an actual retained-source descriptor
            // supplies them. Never fill them from the imported XMP's claims.
            camera_make: None,
            camera_model: None,
            camera_profile: None,
        })
    }
    fn camera_xmp_admit(&mut self, cx: &Context<Self>) -> Result<Admission> {
        ensure!(
            self.dialog == Dialog::CameraRaw && !self.busy,
            "Finish the current camera operation first"
        );
        ensure!(
            self.camera_preview.xmp.promotion.is_none(),
            "Finish or cancel the photo promotion first"
        );
        self.camera_source()?;
        self.read_camera_form(cx)?;
        let settings: Settings = serde_json::from_value(self.camera_draft.clone())?;
        omuse::camera_raw::validate(&settings)?;
        let target = self.camera_xmp_target(&settings)?;
        let key = self.camera_source_key();
        let state = &mut self.camera_preview.xmp;
        state.next_id = state.next_id.wrapping_add(1);
        state.active = Some(state.next_id);
        state.pending_key = Some(key.clone());
        state.review = None;
        state.message = None;
        Ok(Admission {
            id: state.next_id,
            key,
            target,
            base: self.camera_draft.clone(),
        })
    }
    fn camera_xmp_drop_stale_form(&mut self, admission: &Admission, cx: &mut Context<Self>) {
        // This stored key identifies the actual pending job even if the current
        // editor/page/selection changed. A reopened dialog can reuse a small
        // request number but can never inherit this exact source/dialog key.
        if self.camera_preview.xmp.active == Some(admission.id)
            && self.camera_preview.xmp.pending_key.as_ref() == Some(&admission.key)
        {
            if let Some(cancel) = self.camera_preview.xmp.promotion.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.camera_preview.xmp.active = None;
            self.camera_preview.xmp.pending_key = None;
            self.camera_preview.xmp.review = None;
            self.camera_preview.xmp.message = Some("Camera source or draft changed while importing; import again. Current edits retained.".into());
            cx.notify();
        }
    }
    fn camera_xmp_current(&mut self, admission: &Admission, cx: &Context<Self>) -> bool {
        self.dialog == Dialog::CameraRaw
            && self.camera_preview.xmp.active == Some(admission.id)
            && self.camera_preview.xmp.pending_key.as_ref() == Some(&admission.key)
            && self.camera_source_key() == admission.key
            && self.read_camera_form(cx).is_ok()
            && self.camera_draft == admission.base
    }
    fn camera_xmp_cancel_review(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.camera_preview.xmp.promotion.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.camera_preview.xmp.active = None;
        self.camera_preview.xmp.pending_key = None;
        self.camera_preview.xmp.review = None;
        self.camera_preview.xmp.message =
            Some("Preset review cancelled; current settings and artwork retained".into());
        cx.notify();
    }
    fn camera_xmp_manual_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.camera_preview.xmp.active.is_some() && self.camera_preview.xmp.review.is_none() {
            self.status = "Wait for or cancel the current preset read".into();
            cx.notify();
            return;
        }
        let path = self
            .camera_preview
            .xmp
            .path
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        if path.is_empty() {
            self.status = "Enter a local .xmp path; nothing was changed".into();
            cx.notify();
            return;
        }
        match self.camera_xmp_admit(cx) {
            Ok(admission) => self.load_camera_xmp(PathBuf::from(path), admission, window, cx),
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }
    /// Hook before generic modal Enter/Escape dispatch. Enter in the path
    /// field imports for review and can never accidentally Apply the grade.
    pub(in crate::ui) fn camera_xmp_key(
        &mut self,
        key: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialog != Dialog::CameraRaw {
            return false;
        }
        let plain = !key.modifiers.control
            && !key.modifiers.shift
            && !key.modifiers.alt
            && !key.modifiers.platform;
        if plain
            && key.key == "enter"
            && self
                .camera_preview
                .xmp
                .path
                .as_ref()
                .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
        {
            self.camera_xmp_manual_path(window, cx);
            return true;
        }
        if plain
            && key.key == "escape"
            && (self.camera_preview.xmp.active.is_some()
                || self.camera_preview.xmp.review.is_some())
        {
            self.camera_xmp_cancel_review(cx);
            return true;
        }
        false
    }
    pub(in crate::ui) fn choose_camera_xmp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let admission = match self.camera_xmp_admit(cx) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import XMP preset".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = choice.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.camera_xmp_current(&admission, cx) {
                    this.camera_xmp_drop_stale_form(&admission, cx);
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) if paths.len() == 1 => {
                        this.load_camera_xmp(paths[0].clone(), admission.clone(), window, cx)
                    }
                    Ok(Err(error)) => {
                        this.camera_preview.xmp.active = None;
                        this.camera_preview.xmp.message = Some(format!(
                            "File chooser unavailable: {error}. Enter the local .xmp path below."
                        ));
                    }
                    _ => {
                        this.camera_preview.xmp.active = None;
                        this.camera_preview.xmp.message = Some(
                            "Preset selection cancelled; current settings and artwork retained"
                                .into(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn load_camera_xmp(
        &mut self,
        path: PathBuf,
        admission: Admission,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.camera_preview.xmp.message =
            Some("Reading preset for review; current settings are unchanged".into());
        let target = admission.target.clone();
        let task = cx.background_executor().spawn(async move {
            omuse::xmp_preset::import(&path, target).map_err(|error| format!("{error:#}"))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if !this.camera_xmp_current(&admission, cx) { this.camera_xmp_drop_stale_form(&admission, cx); return; }
                match result {
                    Ok(plan) => { this.camera_preview.xmp.message = Some(if plan.mapped_count() == 0 { "No supported settings in this preset. Review refused fields; nothing can be applied.".into() } else { format!("{} approximate native settings ready for review", plan.mapped_count()) }); this.camera_preview.xmp.review = Some(Review { admission, plan }); }
                    Err(error) => { this.camera_preview.xmp.active = None; this.camera_preview.xmp.review = None; this.camera_preview.xmp.message = Some(format!("Preset not imported: {error}. Current settings and pixels are unchanged.")); }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn camera_xmp_use(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let review = self
            .camera_preview
            .xmp
            .review
            .clone()
            .context("Import a preset for review first")?;
        ensure!(
            self.camera_xmp_current(&review.admission, cx),
            "Source or draft changed; import again"
        );
        let base: Settings = serde_json::from_value(self.camera_draft.clone())?;
        let target = self.camera_xmp_target(&base)?;
        ensure!(
            self.editor
                .document
                .find_layer(&target.layer_id)
                .is_some_and(|layer| layer.advanced.is_some()),
            "Explicitly promote the original photo before importing a retained preset grade"
        );
        ensure!(
            target.processing == "Native16V1",
            "Byte-compatible output is not qualified for retained preset application. Explicitly promote the original photo, then review again."
        );
        let settings = review.plan.reviewed_settings(&base, &target)?;
        let prepared_form = serde_json::to_value(&settings)?;
        // Admission/budget failure must leave the current form unchanged.
        omuse::xmp_preset::SavedProvenance::new(
            review.plan.report().clone(),
            &prepared_form,
            &settings,
        )?;
        self.camera_draft = prepared_form;
        self.camera_preview.xmp.accepted = Some(Accepted {
            key: self.camera_source_key(),
            report: review.plan.report().clone(),
            settings: self.camera_draft.clone(),
        });
        // Retain the mapping report while the user reviews native Preview/Apply.
        self.camera_preview.xmp.active = None;
        self.camera_preview.xmp.pending_key = None;
        self.camera_preview.xmp.message = Some("Supported approximate settings loaded. Preview them; Apply creates one editable grade. Refused fields stayed unchanged.".into());
        self.load_camera_form(window, cx);
        self.start_camera_raw(settings, true, cx);
        Ok(())
    }
    fn camera_xmp_promote(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let review = self
            .camera_preview
            .xmp
            .review
            .clone()
            .context("Import a preset for review first")?;
        ensure!(
            self.camera_xmp_current(&review.admission, cx),
            "Source or draft changed; import again"
        );
        ensure!(
            self.camera_preview.xmp.promotion.is_none(),
            "Photo promotion is already preparing"
        );
        let layer = self
            .editor
            .document
            .find_layer(&review.admission.target.layer_id)
            .context("Photo target missing")?;
        ensure!(
            layer.advanced.is_none(),
            "Promotion is only for ordinary photo pixels; existing retained recipes are never reinterpreted"
        );
        let source = layer
            .image
            .as_ref()
            .context("Photo has no pixels")?
            .as_arc();
        let name = layer.name.clone();
        let blend_if = omuse::advanced::layer_blend_if(layer)?;
        // Promotion is explicit but must not bypass native/Create contextual
        // admission before allocating the retained 16-bit identity source.
        omuse::camera_raw::native::ui_admission_for_editor(
            &self.editor,
            source.dimensions(),
            false,
            false,
            self.editor
                .selection
                .as_ref()
                .map_or(0, |selection| selection.mask.capacity()) as u64,
            self.create_context_resident_bytes()?
                .saturating_add(2 * omuse::xmp_preset::MAX_INPUT_BYTES as u64),
        )?;
        let target = review.admission.target.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.camera_preview.xmp.promotion = Some(cancel.clone());
        self.camera_preview.xmp.message=Some("Preparing editable original in the background. Cancel keeps the original photo unchanged.".into());
        let work_cancel = cancel.clone();
        let task = cx.background_executor().spawn(async move {
            omuse::xmp_preset::PreparedPhotoPromotion::prepare(
                target,
                source,
                name,
                blend_if,
                &work_cancel,
            )
            .map_err(|error| format!("{error:#}"))
        });
        cx.spawn_in(window,async move|view,cx|{
            let prepared=task.await;
            let _=view.update_in(cx,|this,window,cx|{
                if cancel.load(Ordering::Relaxed) || this.camera_preview.xmp.promotion.as_ref().is_none_or(|current|!Arc::ptr_eq(current,&cancel)) {return;}
                if !this.camera_xmp_current(&review.admission,cx){this.camera_xmp_drop_stale_form(&review.admission,cx);return;}
                this.camera_preview.xmp.promotion=None;
                let result=(||->Result<()>{
                    let prepared=prepared.map_err(anyhow::Error::msg)?;
                    ensure!(this.editor.commit_prepared_photo_promotion(prepared)?, "Photo promotion did not create an editable source");
                    this.cancel_camera_raw();this.changed(cx);this.open_camera_raw(window,cx);
                    let admission=this.camera_xmp_admit(cx)?;
                    let plan=review.plan.for_promoted_target(admission.target.clone())?;
                    this.camera_preview.xmp.review=Some(Review{admission,plan});
                    this.camera_preview.xmp.message=Some("Original photo promoted unchanged in one Undo step. A fresh Native16 report is ready; Use supported settings separately. Pending byte-draft adjustments were not applied.".into());
                    Ok(())
                })();
                if let Err(error)=result {this.camera_preview.xmp.message=Some(format!("Photo not promoted: {error:#}"));}
                cx.notify();
            });
        }).detach();
        cx.notify();
        Ok(())
    }
    /// Capture this value into the immutable camera Request. The engine must
    /// attach it atomically with the prepared native state, never after commit.
    pub(super) fn camera_xmp_provenance(
        &self,
        settings: &Settings,
    ) -> Result<Option<omuse::xmp_preset::SavedProvenance>> {
        let Some(accepted) = &self.camera_preview.xmp.accepted else {
            return Ok(None);
        };
        ensure!(
            accepted.key == self.camera_source_key(),
            "Imported preset source changed; review again"
        );
        let target = self.camera_xmp_target(settings)?;
        ensure!(
            target == accepted.report.target,
            "Imported preset processing/source changed; review again"
        );
        Ok(Some(omuse::xmp_preset::SavedProvenance::new(
            accepted.report.clone(),
            &accepted.settings,
            settings,
        )?))
    }
    pub(in crate::ui) fn camera_xmp_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::inspector_ui::{
            panel_button as button, panel_input, panel_note, panel_section,
        };
        let state = &self.camera_preview.xmp;
        let reading =
            (state.active.is_some() && state.review.is_none()) || state.promotion.is_some();
        let mut body = panel_section("XMP preset · Approximate native settings", cx).child(
            button(
                "camera-xmp-choose",
                "Import XMP preset…",
                ButtonVariant::Outline,
                cx,
            )
            .disabled(self.busy || reading)
            .debug_selector(|| "camera-xmp-choose".into())
            .on_click(cx.listener(|this, _, w, cx| this.choose_camera_xmp(w, cx))),
        );
        if let Some(input) = &state.path {
            body = body
                .child(panel_input("camera-xmp-path", input, window, cx))
                .child(
                    button(
                        "camera-xmp-read-path",
                        "Read this local XMP path",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .disabled(self.busy || reading)
                    .on_click(cx.listener(|this, _, w, cx| {
                        let path = this
                            .camera_preview
                            .xmp
                            .path
                            .as_ref()
                            .unwrap()
                            .read(cx)
                            .value()
                            .trim()
                            .to_owned();
                        if path.is_empty() {
                            this.status = "Enter a local .xmp path; nothing was changed".into();
                            cx.notify();
                            return;
                        }
                        match this.camera_xmp_admit(cx) {
                            Ok(admission) => {
                                this.load_camera_xmp(PathBuf::from(path), admission, w, cx)
                            }
                            Err(error) => {
                                this.status = error.to_string();
                                cx.notify();
                            }
                        }
                    })),
                );
        }
        if let Some(message) = &state.message {
            body = body.child(panel_note(message.clone(), cx));
        }
        if let Some(review) = &state.review {
            let mut rows = div()
                .id("camera-xmp-report")
                .max_h(px(240.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            for row in &review.plan.report().rows {
                let disposition = match row.disposition {
                    omuse::xmp_preset::Disposition::Approximate => "Approximate native setting",
                    omuse::xmp_preset::Disposition::Refused => "Not imported",
                    omuse::xmp_preset::Disposition::RetainedIdentity => "Source identity only",
                };
                rows = rows.child(panel_note(
                    format!(
                        "{} · {}: {} — {}",
                        row.property, disposition, row.input, row.explanation
                    ),
                    cx,
                ));
            }
            let ordinary = self
                .editor
                .document
                .find_layer(&self.editor.active_layer)
                .is_some_and(|layer| layer.advanced.is_none());
            let native = review.admission.target.processing == "Native16V1" && !ordinary;
            body = body
                .child(panel_note(
                    format!(
                        "Source SHA-256: {} · Target {} · {}",
                        review.plan.report().source_sha256,
                        review.admission.target.source_id,
                        review.admission.target.processing
                    ),
                    cx,
                ))
                .child(rows)
                .child(
                    button(
                        "camera-xmp-use",
                        "Use supported settings",
                        ButtonVariant::Primary,
                        cx,
                    )
                    .debug_selector(|| "camera-xmp-use".into())
                    .disabled(
                        self.busy
                            || state.active.is_none()
                            || !native
                            || review.plan.mapped_count() == 0,
                    )
                    .on_click(cx.listener(|this, _, w, cx| {
                        if let Err(error) = this.camera_xmp_use(w, cx) {
                            this.status = error.to_string();
                            cx.notify();
                        }
                    })),
                );
            if !native {
                body=body.child(panel_note("Byte-compatible selection/alpha behavior differs from retained grade replay. Promotion is explicit: original pixels unchanged, one separate Undo step, then a fresh native review. Existing retained byte recipes are never converted.",cx))
                    .child(button("camera-xmp-promote","Promote to editable photo",ButtonVariant::Outline,cx).disabled(self.busy || state.promotion.is_some() || state.active.is_none() || !ordinary || review.plan.mapped_count()==0).on_click(cx.listener(|this,_,w,cx|{if let Err(error)=this.camera_xmp_promote(w,cx){this.status=error.to_string();cx.notify();}})));
            }
        }
        if state.active.is_some() || state.review.is_some() {
            body = body.child(
                button(
                    "camera-xmp-cancel",
                    "Cancel preset review",
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.camera_xmp_cancel_review(cx);
                })),
            );
        }
        body.into_any_element()
    }
}

#[cfg(all(test, feature = "ui-test"))]
#[path = "camera_xmp_ui_tests.rs"]
mod tests;
