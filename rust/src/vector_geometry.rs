//! Bounded Bézier geometry adapters. No operation mutates the source object.
//!
//! The sweep/stroker/fitter do not expose cancellation within one invocation.
//! Input, intermediate and output budgets limit admitted work; cancellation is
//! checked between invocations and before returning anything to the caller.
use crate::vector_path::{Anchor, FillRule, Point, Subpath, VectorPath};
use crate::vector_scene::{StrokeCap, StrokeJoin, StrokeOptions, VectorObject};
use anyhow::{Result, anyhow, ensure};
use kurbo::{BezPath, Cap, Join, PathEl, Shape, Stroke, StrokeOpts};
use linesweeper::topology::{BinaryWindingNumber, ContourIdx, Contours, OneOfTwo, Topology};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "../vendor/omuse-pathops/snap.rs"]
mod horizontal;

/// This is deliberately lower than the document's storage limit: the sweep can
/// create quadratically many intersections. It is not a wall-clock guarantee.
pub const MAX_GEOMETRY_SEGMENTS: usize = 2_048;
pub const MAX_GEOMETRY_OUTPUT_ANCHORS: usize = 16_384;
const MAX_COMPONENTS: usize = 1_024;
const STROKE_TOLERANCE: f64 = 0.01;

#[derive(Clone, Copy)]
pub(crate) enum RegionOperation {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Vector geometry operation cancelled"
    );
    Ok(())
}

pub(crate) fn count_anchors(path: &VectorPath) -> usize {
    path.subpaths.iter().map(|s| s.anchors.len()).sum()
}

fn validate_object(object: &VectorObject) -> Result<()> {
    object.path.validate()?;
    ensure!(
        object.text_path.is_none(),
        "Convert text to outlines before editing its geometry"
    );
    ensure!(
        object.transform.iter().all(|v| v.is_finite()),
        "Invalid vector transform"
    );
    let [a, b, c, d, _, _] = object.transform;
    ensure!(
        (a * d - b * c).is_finite() && (a * d - b * c).abs() >= 1e-8,
        "Invalid singular vector transform"
    );
    ensure!(
        object.opacity.is_finite() && (0. ..=1.).contains(&object.opacity),
        "Invalid vector opacity"
    );
    if let Some(gradient) = &object.fill_gradient {
        ensure!(
            object.fill.is_some(),
            "Gradient requires a fallback fill colour"
        );
        gradient.validate()?;
    }
    if let Some(options) = &object.stroke_options {
        ensure!(object.stroke.is_some(), "Stroke options need a stroke");
        options.validate()?;
    }
    if let Some(stroke) = object.stroke {
        let scale = stroke_scale(object)?;
        ensure!(
            stroke.width.is_finite() && stroke.width > 0. && stroke.width * scale <= 100_000.,
            "Invalid transformed stroke width"
        );
    }
    Ok(())
}

pub(crate) fn validate_input(object: &VectorObject) -> Result<()> {
    validate_object(object)?;
    ensure!(
        count_anchors(&object.path) <= MAX_GEOMETRY_SEGMENTS,
        "Vector geometry exceeds the 2048-segment work limit; use fewer objects or simplify a traced path first"
    );
    Ok(())
}

fn stroke_scale(object: &VectorObject) -> Result<f32> {
    let [a, b, c, d, _, _] = object.transform;
    let (x, y) = (a.hypot(b), c.hypot(d));
    ensure!(
        x > 1e-6
            && y > 1e-6
            && (x - y).abs() <= x.max(y).max(1.) * 1e-5
            && (a * c + b * d).abs() <= x * y * 1e-5,
        "Stroked paths require a uniform rotation/scale transform"
    );
    Ok((x + y) * 0.5)
}

pub(crate) fn world_path(object: &VectorObject) -> Result<VectorPath> {
    validate_input(object)?;
    let [a, b, c, d, e, f] = object.transform;
    let map = |p: Point| Point {
        x: a * p.x + c * p.y + e,
        y: b * p.x + d * p.y + f,
    };
    let mut path = object.path.clone();
    for sub in &mut path.subpaths {
        for anchor in &mut sub.anchors {
            anchor.position = map(anchor.position);
            anchor.incoming = anchor.incoming.map(map);
            anchor.outgoing = anchor.outgoing.map(map);
        }
    }
    path.validate()?;
    Ok(path)
}

