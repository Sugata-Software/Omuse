//! Interactive normalized tone-curve graph for Camera Raw forms.
use gpui_kit::{
    Bounds, Context, EventEmitter, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Render,
    Window, canvas, div, fill, point, prelude::*, px, size,
};
use gpui_omarchy::ActiveTheme;
use omuse::camera_raw::CurvePoint;
use std::{cell::Cell, rc::Rc};

const WIDTH: f32 = 260.;
const HEIGHT: f32 = 160.;
const MAX_POINTS: usize = 32;
const MIN_GAP: f32 = 0.001;

#[derive(Clone, Debug, PartialEq)]
pub struct CurveChanged(pub Vec<CurvePoint>);

pub struct CurveEditor {
    points: Vec<CurvePoint>,
    drag: Option<usize>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<CurveChanged> for CurveEditor {}

impl CurveEditor {
    pub fn new(points: Vec<CurvePoint>) -> anyhow::Result<Self> {
        validate(&points)?;
        Ok(Self {
            points,
            drag: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
        })
    }

    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    pub fn set_points(
        &mut self,
        points: Vec<CurvePoint>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        validate(&points)?;
        self.points = points;
        self.drag = None;
        cx.notify();
        Ok(())
    }

    fn normalized(&self, position: gpui_kit::Point<Pixels>) -> CurvePoint {
        let bounds = self.bounds.get();
        CurvePoint {
            x: ((f32::from(position.x - bounds.origin.x)) / WIDTH).clamp(0., 1.),
            y: (1. - (f32::from(position.y - bounds.origin.y)) / HEIGHT).clamp(0., 1.),
        }
    }

    fn nearest(&self, point: CurvePoint, radius_pixels: f32) -> Option<usize> {
        self.points
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let dx = (value.x - point.x) * WIDTH;
                let dy = (value.y - point.y) * HEIGHT;
                let distance = dx.hypot(dy);
                (distance <= radius_pixels).then_some((distance, index))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, index)| index)
    }

    fn down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let point = self.normalized(event.position);
        if let Some(index) = self.nearest(point, 9.) {
            self.drag = Some(index);
        } else if self.points.len() < MAX_POINTS {
            let index = self.points.partition_point(|value| value.x < point.x);
            if index > 0
                && index < self.points.len()
                && point.x > self.points[index - 1].x + MIN_GAP
                && point.x < self.points[index].x - MIN_GAP
            {
                self.points.insert(index, point);
                self.drag = Some(index);
                cx.emit(CurveChanged(self.points.clone()));
            }
        }
        cx.notify();
    }

    fn moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some(index) = self.drag else { return };
        let mut next = self.normalized(event.position);
        if index == 0 {
            next.x = 0.;
        } else if index + 1 == self.points.len() {
            next.x = 1.;
        } else {
            let available = self.points[index + 1].x - self.points[index - 1].x;
            let gap = MIN_GAP.min(available / 3.);
            next.x = next.x.clamp(
                self.points[index - 1].x + gap,
                self.points[index + 1].x - gap,
            );
        }
        if self.points[index] != next {
            self.points[index] = next;
            cx.emit(CurveChanged(self.points.clone()));
            cx.notify();
        }
    }

    fn remove(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let point = self.normalized(event.position);
        if let Some(index) = self.nearest(point, 10.)
            && index > 0
            && index + 1 < self.points.len()
        {
            self.points.remove(index);
            self.drag = None;
            cx.emit(CurveChanged(self.points.clone()));
            cx.notify();
        }
    }
}

