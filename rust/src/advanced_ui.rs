//! Draft-based advanced editing workspaces with cancellable background results.
use super::inspector_ui::{colour_swatch, panel_button as button};
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::{
    advanced::{Component, LayerState},
    advanced_ops::*,
    refinement,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Stack,
    Blend,
    Retouch,
    Remove,
    Warp,
    Refine,
    Brush,
}
impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::Stack => "Editable filter stack",
            Self::Blend => "Blend If",
            Self::Retouch => "Frequency & tonal retouch",
            Self::Remove => "Content-aware removal",
            Self::Warp => "Editable mesh & pin warp",
            Self::Refine => "Selection refinement",
            Self::Brush => "Brush studio",
        }
    }
}

pub(super) struct ProDraft {
    window: gpui_kit::AnyWindowHandle,
    kind: Kind,
    layer: String,
    revision: u64,
    state: LayerState,
    placement: Layer,
    selected: Option<usize>,
    effect: usize,
    selection: Option<SoftMask>,
    base_mask: image::GrayImage,
    corrections: Vec<refinement::BrushCorrection>,
    foreground: bool,
    background: usize,
    pins: Vec<WarpPin>,
    pending_pin: Option<[f32; 2]>,
    freeze: bool,
    retouch: usize,
    source: Arc<RenderImage>,
    preview: Option<Arc<RenderImage>>,
    preview_dimensions: (u32, u32),
    reference: Option<omuse::color_match::Statistics>,
    reference_image: Option<Arc<RenderImage>>,
    reference_dimensions: (u32, u32),
    reference_note: String,
    reference_path_open: bool,
    preserve_lightness: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cancel: Arc<AtomicBool>,
    job: u64,
}
impl Drop for ProDraft {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

enum Prepared {
    States(Vec<(String, LayerState)>),
    Layers(Vec<Layer>),
    Refined(refinement::Refined),
    Brush(omuse::brush_dynamics::Settings),
}

const EFFECTS: &[&str] = &[
    "Exposure",
    "Gaussian blur",
    "Unsharp mask",
    "Denoise",
    "Levels",
    "Hue / saturation",
    "Colour balance",
    "Noise",
    "Vignette",
    "Bloom",
    "Tonal contrast",
    "Invert",
    "Grayscale",
    "Curves",
    "Target colour uniformity",
    "Match reference colour",
];
fn effect_fields(kind: usize) -> Vec<(&'static str, &'static str)> {
    match kind {
        0 => vec![("Exposure (stops)", "0.5")],
        1 => vec![("Radius (px)", "4")],
        2 => vec![
            ("Radius (px)", "2"),
            ("Amount", "1"),
            ("Threshold (0–1)", "0.02"),
        ],
        3 => vec![("Radius (1–32 px)", "2"), ("Strength (0–1)", "0.5")],
        4 => vec![("Black (0–1)", "0"), ("White (0–1)", "1"), ("Gamma", "1")],
        5 => vec![
            ("Hue (degrees)", "0"),
            ("Saturation (−1–1)", "0.15"),
            ("Lightness (−1–1)", "0"),
        ],
        6 => vec![
            ("Red (−1–1)", "0"),
            ("Green (−1–1)", "0"),
            ("Blue (−1–1)", "0"),
        ],
        7 => vec![
            ("Amount (0–1)", "0.05"),
            ("Seed", "1"),
            ("Monochrome (0 or 1)", "1"),
        ],
        8 => vec![
            ("Amount (−1–1)", "0.3"),
            ("Midpoint (0–1)", "0.5"),
            ("Feather (0–1)", "0.5"),
        ],
        9 => vec![
            ("Radius (px)", "8"),
            ("Amount", "0.3"),
            ("Threshold (0–1)", "0.7"),
        ],
        10 => vec![
            ("Shadows (−1–1)", "0"),
            ("Midtones (−1–1)", "0.1"),
            ("Highlights (−1–1)", "0"),
        ],
        13 => vec![(
            "Curve points · input:output (0–1)",
            "0:0,0.25:0.2,0.75:0.8,1:1",
        )],
        14 => vec![
            ("Target colour (#RRGGBB)", "#D69A7A"),
            ("Full hue range (0–180°)", "20"),
            ("Hue falloff (0–180°)", "20"),
            ("Hue uniformity (0–1)", "0.25"),
            ("Saturation uniformity (0–1)", "0.5"),
            ("Lightness uniformity (0–1)", "0"),
        ],
        15 => vec![("Match amount (%)", "70")],
        _ => vec![],
    }
}

fn pro_thumbnail(image: &image::RgbaImage) -> image::RgbaImage {
    let scale = (540. / image.width().max(1) as f64)
        .min(210. / image.height().max(1) as f64)
        .min(1.);
    image::imageops::thumbnail(
        image,
        (image.width() as f64 * scale).round().max(1.) as u32,
        (image.height() as f64 * scale).round().max(1.) as u32,
    )
}

impl EditorView {
    pub(super) fn pro_title(&self) -> &'static str {
        self.pro_draft
            .as_ref()
            .map_or("Advanced editing", |d| d.kind.title())
    }
    pub(super) fn clear_pro(&mut self, cx: &mut App) {
        if let Some(mut draft) = self.pro_draft.take() {
            draft.cancel.store(true, Ordering::Relaxed);
            cx.drop_image(draft.source.clone(), None);
            if let Some(p) = draft.preview.take() {
                cx.drop_image(p, None);
            }
            if let Some(p) = draft.reference_image.take() {
                cx.drop_image(p, None);
            }
            self.busy = false;
        }
    }
    pub(super) fn open_pro(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_pro(cx);
        let id = self.editor.active_layer.clone();
        let result = (|| -> Result<ProDraft> {
            let state = self.editor.editable_state(&id)?;
            let placement = self
                .editor
                .document
                .find_layer(&id)
                .context("Choose a pixel layer")?
                .clone();
            let image = state.proxy()?;
            let selection = self.editor.selection_for_layer(&id)?;
            if kind == Kind::Remove {
                ensure!(
                    selection.is_some(),
                    "Select the object to remove before opening this workspace"
                );
            }
            let base_mask = if let Some(selection) = &selection {
                image::GrayImage::from_raw(
                    selection.width,
                    selection.height,
                    selection.data.clone(),
                )
                .unwrap()
            } else if let Some(mask) = &placement.mask {
                image::imageops::resize(
                    &image::DynamicImage::ImageRgba8(mask.to_image()).into_luma8(),
                    image.width(),
                    image.height(),
                    image::imageops::FilterType::Triangle,
                )
            } else {
                image::GrayImage::from_fn(image.width(), image.height(), |x, y| {
                    image::Luma([image.get_pixel(x, y)[3]])
                })
            };
            Ok(ProDraft {
                window: window.window_handle(),
                kind,
                layer: id,
                revision: self.editor.revision(),
                state,
                placement,
                selected: None,
                effect: 0,
                selection,
                base_mask,
                corrections: vec![],
                foreground: true,
                background: 3,
                pins: vec![],
                pending_pin: None,
                freeze: false,
                retouch: 0,
                source: render_image(&pro_thumbnail(&image)),
                preview: None,
                preview_dimensions: image.dimensions(),
                reference: None,
                reference_image: None,
                reference_dimensions: (1, 1),
                reference_note: "Choose a reference image to borrow its colour palette".into(),
                reference_path_open: false,
                preserve_lightness: true,
                bounds: Rc::new(Cell::new(Bounds::default())),
                cancel: Arc::new(AtomicBool::new(false)),
                job: 0,
            })
        })();
        let mut draft = match result {
            Ok(d) => d,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let (w, h) = draft.state.source.dimensions();
        let existing_warp = draft.state.recipe.nodes.iter().find_map(|node| {
            if let AdvancedOperation::Warp(warp) = &node.operation {
                Some(warp.clone())
            } else {
                None
            }
        });
        if kind == Kind::Warp {
            if let Some(warp) = &existing_warp {
                draft.pins = warp.pins.clone();
                draft.freeze = warp.freeze_mask.is_some();
                if warp.freeze_mask.is_some() {
                    draft.selection = warp.freeze_mask.clone();
                }
            }
        }
        let values: Vec<String> = match kind {
            Kind::Stack => effect_fields(0)
                .into_iter()
                .map(|(_, v)| v.into())
                .collect(),
            Kind::Blend => {
                let s = draft.state.recipe.blend_if.unwrap_or_default();
                [
                    s.source.black,
                    s.source.black_split,
                    s.source.white_split,
                    s.source.white,
                    s.backdrop.black,
                    s.backdrop.black_split,
                    s.backdrop.white_split,
                    s.backdrop.white,
                ]
                .map(|v| (v * 255.).to_string())
                .to_vec()
            }
            Kind::Retouch => vec!["4", "0.25", "4"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            Kind::Remove => vec![
                "0".into(),
                "0".into(),
                w.to_string(),
                h.to_string(),
                "32".into(),
                "2".into(),
                "0".into(),
            ],
            Kind::Warp => {
                if let Some(warp) = existing_warp {
                    let mut values = warp
                        .points
                        .iter()
                        .flat_map(|p| p.iter().map(ToString::to_string))
                        .collect::<Vec<_>>();
                    values.extend(["100".into(), "1".into()]);
                    values
                } else {
                    ["0", "0", "1", "0", "0", "1", "1", "1", "100", "1"]
                        .map(str::to_owned)
                        .to_vec()
                }
            }
            Kind::Refine => ["2", "0", "0", "3", "0.5", "24"]
                .map(str::to_owned)
                .to_vec(),
            Kind::Brush => {
                let s = self.editor.brush_dynamics.clone().unwrap_or_default();
                vec![
                    s.size.to_string(),
                    s.flow.to_string(),
                    s.spacing.to_string(),
                    s.hardness.to_string(),
                    s.scatter.to_string(),
                    s.angle.to_string(),
                    s.angle_jitter.to_string(),
                    s.texture_strength.to_string(),
                    s.pressure_curve
                        .iter()
                        .map(|p| format!("{}:{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(","),
                    String::new(),
                ]
            }
        };
        for (i, value) in values.iter().enumerate() {
            self.detail_inputs[i].update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.detail_inputs[30].update(cx, |s, cx| s.set_value("100", window, cx));
        self.detail_inputs[29].update(cx, |s, cx| s.set_value("", window, cx));
        self.pro_draft = Some(draft);
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.dialog = Dialog::Pro;
        self.modal_focus.focus(window, cx);
        self.status=match kind{Kind::Stack=>"Add effects to the draft, preview, then Apply. Originals stay embedded.",Kind::Blend=>"Split each tonal boundary for a soft transition over the actual backdrop.",Kind::Remove=>"Choose an allowed sampling rectangle. Selected target pixels are excluded from sampling.",Kind::Warp=>"Click a source point, then its destination to add a pin. Corner coordinates are normalized.",Kind::Refine=>"Paint corrections on the source preview, choose a background, then preview.",Kind::Brush=>"Wayland tablets supply pressure and tilt. Mouse strokes use constant pressure.",Kind::Retouch=>"Create editable frequency layers or a separate dodge/burn layer."}.into();
        cx.notify();
    }

    fn pro_number(&self, index: usize, cx: &Context<Self>) -> Result<f32> {
        let n = self.detail_inputs[index]
            .read(cx)
            .value()
            .parse::<f32>()
            .context("Enter a numeric value")?;
        ensure!(n.is_finite(), "Values must be finite");
        Ok(n)
    }
    fn pro_integer(&self, index: usize, max: u32, cx: &Context<Self>) -> Result<u32> {
        let n = self.pro_number(index, cx)?;
        ensure!(
            (0. ..=max as f32).contains(&n) && n.fract() == 0.,
            "Enter an integer between 0 and {max}"
        );
        Ok(n as u32)
    }
    fn pro_operation(&self, cx: &Context<Self>) -> Result<AdvancedOperation> {
        let d = self.pro_draft.as_ref().context("Draft closed")?;
        let n = |i| self.pro_number(i, cx);
        Ok(match d.effect {
            0 => AdvancedOperation::Filter(Filter::Exposure { stops: n(0)? }),
            1 => AdvancedOperation::Filter(Filter::GaussianBlur { sigma: n(0)? }),
            2 => AdvancedOperation::Filter(Filter::UnsharpMask {
                sigma: n(0)?,
                amount: n(1)?,
                threshold: n(2)?,
            }),
            3 => AdvancedOperation::Denoise {
                radius: self.pro_integer(0, 32, cx)? as u8,
                strength: n(1)?,
            },
            4 => AdvancedOperation::Filter(Filter::Levels {
                black: n(0)?,
                white: n(1)?,
                gamma: n(2)?,
            }),
            5 => AdvancedOperation::Filter(Filter::Hsl {
                hue_degrees: n(0)?,
                saturation: n(1)?,
                lightness: n(2)?,
            }),
            6 => AdvancedOperation::Filter(Filter::ColorBalance {
                red: n(0)?,
                green: n(1)?,
                blue: n(2)?,
            }),
            7 => AdvancedOperation::Filter(Filter::Noise {
                amount: n(0)?,
                seed: self.pro_integer(1, u32::MAX, cx)? as u64,
                monochrome: self.pro_integer(2, 1, cx)? == 1,
            }),
            8 => AdvancedOperation::Filter(Filter::Vignette {
                amount: n(0)?,
                midpoint: n(1)?,
                feather: n(2)?,
            }),
            9 => AdvancedOperation::Filter(Filter::Bloom {
                sigma: n(0)?,
                amount: n(1)?,
                threshold: n(2)?,
            }),
            10 => AdvancedOperation::Filter(Filter::TonalContrast {
                shadows: n(0)?,
                midtones: n(1)?,
                highlights: n(2)?,
            }),
            11 => AdvancedOperation::Filter(Filter::Invert),
            12 => AdvancedOperation::Filter(Filter::Grayscale),
            13 => AdvancedOperation::Filter(Filter::Curves {
                points: parse_points(&self.detail_inputs[0].read(cx).value())?,
            }),
            14 => AdvancedOperation::TargetColourUniformity(TargetColourUniformity {
                target_rgb: parse_hex_colour(&self.detail_inputs[0].read(cx).value())?,
                hue_range_degrees: n(1)?,
                hue_falloff_degrees: n(2)?,
                hue_uniformity: n(3)?,
                saturation_uniformity: n(4)?,
                lightness_uniformity: n(5)?,
            }),
            15 => AdvancedOperation::ReferenceColourMatch(omuse::color_match::Settings {
                version: 1,
                reference: d
                    .reference
                    .clone()
                    .context("Choose and load a reference image first")?,
                amount: n(0)? / 100.,
                preserve_lightness: d.preserve_lightness,
            }),
            _ => anyhow::bail!("Unknown editable effect"),
        })
    }

    pub(super) fn pro_add_node(&mut self, update: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<()> {
            let operation = self.pro_operation(cx)?;
            let opacity = self.pro_number(30, cx)? / 100.;
            ensure!((0. ..=1.).contains(&opacity), "Opacity must be 0–100%");
            let d = self.pro_draft.as_ref().context("Draft closed")?;
            let mut state = d.state.clone();
            if update {
                let i = d.selected.context("Select an effect to update")?;
                ensure!(
                    matches!(
                        state.recipe.nodes[i].operation,
                        AdvancedOperation::Filter(_)
                            | AdvancedOperation::Denoise { .. }
                            | AdvancedOperation::TargetColourUniformity(_)
                            | AdvancedOperation::ReferenceColourMatch(_)
                    ),
                    "Edit this operation in its dedicated workspace; stack order, enable and masks remain editable here"
                );
                state.recipe.nodes[i].operation = operation;
                state.recipe.nodes[i].opacity = opacity;
                state.recipe.nodes[i].name = EFFECTS[d.effect].into();
            } else {
                state.recipe.nodes.push(FilterNode {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: EFFECTS[d.effect].into(),
                    enabled: true,
                    opacity,
                    operation,
                    soft_mask: None,
                });
            }
            state.validate()?;
            let d = self.pro_draft.as_mut().unwrap();
            d.state = state;
            if !update {
                d.selected = Some(d.state.recipe.nodes.len() - 1);
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.status = "Effect draft updated; preview or Apply when ready".into();
                self.run_pro(false, cx)
            }
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
            }
        }
    }

    pub(super) fn pro_choose_effect(
        &mut self,
        kind: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Some(d) = self.pro_draft.as_mut() {
            d.effect = kind;
        }
        for (i, (_, value)) in effect_fields(kind).iter().enumerate() {
            self.detail_inputs[i].update(cx, |s, cx| s.set_value(*value, window, cx));
        }
        cx.notify();
    }

    fn pro_select_node(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(node) = self
            .pro_draft
            .as_ref()
            .and_then(|d| d.state.recipe.nodes.get(index))
            .cloned()
        else {
            return;
        };
        let (kind, values) = operation_values(&node.operation);
        self.pro_choose_effect(kind, window, cx);
        if let AdvancedOperation::ReferenceColourMatch(settings) = &node.operation {
            let d = self.pro_draft.as_mut().unwrap();
            d.reference = Some(settings.reference.clone());
            d.preserve_lightness = settings.preserve_lightness;
            d.reference_note = format!(
                "Embedded palette · {} visible samples · reference file is not needed",
                settings.reference.samples
            );
            if let Some(previous) = d.reference_image.take() {
                cx.drop_image(previous, None);
            }
        }
        for (i, value) in values.iter().enumerate() {
            self.detail_inputs[i].update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.detail_inputs[30].update(cx, |s, cx| {
            s.set_value((node.opacity * 100.).to_string(), window, cx)
        });
        self.pro_draft.as_mut().unwrap().selected = Some(index);
        cx.notify();
    }

    fn pro_node_action(&mut self, action: &str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(d) = self.pro_draft.as_mut() {
            if let Some(i) = d.selected {
                if i < d.state.recipe.nodes.len() {
                    match action {
                        "toggle" => {
                            d.state.recipe.nodes[i].enabled = !d.state.recipe.nodes[i].enabled
                        }
                        "up" if i > 0 => {
                            d.state.recipe.nodes.swap(i, i - 1);
                            d.selected = Some(i - 1);
                        }
                        "down" if i + 1 < d.state.recipe.nodes.len() => {
                            d.state.recipe.nodes.swap(i, i + 1);
                            d.selected = Some(i + 1);
                        }
                        "remove" => {
                            d.state.recipe.nodes.remove(i);
                            d.selected = None;
                        }
                        "mask" => d.state.recipe.nodes[i].soft_mask = d.selection.clone(),
                        "clear-mask" => d.state.recipe.nodes[i].soft_mask = None,
                        _ => {}
                    }
                }
            }
        }
        cx.notify();
    }

    fn pro_browse_reference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(d) = self
            .pro_draft
            .as_mut()
            .filter(|d| d.kind == Kind::Stack && d.effect == 15)
        else {
            return;
        };
        d.cancel.store(true, Ordering::Relaxed);
        d.job = d.job.wrapping_add(1);
        let job = d.job;
        let revision = d.revision;
        let generation = self.dialog_generation;
        self.busy = true;
        let task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose colour reference".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if this.dialog != Dialog::Pro
                    || this.dialog_generation != generation
                    || this
                        .pro_draft
                        .as_ref()
                        .is_none_or(|d| d.job != job || d.revision != revision)
                {
                    return;
                }
                this.busy = false;
                if this.editor.revision() != revision {
                    this.status = "Document changed; reopen the colour match draft".into();
                } else {
                    match result {
                        Ok(Ok(Some(paths))) if !paths.is_empty() => {
                            this.detail_inputs[29].update(cx, |input, cx| {
                                input.set_value(paths[0].to_string_lossy().to_string(), window, cx)
                            });
                            this.pro_load_reference(paths[0].clone(), cx);
                        }
                        Ok(Err(error)) => {
                            if let Some(draft) = this.pro_draft.as_mut() {
                                draft.reference_path_open = true;
                            }
                            this.status = format!(
                                "File chooser: {error}. Enter a reference path and press Load path."
                            )
                        }
                        _ => {
                            this.status =
                                "Reference selection cancelled; artwork is unchanged".into()
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn pro_load_reference(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(d) = self
            .pro_draft
            .as_mut()
            .filter(|d| d.kind == Kind::Stack && d.effect == 15)
        else {
            return;
        };
        d.cancel.store(true, Ordering::Relaxed);
        d.cancel = Arc::new(AtomicBool::new(false));
        d.job = d.job.wrapping_add(1);
        let job = d.job;
        let revision = d.revision;
        let generation = self.dialog_generation;
        let cancel = d.cancel.clone();
        self.busy = true;
        self.status = "Reading reference colours…".into();
        let task = cx
            .background_executor()
            .spawn(async move { omuse::color_match::load_reference(&path, &cancel) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::Pro || this.dialog_generation != generation
                    || this.pro_draft.as_ref().is_none_or(|d| d.job != job || d.revision != revision) { return; }
                this.busy = false;
                if this.editor.revision() != revision {
                    this.status = "Document changed; reopen the colour match draft".into();
                    cx.notify();
                    return;
                }
                match result {
                    Ok(reference) => {
                        let d = this.pro_draft.as_mut().unwrap();
                        d.reference_note = format!("{} × {} · {} visible samples · {}", reference.dimensions.0, reference.dimensions.1, reference.statistics.samples, if reference.profile_applied { "ICC profile converted to sRGB" } else { "Untagged: assumed sRGB" });
                        d.reference = Some(reference.statistics);
                        d.reference_dimensions = reference.thumbnail.dimensions();
                        if let Some(previous) = d.reference_image.replace(render_image(&reference.thumbnail)) { cx.drop_image(previous, None); }
                        this.status = "Reference ready · Add effect or Update selected effect to preview, then Apply".into();
                    }
                    Err(error) => this.status = format!("Reference image: {error:#}"),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn pro_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let radius = self.pro_number(5, cx).unwrap_or(24.);
        let warp_radius = self.pro_number(8, cx).unwrap_or(100.);
        let strength = self.pro_number(9, cx).unwrap_or(1.);
        if let Some(d) = self.pro_draft.as_mut() {
            let bounds = d.bounds.get();
            if !bounds.contains(&position)
                || bounds.size.width <= px(0.)
                || bounds.size.height <= px(0.)
            {
                return;
            }
            let x = (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width))
                .clamp(0., 0.99999);
            let y = (f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height))
                .clamp(0., 0.99999);
            if d.kind == Kind::Refine && d.corrections.len() < 4096 {
                let (w, h) = d.state.result.dimensions();
                d.corrections.push(refinement::BrushCorrection {
                    x: (x * w as f32) as u32,
                    y: (y * h as f32) as u32,
                    radius,
                    hardness: 0.8,
                    foreground: d.foreground,
                });
                self.status = format!(
                    "{} correction dabs · Preview to inspect",
                    d.corrections.len()
                );
            }
            if d.kind == Kind::Warp {
                if let Some(source) = d.pending_pin.take() {
                    if d.pins.len() < 256 {
                        d.pins.push(WarpPin {
                            source,
                            target: [x, y],
                            radius: warp_radius
                                / d.state.source.width().max(d.state.source.height()) as f32,
                            strength,
                        });
                        self.status = format!("{} pins · Preview to inspect", d.pins.len());
                    }
                } else {
                    d.pending_pin = Some([x, y]);
                    self.status = "Click the destination for this pin".into();
                }
            }
        }
        cx.notify();
    }
}

fn parse_points(value: &str) -> Result<Vec<(f32, f32)>> {
    value
        .split(',')
        .map(|part| {
            let (a, b) = part
                .trim()
                .split_once(':')
                .context("Use input:output pairs separated by commas")?;
            Ok((a.trim().parse()?, b.trim().parse()?))
        })
        .collect()
}

fn parse_hex_colour(value: &str) -> Result<[u8; 3]> {
    let value = value.trim();
    let digits = value.strip_prefix('#').unwrap_or(value);
    ensure!(
        digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Target colour must use #RRGGBB"
    );
    Ok([
        u8::from_str_radix(&digits[0..2], 16)?,
        u8::from_str_radix(&digits[2..4], 16)?,
        u8::from_str_radix(&digits[4..6], 16)?,
    ])
}

fn format_hex_colour(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn operation_values(op: &AdvancedOperation) -> (usize, Vec<String>) {
    let (k, v) = match op {
        AdvancedOperation::Filter(Filter::Exposure { stops }) => (0, vec![*stops]),
        AdvancedOperation::Filter(Filter::GaussianBlur { sigma }) => (1, vec![*sigma]),
        AdvancedOperation::Filter(Filter::UnsharpMask {
            sigma,
            amount,
            threshold,
        }) => (2, vec![*sigma, *amount, *threshold]),
        AdvancedOperation::Denoise { radius, strength } => (3, vec![*radius as f32, *strength]),
        AdvancedOperation::Filter(Filter::Levels {
            black,
            white,
            gamma,
        }) => (4, vec![*black, *white, *gamma]),
        AdvancedOperation::Filter(Filter::Hsl {
            hue_degrees,
            saturation,
            lightness,
        }) => (5, vec![*hue_degrees, *saturation, *lightness]),
        AdvancedOperation::Filter(Filter::ColorBalance { red, green, blue }) => {
            (6, vec![*red, *green, *blue])
        }
        AdvancedOperation::Filter(Filter::Noise {
            amount,
            seed,
            monochrome,
        }) => (
            7,
            vec![*amount, *seed as f32, if *monochrome { 1. } else { 0. }],
        ),
        AdvancedOperation::Filter(Filter::Vignette {
            amount,
            midpoint,
            feather,
        }) => (8, vec![*amount, *midpoint, *feather]),
        AdvancedOperation::Filter(Filter::Bloom {
            sigma,
            amount,
            threshold,
        }) => (9, vec![*sigma, *amount, *threshold]),
        AdvancedOperation::Filter(Filter::TonalContrast {
            shadows,
            midtones,
            highlights,
        }) => (10, vec![*shadows, *midtones, *highlights]),
        AdvancedOperation::Filter(Filter::Invert) => (11, vec![]),
        AdvancedOperation::Filter(Filter::Grayscale) => (12, vec![]),
        AdvancedOperation::Filter(Filter::Curves { points }) => {
            return (
                13,
                vec![
                    points
                        .iter()
                        .map(|(x, y)| format!("{x}:{y}"))
                        .collect::<Vec<_>>()
                        .join(","),
                ],
            );
        }
        AdvancedOperation::TargetColourUniformity(settings) => {
            return (
                14,
                vec![
                    format_hex_colour(settings.target_rgb),
                    settings.hue_range_degrees.to_string(),
                    settings.hue_falloff_degrees.to_string(),
                    settings.hue_uniformity.to_string(),
                    settings.saturation_uniformity.to_string(),
                    settings.lightness_uniformity.to_string(),
                ],
            );
        }
        AdvancedOperation::ReferenceColourMatch(settings) => {
            return (15, vec![(settings.amount * 100.).to_string()]);
        }
        _ => (0, vec![0.]),
    };
    (k, v.iter().map(ToString::to_string).collect())
}

impl EditorView {
    fn prepare_pro(
        &self,
        cx: &Context<Self>,
    ) -> Result<Box<dyn FnOnce() -> Result<Prepared> + Send>> {
        let d = self.pro_draft.as_ref().context("Draft closed")?;
        let mut state = d.state.clone();
        let id = d.layer.clone();
        let cancel = d.cancel.clone();
        let placement = d.placement.clone();
        let n = |i| self.pro_number(i, cx);
        match d.kind {
            Kind::Stack => Ok(Box::new(move || {
                Ok(Prepared::States(vec![(id, state.evaluate(&cancel)?)]))
            })),
            Kind::Blend => {
                let values = (0..8)
                    .map(|i| n(i).map(|v| v / 255.))
                    .collect::<Result<Vec<_>>>()?;
                let settings = BlendIf {
                    source_channel: state.recipe.blend_if.unwrap_or_default().source_channel,
                    backdrop_channel: state.recipe.blend_if.unwrap_or_default().backdrop_channel,
                    source: BlendIfRange {
                        black: values[0],
                        black_split: values[1],
                        white_split: values[2],
                        white: values[3],
                    },
                    backdrop: BlendIfRange {
                        black: values[4],
                        black_split: values[5],
                        white_split: values[6],
                        white: values[7],
                    },
                    ..Default::default()
                };
                validate_blend_if(&settings)?;
                state.recipe.blend_if = Some(settings);
                Ok(Box::new(move || Ok(Prepared::States(vec![(id, state)]))))
            }
            Kind::Retouch => {
                let sigma = n(0)?;
                let amount = n(1)?;
                let radius = self.pro_integer(2, 128, cx)? as u8;
                let mode = d.retouch;
                let mask = d.selection.clone();
                Ok(Box::new(move || {
                    if mode == 0 {
                        state.recipe.component = Component::LowFrequency { sigma };
                        let low = state.evaluate(&cancel)?;
                        state.recipe.component = Component::HighFrequency { sigma };
                        let high = state.evaluate(&cancel)?;
                        ensure!(
                            state.recipe.blend_if.is_none(),
                            "Remove Blend If before frequency separation; it depends on the external backdrop"
                        );
                        let mut lo = derived_layer(&placement, low, "Low frequency · tone")?;
                        let mut hi = derived_layer(&placement, high, "High frequency · texture")?;
                        for child in [&mut lo, &mut hi] {
                            child.opacity = 1.;
                            child.mask = None;
                            if let Some(m) = child.metadata.as_object_mut() {
                                for key in [
                                    "maskEnabled",
                                    "maskLinked",
                                    "maskPlacement",
                                    "maskSourceID",
                                    "maskOutsideCoverage",
                                ] {
                                    m.remove(key);
                                }
                            }
                        }
                        lo.blend_mode = "Normal".into();
                        hi.blend_mode = "Linear Light".into();
                        let mut group = placement.clone();
                        group.id = uuid::Uuid::new_v4().to_string();
                        group.name = format!("{} · frequency separation", placement.name);
                        group.image = None;
                        group.advanced = None;
                        group.metadata["isGroup"] = serde_json::json!(true);
                        let (width, height) = state.source.dimensions();
                        group.metadata["transform"] = serde_json::json!({
                            "origin":[group.offset_x, group.offset_y],
                            "size":[width as f32 * group.scale_x.abs(), height as f32 * group.scale_y.abs()],
                            "rotation":group.rotation,
                            "flipX":group.scale_x < 0., "flipY":group.scale_y < 0.
                        });
                        group.scale_x = group.scale_x.signum();
                        group.scale_y = group.scale_y.signum();
                        if let Some(m) = group.metadata.as_object_mut() {
                            m.remove("rustEditableAsset");
                            m.remove("imageFile");
                        }
                        group.children = vec![lo, hi];
                        Ok(Prepared::Layers(vec![group]))
                    } else {
                        let operation = AdvancedOperation::DodgeBurn(DodgeBurn {
                            mode: if mode == 1 {
                                DodgeBurnMode::Dodge
                            } else {
                                DodgeBurnMode::Burn
                            },
                            amount,
                            radius,
                        });
                        state.recipe.nodes.push(new_node(
                            if mode == 1 { "Dodge" } else { "Burn" },
                            operation,
                            mask,
                        ));
                        let result = state.evaluate(&cancel)?;
                        Ok(Prepared::Layers(vec![derived_layer(
                            &placement,
                            result,
                            if mode == 1 {
                                "Dodge · editable"
                            } else {
                                "Burn · editable"
                            },
                        )?]))
                    }
                }))
            }
            Kind::Remove => {
                let (w, h) = state.source.dimensions();
                let x = self.pro_integer(0, w, cx)?;
                let y = self.pro_integer(1, h, cx)?;
                let width = self.pro_integer(2, w, cx)?;
                let height = self.pro_integer(3, h, cx)?;
                ensure!(
                    width > 0
                        && height > 0
                        && x.checked_add(width).is_some_and(|v| v <= w)
                        && y.checked_add(height).is_some_and(|v| v <= h),
                    "Sampling rectangle must fit the source image"
                );
                let target = d.selection.clone().context("Select the target first")?;
                let search_radius = self.pro_integer(4, 64, cx)?;
                let patch_radius = self.pro_integer(5, 4, cx)? as u8;
                let feather = n(6)?;
                Ok(Box::new(move || {
                    let mut allowed = vec![0; w as usize * h as usize];
                    for py in y..y + height {
                        for px in x..x + width {
                            let i = (py * w + px) as usize;
                            if target.data[i] == 0 {
                                allowed[i] = 255;
                            }
                        }
                    }
                    state.recipe.nodes.push(new_node(
                        "Content-aware removal",
                        AdvancedOperation::ContentAwareReplace(ContentAwareReplace {
                            target_mask: target,
                            allowed_source_mask: SoftMask::new(w, h, allowed)?,
                            search_radius,
                            patch_radius,
                            feather,
                        }),
                        None,
                    ));
                    Ok(Prepared::Layers(vec![derived_layer(
                        &placement,
                        state.evaluate(&cancel)?,
                        "Removal · editable",
                    )?]))
                }))
            }
            Kind::Warp => {
                let points = (0..4)
                    .map(|i| Ok([n(i * 2)?, n(i * 2 + 1)?]))
                    .collect::<Result<Vec<_>>>()?;
                let pins = d.pins.clone();
                let freeze_mask = if d.freeze { d.selection.clone() } else { None };
                let warp = AdvancedOperation::Warp(WarpMesh {
                    columns: 2,
                    rows: 2,
                    points,
                    pins,
                    freeze_mask,
                    protect_mask: None,
                });
                if let Some(index) = state
                    .recipe
                    .nodes
                    .iter()
                    .position(|node| matches!(node.operation, AdvancedOperation::Warp(_)))
                {
                    state.recipe.nodes[index].operation = warp;
                } else {
                    state
                        .recipe
                        .nodes
                        .push(new_node("Mesh & pin warp", warp, None));
                }
                Ok(Box::new(move || {
                    Ok(Prepared::States(vec![(id, state.evaluate(&cancel)?)]))
                }))
            }
            Kind::Refine => {
                let settings = refinement::Settings {
                    matte: omuse::matte::Settings {
                        refine: n(0)?,
                        contrast: n(1)?,
                        shift: n(2)?,
                    },
                    defringe_radius: self.pro_integer(3, 64, cx)? as u8,
                    defringe_strength: n(4)?,
                };
                let corrections = d.corrections.clone();
                let mask = d.base_mask.clone();
                Ok(Box::new(move || {
                    Ok(Prepared::Refined(refinement::refine(
                        &mask,
                        &state.proxy()?,
                        &settings,
                        &corrections,
                        &cancel,
                    )?))
                }))
            }
            Kind::Brush => {
                use omuse::brush_dynamics::{CurvePoint, GrayTip, Settings, Tip};
                let mut settings = Settings {
                    size: n(0)?,
                    flow: n(1)?,
                    spacing: n(2)?,
                    hardness: n(3)?,
                    scatter: n(4)?,
                    angle: n(5)?,
                    angle_jitter: n(6)?,
                    texture_strength: n(7)?,
                    pressure_curve: parse_points(&self.detail_inputs[8].read(cx).value())?
                        .into_iter()
                        .map(|(x, y)| CurvePoint { x, y })
                        .collect(),
                    tip: Tip::Round,
                };
                let path = self.detail_inputs[9].read(cx).value().to_string();
                let previous_tip = self.editor.brush_dynamics.as_ref().map(|s| s.tip.clone());
                Ok(Box::new(move || {
                    if path.trim().is_empty() {
                        if let Some(tip) = previous_tip {
                            settings.tip = tip;
                        }
                    } else {
                        let mut reader =
                            image::ImageReader::open(path.trim())?.with_guessed_format()?;
                        let mut limits = image::Limits::default();
                        limits.max_image_width = Some(512);
                        limits.max_image_height = Some(512);
                        limits.max_alloc = Some(2 * 1024 * 1024);
                        reader.limits(limits);
                        let tip = reader.decode()?.to_rgba8();
                        let pixels = tip
                            .pixels()
                            .map(|p| {
                                let l = (u32::from(p[0]) * 54
                                    + u32::from(p[1]) * 183
                                    + u32::from(p[2]) * 19)
                                    / 256;
                                (l * u32::from(p[3]) / 255) as u8
                            })
                            .collect();
                        settings.tip = Tip::Custom(GrayTip {
                            width: tip.width(),
                            height: tip.height(),
                            pixels,
                        });
                    }
                    settings.validate()?;
                    Ok(Prepared::Brush(settings))
                }))
            }
        }
    }

    pub(super) fn run_pro(&mut self, apply: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(d) = self.pro_draft.as_mut() {
            d.cancel.store(true, Ordering::Relaxed);
            d.cancel = Arc::new(AtomicBool::new(false));
            d.job = d.job.wrapping_add(1);
        }
        let work = match self.prepare_pro(cx) {
            Ok(w) => w,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let Some(d) = self.pro_draft.as_ref() else {
            return;
        };
        let job = d.job;
        let window = d.window;
        let generation = self.dialog_generation;
        let revision = d.revision;
        let layer = d.layer.clone();
        let kind = d.kind;
        let background = d.background;
        let cancel = d.cancel.clone();
        let document = self.editor.document.clone();
        let placement = d.placement.clone();
        let source_id = d.layer.clone();
        self.busy = true;
        self.status = if apply {
            "Preparing edit…"
        } else {
            "Rendering preview…"
        }
        .into();
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let prepared = work()?;
            ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
            let preview = match &prepared {
                Prepared::States(states) => {
                    let mut editor = Editor::new(document);
                    editor.replace_editable_states(states.clone())?;
                    raster::composite(&editor.document)
                }
                Prepared::Layers(layers) => {
                    let mut editor = Editor::new(document);
                    editor.insert_derived_layers(&source_id, layers.clone())?;
                    raster::composite(&editor.document)
                }
                Prepared::Refined(result) => refinement::display_preview(
                    result,
                    placement
                        .image
                        .as_deref()
                        .context("Source pixels unavailable")?,
                    match background {
                        0 => refinement::PreviewBackground::Original,
                        1 => refinement::PreviewBackground::Black,
                        2 => refinement::PreviewBackground::White,
                        3 => refinement::PreviewBackground::Checkerboard,
                        _ => refinement::PreviewBackground::Mask,
                    },
                )?,
                Prepared::Brush(settings) => brush_preview(settings)?,
            };
            ensure!(!cancel.load(Ordering::Relaxed), "Edit cancelled");
            let dimensions = preview.dimensions();
            Ok::<_, anyhow::Error>((prepared, pro_thumbnail(&preview), dimensions))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog_generation != generation
                    || this.dialog != Dialog::Pro
                    || this.pro_draft.as_ref().is_none_or(|d| d.job != job)
                {
                    return;
                }
                this.busy = false;
                if this.editor.revision() != revision {
                    this.status = "Document changed; reopen this draft before applying".into();
                    cx.notify();
                    return;
                }
                match result {
                    Err(error) => {
                        this.status = format!("{}: {error:#}", kind.title());
                        cx.notify();
                    }
                    Ok((prepared, pixels, dimensions)) => {
                        if !apply {
                            if let Some(d) = this.pro_draft.as_mut() {
                                d.preview_dimensions = dimensions;
                                if let Some(previous) = d.preview.replace(render_image(&pixels)) {
                                    cx.drop_image(previous, None);
                                }
                            }
                            this.status = "Preview ready · source is unchanged".into();
                            cx.notify();
                            return;
                        }
                        let committed = (|| -> Result<()> {
                            match prepared {
                                Prepared::States(states) => {
                                    this.editor.replace_editable_states(states)?;
                                }
                                Prepared::Layers(layers) => {
                                    this.editor.insert_derived_layers(&layer, layers)?;
                                }
                                Prepared::Refined(result) => {
                                    let mut refined_layer = this
                                        .editor
                                        .document
                                        .find_layer(&layer)
                                        .context("Layer missing")?
                                        .clone();
                                    refined_layer.name =
                                        format!("{} · refined", refined_layer.name);
                                    refined_layer.mask = None;
                                    refined_layer.advanced = None;
                                    refined_layer.children.clear();
                                    objects::detach_live_object(&mut refined_layer);
                                    if let Some(m) = refined_layer.metadata.as_object_mut() {
                                        for key in [
                                            "maskSourceID",
                                            "maskPlacement",
                                            "maskOutsideCoverage",
                                            "maskEnabled",
                                            "maskLinked",
                                            "rustEditableAsset",
                                            "effects",
                                        ] {
                                            m.remove(key);
                                        }
                                    }
                                    refined_layer.image = Some(result.image.into());
                                    this.editor
                                        .insert_derived_layers(&layer, vec![refined_layer])?;
                                }
                                Prepared::Brush(settings) => {
                                    persist_brush(&settings)?;
                                    this.editor.brush.size = settings.size;
                                    this.editor.brush.hardness = settings.hardness;
                                    this.editor.set_brush_dynamics(Some(settings.clone()))?;
                                }
                            }
                            Ok(())
                        })();
                        match committed {
                            Ok(()) => {
                                this.clear_pro(cx);
                                this.dialog = Dialog::None;
                                this.dialog_generation = this.dialog_generation.wrapping_add(1);
                                let focus = this.focus.clone();
                                cx.defer(move |cx| {
                                    let _ = cx.update_window(window, |_, window, cx| {
                                        focus.focus(window, cx)
                                    });
                                });
                                this.status = format!("{} applied", kind.title());
                                if kind == Kind::Brush {
                                    this.refresh(cx);
                                } else {
                                    this.changed(cx);
                                }
                            }
                            Err(error) => {
                                this.status = format!("Edit was not applied: {error:#}");
                                cx.notify();
                            }
                        }
                    }
                }
            });
        })
        .detach();
    }
}

fn new_node(name: &str, operation: AdvancedOperation, soft_mask: Option<SoftMask>) -> FilterNode {
    FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        enabled: true,
        opacity: 1.,
        operation,
        soft_mask,
    }
}
fn derived_layer(placement: &Layer, state: LayerState, name: &str) -> Result<Layer> {
    let mut layer = placement.clone();
    layer.name = name.into();
    layer.children.clear();
    objects::detach_live_object(&mut layer);
    if let Some(m) = layer.metadata.as_object_mut() {
        for key in ["maskSourceID", "effects", "rustEditableAsset"] {
            m.remove(key);
        }
    }
    layer.image = Some(state.proxy()?.into());
    layer.advanced = Some(Arc::new(state));
    Ok(layer)
}
fn brush_preview(settings: &omuse::brush_dynamics::Settings) -> Result<image::RgbaImage> {
    use omuse::brush_dynamics::{InputPoint, StrokeGenerator};
    let mut scaled = settings.clone();
    scaled.size = scaled.size.min(100.);
    let generator = StrokeGenerator::new(scaled, 1)?;
    let points = (0..90)
        .map(|i| {
            let x = i as f32 / 89.;
            InputPoint {
                x: 20. + x * 480.,
                y: 90. + (x * 6.).sin() * 35.,
                pressure: 0.1 + x * 0.9,
                tilt_x: 0.,
                tilt_y: 0.,
            }
        })
        .collect::<Vec<_>>();
    let dabs = generator.generate(&points, || false)?;
    let mut out = image::RgbaImage::from_pixel(540, 180, image::Rgba([28, 30, 39, 255]));
    for dab in dabs {
        let r = dab.size / 2.;
        for y in (dab.y - r).max(0.) as u32..(dab.y + r).min(180.) as u32 {
            for x in (dab.x - r).max(0.) as u32..(dab.x + r).min(540.) as u32 {
                let amount = generator.sample_tip(
                    &dab,
                    (x as f32 + 0.5 - dab.x) / r,
                    (y as f32 + 0.5 - dab.y) / r,
                ) * dab.opacity;
                let p = out.get_pixel_mut(x, y);
                for (c, target) in [130., 173., 247.].iter().enumerate() {
                    p[c] = (p[c] as f32 + (*target - p[c] as f32) * amount).round() as u8;
                }
            }
        }
    }
    Ok(out)
}
fn brush_path() -> PathBuf {
    omuse::identity::config_file_path("brush.json")
}
fn persist_brush(settings: &omuse::brush_dynamics::Settings) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    settings.validate()?;
    let path = brush_path();
    let parent = path.parent().context("Missing brush settings directory")?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".brush-{}.tmp", uuid::Uuid::new_v4()));
    use std::io::Write;
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec(settings)?)?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
pub(super) fn current_brush() -> Option<omuse::brush_dynamics::Settings> {
    if cfg!(test) {
        return None;
    }
    use std::io::Read;
    let file = std::fs::File::open(brush_path()).ok()?;
    if !file.metadata().ok()?.is_file() || file.metadata().ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    let settings: omuse::brush_dynamics::Settings = serde_json::from_slice(&bytes).ok()?;
    settings.validate().ok()?;
    Some(settings)
}

impl EditorView {
    pub(super) fn pro_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(d) = self.pro_draft.as_ref() else {
            return div().into_any_element();
        };
        let kind = d.kind;
        let effect = d.effect;
        let selected = d.selected;
        let nodes = d.state.recipe.nodes.clone();
        let dims = d.state.source.dimensions();
        let source = d.source.clone();
        let preview = d.preview.clone();
        let preview_dims = d.preview_dimensions;
        let reference_image = d.reference_image.clone();
        let reference_dimensions = d.reference_dimensions;
        let reference_note = d.reference_note.clone();
        let reference_path_open = d.reference_path_open;
        let preserve_lightness = d.preserve_lightness;
        let bounds = d.bounds.clone();
        let corrections = d.corrections.clone();
        let pins = d.pins.clone();
        let pending_pin = d.pending_pin;
        let foreground = d.foreground;
        let background = d.background;
        let freeze = d.freeze;
        let retouch = d.retouch;
        let overlay_bounds = bounds.clone();
        let source_view = range_ui::range_image("pro-source", Some(source), dims, Some(bounds))
            .relative()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.pro_pointer(event.position, cx)
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left)
                    && this
                        .pro_draft
                        .as_ref()
                        .is_some_and(|d| d.kind == Kind::Refine)
                {
                    this.pro_pointer(event.position, cx);
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let b = overlay_bounds.get();
                        let sx = f32::from(b.size.width) / dims.0 as f32;
                        let sy = f32::from(b.size.height) / dims.1 as f32;
                        for correction in corrections.iter().rev().take(64) {
                            let r = (correction.radius * sx).clamp(2., 24.);
                            let p = point(
                                b.origin.x + px(correction.x as f32 * sx - r),
                                b.origin.y + px(correction.y as f32 * sy - r),
                            );
                            window.paint_quad(
                                outline(
                                    Bounds::new(p, size(px(r * 2.), px(r * 2.))),
                                    if correction.foreground {
                                        rgba(0x9ece6acc)
                                    } else {
                                        rgba(0xf7768ecc)
                                    },
                                    BorderStyle::Solid,
                                )
                                .corner_radii(px(r)),
                            );
                        }
                        for p in pins
                            .iter()
                            .flat_map(|p| [p.source, p.target])
                            .chain(pending_pin)
                        {
                            let point = point(
                                b.origin.x + b.size.width * p[0] - px(3.),
                                b.origin.y + b.size.height * p[1] - px(3.),
                            );
                            window.paint_quad(
                                outline(
                                    Bounds::new(point, size(px(6.), px(6.))),
                                    rgb(0x7aa2f7),
                                    BorderStyle::Solid,
                                )
                                .corner_radii(px(3.)),
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        let previews = div()
            .w(px(220.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().child("Source layer").child(source_view))
            .child(div().child("Result preview").child(range_ui::range_image(
                "pro-preview",
                preview,
                preview_dims,
                None,
            )))
            .child(
                button(
                    "pro-preview-button",
                    "Refresh preview",
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(|| "pro-preview-button".into())
                .on_click(cx.listener(|this, _, _, cx| this.run_pro(false, cx))),
            );
        let mut body = div().min_w_0().flex().flex_col().gap_3();
        if kind == Kind::Blend {
            let settings = self
                .pro_draft
                .as_ref()
                .unwrap()
                .state
                .recipe
                .blend_if
                .unwrap_or_default();
            for (backdrop, title, selected_channel) in [
                (false, "This layer channel", settings.source_channel),
                (true, "Backdrop channel", settings.backdrop_channel),
            ] {
                let mut row = div().flex().flex_wrap().gap_1().child(title);
                for (index, label, channel) in [
                    (0, "Luminance", BlendIfChannel::Luminance),
                    (1, "Red", BlendIfChannel::Red),
                    (2, "Green", BlendIfChannel::Green),
                    (3, "Blue", BlendIfChannel::Blue),
                ] {
                    let id = format!(
                        "pro-blend-{}-{index}",
                        if backdrop { "backdrop" } else { "source" }
                    );
                    let selector = id.clone();
                    row = row.child(
                        button(
                            SharedString::from(id),
                            label,
                            if selected_channel == channel {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .debug_selector(move || selector.clone().into())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(draft) = this.pro_draft.as_mut() {
                                let settings = draft
                                    .state
                                    .recipe
                                    .blend_if
                                    .get_or_insert_with(Default::default);
                                if backdrop {
                                    settings.backdrop_channel = channel;
                                } else {
                                    settings.source_channel = channel;
                                }
                            }
                            cx.notify();
                        })),
                    );
                }
                body = body.child(row);
            }
        }
        if kind == Kind::Stack {
            let mut list = div().flex().flex_col().gap_1();
            for (index, node) in nodes.iter().enumerate() {
                let label = format!(
                    "{} {} · {:.0}%{}",
                    if node.enabled { "●" } else { "○" },
                    node.name,
                    node.opacity * 100.,
                    if node.soft_mask.is_some() {
                        " · masked"
                    } else {
                        ""
                    }
                );
                list = list.child(
                    button(
                        SharedString::from(format!("pro-node-{index}")),
                        SharedString::from(label),
                        if selected == Some(index) {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("pro-node-{index}").into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pro_select_node(index, window, cx)
                    })),
                );
            }
            body = body.child(list);
            let mut actions = div().flex().flex_wrap().gap_2();
            for (action, label) in [
                ("toggle", "Enable / disable"),
                ("up", "Move up"),
                ("down", "Move down"),
                ("mask", "Use selection mask"),
                ("clear-mask", "Clear mask"),
                ("remove", "Remove"),
            ] {
                actions = actions.child(
                    button(
                        SharedString::from(format!("pro-node-{action}")),
                        label,
                        ButtonVariant::Outline,
                        cx,
                    )
                    .debug_selector(move || format!("pro-node-{action}").into())
                    .on_click(cx.listener(move |this, _, _, cx| this.pro_node_action(action, cx))),
                );
            }
            body = body.child(actions);
            let mut effects = div().flex().flex_wrap().gap_1();
            for (index, label) in EFFECTS.iter().enumerate() {
                effects = effects.child(
                    button(
                        SharedString::from(format!("pro-effect-{index}")),
                        *label,
                        if effect == index {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("pro-effect-{index}").into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pro_choose_effect(index, window, cx)
                    })),
                );
            }
            body = body.child(effects);
            if effect == 15 {
                body = body
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                button(
                                    "pro-reference-browse",
                                    "Choose reference image…",
                                    ButtonVariant::Primary,
                                    cx,
                                )
                                .disabled(self.busy)
                                .debug_selector(|| "pro-reference-browse".into())
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.pro_browse_reference(window, cx),
                                )),
                            )
                            .child(
                                button(
                                    "pro-reference-lightness",
                                    if preserve_lightness {
                                        "✓ Preserve lightness"
                                    } else {
                                        "Match reference lightness"
                                    },
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .disabled(self.busy)
                                .debug_selector(|| "pro-reference-lightness".into())
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if !this.busy {
                                            if let Some(d) = this.pro_draft.as_mut() {
                                                d.preserve_lightness = !d.preserve_lightness;
                                            }
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(reference_note)
                    .child(inspector_ui::panel_note(
                        "Borrow the overall palette. Similar subjects work best.",
                        cx,
                    ))
                    .child(
                        button(
                            "pro-reference-toggle-path",
                            if reference_path_open {
                                "Hide file path"
                            } else {
                                "Enter a file path…"
                            },
                            ButtonVariant::Outline,
                            cx,
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(draft) = this.pro_draft.as_mut() {
                                draft.reference_path_open = !draft.reference_path_open;
                            }
                            cx.notify();
                        })),
                    );
                if reference_path_open {
                    body = body
                        .child(input(
                            "pro-reference-path",
                            &self.detail_inputs[29],
                            window,
                            cx,
                        ))
                        .child(
                            button(
                                "pro-reference-load-path",
                                "Load path",
                                ButtonVariant::Outline,
                                cx,
                            )
                            .disabled(self.busy)
                            .debug_selector(|| "pro-reference-load-path".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let path =
                                    PathBuf::from(this.detail_inputs[29].read(cx).value().as_ref());
                                this.pro_load_reference(path, cx);
                            })),
                        );
                }
                if reference_image.is_some() {
                    body = body.child(range_ui::range_image(
                        "pro-reference-image",
                        reference_image,
                        reference_dimensions,
                        None,
                    ));
                }
            }
        }
        if kind == Kind::Retouch {
            let mut choices = div().flex().flex_wrap().gap_2();
            for (index, label) in ["Frequency layers", "Dodge layer", "Burn layer"]
                .iter()
                .enumerate()
            {
                choices = choices.child(
                    button(
                        SharedString::from(format!("pro-retouch-{index}")),
                        *label,
                        if retouch == index {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("pro-retouch-{index}").into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.pro_draft.as_mut() {
                            d.retouch = index;
                        }
                        cx.notify();
                    })),
                );
            }
            body=body.child(choices).child("Frequency separation creates 16-bit tone and texture layers. Paint on a rasterized copy to retain the original recipe.");
            body = body.child(button("pro-healing-layer", "New healing layer", ButtonVariant::Outline, cx)
                .on_click(cx.listener(|this, _, window, cx| {
                    let source = this.editor.active_layer.clone();
                    let id = this.editor.add_layer("Healing · all layers");
                    if id == source { this.status = "Unable to create a healing layer".into(); cx.notify(); return; }
                    this.tool = Tool::Heal;
                    this.clone_all_layers = true;
                    this.clone_source = None;
                    this.paint_mask = false;
                    this.clear_pro(cx);
                    this.dialog = Dialog::None;
                    this.focus.focus(window, cx);
                    this.status = "Healing layer ready · Alt-click to sample, then paint. The canvas supplies source and destination tone.".into();
                    this.changed(cx);
                })));
        }
        if kind == Kind::Refine {
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        button(
                            "pro-correct-foreground",
                            "Paint foreground",
                            if foreground {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .debug_selector(|| "pro-correct-foreground".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(d) = this.pro_draft.as_mut() {
                                d.foreground = true;
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        button(
                            "pro-correct-background",
                            "Paint background",
                            if !foreground {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .debug_selector(|| "pro-correct-background".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(d) = this.pro_draft.as_mut() {
                                d.foreground = false;
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        button(
                            "pro-correct-clear",
                            "Clear corrections",
                            ButtonVariant::Outline,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(d) = this.pro_draft.as_mut() {
                                d.corrections.clear();
                            }
                            cx.notify();
                        })),
                    ),
            );
            let mut backgrounds = div().flex().flex_wrap().gap_1();
            for (index, label) in ["Original", "Black", "White", "Checkerboard", "Mask"]
                .iter()
                .enumerate()
            {
                backgrounds = backgrounds.child(
                    button(
                        SharedString::from(format!("pro-background-{index}")),
                        *label,
                        if background == index {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                        cx,
                    )
                    .debug_selector(move || format!("pro-background-{index}").into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.pro_draft.as_mut() {
                            d.background = index;
                        }
                        this.run_pro(false, cx);
                    })),
                );
            }
            body=body.child(backgrounds).child("Apply creates a refined cut-out on a separate layer; the source and its mask remain available.");
        }
        if kind == Kind::Warp {
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        button(
                            "pro-freeze",
                            if freeze {
                                "Selection frozen"
                            } else {
                                "Freeze selection"
                            },
                            if freeze {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .debug_selector(|| "pro-freeze".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(d) = this.pro_draft.as_mut() {
                                d.freeze = !d.freeze;
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        button("pro-clear-pins", "Clear pins", ButtonVariant::Outline, cx)
                            .debug_selector(|| "pro-clear-pins".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.pro_draft.as_mut() {
                                    d.pins.clear();
                                    d.pending_pin = None;
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let labels: Vec<&str> = match kind {
            Kind::Stack => effect_fields(effect)
                .iter()
                .map(|(label, _)| *label)
                .collect(),
            Kind::Blend => vec![
                "This layer · black",
                "This layer · black split",
                "This layer · white split",
                "This layer · white",
                "Backdrop · black",
                "Backdrop · black split",
                "Backdrop · white split",
                "Backdrop · white",
            ],
            Kind::Retouch => vec![
                "Frequency blur radius (px)",
                "Dodge / burn strength (0–1)",
                "Local tonal radius (px)",
            ],
            Kind::Remove => vec![
                "Sampling rectangle X",
                "Sampling rectangle Y",
                "Sampling width",
                "Sampling height",
                "Search radius (1–64 px)",
                "Patch radius (0–4 px)",
                "Feather (0–1)",
            ],
            Kind::Warp => vec![
                "Top left X",
                "Top left Y",
                "Top right X",
                "Top right Y",
                "Bottom left X",
                "Bottom left Y",
                "Bottom right X",
                "Bottom right Y",
                "New pin radius (px)",
                "New pin strength (0–1)",
            ],
            Kind::Refine => vec![
                "Edge refinement (0–100 px)",
                "Contrast (0–100)",
                "Edge shift (−100–100)",
                "Defringe radius (0–64 px)",
                "Defringe strength (0–1)",
                "Correction brush radius (px)",
            ],
            Kind::Brush => vec![
                "Size (px)",
                "Flow (0–1)",
                "Spacing (diameters)",
                "Hardness (0–1)",
                "Scatter (diameters)",
                "Angle (degrees)",
                "Angle jitter (degrees)",
                "Texture strength (0–1)",
                "Pressure curve · input:output pairs",
                "Custom tip image (optional, ≤512 px)",
            ],
        };
        let mut fields = div().flex().flex_wrap().gap_2();
        for (i, label) in labels.iter().enumerate() {
            let selector = format!("pro-field-{i}");
            fields = fields.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone().into())
                    .w(px(210.))
                    .min_w_0()
                    .child(*label)
                    .child({
                        let mut field = input(
                            SharedString::from(format!("pro-input-{i}")),
                            &self.detail_inputs[i],
                            window,
                            cx,
                        );
                        if kind == Kind::Stack && effect == 14 && i == 0 {
                            field = field.prefix(colour_swatch(
                                parse_hex_colour(self.detail_inputs[i].read(cx).value().as_ref())
                                    .ok()
                                    .map(|rgb| [rgb[0], rgb[1], rgb[2], 255]),
                                false,
                                cx,
                            ));
                        }
                        field.debug_selector(move || format!("pro-input-{i}").into())
                    }),
            );
        }
        body = body.child(fields);
        let mut stack_actions = None;
        if kind == Kind::Stack {
            body = body.child(div().child("Effect opacity (%)").child(input(
                "pro-opacity",
                &self.detail_inputs[30],
                window,
                cx,
            )));
            stack_actions = Some(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        button("pro-add-effect", "Add effect", ButtonVariant::Primary, cx)
                            .debug_selector(|| "pro-add-effect".into())
                            .on_click(cx.listener(|this, _, _, cx| this.pro_add_node(false, cx))),
                    )
                    .child(
                        button(
                            "pro-update-effect",
                            "Update selected effect",
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "pro-update-effect".into())
                        .on_click(cx.listener(|this, _, _, cx| this.pro_add_node(true, cx))),
                    ),
            );
        }
        if kind == Kind::Brush {
            body=body.child("The preview demonstrates a pressure ramp. Mouse strokes use constant pressure; no tablet readings are simulated in artwork.").child(button("pro-brush-basic","Use basic brush",ButtonVariant::Outline,cx).on_click(cx.listener(|this,_,window,cx|{
                if !cfg!(test) {
                    if let Err(error) = std::fs::remove_file(brush_path()) {
                        if error.kind() != std::io::ErrorKind::NotFound { this.status = format!("Brush settings: {error}"); cx.notify(); return; }
                    }
                }
                this.editor.brush_dynamics=None;this.clear_pro(cx);this.dialog=Dialog::None;this.focus.focus(window,cx);this.status="Basic brush selected".into();cx.notify();
            })));
        }
        div()
            .flex()
            .gap_4()
            .h(px(
                (f32::from(window.viewport_size().height) - 240.).clamp(340., 580.)
            ))
            .flex_shrink_0()
            .child(previews)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .id("pro-settings")
                            .debug_selector(|| "pro-settings".into())
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(body),
                    )
                    .children(stack_actions),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod target_colour_ui_tests {
    use super::*;

    #[test]
    fn target_colour_fields_round_trip_hex_and_label_every_parameter() {
        assert_eq!(parse_hex_colour("#0a7BCf").unwrap(), [10, 123, 207]);
        assert_eq!(parse_hex_colour("0A7BCF").unwrap(), [10, 123, 207]);
        assert!(parse_hex_colour("#abc").is_err());
        assert!(parse_hex_colour("#GG0000").is_err());
        let settings = TargetColourUniformity {
            target_rgb: [10, 123, 207],
            hue_range_degrees: 12.,
            hue_falloff_degrees: 18.,
            hue_uniformity: 0.25,
            saturation_uniformity: 0.5,
            lightness_uniformity: 0.75,
        };
        let (kind, values) = operation_values(&AdvancedOperation::TargetColourUniformity(settings));
        assert_eq!(kind, 14);
        assert_eq!(values, ["#0A7BCF", "12", "18", "0.25", "0.5", "0.75"]);
        let fields = effect_fields(kind);
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0].0, "Target colour (#RRGGBB)");
        assert!(fields.iter().any(|(label, _)| label.contains("Lightness")));
    }
}

#[cfg(all(test, feature = "ui-test"))]
#[path = "advanced_tests.rs"]
mod tests;
