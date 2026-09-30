//! Camera Raw owns one worker and at most one replacement request. All results
//! belong to an exact editor, dialog, layer, selection and document generation.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::camera_raw::Settings;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceKey {
    editor: u64,
    page: u64,
    document: u64,
    selection: u64,
    dialog: u64,
    layer: String,
}

struct Request {
    id: u64,
    key: SourceKey,
    settings: Settings,
    sampling: Option<omuse::camera_raw::SampleStage>,
    preview: bool,
    source: Arc<image::RgbaImage>,
    selection: Option<Selection>,
    document: Document,
    clip_shadows: bool,
    clip_highlights: bool,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct CameraPreviewState {
    next_id: u64,
    active: Option<Arc<Request>>,
    queued: Option<Arc<Request>>,
}
impl Drop for CameraPreviewState {
    fn drop(&mut self) {
        if let Some(request) = &self.active {
            request.cancel.store(true, Ordering::Relaxed);
        }
    }
}

enum Output {
    Preview {
        pixels: image::RgbaImage,
        scopes: Arc<omuse::photo_scopes::PhotoScopes>,
        sampled: Option<image::RgbaImage>,
    },
    Apply(image::RgbaImage),
}

fn unlocked(layers: &[Layer], id: &str, parent_locked: bool) -> bool {
    for layer in layers {
        let locked = parent_locked || layer.locked;
        if layer.id == id {
            return !locked;
        }
        if unlocked(&layer.children, id, locked) {
            return true;
        }
    }
    false
}

impl EditorView {
    fn camera_source_key(&self) -> SourceKey {
        SourceKey {
            editor: self.editor.instance_id(),
            page: self.create.epoch,
            document: self.editor.revision(),
            selection: self.editor.selection_revision(),
            dialog: self.dialog_generation,
            layer: self.editor.active_layer.clone(),
        }
    }

    pub(super) fn camera_source(&self) -> Result<Arc<image::RgbaImage>> {
        ensure!(
            !self.paint_mask,
            "Switch to layer pixels before using Camera Raw"
        );
        ensure!(
            self.editor.floating_selection_layer().is_none(),
            "Commit or cancel the floating selection first"
        );
        let layer = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .context("Select a pixel layer for Camera Raw")?;
        ensure!(
            unlocked(&self.editor.document.layers, &layer.id, false),
            "Unlock this layer and its parents first"
        );
        ensure!(
            layer.advanced.is_none()
                && !layer.is_group()
                && !layer.metadata.get("text").is_some_and(|v| !v.is_null())
                && !layer.metadata.get("shape").is_some_and(|v| !v.is_null()),
            "Camera Raw requires an 8-bit paint layer; rasterize a copy to preserve this editable source"
        );
        let image = layer
            .image
            .as_ref()
            .context("Select a pixel layer for Camera Raw")?;
        ensure!(
            u64::from(image.width()) * u64::from(image.height()) <= 16_777_216
                && u64::from(self.editor.document.width) * u64::from(self.editor.document.height)
                    <= 16_777_216,
            "Camera Raw supports layers and canvases up to 16 megapixels"
        );
        Ok(image.as_arc())
    }

    fn camera_request_is_current(&self, request: &Request) -> bool {
        self.dialog == Dialog::CameraRaw
            && self.camera_source_key() == request.key
            && self.editor.selection == request.selection
            && self.camera_source().is_ok_and(|source| {
                Arc::ptr_eq(&source, &request.source) || *source == *request.source
            })
    }

    pub(super) fn cancel_camera_raw(&mut self) {
        if let Some(request) = &self.camera_preview.active {
            request.cancel.store(true, Ordering::Relaxed);
        }
        self.camera_preview.queued = None;
        self.camera_gestures = Default::default();
        self.camera_scopes_generation = self.camera_scopes_generation.wrapping_add(1);
        if self.dialog == Dialog::CameraRaw {
            self.busy = false;
        }
    }

    pub(super) fn open_camera_raw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_interaction(cx);
        let source = match self.camera_source() {
            Ok(source) => source,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.cancel_camera_raw();
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.dialog = Dialog::CameraRaw;
        self.camera_draft = serde_json::to_value(Settings::default()).unwrap();
        self.camera_section = 0;
        self.camera_clip_shadows = false;
        self.camera_clip_highlights = false;
        let _ = self
            .camera_canvas
            .update(cx, |canvas, cx| canvas.set_source(source.clone(), cx));
        self.load_camera_form(window, cx);
        self.init_camera_gestures();
        self.start_camera_scopes(source, cx);
        self.status = "Preview changes before applying them to the selected layer".into();
        cx.notify();
    }

