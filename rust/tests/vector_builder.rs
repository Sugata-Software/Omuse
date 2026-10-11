use omuse::vector_builder::{MAX_BUILDER_INPUT_ANCHORS, prepare};
use omuse::vector_path::{FillRule, Point, StrokeStyle};
use omuse::vector_scene::{
    GradientFill, GradientKind, GradientSpread, GradientStop, VECTOR_SCENE_VERSION, VectorObject,
    VectorScene,
};
use std::sync::atomic::AtomicBool;

fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn rect(name: &str, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) -> VectorObject {
    VectorObject::rectangle(name, x, y, w, h, Some(color), None).unwrap()
}
fn render(objects: Vec<VectorObject>) -> image::RgbaImage {
    VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 160,
        height: 120,
        objects,
    }
    .render(&idle())
    .unwrap()
}
fn basic() -> Vec<VectorObject> {
    vec![
        rect("lower", 10., 10., 60., 60., [255, 0, 0, 255]),
        rect("upper", 40., 10., 60., 60., [0, 0, 255, 255]),
    ]
}

#[test]
fn regions_cover_whole_union_preserve_topmost_fill_and_do_not_mutate_sources() {
    let source = basic();
    let original = source.clone();
    let builder = prepare(&source, &idle()).unwrap();
    assert_eq!(builder.regions.len(), 3);
    assert_eq!(source, original);
    for region in &builder.regions {
        assert_eq!(
            region.object.name,
            format!("Shape · region {}", region.id + 1)
        );
    }
    let exclusive_first = builder.hit_test(Point { x: 20., y: 40. }).unwrap();
    let overlap = builder.hit_test(Point { x: 50., y: 40. }).unwrap();
    let exclusive_last = builder.hit_test(Point { x: 90., y: 40. }).unwrap();
    assert_eq!(builder.regions[exclusive_first].sources, vec![0]);
    assert_eq!(builder.regions[overlap].sources, vec![0, 1]);
    assert_eq!(builder.regions[exclusive_last].sources, vec![1]);
    assert_eq!(builder.regions[overlap].object.fill, source[1].fill);
    assert_eq!(
        render(builder.regions.iter().map(|r| r.object.clone()).collect()),
        render(source)
    );
    assert_eq!(builder.hit_test(Point { x: 120., y: 40. }), None);
}

#[test]
fn painted_merge_uses_first_region_style_keeps_unpainted_regions() {
    let builder = prepare(&basic(), &idle()).unwrap();
    let red = builder.hit_test(Point { x: 20., y: 40. }).unwrap();
    let overlap = builder.hit_test(Point { x: 50., y: 40. }).unwrap();
    let combined = builder.merge(&[red, overlap, red], &idle()).unwrap();
    assert_eq!(combined.last().unwrap().name, "Merged shape");
    // Preparing an already generated result must not accumulate name suffixes.
    let again = prepare(&combined, &idle()).unwrap();
    for region in &again.regions {
        assert_eq!(
            region.object.name,
            format!("Shape · region {}", region.id + 1)
        );
    }
    let image = render(combined);
    assert_eq!(image.get_pixel(20, 40).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(50, 40).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(90, 40).0, [0, 0, 255, 255]);
    let reversed = render(builder.merge(&[overlap, red], &idle()).unwrap());
    assert_eq!(reversed.get_pixel(20, 40).0, [0, 0, 255, 255]);
    assert_eq!(builder.regions.len(), 3);
}

#[test]
fn erase_only_painted_region_and_all_regions_may_be_empty() {
    let builder = prepare(&basic(), &idle()).unwrap();
    let overlap = builder.hit_test(Point { x: 50., y: 40. }).unwrap();
    let image = render(builder.erase(&[overlap], &idle()).unwrap());
    assert_eq!(image.get_pixel(50, 40)[3], 0);
    assert_eq!(image.get_pixel(20, 40)[3], 255);
    assert_eq!(image.get_pixel(90, 40)[3], 255);
    let all: Vec<_> = builder.regions.iter().map(|r| r.id).collect();
    assert!(builder.erase(&all, &idle()).unwrap().is_empty());
}

