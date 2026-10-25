//! Reversible font previews on the captured UTF-8 text selection.
use super::*;
use anyhow::Context as _;
use gpui_kit::FontWeight;
#[cfg(all(test, feature = "ui-test"))]
#[path = "inline_font_audition_tests.rs"]
mod audition_tests;

pub(super) struct FontEdit {
    pub original: objects::LiveTextStyle,
    pub range: std::ops::Range<usize>,
    pub typing: Option<String>,
    pub highlighted: Option<String>,
    pub comparisons: Vec<FontComparison>,
    pub identity: (u64, u64, u64),
}

pub(super) struct FontComparison {
    pub font: String,
    pub sample: String,
    pub truncated: bool,
    pub families: Vec<String>,
    pub image: Arc<RenderImage>,
}

pub(super) fn release_comparison_image(image: Arc<RenderImage>, cx: &mut App) {
    // During update_in/input callbacks the current window is temporarily out
    // of App.windows. Evict it after the effect cycle returns that window, as
    // well as immediately evicting any other windows that painted the sample.
    cx.drop_image(image.clone(), None);
    cx.defer(move |cx| cx.drop_image(image, None));
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
    fn font_identity(&self) -> (u64, u64, u64) {
        (
            self.editor.instance_id(),
            self.create.epoch,
            self.editor.revision(),
        )
    }

    pub(super) fn compare_inline_font(&mut self, font: &str, cx: &mut Context<Self>) {
        let identity = self.font_identity();
        if !self
            .font_names
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|name| name == font)
        {
            self.status = "Choose an installed font from this list".into();
            cx.notify();
            return;
        }
        let Some(draft) = self.inline_text.as_ref() else {
            return;
        };
        let Some(edit) = draft.font_edit.as_ref() else {
            return;
        };
        if edit.identity != identity
            || draft.input.read(cx).value().as_ref() != edit.original.content
        {
            self.status = "Text changed; reopen the font chooser to compare current text".into();
            cx.notify();
            return;
        }
        if edit.comparisons.len() >= 3 {
            self.status = "Compare up to three fonts; remove one to try another".into();
            cx.notify();
            return;
        }
        if edit.comparisons.iter().any(|s| s.font == font) {
            return;
        }
        let content = if edit.range.is_empty() {
            Some(edit.original.content.as_str())
        } else {
            edit.original.content.get(edit.range.clone())
        };
        let Some(content) = content else {
            return;
        };
        if content.is_empty() {
            self.status = "Enter some text before comparing fonts".into();
            cx.notify();
            return;
        }
        let sample: String = content.chars().take(160).collect();
        let truncated = sample.len() < content.len();
        let style = objects::LiveTextStyle {
            content: sample.clone(),
            font_name: font.into(),
            font_size: 24.,
            leading: 28.,
            box_size: Some(objects::ObjectSize {
                width: 320.,
                height: 84.,
            }),
            ..Default::default()
        };
        let result = (|| -> anyhow::Result<_> {
            let families = objects::text_font_usage(&style)?;
            let layer = objects::text_layer(
                "Font comparison",
                320,
                84,
                &sample,
                &objects::TextStyle {
                    family: font.into(),
                    size: 24.,
                    leading: 28.,
                    x: 6.,
                    y: 6.,
                    color: [30, 30, 30, 255],
                    ..Default::default()
                },
            )?;
            let image = layer.image.context("Font preview did not render")?;
            Ok((families, render_image(&image)))
        })();
        match result {
            Ok((families, image)) => {
                self.preview_inline_font(font, cx);
                if let Some(edit) = self.inline_text.as_mut().and_then(|d| d.font_edit.as_mut()) {
                    edit.comparisons.push(FontComparison {
                        font: font.into(),
                        sample,
                        truncated,
                        families,
                        image,
                    });
                }
                cx.notify();
            }
            Err(error) => {
                self.status = format!("Font comparison: {error:#}");
                cx.notify();
            }
        }
    }

    fn remove_font_comparison(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(edit) = self.inline_text.as_mut().and_then(|d| d.font_edit.as_mut()) {
            if index < edit.comparisons.len() {
                let removed = edit.comparisons.remove(index);
                release_comparison_image(removed.image, cx);
            }
        }
        cx.notify();
    }
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
        let identity = self.font_identity();
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        draft.font_edit = Some(FontEdit {
            original: draft.style.clone(),
            range: draft.input.read(cx).selected_range(),
            typing: draft.typing_font.clone(),
            highlighted: None,
            comparisons: Vec::new(),
            identity,
        });
        draft.font_search.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn preview_inline_font(&mut self, font: &str, cx: &mut Context<Self>) {
        let identity = self.font_identity();
        let Some(draft) = self.inline_text.as_mut() else {
            return;
        };
        let Some(edit) = draft.font_edit.as_mut() else {
            return;
        };
        // A late hover from a removed popup must not restore obsolete content.
        if edit.identity != identity
            || draft.input.read(cx).value().as_ref() != edit.original.content
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
        let identity = self.font_identity();
        if self
            .inline_text
            .as_ref()
            .and_then(|d| {
                d.font_edit.as_ref().map(|e| {
                    e.identity != identity
                        || d.input.read(cx).value().as_ref() != e.original.content
                })
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
        for comparison in &edit.comparisons {
            cx.drop_image(comparison.image.clone(), Some(window));
        }
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
        for comparison in &edit.comparisons {
            cx.drop_image(comparison.image.clone(), Some(window));
        }
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
        let edit = draft.font_edit.as_ref().unwrap();
        let pinned = edit.comparisons.len();
        let mut list = div()
            .id("inline-font-list")
            .max_h(px(if pinned == 0 { 196. } else { 112. }))
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
            let compared = font.clone();
            list = list.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
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
                        .flex_1()
                        .min_w_0()
                        .h(px(28.))
                        .flex_shrink_0()
                        .py_0()
                        .on_hover(cx.listener(move |this, over, _, cx| {
                            if *over {
                                this.preview_inline_font(&hover, cx);
                            }
                        }))
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.choose_inline_font(&font, window, cx),
                        )),
                    )
                    .child(
                        button(
                            SharedString::from(format!("inline-font-pin-{compared}")),
                            "Compare",
                            ButtonVariant::Outline,
                            cx,
                        )
                        .h(px(28.))
                        .py_0()
                        .disabled(
                            pinned >= 3 || edit.comparisons.iter().any(|s| s.font == compared),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.compare_inline_font(&compared, cx)
                        })),
                    ),
            );
        }
        let columns = if width >= 560. { 2. } else { 1. };
        let card_width = ((width - 24. - (columns - 1.) * 8.) / columns).max(120.);
        let mut comparisons = div()
            .id("inline-font-comparisons")
            .flex()
            .flex_wrap()
            .gap_2();
        for (index, sample) in edit.comparisons.iter().enumerate() {
            let chosen = sample.font.clone();
            let image = sample.image.clone();
            comparisons = comparisons.child(
                div()
                    .w(px(card_width))
                    .min_w_0()
                    .p_1()
                    .border_1()
                    .border_color(theme.border)
                    .rounded(control_radius())
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .child(sample.font.clone()),
                    )
                    .child(
                        div().h(px(70.)).w_full().overflow_hidden().child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let scale = (f32::from(bounds.size.width) / 320.)
                                        .min(f32::from(bounds.size.height) / 84.);
                                    let target = Bounds::new(
                                        bounds.origin,
                                        size(px(320. * scale), px(84. * scale)),
                                    );
                                    window.paint_quad(fill(bounds, rgb(0xfaf8f4)));
                                    let _ = window.paint_image(
                                        bounds,
                                        target,
                                        Corners::default(),
                                        image.clone(),
                                        0,
                                        false,
                                    );
                                },
                            )
                            .size_full(),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.secondary)
                            .text_ellipsis()
                            .child(format!("Sample: {}", sample.sample)),
                    )
                    .child(div().text_xs().text_color(theme.secondary).child(format!(
                        "Shaped with: {}",
                        if sample.families.is_empty() {
                            "no visible glyphs".into()
                        } else {
                            sample.families.join(", ")
                        }
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.secondary)
                            .child(if sample.truncated {
                                "First 160 characters · sample may clip"
                            } else {
                                "24 px comparison · sample may clip"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                button(
                                    SharedString::from(format!("inline-font-use-{index}")),
                                    "Use font",
                                    ButtonVariant::Primary,
                                    cx,
                                )
                                .flex_1()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.choose_inline_font(&chosen, window, cx)
                                    },
                                )),
                            )
                            .child(
                                button(
                                    SharedString::from(format!("inline-font-unpin-{index}")),
                                    "Remove",
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| this.remove_font_comparison(index, cx),
                                )),
                            ),
                    ),
            );
        }
        let keep = highlighted.cloned().unwrap_or_default();
        div()
            .id("inline-font-popup")
            .debug_selector(|| "inline-font-popup".into())
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(width))
            .max_h(px((f32::from(window.viewport_size().height)-top-8.).max(120.)))
            .overflow_y_scroll()
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
            .child(comparisons)
            .child(div().flex().gap_1()
                .child(button("inline-font-restore","Cancel",ButtonVariant::Outline,cx).flex_1().on_click(cx.listener(|this,_,window,cx|{this.cancel_inline_font(window,cx);})))
                .child(button("inline-font-keep","Keep font",ButtonVariant::Primary,cx).flex_1().disabled(highlighted.is_none()).on_click(cx.listener(move|this,_,window,cx|this.choose_inline_font(&keep,window,cx)))))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.secondary)
                    .child("Compare up to 3 fonts on your selected text. Hover or ↑↓ previews the canvas · Enter keeps · Esc restores."),
            )
            .into_any_element()
    }
}
