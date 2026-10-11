//! Native test-window coverage of asynchronous PSD publication. These tests
//! exercise Omuse's separate PSD reader; they are not Photoshop interoperability QA.
use super::*;
use gpui_kit::{TestAppContext, VisualTestContext};

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("exports")).unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = Recovery::at(temp.path().join("recovery"));
        view.path = Some(temp.path().join("Original.omuse"));
        let mut document = Document::new(12, 9);
        document.name = "Editable original".into();
        document.metadata["sourceNote"] = serde_json::json!("Keep this in the master");
        document.layers[0].name = "Background".into();
        let background_id = document.layers[0].id.clone();
        let mut detail = omuse::model::Layer::paint("Detail — café", 4, 3);
        detail.offset_x = 3.;
        detail.offset_y = 2.;
        detail.image =
            Some(image::RgbaImage::from_pixel(4, 3, image::Rgba([197, 97, 45, 210])).into());
        detail.advanced = Some(Arc::new(
            omuse::advanced::LayerState::from_image(
                detail.image.as_ref().unwrap(),
                "Embedded original",
            )
            .unwrap(),
        ));
        document.layers.push(detail);
        view.editor = Editor::new(document);
        // New editors select the topmost image, which here retains an Advanced
        // source and correctly refuses destructive fills. Build real history on
        // the ordinary background while keeping that embedded source intact.
        assert!(view.editor.select_layer(&background_id));
        assert!(view.editor.fill_selection([21, 54, 87, 255]));
        assert!(view.editor.fill_selection([120, 150, 180, 255]));
        assert!(view.editor.undo());
        assert_eq!(view.editor.undo_depth(), 1);
        assert_eq!(view.editor.redo_depth(), 1);
        assert_eq!(
            view.editor.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [21, 54, 87, 255]
        );
        view.editor.select_rectangle(1., 1., 5., 4.);
        view.dialog = Dialog::Export;
        view.refresh(cx);
        view
    });
    cx.run_until_parked();
    (view, cx, temp)
}

#[derive(Debug, PartialEq)]
struct SourceSnapshot {
    // The fixture is tiny. Debug includes all native fields and retained pixel
    // buffers, unlike a rendered composite which could hide lost source data.
    document: String,
    revision: u64,
    undo: usize,
    redo: usize,
    dirty: bool,
    active_layer: String,
    selection: Option<omuse::editor::Selection>,
    path: Option<PathBuf>,
}

fn snapshot(view: &EditorView) -> SourceSnapshot {
    SourceSnapshot {
        document: format!("{:?}", view.editor.document),
        revision: view.editor.revision(),
        undo: view.editor.undo_depth(),
        redo: view.editor.redo_depth(),
        dirty: view.editor.is_dirty(),
        active_layer: view.editor.active_layer.clone(),
        selection: view.editor.selection.clone(),
        path: view.path.clone(),
    }
}

fn export_entries(directory: &std::path::Path) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

#[gpui_kit::test]
fn psd_export_publishes_reopenable_layers_without_changing_master_or_history(
    cx: &mut TestAppContext,
) {
    let (view, cx, temp) = setup(cx);
    // Exercise the same case-insensitive extension dispatch used by Export.
    let path = temp.path().join("exports/Exchange.PSD");
    let (before, pixels, embedded) = view.update_in(cx, |view, window, cx| {
        let before = snapshot(view);
        let pixels = raster::composite(&view.editor.document);
        let embedded = view.editor.document.layers[1].advanced.clone().unwrap();
        view.export_photo_background(
            view.editor.document.clone(),
            path.clone(),
            Default::default(),
            window,
            cx,
        );
        assert!(view.busy);
        assert!(view.photo_io.is_some());
        assert!(!path.exists(), "Publication must be deferred");
        (before, pixels, embedded)
    });
    cx.run_until_parked();
    let reopened = omuse::psd::open(&path).expect("Export must reopen as a layered PSD");
    assert_eq!(reopened.layers.len(), 2);
    assert_eq!(reopened.layers[0].name, "Background");
    assert_eq!(reopened.layers[1].name, "Detail — café");
    assert_eq!(
        (reopened.layers[1].offset_x, reopened.layers[1].offset_y),
        (3., 2.)
    );
    assert_eq!(raster::composite(&reopened), pixels);
    assert_eq!(export_entries(path.parent().unwrap()), vec![path.clone()]);
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.photo_io.is_none());
        assert_eq!(view.dialog, Dialog::None, "{}", view.status);
        assert!(view.status.starts_with("Exported "), "{}", view.status);
        assert!(view.status.contains("2 pixel layers"), "{}", view.status);
        assert!(view.status.contains("8-bit"), "{}", view.status);
        assert!(
            view.export_notes
                .iter()
                .any(|note| note.contains("full-precision"))
        );
        assert_eq!(snapshot(view), before);
        assert!(Arc::ptr_eq(
            view.editor.document.layers[1].advanced.as_ref().unwrap(),
            &embedded
        ));
        // Export must not discard or consume an existing Redo entry.
        assert!(view.editor.redo());
        assert_ne!(raster::composite(&view.editor.document), pixels);
        assert!(view.editor.undo());
        assert_eq!(snapshot(view), before);
    });
    view.update_in(cx, |view, window, cx| {
        view.command("export-report", window, cx);
        assert_eq!(view.dialog, Dialog::ExportReport);
        assert_eq!(snapshot(view), before);
    });
}