#[test]
fn holes_and_separate_islands_hit_correctly_with_curves_retained() {
    let mut ring =
        VectorObject::ellipse("ring", 10., 10., 100., 100., Some([255, 100, 0, 255]), None)
            .unwrap();
    ring.path.fill_rule = FillRule::EvenOdd;
    ring.path.subpaths.extend(
        VectorObject::ellipse("hole", 30., 30., 60., 60., Some([0, 0, 0, 255]), None)
            .unwrap()
            .path
            .subpaths,
    );
    let island = rect("island", 55., 55., 10., 10., [0, 0, 255, 255]);
    let builder = prepare(&[ring, island], &idle()).unwrap();
    assert_eq!(builder.regions.len(), 2);
    assert!(builder.hit_test(Point { x: 15., y: 60. }).is_some());
    assert_eq!(builder.hit_test(Point { x: 40., y: 60. }), None);
    assert!(builder.hit_test(Point { x: 60., y: 60. }).is_some());
    assert!(
        builder
            .regions
            .iter()
            .flat_map(|r| &r.object.path.subpaths)
            .flat_map(|s| &s.anchors)
            .any(|a| a.incoming.is_some())
    );
    let merged = builder.merge(&[0, 1], &idle()).unwrap();
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].name, "Merged shape · part 1");
    assert_eq!(merged[1].name, "Merged shape · part 2");
}

#[test]
fn all_three_source_membership_and_full_union_are_retained() {
    let mut source = basic();
    source.push(rect("third", 20., 30., 60., 60., [0, 255, 0, 255]));
    let original = source.clone();
    let builder = prepare(&source, &idle()).unwrap();
    assert_eq!(source, original);
    for region in &builder.regions {
        assert_eq!(
            region.object.name,
            format!("Shape · region {}", region.id + 1)
        );
    }
    let triple = builder.hit_test(Point { x: 50., y: 40. }).unwrap();
    assert_eq!(builder.regions[triple].sources, vec![0, 1, 2]);
    assert_eq!(builder.regions[triple].object.fill, Some([0, 255, 0, 255]));
    assert_eq!(
        render(builder.regions.iter().map(|r| r.object.clone()).collect()),
        render(source)
    );
}

#[test]
fn transformed_gradient_stays_in_world_space_when_regions_are_split() {
    let mut source = basic();
    source[1].transform = [1., 0., 0., 1., 5., 10.];
    source[1].fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 40., y: 10. },
            end: Point { x: 100., y: 10. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [255, 255, 0, 255],
            },
            GradientStop {
                offset: 1.,
                color: [0, 0, 255, 255],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
    });
    let builder = prepare(&source, &idle()).unwrap();
    let actual = render(builder.regions.iter().map(|r| r.object.clone()).collect());
    let expected = render(source);
    for (x, y) in [(20, 40), (50, 40), (90, 40)] {
        assert_eq!(actual.get_pixel(x, y), expected.get_pixel(x, y));
    }
    assert!(
        builder
            .regions
            .iter()
            .all(|r| r.object.transform == [1., 0., 0., 1., 0., 0.])
    );
}

#[test]
fn strokes_translucency_hidden_objects_dense_paths_and_bad_counts_refuse_atomically() {
    let source = basic();
    assert!(prepare(&source[..1], &idle()).is_err());
    let many: Vec<_> = (0..9)
        .map(|i| rect("too many", i as f32, 0., 10., 10., [0, 0, 0, 255]))
        .collect();
    assert!(prepare(&many, &idle()).is_err());
    for mode in 0..4 {
        let mut invalid = source.clone();
        match mode {
            0 => {
                invalid[0].stroke = Some(StrokeStyle {
                    color: [0, 0, 0, 255],
                    width: 1.,
                })
            }
            1 => invalid[0].opacity = 0.5,
            2 => invalid[0].fill = Some([0, 0, 0, 100]),
            _ => invalid[0].visible = false,
        }
        let original = invalid.clone();
        assert!(prepare(&invalid, &idle()).is_err());
        assert_eq!(invalid, original);
    }
    let mut dense = source.clone();
    dense[0].path.subpaths[0].anchors =
        vec![dense[0].path.subpaths[0].anchors[0].clone(); MAX_BUILDER_INPUT_ANCHORS];
    assert!(prepare(&dense, &idle()).is_err());
}

