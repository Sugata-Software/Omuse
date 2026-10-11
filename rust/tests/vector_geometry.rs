use std::sync::atomic::AtomicBool;

use omuse::vector_boolean::{BooleanOperation, combine};
use omuse::vector_geometry::{MAX_GEOMETRY_SEGMENTS, offset_path, outline_stroke, simplify};
use omuse::vector_path::{
    Anchor, FillRule, Point, StrokeStyle, Subpath, VectorPath, rasterize_rgba,
};
use omuse::vector_scene::{
    GradientFill, GradientKind, GradientSpread, GradientStop, StrokeCap, StrokeJoin, StrokeOptions,
    VectorObject,
};

fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn rect(x: f32, y: f32, width: f32, height: f32) -> VectorObject {
    VectorObject::rectangle(
        "Rectangle",
        x,
        y,
        width,
        height,
        Some([150, 60, 30, 255]),
        None,
    )
    .unwrap()
}
fn ellipse(x: f32, y: f32, width: f32, height: f32) -> VectorObject {
    VectorObject::ellipse(
        "Ellipse",
        x,
        y,
        width,
        height,
        Some([150, 60, 30, 255]),
        None,
    )
    .unwrap()
}
fn nodes(objects: &[VectorObject]) -> usize {
    objects
        .iter()
        .flat_map(|o| &o.path.subpaths)
        .map(|s| s.anchors.len())
        .sum()
}
fn curves(objects: &[VectorObject]) -> usize {
    objects
        .iter()
        .flat_map(|o| &o.path.subpaths)
        .flat_map(|s| &s.anchors)
        .filter(|a| a.incoming.is_some() || a.outgoing.is_some())
        .count()
}
fn mask(objects: &[VectorObject], width: u32, height: u32) -> Vec<bool> {
    let mut mask = vec![false; (width * height) as usize];
    for object in objects {
        let raster = rasterize_rgba(
            &object.path,
            width,
            height,
            Some([0, 0, 0, 255]),
            None,
            0.01,
            || false,
        )
        .unwrap();
        for (out, pixel) in mask.iter_mut().zip(raster.pixels()) {
            *out |= pixel[3] != 0;
        }
    }
    mask
}

#[test]
fn curved_booleans_keep_handles_and_far_fewer_nodes_than_polygon_baseline() {
    let left = ellipse(10., 10., 100., 100.);
    let right = ellipse(60., 10., 100., 100.);
    let inputs = [left.clone(), right.clone()];
    let result = combine(&inputs, BooleanOperation::Intersect, &idle()).unwrap();
    assert_eq!(
        inputs,
        [left.clone(), right.clone()],
        "inputs remain untouched"
    );
    assert!(curves(&result) >= 4);
    assert!(nodes(&result) <= 12, "{} nodes", nodes(&result));
    let flattened_count: usize = inputs
        .iter()
        .flat_map(|o| o.path.flatten(0.05, || false).unwrap())
        .map(|s| s.points.len())
        .sum();
    assert!(flattened_count > nodes(&result) * 8);

    // Compare the original polygon engine at its former 0.05px tolerance.
    use i_overlay::{
        core::{fill_rule::FillRule as OverlayFill, overlay_rule::OverlayRule},
        float::overlay::FloatOverlay,
    };
    let contours = |object: &VectorObject| -> Vec<Vec<[f32; 2]>> {
        object
            .path
            .flatten(0.05, || false)
            .unwrap()
            .into_iter()
            .map(|s| s.points.into_iter().map(|p| [p.x, p.y]).collect())
            .collect()
    };
    let old = FloatOverlay::with_subj_and_clip(&contours(&left), &contours(&right))
        .overlay(OverlayRule::Intersect, OverlayFill::EvenOdd);
    let polygon_nodes: usize = old.iter().flatten().map(Vec::len).sum();
    assert!(polygon_nodes > nodes(&result) * 4);
    let polygons: Vec<_> = old
        .into_iter()
        .map(|shape| {
            VectorObject::new(
                "old polygon",
                VectorPath {
                    subpaths: shape
                        .into_iter()
                        .map(|ring| Subpath {
                            closed: true,
                            anchors: ring
                                .into_iter()
                                .map(|p| Anchor {
                                    position: Point { x: p[0], y: p[1] },
                                    incoming: None,
                                    outgoing: None,
                                })
                                .collect(),
                        })
                        .collect(),
                    fill_rule: FillRule::EvenOdd,
                },
                Some([0, 0, 0, 255]),
                None,
            )
        })
        .collect();
    let actual = mask(&result, 180, 130);
    let expected = mask(&polygons, 180, 130);
    let different = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
    assert!(
        different <= 24,
        "{different} differing 1px boundary samples"
    );
    eprintln!(
        "curve boolean lens: {} curve nodes versus {polygon_nodes} polygon nodes; {different}/23400 raster samples differ",
        nodes(&result)
    );
}

