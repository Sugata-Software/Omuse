//! Reusable repeat settings expanded into editable vector snapshot copies.
//!
//! This module does not add a persistent live-repeat document object. The exact
//! source and spec remain in the returned snapshot for a caller's disposable
//! preview/re-generation; accepting its objects is an ordinary scene edit.
use crate::asset_library::sha256_hex;
use crate::vector_path::Point;
use crate::vector_scene::{
    MAX_SCENE_ANCHORS, MAX_SCENE_OBJECTS, MAX_SCENE_SUBPATHS, VECTOR_SCENE_VERSION, VectorObject,
    VectorScene,
};
use anyhow::{Context, Result, ensure};
use kurbo::Shape;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, Write};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_REPEAT_SOURCES: usize = 64;
pub const MAX_REPEAT_INSTANCES: u32 = 256;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Stable first-generation repeat semantics. Counts include the original,
/// unmodified motif at instance zero. Steps/pivots are in scene/world pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RepeatSpec {
    /// Instances are ordered row-major. General translation vectors allow
    /// staggered/skewed grids without distorting any source shape.
    Grid {
        columns: u32,
        rows: u32,
        column_step: Point,
        row_step: Point,
    },
    /// Positive angles rotate clockwise in canvas coordinates. With rotation
    /// disabled, the whole motif translates around its geometric bounds centre
    /// and each copy retains its original orientation and internal spacing.
    Radial {
        count: u32,
        center: Point,
        angle_step_degrees: f32,
        rotate_copies: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepeatSnapshot {
    pub spec: RepeatSpec,
    /// A caller-created UUID. Reuse it while editing one repeat preview; use a
    /// fresh namespace for a separate repeat added to the same document.
    pub namespace: String,
    /// Exact originals, retained separately for non-destructive re-generation.
    pub source: Vec<VectorObject>,
    /// Includes unchanged originals first, followed by editable snapshot copies.
    pub objects: Vec<VectorObject>,
    pub instance_ranges: Vec<Range<usize>>,
}

impl RepeatSpec {
    pub fn instance_count(&self) -> Result<u32> {
        let valid_point = |p: Point| {
            p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1_000_000. && p.y.abs() <= 1_000_000.
        };
        let count = match *self {
            Self::Grid {
                columns,
                rows,
                column_step,
                row_step,
            } => {
                ensure!(
                    columns > 0 && rows > 0,
                    "Grid needs at least one row and column"
                );
                ensure!(
                    valid_point(column_step) && valid_point(row_step),
                    "Grid steps must be finite scene coordinates"
                );
                columns.checked_mul(rows).context("Grid count overflow")?
            }
            Self::Radial {
                count,
                center,
                angle_step_degrees,
                ..
            } => {
                ensure!(
                    valid_point(center),
                    "Repeat centre must be a finite scene coordinate"
                );
                ensure!(
                    angle_step_degrees.is_finite() && angle_step_degrees.abs() <= 360.,
                    "Radial angle step must be within −360 to 360 degrees"
                );
                count
            }
        };
        ensure!(
            (1..=MAX_REPEAT_INSTANCES).contains(&count),
            "Repeat supports 1–256 instances including the original"
        );
        Ok(count)
    }
}

/// Generate deterministic, editable copies without changing any input. All
/// output objects/anchors/subpaths fit the existing scene storage limits, and
/// serialised source/output size budgets bound duplicated text and style data.
/// The caller validates insertion into its complete scene and owns one Undo.
pub fn generate(
    sources: &[VectorObject],
    spec: &RepeatSpec,
    namespace: &str,
    cancel: &AtomicBool,
) -> Result<RepeatSnapshot> {
    check_cancel(cancel)?;
    ensure!(
        !sources.is_empty() && sources.len() <= MAX_REPEAT_SOURCES,
        "Select 1–64 source objects for Repeat"
    );
    let count = spec.instance_count()?;
    let namespace = uuid::Uuid::parse_str(namespace).context("Repeat namespace must be a UUID")?;
    let count_usize = count as usize;
    ensure!(
        sources
            .len()
            .checked_mul(count_usize)
            .is_some_and(|n| n <= MAX_SCENE_OBJECTS),
        "Repeat exceeds the 1024-object scene limit"
    );
    let anchors = sources
        .iter()
        .flat_map(|o| &o.path.subpaths)
        .map(|s| s.anchors.len())
        .try_fold(0usize, usize::checked_add)
        .context("Repeat anchor count overflow")?;
    let subpaths = sources
        .iter()
        .map(|o| o.path.subpaths.len())
        .try_fold(0usize, usize::checked_add)
        .context("Repeat subpath count overflow")?;
    ensure!(
        anchors
            .checked_mul(count_usize)
            .is_some_and(|n| n <= MAX_SCENE_ANCHORS),
        "Repeat exceeds the 100000-anchor scene limit"
    );
    ensure!(
        subpaths
            .checked_mul(count_usize)
            .is_some_and(|n| n <= MAX_SCENE_SUBPATHS),
        "Repeat exceeds the 4096-subpath scene limit"
    );
    let source_bytes = serialized_size(sources, MAX_SOURCE_BYTES, cancel)?;
    // UUIDs may expand a compact input spelling. Names and paints are retained
    // exactly, so reserve a small fixed allowance per object and group identity.
    let identity_allowance = sources
        .iter()
        .map(|o| 64 * (1 + o.groups.len()))
        .sum::<usize>();
    ensure!(
        source_bytes
            .checked_add(identity_allowance)
            .and_then(|n| n.checked_mul(count_usize))
            .is_some_and(|n| n <= MAX_OUTPUT_BYTES),
        "Repeat exceeds the 16 MiB snapshot-size budget"
    );
    let validated_source = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 1,
        height: 1,
        objects: sources.to_vec(),
    };
    validated_source
        .validate()
        .context("Invalid repeat source artwork")?;
    check_cancel(cancel)?;
    let motif_center = motif_center(sources)?;
    let source_groups: HashSet<_> = sources
        .iter()
        .flat_map(|o| &o.groups)
        .map(|g| uuid::Uuid::parse_str(&g.id))
        .collect::<std::result::Result<_, _>>()?;
    let mut objects = Vec::with_capacity(sources.len() * count_usize);
    let mut instance_ranges = Vec::with_capacity(count_usize);
    for instance in 0..count {
        check_cancel(cancel)?;
        let start = objects.len();
        let placement = placement(spec, instance, motif_center);
        for source in sources {
            check_cancel(cancel)?;
            let mut object = source.clone();
            if instance != 0 {
                object.id = derived_id(namespace, &source.id, instance, b"object")?.to_string();
                object.transform = compose(placement, source.transform)?;
                for group in &mut object.groups {
                    let id = derived_id(namespace, &group.id, instance, b"group")?;
                    ensure!(
                        !source_groups.contains(&id),
                        "Repeat group identity collides with an original group"
                    );
                    group.id = id.to_string();
                }
            }
            objects.push(object);
        }
        instance_ranges.push(start..objects.len());
    }
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 1,
        height: 1,
        objects,
    };
    scene
        .validate()
        .context("Repeat copies exceed scene geometry or identity bounds")?;
    serialized_size(&scene.objects, MAX_OUTPUT_BYTES, cancel)?;
    check_cancel(cancel)?;
    Ok(RepeatSnapshot {
        spec: spec.clone(),
        namespace: namespace.to_string(),
        source: validated_source.objects,
        objects: scene.objects,
        instance_ranges,
    })
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Vector repeat cancelled");
    Ok(())
}

