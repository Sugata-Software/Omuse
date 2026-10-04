use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::{Editor, Selection},
    model::Document,
    retouch_brush::{self, RetouchMode, StrokePoint},
};

#[test]
fn retouch_respects_soft_selection_and_remains_one_undoable_reopenable_edit() {
    for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
        let source = RgbaImage::from_fn(32, 20, |x, y| {
            Rgba([
                (x * 37) as u8,
                (y * 29) as u8,
                if x % 2 == 0 { 210 } else { 20 },
                255,
            ])
        });
        let path = [(9.5, 10.5), (20.75, 10.5)];
        let full = retouch_brush::apply(
            &source,
            &path.map(|(x, y)| StrokePoint { x, y }),
            10.,
            0.5,
            0.7,
            mode,
        )
        .unwrap();
        assert_ne!(full, source, "{mode:?} fixture should be changed");
        let mut doc = Document::new(32, 20);
        doc.layers[0].image = Some(source.clone().into());
        let id = doc.layers[0].id.clone();
        let mut editor = Editor::new(doc);
        editor.brush.size = 10.;
        editor.brush.blur_radius = 1.5; // The compatibility kernel above uses its legacy radius.
        editor.brush.hardness = 0.5;
        editor.brush.opacity = 0.7;
        editor.selection = Some(Selection {
            width: 32,
            height: 20,
            mask: (0..640).map(|i| [0, 128, 255][i % 3]).collect(),
        });
        assert!(editor.retouch_stroke(&path, mode).unwrap());
        assert_eq!(editor.undo_depth(), 1);
        let result = editor
            .document
            .find_layer(&id)
            .unwrap()
            .image
            .clone()
            .unwrap();
        for (i, ((old, changed), actual)) in source
            .pixels()
            .zip(full.pixels())
            .zip(result.pixels())
            .enumerate()
        {
            let amount = f32::from([0u8, 128, 255][i % 3]) / 255.;
            for channel in 0..3 {
                let expected = (f32::from(old[channel]) * (1. - amount)
                    + f32::from(changed[channel]) * amount)
                    .round() as u8;
                assert!(
                    actual[channel].abs_diff(expected) <= 1,
                    "{mode:?} pixel {i} channel {channel}: {actual:?}"
                );
            }
            assert_eq!(actual[3], 255);
        }
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("Retouch.omuse");
        document::save(&editor.document, &path).unwrap();
        assert_eq!(
            document::open(&path)
                .unwrap()
                .find_layer(&id)
                .unwrap()
                .image
                .as_ref(),
            Some(&result)
        );
        assert!(editor.undo());
        assert_eq!(
            editor.document.find_layer(&id).unwrap().image.as_deref(),
            Some(&source)
        );
        assert!(editor.redo());
        assert_eq!(
            editor.document.find_layer(&id).unwrap().image.as_ref(),
            Some(&result)
        );
    }
}

#[test]
fn retouch_no_op_and_admission_failure_do_not_change_pixels_or_history() {
    let mut doc = Document::new(20, 20);
    let source = RgbaImage::from_fn(20, 20, |x, y| Rgba([x as u8, y as u8, 137, 17]));
    doc.layers[0].image = Some(source.clone().into());
    let id = doc.layers[0].id.clone();
    let mut editor = Editor::new(doc);
    editor.brush.size = 8.;
    for mode in [RetouchMode::Smudge, RetouchMode::Liquify] {
        assert!(!editor.retouch_stroke(&[(10.5, 10.5)], mode).unwrap());
        assert_eq!(editor.undo_depth(), 0);
        assert_eq!(
            editor.document.find_layer(&id).unwrap().image.as_deref(),
            Some(&source)
        );
    }
    editor.brush.size = 4096.;
    assert!(
        editor
            .retouch_stroke(&[(0., 0.), (1_000_000., 1_000_000.)], RetouchMode::Liquify)
            .is_err()
    );
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(
        editor.document.find_layer(&id).unwrap().image.as_deref(),
        Some(&source)
    );
}

