//! Bounded fill-rule-aware boolean operations for filled vector objects.

use crate::vector_path::{Anchor, FillRule, Point, Subpath, VectorPath};
use crate::vector_scene::VectorObject;
use anyhow::{Context, Result, ensure};
use i_overlay::{
    core::{fill_rule::FillRule as OverlayFillRule, overlay_rule::OverlayRule},
    float::overlay::FloatOverlay,
};
use std::sync::atomic::{AtomicBool, Ordering};

const WORLD_TOLERANCE: f32 = 0.05;
const MAX_INPUT_POINTS: usize = 100_000;
const MAX_OUTPUT_POINTS: usize = 100_000;
const MAX_OUTPUT_SHAPES: usize = 4_096;
type Contour = Vec<[f32; 2]>;
type Shape = Vec<Contour>;
type Shapes = Vec<Shape>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOperation {
    Union,
    Subtract,
    Intersect,
    Exclude,
    Divide,
}

/// Combines selected filled objects. The first object supplies resulting style.
/// Returned objects use world coordinates and identity transforms.
pub fn combine(
    objects: &[VectorObject],
    operation: BooleanOperation,
    cancel: &AtomicBool,
) -> Result<Vec<VectorObject>> {
    ensure!(!objects.is_empty(), "Select at least one vector object");
    ensure!(
        objects.len() <= 256,
        "Too many vector objects for one boolean operation"
    );
    let mut total_points = 0usize;
    let mut normalized = Vec::with_capacity(objects.len());
    for object in objects {
        check_cancel(cancel)?;
        object.path.validate()?;
        ensure!(
            object.text_path.is_none(),
            "Convert text to outlines before using boolean operations"
        );
        let raw_points = object
            .path
            .subpaths
            .iter()
            .map(|subpath| subpath.anchors.len())
            .sum::<usize>();
        total_points = total_points.saturating_add(raw_points);
        ensure!(
            total_points <= MAX_INPUT_POINTS,
            "Boolean input exceeds point limit"
        );
        ensure!(
            object.fill.is_some(),
            "Boolean operations require filled paths"
        );
        ensure!(
            object.path.subpaths.iter().all(|s| s.closed),
            "Open paths cannot be used in boolean operations"
        );
        let (shape, count) = to_world_shape(object, cancel)?;
        total_points = total_points.saturating_add(count);
        ensure!(
            total_points <= MAX_INPUT_POINTS,
            "Boolean input exceeds point limit"
        );
        normalized.push(normalize(shape, object.path.fill_rule, cancel)?);
    }
    let mut result = normalized[0].clone();
    for rhs in normalized.iter().skip(1) {
        check_cancel(cancel)?;
        if operation == BooleanOperation::Divide {
            let mut pieces = Vec::new();
            for subject_piece in result {
                let subject = vec![subject_piece];
                pieces.extend(overlay(&subject, rhs, OverlayRule::Difference, cancel)?);
                pieces.extend(overlay(&subject, rhs, OverlayRule::Intersect, cancel)?);
                ensure_limits(&pieces)?;
            }
            result = pieces;
        } else {
            result = overlay(&result, rhs, operation_rule(operation), cancel)?;
        }
        ensure_limits(&result)?;
    }
    let style = &objects[0];
    result
        .into_iter()
        .enumerate()
        .map(|(index, shape)| {
            check_cancel(cancel)?;
            let mut object = VectorObject::new(
                format!("{} {}", style.name, index + 1),
                from_shape(shape)?,
                style.fill,
                style.stroke,
            );
            object.fill_gradient = style.fill_gradient.clone();
            if let Some(gradient) = &mut object.fill_gradient {
                gradient.bake_transform(style.transform);
            }
            object.stroke_options = style.stroke_options.clone();
            let scale = style.transform[0].hypot(style.transform[1]);
            if let Some(stroke) = &mut object.stroke {
                stroke.width *= scale;
            }
            if let Some(options) = &mut object.stroke_options {
                options.scale(scale);
            }
            object.opacity = style.opacity;
            object.visible = style.visible;
            Ok(object)
        })
        .collect()
}

fn operation_rule(operation: BooleanOperation) -> OverlayRule {
    match operation {
        BooleanOperation::Union => OverlayRule::Union,
        BooleanOperation::Subtract => OverlayRule::Difference,
        BooleanOperation::Intersect => OverlayRule::Intersect,
        BooleanOperation::Exclude => OverlayRule::Xor,
        BooleanOperation::Divide => unreachable!(),
    }
}

fn overlay(
    subject: &Shapes,
    clip: &Shapes,
    rule: OverlayRule,
    cancel: &AtomicBool,
) -> Result<Shapes> {
    check_cancel(cancel)?;
    let result =
        FloatOverlay::with_subj_and_clip(subject, clip).overlay(rule, OverlayFillRule::EvenOdd);
    check_cancel(cancel)?;
    ensure_limits(&result)?;
    Ok(result)
}

