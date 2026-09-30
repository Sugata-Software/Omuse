use super::*;
use gpui_kit::{TestAppContext, VisualTestContext};

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
    PathBuf,
) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Artwork.omuse");
    document::save(&Document::new(12, 8), &path).unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = EditorView::new(Some(path.clone()), window, cx);
        v.dialog = Dialog::None;
        v.recovery = Recovery::at(temp.path().join("recovery"));
        v.zoom = 1.5;
        v.pan = (17., -9.);
        v.focus.focus(window, cx);
        v
    });
    cx.run_until_parked();
    (view, cx, temp, path)
}
fn change_disk(path: &std::path::Path, color: [u8; 4]) {
    let mut editor = Editor::new(document::open(path).unwrap());
    assert!(editor.fill_selection(color));
    document::save(&editor.document, path).unwrap();
}
fn poll(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.check_external_change(window, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn external_upgrade_clean_reload_is_debounced_and_preserves_view(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    change_disk(&path, [23, 54, 198, 255]);
    let before = cx.update(|_, cx| view.read(cx).pixels.clone());
    poll(&view, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).pixels, before));
    poll(&view, cx);
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.pixels.get_pixel(0, 0).0, [23, 54, 198, 255]);
        assert_eq!(v.live_stamp, project_stamp(&path));
        assert!(!v.has_unsaved_work());
        assert_eq!(v.zoom, 1.5);
        assert_eq!(v.pan, (17., -9.));
        assert_eq!(v.dialog, Dialog::None);
        assert!(!v.busy);
        assert!(v.photo_io.is_none());
    });
}

#[gpui_kit::test]
fn external_upgrade_dirty_work_and_disk_version_survive_keep_editing(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    view.update(cx, |v, cx| {
        assert!(v.editor.fill_selection([230, 55, 90, 255]));
        v.changed(cx);
    });
    change_disk(&path, [23, 54, 198, 255]);
    poll(&view, cx);
    poll(&view, cx);
    view.update_in(cx, |v, window, cx| {
        assert!(v.external.notice.is_some());
        assert_eq!(v.dialog, Dialog::None);
        assert_eq!(v.pixels.get_pixel(0, 0).0, [230, 55, 90, 255]);
        v.keep_external(window, cx);
        v.save(window, cx);
        assert!(!v.create.saving);
        assert!(v.status.contains("changed on disk"));
        assert!(v.has_unsaved_work());
    });
    assert_eq!(
        raster::composite(&document::open(&path).unwrap())
            .get_pixel(0, 0)
            .0,
        [23, 54, 198, 255]
    );
    poll(&view, cx);
    cx.update(|_, cx| assert!(view.read(cx).external.notice.is_none()));
    change_disk(&path, [8, 9, 10, 255]);
    poll(&view, cx);
    poll(&view, cx);
    view.update_in(cx, |v, window, cx| {
        assert!(v.external.notice.is_some());
        v.dialog = Dialog::ExternalChange;
        v.dialog_generation += 1;
        v.reload_external(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.pixels.get_pixel(0, 0).0, [8, 9, 10, 255]);
        assert!(!v.has_unsaved_work());
    });
}

#[gpui_kit::test]
fn external_upgrade_failed_or_stale_reload_keeps_artwork(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    let original = cx.update(|_, cx| view.read(cx).pixels.clone());
    let manifest = std::fs::read(path.join("manifest.json")).unwrap();
    std::fs::write(path.join("manifest.json"), b"{unfinished").unwrap();
    poll(&view, cx);
    poll(&view, cx);
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.pixels, original);
        assert!(!v.busy);
        assert_eq!(v.dialog, Dialog::ExternalChange);
        assert!(v.external.ignored.is_some());
        assert!(v.status.contains("unchanged"));
    });
    let status = cx.update(|_, cx| view.read(cx).status.clone());
    poll(&view, cx);
    poll(&view, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).status, status));
    // Restore the external writer's valid package. Normal Save deliberately
    // refuses to overwrite a corrupt destination, even in a reload fixture.
    std::fs::write(path.join("manifest.json"), manifest).unwrap();
    change_disk(&path, [22, 66, 99, 255]);
    view.update_in(cx, |v, window, cx| {
        v.external.notice = Some(Notice {
            path: path.clone(),
            stamp: project_stamp(&path),
        });
        v.reload_external(window, cx);
        v.editor.fill_selection([45, 67, 89, 255]);
        v.changed(cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.pixels.get_pixel(0, 0).0, [45, 67, 89, 255]);
        assert!(v.has_unsaved_work());
        assert!(!v.busy);
        assert!(v.photo_io.is_none());
    });
}