#[test]
fn downscaled_rotated_and_flipped_layers_keep_native_detail_and_exact_undo() {
    for flip in [1., -1.] {
        let source = RgbaImage::from_fn(80, 24, |x, _| {
            Rgba([if x % 2 == 0 { 0 } else { 220 }, 31, 87, 128])
        });
        let mut document = Document::new(64, 64);
        let layer = &mut document.layers[0];
        layer.image = Some(source.clone().into());
        layer.offset_x = 20.;
        layer.offset_y = 20.;
        layer.scale_x = 0.25 * flip;
        layer.scale_y = 0.75;
        layer.rotation = 90.;
        let id = layer.id.clone();
        let mut editor = Editor::new(document);
        editor.brush.size = 4.;
        editor.brush.hardness = 0.98;
        editor.brush.opacity = 0.25;
        // Independently calculate the rotated points about scaled bounds (30,29).
        let path = [30.5, 34.5].map(|x| (29.625, 29. + (x - 40.) * 0.25 * flip));
        let placement = editor.layer_placement(&id).unwrap();
        assert!(editor.retouch_stroke(&path, RetouchMode::Liquify).unwrap());
        let result = editor.document.layers[0].image.clone().unwrap();
        assert_eq!(result.dimensions(), (80, 24));
        assert_eq!(editor.layer_placement(&id), Some(placement));
        for x in 31..38 {
            assert_eq!(
                result.get_pixel(x, 12),
                source.get_pixel(x - 1, 12),
                "flip {flip}, stripe {x}"
            );
        }
        assert_eq!(result.get_pixel(34, 16), source.get_pixel(34, 16));
        assert_eq!(editor.undo_depth(), 1);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Native-detail.omuse");
        document::save(&editor.document, &path).unwrap();
        assert_eq!(
            document::open(&path)
                .unwrap()
                .find_layer(&id)
                .unwrap()
                .image
                .as_ref(),
            Some(&result)
        );
        assert!(editor.undo());
        assert_eq!(editor.document.layers[0].image.as_deref(), Some(&source));
        assert_eq!(editor.layer_placement(&id), Some(placement));
        assert!(editor.redo());
        assert_eq!(editor.document.layers[0].image.as_ref(), Some(&result));
    }
}

#[test]
fn transformed_soft_selection_clips_native_writes_without_clipping_sampled_source() {
    let source = RgbaImage::from_fn(48, 32, |x, y| {
        Rgba([
            (x * 29) as u8,
            (y * 31) as u8,
            if x % 2 == 0 { 230 } else { 10 },
            255,
        ])
    });
    let mut document = Document::new(64, 64);
    let layer = &mut document.layers[0];
    layer.image = Some(source.clone().into());
    layer.offset_x = 13.25;
    layer.offset_y = 16.75;
    layer.scale_x = -0.5;
    layer.scale_y = 0.75;
    layer.rotation = 37.;
    let id = layer.id.clone();
    let coverage_at_column = |x: u32| x.saturating_sub(20).saturating_mul(16).min(255) as u8;
    for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
        let mut full = Editor::new(document.clone());
        full.brush.size = 12.;
        full.brush.hardness = 0.8;
        full.brush.opacity = 0.7;
        full.brush.blur_radius = 1.75;
        let path = [
            full.layer_to_canvas(&id, 16.5, 16.5).unwrap(),
            full.layer_to_canvas(&id, 32.5, 16.5).unwrap(),
        ];
        let mut selected = Editor::new(document.clone());
        selected.brush = full.brush.clone();
        selected.selection = Some(Selection {
            width: 64,
            height: 64,
            mask: (0..4096).map(|i| coverage_at_column(i % 64)).collect(),
        });
        let old_selection = selected.selection.clone();
        assert!(full.retouch_stroke(&path, mode).unwrap());
        assert!(selected.retouch_stroke(&path, mode).unwrap());
        let unmasked = full.document.layers[0].image.as_ref().unwrap();
        let result = selected.document.layers[0].image.as_ref().unwrap();
        let mut partial_changes = 0;
        let mut excluded = 0;
        for (x, y, old) in source.enumerate_pixels() {
            let (wx, _) = selected
                .layer_to_canvas(&id, x as f32 + 0.5, y as f32 + 0.5)
                .unwrap();
            let left = (wx - 0.5).floor();
            let fraction = wx - 0.5 - left;
            let coverage = (f32::from(coverage_at_column(left as u32)) * (1. - fraction)
                + f32::from(coverage_at_column(left as u32 + 1)) * fraction)
                .round() as u8;
            let actual = result.get_pixel(x, y);
            let filtered = unmasked.get_pixel(x, y);
            let amount = f32::from(coverage) / 255.;
            if coverage == 0 {
                assert_eq!(actual, old, "{mode:?} wrote outside selection at {x},{y}");
                excluded += 1;
            }
            if coverage > 0 && coverage < 255 && actual != old {
                partial_changes += 1;
            }
            for c in 0..3 {
                let expected = (f32::from(old[c]) * (1. - amount) + f32::from(filtered[c]) * amount)
                    .round() as u8;
                assert!(
                    actual[c].abs_diff(expected) <= 1,
                    "{mode:?} native pixel {x},{y}: {actual:?}, expected {expected}, coverage {coverage}"
                );
            }
            assert_eq!(actual[3], 255);
        }
        assert!(excluded > 0 && partial_changes > 0);
        assert_eq!(selected.undo_depth(), 1);
        assert!(selected.undo());
        assert_eq!(selected.document.layers[0].image.as_deref(), Some(&source));
        assert_eq!(selected.selection, old_selection);
    }
}