#[test]
fn nested_hole_and_island_remain_distinct_filled_components() {
    let mut object = ellipse(2., 2., 96., 96.);
    object.path.fill_rule = FillRule::EvenOdd;
    object
        .path
        .subpaths
        .extend(ellipse(20., 20., 60., 60.).path.subpaths);
    object
        .path
        .subpaths
        .extend(ellipse(40., 40., 20., 20.).path.subpaths);
    let result = combine(&[object], BooleanOperation::Union, &idle()).unwrap();
    assert_eq!(result.len(), 2);
    let pixels = mask(&result, 100, 100);
    assert!(pixels[50 * 100 + 5]);
    assert!(!pixels[50 * 100 + 25]);
    assert!(pixels[50 * 100 + 50]);
    assert!(curves(&result) >= 12);
}

#[test]
fn reflected_curves_preserve_holes_and_world_gradient_stroke_style() {
    let mut source = ellipse(0., 0., 30., 30.);
    source.path.fill_rule = FillRule::EvenOdd;
    source
        .path
        .subpaths
        .extend(ellipse(10., 10., 10., 10.).path.subpaths);
    source.transform = [-2., 0., 0., 2., 80., 10.];
    source.opacity = 0.4;
    source.visible = false;
    source.stroke = Some(StrokeStyle {
        color: [1, 2, 3, 255],
        width: 2.,
    });
    source.stroke_options = Some(StrokeOptions {
        dashes: vec![3., 4.],
        dash_offset: 2.,
        ..Default::default()
    });
    source.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 0., y: 0. },
            end: Point { x: 30., y: 0. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [255, 0, 0, 255],
            },
            GradientStop {
                offset: 1.,
                color: [0, 0, 255, 255],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
    });
    let result = combine(&[source.clone()], BooleanOperation::Union, &idle()).unwrap();
    assert_eq!(result.len(), 1);
    let output = &result[0];
    assert_eq!(output.transform, [1., 0., 0., 1., 0., 0.]);
    assert_eq!(output.stroke.unwrap().width, 4.);
    assert_eq!(output.stroke_options.as_ref().unwrap().dashes, vec![6., 8.]);
    assert_eq!(output.stroke_options.as_ref().unwrap().dash_offset, 4.);
    let mut gradient = source.fill_gradient.unwrap();
    gradient.bake_transform(source.transform);
    assert_eq!(output.fill_gradient, Some(gradient));
    assert_eq!(output.opacity, 0.4);
    assert!(!output.visible);
    let pixels = mask(&result, 100, 80);
    assert!(!pixels[40 * 100 + 50]);
    assert!(pixels[40 * 100 + 25]);
}

#[test]
fn simplify_reduces_collinear_nodes_and_keeps_corners_and_identity() {
    let mut source = rect(0., 0., 40., 40.);
    source.path.insert_on_segment(0, 0, 0.5).unwrap();
    source.path.insert_on_segment(0, 2, 0.5).unwrap();
    source.transform = [2., 0., 0., 2., 10., 20.];
    let before = source.clone();
    let output = simplify(&source, 0.25, &idle()).unwrap();
    assert!(output.path.subpaths[0].anchors.len() < source.path.subpaths[0].anchors.len());
    assert_eq!(output.path.subpaths[0].anchors.len(), 4);
    assert_eq!(output.id, source.id);
    assert_eq!(output.transform, source.transform);
    assert_eq!(output.fill, source.fill);
    assert_eq!(mask(&[output], 50, 50), mask(&[source.clone()], 50, 50));
    assert_eq!(source, before);
}

