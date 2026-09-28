//! Native typography controls for UTF-8 rich text runs.
use super::create_ui::{note, section};
use super::*;
use anyhow::Context as _;
use omuse::objects::{RichTextPatch, RichTextRun};

fn rich_hex_color(value: &str) -> anyhow::Result<[f32; 4]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    anyhow::ensure!(value.len() == 6, "Use a six-digit colour such as #D98566");
    anyhow::ensure!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Use hexadecimal colour digits"
    );
    Ok([
        u8::from_str_radix(&value[0..2], 16)? as f32 / 255.0,
        u8::from_str_radix(&value[2..4], 16)? as f32 / 255.0,
        u8::from_str_radix(&value[4..6], 16)? as f32 / 255.0,
        1.0,
    ])
}

fn run_description(run: &RichTextRun, content: &str) -> String {
    let preview = content[run.start..run.end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(24)
        .collect::<String>();
    let mut attributes = vec![];
    if let Some(weight) = run.weight {
        attributes.push(format!("weight {weight}"));
    }
    if run.italic == Some(true) {
        attributes.push("italic".into());
    }
    if let Some(font) = &run.font_name {
        attributes.push(font.clone());
    }
    if let Some(size) = run.font_size {
        attributes.push(format!("{size} px"));
    }
    if run.color.is_some() {
        attributes.push("colour".into());
    }
    format!("“{preview}” · {}", attributes.join(", "))
}

#[derive(Clone, Copy)]
enum RichTextScope {
    Full,
    Characters,
    Word,
}

impl EditorView {
    pub(super) fn create_rich_text_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|layer| objects::live_text(layer).ok().flatten());
        let mut body = section("RICH TYPOGRAPHY", cx).child(note(
            "Select a live text layer, then style the full text, a character range, or the word at Start.",
            cx,
        ));
        if let Some(style) = style {
            let layout = objects::text_layout_report(&style);
            body = body
                .child(note(
                    format!(
                        "{} characters · {} styled runs",
                        style.content.chars().count(),
                        style.runs.len()
                    ),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .child(note("Start character", cx))
                                .child(input(
                                    "rich-text-start",
                                    &self.create.fields[17],
                                    window,
                                    cx,
                                )),
                        )
                        .child(div().flex_1().child(note("End character", cx)).child(input(
                            "rich-text-end",
                            &self.create.fields[18],
                            window,
                            cx,
                        ))),
                )
                .child(note("Weight (1-1000)", cx))
                .child(input(
                    "rich-text-weight",
                    &self.create.fields[19],
                    window,
                    cx,
                ))
                .child(note("Colour", cx))
                .child(input(
                    "rich-text-color",
                    &self.create.fields[20],
                    window,
                    cx,
                ))
                .child(note("Font override (optional)", cx))
                .child(input("rich-text-font", &self.create.fields[21], window, cx))
                .child(note("Size override (optional)", cx))
                .child(input("rich-text-size", &self.create.fields[22], window, cx))
                .child(note("Italic (true or false)", cx))
                .child(input(
                    "rich-text-italic",
                    &self.create.fields[23],
                    window,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            button("rich-text-full", "Full text", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result =
                                        this.apply_rich_text_scope(RichTextScope::Full, cx);
                                    this.create_error(result, cx);
                                })),
                        )
                        .child(
                            button(
                                "rich-text-range",
                                "Characters",
                                ButtonVariant::Secondary,
                                cx,
                            )
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result =
                                    this.apply_rich_text_scope(RichTextScope::Characters, cx);
                                this.create_error(result, cx);
                            })),
                        )
                        .child(
                            button("rich-text-word", "Word", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result =
                                        this.apply_rich_text_scope(RichTextScope::Word, cx);
                                    this.create_error(result, cx);
                                })),
                        ),
                )
                .child(
                    button(
                        "rich-text-clear",
                        "Clear rich formatting",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .w_full()
                    .disabled(style.runs.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.clear_rich_text(cx);
                        this.create_error(result, cx);
                    })),
                )
                .child(note("FIT AND OVERFLOW", cx));
            match layout {
                Ok(report) => {
                    let state = if report.overflows() {
                        "Text is clipped"
                    } else {
                        "Text fits"
                    };
                    body = body.child(note(
                        format!(
                            "{state} · {:.0} × {:.0} px content in {:.0} × {:.0} px · {} lines",
                            report.content_width,
                            report.content_height,
                            report.available_width,
                            report.available_height,
                            report.line_count,
                        ),
                        cx,
                    ));
                    if !report.missing_fonts.is_empty() {
                        body = body.child(note(
                            format!(
                                "Missing font{}: {}. A system fallback is being used.",
                                if report.missing_fonts.len() == 1 {
                                    ""
                                } else {
                                    "s"
                                },
                                report.missing_fonts.join(", ")
                            ),
                            cx,
                        ));
                    }
                }
                Err(error) => body = body.child(note(format!("Layout report: {error:#}"), cx)),
            }
            body = body
                .child(note("Minimum readable size", cx))
                .child(input(
                    "rich-text-minimum-size",
                    &self.create.fields[24],
                    window,
                    cx,
                ))
                .child(
                    button(
                        "rich-text-fit",
                        "Fit text to box",
                        ButtonVariant::Primary,
                        cx,
                    )
                    .w_full()
                    .disabled(style.box_size.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.fit_selected_text(cx);
                        this.create_error(result, cx);
                    })),
                );
            for run in style.runs.iter().take(8) {
                body = body.child(note(run_description(run, &style.content), cx));
            }
            if style.runs.len() > 8 {
                body = body.child(note(format!("{} more runs", style.runs.len() - 8), cx));
            }
        } else {
            body = body.child(note("The active layer is not editable live text.", cx));
        }
        body.into_any_element()
    }

    fn rich_text_patch(&self, cx: &App) -> anyhow::Result<RichTextPatch> {
        let weight = self.create.value(19, cx).parse::<u16>()?;
        anyhow::ensure!((1..=1_000).contains(&weight), "Weight must be 1-1000");
        let color = rich_hex_color(&self.create.value(20, cx))?;
        let font = self.create.value(21, cx);
        let size = self.create.value(22, cx);
        let size = if size.is_empty() {
            None
        } else {
            let value = size.parse::<f32>()?;
            anyhow::ensure!(
                value.is_finite() && (1.0..=2_000.0).contains(&value),
                "Size must be 1-2000 pixels"
            );
            Some(value)
        };
        let italic = match self.create.value(23, cx).to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" | "italic" => true,
            "false" | "no" | "0" | "normal" => false,
            _ => anyhow::bail!("Italic must be true or false"),
        };
        Ok(RichTextPatch {
            font_name: (!font.is_empty()).then_some(font),
            font_size: size,
            weight: Some(weight),
            italic: Some(italic),
            color: Some(color),
        })
    }

    fn apply_rich_text_scope(
        &mut self,
        scope: RichTextScope,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let id = self.editor.active_layer.clone();
        let mut style = self
            .editor
            .document
            .find_layer(&id)
            .and_then(|layer| objects::live_text(layer).ok().flatten())
            .context("Select an editable live text layer")?;
        let patch = self.rich_text_patch(cx)?;
        let start = self.create.value(17, cx).parse::<usize>()?;
        match scope {
            RichTextScope::Full => {
                let characters = style.content.chars().count();
                anyhow::ensure!(characters > 0, "Text is empty");
                objects::apply_rich_text_patch_characters(&mut style, 0, characters, patch)?;
            }
            RichTextScope::Characters => {
                let end = self.create.value(18, cx).parse::<usize>()?;
                objects::apply_rich_text_patch_characters(&mut style, start, end, patch)?;
            }
            RichTextScope::Word => {
                objects::apply_rich_text_patch_word(&mut style, start, patch)?;
            }
        }
        anyhow::ensure!(
            self.editor.set_live_text(&id, style)?,
            "Text layer is locked or unchanged"
        );
        self.changed(cx);
        self.schedule_content_recovery();
        Ok(())
    }

    fn clear_rich_text(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let id = self.editor.active_layer.clone();
        let mut style = self
            .editor
            .document
            .find_layer(&id)
            .and_then(|layer| objects::live_text(layer).ok().flatten())
            .context("Select an editable live text layer")?;
        anyhow::ensure!(!style.runs.is_empty(), "Text has no rich formatting");
        style.runs.clear();
        anyhow::ensure!(
            self.editor.set_live_text(&id, style)?,
            "Text layer is locked or unchanged"
        );
        self.changed(cx);
        self.schedule_content_recovery();
        Ok(())
    }

    fn fit_selected_text(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let id = self.editor.active_layer.clone();
        let style = self
            .editor
            .document
            .find_layer(&id)
            .and_then(|layer| objects::live_text(layer).ok().flatten())
            .context("Select an editable live text layer")?;
        let minimum = self.create.value(24, cx).parse::<f32>()?;
        anyhow::ensure!(
            minimum.is_finite() && (8.0..=style.font_size).contains(&minimum),
            "Minimum readable size must be 8 pixels or more and no larger than the current size"
        );
        let fitted = objects::fit_text_to_box(&style, minimum)?;
        if !fitted.fitted {
            self.status = "Text already fits its box".into();
            cx.notify();
            return Ok(());
        }
        let final_size = fitted.style.font_size;
        let remains_clipped = fitted.report.overflows();
        anyhow::ensure!(
            self.editor.set_live_text(&id, fitted.style)?,
            "Text layer is locked or unchanged"
        );
        self.changed(cx);
        self.status = if remains_clipped {
            format!(
                "Text remains clipped at the {:.0} px minimum; enlarge the box or shorten the copy",
                minimum
            )
        } else {
            format!("Text fitted at {final_size:.1} px")
        };
        self.schedule_content_recovery();
        Ok(())
    }
}
