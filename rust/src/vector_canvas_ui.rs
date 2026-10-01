//! Vector sessions live on the ordinary canvas. Expensive compositing is
//! disposable; only a completed, revision-fenced edit enters document history.
use super::super::inspector_ui::{panel_note, panel_section};
use super::scene::{AfterScene, SceneEdit, SceneMode};
use super::*;

const SEGMENT_PICK_TOLERANCE: f32 = 8.;
const MAX_SEGMENT_PICK_WORK: usize = 524_288;

#[derive(Clone, Copy)]
struct SegmentPick {
    subpath: usize,
    segment: usize,
    t: f32,
    distance: f32,
}

#[derive(Clone, Copy)]
struct CubicPiece {
    points: [VectorPoint; 4],
    t0: f32,
    t1: f32,
    depth: u8,
}

fn screen_line_distance(p: VectorPoint, a: VectorPoint, b: VectorPoint) -> (f32, f32) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let denominator = dx * dx + dy * dy;
    let t = if denominator <= 1e-12 {
        0.
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / denominator).clamp(0., 1.)
    };
    ((p.x - (a.x + dx * t)).hypot(p.y - (a.y + dy * t)), t)
}

fn split_cubic(piece: CubicPiece) -> (CubicPiece, CubicPiece) {
    let [p0, p1, p2, p3] = piece.points;
    let midpoint = |a: VectorPoint, b: VectorPoint| VectorPoint {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    };
    let a = midpoint(p0, p1);
    let b = midpoint(p1, p2);
    let c = midpoint(p2, p3);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let m = midpoint(d, e);
    let tm = (piece.t0 + piece.t1) * 0.5;
    (
        CubicPiece {
            points: [p0, a, d, m],
            t0: piece.t0,
            t1: tm,
            depth: piece.depth + 1,
        },
        CubicPiece {
            points: [m, e, c, p3],
            t0: tm,
            t1: piece.t1,
            depth: piece.depth + 1,
        },
    )
}

pub(in crate::ui) struct CanvasVectorOverlay {
    path: VectorPath,
    selected: Option<(usize, usize)>,
    dimensions: (u32, u32),
    show_nodes: bool,
}

impl CanvasVectorOverlay {
    pub(in crate::ui) fn paint(
        self,
        rect: Bounds<Pixels>,
        viewport: Bounds<Pixels>,
        window: &mut Window,
    ) {
        window.with_content_mask(Some(gpui_kit::ContentMask { bounds: viewport }), |window| {
            paint_vector_overlay(
                window,
                rect,
                self.dimensions,
                &self.path,
                self.selected,
                None,
                None,
                self.show_nodes,
            );
        });
    }
}

impl EditorView {
    fn scene_edit(&self) -> Option<SceneEdit> {
        let draft = self.vector_draft.as_ref()?;
        let scene = draft.scene.as_ref()?;
        Some(SceneEdit {
            artwork: draft.scene_snapshot().ok()?,
            active: scene.active,
            selected: draft.selected,
            mode: scene.mode,
        })
    }

    pub(super) fn scene_checkpoint(&mut self) {
        let Some(edit) = self.scene_edit() else {
            return;
        };
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        scene.pending_checkpoint = true;
        if scene
            .undo
            .last()
            .is_none_or(|last| last.artwork != edit.artwork)
        {
            scene.undo.push(edit);
        }
        trim_history(scene);
    }

    pub(super) fn finish_scene_checkpoint(&mut self) {
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let Some(scene) = &draft.scene else {
            return;
        };
        if !scene.pending_checkpoint {
            return;
        }
        if draft.scene_snapshot().is_ok_and(|current| {
            scene
                .undo
                .last()
                .is_none_or(|previous| previous.artwork != current)
        }) {
            let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            scene.redo.clear();
            scene.pending_checkpoint = false;
        }
    }

