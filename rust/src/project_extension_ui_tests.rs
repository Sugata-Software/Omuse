use super::*;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn single_canvas_and_collection_offer_the_same_project_extension(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.editor = Editor::new(Document::new(8, 8));
        view
    });
    view.update_in(cx, |view, window, cx| {
        view.save_dialog(false, window, cx);
        assert!(view.path_input.read(cx).value().ends_with("Untitled.omuse"));
        view.ensure_create().unwrap();
        view.save_dialog(false, window, cx);
        assert!(view.path_input.read(cx).value().ends_with("Untitled.omuse"));
        view.save_dialog(true, window, cx);
        assert!(view.path_input.read(cx).value().ends_with("Untitled.png"));
    });
}

#[gpui_kit::test]
fn legacy_save_shortcut_creates_a_copy_and_keeps_original_pixels(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    cx.update(bind_keys);
    let temp = tempfile::tempdir().unwrap();
    let legacy = temp.path().join("Artwork.comp");
    let modern = temp.path().join("Artwork.omuse");
    document::save(&Document::new(8, 8), &legacy).unwrap();
    let original_stamp = project_stamp(&legacy);
    let original_pixels = raster::composite(&document::open(&legacy).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(Some(legacy.clone()), window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.dialog = Dialog::None;
        view.editor.fill_selection([90, 35, 170, 255]);
        view.focus.focus(window, cx);
        view
    });
    cx.simulate_keystrokes("ctrl-s");
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.dialog, Dialog::Save);
        assert_eq!(
            view.path_input.read(cx).value().as_ref(),
            modern.to_str().unwrap()
        );
        assert!(!view.create.saving);
        assert!(!modern.exists());
        view.confirm_dialog(window, cx);
        assert!(view.create.saving);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.path.as_ref(), Some(&modern));
        assert!(!view.editor.is_dirty(), "{}", view.status);
        assert_eq!(view.dialog, Dialog::None);
    });
    assert_eq!(project_stamp(&legacy), original_stamp);
    assert_eq!(
        raster::composite(&document::open(&legacy).unwrap()),
        original_pixels
    );
    assert_eq!(
        raster::composite(&document::open(&modern).unwrap())
            .get_pixel(0, 0)
            .0,
        [90, 35, 170, 255]
    );
}

#[gpui_kit::test]
fn legacy_sibling_collision_requires_explicit_replace_and_rechecks_disk(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let legacy = temp.path().join("Artwork.comp");
    let modern = temp.path().join("Artwork.omuse");
    document::save(&Document::new(8, 8), &legacy).unwrap();
    document::save(&Document::new(8, 8), &modern).unwrap();
    let original_stamp = project_stamp(&legacy);
    let destination_stamp = project_stamp(&modern);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(Some(legacy.clone()), window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.editor.fill_selection([15, 80, 175, 255]);
        view.save_dialog(false, window, cx);
        view
    });
    view.update_in(cx, |view, window, cx| {
        // Even a manually entered old suffix must resolve before collision checks.
        view.path_input.update(cx, |input, cx| {
            input.set_value(legacy.to_string_lossy().to_string(), window, cx)
        });
        view.confirm_dialog(window, cx);
        assert!(!view.create.saving);
        assert!(view.confirming_project_replacement(cx));
        assert_eq!(project_stamp(&modern), destination_stamp);
        std::fs::write(modern.join("external-edit"), b"New disk version").unwrap();
        view.confirm_dialog(window, cx);
        assert!(
            !view.create.saving,
            "changed destination needs a new confirmation"
        );
        assert!(modern.join("external-edit").is_file());
        view.confirm_dialog(window, cx);
        assert!(view.create.saving);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(!view.editor.is_dirty(), "{}", view.status)
    });
    assert_eq!(project_stamp(&legacy), original_stamp);
    assert_eq!(
        raster::composite(&document::open(&modern).unwrap())
            .get_pixel(0, 0)
            .0,
        [15, 80, 175, 255]
    );
}

#[gpui_kit::test]
fn changing_save_destination_does_not_reuse_another_projects_confirmation(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("First.omuse");
    let second = temp.path().join("Second.omuse");
    document::save(&Document::new(8, 8), &first).unwrap();
    document::save(&Document::new(8, 8), &second).unwrap();
    let first_stamp = project_stamp(&first);
    let second_stamp = project_stamp(&second);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.editor = Editor::new(Document::new(8, 8));
        view
    });
    view.update_in(cx, |view, window, cx| {
        view.save_to(first.clone(), window, cx);
        assert!(view.confirming_project_replacement(cx));
        view.path_input.update(cx, |input, cx| {
            input.set_value(second.to_string_lossy().to_string(), window, cx)
        });
        assert!(!view.confirming_project_replacement(cx));
        view.confirm_dialog(window, cx);
        assert!(!view.create.saving);
        assert!(view.confirming_project_replacement(cx));
        assert_eq!(project_stamp(&first), first_stamp);
        assert_eq!(project_stamp(&second), second_stamp);
        view.save_dialog(false, window, cx);
        assert!(
            view.save_confirmation.is_none(),
            "a new dialog clears confirmation"
        );
    });
}

#[gpui_kit::test]
fn suffixless_save_produces_a_reopenable_omuse_package(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("New artwork");
    let target = path.with_extension("omuse");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.editor = Editor::new(Document::new(8, 8));
        view.editor.fill_selection([12, 80, 200, 255]);
        view.save_to(path.clone(), window, cx);
        view
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.path.as_ref(), Some(&target));
        assert!(!view.editor.is_dirty(), "{}", view.status);
    });
    assert!(!path.exists());
    let (doc, collection) = EditorView::open_content(&target).unwrap();
    assert!(collection.is_none());
    assert_eq!(
        raster::composite(&doc).get_pixel(0, 0).0,
        [12, 80, 200, 255]
    );
}
