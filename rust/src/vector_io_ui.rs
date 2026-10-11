//! Explicit editable-path interchange; ordinary SVG image import stays separate.
use super::*;
use anyhow::{Context as _, Result, ensure};
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
            object.id = uuid::Uuid::new_v4().to_string();
            for group in &mut object.groups {
                group.id = groups
                    .entry(group.id.clone())
                    .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone();
            }
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
        let first = artwork.objects.len();
        artwork.version = artwork.version.max(imported.version);
        artwork.objects.extend(imported.objects);
        let count = artwork.objects.len() - first;
        let selected = (first..artwork.objects.len()).collect();
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
                self.status = format!("SVG export: {error:#}");
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
            let result = match chosen {
                Ok(Ok(Some(paths))) if current && paths.len() == 1 => {
                    let path = paths[0].clone();
                    let cancel = guard.cancel.clone();
                    Some(
                        cx.background_executor()
                            .spawn(async move {
                                ensure!(!cancel.load(Ordering::Relaxed), "SVG import cancelled");
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
                            EditableSvg::Path(path) => this.import_vector_artwork(path, window, cx),
                            EditableSvg::Scene(report) => {
                                this.import_vector_scene_artwork(report.scene, window, cx)?;
                                if !report.warnings.is_empty() {
                                    this.status =
                                        format!("SVG imported · {}", report.warnings.join(" · "));
                                }
                                Ok(())
                            }
                        }) {
                            this.status = format!("Editable SVG import: {error:#}");
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
                self.status = format!("{format} export: {error:#}");
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
            let result = match chosen {
                Ok(Ok(Some(mut path))) if current => {
                    if path.extension().is_none() {
                        path.set_extension(if pdf { "pdf" } else { "svg" });
                    }
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
                        if same_draft {
                            this.vector_exchange_finished(window, cx);
                        }
                        published
                    }
                    Some(Err(error)) => {
                        if guard.finish(this, cx) {
                            this.status = format!("{format} export: {error:#}");
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
                    Ok(_) => format!("Exported {label}: {}", destination.display()),
                    Err(error) => format!(
                        "Exported {label}: {}; directory sync warning: {error:#}",
                        destination.display()
                    ),
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
}
