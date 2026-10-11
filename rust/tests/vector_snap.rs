use omuse::canvas_grid::GridSettings;
use omuse::editor::GuideAxis;
use omuse::vector_path::Point;
use omuse::vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene};
use omuse::vector_snap::{
    AnchorId, AnchorTarget, AxisSnapKind, MAX_QUERY_CANDIDATES, MAX_SNAP_ANCHORS, MAX_SNAP_GUIDES,
    SnapGrid, SnapGuide, SnapIndex,
};
use std::sync::atomic::AtomicBool;

fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}
fn id(anchor: usize) -> AnchorId {
    AnchorId {
        object: 0,
        subpath: 0,
        anchor,
    }
}
fn target(anchor: usize, x: f32, y: f32) -> AnchorTarget {
    AnchorTarget {
        id: id(anchor),
        point: point(x, y),
    }
}
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn grid(spacing: u32, subdivisions: u8, origin: Point) -> SnapGrid {
    SnapGrid {
        settings: GridSettings {
            spacing,
            subdivisions,
        },
        origin,
    }
}
fn guide(id: usize, axis: GuideAxis, position: f32) -> SnapGuide {
    SnapGuide { id, axis, position }
}

#[test]
fn tolerance_is_measured_in_screen_pixels_at_near_and_far_zooms() {
    let index = SnapIndex::build(&[target(0, 100., 100.)], &[], None, &[], &idle()).unwrap();
    assert_eq!(
        index.resolve(point(100.3, 100.), 16., 6.).unwrap().anchor,
        Some(id(0))
    );
    assert_eq!(
        index.resolve(point(100.4, 100.), 16., 6.).unwrap().anchor,
        None
    );
    assert_eq!(
        index.resolve(point(399., 100.), 0.02, 6.).unwrap().anchor,
        Some(id(0))
    );
    assert_eq!(
        index.resolve(point(401., 100.), 0.02, 6.).unwrap().anchor,
        None
    );
    assert_eq!(
        index.resolve(point(105., 100.), 1., 6.).unwrap().point,
        point(100., 100.)
    );
    assert_eq!(
        index.resolve(point(105., 100.), 2., 6.).unwrap().point,
        point(105., 100.)
    );
}

#[test]
fn guides_and_grid_snap_axes_independently_with_explicit_indicators() {
    let index = SnapIndex::build(
        &[],
        &[guide(5, GuideAxis::Vertical, 13.)],
        Some(grid(10, 1, point(0., 0.))),
        &[],
        &idle(),
    )
    .unwrap();
    let result = index.resolve(point(12., 19.), 1., 3.).unwrap();
    assert_eq!(result.point, point(13., 20.));
    assert_eq!(result.vertical.unwrap().kind, AxisSnapKind::Guide(5));
    assert_eq!(result.horizontal.unwrap().kind, AxisSnapKind::Grid);
    assert_eq!(result.anchor, None);
    let result = index.resolve(point(12., 25.), 1., 3.).unwrap();
    assert_eq!(result.point, point(13., 25.));
    assert!(result.horizontal.is_none());
}

#[test]
fn anchor_full_pair_has_priority_and_uses_circular_tolerance() {
    let index = SnapIndex::build(
        &[target(3, 14., 13.)],
        &[],
        Some(grid(10, 1, point(0., 0.))),
        &[],
        &idle(),
    )
    .unwrap();
    let result = index.resolve(point(10., 10.), 1., 6.).unwrap();
    assert_eq!(result.point, point(14., 13.));
    assert_eq!(result.anchor, Some(id(3)));
    assert!(result.vertical.is_none() && result.horizontal.is_none());
    let index = SnapIndex::build(&[target(3, 14., 14.)], &[], None, &[], &idle()).unwrap();
    assert_eq!(index.resolve(point(10., 10.), 1., 5.).unwrap().anchor, None);
}

#[test]
fn excluded_dragged_anchors_cannot_snap_to_themselves_or_hide_coincident_targets() {
    let index = SnapIndex::build(
        &[
            target(0, 10., 10.),
            target(1, 12., 10.),
            target(2, 10., 10.),
        ],
        &[],
        None,
        &[id(0), id(2)],
        &idle(),
    )
    .unwrap();
    assert_eq!(index.anchor_count(), 1);
    assert_eq!(
        index.resolve(point(10., 10.), 1., 6.).unwrap().anchor,
        Some(id(1))
    );
    let index = SnapIndex::build(
        &[target(0, 10., 10.), target(2, 10., 10.)],
        &[],
        None,
        &[id(0)],
        &idle(),
    )
    .unwrap();
    assert_eq!(
        index.resolve(point(10., 10.), 1., 0.).unwrap().anchor,
        Some(id(2))
    );
}

