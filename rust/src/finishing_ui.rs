//! Full-resolution finishing drafts. Preview never edits the document; Apply
//! consumes the exact computed pixels through the editor's selection/undo path.
use super::inspector_ui::panel_button as button;
use super::*;
use anyhow::{Context as _, Result, ensure};
use gpui_kit::img;
use omuse::dither::{Palette, PixelShape, Settings, Style};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(test, feature = "ui-test"))]
#[path = "finishing_tests.rs"]
mod tests;

pub(super) struct FinishingDraft {
    window: gpui_kit::AnyWindowHandle,
    kind: usize,
    layer: String,
    revision: u64,
    selection: Option<Selection>,
    source: Arc<image::RgbaImage>,
    inputs: Vec<Entity<InputState>>,
    settings: Settings,
    original: Arc<RenderImage>,
    preview: Option<Arc<RenderImage>>,
    show_original: bool,
    computed: Option<(Filter, Arc<image::RgbaImage>)>,
    cancel: Arc<AtomicBool>,
    job: u64,
}
impl Drop for FinishingDraft {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

const NAMES: [&str; 4] = [
    "Dither & halftone",
    "Bloom into transparency",
    "Vignette overlay",
    "Local tonal contrast",
];
fn fields(kind: usize) -> &'static [(&'static str, &'static str)] {
    match kind {
        1 => &[
            ("Radius (0–128 px)", "8"),
            ("Amount (0–10)", "0.7"),
            ("Highlight threshold (0–1)", "0.65"),
        ],
        2 => &[
            ("Opacity (0–1)", "0.65"),
            ("Midpoint (0–1)", "0.3"),
            ("Feather (0.001–1)", "0.6"),
            ("Overlay colour (#RRGGBB)", "#1B1D1F"),
        ],
        3 => &[
            ("Radius (0–128 px)", "16"),
            ("Shadows (−1–1)", "0.15"),
            ("Midtones (−1–1)", "0.3"),
            ("Highlights (−1–1)", "0.15"),
        ],
        _ => &[
            ("Pixel size (1–32 px)", "2"),
            ("Cell size (4–64 pixels)", "8"),
            ("Tones (2–8)", "2"),
            ("Screen angle (−90–90°)", "45"),
            ("Diffusion (0–100%)", "100"),
            ("Ink density (−100–100%)", "0"),
            ("Contrast (−100–100%)", "0"),
            ("Dark colour (#RRGGBB)", "#1B1D1F"),
            ("Light colour (#RRGGBB)", "#F5D3A2"),
            ("ASCII characters (1–64)", " .:-=+*#%@"),
        ],
    }
}
fn parse_color(value: &str) -> Result<[u8; 3]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    ensure!(
        value.len() == 6 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "Use a six-digit colour such as #F5D3A2"
    );
    Ok([
        u8::from_str_radix(&value[0..2], 16)?,
        u8::from_str_radix(&value[2..4], 16)?,
        u8::from_str_radix(&value[4..6], 16)?,
    ])
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
fn thumbnail(image: &image::RgbaImage) -> image::RgbaImage {
    let scale = (660. / image.width() as f64)
        .min(280. / image.height() as f64)
        .min(1.);
    image::imageops::thumbnail(
        image,
        (image.width() as f64 * scale).round().max(1.) as u32,
        (image.height() as f64 * scale).round().max(1.) as u32,
    )
}

