use omuse::vector_edit::{Endpoint, join, nearest_endpoint, split_at};
use omuse::vector_path::{Anchor, Point, Subpath, VectorPath};

fn curved(closed: bool) -> VectorPath {
    VectorPath {
        fill_rule: Default::default(),
        subpaths: vec![Subpath {
            closed,
            anchors: (0..4)
                .map(|i| Anchor {
                    position: Point {
                        x: i as f32 * 10.,
                        y: (i % 2) as f32 * 10.,
                    },
                    incoming: Some(Point {
                        x: i as f32 * 10. - 3.,
                        y: -4.,
                    }),
                    outgoing: Some(Point {
                        x: i as f32 * 10. + 3.,
                        y: 14.,
                    }),
                })
                .collect(),
        }],
    }
}

#[test]
fn splitting_and_rejoining_preserves_every_used_cubic_handle() {
    let mut original = curved(false);
    original.subpaths[0].anchors[0].incoming = None;
    original.subpaths[0].anchors[3].outgoing = None;
    let split = split_at(&original, 0, 2).unwrap();
    assert_eq!(split.subpaths.len(), 2);
    assert_eq!(split.subpaths[0].anchors.len(), 3);
    let restored = join(
        &split,
        Endpoint {
            subpath: 0,
            last: true,
        },
        Endpoint {
            subpath: 1,
            last: false,
        },
    )
    .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn closed_contour_cut_and_close_keeps_shape_with_a_rotated_start() {
    let original = curved(true);
    let cut = split_at(&original, 0, 2).unwrap();
    assert!(!cut.subpaths[0].closed);
    assert_eq!(cut.subpaths[0].anchors.len(), 5);
    let restored = join(
        &cut,
        Endpoint {
            subpath: 0,
            last: false,
        },
        Endpoint {
            subpath: 0,
            last: true,
        },
    )
    .unwrap();
    let mut expected = original;
    expected.subpaths[0].anchors.rotate_left(2);
    assert_eq!(restored, expected);
}

#[test]
fn reversed_join_swaps_handles_and_does_not_mutate_source() {
    let original = curved(false);
    let split = split_at(&original, 0, 1).unwrap();
    let result = join(
        &split,
        Endpoint {
            subpath: 1,
            last: false,
        },
        Endpoint {
            subpath: 0,
            last: true,
        },
    )
    .unwrap();
    let mut expected = original.subpaths[0].clone();
    expected.anchors.first_mut().unwrap().incoming = None;
    expected.anchors.last_mut().unwrap().outgoing = None;
    expected.anchors.reverse();
    for a in &mut expected.anchors {
        std::mem::swap(&mut a.incoming, &mut a.outgoing);
    }
    // Unused outer handles survive reversal; compare only rendered segments.
    for pair in result.subpaths[0]
        .anchors
        .windows(2)
        .zip(expected.anchors.windows(2))
    {
        assert_eq!(pair.0[0].position, pair.1[0].position);
        assert_eq!(pair.0[0].outgoing, pair.1[0].outgoing);
        assert_eq!(pair.0[1].incoming, pair.1[1].incoming);
    }
    assert_eq!(split.subpaths.len(), 2);
}

#[test]
fn invalid_splits_and_closed_endpoints_fail_without_changes() {
    let original = curved(false);
    assert!(split_at(&original, 0, 0).is_err());
    assert!(split_at(&original, 0, 3).is_err());
    assert!(split_at(&original, 99, 0).is_err());
    assert!(
        nearest_endpoint(
            &curved(true),
            Endpoint {
                subpath: 0,
                last: true
            }
        )
        .is_err()
    );
    let split = split_at(&original, 0, 1).unwrap();
    assert_eq!(
        nearest_endpoint(
            &split,
            Endpoint {
                subpath: 0,
                last: true
            }
        )
        .unwrap(),
        Endpoint {
            subpath: 1,
            last: false
        }
    );
}
