//! Bounded fill-rule-aware boolean operations that retain Bézier segments.

use crate::vector_geometry::{
    MAX_GEOMETRY_OUTPUT_ANCHORS, MAX_GEOMETRY_SEGMENTS, RegionOperation, check_cancel, compound,
    count_anchors, normalize, region, styled_world_result, world_path,
};
use crate::vector_path::VectorPath;
use crate::vector_scene::VectorObject;
use anyhow::{Result, ensure};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOperation {
    Union,
    Subtract,
    Intersect,
    Exclude,
    Divide,
}

/// Combines selected filled objects without modifying their inputs. The first
/// object supplies the result style. Output geometry is world-space, with an
/// identity transform, so the caller's existing one-Undo transaction is intact.
/// Curves are split at intersections/extrema, never flattened into polygons.
pub fn combine(
    objects: &[VectorObject],
    operation: BooleanOperation,
    cancel: &AtomicBool,
) -> Result<Vec<VectorObject>> {
    check_cancel(cancel)?;
    ensure!(!objects.is_empty(), "Select at least one vector object");
    ensure!(
        objects.len() <= 256,
        "Too many vector objects for one boolean operation"
    );
    let total: usize = objects.iter().map(|o| count_anchors(&o.path)).sum();
    ensure!(
        total <= MAX_GEOMETRY_SEGMENTS,
        "Boolean input exceeds the 2048-segment work limit; combine fewer objects"
    );
    let mut normalized = Vec::with_capacity(objects.len());
    for object in objects {
        check_cancel(cancel)?;
        ensure!(
            object.fill.is_some(),
            "Boolean operations require filled paths"
        );
        let world = world_path(object)?;
        ensure!(
            world.subpaths.iter().all(|s| s.closed),
            "Open paths cannot be used in boolean operations"
        );
        normalized.push(normalize(&world, cancel)?);
    }
    let mut result = normalized[0].clone();
    // Bound aggregate work across repeated Divide partitions as well as each
    // individual sweep; no partial pieces are returned on limit/cancel/error.
    let mut work = 0usize;
    for rhs in normalized.iter().skip(1) {
        check_cancel(cancel)?;
        let rhs = compound(rhs);
        if operation == BooleanOperation::Divide {
            let mut pieces = Vec::new();
            for subject in result {
                work = work.saturating_add(count_anchors(&subject) + count_anchors(&rhs));
                ensure!(work <= 65_536, "Divide exceeds the geometry work limit");
                pieces.extend(region(&subject, &rhs, RegionOperation::Subtract, cancel)?);
                pieces.extend(region(&subject, &rhs, RegionOperation::Intersect, cancel)?);
                ensure_limits(&pieces)?;
            }
            result = pieces;
        } else {
            let subject = compound(&result);
            work = work.saturating_add(count_anchors(&subject) + count_anchors(&rhs));
            ensure!(
                work <= 65_536,
                "Boolean operation exceeds the geometry work limit"
            );
            let rule = match operation {
                BooleanOperation::Union => RegionOperation::Union,
                BooleanOperation::Subtract => RegionOperation::Subtract,
                BooleanOperation::Intersect => RegionOperation::Intersect,
                BooleanOperation::Exclude => RegionOperation::Exclude,
                BooleanOperation::Divide => {
                    return Err(anyhow::anyhow!("Invalid boolean operation"));
                }
            };
            result = region(&subject, &rhs, rule, cancel)?;
        }
        ensure_limits(&result)?;
    }
    let output = result
        .into_iter()
        .enumerate()
        .map(|(index, path)| styled_world_result(&objects[0], path, index))
        .collect::<Result<Vec<_>>>()?;
    check_cancel(cancel)?;
    Ok(output)
}

fn ensure_limits(paths: &[VectorPath]) -> Result<()> {
    ensure!(paths.len() <= 1_024, "Boolean output has too many shapes");
    ensure!(
        paths.iter().map(count_anchors).sum::<usize>() <= MAX_GEOMETRY_OUTPUT_ANCHORS,
        "Boolean output exceeds anchor budget"
    );
    Ok(())
}