#[test]
fn anchor_ties_are_stable_across_input_order_and_bucket_boundaries() {
    let targets = [target(7, 62., 0.), target(2, 66., 0.), target(1, 66., -0.)];
    let index = SnapIndex::build(&targets, &[], None, &[], &idle()).unwrap();
    let reversed = SnapIndex::build(
        &targets.into_iter().rev().collect::<Vec<_>>(),
        &[],
        None,
        &[],
        &idle(),
    )
    .unwrap();
    assert_eq!(index.anchor_count(), 2);
    assert_eq!(
        index.resolve(point(64., 0.), 1., 6.).unwrap().anchor,
        Some(id(1))
    );
    assert_eq!(
        index.resolve(point(64., 0.), 1., 6.).unwrap(),
        reversed.resolve(point(64., 0.), 1., 6.).unwrap()
    );
}

#[test]
fn axis_ties_prefer_guides_then_lowest_identity_not_input_order() {
    let guides = [
        guide(7, GuideAxis::Vertical, 8.),
        guide(2, GuideAxis::Vertical, 12.),
        guide(1, GuideAxis::Vertical, 12.),
    ];
    let index =
        SnapIndex::build(&[], &guides, Some(grid(4, 1, point(0., 0.))), &[], &idle()).unwrap();
    let result = index.resolve(point(10., 10.), 1., 3.).unwrap();
    assert_eq!(result.vertical.unwrap().kind, AxisSnapKind::Guide(1));
    assert_eq!(result.point.x, 12.);
    let closer_grid = index.resolve(point(0.1, 10.), 1., 3.).unwrap();
    assert_eq!(closer_grid.vertical.unwrap().kind, AxisSnapKind::Grid);
}

#[test]
fn negative_origins_and_subdivisions_keep_the_exact_grid_lattice() {
    let index =
        SnapIndex::build(&[], &[], Some(grid(12, 3, point(-5., -7.))), &[], &idle()).unwrap();
    assert_eq!(
        index.resolve(point(-9.2, -10.8), 1., 1.).unwrap().point,
        point(-9., -11.)
    );
    let index =
        SnapIndex::build(&[], &[], Some(grid(10, 1, point(-5., -5.))), &[], &idle()).unwrap();
    assert_eq!(
        index.resolve(point(0., -10.), 1., 5.).unwrap().point,
        point(5., -15.)
    );
}

#[test]
fn cache_handles_all_document_anchors_without_a_full_scan_per_query() {
    let targets = (0..MAX_SNAP_ANCHORS)
        .map(|i| target(i, (i % 1000) as f32 * 128., (i / 1000) as f32 * 128.))
        .collect::<Vec<_>>();
    let index = SnapIndex::build(&targets, &[], None, &[], &idle()).unwrap();
    assert_eq!(index.anchor_count(), MAX_SNAP_ANCHORS);
    let result = index
        .resolve(point(636. * 128. + 2., 49. * 128. + 1.), 1., 6.)
        .unwrap();
    assert_eq!(result.anchor, Some(id(49_636)));
    assert!(result.examined_anchors <= 4, "{}", result.examined_anchors);
    let far = index.resolve(point(-500., -500.), 0.02, 6.).unwrap();
    assert!(far.anchor.is_none());
    assert_eq!(far.examined_anchors, 0);
}

#[test]
fn dense_queries_fail_atomically_instead_of_returning_a_partial_nearest_target() {
    let targets = (0..=MAX_QUERY_CANDIDATES)
        .map(|i| target(i, i as f32 * 0.0001, 0.))
        .collect::<Vec<_>>();
    let index = SnapIndex::build(&targets, &[], None, &[], &idle()).unwrap();
    let error = index
        .resolve(point(0., 0.), 1., 6.)
        .unwrap_err()
        .to_string();
    assert!(error.contains("4096 candidates"), "{error}");
    // A tighter query still uses the same cache successfully.
    let result = index.resolve(point(0., 0.), 16., 0.001).unwrap();
    assert_eq!(result.anchor, Some(id(0)));
    assert!(result.examined_anchors < 10);
}