fn motif_center(sources: &[VectorObject]) -> Result<kurbo::Point> {
    let mut bounds: Option<kurbo::Rect> = None;
    for source in sources {
        if source.path.subpaths.iter().all(|s| s.anchors.is_empty()) {
            continue;
        }
        let path = crate::vector_geometry::to_bez(&source.path);
        let world = kurbo::Affine::new(source.transform.map(f64::from)) * path;
        let rect = world.bounding_box();
        bounds = Some(bounds.map_or(rect, |old| old.union(rect)));
    }
    let bounds = bounds.context("Select artwork with geometry before using Repeat")?;
    Ok(bounds.center())
}

fn placement(spec: &RepeatSpec, instance: u32, motif: kurbo::Point) -> [f64; 6] {
    match *spec {
        RepeatSpec::Grid {
            columns,
            column_step,
            row_step,
            ..
        } => {
            let (x, y) = (f64::from(instance % columns), f64::from(instance / columns));
            [
                1.,
                0.,
                0.,
                1.,
                x * f64::from(column_step.x) + y * f64::from(row_step.x),
                x * f64::from(column_step.y) + y * f64::from(row_step.y),
            ]
        }
        RepeatSpec::Radial {
            center,
            angle_step_degrees,
            rotate_copies,
            ..
        } => {
            let angle = (f64::from(angle_step_degrees) * f64::from(instance)).rem_euclid(360.);
            // Exact quadrant transforms avoid artificial 6e-17 shear terms.
            let (sin, cos) = if (angle / 90. - (angle / 90.).round()).abs() < 1e-12 {
                match (angle / 90.).round() as i32 % 4 {
                    0 => (0., 1.),
                    1 => (1., 0.),
                    2 => (0., -1.),
                    _ => (-1., 0.),
                }
            } else {
                angle.to_radians().sin_cos()
            };
            let (cx, cy) = (f64::from(center.x), f64::from(center.y));
            if rotate_copies {
                [
                    cos,
                    sin,
                    -sin,
                    cos,
                    cx - cos * cx + sin * cy,
                    cy - sin * cx - cos * cy,
                ]
            } else {
                let (dx, dy) = (motif.x - cx, motif.y - cy);
                [
                    1.,
                    0.,
                    0.,
                    1.,
                    cx + cos * dx - sin * dy - motif.x,
                    cy + sin * dx + cos * dy - motif.y,
                ]
            }
        }
    }
}