pub(crate) fn styled_world_result(
    style: &VectorObject,
    path: VectorPath,
    index: usize,
) -> Result<VectorObject> {
    path.validate()?;
    let mut object = VectorObject::new(
        format!("{} {}", style.name, index + 1),
        path,
        style.fill,
        style.stroke,
    );
    object.fill_gradient = style.fill_gradient.clone();
    if let Some(gradient) = &mut object.fill_gradient {
        gradient.bake_transform(style.transform);
        gradient.validate()?;
    }
    object.stroke_options = style.stroke_options.clone();
    if let Some(stroke) = &mut object.stroke {
        let scale = stroke_scale(style)?;
        stroke.width *= scale;
        if let Some(options) = &mut object.stroke_options {
            options.scale(scale);
        }
    }
    object.opacity = style.opacity;
    object.visible = style.visible;
    // As with the previous boolean engine, caller assigns common group ancestry.
    Ok(object)
}

pub(crate) fn to_bez(path: &VectorPath) -> BezPath {
    let point = |p: Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
    let mut out = BezPath::new();
    for sub in &path.subpaths {
        let Some(first) = sub.anchors.first() else {
            continue;
        };
        out.move_to(point(first.position));
        let n = sub.anchors.len();
        for i in 0..if sub.closed { n } else { n.saturating_sub(1) } {
            let (a, b) = (&sub.anchors[i], &sub.anchors[(i + 1) % n]);
            if a.outgoing.is_none() && b.incoming.is_none() {
                out.line_to(point(b.position));
            } else {
                out.curve_to(
                    point(a.outgoing.unwrap_or(a.position)),
                    point(b.incoming.unwrap_or(b.position)),
                    point(b.position),
                );
            }
        }
        if sub.closed {
            out.close_path();
        }
    }
    out
}

fn from_bez(path: &BezPath, rule: FillRule) -> Result<VectorPath> {
    let point = |p: kurbo::Point| Point {
        x: p.x as f32,
        y: p.y as f32,
    };
    let mut result = VectorPath {
        subpaths: Vec::new(),
        fill_rule: rule,
    };
    let mut current = Subpath {
        anchors: Vec::new(),
        closed: false,
    };
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                if !current.anchors.is_empty() {
                    result.subpaths.push(current);
                }
                current = Subpath {
                    anchors: vec![Anchor {
                        position: point(p),
                        incoming: None,
                        outgoing: None,
                    }],
                    closed: false,
                };
            }
            PathEl::LineTo(p) => current.anchors.push(Anchor {
                position: point(p),
                incoming: None,
                outgoing: None,
            }),
            PathEl::CurveTo(a, b, p) => {
                let last = current
                    .anchors
                    .last_mut()
                    .ok_or_else(|| anyhow!("Curve has no starting point"))?;
                last.outgoing = Some(point(a));
                current.anchors.push(Anchor {
                    position: point(p),
                    incoming: Some(point(b)),
                    outgoing: None,
                });
            }
            PathEl::QuadTo(c, p) => {
                let last = current
                    .anchors
                    .last_mut()
                    .ok_or_else(|| anyhow!("Curve has no starting point"))?;
                let start =
                    kurbo::Point::new(f64::from(last.position.x), f64::from(last.position.y));
                last.outgoing = Some(point(start.lerp(c, 2. / 3.)));
                current.anchors.push(Anchor {
                    position: point(p),
                    incoming: Some(point(p.lerp(c, 2. / 3.))),
                    outgoing: None,
                });
            }
            PathEl::ClosePath => {
                current.closed = true;
                if current.anchors.len() > 2
                    && current.anchors.first().map(|a| a.position)
                        == current.anchors.last().map(|a| a.position)
                {
                    if let Some(last) = current.anchors.pop() {
                        current.anchors[0].incoming = last.incoming;
                    }
                }
            }
        }
    }
    if !current.anchors.is_empty() {
        result.subpaths.push(current);
    }
    ensure!(
        count_anchors(&result) <= MAX_GEOMETRY_OUTPUT_ANCHORS,
        "Vector geometry output exceeds anchor budget"
    );
    result.validate()?;
    Ok(result)
}

fn inside(rule: FillRule, winding: i32) -> bool {
    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding % 2 != 0,
    }
}

