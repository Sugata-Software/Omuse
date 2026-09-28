#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientSettings {
    pub kind: GradientKind,
    pub transparent: bool,
    pub reverse: bool,
    pub opacity: f32,
}
impl Default for GradientSettings {
    fn default() -> Self {
        Self {
            kind: GradientKind::Linear,
            transparent: false,
            reverse: false,
            opacity: 1.,
        }
    }
}

pub fn sample(
    position: (f32, f32),
    start: (f32, f32),
    end: (f32, f32),
    from: [u8; 4],
    to: [u8; 4],
    settings: &GradientSettings,
) -> [u8; 4] {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length2 = dx * dx + dy * dy;
    let mut t = if !length2.is_finite() || length2 <= f32::EPSILON {
        0.
    } else {
        match settings.kind {
            GradientKind::Linear => {
                ((position.0 - start.0) * dx + (position.1 - start.1) * dy) / length2
            }
            GradientKind::Radial => {
                ((position.0 - start.0).hypot(position.1 - start.1)) / length2.sqrt()
            }
        }
        .clamp(0., 1.)
    };
    if settings.reverse {
        t = 1. - t;
    }
    let to = if settings.transparent {
        [from[0], from[1], from[2], 0]
    } else {
        to
    };
    let a0 = f32::from(from[3]) / 255.;
    let a1 = f32::from(to[3]) / 255.;
    let a = a0 + (a1 - a0) * t;
    let mut out = [0; 4];
    if a > 0. {
        for c in 0..3 {
            let p0 = f32::from(from[c]) * a0;
            let p1 = f32::from(to[c]) * a1;
            out[c] = ((p0 + (p1 - p0) * t) / a).clamp(0., 255.).round() as u8;
        }
    }
    out[3] = (a * settings.opacity.clamp(0., 1.) * 255.).round() as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linear_radial_reverse_and_opacity() {
        let s = GradientSettings::default();
        assert_eq!(
            sample(
                (5., 0.),
                (0., 0.),
                (10., 0.),
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                &s
            ),
            [128, 0, 128, 255]
        );
        let s = GradientSettings {
            kind: GradientKind::Radial,
            reverse: true,
            opacity: 0.5,
            ..Default::default()
        };
        assert_eq!(
            sample(
                (0., 0.),
                (0., 0.),
                (10., 0.),
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                &s
            ),
            [0, 0, 255, 128]
        );
    }
    #[test]
    fn transparent_interpolation_keeps_foreground_color() {
        let s = GradientSettings {
            transparent: true,
            ..Default::default()
        };
        assert_eq!(
            sample(
                (5., 0.),
                (0., 0.),
                (10., 0.),
                [20, 40, 60, 255],
                [200, 0, 0, 255],
                &s
            ),
            [20, 40, 60, 128]
        );
    }
}
