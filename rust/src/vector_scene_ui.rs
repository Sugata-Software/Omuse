//! Several editable objects share one layer cache. Draft previews are bounded
//! and coalesced; document geometry and pixels change together only on Apply.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_scene::{VectorObject, VectorScene};

pub(super) struct SceneDraft {
    pub(super) artwork: VectorScene,
    pub(super) active: usize,
    pub(super) is_new: bool,
    pub(super) display: Option<crate::display_surface::DisplaySurface>,
    pub(super) preview_cancel: Option<Arc<AtomicBool>>,
    pub(super) original: VectorScene,
    pub(super) mode: SceneMode,
    pub(super) after: Option<AfterScene>,
    pub(super) undo: Vec<SceneEdit>,
    pub(super) redo: Vec<SceneEdit>,
    pub(super) pending_checkpoint: bool,
    pub(super) svg_exported: bool,
    pub(super) drag_origin: Option<VectorPoint>,
    requested: u64,
    pub(super) running: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneMode {
    Select,
    Nodes,
    Pen,
}

pub(super) struct SceneEdit {
    pub(super) artwork: VectorScene,
    pub(super) active: usize,
    pub(super) selected: Option<(usize, usize)>,
    pub(super) mode: SceneMode,
}

pub(super) enum AfterScene {
    Command(String),
    Tool(Tool),
    Layer(String, SelectionAction, bool, bool),
    Inspector(studio_ui::InspectorTab),
    Request(Pending),
    Import(Vec<PathBuf>),
}

impl VectorDraft {
    pub(in crate::ui) fn is_scene(&self) -> bool {
        self.scene.is_some()
    }

    pub(super) fn scene_snapshot(&self) -> Result<VectorScene> {
        let draft = self.scene.as_ref().context("Vector artwork draft closed")?;
        let mut artwork = draft.artwork.clone();
        let object = artwork
            .objects
            .get_mut(draft.active)
            .context("Select an object")?;
        object.path = self.path.clone();
        object.fill = if object.fill.is_none() && self.fill == [0; 4] {
            None
        } else {
            Some(self.fill)
        };
        object.stroke = self.stroke;
        artwork.validate()?;
        Ok(artwork)
    }