fn sweep(
    a: &VectorPath,
    b: &VectorPath,
    operation: RegionOperation,
    cancel: &AtomicBool,
) -> Result<Contours> {
    a.validate()?;
    b.validate()?;
    ensure!(
        a.subpaths.iter().chain(&b.subpaths).all(|s| s.closed),
        "Open paths cannot be used in filled geometry operations"
    );
    ensure!(
        count_anchors(a).saturating_add(count_anchors(b)) <= MAX_GEOMETRY_SEGMENTS,
        "Vector geometry exceeds the 2048-segment sweep limit"
    );
    check_cancel(cancel)?;
    let (mut left, mut right) = (to_bez(a), to_bez(b));
    // Reject pathological arrangements before the noninterruptible sweep can
    // allocate their intersections. Control hulls conservatively enclose every
    // curve; include tangencies and self-intersection candidates. The pair scan
    // itself is bounded by the admitted segment count and checks cancellation.
    let boxes: Vec<_> = left
        .segments()
        .chain(right.segments())
        .map(|segment| {
            let curve = segment.to_cubic();
            kurbo::Rect::from_points(curve.p0, curve.p3)
                .union_pt(curve.p1)
                .union_pt(curve.p2)
        })
        .collect();
    let mut pairs = 0usize;
    for (i, first) in boxes.iter().enumerate() {
        check_cancel(cancel)?;
        for second in &boxes[i + 1..] {
            if first.x0 <= second.x1 + 1e-6
                && second.x0 <= first.x1 + 1e-6
                && first.y0 <= second.y1 + 1e-6
                && second.y0 <= first.y1 + 1e-6
            {
                pairs += 1;
                ensure!(
                    pairs <= 8_192,
                    "Vector geometry intersection work limit exceeded; select fewer or less-overlapping paths"
                );
            }
        }
    }
    // Coordinates are already bounded by VectorPath. The sweep uses f64, so
    // this tolerance is below the document's f32 resolution at large positions.
    horizontal::snap_horizontals(&mut [&mut left, &mut right], 1e-6);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let topology = Topology::<BinaryWindingNumber>::from_paths(
            [(&left, OneOfTwo::A), (&right, OneOfTwo::B)],
            1e-6,
        )
        .map_err(|_| anyhow!("Vector path could not be closed"))?;
        check_cancel(cancel)?;
        // Stop before contour construction when the topology expanded too far.
        ensure!(
            topology
                .segment_indices()
                .take(MAX_GEOMETRY_OUTPUT_ANCHORS + 1)
                .count()
                <= MAX_GEOMETRY_OUTPUT_ANCHORS,
            "Vector geometry intersection budget exceeded"
        );
        Ok(topology.contours(|w| {
            let (a, b) = (
                inside(a.fill_rule, w.shape_a),
                inside(b.fill_rule, w.shape_b),
            );
            match operation {
                RegionOperation::Union => a || b,
                RegionOperation::Subtract => a && !b,
                RegionOperation::Intersect => a && b,
                RegionOperation::Exclude => a != b,
            }
        }))
    }))
    .map_err(|_| anyhow!("These paths are too degenerate to combine safely"))?;
    check_cancel(cancel)?;
    result
}

fn contour_paths(contours: &Contours, cancel: &AtomicBool) -> Result<Vec<VectorPath>> {
    let all: Vec<_> = contours.contours().collect();
    ensure!(
        all.len() <= MAX_COMPONENTS,
        "Vector geometry produces too many contours"
    );
    let mut result = Vec::new();
    let mut total = 0usize;
    for (i, contour) in all.iter().enumerate().filter(|(_, c)| c.outer) {
        check_cancel(cancel)?;
        let mut path = from_bez(&contour.path, FillRule::EvenOdd)?;
        for hole in all
            .iter()
            .filter(|c| !c.outer && c.parent == Some(ContourIdx(i)))
        {
            path.subpaths
                .extend(from_bez(&hole.path, FillRule::EvenOdd)?.subpaths);
        }
        total = total.saturating_add(count_anchors(&path));
        ensure!(
            total <= MAX_GEOMETRY_OUTPUT_ANCHORS,
            "Vector geometry output exceeds anchor budget"
        );
        path.validate()?;
        result.push(path);
    }
    Ok(result)
}

pub(crate) fn region(
    a: &VectorPath,
    b: &VectorPath,
    operation: RegionOperation,
    cancel: &AtomicBool,
) -> Result<Vec<VectorPath>> {
    contour_paths(&sweep(a, b, operation, cancel)?, cancel)
}

