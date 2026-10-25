//! Editable paints and stroke geometry for version 3 vector scenes.
use crate::vector_path::{FillRule, Point, VectorPath};
use anyhow::{Context, Result, ensure};
use resvg::tiny_skia as sk;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    pub offset: f32,
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GradientKind {
    Linear {
        start: Point,
        end: Point,
    },
    Radial {
        center: Point,
        focus: Point,
        radius: f32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientSpread {
    #[default]
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientMethod {
    #[default]
    Classic,
    Perceptual,
}
impl GradientMethod {
    fn is_classic(&self) -> bool {
        *self == Self::Classic
    }
}
fn is_false(value: &bool) -> bool {
    !*value
}

/// Coordinates and the affine transform are local to the containing object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientFill {
    pub kind: GradientKind,
    pub stops: Vec<GradientStop>,
    pub spread: GradientSpread,
    pub transform: [f32; 6],
    #[serde(default, skip_serializing_if = "GradientMethod::is_classic")]
    pub method: GradientMethod,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dither: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeCap {
    Butt,
    #[default]
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeJoin {
    Miter,
    #[default]
    Round,
    Bevel,
}

/// Absent options preserve the legacy round, solid stroke rasterizer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeOptions {
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
    pub dashes: Vec<f32>,
    pub dash_offset: f32,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            miter_limit: 4.,
            dashes: Vec::new(),
            dash_offset: 0.,
        }
    }
}

pub fn transform(value: [f32; 6]) -> sk::Transform {
    sk::Transform::from_row(value[0], value[1], value[2], value[3], value[4], value[5])
}

pub fn matrix(t: sk::Transform) -> [f32; 6] {
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]
}

impl GradientFill {
    pub fn requires_extended_format(&self) -> bool {
        self.method != GradientMethod::Classic || self.dither
    }

