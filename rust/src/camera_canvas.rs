//! Camera Raw preview gestures for point-color sampling and guided geometry.
use gpui_kit::{
    Bounds, Context, Corners, EventEmitter, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, RenderImage, Window, canvas, div, fill, point, prelude::*, px,
    size,
};
use gpui_omarchy::ActiveTheme;
use image::{Frame, RgbaImage};
use omuse::camera_raw::{GeometryGuide, PointColor};
use std::{cell::Cell, rc::Rc, sync::Arc};

const WIDTH: f32 = 350.;
const HEIGHT: f32 = 200.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraCanvasMode {
    PointColor,
    Geometry,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CameraCanvasEvent {
    Picked(PointColor),
    Guide(GeometryGuide),
}

pub struct CameraCanvas {
    source: Arc<RgbaImage>,
    preview: Arc<RenderImage>,
    mode: CameraCanvasMode,
    guides: Vec<GeometryGuide>,
    drag_start: Option<(f32, f32)>,
    drag_current: Option<(f32, f32)>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<CameraCanvasEvent> for CameraCanvas {}

impl CameraCanvas {
    pub fn new(source: Arc<RgbaImage>, mode: CameraCanvasMode) -> anyhow::Result<Self> {
        anyhow::ensure!(
            source.width() > 0 && source.height() > 0,
            "preview image is empty"
        );
        let preview = render_image(&source);
        Ok(Self {
            source,
            preview,
            mode,
            guides: Vec::new(),
            drag_start: None,
            drag_current: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
        })
    }

    pub fn set_source(
        &mut self,
        source: Arc<RgbaImage>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            source.width() > 0 && source.height() > 0,
            "preview image is empty"
        );
        self.preview = render_image(&source);
        self.source = source;
        self.drag_start = None;
        self.drag_current = None;
        cx.notify();
        Ok(())
    }

    pub fn set_mode(&mut self, mode: CameraCanvasMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.drag_start = None;
        self.drag_current = None;
        cx.notify();
    }

    pub fn set_guides(&mut self, guides: Vec<GeometryGuide>, cx: &mut Context<Self>) {
        self.guides = guides.into_iter().take(16).collect();
        cx.notify();
    }

    fn image_rect(&self) -> Bounds<Pixels> {
        aspect_fit(self.bounds.get(), self.source.width(), self.source.height())
    }

    fn normalized(&self, position: gpui_kit::Point<Pixels>) -> Option<(f32, f32)> {
        let rect = self.image_rect();
        if rect.size.width <= px(0.) || rect.size.height <= px(0.) {
            return None;
        }
        if position.x < rect.origin.x
            || position.y < rect.origin.y
            || position.x > rect.origin.x + rect.size.width
            || position.y > rect.origin.y + rect.size.height
        {
            return None;
        }
        Some((
            (f32::from(position.x - rect.origin.x) / f32::from(rect.size.width)).clamp(0., 1.),
            (f32::from(position.y - rect.origin.y) / f32::from(rect.size.height)).clamp(0., 1.),
        ))
    }

    fn down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(normalized) = self.normalized(event.position) else {
            return;
        };
        match self.mode {
            CameraCanvasMode::PointColor => {
                let x = (normalized.0 * self.source.width() as f32)
                    .floor()
                    .min(self.source.width() as f32 - 1.) as u32;
                let y = (normalized.1 * self.source.height() as f32)
                    .floor()
                    .min(self.source.height() as f32 - 1.) as u32;
                let pixel = self.source.get_pixel(x, y).0;
                let (hue, saturation, luminance) = rgb_to_hsl(pixel[0], pixel[1], pixel[2]);
                cx.emit(CameraCanvasEvent::Picked(PointColor {
                    hue,
                    saturation,
                    luminance,
                    ..PointColor::default()
                }));
            }
            CameraCanvasMode::Geometry => {
                self.drag_start = Some(normalized);
                self.drag_current = Some(normalized);
                cx.notify();
            }
        }
    }

    fn moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.dragging() && self.drag_start.is_some() {
            self.drag_current = self.normalized(event.position);
            cx.notify();
        }
    }

    fn up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(start) = self.drag_start.take() else {
            return;
        };
        self.drag_current = None;
        let Some(end) = self.normalized(event.position) else {
            cx.notify();
            return;
        };
        if (end.0 - start.0).hypot(end.1 - start.1) >= 0.005 {
            cx.emit(CameraCanvasEvent::Guide(GeometryGuide {
                start_x: start.0,
                start_y: 1. - start.1,
                end_x: end.0,
                end_y: 1. - end.1,
            }));
        }
        cx.notify();
    }
}

