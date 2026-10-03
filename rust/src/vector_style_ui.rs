//! Paint controls remain in the shared canvas inspector and use scene undo.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_scene::{
    GradientFill, GradientKind, GradientSpread, GradientStop, StrokeCap, StrokeJoin, StrokeOptions,
};

pub(super) struct PendingPaint {
    pub(super) gradient: Option<GradientFill>,
    pub(super) stroke: Option<StrokeOptions>,
    pub(super) selected_stop: usize,
}

impl EditorView {
    pub(super) fn pending_vector_paints(&self, cx: &Context<Self>) -> Result<Option<PendingPaint>> {
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return Ok(None);
        };
        let stop_index = scene.gradient_stop;
        let mut selected_stop = stop_index;
        let active = &scene.artwork.objects[scene.active];
        let mut edited_gradient = active.fill_gradient.clone();
        let mut edited_stroke = active.stroke_options.clone().unwrap_or_default();
        let number = |index: usize| -> Result<f32> {
            let value: f32 = self.detail_inputs[index]
                .read(cx)
                .value()
                .trim()
                .parse()
                .context("Enter a valid paint number")?;
            ensure!(value.is_finite(), "Paint values must be finite");
            Ok(value)
        };
        if let Some(gradient) = &mut edited_gradient {
            gradient.kind = match gradient.kind {
                GradientKind::Linear { .. } => GradientKind::Linear {
                    start: VectorPoint {
                        x: number(8)?,
                        y: number(9)?,
                    },
                    end: VectorPoint {
                        x: number(10)?,
                        y: number(11)?,
                    },
                },
                GradientKind::Radial { .. } => GradientKind::Radial {
                    center: VectorPoint {
                        x: number(8)?,
                        y: number(9)?,
                    },
                    focus: VectorPoint {
                        x: number(10)?,
                        y: number(11)?,
                    },
                    radius: number(12)?,
                },
            };
            let index = stop_index.min(gradient.stops.len() - 1);
            let percent = number(13)?;
            let offset = if percent == gradient.stops[index].offset * 100. {
                gradient.stops[index].offset
            } else {
                percent / 100.
            };
            ensure!((0. ..=1.).contains(&offset), "Stop position must be 0–100%");
            gradient.stops[index] = GradientStop {
                offset,
                color: {
                    let edited = parse_vector_colour(&self.detail_inputs[14].read(cx).value())?;
                    if scene.gradient_stop_loaded_color == Some(edited) {
                        gradient.stops[index].color
                    } else {
                        edited
                    }
                },
            };
            let edited = gradient.stops[index];
            gradient.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            selected_stop = gradient
                .stops
                .iter()
                .position(|stop| *stop == edited)
                .unwrap_or(index);
            gradient.validate()?;
        }
        if active.stroke.is_some() {
            edited_stroke.dashes = self.detail_inputs[15]
                .read(cx)
                .value()
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(str::parse::<f32>)
                .collect::<std::result::Result<_, _>>()
                .context("Use comma-separated dash and gap lengths")?;
            edited_stroke.dash_offset = number(16)?;
            edited_stroke.miter_limit = number(17)?;
            edited_stroke.validate()?;
        }