    pub(super) fn store_scene_object(&mut self) {
        if let Some(scene) = self.scene.as_mut()
            && let Some(object) = scene.artwork.objects.get_mut(scene.active)
        {
            object.path = self.path.clone();
            object.fill = if object.fill.is_none() && self.fill == [0; 4] {
                None
            } else {
                Some(self.fill)
            };
            object.stroke = self.stroke;
        }
    }
}

fn new_object(name: &str, path: VectorPath, color: [u8; 4]) -> VectorObject {
    VectorObject {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        path,
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some(color),
        stroke: None,
        opacity: 1.,
        visible: true,
    }
}

fn shape_path(kind: &str, width: u32, height: u32) -> VectorPath {
    let w = width as f32;
    let h = height as f32;
    let (x0, y0, x1, y1) = (w * 0.2, h * 0.2, w * 0.8, h * 0.8);
    let anchor = |x, y| Anchor {
        position: VectorPoint { x, y },
        incoming: None,
        outgoing: None,
    };
    let anchors = match kind {
        "rectangle" => vec![
            anchor(x0, y0),
            anchor(x1, y0),
            anchor(x1, y1),
            anchor(x0, y1),
        ],
        "ellipse" => {
            let (cx, cy, rx, ry) = (
                (x0 + x1) / 2.,
                (y0 + y1) / 2.,
                (x1 - x0) / 2.,
                (y1 - y0) / 2.,
            );
            let k = 0.552_284_8;
            vec![
                Anchor {
                    position: VectorPoint { x: cx, y: y0 },
                    incoming: Some(VectorPoint {
                        x: cx - rx * k,
                        y: y0,
                    }),
                    outgoing: Some(VectorPoint {
                        x: cx + rx * k,
                        y: y0,
                    }),
                },
                Anchor {
                    position: VectorPoint { x: x1, y: cy },
                    incoming: Some(VectorPoint {
                        x: x1,
                        y: cy - ry * k,
                    }),
                    outgoing: Some(VectorPoint {
                        x: x1,
                        y: cy + ry * k,
                    }),
                },
                Anchor {
                    position: VectorPoint { x: cx, y: y1 },
                    incoming: Some(VectorPoint {
                        x: cx + rx * k,
                        y: y1,
                    }),
                    outgoing: Some(VectorPoint {
                        x: cx - rx * k,
                        y: y1,
                    }),
                },
                Anchor {
                    position: VectorPoint { x: x0, y: cy },
                    incoming: Some(VectorPoint {
                        x: x0,
                        y: cy + ry * k,
                    }),
                    outgoing: Some(VectorPoint {
                        x: x0,
                        y: cy - ry * k,
                    }),
                },
            ]
        }
        _ => Vec::new(),
    };
    VectorPath {
        subpaths: vec![Subpath {
            anchors,
            closed: matches!(kind, "rectangle" | "ellipse"),
        }],
        fill_rule: Default::default(),
    }
}

// The node editor works in scene coordinates. Baking a supported transform
// into a disposable draft keeps the original scene unchanged until Apply.
fn bake_transforms(artwork: &mut VectorScene) -> Result<()> {
    artwork.validate()?;
    for object in &mut artwork.objects {
        let [a, b, c, d, e, f] = object.transform;
        for subpath in &mut object.path.subpaths {
            for anchor in &mut subpath.anchors {
                for point in [
                    Some(&mut anchor.position),
                    anchor.incoming.as_mut(),
                    anchor.outgoing.as_mut(),
                ]
                .into_iter()
                .flatten()
                {
                    let (x, y) = (point.x, point.y);
                    point.x = a * x + c * y + e;
                    point.y = b * x + d * y + f;
                }
            }
        }
        if let Some(stroke) = &mut object.stroke {
            stroke.width *= a.hypot(b);
        }
        object.transform = [1., 0., 0., 1., 0., 0.];
    }
    artwork.validate()
}

impl EditorView {
    pub(in crate::ui) fn open_vector_scene(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vector_scene_active() {
            return;
        }
        if self.busy
            || self.inline_text.is_some()
            || self.crop.is_some()
            || self.editor.floating_selection_layer().is_some()
        {
            self.status = "Finish the current operation before opening vector artwork.".into();
            cx.notify();
            return;
        }
        let existing = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|layer| layer.vector_scene.clone());
        if existing.is_some()
            && let Some(reason) = self.layer_action_unavailable("edit-object")
        {
            self.status = reason.into();
            cx.notify();
            return;
        }
        let is_new = existing.is_none();
        let mut artwork = existing
            .map(|scene| (*scene).clone())
            .unwrap_or_else(|| VectorScene {
                version: 1,
                width: self.editor.document.width,
                height: self.editor.document.height,
                objects: vec![new_object(
                    "Path 1",
                    shape_path(
                        "path",
                        self.editor.document.width,
                        self.editor.document.height,
                    ),
                    self.editor.brush.color,
                )],
            });
        if artwork.objects.is_empty() {
            artwork.objects.push(new_object(
                "Path 1",
                shape_path("path", artwork.width, artwork.height),
                self.editor.brush.color,
            ));
        }
        if let Err(error) = bake_transforms(&mut artwork) {
            self.status = format!("Vector artwork: {error:#}");
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        self.clear_vector(cx);
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        let object = &artwork.objects[0];
        self.vector_draft = Some(VectorDraft {
            layer: self.editor.active_layer.clone(),
            revision: self.editor.revision(),
            identity: (self.editor.instance_id(), self.create.epoch),
            cancel: Arc::new(AtomicBool::new(false)),
            window: window.window_handle(),
            state: None,
            dimensions: (artwork.width, artwork.height),
            path: object.path.clone(),
            as_mask: false,
            fill: object.fill.unwrap_or([0; 4]),
            stroke: object.stroke,
            source_preview: None,
            preview_bounds: Rc::new(Cell::new(Bounds::default())),
            selected: None,
            drag: None,
            scene: Some(SceneDraft {
                original: artwork.clone(),
                mode: if is_new {
                    SceneMode::Pen
                } else {
                    SceneMode::Select
                },
                after: None,
                undo: Vec::new(),
                redo: Vec::new(),
                pending_checkpoint: false,
                svg_exported: false,
                artwork,
                active: 0,
                is_new,
                display: None,
                preview_cancel: None,
                drag_origin: None,
                requested: 0,
                running: false,
            }),
        });
        self.load_scene_style(window, cx);
        self.dialog = Dialog::None;
        self.tool = Tool::Move;
        self.inspector_tab = studio_ui::InspectorTab::Layers;
        self.inspector_visible = true;
        self.focus.focus(window, cx);
        self.status = if is_new {
            "Draw on the canvas or add a shape · Enter to keep · Escape to discard"
        } else {
            "Click an object to move it · Nodes to edit curves · Enter to keep"
        }
        .into();
        self.vector_scene_changed(cx);
        cx.notify();
    }