#[test]
fn independent_transformed_mask_grid_and_ground_survive_retouch_save_and_undo() {
    use serde_json::json;
    for outside in [0, 255] {
        let source = RgbaImage::from_fn(80, 24, |x, _| {
            Rgba([
                if x % 2 == 0 { 0 } else { 220 },
                if x % 2 == 0 { 0 } else { 220 },
                if x % 2 == 0 { 0 } else { 220 },
                255,
            ])
        });
        let mut document = Document::new(64, 64);
        let layer = &mut document.layers[0];
        let artwork = layer.image.clone().unwrap();
        layer.mask = Some(source.clone().into());
        layer.metadata["maskLinked"] = json!(false);
        layer.metadata["maskOutsideCoverage"] = json!(outside);
        layer.metadata["maskPlacement"] = json!({"origin":[20,20],"size":[20,18],"rotation":90,"flipX":true,"sampling":"Nearest"});
        let metadata = layer.metadata.clone();
        let id = layer.id.clone();
        let mut editor = Editor::new(document);
        let placement = editor.mask_placement(&id).unwrap();
        editor.brush.size = 4.;
        editor.brush.hardness = 0.98;
        editor.brush.opacity = 0.25;
        let path = [30.5, 34.5].map(|x| (29.625, 29. - (x - 40.) * 0.25));
        assert!(
            editor
                .retouch_mask_stroke(&id, &path, RetouchMode::Liquify)
                .unwrap()
        );
        let layer = editor.document.find_layer(&id).unwrap();
        let result = layer.mask.clone().unwrap();
        for x in 31..38 {
            assert_eq!(result.get_pixel(x, 12), source.get_pixel(x - 1, 12));
        }
        assert!(layer.image.as_ref().unwrap().shares_pixels_with(&artwork));
        assert_eq!(layer.metadata, metadata);
        assert_eq!(editor.mask_placement(&id), Some(placement));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Native-mask.omuse");
        document::save(&editor.document, &path).unwrap();
        let reopened = document::open(&path).unwrap();
        let saved = reopened.find_layer(&id).unwrap();
        assert_eq!(saved.mask.as_ref(), Some(&result));
        assert_eq!(saved.metadata["maskOutsideCoverage"], json!(outside));
        assert_eq!(editor.undo_depth(), 1);
        assert!(editor.undo());
        assert_eq!(
            editor.document.find_layer(&id).unwrap().mask.as_deref(),
            Some(&source)
        );
        assert_eq!(editor.document.find_layer(&id).unwrap().metadata, metadata);
        assert!(editor.redo());
        assert_eq!(
            editor.document.find_layer(&id).unwrap().mask.as_ref(),
            Some(&result)
        );
    }
}

