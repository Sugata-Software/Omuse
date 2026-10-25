use super::*;

fn line(name: &str, points: &[(f32, f32)]) -> omuse::vector_scene::VectorObject {
    omuse::vector_scene::VectorObject::new(
        name,
        VectorPath {
            subpaths: vec![Subpath {
                closed: false,
                anchors: points
                    .iter()
                    .map(|(x, y)| Anchor {
                        position: VectorPoint { x: *x, y: *y },
                        incoming: None,
                        outgoing: None,
                    })
                    .collect(),
            }],
            fill_rule: Default::default(),
        },
        None,
        Some(StrokeStyle {
            width: 1.,
            color: [255, 255, 255, 255],
        }),
    )
}

#[gpui_kit::test]
fn pen_contact_toggle_hover_and_endpoint_use_the_same_perpendicular(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zoom = 8.;
            view.preferences.snapping = true;
            view.show_grid = false;
            view.show_guides = false;
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(
                omuse::vector_scene::VectorScene {
                    version: omuse::vector_scene::VECTOR_SCENE_VERSION,
                    width: 40,
                    height: 30,
                    objects: vec![
                        line("Pen", &[(18., 3.)]),
                        line("Target", &[(2., 20.), (36., 20.)]),
                    ],
                },
                [0].into_iter().collect(),
                window,
                cx,
            )
            .unwrap();
            view.command("vector-path", window, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(
            !view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .contacts
                .options
                .perpendicular
        )
    });
    click_inspector_id(cx, "vector-contact-perpendicular");
    let pointer = canvas_point(&view, cx, 18.2, 19.5);
    cx.simulate_mouse_move(pointer, None, Modifiers::default());
    draw(cx);
    view.update(cx, |view, _| {
        let hit = view
            .vector_draft
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .contacts
            .indicator
            .unwrap();
        assert_eq!(hit.kind, omuse::vector_snap::ContactKind::Perpendicular);
        assert_eq!(hit.point, VectorPoint { x: 18., y: 20. });
    });
    cx.simulate_click(pointer, Modifiers::default());
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft.as_ref().unwrap().path.subpaths[0]
                .anchors
                .last()
                .unwrap()
                .position,
            VectorPoint { x: 18., y: 20. }
        )
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft.as_ref().unwrap().path.subpaths[0]
                .anchors
                .len(),
            1
        )
    });
}

#[gpui_kit::test]
fn pen_contact_bypass_and_isolation_do_not_change_the_raw_endpoint(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preferences.snapping = true;
            view.show_grid = false;
            view.show_guides = false;
            view.open_vector_scene(window, cx);
            let mut target = line("Outside", &[(2., 20.), (36., 20.)]);
            target.groups = vec![omuse::vector_scene::VectorGroup {
                id: "1d48a3be-d123-4e6a-8f51-743c7023df6b".into(),
                name: "Outside".into(),
            }];
            let mut active = line("Inside", &[(18., 3.)]);
            active.groups = vec![omuse::vector_scene::VectorGroup {
                id: "8a8dc795-ac1f-4259-a4e2-5d5e89ab3a0b".into(),
                name: "Inside".into(),
            }];
            view.replace_scene_artwork(
                omuse::vector_scene::VectorScene {
                    version: omuse::vector_scene::VECTOR_SCENE_VERSION,
                    width: 40,
                    height: 30,
                    objects: vec![active, target],
                },
                [0].into_iter().collect(),
                window,
                cx,
            )
            .unwrap();
            view.command("vector-path", window, cx);
            let scene = view.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            scene.contacts.options.perpendicular = true;
            scene.isolation = vec!["8a8dc795-ac1f-4259-a4e2-5d5e89ab3a0b".into()];
            let raw = VectorPoint { x: 18.2, y: 19.5 };
            assert_eq!(view.snap_pen_construction_point(raw, false), raw);
            view.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .isolation
                .clear();
            assert_eq!(view.snap_pen_construction_point(raw, true), raw);
            assert_ne!(view.snap_pen_construction_point(raw, false), raw);
        })
    });
}
