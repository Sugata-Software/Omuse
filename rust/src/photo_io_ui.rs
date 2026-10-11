//! Bounded background photo I/O. A cancelled worker retains its admission slot
//! until it exits, so repeated Open/Cancel cannot pile up image decoders.
use super::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct PhotoIoJob {
    generation: u64,
    cancel: Arc<AtomicBool>,
    export: Option<Arc<raster::ExportCancellation>>,
    finished: bool,
}

impl Drop for PhotoIoJob {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(export) = &self.export {
            export.cancel();
        }
    }
}

impl EditorView {
    fn photo_status(action: &str, retained_precision: bool) -> String {
        if retained_precision {
            format!(
                "{action}; 16-bit source retained. Use editable filters, or Convert to pixels for 8-bit painting."
            )
        } else {
            action.into()
        }
    }

    fn photo_ready_status(&self, action: &str) -> String {
        Self::photo_status(
            action,
            self.editor
                .document
                .find_layer(&self.editor.active_layer)
                .is_some_and(|layer| layer.advanced.is_some()),
        )
    }

    pub(super) fn prepare_photo(
        path: &Path,
        cancel: &AtomicBool,
    ) -> anyhow::Result<(
        Document,
        Option<omuse::create_project::Project>,
        image::RgbaImage,
        Option<u64>,
    )> {
        let ((doc, project, pixels), stamp) = omuse::save_guard::read_consistent(path, || {
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Open cancelled");
            let (doc, project) = Self::open_content(path)?;
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Open cancelled");
            let pixels = raster::composite(&doc);
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Open cancelled");
            anyhow::ensure!(
                pixels.dimensions() == (doc.width, doc.height),
                "The image could not be rendered"
            );
            Ok((doc, project, pixels))
        })?;
        Ok((doc, project, pixels, stamp))
    }

