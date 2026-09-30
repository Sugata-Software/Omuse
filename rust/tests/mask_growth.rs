use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::{Editor, PaintTool, Selection},
    effects::{self, MaskSampler},
    gradient_tools::{GradientKind, GradientSettings},
    model::{Document, Layer},
    raster,
};
use serde_json::json;

fn masked(reveal: bool, placed: bool) -> Editor {
    let mut layer = Layer::paint("Artwork", 8, 6);
    layer.image = Some(RgbaImage::from_pixel(8, 6, Rgba([35, 90, 210, 255])).into());
    layer.mask = Some(RgbaImage::from_pixel(8, 6, Rgba([if reveal { 255 } else { 0 }; 4])).into());
    for pixel in layer.mask.as_mut().unwrap().pixels_mut() {
        pixel[3] = 255;
    }
    layer.offset_x = 8.;
    layer.offset_y = 8.;
    layer.metadata["maskLinked"] = json!(!placed);
    if placed {
        layer.metadata["maskPlacement"] = json!({"origin":[8,8],"size":[8,6],"sampling":"Nearest"});
    }
    let mut editor = Editor::new(Document {
        width: 32,
        height: 24,
        name: "Mask growth".into(),
        background: [0; 4],
        layers: vec![layer],
        metadata: json!({}),
    });
    editor.brush.size = 2.;
    editor.brush.hardness = 1.;
    editor.brush.color = if reveal { [0, 0, 0, 255] } else { [255; 4] };
    editor
}

fn at(editor: &Editor, x: f64, y: f64) -> u8 {
    let layer = editor.document.find_layer(&editor.active_layer).unwrap();
    let mask = layer.mask.as_ref().unwrap();
    let placement = editor.mask_placement(&layer.id).unwrap();
    let (s, c) = f64::from(placement.rotation).to_radians().sin_cos();
    let dx = x - f64::from(placement.x + placement.width * 0.5);
    let dy = y - f64::from(placement.y + placement.height * 0.5);
    let mut u = (dx * c + dy * s) / f64::from(placement.width) + 0.5;
    let mut v = (-dx * s + dy * c) / f64::from(placement.height) + 0.5;
    if placement.flip_x {
        u = 1. - u;
    }
    if placement.flip_y {
        v = 1. - v;
    }
    (MaskSampler::new(&layer.metadata, mask).coverage(
        x,
        y,
        u * f64::from(mask.width()),
        v * f64::from(mask.height()),
        f64::from(mask.width()),
        f64::from(mask.height()),
    ) * 255.)
        .round() as u8
}

fn document_state(editor: &Editor) -> String {
    format!("{:?}", editor.document)
}

#[test]
fn brush_grows_reveal_and_hide_masks_without_changing_artwork_and_round_trips() {
    for reveal in [false, true] {
        let mut editor = masked(reveal, false);
        let id = editor.active_layer.clone();
        let original = editor.document.layers[0].clone();
        let before = document_state(&editor);
        let rendered = raster::composite(&editor.document);
        assert!(editor.begin_mask_stroke(26.5, 18.5, 1., PaintTool::Brush));
        assert!(editor.continue_stroke(28.5, 18.5, 1.));
        assert!(editor.finish_stroke());
        assert_eq!(editor.undo_depth(), 1);
        let layer = editor.document.find_layer(&id).unwrap();
        assert!(layer.mask.as_ref().unwrap().width() >= 32);
        assert!(layer.mask.as_ref().unwrap().height() >= 24);
        assert!(
            original
                .image
                .as_ref()
                .unwrap()
                .shares_pixels_with(layer.image.as_ref().unwrap())
        );
        assert_eq!(
            (
                layer.offset_x,
                layer.offset_y,
                layer.rotation,
                layer.scale_x,
                layer.scale_y
            ),
            (
                original.offset_x,
                original.offset_y,
                original.rotation,
                original.scale_x,
                original.scale_y
            )
        );
        assert_eq!(layer.metadata["maskLinked"], true);
        assert_eq!(
            layer.metadata["maskOutsideCoverage"],
            if reveal { 255 } else { 0 }
        );
        assert_eq!(at(&editor, 27.5, 18.5), if reveal { 0 } else { 255 });
        assert_eq!(at(&editor, 10.5, 10.5), if reveal { 255 } else { 0 });
        assert_eq!(at(&editor, 22.5, 2.5), if reveal { 255 } else { 0 });
        assert_eq!(raster::composite(&editor.document), rendered);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("grown.omuse");
        document::save(&editor.document, &path).unwrap();
        let reopened = Editor::new(document::open(&path).unwrap());
        assert_eq!(raster::composite(&reopened.document), rendered);
        assert_eq!(at(&reopened, 27.5, 18.5), if reveal { 0 } else { 255 });
        assert_eq!(
            reopened.document.layers[0].metadata["maskOutsideCoverage"],
            if reveal { 255 } else { 0 }
        );
        assert!(editor.undo());
        assert_eq!(document_state(&editor), before);
        assert!(editor.redo());
        assert_eq!(at(&editor, 27.5, 18.5), if reveal { 0 } else { 255 });
    }
}

