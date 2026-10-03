//! Retained text on curves uses the existing canvas and undo history. Reflow is
//! a cancellable background task; the original guide remains an editable path.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_scene::{TextOnPath, TextPathAlignment, VectorObject};

fn alignment(value: &str) -> TextPathAlignment {
    match value {
        "center" => TextPathAlignment::Center,
        "end" => TextPathAlignment::End,
        _ => TextPathAlignment::Start,
    }
}
fn alignment_name(value: TextPathAlignment) -> &'static str {
    match value {
        TextPathAlignment::Start => "start",
        TextPathAlignment::Center => "center",
        TextPathAlignment::End => "end",
    }
}

// Selecting a guide and its text can leave the guide as the active geometry.
// Keep the typography form tied to the sole selected text in that case.
fn selected_text(scene: &super::scene::SceneDraft) -> Option<&TextOnPath> {
    if let Some(text) = &scene.artwork.objects[scene.active].text_path {
        return Some(text);
    }
    let mut texts = scene
        .selected_objects
        .iter()
        .filter_map(|i| scene.artwork.objects[*i].text_path.as_ref());
    let text = texts.next()?;
    texts.next().is_none().then_some(text)
}

impl EditorView {
    pub(super) fn load_vector_text_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return;
        };
        let text_values = selected_text(scene).map(|text| {
            vec![
                text.text.clone(),
                text.font_family.clone(),
                text.font_size.to_string(),
                text.letter_spacing.to_string(),
                (text.start_offset * 100.).to_string(),
                alignment_name(text.alignment).into(),
            ]
        });
        let values = if let Some(values) = text_values {
            scene.text_controls_open = true;
            values
        } else if scene.text_fields_initialized {
            return;
        } else {
            vec![
                "Omuse".into(),
                "Outfit".into(),
                "48".into(),
                "0".into(),
                "0".into(),
                "start".into(),
            ]
        };
        scene.text_fields_initialized = true;
        for (index, value) in values.into_iter().enumerate() {
            self.detail_inputs[32 + index]
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    pub(super) fn pending_vector_text_edits(&self, cx: &Context<Self>) -> bool {
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return false;
        };
        if scene.updating_text || !scene.text_fields_initialized {
            return false;
        }
        let Some(text) = selected_text(scene) else {
            return false;
        };
        let field = |i: usize| self.detail_inputs[i].read(cx).value();
        field(32).as_ref() != text.text
            || field(33).as_ref() != text.font_family
            || field(34).parse::<f32>() != Ok(text.font_size)
            || field(35).parse::<f32>() != Ok(text.letter_spacing)
            || field(36).parse::<f32>() != Ok(text.start_offset * 100.)
            || alignment(field(37).as_ref()) != text.alignment
    }

    fn vector_text_recipe(
        &self,
        base: Option<&TextOnPath>,
        guide: VectorPath,
        cx: &Context<Self>,
    ) -> Result<TextOnPath> {
        let number = |i: usize| -> Result<f32> {
            self.detail_inputs[i]
                .read(cx)
                .value()
                .trim()
                .parse::<f32>()
                .context("Enter valid text size, tracking and curve position")
        };
        let percent = number(36)?;
        let offset = base
            .filter(|base| base.start_offset * 100. == percent)
            .map_or(percent / 100., |base| base.start_offset);
        let result = TextOnPath {
            text: self.detail_inputs[32].read(cx).value().to_string(),
            font_family: self.detail_inputs[33].read(cx).value().to_string(),
            font_size: number(34)?,
            letter_spacing: number(35)?,
            start_offset: offset,
            alignment: alignment(self.detail_inputs[37].read(cx).value().as_ref()),
            guide,
            transform: base.map_or([1., 0., 0., 1., 0., 0.], |base| base.transform),
            resolved_fonts: base.map_or_else(Vec::new, |base| base.resolved_fonts.clone()),
        };
        result.validate()?;
        Ok(result)
    }

    fn vector_text_action(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<()> {
            if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
                scene.updating_text = true;
            }
            let style_result = self.update_vector_style(cx);
            if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
                scene.updating_text = false;
            }
            style_result?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            let mut source = draft.scene_snapshot()?;
            let selected = scene.selected_objects.clone();
            ensure!(!selected.is_empty(), "Select a curve or text object");
            if action == "outlines" {
                ensure!(
                    !self.pending_vector_text_edits(cx),
                    "Update text before converting it to outlines"
                );
                for index in &selected {
                    source.objects[*index].text_path = None;
                }
                self.replace_scene_artwork(source, selected, window, cx)?;
                self.status =
                    "Text converted to editable glyph outlines · Ctrl+Z restores the text recipe"
                        .into();
                return Ok(());
            }
            let active = scene.active;
            let (target, recipe) = if action == "replace-guide" {
                ensure!(
                    selected.len() == 2,
                    "Select one text object and one curve with Shift+click"
                );
                let text = selected
                    .iter()
                    .copied()
                    .find(|i| source.objects[*i].text_path.is_some())
                    .context("Select a text object")?;
                let guide = selected
                    .iter()
                    .copied()
                    .find(|i| source.objects[*i].text_path.is_none())
                    .context("Select a separate curve")?;
                let base = source.objects[text].text_path.as_ref().unwrap();
                // Replacing a guide is also an explicit text update. Preserve
                // settings typed while both objects were selected.
                let mut recipe = self.vector_text_recipe(Some(base), base.guide.clone(), cx)?;
                recipe.set_world_guide(&source.objects[guide].path)?;
                (Some(text), recipe)
            } else {
                ensure!(selected.len() == 1, "Select a single curve or text object");
                let base = source.objects[active].text_path.as_ref();
                let guide = base.map_or_else(
                    || source.objects[active].path.clone(),
                    |text| text.guide.clone(),
                );
                let mut recipe = self.vector_text_recipe(base, guide, cx)?;
                if action == "reverse" {
                    let sub = &mut recipe.guide.subpaths[0];
                    sub.anchors.reverse();
                    for anchor in &mut sub.anchors {
                        std::mem::swap(&mut anchor.incoming, &mut anchor.outgoing);
                    }
                }
                (base.map(|_| active), recipe)
            };
            let object = target.map_or_else(
                || {
                    let mut object = VectorObject::new(
                        format!(
                            "Text · {}",
                            recipe.text.chars().take(32).collect::<String>()
                        ),
                        VectorPath::default(),
                        Some(self.editor.brush.color),
                        None,
                    );
                    object.groups = source.objects[active].groups.clone();
                    object
                },
                |index| source.objects[index].clone(),
            );
            let token = draft.cancel.clone();
            let task_token = token.clone();
            let task = cx.background_executor().spawn(async move {
                omuse::vector_scene::text::update_object(&object, &recipe, &task_token)
            });
            self.busy = true;
            self.status = "Shaping text along the curve… Escape cancels".into();
            self.focus.focus(window, cx);
            cx.spawn_in(window,async move|view,cx| {
                let result=task.await;
                let _=view.update_in(cx,|this,window,cx| {
                    if !this.vector_draft.as_ref().is_some_and(|draft|Arc::ptr_eq(&draft.cancel,&token)) {return;}
                    this.busy=false;
                    if !this.vector_scene_current() || token.load(Ordering::Relaxed) {this.status="Text operation cancelled or document changed".into();cx.notify();return;}
                    let result=result.and_then(|object| {
                        let text=object.text_path.as_ref().unwrap();
                        let fallback=!matches!(text.font_family.as_str(),"sans-serif"|"serif"|"monospace") && !text.resolved_fonts.iter().any(|font|font.eq_ignore_ascii_case(&text.font_family));
                        let fonts=text.resolved_fonts.join(", ");
                        let index=if let Some(index)=target {source.objects[index]=object;index} else {
                            ensure!(source.objects.len()<omuse::vector_scene::MAX_SCENE_OBJECTS,"Artwork has reached its object limit");
                            let index=active+1;source.objects.insert(index,object);index
                        };
                        source.version=omuse::vector_scene::VECTOR_SCENE_TEXT_VERSION;
                        this.replace_scene_artwork(source,[index].into_iter().collect(),window,cx)?;
                        this.status=if fallback {format!("Text updated · Font fallback: {fonts} · Outlines preserve this appearance")}
                            else {format!("Text updated · Fonts: {fonts} · Ctrl+Z to undo")};
                        Ok(())
                    });
                    if let Err(error)=result {this.status=format!("Text on path: {error:#}");}
                    this.start_scene_preview(cx);this.focus.focus(window,cx);cx.notify();
                });
            }).detach();
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Text on path: {error:#}");
        }
        cx.notify();
    }

    pub(super) fn vector_text_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let text = selected_text(scene);
        let disabled = self.busy || scene.selected_objects.is_empty();
        let open = scene.text_controls_open;
        let mut view = div().flex().flex_col().gap_2().child(
            button(
                "vector-text-section",
                if open {
                    "Text on a path ▾"
                } else {
                    "Text on a path ▸"
                },
                ButtonVariant::Outline,
                cx,
            )
            .disabled(self.busy)
            .debug_selector(|| "vector-text-section".into())
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(scene) = this.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
                    scene.text_controls_open = !scene.text_controls_open;
                }
                cx.notify();
            })),
        );
        if !open {
            return view.into_any_element();
        }
        for (index, label) in [(32, "Text"), (33, "Font family")] {
            view = view.child(self.vector_text_input(index, label, window, cx));
        }
        let mut fields = div().flex().flex_wrap().gap_2();
        for (index, label) in [
            (34, "Size · local px"),
            (35, "Tracking · px"),
            (36, "Curve position · %"),
        ] {
            fields = fields.child(self.vector_text_input(index, label, window, cx));
        }
        view = view.child(fields);
        let mut align = div().flex().gap_1();
        for (name, label, offset) in [
            ("start", "Start", "0"),
            ("center", "Centre", "50"),
            ("end", "End", "100"),
        ] {
            let id = format!("vector-text-align-{name}");
            align = align.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .selected(self.detail_inputs[37].read(cx).value().as_ref() == name)
                .disabled(disabled)
                .flex_1()
                .min_w_0()
                .debug_selector(move || id.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.detail_inputs[37]
                        .update(cx, |input, cx| input.set_value(name, window, cx));
                    this.detail_inputs[36]
                        .update(cx, |input, cx| input.set_value(offset, window, cx));
                    cx.notify();
                })),
            );
        }
        view = view.child(align);
        let actions = if text.is_some() {
            vec![
                ("update", "Update text"),
                ("reverse", "Reverse curve"),
                ("outlines", "Convert to outlines"),
            ]
        } else {
            vec![("create", "Create text on curve")]
        };
        for (action, label) in actions {
            let id = format!("vector-text-{action}");
            view = view.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(disabled || scene.selected_objects.len() != 1)
                .debug_selector(move || id.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.vector_text_action(action, window, cx)
                })),
            );
        }
        let can_replace = scene.selected_objects.len() == 2
            && scene
                .selected_objects
                .iter()
                .filter(|i| scene.artwork.objects[**i].text_path.is_some())
                .count()
                == 1;
        view=view.child(button("vector-text-replace-guide","Use selected curve for text",ButtonVariant::Outline,cx).disabled(disabled || !can_replace).debug_selector(||"vector-text-replace-guide".into())
            .on_click(cx.listener(|this,_,window,cx|this.vector_text_action("replace-guide",window,cx))))
            .child(inspector_ui::panel_note("The source curve stays editable. Edit its nodes, then Shift+select the curve and text and choose Use selected curve. Update text keeps typed settings before you leave.",cx));
        if let Some(text) = text {
            view=view.child(inspector_ui::panel_note(format!("Rendered with {}. Saved outlines preserve appearance; text edits may use a fallback if a font is missing.",text.resolved_fonts.join(", ")),cx));
        }
        view.into_any_element()
    }

    fn vector_text_input(
        &self,
        index: usize,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = format!("vector-text-field-{index}");
        div()
            .flex_1()
            .min_w(px(108.))
            .flex()
            .flex_col()
            .gap_1()
            .child(inspector_ui::panel_note(label, cx))
            .child(
                input(
                    SharedString::from(id.clone()),
                    &self.detail_inputs[index],
                    window,
                    cx,
                )
                .debug_selector(move || id.clone()),
            )
            .into_any_element()
    }
}

