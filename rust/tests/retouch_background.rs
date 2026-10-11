use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::{Editor, PaintTool, Selection},
    model::{Document, Layer},
    retouch_brush::RetouchMode,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

const PATH: [(f32, f32); 2] = [(11.5, 12.5), (21.5, 12.5)];

fn fixture() -> Editor {
    let mut document = Document::new(40, 32);
    document.layers[0].image = Some(
        RgbaImage::from_fn(40, 32, |x, y| {
            Rgba([(x * 37) as u8, (y * 29) as u8, (x * 13 + y * 17) as u8, 173])
        })
        .into(),
    );
    document.layers[0].mask = Some(
        RgbaImage::from_fn(40, 32, |x, y| {
            // Exercise conversion of coloured/alpha masks as well as restoration
            // of untouched original bytes after the coverage operation.
            Rgba([(x * 23) as u8, (y * 19) as u8, 113, 127])
        })
        .into(),
    );
    document.layers[0].metadata["maskOutsideCoverage"] = json!(255);
    let mut editor = Editor::new(document);
    editor.brush.size = 10.;
    editor.brush.blur_radius = 1.75;
    editor.brush.hardness = 0.5;
    editor.brush.opacity = 0.7;
    editor.selection = Some(Selection {
        width: 40,
        height: 32,
        mask: (0..40 * 32).map(|i| [0, 128, 255][i % 3]).collect(),
    });
    editor
}

fn snapshot(editor: &Editor) -> (String, Option<Selection>, u64, usize, usize) {
    (
        format!("{:?}", editor.document),
        editor.selection.clone(),
        editor.revision(),
        editor.undo_depth(),
        editor.redo_depth(),
    )
}

#[test]
fn real_worker_matches_native_raster_and_mask_with_one_saved_undoable_edit() {
    for mask in [false, true] {
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let mut editor = fixture();
            let layer = &mut editor.document.layers[0];
            // Native project files require opaque grayscale masks. Imported
            // colour/alpha mask compatibility is exercised separately below.
            layer.mask = Some(
                RgbaImage::from_fn(40, 32, |x, y| {
                    let v = (x * 23 + y * 19) as u8;
                    Rgba([v, v, v, 255])
                })
                .into(),
            );
            layer.offset_x = 9.;
            layer.offset_y = 7.;
            layer.scale_x = -0.5;
            layer.scale_y = 0.75;
            layer.rotation = 37.;
            let path = if mask {
                let placement = editor.mask_placement(&editor.active_layer).unwrap();
                [placement.point(0.3, 0.5), placement.point(0.65, 0.5)]
            } else {
                [
                    editor
                        .layer_to_canvas(&editor.active_layer, 12., 16.)
                        .unwrap(),
                    editor
                        .layer_to_canvas(&editor.active_layer, 26., 16.)
                        .unwrap(),
                ]
            };
            let before = snapshot(&editor);
            let original_image = editor.document.layers[0].image.clone().unwrap();
            let original_mask = editor.document.layers[0].mask.clone().unwrap();
            let mut sync = Editor::new(editor.document.clone());
            sync.selection = editor.selection.clone();
            sync.brush = editor.brush.clone();
            let id = sync.active_layer.clone();
            let changed = if mask {
                sync.retouch_mask_stroke(&id, &path, mode).unwrap()
            } else {
                sync.retouch_stroke(&path, mode).unwrap()
            };
            assert!(changed, "{mode:?}, mask {mask}");
            let request = editor.prepare_retouch(path.to_vec(), mode, mask).unwrap();
            assert_eq!(snapshot(&editor), before);
            assert!(
                editor.document.layers[0]
                    .image
                    .as_ref()
                    .unwrap()
                    .shares_pixels_with(&original_image)
            );
            assert!(
                editor.document.layers[0]
                    .mask
                    .as_ref()
                    .unwrap()
                    .shares_pixels_with(&original_mask)
            );
            // This checks that the captured request and output really cross a
            // worker boundary, not merely a synchronous test convenience API.
            let prepared = std::thread::spawn(move || request.compute(&AtomicBool::new(false)))
                .join()
                .unwrap()
                .unwrap();
            assert_eq!(snapshot(&editor), before);
            assert!(
                editor
                    .apply_prepared_retouch(prepared, &AtomicBool::new(false))
                    .unwrap()
            );
            assert_eq!(
                format!("{:?}", editor.document),
                format!("{:?}", sync.document)
            );
            assert_eq!(editor.undo_depth(), 1);
            let result = format!("{:?}", editor.document);
            let scratch = tempfile::tempdir().unwrap();
            let path = scratch.path().join("Background-retouch.omuse");
            document::save(&editor.document, &path).unwrap();
            let reopened = document::open(&path).unwrap();
            assert_eq!(reopened.layers[0].image, editor.document.layers[0].image);
            assert_eq!(reopened.layers[0].mask, editor.document.layers[0].mask);
            assert_eq!(
                reopened.layers[0].metadata["maskOutsideCoverage"],
                editor.document.layers[0].metadata["maskOutsideCoverage"]
            );
            assert!(editor.undo());
            assert_eq!(format!("{:?}", editor.document), before.0);
            assert_eq!(editor.selection, before.1);
            assert!(editor.redo());
            assert_eq!(format!("{:?}", editor.document), result);
        }
    }
}