#[test]
fn growth_mid_stroke_reindexes_opacity_history_instead_of_darkening_existing_dabs() {
    let mut editor = masked(true, true);
    editor.brush.size = 1.;
    editor.brush.opacity = 0.5;
    assert!(editor.begin_mask_stroke(14.5, 10.5, 1., PaintTool::Pencil));
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .dimensions(),
        (8, 6)
    );
    assert_eq!(at(&editor, 14.5, 10.5), 128);
    assert!(editor.continue_stroke(24.5, 10.5, 1.));
    for _ in 0..4 {
        assert!(editor.continue_stroke(14.5, 10.5, 1.));
        assert!(editor.continue_stroke(24.5, 10.5, 1.));
    }
    assert!(editor.finish_stroke());
    assert_eq!(at(&editor, 14.5, 10.5), 128);
    assert_eq!(at(&editor, 24.5, 10.5), 128);
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .dimensions(),
        (8, 6)
    );
    assert_eq!(at(&editor, 14.5, 10.5), 255);
}

#[test]
fn cancelling_and_no_op_growth_leave_document_history_and_selection_unchanged() {
    let mut editor = masked(true, true);
    let before = document_state(&editor);
    assert!(editor.begin_mask_stroke(28.5, 18.5, 1., PaintTool::Brush));
    assert_ne!(document_state(&editor), before);
    editor.cancel_stroke();
    assert_eq!(document_state(&editor), before);
    assert_eq!(editor.undo_depth(), 0);
    assert!(!editor.is_dirty());
    editor.brush.color = [255; 4];
    let selection_revision = editor.selection_revision();
    assert!(editor.begin_mask_stroke(28.5, 18.5, 1., PaintTool::Brush));
    assert!(!editor.finish_stroke());
    assert_eq!(document_state(&editor), before);
    assert_eq!(editor.selection_revision(), selection_revision);
    assert!(!editor.is_dirty());
    editor.select_rectangle(0., 0., 0., 0.);
    let selection = editor.selection.clone();
    assert!(!editor.begin_mask_stroke(28.5, 18.5, 1., PaintTool::Brush));
    assert_eq!(editor.selection, selection);
    assert_eq!(document_state(&editor), before);
}

#[test]
fn fill_grows_only_as_needed_and_respects_soft_selection() {
    let mut editor = masked(true, true);
    let id = editor.active_layer.clone();
    let mut mask = vec![0; 32 * 24];
    mask[18 * 32 + 26] = 128;
    editor.selection = Some(Selection {
        width: 32,
        height: 24,
        mask,
    });
    let selection = editor.selection.clone();
    assert!(editor.fill_mask_selection(&id, 0).unwrap());
    assert!((127..=128).contains(&at(&editor, 26.5, 18.5)));
    assert_eq!(at(&editor, 25.5, 18.5), 255);
    assert_eq!(at(&editor, 10.5, 10.5), 255);
    assert_eq!(editor.selection, selection);
    assert_eq!(editor.document.layers[0].metadata["maskLinked"], false);
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(editor.selection, selection);
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .dimensions(),
        (8, 6)
    );
}

