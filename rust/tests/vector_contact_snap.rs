use omuse::vector_path::{Anchor, Point, Subpath, VectorPath};
use omuse::vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene};
use omuse::vector_snap::{
    ContactIndex, ContactKind, ContactOptions, ContactSegment, MAX_CONTACT_CANDIDATES,
    MAX_CONTACT_SEGMENTS, SegmentId,
};
use std::sync::atomic::AtomicBool;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}
fn id(n: usize) -> SegmentId {
    SegmentId {
        object: n,
        subpath: 0,
        segment: 0,
    }
}
fn line(n: usize) -> ContactSegment {
    ContactSegment {
        id: id(n),
        controls: [p(0., 0.), p(0., 0.), p(20., 0.), p(20., 0.)],
        curved: false,
    }
}
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn opts() -> ContactOptions {
    ContactOptions {
        tangent: true,
        perpendicular: true,
    }
}

#[test]
fn perpendicular_is_an_interior_segment_foot_not_a_clamped_endpoint() {
    let index = ContactIndex::build(&[line(0)], &idle()).unwrap();
    let hit = index
        .resolve(p(10., 5.), p(10.3, 0.2), 1., 6., opts(), &idle())
        .unwrap()
        .unwrap();
    assert_eq!(hit.point, p(10., 0.));
    assert_eq!(hit.kind, ContactKind::Perpendicular);
    assert!(
        (hit.direction.x * (hit.point.x - hit.from.x)
            + hit.direction.y * (hit.point.y - hit.from.y))
            .abs()
            < 1e-6
    );
    assert!(
        index
            .resolve(p(-5., 5.), p(0., 0.), 1., 6., opts(), &idle())
            .unwrap()
            .is_none()
    );
    assert!(
        index
            .resolve(p(10., 0.), p(10., 0.), 1., 6., opts(), &idle())
            .unwrap()
            .is_none()
    );
    assert!(
        index
            .resolve(
                p(10., 5.),
                p(10., 0.),
                1.,
                6.,
                ContactOptions {
                    tangent: true,
                    perpendicular: false
                },
                &idle()
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn cubic_tangency_matches_an_independent_curve_and_derivative_oracle() {
    use kurbo::{ParamCurve, ParamCurveDeriv};
    let curve = kurbo::CubicBez::new((0., 0.), (0., 20.), (20., 0.), (20., 20.));
    let t = 0.37;
    let q = curve.eval(t);
    let d = curve.deriv().eval(t);
    let from = p((q.x - d.x * 0.8) as f32, (q.y - d.y * 0.8) as f32);
    let segment = ContactSegment {
        id: id(0),
        controls: [p(0., 0.), p(0., 20.), p(20., 0.), p(20., 20.)],
        curved: true,
    };
    let index = ContactIndex::build(&[segment], &idle()).unwrap();
    let hit = index
        .resolve(
            from,
            p(q.x as f32 + 0.02, q.y as f32 - 0.01),
            4.,
            6.,
            opts(),
            &idle(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(hit.kind, ContactKind::Tangent);
    assert!((hit.point.x - q.x as f32).hypot(hit.point.y - q.y as f32) < 0.001);
    let ray = p(hit.point.x - from.x, hit.point.y - from.y);
    assert!((ray.x * hit.direction.y - ray.y * hit.direction.x).abs() < 1e-4);
    assert!(
        index
            .resolve(
                from,
                p(q.x as f32, q.y as f32),
                1.,
                6.,
                ContactOptions {
                    tangent: false,
                    perpendicular: true
                },
                &idle()
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn affine_scene_and_object_transforms_preserve_the_contact_oracle() {
    let object_transform = [0., 2., -3., 0., 70., 10.];
    let scene_transform = [1.5, 0.2, 0.3, 1., 5., 7.];
    let map = |m: [f32; 6], p: Point| Point {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    };
    let mut object = VectorObject::new(
        "Curve",
        VectorPath {
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: p(0., 0.),
                        incoming: None,
                        outgoing: Some(p(20. / 3., 0.)),
                    },
                    Anchor {
                        position: p(20., 20.),
                        incoming: Some(p(40. / 3., 20. / 3.)),
                        outgoing: None,
                    },
                ],
            }],
            fill_rule: Default::default(),
        },
        Some([20, 30, 40, 255]),
        None,
    );
    object.transform = object_transform;
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 200,
        height: 200,
        objects: vec![object],
    };
    let index = ContactIndex::from_scene(&scene, scene_transform, &idle()).unwrap();
    let from = map(scene_transform, map(object_transform, p(0., -5.)));
    let expected = map(scene_transform, map(object_transform, p(10., 5.)));
    let result = index
        .resolve(
            from,
            p(expected.x + 0.01, expected.y - 0.02),
            2.,
            6.,
            opts(),
            &idle(),
        )
        .unwrap()
        .unwrap();
    assert!((result.point.x - expected.x).hypot(result.point.y - expected.y) < 0.001);
}

#[test]
fn screen_tolerance_and_tie_breaks_are_stable_at_both_zoom_extremes() {
    let index = ContactIndex::build(&[line(8), line(2)], &idle()).unwrap();
    for zoom in [0.02, 1., 16.] {
        let within = p(10., 5. / zoom);
        let outside = p(10., 6.1 / zoom);
        let result = index
            .resolve(p(10., 5.), within, zoom, 6., opts(), &idle())
            .unwrap()
            .unwrap();
        assert_eq!(result.id, id(2));
        assert!(
            index
                .resolve(p(10., 5.), outside, zoom, 6., opts(), &idle())
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn degenerates_invalid_inputs_cancellation_and_busy_queries_fail_safely() {
    let point = ContactSegment {
        id: id(1),
        controls: [p(3., 3.); 4],
        curved: true,
    };
    let index = ContactIndex::build(&[point], &idle()).unwrap();
    assert!(
        index
            .resolve(p(2., 2.), p(3., 3.), 1., 6., opts(), &idle())
            .unwrap()
            .is_none()
    );
    assert!(
        index
            .resolve(p(f32::NAN, 2.), p(3., 3.), 1., 6., opts(), &idle())
            .is_err()
    );
    assert!(ContactIndex::build(&[line(0)], &AtomicBool::new(true)).is_err());
    let index = ContactIndex::build(&[line(0)], &idle()).unwrap();
    assert!(
        index
            .resolve(
                p(10., 5.),
                p(10., 0.),
                1.,
                6.,
                opts(),
                &AtomicBool::new(true)
            )
            .is_err()
    );
    let crowded = (0..=MAX_CONTACT_CANDIDATES).map(line).collect::<Vec<_>>();
    let index = ContactIndex::build(&crowded, &idle()).unwrap();
    assert!(
        index
            .resolve(p(10., 5.), p(10., 0.), 1., 6., opts(), &idle())
            .unwrap_err()
            .to_string()
            .contains("128")
    );
    assert!(
        ContactIndex::build(
            &(0..=MAX_CONTACT_SEGMENTS).map(line).collect::<Vec<_>>(),
            &idle()
        )
        .is_err()
    );
}
