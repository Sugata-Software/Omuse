//! Display-only selection outlines. The exact mask is never modified. Work is
//! clipped to the visible canvas, cancellable and bounded by the model budget;
//! low zoom groups source pixels into display-sized cells before painting.
use crate::{editor::Selection, model::PixelRect};
use anyhow::{Result, ensure};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_POINTS: usize = 65_536;

#[derive(Clone, Debug, Default)]
pub struct Outline {
    pub points: Vec<(u32, u32)>,
    /// Source-pixel cell size used to sample the visible contour.
    pub cell_size: u32,
}

/// Power-of-two buckets avoid recomputing for every small zoom-wheel delta.
/// Cells cover approximately one display pixel at low magnification.
pub fn display_cell(zoom: f32) -> u32 {
    if !zoom.is_finite() || zoom <= 0. {
        return 1;
    }
    let mut step = 1u32;
    while step < 32_768 && step as f32 * zoom < 0.75 {
        step *= 2;
    }
    step
}

/// Return actual boundary pixel coordinates, never a bounding-box substitute.
/// Each cell contributes at most one real boundary point, preserving thin
/// islands and holes that centre-only sampling would miss at low zoom.
pub fn generate(
    selection: &Selection,
    cell_size: u32,
    visible: PixelRect,
    point_limit: usize,
    cancel: &AtomicBool,
) -> Result<Outline> {
    ensure!(
        crate::model::valid_dimensions(selection.width, selection.height),
        "Invalid selection dimensions"
    );
    ensure!(
        selection.mask.len() as u64 == u64::from(selection.width) * u64::from(selection.height),
        "Selection mask dimensions do not match its data"
    );
    ensure!(
        cell_size.is_power_of_two() && cell_size <= 32_768,
        "Invalid selection display cell"
    );
    check(cancel)?;
    let mut outline = Outline {
        points: Vec::new(),
        cell_size,
    };
    let Some(visible) = visible.clipped(selection.width, selection.height) else {
        return Ok(outline);
    };
    let limit = point_limit.min(MAX_POINTS);
    if limit == 0 {
        return Ok(outline);
    }
    outline.points.reserve(limit.min(4096));
    let (right, bottom) = (visible.x + visible.width, visible.y + visible.height);
    let (left, top) = (
        visible.x / cell_size * cell_size,
        visible.y / cell_size * cell_size,
    );
    let mut bins = HashSet::new();
    for y in (top..bottom).step_by(cell_size as usize) {
        check(cancel)?;
        for x in (left..right).step_by(cell_size as usize) {
            let Some(point) = first_boundary(
                selection,
                x.max(visible.x),
                y.max(visible.y),
                (x + cell_size).min(right),
                (y + cell_size).min(bottom),
                cancel,
            )?
            else {
                continue;
            };
            if outline.cell_size > cell_size
                && !bins.insert((point.0 / outline.cell_size, point.1 / outline.cell_size))
            {
                continue;
            }
            outline.points.push(point);
            // Extremely noisy masks can have more edges than display pixels.
            // Coarsen deterministically instead of growing an unbounded vector
            // or dropping everything after the first rows of the selection.
            while outline.points.len() > limit {
                outline.cell_size *= 2;
                bins.clear();
                outline
                    .points
                    .retain(|&(x, y)| bins.insert((x / outline.cell_size, y / outline.cell_size)));
            }
        }
    }
    check(cancel)?;
    Ok(outline)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Selection outline cancelled"
    );
    Ok(())
}
fn first_boundary(
    selection: &Selection,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    cancel: &AtomicBool,
) -> Result<Option<(u32, u32)>> {
    let (w, h) = (selection.width as usize, selection.height as usize);
    for y in top as usize..bottom as usize {
        check(cancel)?;
        let row = y * w;
        for x in left as usize..right as usize {
            if x & 1023 == 0 {
                check(cancel)?;
            }
            let at = row + x;
            if selection.mask[at] >= 128
                && (x == 0
                    || x + 1 == w
                    || y == 0
                    || y + 1 == h
                    || selection.mask[at - 1] < 128
                    || selection.mask[at + 1] < 128
                    || selection.mask[at - w] < 128
                    || selection.mask[at + w] < 128)
            {
                return Ok(Some((x as u32, y as u32)));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection(width: u32, height: u32, selected: impl Fn(u32, u32) -> bool) -> Selection {
        Selection {
            width,
            height,
            mask: (0..height)
                .flat_map(|y| {
                    (0..width)
                        .map(|x| if selected(x, y) { 255 } else { 0 })
                        .collect::<Vec<_>>()
                })
                .collect(),
        }
    }
    fn outline(selection: &Selection, zoom: f32) -> Outline {
        generate(
            selection,
            display_cell(zoom),
            PixelRect {
                x: 0,
                y: 0,
                width: selection.width,
                height: selection.height,
            },
            MAX_POINTS,
            &AtomicBool::new(false),
        )
        .unwrap()
    }
    #[test]
    fn full_zoom_has_exact_rectangle_edges_and_hole_boundaries() {
        let mask = selection(16, 16, |x, y| {
            (2..14).contains(&x)
                && (3..13).contains(&y)
                && !((6..10).contains(&x) && (6..10).contains(&y))
        });
        let result = outline(&mask, 1.);
        assert!(result.points.contains(&(2, 7)) && result.points.contains(&(13, 7)));
        assert!(result.points.contains(&(7, 3)) && result.points.contains(&(7, 12)));
        assert!(result.points.contains(&(5, 7)) && result.points.contains(&(10, 7)));
        assert!(result.points.contains(&(7, 5)) && result.points.contains(&(7, 10)));
        assert!(!result.points.contains(&(7, 7)) && !result.points.contains(&(3, 4)));
    }
    #[test]
    fn low_zoom_retains_off_grid_thin_islands_and_holes_without_changing_mask_bounds() {
        let mask = selection(97, 65, |x, y| {
            x == 3
                || y == 5
                || ((20..90).contains(&x) && (20..60).contains(&y) && !(x == 37 && y == 35))
        });
        let before = mask.clone();
        for zoom in [0.125, 0.25, 0.5, 1., 8.] {
            let result = outline(&mask, zoom);
            assert!(result.points.iter().any(|&(x, y)| x == 3 && y > 10));
            assert!(result.points.iter().any(|&(x, y)| y == 5 && x > 10));
            assert!(
                result
                    .points
                    .iter()
                    .any(|&(x, y)| x.abs_diff(37) <= 1 && y.abs_diff(35) <= 1)
            );
            assert_eq!(mask, before);
            assert_eq!(mask.bounds(), before.bounds());
        }
    }
    #[test]
    fn clipping_never_invents_an_edge_at_the_viewport_border() {
        let mask = selection(100, 80, |_, _| true);
        let result = generate(
            &mask,
            1,
            PixelRect {
                x: 20,
                y: 20,
                width: 30,
                height: 30,
            },
            MAX_POINTS,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(result.points.is_empty());
        let edge = generate(
            &mask,
            2,
            PixelRect {
                x: 0,
                y: 10,
                width: 10,
                height: 20,
            },
            MAX_POINTS,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            edge.points
                .iter()
                .all(|&(x, y)| x == 0 && (10..30).contains(&y))
        );
    }
    #[test]
    fn noisy_masks_coarsen_across_the_whole_visible_region_with_bounded_points() {
        let mask = selection(128, 128, |x, y| (x + y) % 2 == 0);
        let bounds = PixelRect {
            x: 0,
            y: 0,
            width: 128,
            height: 128,
        };
        let a = generate(&mask, 1, bounds, 100, &AtomicBool::new(false)).unwrap();
        let b = generate(&mask, 1, bounds, 100, &AtomicBool::new(false)).unwrap();
        assert!(a.points.len() <= 100 && a.cell_size > 1);
        assert_eq!(a.points, b.points);
        assert!(a.points.iter().any(|&(x, y)| x > 100 && y > 100));
        assert!(
            a.points
                .iter()
                .all(|&(x, y)| mask.contains(x as i32, y as i32))
        );
    }
    #[test]
    fn malformed_cancelled_and_zero_limit_work_never_walk_invalid_memory() {
        let mask = selection(5, 4, |_, _| true);
        let bounds = PixelRect {
            x: 0,
            y: 0,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert!(generate(&mask, 1, bounds, 10, &AtomicBool::new(true)).is_err());
        assert!(generate(&mask, 3, bounds, 10, &AtomicBool::new(false)).is_err());
        assert!(
            generate(
                &Selection {
                    mask: vec![],
                    ..mask.clone()
                },
                1,
                bounds,
                10,
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(
            generate(&mask, 1, bounds, 0, &AtomicBool::new(false))
                .unwrap()
                .points
                .is_empty()
        );
        assert_eq!(outline(&mask, 16.).points.len(), 14);
    }
    #[test]
    fn display_buckets_only_change_when_outline_resolution_changes() {
        assert_eq!(display_cell(1.), 1);
        assert_eq!(display_cell(64.), 1);
        assert_eq!(display_cell(0.5), 2);
        assert_eq!(display_cell(0.51), 2);
        assert_eq!(display_cell(0.1), 8);
        assert_eq!(display_cell(0.01), 128);
    }
}
