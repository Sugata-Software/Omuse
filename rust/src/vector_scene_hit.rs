use super::{VectorScene, transform_path_raw};
use crate::vector_path::{FillRule, FlatSubpath, Point};
use anyhow::{Result, ensure};

// This is an aggregate cap across every object considered by one query. It
// matches VectorPath's per-path flatten limit while preventing a legal scene
// from multiplying that limit by its object count.
const MAX_HIT_FLAT_POINTS: usize = 2_000_000;
const HIT_FLATTEN_TOLERANCE: f32 = 0.25;

impl VectorScene {
    /// Return the topmost painted object at a scene-local point.
    ///
    /// `tolerance` is measured in scene-local pixels. Filled objects match
    /// their actual interior (including compound-path fill rules) and edges;
    /// stroked objects match their rendered stroke plus the tolerance.
    pub fn hit_test(&self, point: Point, tolerance: f32) -> Result<Option<usize>> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "invalid vector scene hit point"
        );
        ensure!(
            tolerance.is_finite() && tolerance >= 0.,
            "invalid vector scene hit tolerance"
        );
        self.validate()?;

        let mut flattened_points = 0usize;
        for (index, object) in self.objects.iter().enumerate().rev() {
            if !object.visible || object.opacity <= 0. {
                continue;
            }
            let fill_visible = object.fill.is_some_and(|color| color[3] > 0);
            let visible_stroke = object.stroke.filter(|stroke| stroke.color[3] > 0);
            let stroke_visible = visible_stroke.is_some();
            if !fill_visible && !stroke_visible {
                continue;
            }

            // Flatten after applying the object transform so the tolerance,
            // edge distance and curved geometry are all evaluated in the
            // scene-local coordinate system supplied by the caller.
            let path = transform_path_raw(&object.path, object.transform)?;
            let stroke_radius = match visible_stroke {
                Some(stroke) => stroke.width * object.similarity_scale()? * 0.5,
                None => 0.,
            };
            let padding = tolerance + stroke_radius;
            if !bounds_can_hit(&path, point, padding) {
                continue;
            }

            let flat = path.flatten(HIT_FLATTEN_TOLERANCE, || false)?;
            let object_points = flat
                .iter()
                .try_fold(0usize, |total, subpath| {
                    total.checked_add(subpath.points.len())
                })
                .ok_or_else(|| anyhow::anyhow!("vector scene hit test exceeds work limit"))?;
            flattened_points = flattened_points
                .checked_add(object_points)
                .ok_or_else(|| anyhow::anyhow!("vector scene hit test exceeds work limit"))?;
            ensure!(
                flattened_points <= MAX_HIT_FLAT_POINTS,
                "vector scene hit test exceeds work limit"
            );

            if fill_visible && fill_matches(&flat, path.fill_rule, point, tolerance) {
                return Ok(Some(index));
            }
            if stroke_visible && edge_matches(&flat, point, tolerance + stroke_radius, false, 2) {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

fn bounds_can_hit(path: &crate::vector_path::VectorPath, point: Point, padding: f32) -> bool {
    let Some((low, high)) = path.bounds() else {
        return false;
    };
    let padding = f64::from(padding);
    let x = f64::from(point.x);
    let y = f64::from(point.y);
    x >= f64::from(low.x) - padding
        && x <= f64::from(high.x) + padding
        && y >= f64::from(low.y) - padding
        && y <= f64::from(high.y) + padding
}

fn fill_matches(flat: &[FlatSubpath], rule: FillRule, point: Point, tolerance: f32) -> bool {
    if edge_matches(flat, point, tolerance, true, 3) {
        return true;
    }

    let mut parity = false;
    let mut winding = 0i32;
    for subpath in flat {
        for (a, b) in segments(subpath, true) {
            let ay = f64::from(a.y);
            let by = f64::from(b.y);
            let py = f64::from(point.y);
            if (ay <= py && by > py) || (ay > py && by <= py) {
                let cross_x =
                    f64::from(a.x) + (py - ay) * (f64::from(b.x) - f64::from(a.x)) / (by - ay);
                if cross_x > f64::from(point.x) {
                    parity = !parity;
                    winding += if by > ay { 1 } else { -1 };
                }
            }
        }
    }
    match rule {
        FillRule::EvenOdd => parity,
        FillRule::NonZero => winding != 0,
    }
}

fn edge_matches(
    flat: &[FlatSubpath],
    point: Point,
    threshold: f32,
    close_open_subpaths: bool,
    minimum_points: usize,
) -> bool {
    let threshold_squared = f64::from(threshold) * f64::from(threshold);
    flat.iter().any(|subpath| {
        if subpath.points.len() < minimum_points {
            return false;
        }
        let close = subpath.closed || close_open_subpaths;
        segments(subpath, close)
            .any(|(a, b)| segment_distance_squared(point, a, b) <= threshold_squared)
    })
}

fn segments(subpath: &FlatSubpath, close: bool) -> impl Iterator<Item = (Point, Point)> + '_ {
    let closing = (close && subpath.points.len() > 1)
        .then(|| {
            let first = subpath.points[0];
            let last = *subpath.points.last().unwrap();
            (last != first).then_some((last, first))
        })
        .flatten();
    subpath
        .points
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .chain(closing)
}

fn segment_distance_squared(point: Point, a: Point, b: Point) -> f64 {
    let px = f64::from(point.x);
    let py = f64::from(point.y);
    let ax = f64::from(a.x);
    let ay = f64::from(a.y);
    let dx = f64::from(b.x) - ax;
    let dy = f64::from(b.y) - ay;
    let denominator = dx * dx + dy * dy;
    let t = if denominator <= 1e-24 {
        0.
    } else {
        (((px - ax) * dx + (py - ay) * dy) / denominator).clamp(0., 1.)
    };
    let distance_x = px - (ax + dx * t);
    let distance_y = py - (ay + dy * t);
    distance_x * distance_x + distance_y * distance_y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_path::{Anchor, StrokeStyle, Subpath, VectorPath};
    use crate::vector_scene::{VECTOR_SCENE_VERSION, VectorObject};

    fn anchor(x: f32, y: f32) -> Anchor {
        Anchor {
            position: Point { x, y },
            incoming: None,
            outgoing: None,
        }
    }

    fn scene(objects: Vec<VectorObject>) -> VectorScene {
        VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 100,
            height: 100,
            objects,
        }
    }

    #[test]
    fn compound_fill_respects_holes_and_fill_rule() {
        let mut object = VectorObject::rectangle(
            "Compound",
            10.,
            10.,
            80.,
            80.,
            Some([10, 20, 30, 255]),
            None,
        )
        .unwrap();
        object.path.subpaths.push(Subpath {
            anchors: vec![
                anchor(30., 30.),
                anchor(70., 30.),
                anchor(70., 70.),
                anchor(30., 70.),
            ],
            closed: true,
        });
        object.path.fill_rule = FillRule::EvenOdd;
        let mut scene = scene(vec![object]);
        assert_eq!(
            scene.hit_test(Point { x: 20., y: 20. }, 0.).unwrap(),
            Some(0)
        );
        assert_eq!(scene.hit_test(Point { x: 50., y: 50. }, 0.).unwrap(), None);

        scene.objects[0].path.fill_rule = FillRule::NonZero;
        assert_eq!(
            scene.hit_test(Point { x: 50., y: 50. }, 0.).unwrap(),
            Some(0)
        );
    }

    #[test]
    fn topmost_painted_visible_object_wins() {
        let bottom =
            VectorObject::rectangle("Bottom", 5., 5., 60., 60., Some([255, 0, 0, 255]), None)
                .unwrap();
        let top = VectorObject::rectangle("Top", 20., 20., 60., 60., Some([0, 0, 255, 255]), None)
            .unwrap();
        let mut scene = scene(vec![bottom, top]);
        let point = Point { x: 30., y: 30. };
        assert_eq!(scene.hit_test(point, 0.).unwrap(), Some(1));

        scene.objects[1].visible = false;
        assert_eq!(scene.hit_test(point, 0.).unwrap(), Some(0));
        scene.objects[1].visible = true;
        scene.objects[1].opacity = 0.;
        assert_eq!(scene.hit_test(point, 0.).unwrap(), Some(0));
        scene.objects[1].opacity = 1.;
        scene.objects[1].fill = Some([0, 0, 255, 0]);
        assert_eq!(scene.hit_test(point, 0.).unwrap(), Some(0));
    }

    #[test]
    fn open_stroke_uses_width_and_tolerance_without_closing() {
        let object = VectorObject::new(
            "Open line",
            VectorPath {
                subpaths: vec![Subpath {
                    anchors: vec![anchor(10., 50.), anchor(90., 50.)],
                    closed: false,
                }],
                fill_rule: FillRule::NonZero,
            },
            None,
            Some(StrokeStyle {
                color: [0, 0, 0, 255],
                width: 4.,
            }),
        );
        let scene = scene(vec![object]);
        assert_eq!(
            scene.hit_test(Point { x: 50., y: 52.75 }, 1.).unwrap(),
            Some(0)
        );
        assert_eq!(
            scene.hit_test(Point { x: 50., y: 53.25 }, 1.).unwrap(),
            None
        );
        assert_eq!(scene.hit_test(Point { x: 50., y: 30. }, 1.).unwrap(), None);
    }

    #[test]
    fn affine_fill_uses_transformed_geometry_not_its_bounds() {
        let mut object =
            VectorObject::rectangle("Skewed", 0., 0., 10., 10., Some([20, 80, 140, 255]), None)
                .unwrap();
        object.transform = [2., 0.5, 0.25, 1.5, 30., 20.];
        let scene = scene(vec![object]);
        assert_eq!(
            scene.hit_test(Point { x: 41.25, y: 30. }, 0.).unwrap(),
            Some(0)
        );
        assert_eq!(scene.hit_test(Point { x: 31., y: 39. }, 0.).unwrap(), None);
    }

    #[test]
    fn invalid_query_inputs_are_rejected() {
        let object =
            VectorObject::rectangle("Rectangle", 10., 10., 20., 20., Some([1, 2, 3, 255]), None)
                .unwrap();
        let scene = scene(vec![object]);
        assert!(scene.hit_test(Point { x: f32::NAN, y: 0. }, 1.).is_err());
        assert!(
            scene
                .hit_test(Point { x: 0., y: 0. }, f32::INFINITY)
                .is_err()
        );
        assert!(scene.hit_test(Point { x: 0., y: 0. }, -1.).is_err());
    }
}