#[test]
fn simplify_subdivided_straight_cubics_keeps_closing_edge_and_true_corners() {
    let mut source = rect(10., 10., 40., 40.);
    source.path.fill_rule = FillRule::EvenOdd;
    let corners = source.path.subpaths[0].anchors.clone();
    for segment in (0..4).rev() {
        source.path.insert_on_segment(0, segment, 0.5).unwrap();
    }
    assert_eq!(source.path.subpaths[0].anchors.len(), 8);
    assert!(curves(&[source.clone()]) > 0);
    let before = source.clone();
    // Even an intentionally large tolerance must not round away true corners.
    let output = simplify(&source, 100., &idle()).unwrap();
    assert!(output.path.subpaths[0].closed);
    assert_eq!(output.path.fill_rule, FillRule::EvenOdd);
    assert_eq!(output.path.subpaths[0].anchors, corners);
    assert_eq!(mask(&[output], 64, 64), mask(&[source.clone()], 64, 64));
    assert_eq!(source, before);
}

#[test]
fn simplify_straight_open_cubics_retains_endpoints_and_unused_closing_handles() {
    let mut source = rect(0., 0., 40., 10.);
    source.path.subpaths[0].closed = false;
    source.path.subpaths[0].anchors.truncate(2);
    source.path.insert_on_segment(0, 0, 0.5).unwrap();
    source.path.subpaths[0].anchors[0].incoming = Some(Point { x: -2., y: 3. });
    source.path.subpaths[0].anchors[2].outgoing = Some(Point { x: 42., y: 4. });
    let before = source.clone();
    let output = simplify(&source, 0.01, &idle()).unwrap();
    let anchors = &output.path.subpaths[0].anchors;
    assert!(!output.path.subpaths[0].closed);
    assert_eq!(anchors.len(), 2);
    assert_eq!(anchors[0].position, Point { x: 0., y: 0. });
    assert_eq!(anchors[1].position, Point { x: 40., y: 0. });
    assert_eq!(
        anchors[0].incoming,
        source.path.subpaths[0].anchors[0].incoming
    );
    assert_eq!(
        anchors[1].outgoing,
        source.path.subpaths[0].anchors[2].outgoing
    );
    assert!(anchors[0].outgoing.is_none() && anchors[1].incoming.is_none());
    assert_eq!(source, before);
}

#[test]
fn simplify_does_not_misclassify_bowed_or_backtracking_cubics_as_lines() {
    let mut source = rect(0., 0., 40., 10.);
    source.path.subpaths[0].closed = false;
    source.path.subpaths[0].anchors.truncate(2);
    source.path.insert_on_segment(0, 0, 0.5).unwrap();
    let mut bowed = source.clone();
    bowed.path.subpaths[0].anchors[0].outgoing = Some(Point { x: 5., y: 10. });
    bowed.path.subpaths[0].anchors[1].incoming = Some(Point { x: 15., y: 10. });
    let output = simplify(&bowed, 0.01, &idle()).unwrap();
    assert!(
        output.path.flatten(0.01, || false).unwrap()[0]
            .points
            .iter()
            .any(|p| p.y > 7.)
    );
    source.path.subpaths[0].anchors[0].outgoing = Some(Point { x: 80., y: 0. });
    source.path.subpaths[0].anchors[1].incoming = Some(Point { x: 80., y: 0. });
    let output = simplify(&source, 0.01, &idle()).unwrap();
    assert!(
        output.path.flatten(0.01, || false).unwrap()[0]
            .points
            .iter()
            .any(|p| p.x > 50.)
    );
    assert_eq!(
        output.path.subpaths[0].anchors.first().unwrap().position,
        Point { x: 0., y: 0. }
    );
    assert_eq!(
        output.path.subpaths[0].anchors.last().unwrap().position,
        Point { x: 40., y: 0. }
    );
}

