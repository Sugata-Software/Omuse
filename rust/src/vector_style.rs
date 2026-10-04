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

/// Coordinates and the affine transform are local to the containing object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientFill {
    pub kind: GradientKind,
    pub stops: Vec<GradientStop>,
    pub spread: GradientSpread,
    pub transform: [f32; 6],
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
        let stops = self
            .stops
            .iter()
            .map(|s| {
                sk::GradientStop::new(
                    s.offset,
                    sk::Color::from_rgba8(s.color[0], s.color[1], s.color[2], s.color[3]),
                )
            })
            .collect();
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