#[test]
fn construction_limits_duplicate_identities_and_cancellation_are_explicit() {
    let too_many = vec![target(0, 0., 0.); MAX_SNAP_ANCHORS + 1];
    assert!(
        SnapIndex::build(&too_many, &[], None, &[], &idle())
            .unwrap_err()
            .to_string()
            .contains("100000")
    );
    assert!(
        SnapIndex::build(
            &[target(0, 0., 0.), target(0, 1., 0.)],
            &[],
            None,
            &[],
            &idle()
        )
        .is_err()
    );
    let too_many_guides = vec![guide(0, GuideAxis::Vertical, 0.); MAX_SNAP_GUIDES + 1];
    assert!(SnapIndex::build(&[], &too_many_guides, None, &[], &idle()).is_err());
    assert!(
        SnapIndex::build(
            &[],
            &[
                guide(0, GuideAxis::Vertical, 0.),
                guide(0, GuideAxis::Horizontal, 0.)
            ],
            None,
            &[],
            &idle()
        )
        .is_err()
    );
    assert!(
        SnapIndex::build(&[], &[], None, &[], &AtomicBool::new(true))
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
}

#[test]
fn nonfinite_and_out_of_range_inputs_do_not_enter_cache_or_queries() {
    let index = SnapIndex::build(&[], &[], None, &[], &idle()).unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.] {
        assert!(SnapIndex::build(&[target(0, value, 0.)], &[], None, &[], &idle()).is_err());
        assert!(
            SnapIndex::build(
                &[],
                &[guide(0, GuideAxis::Vertical, value)],
                None,
                &[],
                &idle()
            )
            .is_err()
        );
        assert!(
            SnapIndex::build(&[], &[], Some(grid(8, 1, point(value, 0.))), &[], &idle()).is_err()
        );
        assert!(index.resolve(point(value, 0.), 1., 6.).is_err());
    }
    for zoom in [0., -1., 0.019, 16.1, f32::NAN, f32::INFINITY] {
        assert!(index.resolve(point(0., 0.), zoom, 6.).is_err());
    }
    for tolerance in [-1., 32.1, f32::NAN, f32::INFINITY] {
        assert!(index.resolve(point(0., 0.), 1., tolerance).is_err());
    }
    assert!(SnapIndex::build(&[], &[], Some(grid(0, 1, point(0., 0.))), &[], &idle()).is_err());
    assert!(SnapIndex::build(&[], &[], Some(grid(8, 0, point(0., 0.))), &[], &idle()).is_err());
    assert!(
        index
            .resolve(point(1_000_000., -1_000_000.), 0.02, 32.)
            .is_ok()
    );
}

#[test]
fn scene_constructor_composes_transforms_ignores_hidden_and_preserves_source() {
    let mut object =
        VectorObject::rectangle("Visible", 0., 0., 10., 10., Some([255; 4]), None).unwrap();
    object.transform = [0., 2., -3., 0., 50., 20.];
    let mut hidden = object.clone();
    hidden.id = uuid::Uuid::new_v4().to_string();
    hidden.visible = false;
    let mut transparent = object.clone();
    transparent.id = uuid::Uuid::new_v4().to_string();
    transparent.opacity = 0.;
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 100,
        height: 100,
        objects: vec![object, hidden, transparent],
    };
    let before = scene.clone();
    let index = SnapIndex::from_scene(
        &scene,
        [2., 0., 0., 0.5, 10., -5.],
        &[],
        None,
        &[id(1)],
        &idle(),
    )
    .unwrap();
    assert_eq!(index.anchor_count(), 3);
    assert_eq!(
        index.resolve(point(111., 6.), 1., 6.).unwrap().point,
        point(110., 5.)
    );
    assert_eq!(
        index.resolve(point(110., 15.), 1., 1.).unwrap().anchor,
        None
    );
    assert_eq!(scene, before);
    assert!(SnapIndex::from_scene(&scene, [0.; 6], &[], None, &[], &idle()).is_err());
    assert!(
        SnapIndex::from_scene(
            &scene,
            [1., 0., 0., 1., 1_000_000., 0.],
            &[],
            None,
            &[],
            &idle()
        )
        .is_err()
    );
    assert!(
        SnapIndex::from_scene(
            &scene,
            [1., 0., 0., 1., 0., 0.],
            &[],
            None,
            &[],
            &AtomicBool::new(true)
        )
        .is_err()
    );
}

#[test]
fn disabled_sources_leave_the_point_unchanged_for_caller_controlled_bypass() {
    let index = SnapIndex::build(&[], &[], None, &[], &idle()).unwrap();
    let query = point(-123.4, 567.8);
    let result = index.resolve(query, 1., 6.).unwrap();
    assert_eq!(result.point, query);
    assert!(result.anchor.is_none() && result.vertical.is_none() && result.horizontal.is_none());
    // Shift/Alt are intentionally not API inputs: the UI bypasses resolution
    // before applying its existing angular and independent-handle constraints.
}
