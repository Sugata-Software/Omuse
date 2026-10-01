//! Disposable graphical Bézier-path editor. The document changes only on Apply.
use super::inspector_ui::{colour_swatch, panel_button as button, panel_input as input};
use super::*;
use gpui_kit::{FillOptions, PathBuilder, PathStyle};
use omuse::{
    advanced::LayerState,
    vector_path::{Anchor, Point as VectorPoint, StrokeStyle, Subpath, VectorPath},
};
use std::sync::atomic::{AtomicBool, Ordering};
#[path = "vector_io_ui.rs"]
mod exchange;
#[path = "vector_scene_ui.rs"]
mod scene;
#[cfg(all(test, feature = "ui-test"))]
#[path = "vector_tests.rs"]
mod tests;
#[path = "vector_canvas_ui.rs"]
mod vector_canvas;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragPart {
    Anchor,
    Incoming,
    Outgoing,
    NewNode,
}

#[derive(Clone, Debug)]
struct VectorDrag {
    subpath: usize,
    anchor: usize,
    part: DragPart,
    start: VectorPoint,
    original: Anchor,
    screen_start: Point<Pixels>,
}

pub(super) struct VectorDraft {
    layer: String,
    revision: u64,
    identity: (u64, u64),
    cancel: Arc<AtomicBool>,
    window: gpui_kit::AnyWindowHandle,
    state: Option<LayerState>,
    dimensions: (u32, u32),
    scene: Option<scene::SceneDraft>,
    path: VectorPath,
    as_mask: bool,
    fill: [u8; 4],
    stroke: Option<StrokeStyle>,
    source_preview: Option<Arc<RenderImage>>,
    preview_bounds: Rc<Cell<Bounds<Pixels>>>,
    selected: Option<(usize, usize)>,
    drag: Option<VectorDrag>,
}

impl VectorDraft {
    fn begin_drag(
        &mut self,
        subpath: usize,
        anchor: usize,
        part: DragPart,
        screen_start: Point<Pixels>,
    ) {
        let Some(original) = self
            .path
            .subpaths
            .get(subpath)
            .and_then(|path| path.anchors.get(anchor))
            .cloned()
        else {
            return;
        };
        self.drag = Some(VectorDrag {
            subpath,
            anchor,
            part,
            start: original.position,
            original,
            screen_start,
        });
    }
}

fn snap_vector(delta: VectorPoint, constrain: bool) -> VectorPoint {
    if !constrain || (delta.x == 0. && delta.y == 0.) {
        return delta;
    }
    let length = delta.x.hypot(delta.y);
    let angle = (delta.y.atan2(delta.x) / std::f32::consts::FRAC_PI_4).round()
        * std::f32::consts::FRAC_PI_4;
    VectorPoint {
        x: angle.cos() * length,
        y: angle.sin() * length,
    }
}

fn vector_point_in_bounds(point: VectorPoint) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= 1_000_000.
        && point.y.abs() <= 1_000_000.
}