#[test]
fn imported_colour_alpha_masks_keep_unedited_source_bytes_and_exact_undo() {
    for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
        let mut editor = fixture();
        let before = snapshot(&editor);
        let original = editor.document.layers[0].mask.clone().unwrap();
        let request = editor.prepare_retouch(PATH.to_vec(), mode, true).unwrap();
        let prepared = request.compute(&AtomicBool::new(false)).unwrap();
        assert!(
            editor
                .apply_prepared_retouch(prepared, &AtomicBool::new(false))
                .unwrap()
        );
        let result = editor.document.layers[0].mask.as_ref().unwrap();
        for (i, (old, new)) in original.pixels().zip(result.pixels()).enumerate() {
            if i % 3 == 0 {
                assert_eq!(old, new);
            }
        }
        assert_eq!(editor.undo_depth(), 1);
        assert!(editor.undo());
        assert_eq!(snapshot(&editor).0, before.0);
    }
}

#[test]
fn no_op_and_cancelled_jobs_leave_artwork_selection_and_history_exact() {
    for mask in [false, true] {
        for mode in [RetouchMode::Blur, RetouchMode::Smudge, RetouchMode::Liquify] {
            let mut editor = fixture();
            let before = snapshot(&editor);
            let request = editor.prepare_retouch(PATH.to_vec(), mode, mask).unwrap();
            assert!(request.compute(&AtomicBool::new(true)).is_err());
            assert_eq!(snapshot(&editor), before);
            let request = editor.prepare_retouch(PATH.to_vec(), mode, mask).unwrap();
            let cancellation = AtomicBool::new(false);
            let prepared = request.compute(&cancellation).unwrap();
            cancellation.store(true, Ordering::Relaxed);
            assert!(
                editor
                    .apply_prepared_retouch(prepared, &cancellation)
                    .is_err()
            );
            assert_eq!(snapshot(&editor), before);
            let request = editor.prepare_retouch(Vec::new(), mode, mask).unwrap();
            let prepared = request.compute(&AtomicBool::new(false)).unwrap();
            assert!(
                !editor
                    .apply_prepared_retouch(prepared, &AtomicBool::new(false))
                    .unwrap()
            );
            assert_eq!(snapshot(&editor), before);
        }
    }
}

#[test]
fn stale_results_never_overwrite_new_editor_document_selection_or_active_target() {
    for mask in [false, true] {
        for mutation in 0..10 {
            let mut editor = fixture();
            let request = editor
                .prepare_retouch(PATH.to_vec(), RetouchMode::Liquify, mask)
                .unwrap();
            let prepared = request.compute(&AtomicBool::new(false)).unwrap();
            let original_revision = editor.revision();
            match mutation {
                0 => {
                    let selection = editor.selection.clone();
                    let brush = editor.brush.clone();
                    editor = Editor::new(editor.document.clone());
                    editor.selection = selection;
                    editor.brush = brush;
                }
                1 => {
                    let id = editor.active_layer.clone();
                    assert!(editor.rename_layer(&id, "Changed"));
                }
                2 => {
                    let id = editor.active_layer.clone();
                    assert!(editor.rename_layer(&id, "Changed then undone"));
                    assert!(editor.undo());
                    assert_eq!(editor.revision(), original_revision);
                }
                3 => {
                    editor.selection.as_mut().unwrap().mask[0] = 255;
                }
                4 => {
                    editor.selection = None;
                }
                5 => {
                    editor.active_layer = "another-target".into();
                }
                6 => {
                    editor.document.layers[0]
                        .image
                        .as_mut()
                        .unwrap()
                        .put_pixel(0, 0, Rgba([1; 4]));
                }
                7 => {
                    editor.document.layers[0]
                        .mask
                        .as_mut()
                        .unwrap()
                        .put_pixel(0, 0, Rgba([2; 4]));
                }
                8 => {
                    editor.document.layers[0].offset_x += 1.;
                }
                9 => {
                    editor.document.layers[0].locked = true;
                }
                _ => unreachable!(),
            }
            let changed = snapshot(&editor);
            let error = editor
                .apply_prepared_retouch(prepared, &AtomicBool::new(false))
                .unwrap_err();
            assert!(error.to_string().contains("stale"), "{mutation}: {error}");
            assert_eq!(
                snapshot(&editor),
                changed,
                "mutation {mutation}, mask {mask}"
            );
        }
    }
}