impl EditorView {
    /// Uses the same asynchronous create action as the visible inspector.
    pub(in crate::ui) fn prepare_vector_text_inspection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.open_vector_scene(window, cx);
        let draft = self.vector_draft.as_ref().context("Vector editor closed")?;
        let mut artwork = draft.scene_snapshot()?;
        let w = artwork.width as f32;
        let h = artwork.height as f32;
        let guide = VectorPath {
            fill_rule: Default::default(),
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: VectorPoint {
                            x: w * 0.1,
                            y: h * 0.65,
                        },
                        incoming: None,
                        outgoing: Some(VectorPoint {
                            x: w * 0.25,
                            y: h * 0.15,
                        }),
                    },
                    Anchor {
                        position: VectorPoint {
                            x: w * 0.9,
                            y: h * 0.65,
                        },
                        incoming: Some(VectorPoint {
                            x: w * 0.75,
                            y: h * 0.15,
                        }),
                        outgoing: None,
                    },
                ],
            }],
        };
        artwork.objects = vec![VectorObject::new(
            "Editable type curve",
            guide,
            None,
            Some(StrokeStyle {
                color: [128, 155, 159, 180],
                width: 2.,
            }),
        )];
        self.replace_scene_artwork(artwork, [0].into_iter().collect(), window, cx)?;
        self.editor.brush.color = [237, 169, 110, 255];
        for (index, value) in [
            (32, "MADE TO INSPIRE".to_string()),
            (33, "Outfit".into()),
            (34, (w * 0.042).clamp(12., 64.).to_string()),
            (35, "2".into()),
            (36, "50".into()),
            (37, "center".into()),
        ] {
            self.detail_inputs[index].update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.vector_draft
            .as_mut()
            .unwrap()
            .scene
            .as_mut()
            .unwrap()
            .text_controls_open = true;
        self.vector_text_action("create", window, cx);
        Ok(())
    }
}
