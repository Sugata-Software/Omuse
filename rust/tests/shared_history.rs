use image::Rgba;
use omuse::{
    document,
    editor::{Editor, PaintTool},
    model::{Document, Layer},
};

#[test]
fn metadata_history_shares_pixels_and_restores_each_revision() {
    let mut doc = Document::new(512, 512);
    doc.layers.push(Layer::paint("Other", 512, 512));
    let original = doc.clone();
    let mut editor = Editor::new(doc);
    let id = editor.active_layer.clone();
    editor.set_history_limit(64 * 1024);
    for n in 0..20 {
        assert!(editor.rename_layer(&id, &format!("Rename {n}")));
        for (layer, old) in editor.document.layers.iter().zip(&original.layers) {
            assert!(
                layer
                    .image
                    .as_ref()
                    .unwrap()
                    .shares_pixels_with(old.image.as_ref().unwrap())
            );
        }
    }
    assert_eq!(editor.undo_depth(), 20);
    assert!(editor.history_bytes() < 64 * 1024);
    for n in (0..20).rev() {
        assert!(editor.undo());
        let expected = if n == 0 {
            "Other".into()
        } else {
            format!("Rename {}", n - 1)
        };
        assert_eq!(editor.document.find_layer(&id).unwrap().name, expected);
    }
    for _ in 0..20 {
        assert!(editor.redo());
    }
    assert_eq!(editor.document.find_layer(&id).unwrap().name, "Rename 19");
}

#[test]
fn painting_detaches_only_target_and_frozen_save_keeps_original_pixels() {
    let mut doc = Document::new(32, 32);
    doc.layers.push(Layer::paint("Other", 32, 32));
    let mut editor = Editor::new(doc);
    let frozen = editor.document.clone();
    editor.brush.size = 1.;
    editor.brush.color = [200, 30, 80, 255];
    assert!(editor.begin_stroke(4.5, 5.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    assert!(
        editor.document.layers[0]
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(frozen.layers[0].image.as_ref().unwrap())
    );
    assert!(
        !editor.document.layers[1]
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(frozen.layers[1].image.as_ref().unwrap())
    );
    assert_eq!(
        frozen.layers[1].image.as_ref().unwrap().get_pixel(4, 5).0,
        [0; 4]
    );
    let painted = editor.document.layers[1].image.clone().unwrap();
    assert_eq!(painted.get_pixel(4, 5).0, [200, 30, 80, 255]);
    assert!(editor.undo());
    assert_eq!(
        editor.document.layers[1]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(4, 5)
            .0,
        [0; 4]
    );
    assert!(editor.redo());
    assert_eq!(
        editor.document.layers[1]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(4, 5),
        painted.get_pixel(4, 5)
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("frozen.omuse");
    std::thread::spawn(move || {
        document::save(&frozen, &path).unwrap();
        document::open(&path).unwrap()
    })
    .join()
    .map(|saved| {
        assert_eq!(
            saved.layers[1].image.as_ref().unwrap().get_pixel(4, 5).0,
            [0; 4]
        );
    })
    .unwrap();
}

#[test]
fn mask_and_nested_layer_edits_cannot_mutate_shared_document() {
    let mut doc = Document::new(16, 16);
    let mut child = doc.layers.remove(0);
    child.mask = Some(image::RgbaImage::from_pixel(16, 16, Rgba([255; 4])).into());
    let mut group = Layer::group("Group");
    group.children.push(child);
    doc.layers.push(group);
    let frozen = doc.clone();
    let layer = &mut doc.layers[0].children[0];
    layer.image.as_mut().unwrap().put_pixel(1, 1, Rgba([22; 4]));
    layer.mask.as_mut().unwrap().put_pixel(2, 2, Rgba([0; 4]));
    let old = &frozen.layers[0].children[0];
    assert_eq!(old.image.as_ref().unwrap().get_pixel(1, 1).0, [0; 4]);
    assert_eq!(old.mask.as_ref().unwrap().get_pixel(2, 2).0, [255; 4]);
}

#[test]
fn history_counts_shared_rasters_once_and_releases_them_at_zero_budget() {
    let mut editor = Editor::new(Document::new(64, 64));
    let id = editor.active_layer.clone();
    for n in 0..10 {
        assert!(editor.rename_layer(&id, &format!("Name {n}")));
    }
    let metadata_bytes = editor.history_bytes();
    editor.brush.size = 1.;
    assert!(editor.begin_stroke(1.5, 1.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    // Ten earlier revisions share a single old raster, charged only once.
    assert!(editor.history_bytes() >= metadata_bytes + 64 * 64 * 4);
    assert!(editor.history_bytes() < metadata_bytes + 2 * 64 * 64 * 4);
    while editor.undo() {
        assert!(editor.history_bytes() < 64 * 1024);
    }
    while editor.redo() {
        assert!(editor.history_bytes() < 64 * 1024);
    }
    editor.set_history_limit(0);
    assert_eq!(editor.history_bytes(), 0);
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
}

#[test]
fn metadata_only_history_has_a_step_bound_even_with_large_budget() {
    let mut editor = Editor::new(Document::new(1, 1));
    editor.set_history_limit(usize::MAX);
    let id = editor.active_layer.clone();
    for n in 0..150 {
        assert!(editor.rename_layer(&id, &format!("Name {n}")));
    }
    assert_eq!(editor.undo_depth(), 100);
    for _ in 0..100 {
        assert!(editor.undo());
    }
    assert!(!editor.undo());
    assert_eq!(editor.redo_depth(), 100);
}

#[test]
fn history_charges_pixel_capacity_after_current_document_detaches() {
    let mut bytes = Vec::with_capacity(64 * 1024);
    bytes.resize(4, 0);
    let mut doc = Document::new(1, 1);
    doc.layers[0].image = Some(image::RgbaImage::from_raw(1, 1, bytes).unwrap().into());
    let mut editor = Editor::new(doc);
    editor.brush.size = 1.;
    assert!(editor.begin_stroke(0.5, 0.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    assert!(editor.history_bytes() >= 64 * 1024);
    editor.set_history_limit(32 * 1024);
    assert!(!editor.can_undo());
}
