//! Bounded direct manipulation of retained vector gradients on the shared canvas.
use super::*;
use omuse::vector_scene::{GradientFill, GradientKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Handle {
    Start,
    End,
    Focus,
    Stop(usize),
}
#[derive(Clone)]
pub(super) struct GradientDrag {
    original: GradientFill,
    handle: Handle,
    screen_start: Point<Pixels>,
    handle_start: Point<Pixels>,
}

fn mapped(p: VectorPoint, m: [f32; 6]) -> VectorPoint {
    VectorPoint {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    }
}
fn inverse(p: VectorPoint, m: [f32; 6]) -> Option<VectorPoint> {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let x = p.x - m[4];
    let y = p.y - m[5];
    Some(VectorPoint {
        x: (m[3] * x - m[2] * y) / det,
        y: (m[0] * y - m[1] * x) / det,
    })
}
pub(super) fn handles(g: &GradientFill) -> Vec<(Handle, VectorPoint, [u8; 4])> {
    let (start, end, focus) = match g.kind {
        GradientKind::Linear { start, end } => (start, end, None),
        GradientKind::Radial {
            center,
            focus,
            radius,
        } => (
            center,
            VectorPoint {
                x: center.x + radius,
                y: center.y,
            },
            Some(focus),
        ),
    };
    let mut out = vec![
        (
            Handle::Start,
            mapped(start, g.transform),
            [245, 245, 245, 255],
        ),
        (Handle::End, mapped(end, g.transform), [245, 245, 245, 255]),
    ];
    if let Some(focus) = focus {
        out.push((
            Handle::Focus,
            mapped(focus, g.transform),
            [240, 190, 80, 255],
        ));
    }
    for (i, stop) in g.stops.iter().enumerate() {
        let t = stop.offset;
        out.push((
            Handle::Stop(i),
            mapped(
                VectorPoint {
                    x: start.x + (end.x - start.x) * t,
                    y: start.y + (end.y - start.y) * t,
                },
                g.transform,
            ),
            stop.color,
        ));
    }
    out
}
fn changed(
    drag: &GradientDrag,
    p: VectorPoint,
    constrain: bool,
) -> Option<(GradientFill, Option<usize>)> {
    let mut g = drag.original.clone();
    let p = inverse(p, g.transform)?;
    let mut selected = None;
    match drag.handle {
        Handle::Start => match &mut g.kind {
            GradientKind::Linear { start, end } => {
                let delta = snap_vector(
                    VectorPoint {
                        x: p.x - end.x,
                        y: p.y - end.y,
                    },
                    constrain,
                );
                *start = VectorPoint {
                    x: end.x + delta.x,
                    y: end.y + delta.y,
                };
            }
            GradientKind::Radial { center, focus, .. } => {
                focus.x += p.x - center.x;
                focus.y += p.y - center.y;
                *center = p;
            }
        },
        Handle::End => match &mut g.kind {
            GradientKind::Linear { start, end } => {
                let delta = snap_vector(
                    VectorPoint {
                        x: p.x - start.x,
                        y: p.y - start.y,
                    },
                    constrain,
                );
                *end = VectorPoint {
                    x: start.x + delta.x,
                    y: start.y + delta.y,
                };
            }
            GradientKind::Radial {
                center,
                focus,
                radius,
            } => {
                *radius = (p.x - center.x).hypot(p.y - center.y).max(0.001);
                let d = (focus.x - center.x).hypot(focus.y - center.y);
                if d >= *radius {
                    let t = *radius * 0.999 / d;
                    focus.x = center.x + (focus.x - center.x) * t;
                    focus.y = center.y + (focus.y - center.y) * t;
                }
            }
        },
        Handle::Focus => {
            if let GradientKind::Radial {
                center,
                focus,
                radius,
            } = &mut g.kind
            {
                let delta = VectorPoint {
                    x: p.x - center.x,
                    y: p.y - center.y,
                };
                let len = delta.x.hypot(delta.y);
                let t = if len >= *radius {
                    *radius * 0.999 / len
                } else {
                    1.
                };
                *focus = VectorPoint {
                    x: center.x + delta.x * t,
                    y: center.y + delta.y * t,
                };
            }
        }
        Handle::Stop(index) => {
            let (a, b) = match g.kind {
                GradientKind::Linear { start, end } => (start, end),
                GradientKind::Radial { center, radius, .. } => (
                    center,
                    VectorPoint {
                        x: center.x + radius,
                        y: center.y,
                    },
                ),
            };
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = dx * dx + dy * dy;
            if length < 1e-12 {
                return None;
            }
            g.stops[index].offset = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0., 1.);
            let mut stops: Vec<_> = g.stops.into_iter().enumerate().collect();
            stops.sort_by(|a, b| a.1.offset.total_cmp(&b.1.offset));
            selected = stops.iter().position(|(i, _)| *i == index);
            g.stops = stops.into_iter().map(|(_, s)| s).collect();
        }
    }
    g.validate().ok()?;
    Some((g, selected))
}

