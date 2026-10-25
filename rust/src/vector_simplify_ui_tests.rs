use super::*;

fn artwork() -> omuse::vector_scene::VectorScene {
    use omuse::vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene};
    VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 40,
        height: 30,
        objects: vec![
            VectorObject::new(
                "Detailed line",
                VectorPath {
                    subpaths: vec![Subpath {
                        closed: false,
                        anchors: (0..20)
                            .map(|i| Anchor {
                                position: VectorPoint {
                                    x: 1. + i as f32 * 2.,
                                    y: if i % 2 == 0 { 11. } else { 17. },
                                },
                                incoming: None,
                                outgoing: None,
                            })
                            .collect(),
                    }],
                    fill_rule: Default::default(),
                },
                None,
                Some(StrokeStyle {
                    color: [255, 255, 255, 255],
                    width: 1.,
                }),
            ),
            VectorObject::rectangle("Other", 2., 22., 5., 5., Some([50, 100, 220, 255]), None)
                .unwrap(),
        ],
    }
}

fn preview(view: &Entity<EditorView>, cx: &mut VisualTestContext, tolerance: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.detail_inputs[18].update(cx, |input, cx| input.set_value(tolerance, window, cx));
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.scene_geometry_action("vector-simplify", window, cx)
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| assert!(!view.busy, "{}", view.status));
}

#[gpui_kit::test]
fn simplify_repeated_previews_restore_detail_from_one_immutable_original(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(artwork(), [0].into_iter().collect(), window, cx)
                .unwrap();
            view.command("tool-move", window, cx);
            view.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .simplify
                .preserve_corners = false;
        })
    });
    cx.run_until_parked();
    draw(cx);
    let original = view.update(cx, |view, _| {
        view.vector_draft
            .as_ref()
            .unwrap()
            .scene_snapshot()
            .unwrap()
    });
    preview(&view, cx, "10");
    let coarse = view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        let session = draft
            .scene
            .as_ref()
            .unwrap()
            .simplify
            .current(draft)
            .unwrap();
        assert_eq!(session.source, original);
        assert!(session.after < session.before);
        session.after
    });
    preview(&view, cx, "0.01");
    view.update(cx, |view, _| {
        let draft = view.vector_draft.as_ref().unwrap();
        let session = draft
            .scene
            .as_ref()
            .unwrap()
            .simplify
            .current(draft)
            .unwrap();
        assert_eq!(session.source, original);
        assert!(session.after > coarse);
        assert_eq!(draft.scene_snapshot().unwrap(), original);
    });
    click_inspector_id(cx, "vector-simplify-corners");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .simplify
                .preserve_corners
        )
    });
    click_inspector_id(cx, "vector-simplify-original");
    view.update(cx, |view, _| {
        assert!(
            !view
                .vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .simplify
                .show_original
        )
    });
    click_inspector_id(cx, "vector-simplify-reset");
    view.update(cx, |view, _| {
        assert_eq!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene_snapshot()
                .unwrap(),
            original
        )
    });
}

#[gpui_kit::test]
fn simplify_original_review_invalidates_after_selection_or_undo(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_vector_scene(window, cx);
            view.replace_scene_artwork(artwork(), [0].into_iter().collect(), window, cx)
                .unwrap();
            view.command("tool-move", window, cx);
            view.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .simplify
                .preserve_corners = false;
        })
    });
    cx.run_until_parked();
    draw(cx);
    preview(&view, cx, "10");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([1].into_iter().collect(), Some(1), window, cx)
        })
    });
    view.update(cx, |view, _| {
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .simplify
                .session
                .is_none()
        )
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scene_selection([0].into_iter().collect(), Some(0), window, cx)
        })
    });
    preview(&view, cx, "20");
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    view.update(cx, |view, _| {
        assert!(
            view.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .simplify
                .session
                .is_none()
        )
    });
}
