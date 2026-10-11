//! Bounded, cached snapping for vector gestures in one world coordinate space.
//!
//! Build once from the immutable gesture source, excluding moving anchors.
//! Query points, guides and grid origins must use that same space. `zoom` is
//! screen pixels per world pixel; nonuniformly transformed layers must first
//! map their points into canvas space. Modifier/angle constraints and cache
//! invalidation belong to the caller, not to this geometry resolver.
use crate::canvas_grid::GridSettings;
use crate::editor::GuideAxis;
use crate::vector_path::Point;
use crate::vector_scene::{MAX_SCENE_OBJECTS, MAX_SCENE_SUBPATHS, VectorScene};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_SNAP_ANCHORS: usize = 100_000;
pub const MAX_SNAP_GUIDES: usize = 4_096;
pub const MAX_QUERY_CANDIDATES: usize = 4_096;
pub const MAX_QUERY_BUCKETS: usize = 4_096;
const CELL_SIZE: f64 = 64.;
const MAX_COORDINATE: f32 = 1_000_000.;

/// Indices refer to one immutable scene snapshot, not the changing draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnchorId {
    pub object: usize,
    pub subpath: usize,
    pub anchor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorTarget {
    pub id: AnchorId,
    pub point: Point,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    /// Stable caller-owned identity, for example the source guide list index.
    pub id: usize,
    pub axis: GuideAxis,
    pub position: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGrid {
    pub settings: GridSettings,
    pub origin: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisSnapKind {
    Guide(usize),
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisSnap {
    pub coordinate: f32,
    pub kind: AxisSnapKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapResult {
    pub point: Point,
    /// An anchor snaps both coordinates. It takes priority over all axis snaps.
    pub anchor: Option<AnchorId>,
    pub vertical: Option<AxisSnap>,
    pub horizontal: Option<AxisSnap>,
    /// Bounded query work, useful for diagnostics without wall-time claims.
    pub examined_anchors: usize,
}

#[derive(Clone, Debug)]
pub struct SnapIndex {
    buckets: BTreeMap<(i32, i32), Vec<AnchorTarget>>,
    vertical: Vec<SnapGuide>,
    horizontal: Vec<SnapGuide>,
    grid: Option<SnapGrid>,
    anchor_count: usize,
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Vector snap preparation cancelled"
    );
    Ok(())
}

fn valid_coordinate(value: f32) -> bool {
    value.is_finite() && value.abs() <= MAX_COORDINATE
}

fn valid_point(point: Point) -> bool {
    valid_coordinate(point.x) && valid_coordinate(point.y)
}

fn cell(value: f64) -> i32 {
    (value / CELL_SIZE).floor() as i32
}

impl SnapIndex {
    /// Index at most 100,000 anchors and 4,096 guides. Coincident targets are
    /// coalesced after exclusion, retaining the lowest identity. Bucket sorting
    /// is bounded but not internally interruptible; cancellation is checked
    /// between buckets and before any completed index is returned.
    pub fn build(
        targets: &[AnchorTarget],
        guides: &[SnapGuide],
        grid: Option<SnapGrid>,
        excluded: &[AnchorId],
        cancel: &AtomicBool,
    ) -> Result<Self> {
        check_cancel(cancel)?;
        ensure!(
            targets.len() <= MAX_SNAP_ANCHORS,
            "Snap source exceeds 100000 anchors"
        );
        ensure!(
            excluded.len() <= MAX_SNAP_ANCHORS,
            "Snap exclusions exceed 100000 anchors"
        );
        ensure!(
            guides.len() <= MAX_SNAP_GUIDES,
            "Snap source exceeds 4096 guides"
        );
        if let Some(grid) = grid {
            grid.settings.validate()?;
            ensure!(
                valid_point(grid.origin),
                "Snap grid origin must be finite world coordinates"
            );
        }
        let mut exclusion = HashSet::with_capacity(excluded.len());
        for (i, id) in excluded.iter().enumerate() {
            if i % 256 == 0 {
                check_cancel(cancel)?;
            }
            exclusion.insert(*id);
        }
        let mut identities = HashSet::with_capacity(targets.len());
        let mut buckets: BTreeMap<(i32, i32), Vec<AnchorTarget>> = BTreeMap::new();
        for (i, target) in targets.iter().enumerate() {
            if i % 256 == 0 {
                check_cancel(cancel)?;
            }
            ensure!(
                valid_point(target.point),
                "Snap anchor must be finite world coordinates"
            );
            ensure!(
                identities.insert(target.id),
                "Snap anchor identities must be unique"
            );
            if exclusion.contains(&target.id) {
                continue;
            }
            let mut target = *target;
            // One ordering for negative/positive zero keeps coincident ties stable.
            if target.point.x == 0. {
                target.point.x = 0.;
            }
            if target.point.y == 0. {
                target.point.y = 0.;
            }
            buckets
                .entry((
                    cell(f64::from(target.point.x)),
                    cell(f64::from(target.point.y)),
                ))
                .or_default()
                .push(target);
        }
        let mut anchor_count = 0;
        for bucket in buckets.values_mut() {
            check_cancel(cancel)?;
            bucket.sort_unstable_by(|a, b| {
                a.point
                    .x
                    .total_cmp(&b.point.x)
                    .then(a.point.y.total_cmp(&b.point.y))
                    .then(a.id.cmp(&b.id))
            });
            bucket.dedup_by(|a, b| a.point == b.point);
            anchor_count += bucket.len();
        }
        let mut guide_ids = HashSet::with_capacity(guides.len());
        let (mut vertical, mut horizontal) = (Vec::new(), Vec::new());
        for guide in guides {
            check_cancel(cancel)?;
            ensure!(
                valid_coordinate(guide.position),
                "Snap guide must have a finite world position"
            );
            ensure!(
                guide_ids.insert(guide.id),
                "Snap guide identities must be unique"
            );
            let mut guide = *guide;
            if guide.position == 0. {
                guide.position = 0.;
            }
            match guide.axis {
                GuideAxis::Vertical => vertical.push(guide),
                GuideAxis::Horizontal => horizontal.push(guide),
            }
        }
        for axis in [&mut vertical, &mut horizontal] {
            axis.sort_unstable_by(|a, b| a.position.total_cmp(&b.position).then(a.id.cmp(&b.id)));
            axis.dedup_by(|a, b| a.position == b.position);
        }
        check_cancel(cancel)?;
        Ok(Self {
            buckets,
            vertical,
            horizontal,
            grid,
            anchor_count,
        })
    }

    /// Collect visible nontransparent objects' anchors, applying object then
    /// scene-to-world affine transforms. This validates geometry used by the
    /// cache, not the scene's unrelated paint/text/document semantics.
    pub fn from_scene(
        scene: &VectorScene,
        scene_to_world: [f32; 6],
        guides: &[SnapGuide],
        grid: Option<SnapGrid>,
        excluded: &[AnchorId],
        cancel: &AtomicBool,
    ) -> Result<Self> {
        check_cancel(cancel)?;
        ensure!(
            scene.objects.len() <= MAX_SCENE_OBJECTS,
            "Snap scene exceeds object limit"
        );
        validate_transform(scene_to_world)?;
        let mut targets = Vec::new();
        let (mut anchors, mut subpaths) = (0usize, 0usize);
        for (object_index, object) in scene.objects.iter().enumerate() {
            check_cancel(cancel)?;
            validate_transform(object.transform)?;
            subpaths = subpaths.saturating_add(object.path.subpaths.len());
            ensure!(
                subpaths <= MAX_SCENE_SUBPATHS,
                "Snap scene exceeds subpath limit"
            );
            ensure!(
                object.opacity.is_finite() && (0. ..=1.).contains(&object.opacity),
                "Invalid snap object opacity"
            );
            for (subpath_index, subpath) in object.path.subpaths.iter().enumerate() {
                anchors = anchors.saturating_add(subpath.anchors.len());
                ensure!(
                    anchors <= MAX_SNAP_ANCHORS,
                    "Snap scene exceeds 100000 anchors"
                );
                for (anchor_index, anchor) in subpath.anchors.iter().enumerate() {
                    if anchor_index % 256 == 0 {
                        check_cancel(cancel)?;
                    }
                    ensure!(valid_point(anchor.position), "Invalid snap source anchor");
                    if !object.visible || object.opacity == 0. {
                        continue;
                    }
                    let point = map_point(
                        scene_to_world,
                        map_point(object.transform, anchor.position)?,
                    )?;
                    targets.push(AnchorTarget {
                        id: AnchorId {
                            object: object_index,
                            subpath: subpath_index,
                            anchor: anchor_index,
                        },
                        point,
                    });
                }
            }
        }
        Self::build(&targets, guides, grid, excluded, cancel)
    }

    /// Number of targets after exclusions and coincident-target coalescing.
    pub fn anchor_count(&self) -> usize {
        self.anchor_count
    }

    /// Query within 0–32 screen pixels at the canvas's 2%–1600% zoom range.
    /// Full anchor pairs win; otherwise each axis chooses its nearest guide or
    /// grid line. Equal-distance anchors use lowest AnchorId; equal-distance
    /// axis snaps prefer guides, then lowest guide id. Grid half-step ties round
    /// away from the configured origin. Over-budget queries fail atomically;
    /// the caller may then keep the unsnapped gesture point and show a notice.
    pub fn resolve(&self, point: Point, zoom: f32, tolerance_px: f32) -> Result<SnapResult> {
        ensure!(
            valid_point(point),
            "Snap query must be finite world coordinates"
        );
        ensure!(
            zoom.is_finite() && (0.02..=16.).contains(&zoom),
            "Snap zoom must be within 2%–1600%"
        );
        ensure!(
            tolerance_px.is_finite() && (0. ..=32.).contains(&tolerance_px),
            "Snap tolerance must be 0–32 screen pixels"
        );
        let radius = f64::from(tolerance_px) / f64::from(zoom);
        let (x, y) = (f64::from(point.x), f64::from(point.y));
        let (left, right, top, bottom) = (
            cell(x - radius),
            cell(x + radius),
            cell(y - radius),
            cell(y + radius),
        );
        ensure!(
            (i64::from(right) - i64::from(left) + 1) * (i64::from(bottom) - i64::from(top) + 1)
                <= MAX_QUERY_BUCKETS as i64,
            "Snap query exceeds bucket-work limit"
        );
        let mut best: Option<(f64, AnchorTarget)> = None;
        let mut examined = 0;
        for bx in left..=right {
            for by in top..=bottom {
                let Some(bucket) = self.buckets.get(&(bx, by)) else {
                    continue;
                };
                let start = bucket.partition_point(|t| f64::from(t.point.x) < x - radius);
                for target in bucket[start..]
                    .iter()
                    .take_while(|t| f64::from(t.point.x) <= x + radius)
                {
                    examined += 1;
                    ensure!(
                        examined <= MAX_QUERY_CANDIDATES,
                        "Snap neighborhood exceeds 4096 candidates; reduce tolerance or zoom in"
                    );
                    let (dx, dy) = (f64::from(target.point.x) - x, f64::from(target.point.y) - y);
                    let distance = dx * dx + dy * dy;
                    if distance <= radius * radius
                        && best.is_none_or(|(old, id)| {
                            distance < old || (distance == old && target.id < id.id)
                        })
                    {
                        best = Some((distance, *target));
                    }
                }
            }
        }
        if let Some((_, target)) = best {
            return Ok(SnapResult {
                point: target.point,
                anchor: Some(target.id),
                vertical: None,
                horizontal: None,
                examined_anchors: examined,
            });
        }
        let vertical = axis_snap(
            x,
            radius,
            &self.vertical,
            self.grid.map(|g| (g.origin.x, g.settings)),
        );
        let horizontal = axis_snap(
            y,
            radius,
            &self.horizontal,
            self.grid.map(|g| (g.origin.y, g.settings)),
        );
        Ok(SnapResult {
            point: Point {
                x: vertical.map_or(point.x, |v| v.coordinate),
                y: horizontal.map_or(point.y, |v| v.coordinate),
            },
            anchor: None,
            vertical,
            horizontal,
            examined_anchors: examined,
        })
    }
}

fn validate_transform(matrix: [f32; 6]) -> Result<()> {
    ensure!(
        matrix.into_iter().all(valid_coordinate),
        "Snap transform must be finite and bounded"
    );
    let [a, b, c, d, _, _] = matrix.map(f64::from);
    ensure!(
        (a * d - b * c).abs() > 1e-12,
        "Snap transform must be invertible"
    );
    Ok(())
}

fn map_point(matrix: [f32; 6], point: Point) -> Result<Point> {
    let [a, b, c, d, e, f] = matrix.map(f64::from);
    let (x, y) = (f64::from(point.x), f64::from(point.y));
    let result = Point {
        x: (a * x + c * y + e) as f32,
        y: (b * x + d * y + f) as f32,
    };
    ensure!(
        valid_point(result),
        "Transformed snap anchor exceeds world coordinate bounds"
    );
    Ok(result)
}

fn axis_snap(
    value: f64,
    radius: f64,
    guides: &[SnapGuide],
    grid: Option<(f32, GridSettings)>,
) -> Option<AxisSnap> {
    let at = guides.partition_point(|g| f64::from(g.position) < value);
    let mut best: Option<(f64, AxisSnap)> = None;
    for guide in at
        .checked_sub(1)
        .into_iter()
        .chain((at < guides.len()).then_some(at))
        .map(|i| guides[i])
    {
        let distance = (f64::from(guide.position) - value).abs();
        let candidate = AxisSnap {
            coordinate: guide.position,
            kind: AxisSnapKind::Guide(guide.id),
        };
        if distance <= radius
            && best.is_none_or(|(old, target)| {
                distance < old
                    || (distance == old
                        && matches!(target.kind, AxisSnapKind::Guide(id) if guide.id < id))
            })
        {
            best = Some((distance, candidate));
        }
    }
    if let Some((origin, settings)) = grid {
        let step = f64::from(settings.spacing) / f64::from(settings.subdivisions);
        let origin = f64::from(origin);
        let coordinate = (origin + ((value - origin) / step).round() * step) as f32;
        let distance = (f64::from(coordinate) - value).abs();
        if valid_coordinate(coordinate)
            && distance <= radius
            && best.is_none_or(|(old, _)| distance < old)
        {
            best = Some((
                distance,
                AxisSnap {
                    coordinate,
                    kind: AxisSnapKind::Grid,
                },
            ));
        }
    }
    best.map(|(_, target)| target)
}
