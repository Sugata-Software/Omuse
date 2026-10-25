use super::*;

#[gpui_kit::test]
fn gradient_quality_controls_are_reversible_in_the_shared_inspector(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-rectangle");
    click_inspector_id(cx, "vector-paint-linear");
    click_inspector_id(cx, "vector-paint-perceptual");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let paint = scene.artwork.objects[scene.active]
            .fill_gradient
            .as_ref()
            .unwrap();
        assert_eq!(
            paint.method,
            omuse::vector_scene::GradientMethod::Perceptual
        );
        assert!(!paint.dither);
        assert_eq!(scene.artwork.version, 6);
        assert_eq!(view.dialog, Dialog::None);
    });
    click_inspector_id(cx, "vector-paint-dither");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .dither
        );
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let paint = scene.artwork.objects[scene.active]
            .fill_gradient
            .as_ref()
            .unwrap();
        assert!(!paint.dither);
        assert_eq!(
            paint.method,
            omuse::vector_scene::GradientMethod::Perceptual
        );
    });
    click_inspector_id(cx, "vector-paint-radial");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .method,
            omuse::vector_scene::GradientMethod::Perceptual
        );
    });
}

#[gpui_kit::test]
fn gradient_and_stroke_controls_preview_undo_and_keep_on_shared_canvas(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-rectangle");
    click_inspector_id(cx, "vector-paint-linear");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(scene.artwork.version, 3);
        assert!(scene.artwork.objects[scene.active].fill_gradient.is_some());
        assert!(scene.display.is_some());
        assert_eq!(view.dialog, Dialog::None);
    });
    scene_field(&view, cx, 0, "#C35E2EFF");
    click_inspector_id(cx, "vector-style-preview");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .stops[0]
                .color,
            [195, 94, 46, 255]
        );
    });
    click_inspector_id(cx, "vector-paint-add-stop");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .stops
                .len(),
            3
        );
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .stops
                .len(),
            2
        );
    });
    scene_field(&view, cx, 2, "3");
    click_inspector_id(cx, "vector-style-preview");
    click_inspector_id(cx, "vector-paint-dashed");
    click_inspector_id(cx, "vector-paint-square");
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let options = scene.artwork.objects[scene.active]
            .stroke_options
            .as_ref()
            .unwrap();
        assert_eq!(options.cap, omuse::vector_scene::StrokeCap::Square);
        assert_eq!(options.dashes, vec![9., 6.]);
    });
    click_inspector_id(cx, "vector-paint-radial");
    scroll_to_id(cx, "vector-paint-field-14", "inspector-content");
    click_id(cx, "vector-paint-field-14");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("#22446680");
    draw(cx);
    click_inspector_id(cx, "vector-paint-apply");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap()
                .stops[0]
                .color,
            [34, 68, 102, 128]
        );
    });
    view.update_in(cx, |view, window, cx| view.focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(view.vector_draft.is_none());
        let layer = view
            .editor
            .document
            .layers
            .iter()
            .find(|l| l.vector_scene.is_some())
            .unwrap();
        assert!(
            layer.vector_scene.as_ref().unwrap().objects[0]
                .fill_gradient
                .is_some()
        );
    });
}

fn text_field(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    index: usize,
    id: &'static str,
    value: &str,
) {
    click_inspector_id(cx, id);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(value);
    draw(cx);
    view.update(cx, |view, cx| {
        assert_eq!(view.detail_inputs[index].read(cx).value(), value)
    });
}

#[gpui_kit::test]
fn retained_text_updates_undo_and_explicit_outline_conversion(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-ellipse");
    click_inspector_id(cx, "vector-text-section");
    text_field(&view, cx, 34, "vector-text-field-34", "6");
    click_inspector_id(cx, "vector-text-align-center");
    click_inspector_id(cx, "vector-text-create");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.busy, "{}", view.status);
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(scene.artwork.version, 4, "{}", view.status);
        assert_eq!(scene.artwork.objects.len(), 2);
        assert_eq!(
            scene.artwork.objects[scene.active]
                .text_path
                .as_ref()
                .unwrap()
                .text,
            "Omuse"
        );
    });
    text_field(&view, cx, 32, "vector-text-field-32", "Muse");
    view.update_in(cx, |view, window, cx| view.focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(
            view.vector_draft.is_some(),
            "Unapplied text must remain in the inspector"
        )
    });
    click_inspector_id(cx, "vector-text-update");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .text_path
                .as_ref()
                .unwrap()
                .text,
            "Muse"
        );
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[scene.active]
                .text_path
                .as_ref()
                .unwrap()
                .text,
            "Omuse"
        );
    });
    click_inspector_id(cx, "vector-text-outlines");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert!(scene.artwork.objects[scene.active].text_path.is_none());
    });
}