    fn scene_history_step(
        &mut self,
        undo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(current) = self.scene_edit() else {
            return false;
        };
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        let source = if undo {
            &mut scene.undo
        } else {
            &mut scene.redo
        };
        let edit = loop {
            let Some(edit) = source.pop() else {
                return false;
            };
            if edit.artwork != current.artwork {
                break edit;
            }
        };
        if undo {
            scene.redo.push(current);
        } else {
            scene.undo.push(current);
        }
        trim_history(scene);
        scene.pending_checkpoint = false;
        scene.artwork = edit.artwork;
        scene.active = edit.active;
        scene.mode = edit.mode;
        scene.drag_origin = None;
        let object = scene.artwork.objects[scene.active].clone();
        let draft = self.vector_draft.as_mut().unwrap();
        draft.path = object.path;
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = edit.selected;
        draft.drag = None;
        self.load_scene_style(window, cx);
        self.vector_scene_changed(cx);
        self.status = if undo {
            "Vector edit undone"
        } else {
            "Vector edit redone"
        }
        .into();
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(in crate::ui) fn vector_export_published(&mut self) {
        if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
            scene.svg_exported = true;
        }
    }

    pub(in crate::ui) fn stop_vector_drag(&mut self) {
        if let Some(draft) = self.vector_draft.as_mut() {
            draft.drag = None;
            if let Some(scene) = &mut draft.scene {
                scene.drag_origin = None;
            }
        }
    }