pub(crate) fn normalize(path: &VectorPath, cancel: &AtomicBool) -> Result<Vec<VectorPath>> {
    region(path, &VectorPath::default(), RegionOperation::Union, cancel)
}

pub(crate) fn compound(paths: &[VectorPath]) -> VectorPath {
    VectorPath {
        subpaths: paths
            .iter()
            .flat_map(|p| p.subpaths.iter().cloned())
            .collect(),
        fill_rule: FillRule::EvenOdd,
    }
}

/// Simplify each subpath in local space, preserving its fill rule, closure,
/// endpoints, style, identity and transform. Tolerance is in world pixels.
/// Straight-segment paths use distance-bounded polyline reduction, preserving
/// turns sharper than 30 degrees. Cubics use kurbo's approximate fitter; this
/// requested tolerance is not a universal topology/Hausdorff guarantee. A fit
/// that adds anchors is discarded. Polyline work has an eight-million distance
/// test budget; cubic fitting admits at most 2048 anchors across the object.
pub fn simplify(
    object: &VectorObject,
    tolerance: f32,
    cancel: &AtomicBool,
) -> Result<VectorObject> {
    validate_object(object)?;
    check_cancel(cancel)?;
    ensure!(
        tolerance.is_finite() && (0.01..=100.).contains(&tolerance),
        "Simplify tolerance must be 0.01–100 pixels"
    );
    let [a, b, c, d, _, _] = object.transform;
    let scale = f64::from(a)
        .hypot(f64::from(b))
        .hypot(f64::from(c))
        .hypot(f64::from(d));
    let accuracy = f64::from(tolerance) / scale;
    ensure!(
        accuracy >= 1e-6,
        "Transform is too large for this simplify tolerance"
    );
    let mut result = object.clone();
    let mut distance_tests = 0usize;
    let mut fitted_anchors = 0usize;
    for sub in &mut result.path.subpaths {
        check_cancel(cancel)?;
        if sub.anchors.len() < 3 {
            continue;
        }
        if straight_subpath(sub) {
            // De Casteljau insertion gives even a straight segment nontrivial
            // handles. Treat geometrically straight cubics as lines instead
            // of asking the approximate cubic fitter to rediscover corners.
            // After a shortcut, old line handles may no longer lie on its new
            // chord; clear used handles before distance-bounded reduction.
            let mut lines = sub.clone();
            for anchor in &mut lines.anchors {
                anchor.incoming = None;
                anchor.outgoing = None;
            }
            if !sub.closed {
                lines.anchors.first_mut().unwrap().incoming = sub.anchors[0].incoming;
                lines.anchors.last_mut().unwrap().outgoing = sub.anchors.last().unwrap().outgoing;
            }
            let reduced = simplify_polyline(&lines, accuracy, &mut distance_tests, cancel)?;
            if reduced.anchors.len() < sub.anchors.len() {
                *sub = reduced;
            }
            continue;
        }
        fitted_anchors = fitted_anchors.saturating_add(sub.anchors.len());
        ensure!(
            fitted_anchors <= MAX_GEOMETRY_SEGMENTS,
            "Curve fitting supports up to 2048 anchors per object; split the path before simplifying"
        );
        let original = VectorPath {
            subpaths: vec![sub.clone()],
            fill_rule: result.path.fill_rule,
        };
        let input = to_bez(&original);
        let fitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            kurbo::simplify::simplify_bezpath(
                input.iter(),
                accuracy,
                &kurbo::simplify::SimplifyOptions::default(),
            )
        }))
        .map_err(|_| anyhow!("This path could not be simplified safely"))?;
        let candidate = from_bez(&fitted, original.fill_rule)?;
        if let [one] = candidate.subpaths.as_slice() {
            let same_ends = sub.closed
                || (one.anchors.first().map(|a| a.position)
                    == sub.anchors.first().map(|a| a.position)
                    && one.anchors.last().map(|a| a.position)
                        == sub.anchors.last().map(|a| a.position));
            if one.closed == sub.closed && one.anchors.len() <= sub.anchors.len() && same_ends {
                *sub = one.clone();
            }
        }
    }
    check_cancel(cancel)?;
    result.path.validate()?;
    Ok(result)
}