#[gpui_kit::test]
fn guide_replacement_keeps_typed_text_when_curve_is_active(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-ellipse");
    click_inspector_id(cx, "vector-text-section");
    text_field(&view, cx, 34, "vector-text-field-34", "6");
    click_inspector_id(cx, "vector-text-align-center");
    click_inspector_id(cx, "vector-text-create");
    cx.run_until_parked();
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        assert!(!view.busy, "{}", view.status);
        view.set_scene_selection([0, 1].into_iter().collect(), Some(0), window, cx);
    });
    draw(cx);
    view.update(cx, |view, cx| {
        assert_eq!(view.detail_inputs[32].read(cx).value(), "Omuse");
        assert_eq!(view.detail_inputs[34].read(cx).value(), "6");
    });
    text_field(&view, cx, 32, "vector-text-field-32", "Muse");
    click_inspector_id(cx, "vector-text-replace-guide");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |view, _| {
        assert!(!view.busy, "{}", view.status);
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(scene.active, 1);
        let text = scene.artwork.objects[1].text_path.as_ref().unwrap();
        assert_eq!(text.text, "Muse", "Guide replacement discarded typed text");
        assert_eq!(text.guide, scene.artwork.objects[0].path);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command("undo", window, cx)));
    draw(cx);
    view.update(cx, |view, _| {
        let scene = view.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        assert_eq!(
            scene.artwork.objects[1].text_path.as_ref().unwrap().text,
            "Omuse"
        );
    });
}

#[gpui_kit::test]
fn canvas_gradient_endpoint_and_stop_gestures_are_one_undo_and_save_exactly(
    cx: &mut TestAppContext,
) {
    let (view, cx) = setup(cx);
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-rectangle");
    click_inspector_id(cx, "vector-paint-linear");
    cx.run_until_parked();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.zoom = 8.;
            let scene = v.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_mut()
                .unwrap()
                .transform = [0.8, 0.6, -0.6, 0.8, 15., -5.];
            v.focus.focus(w, cx);
        })
    });
    draw(cx);
    cx.simulate_keystrokes("g");
    view.update(cx, |v, _| assert!(v.vector_gradient_active()));
    let original = view.update(cx, |v, _| {
        v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap()
    });
    let (from, to) = view.update(cx, |v, _| {
        let scene = v.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let handles = super::super::vector_gradient_ui::handles(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap(),
        );
        let from = v.vector_screen_point(handles[1].1).unwrap();
        (from, from + point(px(24.), px(8.)))
    });
    drag_with_modifiers(
        cx,
        from,
        to,
        MouseButton::Left,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    cx.run_until_parked();
    draw(cx);
    let changed = view.update(cx, |v, _| {
        assert!(
            v.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .gradient_drag
                .is_none()
        );
        v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap()
    });
    assert_ne!(changed, original);
    view.update(cx, |v, _| {
        let scene = v.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let h = super::super::vector_gradient_ui::handles(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap(),
        );
        let a = v.vector_screen_point(h[0].1).unwrap();
        let b = v.vector_screen_point(h[1].1).unwrap();
        let dx = f32::from(a.x - b.x).abs();
        let dy = f32::from(a.y - b.y).abs();
        assert!(dx.min(dy).min((dx - dy).abs()) < 0.01);
    });
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(
        view.update(cx, |v, _| v
            .vector_draft
            .as_ref()
            .unwrap()
            .scene_snapshot()
            .unwrap()),
        original
    );
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(
        view.update(cx, |v, _| v
            .vector_draft
            .as_ref()
            .unwrap()
            .scene_snapshot()
            .unwrap()),
        changed
    );
    let (from, to) = view.update(cx, |v, _| {
        let scene = v.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let handles = super::super::vector_gradient_ui::handles(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap(),
        );
        let from = v.vector_screen_point(handles[2].1).unwrap() + point(px(0.), px(15.));
        (from, from + point(px(30.), px(0.)))
    });
    drag(cx, from, to, MouseButton::Left);
    cx.run_until_parked();
    draw(cx);
    let final_scene = view.update(cx, |v, _| {
        v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap()
    });
    assert_ne!(final_scene, changed);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |v, _| {
        assert!(v.vector_draft.is_none());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gradient.omuse");
        omuse::document::save(&v.editor.document, &file).unwrap();
        let loaded = omuse::document::open(&file).unwrap();
        assert_eq!(
            loaded.layers.last().unwrap().vector_scene.as_deref(),
            Some(&final_scene)
        );
    });
}

