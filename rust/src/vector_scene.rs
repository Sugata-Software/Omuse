//! Bounded multi-object procedural vector scenes.
//!
//! Scene objects are authoritative geometry. Rendering produces a derived
//! straight-alpha RGBA8 compatibility cache, one bounded tile at a time; it
//! never allocates a full-canvas intermediate for each object.

#[path = "vector_scene_hit.rs"]
mod vector_scene_hit;

use crate::{
    model::{PixelRect, valid_dimensions},
    vector_path::{Anchor, FillRule, Point, StrokeStyle, Subpath, VectorPath, rasterize_rgba},
};
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
};

pub const VECTOR_SCENE_VERSION: u32 = 1;
pub const MAX_SCENE_OBJECTS: usize = 1_024;
pub const MAX_SCENE_ANCHORS: usize = 100_000;
pub const MAX_SCENE_SUBPATHS: usize = 4_096;
pub const MAX_SCENE_PIXELS: u64 = 16_777_216;
const MAX_OBJECT_NAME_BYTES: usize = 16_384;
const MAX_OBJECT_ID_BYTES: usize = 128;
// Leave room for the largest legal tile translation before VectorPath's own
// one-million-coordinate bound is checked by the rasterizer.
const MAX_TRANSFORMED_COORDINATE: f32 = 970_000.;
const MAX_SCENE_RENDER_WORK: u64 = 1_000_000_000;
const TILE_EDGE: u32 = 256;

/// A procedural vector layer. Objects paint in vector order, bottom first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorScene {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub objects: Vec<VectorObject>,
}

/// One independently styled path in scene-local pixel coordinates.
///
/// Affine coefficients map `(x, y)` to
/// `(a*x + c*y + e, b*x + d*y + f)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorObject {
    pub id: String,
    pub name: String,
    pub path: VectorPath,
    pub transform: [f32; 6],
    pub fill: Option<[u8; 4]>,
    pub stroke: Option<StrokeStyle>,
    pub opacity: f32,
    pub visible: bool,
}