#[test]
fn linear_and_radial_gradients_reach_canvas_beyond_the_original_mask() {
    let mut editor = masked(true, true);
    let id = editor.active_layer.clone();
    assert!(
        editor
            .gradient_mask(&id, (0.5, 0.), (31.5, 0.), 0, 255)
            .unwrap()
    );
    assert_eq!(at(&editor, 0.5, 10.5), 0);
    assert_eq!(at(&editor, 31.5, 10.5), 255);
    assert!((123..=124).contains(&at(&editor, 15.5, 10.5)));
    assert!(editor.selection.is_none());
    assert!(editor.undo());
    let settings = GradientSettings {
        kind: GradientKind::Radial,
        ..Default::default()
    };
    assert!(
        editor
            .gradient_mask_with(&id, (26.5, 18.5), (30.5, 18.5), 0, 255, &settings)
            .unwrap()
    );
    assert_eq!(at(&editor, 26.5, 18.5), 0);
    assert_eq!(at(&editor, 30.5, 18.5), 255);
    assert_eq!(editor.undo_depth(), 1);
}

#[test]
fn rotated_flipped_masks_keep_old_samples_and_linked_or_detached_placement() {
    for linked in [false, true] {
        for rotation in [-31., 90.] {
            let mut editor = masked(true, true);
            let id = editor.active_layer.clone();
            let layer = &mut editor.document.layers[0];
            layer.metadata["maskLinked"] = json!(linked);
            layer.metadata["maskPlacement"] = json!({"origin":[8,8],"size":[8,6],"rotation":rotation,
                "flipX":true,"flipY":true,"sampling":"Nearest"});
            layer
                .mask
                .as_mut()
                .unwrap()
                .put_pixel(2, 2, Rgba([90, 90, 90, 255]));
            let old = editor.mask_placement(&id).unwrap();
            let sample = old.point(2.5 / 8., 2.5 / 6.);
            let before = raster::composite(&editor.document);
            assert_eq!(at(&editor, sample.0 as f64, sample.1 as f64), 90);
            editor.brush.size = 3.;
            assert!(editor.begin_mask_stroke(28.5, 20.5, 1., PaintTool::Brush));
            assert!(editor.finish_stroke());
            assert_eq!(at(&editor, sample.0 as f64, sample.1 as f64), 90);
            assert_eq!(raster::composite(&editor.document), before);
            let grown = editor.mask_placement(&id).unwrap();
            assert!(grown.flip_x && grown.flip_y);
            assert_eq!(grown.rotation, rotation);
            assert!(editor.transform_layer(&id, 10., 8., 0., 1., 1.));
            let moved = editor.mask_placement(&id).unwrap();
            assert!((moved.x - grown.x - if linked { 2. } else { 0. }).abs() < 0.0001);
            assert!((moved.y - grown.y).abs() < 0.0001);
        }
    }
}

