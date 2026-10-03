use omuse::vector_path::Point;
use omuse::{
    vector_scene::{VECTOR_SCENE_LEGACY_VERSION, VectorGroup, VectorObject, VectorScene},
    vector_scene_ops::{self, AlignAxis, DistributeAxis},
};

fn scene() -> VectorScene {
    VectorScene {
        version: omuse::vector_scene::VECTOR_SCENE_VERSION,
        width: 200,
        height: 100,
        objects: (0..4)
            .map(|index| {
                VectorObject::rectangle(
                    format!("Object {index}"),
                    index as f32 * 20.,
                    10.,
                    10.,
                    10.,
                    Some([255, 0, 0, 255]),
                    None,
                )
                .unwrap()
            })
            .collect(),
    }
}

#[test]
fn group_reorders_selected_block_and_round_trips_group_paths() {
    let original = scene();
    let (grouped, selected) = vector_scene_ops::group(&original, &[0, 2], "Marks").unwrap();
    assert_eq!(selected, vec![1, 2]);
    assert_eq!(
        grouped
            .objects
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Object 1", "Object 0", "Object 2", "Object 3"]
    );
    assert!(grouped.objects[1].groups[0].id == grouped.objects[2].groups[0].id);
    assert!(
        original
            .objects
            .iter()
            .all(|object| object.groups.is_empty())
    );
    grouped.validate().unwrap();
    let encoded = serde_json::to_vec(&grouped).unwrap();
    let reopened: VectorScene = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(reopened, grouped);
}

#[test]
fn grouping_a_legacy_flat_scene_upgrades_only_the_result() {
    let mut legacy = scene();
    legacy.version = VECTOR_SCENE_LEGACY_VERSION;
    let (grouped, _) = vector_scene_ops::group(&legacy, &[0, 1], "Marks").unwrap();
    assert_eq!(legacy.version, VECTOR_SCENE_LEGACY_VERSION);
    assert_eq!(
        grouped.version,
        omuse::vector_scene::VECTOR_SCENE_GROUP_VERSION
    );
}

#[test]
fn grouping_fully_selected_sibling_groups_creates_a_parent() {
    let (first, _) = vector_scene_ops::group(&scene(), &[0, 1], "A").unwrap();
    let (second, _) = vector_scene_ops::group(&first, &[2, 3], "B").unwrap();
    let (nested, _) = vector_scene_ops::group(&second, &[0, 1, 2, 3], "Both").unwrap();
    assert!(nested.objects.iter().all(|object| object.groups.len() == 2));
    nested.validate().unwrap();
}

#[test]
fn grouping_and_ungrouping_preserve_nested_children() {
    let (first, _) = vector_scene_ops::group(&scene(), &[0, 1], "A").unwrap();
    let (second, _) = vector_scene_ops::group(&first, &[2, 3], "B").unwrap();
    let (nested, selected) = vector_scene_ops::group(&second, &[0, 1, 2, 3], "Both").unwrap();
    let (restored, retained) = vector_scene_ops::ungroup(&nested, &selected).unwrap();
    assert_eq!(retained, selected);
    assert_eq!(restored.objects[0].groups[0].name, "A");
    assert_eq!(restored.objects[2].groups[0].name, "B");
    restored.validate().unwrap();
}

#[test]
fn grouping_mixed_complete_group_and_free_object_preserves_suffixes() {
    let (grouped, _) = vector_scene_ops::group(&scene(), &[0, 1], "A").unwrap();
    let (wrapped, selected) = vector_scene_ops::group(&grouped, &[0, 1, 2], "Wrapper").unwrap();
    assert_eq!(selected, vec![0, 1, 2]);
    assert_eq!(wrapped.objects[0].groups[0].name, "Wrapper");
    assert_eq!(wrapped.objects[0].groups[1].name, "A");
    assert_eq!(wrapped.objects[2].groups.len(), 1);
    wrapped.validate().unwrap();
}

#[test]
fn reorder_moves_whole_groups_and_ungrouped_siblings() {
    let (grouped, _) = vector_scene_ops::group(&scene(), &[0, 1], "A").unwrap();
    let (moved, selected) = vector_scene_ops::reorder(&grouped, &[0, 1], true).unwrap();
    assert_eq!(selected, vec![1, 2]);
    assert_eq!(moved.objects[1].groups[0].name, "A");
    assert_eq!(moved.objects[2].groups[0].name, "A");

    let (individual, selected) = vector_scene_ops::reorder(&moved, &[0], true).unwrap();
    assert_eq!(selected, vec![2]);
    assert_eq!(individual.objects[2].name, "Object 2");
}