    /// Bounded classic stops approximating the perceptual curve. The editable
    /// master retains its original stops; exchange uses these appearance stops.
    pub fn expanded_stops(&self) -> Result<Vec<(f32, [f32; 4])>> {
        self.validate()?;
        let rgba = |s: GradientStop| s.color.map(|v| f32::from(v) / 255.);
        if self.method == GradientMethod::Classic {
            return Ok(self.stops.iter().map(|s| (s.offset, rgba(*s))).collect());
        }
        let mut out = Vec::new();
        fn expand(
            a: GradientStop,
            b: GradientStop,
            lo: f32,
            hi: f32,
            depth: u8,
            out: &mut Vec<(f32, [f32; 4])>,
        ) {
            let left = perceptual(a.color, b.color, lo);
            let right = perceptual(a.color, b.color, hi);
            let error = [0.25, 0.5, 0.75]
                .into_iter()
                .map(|u| {
                    let expected = perceptual(a.color, b.color, lo + (hi - lo) * u);
                    (0..3)
                        .map(|c| {
                            // tiny-skia (and SVG) interpolates straight RGBA,
                            // then premultiplies. Measure that actual shader,
                            // rather than an associated-alpha interpolation.
                            let alpha = left[3] * (1. - u) + right[3] * u;
                            let approx = (left[c] * (1. - u) + right[c] * u) * alpha;
                            (approx - expected[c] * expected[3]).abs()
                        })
                        .fold(0f32, f32::max)
                })
                .fold(0f32, f32::max);
            if depth < 8 && error > 0.35 / 255. {
                let mid = (lo + hi) * 0.5;
                expand(a, b, lo, mid, depth + 1, out);
                expand(a, b, mid, hi, depth + 1, out);
            } else {
                out.push((a.offset + (b.offset - a.offset) * hi, right));
            }
        }
        for pair in self.stops.windows(2) {
            let left = if pair[0].offset == pair[1].offset {
                rgba(pair[0])
            } else {
                perceptual(pair[0].color, pair[1].color, 0.)
            };
            // A transparent middle stop may approach different visible colours
            // on either side. Two coincident zero-alpha appearance stops retain
            // those limits without changing the editable source stop.
            if out
                .last()
                .is_none_or(|last| *last != (pair[0].offset, left))
            {
                out.push((pair[0].offset, left));
            }
            if pair[0].offset == pair[1].offset {
                out.push((pair[1].offset, rgba(pair[1])));
            } else {
                expand(pair[0], pair[1], 0., 1., 0, &mut out);
            }
        }
        ensure!(
            out.len() <= 4096,
            "Expanded perceptual gradient exceeds stop budget"
        );
        Ok(out)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (2..=16).contains(&self.stops.len()),
            "A gradient needs 2–16 stops"
        );
        ensure!(
            self.stops
                .iter()
                .all(|s| s.offset.is_finite() && (0. ..=1.).contains(&s.offset))
                && self.stops.windows(2).all(|s| s[0].offset <= s[1].offset),
            "Gradient stops must be ordered from 0 to 100%"
        );
        ensure!(
            self.transform
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
                && transform(self.transform).invert().is_some(),
            "Gradient transform is invalid"
        );
        let valid = |p: Point| {
            p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1_000_000. && p.y.abs() <= 1_000_000.
        };
        match self.kind {
            GradientKind::Linear { start, end } => ensure!(
                valid(start) && valid(end) && (start.x - end.x).hypot(start.y - end.y) >= 0.001,
                "Gradient endpoints must be different"
            ),
            GradientKind::Radial {
                center,
                focus,
                radius,
            } => ensure!(
                valid(center)
                    && valid(focus)
                    && radius.is_finite()
                    && (0.001..=1_000_000.).contains(&radius)
                    && (focus.x - center.x).hypot(focus.y - center.y) < radius,
                "Radial gradient focus must be inside its positive radius"
            ),
        }
        Ok(())
    }

    pub fn bake_transform(&mut self, value: [f32; 6]) {
        self.transform = matrix(transform(value).pre_concat(transform(self.transform)));
    }

    pub fn shader(&self, object_to_tile: sk::Transform) -> Result<sk::Shader<'static>> {
        self.validate()?;
        let stops = if self.method == GradientMethod::Classic {
            self.stops
                .iter()
                .map(|s| {
                    sk::GradientStop::new(
                        s.offset,
                        sk::Color::from_rgba8(s.color[0], s.color[1], s.color[2], s.color[3]),
                    )
                })
                .collect()
        } else {
            self.expanded_stops()?
                .into_iter()
                .map(|(offset, color)| {
                    sk::GradientStop::new(
                        offset,
                        sk::Color::from_rgba(color[0], color[1], color[2], color[3]).unwrap(),
                    )
                })
                .collect()
        };
        let spread = match self.spread {
            GradientSpread::Pad => sk::SpreadMode::Pad,
            GradientSpread::Repeat => sk::SpreadMode::Repeat,
            GradientSpread::Reflect => sk::SpreadMode::Reflect,
        };
        let t = object_to_tile.pre_concat(transform(self.transform));
        match self.kind {
            GradientKind::Linear { start, end } => sk::LinearGradient::new(
                sk::Point::from_xy(start.x, start.y),
                sk::Point::from_xy(end.x, end.y),
                stops,
                spread,
                t,
            ),
            GradientKind::Radial {
                center,
                focus,
                radius,
            } => sk::RadialGradient::new(
                sk::Point::from_xy(focus.x, focus.y),
                sk::Point::from_xy(center.x, center.y),
                radius,
                stops,
                spread,
                t,
            ),
        }
        .context("Cannot render gradient transform")
    }
}

// Independently expressed OKLab conversion on encoded-sRGB stop colours.
// Interpolate associated alpha weights so invisible stop RGB cannot fringe.
fn perceptual(a: [u8; 4], b: [u8; 4], t: f32) -> [f32; 4] {
    let alpha = ((f32::from(a[3]) * (1. - t) + f32::from(b[3]) * t) / 255.).clamp(0., 1.);
    if alpha == 0. {
        // Preserve the adjacent visible colour's limit. Arbitrary hidden RGB
        // in a transparent endpoint would fringe under a straight-RGBA shader.
        let visible = if a[3] > 0 {
            a
        } else if b[3] > 0 {
            b
        } else {
            [0; 4]
        };
        return [
            f32::from(visible[0]) / 255.,
            f32::from(visible[1]) / 255.,
            f32::from(visible[2]) / 255.,
            0.,
        ];
    }
    let weight = (t * f32::from(b[3]) / (alpha * 255.)).clamp(0., 1.);
    let lab = |c: [u8; 4]| {
        let v = c.map(|v| f32::from(v) / 255.);
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (linear(v[0]), linear(v[1]), linear(v[2]));
        let l = (0.41222147 * r + 0.53633255 * g + 0.05144599 * b).cbrt();
        let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
        let s = (0.08830246 * r + 0.28171885 * g + 0.6299787 * b).cbrt();
        [
            0.21045426 * l + 0.7936178 * m - 0.004072047 * s,
            1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
            0.025904037 * l + 0.78277177 * m - 0.80867577 * s,
        ]
    };
    let x = lab(a);
    let y = lab(b);
    let p = std::array::from_fn::<_, 3, _>(|c| x[c] + (y[c] - x[c]) * weight);
    let l = (p[0] + 0.39633778 * p[1] + 0.21580376 * p[2]).powi(3);
    let m = (p[0] - 0.105561346 * p[1] - 0.06385417 * p[2]).powi(3);
    let s = (p[0] - 0.08948418 * p[1] - 1.2914855 * p[2]).powi(3);
    let transfer = |v: f32| {
        let v = v.clamp(0., 1.);
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    };
    [
        transfer(4.0767417 * l - 3.3077116 * m + 0.23096994 * s),
        transfer(-1.268438 * l + 2.6097574 * m - 0.3413194 * s),
        transfer(-0.0041960863 * l - 0.7034186 * m + 1.7076147 * s),
        alpha,
    ]
}