#[test]
fn small_solid_placed_masks_materialize_at_document_resolution() {
    let mut editor = masked(true, true);
    editor.document.layers[0].mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255; 4])).into());
    editor.document.layers[0].metadata["maskPlacement"] =
        json!({"origin":[8,8],"size":[8,6],"sampling":"Nearest"});
    editor.brush.size = 1.;
    assert!(editor.begin_mask_stroke(10.5, 10.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    assert_eq!(at(&editor, 10.5, 10.5), 0);
    assert_eq!(at(&editor, 11.5, 10.5), 255);
    assert_eq!(at(&editor, 30.5, 20.5), 255);
}

#[test]
fn mask_growth_keeps_effect_rendering_and_apply_mask_in_the_same_place() {
    let mut editor = masked(true, true);
    let id = editor.active_layer.clone();
    editor.document.layers[0].metadata["effects"] =
        json!({"stroke": {"size":1,"red":1,"green":0,"blue":0,"opacity":1}});
    let before = raster::composite(&editor.document);
    assert!(editor.begin_mask_stroke(28.5, 20.5, 1., PaintTool::Brush));
    assert!(editor.finish_stroke());
    assert_eq!(raster::composite(&editor.document), before);
    editor.document.layers[0]
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("effects");
    assert!(editor.begin_mask_stroke(10.5, 10.5, 1., PaintTool::Brush));
    assert!(editor.finish_stroke());
    let masked = raster::composite(&editor.document);
    assert_eq!(masked.get_pixel(10, 10)[3], 0);
    assert!(editor.remove_mask(&id, true));
    assert_eq!(raster::composite(&editor.document), masked);
    for key in [
        "maskPlacement",
        "maskOutsideCoverage",
        "maskLinked",
        "maskEnabled",
    ] {
        assert!(editor.document.layers[0].metadata.get(key).is_none());
    }
    assert!(editor.undo());
    assert_eq!(raster::composite(&editor.document), masked);
}

#[test]
fn folder_growth_preserves_children_and_matches_high_precision_after_reopening() {
    let mut child = Layer::paint("Child", 32, 24);
    child.image = Some(RgbaImage::from_pixel(32, 24, Rgba([35, 90, 210, 255])).into());
    let child_pixels = child.image.clone().unwrap();
    let mut group = Layer::group("Folder mask");
    group.children.push(child);
    group.mask = Some(RgbaImage::from_pixel(4, 4, Rgba([255; 4])).into());
    group.offset_x = 3.;
    group.offset_y = 3.;
    group.metadata["transform"] = json!({"origin":[3,3],"size":[4,4],"sampling":"Nearest"});
    let id = group.id.clone();
    let mut editor = Editor::new(Document {
        width: 32,
        height: 24,
        name: "Folder".into(),
        background: [0; 4],
        layers: vec![group],
        metadata: json!({}),
    });
    assert!(editor.select_layer(&id));
    editor.brush.size = 2.;
    editor.brush.hardness = 1.;
    assert!(editor.begin_mask_stroke(22.5, 18.5, 1., PaintTool::Brush));
    assert!(editor.finish_stroke());
    let output = raster::composite(&editor.document);
    assert_eq!(output.get_pixel(22, 18)[3], 0);
    assert_eq!(output.get_pixel(1, 1)[3], 255);
    assert_eq!(editor.document.layers[0].offset_x, 3.);
    assert!(
        child_pixels.shares_pixels_with(
            editor.document.layers[0].children[0]
                .image
                .as_ref()
                .unwrap()
        )
    );
    let precise = raster::composite16(&editor.document).unwrap();
    assert_eq!(precise.get_pixel(22, 18).0[3], 0);
    assert_eq!(precise.get_pixel(1, 1).0[3], 65535);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("folder.omuse");
    document::save(&editor.document, &path).unwrap();
    assert_eq!(raster::composite(&document::open(&path).unwrap()), output);
}

#[test]
fn rejected_growth_rolls_back_earlier_dabs_and_reports_why_without_history() {
    let mut editor = masked(true, true);
    editor.document.width = 5_000;
    editor.document.height = 5_000;
    editor.brush.size = 1.;
    let before = document_state(&editor);
    let selection_revision = editor.selection_revision();
    assert!(editor.begin_mask_stroke(10.5, 10.5, 1., PaintTool::Pencil));
    assert_ne!(document_state(&editor), before);
    assert!(editor.continue_stroke(20.5, 10.5, 1.));
    assert_eq!(document_state(&editor), before);
    assert!(
        editor
            .take_mask_paint_error()
            .unwrap()
            .contains("16-million-pixel")
    );
    assert!(!editor.finish_stroke());
    assert_eq!(document_state(&editor), before);
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(editor.redo_depth(), 0);
    assert_eq!(editor.selection_revision(), selection_revision);
    assert!(!editor.is_dirty());
    let id = editor.active_layer.clone();
    assert!(
        editor
            .fill_mask_selection(&id, 0)
            .unwrap_err()
            .to_string()
            .contains("16-million-pixel")
    );
    assert_eq!(document_state(&editor), before);
}

#[test]
fn explicit_outside_coverage_survives_inversion_copy_and_edge_repainting() {
    let mut editor = masked(true, true);
    let id = editor.active_layer.clone();
    editor.document.layers[0].metadata["maskOutsideCoverage"] = json!(255);
    for p in editor.document.layers[0]
        .mask
        .as_mut()
        .unwrap()
        .pixels_mut()
    {
        *p = Rgba([0, 0, 0, 255]);
    }
    assert_eq!(at(&editor, 30.5, 20.5), 255);
    assert!(editor.invert_mask(&id));
    assert_eq!(at(&editor, 30.5, 20.5), 0);
    let target = editor.add_layer("Copy target");
    assert!(editor.copy_layer_mask(&id, &target, true).unwrap());
    assert_eq!(
        editor.document.find_layer(&target).unwrap().metadata["maskOutsideCoverage"],
        0
    );
    assert_eq!(at(&editor, 30.5, 20.5), 0);
}

#[test]
fn malformed_outside_coverage_is_rejected_and_legacy_masks_keep_edge_fallback() {
    let white = RgbaImage::from_pixel(4, 4, Rgba([255; 4]));
    let black = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
    assert_eq!(effects::mask_outside_coverage(&json!({}), &white), 255);
    assert_eq!(effects::mask_outside_coverage(&json!({}), &black), 0);
    for invalid in [
        json!(-1),
        json!(1),
        json!(128),
        json!(256),
        json!(true),
        json!(null),
        json!("255"),
    ] {
        assert!(effects::validate_mask_metadata(&json!({"maskOutsideCoverage":invalid})).is_err());
    }
    for valid in [0, 255] {
        effects::validate_mask_metadata(&json!({"maskOutsideCoverage":valid})).unwrap();
    }
}

#[test]
fn an_implicit_mask_keeps_its_original_outside_ground_after_edges_are_painted() {
    let mut editor = masked(true, false);
    editor.brush.size = 1.;
    assert!(editor.begin_mask_stroke(8.5, 8.5, 1., PaintTool::Pencil));
    // Painting the entire small source can change its edge majority before a
    // later dab needs growth; the original reveal ground must stay white.
    for (x, y) in [(15.5, 8.5), (15.5, 13.5), (8.5, 13.5), (8.5, 8.5)] {
        assert!(editor.continue_stroke(x, y, 1.));
    }
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .dimensions(),
        (8, 6)
    );
    assert!(editor.continue_stroke(28., 18., 1.));
    assert!(editor.finish_stroke());
    assert_eq!(
        editor.document.layers[0].metadata["maskOutsideCoverage"],
        255
    );
    assert_eq!(at(&editor, 2.5, 2.5), 255);
}

