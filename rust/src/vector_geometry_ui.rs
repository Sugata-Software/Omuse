//! Background path finishing in the same disposable canvas draft.
use super::*;
use anyhow::{Context as _, Result, ensure};
use std::collections::BTreeSet;

pub(super) struct SimplifyUi {
    pub(super) preserve_corners: bool,
    pub(super) show_original: bool,
    pub(super) session: Option<SimplifySession>,
}

impl Default for SimplifyUi {
    fn default() -> Self {
        Self {
            preserve_corners: true,
            show_original: true,
            session: None,
        }
    }
}

pub(super) struct SimplifySession {
    pub(super) source: omuse::vector_scene::VectorScene,
    pub(super) expected: omuse::vector_scene::VectorScene,
    pub(super) selected: BTreeSet<usize>,
    pub(super) before: usize,
    pub(super) after: usize,
}

impl SimplifyUi {
    pub(super) fn current<'a>(&'a self, draft: &VectorDraft) -> Option<&'a SimplifySession> {
        let session = self.session.as_ref()?;
        let scene = draft.scene.as_ref()?;
        (scene.artwork == session.expected
            && scene.selected_objects == session.selected
            && scene
                .artwork
                .objects
                .get(scene.active)
                .is_some_and(|object| object.path == draft.path))
        .then_some(session)
    }
}

