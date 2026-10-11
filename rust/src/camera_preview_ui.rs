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

#[derive(Clone)]
struct Request {
    id: u64,
    key: SourceKey,
    settings: Settings,
    sampling: Option<omuse::camera_raw::SampleStage>,
    preview: bool,
    draft_size: Option<(u32, u32)>,
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
    window: Option<gpui_kit::AnyWindowHandle>,
    display_scale: f32,
    cache: Option<Arc<omuse::camera_raw::preview::DraftSource>>,
    draft_visible: bool,
}
impl Drop for CameraPreviewState {
    fn drop(&mut self) {
        if let Some(request) = &self.active {
            request.cancel.store(true, Ordering::Relaxed);
        }
    }
}

enum Output {
    Draft {
        pixels: image::RgbaImage,
        cache: Arc<omuse::camera_raw::preview::DraftSource>,
    },
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
                && layer.vector_scene.is_none()
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
        self.camera_preview.cache = None;
        self.camera_preview.draft_visible = false;
        self.camera_gestures = Default::default();
        self.camera_scopes_generation = self.camera_scopes_generation.wrapping_add(1);
        if self.dialog == Dialog::CameraRaw {
            // Native close can show the Unsaved dialog without a later
            // refresh. A cancelled preview must never remain behind it.
            self.display.replace(&self.pixels);
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
        self.camera_preview.window = Some(window.window_handle());
        self.camera_preview.display_scale = window.scale_factor();
        self.camera_draft = serde_json::to_value(Settings::for_new_edit()).unwrap();
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
        let sampling = preview.then(|| self.camera_sample_stage()).flatten();
        let draft_size = (preview
            && omuse::camera_raw::preview::supported(
                &self.editor.document,
                &self.editor.active_layer,
                self.editor.selection.is_some(),
                &settings,
            ))
        .then(|| {
            omuse::camera_raw::preview::dimensions(
                source.width(),
                source.height(),
                self.zoom * self.camera_preview.display_scale,
            )
        })
        .flatten();
        let request = Arc::new(Request {
            id: self.camera_preview.next_id,
            key: self.camera_source_key(),
            settings,
            sampling,
            preview,
            draft_size,
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
        let cached = self.camera_preview.cache.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(size) = worker.draft_size {
                let cache = match cached.filter(|cache| cache.matches(&worker.source, size)) {
                    Some(cache) => cache,
                    None => Arc::new(omuse::camera_raw::preview::DraftSource::new(
                        worker.source.clone(),
                        size,
                        &worker.cancel,
                    )?),
                };
                let mut pixels = omuse::camera_raw::apply_cancellable(
                    cache.pixels(),
                    &worker.settings,
                    &worker.cancel,
                )?;
                if worker.clip_shadows || worker.clip_highlights {
                    pixels = omuse::camera_raw::clipping_preview(
                        &pixels,
                        worker.clip_shadows,
                        worker.clip_highlights,
                    );
                }
                let mut document = worker.document.clone();
                document.width = size.0;
                document.height = size.1;
                document.layers[0].image = Some(pixels.into());
                ensure!(
                    !worker.cancel.load(Ordering::Relaxed),
                    "Camera Raw cancelled"
                );
                let pixels = raster::composite(&document);
                ensure!(
                    !worker.cancel.load(Ordering::Relaxed),
                    "Camera Raw cancelled"
                );
                return Ok(Output::Draft { pixels, cache });
            }
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
        if !self.camera_preview.active.as_ref().is_some_and(|active| {
            active.id == request.id && active.draft_size == request.draft_size
        }) {
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
                self.camera_preview.draft_visible = false;
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
            self.camera_preview.draft_visible = false;
            self.refresh(cx);
            return;
        }
        match result {
            Ok(Output::Draft { pixels, cache }) => {
                self.display.replace(&pixels);
                self.camera_preview.draft_visible = true;
                self.camera_preview.cache = Some(cache);
                // Scopes and colour pickers wait for the full-size reference.
                self.camera_scopes = None;
                self.camera_scopes_preview = false;
                self.busy = true;
                self.status = "Quick preview · refining full detail…".into();
                self.launch_camera_request(
                    Arc::new(Request {
                        draft_size: None,
                        ..request.as_ref().clone()
                    }),
                    cx,
                );
            }
            Ok(Output::Preview {
                pixels,
                scopes,
                sampled,
            }) => {
                self.camera_preview.draft_visible = false;
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
                    self.camera_preview.cache = None;
                    self.camera_preview.draft_visible = false;
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
                    // Apply completes after confirm_dialog has returned, so its
                    // synchronous focus restoration cannot run for this modal.
                    if let Some(window) = self.camera_preview.window.take() {
                        let view = cx.entity().downgrade();
                        let generation = self.dialog_generation;
                        cx.defer(move |cx| {
                            let _ = cx.update_window(window, |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    if this.dialog == Dialog::None
                                        && this.dialog_generation == generation
                                    {
                                        this.focus.focus(window, cx);
                                    }
                                });
                            });
                        });
                    }
                }
                Err(error) => {
                    if self.camera_preview.draft_visible {
                        self.camera_preview.draft_visible = false;
                        self.refresh(cx);
                    }
                    self.status = error.to_string();
                }
            },
            Err(error) => {
                if self.camera_preview.draft_visible {
                    self.camera_preview.draft_visible = false;
                    self.refresh(cx);
                }
                self.status = format!("Development failed: {error:#}");
            }
        }
        cx.notify();
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

    fn settings(exposure: f32) -> Settings {
        Settings {
            exposure,
            ..Settings::for_new_edit()
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
            assert_eq!(view.camera_draft["toneMapping"], "SmoothV1");
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
    fn camera_curve_apply_restores_editor_focus_for_immediate_keyboard_undo_redo(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        cx.update(|_, cx| {
            install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
        });
        cx.simulate_resize(size(px(1200.), px(900.)));
        let original = view.update_in(cx, |view, window, cx| {
            view.camera_section = crate::camera_controls::SECTIONS
                .iter()
                .position(|section| section.0 == "curve")
                .unwrap();
            view.camera_curve_channel = 0;
            view.load_camera_form(window, cx);
            view.pixels.clone()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let graph = cx.debug_bounds("camera-curve-editor").unwrap();
        let midpoint = point(
            graph.origin.x + graph.size.width * 0.5,
            graph.origin.y + graph.size.height * 0.35,
        );
        cx.simulate_mouse_down(midpoint, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(midpoint, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            let view = view.read(cx);
            let curve = view.camera_draft["curve"]["rgb"].as_array().unwrap();
            assert_eq!(curve.len(), 3);
            assert!(curve[1]["y"].as_f64().unwrap() > 0.6);
            assert_eq!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 0);
            window.draw(cx).clear(cx);
        });
        let apply = cx.debug_bounds("confirm-dialog").unwrap().center();
        cx.simulate_mouse_down(apply, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert!(view.modal_focus.contains_focused(window, cx));
            assert!(!view.focus.is_focused(window));
        });
        cx.simulate_mouse_up(apply, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let developed = cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::None);
            assert!(!view.busy);
            assert!(view.focus.is_focused(window));
            assert_ne!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 1);
            view.pixels.clone()
        });
        // No direct history call, focus assignment or canvas click after Apply.
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.simulate_keystrokes("ctrl-shift-z");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pixels, developed);
            assert_eq!(view.editor.undo_depth(), 1);
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
        view.update_in(cx, |view, window, cx| {
            view.start_camera_raw(settings(1.), true, cx);
            view.start_camera_raw(settings(2.), false, cx);
            view.cancel_camera_raw();
            assert!(view.camera_preview.queued.is_none());
            view.dialog_generation += 1;
            view.dialog = Dialog::RawImport;
            view.busy = true;
            view.status = "Later import".into();
            view.modal_focus.focus(window, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert!(view.busy);
            assert_eq!(view.status, "Later import");
            assert_eq!(view.editor.undo_depth(), 0);
            assert!(view.camera_preview.active.is_none());
            assert!(view.modal_focus.is_focused(window));
            assert!(!view.focus.is_focused(window));
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

    fn large_photo(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
        view.cancel_camera_raw();
        let mut document = Document::new(640, 480);
        document.background = [18, 42, 73, 255];
        document.layers[0].opacity = 0.8;
        document.layers[0].image = Some(
            image::RgbaImage::from_fn(640, 480, |x, y| {
                image::Rgba([
                    (x * 5) as u8,
                    (y * 7) as u8,
                    84,
                    if x % 7 == 0 { 128 } else { 255 },
                ])
            })
            .into(),
        );
        view.editor = Editor::new(document);
        // Exercise a 40% physical-pixel view regardless of test-display DPI.
        view.zoom = 0.4 / window.scale_factor();
        view.refresh(cx);
        view.open_camera_raw(window, cx);
    }

    #[gpui_kit::test]
    fn camera_large_preview_refines_exactly_and_apply_keeps_one_full_size_undo(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        let original = view.update_in(cx, |view, window, cx| {
            large_photo(view, window, cx);
            let original = view.pixels.clone();
            view.start_camera_raw(settings(0.6), true, cx);
            let request = view.camera_preview.active.as_ref().unwrap();
            assert!(request.draft_size.is_some());
            assert_eq!(
                request.sampling,
                Some(omuse::camera_raw::SampleStage::WhiteBalance)
            );
            original
        });
        cx.run_until_parked();
        let (cache, expected) = view.update(cx, |view, cx| {
            assert!(!view.busy && !view.camera_preview.draft_visible);
            assert_eq!(view.display.dimensions(), (640, 480));
            assert_eq!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 0);
            let graded =
                omuse::camera_raw::apply(&view.camera_source().unwrap(), &settings(0.6)).unwrap();
            assert_eq!(
                view.camera_scopes.as_deref(),
                Some(&omuse::photo_scopes::PhotoScopes::analyze(&graded))
            );
            let cache = view.camera_preview.cache.as_ref().unwrap().clone();
            assert_eq!(cache.pixels().dimensions(), (256, 192));
            let mut expected = view.editor.document.clone();
            expected.layers[0].image = Some(graded.into());
            // A second preview shares the sampled source, never the graded result.
            view.start_camera_raw(settings(0.6), true, cx);
            (cache, raster::composite(&expected))
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert!(Arc::ptr_eq(
                &cache,
                view.camera_preview.cache.as_ref().unwrap()
            ));
            view.start_camera_raw(settings(0.6), false, cx);
            assert!(
                view.camera_preview
                    .active
                    .as_ref()
                    .unwrap()
                    .draft_size
                    .is_none()
            );
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.pixels, expected);
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(view.camera_preview.cache.is_none());
            assert!(view.editor.undo());
            view.refresh(cx);
            assert_eq!(view.pixels, original);
        });
    }

    #[gpui_kit::test]
    fn camera_cancel_and_source_change_clear_draft_cache(cx: &mut TestAppContext) {
        let (view, cx, _recovery) = setup(cx);
        view.update_in(cx, |view, window, cx| {
            large_photo(view, window, cx);
            view.start_camera_raw(settings(0.4), true, cx);
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert!(view.camera_preview.cache.is_some());
            view.start_camera_raw(settings(1.0), true, cx);
            view.cancel_camera_raw();
            assert!(view.camera_preview.cache.is_none());
            large_photo(view, window, cx);
            view.start_camera_raw(settings(-0.3), true, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy && !view.camera_preview.draft_visible);
            let cache = view.camera_preview.cache.as_ref().unwrap();
            assert!(cache.matches(&view.camera_source().unwrap(), (256, 192)));
            assert_eq!(view.editor.undo_depth(), 0);
            let expected =
                omuse::camera_raw::apply(&view.camera_source().unwrap(), &settings(-0.3)).unwrap();
            assert_eq!(
                view.camera_scopes.as_deref(),
                Some(&omuse::photo_scopes::PhotoScopes::analyze(&expected))
            );
        });
    }

    #[gpui_kit::test]
    fn camera_failed_refinement_restores_original_view(cx: &mut TestAppContext) {
        let (view, cx, _recovery) = setup(cx);
        view.update(cx, |view, cx| {
            view.start_camera_raw(settings(0.5), true, cx);
            let request = view.camera_preview.active.as_ref().unwrap().clone();
            // A full worker failure following a visible draft must not leave
            // an approximate view presented as the completed preview.
            view.display.replace(&image::RgbaImage::new(4, 3));
            view.camera_preview.draft_visible = true;
            view.finish_camera_request(
                request,
                Err(anyhow::anyhow!("injected refinement failure")),
                cx,
            );
            assert!(!view.busy && !view.camera_preview.draft_visible);
            assert_eq!(view.display.dimensions(), (48, 32));
            assert!(view.status.contains("injected refinement failure"));
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn camera_visible_draft_cancel_restores_committed_display_before_close_prompt(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        view.update_in(cx, |view, window, cx| {
            large_photo(view, window, cx);
            view.start_camera_raw(settings(0.5), true, cx);
            let request = view.camera_preview.active.as_ref().unwrap().clone();
            // Simulate a received first frame while its refinement is pending.
            // No user pixels/history are changed by a transient display frame.
            let cache = Arc::new(
                omuse::camera_raw::preview::DraftSource::new(
                    request.source.clone(),
                    request.draft_size.unwrap(),
                    &AtomicBool::new(false),
                )
                .unwrap(),
            );
            view.display.replace(cache.pixels());
            view.camera_preview.cache = Some(cache);
            view.camera_preview.draft_visible = true;
            assert_ne!(view.display.dimensions(), view.pixels.dimensions());
            view.cancel_camera_raw();
            view.dialog_generation += 1;
            view.dialog = Dialog::Unsaved;
            assert_eq!(view.display.dimensions(), view.pixels.dimensions());
            assert!(view.camera_preview.cache.is_none());
            assert!(!view.camera_preview.draft_visible);
            assert!(request.cancel.load(Ordering::Relaxed));
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert_eq!(view.dialog, Dialog::Unsaved);
            assert_eq!(view.display.dimensions(), (640, 480));
            assert!(view.camera_preview.active.is_none());
        });
    }

    #[gpui_kit::test]
    fn camera_rejects_editable_vector_even_with_a_raster_cache(cx: &mut TestAppContext) {
        let (view, cx, _recovery) = setup(cx);
        view.update(cx, |view, _| {
            let layer = view
                .editor
                .document
                .find_layer_mut(&view.editor.active_layer)
                .unwrap();
            assert!(layer.image.is_some());
            layer.vector_scene = Some(Arc::new(omuse::vector_scene::VectorScene {
                version: 1,
                width: 32,
                height: 24,
                objects: Vec::new(),
            }));
            assert!(
                view.camera_source()
                    .unwrap_err()
                    .to_string()
                    .contains("rasterize a copy")
            );
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }
}