#[gpui_kit::test]
fn psd_export_cancel_before_publication_leaves_no_file_or_staging_artifact(
    cx: &mut TestAppContext,
) {
    let (view, cx, temp) = setup(cx);
    let path = temp.path().join("exports/Cancelled.psd");
    let before = view.update_in(cx, |view, window, cx| {
        let before = snapshot(view);
        view.export_photo_background(
            view.editor.document.clone(),
            path.clone(),
            Default::default(),
            window,
            cx,
        );
        assert!(view.busy);
        assert!(view.cancel_photo_io());
        assert!(
            view.photo_io.is_some(),
            "The worker retains its admission slot"
        );
        assert!(!path.exists());
        before
    });
    cx.run_until_parked();
    assert!(export_entries(path.parent().unwrap()).is_empty());
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.photo_io.is_none());
        assert_eq!(view.dialog, Dialog::Export);
        assert_eq!(view.status, "Image operation cancelled.");
        assert_eq!(snapshot(view), before);
    });
}

#[gpui_kit::test]
fn psd_export_changed_revision_before_prepare_returns_cannot_publish(cx: &mut TestAppContext) {
    let (view, cx, temp) = setup(cx);
    let path = temp.path().join("exports/Obsolete.psd");
    let edited = view.update_in(cx, |view, window, cx| {
        let original_revision = view.editor.revision();
        view.export_photo_background(
            view.editor.document.clone(),
            path.clone(),
            Default::default(),
            window,
            cx,
        );
        // No executor turn has elapsed. The worker may prepare its snapshot,
        // but its completion must check this newer editor before publishing.
        view.editor.fill_selection([7, 170, 64, 255]);
        assert_ne!(view.editor.revision(), original_revision);
        assert!(view.busy);
        assert!(!path.exists());
        snapshot(view)
    });
    cx.run_until_parked();
    assert!(export_entries(path.parent().unwrap()).is_empty());
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.photo_io.is_none());
        assert_eq!(view.dialog, Dialog::Export);
        assert_eq!(
            view.status,
            "Document changed; PSD export discarded before publication"
        );
        assert_eq!(snapshot(view), edited);
    });
}

#[gpui_kit::test]
fn psd_export_replaced_editor_with_same_revision_cannot_publish(cx: &mut TestAppContext) {
    let (view, cx, temp) = setup(cx);
    let path = temp.path().join("exports/Replaced.psd");
    let replacement = view.update_in(cx, |view, window, cx| {
        let original_revision = view.editor.revision();
        let original_instance = view.editor.instance_id();
        let original_epoch = view.create.epoch;
        let original_generation = view.dialog_generation;
        view.export_photo_background(
            view.editor.document.clone(),
            path.clone(),
            Default::default(),
            window,
            cx,
        );
        view.editor = Editor::new(Document::new(7, 5));
        view.editor.fill_selection([4, 140, 210, 255]);
        assert_eq!(view.editor.revision(), original_revision);
        assert_ne!(view.editor.instance_id(), original_instance);
        assert_eq!(view.create.epoch, original_epoch);
        assert_eq!(view.dialog_generation, original_generation);
        assert_eq!(view.dialog, Dialog::Export);
        assert!(view.busy);
        snapshot(view)
    });
    cx.run_until_parked();
    assert!(export_entries(path.parent().unwrap()).is_empty());
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.photo_io.is_none());
        assert_eq!(view.dialog, Dialog::Export);
        assert_eq!(
            view.status,
            "Document changed; PSD export discarded before publication"
        );
        assert_eq!(snapshot(view), replacement);
    });
}

#[gpui_kit::test]
fn psd_export_refuses_to_replace_an_existing_destination(cx: &mut TestAppContext) {
    let (view, cx, temp) = setup(cx);
    let path = temp.path().join("exports/Keep.psd");
    let existing = b"Existing destination must remain byte-identical";
    std::fs::write(&path, existing).unwrap();
    let before = view.update_in(cx, |view, window, cx| {
        let before = snapshot(view);
        view.export_photo_background(
            view.editor.document.clone(),
            path.clone(),
            Default::default(),
            window,
            cx,
        );
        assert!(view.busy);
        before
    });
    cx.run_until_parked();
    assert_eq!(std::fs::read(&path).unwrap(), existing);
    assert_eq!(export_entries(path.parent().unwrap()), vec![path]);
    view.update(cx, |view, _| {
        assert!(!view.busy);
        assert!(view.photo_io.is_none());
        assert_eq!(view.dialog, Dialog::Export);
        assert!(
            view.status.starts_with("PSD export failed:"),
            "{}",
            view.status
        );
        assert_eq!(snapshot(view), before);
    });
}