fn dither_fill(
    pixmap: &mut sk::Pixmap,
    object_to_tile: sk::Transform,
    gradient: &GradientFill,
) -> Result<()> {
    let inverse = object_to_tile
        .invert()
        .context("Cannot anchor gradient dither")?;
    let gradient_inverse = object_to_tile
        .pre_concat(transform(gradient.transform))
        .invert()
        .context("Cannot sample dithered gradient")?;
    let stops = gradient.expanded_stops()?;
    let width = pixmap.width() as usize;
    for (i, pixel) in pixmap.data_mut().chunks_exact_mut(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let mut point = sk::Point::from_xy((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
        let mut position = point;
        gradient_inverse.map_point(&mut position);
        ensure!(
            position.x.is_finite() && position.y.is_finite(),
            "Dithered gradient coordinates exceed rendering limits"
        );
        let t = match gradient.kind {
            GradientKind::Linear { start, end } => {
                let dx = f64::from(end.x) - f64::from(start.x);
                let dy = f64::from(end.y) - f64::from(start.y);
                ((f64::from(position.x) - f64::from(start.x)) * dx
                    + (f64::from(position.y) - f64::from(start.y)) * dy)
                    / (dx * dx + dy * dy)
            }
            GradientKind::Radial {
                center,
                focus,
                radius,
            } => {
                let dx = f64::from(center.x) - f64::from(focus.x);
                let dy = f64::from(center.y) - f64::from(focus.y);
                let qx = f64::from(position.x) - f64::from(focus.x);
                let qy = f64::from(position.y) - f64::from(focus.y);
                let a = dx * dx + dy * dy - f64::from(radius).powi(2);
                let b = -2. * (qx * dx + qy * dy);
                let c = qx * qx + qy * qy;
                (-b - (b * b - 4. * a * c).max(0.).sqrt()) / (2. * a)
            }
        };
        let t = match gradient.spread {
            GradientSpread::Pad => t.clamp(0., 1.),
            GradientSpread::Repeat => t.rem_euclid(1.),
            GradientSpread::Reflect => {
                let n = t.rem_euclid(2.);
                if n > 1. { 2. - n } else { n }
            }
        } as f32;
        let colour = ramp_sample(&stops, t);
        inverse.map_point(&mut point);
        let seed = (point.x.floor() as i64 as u64).wrapping_mul(0x9e3779b97f4a7c15)
            ^ (point.y.floor() as i64 as u64).wrapping_mul(0xbf58476d1ce4e5b9);
        for c in 0..3 {
            let mut hash = seed ^ (c as u64 + 1).wrapping_mul(0x94d049bb133111eb);
            hash ^= hash >> 30;
            hash = hash.wrapping_mul(0xbf58476d1ce4e5b9);
            hash ^= hash >> 27;
            hash = hash.wrapping_mul(0x94d049bb133111eb);
            hash ^= hash >> 31;
            let noise = ((hash & 65535) as f32 / 65535. - 0.5) * f32::from(pixel[3]) / 255.;
            pixel[c] = (colour[c] * f32::from(pixel[3]) + noise)
                .round()
                .clamp(0., f32::from(pixel[3])) as u8;
        }
    }
    Ok(())
}

fn ramp_sample(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let i = stops.partition_point(|s| s.0 <= t);
    if i == 0 {
        return stops[0].1;
    }
    if i == stops.len() {
        return stops[i - 1].1;
    }
    let a = stops[i - 1];
    let b = stops[i];
    let weight = (t - a.0) / (b.0 - a.0);
    // Match the native shader's straight stop interpolation exactly. The
    // caller applies retained pixel alpha (including geometry coverage).
    std::array::from_fn(|c| a.1[c] * (1. - weight) + b.1[c] * weight)
}

impl StrokeOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.miter_limit.is_finite() && (1. ..=32.).contains(&self.miter_limit),
            "Miter limit must be 1–32"
        );
        ensure!(
            self.dash_offset.is_finite() && self.dash_offset.abs() <= 1_000_000.,
            "Dash offset is outside supported bounds"
        );
        ensure!(
            self.dashes.len() <= 16
                && self.dashes.len() % 2 == 0
                && self
                    .dashes
                    .iter()
                    .all(|v| v.is_finite() && (0.1..=100_000.).contains(v)),
            "Dash pattern needs up to eight dash/gap pairs, each at least 0.1 px"
        );
        Ok(())
    }

    pub fn scale(&mut self, scale: f32) {
        for value in &mut self.dashes {
            *value *= scale;
        }
        self.dash_offset *= scale;
    }

    pub fn stroke(&self, width: f32, scale: f32) -> sk::Stroke {
        sk::Stroke {
            width,
            line_cap: match self.cap {
                StrokeCap::Butt => sk::LineCap::Butt,
                StrokeCap::Round => sk::LineCap::Round,
                StrokeCap::Square => sk::LineCap::Square,
            },
            line_join: match self.join {
                StrokeJoin::Miter => sk::LineJoin::Miter,
                StrokeJoin::Round => sk::LineJoin::Round,
                StrokeJoin::Bevel => sk::LineJoin::Bevel,
            },
            miter_limit: self.miter_limit,
            dash: (!self.dashes.is_empty())
                .then(|| {
                    sk::StrokeDash::new(
                        self.dashes.iter().map(|v| v * scale).collect(),
                        self.dash_offset * scale,
                    )
                })
                .flatten(),
        }
    }
}