    pub(super) fn start_camera_scopes(
        &mut self,
        source: Arc<image::RgbaImage>,
        cx: &mut Context<Self>,
    ) {
        self.camera_scopes = None;
        self.camera_scopes_preview = false;
        self.camera_scopes_generation = self.camera_scopes_generation.wrapping_add(1);
        let generation = self.camera_scopes_generation;
        let key = self.camera_source_key();
        let task = cx
            .background_executor()
            .spawn(async move { Arc::new(omuse::photo_scopes::PhotoScopes::analyze(&source)) });
        cx.spawn(async move |view, cx| {
            let scopes = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog == Dialog::CameraRaw
                    && this.camera_source_key() == key
                    && this.camera_scopes_generation == generation
                {
                    this.camera_scopes = Some(scopes);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn start_camera_raw(
        &mut self,
        settings: Settings,
        preview: bool,
        cx: &mut Context<Self>,
    ) {
        if self.dialog != Dialog::CameraRaw || (self.busy && self.camera_preview.active.is_none()) {
            return;
        }
        // Apply owns the final result once admitted. Late preview signals cannot
        // replace it or clear its busy/status indicator.
        if self
            .camera_preview
            .active
            .iter()
            .chain(self.camera_preview.queued.iter())
            .any(|request| {
                !request.preview
                    && !request.cancel.load(Ordering::Relaxed)
                    && request.key.dialog == self.dialog_generation
            })
        {
            return;
        }
        let source = match omuse::camera_raw::validate(&settings).and_then(|_| self.camera_source())
        {
            Ok(source) => source,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.camera_scopes_generation = self.camera_scopes_generation.wrapping_add(1);
        self.camera_preview.next_id = self.camera_preview.next_id.wrapping_add(1);
        let request = Arc::new(Request {
            id: self.camera_preview.next_id,
            key: self.camera_source_key(),
            settings,
            sampling: preview.then(|| self.camera_sample_stage()).flatten(),
            preview,
            source,
            selection: self.editor.selection.clone(),
            document: self.editor.document.clone(),
            clip_shadows: self.camera_clip_shadows,
            clip_highlights: self.camera_clip_highlights,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.busy = true;
        self.status = if preview {
            "Developing preview…"
        } else {
            "Applying Camera Raw…"
        }
        .into();
        if let Some(active) = &self.camera_preview.active {
            active.cancel.store(true, Ordering::Relaxed);
            self.camera_preview.queued = Some(request);
        } else {
            self.launch_camera_request(request, cx);
        }
        cx.notify();
    }

    fn launch_camera_request(&mut self, request: Arc<Request>, cx: &mut Context<Self>) {
        self.camera_preview.active = Some(request.clone());
        let worker = request.clone();
        let task = cx.background_executor().spawn(async move {
            let (image, sampled) = if let Some(stage) = worker.sampling {
                let (image, sampled) = omuse::camera_raw::apply_with_sample(
                    &worker.source,
                    &worker.settings,
                    &worker.cancel,
                    stage,
                )?;
                (image, Some(sampled))
            } else {
                (
                    omuse::camera_raw::apply_cancellable(
                        &worker.source,
                        &worker.settings,
                        &worker.cancel,
                    )?,
                    None,
                )
            };
            ensure!(
                !worker.cancel.load(Ordering::Relaxed),
                "Camera Raw cancelled"
            );
            if !worker.preview {
                return Ok(Output::Apply(image));
            }
            let mut editor = Editor::new(worker.document.clone());
            editor.active_layer = worker.key.layer.clone();
            editor.selection = worker.selection.clone();
            editor.apply_image_operation(|_| Ok(image))?;
            ensure!(
                !worker.cancel.load(Ordering::Relaxed),
                "Camera Raw cancelled"
            );
            let layer = editor
                .document
                .find_layer_mut(&worker.key.layer)
                .context("Preview layer is unavailable")?;
            let image = layer
                .image
                .as_ref()
                .context("Preview pixels are unavailable")?;
            let scopes = Arc::new(omuse::photo_scopes::PhotoScopes::analyze(image));
            if worker.clip_shadows || worker.clip_highlights {
                layer.image = Some(
                    omuse::camera_raw::clipping_preview(
                        image,
                        worker.clip_shadows,
                        worker.clip_highlights,
                    )
                    .into(),
                );
            }
            ensure!(
                !worker.cancel.load(Ordering::Relaxed),
                "Camera Raw cancelled"
            );
            let pixels = raster::composite(&editor.document);
            ensure!(
                !worker.cancel.load(Ordering::Relaxed),
                "Camera Raw cancelled"
            );
            Ok::<_, anyhow::Error>(Output::Preview {
                pixels,
                scopes,
                sampled,
            })
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.finish_camera_request(request, result, cx)
            });
        })
        .detach();
    }

    fn finish_camera_request(
        &mut self,
        request: Arc<Request>,
        result: Result<Output>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .camera_preview
            .active
            .as_ref()
            .is_some_and(|active| active.id == request.id)
        {
            return;
        }
        self.camera_preview.active = None;
        if let Some(next) = self.camera_preview.queued.take() {
            if self.camera_request_is_current(&next) {
                self.launch_camera_request(next, cx);
                return;
            }
            if self.dialog == Dialog::CameraRaw && self.dialog_generation == next.key.dialog {
                self.busy = false;
                self.status =
                    "The layer or selection changed. Preview again before applying.".into();
                self.refresh(cx);
            }
            return;
        }
        // An old worker must not alter a new dialog's display, busy state or status.
        if self.dialog != Dialog::CameraRaw || self.dialog_generation != request.key.dialog {
            return;
        }
        self.busy = false;
        if request.cancel.load(Ordering::Relaxed) || !self.camera_request_is_current(&request) {
            self.status = "The layer or selection changed. Preview again before applying.".into();
            self.refresh(cx);
            return;
        }
        match result {
            Ok(Output::Preview {
                pixels,
                scopes,
                sampled,
            }) => {
                self.display.replace(&pixels);
                self.camera_scopes = Some(scopes);
                self.camera_scopes_preview = true;
                if let (Some(stage), Some(sampled)) = (request.sampling, sampled) {
                    self.accept_camera_sample(
                        request.settings.clone(),
                        stage,
                        Arc::new(sampled),
                        cx,
                    );
                }
                self.status = self
                    .camera_gesture_notice()
                    .unwrap_or_else(|| "Preview — original pixels are unchanged".into());
            }
            Ok(Output::Apply(image)) => match self.editor.apply_image_operation(|_| Ok(image)) {
                Ok(changed) => {
                    self.dialog = Dialog::None;
                    self.camera_gestures = Default::default();
                    self.dialog_generation = self.dialog_generation.wrapping_add(1);
                    self.status = if changed {
                        "Camera Raw applied"
                    } else {
                        "Camera Raw unchanged"
                    }
                    .into();
                    self.changed(cx);
                }
                Err(error) => self.status = error.to_string(),
            },
            Err(error) => self.status = format!("Development failed: {error:#}"),
        }
        cx.notify();
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, VisualTestContext};

    fn settings(exposure: f32) -> Settings {
        Settings {
            exposure,
            ..Default::default()
        }
    }

    fn setup(
        cx: &mut TestAppContext,
    ) -> (
        Entity<EditorView>,
        &mut VisualTestContext,
        tempfile::TempDir,
    ) {
        cx.update(crate::init_test_theme);
        let recovery = tempfile::tempdir().unwrap();
        let path = recovery.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.recovery = Recovery::at(path);
            let mut document = Document::new(48, 32);
            document.layers[0].image = Some(
                image::RgbaImage::from_fn(32, 24, |x, y| {
                    image::Rgba([
                        (x * 5) as u8,
                        (y * 7) as u8,
                        84,
                        if x % 7 == 0 { 128 } else { 255 },
                    ])
                })
                .into(),
            );
            document.layers[0].offset_x = 6.;
            document.layers[0].offset_y = 4.;
            view.editor = Editor::new(document);
            view.refresh(cx);
            view.open_camera_raw(window, cx);
            view
        });
        (view, cx, recovery)
    }

    #[gpui_kit::test]
    fn camera_requests_coalesce_to_one_newest_preview(cx: &mut TestAppContext) {
        let (view, cx, _recovery) = setup(cx);
        let original = view.update(cx, |view, cx| {
            let original = view.pixels.clone();
            view.start_camera_raw(settings(0.5), true, cx);
            let active = view.camera_preview.active.as_ref().unwrap().id;
            view.start_camera_raw(settings(1.), true, cx);
            view.start_camera_raw(settings(1.5), true, cx);
            assert_eq!(view.camera_preview.active.as_ref().unwrap().id, active);
            assert!(
                view.camera_preview
                    .active
                    .as_ref()
                    .unwrap()
                    .cancel
                    .load(Ordering::Relaxed)
            );
            assert_eq!(
                view.camera_preview.queued.as_ref().unwrap().settings,
                settings(1.5)
            );
            original
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            let expected =
                omuse::camera_raw::apply(&view.camera_source().unwrap(), &settings(1.5)).unwrap();
            assert_eq!(
                view.camera_scopes.as_deref(),
                Some(&omuse::photo_scopes::PhotoScopes::analyze(&expected))
            );
            assert!(view.camera_preview.active.is_none() && view.camera_preview.queued.is_none());
            assert!(view.camera_scopes_preview);
            assert!(!view.busy);
            assert_eq!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }

    #[gpui_kit::test]
    fn camera_apply_cannot_be_replaced_by_preview_and_is_exact_one_step_undo(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        let (original, expected) = view.update(cx, |view, cx| {
            view.editor.selection = Some(Selection {
                width: 48,
                height: 32,
                mask: (0..48 * 32)
                    .map(|i| {
                        if i % 48 < 16 {
                            0
                        } else if i % 48 < 32 {
                            128
                        } else {
                            255
                        }
                    })
                    .collect(),
            });
            let original = view.pixels.clone();
            let mut expected = Editor::new(view.editor.document.clone());
            expected.active_layer = view.editor.active_layer.clone();
            expected.selection = view.editor.selection.clone();
            expected
                .apply_image_operation(|source| omuse::camera_raw::apply(source, &settings(1.)))
                .unwrap();
            view.start_camera_raw(settings(0.5), true, cx);
            view.start_camera_raw(settings(1.), false, cx);
            let apply_id = view.camera_preview.queued.as_ref().unwrap().id;
            view.start_camera_raw(settings(2.), true, cx);
            assert_eq!(view.camera_preview.queued.as_ref().unwrap().id, apply_id);
            assert!(!view.camera_preview.queued.as_ref().unwrap().preview);
            assert!(view.busy && view.status.contains("Applying"));
            (original, raster::composite(&expected.document))
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.dialog, Dialog::None);
            assert!(!view.busy);
            assert_eq!(view.pixels, expected);
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(view.editor.undo());
            view.refresh(cx);
            assert_eq!(view.pixels, original);
        });
    }

    #[gpui_kit::test]
    fn camera_apply_rejects_selection_revision_and_editor_replacement(cx: &mut TestAppContext) {
        let (view, cx, _recovery) = setup(cx);
        for change in 0..3 {
            view.update_in(cx, |view, window, cx| {
                if view.dialog != Dialog::CameraRaw {
                    view.open_camera_raw(window, cx);
                }
                view.start_camera_raw(settings(1.), false, cx);
                match change {
                    0 => view.editor.select_all(),
                    1 => {
                        view.editor
                            .rename_layer(&view.editor.active_layer.clone(), "Changed");
                    }
                    _ => {
                        view.editor = Editor::new(view.editor.document.clone());
                    }
                }
            });
            let (original, depth) = cx.update(|_, cx| {
                let view = view.read(cx);
                (
                    raster::composite(&view.editor.document),
                    view.editor.undo_depth(),
                )
            });
            cx.run_until_parked();
            view.update(cx, |view, _| {
                assert_eq!(view.pixels, original);
                assert_eq!(view.editor.undo_depth(), depth);
                assert_eq!(view.dialog, Dialog::CameraRaw);
                assert!(view.status.contains("changed"), "{}", view.status);
                assert!(!view.busy);
            });
        }
    }

    #[gpui_kit::test]
    fn camera_cancel_discards_queued_apply_and_cannot_clear_later_job_status(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        view.update(cx, |view, cx| {
            view.start_camera_raw(settings(1.), true, cx);
            view.start_camera_raw(settings(2.), false, cx);
            view.cancel_camera_raw();
            assert!(view.camera_preview.queued.is_none());
            view.dialog_generation += 1;
            view.dialog = Dialog::RawImport;
            view.busy = true;
            view.status = "Later import".into();
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.busy);
            assert_eq!(view.status, "Later import");
            assert_eq!(view.editor.undo_depth(), 0);
            assert!(view.camera_preview.active.is_none());
        });
    }

    #[gpui_kit::test]
    fn camera_reopen_waits_for_cancelled_worker_and_invalid_settings_do_not_launch(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        view.update_in(cx, |view, window, cx| {
            view.start_camera_raw(settings(1.), true, cx);
            let old_id = view.camera_preview.active.as_ref().unwrap().id;
            view.cancel_camera_raw();
            view.open_camera_raw(window, cx);
            view.start_camera_raw(settings(f32::NAN), true, cx);
            assert!(view.camera_preview.queued.is_none());
            assert!(!view.busy);
            view.start_camera_raw(settings(0.25), true, cx);
            assert_eq!(view.camera_preview.active.as_ref().unwrap().id, old_id);
            assert!(view.camera_preview.queued.is_some());
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.camera_scopes_preview && !view.busy);
            let expected =
                omuse::camera_raw::apply(&view.camera_source().unwrap(), &settings(0.25)).unwrap();
            assert_eq!(
                view.camera_scopes.as_deref(),
                Some(&omuse::photo_scopes::PhotoScopes::analyze(&expected))
            );
        });
    }
}