/// Only exact, forward straight segments qualify. Collinear control points
/// beyond the chord or with reversed order can encode an excursion/backtrack;
/// those remain cubic input. No tolerance-based flattening hides a small bow.
fn straight_subpath(sub: &Subpath) -> bool {
    let n = sub.anchors.len();
    (0..if sub.closed { n } else { n.saturating_sub(1) }).all(|i| {
        let start = &sub.anchors[i];
        let end = &sub.anchors[(i + 1) % n];
        let point = |p: Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
        let p0 = point(start.position);
        let p1 = point(start.outgoing.unwrap_or(start.position));
        let p2 = point(end.incoming.unwrap_or(end.position));
        let p3 = point(end.position);
        let chord = p3 - p0;
        let length = chord.hypot2();
        if length == 0. {
            return p1 == p0 && p2 == p0;
        }
        let first = p1 - p0;
        let second = p2 - p0;
        let (t1, t2) = (first.dot(chord), second.dot(chord));
        first.cross(chord) == 0.
            && second.cross(chord) == 0.
            && 0. <= t1
            && t1 <= t2
            && t2 <= length
    })
}

fn simplify_polyline(
    sub: &Subpath,
    tolerance: f64,
    work: &mut usize,
    cancel: &AtomicBool,
) -> Result<Subpath> {
    let mut points: Vec<_> = sub
        .anchors
        .iter()
        .map(|a| kurbo::Point::new(f64::from(a.position.x), f64::from(a.position.y)))
        .collect();
    let n = points.len();
    if sub.closed {
        points.push(points[0]);
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    for i in 1..n {
        if !sub.closed && i + 1 == n {
            continue;
        }
        let before = points[i] - points[(i + n - 1) % n];
        let after = points[(i + 1) % n] - points[i];
        let length = before.hypot() * after.hypot();
        if length > 0. && before.dot(after) / length < 30_f64.to_radians().cos() {
            keep[i] = true;
        }
    }
    let boundaries: Vec<_> = keep
        .iter()
        .enumerate()
        .filter_map(|(i, k)| k.then_some(i))
        .collect();
    let mut stack: Vec<_> = boundaries.windows(2).map(|w| (w[0], w[1])).collect();
    while let Some((a, b)) = stack.pop() {
        check_cancel(cancel)?;
        let chord = points[b] - points[a];
        let length_sq = chord.hypot2();
        let mut worst = (tolerance * tolerance, a);
        for i in a + 1..b {
            *work += 1;
            ensure!(
                *work <= 8_000_000,
                "Simplify distance-test budget exceeded; split the path or choose a larger tolerance"
            );
            if *work % 1024 == 0 {
                check_cancel(cancel)?;
            }
            let t = if length_sq > 0. {
                ((points[i] - points[a]).dot(chord) / length_sq).clamp(0., 1.)
            } else {
                0.
            };
            let distance = points[i].distance_squared(points[a] + chord * t);
            if distance > worst.0 {
                worst = (distance, i);
            }
        }
        if worst.1 != a {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    let anchors: Vec<_> = sub
        .anchors
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, a)| a.clone())
        .collect();
    // Retain degenerate or tiny closed paths instead of silently deleting them.
    if anchors.len() < if sub.closed { 3 } else { 2 } {
        return Ok(sub.clone());
    }
    Ok(Subpath {
        anchors,
        closed: sub.closed,
    })
}

fn cap(cap: StrokeCap) -> Cap {
    match cap {
        StrokeCap::Butt => Cap::Butt,
        StrokeCap::Round => Cap::Round,
        StrokeCap::Square => Cap::Square,
    }
}
fn join(join: StrokeJoin) -> Join {
    match join {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    }
}

fn stroke_region(
    path: &VectorPath,
    width: f32,
    options: &StrokeOptions,
    cancel: &AtomicBool,
) -> Result<Vec<VectorPath>> {
    check_cancel(cancel)?;
    path.validate()?;
    // These options are already validated in source-local units. Object scale
    // can legitimately make a world-space dash smaller than the editor's 0.1px
    // minimum, so validate physical values rather than reapplying that UI bound.
    ensure!(
        width.is_finite() && width > 0. && width <= 100_000.,
        "Invalid outline width"
    );
    ensure!(
        options.miter_limit.is_finite()
            && (1. ..=32.).contains(&options.miter_limit)
            && options.dash_offset.is_finite()
            && options.dashes.iter().all(|d| d.is_finite() && *d > 0.),
        "Invalid outline options"
    );
    let source = to_bez(path);
    if !options.dashes.is_empty() {
        // A control-polygon length bounds curve length without flattening.
        let upper: f64 = source
            .segments()
            .map(|s| {
                let c = s.to_cubic();
                c.p0.distance(c.p1) + c.p1.distance(c.p2) + c.p2.distance(c.p3)
            })
            .sum();
        let smallest = options.dashes.iter().copied().fold(f32::INFINITY, f32::min);
        ensure!(
            upper / f64::from(smallest) <= 1024.,
            "Dashed outline exceeds the work limit; increase dash lengths"
        );
    }
    let style = Stroke::new(f64::from(width))
        .with_caps(cap(options.cap))
        .with_join(join(options.join))
        .with_miter_limit(f64::from(options.miter_limit))
        .with_dashes(
            f64::from(options.dash_offset),
            options.dashes.iter().copied().map(f64::from),
        );
    let outlined = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        kurbo::stroke(
            source.iter(),
            &style,
            &StrokeOpts::default(),
            STROKE_TOLERANCE,
        )
    }))
    .map_err(|_| anyhow!("This stroke could not be outlined safely"))?;
    check_cancel(cancel)?;
    ensure!(
        outlined.elements().len() <= MAX_GEOMETRY_SEGMENTS,
        "Stroke outline exceeds the geometry work limit"
    );
    let mut path = from_bez(&outlined, FillRule::NonZero)?;
    // The stroker may omit ClosePath on open-path outlines.
    for sub in &mut path.subpaths {
        sub.closed = true;
    }
    normalize(&path, cancel)
}