impl EditorView {
    pub(super) fn clear_finishing(&mut self, cx: &mut App) {
        if let Some(mut draft) = self.finishing_draft.take() {
            draft.cancel.store(true, Ordering::Relaxed);
            cx.drop_image(draft.original.clone(), None);
            if let Some(image) = draft.preview.take() {
                cx.drop_image(image, None);
            }
            self.busy = false;
        }
    }
    pub(super) fn cancel_finishing(&mut self, cx: &mut App) {
        if self.finishing_draft.is_some() {
            self.clear_finishing(cx);
            self.status = "Finishing preview cancelled".into();
        }
    }
    pub(super) fn open_finishing(
        &mut self,
        kind: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_interaction(cx);
        let prepared = (|| -> Result<_> {
            ensure!(
                !self.paint_mask,
                "Switch to layer pixels before using finishing effects"
            );
            ensure!(
                self.editor.floating_selection_layer().is_none(),
                "Commit or cancel the floating selection first"
            );
            let id = self.editor.active_layer.clone();
            let layer = self
                .editor
                .document
                .find_layer(&id)
                .context("Choose a paint layer")?;
            ensure!(
                unlocked(&self.editor.document.layers, &id, false),
                "Unlock this layer and its parents first"
            );
            ensure!(
                layer.advanced.is_none()
                    && !layer.is_group()
                    && !layer.metadata.get("text").is_some_and(|v| !v.is_null())
                    && !layer.metadata.get("shape").is_some_and(|v| !v.is_null()),
                "Rasterize a copy of this editable source before using finishing effects"
            );
            let source = layer.image.as_deref().context("Choose a paint layer")?;
            ensure!(
                u64::from(source.width()) * u64::from(source.height()) <= omuse::dither::MAX_PIXELS
                    && u64::from(self.editor.document.width)
                        * u64::from(self.editor.document.height)
                        <= omuse::dither::MAX_PIXELS,
                "Finishing previews support layers and canvases up to 16 megapixels"
            );
            Ok((id, Arc::new(source.clone())))
        })();
        let (layer, source) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.clear_finishing(cx);
        let kind = kind.min(3);
        let inputs = fields(kind)
            .iter()
            .map(|(_, value)| {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(*value));
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.schedule_finishing_preview(cx);
                    }
                })
                .detach();
                input
            })
            .collect();
        self.finishing_draft = Some(FinishingDraft {
            window: window.window_handle(),
            kind,
            layer,
            revision: self.editor.revision(),
            selection: self.editor.selection.clone(),
            source,
            inputs,
            settings: Settings::default(),
            original: render_image(&thumbnail(&self.pixels)),
            preview: None,
            show_original: false,
            computed: None,
            cancel: Arc::new(AtomicBool::new(false)),
            job: 0,
        });
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.dialog = Dialog::Finishing;
        self.modal_focus.focus(window, cx);
        self.schedule_finishing_preview(cx);
    }
    fn finishing_filter(&self, cx: &Context<Self>) -> Result<Filter> {
        let draft = self
            .finishing_draft
            .as_ref()
            .context("Finishing draft closed")?;
        let number = |i: usize| -> Result<f32> {
            let value = draft.inputs[i]
                .read(cx)
                .value()
                .parse::<f32>()
                .context("Enter a number for each numeric setting")?;
            ensure!(value.is_finite(), "Settings must be finite numbers");
            Ok(value)
        };
        let integer = |i: usize| -> Result<u32> {
            let value = number(i)?;
            ensure!(
                value.fract() == 0. && (0. ..=u32::MAX as f32).contains(&value),
                "Pixel, cell and tone counts must be whole numbers"
            );
            Ok(value as u32)
        };
        let filter = match draft.kind {
            1 => Filter::BloomGlow {
                sigma: number(0)?,
                amount: number(1)?,
                threshold: number(2)?,
            },
            2 => Filter::VignetteOverlay {
                opacity: number(0)?,
                midpoint: number(1)?,
                feather: number(2)?,
                color: parse_color(&draft.inputs[3].read(cx).value())?,
            },
            3 => Filter::LocalContrast {
                sigma: number(0)?,
                shadows: number(1)?,
                midtones: number(2)?,
                highlights: number(3)?,
            },
            _ => {
                let mut settings = draft.settings.clone();
                settings.pixel_size = integer(0)?;
                settings.cell_size = integer(1)?;
                settings.levels = u8::try_from(integer(2)?).context("Tones must be 2–8")?;
                settings.angle = number(3)?;
                settings.diffusion = number(4)? / 100.;
                settings.density = number(5)? / 100.;
                settings.contrast = number(6)? / 100.;
                settings.dark = parse_color(&draft.inputs[7].read(cx).value())?;
                settings.light = parse_color(&draft.inputs[8].read(cx).value())?;
                settings.characters = draft.inputs[9].read(cx).value().to_string();
                Filter::Dither(settings)
            }
        };
        omuse::filters::validate(&filter)?;
        Ok(filter)
    }
    pub(super) fn schedule_finishing_preview(&mut self, cx: &mut Context<Self>) {
        if self.dialog != Dialog::Finishing {
            return;
        }
        let Some(draft) = self.finishing_draft.as_mut() else {
            return;
        };
        draft.cancel.store(true, Ordering::Relaxed);
        draft.job = draft.job.wrapping_add(1);
        draft.computed = None;
        let (job, generation) = (draft.job, self.dialog_generation);
        self.busy = false;
        self.status = "Updating finishing preview…".into();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(140))
                .await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog == Dialog::Finishing
                    && this.dialog_generation == generation
                    && this
                        .finishing_draft
                        .as_ref()
                        .is_some_and(|draft| draft.job == job)
                {
                    this.run_finishing(false, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn run_finishing(&mut self, apply: bool, cx: &mut Context<Self>) {
        if self.dialog != Dialog::Finishing {
            return;
        }
        let filter = match self.finishing_filter(cx) {
            Ok(filter) => filter,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let Some(draft) = self.finishing_draft.as_mut() else {
            return;
        };
        if apply
            && let Some((previous, output)) = &draft.computed
            && *previous == filter
        {
            let output = output.clone();
            self.commit_finishing(filter, output, cx);
            return;
        }
        draft.cancel.store(true, Ordering::Relaxed);
        draft.cancel = Arc::new(AtomicBool::new(false));
        draft.job = draft.job.wrapping_add(1);
        let (job, generation) = (draft.job, self.dialog_generation);
        let cancel = draft.cancel.clone();
        let source = draft.source.clone();
        let layer = draft.layer.clone();
        let selection = draft.selection.clone();
        let document = self.editor.document.clone();
        let operation = filter.clone();
        self.busy = true;
        self.status = if apply {
            "Preparing finishing edit…"
        } else {
            "Rendering finishing preview…"
        }
        .into();
        let task = cx.background_executor().spawn(async move {
            let mut output = (*source).clone();
            omuse::filters::apply_cancellable(&mut output, &operation, &cancel)?;
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Finishing preview cancelled"
            );
            let mut preview_editor = Editor::new(document);
            preview_editor.active_layer = layer;
            preview_editor.selection = selection;
            preview_editor.apply_image_operation(|_| Ok(output.clone()))?;
            let preview = raster::composite(&preview_editor.document);
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Finishing preview cancelled"
            );
            Ok::<_, anyhow::Error>((Arc::new(output), thumbnail(&preview)))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::Finishing || this.dialog_generation != generation ||
                    this.finishing_draft.as_ref().is_none_or(|draft| draft.job != job) { return; }
                this.busy = false;
                if this.finishing_filter(cx).ok().as_ref() != Some(&filter) {
                    this.schedule_finishing_preview(cx); return;
                }
                match result {
                    Err(error) => { this.status = format!("Finishing effect: {error:#}"); cx.notify(); }
                    Ok((output, preview)) => {
                        let draft = this.finishing_draft.as_mut().unwrap();
                        if let Some(old) = draft.preview.replace(render_image(&preview)) { cx.drop_image(old, None); }
                        draft.computed = Some((filter.clone(), output.clone()));
                        if apply { this.commit_finishing(filter, output, cx); }
                        else { this.status = "Preview ready · selected pixels, masks and layer appearance included".into(); cx.notify(); }
                    }
                }
            });
        }).detach();
        cx.notify();
    }
    fn commit_finishing(
        &mut self,
        filter: Filter,
        output: Arc<image::RgbaImage>,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.finishing_draft.as_ref() else {
            return;
        };
        if self.editor.revision() != draft.revision
            || self.editor.active_layer != draft.layer
            || self.editor.selection != draft.selection
            || self.editor.floating_selection_layer().is_some()
            || self
                .editor
                .document
                .find_layer(&draft.layer)
                .and_then(|l| l.image.as_deref())
                != Some(draft.source.as_ref())
        {
            self.status = "The document or selection changed; reopen this finishing draft".into();
            cx.notify();
            return;
        }
        let window = draft.window;
        let title = NAMES[draft.kind];
        let result = self.editor.apply_image_operation(|_| Ok((*output).clone()));
        match result {
            Err(error) => {
                self.status = format!("Finishing edit was not applied: {error:#}");
                cx.notify();
            }
            Ok(changed) => {
                self.clear_finishing(cx);
                self.dialog = Dialog::None;
                self.dialog_generation = self.dialog_generation.wrapping_add(1);
                self.status = if changed {
                    format!("{title} applied · Undo restores the original")
                } else {
                    "No pixel change from these settings".into()
                };
                if changed {
                    self.record_recipe_step(omuse::recipes::Step::Filter { filter });
                    self.changed(cx);
                } else {
                    cx.notify();
                }
                let focus = self.focus.clone();
                cx.defer(move |cx| {
                    let _ = cx.update_window(window, |_, window, cx| focus.focus(window, cx));
                });
            }
        }
    }
    pub(super) fn finishing_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(draft) = self.finishing_draft.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        // Keep the effect choices visible on short windows while retaining the
        // larger artwork preview when the window has room for it.
        let preview_height = (f32::from(window.viewport_size().height) - 440.).clamp(140., 280.);
        let mut body = div().flex().flex_col().gap_3().text_sm();
        let mut tabs = div().flex().flex_wrap().gap_1();
        for (i, name) in NAMES.iter().enumerate() {
            tabs = tabs.child(
                button(
                    SharedString::from(format!("finishing-kind-{i}")),
                    *name,
                    if draft.kind == i {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .debug_selector(move || format!("finishing-kind-{i}").into())
                .on_click(
                    cx.listener(move |this, _, window, cx| this.open_finishing(i, window, cx)),
                ),
            );
        }
        body = body.child(tabs);
        let shown = if draft.show_original {
            draft.original.clone()
        } else {
            draft
                .preview
                .clone()
                .unwrap_or_else(|| draft.original.clone())
        };
        body = body.child(
            div()
                .id("finishing-artwork-preview")
                .debug_selector(|| "finishing-artwork-preview".into())
                .h(px(preview_height))
                .flex_shrink_0()
                .w_full()
                .flex()
                .justify_center()
                .items_center()
                .rounded_md()
                .bg(t.surface)
                .border_1()
                .border_color(t.divider())
                .child(
                    img(shown)
                        .max_w_full()
                        .max_h_full()
                        .object_fit(gpui_kit::ObjectFit::Contain),
                ),
        );
        body = body.child(
            button(
                "finishing-before-after",
                if draft.show_original {
                    "Show effect"
                } else {
                    "Show original"
                },
                ButtonVariant::Outline,
                cx,
            )
            .debug_selector(|| "finishing-before-after".into())
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(draft) = this.finishing_draft.as_mut() {
                    draft.show_original = !draft.show_original;
                }
                cx.notify();
            })),
        );
        if draft.kind == 0 {
            let mut styles = div().flex().flex_wrap().gap_1();
            for (i, style) in Style::ALL.into_iter().enumerate() {
                styles = styles.child(
                    button(
                        SharedString::from(format!("dither-style-{i}")),
                        style.label(),
                        if draft.settings.style == style {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("dither-style-{i}").into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.finishing_draft.as_mut() {
                            draft.settings.style = style;
                        }
                        this.schedule_finishing_preview(cx);
                    })),
                );
            }
            body = body.child(styles);
            let mut palettes = div().flex().flex_wrap().gap_1();
            for (i, palette, label) in [
                (0, Palette::BlackWhite, "Black & white"),
                (1, Palette::TwoColors, "Two colours"),
                (2, Palette::Original, "Original colours"),
            ] {
                palettes = palettes.child(
                    button(
                        SharedString::from(format!("dither-palette-{i}")),
                        label,
                        if draft.settings.palette == palette {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("dither-palette-{i}").into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.finishing_draft.as_mut() {
                            draft.settings.palette = palette;
                        }
                        this.schedule_finishing_preview(cx);
                    })),
                );
            }
            palettes = palettes.child(
                button(
                    "dither-shape",
                    if draft.settings.pixel_shape == PixelShape::Square {
                        "Pixels: square"
                    } else {
                        "Pixels: dot"
                    },
                    ButtonVariant::Outline,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(draft) = this.finishing_draft.as_mut() {
                        draft.settings.pixel_shape =
                            if draft.settings.pixel_shape == PixelShape::Square {
                                PixelShape::Dot
                            } else {
                                PixelShape::Square
                            };
                    }
                    this.schedule_finishing_preview(cx);
                })),
            );
            palettes = palettes.child(
                button(
                    "dither-polarity",
                    if draft.settings.light_on_dark {
                        "Light marks on dark"
                    } else {
                        "Dark marks on light"
                    },
                    ButtonVariant::Outline,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(draft) = this.finishing_draft.as_mut() {
                        draft.settings.light_on_dark = !draft.settings.light_on_dark;
                    }
                    this.schedule_finishing_preview(cx);
                })),
            );
            body = body.child(palettes);
        }
        let mut controls = div().flex().flex_wrap().gap_3();
        for (i, (label, _)) in fields(draft.kind).iter().enumerate() {
            if draft.kind == 0
                && ((i == 1
                    && matches!(
                        draft.settings.style,
                        Style::Atkinson
                            | Style::FloydSteinberg
                            | Style::Bayer2
                            | Style::Bayer4
                            | Style::Bayer8
                    ))
                    || (i == 2
                        && !matches!(
                            draft.settings.style,
                            Style::Atkinson
                                | Style::FloydSteinberg
                                | Style::Bayer2
                                | Style::Bayer4
                                | Style::Bayer8
                        ))
                    || (i == 3
                        && !matches!(
                            draft.settings.style,
                            Style::Dots | Style::Lines | Style::Diamonds
                        ))
                    || (i == 4
                        && !matches!(
                            draft.settings.style,
                            Style::Atkinson | Style::FloydSteinberg
                        ))
                    || ((i == 7 || i == 8) && draft.settings.palette != Palette::TwoColors)
                    || (i == 9 && draft.settings.style != Style::Ascii))
            {
                continue;
            }
            controls = controls.child(
                div()
                    .w(px(if i == 9 { 440. } else { 205. }))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(*label)
                    .child(
                        input(
                            SharedString::from(format!("finishing-field-{i}")),
                            &draft.inputs[i],
                            window,
                            cx,
                        )
                        .debug_selector(move || format!("finishing-field-{i}").into()),
                    ),
            );
        }
        body = body.child(controls).child(match draft.kind {
            1 => "Adds glow within this layer's existing bounds, including transparent margins. Original Bloom recipes keep their alpha-preserving behaviour.",
            2 => "Paints a soft coloured edge, including on an empty transparent paint layer. The current selection limits the overlay.",
            3 => "Uses nearby image detail at the chosen radius. Negative amounts soften detail; positive amounts add local contrast.",
            _ => "Full-resolution effect, shown as a thumbnail. Pixel size uses layer pixels; cell size uses dithered pixels. Source transparency is preserved; ASCII uses a built-in pixel alphabet.",
        });
        body.into_any_element()
    }
}