#[gpui_kit::test]
fn save_upgrade_coalesces_latest_snapshot_before_pending_navigation(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.editor.add_layer("First save");
        v.changed(cx);
        v.save(window, cx);
        assert!(v.create.saving);
        v.editor.add_layer("Latest edit");
        v.changed(cx);
        v.save(window, cx);
        v.save(window, cx);
        assert!(v.save_again);
        v.request(Pending::New, window, cx);
        assert!(matches!(v.pending, Some(Pending::New)));
        assert_eq!(v.dialog, Dialog::None);
    });
    cx.run_until_parked();
    let saved = document::open(&path).unwrap();
    assert!(saved.layers.iter().any(|l| l.name == "Latest edit"));
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(!v.create.saving);
        assert!(!v.save_again);
        assert!(!v.has_unsaved_work());
        assert_eq!(v.dialog, Dialog::New);
        assert!(v.pending.is_none());
    });
}

#[gpui_kit::test]
fn save_upgrade_conflicting_queued_save_does_not_discard_pending_work(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.editor.add_layer("Unsaved latest");
        v.changed(cx);
        v.save(window, cx);
        v.save(window, cx);
        v.request(Pending::New, window, cx);
        std::fs::write(path.join("external-edit"), b"Keep outside writer").unwrap();
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(!v.create.saving);
        assert!(!v.save_again);
        assert!(v.has_unsaved_work());
        assert_eq!(v.dialog, Dialog::Unsaved);
        assert!(matches!(v.pending, Some(Pending::New)));
    });
    assert_eq!(
        std::fs::read(path.join("external-edit")).unwrap(),
        b"Keep outside writer"
    );
}

#[gpui_kit::test]
fn save_upgrade_queued_completion_preserves_a_newer_tool_settings_draft(cx: &mut TestAppContext) {
    let (view, cx, _temp, _path) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.editor.add_layer("First save");
        v.changed(cx);
        v.save(window, cx);
        v.editor.add_layer("Later");
        v.changed(cx);
        v.save(window, cx);
        v.dialog = Dialog::ToolSettings;
        v.dialog_generation += 1;
        v.detail_inputs[0].update(cx, |input, cx| input.set_value("37.5", window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(!v.create.saving);
        assert!(!v.has_unsaved_work());
        assert_eq!(v.dialog, Dialog::ToolSettings);
        assert_eq!(v.detail_inputs[0].read(cx).value().as_ref(), "37.5");
    });
}

#[gpui_kit::test]
fn external_upgrade_acknowledged_reload_drops_previous_crop_geometry(cx: &mut TestAppContext) {
    let (view, cx, _temp, path) = setup(cx);
    view.update_in(cx, |v, window, cx| v.command("crop", window, cx));
    change_disk(&path, [20, 30, 40, 255]);
    poll(&view, cx);
    poll(&view, cx);
    view.update_in(cx, |v, window, cx| {
        assert!(v.crop.is_some());
        assert!(v.external.notice.is_some());
        v.dialog = Dialog::ExternalChange;
        v.dialog_generation += 1;
        v.reload_external(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(v.crop.is_none());
        assert_eq!(v.pixels.get_pixel(0, 0).0, [20, 30, 40, 255]);
        assert_eq!(v.editor.undo_depth(), 0);
    });
}
