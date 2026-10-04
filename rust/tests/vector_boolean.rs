use std::sync::atomic::AtomicBool;

use omuse::vector_boolean::{BooleanOperation, combine};
use omuse::vector_path::{FillRule, Subpath, VectorPath, rasterize_rgba};
use omuse::vector_scene::VectorObject;

fn rect(name: &str, x: f32, y: f32, w: f32, h: f32) -> VectorObject {
    VectorObject::rectangle(name, x, y, w, h, Some([255, 0, 0, 255]), None).unwrap()
}

fn run(a: &[VectorObject], op: BooleanOperation) -> Vec<VectorObject> {
    combine(a, op, &AtomicBool::new(false)).unwrap()
}

fn alpha_at(object: &VectorObject, x: u32, y: u32, width: u32, height: u32) -> u8 {
    rasterize_rgba(&object.path, width, height, object.fill, None, 0.25, || {
        false
    })
    .unwrap()
    .get_pixel(x, y)
    .0[3]
}

fn union_alpha_at(objects: &[VectorObject], x: u32, y: u32, width: u32, height: u32) -> u8 {
    objects
        .iter()
        .map(|object| alpha_at(object, x, y, width, height))
        .max()
        .unwrap_or(0)
}

fn filled_pixels(objects: &[VectorObject], width: u32, height: u32) -> usize {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| union_alpha_at(objects, x, y, width, height) > 0)
        .count()
}

#[test]
fn union_and_intersection_overlap() {
    let a = rect("A", 0., 0., 10., 10.);
    let b = rect("B", 5., 0., 10., 10.);
    assert_eq!(
        run(&[a.clone(), b.clone()], BooleanOperation::Union).len(),
        1
    );
    assert_eq!(run(&[a, b], BooleanOperation::Intersect).len(), 1);
}

#[test]
fn subtraction_and_exclude_are_empty_or_split_as_expected() {
    let a = rect("A", 0., 0., 10., 10.);
    let b = rect("B", 0., 0., 10., 10.);
    assert!(run(&[a.clone(), b.clone()], BooleanOperation::Subtract).is_empty());
    assert!(run(&[a, b], BooleanOperation::Exclude).is_empty());
}

#[test]
fn holes_and_transforms_are_preserved() {
    let mut path = VectorPath {
        subpaths: vec![
            Subpath {
                anchors: vec![],
                closed: true,
            },
            Subpath {
                anchors: vec![],
                closed: true,
            },
        ],
        fill_rule: FillRule::EvenOdd,
    };
    path.subpaths[0] = rect("outer", 0., 0., 20., 20.).path.subpaths[0].clone();
    path.subpaths[1] = rect("hole", 5., 5., 10., 10.).path.subpaths[0].clone();
    let mut a = VectorObject::new("hole", path, Some([1, 2, 3, 255]), None);
    a.transform = [1., 0., 0., 1., 10., 20.];
    let result = run(&[a], BooleanOperation::Union);
    assert_eq!(result.len(), 1);
    assert_eq!(alpha_at(&result[0], 20, 30, 40, 50), 0);
    assert!(alpha_at(&result[0], 12, 22, 40, 50) > 0);
}