impl EditorView {
    pub(super) fn scene_geometry_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let prepared = (|| -> Result<_> {
            self.update_vector_style(cx)?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            ensure!(
                !scene.selected_objects.is_empty(),
                "Select at least one path"
            );
            ensure!(
                scene
                    .selected_objects
                    .iter()
                    .all(|i| *i < scene.artwork.objects.len()),
                "Object selection changed; select paths again"
            );
            ensure!(
                scene.selected_objects.len() <= 64,
                "Finish at most 64 paths at a time"
            );
            let value = match action {
                "vector-simplify" => self.detail_inputs[18].read(cx).value().parse::<f32>()?,
                "vector-offset" => self.detail_inputs[19].read(cx).value().parse::<f32>()?,
                _ => 0.,
            };
            ensure!(value.is_finite(), "Use a finite distance in pixels");
            let current = draft.scene_snapshot()?;
            let source = if action == "vector-simplify" {
                scene
                    .simplify
                    .current(draft)
                    .map_or_else(|| current.clone(), |s| s.source.clone())
            } else {
                current.clone()
            };
            ensure!(
                source
                    .retained_bytes()
                    .saturating_add(current.retained_bytes())
                    <= 32 * 1024 * 1024,
                "Path review exceeds its 32 MiB snapshot budget; choose smaller artwork"
            );
            Ok((
                source,
                current,
                scene.selected_objects.clone(),
                draft.cancel.clone(),
                value,
                scene.simplify.preserve_corners,
            ))
        })();
        let (source, current, selected, token, value, preserve_corners) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let action = action.to_owned();
        let original = current;
        let simplify_source = (action == "vector-simplify").then(|| source.clone());
        let original_selection = selected.clone();
        let task_token = token.clone();
        let task = cx.background_executor().spawn(async move {
            use omuse::vector_commands::{self, VectorCommand};
            let ids = selected
                .iter()
                .map(|i| source.objects[*i].id.clone())
                .collect::<Vec<_>>();
            let command = match action.as_str() {
                "vector-simplify" => VectorCommand::Simplify { tolerance: value },
                "vector-offset" => VectorCommand::Offset { distance: value },
                "vector-outline-stroke" => VectorCommand::OutlineStroke,
                _ => anyhow::bail!("Unknown path operation"),
            };
            let result = if matches!(command, VectorCommand::Simplify { .. }) {
                vector_commands::apply_simplify(
                    &source,
                    &ids,
                    value,
                    preserve_corners,
                    &task_token,
                )?
            } else {
                vector_commands::apply(&source, &ids, &command, &task_token)?
            };
            Ok::<_, anyhow::Error>((
                result.scene,
                result.selected,
                result.anchors_before,
                result.anchors_after,
            ))
        });
        self.busy = true;
        self.status = "Preparing editable paths… Escape cancels".into();
        self.focus.focus(window, cx);
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.vector_draft.as_ref().is_some_and(|d| Arc::ptr_eq(&d.cancel, &token)) { return; }
                this.busy = false;
                if !this.vector_scene_current() || !this.vector_draft.as_ref().is_some_and(|d| d.scene_snapshot().is_ok_and(|s| s == original) && d.scene.as_ref().is_some_and(|s| s.selected_objects == original_selection)) {
                    this.status = "Document changed; path result discarded".into();
                    cx.notify(); return;
                }
                match result.and_then(|(artwork, chosen, before, after)| {
                    if simplify_source.is_some() {
                        ensure!(this.detail_inputs[18].read(cx).value().parse::<f32>().ok() == Some(value)
                            && this.vector_draft.as_ref().and_then(|d| d.scene.as_ref()).is_some_and(|s| s.simplify.preserve_corners == preserve_corners),
                            "Simplify settings changed; preview again");
                    }
                    this.replace_scene_artwork(artwork, chosen, window, cx)?;
                    if let Some(source) = simplify_source {
                        let expected = this.vector_draft.as_ref().unwrap().scene_snapshot()?;
                        let scene = this.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
                        scene.simplify.session = Some(SimplifySession { source, expected,
                            selected: scene.selected_objects.clone(), before, after });
                    }
                    Ok((before, after))
                }) {
                    Ok((before, after)) => this.status = format!("Path preview · {before} → {after} anchors · Undo compares · Enter keeps edits"),
                    Err(error) => this.status = format!("Path operation: {error:#}"),
                }
                this.start_scene_preview(cx);
                this.focus.focus(window, cx);
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn vector_geometry_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = self.vector_draft.as_ref();
        let simplify = draft.and_then(|d| d.scene.as_ref()).map(|s| &s.simplify);
        let preserve_corners = simplify.is_none_or(|s| s.preserve_corners);
        let show_original = simplify.is_none_or(|s| s.show_original);
        let session = draft.and_then(|d| d.scene.as_ref()?.simplify.current(d));
        let selected = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| !s.selected_objects.is_empty());
        let mut rows = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(inspector_ui::panel_note(
                "Finish paths · Simplify previews always start from the original paths",
                cx,
            ));
        for (index, id, label, action, button_label) in [
            (
                18,
                "vector-simplify-tolerance",
                "Simplify tolerance · px",
                "vector-simplify",
                "Simplify",
            ),
            (
                19,
                "vector-offset-distance",
                "Offset distance · px",
                "vector-offset",
                "Offset path",
            ),
        ] {
            rows = rows.child(
                div()
                    .flex()
                    .gap_2()
                    .items_end()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(inspector_ui::panel_note(label, cx))
                            .child(
                                input(id, &self.detail_inputs[index], window, cx)
                                    .debug_selector(move || id.into()),
                            ),
                    )
                    .child(
                        button(action, button_label, ButtonVariant::Outline, cx)
                            .disabled(self.busy || !selected)
                            .debug_selector(move || action.into())
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.scene_geometry_action(action, w, cx)
                            })),
                    ),
            );
        }
        rows = rows.child(
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .child(
                    button(
                        "vector-simplify-corners",
                        "Preserve corners",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .selected(preserve_corners)
                    .disabled(self.busy || !selected)
                    .debug_selector(|| "vector-simplify-corners".into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(scene) =
                            this.vector_draft.as_mut().and_then(|d| d.scene.as_mut())
                        {
                            scene.simplify.preserve_corners = !scene.simplify.preserve_corners;
                        }
                        this.scene_geometry_action("vector-simplify", window, cx);
                    })),
                )
                .child(
                    button(
                        "vector-simplify-original",
                        "Original overlay",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .selected(show_original)
                    .disabled(self.busy || session.is_none_or(|s| s.before > 16_384))
                    .debug_selector(|| "vector-simplify-original".into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(scene) =
                            this.vector_draft.as_mut().and_then(|d| d.scene.as_mut())
                        {
                            scene.simplify.show_original = !scene.simplify.show_original;
                        }
                        cx.notify();
                    })),
                ),
        );
        if let Some(session) = session {
            rows = rows
                .child(inspector_ui::panel_note(
                    format!(
                        "Simplify · {} → {} anchors · {}",
                        session.before,
                        session.after,
                        if session.before <= 16_384 {
                            "original overlay in amber"
                        } else {
                            "overlay limited to 16,384 original anchors"
                        }
                    ),
                    cx,
                ))
                .child(
                    button(
                        "vector-simplify-reset",
                        "Reset Simplify",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .disabled(self.busy)
                    .debug_selector(|| "vector-simplify-reset".into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        let original = this
                            .vector_draft
                            .as_ref()
                            .and_then(|d| d.scene.as_ref()?.simplify.current(d))
                            .map(|s| (s.source.clone(), s.selected.clone()));
                        if let Some((artwork, selected)) = original {
                            if let Err(error) =
                                this.replace_scene_artwork(artwork, selected, window, cx)
                            {
                                this.status = format!("Reset Simplify: {error:#}");
                            } else {
                                this.status =
                                    "Simplify reset · earlier draft edits retained".into();
                            }
                        }
                    })),
                );
        }
        rows.child(
            button(
                "vector-outline-stroke",
                "Outline strokes",
                ButtonVariant::Outline,
                cx,
            )
            .disabled(self.busy || !selected)
            .debug_selector(|| "vector-outline-stroke".into())
            .on_click(cx.listener(|this, _, w, cx| {
                this.scene_geometry_action("vector-outline-stroke", w, cx)
            })),
        )
        .into_any_element()
    }
}
