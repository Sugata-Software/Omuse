use omuse::vector_path::{Point, StrokeStyle};
use omuse::vector_repeat::{RepeatSpec, generate};
use omuse::vector_scene::{
    GradientFill, GradientKind, GradientSpread, GradientStop, StrokeOptions, TextOnPath,
    TextPathAlignment, VectorGroup, VectorObject,
};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

const NAMESPACE: &str = "74af1746-680b-4554-9de0-70fbb1ef8e23";
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn rect() -> VectorObject {
    VectorObject::rectangle("Motif", 10., 20., 20., 10., Some([220, 100, 30, 255]), None).unwrap()
}
fn grid(columns: u32, rows: u32) -> RepeatSpec {
    RepeatSpec::Grid {
        columns,
        rows,
        column_step: Point { x: 40., y: 0. },
        row_step: Point { x: 0., y: 30. },
    }
}
fn radial(rotate_copies: bool) -> RepeatSpec {
    RepeatSpec::Radial {
        count: 4,
        center: Point { x: 0., y: 0. },
        angle_step_degrees: 90.,
        rotate_copies,
    }
}

#[test]
fn grid_keeps_originals_and_produces_row_major_editable_copies() {
    let source = vec![rect()];
    let original = source.clone();
    let spec = grid(3, 2);
    let result = generate(&source, &spec, NAMESPACE, &idle()).unwrap();
    assert_eq!(result.spec, spec);
    assert_eq!(result.source, source);
    assert_eq!(source, original);
    assert_eq!(result.objects.len(), 6);
    assert_eq!(result.objects[0], source[0]);
    assert_eq!(
        result.instance_ranges,
        vec![0..1, 1..2, 2..3, 3..4, 4..5, 5..6]
    );
    let translations: Vec<_> = result
        .objects
        .iter()
        .map(|o| (o.transform[4], o.transform[5]))
        .collect();
    assert_eq!(
        translations,
        vec![
            (0., 0.),
            (40., 0.),
            (80., 0.),
            (0., 30.),
            (40., 30.),
            (80., 30.)
        ]
    );
    assert!(
        result
            .objects
            .iter()
            .all(|o| o.path == source[0].path && o.fill == source[0].fill)
    );
    assert_eq!(
        result
            .objects
            .iter()
            .map(|o| &o.id)
            .collect::<HashSet<_>>()
            .len(),
        6
    );
}

#[test]
fn ids_are_deterministic_during_parameter_edits_and_namespaced_for_new_repeats() {
    let source = vec![rect()];
    let first = generate(&source, &grid(2, 1), NAMESPACE, &idle()).unwrap();
    let same = generate(&source, &grid(2, 1), NAMESPACE, &idle()).unwrap();
    assert_eq!(first, same);
    let changed = generate(&source, &grid(2, 2), NAMESPACE, &idle()).unwrap();
    assert_eq!(first.objects[1].id, changed.objects[1].id);
    let other = generate(
        &source,
        &grid(2, 1),
        "322dc791-dc27-4812-ae03-01da30380e16",
        &idle(),
    )
    .unwrap();
    assert_ne!(first.objects[1].id, other.objects[1].id);
    assert_eq!(
        uuid::Uuid::parse_str(&first.objects[1].id)
            .unwrap()
            .get_version_num(),
        8
    );
}

#[test]
fn group_hierarchies_are_cloned_as_independent_contiguous_instances() {
    let outer = VectorGroup {
        id: "22596e36-dcaa-4552-9fba-7c4d231d423b".into(),
        name: "Outer".into(),
    };
    let inner = VectorGroup {
        id: "82a89536-7ed9-468f-9c8d-5fa9869032f8".into(),
        name: "Inner".into(),
    };
    let mut a = rect();
    let mut b = rect();
    let mut c = rect();
    a.groups = vec![outer.clone(), inner.clone()];
    b.groups = a.groups.clone();
    c.groups = vec![outer.clone()];
    let source = vec![a, b, c];
    let result = generate(&source, &grid(2, 2), NAMESPACE, &idle()).unwrap();
    assert_eq!(&result.objects[..3], source.as_slice());
    for range in &result.instance_ranges[1..] {
        let block = &result.objects[range.clone()];
        assert_eq!(block[0].groups, block[1].groups);
        assert_eq!(block[0].groups[0], block[2].groups[0]);
        assert_ne!(block[0].groups[0].id, outer.id);
        assert_ne!(block[0].groups[1].id, inner.id);
        assert_eq!(block[0].groups[0].name, "Outer");
        assert_eq!(block[0].groups[1].name, "Inner");
    }
    assert_ne!(
        result.objects[3].groups[0].id,
        result.objects[6].groups[0].id
    );
}

#[test]
fn radial_rotation_composes_exact_quadrants_with_existing_transform() {
    let mut source = rect();
    source.transform = [2., 0., 1., 3., 10., 20.];
    let result = generate(&[source.clone()], &radial(true), NAMESPACE, &idle()).unwrap();
    assert_eq!(result.objects[0].transform, source.transform);
    assert_eq!(result.objects[1].transform, [0., 2., -3., 1., -20., 10.]);
    assert_eq!(result.objects[2].transform, [-2., 0., -1., -3., -10., -20.]);
    assert_eq!(result.objects[3].transform, [0., -2., 3., -1., 20., -10.]);
    assert!(result.objects.iter().all(|o| o.path == source.path));
}

