//! Canvas-accurate text drafts with bounded background work and native selection.
use super::*;
use gpui_kit::Focusable;
#[cfg(all(test, feature = "ui-test"))]
#[path = "inline_text_upgrade_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct InlinePreview {
    generation: u64,
    running: Option<u64>,
    pending: bool,
}

pub(super) struct TextColorEdit {
    original: objects::LiveTextStyle,
    range: std::ops::Range<usize>,
    typing: Option<[f32; 4]>,
    initial_color: Rgba,
    pub(super) committed: bool,
}

fn text_color(style: &objects::LiveTextStyle, range: std::ops::Range<usize>) -> [f32; 4] {
    let at = if range.is_empty() {
        range.start.saturating_sub(1)
    } else {
        range.start
    };
    style
        .runs
        .iter()
        .find(|run| run.start <= at && run.end > at)
        .and_then(|run| run.color)
        .unwrap_or([style.red, style.green, style.blue, 1.])
}

fn rgba_color(color: [f32; 4]) -> Rgba {
    Rgba {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    }
}

impl EditorView {
    pub(super) fn arm_inline_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .inline_text
            .as_ref()
            .is_none_or(|draft| !draft.input.read(cx).focus_handle(cx).is_focused(window))
        {
            return;
        }
        // Capture the native Undo/Redo action, including context-menu actions.
        // Merely typing a string seen before must never resurrect old styling.
        if self.sync_inline_text(cx).is_err() {
            return;
        }
        self.inline_text.as_mut().unwrap().restore_text_history = true;
        let view = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = view.update(cx, |this, cx| {
                if this.sync_inline_text(cx).unwrap_or(false) {
                    this.schedule_inline_preview(cx);
                }
                if let Some(draft) = this.inline_text.as_mut() {
                    draft.restore_text_history = false;
                }
            });
        });
    }
    pub(super) fn sync_inline_text(&mut self, cx: &mut Context<Self>) -> anyhow::Result<bool> {
        let Some(draft) = self.inline_text.as_mut() else {
            return Ok(false);
        };
        let content = draft.input.read(cx).value().to_string();
        if draft.style.content == content {
            return Ok(false);
        }
        // A content edit invalidates a hovered font; restore style only, then
        // reconcile the current input so later typing can never disappear.
        if let Some(edit) = draft.font_edit.take() {
            draft.style = edit.original;
            draft.typing_font = edit.typing;
        }
        let mut candidate = draft
            .restore_text_history
            .then(|| {
                draft
                    .history
                    .iter()
                    .rev()
                    .find(|s| s.content == content)
                    .cloned()
            })
            .flatten()
            .unwrap_or_else(|| draft.style.clone());
        draft.restore_text_history = false;
        if candidate.content != content {
            let inserted = objects::edit_text_content(&mut candidate, content)?;
            if !inserted.is_empty() && (draft.typing_color.is_some() || draft.typing_font.is_some())
            {
                objects::apply_rich_text_patch(
                    &mut candidate,
                    inserted,
                    objects::RichTextPatch {
                        color: draft.typing_color,
                        font_name: draft.typing_font.clone(),
                        ..Default::default()
                    },
                )?;
            }
        }
        draft.history.push_back(draft.style.clone());
        while draft.history.len() > 32 {
            draft.history.pop_front();
        }
        draft.style = candidate;
        Ok(true)
    }

    pub(super) fn inline_input_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let change = self.sync_inline_text(cx);
        let content_changed = matches!(change, Ok(true));
        match change {
            Ok(true) => self.schedule_inline_preview(cx),
            Err(error) => {
                self.status = format!("Text: {error:#}");
                cx.notify();
            }
            _ => {}
        }
        let Some(draft) = &mut self.inline_text else {
            return;
        };
        if draft.color.read(cx).is_open() || draft.color_edit.is_some() || draft.font_edit.is_some()
        {
            return;
        }
        let selection = draft.input.read(cx).selected_range();
        if !content_changed && draft.last_selection != selection {
            draft.typing_color = None;
            draft.typing_font = None;
        }
        draft.last_selection = selection.clone();
        let color = draft
            .typing_color
            .unwrap_or_else(|| text_color(&draft.style, selection));
        let current = draft.color.read(cx).value().map(Rgba::from);
        let target = rgba_color(color);
        if current.is_none_or(|value| {
            (value.r - target.r).abs()
                + (value.g - target.g).abs()
                + (value.b - target.b).abs()
                + (value.a - target.a).abs()
                > 0.0001
        }) {
            draft
                .color
                .update(cx, |state, cx| state.set_value(target, window, cx));
        }
        cx.notify();
    }

    pub(super) fn inline_color_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .inline_text
            .as_ref()
            .is_some_and(|draft| draft.color.read(cx).is_open())
        {
            self.cancel_inline_font(window, cx);
        }
        if self.sync_inline_text(cx).is_err() {
            return;
        }
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        let open = draft.color.read(cx).is_open();
        if open && draft.color_edit.is_none() {
            draft.color_edit =
                Some(TextColorEdit {
                    original: draft.style.clone(),
                    range: draft.input.read(cx).selected_range(),
                    typing: draft.typing_color,
                    initial_color: draft.color.read(cx).value().map(Rgba::from).unwrap_or_else(
                        || {
                            rgba_color(text_color(
                                &draft.style,
                                draft.input.read(cx).selected_range(),
                            ))
                        },
                    ),
                    committed: false,
                });
        }
        let Some(edit) = draft.color_edit.as_ref() else {
            return;
        };
        if !open && !edit.committed {
            let edit = draft.color_edit.take().unwrap();
            let changed = draft.style != edit.original;
            draft.style = edit.original;
            draft.typing_color = edit.typing;
            draft.last_selection = edit.range.clone();
            draft.color.update(cx, |picker, cx| {
                picker.set_value(edit.initial_color, window, cx)
            });
            draft.input.update(cx, |input, cx| {
                input.set_selected_range(edit.range, cx);
                input.focus(window, cx);
            });
            if changed {
                self.schedule_inline_preview(cx);
            }
            cx.notify();
            return;
        }
        let value = if open {
            draft.color.read(cx).displayed_color()
        } else {
            draft.color.read(cx).value()
        };
        let Some(value) = value else {
            return;
        };
        let c: Rgba = value.into();
        let color = [c.r, c.g, c.b, c.a];
        let mut candidate = edit.original.clone();
        let result = if edit.range.is_empty() {
            draft.typing_color = Some(color);
            if candidate.content.is_empty() {
                candidate.red = c.r;
                candidate.green = c.g;
                candidate.blue = c.b;
            }
            Ok(())
        } else {
            objects::apply_rich_text_patch(
                &mut candidate,
                edit.range.clone(),
                objects::RichTextPatch {
                    color: Some(color),
                    ..Default::default()
                },
            )
        };
        let changed = candidate != draft.style;
        if let Err(error) = result {
            self.status = format!("Text colour: {error:#}");
        } else {
            draft.style = candidate;
        }
        if !open {
            let edit = draft.color_edit.take().unwrap();
            draft.last_selection = edit.range.clone();
            draft.input.update(cx, |input, cx| {
                input.set_selected_range(edit.range, cx);
                input.focus(window, cx);
            });
        }
        if changed {
            self.schedule_inline_preview(cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_inline_color(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft) = self.inline_text.as_mut() else {
            return false;
        };
        if !draft.color.read(cx).is_open() {
            return false;
        }
        if let Some(edit) = draft.color_edit.take() {
            draft.style = edit.original;
            draft.typing_color = edit.typing;
            draft.last_selection = edit.range.clone();
            let color = edit.initial_color;
            draft.color.update(cx, |state, cx| {
                state.set_value(color, window, cx);
                state.set_open(false, cx);
            });
            draft.input.update(cx, |input, cx| {
                input.set_selected_range(edit.range, cx);
                input.focus(window, cx);
            });
        } else {
            draft
                .color
                .update(cx, |state, cx| state.set_open(false, cx));
        }
        self.schedule_inline_preview(cx);
        true
    }

    pub(super) fn schedule_inline_preview(&mut self, cx: &mut Context<Self>) {
        self.inline_preview.generation = self.inline_preview.generation.wrapping_add(1);
        self.inline_preview.pending = true;
        self.start_inline_preview(cx);
    }

    fn start_inline_preview(&mut self, cx: &mut Context<Self>) {
        if self.inline_preview.running.is_some() || !self.inline_preview.pending {
            return;
        }
        let Some(draft) = &self.inline_text else {
            return;
        };
        let generation = self.inline_preview.generation;
        self.inline_preview.running = Some(generation);
        self.inline_preview.pending = false;
        let identity = (
            self.editor.instance_id(),
            self.create.epoch,
            self.editor.revision(),
        );
        let mut editor = Editor::new(self.editor.document.clone());
        let style = draft.style.clone();
        let layer_id = draft.layer.clone();
        let origin = draft.origin;
        let proof = self.proof_settings.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(id) = layer_id {
                editor.set_live_text(&id, style)?;
            } else if !style.content.trim().is_empty() {
                let layer = objects::live_text_layer(
                    "Text preview",
                    objects::ObjectPoint {
                        x: origin.0,
                        y: origin.1,
                    },
                    style,
                )?;
                anyhow::ensure!(
                    !editor.import_layer(layer).is_empty(),
                    "Text exceeds the document limits"
                );
            }
            let pixels = raster::composite(&editor.document);
            anyhow::ensure!(
                pixels.dimensions() == (editor.document.width, editor.document.height),
                "Text preview could not be rendered"
            );
            if proof.enabled {
                omuse::proofing::render(&pixels, &proof)
            } else {
                Ok(pixels)
            }
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.inline_preview.running != Some(generation) {
                    return;
                }
                this.inline_preview.running = None;
                if this.inline_text.is_some()
                    && this.inline_preview.generation == generation
                    && (
                        this.editor.instance_id(),
                        this.create.epoch,
                        this.editor.revision(),
                    ) == identity
                {
                    match result {
                        Ok(pixels) => {
                            this.display.replace(&pixels);
                        }
                        Err(error) => {
                            this.status = format!("Text preview: {error:#}");
                        }
                    }
                    cx.notify();
                }
                this.start_inline_preview(cx);
            });
        })
        .detach();
    }

    pub(super) fn end_inline_preview(&mut self, cx: &mut Context<Self>) {
        self.inline_preview.generation = self.inline_preview.generation.wrapping_add(1);
        self.inline_preview.pending = false;
        self.present_pixels(cx);
    }
}
