//! Selection, arrangement and construction on the shared vector canvas.
use super::scene::bake_transforms;
use super::*;
use anyhow::{Context as _, Result, ensure};
use gpui_kit::Focusable;
use omuse::vector_scene::{VectorObject, VectorScene};
use std::collections::{BTreeSet, HashMap};

pub(super) fn object_bounds(object: &VectorObject) -> Option<(VectorPoint, VectorPoint)> {
    // Draft transforms are baked when opened or replaced.
    let (mut a, mut b) = object.path.bounds()?;
    let radius = object.stroke.map_or(0., |stroke| stroke.width * 0.5);
    a.x -= radius;
    a.y -= radius;
    b.x += radius;
    b.y += radius;
    Some((a, b))
}

pub(super) fn group_members(artwork: &VectorScene, index: usize) -> BTreeSet<usize> {
    let Some(object) = artwork.objects.get(index) else {
        return BTreeSet::new();
    };
    match object.groups.first() {
        Some(group) => artwork
            .objects
            .iter()
            .enumerate()
            .filter(|(_, item)| item.groups.first().is_some_and(|g| g.id == group.id))
            .map(|(i, _)| i)
            .collect(),
        None => [index].into_iter().collect(),
    }
}

impl EditorView {
    pub(super) fn scene_arrange_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<()> {
            self.update_vector_style(cx)?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            let selected: Vec<_> = scene.selected_objects.iter().copied().collect();
            let source = draft.scene_snapshot()?;
            use omuse::vector_scene_ops::{self as ops, AlignAxis as A, DistributeAxis as D};
            let (artwork, selection) = match action {
                "group" => ops::group(&source, &selected, "Group")?,
                "ungroup" => ops::ungroup(&source, &selected)?,
                "transform" => {
                    let values: Vec<f32> = self.detail_inputs[4..8]
                        .iter()
                        .map(|i| i.read(cx).value().parse::<f32>())
                        .collect::<std::result::Result<_, _>>()?;
                    ensure!(
                        values.iter().all(|v| v.is_finite()),
                        "Transform values must be finite"
                    );
                    ensure!(
                        (0.01..=10_000.).contains(&values[2]),
                        "Scale must be between 0.01 and 10,000%"
                    );
                    let center = ops::bounds(&source, &selected)?
                        .context("Selection has no geometry")?
                        .center();
                    let artwork = ops::scale(
                        &source,
                        &selected,
                        values[2] / 100.,
                        values[2] / 100.,
                        center,
                    )?;
                    let artwork = ops::rotate(&artwork, &selected, values[3], center)?;
                    (
                        ops::translate(&artwork, &selected, values[0], values[1])?,
                        selected.clone(),
                    )
                }
                "distribute-x" | "distribute-y" => (
                    ops::distribute(
                        &source,
                        &selected,
                        if action == "distribute-x" {
                            D::Horizontal
                        } else {
                            D::Vertical
                        },
                    )?,
                    selected.clone(),
                ),
                _ => {
                    let axis = match action {
                        "align-left" => A::Left,
                        "align-center-x" => A::HorizontalCenter,
                        "align-right" => A::Right,
                        "align-top" => A::Top,
                        "align-center-y" => A::VerticalCenter,
                        "align-bottom" => A::Bottom,
                        _ => anyhow::bail!("Unknown vector operation"),
                    };
                    (ops::align(&source, &selected, axis)?, selected.clone())
                }
            };
            self.replace_scene_artwork(artwork, selection.into_iter().collect(), window, cx)?;
            self.reset_scene_transform_inputs(window, cx);
            self.status = "Vector selection updated · Ctrl+Z to undo".into();
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Vector selection: {error:#}");
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn scene_select_matching(
        &mut self,
        property: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let Ok(artwork) = draft.scene_snapshot() else {
            return;
        };
        let Some(scene) = draft.scene.as_ref() else {
            return;
        };
        let active = &artwork.objects[scene.active];
        let selected = artwork
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                o.visible
                    && match property {
                        "fill" => o.fill == active.fill && o.fill_gradient == active.fill_gradient,
                        "stroke" => {
                            o.stroke == active.stroke && o.stroke_options == active.stroke_options
                        }
                        "opacity" => o.opacity == active.opacity,
                        _ => {
                            o.fill == active.fill
                                && o.stroke == active.stroke
                                && o.fill_gradient == active.fill_gradient
                                && o.stroke_options == active.stroke_options
                        }
                    }
            })
            .map(|(i, _)| i)
            .collect();
        self.set_scene_selection(selected, Some(scene.active), window, cx);
        self.status = "Matching objects selected · Style edits apply to this selection".into();
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn scene_boolean(
        &mut self,
        operation: omuse::vector_boolean::BooleanOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let Some(scene) = draft.scene.as_ref() else {
            return;
        };
        let selected = scene.selected_objects.clone();
        if selected.len() < 2 {
            self.status = "Select at least two filled objects".into();
            cx.notify();
            return;
        }
        let source = match draft.scene_snapshot() {
            Ok(source) => source,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let token = draft.cancel.clone();
        let task_token = token.clone();
        let Some(objects) = selected
            .iter()
            .map(|i| source.objects.get(*i).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            self.status = "Object selection changed; select the shapes again".into();
            cx.notify();
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async move { omuse::vector_boolean::combine(&objects, operation, &task_token) });
        self.busy = true;
        self.status = "Combining shapes… Escape cancels".into();
        self.focus.focus(window, cx);
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this
                    .vector_draft
                    .as_ref()
                    .is_some_and(|d| Arc::ptr_eq(&d.cancel, &token))
                {
                    return;
                }
                this.busy = false;
                if !this.vector_scene_current() {
                    this.status = "Document changed; the shape operation was discarded".into();
                    cx.notify();
                    return;
                }
                let result = result.and_then(|mut objects| {
                    let mut artwork = source.clone();
                    let mut groups = source.objects[*selected.first().unwrap()].groups.clone();
                    for index in &selected {
                        let common = groups
                            .iter()
                            .zip(&source.objects[*index].groups)
                            .take_while(|(a, b)| a == b)
                            .count();
                        groups.truncate(common);
                    }
                    for object in &mut objects {
                        object.groups = groups.clone();
                    }
                    let insertion = selected.last().unwrap() + 1 - selected.len();
                    artwork.objects = artwork
                        .objects
                        .into_iter()
                        .enumerate()
                        .filter(|(i, _)| !selected.contains(i))
                        .map(|(_, o)| o)
                        .collect();
                    let count = objects.len();
                    artwork.objects.splice(insertion..insertion, objects);
                    if artwork.objects.is_empty() {
                        artwork.objects.push(super::scene::new_object(
                            "Path 1",
                            VectorPath::default(),
                            [0; 4],
                        ));
                    }
                    let selection = if count == 0 {
                        BTreeSet::new()
                    } else {
                        (insertion..insertion + count).collect()
                    };
                    this.replace_scene_artwork(artwork, selection, window, cx)?;
                    this.status = if count == 0 {
                        "No filled area remains · Ctrl+Z restores the shapes".into()
                    } else {
                        "Shapes combined · Editable points retained · Ctrl+Z restores originals"
                            .into()
                    };
                    Ok(())
                });
                if let Err(error) = result {
                    this.status = format!("Combine shapes: {error:#}");
                }
                this.start_scene_preview(cx);
                this.focus.focus(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn vector_selection_command(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.vector_scene_active() {
            return false;
        }
        if self.busy {
            return name.starts_with("vector-");
        }
        match name {
            "select-all" | "deselect" => {
                let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
                let selected = if name == "select-all" {
                    scene
                        .artwork
                        .objects
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| o.visible)
                        .map(|(i, _)| i)
                        .collect()
                } else {
                    BTreeSet::new()
                };
                self.set_scene_selection(selected, None, window, cx);
            }
            "group" | "vector-group" | "vector-ungroup" => self.scene_arrange_action(
                if name == "vector-ungroup" {
                    "ungroup"
                } else {
                    "group"
                },
                window,
                cx,
            ),
            "duplicate" => {
                self.scene_action("duplicate", window, cx);
            }
            "transform" => {
                self.set_scene_mode(super::scene::SceneMode::Select, window, cx);
                self.inspector_visible = true;
                self.inspector_tab = studio_ui::InspectorTab::Layers;
                self.detail_inputs[4]
                    .read(cx)
                    .focus_handle(cx)
                    .focus(window, cx);
                if let Some(scene) = self
                    .vector_draft
                    .as_ref()
                    .and_then(|draft| draft.scene.as_ref())
                {
                    scene.transform_anchor.scroll_to(window, cx);
                }
                cx.notify();
            }
            "vector-import-svg" => self.choose_vector_import(window, cx),
            "vector-export-svg" => self.choose_vector_export(window, cx),
            "vector-outline" => {
                let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
                scene.outline = !scene.outline;
                self.status = if scene.outline {
                    "Outline view · Export keeps the original appearance"
                } else {
                    "Artwork preview"
                }
                .into();
                self.focus.focus(window, cx);
                cx.notify();
            }
            "vector-same-fill" | "vector-same-stroke" | "vector-same-opacity" => {
                self.scene_select_matching(name.strip_prefix("vector-same-").unwrap(), window, cx)
            }
            "vector-union" | "vector-subtract" | "vector-intersect" | "vector-exclude"
            | "vector-divide" => {
                use omuse::vector_boolean::BooleanOperation as B;
                self.scene_boolean(
                    match name {
                        "vector-union" => B::Union,
                        "vector-subtract" => B::Subtract,
                        "vector-intersect" => B::Intersect,
                        "vector-exclude" => B::Exclude,
                        _ => B::Divide,
                    },
                    window,
                    cx,
                );
            }
            _ if name.starts_with("vector-align-") || name.starts_with("vector-distribute-") => {
                self.scene_arrange_action(&name[7..], window, cx)
            }
            _ => return false,
        }
        true
    }

    pub(in crate::ui) fn vector_outline_active(&self) -> bool {
        self.vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| s.outline)
    }

    pub(in crate::ui) fn vector_inspector_scroll(&self) -> Option<gpui_kit::ScrollHandle> {
        self.vector_draft
            .as_ref()?
            .scene
            .as_ref()
            .map(|scene| scene.inspector_scroll.clone())
    }

    pub(super) fn vector_selection_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let count = scene.selected_objects.len();
        let mut controls = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(inspector_ui::panel_note(
                SharedString::from(format!(
                    "{count} selected · Shift-click adds · Drag empty canvas to select"
                )),
                cx,
            ));
        let mut groups = div().flex().flex_wrap().gap_1();
        for (id, label, action) in [
            ("vector-group", "Group · Ctrl+G", "group"),
            ("vector-ungroup", "Ungroup", "ungroup"),
        ] {
            groups = groups.child(
                button(id, label, ButtonVariant::Outline, cx)
                    .disabled(self.busy || count == 0)
                    .debug_selector(move || id.into())
                    .on_click(
                        cx.listener(move |this, _, w, cx| this.scene_arrange_action(action, w, cx)),
                    ),
            );
        }
        groups = groups.child(
            button("vector-outline", "Outline", ButtonVariant::Secondary, cx)
                .selected(scene.outline)
                .disabled(self.busy)
                .debug_selector(|| "vector-outline".into())
                .on_click(cx.listener(|this, _, w, cx| {
                    this.vector_selection_command("vector-outline", w, cx);
                })),
        );
        controls = controls.child(groups);
        let mut transforms = div()
            .id("vector-transform-fields")
            .anchor_scroll(Some(scene.transform_anchor.clone()))
            .flex()
            .flex_wrap()
            .gap_2();
        for (i, label) in ["Move X · px", "Move Y · px", "Scale · %", "Rotate · °"]
            .into_iter()
            .enumerate()
        {
            let id = format!("vector-transform-{i}");
            transforms = transforms.child(
                div()
                    .flex_1()
                    .min_w(px(100.))
                    .child(inspector_ui::panel_note(label, cx))
                    .child(
                        input(
                            SharedString::from(id.clone()),
                            &self.detail_inputs[4 + i],
                            window,
                            cx,
                        )
                        .debug_selector(move || id.clone()),
                    ),
            );
        }
        controls = controls.child(transforms).child(
            button(
                "vector-transform-apply",
                "Transform selection",
                ButtonVariant::Outline,
                cx,
            )
            .disabled(self.busy || count == 0)
            .debug_selector(|| "vector-transform-apply".into())
            .on_click(cx.listener(|this, _, w, cx| this.scene_arrange_action("transform", w, cx))),
        );
        let mut align = div().flex().flex_wrap().gap_1();
        for (action, label, minimum) in [
            ("align-left", "Left", 2),
            ("align-center-x", "Centre X", 2),
            ("align-right", "Right", 2),
            ("align-top", "Top", 2),
            ("align-center-y", "Centre Y", 2),
            ("align-bottom", "Bottom", 2),
            ("distribute-x", "Space X", 3),
            ("distribute-y", "Space Y", 3),
        ] {
            let id = format!("vector-{action}");
            align = align.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy || count < minimum)
                .debug_selector(move || id.clone())
                .on_click(
                    cx.listener(move |this, _, w, cx| this.scene_arrange_action(action, w, cx)),
                ),
            );
        }
        controls = controls
            .child(inspector_ui::panel_note("Align and distribute", cx))
            .child(align);
        let mut booleans = div().flex().flex_wrap().gap_1();
        for (action, label) in [
            ("vector-union", "Unite"),
            ("vector-subtract", "Subtract"),
            ("vector-intersect", "Intersect"),
            ("vector-exclude", "Exclude"),
            ("vector-divide", "Divide"),
        ] {
            booleans = booleans.child(
                button(action, label, ButtonVariant::Outline, cx)
                    .disabled(self.busy || count < 2)
                    .debug_selector(move || action.into())
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.vector_selection_command(action, w, cx);
                    })),
            );
        }
        let mut matching = div().flex().flex_wrap().gap_1();
        for (property, label) in [
            ("fill", "Same fill"),
            ("stroke", "Same stroke"),
            ("opacity", "Same opacity"),
        ] {
            let id = format!("vector-same-{property}");
            matching = matching.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy || count == 0)
                .debug_selector(move || id.clone())
                .on_click(
                    cx.listener(move |this, _, w, cx| this.scene_select_matching(property, w, cx)),
                ),
            );
        }
        controls
            .child(inspector_ui::panel_note(
                "Combine filled shapes · Bottom shape supplies the style",
                cx,
            ))
            .child(booleans)
            .child(matching)
            .into_any_element()
    }

    pub(super) fn reset_scene_transform_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (input, value) in self.detail_inputs[4..8].iter().zip(["0", "0", "100", "0"]) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    pub(super) fn set_scene_selection(
        &mut self,
        selected: BTreeSet<usize>,
        preferred: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        draft.store_scene_object();
        let Some(scene) = draft.scene.as_mut() else {
            return;
        };
        scene.selected_objects = selected
            .into_iter()
            .filter(|i| *i < scene.artwork.objects.len())
            .collect();
        if scene.artwork.objects.is_empty() {
            return;
        }
        scene.active = preferred
            .filter(|i| scene.selected_objects.contains(i))
            .or_else(|| scene.selected_objects.last().copied())
            .unwrap_or(scene.active)
            .min(scene.artwork.objects.len() - 1);
        let object = &scene.artwork.objects[scene.active];
        if object.text_path.is_some() {
            scene.mode = super::scene::SceneMode::Select;
        }
        draft.path = object.path.clone();
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = None;
        draft.drag = None;
        scene.drag_origin = None;
        self.load_scene_style(window, cx);
        cx.notify();
    }

    pub(super) fn select_scene_members(
        &mut self,
        index: usize,
        additive: bool,
        individual: bool,
        preserve: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return;
        };
        let members = if individual {
            [index].into_iter().collect()
        } else {
            group_members(&scene.artwork, index)
        };
        let mut selected =
            if additive || (preserve && !individual && scene.selected_objects.contains(&index)) {
                scene.selected_objects.clone()
            } else {
                BTreeSet::new()
            };
        if additive && members.iter().all(|i| selected.contains(i)) {
            for i in members {
                selected.remove(&i);
            }
        } else {
            selected.extend(members);
        }
        self.set_scene_selection(selected, Some(index), window, cx);
    }

    pub(super) fn replace_scene_artwork(
        &mut self,
        mut artwork: VectorScene,
        selected: BTreeSet<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        bake_transforms(&mut artwork)?;
        ensure!(
            !artwork.objects.is_empty(),
            "Artwork needs a drawing object"
        );
        self.scene_checkpoint();
        let draft = self.vector_draft.as_mut().context("Vector editor closed")?;
        let scene = draft.scene.as_mut().context("Vector editor closed")?;
        scene.active = selected
            .last()
            .copied()
            .unwrap_or(0)
            .min(artwork.objects.len() - 1);
        scene.artwork = artwork;
        scene.selected_objects = selected
            .into_iter()
            .filter(|i| *i < scene.artwork.objects.len())
            .collect();
        scene.drag_origin = None;
        scene.marquee = None;
        let object = &scene.artwork.objects[scene.active];
        if object.text_path.is_some() {
            scene.mode = super::scene::SceneMode::Select;
        }
        draft.path = object.path.clone();
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = None;
        draft.drag = None;
        self.load_scene_style(window, cx);
        self.vector_scene_changed(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn scene_translate_selected(
        &mut self,
        dx: f32,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let draft = self.vector_draft.as_ref().context("Vector editor closed")?;
        let selected = draft
            .scene
            .as_ref()
            .context("Vector editor closed")?
            .selected_objects
            .clone();
        let mut artwork = draft.scene_snapshot()?;
        for i in selected {
            if let Some(text) = &mut artwork.objects[i].text_path {
                text.bake_transform([1., 0., 0., 1., dx, dy]);
            }
            if let Some(gradient) = &mut artwork.objects[i].fill_gradient {
                gradient.bake_transform([1., 0., 0., 1., dx, dy]);
            }
            for subpath in &mut artwork.objects[i].path.subpaths {
                for anchor in &mut subpath.anchors {
                    for p in [
                        Some(&mut anchor.position),
                        anchor.incoming.as_mut(),
                        anchor.outgoing.as_mut(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        p.x += dx;
                        p.y += dy;
                    }
                }
            }
        }
        artwork.validate()?;
        let draft = self.vector_draft.as_mut().unwrap();
        let scene = draft.scene.as_mut().unwrap();
        draft.path = artwork.objects[scene.active].path.clone();
        scene.artwork = artwork;
        self.vector_scene_changed(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn update_scene_marquee(
        &mut self,
        p: VectorPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return false;
        };
        let Some((start, _, base)) = &scene.marquee else {
            return false;
        };
        let start = *start;
        let base = base.clone();
        let mut selected = base.clone();
        let (left, right) = (start.x.min(p.x), start.x.max(p.x));
        let (top, bottom) = (start.y.min(p.y), start.y.max(p.y));
        for (i, object) in scene
            .artwork
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.visible)
        {
            if object_bounds(object)
                .is_some_and(|(a, b)| a.x >= left && b.x <= right && a.y >= top && b.y <= bottom)
            {
                selected.extend(group_members(&scene.artwork, i));
            }
        }
        self.set_scene_selection(selected, None, window, cx);
        self.vector_draft
            .as_mut()
            .unwrap()
            .scene
            .as_mut()
            .unwrap()
            .marquee = Some((start, p, base));
        cx.notify();
        true
    }

    pub(super) fn scene_selection_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(
            action,
            "remove" | "duplicate" | "visibility" | "forward" | "backward"
        ) {
            return false;
        }
        let result = (|| -> Result<()> {
            let draft = self.vector_draft.as_ref().context("Vector editor closed")?;
            let scene = draft.scene.as_ref().context("Vector editor closed")?;
            let selected = scene.selected_objects.clone();
            ensure!(!selected.is_empty(), "Select an object first");
            let mut artwork = draft.scene_snapshot()?;
            let selected_ids: BTreeSet<_> = selected
                .iter()
                .map(|i| artwork.objects[*i].id.clone())
                .collect();
            let mut new_ids = selected_ids.clone();
            match action {
                "remove" => {
                    artwork.objects.retain(|o| !selected_ids.contains(&o.id));
                    if artwork.objects.is_empty() {
                        artwork.objects.push(super::scene::new_object(
                            "Path 1",
                            VectorPath::default(),
                            draft.fill,
                        ));
                    }
                    let next = (*selected.first().unwrap()).min(artwork.objects.len() - 1);
                    new_ids = [artwork.objects[next].id.clone()].into_iter().collect();
                }
                "duplicate" => {
                    ensure!(
                        artwork.objects.len() + selected.len()
                            <= omuse::vector_scene::MAX_SCENE_OBJECTS,
                        "Artwork has reached its object limit"
                    );
                    let mut group_ids = HashMap::new();
                    let copies: Vec<_> = selected
                        .iter()
                        .map(|i| {
                            let mut object = artwork.objects[*i].clone();
                            object.id = uuid::Uuid::new_v4().to_string();
                            object.name = format!("{} copy", object.name);
                            for group in &mut object.groups {
                                group.id = group_ids
                                    .entry(group.id.clone())
                                    .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                                    .clone();
                            }
                            object
                        })
                        .collect();
                    new_ids = copies.iter().map(|o| o.id.clone()).collect();
                    // Insert beyond the containing group so copying an individual
                    // member cannot split the original group's contiguous stack.
                    let insertion = group_members(&artwork, *selected.last().unwrap())
                        .last()
                        .copied()
                        .unwrap()
                        + 1;
                    artwork.objects.splice(insertion..insertion, copies);
                }
                "visibility" => {
                    let visible = !artwork.objects[scene.active].visible;
                    for i in &selected {
                        artwork.objects[*i].visible = visible;
                    }
                }
                "forward" | "backward" => {
                    (artwork, _) = omuse::vector_scene_ops::reorder(
                        &artwork,
                        &selected.iter().copied().collect::<Vec<_>>(),
                        action == "forward",
                    )?;
                }
                _ => unreachable!(),
            }
            let selection = artwork
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| new_ids.contains(&o.id))
                .map(|(i, _)| i)
                .collect();
            self.replace_scene_artwork(artwork, selection, window, cx)
        })();
        if let Err(error) = result {
            self.status = format!("Vector selection: {error:#}");
        }
        self.focus.focus(window, cx);
        cx.notify();
        true
    }
}