    pub(in crate::ui) fn vector_has_redo(&self) -> bool {
        self.vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| !s.redo.is_empty())
    }

    pub(in crate::ui) fn vector_scene_active(&self) -> bool {
        self.vector_draft
            .as_ref()
            .is_some_and(VectorDraft::is_scene)
    }

    pub(in crate::ui) fn vector_scene_current(&self) -> bool {
        self.vector_draft.as_ref().is_some_and(|d| {
            d.is_scene()
                && d.identity == (self.editor.instance_id(), self.create.epoch)
                && d.revision == self.editor.revision()
                && !d.cancel.load(Ordering::Relaxed)
        })
    }

    pub(in crate::ui) fn validate_vector_canvas(&mut self, cx: &mut Context<Self>) {
        if self.vector_scene_active() && !self.vector_scene_current() {
            self.clear_vector(cx);
            self.status = "Document changed; the unfinished vector edit was discarded.".into();
        }
    }

    pub(in crate::ui) fn vector_canvas_ready(&self) -> bool {
        self.vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| !s.running && s.display.is_some())
    }

    pub(in crate::ui) fn vector_canvas_display(
        &self,
    ) -> Option<&crate::display_surface::DisplaySurface> {
        self.vector_draft.as_ref()?.scene.as_ref()?.display.as_ref()
    }

    pub(in crate::ui) fn vector_canvas_overlay(&self) -> Option<CanvasVectorOverlay> {
        let draft = self.vector_draft.as_ref()?;
        let scene = draft.scene.as_ref()?;
        if !scene.artwork.objects[scene.active].visible {
            return None;
        }
        let mut path = draft.path.clone();
        if !scene.is_new {
            for sub in &mut path.subpaths {
                for anchor in &mut sub.anchors {
                    for p in [
                        Some(&mut anchor.position),
                        anchor.incoming.as_mut(),
                        anchor.outgoing.as_mut(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let (x, y) = self.editor.layer_to_canvas(&draft.layer, p.x, p.y)?;
                        *p = VectorPoint { x, y };
                    }
                }
            }
        }
        Some(CanvasVectorOverlay {
            path,
            selected: draft.selected,
            dimensions: (self.editor.document.width, self.editor.document.height),
            show_nodes: scene.mode != SceneMode::Select,
        })
    }

    pub(super) fn vector_canvas_point(&self, position: Point<Pixels>) -> Option<VectorPoint> {
        let draft = self.vector_draft.as_ref()?;
        let scene = draft.scene.as_ref()?;
        let (x, y) = self.coordinates(position);
        let (x, y) = if scene.is_new {
            (x, y)
        } else {
            self.editor.canvas_to_layer(&draft.layer, x, y)?
        };
        (x.is_finite() && y.is_finite() && x.abs() <= 1_000_000. && y.abs() <= 1_000_000.)
            .then_some(VectorPoint { x, y })
    }

    fn vector_screen_point(&self, p: VectorPoint) -> Option<Point<Pixels>> {
        let draft = self.vector_draft.as_ref()?;
        let scene = draft.scene.as_ref()?;
        let (x, y) = if scene.is_new {
            (p.x, p.y)
        } else {
            self.editor.layer_to_canvas(&draft.layer, p.x, p.y)?
        };
        let bounds = self.viewport.get();
        Some(point(
            bounds.origin.x
                + (bounds.size.width - px(self.editor.document.width as f32 * self.zoom)) / 2.
                + px(self.pan.0 + x * self.zoom),
            bounds.origin.y
                + (bounds.size.height - px(self.editor.document.height as f32 * self.zoom)) / 2.
                + px(self.pan.1 + y * self.zoom),
        ))
    }

    fn nearest_vector_segment(
        &self,
        position: Point<Pixels>,
        tolerance: f32,
    ) -> anyhow::Result<Option<SegmentPick>> {
        let path = &self.vector_draft.as_ref().unwrap().path;
        let target = VectorPoint {
            x: f32::from(position.x),
            y: f32::from(position.y),
        };
        let to_screen = |point: VectorPoint| {
            self.vector_screen_point(point).map(|point| VectorPoint {
                x: f32::from(point.x),
                y: f32::from(point.y),
            })
        };
        let mut best = None;
        let mut work = 0usize;
        let mut stack = Vec::with_capacity(32);
        for (subpath, sub) in path.subpaths.iter().enumerate() {
            let count = if sub.closed {
                sub.anchors.len()
            } else {
                sub.anchors.len().saturating_sub(1)
            };
            for segment in 0..count {
                let next = (segment + 1) % sub.anchors.len();
                let start = &sub.anchors[segment];
                let end = &sub.anchors[next];
                let (Some(p0), Some(p1), Some(p2), Some(p3)) = (
                    to_screen(start.position),
                    to_screen(start.outgoing.unwrap_or(start.position)),
                    to_screen(end.incoming.unwrap_or(end.position)),
                    to_screen(end.position),
                ) else {
                    continue;
                };
                stack.clear();
                stack.push(CubicPiece {
                    points: [p0, p1, p2, p3],
                    t0: 0.,
                    t1: 1.,
                    depth: 0,
                });
                while let Some(piece) = stack.pop() {
                    work += 1;
                    anyhow::ensure!(
                        work <= MAX_SEGMENT_PICK_WORK,
                        "Path is too detailed for interactive segment picking"
                    );
                    let [p0, p1, p2, p3] = piece.points;
                    let flatness = screen_line_distance(p1, p0, p3)
                        .0
                        .max(screen_line_distance(p2, p0, p3).0);
                    if flatness <= 0.5 || piece.depth >= 12 {
                        let (distance, line_t) = screen_line_distance(target, p0, p3);
                        if distance <= tolerance
                            && best.is_none_or(|current: SegmentPick| distance < current.distance)
                        {
                            best = Some(SegmentPick {
                                subpath,
                                segment,
                                t: (piece.t0 + (piece.t1 - piece.t0) * line_t)
                                    .clamp(0.000_1, 0.999_9),
                                distance,
                            });
                        }
                    } else {
                        let (left, right) = split_cubic(piece);
                        stack.push(right);
                        stack.push(left);
                    }
                }
            }
        }
        Ok(best)
    }

    pub(in crate::ui) fn vector_canvas_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.vector_scene_current() {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let Some(p) = self.vector_canvas_point(event.position) else {
            return;
        };
        let draft = self.vector_draft.as_ref().unwrap();
        let mode = draft.scene.as_ref().unwrap().mode;
        // Test handles in screen coordinates: hit targets stay nine pixels wide
        // with rotated, flipped or anisotropically scaled layers at any zoom.
        let mut nearest = None;
        let mut distance = 9.;
        if mode != SceneMode::Select {
            for (si, sub) in draft.path.subpaths.iter().enumerate() {
                for (ai, anchor) in sub.anchors.iter().enumerate() {
                    for (part, candidate) in [
                        (DragPart::Anchor, Some(anchor.position)),
                        (DragPart::Incoming, anchor.incoming),
                        (DragPart::Outgoing, anchor.outgoing),
                    ] {
                        if let Some(q) = candidate.and_then(|q| self.vector_screen_point(q)) {
                            let d = f32::from(q.x - event.position.x)
                                .hypot(f32::from(q.y - event.position.y));
                            if d <= distance {
                                distance = d;
                                nearest = Some((si, ai, part));
                            }
                        }
                    }
                }
            }
        }
        let close = mode == SceneMode::Pen
            && nearest.is_some_and(|(si, ai, part)| {
                part == DragPart::Anchor
                    && ai == 0
                    && si + 1 == draft.path.subpaths.len()
                    && draft.path.subpaths[si].anchors.len() >= 2
                    && !draft.path.subpaths[si].closed
            });
        let segment = if nearest.is_none()
            && (mode == SceneMode::Pen || (mode == SceneMode::Nodes && event.click_count >= 2))
        {
            match self.nearest_vector_segment(event.position, SEGMENT_PICK_TOLERANCE) {
                Ok(hit) => hit,
                Err(error) => {
                    self.status = error.to_string();
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        if close {
            self.scene_checkpoint();
            let (si, _, _) = nearest.unwrap();
            let draft = self.vector_draft.as_mut().unwrap();
            draft.path.subpaths[si].closed = true;
            draft.selected = Some((si, 0));
            draft.drag = None;
            self.vector_scene_changed(cx);
            self.status = "Path closed. Switch to Nodes to adjust anchors and handles.".into();
        } else if let Some((si, ai, part)) = nearest {
            self.scene_checkpoint();
            let draft = self.vector_draft.as_mut().unwrap();
            draft.selected = Some((si, ai));
            draft.begin_drag(si, ai, part, event.position);
        } else if let Some(hit) = segment {
            self.scene_checkpoint();
            let result = self.vector_draft.as_mut().unwrap().path.insert_on_segment(
                hit.subpath,
                hit.segment,
                hit.t,
            );
            match result {
                Ok(anchor) => {
                    let draft = self.vector_draft.as_mut().unwrap();
                    draft.selected = Some((hit.subpath, anchor));
                    draft.begin_drag(hit.subpath, anchor, DragPart::Anchor, event.position);
                    self.vector_scene_changed(cx);
                    self.status = "Inserted an anchor without changing the curve.".into();
                }
                Err(error) => self.status = error.to_string(),
            }
        } else if mode == SceneMode::Pen && !event.modifiers.alt {
            self.scene_checkpoint();
            let draft = self.vector_draft.as_mut().unwrap();
            if draft
                .path
                .subpaths
                .iter()
                .map(|p| p.anchors.len())
                .sum::<usize>()
                >= 100_000
            {
                return;
            }
            if draft.path.subpaths.last().is_none_or(|p| p.closed) {
                if draft.path.subpaths.len() >= 4096 {
                    return;
                }
                draft.path.subpaths.push(Subpath {
                    anchors: vec![],
                    closed: false,
                });
            }
            let si = draft.path.subpaths.len() - 1;
            draft.path.subpaths[si].anchors.push(Anchor {
                position: p,
                incoming: None,
                outgoing: None,
            });
            let ai = draft.path.subpaths[si].anchors.len() - 1;
            draft.selected = Some((si, ai));
            draft.begin_drag(si, ai, DragPart::NewNode, event.position);
            self.vector_scene_changed(cx);
        } else if mode == SceneMode::Nodes {
            // Direct Selection only edits geometry. Clicking away deselects a
            // node instead of unexpectedly dragging the whole object.
            let draft = self.vector_draft.as_mut().unwrap();
            draft.selected = None;
            draft.drag = None;
        } else {
            self.scene_checkpoint();
            let draft = self.vector_draft.as_ref().unwrap();
            let tolerance = if draft.scene.as_ref().unwrap().is_new {
                6. / self.zoom
            } else {
                self.editor
                    .layer_placement(&draft.layer)
                    .map(|b| {
                        6. / self.zoom
                            * (draft.dimensions.0 as f32 / b.width)
                                .max(draft.dimensions.1 as f32 / b.height)
                    })
                    .unwrap_or(6. / self.zoom)
            };
            let hit = draft
                .scene_snapshot()
                .and_then(|s| s.hit_test(p, tolerance))
                .ok()
                .flatten();
            if let Some(index) = hit {
                self.select_scene_object(index, window, cx);
                let draft = self.vector_draft.as_mut().unwrap();
                draft.scene.as_mut().unwrap().drag_origin = Some(p);
                draft.drag = None;
                // Double click enters node editing without opening another surface.
                if event.click_count == 2 {
                    draft.scene.as_mut().unwrap().mode = SceneMode::Nodes;
                }
            } else if event.modifiers.alt {
                // Alt-drag explicitly moves the selected object, including a
                // transparent object whose geometry is visible as an outline.
                self.vector_draft
                    .as_mut()
                    .unwrap()
                    .scene
                    .as_mut()
                    .unwrap()
                    .drag_origin = Some(p);
            } else {
                self.vector_draft.as_mut().unwrap().selected = None;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn select_scene_object(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        draft.store_scene_object();
        let scene = draft.scene.as_mut().unwrap();
        let Some(object) = scene.artwork.objects.get(index) else {
            return;
        };
        scene.active = index;
        scene.drag_origin = None;
        draft.path = object.path.clone();
        draft.fill = object.fill.unwrap_or([0; 4]);
        draft.stroke = object.stroke;
        draft.selected = None;
        draft.drag = None;
        self.load_scene_style(window, cx);
        cx.notify();
    }

    fn set_scene_mode(&mut self, mode: SceneMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(draft) = self.vector_draft.as_mut() {
            if let Some(scene) = draft.scene.as_mut() {
                scene.mode = mode;
                scene.drag_origin = None;
            }
            draft.drag = None;
            if mode == SceneMode::Select {
                // Move-mode arrows and Delete must act on the whole object,
                // never a node whose handles are no longer shown.
                draft.selected = None;
            }
        }
        self.tool = Tool::Move;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn cancel_vector_canvas(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exported = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| s.svg_exported);
        self.clear_vector(cx);
        self.status = if exported {
            "Vector edit discarded. The exported SVG file is already saved."
        } else {
            "Vector edit discarded; your document is unchanged."
        }
        .into();
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn finish_vector_then(
        &mut self,
        after: AfterScene,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.vector_scene_active() {
            return false;
        }
        if self.busy {
            if matches!(&after, AfterScene::Request(_)) {
                self.vector_draft
                    .as_mut()
                    .unwrap()
                    .scene
                    .as_mut()
                    .unwrap()
                    .after = Some(after);
                self.status =
                    "Finishing the current vector operation before changing files or closing…"
                        .into();
                cx.notify();
            }
            return true;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return true;
        }
        let draft = self.vector_draft.as_ref().unwrap();
        let scene = draft.scene.as_ref().unwrap();
        let unchanged = draft.scene_snapshot().is_ok_and(|s| {
            s == scene.original
                || (scene.is_new
                    && s.objects
                        .iter()
                        .all(|o| o.path.subpaths.iter().all(|p| p.anchors.is_empty())))
        });
        if unchanged {
            self.clear_vector(cx);
            self.resume_after_vector(after, window, cx);
        } else {
            self.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .after = Some(after);
            self.apply_vector(cx);
        }
        true
    }

    pub(super) fn resume_after_vector(
        &mut self,
        after: AfterScene,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match after {
            AfterScene::Command(name) => self.command(&name, window, cx),
            AfterScene::Tool(tool) => {
                self.tool = tool;
                self.focus.focus(window, cx);
                cx.notify();
            }
            AfterScene::Inspector(tab) => {
                self.inspector_tab = tab;
                self.focus.focus(window, cx);
                cx.notify();
            }
            AfterScene::Request(what) => self.request(what, window, cx),
            AfterScene::Import(paths) => self.import_photos_background(paths, window, cx),
            AfterScene::Layer(id, action, edit, menu) => {
                if self.editor.document.find_layer(&id).is_none() {
                    return;
                }
                self.paint_mask = false;
                self.layer_selection.click(id.clone(), action);
                if let Some(primary) = self.layer_selection.primary.clone() {
                    self.editor.active_layer = primary;
                }
                self.focus.focus(window, cx);
                if menu {
                    self.dialog = Dialog::LayerMenu;
                    self.modal_focus.focus(window, cx);
                } else if edit {
                    let adjustment = self.editor.document.find_layer(&id).is_some_and(|l| {
                        l.metadata.get("adjustment").is_some_and(|v| !v.is_null())
                    });
                    self.command(
                        if adjustment {
                            "edit-adjustment"
                        } else {
                            "edit-object"
                        },
                        window,
                        cx,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(in crate::ui) fn vector_before_command(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.vector_scene_active() {
            return false;
        }
        match name {
            "undo" | "redo" => {
                if !self.scene_history_step(name == "undo", window, cx) {
                    self.status = format!(
                        "No more vector changes to {name}. Finish this edit to use document history."
                    );
                    cx.notify();
                }
                true
            }
            "zoom-in" | "zoom-in-plus" | "zoom-out" | "fit" | "actual" | "toggle-panels"
            | "grid" | "guides" | "rulers" | "command-search" | "shortcuts" => false,
            "vector-scene" | "edit-object" => {
                self.inspector_visible = true;
                self.inspector_tab = studio_ui::InspectorTab::Layers;
                cx.notify();
                true
            }
            "vector-path" => {
                self.set_scene_mode(SceneMode::Pen, window, cx);
                true
            }
            "vector-nodes" => {
                self.set_scene_mode(SceneMode::Nodes, window, cx);
                true
            }
            "tool-move" => {
                self.set_scene_mode(SceneMode::Select, window, cx);
                true
            }
            "tool-hand" => {
                self.tool = Tool::Hand;
                cx.notify();
                true
            }
            "delete-content" | "delete-forward" => {
                if self.vector_draft.as_ref().unwrap().selected.is_some() {
                    self.vector_delete_selected(cx);
                } else {
                    self.scene_action("remove", window, cx);
                }
                true
            }
            _ if name.starts_with("nudge-") => {
                self.scene_checkpoint();
                let step = if name.ends_with("-large") { 10. } else { 1. };
                let (dx, dy) = if name.starts_with("nudge-left") {
                    (-step, 0.)
                } else if name.starts_with("nudge-right") {
                    (step, 0.)
                } else if name.starts_with("nudge-up") {
                    (0., -step)
                } else {
                    (0., step)
                };
                let draft = self.vector_draft.as_mut().unwrap();
                if draft.path.bounds().is_some_and(|(a, b)| {
                    [a.x + dx, a.y + dy, b.x + dx, b.y + dy]
                        .iter()
                        .any(|v| v.abs() > 1_000_000.)
                }) {
                    return true;
                }
                for (si, sub) in draft.path.subpaths.iter_mut().enumerate() {
                    for (ai, a) in sub.anchors.iter_mut().enumerate() {
                        if draft.selected.is_none_or(|selected| selected == (si, ai)) {
                            for p in [
                                Some(&mut a.position),
                                a.incoming.as_mut(),
                                a.outgoing.as_mut(),
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
                self.vector_scene_changed(cx);
                cx.notify();
                true
            }
            _ => self.finish_vector_then(AfterScene::Command(name.into()), window, cx),
        }
    }

    pub(in crate::ui) fn vector_before_tool(
        &mut self,
        tool: Tool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.vector_scene_active() {
            return false;
        }
        if matches!(tool, Tool::Move | Tool::Hand) {
            return self.vector_before_command(tool.studio_id(), window, cx);
        }
        self.finish_vector_then(AfterScene::Tool(tool), window, cx)
    }
    pub(in crate::ui) fn vector_before_layer(
        &mut self,
        id: String,
        action: SelectionAction,
        edit: bool,
        menu: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .vector_draft
            .as_ref()
            .is_some_and(|d| d.scene.as_ref().is_some_and(|s| !s.is_new) && d.layer == id)
            && action == SelectionAction::Replace
            && !edit
            && !menu
        {
            self.focus.focus(window, cx);
            return true;
        }
        self.finish_vector_then(AfterScene::Layer(id, action, edit, menu), window, cx)
    }
    pub(in crate::ui) fn vector_before_inspector(
        &mut self,
        tab: studio_ui::InspectorTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if tab == studio_ui::InspectorTab::Layers {
            return false;
        }
        self.finish_vector_then(AfterScene::Inspector(tab), window, cx)
    }
    pub(in crate::ui) fn vector_resume_after_exchange(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Some(after) = self
            .vector_draft
            .as_mut()
            .and_then(|d| d.scene.as_mut())
            .and_then(|s| s.after.take())
        {
            self.finish_vector_then(after, window, cx);
        }
    }

    pub(in crate::ui) fn vector_before_request(
        &mut self,
        what: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.finish_vector_then(AfterScene::Request(what), window, cx)
    }
    pub(in crate::ui) fn vector_before_import(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.finish_vector_then(AfterScene::Import(paths), window, cx)
    }

    pub(in crate::ui) fn vector_canvas_context(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self
            .vector_draft
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .mode;
        let mut modes = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_1()
            .min_w_0()
            .id("vector-modes")
            .overflow_x_scroll();
        for (id, label, value) in [
            ("scene-select", "Move · V", SceneMode::Select),
            ("scene-nodes", "Nodes · A", SceneMode::Nodes),
            ("scene-pen", "Pen · P", SceneMode::Pen),
        ] {
            modes = modes.child(
                button(id, label, ButtonVariant::Secondary, cx)
                    .selected(mode == value && self.tool != Tool::Hand)
                    .disabled(self.busy)
                    .debug_selector(move || id.into())
                    .on_click(cx.listener(move |this, _, w, cx| this.set_scene_mode(value, w, cx))),
            );
        }
        div()
            .id("vector-canvas-context")
            .debug_selector(|| "vector-canvas-context".into())
            .h(px(44.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.omarchy().divider())
            .bg(cx.omarchy().surface)
            .child(modes)
            .child(
                button("scene-cancel", "Cancel", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "scene-cancel".into())
                    .on_click(cx.listener(|this, _, w, cx| this.cancel_vector_canvas(w, cx))),
            )
            .child(
                button("scene-done", "Done", ButtonVariant::Primary, cx)
                    .debug_selector(|| "scene-done".into())
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.apply_vector(cx))),
            )
            .into_any_element()
    }

    pub(in crate::ui) fn vector_canvas_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let mut objects = div()
            .id("scene-objects")
            .max_h(px(96.))
            .flex_shrink_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (index, object) in scene.artwork.objects.iter().enumerate().rev() {
            let id = format!("scene-object-{index}");
            objects = objects.child(
                button(
                    SharedString::from(id.clone()),
                    SharedString::from(format!(
                        "{}{}",
                        object.name,
                        if object.visible { "" } else { " · hidden" }
                    )),
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(move || id.clone())
                .selected(index == scene.active)
                .disabled(self.busy)
                .w_full()
                .h(px(28.))
                .min_w_0()
                .overflow_hidden()
                .on_click(cx.listener(move |this, _, w, cx| {
                    if let Err(error) = this.update_vector_style(cx) {
                        this.status = error.to_string();
                        cx.notify();
                        return;
                    }
                    this.select_scene_object(index, w, cx);
                    this.focus.focus(w, cx);
                })),
            );
        }
        let mut add = div().flex().gap_1().flex_shrink_0();
        for (action, label) in [
            ("path", "New path"),
            ("rectangle", "Rectangle"),
            ("ellipse", "Ellipse"),
        ] {
            let id = format!("scene-{action}");
            add = add.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(move || id.clone())
                .disabled(
                    self.busy
                        || scene.artwork.objects.len() >= omuse::vector_scene::MAX_SCENE_OBJECTS,
                )
                .flex_1()
                .min_w_0()
                .px_1()
                .on_click(cx.listener(move |this, _, w, cx| this.scene_action(action, w, cx))),
            );
        }
        panel_section("Vector artwork", cx)
            .debug_selector(||"vector-canvas-inspector".into())
            .child(add)
            .child(objects)
            .child(self.vector_settings(window, cx))
            .child(panel_note("Space or middle-drag pans. Tool and layer switches keep edits; Escape discards them.", cx))
            .into_any_element()
    }
}

fn trim_history(scene: &mut super::scene::SceneDraft) {
    while scene.undo.len() + scene.redo.len() > 64
        || scene
            .undo
            .iter()
            .chain(&scene.redo)
            .map(|e| e.artwork.retained_bytes())
            .sum::<usize>()
            > 32 * 1024 * 1024
    {
        if !scene.undo.is_empty() {
            scene.undo.remove(0);
        } else {
            scene.redo.remove(0);
        }
    }
}