impl Render for CameraCanvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds_state = self.bounds.clone();
        let preview = self.preview.clone();
        let source_size = (self.source.width(), self.source.height());
        let mut guides = self.guides.clone();
        if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
            guides.push(GeometryGuide {
                start_x: start.0,
                start_y: 1. - start.1,
                end_x: end.0,
                end_y: 1. - end.1,
            });
        }
        let theme = cx.omarchy().clone();
        div()
            .id("camera-canvas")
            .debug_selector(|| "camera-canvas".into())
            .relative()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.down(event, cx)),
            )
            .on_mouse_move(cx.listener(|this, event, _, cx| this.moved(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.up(event, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.up(event, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, _| bounds_state.set(bounds),
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(bounds, theme.inset));
                        let rect = aspect_fit(bounds, source_size.0, source_size.1);
                        let _ = window.paint_image(
                            rect,
                            rect,
                            Corners::default(),
                            preview.clone(),
                            0,
                            false,
                        );
                        for guide in &guides {
                            let start = point(
                                rect.origin.x + rect.size.width * guide.start_x,
                                rect.origin.y + rect.size.height * (1. - guide.start_y),
                            );
                            let end = point(
                                rect.origin.x + rect.size.width * guide.end_x,
                                rect.origin.y + rect.size.height * (1. - guide.end_y),
                            );
                            let steps = (f32::from((end.x - start.x).abs())
                                .hypot(f32::from((end.y - start.y).abs()))
                                / 3.)
                                .ceil()
                                .clamp(1., 512.) as usize;
                            for step in 0..=steps {
                                let t = step as f32 / steps as f32;
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(
                                            start.x + (end.x - start.x) * t,
                                            start.y + (end.y - start.y) * t,
                                        ),
                                        size(px(2.), px(2.)),
                                    ),
                                    theme.accent,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}

fn aspect_fit(bounds: Bounds<Pixels>, width: u32, height: u32) -> Bounds<Pixels> {
    let scale = (f32::from(bounds.size.width) / width as f32)
        .min(f32::from(bounds.size.height) / height as f32);
    let size = size(px(width as f32 * scale), px(height as f32 * scale));
    Bounds::new(
        point(
            bounds.origin.x + (bounds.size.width - size.width) / 2.,
            bounds.origin.y + (bounds.size.height - size.height) / 2.,
        ),
        size,
    )
}

fn rgb_to_hsl(red: u8, green: u8, blue: u8) -> (f32, f32, f32) {
    let (red, green, blue) = (red as f32 / 255., green as f32 / 255., blue as f32 / 255.);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let luminance = (max + min) * 0.5;
    let delta = max - min;
    if delta == 0. {
        return (0., 0., luminance);
    }
    let saturation = delta / (1. - (2. * luminance - 1.).abs());
    let sector = if max == red {
        ((green - blue) / delta).rem_euclid(6.)
    } else if max == green {
        (blue - red) / delta + 2.
    } else {
        (red - green) / delta + 4.
    };
    (sector * 60., saturation, luminance)
}

fn render_image(source: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = source.clone();
    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext};

    #[test]
    fn hsl_units_match_point_color_contract() {
        assert_eq!(rgb_to_hsl(255, 0, 0), (0., 1., 0.5));
        assert_eq!(rgb_to_hsl(0, 255, 0), (120., 1., 0.5));
        assert_eq!(rgb_to_hsl(128, 128, 128), (0., 0., 128. / 255.));
    }

    #[gpui_kit::test]
    fn geometry_drag_emits_normalized_guide_and_ignores_letterbox(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let source = Arc::new(RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([255, 0, 0, 255]),
        ));
        let (view, cx) = cx
            .add_window_view(|_, _| CameraCanvas::new(source, CameraCanvasMode::Geometry).unwrap());
        let emitted = Rc::new(Cell::new(None));
        let capture = emitted.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &CameraCanvasEvent, _| {
                if let CameraCanvasEvent::Guide(guide) = event {
                    capture.set(Some(*guide));
                }
            })
            .detach();
        });
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("camera-canvas").unwrap();
        let start = point(bounds.center().x - px(25.), bounds.center().y - px(25.));
        let end = point(bounds.center().x + px(25.), bounds.center().y + px(25.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        let guide = emitted.get().unwrap();
        assert!(guide.start_x < guide.end_x && guide.start_y > guide.end_y);
        assert!(
            [guide.start_x, guide.start_y, guide.end_x, guide.end_y]
                .into_iter()
                .all(|value| (0. ..=1.).contains(&value))
        );
    }
}
