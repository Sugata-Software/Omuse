//! Explicit editable-path interchange; ordinary SVG image import stays separate.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::compatibility_report::CompatibilityReport;
use omuse::vector_svg::{self, SvgArtwork};
use omuse::{vector_scene::VectorScene, vector_svg_scene};

enum EditableSvg {
    Path(SvgArtwork),
    Scene(vector_svg_scene::ImportedScene),
}

pub(super) struct Guard {
    cancel: Arc<AtomicBool>,
    generation: u64,
    revision: u64,
}

impl Guard {
    fn current(&self, view: &EditorView) -> bool {
        self.same_draft(view)
            && view.vector_draft.as_ref().is_some_and(|draft| {
                draft.identity == (view.editor.instance_id(), view.create.epoch)
            })
            && view.editor.revision() == self.revision
            && !self.cancel.load(Ordering::Relaxed)
    }

    fn same_draft(&self, view: &EditorView) -> bool {
        (view.dialog == Dialog::VectorPath
            || (view.dialog == Dialog::None && view.vector_scene_current()))
            && view.dialog_generation == self.generation
            && view
                .vector_draft
                .as_ref()
                .is_some_and(|draft| Arc::ptr_eq(&draft.cancel, &self.cancel))
    }

    fn finish(&self, view: &mut EditorView, cx: &mut Context<EditorView>) -> bool {
        if !self.same_draft(view) {
            return false;
        }
        view.busy = false;
        if view.editor.revision() != self.revision || self.cancel.load(Ordering::Relaxed) {
            view.status =
                "Document changed during SVG exchange. Close and reopen the path editor.".into();
            cx.notify();
            return false;
        }
        true
    }
}

/// Fit the SVG viewport, not its drawn bounds, so spacing and holes survive.
fn fit_artwork(mut artwork: SvgArtwork, dimensions: (u32, u32)) -> Result<SvgArtwork> {
    ensure!(
        artwork.width > 0 && artwork.height > 0,
        "SVG has no viewport"
    );
    let scale = (dimensions.0 as f64 / artwork.width as f64)
        .min(dimensions.1 as f64 / artwork.height as f64);
    let dx = (dimensions.0 as f64 - artwork.width as f64 * scale) / 2.;
    let dy = (dimensions.1 as f64 - artwork.height as f64 * scale) / 2.;
    for subpath in &mut artwork.path.subpaths {
        for anchor in &mut subpath.anchors {
            for point in [
                Some(&mut anchor.position),
                anchor.incoming.as_mut(),
                anchor.outgoing.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                point.x = (point.x as f64 * scale + dx) as f32;
                point.y = (point.y as f64 * scale + dy) as f32;
            }
        }
    }
    if let Some(stroke) = &mut artwork.stroke {
        stroke.width = (stroke.width as f64 * scale) as f32;
        ensure!(
            stroke.width.is_finite() && stroke.width > 0. && stroke.width <= 4096.,
            "Fitted stroke is too small or exceeds 4096 px; adjust it in the source SVG"
        );
    }
    artwork.path.validate()?;
    artwork.width = dimensions.0;
    artwork.height = dimensions.1;
    Ok(artwork)
}