#[test]
fn active_gestures_refuse_prepare_and_commit_without_finishing_them() {
    for floating in [false, true] {
        let mut editor = fixture();
        let prepared = editor
            .prepare_retouch(PATH.to_vec(), RetouchMode::Blur, false)
            .unwrap()
            .compute(&AtomicBool::new(false))
            .unwrap();
        if floating {
            assert!(editor.begin_floating_selection().unwrap().is_some());
        } else {
            assert!(editor.begin_stroke(12., 12., 1., PaintTool::Brush));
        }
        let before = snapshot(&editor);
        assert!(
            editor
                .prepare_retouch(PATH.to_vec(), RetouchMode::Blur, false)
                .is_err()
        );
        assert!(
            editor
                .apply_prepared_retouch(prepared, &AtomicBool::new(false))
                .is_err()
        );
        assert_eq!(snapshot(&editor), before);
        if floating {
            assert!(editor.cancel_floating_selection());
        } else {
            editor.cancel_stroke();
        }
    }
}

#[test]
fn admission_failures_and_kernel_work_refusal_are_atomic() {
    let mut editor = fixture();
    let original = snapshot(&editor);
    assert!(
        editor
            .prepare_retouch(vec![(0., 0.); 100_001], RetouchMode::Blur, false)
            .is_err()
    );
    assert_eq!(snapshot(&editor), original);
    editor.selection.as_mut().unwrap().mask.pop();
    let malformed = snapshot(&editor);
    assert!(
        editor
            .prepare_retouch(PATH.to_vec(), RetouchMode::Blur, false)
            .is_err()
    );
    assert_eq!(snapshot(&editor), malformed);
    editor.selection = original.1;
    editor.brush.size = 4096.;
    let before = snapshot(&editor);
    let excessive: Vec<_> = [(0., 0.), (1_000_000., 1_000_000.)]
        .into_iter()
        .cycle()
        .take(100)
        .collect();
    let error = editor
        .prepare_retouch(excessive, RetouchMode::Liquify, false)
        .unwrap()
        .compute(&AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("Stroke or Blur radius is too large")
    );
    assert_eq!(snapshot(&editor), before);
}

#[test]
fn background_mask_preserves_legacy_folder_grid_outside_and_children() {
    let mut folder = Layer::group("Masked folder");
    folder.children.push(Layer::paint("Artwork", 32, 24));
    folder.mask = Some(
        RgbaImage::from_fn(9, 9, |x, y| {
            let value = if x == 4 && y == 4 { 0 } else { 255 };
            Rgba([value, value, value, 255])
        })
        .into(),
    );
    folder.offset_x = 8.;
    folder.offset_y = 6.;
    folder.rotation = 90.;
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
    editor.active_layer = id.clone();
    let placement = editor.mask_placement(&id).unwrap();
    let before = snapshot(&editor);
    let child = editor.document.layers[0].children[0].image.clone().unwrap();
    editor.brush.size = 6.;
    editor.brush.hardness = 0.98;
    let request = editor
        .prepare_retouch(vec![placement.point(0.5, 0.5)], RetouchMode::Blur, true)
        .unwrap();
    let prepared = request.compute(&AtomicBool::new(false)).unwrap();
    assert!(
        editor
            .apply_prepared_retouch(prepared, &AtomicBool::new(false))
            .unwrap()
    );
    assert_eq!(editor.mask_placement(&id), Some(placement));
    assert_eq!(
        editor.document.layers[0].metadata["maskPlacement"]["sampling"],
        "Nearest"
    );
    assert_eq!(
        editor.document.layers[0].metadata["maskOutsideCoverage"],
        255
    );
    assert!(
        editor.document.layers[0].children[0]
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(&child)
    );
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(snapshot(&editor).0, before.0);
}