/// Replace the painted stroke with editable filled contours. Source fill is
/// deliberately excluded; caller can retain a copy if both are wanted.
pub fn outline_stroke(object: &VectorObject, cancel: &AtomicBool) -> Result<Vec<VectorObject>> {
    let path = world_path(object)?;
    let stroke = object
        .stroke
        .ok_or_else(|| anyhow!("Select a path with a stroke"))?;
    let scale = stroke_scale(object)?;
    let mut options = object.stroke_options.clone().unwrap_or_default();
    options.scale(scale);
    let paths = stroke_region(&path, stroke.width * scale, &options, cancel)?;
    let mut style = object.clone();
    style.fill = Some(stroke.color);
    style.fill_gradient = None;
    style.stroke = None;
    style.stroke_options = None;
    let output = paths
        .into_iter()
        .enumerate()
        .map(|(i, p)| styled_world_result(&style, p, i))
        .collect::<Result<Vec<_>>>()?;
    check_cancel(cancel)?;
    Ok(output)
}

/// Grow or inset a closed filled region by world-space pixels. The boundary is
/// stroked then united/subtracted under the path's original fill rule. Offsets
/// are approximate parallel curves, not an exact CAD distance field.
pub fn offset_path(
    object: &VectorObject,
    distance: f32,
    cancel: &AtomicBool,
) -> Result<Vec<VectorObject>> {
    let path = world_path(object)?;
    ensure!(object.fill.is_some(), "Offset Path requires a filled path");
    ensure!(
        path.subpaths.iter().all(|s| s.closed),
        "Close open paths before using Offset Path"
    );
    ensure!(
        distance.is_finite() && distance.abs() <= 10_000.,
        "Offset must be within 10000 pixels"
    );
    let mut components = normalize(&path, cancel)?;
    if distance < 0. {
        // No point in a bounded region can be farther from its boundary than
        // half the smaller bounding-box extent. Avoid the stroker's inverted
        // inner loops once this entire region must have disappeared.
        components.retain(|component| {
            let bounds = to_bez(component).bounding_box();
            f64::from(-distance) * 2. < bounds.width().min(bounds.height())
        });
        if components.is_empty() {
            check_cancel(cancel)?;
            return Ok(Vec::new());
        }
    }
    let normalized = compound(&components);
    let paths = if distance == 0. {
        normalize(&normalized, cancel)?
    } else {
        let mut options = object.stroke_options.clone().unwrap_or_default();
        options.dashes.clear();
        options.dash_offset = 0.;
        let ring = compound(&stroke_region(
            &normalized,
            2. * distance.abs(),
            &options,
            cancel,
        )?);
        region(
            &normalized,
            &ring,
            if distance > 0. {
                RegionOperation::Union
            } else {
                RegionOperation::Subtract
            },
            cancel,
        )?
    };
    let output = paths
        .into_iter()
        .enumerate()
        .map(|(i, p)| styled_world_result(object, p, i))
        .collect::<Result<Vec<_>>>()?;
    check_cancel(cancel)?;
    Ok(output)
}