impl VectorScene {
    pub fn from_path(
        width: u32,
        height: u32,
        name: impl Into<String>,
        path: VectorPath,
        fill: Option<[u8; 4]>,
        stroke: Option<StrokeStyle>,
    ) -> Result<Self> {
        let scene = Self {
            version: VECTOR_SCENE_VERSION,
            width,
            height,
            objects: vec![VectorObject::new(name, path, fill, stroke)],
        };
        scene.validate()?;
        Ok(scene)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == VECTOR_SCENE_VERSION,
            "unsupported vector scene version"
        );
        ensure!(
            valid_dimensions(self.width, self.height)
                && u64::from(self.width) * u64::from(self.height) <= MAX_SCENE_PIXELS,
            "vector scene cache exceeds 16 megapixels"
        );
        ensure!(
            self.objects.len() <= MAX_SCENE_OBJECTS,
            "vector scene has too many objects"
        );
        let mut ids = HashSet::with_capacity(self.objects.len());
        let mut anchors = 0usize;
        let mut subpaths = 0usize;
        for object in &self.objects {
            object.validate()?;
            let id = uuid::Uuid::parse_str(&object.id).context("invalid vector object UUID")?;
            ensure!(ids.insert(id), "vector object identities must be unique");
            subpaths = subpaths.saturating_add(object.path.subpaths.len());
            anchors = anchors.saturating_add(
                object
                    .path
                    .subpaths
                    .iter()
                    .map(|subpath| subpath.anchors.len())
                    .sum::<usize>(),
            );
            ensure!(
                subpaths <= MAX_SCENE_SUBPATHS,
                "vector scene has too many subpaths"
            );
            ensure!(
                anchors <= MAX_SCENE_ANCHORS,
                "vector scene has too many anchors"
            );
        }
        Ok(())
    }

    /// Estimated owned heap bytes for editor admission and clipboard budgets.
    pub fn retained_bytes(&self) -> usize {
        self.objects.iter().fold(
            size_of::<Self>().saturating_add(
                self.objects
                    .capacity()
                    .saturating_mul(size_of::<VectorObject>()),
            ),
            |total, object| {
                object.path.subpaths.iter().fold(
                    total
                        .saturating_add(object.id.capacity())
                        .saturating_add(object.name.capacity())
                        .saturating_add(
                            object
                                .path
                                .subpaths
                                .capacity()
                                .saturating_mul(size_of::<Subpath>()),
                        ),
                    |subtotal, subpath| {
                        subtotal.saturating_add(
                            subpath
                                .anchors
                                .capacity()
                                .saturating_mul(size_of::<Anchor>()),
                        )
                    },
                )
            },
        )
    }

    /// Render the full-size straight-alpha RGBA8 compatibility cache.
    pub fn render(&self, cancel: &AtomicBool) -> Result<RgbaImage> {
        self.render_region(
            PixelRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            1.,
            cancel,
        )
    }

    /// Render a whole-scene preview whose longest edge is at most `max_edge`.
    pub fn render_preview(&self, max_edge: u32, cancel: &AtomicBool) -> Result<RgbaImage> {
        ensure!(max_edge > 0, "preview edge must be positive");
        let scale = (max_edge as f32 / self.width.max(self.height) as f32).min(1.);
        self.render_region(
            PixelRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            scale,
            cancel,
        )
    }

    /// Render an integer scene region at a uniform scale. The output is built
    /// from at most 256x256 object tiles; only the returned compatibility image
    /// spans the complete requested region.
    pub fn render_region(
        &self,
        region: PixelRect,
        scale: f32,
        cancel: &AtomicBool,
    ) -> Result<RgbaImage> {
        self.validate()?;
        check_cancel(cancel)?;
        ensure!(
            region.width > 0
                && region.height > 0
                && region
                    .x
                    .checked_add(region.width)
                    .is_some_and(|x| x <= self.width)
                && region
                    .y
                    .checked_add(region.height)
                    .is_some_and(|y| y <= self.height),
            "vector render region is outside the scene"
        );
        ensure!(
            scale.is_finite() && scale > 0. && scale <= 64.,
            "vector render scale is outside 0..64"
        );
        let output_width = (region.width as f64 * f64::from(scale)).round().max(1.) as u32;
        let output_height = (region.height as f64 * f64::from(scale)).round().max(1.) as u32;
        ensure!(
            valid_dimensions(output_width, output_height)
                && u64::from(output_width) * u64::from(output_height) <= MAX_SCENE_PIXELS,
            "vector render output exceeds 16 megapixels"
        );
        let mut prepared = Vec::with_capacity(self.objects.len());
        let mut work = 0u64;
        for object in self
            .objects
            .iter()
            .filter(|object| object.visible && object.opacity > 0.)
        {
            check_cancel(cancel)?;
            let path = transform_path(&object.path, object.transform, region, scale)?;
            let stroke = match object.stroke {
                Some(mut stroke) => {
                    stroke.width *= object.similarity_scale()? * scale;
                    Some(stroke)
                }
                None => None,
            };
            let pad = stroke.map_or(1., |stroke| stroke.width * 0.5 + 1.);
            let Some(bounds) = path_bounds(&path, output_width, output_height, pad) else {
                continue;
            };
            let flattened = path
                .flatten(0.25, || cancel.load(Ordering::Relaxed))
                .with_context(|| format!("Cannot prepare vector object {}", object.name))?;
            let segments = flattened
                .iter()
                .map(|subpath| {
                    if subpath.closed {
                        subpath.points.len()
                    } else {
                        subpath.points.len().saturating_sub(1)
                    }
                })
                .sum::<usize>() as u64;
            let area = u64::from(bounds.width) * u64::from(bounds.height);
            let tile_columns = u64::from(bounds.width.div_ceil(TILE_EDGE));
            let tile_rows = u64::from(bounds.height.div_ceil(TILE_EDGE));
            let tile_count = tile_columns.saturating_mul(tile_rows);
            let flatten_passes = if object.fill.is_some() { 2 } else { 1 };
            work = work.saturating_add(
                segments
                    .saturating_mul(tile_count)
                    .saturating_mul(flatten_passes),
            );
            if object.fill.is_some() {
                work = work.saturating_add(area.saturating_mul(16)).saturating_add(
                    u64::from(bounds.height)
                        .saturating_mul(4)
                        .saturating_mul(segments)
                        .saturating_mul(tile_columns),
                );
            }
            if stroke.is_some() {
                work = work.saturating_add(area.saturating_mul(16).saturating_mul(segments.max(1)));
            }
            ensure!(
                work <= MAX_SCENE_RENDER_WORK,
                "vector scene render exceeds work limit"
            );
            prepared.push(PreparedObject {
                object,
                path,
                stroke,
                bounds,
            });
        }

        let mut output = RgbaImage::new(output_width, output_height);
        for prepared in prepared {
            for tile_y in (prepared.bounds.y..prepared.bounds.y + prepared.bounds.height)
                .step_by(TILE_EDGE as usize)
            {
                for tile_x in (prepared.bounds.x..prepared.bounds.x + prepared.bounds.width)
                    .step_by(TILE_EDGE as usize)
                {
                    check_cancel(cancel)?;
                    let tile_width = TILE_EDGE
                        .min(prepared.bounds.x + prepared.bounds.width - tile_x)
                        .min(output_width - tile_x);
                    let tile_height = TILE_EDGE
                        .min(prepared.bounds.y + prepared.bounds.height - tile_y)
                        .min(output_height - tile_y);
                    let path = translated_path(&prepared.path, -(tile_x as f32), -(tile_y as f32));
                    let tile = rasterize_rgba(
                        &path,
                        tile_width,
                        tile_height,
                        prepared.object.fill,
                        prepared.stroke,
                        0.25,
                        || cancel.load(Ordering::Relaxed),
                    )
                    .with_context(|| {
                        format!("Cannot render vector object {}", prepared.object.name)
                    })?;
                    for y in 0..tile_height {
                        check_cancel(cancel)?;
                        for x in 0..tile_width {
                            let mut source = tile.get_pixel(x, y).0;
                            source[3] = (f32::from(source[3]) * prepared.object.opacity)
                                .round()
                                .clamp(0., 255.) as u8;
                            if source[3] == 0 {
                                continue;
                            }
                            let destination = output.get_pixel_mut(tile_x + x, tile_y + y);
                            destination.0 = over(destination.0, source);
                        }
                    }
                }
            }
        }
        check_cancel(cancel)?;
        Ok(output)
    }
}

