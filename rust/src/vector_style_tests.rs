use super::*;

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