#[test]
fn fill_rules_distinguish_same_and_opposite_winding() {
    let outer = rect("outer", 0., 0., 20., 20.);
    let mut inner_path = rect("inner", 5., 5., 10., 10.).path;
    let same = VectorObject::new(
        "same",
        VectorPath {
            subpaths: vec![
                outer.path.subpaths[0].clone(),
                inner_path.subpaths[0].clone(),
            ],
            fill_rule: FillRule::NonZero,
        },
        Some([1, 2, 3, 255]),
        None,
    );
    let non_zero = run(&[same], BooleanOperation::Union);
    assert_eq!(non_zero[0].path.subpaths.len(), 1);

    inner_path.subpaths[0].anchors.reverse();
    let opposite = VectorObject::new(
        "opposite",
        VectorPath {
            subpaths: vec![
                outer.path.subpaths[0].clone(),
                inner_path.subpaths[0].clone(),
            ],
            fill_rule: FillRule::NonZero,
        },
        Some([1, 2, 3, 255]),
        None,
    );
    let hole = run(&[opposite], BooleanOperation::Union);
    assert_eq!(alpha_at(&hole[0], 10, 10, 24, 24), 0);

    let even_odd = VectorObject::new(
        "evenodd",
        VectorPath {
            subpaths: vec![
                outer.path.subpaths[0].clone(),
                rect("inner", 5., 5., 10., 10.).path.subpaths[0].clone(),
            ],
            fill_rule: FillRule::EvenOdd,
        },
        Some([1, 2, 3, 255]),
        None,
    );
    assert_eq!(
        alpha_at(
            &run(&[even_odd], BooleanOperation::Union)[0],
            10,
            10,
            24,
            24
        ),
        0
    );
}

#[test]
fn tangent_and_disjoint_union_have_stable_components() {
    let tangent = run(
        &[rect("a", 0., 0., 10., 10.), rect("b", 10., 0., 10., 10.)],
        BooleanOperation::Union,
    );
    assert_eq!(filled_pixels(&tangent, 20, 10), 200);
    let disjoint = run(
        &[rect("a", 0., 0., 10., 10.), rect("b", 30., 0., 10., 10.)],
        BooleanOperation::Union,
    );
    assert_eq!(filled_pixels(&disjoint, 40, 10), 200);
}

#[test]
fn divide_returns_subject_partitions_and_curves_respect_world_transform() {
    let subject = rect("subject", 0., 0., 20., 20.);
    let cutter = rect("cutter", 8., -2., 4., 24.);
    let divided = run(&[subject, cutter], BooleanOperation::Divide);
    assert!(divided.len() >= 2);
    assert!(union_alpha_at(&divided, 2, 10, 20, 20) > 0);
    assert!(union_alpha_at(&divided, 10, 10, 20, 20) > 0);
    assert!(union_alpha_at(&divided, 18, 10, 20, 20) > 0);
    assert_eq!(filled_pixels(&divided, 20, 20), 400);

    let mut ellipse =
        VectorObject::ellipse("ellipse", 0., 0., 20., 10., Some([1, 2, 3, 255]), None).unwrap();
    ellipse.transform = [2., 0., 0., 2., 100., 50.];
    let output = run(&[ellipse], BooleanOperation::Union);
    let (lo, _) = output[0].path.bounds().unwrap();
    assert!(lo.x >= 100. && lo.y >= 50.);
}

#[test]
fn divide_applies_each_cutter_to_each_existing_partition() {
    let subject = rect("subject", 0., 0., 30., 20.);
    let first = rect("first", 8., -2., 4., 24.);
    let second = rect("second", 18., -2., 4., 24.);
    let divided = combine(
        &[subject, first, second],
        BooleanOperation::Divide,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(divided.len(), 5);
    for x in [2, 10, 15, 20, 27] {
        assert!(union_alpha_at(&divided, x, 10, 30, 20) > 0);
    }
    assert_eq!(filled_pixels(&divided, 30, 20), 600);
}

#[test]
fn rejects_open_and_stroke_only_inputs_and_honours_cancel() {
    let mut open = rect("open", 0., 0., 10., 10.);
    open.path.subpaths[0].closed = false;
    assert!(combine(&[open], BooleanOperation::Union, &AtomicBool::new(false)).is_err());
    let mut stroke = rect("stroke", 0., 0., 10., 10.);
    stroke.fill = None;
    stroke.stroke = Some(omuse::vector_path::StrokeStyle {
        color: [0; 4],
        width: 1.,
    });
    assert!(combine(&[stroke], BooleanOperation::Union, &AtomicBool::new(false)).is_err());
    let cancel = AtomicBool::new(true);
    assert!(
        combine(
            &[rect("cancel", 0., 0., 10., 10.)],
            BooleanOperation::Union,
            &cancel
        )
        .is_err()
    );
}
