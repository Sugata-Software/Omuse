//! Disposable graphical Bézier-path editor. The document changes only on Apply.
use super::*;
use gpui_kit::PathBuilder;
use omuse::{
    advanced::LayerState,
    vector_path::{Anchor, Point as VectorPoint, StrokeStyle, Subpath, VectorPath},
};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(test, feature = "ui-test"))]
#[path = "vector_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragPart {
    Anchor,
    Incoming,
    Outgoing,
}

pub(super) struct VectorDraft {
    layer: String,
    revision: u64,
    cancel: Arc<AtomicBool>,
    window: gpui_kit::AnyWindowHandle,
    state: LayerState,
    path: VectorPath,
    as_mask: bool,
    fill: [u8; 4],
    stroke: Option<StrokeStyle>,
    source_preview: Arc<RenderImage>,
    preview_bounds: Rc<Cell<Bounds<Pixels>>>,
    selected: Option<(usize, usize)>,
    drag: Option<(usize, usize, DragPart)>,
}

impl EditorView {
    pub(super) fn open_vector(
        &mut self,
        as_mask: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_vector(cx);
        let layer = self.editor.active_layer.clone();
        let mut state = match self.editor.editable_state(&layer) {
            Ok(state) => state,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let pixels = match state.proxy() {
            Ok(pixels) => pixels,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let path = state.recipe.vector.take().unwrap_or_else(|| VectorPath {
            subpaths: vec![Subpath {
                anchors: Vec::new(),
                closed: false,
            }],
            fill_rule: Default::default(),
        });
        let draft_as_mask = as_mask || state.recipe.vector_is_mask;
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
            cancel: Arc::new(AtomicBool::new(false)),
            window: window.window_handle(),
            path,
            as_mask: draft_as_mask,
            fill: state.recipe.vector_fill,
            stroke: state.recipe.vector_stroke,
            source_preview: render_image(&pixels),
            preview_bounds: Rc::new(Cell::new(Bounds::default())),
            selected: None,
            drag: None,
            state,
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
            cx.drop_image(draft.source_preview, None);
        }
    }

    fn update_vector_style(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let Some(draft) = self.vector_draft.as_mut() else {
            return Ok(());
        };
        if draft.as_mask {
            return Ok(());
        }
        let parse_color = |text: &str| -> anyhow::Result<[u8; 4]> {
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
        };
        let fill = parse_color(self.detail_inputs[0].read(cx).value().as_ref())?;
        let color = parse_color(self.detail_inputs[1].read(cx).value().as_ref())?;
        let width: f32 = self.detail_inputs[2].read(cx).value().parse()?;
        anyhow::ensure!(
            width.is_finite() && (0. ..=4096.).contains(&width),
            "Stroke width must be 0–4096 px; 0 disables the stroke"
        );
        draft.fill = fill;
        draft.stroke = (width > 0.).then_some(StrokeStyle { color, width });
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
        let Some(draft) = self.vector_draft.as_ref() else {
            return;
        };
        let layer = draft.layer.clone();
        let revision = draft.revision;
        let generation = self.dialog_generation;
        let window = draft.window;
        let cancel = draft.cancel.clone();
        let mut state = draft.state.clone();
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
                    || this.editor.revision() != revision
                    || !this
                        .vector_draft
                        .as_ref()
                        .is_some_and(|draft| draft.layer == layer && draft.revision == revision)
                {
                    return;
                }
                this.busy = false;
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
        let bounds = draft.preview_bounds.get();
        if !bounds.contains(&position) {
            return None;
        }
        let (width, height) = draft.state.source.dimensions();
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
        let Some(point) = self.vector_point(event.position) else {
            return;
        };
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let (width, _) = draft.state.source.dimensions();
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
            draft.drag = Some((si, ai, part));
        } else {
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
            draft.drag = Some((si, ai, DragPart::Anchor));
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn vector_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(point) = self.vector_point(event.position) else {
            return;
        };
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let Some((si, ai, part)) = draft.drag else {
            return;
        };
        let Some(anchor) = draft
            .path
            .subpaths
            .get_mut(si)
            .and_then(|s| s.anchors.get_mut(ai))
        else {
            return;
        };
        match part {
            DragPart::Anchor => {
                let delta = VectorPoint {
                    x: point.x - anchor.position.x,
                    y: point.y - anchor.position.y,
                };
                anchor.position = point;
                for handle in [&mut anchor.incoming, &mut anchor.outgoing] {
                    if let Some(handle) = handle {
                        handle.x += delta.x;
                        handle.y += delta.y;
                    }
                }
            }
            DragPart::Incoming => anchor.incoming = Some(point),
            DragPart::Outgoing => anchor.outgoing = Some(point),
        }
        cx.notify();
    }

    fn vector_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.vector_draft.as_mut() {
            draft.drag = None;
        }
        cx.notify();
    }

