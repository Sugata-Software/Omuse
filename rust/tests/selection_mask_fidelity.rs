use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::{Editor, Selection},
    effects,
    filters::Filter,
    model::{Document, Layer},
    raster,
};
use serde_json::json;

fn mask_values(editor: &Editor, id: &str) -> Vec<u8> {
    editor
        .document
        .find_layer(id)
        .unwrap()
        .mask
        .as_ref()
        .unwrap()
        .pixels()
        .map(|pixel| {
            assert_eq!(pixel.0, [pixel[0], pixel[0], pixel[0], 255]);
            pixel[0]
        })
        .collect()
}

fn verify_reopen(editor: &Editor) {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("Soft masks.omuse");
    document::save(&editor.document, &path).unwrap();
    let restored = document::open(&path).unwrap();
    assert_eq!(
        raster::composite(&restored),
        raster::composite(&editor.document)
    );
    for layer in &editor.document.layers {
        let reopened = restored.find_layer(&layer.id).unwrap();
        assert_eq!(reopened.mask, layer.mask);
        assert_eq!(
            reopened.metadata["maskOutsideCoverage"],
            layer.metadata["maskOutsideCoverage"]
        );
    }
}

#[test]
fn antialiased_selection_becomes_exact_reveal_or_complementary_hide_mask() {
    for reveal in [true, false] {
        let mut doc = Document::new(6, 1);
        doc.layers[0].image = Some(RgbaImage::from_pixel(6, 1, Rgba([50, 130, 210, 255])).into());
        let id = doc.layers[0].id.clone();
        let mut editor = Editor::new(doc);
        let coverage = vec![0, 1, 64, 128, 254, 255];
        editor.selection = Some(Selection {
            width: 6,
            height: 1,
            mask: coverage.clone(),
        });
        let original = raster::composite(&editor.document);
        assert!(editor.add_mask(&id, reveal));
        let expected = coverage
            .iter()
            .map(|v| if reveal { *v } else { 255 - *v })
            .collect::<Vec<_>>();
        assert_eq!(mask_values(&editor, &id), expected);
        let rendered = raster::composite(&editor.document);
        assert_eq!(
            rendered.pixels().map(|p| p[3]).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(editor.undo_depth(), 1);
        assert!(!editor.add_mask(&id, reveal));
        assert_eq!(editor.undo_depth(), 1);
        verify_reopen(&editor);
        assert!(editor.undo());
        assert!(editor.document.find_layer(&id).unwrap().mask.is_none());
        assert_eq!(raster::composite(&editor.document), original);
        assert_eq!(editor.selection.as_ref().unwrap().mask, coverage);
        assert!(editor.redo());
        assert_eq!(raster::composite(&editor.document), rendered);
    }
}

#[test]
fn feathering_keeps_all_coverage_levels_when_made_into_a_mask() {
    let mut editor = Editor::new(Document::new(17, 9));
    let id = editor.active_layer.clone();
    editor.selection = Some(Selection {
        width: 17,
        height: 9,
        mask: (0..9)
            .flat_map(|y| {
                (0..17).map(move |x| {
                    if (5..12).contains(&x) && (2..7).contains(&y) {
                        255
                    } else {
                        0
                    }
                })
            })
            .collect(),
    });
    assert!(editor.feather_selection(1.5));
    let coverage = editor.selection.as_ref().unwrap().mask.clone();
    assert!(coverage.iter().any(|v| (1..255).contains(v)));
    assert!(editor.add_mask(&id, true));
    assert_eq!(mask_values(&editor, &id), coverage);
    assert_eq!(editor.undo_depth(), 1);
    verify_reopen(&editor);
}

#[test]
fn fractional_translation_interpolates_selection_in_canvas_space() {
    for reveal in [true, false] {
        let mut doc = Document::new(5, 3);
        let layer = &mut doc.layers[0];
        layer.image = Some(RgbaImage::from_pixel(2, 1, Rgba([120, 30, 80, 255])).into());
        layer.offset_x = 1.25;
        layer.offset_y = 1.;
        let id = layer.id.clone();
        let mut editor = Editor::new(doc);
        editor.selection = Some(Selection {
            width: 5,
            height: 3,
            mask: [0, 64, 192, 255, 0].repeat(3),
        });
        assert!(editor.add_mask(&id, reveal));
        // The layer centres land at 1.75 and 2.75; quarter-pixel blends are
        // 64 * .75 + 192 * .25 = 96, 192 * .75 + 255 * .25 = 207.75.
        assert_eq!(
            mask_values(&editor, &id),
            if reveal { vec![96, 208] } else { vec![159, 47] }
        );
        assert_eq!(editor.document.find_layer(&id).unwrap().offset_x, 1.25);
        verify_reopen(&editor);
    }
}

#[test]
fn rotated_scaled_and_flipped_layers_use_the_visible_canvas_selection() {
    for flipped in [false, true] {
        for reveal in [false, true] {
            let mut doc = Document::new(10, 10);
            let layer = &mut doc.layers[0];
            layer.image = Some(RgbaImage::from_pixel(2, 3, Rgba([190, 110, 50, 255])).into());
            layer.offset_x = 2.5;
            layer.offset_y = 2.;
            layer.rotation = 90.;
            layer.scale_x = if flipped { -2. } else { 2. };
            layer.metadata["transform"] = json!({"sampling":"Nearest"});
            let id = layer.id.clone();
            let mut editor = Editor::new(doc);
            editor.selection = Some(Selection {
                width: 10,
                height: 10,
                mask: (0..10u16)
                    .flat_map(|y| (0..10).map(move |x| (20 * x + 10 * y).min(255) as u8))
                    .collect(),
            });
            assert!(editor.add_mask(&id, reveal));
            // Explicit 90-degree geometry maps the six source centres to
            // canvas columns 5,4,3 and rows 2,4 (reversed by the source flip).
            let expected = if flipped {
                vec![140, 120, 120, 100, 100, 80]
            } else {
                vec![120, 140, 100, 120, 80, 100]
            };
            let expected = expected
                .into_iter()
                .map(|v| if reveal { v } else { 255 - v })
                .collect::<Vec<_>>();
            assert_eq!(mask_values(&editor, &id), expected);
            verify_reopen(&editor);
            assert!(editor.undo());
            assert!(editor.redo());
            assert_eq!(mask_values(&editor, &id), expected);
        }
    }
}

#[test]
fn image_less_adjustment_and_group_preserve_partial_selection_coverage() {
    for group in [false, true] {
        let mut doc = Document::new(3, 1);
        doc.layers[0].image = Some(RgbaImage::from_pixel(3, 1, Rgba([80, 80, 80, 255])).into());
        let mut editor = Editor::new(doc);
        let id = if group {
            let mut folder = Layer::group("Masked folder");
            folder.children = editor.document.layers.drain(..).collect();
            let id = folder.id.clone();
            editor.document.layers.push(folder);
            id
        } else {
            editor
                .add_adjustment(effects::adjustment_for_filter(&Filter::Invert).unwrap())
                .unwrap()
        };
        editor.selection = Some(Selection {
            width: 3,
            height: 1,
            mask: vec![0, 128, 255],
        });
        assert!(editor.add_mask(&id, true));
        assert_eq!(mask_values(&editor, &id), vec![0, 128, 255]);
        let rendered = raster::composite(&editor.document);
        if group {
            assert_eq!(
                rendered.pixels().map(|p| p[3]).collect::<Vec<_>>(),
                vec![0, 128, 255]
            );
        } else {
            assert_eq!(
                rendered.pixels().map(|p| p[0]).collect::<Vec<_>>(),
                vec![80, 128, 175]
            );
        }
        verify_reopen(&editor);
    }
}

#[test]
fn absent_empty_and_off_canvas_selections_keep_reveal_hide_meaning_and_ground() {
    for reveal in [false, true] {
        for selected in [false, true] {
            let mut editor = Editor::new(Document::new(2, 2));
            let id = editor.active_layer.clone();
            editor.document.layers[0].offset_x = -4.;
            if selected {
                editor.selection = Some(Selection {
                    width: 2,
                    height: 2,
                    mask: vec![255; 4],
                });
            }
            assert!(editor.add_mask(&id, reveal));
            let expected = if reveal != selected { 255 } else { 0 };
            assert_eq!(mask_values(&editor, &id), vec![expected; 4]);
            assert_eq!(
                editor.document.layers[0].metadata["maskOutsideCoverage"],
                expected
            );
            verify_reopen(&editor);
        }
        let mut editor = Editor::new(Document::new(2, 2));
        let id = editor.active_layer.clone();
        editor.selection = Some(Selection {
            width: 2,
            height: 2,
            mask: vec![0; 4],
        });
        assert!(editor.add_mask(&id, reveal));
        assert_eq!(
            mask_values(&editor, &id),
            vec![if reveal { 0 } else { 255 }; 4]
        );
    }
}
