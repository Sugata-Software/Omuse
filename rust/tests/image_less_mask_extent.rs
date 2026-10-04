use image::{GrayImage, Luma, Rgba, RgbaImage};
use omuse::{
    document,
    editor::{Editor, Selection},
    effects,
    filters::Filter,
    model::{Document, Layer},
    raster,
};
use serde_json::json;

fn fixture(group: bool, width: u32, height: u32) -> (Editor, String) {
    let mut document = Document::new(width, height);
    document.layers[0].image =
        Some(RgbaImage::from_pixel(width, height, Rgba([80, 80, 80, 255])).into());
    let mut editor = Editor::new(document);
    let id = if group {
        let mut layer = Layer::group("Retained mask extent");
        layer.children = editor.document.layers.drain(..).collect();
        let id = layer.id.clone();
        editor.document.layers.push(layer);
        id
    } else {
        editor
            .add_adjustment(effects::adjustment_for_filter(&Filter::Invert).unwrap())
            .unwrap()
    };
    editor.document.find_layer_mut(&id).unwrap().metadata["transform"] =
        json!({"size":[2,1],"sampling":"Nearest","retainedExtension":"keep"});
    assert!(raster::validate(&editor.document).is_empty());
    (editor, id)
}

fn expected_image(group: bool, width: u32, height: u32, coverage: &[u8]) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let coverage = coverage[(y * width + x) as usize];
        if group {
            if coverage == 0 {
                Rgba([0, 0, 0, 0])
            } else {
                Rgba([80, 80, 80, coverage])
            }
        } else {
            // Invert takes the grey source from 80 to 175. Independently blend
            // those endpoints by the requested canvas-space mask coverage.
            let value = (80. + 95. * f64::from(coverage) / 255.).round() as u8;
            Rgba([value, value, value, 255])
        }
    })
}

fn assert_edit_and_reopen(
    editor: &mut Editor,
    id: &str,
    before: &Document,
    before_depth: usize,
    expected: &RgbaImage,
) {
    let after = editor.document.find_layer(id).unwrap();
    let previous = before.find_layer(id).unwrap();
    assert_eq!(
        (
            after.offset_x,
            after.offset_y,
            after.rotation,
            after.scale_x,
            after.scale_y
        ),
        (
            previous.offset_x,
            previous.offset_y,
            previous.rotation,
            previous.scale_x,
            previous.scale_y
        )
    );
    assert_eq!(after.children.len(), previous.children.len());
    for (child, old) in after.children.iter().zip(&previous.children) {
        assert_eq!(child.id, old.id);
        assert_eq!(child.image, old.image);
        assert_eq!(child.metadata, old.metadata);
        assert_eq!(
            (
                child.offset_x,
                child.offset_y,
                child.rotation,
                child.scale_x,
                child.scale_y
            ),
            (
                old.offset_x,
                old.offset_y,
                old.rotation,
                old.scale_x,
                old.scale_y
            )
        );
    }
    assert_eq!(after.metadata["transform"]["retainedExtension"], "keep");
    assert_eq!(raster::composite(&editor.document), *expected);
    assert_eq!(editor.undo_depth(), before_depth + 1);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Canvas mask extent.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(raster::composite(&reopened), *expected);
    assert!(editor.undo());
    assert_eq!(editor.document.find_layer(id).unwrap().mask, previous.mask);
    assert_eq!(
        editor.document.find_layer(id).unwrap().metadata,
        previous.metadata
    );
    assert_eq!(
        raster::composite(&editor.document),
        raster::composite(before)
    );
    assert!(editor.redo());
    assert_eq!(raster::composite(&editor.document), *expected);
}

#[test]
fn new_selection_masks_replace_retained_group_and_adjustment_extents() {
    for group in [false, true] {
        for reveal in [false, true] {
            let (mut editor, id) = fixture(group, 4, 1);
            let incoming = vec![0, 64, 192, 255];
            editor.selection = Some(Selection {
                width: 4,
                height: 1,
                mask: incoming.clone(),
            });
            let before = editor.document.clone();
            let depth = editor.undo_depth();
            assert!(editor.add_mask(&id, reveal));
            let coverage: Vec<_> = incoming
                .iter()
                .map(|v| if reveal { *v } else { 255 - *v })
                .collect();
            let expected = expected_image(group, 4, 1, &coverage);
            assert_edit_and_reopen(&mut editor, &id, &before, depth, &expected);
        }
    }
}

#[test]
fn range_replacement_corrects_stale_extent_even_when_mask_bytes_are_identical() {
    for group in [false, true] {
        let (mut editor, id) = fixture(group, 4, 1);
        let incoming = [0, 64, 192, 255];
        let layer = editor.document.find_layer_mut(&id).unwrap();
        layer.mask = Some(
            RgbaImage::from_fn(4, 1, |x, _| {
                let value = incoming[x as usize];
                Rgba([value, value, value, 255])
            })
            .into(),
        );
        layer.metadata["maskEnabled"] = true.into();
        layer.metadata["maskLinked"] = true.into();
        let before = editor.document.clone();
        let depth = editor.undo_depth();
        let source = GrayImage::from_fn(4, 1, |x, _| Luma([incoming[x as usize]]));
        let expected = expected_image(group, 4, 1, &incoming);
        assert_ne!(raster::composite(&before), expected);
        assert!(editor.replace_canvas_mask(&id, &source).unwrap());
        assert!(!editor.replace_canvas_mask(&id, &source).unwrap());
        assert_edit_and_reopen(&mut editor, &id, &before, depth, &expected);
    }
}

#[test]
fn corrected_extent_keeps_rotated_flipped_group_and_adjustment_projection() {
    for group in [false, true] {
        for range in [false, true] {
            let (mut editor, id) = fixture(group, 4, 4);
            let layer = editor.document.find_layer_mut(&id).unwrap();
            layer.rotation = 90.;
            layer.scale_x = -1.;
            layer.metadata["transform"]["size"] = json!([2, 3]);
            let incoming: Vec<_> = (0..4)
                .flat_map(|y| (0..4).map(move |x| x * 40 + y * 10))
                .collect();
            editor.selection = Some(Selection {
                width: 4,
                height: 4,
                mask: incoming.clone(),
            });
            let before = editor.document.clone();
            let depth = editor.undo_depth();
            if range {
                let source = GrayImage::from_raw(4, 4, incoming.clone()).unwrap();
                assert!(editor.replace_canvas_mask(&id, &source).unwrap());
            } else {
                assert!(editor.add_mask(&id, true));
            }
            let expected = expected_image(group, 4, 4, &incoming);
            assert_edit_and_reopen(&mut editor, &id, &before, depth, &expected);
        }
    }
}