#[test]
fn cancellation_invalid_region_ids_and_nonfinite_hit_leave_snapshot_intact() {
    let source = basic();
    let cancelled = AtomicBool::new(true);
    assert!(prepare(&source, &cancelled).is_err());
    let builder = prepare(&source, &idle()).unwrap();
    let before: Vec<_> = builder.regions.iter().map(|r| r.object.clone()).collect();
    assert!(builder.merge(&[0], &cancelled).is_err());
    assert!(builder.erase(&[0], &cancelled).is_err());
    assert!(builder.merge(&[], &idle()).is_err());
    assert!(builder.erase(&[999], &idle()).is_err());
    assert_eq!(builder.hit_test(Point { x: f32::NAN, y: 0. }), None);
    assert_eq!(
        before,
        builder
            .regions
            .iter()
            .map(|r| r.object.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn dense_grid_stops_at_the_aggregate_work_budget_without_partial_output() {
    let mut source = Vec::new();
    for i in 0..4 {
        source.push(rect(
            "vertical",
            20. + i as f32 * 15.,
            10.,
            4.,
            90.,
            [255, 0, 0, 255],
        ));
    }
    for i in 0..4 {
        source.push(rect(
            "horizontal",
            10.,
            20. + i as f32 * 15.,
            90.,
            4.,
            [0, 0, 255, 255],
        ));
    }
    let before = source.clone();
    let error = prepare(&source, &idle()).unwrap_err();
    assert!(
        error.to_string().contains("budget") || error.to_string().contains("limit"),
        "{error:#}"
    );
    assert_eq!(source, before);
}

#[test]
fn fast_drag_hits_narrow_regions_in_traversal_order() {
    let source = vec![
        rect("wide", 10., 10., 100., 80., [255, 0, 0, 255]),
        rect("thin", 59., 10., 2., 80., [0, 0, 255, 255]),
    ];
    let builder = prepare(&source, &idle()).unwrap();
    let left = builder.hit_test(Point { x: 20., y: 40. }).unwrap();
    let thin = builder.hit_test(Point { x: 60., y: 40. }).unwrap();
    let right = builder.hit_test(Point { x: 100., y: 40. }).unwrap();
    assert_eq!(
        builder
            .hit_test_segment(Point { x: 0., y: 40. }, Point { x: 120., y: 40. })
            .unwrap(),
        vec![left, thin, right]
    );
    assert_eq!(
        builder
            .hit_test_segment(Point { x: 120., y: 40. }, Point { x: 0., y: 40. })
            .unwrap(),
        vec![right, thin, left]
    );
    assert_eq!(
        builder
            .hit_test_segment(Point { x: 20., y: 40. }, Point { x: 20., y: 40. })
            .unwrap(),
        vec![left]
    );
}

#[test]
fn drag_entirely_inside_a_hole_does_not_hit_the_surrounding_region() {
    let mut ring =
        VectorObject::ellipse("ring", 10., 10., 100., 100., Some([255, 0, 0, 255]), None).unwrap();
    ring.path.fill_rule = FillRule::EvenOdd;
    ring.path.subpaths.extend(
        VectorObject::ellipse("hole", 30., 30., 60., 60., Some([0, 0, 0, 255]), None)
            .unwrap()
            .path
            .subpaths,
    );
    let other = rect("other", 120., 10., 10., 10., [0, 0, 255, 255]);
    let builder = prepare(&[ring, other], &idle()).unwrap();
    assert!(
        builder
            .hit_test_segment(Point { x: 40., y: 60. }, Point { x: 80., y: 60. })
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        builder
            .hit_test_segment(Point { x: 0., y: 60. }, Point { x: 115., y: 60. })
            .unwrap()
            .len(),
        1
    );
    assert!(
        builder
            .hit_test_segment(
                Point {
                    x: f32::INFINITY,
                    y: 0.
                },
                Point { x: 1., y: 0. }
            )
            .is_err()
    );
}