        let options = if active.stroke.is_none()
            || (active.stroke_options.is_none() && edited_stroke == StrokeOptions::default())
        {
            None
        } else {
            Some(edited_stroke)
        };
        Ok(Some(PendingPaint {
            gradient: edited_gradient,
            stroke: options,
            selected_stop,
        }))
    }

    pub(super) fn load_advanced_vector_style(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return;
        };
        let object = &scene.artwork.objects[scene.active];
        let mut values = vec![String::new(); 10];
        if let Some(gradient) = &object.fill_gradient {
            scene.gradient_stop = scene.gradient_stop.min(gradient.stops.len() - 1);
            let coordinates = match gradient.kind {
                GradientKind::Linear { start, end } => [start.x, start.y, end.x, end.y, 0.],
                GradientKind::Radial {
                    center,
                    focus,
                    radius,
                } => [center.x, center.y, focus.x, focus.y, radius],
            };
            for (value, coordinate) in values.iter_mut().zip(coordinates) {
                *value = coordinate.to_string();
            }
            let stop = &gradient.stops[scene.gradient_stop];
            scene.gradient_stop_loaded_color = Some(stop.color);
            values[5] = (stop.offset * 100.).to_string();
            values[6] = format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                stop.color[0], stop.color[1], stop.color[2], stop.color[3]
            );
        }
        let options = object.stroke_options.clone().unwrap_or_default();
        values[7] = options
            .dashes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        values[8] = options.dash_offset.to_string();
        values[9] = options.miter_limit.to_string();
        for (index, value) in values.into_iter().take(10).enumerate() {
            self.detail_inputs[8 + index]
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    fn edit_scene_style(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        ensure!(!self.busy, "Wait for the current operation");
        self.update_vector_style(cx)?;
        if action == "apply" {
            self.load_scene_style(window, cx);
            self.status = "Paint updated for selected objects · Ctrl+Z to undo".into();
            return Ok(());
        }
        let draft = self
            .vector_draft
            .as_ref()
            .context("Open vector artwork first")?;
        let scene = draft.scene.as_ref().context("Open vector artwork first")?;
        let selected = scene.selected_objects.clone();
        ensure!(!selected.is_empty(), "Select an object");
        let stop_index = scene.gradient_stop;
        let mut artwork = draft.scene_snapshot()?;
        for index in &selected {
            let object = &mut artwork.objects[*index];
            match action {
                "solid" => object.fill_gradient = None,
                "linear" | "radial" | "fit" => {
                    let (low, high) = object
                        .path
                        .bounds()
                        .context("Draw a shape before adding a gradient")?;
                    let center = VectorPoint {
                        x: (low.x + high.x) * 0.5,
                        y: (low.y + high.y) * 0.5,
                    };
                    let radial = action == "radial"
                        || action == "fit"
                            && object
                                .fill_gradient
                                .as_ref()
                                .is_some_and(|g| matches!(g.kind, GradientKind::Radial { .. }));
                    let kind = if radial {
                        GradientKind::Radial {
                            center,
                            focus: center,
                            radius: ((high.x - low.x).max(high.y - low.y) * 0.5).max(1.),
                        }
                    } else {
                        GradientKind::Linear {
                            start: VectorPoint {
                                x: low.x,
                                y: center.y,
                            },
                            end: VectorPoint {
                                x: high.x.max(low.x + 1.),
                                y: center.y,
                            },
                        }
                    };
                    let stops = object.fill_gradient.as_ref().map_or_else(
                        || {
                            vec![
                                GradientStop {
                                    offset: 0.,
                                    color: object.fill.unwrap_or([213, 128, 73, 255]),
                                },
                                GradientStop {
                                    offset: 1.,
                                    color: [71, 58, 54, 255],
                                },
                            ]
                        },
                        |g| g.stops.clone(),
                    );
                    object.fill_gradient = Some(GradientFill {
                        kind,
                        stops,
                        spread: GradientSpread::Pad,
                        transform: [1., 0., 0., 1., 0., 0.],
                    });
                }
                "reverse" => {
                    if let Some(gradient) = &mut object.fill_gradient {
                        gradient.stops.reverse();
                        for stop in &mut gradient.stops {
                            stop.offset = 1. - stop.offset;
                        }
                    }
                }
                "add-stop" => {
                    if let Some(gradient) = &mut object.fill_gradient {
                        ensure!(
                            gradient.stops.len() < 16,
                            "A gradient supports at most 16 stops"
                        );
                        let widest = gradient
                            .stops
                            .windows(2)
                            .enumerate()
                            .max_by(|(_, a), (_, b)| {
                                (a[1].offset - a[0].offset).total_cmp(&(b[1].offset - b[0].offset))
                            })
                            .map(|(i, _)| i)
                            .unwrap();
                        let a = gradient.stops[widest];
                        let b = gradient.stops[widest + 1];
                        let mut color = [0; 4];
                        for i in 0..4 {
                            color[i] = ((u16::from(a.color[i]) + u16::from(b.color[i])) / 2) as u8;
                        }
                        gradient.stops.insert(
                            widest + 1,
                            GradientStop {
                                offset: (a.offset + b.offset) * 0.5,
                                color,
                            },
                        );
                    }
                }
                "remove-stop" => {
                    if let Some(gradient) = &mut object.fill_gradient {
                        ensure!(
                            gradient.stops.len() > 2,
                            "A gradient needs at least two stops"
                        );
                        gradient
                            .stops
                            .remove(stop_index.min(gradient.stops.len() - 1));
                    }
                }
                "pad" | "repeat" | "reflect" => {
                    if let Some(gradient) = &mut object.fill_gradient {
                        gradient.spread = match action {
                            "repeat" => GradientSpread::Repeat,
                            "reflect" => GradientSpread::Reflect,
                            _ => GradientSpread::Pad,
                        };
                    }
                }
                _ => {
                    if object.stroke.is_some() {
                        let options = object
                            .stroke_options
                            .get_or_insert_with(StrokeOptions::default);
                        match action {
                            "butt" => options.cap = StrokeCap::Butt,
                            "round-cap" => options.cap = StrokeCap::Round,
                            "square" => options.cap = StrokeCap::Square,
                            "miter" => options.join = StrokeJoin::Miter,
                            "round-join" => options.join = StrokeJoin::Round,
                            "bevel" => options.join = StrokeJoin::Bevel,
                            "solid-stroke" => {
                                options.dashes.clear();
                                options.dash_offset = 0.;
                            }
                            "dashed" => {
                                let width = object.stroke.unwrap().width;
                                options.dashes = vec![width * 3., width * 2.];
                                options.dash_offset = 0.;
                            }
                            _ => anyhow::bail!("Unknown paint operation"),
                        }
                    }
                }
            }
            if let Some(gradient) = &object.fill_gradient {
                object.fill = Some(gradient.stops[0].color);
            }
        }
        artwork.version = artwork
            .version
            .max(omuse::vector_scene::VECTOR_SCENE_STYLE_VERSION);
        artwork.validate()?;
        self.replace_scene_artwork(artwork, selected, window, cx)?;
        self.status = "Paint updated for selected objects · Ctrl+Z to undo".into();
        Ok(())
    }

    fn scene_style_action(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.edit_scene_style(action, window, cx) {
            self.status = error.to_string();
        }
        cx.notify();
    }

    pub(super) fn vector_style_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let object = &scene.artwork.objects[scene.active];
        let gradient = object.fill_gradient.as_ref();
        let options = object.stroke_options.clone().unwrap_or_default();
        let disabled = self.busy || scene.selected_objects.is_empty();
        let action_button = |action: &'static str,
                             label: &'static str,
                             selected: bool,
                             disabled: bool,
                             cx: &mut Context<Self>| {
            let id = format!("vector-paint-{action}");
            button(
                SharedString::from(id.clone()),
                label,
                ButtonVariant::Outline,
                cx,
            )
            .debug_selector(move || id.clone())
            .selected(selected)
            .disabled(disabled)
            .flex_1()
            .min_w_0()
            .px_1()
            .on_click(
                cx.listener(move |this, _, window, cx| this.scene_style_action(action, window, cx)),
            )
        };
        let mut view = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(inspector_ui::panel_note("Fill paint", cx))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(action_button(
                        "solid",
                        "Solid",
                        gradient.is_none(),
                        disabled,
                        cx,
                    ))
                    .child(action_button(
                        "linear",
                        "Linear",
                        gradient.is_some_and(|g| matches!(g.kind, GradientKind::Linear { .. })),
                        disabled,
                        cx,
                    ))
                    .child(action_button(
                        "radial",
                        "Radial",
                        gradient.is_some_and(|g| matches!(g.kind, GradientKind::Radial { .. })),
                        disabled,
                        cx,
                    )),
            );
        if let Some(gradient) = gradient {
            let mut stops = div().flex().flex_wrap().gap_1();
            for (index, stop) in gradient.stops.iter().enumerate() {
                let id = format!("vector-gradient-stop-{index}");
                stops = stops.child(
                    button(
                        SharedString::from(id.clone()),
                        SharedString::from(format!("{}%", (stop.offset * 100.).round())),
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .child(colour_swatch(Some(stop.color), false, cx))
                    .selected(index == scene.gradient_stop)
                    .disabled(disabled)
                    .debug_selector(move || id.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Err(error) = this.edit_scene_style("apply", window, cx) {
                            this.status = error.to_string();
                            cx.notify();
                            return;
                        }
                        this.vector_draft
                            .as_mut()
                            .unwrap()
                            .scene
                            .as_mut()
                            .unwrap()
                            .gradient_stop = index;
                        this.load_advanced_vector_style(window, cx);
                        cx.notify();
                    })),
                );
            }
            view = view.child(stops).child(
                div()
                    .flex()
                    .gap_1()
                    .child(action_button(
                        "add-stop",
                        "Add stop",
                        false,
                        disabled || gradient.stops.len() >= 16,
                        cx,
                    ))
                    .child(action_button(
                        "remove-stop",
                        "Remove",
                        false,
                        disabled || gradient.stops.len() <= 2,
                        cx,
                    ))
                    .child(action_button("reverse", "Reverse", false, disabled, cx)),
            );
            let radial = matches!(gradient.kind, GradientKind::Radial { .. });
            let mut fields = div().flex().flex_wrap().gap_2();
            let mut labels = vec![
                (13, "Stop · %"),
                (14, "Stop colour"),
                (8, if radial { "Centre X" } else { "Start X" }),
                (9, if radial { "Centre Y" } else { "Start Y" }),
                (10, if radial { "Focus X" } else { "End X" }),
                (11, if radial { "Focus Y" } else { "End Y" }),
            ];
            if radial {
                labels.push((12, "Radius · px"));
            }
            for (index, label) in labels {
                fields = fields.child(self.vector_paint_input(index, label, window, cx));
            }
            view = view
                .child(fields)
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(action_button(
                            "pad",
                            "Pad",
                            gradient.spread == GradientSpread::Pad,
                            disabled,
                            cx,
                        ))
                        .child(action_button(
                            "repeat",
                            "Repeat",
                            gradient.spread == GradientSpread::Repeat,
                            disabled,
                            cx,
                        ))
                        .child(action_button(
                            "reflect",
                            "Reflect",
                            gradient.spread == GradientSpread::Reflect,
                            disabled,
                            cx,
                        )),
                )
                .child(action_button(
                    "fit",
                    "Fit gradient to shape",
                    false,
                    disabled,
                    cx,
                ));
        }
        if object.stroke.is_some() {
            view = view.child(inspector_ui::panel_note("Stroke geometry", cx));
            let no_stroke = disabled;
            view = view
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(action_button(
                            "butt",
                            "Butt",
                            options.cap == StrokeCap::Butt,
                            no_stroke,
                            cx,
                        ))
                        .child(action_button(
                            "round-cap",
                            "Round cap",
                            options.cap == StrokeCap::Round,
                            no_stroke,
                            cx,
                        ))
                        .child(action_button(
                            "square",
                            "Square",
                            options.cap == StrokeCap::Square,
                            no_stroke,
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(action_button(
                            "miter",
                            "Miter",
                            options.join == StrokeJoin::Miter,
                            no_stroke,
                            cx,
                        ))
                        .child(action_button(
                            "round-join",
                            "Round join",
                            options.join == StrokeJoin::Round,
                            no_stroke,
                            cx,
                        ))
                        .child(action_button(
                            "bevel",
                            "Bevel",
                            options.join == StrokeJoin::Bevel,
                            no_stroke,
                            cx,
                        )),
                );
            view = view
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(action_button(
                            "solid-stroke",
                            "Solid stroke",
                            options.dashes.is_empty(),
                            disabled,
                            cx,
                        ))
                        .child(action_button(
                            "dashed",
                            "Dashed",
                            !options.dashes.is_empty(),
                            disabled,
                            cx,
                        )),
                )
                .child(self.vector_paint_input(15, "Dash, gap · px (empty = solid)", window, cx))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(self.vector_paint_input(16, "Dash offset · px", window, cx))
                        .child(self.vector_paint_input(17, "Miter limit", window, cx)),
                );
        }
        if gradient.is_some() || object.stroke.is_some() {
            view=view.child(action_button("apply","Apply paint settings",false,disabled,cx))
                .child(inspector_ui::panel_note("Apply fields to selected objects. Stops support alpha. Gradient coordinates are local to the object.",cx));
        }
        view.into_any_element()
    }

    fn vector_paint_input(
        &self,
        index: usize,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = format!("vector-paint-field-{index}");
        let mut field = input(
            SharedString::from(id.clone()),
            &self.detail_inputs[index],
            window,
            cx,
        )
        .debug_selector(move || id.clone());
        if index == 14 {
            field = field.prefix(colour_swatch(
                parse_vector_colour(&self.detail_inputs[index].read(cx).value()).ok(),
                false,
                cx,
            ));
        }
        div()
            .flex_1()
            .min_w(px(112.))
            .flex()
            .flex_col()
            .gap_1()
            .child(inspector_ui::panel_note(label, cx))
            .child(field)
            .into_any_element()
    }
}