impl VectorObject {
    pub fn new(
        name: impl Into<String>,
        path: VectorPath,
        fill: Option<[u8; 4]>,
        stroke: Option<StrokeStyle>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            name: name.into(),
            path,
            transform: [1., 0., 0., 1., 0., 0.],
            fill,
            stroke,
            opacity: 1.,
            visible: true,
        }
    }

    pub fn rectangle(
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fill: Option<[u8; 4]>,
        stroke: Option<StrokeStyle>,
    ) -> Result<Self> {
        valid_shape_bounds(x, y, width, height)?;
        Ok(Self::new(
            name,
            VectorPath {
                subpaths: vec![Subpath {
                    anchors: vec![
                        anchor(x, y),
                        anchor(x + width, y),
                        anchor(x + width, y + height),
                        anchor(x, y + height),
                    ],
                    closed: true,
                }],
                fill_rule: FillRule::NonZero,
            },
            fill,
            stroke,
        ))
    }

    pub fn ellipse(
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fill: Option<[u8; 4]>,
        stroke: Option<StrokeStyle>,
    ) -> Result<Self> {
        valid_shape_bounds(x, y, width, height)?;
        let (cx, cy) = (x + width * 0.5, y + height * 0.5);
        let (rx, ry) = (width * 0.5, height * 0.5);
        let (kx, ky) = (rx * 0.552_284_8, ry * 0.552_284_8);
        Ok(Self::new(
            name,
            VectorPath {
                subpaths: vec![Subpath {
                    anchors: vec![
                        Anchor {
                            position: Point { x: cx + rx, y: cy },
                            incoming: Some(Point {
                                x: cx + rx,
                                y: cy - ky,
                            }),
                            outgoing: Some(Point {
                                x: cx + rx,
                                y: cy + ky,
                            }),
                        },
                        Anchor {
                            position: Point { x: cx, y: cy + ry },
                            incoming: Some(Point {
                                x: cx + kx,
                                y: cy + ry,
                            }),
                            outgoing: Some(Point {
                                x: cx - kx,
                                y: cy + ry,
                            }),
                        },
                        Anchor {
                            position: Point { x: cx - rx, y: cy },
                            incoming: Some(Point {
                                x: cx - rx,
                                y: cy + ky,
                            }),
                            outgoing: Some(Point {
                                x: cx - rx,
                                y: cy - ky,
                            }),
                        },
                        Anchor {
                            position: Point { x: cx, y: cy - ry },
                            incoming: Some(Point {
                                x: cx - kx,
                                y: cy - ry,
                            }),
                            outgoing: Some(Point {
                                x: cx + kx,
                                y: cy - ry,
                            }),
                        },
                    ],
                    closed: true,
                }],
                fill_rule: FillRule::NonZero,
            },
            fill,
            stroke,
        ))
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && self.id.len() <= MAX_OBJECT_ID_BYTES,
            "invalid vector object identity"
        );
        uuid::Uuid::parse_str(&self.id).context("invalid vector object UUID")?;
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= MAX_OBJECT_NAME_BYTES,
            "invalid vector object name"
        );
        self.path.validate()?;
        ensure!(
            self.fill.is_some() || self.stroke.is_some(),
            "vector object must have a fill or stroke"
        );
        ensure!(
            self.opacity.is_finite() && (0. ..=1.).contains(&self.opacity),
            "invalid vector object opacity"
        );
        ensure!(
            self.transform.iter().all(|value| value.is_finite()),
            "invalid vector object transform"
        );
        let [a, b, c, d, _, _] = self.transform;
        ensure!(
            (a * d - b * c).abs() >= 1e-8,
            "vector object transform is singular"
        );
        if self.stroke.is_some() {
            let _ = self.similarity_scale()?;
        }
        let transformed = transform_path_raw(&self.path, self.transform)?;
        transformed.validate()?;
        if let Some(stroke) = self.stroke {
            let scale = self.similarity_scale()?;
            ensure!(
                stroke.width.is_finite() && stroke.width > 0. && stroke.width * scale <= 100_000.,
                "invalid transformed vector stroke width"
            );
        }
        Ok(())
    }

    fn similarity_scale(&self) -> Result<f32> {
        let [a, b, c, d, _, _] = self.transform;
        let (x_scale, y_scale) = (a.hypot(b), c.hypot(d));
        let reference = x_scale.max(y_scale).max(1.);
        ensure!(
            x_scale > 1e-6
                && y_scale > 1e-6
                && (x_scale - y_scale).abs() <= reference * 1e-5
                && (a * c + b * d).abs() <= x_scale * y_scale * 1e-5,
            "stroked vector objects require a uniform rotation/scale transform"
        );
        Ok((x_scale + y_scale) * 0.5)
    }
}