#[test]
fn radial_without_rotation_translates_whole_motif_preserving_internal_spacing() {
    let a = rect();
    let mut b = rect();
    b.transform[4] = 40.;
    // Combined geometric bounds (10,20)..(70,30), centre (40,25).
    // A 90-degree turn about origin moves that centre to (-25,40).
    let result = generate(&[a.clone(), b.clone()], &radial(false), NAMESPACE, &idle()).unwrap();
    assert_eq!(result.objects[2].transform, [1., 0., 0., 1., -65., 15.]);
    assert_eq!(result.objects[3].transform, [1., 0., 0., 1., -25., 15.]);
    assert_eq!(result.objects[2].path, a.path);
    assert_eq!(result.objects[3].path, b.path);
}

#[test]
fn curves_gradients_strokes_opacity_and_text_recipe_remain_editable() {
    let mut source = VectorObject::ellipse(
        "Curved motif",
        20.,
        20.,
        30.,
        20.,
        Some([200, 100, 50, 255]),
        Some(StrokeStyle {
            width: 2.,
            color: [10, 20, 30, 200],
        }),
    )
    .unwrap();
    source.opacity = 0.6;
    source.transform = [2., 0., 0., 2., 5., 10.];
    source.stroke_options = Some(StrokeOptions {
        dashes: vec![3., 4.],
        ..Default::default()
    });
    source.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 20., y: 20. },
            end: Point { x: 50., y: 20. },
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
    let result = generate(&[source.clone()], &radial(true), NAMESPACE, &idle()).unwrap();
    for copy in &result.objects {
        assert_eq!(copy.path, source.path);
        assert_eq!(copy.fill_gradient, source.fill_gradient);
        assert_eq!(copy.stroke, source.stroke);
        assert_eq!(copy.stroke_options, source.stroke_options);
        assert_eq!(copy.opacity, source.opacity);
    }
    // A retained text recipe is cloned without reshaping or requiring fonts.
    let mut text = rect();
    text.text_path = Some(TextOnPath {
        text: "A".into(),
        font_family: "sans-serif".into(),
        font_size: 12.,
        letter_spacing: 0.,
        start_offset: 0.,
        alignment: TextPathAlignment::Start,
        guide: text.path.clone(),
        transform: [1., 0., 0., 1., 0., 0.],
        resolved_fonts: vec![],
    });
    let repeated = generate(&[text.clone()], &grid(2, 1), NAMESPACE, &idle()).unwrap();
    assert_eq!(repeated.objects[1].text_path, text.text_path);
}

#[test]
fn serialized_repeat_spec_round_trips_and_rejects_unknown_fields() {
    for spec in [grid(3, 2), radial(false), radial(true)] {
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<RepeatSpec>(&json).unwrap(), spec);
        let mut value = serde_json::to_value(&spec).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unrecognized".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<RepeatSpec>(value).is_err());
    }
    assert!(serde_json::from_str::<RepeatSpec>(r#"{"kind":"futureRepeat"}"#).is_err());
}

#[test]
fn invalid_counts_geometry_limits_and_cancel_do_not_modify_sources() {
    let source = vec![rect()];
    let before = source.clone();
    for spec in [
        grid(0, 1),
        grid(u32::MAX, 2),
        grid(257, 1),
        RepeatSpec::Radial {
            count: 1,
            center: Point { x: f32::NAN, y: 0. },
            angle_step_degrees: 45.,
            rotate_copies: true,
        },
        RepeatSpec::Radial {
            count: 1,
            center: Point::default(),
            angle_step_degrees: f32::INFINITY,
            rotate_copies: true,
        },
    ] {
        assert!(generate(&source, &spec, NAMESPACE, &idle()).is_err());
    }
    assert!(generate(&source, &grid(2, 1), "not-a-uuid", &idle()).is_err());
    assert!(generate(&source, &grid(2, 1), NAMESPACE, &AtomicBool::new(true)).is_err());
    let far = RepeatSpec::Grid {
        columns: 2,
        rows: 1,
        column_step: Point {
            x: 1_000_000.,
            y: 0.,
        },
        row_step: Point::default(),
    };
    assert!(generate(&source, &far, NAMESPACE, &idle()).is_err());
    let many: Vec<_> = (0..5).map(|_| rect()).collect();
    assert!(generate(&many, &grid(256, 1), NAMESPACE, &idle()).is_err());
    let mut dense = rect();
    dense.path.subpaths[0].anchors = vec![dense.path.subpaths[0].anchors[0].clone(); 500];
    assert!(generate(&[dense], &grid(256, 1), NAMESPACE, &idle()).is_err());
    assert_eq!(source, before);
}

#[test]
fn identity_repeat_is_exact_and_negative_skew_steps_are_supported() {
    let source = vec![rect()];
    assert_eq!(
        generate(&source, &grid(1, 1), NAMESPACE, &idle())
            .unwrap()
            .objects,
        source
    );
    let skew = RepeatSpec::Grid {
        columns: 2,
        rows: 2,
        column_step: Point { x: -20., y: 5. },
        row_step: Point { x: 7., y: -30. },
    };
    let copies = generate(&source, &skew, NAMESPACE, &idle()).unwrap();
    assert_eq!(copies.objects[3].transform, [1., 0., 0., 1., -13., -25.]);
}

#[test]
fn large_style_payload_is_refused_before_copy_allocation() {
    let mut object = rect();
    for i in 0..32 {
        object.groups.push(VectorGroup {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{i:02}{}", "g".repeat(15_900)),
        });
    }
    let error = generate(&[object], &grid(64, 1), NAMESPACE, &idle()).unwrap_err();
    assert!(error.to_string().contains("snapshot-size"), "{error:#}");
}