impl EditorView {
    /// Deterministic, real-editor artwork for the screenshot/interaction run.
    pub(in crate::ui) fn prepare_vector_styles_inspection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.prepare_vector_scene_inspection(window, cx)?;
        let draft = self.vector_draft.as_ref().context("Vector editor closed")?;
        let scene = draft.scene.as_ref().unwrap();
        let selected = scene.selected_objects.clone();
        let mut artwork = draft.scene_snapshot()?;
        for (index, object) in artwork.objects.iter_mut().enumerate() {
            let Some((low, high)) = object.path.bounds() else {
                continue;
            };
            let center = VectorPoint {
                x: (low.x + high.x) * 0.5,
                y: (low.y + high.y) * 0.5,
            };
            object.fill_gradient = Some(GradientFill {
                kind: if index % 2 == 0 {
                    GradientKind::Radial {
                        center,
                        focus: VectorPoint {
                            x: center.x - (high.x - low.x) * 0.18,
                            y: center.y - (high.y - low.y) * 0.18,
                        },
                        radius: (high.x - low.x).max(high.y - low.y) * 0.6,
                    }
                } else {
                    GradientKind::Linear {
                        start: low,
                        end: high,
                    }
                },
                stops: if index % 2 == 0 {
                    vec![
                        GradientStop {
                            offset: 0.,
                            color: [193, 225, 218, 255],
                        },
                        GradientStop {
                            offset: 0.45,
                            color: [84, 122, 142, 255],
                        },
                        GradientStop {
                            offset: 1.,
                            color: [34, 56, 71, 255],
                        },
                    ]
                } else {
                    vec![
                        GradientStop {
                            offset: 0.,
                            color: [243, 202, 151, 255],
                        },
                        GradientStop {
                            offset: 0.5,
                            color: [213, 128, 73, 255],
                        },
                        GradientStop {
                            offset: 1.,
                            color: [113, 53, 47, 255],
                        },
                    ]
                },
                spread: GradientSpread::Pad,
                transform: [1., 0., 0., 1., 0., 0.],
            });
            object.fill = Some(object.fill_gradient.as_ref().unwrap().stops[0].color);
            object.stroke = Some(StrokeStyle {
                color: [242, 218, 175, 255],
                width: 4.,
            });
            object.stroke_options = Some(StrokeOptions {
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                dashes: vec![16., 10.],
                ..Default::default()
            });
            object.opacity = 1.;
        }
        artwork.version = omuse::vector_scene::VECTOR_SCENE_STYLE_VERSION;
        self.replace_scene_artwork(artwork, selected, window, cx)?;
        self.status =
            "Editable gradients and precision strokes · Select a stop to refine its colour".into();
        Ok(())
    }
}