struct PreparedObject<'a> {
    object: &'a VectorObject,
    path: VectorPath,
    stroke: Option<StrokeStyle>,
    bounds: PixelRect,
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "vector scene render cancelled"
    );
    Ok(())
}

fn transform_path(
    path: &VectorPath,
    transform: [f32; 6],
    region: PixelRect,
    scale: f32,
) -> Result<VectorPath> {
    let [a, b, c, d, e, f] = transform;
    transform_path_raw(
        path,
        [
            a * scale,
            b * scale,
            c * scale,
            d * scale,
            (e - region.x as f32) * scale,
            (f - region.y as f32) * scale,
        ],
    )
}

fn transform_path_raw(path: &VectorPath, transform: [f32; 6]) -> Result<VectorPath> {
    let [a, b, c, d, e, f] = transform;
    let apply = |point: Point| -> Result<Point> {
        let point = Point {
            x: a * point.x + c * point.y + e,
            y: b * point.x + d * point.y + f,
        };
        ensure!(
            point.x.is_finite()
                && point.y.is_finite()
                && point.x.abs() <= MAX_TRANSFORMED_COORDINATE
                && point.y.abs() <= MAX_TRANSFORMED_COORDINATE,
            "transformed vector coordinate is outside supported bounds"
        );
        Ok(point)
    };
    let mut transformed = path.clone();
    for subpath in &mut transformed.subpaths {
        for anchor in &mut subpath.anchors {
            anchor.position = apply(anchor.position)?;
            if let Some(point) = anchor.incoming {
                anchor.incoming = Some(apply(point)?);
            }
            if let Some(point) = anchor.outgoing {
                anchor.outgoing = Some(apply(point)?);
            }
        }
    }
    Ok(transformed)
}

