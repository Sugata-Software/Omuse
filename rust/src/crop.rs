//! Reversible, view-only crop geometry, bounded to the existing canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    Move {
        start: (f32, f32),
        original: CropRect,
    },
    Corner {
        anchor: (f32, f32),
    },
}

#[derive(Clone, Debug)]
pub struct CropFrame {
    pub rect: CropRect,
    pub ratio: Option<f32>,
    pub preset: usize,
    pub swapped: bool,
    width: f32,
    height: f32,
    drag: Option<Drag>,
}

pub const PRESETS: &[&str] = &["Free", "Original", "1:1", "4:5", "3:2", "16:9", "3:4"];

impl CropFrame {
    pub fn canvas_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
    pub fn preset_label(&self, index: usize) -> &'static str {
        let label = PRESETS.get(index).copied().unwrap_or("Free");
        if !self.swapped {
            return label;
        }
        match index {
            3 => "5:4",
            4 => "2:3",
            5 => "9:16",
            6 => "4:3",
            _ => label,
        }
    }
    pub fn new(width: u32, height: u32, selection: Option<(u32, u32, u32, u32)>) -> Self {
        let (x, y, w, h) = selection
            .filter(|&(x, y, w, h)| {
                w > 0 && h > 0 && x.saturating_add(w) <= width && y.saturating_add(h) <= height
            })
            .unwrap_or((0, 0, width, height));
        Self {
            rect: CropRect {
                x: x as f32,
                y: y as f32,
                width: w as f32,
                height: h as f32,
            },
            ratio: None,
            preset: 0,
            swapped: false,
            width: width as f32,
            height: height as f32,
            drag: None,
        }
    }
    pub fn set_preset(&mut self, preset: usize) {
        if preset >= PRESETS.len() {
            return;
        }
        self.drag = None;
        self.preset = preset;
        let ratio = match preset {
            1 => Some(self.width / self.height),
            2 => Some(1.),
            3 => Some(4. / 5.),
            4 => Some(3. / 2.),
            5 => Some(16. / 9.),
            6 => Some(3. / 4.),
            _ => None,
        };
        self.ratio = ratio.map(|r| if self.swapped { 1. / r } else { r });
        if let Some(r) = self.ratio {
            let center = (
                self.rect.x + self.rect.width * 0.5,
                self.rect.y + self.rect.height * 0.5,
            );
            let w = (self.rect.width * self.rect.height * r)
                .sqrt()
                .max(1.)
                .min(self.width)
                .min(self.height * r);
            let h = (w / r).min(self.height);
            self.rect = CropRect {
                x: (center.0 - w * 0.5).clamp(0., (self.width - w).max(0.)),
                y: (center.1 - h * 0.5).clamp(0., (self.height - h).max(0.)),
                width: w,
                height: h,
            };
        }
    }
    pub fn swap(&mut self) {
        self.swapped = !self.swapped;
        self.set_preset(self.preset);
    }
    pub fn begin(&mut self, p: (f32, f32), tolerance: f32) {
        if !p.0.is_finite() || !p.1.is_finite() {
            return;
        }
        let r = self.rect;
        let corners = [
            (r.x, r.y),
            (r.x + r.width, r.y),
            (r.x + r.width, r.y + r.height),
            (r.x, r.y + r.height),
        ];
        let tolerance = tolerance.min(r.width * 0.3).min(r.height * 0.3);
        if let Some(i) = corners
            .iter()
            .position(|c| (c.0 - p.0).abs() <= tolerance && (c.1 - p.1).abs() <= tolerance)
        {
            self.drag = Some(Drag::Corner {
                anchor: corners[(i + 2) % 4],
            });
        } else if p.0 >= r.x && p.0 <= r.x + r.width && p.1 >= r.y && p.1 <= r.y + r.height {
            self.drag = Some(Drag::Move {
                start: p,
                original: r,
            });
        } else {
            self.drag = Some(Drag::Corner {
                anchor: (p.0.clamp(0., self.width), p.1.clamp(0., self.height)),
            });
        }
    }
    pub fn update(&mut self, p: (f32, f32)) {
        if !p.0.is_finite() || !p.1.is_finite() {
            return;
        }
        match self.drag {
            Some(Drag::Move { start, original }) => {
                self.rect.x =
                    (original.x + p.0 - start.0).clamp(0., (self.width - original.width).max(0.));
                self.rect.y =
                    (original.y + p.1 - start.1).clamp(0., (self.height - original.height).max(0.));
            }
            Some(Drag::Corner { anchor: a }) => {
                let p = (p.0.clamp(0., self.width), p.1.clamp(0., self.height));
                let (sx, sy) = (
                    if p.0 < a.0 { -1. } else { 1. },
                    if p.1 < a.1 { -1. } else { 1. },
                );
                let (max_w, max_h) = (
                    if sx < 0. { a.0 } else { self.width - a.0 },
                    if sy < 0. { a.1 } else { self.height - a.1 },
                );
                let (mut w, mut h) = ((p.0 - a.0).abs(), (p.1 - a.1).abs());
                if let Some(r) = self.ratio {
                    w = w.max(h * r).min(max_w).min(max_h * r);
                    h = (w / r).min(max_h);
                }
                if w < 1. || h < 1. {
                    return;
                }
                self.rect = CropRect {
                    x: a.0.min(a.0 + sx * w).max(0.),
                    y: a.1.min(a.1 + sy * h).max(0.),
                    width: w,
                    height: h,
                };
            }
            None => {}
        }
    }
    pub fn end(&mut self) {
        self.drag = None;
    }
    pub fn nudge(&mut self, dx: f32, dy: f32) {
        self.end();
        self.rect.x = (self.rect.x + dx).clamp(0., (self.width - self.rect.width).max(0.));
        self.rect.y = (self.rect.y + dy).clamp(0., (self.height - self.rect.height).max(0.));
    }
    pub fn pixels(&self) -> (i32, i32, u32, u32) {
        let x = self.rect.x.round().clamp(0., self.width - 1.) as u32;
        let y = self.rect.y.round().clamp(0., self.height - 1.) as u32;
        let w = self.rect.width.round().max(1.) as u32;
        let h = self.rect.height.round().max(1.) as u32;
        (
            x as i32,
            y as i32,
            w.min(self.width as u32 - x),
            h.min(self.height as u32 - y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portrait_three_four_crop_and_swapped_label_match_the_geometry() {
        let mut crop = CropFrame::new(1200, 1200, None);
        crop.set_preset(6);
        assert_eq!(crop.preset_label(6), "3:4");
        assert_eq!(crop.ratio, Some(0.75));
        assert_eq!((crop.pixels().2, crop.pixels().3), (900, 1200));
        crop.swap();
        assert_eq!(crop.preset_label(6), "4:3");
        assert!((crop.ratio.unwrap() - 4. / 3.).abs() < 0.0001);
        assert!(crop.pixels().2 > crop.pixels().3);
        crop.swap();
        assert_eq!(crop.preset_label(6), "3:4");
    }
    #[test]
    fn ratios_and_swapping_stay_inside_canvas() {
        for (w, h) in [
            (1200, 800),
            (1, 1),
            (1, 200),
            (200, 1),
            (101, 99),
            (3, 59),
            (29999, 3333),
        ] {
            for preset in 0..PRESETS.len() {
                let mut c = CropFrame::new(w, h, None);
                c.set_preset(preset);
                for _ in 0..2 {
                    let (x, y, cw, ch) = c.pixels();
                    assert!(
                        cw > 0
                            && ch > 0
                            && x >= 0
                            && y >= 0
                            && x as u32 + cw <= w
                            && y as u32 + ch <= h
                    );
                    if let Some(r) = c.ratio {
                        assert!((c.rect.width / c.rect.height - r).abs() < 0.001);
                    }
                    c.swap();
                }
            }
        }
    }
    #[test]
    fn resize_move_crossing_and_release_keep_a_bounded_rectangle() {
        let mut c = CropFrame::new(120, 80, None);
        c.set_preset(2);
        c.begin((20., 0.), 4.);
        c.update((50., 30.));
        c.end();
        assert_eq!(c.pixels(), (50, 30, 50, 50));
        c.begin((75., 55.), 4.);
        c.update((-50., -50.));
        c.end();
        assert_eq!(c.pixels(), (0, 0, 50, 50));
        c.update((1000., 1000.));
        assert_eq!(c.pixels(), (0, 0, 50, 50));
        c.begin((0., 0.), 4.);
        c.update((100., 70.));
        c.end();
        assert_eq!(c.pixels(), (50, 50, 30, 30));
    }
    #[test]
    fn selection_seed_and_new_draw_are_independent_of_editor_selection() {
        let mut c = CropFrame::new(100, 80, Some((20, 30, 40, 10)));
        assert_eq!(c.pixels(), (20, 30, 40, 10));
        c.begin((5., 5.), 2.);
        c.update((15., 25.));
        c.end();
        assert_eq!(c.pixels(), (5, 5, 10, 20));
        c.nudge(-100., 100.);
        assert_eq!(c.pixels(), (0, 60, 10, 20));
    }

    #[test]
    fn narrow_canvas_rounding_and_boundary_drags_never_exceed_the_canvas() {
        for width in [1, 3, 7, 13, 37, 79, 121, 719, 29999] {
            for height in [1, 3, 17, 59, 101, 719, 3333] {
                for preset in 0..PRESETS.len() {
                    let mut c = CropFrame::new(width, height, None);
                    c.set_preset(preset);
                    for _ in 0..2 {
                        let r = c.rect;
                        c.begin((r.x, r.y), 4.);
                        c.update((-100., -100.));
                        c.end();
                        c.nudge(100., 100.);
                        let (x, y, w, h) = c.pixels();
                        assert!(
                            w > 0
                                && h > 0
                                && x >= 0
                                && y >= 0
                                && x as u32 + w <= width
                                && y as u32 + h <= height
                        );
                        assert!(c.rect.width <= width as f32 && c.rect.height <= height as f32);
                        c.swap();
                    }
                }
            }
        }
    }
}
