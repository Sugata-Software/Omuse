//! Opt-in Pen construction contacts. Inputs and outputs share canvas-world
//! coordinates. Cubic tangencies solve the bounded quartic exactly enough for
//! a geometric residual check; no view-dependent flattening invents a tangent.
use super::{check_cancel, map_point, valid_point, validate_transform};
use crate::{vector_path::Point, vector_scene::VectorScene};
use anyhow::{Result, ensure};
use std::{collections::HashSet, sync::atomic::AtomicBool};

pub const MAX_CONTACT_SEGMENTS: usize = 4_096;
pub const MAX_CONTACT_CANDIDATES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentId {
    pub object: usize,
    pub subpath: usize,
    pub segment: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSegment {
    pub id: SegmentId,
    pub controls: [Point; 4],
    pub curved: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContactOptions {
    pub tangent: bool,
    pub perpendicular: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContactKind {
    Perpendicular,
    Tangent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactResult {
    pub point: Point,
    pub from: Point,
    pub direction: Point,
    pub id: SegmentId,
    pub kind: ContactKind,
    pub examined_segments: usize,
    pub candidates: usize,
}

#[derive(Clone, Debug)]
pub struct ContactIndex {
    segments: Vec<(ContactSegment, [f32; 4])>,
}

impl ContactIndex {
    pub fn build(segments: &[ContactSegment], cancel: &AtomicBool) -> Result<Self> {
        check_cancel(cancel)?;
        ensure!(
            segments.len() <= MAX_CONTACT_SEGMENTS,
            "Contact snapping supports at most4096 visible target segments"
        );
        let mut ids = HashSet::new();
        let mut indexed = Vec::with_capacity(segments.len());
        for segment in segments {
            check_cancel(cancel)?;
            ensure!(ids.insert(segment.id), "Duplicate contact segment identity");
            ensure!(
                segment.controls.into_iter().all(valid_point),
                "Invalid contact coordinates"
            );
            let bounds = segment.controls.iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |a, p| [a[0].min(p.x), a[1].min(p.y), a[2].max(p.x), a[3].max(p.y)],
            );
            indexed.push((*segment, bounds));
        }
        indexed.sort_by_key(|(s, _)| s.id);
        Ok(Self { segments: indexed })
    }

    pub fn from_scene(
        scene: &VectorScene,
        scene_to_world: [f32; 6],
        cancel: &AtomicBool,
    ) -> Result<Self> {
        scene.validate()?;
        validate_transform(scene_to_world)?;
        let mut targets = Vec::new();
        for (object_index, object) in scene.objects.iter().enumerate() {
            check_cancel(cancel)?;
            if !object.visible || object.opacity == 0. {
                continue;
            }
            validate_transform(object.transform)?;
            for (subpath_index, subpath) in object.path.subpaths.iter().enumerate() {
                let n = subpath.anchors.len();
                for segment in 0..if subpath.closed {
                    n
                } else {
                    n.saturating_sub(1)
                } {
                    check_cancel(cancel)?;
                    ensure!(
                        targets.len() < MAX_CONTACT_SEGMENTS,
                        "Contact snapping supports at most4096 visible target segments"
                    );
                    let a = &subpath.anchors[segment];
                    let b = &subpath.anchors[(segment + 1) % n];
                    let mut controls = [
                        a.position,
                        a.outgoing.unwrap_or(a.position),
                        b.incoming.unwrap_or(b.position),
                        b.position,
                    ];
                    for p in &mut controls {
                        *p = map_point(scene_to_world, map_point(object.transform, *p)?)?;
                    }
                    targets.push(ContactSegment {
                        id: SegmentId {
                            object: object_index,
                            subpath: subpath_index,
                            segment,
                        },
                        controls,
                        curved: a.outgoing.is_some() || b.incoming.is_some(),
                    });
                }
            }
        }
        Self::build(&targets, cancel)
    }

    /// A previous Pen anchor is fixed. Snap its next endpoint onto a straight
    /// segment's perpendicular foot or a cubic's tangent contact. Busy queries
    /// fail before root solving, never return a partially searched result.
    pub fn resolve(
        &self,
        from: Point,
        pointer: Point,
        zoom: f32,
        tolerance_px: f32,
        options: ContactOptions,
        cancel: &AtomicBool,
    ) -> Result<Option<ContactResult>> {
        check_cancel(cancel)?;
        ensure!(
            valid_point(from) && valid_point(pointer),
            "Invalid contact query"
        );
        ensure!(
            zoom.is_finite() && (0.02..=16.).contains(&zoom),
            "Invalid contact zoom"
        );
        ensure!(
            tolerance_px.is_finite() && (0. ..=32.).contains(&tolerance_px),
            "Invalid contact tolerance"
        );
        if !options.tangent && !options.perpendicular {
            return Ok(None);
        }
        let radius = f64::from(tolerance_px / zoom);
        let mut candidates = Vec::new();
        for (segment, bounds) in &self.segments {
            check_cancel(cancel)?;
            if (segment.curved && !options.tangent) || (!segment.curved && !options.perpendicular) {
                continue;
            }
            if f64::from(pointer.x) + radius < f64::from(bounds[0])
                || f64::from(pointer.x) - radius > f64::from(bounds[2])
                || f64::from(pointer.y) + radius < f64::from(bounds[1])
                || f64::from(pointer.y) - radius > f64::from(bounds[3])
            {
                continue;
            }
            candidates.push(segment);
            ensure!(
                candidates.len() <= MAX_CONTACT_CANDIDATES,
                "Contact neighborhood exceeds128 candidates; zoom in or disable contacts"
            );
        }
        let mut best: Option<(f64, ContactResult)> = None;
        for segment in &candidates {
            check_cancel(cancel)?;
            let p = segment.controls.map(|p| [f64::from(p.x), f64::from(p.y)]);
            let from64 = [f64::from(from.x), f64::from(from.y)];
            let mut contacts = Vec::new();
            if !segment.curved {
                let direction = sub(p[3], p[0]);
                let length = dot(direction, direction);
                if length <= 1e-12 {
                    continue;
                }
                let t = dot(sub(from64, p[0]), direction) / length;
                if (0. ..=1.).contains(&t) {
                    contacts.push((
                        add(p[0], mul(direction, t)),
                        direction,
                        ContactKind::Perpendicular,
                    ));
                }
            } else {
                let d = sub(p[0], from64);
                let c = mul(sub(p[1], p[0]), 3.);
                let b = mul(add(sub(p[2], mul(p[1], 2.)), p[0]), 3.);
                let a = add(sub(p[3], mul(p[2], 3.)), sub(mul(p[1], 3.), p[0]));
                let coeff = [
                    cross(d, c),
                    2. * cross(d, b),
                    3. * cross(d, a) + cross(c, b),
                    2. * cross(c, a),
                    cross(b, a),
                ];
                let scale = coeff.into_iter().map(f64::abs).fold(0., f64::max);
                if scale <= 1e-18 {
                    continue;
                }
                let coeff = coeff.map(|v| v / scale);
                for t in
                    kurbo::common::solve_quartic(coeff[0], coeff[1], coeff[2], coeff[3], coeff[4])
                {
                    if !t.is_finite() || !(-1e-9..=1. + 1e-9).contains(&t) {
                        continue;
                    }
                    let t = t.clamp(0., 1.);
                    let point = add(add(add(p[0], mul(c, t)), mul(b, t * t)), mul(a, t * t * t));
                    let derivative = add(c, add(mul(b, 2. * t), mul(a, 3. * t * t)));
                    let ray = sub(point, from64);
                    let magnitude = (dot(ray, ray) * dot(derivative, derivative)).sqrt();
                    if magnitude > 1e-10 && cross(ray, derivative).abs() <= magnitude * 1e-6 {
                        contacts.push((point, derivative, ContactKind::Tangent));
                    }
                }
            }
            for (point, direction, kind) in contacts {
                let delta = [
                    point[0] - f64::from(pointer.x),
                    point[1] - f64::from(pointer.y),
                ];
                let distance = dot(delta, delta);
                let norm = dot(direction, direction).sqrt();
                if distance > radius * radius
                    || norm <= 1e-10
                    || dot(sub(point, from64), sub(point, from64)) <= 1e-12
                {
                    continue;
                }
                let result = ContactResult {
                    point: Point {
                        x: point[0] as f32,
                        y: point[1] as f32,
                    },
                    from,
                    direction: Point {
                        x: (direction[0] / norm) as f32,
                        y: (direction[1] / norm) as f32,
                    },
                    id: segment.id,
                    kind,
                    examined_segments: self.segments.len(),
                    candidates: candidates.len(),
                };
                if !valid_point(result.point) {
                    continue;
                }
                if best.as_ref().is_none_or(|(old, previous)| {
                    distance < *old
                        || (distance == *old
                            && (result.id, result.kind) < (previous.id, previous.kind))
                }) {
                    best = Some((distance, result));
                }
            }
        }
        Ok(best.map(|(_, result)| result))
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: [f64; 2], n: f64) -> [f64; 2] {
    [a[0] * n, a[1] * n]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