fn normalize(shape: Shape, fill_rule: FillRule, cancel: &AtomicBool) -> Result<Shapes> {
    let rule = match fill_rule {
        FillRule::EvenOdd => OverlayFillRule::EvenOdd,
        FillRule::NonZero => OverlayFillRule::NonZero,
    };
    check_cancel(cancel)?;
    // i_overlay canonicalizes output as clockwise outer contours followed by
    // counter-clockwise holes. VectorPath accepts either global orientation,
    // so normalize a counter-clockwise outer contour before asking the engine
    // to simplify it; reversing every contour preserves non-zero winding
    // relationships and leaves EvenOdd geometry unchanged.
    let shape = canonicalize_orientation(shape);
    let result = FloatOverlay::with_subj(&shape).overlay(OverlayRule::Subject, rule);
    check_cancel(cancel)?;
    ensure_limits(&result)?;
    Ok(result)
}

fn canonicalize_orientation(mut shape: Shape) -> Shape {
    let Some(first) = shape.iter().find(|contour| contour.len() >= 3) else {
        return shape;
    };
    if signed_area(first) > 0.0 {
        for contour in &mut shape {
            contour.reverse();
        }
    }
    shape
}

fn signed_area(contour: &Contour) -> f32 {
    contour
        .iter()
        .zip(contour.iter().cycle().skip(1))
        .take(contour.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f32>()
        * 0.5
}

fn to_world_shape(object: &VectorObject, cancel: &AtomicBool) -> Result<(Shape, usize)> {
    let scale = object.transform[0]
        .mul_add(
            object.transform[0],
            object.transform[1].mul_add(
                object.transform[1],
                object.transform[2].mul_add(
                    object.transform[2],
                    object.transform[3] * object.transform[3],
                ),
            ),
        )
        .sqrt();
    ensure!(
        scale.is_finite() && scale > 0.,
        "Invalid vector transform scale"
    );
    let determinant =
        object.transform[0] * object.transform[3] - object.transform[1] * object.transform[2];
    ensure!(
        determinant.is_finite() && determinant.abs() >= 1.0e-8,
        "Invalid singular vector transform"
    );
    let world_path = world_path(&object.path, object.transform)?;
    let flattened = world_path
        .flatten(WORLD_TOLERANCE, || cancel.load(Ordering::Relaxed))
        .context("Cannot flatten vector path for boolean operation")?;
    let mut count = 0usize;
    let mut shape = Vec::with_capacity(flattened.len());
    for subpath in flattened {
        ensure!(
            subpath.closed,
            "Open paths cannot be used in boolean operations"
        );
        ensure!(
            subpath.points.len() >= 3,
            "Boolean path needs at least three points"
        );
        count = count.saturating_add(subpath.points.len());
        ensure!(
            count <= MAX_INPUT_POINTS,
            "Boolean input exceeds point limit"
        );
        let mut points = subpath.points;
        if points.len() > 3 && points.first() == points.last() {
            points.pop();
        }
        let contour = points
            .into_iter()
            .map(|p| Ok([p.x, p.y]))
            .collect::<Result<Contour>>()?;
        shape.push(contour);
    }
    Ok((shape, count))
}

fn from_shape(shape: Shape) -> Result<VectorPath> {
    ensure!(!shape.is_empty(), "Boolean result has no filled area");
    let subpaths = shape
        .into_iter()
        .map(|ring| {
            ensure!(ring.len() >= 3, "Boolean result ring is too short");
            Ok(Subpath {
                anchors: ring
                    .into_iter()
                    .map(|p| Anchor {
                        position: Point { x: p[0], y: p[1] },
                        incoming: None,
                        outgoing: None,
                    })
                    .collect(),
                closed: true,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(VectorPath {
        subpaths,
        fill_rule: FillRule::EvenOdd,
    })
}

fn world_path(path: &VectorPath, transform: [f32; 6]) -> Result<VectorPath> {
    let map = |point: Point| Point {
        x: transform[0] * point.x + transform[2] * point.y + transform[4],
        y: transform[1] * point.x + transform[3] * point.y + transform[5],
    };
    let path = VectorPath {
        subpaths: path
            .subpaths
            .iter()
            .map(|subpath| Subpath {
                anchors: subpath
                    .anchors
                    .iter()
                    .map(|anchor| Anchor {
                        position: map(anchor.position),
                        incoming: anchor.incoming.map(map),
                        outgoing: anchor.outgoing.map(map),
                    })
                    .collect(),
                closed: subpath.closed,
            })
            .collect(),
        fill_rule: path.fill_rule,
    };
    path.validate()?;
    Ok(path)
}

fn ensure_limits(shapes: &Shapes) -> Result<()> {
    ensure!(
        shapes.len() <= MAX_OUTPUT_SHAPES,
        "Boolean output has too many shapes"
    );
    ensure!(
        shapes.iter().flatten().map(Vec::len).sum::<usize>() <= MAX_OUTPUT_POINTS,
        "Boolean output exceeds point limit"
    );
    Ok(())
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Boolean operation cancelled"
    );
    Ok(())
}