impl EditorView {
    pub(in crate::ui) fn vector_gradient_active(&self) -> bool {
        self.tool != Tool::Hand
            && self
                .vector_draft
                .as_ref()
                .and_then(|d| d.scene.as_ref())
                .is_some_and(|s| s.gradient_edit)
    }
    pub(super) fn toggle_vector_gradient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return;
        };
        if scene.selected_objects.len() != 1
            || scene.artwork.objects[scene.active].fill_gradient.is_none()
        {
            self.status = "Select one object with a linear or radial gradient first".into();
            cx.notify();
            return;
        }
        scene.gradient_edit = !scene.gradient_edit;
        scene.gradient_drag = None;
        scene.drag_origin = None;
        scene.marquee = None;
        scene.mode = super::scene::SceneMode::Select;
        self.tool = Tool::Move;
        self.status="Gradient handles · Drag endpoints or colour stops · Shift constrains the axis · Alt accesses a radial centre beneath the focus".into();
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn vector_gradient_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(scene) = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .filter(|s| s.gradient_edit)
        else {
            return false;
        };
        let Some(g) = scene.artwork.objects[scene.active].fill_gradient.as_ref() else {
            return false;
        };
        let found = handles(g)
            .into_iter()
            .filter(|(h, _, _)| !event.modifiers.alt || matches!(h, Handle::Start | Handle::End))
            .filter_map(|(h, p, _)| {
                let q = self.vector_screen_point(p)?;
                let d = f32::from(q.x - event.position.x).hypot(f32::from(q.y - event.position.y));
                // Stop chips are drawn below the axis to keep endpoints accessible.
                let d = if matches!(h, Handle::Stop(_)) {
                    f32::from(q.x - event.position.x)
                        .hypot(f32::from(q.y + px(15.) - event.position.y))
                } else {
                    d
                };
                (d <= 9.).then_some((h, d))
            })
            .min_by(|a, b| {
                a.1.total_cmp(&b.1).then_with(|| {
                    let priority = |h| if matches!(h, Handle::Focus) { 0 } else { 1 };
                    priority(a.0).cmp(&priority(b.0))
                })
            });
        let Some((handle, _)) = found else {
            return false;
        };
        let original = g.clone();
        let Some(handle_start) = handles(&original)
            .into_iter()
            .find(|(candidate, _, _)| *candidate == handle)
            .and_then(|(_, point, _)| self.vector_screen_point(point))
        else {
            return false;
        };
        self.scene_checkpoint();
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        if let Handle::Stop(i) = handle {
            scene.gradient_stop = i;
        }
        scene.gradient_drag = Some(GradientDrag {
            original,
            handle,
            screen_start: event.position,
            handle_start,
        });
        self.load_scene_style(window, cx);
        cx.stop_propagation();
        cx.notify();
        true
    }
    pub(super) fn vector_gradient_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .and_then(|s| s.gradient_drag.clone())
        else {
            return false;
        };
        // Move the exact original handle by the physical pointer delta. This
        // preserves an off-centre grab, including stop chips drawn below the axis.
        let target = drag.handle_start + (event.position - drag.screen_start);
        let Some(mut p) = self.vector_canvas_point(target) else {
            return true;
        };
        // Constrain the visible axis, including rotated/nonuniform gradients
        // and placed vector layers, rather than snapping in gradient-local space.
        if event.modifiers.shift
            && let GradientKind::Linear { start, end } = drag.original.kind
        {
            let fixed = match drag.handle {
                Handle::Start => Some(end),
                Handle::End => Some(start),
                _ => None,
            };
            if let Some(fixed) = fixed
                .and_then(|fixed| self.vector_screen_point(mapped(fixed, drag.original.transform)))
            {
                let delta = snap_vector(
                    VectorPoint {
                        x: f32::from(target.x - fixed.x),
                        y: f32::from(target.y - fixed.y),
                    },
                    true,
                );
                if let Some(constrained) =
                    self.vector_canvas_point(fixed + point(px(delta.x), px(delta.y)))
                {
                    p = constrained;
                }
            }
        }
        let result = if event.position == drag.screen_start {
            // Preserve exact floats on a stationary click or a return to origin.
            // A screen→layer→gradient round trip can otherwise introduce drift.
            Some((
                drag.original.clone(),
                match drag.handle {
                    Handle::Stop(i) => Some(i),
                    _ => None,
                },
            ))
        } else {
            changed(&drag, p, false)
        };
        if let Some((g, selected)) = result {
            let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            if scene.artwork.objects[scene.active].fill_gradient.as_ref() == Some(&g) {
                return true;
            }
            scene.artwork.objects[scene.active].fill = Some(g.stops[0].color);
            scene.artwork.objects[scene.active].fill_gradient = Some(g);
            if let Some(i) = selected {
                scene.gradient_stop = i;
            }
            self.load_scene_style(window, cx);
            self.vector_scene_changed(cx);
            cx.notify();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omuse::vector_scene::{GradientSpread, GradientStop};
    fn gradient() -> GradientFill {
        GradientFill {
            kind: GradientKind::Linear {
                start: VectorPoint { x: 0., y: 0. },
                end: VectorPoint { x: 10., y: 0. },
            },
            stops: vec![
                GradientStop {
                    offset: 0.,
                    color: [255, 0, 0, 255],
                },
                GradientStop {
                    offset: 0.5,
                    color: [0, 255, 0, 255],
                },
                GradientStop {
                    offset: 1.,
                    color: [0, 0, 255, 255],
                },
            ],
            spread: GradientSpread::Pad,
            transform: [2., 0., 1., 3., 4., 5.],
            method: Default::default(),
            dither: false,
        }
    }
    #[test]
    fn transformed_endpoint_drag_uses_inverse_gradient_coordinates() {
        let drag = GradientDrag {
            original: gradient(),
            handle: Handle::End,
            screen_start: point(px(0.), px(0.)),
            handle_start: point(px(0.), px(0.)),
        };
        let (g, _) = changed(&drag, VectorPoint { x: 30., y: 17. }, false).unwrap();
        assert_eq!(
            g.kind,
            GradientKind::Linear {
                start: VectorPoint { x: 0., y: 0. },
                end: VectorPoint { x: 11., y: 4. }
            }
        );
        assert!(changed(&drag, VectorPoint { x: f32::NAN, y: 0. }, false).is_none());
    }
    #[test]
    fn stop_drag_preserves_colour_identity_after_reordering_and_clamps() {
        let original = gradient();
        let drag = GradientDrag {
            original,
            handle: Handle::Stop(0),
            screen_start: point(px(0.), px(0.)),
            handle_start: point(px(0.), px(0.)),
        };
        let (g, selected) = changed(&drag, VectorPoint { x: 100., y: 5. }, false).unwrap();
        // Equal-offset stops retain their original order; selection follows the
        // dragged colour rather than assuming it sorts after the endpoint.
        assert_eq!(selected, Some(1));
        assert_eq!(g.stops[selected.unwrap()].color, [255, 0, 0, 255]);
        assert_eq!(g.stops[selected.unwrap()].offset, 1.);
        assert_eq!(g.stops[2].color, [0, 0, 255, 255]);
    }
    #[test]
    fn radial_focus_and_radius_remain_valid_at_degenerate_pointer_positions() {
        let mut g = gradient();
        g.kind = GradientKind::Radial {
            center: VectorPoint { x: 0., y: 0. },
            focus: VectorPoint { x: 4., y: 0. },
            radius: 5.,
        };
        let (g, _) = changed(
            &GradientDrag {
                original: g,
                handle: Handle::End,
                screen_start: point(px(0.), px(0.)),
                handle_start: point(px(0.), px(0.)),
            },
            VectorPoint { x: 4., y: 5. },
            false,
        )
        .unwrap();
        g.validate().unwrap();
        let (g, _) = changed(
            &GradientDrag {
                original: g,
                handle: Handle::Focus,
                screen_start: point(px(0.), px(0.)),
                handle_start: point(px(0.), px(0.)),
            },
            VectorPoint { x: 400., y: 500. },
            false,
        )
        .unwrap();
        g.validate().unwrap();
    }
}