    pub(super) fn begin_photo_io(&mut self, cx: &mut Context<Self>) -> Option<Arc<AtomicBool>> {
        if self.photo_io.is_some() {
            self.status = "The previous image operation is finishing. Try again shortly.".into();
            cx.notify();
            return None;
        }
        // A preview is advisory only. Once the export has an admitted file
        // operation, an older preview must not replace that operation's status
        // if the export later fails or is cancelled.
        if self.dialog == Dialog::Export {
            self.invalidate_jpeg_preview();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.photo_io = Some(PhotoIoJob {
            generation: self.dialog_generation,
            cancel: cancel.clone(),
            export: None,
            finished: false,
        });
        self.busy = true;
        Some(cancel)
    }

    pub(super) fn cancel_photo_io(&mut self) -> bool {
        if let Some(job) = &self.photo_io
            && job.generation == self.dialog_generation
        {
            let cancelled = job.export.as_ref().is_none_or(|export| export.cancel());
            job.cancel.store(true, Ordering::Relaxed);
            self.busy = false;
            self.status = if cancelled {
                "Image operation cancelled."
            } else {
                "Export finished before cancellation; the file was saved."
            }
            .into();
            return true;
        }
        false
    }

    pub(super) fn finish_photo_io(
        &mut self,
        cancel: &Arc<AtomicBool>,
        identity: (Dialog, u64, u64, u64),
    ) -> bool {
        if self
            .photo_io
            .as_ref()
            .is_none_or(|job| !Arc::ptr_eq(&job.cancel, cancel))
        {
            return false;
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        // The worker has returned. Releasing its admission slot is not a
        // cancellation request; consumers may still check this token before
        // committing prepared pixels. Disposal of an unfinished job cancels.
        self.photo_io.as_mut().unwrap().finished = true;
        self.photo_io = None;
        // A native close request can replace this dialog with Unsaved without
        // advancing its generation. Release only this job's busy state even
        // when its result has become obsolete.
        if self.dialog_generation == identity.1 {
            self.busy = false;
        }
        if (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        ) != identity
        {
            return false;
        }
        self.busy = false;
        !cancelled
    }

    pub(super) fn open_photo_background(
        &mut self,
        path: PathBuf,
        import: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if omuse::svg_import::matches(&path) {
            self.begin_svg_import(path, import, window, cx);
            return;
        }
        self.open_photo_background_sized(path, import, None, window, cx);
    }

    pub(super) fn open_photo_background_sized(
        &mut self,
        path: PathBuf,
        import: bool,
        svg_size: Option<((u32, u32), u64)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        self.status = if import {
            "Importing photograph…"
        } else {
            "Opening photograph…"
        }
        .into();
        cx.spawn_in(window, async move |view, cx| {
            let worker_cancel = cancel.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| -> anyhow::Result<_> {
                        anyhow::ensure!(!worker_cancel.load(Ordering::Relaxed), "Open cancelled");
                        let content = if let Some((size, stamp)) = svg_size {
                            anyhow::ensure!(Self::svg_source_stamp(&path)? == stamp,
                                "The SVG source changed. Reload dimensions to review it again; the current artwork is unchanged.");
                            let layer = omuse::svg_import::import_with_size(&path, Some(size))?;
                            anyhow::ensure!(Self::svg_source_stamp(&path)? == stamp,
                                "The SVG source changed while rendering. Reload dimensions to review it again; the current artwork is unchanged.");
                            anyhow::ensure!(!worker_cancel.load(Ordering::Relaxed), "SVG import cancelled");
                            if import {
                                (None, Some(layer))
                            } else {
                                let image = layer.image.as_ref().expect("SVG import has cached pixels");
                                let pixels = image.to_image();
                                let doc = Document {
                                    width: image.width(), height: image.height(), name: layer.name.clone(),
                                    background: [0; 4], layers: vec![layer], metadata: serde_json::json!({}),
                                };
                                (Some((doc, None, pixels, None)), None)
                            }
                        } else if import {
                            let layer = document::import_image(&path)?;
                            (None, Some(layer))
                        } else {
                            (Some(Self::prepare_photo(&path, &worker_cancel)?), None)
                        };
                        Ok((content, path))
                    })();
                    result.map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_photo_io(&cancel, identity) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(((Some((doc, project, pixels, stamp)), None), path)) => {
                        this.import_notes = omuse::import_report::conversion_notes(&doc);
                        this.install_opened_content(doc, project);
                        this.live_stamp = stamp;
                        this.path = stamp.map(|_| path.components().collect());
                        if let Some(path)=this.path.clone() {this.note_recent(path,cx);}
                        this.selection_box = None;
                        this.pan = (0., 0.);
                        this.paint_mask = false;
                        this.pending_stroke_frame = None;
                        *this.selection_contour.borrow_mut() = SelectionContourCache::default();
                        this.layer_selection
                            .click(this.editor.active_layer.clone(), SelectionAction::Replace);
                        this.pixels = pixels;
                        this.present_pixels(cx);
                        this.recovery.clear();
                        this.status = this.photo_ready_status("Document opened");
                    }
                    Ok(((None, Some(layer)), _)) => {
                        let mut report = Document::new(1, 1);
                        report.layers = vec![layer.clone()];
                        this.import_notes = omuse::import_report::conversion_notes(&report);
                        if !this.editor.import_layer(layer).is_empty() {
                            this.changed(cx);
                            this.status = this.photo_ready_status("Image added as a layer");
                        } else {
                            this.status =
                                "The layer could not be added within the document limits.".into();
                            cx.notify();
                            return;
                        }
                    }
                    Ok(_) => unreachable!("photo worker returns exactly one result kind"),
                    Err(error) => {
                        this.status =
                            format!("{} failed: {error}", if import { "Import" } else { "Open" });
                        cx.notify();
                        return;
                    }
                }
                this.svg_import_draft = None;
                this.dialog = if this.import_notes.is_empty() {
                    Dialog::None
                } else {
                    Dialog::ImportReport
                };
                if !this.import_notes.is_empty() {
                    this.status = format!(
                        "{} with {} conversion note(s)",
                        this.status,
                        this.import_notes.len()
                    );
                }
                this.focus.focus(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn export_photo_background(
        &mut self,
        document: Document,
        path: PathBuf,
        options: raster::ExportOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("psd"))
        {
            self.export_psd_background(document, path, window, cx);
            return;
        }
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let export_cancel = Arc::new(raster::ExportCancellation::new());
        self.photo_io.as_mut().unwrap().export = Some(export_cancel.clone());
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        self.status = "Exporting photograph…".into();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    raster::export_with_options_cancellable(
                        &document,
                        &path,
                        options,
                        &export_cancel,
                    )
                    .map(|()| path)
                    .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_photo_io(&cancel, identity) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(path) => {
                        this.status = format!("Exported {}", path.display());
                        this.dialog = Dialog::None;
                        this.focus.focus(window, cx);
                    }
                    Err(error) => this.status = format!("Export failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn export_psd_background(
        &mut self,
        document: Document,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let instance = self.editor.instance_id();
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        self.status =
            "Preparing converted layered PSD… Original .omuse artwork stays editable".into();
        cx.spawn_in(window, async move |view, cx| {
            let task_cancel = cancel.clone();
            let result =
                cx.background_executor()
                    .spawn(async move {
                        omuse::psd_export::prepare_export(&path, &document, &task_cancel)
                    })
                    .await;
            let published = view
                .update_in(cx, |this, window, cx| {
                    if !this.finish_photo_io(&cancel, identity) {
                        if !cancel.load(Ordering::Relaxed)
                            && this.dialog == identity.0
                            && this.dialog_generation == identity.1
                        {
                            this.status =
                                "Document changed; PSD export discarded before publication".into();
                        }
                        cx.notify();
                        return None;
                    }
                    if this.editor.instance_id() != instance {
                        this.status =
                            "Document changed; PSD export discarded before publication".into();
                        cx.notify();
                        return None;
                    }
                    match result.and_then(|export| Ok((export.prepared.publish()?, export.report)))
                    {
                        Ok((published, report)) => {
                            // Publication happens on this UI turn after the exact snapshot guard.
                            // Cancellation after this point must not claim the file was removed.
                            let status = format!(
                                "Exported {} · {} pixel layers · Converted 8-bit · Ctrl+K → Export conversion report",
                                published.destination().display(),
                                report.layer_count,
                            );
                            this.export_notes = vec![format!("File: {}", published.destination().display())];
                            this.export_notes.extend(report.warnings);
                            this.status = status.clone();
                            this.dialog = Dialog::None;
                            this.focus.focus(window, cx);
                            cx.notify();
                            Some((published, status))
                        }
                        Err(error) => {
                            this.status = format!("PSD export failed: {error:#}");
                            cx.notify();
                            None
                        }
                    }
                })
                .ok()
                .flatten();
            if let Some((published, status)) = published {
                let result = cx
                    .background_executor()
                    .spawn(async move { published.finish() })
                    .await;
                if let Err(error) = result {
                    let _ = view.update_in(cx, |this, _, cx| {
                        if this.status == status {
                            this.status = format!(
                                "PSD file created, but durability could not be confirmed: {error:#}"
                            );
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(super) fn import_photos_background(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        if paths.len() > 16 || self.editor.floating_selection_layer().is_some() {
            self.status = if paths.len() > 16 {
                "Drop up to 16 images at a time."
            } else {
                "Commit or cancel the floating selection before importing images."
            }
            .into();
            cx.notify();
            return;
        }
        if paths.len() == 1 && omuse::svg_import::matches(&paths[0]) {
            self.begin_svg_import(paths[0].clone(), true, window, cx);
            return;
        }
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        let original = self.editor.document.clone();
        let count = paths.len();
        self.status = format!("Importing {count} image(s)… Escape to cancel");
        cx.spawn_in(window, async move |view, cx| {
            let worker_cancel = cancel.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| -> anyhow::Result<_> {
                        let mut staged = Editor::new(original);
                        staged.set_history_limit(0);
                        let mut retained_precision = false;
                        for path in paths {
                            anyhow::ensure!(
                                !worker_cancel.load(Ordering::Relaxed),
                                "Import cancelled"
                            );
                            let layer = document::import_image(&path)?;
                            let layer_retained_precision = layer.advanced.is_some();
                            anyhow::ensure!(
                                !worker_cancel.load(Ordering::Relaxed),
                                "Import cancelled"
                            );
                            anyhow::ensure!(
                                !staged.import_layer(layer).is_empty(),
                                "The images could not be added within the document limits"
                            );
                            retained_precision |= layer_retained_precision;
                        }
                        Ok((staged.document, staged.active_layer, retained_precision))
                    })();
                    result.map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_photo_io(&cancel, identity) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok((doc, active_layer, retained_precision)) => {
                        let notes = omuse::import_report::conversion_notes(&doc);
                        if let Err(error) = this.editor.replace_document_transaction(doc) {
                            this.status = format!("Import failed: {error:#}");
                        } else {
                            this.editor.active_layer = active_layer;
                            this.import_notes = notes;
                            this.status = Self::photo_status(
                                &format!("Added {count} image(s) — Undo removes this import"),
                                retained_precision,
                            );
                            this.changed(cx);
                            if !this.import_notes.is_empty() {
                                this.dialog = Dialog::ImportReport;
                                this.modal_focus.focus(window, cx);
                            }
                        }
                    }
                    Err(error) => this.status = format!("Import failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn background_photo_open_retains_precision_and_explains_pixel_conversion(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("master.png");
        let master = image::ImageBuffer::from_fn(7, 5, |x, y| {
            image::Rgba([1001 + x as u16 * 101, 23007 + y as u16 * 37, 45011, 65535])
        });
        image::DynamicImage::ImageRgba16(master.clone())
            .save(&path)
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.dialog = Dialog::Open;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(path, false, window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.path.is_none());
            assert_eq!(view.dialog, Dialog::None, "{}", view.status);
            let state = view.editor.document.layers[0].advanced.as_ref().unwrap();
            assert_eq!(state.source.to_rgba16(), master);
            assert!(view.status.contains("16-bit source retained"));
            assert!(view.status.contains("Convert to pixels for 8-bit painting"));
        });
    }

    #[gpui_kit::test]
    fn photo_open_is_deferred_and_a_failed_open_preserves_the_editor(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing.png");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.editor.fill_selection([21, 54, 87, 255]);
            view.dialog = Dialog::Open;
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(missing, false, window, cx);
            assert!(view.busy);
            assert!(view.editor.is_dirty());
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.photo_io.is_none());
            assert!(view.editor.is_dirty());
            assert_eq!(view.pixels.dimensions(), (12, 9));
            assert_eq!(view.pixels.get_pixel(0, 0).0, [21, 54, 87, 255]);
            assert!(view.status.starts_with("Open failed:"));
        });
    }

    #[gpui_kit::test]
    fn cancelled_photo_open_cannot_replace_artwork_or_admit_another_decoder(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.png");
        image::RgbaImage::from_pixel(7, 5, image::Rgba([31, 72, 139, 255]))
            .save(&path)
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.dialog = Dialog::Open;
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(path.clone(), false, window, cx);
            let first = view.photo_io.as_ref().unwrap().cancel.clone();
            assert!(view.cancel_photo_io());
            view.dialog_generation += 1;
            view.open_photo_background(path.clone(), false, window, cx);
            assert!(Arc::ptr_eq(&view.photo_io.as_ref().unwrap().cancel, &first));
            assert!(!view.busy);
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert!(view.photo_io.is_none());
            assert_eq!(view.pixels.dimensions(), (12, 9));
            view.open_photo_background(path, false, window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert_eq!(view.dialog, Dialog::None, "{}", view.status);
            assert_eq!(view.pixels.dimensions(), (7, 5));
            assert_eq!(view.pixels.get_pixel(0, 0).0, [31, 72, 139, 255]);
            assert!(
                view.path.is_none(),
                "Saving an imported photo must not target the original image"
            );
        });
    }

    #[gpui_kit::test]
    fn background_photo_import_retains_pixels_and_one_step_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.png");
        let photo = image::RgbaImage::from_pixel(7, 5, image::Rgba([31, 72, 139, 255]));
        photo.save(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.dialog = Dialog::Import;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(path, true, window, cx)
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert_eq!(view.dialog, Dialog::None, "{}", view.status);
            assert_eq!(view.editor.document.layers.len(), 2);
            assert_eq!(
                &**view.editor.document.layers[1].image.as_ref().unwrap(),
                &photo
            );
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(view.editor.undo());
            assert_eq!(view.editor.document.layers.len(), 1);
            assert!(
                raster::composite(&view.editor.document)
                    .pixels()
                    .all(|p| p[3] == 0)
            );
            assert!(view.editor.redo());
            assert_eq!(
                &**view.editor.document.layers[1].image.as_ref().unwrap(),
                &photo
            );
        });
    }

    #[gpui_kit::test]
    fn cancelled_photo_export_keeps_the_existing_destination(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("keep.png");
        std::fs::write(&path, b"Existing export").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.editor.fill_selection([21, 54, 87, 255]);
            view.dialog = Dialog::Export;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.export_photo_background(
                view.editor.document.clone(),
                path.clone(),
                Default::default(),
                window,
                cx,
            );
            assert!(view.busy);
            assert!(view.cancel_photo_io());
            view.dialog = Dialog::None;
            view.dialog_generation += 1;
        });
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), b"Existing export");
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.photo_io.is_none());
            assert!(view.editor.is_dirty());
            assert_eq!(view.dialog, Dialog::None);
        });
    }

    #[gpui_kit::test]
    fn jpeg_preview_input_invalidation_cannot_stale_an_active_export(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.dialog = Dialog::Export;
            view.dialog_generation = 41;
            view
        });
        view.update(cx, |view, cx| {
            view.jpeg_preview_task = Some(7);
            view.jpeg_preview_generation = 7;
            let identity = (
                view.dialog,
                view.dialog_generation,
                view.create.epoch,
                view.editor.revision(),
            );
            let cancel = view.begin_photo_io(cx).expect("export admission");
            let preview_generation = view.jpeg_preview_generation;
            // This is the same invalidation path an Export option input uses
            // after an export has been submitted.
            view.invalidate_jpeg_preview();

            assert_eq!(view.dialog_generation, identity.1);
            assert_ne!(view.jpeg_preview_generation, preview_generation);
            assert_eq!(
                view.jpeg_preview_task,
                Some(7),
                "retain worker admission until it finishes"
            );
            assert!(view.cancel_photo_io());
            assert!(!view.finish_photo_io(&cancel, identity));
            assert!(view.photo_io.is_none());
            assert!(!view.busy);
            assert!(cancel.load(Ordering::Relaxed));

            let completion_cancel = view.begin_photo_io(cx).expect("second export admission");
            assert!(view.finish_photo_io(&completion_cancel, identity));
            assert!(view.photo_io.is_none());
            assert!(!view.busy);
            assert!(
                !completion_cancel.load(Ordering::Relaxed),
                "completion is not cancellation"
            );
            let unfinished = view.begin_photo_io(cx).unwrap();
            view.photo_io = None;
            assert!(
                unfinished.load(Ordering::Relaxed),
                "disposing unfinished work cancels it"
            );
        });
    }

    #[gpui_kit::test]
    fn late_jpeg_preview_cannot_populate_a_reopened_export_dialog(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.save_dialog(true, window, cx);
            view
        });
        // Drain the initial form's change events before submitting the preview.
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            view.start_jpeg_preview(cx);
            assert!(view.jpeg_preview_task.is_some());
            view.dialog_generation = view.dialog_generation.wrapping_add(1);
            view.dialog = Dialog::Export;
            view.status = "A new export dialog".into();
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(view.jpeg_preview_task.is_none());
            assert!(view.jpeg_preview.is_none());
            assert_eq!(view.status, "A new export dialog");
        });
    }

    #[gpui_kit::test]
    fn jpeg_preview_refuses_to_start_while_photo_io_is_active(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.dialog = Dialog::Export;
            view
        });
        view.update(cx, |view, cx| {
            let _cancel = view.begin_photo_io(cx).expect("export admission");
            view.start_jpeg_preview(cx);
            assert!(view.jpeg_preview_task.is_none());
            assert_eq!(
                view.status,
                "An image operation is already running; JPEG preview is unavailable."
            );
        });
    }

    #[gpui_kit::test]
    fn dropped_photos_commit_together_and_undo_in_one_step(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for (index, color) in [[18, 54, 93, 255], [194, 61, 28, 255]]
            .into_iter()
            .enumerate()
        {
            let path = temp.path().join(format!("photo-{index}.png"));
            image::RgbaImage::from_pixel(7, 5, image::Rgba(color))
                .save(&path)
                .unwrap();
            paths.push(path);
        }
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.import_photos_background(paths, window, cx);
            assert!(view.busy);
            assert_eq!(view.editor.document.layers.len(), 1);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert_eq!(view.editor.document.layers.len(), 3, "{}", view.status);
            assert_eq!(view.pixels.get_pixel(0, 0).0, [194, 61, 28, 255]);
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(view.editor.undo());
            assert_eq!(view.editor.document.layers.len(), 1);
            assert!(view.editor.redo());
            assert_eq!(
                raster::composite(&view.editor.document).get_pixel(0, 0).0,
                [194, 61, 28, 255]
            );
        });
    }

    #[gpui_kit::test]
    fn mixed_precision_drop_reports_any_retained_source(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let master_path = temp.path().join("master-16.png");
        let master = image::ImageBuffer::from_fn(7, 5, |x, y| {
            image::Rgba([1001 + x as u16 * 101, 23007 + y as u16 * 37, 45011, 65535])
        });
        image::DynamicImage::ImageRgba16(master)
            .save(&master_path)
            .unwrap();
        let photo_path = temp.path().join("photo-8.png");
        image::RgbaImage::from_pixel(7, 5, image::Rgba([18, 54, 93, 255]))
            .save(&photo_path)
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.import_photos_background(vec![master_path, photo_path], window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert_eq!(view.editor.document.layers.len(), 3, "{}", view.status);
            assert!(
                view.editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .is_some_and(|layer| layer.advanced.is_none()),
                "the final 8-bit drop should be the active layer"
            );
            assert_eq!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .filter(|layer| layer.advanced.is_some())
                    .count(),
                1
            );
            assert!(view.status.contains("Added 2 image(s)"), "{}", view.status);
            assert!(
                view.status.contains("16-bit source retained"),
                "{}",
                view.status
            );
        });
    }

    #[gpui_kit::test]
    fn a_failed_or_cancelled_drop_never_keeps_a_partial_import(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let photo = temp.path().join("photo.png");
        image::RgbaImage::from_pixel(7, 5, image::Rgba([18, 54, 93, 255]))
            .save(&photo)
            .unwrap();
        let missing = temp.path().join("missing.png");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.import_photos_background(vec![photo.clone(), missing], window, cx);
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert!(view.status.starts_with("Import failed:"), "{}", view.status);
            assert!(!view.busy);
            assert_eq!(view.editor.document.layers.len(), 1);
            assert_eq!(view.editor.undo_depth(), 0);
            assert!(!view.editor.is_dirty());
            view.import_photos_background(vec![photo], window, cx);
            assert!(view.cancel_photo_io());
            view.dialog_generation += 1;
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.photo_io.is_none());
            assert_eq!(view.editor.document.layers.len(), 1);
            assert_eq!(view.editor.undo_depth(), 0);
            assert!(!view.editor.is_dirty());
        });
    }

    #[gpui_kit::test]
    fn obsolete_import_completion_leaves_the_unsaved_dialog_usable(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let photo = temp.path().join("photo.png");
        image::RgbaImage::from_pixel(7, 5, image::Rgba([18, 54, 93, 255]))
            .save(&photo)
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(12, 9));
            view.editor.fill_selection([42, 78, 131, 255]);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.import_photos_background(vec![photo], window, cx);
            view.dialog = Dialog::Unsaved;
            view.pending = Some(Pending::Quit);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.photo_io.is_none());
            assert_eq!(view.dialog, Dialog::Unsaved);
            assert_eq!(view.editor.document.layers.len(), 1);
            assert!(view.editor.is_dirty());
            assert_eq!(
                raster::composite(&view.editor.document).get_pixel(0, 0).0,
                [42, 78, 131, 255]
            );
        });
    }
}
