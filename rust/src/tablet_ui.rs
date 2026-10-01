//! Native Wayland stylus input adapter. Pressure and tilt come from tablet-v2.
use super::*;
use gpui_kit::{TabletEvent, TabletPhase};
use omuse::brush_dynamics::InputPoint;

impl EditorView {
    pub(super) fn tablet(
        &mut self,
        event: &TabletEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tablet_painting && event.tool_id != self.tablet_tool_id {
            cx.stop_propagation();
            return;
        }
        let ending = matches!(event.phase, TabletPhase::Up | TabletPhase::ProximityOut);
        if self.tablet_painting && ending {
            self.tablet_painting = false;
            self.drag_start = None;
            self.editor.finish_stroke();
            self.changed(cx);
            cx.stop_propagation();
            return;
        }
        if self.dialog != Dialog::None
            || self.busy
            || self.inline_text.is_some()
            || self.vector_scene_active()
            || self.image_trace_active()
            || self.crop.is_some()
        {
            return;
        }
        if !matches!(self.tool, Tool::Brush | Tool::Pencil | Tool::Eraser) {
            return;
        }
        let (x, y) = self.coordinates(event.position);
        if ![x, y, event.pressure, event.tilt_x, event.tilt_y]
            .iter()
            .all(|v| v.is_finite())
        {
            return;
        }
        let input = InputPoint {
            x,
            y,
            pressure: event.pressure.clamp(0., 1.),
            tilt_x: (event.tilt_x / 90.).clamp(-1., 1.),
            tilt_y: (event.tilt_y / 90.).clamp(-1., 1.),
        };
        match event.phase {
            TabletPhase::Down => {
                if !self.viewport.get().contains(&event.position)
                    || x < 0.
                    || y < 0.
                    || x >= self.editor.document.width as f32
                    || y >= self.editor.document.height as f32
                {
                    return;
                }
                self.finish_interaction(cx);
                self.focus.focus(window, cx);
                let tool = match self.tool {
                    Tool::Eraser => PaintTool::Eraser,
                    Tool::Pencil => PaintTool::Pencil,
                    _ => PaintTool::Brush,
                };
                let started = if self.paint_mask {
                    self.editor.begin_stylus_mask_stroke(input, tool)
                } else {
                    self.editor.begin_stylus_stroke(input, tool)
                };
                if started {
                    self.tablet_painting = true;
                    self.tablet_tool_id = event.tool_id;
                    self.drag_start = Some((x, y));
                    self.status = "Painting with tablet pressure and tilt".into();
                    self.queue_stroke_frame(window, cx);
                } else {
                    self.status="Choose an unlocked raster layer or mask; paint above editable sources on a new layer".into();
                    cx.notify();
                }
                cx.stop_propagation();
            }
            TabletPhase::Move if self.tablet_painting => {
                self.editor.continue_stylus_stroke_at_zoom(input, self.zoom);
                self.queue_stroke_frame(window, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{PlatformInput, TestAppContext};
    #[gpui_kit::test]
    fn tablet_click_selects_a_normal_toolbar_button(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().join("recovery"));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.recovery = recovery;
            v.dialog = Dialog::None;
            v.tool = Tool::Brush;
            v
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let position = cx.debug_bounds("tool-eraser").unwrap().center();
        for phase in [TabletPhase::Down, TabletPhase::Up] {
            cx.update(|window, cx| {
                window.dispatch_event(
                    PlatformInput::Tablet(TabletEvent {
                        tool_id: 7,
                        phase,
                        position,
                        pressure: if phase == TabletPhase::Down { 0.5 } else { 0. },
                        in_proximity: true,
                        contact: phase == TabletPhase::Down,
                        ..Default::default()
                    }),
                    cx,
                );
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        view.update(cx, |v, _| {
            assert_eq!(v.tool, Tool::Eraser);
            assert!(!v.tablet_painting);
            assert_eq!(v.editor.undo_depth(), 0);
        });
    }

    #[gpui_kit::test]
    fn measured_pressure_changes_ink_and_proximity_out_commits_one_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery = Recovery::at(temp.path().join("recovery"));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.recovery = recovery;
            v.editor = Editor::new(Document::new(64, 32));
            v.dialog = Dialog::None;
            v.tool = Tool::Brush;
            v.zoom = 1.;
            v.editor.brush.size = 10.;
            v.editor.brush.hardness = 1.;
            v.editor.brush.color = [190, 80, 20, 255];
            v.refresh(cx);
            v
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = view.update(cx, |v, _| v.viewport.get().center());
        let send = |phase, dx, pressure, cx: &mut gpui_kit::VisualTestContext| {
            cx.update(|window, cx| {
                window.dispatch_event(
                    PlatformInput::Tablet(TabletEvent {
                        phase,
                        position: center + point(px(dx), px(0.)),
                        pressure,
                        tilt_x: 45.,
                        tilt_y: 0.,
                        in_proximity: phase != TabletPhase::ProximityOut,
                        contact: matches!(phase, TabletPhase::Down | TabletPhase::Move),
                        ..Default::default()
                    }),
                    cx,
                )
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        send(TabletPhase::Down, -20., 0.25, cx);
        send(TabletPhase::Move, -10., 0.25, cx);
        send(TabletPhase::Move, 10., 1., cx);
        send(TabletPhase::Move, 20., 1., cx);
        send(TabletPhase::ProximityOut, 20., 0., cx);
        view.update(cx, |v, _| {
            assert!(!v.tablet_painting);
            assert_eq!(v.editor.undo_depth(), 1);
            let p = v.editor.document.layers[0].image.as_ref().unwrap();
            assert!(p.get_pixel(12, 16)[3] > 0);
            assert!(p.get_pixel(52, 16)[3] > p.get_pixel(12, 16)[3]);
            assert!(v.editor.undo());
            assert!(
                v.editor.document.layers[0]
                    .image
                    .as_ref()
                    .unwrap()
                    .pixels()
                    .all(|p| p[3] == 0)
            );
        });
    }
}