impl Render for CurveEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds_state = self.bounds.clone();
        let points = self.points.clone();
        let theme = cx.omarchy().clone();
        let background = theme.inset;
        let border = theme.border;
        let grid = theme.secondary;
        let curve = theme.foreground;
        let control = theme.accent;
        div()
            .id("camera-curve-editor")
            .debug_selector(|| "camera-curve-editor".into())
            .relative()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .border_1()
            .border_color(border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.down(event, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event, _, cx| this.remove(event, cx)),
            )
            .on_mouse_move(cx.listener(|this, event, _, cx| this.moved(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| bounds_state.set(bounds),
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(bounds, background));
                        for n in 1..4 {
                            let x = bounds.origin.x + px(WIDTH * n as f32 / 4.);
                            let y = bounds.origin.y + px(HEIGHT * n as f32 / 4.);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(x, bounds.origin.y),
                                    size(px(1.), bounds.size.height),
                                ),
                                grid,
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(bounds.origin.x, y),
                                    size(bounds.size.width, px(1.)),
                                ),
                                grid,
                            ));
                        }
                        let samples = 260usize;
                        for sample in 0..samples {
                            let x = sample as f32 / (samples - 1) as f32;
                            let y = evaluate_curve(x, &points);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.origin.x + px(x * WIDTH),
                                        bounds.origin.y + px((1. - y) * HEIGHT),
                                    ),
                                    size(px(2.), px(2.)),
                                ),
                                curve,
                            ));
                        }
                        for value in &points {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.origin.x + px(value.x * WIDTH - 4.),
                                        bounds.origin.y + px((1. - value.y) * HEIGHT - 4.),
                                    ),
                                    size(px(8.), px(8.)),
                                ),
                                control,
                            ));
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

pub fn evaluate_curve(x: f32, points: &[CurvePoint]) -> f32 {
    if points.len() < 2 {
        return x.clamp(0., 1.);
    }
    let x = x.clamp(0., 1.);
    let i = points
        .windows(2)
        .position(|pair| x < pair[1].x)
        .unwrap_or(points.len() - 2);
    let differences: Vec<f32> = points
        .windows(2)
        .map(|pair| (pair[1].y - pair[0].y) / (pair[1].x - pair[0].x))
        .collect();
    let slope = |index: usize| {
        if index == 0 {
            differences[0]
        } else if index + 1 == points.len() {
            differences[differences.len() - 1]
        } else if differences[index - 1] * differences[index] <= 0. {
            0.
        } else {
            2. / (1. / differences[index - 1] + 1. / differences[index])
        }
    };
    let width = points[i + 1].x - points[i].x;
    let t = ((x - points[i].x) / width).clamp(0., 1.);
    ((2. * t.powi(3) - 3. * t.powi(2) + 1.) * points[i].y
        + (t.powi(3) - 2. * t.powi(2) + t) * width * slope(i)
        + (-2. * t.powi(3) + 3. * t.powi(2)) * points[i + 1].y
        + (t.powi(3) - t.powi(2)) * width * slope(i + 1))
    .clamp(0., 1.)
}

fn validate(points: &[CurvePoint]) -> anyhow::Result<()> {
    anyhow::ensure!(
        (2..=MAX_POINTS).contains(&points.len()),
        "curve needs 2–32 points"
    );
    anyhow::ensure!(points.first().is_some_and(|point| point.x == 0.));
    anyhow::ensure!(points.last().is_some_and(|point| point.x == 1.));
    anyhow::ensure!(points.iter().all(|point| point.x.is_finite()
        && point.y.is_finite()
        && (0. ..=1.).contains(&point.x)
        && (0. ..=1.).contains(&point.y)));
    anyhow::ensure!(points.windows(2).all(|pair| pair[0].x < pair[1].x));
    Ok(())
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext};

    #[gpui_kit::test]
    fn pointer_add_drag_and_remove_keep_curve_valid(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (editor, cx) = cx.add_window_view(|_, _| {
            CurveEditor::new(vec![
                CurvePoint { x: 0., y: 0. },
                CurvePoint { x: 1., y: 1. },
            ])
            .unwrap()
        });
        cx.simulate_resize(size(px(600.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("camera-curve-editor").unwrap();
        let middle = bounds.center();
        cx.simulate_mouse_down(middle, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            point(middle.x + px(25.), middle.y - px(20.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(middle, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let editor = editor.read(cx);
            assert_eq!(editor.points().len(), 3);
            validate(editor.points()).unwrap();
        });
        cx.simulate_mouse_down(
            point(middle.x + px(25.), middle.y - px(20.)),
            MouseButton::Right,
            Modifiers::default(),
        );
        cx.update(|_, cx| assert_eq!(editor.read(cx).points().len(), 2));
    }
}