fn translated_path(path: &VectorPath, dx: f32, dy: f32) -> VectorPath {
    let mut translated = path.clone();
    for subpath in &mut translated.subpaths {
        for anchor in &mut subpath.anchors {
            anchor.position.x += dx;
            anchor.position.y += dy;
            if let Some(point) = &mut anchor.incoming {
                point.x += dx;
                point.y += dy;
            }
            if let Some(point) = &mut anchor.outgoing {
                point.x += dx;
                point.y += dy;
            }
        }
    }
    translated
}

fn path_bounds(path: &VectorPath, width: u32, height: u32, padding: f32) -> Option<PixelRect> {
    let (low, high) = path.bounds()?;
    let x0 = (low.x - padding).floor().clamp(0., width as f32) as u32;
    let y0 = (low.y - padding).floor().clamp(0., height as f32) as u32;
    let x1 = (high.x + padding).ceil().clamp(0., width as f32) as u32;
    let y1 = (high.y + padding).ceil().clamp(0., height as f32) as u32;
    (x0 < x1 && y0 < y1).then_some(PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

fn valid_shape_bounds(x: f32, y: f32, width: f32, height: f32) -> Result<()> {
    ensure!(
        [x, y, width, height].iter().all(|value| value.is_finite())
            && width > 0.
            && height > 0.
            && x.abs() <= MAX_TRANSFORMED_COORDINATE
            && y.abs() <= MAX_TRANSFORMED_COORDINATE
            && (x + width).abs() <= MAX_TRANSFORMED_COORDINATE
            && (y + height).abs() <= MAX_TRANSFORMED_COORDINATE,
        "invalid vector shape bounds"
    );
    Ok(())
}

fn anchor(x: f32, y: f32) -> Anchor {
    Anchor {
        position: Point { x, y },
        incoming: None,
        outgoing: None,
    }
}

fn over(destination: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    let source_alpha = f32::from(source[3]) / 255.;
    let destination_alpha = f32::from(destination[3]) / 255.;
    let alpha = source_alpha + destination_alpha * (1. - source_alpha);
    if alpha <= 0. {
        return [0; 4];
    }
    let mut output = [0; 4];
    for channel in 0..3 {
        output[channel] = ((f32::from(source[channel]) * source_alpha
            + f32::from(destination[channel]) * destination_alpha * (1. - source_alpha))
            / alpha)
            .round()
            .clamp(0., 255.) as u8;
    }
    output[3] = (alpha * 255.).round().clamp(0., 255.) as u8;
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(objects: Vec<VectorObject>) -> VectorScene {
        VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 64,
            height: 48,
            objects,
        }
    }

    #[test]
    fn renders_bottom_first_with_group_opacity_and_straight_alpha() {
        let bottom =
            VectorObject::rectangle("Red", 4., 4., 40., 32., Some([255, 0, 0, 255]), None).unwrap();
        let mut top =
            VectorObject::rectangle("Blue", 20., 12., 36., 28., Some([0, 0, 255, 255]), None)
                .unwrap();
        top.opacity = 0.5;
        let rendered = scene(vec![bottom, top])
            .render(&AtomicBool::new(false))
            .unwrap();
        assert_eq!(rendered.get_pixel(8, 8).0, [255, 0, 0, 255]);
        assert_eq!(rendered.get_pixel(52, 20).0, [0, 0, 255, 128]);
        assert_eq!(rendered.get_pixel(24, 20).0, [127, 0, 128, 255]);
        assert_eq!(rendered.get_pixel(0, 0).0, [0; 4]);
    }

    #[test]
    fn affine_preview_and_region_are_deterministic_and_culled() {
        let mut object =
            VectorObject::ellipse("Ellipse", 4., 8., 16., 12., Some([20, 180, 80, 255]), None)
                .unwrap();
        object.transform = [1., 0., 0., 1., 20., 10.];
        let scene = scene(vec![object]);
        let preview = scene.render_preview(32, &AtomicBool::new(false)).unwrap();
        assert_eq!(preview.dimensions(), (32, 24));
        let region = scene
            .render_region(
                PixelRect {
                    x: 16,
                    y: 8,
                    width: 32,
                    height: 24,
                },
                1.,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(region.pixels().any(|pixel| pixel[3] > 0));
        assert_eq!(
            region,
            scene
                .render_region(
                    PixelRect {
                        x: 16,
                        y: 8,
                        width: 32,
                        height: 24,
                    },
                    1.,
                    &AtomicBool::new(false),
                )
                .unwrap()
        );
    }

    #[test]
    fn preserves_even_odd_holes_and_tile_seams() {
        let mut object = VectorObject::rectangle(
            "Compound",
            4.,
            4.,
            504.,
            40.,
            Some([30, 90, 210, 255]),
            None,
        )
        .unwrap();
        object.path.subpaths.push(Subpath {
            anchors: vec![
                anchor(250., 14.),
                anchor(270., 14.),
                anchor(270., 34.),
                anchor(250., 34.),
            ],
            closed: true,
        });
        object.path.fill_rule = FillRule::EvenOdd;
        let scene = VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 512,
            height: 64,
            objects: vec![object],
        };
        let rendered = scene.render(&AtomicBool::new(false)).unwrap();
        assert_eq!(rendered.get_pixel(245, 20).0, [30, 90, 210, 255]);
        assert_eq!(rendered.get_pixel(260, 20).0, [0; 4]);
        assert_eq!(rendered.get_pixel(275, 20).0, [30, 90, 210, 255]);
        assert_eq!(rendered.get_pixel(258, 8).0, [30, 90, 210, 255]);
        assert_eq!(rendered.get_pixel(259, 8).0, [30, 90, 210, 255]);
    }

    #[test]
    fn full_render_matches_independently_assembled_regions() {
        let background = VectorObject::rectangle(
            "Background",
            0.,
            0.,
            512.,
            288.,
            Some([14, 24, 40, 255]),
            None,
        )
        .unwrap();
        let mut curve = VectorObject::ellipse(
            "Curve",
            96.,
            36.,
            340.,
            210.,
            Some([220, 75, 30, 190]),
            None,
        )
        .unwrap();
        curve.transform = [1., 0.08, -0.12, 1., 16., -4.];
        let scene = VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 512,
            height: 288,
            objects: vec![background, curve],
        };
        let cancel = AtomicBool::new(false);
        let full = scene.render(&cancel).unwrap();
        let mut assembled = RgbaImage::new(512, 288);
        for (x, y, width, height) in [
            (0, 0, 257, 123),
            (257, 0, 255, 123),
            (0, 123, 257, 165),
            (257, 123, 255, 165),
        ] {
            let part = scene
                .render_region(
                    PixelRect {
                        x,
                        y,
                        width,
                        height,
                    },
                    1.,
                    &cancel,
                )
                .unwrap();
            for part_y in 0..height {
                for part_x in 0..width {
                    assembled.put_pixel(x + part_x, y + part_y, *part.get_pixel(part_x, part_y));
                }
            }
        }
        assert_eq!(assembled, full);
    }

    #[test]
    fn curved_stroke_is_rejected_by_aggregate_work_budget() {
        let curve = VectorObject::ellipse(
            "Large curved stroke",
            8.,
            8.,
            4_080.,
            4_080.,
            None,
            Some(StrokeStyle {
                color: [255, 255, 255, 255],
                width: 2.,
            }),
        )
        .unwrap();
        let scene = VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 4_096,
            height: 4_096,
            objects: vec![curve],
        };
        let error = scene.render(&AtomicBool::new(false)).unwrap_err();
        assert!(error.to_string().contains("work limit"));
    }

    #[test]
    fn fill_only_objects_allow_general_affine_transforms() {
        let mut object =
            VectorObject::rectangle("Skewed", 4., 4., 12., 10., Some([80, 40, 20, 255]), None)
                .unwrap();
        object.transform = [1.5, 0.2, 0.4, 0.75, 3., 2.];
        let scene = scene(vec![object]);
        scene.validate().unwrap();
        assert!(
            scene
                .render(&AtomicBool::new(false))
                .unwrap()
                .pixels()
                .any(|pixel| pixel[3] > 0)
        );
    }

    #[test]
    fn validation_rejects_duplicate_ids_nonuniform_strokes_and_limits() {
        let object =
            VectorObject::rectangle("Rectangle", 0., 0., 10., 10., Some([1, 2, 3, 255]), None)
                .unwrap();
        let mut duplicate = object.clone();
        duplicate.name = "Duplicate".into();
        duplicate.id = duplicate.id.to_lowercase();
        assert!(scene(vec![object.clone(), duplicate]).validate().is_err());

        let mut stretched = object.clone();
        stretched.stroke = Some(StrokeStyle {
            color: [0, 0, 0, 255],
            width: 2.,
        });
        stretched.transform = [2., 0., 0., 1., 0., 0.];
        assert!(scene(vec![stretched]).validate().is_err());

        let mut too_many = Vec::with_capacity(MAX_SCENE_OBJECTS + 1);
        for index in 0..=MAX_SCENE_OBJECTS {
            let mut object = object.clone();
            object.id = uuid::Uuid::new_v4().to_string();
            object.name = format!("Object {index}");
            too_many.push(object);
        }
        assert!(scene(too_many).validate().is_err());
    }

    #[test]
    fn cancellation_and_retained_memory_are_explicit() {
        let object =
            VectorObject::rectangle("Rectangle", 0., 0., 32., 32., Some([10, 20, 30, 255]), None)
                .unwrap();
        let scene = scene(vec![object]);
        assert!(scene.retained_bytes() >= size_of::<VectorScene>());
        let cancel = AtomicBool::new(true);
        let error = scene.render(&cancel).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn serialization_is_strict_and_round_trips() {
        let scene = scene(vec![
            VectorObject::ellipse(
                "Ellipse",
                2.,
                3.,
                20.,
                10.,
                Some([1, 2, 3, 4]),
                Some(StrokeStyle {
                    color: [5, 6, 7, 8],
                    width: 1.25,
                }),
            )
            .unwrap(),
        ]);
        let encoded = serde_json::to_vec(&scene).unwrap();
        let decoded: VectorScene = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, scene);

        let mut value = serde_json::to_value(&scene).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("future".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<VectorScene>(value).is_err());
    }
}