fn compose(left: [f64; 6], right: [f32; 6]) -> Result<[f32; 6]> {
    let [a, b, c, d, e, f] = left;
    let [g, h, i, j, k, l] = right.map(f64::from);
    let values = [
        a * g + c * h,
        b * g + d * h,
        a * i + c * j,
        b * i + d * j,
        a * k + c * l + e,
        b * k + d * l + f,
    ];
    ensure!(
        values
            .iter()
            .all(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX)),
        "Repeat transform exceeds finite coordinates"
    );
    Ok(values.map(|v| v as f32))
}

fn derived_id(
    namespace: uuid::Uuid,
    source: &str,
    instance: u32,
    kind: &[u8],
) -> Result<uuid::Uuid> {
    let source = uuid::Uuid::parse_str(source).context("Invalid repeat source identity")?;
    let mut input = b"omuse.repeat.snapshot.v1\0".to_vec();
    input.extend(namespace.as_bytes());
    input.extend(kind);
    input.push(0);
    input.extend(source.as_bytes());
    input.extend(instance.to_be_bytes());
    let digest = sha256_hex(&input);
    let mut bytes = *uuid::Uuid::parse_str(&digest[..32])?.as_bytes();
    // RFC 9562 custom UUID version 8, derived from the first 128 hash bits.
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes))
}

fn serialized_size<T: Serialize + ?Sized>(
    value: &T,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<usize> {
    struct Counter<'a> {
        bytes: usize,
        limit: usize,
        cancel: &'a AtomicBool,
    }
    impl Write for Counter<'_> {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(io::Error::other("Vector repeat cancelled"));
            }
            let next = self
                .bytes
                .checked_add(data.len())
                .ok_or_else(|| io::Error::other("Repeat size overflow"))?;
            if next > self.limit {
                return Err(io::Error::other("Repeat exceeds snapshot-size budget"));
            }
            self.bytes = next;
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        bytes: 0,
        limit,
        cancel,
    };
    serde_json::to_writer(&mut counter, value).context("Cannot prepare bounded repeat snapshot")?;
    Ok(counter.bytes)
}