#[test]
fn mask_edge_retouch_uses_stored_ground_and_pins_legacy_ground_without_moving_folders() {
    use omuse::model::Layer;
    use serde_json::json;
    for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
        for outside in [0, 255] {
            let mut document = Document::new(32, 24);
            let layer = &mut document.layers[0];
            layer.mask = Some(RgbaImage::from_pixel(9, 9, Rgba([0, 0, 0, 255])).into());
            layer.metadata["maskPlacement"] =
                json!({"origin":[10,6],"size":[4.5,9],"rotation":90,"sampling":"Nearest"});
            layer.metadata["maskOutsideCoverage"] = json!(outside);
            let id = layer.id.clone();
            let mut editor = Editor::new(document);
            let place = editor.mask_placement(&id).unwrap();
            editor.brush.size = 4.;
            editor.brush.hardness = 0.98;
            let path = if mode == RetouchMode::Blur {
                vec![place.point(0.5 / 9., 4.5 / 9.)]
            } else {
                vec![
                    place.point(-3.5 / 9., 4.5 / 9.),
                    place.point(0.5 / 9., 4.5 / 9.),
                ]
            };
            let changed = editor.retouch_mask_stroke(&id, &path, mode).unwrap();
            assert_eq!(changed, outside == 255);
            let mask = editor
                .document
                .find_layer(&id)
                .unwrap()
                .mask
                .as_ref()
                .unwrap();
            assert_eq!(mask.dimensions(), (9, 9));
            assert!(mask.pixels().all(|p| p[3] == 255));
            if outside == 255 {
                assert!(mask.get_pixel(0, 4)[0] > 0);
                assert!(editor.undo());
            }
            assert_eq!(
                editor.document.find_layer(&id).unwrap().metadata["maskOutsideCoverage"],
                json!(outside)
            );
        }
    }
    let mut folder = Layer::group("Masked folder");
    let child = Layer::paint("Artwork", 32, 24);
    folder.children.push(child);
    folder.mask = Some(
        RgbaImage::from_fn(9, 9, |x, y| {
            let v = if x == 4 && y == 4 { 0 } else { 255 };
            Rgba([v, v, v, 255])
        })
        .into(),
    );
    folder.offset_x = 8.;
    folder.offset_y = 6.;
    folder.rotation = 90.;
    // This legacy image-mask placement is ignored on a folder until an explicit
    // outside ground is stored. Retouch must not accidentally activate it.
    folder.metadata["transform"] = json!({"sampling":"Nearest"});
    folder.metadata["maskPlacement"] =
        json!({"origin":[100,100],"size":[50,50],"sampling":"Smooth"});
    let id = folder.id.clone();
    let mut editor = Editor::new(Document {
        width: 32,
        height: 24,
        name: "Folder".into(),
        background: [0; 4],
        layers: vec![folder],
        metadata: json!({}),
    });
    let placement = editor.mask_placement(&id).unwrap();
    let before = format!("{:?}", editor.document);
    let children = format!("{:?}", editor.document.layers[0].children);
    editor.brush.size = 6.;
    editor.brush.hardness = 0.98;
    assert!(
        editor
            .retouch_mask_stroke(&id, &[placement.point(0.5, 0.5)], RetouchMode::Blur)
            .unwrap()
    );
    assert_eq!(editor.mask_placement(&id), Some(placement));
    assert_eq!(
        editor.document.layers[0].metadata["maskPlacement"]["sampling"],
        "Nearest"
    );
    assert_eq!(
        format!("{:?}", editor.document.layers[0].children),
        children
    );
    assert_eq!(
        editor.document.layers[0].metadata["maskOutsideCoverage"],
        255
    );
    assert!(editor.undo());
    assert_eq!(format!("{:?}", editor.document), before);
}