pub(super) fn path(path: &VectorPath) -> Option<sk::Path> {
    let mut out = sk::PathBuilder::new();
    for subpath in &path.subpaths {
        let Some(first) = subpath.anchors.first() else {
            continue;
        };
        out.move_to(first.position.x, first.position.y);
        let count = subpath.anchors.len();
        for index in 0..count.saturating_sub(1) + usize::from(subpath.closed && count > 1) {
            let a = &subpath.anchors[index];
            let b = &subpath.anchors[(index + 1) % count];
            if a.outgoing.is_some() || b.incoming.is_some() {
                let c = a.outgoing.unwrap_or(a.position);
                let d = b.incoming.unwrap_or(b.position);
                out.cubic_to(c.x, c.y, d.x, d.y, b.position.x, b.position.y);
            } else {
                out.line_to(b.position.x, b.position.y);
            }
        }
        if subpath.closed {
            out.close();
        }
    }
    out.finish()
}

pub(super) fn render_tile(
    object: &super::VectorObject,
    path: &VectorPath,
    width: u32,
    height: u32,
    object_to_tile: sk::Transform,
    stroke_scale: f32,
) -> Result<image::RgbaImage> {
    let mut pixmap = sk::Pixmap::new(width, height).context("Cannot allocate vector tile")?;
    if let Some(path) = self::path(path) {
        let mut paint = sk::Paint::default();
        if let Some(gradient) = &object.fill_gradient {
            paint.shader = gradient.shader(object_to_tile)?;
        } else if let Some(c) = object.fill {
            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
        }
        if object.fill.is_some() || object.fill_gradient.is_some() {
            pixmap.fill_path(
                &path,
                &paint,
                match object.path.fill_rule {
                    FillRule::EvenOdd => sk::FillRule::EvenOdd,
                    FillRule::NonZero => sk::FillRule::Winding,
                },
                sk::Transform::identity(),
                None,
            );
            if let Some(gradient) = object.fill_gradient.as_ref().filter(|g| g.dither) {
                dither_fill(&mut pixmap, object_to_tile, gradient)?;
            }
        }
        if let Some(stroke) = object.stroke {
            paint.set_color_rgba8(
                stroke.color[0],
                stroke.color[1],
                stroke.color[2],
                stroke.color[3],
            );
            let options = object.stroke_options.clone().unwrap_or_default();
            pixmap.stroke_path(
                &path,
                &paint,
                &options.stroke(stroke.width * stroke_scale, stroke_scale),
                sk::Transform::identity(),
                None,
            );
        }
    }
    let mut output = image::RgbaImage::new(width, height);
    for (target, source) in output.pixels_mut().zip(pixmap.pixels()) {
        let color = source.demultiply();
        target.0 = [color.red(), color.green(), color.blue(), color.alpha()];
    }
    Ok(output)
}