#[gpui_kit::test]
fn offcentre_gradient_clicks_are_inert_and_drags_preserve_the_grab_offset(cx: &mut TestAppContext) {
    use super::super::vector_gradient_ui::{Handle, handles};
    let (view, cx) = setup(cx);
    cx.simulate_keystrokes("shift-p");
    cx.run_until_parked();
    draw(cx);
    click_inspector_id(cx, "scene-rectangle");
    click_inspector_id(cx, "vector-paint-linear");
    click_inspector_id(cx, "vector-paint-add-stop");
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.zoom = 8.;
            let scene = v.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_mut()
                .unwrap()
                .transform = [0.8, 0.6, -0.6, 0.8, 15., -5.];
            v.focus.focus(window, cx);
        });
    });
    draw(cx);
    cx.simulate_keystrokes("g");
    let original = view.update(cx, |v, _| {
        v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap()
    });
    let (end, chip) = view.update(cx, |v, _| {
        let scene = v.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let points = handles(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap(),
        );
        let end = v
            .vector_screen_point(points.iter().find(|(h, _, _)| *h == Handle::End).unwrap().1)
            .unwrap();
        let chip = v
            .vector_screen_point(
                points
                    .iter()
                    .find(|(h, _, _)| *h == Handle::Stop(1))
                    .unwrap()
                    .1,
            )
            .unwrap()
            + point(px(0.), px(15.));
        (end, chip)
    });
    for position in [end + point(px(4.), px(2.)), chip + point(px(4.), px(3.))] {
        cx.simulate_click(position, Modifiers::default());
        cx.run_until_parked();
        draw(cx);
        view.update(cx, |v, _| {
            assert_eq!(
                v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap(),
                original,
                "Selecting a handle must not move its geometry"
            );
            assert!(
                v.vector_draft
                    .as_ref()
                    .unwrap()
                    .scene
                    .as_ref()
                    .unwrap()
                    .gradient_drag
                    .is_none()
            );
        });
    }
    view.update(cx, |v, _| {
        assert_eq!(
            v.vector_draft
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .gradient_stop,
            1
        )
    });
    let delta = point(px(24.), px(8.));
    let grab = end + point(px(4.), px(2.));
    drag(cx, grab, grab + delta, MouseButton::Left);
    cx.run_until_parked();
    draw(cx);
    let changed = view.update(cx, |v, _| {
        let scene = v.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let points = handles(
            scene.artwork.objects[scene.active]
                .fill_gradient
                .as_ref()
                .unwrap(),
        );
        let actual = v
            .vector_screen_point(points.iter().find(|(h, _, _)| *h == Handle::End).unwrap().1)
            .unwrap();
        assert!(f32::from(actual.x - end.x - delta.x).abs() < 0.01);
        assert!(f32::from(actual.y - end.y - delta.y).abs() < 0.01);
        v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap()
    });
    assert_ne!(changed, original);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    view.update(cx, |v, _| {
        assert_eq!(
            v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap(),
            original
        )
    });
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    view.update(cx, |v, _| {
        assert_eq!(
            v.vector_draft.as_ref().unwrap().scene_snapshot().unwrap(),
            changed
        )
    });
}