impl EditorView {
    pub(in crate::ui) fn show_compatibility_report(
        &mut self,
        export: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !export {
            self.import_notes = self
                .last_import_compatibility
                .as_ref()
                .filter(|(instance, _)| *instance == self.editor.instance_id())
                .map(|(_, report)| report.lines())
                .unwrap_or_else(|| omuse::import_report::conversion_notes(&self.editor.document));
        }
        if (if export {
            &self.export_notes
        } else {
            &self.import_notes
        })
        .is_empty()
        {
            self.status = if export {
                "No export compatibility report in this session"
            } else {
                "No import compatibility report for this document"
            }
            .into();
        } else {
            self.dialog = if export {
                Dialog::ExportReport
            } else {
                Dialog::ImportReport
            };
            self.modal_focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_compatibility_report(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialog = if self
            .vector_draft
            .as_ref()
            .is_some_and(|draft| !draft.is_scene())
        {
            Dialog::VectorPath
        } else {
            Dialog::None
        };
        if self.dialog == Dialog::VectorPath {
            self.modal_focus.focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn retain_vector_import_report(&mut self, report: CompatibilityReport, imported: bool) {
        self.import_notes = report.lines();
        self.last_import_compatibility = Some((self.editor.instance_id(), report.clone()));
        if imported && let Some(draft) = self.vector_draft.as_mut() {
            draft.import_compatibility = Some(report);
        }
    }

    pub(in crate::ui) fn vector_report_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                button(
                    "vector-import-report",
                    "Import report",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(
                    self.busy
                        || (self.import_notes.is_empty()
                            && !omuse::import_report::has_conversion_notes(&self.editor.document)),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_compatibility_report(false, window, cx)
                })),
            )
            .child(
                button(
                    "vector-export-report",
                    "Export report",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy || self.export_notes.is_empty())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_compatibility_report(true, window, cx)
                })),
            )
            .into_any_element()
    }

    pub(super) fn import_vector_scene_artwork(
        &mut self,
        mut imported: VectorScene,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        imported.validate()?;
        let draft = self
            .vector_draft
            .as_ref()
            .context("Vector artwork closed")?;
        let mut artwork = draft.scene_snapshot()?;
        let scale = (artwork.width as f32 / imported.width as f32)
            .min(artwork.height as f32 / imported.height as f32);
        let dx = (artwork.width as f32 - imported.width as f32 * scale) / 2.;
        let dy = (artwork.height as f32 - imported.height as f32 * scale) / 2.;
        let scope = draft
            .scene
            .as_ref()
            .map(|scene| scene.isolation.clone())
            .unwrap_or_default();
        let prefix = draft
            .scene
            .as_ref()
            .and_then(|scene| artwork.objects.get(scene.active))
            .map(|object| {
                object
                    .groups
                    .iter()
                    .take(scope.len())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let ids: std::collections::HashMap<_, _> = imported
            .objects
            .iter()
            .map(|object| (object.id.clone(), uuid::Uuid::new_v4().to_string()))
            .collect();
        let mut groups = std::collections::HashMap::new();
        for object in &mut imported.objects {
            let old = object.transform;
            object.transform = [
                old[0] * scale,
                old[1] * scale,
                old[2] * scale,
                old[3] * scale,
                old[4] * scale + dx,
                old[5] * scale + dy,
            ];
            object.id = ids[&object.id].clone();
            object.clip_to = object.clip_to.as_ref().map(|id| ids[id].clone());
            for group in &mut object.groups {
                group.id = groups
                    .entry(group.id.clone())
                    .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone();
            }
            object.groups.splice(0..0, prefix.iter().cloned());
        }
        if artwork.objects.len() == 1
            && artwork.objects[0]
                .path
                .subpaths
                .iter()
                .all(|s| s.anchors.is_empty())
        {
            artwork.objects.clear();
        }
        let first = if scope.is_empty() {
            artwork.objects.len()
        } else {
            artwork
                .objects
                .iter()
                .rposition(|object| super::vector_isolation_ui::in_scope(&scope, object))
                .context("The isolated group no longer exists")?
                + 1
        };
        artwork.version = artwork.version.max(imported.version);
        if !prefix.is_empty() {
            artwork.version = artwork.version.max(2);
        }
        let count = imported.objects.len();
        artwork.objects.splice(first..first, imported.objects);
        let selected = (first..first + count).collect();
        self.replace_scene_artwork(artwork, selected, window, cx)?;
        if let Some(scene) = self
            .vector_draft
            .as_mut()
            .and_then(|draft| draft.scene.as_mut())
        {
            scene.mode = super::scene::SceneMode::Select;
        }
        self.status = format!(
            "Imported {count} editable SVG objects · Existing artwork retained · Ctrl+Z to undo"
        );
        Ok(())
    }

    fn vector_exchange_finished(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vector_scene_active() {
            // A preview completion may have been suppressed while the native
            // chooser or SVG worker owned the global busy flag.
            self.start_scene_preview(cx);
            self.vector_resume_after_exchange(window, cx);
            self.focus.focus(window, cx);
        } else {
            self.modal_focus.focus(window, cx);
        }
    }

    pub(super) fn vector_io_guard(&mut self, cx: &mut Context<Self>) -> Option<Guard> {
        let draft = self.vector_draft.as_ref()?;
        if self.busy {
            return None;
        }
        if draft.revision != self.editor.revision() {
            self.status =
                "Document changed. Close and reopen the path editor before exchanging SVG.".into();
            cx.notify();
            return None;
        }
        Some(Guard {
            cancel: draft.cancel.clone(),
            generation: self.dialog_generation,
            revision: draft.revision,
        })
    }

    pub(super) fn import_vector_artwork(
        &mut self,
        artwork: SvgArtwork,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let draft = self.vector_draft.as_ref().context("Path editor closed")?;
        let artwork = fit_artwork(artwork, draft.dimensions)?;
        let as_mask = draft.as_mask;
        let fill = artwork.fill.unwrap_or([0, 0, 0, 0]);
        let stroke = artwork.stroke;
        self.scene_checkpoint();
        let draft = self.vector_draft.as_mut().unwrap();
        draft.path = artwork.path;
        draft.selected = None;
        draft.drag = None;
        if !as_mask {
            draft.fill = fill;
            draft.stroke = stroke;
            let hex = |c: [u8; 4]| format!("#{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3]);
            for (input, value) in self.detail_inputs.iter().zip([
                hex(fill),
                hex(stroke.map_or([0, 0, 0, 255], |stroke| stroke.color)),
                stroke.map_or(0., |stroke| stroke.width).to_string(),
            ]) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        self.status = if self.vector_scene_active() {
            "SVG path fitted to this layer · Done to keep · Cancel to discard"
        } else {
            "SVG path fitted to this layer. Review it, then Apply or Cancel."
        }
        .into();
        self.vector_scene_changed(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn vector_artwork(&mut self, cx: &mut Context<Self>) -> Result<SvgArtwork> {
        self.update_vector_style(cx)?;
        let draft = self.vector_draft.as_ref().context("Path editor closed")?;
        if let Some(scene) = &draft.scene {
            ensure!(
                scene.artwork.objects[scene.active].opacity == 1.,
                "Single-path SVG export requires 100% object opacity. Save .omuse to retain the complete artwork and opacity."
            );
        }
        let (width, height) = draft.dimensions;
        let artwork = SvgArtwork {
            width,
            height,
            path: draft.path.clone(),
            fill: if draft.as_mask {
                Some([0, 0, 0, 255])
            } else {
                Some(draft.fill)
            },
            stroke: if draft.as_mask { None } else { draft.stroke },
        };
        // Validate before showing the destination chooser.
        vector_svg::encode(&artwork)?;
        Ok(artwork)
    }

    /// Recheck the exact draft immediately before the single publication
    /// syscall. Preparation is side-effect-free at the chosen destination.
    pub(super) fn publish_prepared_vector_export(
        &mut self,
        guard: &Guard,
        prepared: vector_svg::PreparedExport,
        cx: &mut Context<Self>,
    ) -> Option<vector_svg::PublishedExport> {
        if !guard.current(self) {
            guard.finish(self, cx);
            return None;
        }
        let destination = prepared.destination().to_path_buf();
        match prepared.publish() {
            Ok(published) => {
                // Publication is committed at this point. Keep the draft busy
                // until directory sync and staged-file cleanup finish, while
                // making Cancel accurately describe the already-created file.
                self.vector_export_published();
                let label = if destination
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                {
                    "vector PDF"
                } else {
                    "editable SVG"
                };
                self.status = format!("Exported {label}: {}", destination.display());
                cx.notify();
                Some(published)
            }
            Err(error) => {
                self.busy = false;
                self.status = format!("Vector export: {error:#}");
                cx.notify();
                None
            }
        }
    }

    pub(super) fn choose_vector_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(guard) = self.vector_io_guard(cx) else {
            return;
        };
        let scene = self.vector_scene_active();
        let task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if scene {
                    "Import editable SVG artwork"
                } else {
                    "Import editable SVG path"
                }
                .into(),
            ),
        });
        self.busy = true;
        self.modal_focus.focus(window, cx);
        self.status = if scene {
            "Choose SVG artwork to add as editable objects."
        } else {
            "Choose a solid-colour SVG path to fit into this layer."
        }
        .into();
        cx.spawn_in(window, async move |view, cx| {
            let chosen = task.await;
            let current = view
                .update_in(cx, |this, _, _| guard.current(this))
                .unwrap_or(false);
            let mut selected_path = None;
            let result = match chosen {
                Ok(Ok(Some(paths))) if current && paths.len() == 1 => {
                    let path = paths[0].clone();
                    selected_path = Some(path.clone());
                    let cancel = guard.cancel.clone();
                    Some(
                        cx.background_executor()
                            .spawn(async move {
                                ensure!(!cancel.load(Ordering::Relaxed), "SVG import cancelled");
                                ensure!(path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("svg")),
                                    "Editable vector import supports .svg files. Export AI/PDF/EPS artwork to SVG first, or use Image Trace for a raster image.");
                                let artwork = if scene {
                                    EditableSvg::Scene(vector_svg_scene::import_scene_with_report(
                                        &path,
                                    )?)
                                } else {
                                    EditableSvg::Path(vector_svg::import(&path)?)
                                };
                                ensure!(!cancel.load(Ordering::Relaxed), "SVG import cancelled");
                                Ok(artwork)
                            })
                            .await,
                    )
                }
                Ok(Err(error)) => Some(Err(error)),
                _ => None,
            };
            let _ = view.update_in(cx, |this, window, cx| {
                if !guard.finish(this, cx) {
                    return;
                }
                match result {
                    Some(result) => {
                        if let Err(error) = result.and_then(|artwork| match artwork {
                            EditableSvg::Path(path) => {
                                let mut report = CompatibilityReport::svg_import(selected_path.as_deref().unwrap_or(std::path::Path::new("Selected SVG")), None, &[]);
                                if this.vector_draft.as_ref().is_some_and(|draft| draft.as_mask) {
                                    report.preserved = vec!["Editable path geometry for the vector mask.".into()];
                                    report.converted.push("Only SVG path geometry is used for this mask; source fill and stroke paint are not used.".into());
                                }
                                this.import_vector_artwork(path, window, cx)?;
                                this.retain_vector_import_report(report, true);
                                this.status.push_str(" · Import report available");
                                Ok(())
                            },
                            EditableSvg::Scene(report) => {
                                let compatibility = CompatibilityReport::svg_import(selected_path.as_deref().unwrap_or(std::path::Path::new("Selected SVG")), Some(&report.scene), &report.warnings);
                                this.import_vector_scene_artwork(report.scene, window, cx)?;
                                this.retain_vector_import_report(compatibility, true);
                                this.status.push_str(&format!(" · {} conversion notice(s) · Import report available", report.warnings.len()));
                                Ok(())
                            }
                        }) {
                            this.retain_vector_import_report(CompatibilityReport::rejected("Editable SVG import blocked", selected_path.as_deref().unwrap_or(std::path::Path::new("Selected SVG")), &format!("{error:#}")), false);
                            this.status = format!("Editable SVG import: {error:#} · Import report available");
                        }
                    }
                    None => this.status = "SVG import cancelled; artwork is unchanged.".into(),
                }
                this.vector_exchange_finished(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn choose_vector_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_vector_export_format(false, window, cx);
    }

    pub(super) fn choose_vector_pdf_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_vector_export_format(true, window, cx);
    }

    fn choose_vector_export_format(
        &mut self,
        pdf: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let format = if pdf { "PDF" } else { "SVG" };
        let label = if pdf { "vector PDF" } else { "editable SVG" };
        let dpi = self
            .editor
            .document
            .metadata
            .get("resolution")
            .and_then(|v| v.as_f64())
            .unwrap_or(72.) as f32;
        let Some(guard) = self.vector_io_guard(cx) else {
            return;
        };
        let artwork = match (|| -> Result<EditableSvg> {
            if self.vector_scene_active() {
                self.update_vector_style(cx)?;
                let scene = self.vector_draft.as_ref().unwrap().scene_snapshot()?;
                vector_svg_scene::encode_scene(&scene)?;
                Ok(EditableSvg::Scene(vector_svg_scene::ImportedScene {
                    scene,
                    warnings: Vec::new(),
                }))
            } else {
                Ok(EditableSvg::Path(self.vector_artwork(cx)?))
            }
        })() {
            Ok(artwork) => artwork,
            Err(error) => {
                self.export_notes = CompatibilityReport::rejected(
                    &format!("{format} export blocked"),
                    std::path::Path::new("No destination selected"),
                    &format!("{error:#}"),
                )
                .lines();
                self.status = format!("{format} export: {error:#} · Export report available");
                cx.notify();
                return;
            }
        };
        let directory = omuse::identity::media_dir(
            omuse::identity::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")),
            omuse::identity::MediaFolder::Pictures,
        );
        let task = cx.prompt_for_new_path(
            &directory,
            Some(if pdf {
                "Omuse artwork.pdf"
            } else if self.vector_scene_active() {
                "Omuse artwork.svg"
            } else {
                "Omuse path.svg"
            }),
        );
        self.busy = true;
        self.modal_focus.focus(window, cx);
        self.status = format!("Choose a new {format} filename for this vector artwork layer.");
        cx.spawn_in(window, async move |view, cx| {
            let chosen = task.await;
            let current = view
                .update_in(cx, |this, _, _| guard.current(this))
                .unwrap_or(false);
            let mut export_report = None;
            let result = match chosen {
                Ok(Ok(Some(mut path))) if current => {
                    if path.extension().is_none() {
                        path.set_extension(if pdf { "pdf" } else { "svg" });
                    }
                    export_report = Some(CompatibilityReport::vector_export(&path, match &artwork { EditableSvg::Scene(scene) => Some(&scene.scene), EditableSvg::Path(_) => None }, pdf));
                    let cancel = guard.cancel.clone();
                    Some(
                        cx.background_executor()
                            .spawn(async move {
                                ensure!(
                                    !cancel.load(Ordering::Relaxed),
                                    "{format} export cancelled"
                                );
                                ensure!(
                                    path.extension()
                                        .and_then(|extension| extension.to_str())
                                        .is_some_and(|extension| extension
                                            .eq_ignore_ascii_case(if pdf { "pdf" } else { "svg" })),
                                    "Choose a filename ending in .{}",
                                    if pdf { "pdf" } else { "svg" }
                                );
                                let prepared = match &artwork {
                                    EditableSvg::Path(artwork) => {
                                        if pdf {
                                            omuse::vector_pdf::prepare_path_export(
                                                &path, artwork, dpi,
                                            )?
                                        } else {
                                            vector_svg::prepare_export(&path, artwork)?
                                        }
                                    }
                                    EditableSvg::Scene(scene) => {
                                        if pdf {
                                            omuse::vector_pdf::prepare_scene_export(
                                                &path,
                                                &scene.scene,
                                                dpi,
                                            )?
                                        } else {
                                            vector_svg_scene::prepare_scene_export(
                                                &path,
                                                &scene.scene,
                                            )?
                                        }
                                    }
                                };
                                ensure!(
                                    !cancel.load(Ordering::Relaxed),
                                    "{format} export cancelled"
                                );
                                Ok(prepared)
                            })
                            .await,
                    )
                }
                Ok(Err(error)) => Some(Err(error)),
                _ => None,
            };
            let published = view
                .update_in(cx, |this, window, cx| match result {
                    Some(Ok(prepared)) => {
                        let same_draft = guard.same_draft(this);
                        let published = this.publish_prepared_vector_export(&guard, prepared, cx);
                        if same_draft && let Some(report) = &export_report {
                            this.export_notes = if published.is_some() { report.lines() } else { CompatibilityReport::rejected(&format!("{format} export blocked"), std::path::Path::new(&report.file), &this.status).lines() };
                        }
                        if same_draft {
                            this.vector_exchange_finished(window, cx);
                        }
                        published
                    }
                    Some(Err(error)) => {
                        if guard.finish(this, cx) {
                            this.export_notes = CompatibilityReport::rejected(&format!("{format} export blocked"), std::path::Path::new(export_report.as_ref().map_or("Selected destination", |report| report.file.as_str())), &format!("{error:#}")).lines();
                            this.status = format!("{format} export: {error:#} · Export report available");
                            this.vector_exchange_finished(window, cx);
                            cx.notify();
                        }
                        None
                    }
                    None => {
                        if guard.finish(this, cx) {
                            this.status =
                                format!("{format} export cancelled; the document is unchanged.");
                            this.vector_exchange_finished(window, cx);
                            cx.notify();
                        }
                        None
                    }
                })
                .ok()
                .flatten();
            let Some(published) = published else {
                return;
            };
            let destination = published.destination().to_path_buf();
            let finished = cx
                .background_executor()
                .spawn(async move { published.finish() })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                // A canceled/reopened draft has already cleared its own busy
                // flag. Never let this old finalizer overwrite the new draft.
                if !guard.same_draft(this) {
                    return;
                }
                this.busy = false;
                this.status = match finished {
                    Ok(_) => format!("Exported {label}: {} · Export report available", destination.display()),
                    Err(error) => {
                        this.export_notes.push(format!("Warning: File published, but directory sync or staging cleanup failed: {error:#}"));
                        format!("Exported {label}: {}; directory sync warning: {error:#}", destination.display())
                    },
                };
                this.vector_exchange_finished(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_preserves_viewport_aspect_ratio_and_scales_handles_and_stroke() {
        let artwork = SvgArtwork {
            width: 100,
            height: 50,
            path: VectorPath {
                subpaths: vec![Subpath {
                    closed: false,
                    anchors: vec![Anchor {
                        position: VectorPoint { x: 20., y: 10. },
                        incoming: Some(VectorPoint { x: 10., y: 5. }),
                        outgoing: None,
                    }],
                }],
                fill_rule: Default::default(),
            },
            fill: None,
            stroke: Some(StrokeStyle {
                color: [12, 34, 56, 78],
                width: 4.,
            }),
        };
        let fitted = fit_artwork(artwork, (200, 200)).unwrap();
        let anchor = &fitted.path.subpaths[0].anchors[0];
        assert_eq!((anchor.position.x, anchor.position.y), (40., 70.));
        assert_eq!(
            (anchor.incoming.unwrap().x, anchor.incoming.unwrap().y),
            (20., 60.)
        );
        assert_eq!(fitted.stroke.unwrap().width, 8.);
        assert_eq!(fitted.stroke.unwrap().color, [12, 34, 56, 78]);
        assert!(fitted.fill.is_none());
        let mut tiny_stroke = fitted;
        tiny_stroke.stroke.as_mut().unwrap().width = f32::from_bits(1);
        assert!(fit_artwork(tiny_stroke, (1, 1)).is_err());
    }

    #[cfg(feature = "ui-test")]
    #[gpui_kit::test]
    fn vector_report_opens_and_closes_without_finishing_or_cancelling_artwork(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(24, 24));
            view.open_vector_scene(window, cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            let draft = view.vector_draft.as_ref().unwrap();
            let token = draft.cancel.clone();
            let generation = view.dialog_generation;
            let report = CompatibilityReport::svg_import(
                std::path::Path::new("logo.svg"),
                None,
                &["Text converted to outlines".into()],
            );
            view.retain_vector_import_report(report, true);
            view.show_compatibility_report(false, window, cx);
            assert_eq!(view.dialog, Dialog::ImportReport);
            assert!(
                view.import_notes
                    .iter()
                    .any(|line| line.contains("Text converted"))
            );
            view.close_compatibility_report(window, cx);
            assert_eq!(view.dialog, Dialog::None);
            assert!(view.vector_scene_current());
            assert_eq!(view.dialog_generation, generation);
            assert!(!token.load(Ordering::Relaxed));
            assert!(Arc::ptr_eq(
                &token,
                &view.vector_draft.as_ref().unwrap().cancel
            ));
            view.show_compatibility_report(false, window, cx);
            assert!(
                view.import_notes
                    .iter()
                    .any(|line| line.contains("logo.svg"))
            );
        });
    }

    #[cfg(feature = "ui-test")]
    #[gpui_kit::test]
    fn previous_document_import_report_cannot_leak_into_another_document(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| EditorView::new(None, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.retain_vector_import_report(
                CompatibilityReport::rejected(
                    "SVG import blocked",
                    std::path::Path::new("private-old.svg"),
                    "Unsupported clip",
                ),
                false,
            );
            view.editor = Editor::new(Document::new(16, 16));
            view.show_compatibility_report(false, window, cx);
            assert!(view.import_notes.is_empty());
            assert_ne!(view.dialog, Dialog::ImportReport);
        });
    }

    #[cfg(feature = "ui-test")]
    #[gpui_kit::test]
    fn clipped_svg_import_remaps_mask_ids_and_stays_inside_isolated_group(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let source = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><g id="group"><path d="M1 1L10 1L10 10Z" fill="#f00"/></g><path d="M12 12L20 12L20 20Z" fill="#00f"/></svg>"##;
        let initial = vector_svg_scene::decode_scene(source).unwrap();
        let (clipped, _) = omuse::vector_clip::make(&initial, &[0, 1]).unwrap();
        let original_ids: Vec<_> = clipped.objects.iter().map(|o| o.id.clone()).collect();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(24, 24));
            view.open_vector_scene(window, cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.import_vector_scene_artwork(initial, window, cx)
                .unwrap();
            assert!(view.enter_vector_isolation(0, window, cx));
            let scope = view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .isolation
                .clone();
            view.import_vector_scene_artwork(clipped, window, cx)
                .unwrap();
            let draft = view.vector_draft.as_ref().unwrap();
            let scene = draft.scene.as_ref().unwrap();
            assert_eq!(scene.isolation, scope);
            assert_eq!(scene.selected_objects.len(), 2);
            let artwork = draft.scene_snapshot().unwrap();
            artwork.validate().unwrap();
            for index in &scene.selected_objects {
                let object = &artwork.objects[*index];
                assert!(super::super::vector_isolation_ui::in_scope(&scope, object));
                assert!(!original_ids.contains(&object.id));
                if let Some(mask) = &object.clip_to {
                    assert!(
                        artwork
                            .objects
                            .iter()
                            .any(|object| object.id == *mask && object.is_clip)
                    );
                }
            }
            assert!(artwork.objects.last().unwrap().groups.is_empty());
        });
    }

    #[cfg(feature = "ui-test")]
    #[gpui_kit::test]
    fn undone_svg_import_cannot_attach_abandoned_report_to_unrelated_artwork(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let source = vector_svg_scene::decode_scene(br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="M1 1L10 1L10 10Z" fill="#f00"/></svg>"##).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(24, 24));
            view.open_vector_scene(window, cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            let report = CompatibilityReport::svg_import(
                std::path::Path::new("abandoned.svg"),
                Some(&source),
                &[],
            );
            view.import_vector_scene_artwork(source, window, cx)
                .unwrap();
            view.retain_vector_import_report(report, true);
            view.command("undo", window, cx);
            assert!(
                view.vector_draft
                    .as_ref()
                    .unwrap()
                    .import_compatibility
                    .is_none()
            );
            view.scene_action("rectangle", window, cx);
            view.apply_vector_scene(cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.vector_scene.is_some()),
                "{}",
                view.status
            );
            assert!(
                !omuse::import_report::conversion_notes(&view.editor.document)
                    .iter()
                    .any(|note| note.contains("abandoned.svg"))
            );
        });
    }
}