    pub(super) fn load_scene_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let Some(scene) = draft.scene.as_ref() else {
            return;
        };
        let hex = |c: [u8; 4]| format!("#{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3]);
        let opacity = scene.artwork.objects[scene.active].opacity;
        for (input, value) in self.detail_inputs.iter().zip([
            hex(draft.fill),
            hex(draft.stroke.map_or(self.editor.brush.color, |s| s.color)),
            draft.stroke.map_or(0., |s| s.width).to_string(),
            (opacity * 100.).to_string(),
        ]) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    pub(super) fn vector_scene_changed(&mut self, cx: &mut Context<Self>) {
        self.finish_scene_checkpoint();
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return;
        };
        if let Some(cancel) = &scene.preview_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        scene.requested = scene.requested.wrapping_add(1);
        self.start_scene_preview(cx);
    }

    pub(in crate::ui) fn start_scene_preview(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let Some(scene) = draft.scene.as_ref() else {
            return;
        };
        if scene.running || !self.vector_scene_current() {
            return;
        }
        let artwork = match draft.scene_snapshot() {
            Ok(scene) => scene,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let requested = scene.requested;
        let identity = (draft.identity, draft.revision);
        let layer = (!scene.is_new).then(|| draft.layer.clone());
        let document = self.editor.document.clone();
        let proof = self.proof_settings.clone();
        let token = draft.cancel.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        scene.preview_cancel = Some(cancel.clone());
        scene.running = true;
        let task = cx.background_executor().spawn(async move {
            // Coalesce rapid pointer/style changes. All expensive work stays off
            // the UI thread; the main canvas wireframe follows the pointer immediately.
            let pixels = omuse::vector_scene_preview::composite(
                &document,
                layer.as_deref(),
                &artwork,
                &cancel,
            )?;
            if proof.enabled {
                omuse::proofing::render(&pixels, &proof)
            } else {
                Ok(pixels)
            }
        });
        cx.spawn(async move |view, cx| {
            let pixels = task.await;
            let _ = view.update(cx, |this, cx| {
                let current = this.vector_scene_current()
                    && (
                        (this.editor.instance_id(), this.create.epoch),
                        this.editor.revision(),
                    ) == identity;
                let Some(draft) = this
                    .vector_draft
                    .as_mut()
                    .filter(|d| Arc::ptr_eq(&d.cancel, &token))
                else {
                    return;
                };
                let Some(scene) = draft.scene.as_mut() else {
                    return;
                };
                scene.running = false;
                scene.preview_cancel = None;
                if !current || token.load(Ordering::Relaxed) {
                    return;
                }
                let stale = scene.requested != requested;
                if !stale && !this.busy {
                    match pixels {
                        Ok(pixels) => {
                            if let Some(display) = &mut scene.display {
                                display.replace(&pixels);
                            } else {
                                scene.display =
                                    Some(crate::display_surface::DisplaySurface::new(&pixels));
                            }
                        }
                        Err(error) => this.status = format!("Vector preview: {error:#}"),
                    }
                }
                if stale {
                    this.start_scene_preview(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn scene_action(
        &mut self,
        action: &str,
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
        if !matches!(action, "previous" | "next") {
            self.scene_checkpoint();
        }
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        draft.store_scene_object();
        let Some(scene) = draft.scene.as_mut() else {
            return;
        };
        let count = scene.artwork.objects.len();
        match action {
            "previous" => scene.active = scene.active.saturating_sub(1),
            "next" => scene.active = (scene.active + 1).min(count - 1),
            "backward" if scene.active > 0 => {
                scene.artwork.objects.swap(scene.active, scene.active - 1);
                scene.active -= 1;
            }
            "forward" if scene.active + 1 < count => {
                scene.artwork.objects.swap(scene.active, scene.active + 1);
                scene.active += 1;
            }
            "remove" if count > 1 => {
                scene.artwork.objects.remove(scene.active);
                scene.active = scene.active.min(count - 2);
            }
            "remove" => {
                scene.artwork.objects[0].path =
                    shape_path("path", draft.dimensions.0, draft.dimensions.1);
                scene.artwork.objects[0].name = "Path 1".into();
                scene.mode = SceneMode::Pen;
            }
            "visibility" => {
                scene.artwork.objects[scene.active].visible =
                    !scene.artwork.objects[scene.active].visible
            }
            "path" | "rectangle" | "ellipse" | "duplicate" => {
                if count >= omuse::vector_scene::MAX_SCENE_OBJECTS {
                    self.status = "This artwork has reached its object limit.".into();
                    cx.notify();
                    return;
                }
                let reuse_empty = count == 1
                    && action != "duplicate"
                    && scene.artwork.objects[0]
                        .path
                        .subpaths
                        .iter()
                        .all(|s| s.anchors.is_empty());
                let object = if action == "duplicate" {
                    let mut object = scene.artwork.objects[scene.active].clone();
                    object.id = uuid::Uuid::new_v4().to_string();
                    object.name = format!("{} copy", object.name);
                    object
                } else {
                    let name = match action {
                        "rectangle" => "Rectangle",
                        "ellipse" => "Ellipse",
                        _ => "Path",
                    };
                    new_object(
                        &format!("{name} {}", if reuse_empty { 1 } else { count + 1 }),
                        shape_path(action, draft.dimensions.0, draft.dimensions.1),
                        draft.fill,
                    )
                };
                let at = if reuse_empty { 0 } else { scene.active + 1 };
                if reuse_empty {
                    scene.artwork.objects[0] = object;
                } else {
                    scene.artwork.objects.insert(at, object);
                }
                scene.active = at;
            }
            _ => {}
        }
        let object = &scene.artwork.objects[scene.active];
        draft.path = object.path.clone();
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = None;
        draft.drag = None;
        scene.drag_origin = None;
        if matches!(action, "path" | "rectangle" | "ellipse") {
            scene.mode = if action == "path" {
                SceneMode::Pen
            } else {
                SceneMode::Select
            };
            self.tool = Tool::Move;
        }
        self.load_scene_style(window, cx);
        self.vector_scene_changed(cx);
        cx.notify();
    }

    pub(super) fn vector_scene_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return div().into_any_element();
        };
        let count = scene.artwork.objects.len();
        let active = scene.active;
        let object = &scene.artwork.objects[active];
        let label = format!(
            "Object {} / {} · {}{}",
            active + 1,
            count,
            object.name,
            if scene.running {
                " · Updating preview…"
            } else {
                ""
            }
        );
        let visible = object.visible;
        let mut actions = div().flex().flex_wrap().gap_2();
        for (action, label) in [
            ("previous", "Previous"),
            ("next", "Next"),
            ("duplicate", "Duplicate"),
            ("remove", "Remove"),
            ("backward", "Lower"),
            ("forward", "Raise"),
            (
                "visibility",
                if visible {
                    "Hide object"
                } else {
                    "Show object"
                },
            ),
        ] {
            let id = format!("scene-{action}");
            let disabled = self.busy
                || match action {
                    "previous" | "backward" => active == 0,
                    "next" | "forward" => active + 1 == count,
                    "path" | "rectangle" | "ellipse" | "duplicate" => {
                        count >= omuse::vector_scene::MAX_SCENE_OBJECTS
                    }
                    _ => false,
                };
            actions = actions.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(disabled)
                .debug_selector(move || id.clone())
                .on_click(
                    cx.listener(move |this, _, window, cx| this.scene_action(action, window, cx)),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(label))
            .child(actions)
            .into_any_element()
    }

    pub(super) fn apply_vector_scene(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        if !self.vector_scene_current() {
            self.status = "Document changed. Reopen vector artwork before applying.".into();
            cx.notify();
            return;
        }
        let artwork = match draft.scene_snapshot() {
            Ok(artwork) => artwork,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let scene = draft.scene.as_ref().unwrap();
        let is_new = scene.is_new;
        if let Some(cancel) = &scene.preview_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if artwork == scene.original
            || (is_new
                && artwork
                    .objects
                    .iter()
                    .all(|o| o.path.subpaths.iter().all(|p| p.anchors.is_empty())))
        {
            let window = draft.window;
            self.clear_vector(cx);
            self.status = "Vector artwork unchanged.".into();
            let focus = self.focus.clone();
            cx.defer(move |cx| {
                let _ = cx.update_window(window, |_, window, cx| focus.focus(window, cx));
            });
            cx.notify();
            return;
        }
        let layer = draft.layer.clone();
        let revision = draft.revision;
        let identity = draft.identity;
        let window = draft.window;
        let cancel = draft.cancel.clone();
        let token = cancel.clone();
        self.busy = true;
        self.status = "Rendering vector artwork…".into();
        let task = cx.background_executor().spawn(async move {
            let pixels = artwork.render(&cancel)?;
            Ok::<_, anyhow::Error>((artwork, pixels))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if !this.vector_scene_current()
                    || (this.editor.instance_id(), this.create.epoch) != identity
                    || !this
                        .vector_draft
                        .as_ref()
                        .is_some_and(|draft| Arc::ptr_eq(&draft.cancel, &token))
                {
                    return;
                }
                this.busy = false;
                let applied = result.and_then(|(artwork, pixels)| {
                    if is_new {
                        this.editor
                            .insert_vector_scene("Vector artwork", revision, artwork, pixels)
                            .map(|id| Some(id))
                    } else {
                        this.editor
                            .replace_vector_scene(&layer, revision, artwork, pixels)
                            .map(|_| None)
                    }
                });
                match applied {
                    Ok(id) => {
                        let after = this
                            .vector_draft
                            .as_mut()
                            .and_then(|d| d.scene.as_mut())
                            .and_then(|s| s.after.take());
                        this.clear_vector(cx);
                        this.dialog = Dialog::None;
                        this.dialog_generation = this.dialog_generation.wrapping_add(1);
                        if let Some(id) = id {
                            this.select_layer_ids(vec![id]);
                        }
                        this.changed(cx);
                        this.status =
                            "Editable vector artwork applied · Undo restores the previous document"
                                .into();
                        let view = cx.entity().downgrade();
                        cx.defer(move |cx| {
                            let _ = cx.update_window(window, |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.focus.focus(window, cx);
                                    if let Some(after) = after {
                                        this.resume_after_vector(after, window, cx);
                                    }
                                });
                            });
                        });
                    }
                    Err(error) => {
                        if let Some(scene) =
                            this.vector_draft.as_mut().and_then(|d| d.scene.as_mut())
                        {
                            scene.after = None;
                        }
                        this.status = format!("Vector artwork: {error:#}");
                        this.start_scene_preview(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::ui) fn prepare_vector_scene_inspection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.open_vector_scene(window, cx);
        ensure!(
            self.vector_draft
                .as_ref()
                .is_some_and(|draft| draft.scene.is_some()),
            "Vector artwork did not open"
        );
        self.scene_action("rectangle", window, cx);
        self.detail_inputs[0].update(cx, |input, cx| input.set_value("#D58049FF", window, cx));
        self.update_vector_style(cx)?;
        self.scene_action("ellipse", window, cx);
        self.detail_inputs[0].update(cx, |input, cx| input.set_value("#547A8EFF", window, cx));
        self.detail_inputs[3].update(cx, |input, cx| input.set_value("75", window, cx));
        self.update_vector_style(cx)?;
        if let Some(draft) = self.vector_draft.as_mut() {
            for subpath in &mut draft.path.subpaths {
                for anchor in &mut subpath.anchors {
                    for p in [
                        Some(&mut anchor.position),
                        anchor.incoming.as_mut(),
                        anchor.outgoing.as_mut(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        p.x += draft.dimensions.0 as f32 * 0.12;
                        p.y -= draft.dimensions.1 as f32 * 0.08;
                    }
                }
            }
        }
        self.vector_scene_changed(cx);
        self.status = "Two objects on the shared canvas · Enter to keep · Escape to discard".into();
        Ok(())
    }
}