#[test]
fn folder_growth_preserves_nonuniform_legacy_clamped_edges() {
    let mut child = Layer::paint("Full canvas", 32, 24);
    child.image = Some(RgbaImage::from_pixel(32, 24, Rgba([20, 40, 90, 255])).into());
    let mut group = Layer::group("Legacy mask");
    group.children.push(child);
    group.mask = Some(
        RgbaImage::from_fn(4, 4, |x, _| {
            let gray = if x == 0 { 0 } else { 255 };
            Rgba([gray, gray, gray, 255])
        })
        .into(),
    );
    group.offset_x = 3.;
    group.offset_y = 3.;
    group.metadata["transform"] = json!({"origin":[3,3],"size":[4,4],"sampling":"Nearest"});
    let id = group.id.clone();
    let mut editor = Editor::new(Document {
        width: 32,
        height: 24,
        name: "Legacy".into(),
        background: [0; 4],
        layers: vec![group],
        metadata: json!({}),
    });
    editor.select_layer(&id);
    editor.brush.size = 1.;
    let before = raster::composite(&editor.document);
    assert_eq!(before.get_pixel(0, 20)[3], 0);
    assert_eq!(before.get_pixel(20, 20)[3], 255);
    assert!(editor.begin_mask_stroke(24.5, 20.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    let after = raster::composite(&editor.document);
    for (x, y, pixel) in before.enumerate_pixels() {
        if (x, y) != (24, 20) {
            assert_eq!(*after.get_pixel(x, y), *pixel, "at {x},{y}");
        }
    }
    assert_eq!(after.get_pixel(24, 20)[3], 0);
    assert_eq!(
        editor.document.layers[0].metadata["maskPlacement"]["sampling"],
        "Nearest"
    );
    let target = editor.add_layer("Transfer target");
    assert!(editor.copy_layer_mask(&id, &target, true).unwrap());
    assert_eq!(editor.mask_placement(&id), editor.mask_placement(&target));
    assert_eq!(
        editor.document.find_layer(&target).unwrap().metadata["maskOutsideCoverage"],
        255
    );
}
