// Adapted from VectorCraft 9f659195c324419c087604a79c4e3b434874a62c.
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
// SPDX-License-Identifier: MIT
use kurbo::{BezPath, Point};

/// Make edges that rise by less than the sweep tolerance `eps` exactly horizontal.
///
/// The sweep orders a curve against another segment over a y-span shorter than `eps` by bounding
/// boxes alone, so an almost-horizontal edge (typically a boolean result's edge whose end, an
/// intersection point, is a rounding error off the input's horizontal) can be mis-ordered against
/// a curve it passes and the output loses a corner; exact horizontals are handled exactly. So
/// on-curve y coordinates within `eps` of each other snap to one value (the lowest of each
/// cluster, keeping distinct values at least `eps` apart), handles move with their anchors, and a
/// handle within `eps` of its anchor's height is levelled with it (no y-extremum is left in a
/// sub-`eps` sliver of a curve). Anchors move by less than `eps` and handles by less than `2·eps`,
/// the order of the sweep's own tolerance.
pub(crate) fn snap_horizontals(paths: &mut [&mut BezPath], eps: f64) {
    use kurbo::PathEl::*;
    let mut ys: Vec<f64> = paths
        .iter()
        .flat_map(|p| p.elements().iter().filter_map(kurbo::PathEl::end_point))
        .map(|p| p.y)
        .collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let mut start = f64::NEG_INFINITY;
    let levels: Vec<f64> = ys
        .iter()
        .map(|&y| {
            if y - start >= eps {
                start = y;
            }
            start
        })
        .collect();
    let snap = |p: &mut Point| {
        let y = levels[ys.partition_point(|&v| v < p.y)];
        let moved = y - p.y;
        p.y = y;
        moved
    };
    // Shift a handle with its anchor, then level it with the anchor when within `eps`.
    let follow = |h: &mut Point, anchor: Point, moved: f64| {
        h.y += moved;
        if (h.y - anchor.y).abs() < eps {
            h.y = anchor.y;
        }
    };
    for p in paths.iter_mut() {
        // The current point (snapped) and how far snapping moved it; the subpath's start.
        let (mut cur, mut moved, mut first) = (Point::ZERO, 0.0, Point::ZERO);
        for el in p.elements_mut() {
            match el {
                MoveTo(q) => {
                    moved = snap(q);
                    (cur, first) = (*q, *q);
                }
                LineTo(q) => {
                    moved = snap(q);
                    cur = *q;
                }
                QuadTo(c, q) => {
                    let m = snap(q);
                    follow(c, cur, moved);
                    follow(c, *q, 0.0);
                    (cur, moved) = (*q, m);
                }
                CurveTo(c1, c2, q) => {
                    let m = snap(q);
                    follow(c1, cur, moved);
                    follow(c2, *q, m);
                    (cur, moved) = (*q, m);
                }
                ClosePath => {
                    // A later segment without a MoveTo starts from the subpath's (snapped) start.
                    (cur, moved) = (first, 0.0);
                }
            }
        }
    }
}
