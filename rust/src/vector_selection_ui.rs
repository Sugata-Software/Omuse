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

/// Conservative painted bounds. A mask is a silhouette, not painted artwork;
/// content is intersected with its silhouette rather than its hidden geometry.
pub(super) fn clipped_object_bounds(
    artwork: &VectorScene,
    index: usize,
) -> Option<(VectorPoint, VectorPoint)> {
    let object = artwork.objects.get(index)?;
    if object.is_clip || !object.visible || object.opacity <= 0. {
        return None;
    }
    let (mut low, mut high) = object_bounds(object)?;
    if let Some(mask_id) = &object.clip_to {
        let mask = artwork.objects.iter().find(|o| &o.id == mask_id)?;
        let (a, b) = mask.path.bounds()?;
        low.x = low.x.max(a.x);
        low.y = low.y.max(a.y);
        high.x = high.x.min(b.x);
        high.y = high.y.min(b.y);
        if low.x > high.x || low.y > high.y {
            return None;
        }
    }
    Some((low, high))
}

/// Normal picking treats a clip as one unit. Ctrl-click and Edit contents keep
/// direct access to the unchanged, full geometry of each content object.
pub(super) fn scoped_selection_members(
    scene: &super::scene::SceneDraft,
    index: usize,
) -> BTreeSet<usize> {
    let mut selected = super::vector_isolation_ui::members(scene, index);
    let masks: BTreeSet<_> = selected
        .iter()
        .filter_map(|i| {
            let object = scene.artwork.objects.get(*i)?;
            if object.is_clip {
                Some(object.id.as_str())
            } else {
                object.clip_to.as_deref()
            }
        })
        .collect();
    for (i, object) in scene.artwork.objects.iter().enumerate() {
        if super::vector_isolation_ui::in_scope(&scene.isolation, object)
            && ((object.is_clip && masks.contains(object.id.as_str()))
                || object
                    .clip_to
                    .as_deref()
                    .is_some_and(|id| masks.contains(id)))
        {
            selected.insert(i);
        }
    }
    selected
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

/// Ungroup only a child group of the current isolation level. Ancestors remain intact.
fn ungroup_scoped(
    source: &VectorScene,
    selected: &[usize],
    depth: usize,
) -> Result<(VectorScene, Vec<usize>)> {
    if depth == 0 {
        return omuse::vector_scene_ops::ungroup(source, selected);
    }
    let groups: BTreeSet<_> = selected
        .iter()
        .filter_map(|index| {
            source
                .objects
                .get(*index)?
                .groups
                .get(depth)
                .map(|group| group.id.clone())
        })
        .collect();
    ensure!(
        !groups.is_empty(),
        "Select a nested group, or leave isolation to ungroup its parent"
    );
    let mut artwork = source.clone();
    let mut selection: BTreeSet<_> = selected.iter().copied().collect();
    for (index, object) in artwork.objects.iter_mut().enumerate() {
        if object
            .groups
            .get(depth)
            .is_some_and(|group| groups.contains(&group.id))
        {
            object.groups.remove(depth);
            selection.insert(index);
        }
    }
    artwork.validate()?;
    Ok((artwork, selection.into_iter().collect()))
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
                "ungroup" => ungroup_scoped(&source, &selected, scene.isolation.len())?,
                "transform" => {
                    let step = self.scene_transform_step(cx)?;
                    (step.apply(&source, &selected)?, selected.clone())
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
            let remembered = (action == "transform")
                .then(|| self.scene_transform_step(cx))
                .transpose()?;
            self.replace_scene_artwork(artwork, selection.into_iter().collect(), window, cx)?;
            if let Some(step) = remembered.filter(|step| !step.is_identity()) {
                self.vector_draft
                    .as_mut()
                    .unwrap()
                    .scene
                    .as_mut()
                    .unwrap()
                    .last_transform = Some(step);
            }
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
                    && super::vector_isolation_ui::in_scope(&scene.isolation, o)
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
            "vector-transform-again" => self.scene_quick_action("again", window, cx),
            "vector-transform-copy" => self.scene_quick_action("transform-copy", window, cx),
            "vector-repeat-transform-copy" => self.scene_quick_action("repeat-copy", window, cx),
            "vector-swap-fill-stroke" | "swap-colors" => {
                self.scene_quick_action("swap", window, cx)
            }
            "vector-add-polygon" | "vector-add-star" => {
                self.scene_add_regular_shape(name == "vector-add-star", window, cx)
            }
            "vector-shape-builder" => self.start_scene_builder(window, cx),
            "vector-repeat-grid" | "vector-repeat-radial" => {
                self.scene_repeat_action(name == "vector-repeat-radial", window, cx);
            }
            "vector-simplify" | "vector-offset" | "vector-outline-stroke" => {
                self.scene_geometry_action(name, window, cx);
            }
            "vector-split-node" | "vector-join-endpoints" => {
                self.vector_topology_action(name == "vector-join-endpoints", cx);
                self.focus.focus(window, cx);
            }
            "select-all" | "deselect" => {
                let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
                let selected = if name == "select-all" {
                    scene
                        .artwork
                        .objects
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| {
                            o.visible && super::vector_isolation_ui::in_scope(&scene.isolation, o)
                        })
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
        let mut repeat = div().flex().flex_wrap().gap_1();
        for (id, label, action, needs_previous) in [
            (
                "vector-transform-copy",
                "Transform a copy",
                "transform-copy",
                false,
            ),
            ("vector-transform-again", "Repeat transform", "again", true),
            (
                "vector-repeat-transform-copy",
                "Repeat copy · Ctrl+Shift+D",
                "repeat-copy",
                true,
            ),
            (
                "vector-swap-fill-stroke",
                "Swap fill / stroke · X",
                "swap",
                false,
            ),
        ] {
            repeat = repeat.child(
                button(id, label, ButtonVariant::Outline, cx)
                    .disabled(
                        self.busy
                            || count == 0
                            || (needs_previous && scene.last_transform.is_none()),
                    )
                    .debug_selector(move || id.into())
                    .on_click(
                        cx.listener(move |this, _, w, cx| this.scene_quick_action(action, w, cx)),
                    ),
            );
        }
        controls = controls.child(repeat).child(inspector_ui::panel_note("Repeats use the current selection centre. Each copy stays editable; Undo restores the previous artwork.", cx));
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
            .child(self.vector_geometry_controls(window, cx))
            .child(self.vector_repeat_controls(window, cx))
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
        scene.builder = None;
        scene.selected_objects = selected
            .into_iter()
            .filter(|i| {
                scene.artwork.objects.get(*i).is_some_and(|object| {
                    super::vector_isolation_ui::in_scope(&scene.isolation, object)
                })
            })
            .collect();
        if scene
            .simplify
            .session
            .as_ref()
            .is_some_and(|s| s.selected != scene.selected_objects)
        {
            scene.simplify.session = None;
        }
        if scene.artwork.objects.is_empty() {
            return;
        }
        scene.active = preferred
            .filter(|i| scene.selected_objects.contains(i))
            .or_else(|| scene.selected_objects.last().copied())
            .unwrap_or(scene.active)
            .min(scene.artwork.objects.len() - 1);
        let object = &scene.artwork.objects[scene.active];
        if scene.selected_objects.len() != 1 || object.fill_gradient.is_none() {
            scene.gradient_edit = false;
        }
        scene.gradient_drag = None;
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
        // Pressing a member of an existing direct selection starts a drag of
        // that exact selection. Expanding it again here silently reselected a
        // clipping mask after Edit contents (or Ctrl-click selected one object).
        let preserve_selection = preserve && !individual && scene.selected_objects.contains(&index);
        let members = if individual || preserve_selection {
            [index].into_iter().collect()
        } else {
            scoped_selection_members(scene, index)
        };
        let mut selected = if additive || preserve_selection {
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
            .filter(|i| {
                scene.artwork.objects.get(*i).is_some_and(|object| {
                    super::vector_isolation_ui::in_scope(&scene.isolation, object)
                })
            })
            .collect();
        scene.drag_origin = None;
        scene.marquee = None;
        let object = &scene.artwork.objects[scene.active];
        if scene.selected_objects.len() != 1 || object.fill_gradient.is_none() {
            scene.gradient_edit = false;
        }
        scene.gradient_drag = None;
        if object.text_path.is_some() {
            scene.mode = super::scene::SceneMode::Select;
        }
        draft.path = object.path.clone();
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = None;
        draft.drag = None;
        self.reconcile_vector_isolation();
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
        let has_selection = !selected.is_empty();
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
        if scene.drag_origin.is_none() && has_selection && (dx != 0. || dy != 0.) {
            scene.last_transform = Some(super::vector_quick_ui::TransformStep {
                dx,
                dy,
                scale: 1.,
                degrees: 0.,
            });
        }
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
        for (i, _object) in
            scene.artwork.objects.iter().enumerate().filter(|(_, o)| {
                o.visible && super::vector_isolation_ui::in_scope(&scene.isolation, o)
            })
        {
            if clipped_object_bounds(&scene.artwork, i)
                .is_some_and(|(a, b)| a.x >= left && b.x <= right && a.y >= top && b.y <= bottom)
            {
                let members = scoped_selection_members(scene, i);
                // Whole clips use all visible clipped contents for containment.
                // One small child must not select another child outside the box.
                let contains_clip = members.iter().any(|i| scene.artwork.objects[*i].is_clip);
                if !contains_clip
                    || members
                        .iter()
                        .filter_map(|i| clipped_object_bounds(&scene.artwork, *i))
                        .all(|(a, b)| a.x >= left && b.x <= right && a.y >= top && b.y <= bottom)
                {
                    selected.extend(members);
                }
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
        let result =
            (|| -> Result<()> {
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
                        ensure!(
                            !artwork.objects.iter().any(|object| !selected_ids
                                .contains(&object.id)
                                && object
                                    .clip_to
                                    .as_ref()
                                    .is_some_and(|id| selected_ids.contains(id))),
                            "Release the clip before removing its source shape"
                        );
                        artwork.objects.retain(|o| !selected_ids.contains(&o.id));
                        let retained_masks: BTreeSet<_> = artwork
                            .objects
                            .iter()
                            .filter_map(|object| object.clip_to.clone())
                            .collect();
                        for object in &mut artwork.objects {
                            if object.is_clip && !retained_masks.contains(&object.id) {
                                object.is_clip = false;
                            }
                        }
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
                        let mut copies: Vec<_> = selected
                            .iter()
                            .map(|i| {
                                let mut object = artwork.objects[*i].clone();
                                object.id = uuid::Uuid::new_v4().to_string();
                                object.name = format!("{} copy", object.name);
                                for group in object.groups.iter_mut().skip(scene.isolation.len()) {
                                    group.id = group_ids
                                        .entry(group.id.clone())
                                        .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                                        .clone();
                                }
                                object
                            })
                            .collect();
                        omuse::vector_clip::repair_clones(
                            &artwork,
                            &selected.iter().copied().collect::<Vec<_>>(),
                            &mut copies,
                        )?;
                        new_ids = copies.iter().map(|o| o.id.clone()).collect();
                        // Insert beyond the containing group so copying an individual
                        // member cannot split the original group's contiguous stack.
                        let insertion = if scene.isolation.is_empty() {
                            group_members(&artwork, *selected.last().unwrap())
                                .last()
                                .copied()
                                .unwrap()
                                + 1
                        } else {
                            artwork
                                .objects
                                .iter()
                                .rposition(|object| {
                                    super::vector_isolation_ui::in_scope(&scene.isolation, object)
                                })
                                .context("Isolation group changed")?
                                + 1
                        };
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