fn parse_vector_colour(text: &str) -> anyhow::Result<[u8; 4]> {
    let value = text.trim().trim_start_matches('#');
    anyhow::ensure!(
        value.len() == 6 || value.len() == 8,
        "Use #RRGGBB or #RRGGBBAA colours"
    );
    anyhow::ensure!(value.is_ascii(), "Use hexadecimal colour digits");
    let mut color = [255; 4];
    for (index, channel) in color.iter_mut().take(value.len() / 2).enumerate() {
        *channel = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(color)
}

fn smooth_handles(anchor: &Anchor) -> bool {
    let (Some(incoming), Some(outgoing)) = (anchor.incoming, anchor.outgoing) else {
        return false;
    };
    let a = VectorPoint {
        x: incoming.x - anchor.position.x,
        y: incoming.y - anchor.position.y,
    };
    let b = VectorPoint {
        x: outgoing.x - anchor.position.x,
        y: outgoing.y - anchor.position.y,
    };
    let scale = a.x.hypot(a.y) * b.x.hypot(b.y);
    scale > f32::EPSILON
        && a.x * b.x + a.y * b.y < 0.
        && (a.x * b.y - a.y * b.x).abs() <= scale * 0.001
}

fn apply_vector_drag(
    anchor: &mut Anchor,
    drag: &VectorDrag,
    point: VectorPoint,
    constrain: bool,
    independent: bool,
) -> bool {
    match drag.part {
        DragPart::Anchor => {
            let delta = snap_vector(
                VectorPoint {
                    x: point.x - drag.start.x,
                    y: point.y - drag.start.y,
                },
                constrain,
            );
            let moved = |p: VectorPoint| VectorPoint {
                x: p.x + delta.x,
                y: p.y + delta.y,
            };
            let position = moved(drag.original.position);
            let incoming = drag.original.incoming.map(moved);
            let outgoing = drag.original.outgoing.map(moved);
            if !vector_point_in_bounds(position)
                || incoming.is_some_and(|p| !vector_point_in_bounds(p))
                || outgoing.is_some_and(|p| !vector_point_in_bounds(p))
            {
                return false;
            }
            anchor.position = position;
            anchor.incoming = incoming;
            anchor.outgoing = outgoing;
        }
        DragPart::NewNode => {
            let delta = snap_vector(
                VectorPoint {
                    x: point.x - anchor.position.x,
                    y: point.y - anchor.position.y,
                },
                constrain,
            );
            let incoming = VectorPoint {
                x: anchor.position.x - delta.x,
                y: anchor.position.y - delta.y,
            };
            let outgoing = VectorPoint {
                x: anchor.position.x + delta.x,
                y: anchor.position.y + delta.y,
            };
            if !vector_point_in_bounds(incoming) || !vector_point_in_bounds(outgoing) {
                return false;
            }
            anchor.incoming = Some(incoming);
            anchor.outgoing = Some(outgoing);
        }
        part @ (DragPart::Incoming | DragPart::Outgoing) => {
            let delta = snap_vector(
                VectorPoint {
                    x: point.x - anchor.position.x,
                    y: point.y - anchor.position.y,
                },
                constrain,
            );
            let moved = VectorPoint {
                x: anchor.position.x + delta.x,
                y: anchor.position.y + delta.y,
            };
            if !vector_point_in_bounds(moved) {
                return false;
            }
            let coupled = !independent && smooth_handles(&drag.original);
            let anchor_position = anchor.position;
            let (active, opposite, original_opposite) = match part {
                DragPart::Incoming => (
                    &mut anchor.incoming,
                    &mut anchor.outgoing,
                    drag.original.outgoing,
                ),
                DragPart::Outgoing => (
                    &mut anchor.outgoing,
                    &mut anchor.incoming,
                    drag.original.incoming,
                ),
                _ => unreachable!(),
            };
            if coupled {
                let length = original_opposite
                    .map(|p| (p.x - drag.original.position.x).hypot(p.y - drag.original.position.y))
                    .unwrap_or(0.);
                let moved_length = delta.x.hypot(delta.y);
                if moved_length > f32::EPSILON && length > 0. {
                    let other = VectorPoint {
                        x: anchor_position.x - delta.x / moved_length * length,
                        y: anchor_position.y - delta.y / moved_length * length,
                    };
                    if !vector_point_in_bounds(other) {
                        return false;
                    }
                    *opposite = Some(other);
                }
            }
            *active = Some(moved);
        }
    }
    true
}

impl EditorView {
    pub(super) fn open_vector(
        &mut self,
        as_mask: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !as_mask
            && self
                .editor
                .document
                .find_layer(&self.editor.active_layer)
                .is_some_and(|layer| layer.vector_scene.is_some())
        {
            self.open_vector_scene(window, cx);
            return;
        }
        self.clear_vector(cx);
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        let layer = self.editor.active_layer.clone();
        let mut state = match self.editor.editable_state(&layer) {
            Ok(state) => state,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let draft_as_mask = as_mask || state.recipe.vector_is_mask;
        // Only masks need the existing image for context. Drawing it beneath
        // artwork paths would double-paint retained vectors or show a photo
        // that Apply replaces. Avoid allocating/uploading that unused proxy.
        let source_preview = if draft_as_mask {
            match state.proxy() {
                Ok(pixels) => Some(render_image(&pixels)),
                Err(error) => {
                    self.status = error.to_string();
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let path = state.recipe.vector.take().unwrap_or_else(|| VectorPath {
            subpaths: vec![Subpath {
                anchors: Vec::new(),
                closed: false,
            }],
            fill_rule: Default::default(),
        });
        let color_hex = |color: [u8; 4]| {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                color[0], color[1], color[2], color[3]
            )
        };
        let stroke = state.recipe.vector_stroke;
        for (input, value) in self.detail_inputs.iter().zip([
            color_hex(state.recipe.vector_fill),
            color_hex(stroke.map_or(self.editor.brush.color, |s| s.color)),
            stroke.map_or(0., |s| s.width).to_string(),
        ]) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.vector_draft = Some(VectorDraft {
            layer,
            revision: self.editor.revision(),
            identity: (self.editor.instance_id(), self.create.epoch),
            cancel: Arc::new(AtomicBool::new(false)),
            window: window.window_handle(),
            path,
            as_mask: draft_as_mask,
            fill: state.recipe.vector_fill,
            stroke: state.recipe.vector_stroke,
            source_preview,
            preview_bounds: Rc::new(Cell::new(Bounds::default())),
            selected: None,
            drag: None,
            dimensions: state.source.dimensions(),
            state: Some(state),
            scene: None,
        });
        self.dialog = Dialog::VectorPath;
        self.modal_focus.focus(window, cx);
        self.status = if draft_as_mask {
            "Edit vector mask; Apply commits one undo step"
        } else {
            "Edit vector path; Apply commits one undo step"
        }
        .into();
        cx.notify();
    }

    pub(super) fn clear_vector(&mut self, cx: &mut App) {
        if let Some(draft) = self.vector_draft.take() {
            draft.cancel.store(true, Ordering::Relaxed);
            self.busy = false;
            if let Some(image) = draft.source_preview {
                cx.drop_image(image, None);
            }
            if let Some(scene) = draft.scene {
                if let Some(cancel) = scene.preview_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                if let Some(display) = scene.display {
                    for tile in display.snapshot().iter() {
                        cx.drop_image(tile.image.clone(), None);
                    }
                }
            }
        }
    }

    pub(super) fn prepare_vector_inspection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let artwork = omuse::vector_svg::decode(
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="192">
          <path d="M40 36 C100 0 240 0 280 36 L280 156 C220 192 100 192 40 156 Z
                   M110 64 L210 64 L210 128 L110 128 Z"
            fill="#D58049" fill-rule="evenodd" stroke="#473A36" stroke-width="4"
            stroke-linecap="round" stroke-linejoin="round"/>
        </svg>"##,
        )?;
        self.import_vector_artwork(artwork, window, cx)?;
        self.vector_draft.as_mut().unwrap().selected = Some((0, 0));
        Ok(())
    }

    pub(super) fn update_vector_style(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let Some(draft) = self.vector_draft.as_ref() else {
            return Ok(());
        };
        if draft.as_mask {
            return Ok(());
        }
        let fill = parse_vector_colour(self.detail_inputs[0].read(cx).value().as_ref())?;
        let color = parse_vector_colour(self.detail_inputs[1].read(cx).value().as_ref())?;
        let width: f32 = self.detail_inputs[2].read(cx).value().parse()?;
        anyhow::ensure!(
            width.is_finite() && (0. ..=4096.).contains(&width),
            "Stroke width must be 0–4096 px; 0 disables the stroke"
        );
        let stroke = (width > 0.).then_some(StrokeStyle { color, width });
        let opacity = if let Some(scene) = &draft.scene {
            let opacity: f32 = self.detail_inputs[3]
                .read(cx)
                .value()
                .parse()
                .map_err(|_| anyhow::anyhow!("Enter object opacity from 0 to 100%"))?;
            anyhow::ensure!(
                opacity.is_finite() && (0. ..=100.).contains(&opacity),
                "Object opacity must be 0–100%"
            );
            Some((opacity / 100., scene.artwork.objects[scene.active].opacity))
        } else {
            None
        };
        if draft.fill == fill
            && draft.stroke == stroke
            && opacity.is_none_or(|(new, old)| new == old)
        {
            return Ok(());
        }
        self.scene_checkpoint();
        let draft = self.vector_draft.as_mut().unwrap();
        draft.fill = fill;
        draft.stroke = stroke;
        if let Some((opacity, _)) = opacity {
            let scene = draft.scene.as_mut().unwrap();
            scene.artwork.objects[scene.active].opacity = opacity;
        }
        self.vector_scene_changed(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn apply_vector(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        if self
            .vector_draft
            .as_ref()
            .is_some_and(|draft| draft.scene.is_some())
        {
            self.apply_vector_scene(cx);
            return;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        if draft.revision != self.editor.revision() {
            self.status =
                "Document changed. Close and reopen the path editor before applying.".into();
            cx.notify();
            return;
        }
        let layer = draft.layer.clone();
        let revision = draft.revision;
        let generation = self.dialog_generation;
        let window = draft.window;
        let cancel = draft.cancel.clone();
        let Some(mut state) = draft.state.clone() else {
            return;
        };
        state.recipe.vector = Some(draft.path.clone());
        state.recipe.vector_is_mask = draft.as_mask;
        state.recipe.vector_fill = draft.fill;
        state.recipe.vector_stroke = draft.stroke;
        self.busy = true;
        self.status = "Rendering vector path…".into();
        let task = cx
            .background_executor()
            .spawn(async move { state.evaluate(&cancel) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::VectorPath
                    || this.dialog_generation != generation
                    || !this
                        .vector_draft
                        .as_ref()
                        .is_some_and(|draft| draft.layer == layer && draft.revision == revision)
                {
                    return;
                }
                this.busy = false;
                if this.editor.revision() != revision {
                    this.status =
                        "Document changed. Close and reopen the path editor before applying."
                            .into();
                    cx.notify();
                    return;
                }
                match result.and_then(|state| this.editor.replace_vector_state(&layer, state)) {
                    Ok(_) => {
                        this.clear_vector(cx);
                        this.dialog = Dialog::None;
                        this.dialog_generation = this.dialog_generation.wrapping_add(1);
                        this.changed(cx);
                        this.status = "Vector path applied".into();
                        let focus = this.focus.clone();
                        cx.defer(move |cx| {
                            let _ =
                                cx.update_window(window, |_, window, cx| focus.focus(window, cx));
                        });
                    }
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn vector_point(&self, position: Point<Pixels>) -> Option<VectorPoint> {
        let draft = self.vector_draft.as_ref()?;
        if draft.is_scene() {
            return self.vector_canvas_point(position);
        }
        let bounds = draft.preview_bounds.get();
        if !bounds.contains(&position) {
            return None;
        }
        let (width, height) = draft.dimensions;
        Some(VectorPoint {
            x: f32::from(position.x - bounds.origin.x) * width as f32
                / f32::from(bounds.size.width),
            y: f32::from(position.y - bounds.origin.y) * height as f32
                / f32::from(bounds.size.height),
        })
    }

    fn vector_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(point) = self.vector_point(event.position) else {
            return;
        };
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let (width, _) = draft.dimensions;
        let tolerance = 9. * width as f32 / f32::from(draft.preview_bounds.get().size.width);
        let mut hit = None;
        for (si, subpath) in draft.path.subpaths.iter().enumerate() {
            for (ai, anchor) in subpath.anchors.iter().enumerate() {
                for (part, candidate) in [
                    (DragPart::Anchor, Some(anchor.position)),
                    (DragPart::Incoming, anchor.incoming),
                    (DragPart::Outgoing, anchor.outgoing),
                ] {
                    if candidate.is_some_and(|p| (p.x - point.x).hypot(p.y - point.y) <= tolerance)
                    {
                        hit = Some((si, ai, part));
                    }
                }
            }
        }
        if let Some((si, ai, part)) = hit {
            draft.selected = Some((si, ai));
            draft.begin_drag(si, ai, part, event.position);
        } else {
            if draft
                .path
                .subpaths
                .iter()
                .map(|path| path.anchors.len())
                .sum::<usize>()
                >= 100_000
            {
                self.status = "This path has reached the 100,000 node limit.".into();
                cx.notify();
                return;
            }
            if draft.path.subpaths.is_empty() {
                draft.path.subpaths.push(Subpath {
                    anchors: Vec::new(),
                    closed: false,
                });
            }
            let si = draft.path.subpaths.len() - 1;
            draft.path.subpaths[si].anchors.push(Anchor {
                position: point,
                incoming: None,
                outgoing: None,
            });
            let ai = draft.path.subpaths[si].anchors.len() - 1;
            draft.selected = Some((si, ai));
            draft.begin_drag(si, ai, DragPart::NewNode, event.position);
        }
        cx.stop_propagation();
        self.vector_scene_changed(cx);
        cx.notify();
    }

    pub(in crate::ui) fn vector_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !event.dragging() {
            return;
        }
        let Some(point) = self.vector_point(event.position) else {
            return;
        };
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        if let Some(origin) = draft.scene.as_ref().and_then(|s| s.drag_origin) {
            let (dx, dy) = (point.x - origin.x, point.y - origin.y);
            if draft.path.bounds().is_some_and(|(a, b)| {
                [a.x + dx, a.y + dy, b.x + dx, b.y + dy]
                    .iter()
                    .any(|v| v.abs() > 1_000_000.)
            }) {
                return;
            }
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
                        p.x += dx;
                        p.y += dy;
                    }
                }
            }
            draft.scene.as_mut().unwrap().drag_origin = Some(point);
            self.vector_scene_changed(cx);
            cx.notify();
            return;
        }
        let Some(drag) = draft.drag.clone() else {
            return;
        };
        if drag.part == DragPart::NewNode
            && f32::from(event.position.x - drag.screen_start.x)
                .hypot(f32::from(event.position.y - drag.screen_start.y))
                < 3.
        {
            return;
        }
        let Some(anchor) = draft
            .path
            .subpaths
            .get_mut(drag.subpath)
            .and_then(|s| s.anchors.get_mut(drag.anchor))
        else {
            return;
        };
        if !apply_vector_drag(
            anchor,
            &drag,
            point,
            event.modifiers.shift,
            event.modifiers.alt,
        ) {
            return;
        }
        self.vector_scene_changed(cx);
        cx.notify();
    }

    pub(in crate::ui) fn vector_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.vector_draft.as_mut() {
            draft.drag = None;
            if let Some(scene) = draft.scene.as_mut() {
                scene.drag_origin = None;
            }
        }
        cx.notify();
    }

    fn vector_delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        if let Some((si, ai)) = draft.selected.take() {
            if let Some(subpath) = draft.path.subpaths.get_mut(si) {
                if ai < subpath.anchors.len() {
                    subpath.anchors.remove(ai);
                    if subpath.closed && subpath.anchors.len() < 2 {
                        subpath.closed = false;
                    }
                }
            }
        }
        self.vector_scene_changed(cx);
        cx.notify();
    }

    fn vector_smooth_selected(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let Some((si, ai)) = draft.selected else {
            return;
        };
        let Some(subpath) = draft.path.subpaths.get_mut(si) else {
            return;
        };
        let n = subpath.anchors.len();
        if n < 2 {
            return;
        }
        let position = subpath.anchors[ai].position;
        let previous = if ai > 0 || subpath.closed {
            subpath.anchors[(ai + n - 1) % n].position
        } else {
            let next = subpath.anchors[1].position;
            VectorPoint {
                x: position.x * 2. - next.x,
                y: position.y * 2. - next.y,
            }
        };
        let next = if ai + 1 < n || subpath.closed {
            subpath.anchors[(ai + 1) % n].position
        } else {
            VectorPoint {
                x: position.x * 2. - previous.x,
                y: position.y * 2. - previous.y,
            }
        };
        let anchor = &mut subpath.anchors[ai];
        let distance = |a: VectorPoint, b: VectorPoint| (a.x - b.x).hypot(a.y - b.y);
        let length = distance(anchor.position, previous).min(distance(anchor.position, next)) / 3.;
        let dx = next.x - previous.x;
        let dy = next.y - previous.y;
        let norm = dx.hypot(dy).max(f32::EPSILON);
        let offset = VectorPoint {
            x: dx / norm * length,
            y: dy / norm * length,
        };
        anchor.incoming = Some(VectorPoint {
            x: anchor.position.x - offset.x,
            y: anchor.position.y - offset.y,
        });
        anchor.outgoing = Some(VectorPoint {
            x: anchor.position.x + offset.x,
            y: anchor.position.y + offset.y,
        });
        self.vector_scene_changed(cx);
        cx.notify();
    }

    fn vector_corner_selected(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        if let Some(draft) = self.vector_draft.as_mut()
            && let Some((si, ai)) = draft.selected
            && let Some(anchor) = draft
                .path
                .subpaths
                .get_mut(si)
                .and_then(|s| s.anchors.get_mut(ai))
        {
            anchor.incoming = None;
            anchor.outgoing = None;
        }
        self.vector_scene_changed(cx);
        cx.notify();
    }

    fn vector_insert_midpoint(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let Some((si, ai)) = draft.selected else {
            return;
        };
        let Some(subpath) = draft.path.subpaths.get(si) else {
            return;
        };
        if subpath.anchors.len() < 2 {
            return;
        }
        let segment = if !subpath.closed && ai + 1 == subpath.anchors.len() {
            ai - 1
        } else {
            ai
        };
        match draft.path.insert_on_segment(si, segment, 0.5) {
            Ok(at) => draft.selected = Some((si, at)),
            Err(error) => self.status = error.to_string(),
        }
        self.vector_scene_changed(cx);
        cx.notify();
    }

    fn vector_reverse_selected(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let Some(si) = draft
            .selected
            .map(|(si, _)| si)
            .or_else(|| draft.path.subpaths.len().checked_sub(1))
        else {
            return;
        };
        let subpath = &mut draft.path.subpaths[si];
        subpath.anchors.reverse();
        for anchor in &mut subpath.anchors {
            std::mem::swap(&mut anchor.incoming, &mut anchor.outgoing);
        }
        if let Some((selected_subpath, ai)) = draft.selected
            && selected_subpath == si
        {
            draft.selected = Some((si, subpath.anchors.len() - 1 - ai));
        }
        self.vector_scene_changed(cx);
        cx.notify();
    }

    fn vector_new_subpath(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scene_checkpoint();
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        if draft
            .path
            .subpaths
            .last()
            .is_none_or(|s| !s.anchors.is_empty())
        {
            if draft.path.subpaths.len() >= 4096 {
                self.status = "This path has reached the subpath limit.".into();
                cx.notify();
                return;
            }
            draft.path.subpaths.push(Subpath {
                anchors: Vec::new(),
                closed: false,
            });
        }
        draft.selected = None;
        draft.drag = None;
        self.vector_scene_changed(cx);
        self.status =
            "Click to start another subpath. Even-odd fill makes overlapping regions into holes."
                .into();
        cx.notify();
    }

    pub(super) fn render_vector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(draft) = self.vector_draft.as_ref() else {
            return div().into_any_element();
        };
        let image = draft.source_preview.clone();
        let is_scene = draft.scene.is_some();
        let bounds_cell = draft.preview_bounds.clone();
        let dimensions = draft.dimensions;
        let path = draft.path.clone();
        let selected = draft.selected;
        let as_mask = draft.as_mask;
        let fill_color = draft.fill;
        let stroke = draft.stroke;
        let compact = f32::from(window.viewport_size().height) < 720.;
        let preview = div()
            .id("vector-path-preview")
            .debug_selector(|| "vector-path-preview".into())
            // Keep the preview and edit/apply controls reachable above the
            // modal's fixed footer at the minimum supported viewport.
            .h(px(if compact { 160. } else { 240. }))
            .flex_shrink_0()
            .w_full()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::vector_down))
            .on_mouse_move(cx.listener(Self::vector_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::vector_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::vector_up))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(bounds, rgb(0x111318)));
                        let scale = (f32::from(bounds.size.width) / dimensions.0 as f32)
                            .min(f32::from(bounds.size.height) / dimensions.1 as f32);
                        let target = Bounds::new(
                            point(
                                bounds.origin.x
                                    + (bounds.size.width - px(dimensions.0 as f32 * scale)) / 2.,
                                bounds.origin.y
                                    + (bounds.size.height - px(dimensions.1 as f32 * scale)) / 2.,
                            ),
                            size(
                                px(dimensions.0 as f32 * scale),
                                px(dimensions.1 as f32 * scale),
                            ),
                        );
                        bounds_cell.set(target);
                        if !as_mask {
                            paint_transparency(window, target);
                        }
                        if let Some(image) = &image {
                            let _ = window.paint_image(
                                bounds,
                                target,
                                Corners::default(),
                                image.clone(),
                                0,
                                false,
                            );
                        } else if as_mask {
                            // Match the canvas's neutral transparency matte.
                            window.paint_quad(fill(target, rgb(0xd6d6d6)));
                            let tile = px(12.);
                            let cols = (f32::from(target.size.width) / 12.).ceil() as i32;
                            let rows = (f32::from(target.size.height) / 12.).ceil() as i32;
                            for y in 0..rows {
                                for x in 0..cols {
                                    if (x + y) % 2 == 0 {
                                        let cell = Bounds::new(
                                            point(
                                                target.origin.x + tile * x as f32,
                                                target.origin.y + tile * y as f32,
                                            ),
                                            size(tile, tile),
                                        )
                                        .intersect(&target);
                                        window.paint_quad(fill(cell, rgb(0xf4f4f4)));
                                    }
                                }
                            }
                        }
                        paint_vector_overlay(
                            window,
                            target,
                            dimensions,
                            &path,
                            selected,
                            (!as_mask && !is_scene).then_some(fill_color),
                            if as_mask || is_scene { None } else { stroke },
                            true,
                        );
                    },
                )
                .size_full(),
            );
        let settings = self.vector_settings(window, cx);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(if as_mask {
                "Edit this layer's vector mask."
            } else {
                "Edit this layer's retained path."
            })
            .child(preview)
            .child(settings)
            .into_any_element()
    }

    pub(in crate::ui) fn vector_settings(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(draft) = self.vector_draft.as_ref() else {
            return div().into_any_element();
        };
        let selected = draft.selected;
        let as_mask = draft.as_mask;
        let is_scene = draft.is_scene();
        let active_subpath = selected
            .map(|(si, _)| si)
            .or_else(|| draft.path.subpaths.len().checked_sub(1));
        let closed = active_subpath
            .and_then(|si| draft.path.subpaths.get(si))
            .is_some_and(|s| s.closed);
        let has_segment = active_subpath
            .and_then(|si| draft.path.subpaths.get(si))
            .is_some_and(|s| s.anchors.len() >= 2);
        let has_selected = selected.is_some();
        let anchor_count = draft
            .path
            .subpaths
            .iter()
            .map(|subpath| subpath.anchors.len())
            .sum::<usize>();
        let anchor_status = selected.map_or_else(
            || format!("{anchor_count} anchors · no node selected"),
            |(subpath, anchor)| {
                format!(
                    "{anchor_count} anchors · subpath {}, anchor {} selected",
                    subpath + 1,
                    anchor + 1
                )
            },
        );
        let even_odd = draft.path.fill_rule == omuse::vector_path::FillRule::EvenOdd;
        let controls = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                button("vector-delete", "Delete node", ButtonVariant::Outline, cx)
                    .disabled(self.busy || !has_selected)
                    .debug_selector(|| "vector-delete".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_delete_selected(cx))),
            )
            .child(
                button("vector-smooth", "Smooth node", ButtonVariant::Outline, cx)
                    .disabled(self.busy || !has_selected)
                    .debug_selector(|| "vector-smooth".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_smooth_selected(cx))),
            )
            .child(
                button("vector-corner", "Clear handles", ButtonVariant::Outline, cx)
                    .disabled(self.busy || !has_selected)
                    .debug_selector(|| "vector-corner".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_corner_selected(cx))),
            )
            .child(
                button(
                    "vector-insert",
                    "Insert midpoint",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy || !has_selected)
                .debug_selector(|| "vector-insert".into())
                .on_click(cx.listener(|this, _, _, cx| this.vector_insert_midpoint(cx))),
            )
            .child(
                button(
                    "vector-close",
                    if closed { "Open path" } else { "Close path" },
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy || !has_segment)
                .debug_selector(|| "vector-close".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.busy {
                        this.scene_checkpoint();
                    }
                    if !this.busy
                        && let Some(draft) = this.vector_draft.as_mut()
                    {
                        let si = draft
                            .selected
                            .map(|(si, _)| si)
                            .or_else(|| draft.path.subpaths.len().checked_sub(1));
                        if let Some(subpath) = si.and_then(|si| draft.path.subpaths.get_mut(si)) {
                            if subpath.anchors.len() >= 2 {
                                subpath.closed = !subpath.closed;
                            }
                        }
                    }
                    this.vector_scene_changed(cx);
                    cx.notify();
                })),
            )
            .child(
                button("vector-reverse", "Reverse path", ButtonVariant::Outline, cx)
                    .disabled(self.busy || !has_segment)
                    .debug_selector(|| "vector-reverse".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_reverse_selected(cx))),
            )
            .child(
                button(
                    "vector-new-subpath",
                    "New subpath",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy)
                .debug_selector(|| "vector-new-subpath".into())
                .on_click(cx.listener(|this, _, _, cx| this.vector_new_subpath(cx))),
            )
            .child(
                button(
                    "vector-fill-rule",
                    if even_odd {
                        "Fill: even-odd"
                    } else {
                        "Fill: non-zero"
                    },
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy)
                .debug_selector(|| "vector-fill-rule".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.busy {
                        this.scene_checkpoint();
                    }
                    if !this.busy
                        && let Some(draft) = this.vector_draft.as_mut()
                    {
                        draft.path.fill_rule =
                            if draft.path.fill_rule == omuse::vector_path::FillRule::EvenOdd {
                                omuse::vector_path::FillRule::NonZero
                            } else {
                                omuse::vector_path::FillRule::EvenOdd
                            };
                    }
                    this.vector_scene_changed(cx);
                    cx.notify();
                })),
            );
        let mut style = div().flex().flex_wrap().gap_2();
        if !as_mask {
            let no_stroke = self.detail_inputs[2].read(cx).value().parse::<f32>() == Ok(0.);
            let mut invalid_colour = false;
            let mut fields = vec!["Fill", "Stroke", "Width · px"];
            if is_scene {
                fields.push("Opacity · %");
            }
            for (index, label) in fields.iter().enumerate() {
                let field_id = format!("vector-style-{index}");
                let color = (index < 2)
                    .then(|| parse_vector_colour(&self.detail_inputs[index].read(cx).value()).ok())
                    .flatten();
                invalid_colour |= index < 2 && color.is_none();
                let inactive = index == 1 && no_stroke;
                let field_label = if inactive {
                    "Stroke · none"
                } else if index == 0 && color.is_some_and(|c| c[3] == 0) {
                    "Fill · none"
                } else {
                    *label
                };
                let field = if self.busy {
                    // Keep an in-flight import/export or Apply snapshot stable.
                    div()
                        .w_full()
                        .h(px(32.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(cx.omarchy().secondary)
                        .when(index < 2, |view| {
                            view.child(colour_swatch(color, inactive, cx))
                        })
                        .child(self.detail_inputs[index].read(cx).value())
                        .into_any_element()
                } else {
                    input(
                        SharedString::from(field_id.clone()),
                        &self.detail_inputs[index],
                        window,
                        cx,
                    )
                    .px_2()
                    .gap_1()
                    .when(index < 2, |field| {
                        field.prefix(
                            colour_swatch(color, inactive, cx)
                                .id(SharedString::from(format!("vector-swatch-{index}")))
                                .debug_selector(move || format!("vector-swatch-{index}")),
                        )
                    })
                    .when(index < 2 && color.is_none(), |field| {
                        field.border_color(cx.omarchy().danger)
                    })
                    .debug_selector(move || field_id.clone())
                    .into_any_element()
                };
                style = style.child(
                    div()
                        .flex_1()
                        .min_w(px(128.))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(inspector_ui::panel_note(field_label, cx))
                        .child(field),
                );
            }
            style = style.child(
                div().w_full().child(inspector_ui::panel_note(
                    if invalid_colour {
                        "Use #RRGGBB or #RRGGBBAA. The last valid style stays on the canvas."
                    } else if no_stroke {
                        "Set Width above 0 to show the stroke. The checkerboard shows transparency."
                    } else {
                        "Hex includes optional alpha · Checkerboard shows transparency"
                    }, cx,
                ).when(invalid_colour, |note| note.text_color(cx.omarchy().danger))),
            );
            style = style.child(
                button(
                    "vector-style-preview",
                    "Update style",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy)
                .debug_selector(|| "vector-style-preview".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(error) = this.update_vector_style(cx) {
                        this.status = error.to_string();
                        cx.notify();
                    }
                })),
            );
        }
        div().flex().flex_col().gap_2()
            .child(
                div()
                    .id("vector-anchor-status")
                    .debug_selector(|| "vector-anchor-status".into())
                    .text_size(px(11.))
                    .text_color(cx.omarchy().secondary)
                    .child(anchor_status),
            )
            .child(style)
            .when(is_scene, |view| view.child(self.vector_scene_controls(cx)))
            .child(controls)
            .child(div().flex().flex_wrap().gap_2()
                .child(button("vector-svg-import", "Import SVG path", ButtonVariant::Outline, cx)
                    .disabled(self.busy).debug_selector(|| "vector-svg-import".into())
                    .on_click(cx.listener(|this, _, window, cx| this.choose_vector_import(window, cx))))
                .child(button("vector-svg-export", "Export path SVG", ButtonVariant::Outline, cx)
                    .disabled(self.busy).debug_selector(|| "vector-svg-export".into())
                    .on_click(cx.listener(|this, _, window, cx| this.choose_vector_export(window, cx)))))
            .child(div().text_size(px(11.)).text_color(cx.omarchy().secondary)
                .child("SVG exchanges the selected path. Save .omuse to keep the complete artwork."))
            .into_any_element()
    }
}

