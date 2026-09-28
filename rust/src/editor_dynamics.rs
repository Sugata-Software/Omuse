//! Incremental brush-dynamics state for `Editor` strokes.
//!
//! The public `brush_dynamics::StrokeGenerator` is intentionally batch
//! oriented. This adapter keeps spacing carry and dab indices between pointer
//! events, so editor motion does not regenerate the complete stroke history.

use anyhow::{Result, ensure};

use crate::brush_dynamics::{CurvePoint, Dab, InputPoint, Settings, Tip};

pub struct StrokeState {
    settings: Settings,
    seed: u64,
    last: InputPoint,
    carry: f32,
    dab_index: u64,
    emitted: bool,
}

impl StrokeState {
    pub fn new(settings: Settings, seed: u64, first: InputPoint) -> Result<Self> {
        settings.validate()?;
        validate_input(first)?;
        Ok(Self {
            settings,
            seed,
            last: first,
            carry: 0.0,
            dab_index: 0,
            emitted: false,
        })
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn start(&mut self) -> Vec<Dab> {
        if self.last.pressure <= 0.0 {
            return Vec::new();
        }
        self.emitted = true;
        vec![self.dab(self.last)]
    }

    pub fn append(&mut self, next: InputPoint) -> Result<Vec<Dab>> {
        validate_input(next)?;
        let previous = self.last;
        self.last = next;
        let distance = (next.x - previous.x).hypot(next.y - previous.y);
        if distance <= f32::EPSILON {
            return Ok(Vec::new());
        }
        let step = (self.settings.size * self.settings.spacing).max(0.01);
        let mut at = (step - self.carry).max(0.0);
        let mut dabs = Vec::new();
        while at <= distance {
            let t = at / distance;
            let point = interpolate(previous, next, t);
            if point.pressure > 0.0 {
                dabs.push(self.dab(point));
                ensure!(dabs.len() <= 2_000_000, "too many brush dabs");
                self.emitted = true;
            }
            at += step;
        }
        self.carry = (distance - (at - step)).rem_euclid(step);
        Ok(dabs)
    }

    pub fn finish(&mut self) -> Vec<Dab> {
        if !self.emitted || self.last.pressure <= 0.0 {
            return Vec::new();
        }
        let step = (self.settings.size * self.settings.spacing).max(0.01);
        if self.carry <= step * 0.25 {
            return Vec::new();
        }
        self.carry = 0.0;
        vec![self.dab(self.last)]
    }

    fn dab(&mut self, point: InputPoint) -> Dab {
        let pressure = curve(&self.settings.pressure_curve, point.pressure);
        let random_a = unit(hash(self.seed ^ self.dab_index.wrapping_mul(2)));
        let random_b = unit(hash(
            self.seed ^ self.dab_index.wrapping_mul(2).wrapping_add(1),
        ));
        let scatter = self.settings.scatter * self.settings.size * random_a.sqrt();
        let theta = random_b * std::f32::consts::TAU;
        let tilt = (point.tilt_x * point.tilt_x + point.tilt_y * point.tilt_y)
            .sqrt()
            .min(1.0);
        let tilt_angle = if tilt > 1e-6 {
            point.tilt_y.atan2(point.tilt_x).to_degrees()
        } else {
            0.0
        };
        let jitter = (unit(hash(self.seed ^ self.dab_index ^ 0xa5a5_5a5a)) - 0.5)
            * 2.0
            * self.settings.angle_jitter;
        let dab = Dab {
            x: point.x + theta.cos() * scatter,
            y: point.y + theta.sin() * scatter,
            size: self.settings.size * pressure,
            opacity: self.settings.flow * pressure,
            hardness: self.settings.hardness,
            angle: (self.settings.angle + tilt_angle + jitter).rem_euclid(360.0),
            tilt,
            texture: self.settings.texture_strength,
        };
        self.dab_index = self.dab_index.wrapping_add(1);
        dab
    }
}

pub fn seed_for_layer(layer_id: &str) -> u64 {
    layer_id.bytes().fold(0x9e37_79b9_7f4a_7c15, |state, byte| {
        hash(state ^ u64::from(byte))
    })
}

/// Coverage at normalized dab-local coordinates, including angle, custom tip,
/// and deterministic texture. `x` and `y` are in the range `-1..=1`.
pub fn sample_dab(settings: &Settings, dab: &Dab, x: f32, y: f32) -> f32 {
    if dab.opacity <= 0.0 || dab.size <= 0.0 {
        return 0.0;
    }
    let angle = dab.angle.to_radians();
    let (u, v) = (
        x * angle.cos() + y * angle.sin(),
        -x * angle.sin() + y * angle.cos(),
    );
    let shape = match &settings.tip {
        Tip::Round => {
            let distance = u.hypot(v);
            if distance > 1.0 {
                0.0
            } else if distance <= dab.hardness {
                1.0
            } else {
                1.0 - (distance - dab.hardness) / (1.0 - dab.hardness).max(0.001)
            }
        }
        Tip::Custom(tip) => {
            if u.abs() > 1.0 || v.abs() > 1.0 {
                0.0
            } else {
                let tx = ((u * 0.5 + 0.5) * (tip.width - 1) as f32).round() as u32;
                let ty = ((v * 0.5 + 0.5) * (tip.height - 1) as f32).round() as u32;
                f32::from(tip.pixels[ty as usize * tip.width as usize + tx as usize]) / 255.0
            }
        }
    };
    shape * dab.opacity * (1.0 - dab.texture + dab.texture * texture(settings, x, y))
}

fn validate_input(point: InputPoint) -> Result<()> {
    ensure!(
        [point.x, point.y, point.pressure, point.tilt_x, point.tilt_y]
            .iter()
            .all(|value| value.is_finite()),
        "non-finite stylus input"
    );
    ensure!(
        (0.0..=1.0).contains(&point.pressure)
            && (-1.0..=1.0).contains(&point.tilt_x)
            && (-1.0..=1.0).contains(&point.tilt_y),
        "stylus input outside supported range"
    );
    Ok(())
}

fn interpolate(a: InputPoint, b: InputPoint, amount: f32) -> InputPoint {
    InputPoint {
        x: a.x + (b.x - a.x) * amount,
        y: a.y + (b.y - a.y) * amount,
        pressure: a.pressure + (b.pressure - a.pressure) * amount,
        tilt_x: a.tilt_x + (b.tilt_x - a.tilt_x) * amount,
        tilt_y: a.tilt_y + (b.tilt_y - a.tilt_y) * amount,
    }
}

fn curve(points: &[CurvePoint], value: f32) -> f32 {
    let pair = points
        .windows(2)
        .find(|window| value <= window[1].x)
        .unwrap_or(&points[points.len() - 2..]);
    let amount = (value - pair[0].x) / (pair[1].x - pair[0].x);
    pair[0].y + (pair[1].y - pair[0].y) * amount
}

fn texture(_settings: &Settings, x: f32, y: f32) -> f32 {
    unit(hash(
        0x517c_c1b7_2722_0a95 ^ x.to_bits() as u64 ^ ((y.to_bits() as u64) << 32),
    ))
}

fn hash(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit(value: u64) -> f32 {
    (value >> 40) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Editor, PaintTool};
    use crate::model::Document;
    use image::RgbaImage;

    fn point(x: f32, pressure: f32) -> InputPoint {
        InputPoint {
            x,
            y: 0.0,
            pressure,
            tilt_x: 0.0,
            tilt_y: 0.0,
        }
    }

    #[test]
    fn incremental_spacing_preserves_carry() {
        let mut settings = Settings::default();
        settings.size = 10.0;
        settings.spacing = 0.5;
        let mut state = StrokeState::new(settings, 7, point(0.0, 1.0)).unwrap();
        assert_eq!(state.start().len(), 1);
        assert_eq!(state.append(point(2.0, 1.0)).unwrap().len(), 0);
        assert_eq!(state.append(point(6.0, 1.0)).unwrap().len(), 1);
    }

    #[test]
    fn custom_tip_and_tilt_are_used() {
        let mut settings = Settings::default();
        settings.tip = Tip::Custom(crate::brush_dynamics::GrayTip {
            width: 2,
            height: 2,
            pixels: vec![0, 255, 255, 0],
        });
        let mut state = StrokeState::new(
            settings.clone(),
            1,
            InputPoint {
                x: 1.0,
                y: 2.0,
                pressure: 1.0,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
        )
        .unwrap();
        let dab = state.start().pop().unwrap();
        assert!(dab.angle.abs() < 1e-4);
        assert_eq!(sample_dab(&settings, &dab, -1.0, -1.0), 0.0);
        assert!(sample_dab(&settings, &dab, 1.0, -1.0) > 0.9);
        let mut tilt_state = StrokeState::new(
            Settings::default(),
            1,
            InputPoint {
                x: 1.0,
                y: 2.0,
                pressure: 1.0,
                tilt_x: 0.0,
                tilt_y: 1.0,
            },
        )
        .unwrap();
        assert!((tilt_state.start().pop().unwrap().angle - 90.0).abs() < 1e-4);
    }

    fn active_image(editor: &Editor) -> RgbaImage {
        editor
            .document
            .find_layer(&editor.active_layer)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .to_image()
    }

    #[test]
    fn real_editor_dynamics_paints_and_undo_restores_source() {
        let mut editor = Editor::new(Document::new(32, 16));
        editor.brush.color = [220, 30, 10, 255];
        editor.brush_dynamics = Some(Settings {
            size: 6.0,
            flow: 0.5,
            spacing: 0.5,
            hardness: 1.0,
            ..Settings::default()
        });
        let original = active_image(&editor);
        assert!(editor.begin_stylus_stroke(
            InputPoint {
                x: 5.0,
                y: 8.0,
                pressure: 1.0,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
            PaintTool::Brush,
        ));
        assert!(editor.continue_stylus_stroke(InputPoint {
            x: 27.0,
            y: 8.0,
            pressure: 1.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
        }));
        assert!(editor.finish_stroke());
        let painted = active_image(&editor);
        assert_ne!(painted, original);
        assert!(painted.pixels().any(|pixel| pixel[3] > 0 && pixel[3] < 255));
        assert!(editor.undo());
        assert_eq!(active_image(&editor), original);
    }

    #[test]
    fn invalid_dynamics_rejects_before_editor_mutation() {
        let mut editor = Editor::new(Document::new(8, 8));
        let original = active_image(&editor);
        let mut settings = Settings::default();
        settings.spacing = 0.0;
        editor.brush_dynamics = Some(settings);
        assert!(!editor.begin_stroke(4.0, 4.0, 1.0, PaintTool::Brush));
        assert_eq!(editor.undo_depth(), 0);
        assert_eq!(active_image(&editor), original);
    }

    #[test]
    fn zero_pressure_stylus_stroke_is_transparent_and_tilt_is_accepted() {
        let mut editor = Editor::new(Document::new(8, 8));
        editor.brush.color = [20, 120, 240, 255];
        editor.brush_dynamics = Some(Settings::default());
        let original = active_image(&editor);
        assert!(editor.begin_stylus_stroke(
            InputPoint {
                x: 4.0,
                y: 4.0,
                pressure: 0.0,
                tilt_x: 0.3,
                tilt_y: -0.4,
            },
            PaintTool::Brush,
        ));
        assert!(editor.continue_stylus_stroke(InputPoint {
            x: 5.0,
            y: 4.0,
            pressure: 0.0,
            tilt_x: 0.3,
            tilt_y: -0.4,
        }));
        assert!(!editor.finish_stroke());
        assert_eq!(active_image(&editor), original);
    }

    #[test]
    fn heal_on_transparent_layer_uses_composited_destination_tone_and_undoes() {
        let mut editor = Editor::new(Document::new(24, 16));
        let background_id = editor.active_layer.clone();
        {
            let background = editor
                .document
                .find_layer_mut(&background_id)
                .unwrap()
                .image
                .as_mut()
                .unwrap();
            for pixel in background.pixels_mut() {
                *pixel = image::Rgba([100, 100, 100, 255]);
            }
            for y in 4..13 {
                for x in 1..10 {
                    background.put_pixel(x, y, image::Rgba([210, 20, 20, 255]));
                }
                for x in 14..23 {
                    background.put_pixel(x, y, image::Rgba([20, 20, 180, 255]));
                }
            }
        }
        let heal_layer = editor.add_layer("Healing");
        let transparent = active_image(&editor);
        editor.brush.size = 6.0;
        editor.brush.hardness = 1.0;
        editor.brush.opacity = 1.0;
        assert!(editor.begin_clone_stroke_from_canvas((5.0, 8.0), (18.0, 8.0), true));
        assert!(editor.finish_clone_stroke());
        let healed = active_image(&editor);
        assert_eq!(healed.get_pixel(18, 8).0, [20, 20, 180, 255]);
        assert_eq!(
            editor
                .document
                .find_layer(&background_id)
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .get_pixel(18, 8)
                .0,
            [20, 20, 180, 255]
        );
        assert!(editor.undo());
        assert_eq!(editor.active_layer, heal_layer);
        assert_eq!(active_image(&editor), transparent);
    }
}