    fn vector_delete_selected(&mut self, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn vector_smooth_selected(&mut self, cx: &mut Context<Self>) {
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
        let bounds_cell = draft.preview_bounds.clone();
        let dimensions = draft.state.source.dimensions();
        let path = draft.path.clone();
        let selected = draft.selected;
        let as_mask = draft.as_mask;
        let fill_color = draft.fill;
        let stroke = draft.stroke;
        let closed = path.subpaths.last().is_some_and(|s| s.closed);
        let preview = div()
            .id("vector-path-preview")
            .debug_selector(|| "vector-path-preview".into())
            // Keep the preview and edit/apply controls reachable above the
            // modal's fixed footer at the minimum supported viewport.
            .h(px(240.))
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
                        let _ = window.paint_image(
                            bounds,
                            target,
                            Corners::default(),
                            image.clone(),
                            0,
                            false,
                        );
                        paint_vector_overlay(
                            window,
                            target,
                            dimensions,
                            &path,
                            selected,
                            (!as_mask).then_some(fill_color),
                            if as_mask { None } else { stroke },
                        );
                    },
                )
                .size_full(),
            );
        let controls = div()
            .flex()
            .gap_2()
            .child(
                button("vector-delete", "Delete node", ButtonVariant::Outline, cx)
                    .debug_selector(|| "vector-delete".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_delete_selected(cx))),
            )
            .child(
                button("vector-smooth", "Smooth node", ButtonVariant::Outline, cx)
                    .debug_selector(|| "vector-smooth".into())
                    .on_click(cx.listener(|this, _, _, cx| this.vector_smooth_selected(cx))),
            )
            .child(
                button(
                    "vector-close",
                    if closed { "Open path" } else { "Close path" },
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(|| "vector-close".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(subpath) = this
                        .vector_draft
                        .as_mut()
                        .and_then(|d| d.path.subpaths.last_mut())
                    {
                        if subpath.anchors.len() >= 2 {
                            subpath.closed = !subpath.closed;
                        }
                    }
                    cx.notify();
                })),
            );
        let mut style = div().flex().flex_wrap().gap_2();
        if !as_mask {
            for (index, label) in [
                "Fill · #RRGGBBAA",
                "Stroke · #RRGGBBAA",
                "Stroke width · 0 = off",
            ]
            .iter()
            .enumerate()
            {
                let field_id = format!("vector-style-{index}");
                style = style.child(
                    div().w(px(150.)).min_w_0().child(*label).child(
                        input(
                            SharedString::from(field_id.clone()),
                            &self.detail_inputs[index],
                            window,
                            cx,
                        )
                        .debug_selector(move || field_id.clone()),
                    ),
                );
            }
            style = style.child(
                button(
                    "vector-style-preview",
                    "Preview style",
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(|| "vector-style-preview".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(error) = this.update_vector_style(cx) {
                        this.status = error.to_string();
                        cx.notify();
                    }
                })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(style)
            .child("Click to add nodes. Drag nodes or their handle dots.")
            .child(preview)
            .child(controls)
            .into_any_element()
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
) {
    for (si, subpath) in path.subpaths.iter().enumerate() {
        if subpath.anchors.is_empty() {
            continue;
        }
        for (paint_fill, color, width) in [
            (true, fill_color.filter(|_| subpath.closed), 0.),
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
                PathBuilder::fill()
            } else {
                PathBuilder::stroke(bounds.size.width * width / dimensions.0 as f32)
            };
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
            if subpath.closed {
                shape.close();
            }
            if let Ok(shape) = shape.build() {
                window.paint_path(shape, rgba(u32::from_be_bytes(color)));
            }
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