#[test]
fn simplify_keeps_open_endpoints_and_curved_controls() {
    let mut source = ellipse(0., 0., 100., 100.);
    source.path.subpaths[0].closed = false;
    for i in (0..3).rev() {
        source.path.insert_on_segment(0, i, 0.5).unwrap();
    }
    let output = simplify(&source, 0.2, &idle()).unwrap();
    assert!(!output.path.subpaths[0].closed);
    assert_eq!(
        output.path.subpaths[0].anchors.first().unwrap().position,
        source.path.subpaths[0].anchors.first().unwrap().position
    );
    assert_eq!(
        output.path.subpaths[0].anchors.last().unwrap().position,
        source.path.subpaths[0].anchors.last().unwrap().position
    );
    assert!(nodes(&[output.clone()]) <= nodes(&[source]));
    assert!(curves(&[output]) > 0);
}

#[test]
fn dense_traced_polyline_can_be_simplified_before_boolean_work() {
    let anchors = (0..5_000)
        .map(|i| {
            let t = i as f32 * std::f32::consts::TAU / 5_000.;
            Anchor {
                position: Point {
                    x: 150. + 100. * t.cos(),
                    y: 150. + 100. * t.sin(),
                },
                incoming: None,
                outgoing: None,
            }
        })
        .collect();
    let source = VectorObject::new(
        "Dense trace",
        VectorPath {
            subpaths: vec![Subpath {
                anchors,
                closed: true,
            }],
            fill_rule: FillRule::EvenOdd,
        },
        Some([0, 0, 0, 255]),
        None,
    );
    assert!(combine(&[source.clone()], BooleanOperation::Union, &idle()).is_err());
    let output = simplify(&source, 0.5, &idle()).unwrap();
    assert!(output.path.subpaths[0].anchors.len() < 100);
    assert_eq!(output.path.fill_rule, FillRule::EvenOdd);
    assert!(combine(&[output.clone()], BooleanOperation::Union, &idle()).is_ok());
    let reduced = &output.path.subpaths[0].anchors;
    let mut max_distance = 0_f64;
    for point in &source.path.subpaths[0].anchors {
        let p = kurbo::Point::new(f64::from(point.position.x), f64::from(point.position.y));
        let mut nearest = f64::INFINITY;
        for i in 0..reduced.len() {
            let a = reduced[i].position;
            let b = reduced[(i + 1) % reduced.len()].position;
            let a = kurbo::Point::new(f64::from(a.x), f64::from(a.y));
            let b = kurbo::Point::new(f64::from(b.x), f64::from(b.y));
            let chord = b - a;
            let t = ((p - a).dot(chord) / chord.hypot2()).clamp(0., 1.);
            nearest = nearest.min(p.distance(a + chord * t));
        }
        max_distance = max_distance.max(nearest);
    }
    assert!(max_distance <= 0.5, "{max_distance}");
    eprintln!(
        "dense trace simplify: 5000 → {} nodes; maximum source-vertex distance {max_distance:.6}px",
        reduced.len()
    );
}

#[test]
fn simplify_keeps_sharp_polyline_corners_at_maximum_tolerance() {
    let source = rect(10., 10., 100., 100.);
    let output = simplify(&source, 100., &idle()).unwrap();
    assert_eq!(output.path, source.path);
}

#[test]
fn offset_grows_and_insets_rectangle_and_respects_hole_fill_rule() {
    let source = rect(20., 20., 40., 40.);
    let grown = offset_path(&source, 5., &idle()).unwrap();
    let inset = offset_path(&source, -5., &idle()).unwrap();
    let grow_pixels = mask(&grown, 90, 90);
    let inset_pixels = mask(&inset, 90, 90);
    assert!(grow_pixels[40 * 90 + 16]);
    assert!(!inset_pixels[40 * 90 + 24]);
    assert!(inset_pixels[40 * 90 + 26]);
    let mut ring = rect(10., 10., 60., 60.);
    ring.path.fill_rule = FillRule::EvenOdd;
    ring.path
        .subpaths
        .extend(rect(25., 25., 30., 30.).path.subpaths);
    let grown = mask(&offset_path(&ring, 4., &idle()).unwrap(), 90, 90);
    assert!(grown[40 * 90 + 27]);
    assert!(!grown[40 * 90 + 40]);
    let shrunk = mask(&offset_path(&ring, -4., &idle()).unwrap(), 90, 90);
    assert!(!shrunk[40 * 90 + 23]);
    assert!(shrunk[40 * 90 + 17]);
}

