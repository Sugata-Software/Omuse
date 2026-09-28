//! Deterministic brush dynamics. Inputs describe measured pressure and tilt only;
//! no device type or missing stylus capability is inferred.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrayTip {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum Tip {
    Round,
    Custom(GrayTip),
}
impl Default for Tip {
    fn default() -> Self {
        Self::Round
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub size: f32,
    pub flow: f32,
    pub spacing: f32,
    pub hardness: f32,
    pub scatter: f32,
    pub angle: f32,
    pub angle_jitter: f32,
    pub texture_strength: f32,
    pub tip: Tip,
    pub pressure_curve: Vec<CurvePoint>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            size: 16.,
            flow: 1.,
            spacing: 0.12,
            hardness: 0.85,
            scatter: 0.,
            angle: 0.,
            angle_jitter: 0.,
            texture_strength: 0.,
            tip: Tip::Round,
            pressure_curve: vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 1., y: 1. }],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub angle: f32,
    pub tilt: f32,
    pub texture: f32,
}
pub struct StrokeGenerator {
    settings: Settings,
    seed: u64,
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        for (v, name, range) in [
            (self.size, "size", (0.01, 4096.)),
            (self.flow, "flow", (0., 1.)),
            (self.spacing, "spacing", (0.01, 10.)),
            (self.hardness, "hardness", (0., 1.)),
            (self.scatter, "scatter", (0., 10.)),
            (self.angle_jitter, "angle jitter", (0., 360.)),
            (self.texture_strength, "texture strength", (0., 1.)),
        ] {
            ensure!(
                v.is_finite() && v >= range.0 && v <= range.1,
                "invalid brush {name}"
            )
        }
        ensure!(
            self.angle.is_finite() && self.angle.abs() <= 360_000.,
            "invalid brush angle"
        );
        ensure!(
            self.pressure_curve.len() >= 2 && self.pressure_curve.len() <= 256,
            "invalid pressure curve length"
        );
        let mut last = -1.;
        let mut last_y = -1.;
        for p in &self.pressure_curve {
            ensure!(
                p.x.is_finite()
                    && p.y.is_finite()
                    && (0.0..=1.).contains(&p.x)
                    && (0.0..=1.).contains(&p.y)
                    && p.x > last
                    && p.y >= last_y,
                "pressure curve must increase monotonically"
            );
            last = p.x;
            last_y = p.y
        }
        ensure!(
            self.pressure_curve[0].x == 0. && self.pressure_curve.last().unwrap().x == 1.,
            "pressure curve must span zero to one"
        );
        if let Tip::Custom(t) = &self.tip {
            ensure!(
                t.width > 0
                    && t.height > 0
                    && t.width <= 4096
                    && t.height <= 4096
                    && u64::from(t.width) * u64::from(t.height) <= 16_777_216
                    && t.pixels.len() == t.width as usize * t.height as usize,
                "invalid custom brush tip"
            )
        }
        Ok(())
    }
}
impl StrokeGenerator {
    pub fn new(settings: Settings, seed: u64) -> Result<Self> {
        settings.validate()?;
        Ok(Self { settings, seed })
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn generate(
        &self,
        points: &[InputPoint],
        mut cancel: impl FnMut() -> bool,
    ) -> Result<Vec<Dab>> {
        ensure!(points.len() <= 1_000_000, "too many brush inputs");
        for p in points {
            ensure!(
                [p.x, p.y, p.pressure, p.tilt_x, p.tilt_y]
                    .iter()
                    .all(|v| v.is_finite()),
                "non-finite brush input"
            );
            ensure!(
                (0.0..=1.).contains(&p.pressure)
                    && (-1.0..=1.).contains(&p.tilt_x)
                    && (-1.0..=1.).contains(&p.tilt_y),
                "brush input outside range"
            )
        }
        if points.is_empty() {
            return Ok(vec![]);
        }
        let step = (self.settings.size * self.settings.spacing).max(0.01);
        let mut samples = Vec::new();
        let mut carry = 0.;
        let emit = |p: InputPoint, samples: &mut Vec<InputPoint>| {
            if p.pressure > 0. {
                samples.push(p)
            }
        };
        emit(points[0], &mut samples);
        for pair in points.windows(2) {
            if cancel() {
                anyhow::bail!("cancelled")
            }
            let a = pair[0];
            let b = pair[1];
            let d = (b.x - a.x).hypot(b.y - a.y);
            if d <= 1e-8 {
                continue;
            }
            let mut at = step - carry;
            while at <= d {
                let t = at / d;
                emit(interpolate(a, b, t), &mut samples);
                ensure!(samples.len() <= 2_000_000, "too many brush dabs");
                at += step
            }
            carry = (d - (at - step)).rem_euclid(step)
        }
        if points.len() > 1
            && samples.last().is_none_or(|s| {
                (s.x - points.last().unwrap().x).hypot(s.y - points.last().unwrap().y) > step * 0.25
            })
        {
            emit(*points.last().unwrap(), &mut samples)
        }
        Ok(samples
            .into_iter()
            .enumerate()
            .map(|(i, p)| self.dab(p, i as u64))
            .collect())
    }
    fn dab(&self, p: InputPoint, index: u64) -> Dab {
        let pressure = curve(&self.settings.pressure_curve, p.pressure);
        let r1 = unit(hash(self.seed ^ index.wrapping_mul(2)));
        let r2 = unit(hash(self.seed ^ index.wrapping_mul(2).wrapping_add(1)));
        let scatter = self.settings.scatter * self.settings.size * r1.sqrt();
        let theta = r2 * std::f32::consts::TAU;
        let tilt = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt().min(1.);
        let tilt_angle = if tilt > 1e-6 {
            p.tilt_y.atan2(p.tilt_x).to_degrees()
        } else {
            0.
        };
        let jitter =
            (unit(hash(self.seed ^ index ^ 0xa5a5_5a5a)) - 0.5) * 2. * self.settings.angle_jitter;
        Dab {
            x: p.x + theta.cos() * scatter,
            y: p.y + theta.sin() * scatter,
            size: self.settings.size * pressure,
            opacity: self.settings.flow * pressure,
            hardness: self.settings.hardness,
            angle: (self.settings.angle + tilt_angle + jitter).rem_euclid(360.),
            tilt,
            texture: self.settings.texture_strength,
        }
    }
    /// Sample a dab at normalized local coordinates (-1.0.1), after its angle.
    pub fn sample_tip(&self, dab: &Dab, x: f32, y: f32) -> f32 {
        if dab.opacity <= 0. || dab.size <= 0. {
            return 0.;
        }
        let r = dab.angle.to_radians();
        let (u, v) = (x * r.cos() + y * r.sin(), -x * r.sin() + y * r.cos());
        let shape = match &self.settings.tip {
            Tip::Round => {
                let d = u.hypot(v);
                if d > 1. {
                    0.
                } else if d <= dab.hardness {
                    1.
                } else {
                    1. - (d - dab.hardness) / (1. - dab.hardness).max(0.001)
                }
            }
            Tip::Custom(t) => sample_custom(t, u, v),
        };
        shape * dab.opacity * (1. - dab.texture + dab.texture * texture(self.seed, u, v))
    }
}
fn interpolate(a: InputPoint, b: InputPoint, t: f32) -> InputPoint {
    InputPoint {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        pressure: a.pressure + (b.pressure - a.pressure) * t,
        tilt_x: a.tilt_x + (b.tilt_x - a.tilt_x) * t,
        tilt_y: a.tilt_y + (b.tilt_y - a.tilt_y) * t,
    }
}
fn curve(c: &[CurvePoint], x: f32) -> f32 {
    let pair = c
        .windows(2)
        .find(|p| x <= p[1].x)
        .unwrap_or(&c[c.len() - 2..]);
    let t = (x - pair[0].x) / (pair[1].x - pair[0].x);
    pair[0].y + (pair[1].y - pair[0].y) * t
}
fn hash(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn unit(x: u64) -> f32 {
    ((x >> 40) as f32) / 16_777_216.
}
fn texture(seed: u64, x: f32, y: f32) -> f32 {
    unit(hash(
        seed ^ (x.to_bits() as u64) ^ ((y.to_bits() as u64) << 32),
    ))
}
fn sample_custom(t: &GrayTip, u: f32, v: f32) -> f32 {
    if u.abs() > 1. || v.abs() > 1. {
        return 0.;
    }
    let x = ((u * 0.5 + 0.5) * (t.width - 1) as f32).round() as u32;
    let y = ((v * 0.5 + 0.5) * (t.height - 1) as f32).round() as u32;
    f32::from(t.pixels[y as usize * t.width as usize + x as usize]) / 255.
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f32, pressure: f32) -> InputPoint {
        InputPoint {
            x,
            y: 0.,
            pressure,
            tilt_x: 0.,
            tilt_y: 0.,
        }
    }
    #[test]
    fn deterministic_spacing_pressure_flow_and_zero_pressure() {
        let mut s = Settings::default();
        s.size = 10.;
        s.spacing = 0.5;
        s.flow = 0.5;
        let g = StrokeGenerator::new(s, 7).unwrap();
        let a = g.generate(&[p(0., 0.), p(20., 1.)], || false).unwrap();
        let b = g.generate(&[p(0., 0.), p(20., 1.)], || false).unwrap();
        assert_eq!(a, b);
        assert!(a.iter().all(|d| d.opacity > 0. && d.opacity <= 0.5));
        assert!((a[0].x - 5.).abs() < 1e-5);
        assert_eq!(a.last().unwrap().x, 20.)
    }
    #[test]
    fn tilt_rotates_tip_without_fabricating_device() {
        let s = Settings::default();
        let g = StrokeGenerator::new(s, 1).unwrap();
        let d = g
            .generate(
                &[InputPoint {
                    x: 1.,
                    y: 2.,
                    pressure: 1.,
                    tilt_x: 0.,
                    tilt_y: 1.,
                }],
                || false,
            )
            .unwrap()[0];
        assert_eq!(d.tilt, 1.);
        assert!((d.angle - 90.).abs() < 1e-4)
    }
    #[test]
    fn custom_tip_and_monotonic_curve_validation() {
        let mut s = Settings::default();
        s.tip = Tip::Custom(GrayTip {
            width: 2,
            height: 2,
            pixels: vec![0, 255, 255, 0],
        });
        let g = StrokeGenerator::new(s.clone(), 0).unwrap();
        let d = g.generate(&[p(0., 1.)], || false).unwrap()[0];
        assert_eq!(g.sample_tip(&d, -1., -1.), 0.);
        assert!(g.sample_tip(&d, 1., -1.) > 0.9);
        s.pressure_curve = vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 0., y: 1. }];
        assert!(StrokeGenerator::new(s, 0).is_err())
    }
}
