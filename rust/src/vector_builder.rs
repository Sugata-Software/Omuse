//! Bounded Shape Builder regions for the shared vector canvas.
//!
//! Preparing regions is disposable geometry, not a document edit. Callers keep
//! original objects until accepting a complete merge/erase result as one Undo.
//! Only visible opaque fills are admitted: splitting strokes or translucent
//! objects would change the composition even in regions the user did not edit.
use crate::vector_geometry::{self as geometry, RegionOperation};
use crate::vector_path::{FillRule, Point, VectorPath};
use crate::vector_scene::VectorObject;
use anyhow::{Result, ensure};
use kurbo::{BezPath, Shape};
use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

pub const MAX_BUILDER_OBJECTS: usize = 8;
pub const MAX_BUILDER_INPUT_ANCHORS: usize = 256;
pub const MAX_BUILDER_REGIONS: usize = 64;
const MAX_OUTPUT_ANCHORS: usize = 4_096;
const MAX_SWEEPS: usize = 128;
const MAX_PROCESSED_ANCHORS: usize = 8_192;

#[derive(Clone, Debug)]
pub struct BuilderRegion {
    /// Stable within this prepared snapshot; IDs are also vector indices.
    pub id: usize,
    pub object: VectorObject,
    /// Indices of input objects covering this region, in bottom-to-top order.
    pub sources: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct ShapeBuilder {
    pub regions: Vec<BuilderRegion>,
    // Retain analytical curves for pointer hit testing, avoiding flattening or
    // raster allocation for every drag event. Region objects are read-only data
    // for callers; geometry edits require preparing a new snapshot.
    hit_paths: Vec<BezPath>,
}

#[derive(Default)]
struct Budget {
    sweeps: usize,
    anchors: usize,
}

impl Budget {
    fn region(
        &mut self,
        a: &VectorPath,
        b: &VectorPath,
        op: RegionOperation,
        cancel: &AtomicBool,
    ) -> Result<Vec<VectorPath>> {
        geometry::check_cancel(cancel)?;
        self.sweeps += 1;
        self.anchors = self
            .anchors
            .saturating_add(geometry::count_anchors(a) + geometry::count_anchors(b));
        ensure!(
            self.sweeps <= MAX_SWEEPS && self.anchors <= MAX_PROCESSED_ANCHORS,
            "Shape Builder exceeds its work budget; select fewer or simpler paths"
        );
        geometry::region(a, b, op, cancel)
    }
}

/// Partition the union of 2–8 filled objects into disjoint editable regions.
/// Overlap regions retain the last (topmost) source's world-space fill/gradient.
/// A hole is never turned into an independently filled region.
pub fn prepare(objects: &[VectorObject], cancel: &AtomicBool) -> Result<ShapeBuilder> {
    geometry::check_cancel(cancel)?;
    ensure!(
        (2..=MAX_BUILDER_OBJECTS).contains(&objects.len()),
        "Shape Builder needs 2–8 selected filled objects"
    );
    ensure!(
        objects
            .iter()
            .map(|o| geometry::count_anchors(&o.path))
            .sum::<usize>()
            <= MAX_BUILDER_INPUT_ANCHORS,
        "Shape Builder supports up to 256 input anchors; simplify or select fewer paths"
    );
    let mut budget = Budget::default();
    let mut regions: Vec<BuilderRegion> = Vec::new();
    for (source_index, object) in objects.iter().enumerate() {
        geometry::check_cancel(cancel)?;
        ensure!(
            object.stroke.is_none(),
            "Shape Builder currently needs fills without strokes; outline strokes first"
        );
        ensure!(
            object.visible
                && object.opacity == 1.
                && object.fill.is_some_and(|c| c[3] == 255)
                && object
                    .fill_gradient
                    .as_ref()
                    .is_none_or(|g| g.stops.iter().all(|s| s.color[3] == 255)),
            "Shape Builder currently needs visible, fully opaque fills and gradient stops"
        );
        let world = geometry::world_path(object)?;
        let normalized = budget.region(
            &world,
            &VectorPath::default(),
            RegionOperation::Union,
            cancel,
        )?;
        ensure!(
            !normalized.is_empty(),
            "A selected Shape Builder path has no filled area"
        );
        let rhs = geometry::compound(&normalized);
        let top_style = geometry::styled_world_result(object, rhs.clone(), 0)?;
        if regions.is_empty() {
            for path in normalized {
                append(&mut regions, path, &top_style, vec![source_index])?;
            }
            continue;
        }
        let occupied = geometry::compound(
            &regions
                .iter()
                .map(|r| r.object.path.clone())
                .collect::<Vec<_>>(),
        );
        // Include the top object's previously uncovered area, not just pieces
        // of the first object. This distinguishes Builder from Divide.
        let uncovered = budget.region(&rhs, &occupied, RegionOperation::Subtract, cancel)?;
        let mut next = Vec::new();
        for old in regions {
            for path in budget.region(&old.object.path, &rhs, RegionOperation::Subtract, cancel)? {
                append(&mut next, path, &old.object, old.sources.clone())?;
            }
            let mut sources = old.sources;
            sources.push(source_index);
            for path in budget.region(&old.object.path, &rhs, RegionOperation::Intersect, cancel)? {
                append(&mut next, path, &top_style, sources.clone())?;
            }
        }
        for path in uncovered {
            append(&mut next, path, &top_style, vec![source_index])?;
        }
        regions = next;
    }
    geometry::check_cancel(cancel)?;
    let hit_paths = regions
        .iter()
        .map(|r| geometry::to_bez(&r.object.path))
        .collect();
    Ok(ShapeBuilder { regions, hit_paths })
}

fn append(
    regions: &mut Vec<BuilderRegion>,
    path: VectorPath,
    style: &VectorObject,
    sources: Vec<usize>,
) -> Result<()> {
    ensure!(
        regions.len() < MAX_BUILDER_REGIONS,
        "Shape Builder produces more than 64 regions; use fewer paths"
    );
    let total = regions
        .iter()
        .map(|r| geometry::count_anchors(&r.object.path))
        .sum::<usize>();
    ensure!(
        total.saturating_add(geometry::count_anchors(&path)) <= MAX_OUTPUT_ANCHORS,
        "Shape Builder exceeds its output anchor budget"
    );
    let id = regions.len();
    let mut object = geometry::styled_world_result(style, path, id)?;
    // Geometry passes may split a region repeatedly. Name the final role,
    // rather than inheriting a suffix from each intermediate geometry result.
    object.name = format!("Shape · region {}", id + 1);
    regions.push(BuilderRegion {
        id,
        object,
        sources,
    });
    Ok(())
}

impl ShapeBuilder {
    /// Analytical winding over retained curves, respecting holes. Coordinates
    /// are scene/world pixels. Points outside admitted coordinates do not hit.
    pub fn hit_test(&self, point: Point) -> Option<usize> {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 1_000_000.
            || point.y.abs() > 1_000_000.
        {
            return None;
        }
        let point = kurbo::Point::new(f64::from(point.x), f64::from(point.y));
        self.regions
            .iter()
            .zip(&self.hit_paths)
            .find_map(|(region, path)| {
                let winding = path.winding(point);
                let inside = match region.object.path.fill_rule {
                    FillRule::EvenOdd => winding % 2 != 0,
                    FillRule::NonZero => winding != 0,
                };
                inside.then_some(region.id)
            })
    }