#[test]
fn large_insets_do_not_leave_spurious_islands() {
    for object in [rect(20., 20., 20., 20.), ellipse(20., 20., 20., 20.)] {
        assert!(offset_path(&object, -15., &idle()).unwrap().is_empty());
    }
}

#[test]
fn outline_open_stroke_respects_caps_dash_gaps_and_style() {
    let mut source = VectorObject::new(
        "line",
        VectorPath {
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: Point { x: 20., y: 40. },
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: Point { x: 80., y: 40. },
                        incoming: None,
                        outgoing: None,
                    },
                ],
            }],
            fill_rule: FillRule::NonZero,
        },
        None,
        Some(StrokeStyle {
            width: 10.,
            color: [12, 34, 56, 200],
        }),
    );
    source.stroke_options = Some(StrokeOptions {
        cap: StrokeCap::Butt,
        join: StrokeJoin::Miter,
        miter_limit: 4.,
        dashes: vec![10., 10.],
        dash_offset: 0.,
    });
    source.opacity = 0.6;
    let result = outline_stroke(&source, &idle()).unwrap();
    assert_eq!(result.len(), 3);
    assert!(
        result
            .iter()
            .all(|o| o.stroke.is_none() && o.fill == Some([12, 34, 56, 200]) && o.opacity == 0.6)
    );
    let pixels = mask(&result, 100, 80);
    assert!(pixels[40 * 100 + 25]);
    assert!(!pixels[40 * 100 + 35]);
    assert!(!pixels[40 * 100 + 19]);
    source.stroke_options = None;
    let round = mask(&outline_stroke(&source, &idle()).unwrap(), 100, 80);
    assert!(round[40 * 100 + 17]);
    assert!(round[40 * 100 + 82]);
}

#[test]
fn geometry_rejects_cancel_nonfinite_singular_dense_and_tiny_dash_work() {
    let source = rect(0., 0., 20., 20.);
    let saved = source.clone();
    let cancelled = AtomicBool::new(true);
    assert!(combine(&[source.clone()], BooleanOperation::Union, &cancelled).is_err());
    assert!(simplify(&source, 0.2, &cancelled).is_err());
    assert!(offset_path(&source, 2., &cancelled).is_err());
    assert!(simplify(&source, f32::NAN, &idle()).is_err());
    assert!(offset_path(&source, f32::INFINITY, &idle()).is_err());
    let mut invalid = source.clone();
    invalid.transform = [1., 0., 0., 0., 0., 0.];
    assert!(offset_path(&invalid, 2., &idle()).is_err());
    invalid = source.clone();
    invalid.path.subpaths[0].anchors =
        vec![invalid.path.subpaths[0].anchors[0].clone(); MAX_GEOMETRY_SEGMENTS + 1];
    assert!(combine(&[invalid], BooleanOperation::Union, &idle()).is_err());
    let mut tiny = source.clone();
    tiny.stroke = Some(StrokeStyle {
        width: 2.,
        color: [0; 4],
    });
    tiny.stroke_options = Some(StrokeOptions {
        dashes: vec![0.01, 0.01],
        ..Default::default()
    });
    assert!(outline_stroke(&tiny, &idle()).is_err());
    assert_eq!(source, saved);
}

#[test]
fn overlapping_curve_hulls_refuse_before_the_intersection_sweep() {
    let mut object = ellipse(0., 0., 100., 100.);
    object.path.subpaths = vec![object.path.subpaths[0].clone(); 128];
    let error = combine(&[object], BooleanOperation::Union, &idle()).unwrap_err();
    assert!(
        error.to_string().contains("intersection work limit"),
        "{error:#}"
    );
}