#[test]
fn reorder_rejects_crossing_partial_group_boundaries() {
    let (first, _) = vector_scene_ops::group(&scene(), &[0, 1], "A").unwrap();
    let (grouped, _) = vector_scene_ops::group(&first, &[2, 3], "B").unwrap();
    assert!(vector_scene_ops::reorder(&grouped, &[0, 2], true).is_err());
}

#[test]
fn ungroup_and_expand_use_outermost_membership() {
    let (grouped, selected) = vector_scene_ops::group(&scene(), &[1, 2], "Marks").unwrap();
    assert_eq!(
        vector_scene_ops::expand_selection(&grouped, &[selected[0]]).unwrap(),
        selected
    );
    let (ungrouped, retained) = vector_scene_ops::ungroup(&grouped, &[selected[0]]).unwrap();
    assert_eq!(retained, selected);
    assert!(
        ungrouped
            .objects
            .iter()
            .all(|object| object.groups.is_empty())
    );
}

#[test]
fn expansion_keeps_selected_ungrouped_objects() {
    let (grouped, selected) = vector_scene_ops::group(&scene(), &[1, 2], "Marks").unwrap();
    let expanded = vector_scene_ops::expand_selection(&grouped, &[0, selected[0]]).unwrap();
    assert_eq!(expanded, vec![0, selected[0], selected[1]]);
}

#[test]
fn selection_transforms_are_transactional_and_preserve_the_original() {
    let original = scene();
    let moved = vector_scene_ops::translate(&original, &[0, 1], 5., 7.).unwrap();
    assert_eq!(original.objects[0].transform, [1., 0., 0., 1., 0., 0.]);
    assert_eq!(moved.objects[0].transform, [1., 0., 0., 1., 5., 7.]);
    let scaled = vector_scene_ops::scale(&moved, &[0, 1], 2., 2., Point { x: 0., y: 0. }).unwrap();
    assert_eq!(scaled.objects[0].transform, [2., 0., 0., 2., 10., 14.]);
    let rotated = vector_scene_ops::rotate(&scaled, &[0], 90., Point { x: 0., y: 0. }).unwrap();
    assert!((rotated.objects[0].transform[0]).abs() < 1e-5);
}

#[test]
fn align_and_distribute_return_valid_scene() {
    let mut source = scene();
    source.objects[1].transform[4] = 35.;
    source.objects[2].transform[4] = 80.;
    let aligned = vector_scene_ops::align(&source, &[0, 1, 2], AlignAxis::Top).unwrap();
    let top = vector_scene_ops::bounds(&aligned, &[0])
        .unwrap()
        .unwrap()
        .top;
    assert!(
        vector_scene_ops::bounds(&aligned, &[1])
            .unwrap()
            .unwrap()
            .top
            == top
    );
    let distributed =
        vector_scene_ops::distribute(&source, &[0, 1, 2], DistributeAxis::Horizontal).unwrap();
    assert_eq!(
        vector_scene_ops::bounds(&distributed, &[0])
            .unwrap()
            .unwrap()
            .left,
        0.
    );
    assert_eq!(
        vector_scene_ops::bounds(&distributed, &[1])
            .unwrap()
            .unwrap()
            .left,
        60.
    );
    assert_eq!(
        vector_scene_ops::bounds(&distributed, &[2])
            .unwrap()
            .unwrap()
            .left,
        120.
    );
    distributed.validate().unwrap();
}

#[test]
fn bounds_include_curve_control_points() {
    let mut source = scene();
    source.objects[0].path.subpaths[0].anchors[0].outgoing = Some(Point { x: 120., y: -20. });
    let bounds = vector_scene_ops::bounds(&source, &[0]).unwrap().unwrap();
    assert_eq!(bounds.right, 120.);
    assert_eq!(bounds.top, -20.);
}

#[test]
fn aligning_individual_members_does_not_move_unselected_siblings() {
    let (grouped, _) = vector_scene_ops::group(&scene(), &[0, 1], "Pair").unwrap();
    let aligned = vector_scene_ops::align(&grouped, &[0, 2], AlignAxis::Left).unwrap();
    assert_eq!(aligned.objects[1], grouped.objects[1]);
    assert_eq!(
        vector_scene_ops::bounds(&aligned, &[2])
            .unwrap()
            .unwrap()
            .left,
        0.
    );
}

#[test]
fn legacy_scene_version_rejects_group_paths() {
    let mut legacy = scene();
    legacy.version = VECTOR_SCENE_LEGACY_VERSION;
    legacy.objects[0].groups = vec![VectorGroup {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Old".into(),
    }];
    assert!(legacy.validate().is_err());
}
