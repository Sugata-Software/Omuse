//! Reversible font previews on the captured UTF-8 text selection.
use super::*;

pub(super) struct FontEdit {
    pub original: objects::LiveTextStyle,
    pub range: std::ops::Range<usize>,
    pub typing: Option<String>,
    pub highlighted: Option<String>,
}

fn font_at(style: &objects::LiveTextStyle, at: usize) -> &str {
    style
        .runs
        .iter()
        .find(|r| r.start <= at && at < r.end)
        .and_then(|r| r.font_name.as_deref())
        .unwrap_or(&style.font_name)
}

impl EditorView {
    pub(super) fn inline_font_label(&self, cx: &App) -> String {
        let Some(draft) = &self.inline_text else {
            return "Font".into();
        };
        let range = draft.input.read(cx).selected_range();
        if range.is_empty() {
            return draft
                .typing_font
                .clone()
                .unwrap_or_else(|| font_at(&draft.style, range.start.saturating_sub(1)).into());
        }
        let mut fonts = draft
            .style
            .content
            .char_indices()
            .filter(|(at, _)| range.contains(at))
            .map(|(at, _)| font_at(&draft.style, at));
        let first = fonts.next().unwrap_or(&draft.style.font_name);
        if fonts.any(|font| font != first) {
            "Mixed fonts".into()
        } else {
            first.into()
        }
    }