fn paint_transparency(window: &mut Window, target: Bounds<Pixels>) {
    window.paint_quad(fill(target, rgb(0xd6d6d6)));
    let tile = px(12.);
    for y in 0..(f32::from(target.size.height) / 12.).ceil() as i32 {
        for x in 0..(f32::from(target.size.width) / 12.).ceil() as i32 {
            if (x + y) % 2 == 0 {
                let cell = Bounds::new(
                    point(
                        target.origin.x + tile * x as f32,
                        target.origin.y + tile * y as f32,
                    ),
                    size(tile, tile),
                )
                .intersect(&target);
                window.paint_quad(fill(cell, rgb(0xf4f4f4)));
            }
        }
    }
}

fn screen_point(
    point: VectorPoint,
    bounds: Bounds<Pixels>,
    dimensions: (u32, u32),
) -> Point<Pixels> {
    gpui_kit::point(
        bounds.origin.x + bounds.size.width * point.x / dimensions.0 as f32,
        bounds.origin.y + bounds.size.height * point.y / dimensions.1 as f32,
    )
}

fn paint_vector_overlay(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    dimensions: (u32, u32),
    path: &VectorPath,
    selected: Option<(usize, usize)>,
    fill_color: Option<[u8; 4]>,
    stroke: Option<StrokeStyle>,
    show_nodes: bool,
) {
    // Fill all subpaths together: separate fill calls lose holes and apply
    // alpha repeatedly where compound regions overlap.
    for (paint_fill, color, width) in [
        (true, fill_color, 0.),
        (
            false,
            stroke.map(|s| s.color),
            stroke.map_or(0., |s| s.width),
        ),
    ] {
        let Some(color) = color.filter(|c| c[3] > 0) else {
            continue;
        };
        let mut shape = if paint_fill {
            PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::default().with_fill_rule(
                match path.fill_rule {
                    omuse::vector_path::FillRule::EvenOdd => gpui_kit::FillRule::EvenOdd,
                    omuse::vector_path::FillRule::NonZero => gpui_kit::FillRule::NonZero,
                },
            )))
        } else {
            PathBuilder::stroke(bounds.size.width * width / dimensions.0 as f32)
        };
        for subpath in &path.subpaths {
            if subpath.anchors.is_empty() {
                continue;
            }
            shape.move_to(screen_point(
                subpath.anchors[0].position,
                bounds,
                dimensions,
            ));
            let count = if subpath.closed {
                subpath.anchors.len()
            } else {
                subpath.anchors.len().saturating_sub(1)
            };
            for index in 0..count {
                let a = &subpath.anchors[index];
                let b = &subpath.anchors[(index + 1) % subpath.anchors.len()];
                shape.cubic_bezier_to(
                    screen_point(b.position, bounds, dimensions),
                    screen_point(a.outgoing.unwrap_or(a.position), bounds, dimensions),
                    screen_point(b.incoming.unwrap_or(b.position), bounds, dimensions),
                );
            }
            // SVG and our rasterizer implicitly close open paths for filling.
            if subpath.closed || paint_fill {
                shape.close();
            }
        }
        if let Ok(shape) = shape.build() {
            window.paint_path(shape, rgba(u32::from_be_bytes(color)));
        }
    }
    for (si, subpath) in path.subpaths.iter().enumerate() {
        if subpath.anchors.is_empty() {
            continue;
        }
        let mut builder = PathBuilder::stroke(px(2.));
        builder.move_to(screen_point(
            subpath.anchors[0].position,
            bounds,
            dimensions,
        ));
        let segments = if subpath.closed {
            subpath.anchors.len()
        } else {
            subpath.anchors.len().saturating_sub(1)
        };
        for index in 0..segments {
            let next = (index + 1) % subpath.anchors.len();
            let a = &subpath.anchors[index];
            let b = &subpath.anchors[next];
            builder.cubic_bezier_to(
                screen_point(b.position, bounds, dimensions),
                screen_point(a.outgoing.unwrap_or(a.position), bounds, dimensions),
                screen_point(b.incoming.unwrap_or(b.position), bounds, dimensions),
            );
        }
        if subpath.closed {
            builder.close();
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(0x56b4ff));
        }
        if !show_nodes {
            continue;
        }
        for (ai, anchor) in subpath.anchors.iter().enumerate() {
            let anchor_point = screen_point(anchor.position, bounds, dimensions);
            for handle in [anchor.incoming, anchor.outgoing].into_iter().flatten() {
                let handle = screen_point(handle, bounds, dimensions);
                let mut line = PathBuilder::stroke(px(1.));
                line.move_to(anchor_point);
                line.line_to(handle);
                if let Ok(line) = line.build() {
                    window.paint_path(line, rgb(0xb8c0cc));
                }
                window.paint_quad(fill(
                    Bounds::new(handle - point(px(3.), px(3.)), size(px(6.), px(6.))),
                    rgb(0xf1c75b),
                ));
            }
            let radius = if selected == Some((si, ai)) { 5. } else { 4. };
            window.paint_quad(fill(
                Bounds::new(
                    anchor_point - point(px(radius), px(radius)),
                    size(px(radius * 2.), px(radius * 2.)),
                ),
                if selected == Some((si, ai)) {
                    rgb(0xffffff)
                } else {
                    rgb(0x56b4ff)
                },
            ));
        }
    }
}
