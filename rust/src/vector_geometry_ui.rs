//! Background path finishing in the same disposable canvas draft.
use super::*;
use anyhow::{Context as _, Result, ensure};

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
            Ok((
                draft.scene_snapshot()?,
                scene.selected_objects.clone(),
                draft.cancel.clone(),
                value,
            ))
        })();
        let (source, selected, token, value) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let action = action.to_owned();
        let original = source.clone();
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
            let result = vector_commands::apply(&source, &ids, &command, &task_token)?;
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
                    this.replace_scene_artwork(artwork, chosen, window, cx)?;
                    Ok((before, after))
                }) {
                    Ok((before, after)) => this.status = format!("Path preview · {before} → {after} anchors · Ctrl+Z restores originals · Enter keeps edits"),
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
                "Finish paths · Preview on canvas, Undo to compare",
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