    fn inline_fonts(&self, cx: &App) -> Vec<String> {
        let Some(draft) = &self.inline_text else {
            return Vec::new();
        };
        let query = draft.font_search.read(cx).value().to_lowercase();
        self.font_names
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|font| query.is_empty() || font.to_lowercase().contains(&query))
            .take(64)
            .cloned()
            .collect()
    }

    pub(super) fn begin_inline_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel_inline_font(window, cx) {
            return;
        }
        self.cancel_inline_color(window, cx);
        if self.sync_inline_text(cx).is_err() {
            return;
        }
        self.ensure_font_names(cx);
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        draft.font_edit = Some(FontEdit {
            original: draft.style.clone(),
            range: draft.input.read(cx).selected_range(),
            typing: draft.typing_font.clone(),
            highlighted: None,
        });
        draft.font_search.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn preview_inline_font(&mut self, font: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        let Some(edit) = draft.font_edit.as_mut() else {
            return;
        };
        // A late hover from a removed popup must not restore obsolete content.
        if draft.input.read(cx).value().as_ref() != edit.original.content
            || edit.highlighted.as_deref() == Some(font)
        {
            return;
        }
        let mut candidate = edit.original.clone();
        let result = if edit.range.is_empty() {
            draft.typing_font = Some(font.into());
            if candidate.content.is_empty() {
                candidate.font_name = font.into();
            }
            Ok(())
        } else {
            objects::apply_rich_text_patch(
                &mut candidate,
                edit.range.clone(),
                objects::RichTextPatch {
                    font_name: Some(font.into()),
                    ..Default::default()
                },
            )
        };
        if let Err(error) = result {
            self.status = format!("Font: {error:#}");
            return;
        }
        edit.highlighted = Some(font.into());
        draft.style = candidate;
        self.schedule_inline_preview(cx);
        cx.notify();
    }

    pub(super) fn choose_inline_font(
        &mut self,
        font: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .inline_text
            .as_ref()
            .and_then(|d| {
                d.font_edit
                    .as_ref()
                    .map(|e| d.input.read(cx).value().as_ref() != e.original.content)
            })
            .unwrap_or(false)
        {
            self.cancel_inline_font(window, cx);
            return;
        }
        self.preview_inline_font(font, cx);
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        if draft
            .font_edit
            .as_ref()
            .and_then(|e| e.highlighted.as_deref())
            != Some(font)
        {
            return;
        }
        let Some(edit) = draft.font_edit.take() else {
            return;
        };
        draft.history.push_back(edit.original);
        while draft.history.len() > 32 {
            draft.history.pop_front();
        }
        draft.last_selection = edit.range.clone();
        draft.input.update(cx, |input, cx| {
            input.set_selected_range(edit.range, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_inline_font(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft) = self.inline_text.as_mut() else {
            return false;
        };
        let Some(edit) = draft.font_edit.take() else {
            return false;
        };
        let range = if draft.input.read(cx).value().as_ref() == edit.original.content {
            edit.range
        } else {
            draft.input.read(cx).selected_range()
        };
        draft.style = edit.original;
        draft.typing_font = edit.typing;
        draft.last_selection = range.clone();
        draft.input.update(cx, |input, cx| {
            input.set_selected_range(range, cx);
            input.focus(window, cx);
        });
        // Preserve any text entered since the preview was opened.
        if let Err(error) = self.sync_inline_text(cx) {
            self.status = format!("Text: {error:#}");
        }
        self.schedule_inline_preview(cx);
        cx.notify();
        true
    }

    pub(super) fn inline_font_key(
        &mut self,
        key: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .inline_text
            .as_ref()
            .is_none_or(|d| d.font_edit.is_none())
        {
            return false;
        }
        if key.modifiers.control || key.modifiers.platform || key.modifiers.alt {
            return false;
        }
        match key.key.as_str() {
            "escape" => {
                self.cancel_inline_font(window, cx);
                true
            }
            "up" | "down" | "enter" => {
                let fonts = self.inline_fonts(cx);
                if fonts.is_empty() {
                    return true;
                }
                let highlighted = self
                    .inline_text
                    .as_ref()
                    .and_then(|d| d.font_edit.as_ref())
                    .and_then(|e| e.highlighted.as_ref());
                let current = highlighted.and_then(|font| fonts.iter().position(|f| f == font));
                let index = match key.key.as_str() {
                    "up" => current.map_or(fonts.len() - 1, |n| n.saturating_sub(1)),
                    "down" => current.map_or(0, |n| (n + 1).min(fonts.len() - 1)),
                    _ => current.unwrap_or(0),
                };
                if key.key == "enter" {
                    self.choose_inline_font(&fonts[index], window, cx);
                } else {
                    self.preview_inline_font(&fonts[index], cx);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn inline_font_popup(
        &self,
        left: f32,
        top: f32,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = self.inline_text.as_ref().unwrap();
        if draft.font_edit.is_none() {
            return div().into_any_element();
        }
        let theme = cx.omarchy().clone();
        let highlighted = draft
            .font_edit
            .as_ref()
            .and_then(|e| e.highlighted.as_ref());
        let fonts = self.inline_fonts(cx);
        let mut list = div()
            .id("inline-font-list")
            .max_h(px(196.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        if fonts.is_empty() {
            list = list.child("No matching installed fonts");
        }
        for font in fonts {
            let hover = font.clone();
            let chosen = font.clone();
            let selected = highlighted == Some(&font);
            list = list.child(
                button(
                    SharedString::from(format!("inline-font-{font}")),
                    font.clone(),
                    if selected {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .debug_selector(move || format!("inline-font-option-{chosen}"))
                .font_family(SharedString::from(font.clone()))
                .w_full()
                .h(px(28.))
                .py_0()
                .on_hover(cx.listener(move |this, over, _, cx| {
                    if *over {
                        this.preview_inline_font(&hover, cx);
                    }
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.choose_inline_font(&font, window, cx)
                })),
            );
        }
        div()
            .id("inline-font-popup")
            .debug_selector(|| "inline-font-popup".into())
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(width))
            .p_2()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.accent)
            .rounded(control_radius())
            .flex()
            .flex_col()
            .gap_2()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                input("inline-font-search", &draft.font_search, window, cx)
                    .debug_selector(|| "inline-font-search".into()),
            )
            .child(list)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.secondary)
                    .child("Hover or ↑↓ to preview · Enter keeps · Esc restores"),
            )
            .into_any_element()
    }
}