    /// Regions crossed by one pointer movement, ordered by first entry. Unlike
    /// event-position sampling, this includes narrow cells between events.
    /// Analytical intersections split the gesture into constant-winding
    /// intervals; testing their midpoints distinguishes a hole from a fill.
    pub fn hit_test_segment(&self, from: Point, to: Point) -> Result<Vec<usize>> {
        ensure!(
            [from, to].iter().all(|p| p.x.is_finite()
                && p.y.is_finite()
                && p.x.abs() <= 1_000_000.
                && p.y.abs() <= 1_000_000.),
            "Invalid Shape Builder pointer position"
        );
        if from == to {
            return Ok(self.hit_test(from).into_iter().collect());
        }
        let start = kurbo::Point::new(f64::from(from.x), f64::from(from.y));
        let end = kurbo::Point::new(f64::from(to.x), f64::from(to.y));
        let line = kurbo::Line::new(start, end);
        let span = kurbo::Rect::from_points(start, end);
        let mut hits = Vec::new();
        let mut winding_work = 0usize;
        for (region, path) in self.regions.iter().zip(&self.hit_paths) {
            let bounds = path.bounding_box();
            if bounds.x0 > span.x1
                || span.x0 > bounds.x1
                || bounds.y0 > span.y1
                || span.y0 > bounds.y1
            {
                continue;
            }
            let mut times = vec![0., 1.];
            for segment in path.segments() {
                times.extend(
                    segment
                        .intersect_line(line)
                        .into_iter()
                        .map(|crossing| crossing.line_t)
                        .filter(|t| t.is_finite() && (0. ..=1.).contains(t)),
                );
            }
            times.sort_by(f64::total_cmp);
            times.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            let inside = |point| {
                let winding = path.winding(point);
                match region.object.path.fill_rule {
                    FillRule::EvenOdd => winding % 2 != 0,
                    FillRule::NonZero => winding != 0,
                }
            };
            winding_work = winding_work.saturating_add(path.elements().len());
            ensure!(
                winding_work <= 1_000_000,
                "Shape Builder gesture exceeds hit-test budget; use a shorter drag"
            );
            if inside(start) {
                hits.push((0., region.id));
                continue;
            }
            let mut hit = false;
            for interval in times.windows(2) {
                winding_work = winding_work.saturating_add(path.elements().len());
                ensure!(
                    winding_work <= 1_000_000,
                    "Shape Builder gesture exceeds hit-test budget; use a shorter drag"
                );
                if inside(start.lerp(end, (interval[0] + interval[1]) * 0.5)) {
                    hits.push((interval[0], region.id));
                    hit = true;
                    break;
                }
            }
            if !hit && inside(end) {
                hits.push((1., region.id));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Ok(hits.into_iter().map(|(_, id)| id).collect())
    }

    fn validate_ids(&self, ids: &[usize]) -> Result<BTreeSet<usize>> {
        ensure!(!ids.is_empty(), "Paint at least one Shape Builder region");
        ensure!(
            ids.len() <= MAX_BUILDER_REGIONS && self.regions.len() <= MAX_BUILDER_REGIONS,
            "Shape Builder region limit exceeded"
        );
        for (i, region) in self.regions.iter().enumerate() {
            ensure!(
                region.id == i,
                "Shape Builder snapshot changed; prepare it again"
            );
        }
        ensure!(
            ids.iter().all(|i| *i < self.regions.len()),
            "Shape Builder region is no longer available"
        );
        Ok(ids.iter().copied().collect())
    }

    /// Return the complete region replacement, uniting painted regions using
    /// the first painted/clicked region's style. Other regions retain their fill.
    /// Duplicate painted IDs do not create duplicate geometry.
    pub fn merge(&self, ids: &[usize], cancel: &AtomicBool) -> Result<Vec<VectorObject>> {
        geometry::check_cancel(cancel)?;
        let chosen = self.validate_ids(ids)?;
        let first = &self.regions[ids[0]].object;
        let mut paths = vec![first.path.clone()];
        let mut budget = Budget::default();
        for id in chosen.iter().copied().filter(|i| *i != ids[0]) {
            paths = budget.region(
                &geometry::compound(&paths),
                &self.regions[id].object.path,
                RegionOperation::Union,
                cancel,
            )?;
        }
        let mut result: Vec<_> = self
            .regions
            .iter()
            .filter(|r| !chosen.contains(&r.id))
            .map(|r| r.object.clone())
            .collect();
        let merged_parts = paths.len();
        for (i, path) in paths.into_iter().enumerate() {
            let mut object = geometry::styled_world_result(first, path, i)?;
            object.name = if merged_parts == 1 {
                "Merged shape".to_owned()
            } else {
                format!("Merged shape · part {}", i + 1)
            };
            result.push(object);
        }
        ensure!(
            result.len() <= MAX_BUILDER_REGIONS,
            "Merged result exceeds region limit"
        );
        ensure!(
            result
                .iter()
                .map(|o| geometry::count_anchors(&o.path))
                .sum::<usize>()
                <= MAX_OUTPUT_ANCHORS,
            "Merged result exceeds anchor limit"
        );
        geometry::check_cancel(cancel)?;
        Ok(result)
    }

    /// Return every unpainted region. Empty output intentionally means erase
    /// all selected source artwork; the caller handles its empty-scene contract.
    pub fn erase(&self, ids: &[usize], cancel: &AtomicBool) -> Result<Vec<VectorObject>> {
        geometry::check_cancel(cancel)?;
        let chosen = self.validate_ids(ids)?;
        let result = self
            .regions
            .iter()
            .filter(|r| !chosen.contains(&r.id))
            .map(|r| r.object.clone())
            .collect();
        geometry::check_cancel(cancel)?;
        Ok(result)
    }
}
