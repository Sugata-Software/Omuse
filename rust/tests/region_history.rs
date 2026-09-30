use omuse::{
    editor::{Editor, PaintTool},
    model::{Document, Layer},
};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 520;

fn document() -> Document {
    let mut document = Document::new(WIDTH, HEIGHT);
    document
        .layers
        .insert(0, Layer::paint("Other", WIDTH, HEIGHT));
    document
}

fn editors() -> (Editor, Editor) {
    let document = document();
    let mut candidate = Editor::new(document.clone());
    let mut oracle = Editor::new(document);
    candidate.set_region_history_enabled(true);
    oracle.set_region_history_enabled(false);
    (candidate, oracle)
}

fn assert_layer_eq(left: &Layer, right: &Layer) {
    assert_eq!(left.id, right.id);
    assert_eq!(left.name, right.name);
    assert_eq!(left.visible, right.visible);
    assert_eq!(left.locked, right.locked);
    assert_eq!(left.opacity, right.opacity);
    assert_eq!(left.blend_mode, right.blend_mode);
    assert_eq!(left.offset_x, right.offset_x);
    assert_eq!(left.offset_y, right.offset_y);
    assert_eq!(left.rotation, right.rotation);
    assert_eq!(left.scale_x, right.scale_x);
    assert_eq!(left.scale_y, right.scale_y);
    assert_eq!(left.image, right.image);
    assert_eq!(left.mask, right.mask);
    assert_eq!(left.metadata, right.metadata);
    assert_eq!(left.children.len(), right.children.len());
    for (left, right) in left.children.iter().zip(&right.children) {
        assert_layer_eq(left, right);
    }
}

fn assert_equivalent(left: &Editor, right: &Editor) {
    assert_eq!(left.document.width, right.document.width);
    assert_eq!(left.document.height, right.document.height);
    assert_eq!(left.document.name, right.document.name);
    assert_eq!(left.document.background, right.document.background);
    assert_eq!(left.document.metadata, right.document.metadata);
    assert_eq!(left.document.layers.len(), right.document.layers.len());
    for (left, right) in left.document.layers.iter().zip(&right.document.layers) {
        assert_layer_eq(left, right);
    }
    assert_eq!(left.active_layer, right.active_layer);
    assert_eq!(left.selection, right.selection);
    assert_eq!(left.is_dirty(), right.is_dirty());
    assert_eq!(left.can_undo(), right.can_undo());
    assert_eq!(left.can_redo(), right.can_redo());
    assert_eq!(left.undo_depth(), right.undo_depth());
    assert_eq!(left.redo_depth(), right.redo_depth());
    assert_eq!(
        omuse::raster::composite(&left.document),
        omuse::raster::composite(&right.document)
    );
}

fn stroke(
    editor: &mut Editor,
    tool: PaintTool,
    color: [u8; 4],
    size: f32,
    opacity: f32,
    hardness: f32,
    points: &[(f32, f32, f32)],
) -> bool {
    editor.brush.color = color;
    editor.brush.size = size;
    editor.brush.opacity = opacity;
    editor.brush.hardness = hardness;
    let &(x, y, pressure) = points.first().unwrap();
    assert!(editor.begin_stroke(x, y, pressure, tool));
    for &(x, y, pressure) in &points[1..] {
        assert!(editor.continue_stroke(x, y, pressure));
    }
    editor.finish_stroke()
}

#[test]
fn eligible_tools_match_snapshot_oracle_across_tiles_and_transform() {
    let (mut candidate, mut oracle) = editors();
    let candidate_id = candidate.active_layer.clone();
    let oracle_id = oracle.active_layer.clone();
    assert!(candidate.transform_layer(&candidate_id, 7.25, -3.5, 11.0, 1.08, 0.94));
    assert!(oracle.transform_layer(&oracle_id, 7.25, -3.5, 11.0, 1.08, 0.94));

    let cases = [
        (PaintTool::Brush, [220, 30, 70, 210], 31.0, 0.63, 0.22),
        (PaintTool::Pencil, [10, 180, 240, 255], 7.0, 0.41, 1.0),
        (PaintTool::Eraser, [0; 4], 19.0, 0.57, 0.8),
    ];
    for (tool, color, size, opacity, hardness) in cases {
        let points = [
            (246.5, 255.5, 0.35),
            (267.5, 255.5, 0.8),
            (246.5, 272.5, 1.0),
            (267.5, 242.5, 0.55),
        ];
        assert_eq!(
            stroke(
                &mut candidate,
                tool,
                color,
                size,
                opacity,
                hardness,
                &points
            ),
            stroke(&mut oracle, tool, color, size, opacity, hardness, &points)
        );
        assert_equivalent(&candidate, &oracle);
    }

    let stats = candidate.history_stats();
    assert!(stats.raster_patch_entries >= 3);
    assert!(stats.raster_patch_tiles >= 3);
    assert!(stats.raster_patch_bytes > 0);
    while candidate.undo() {
        assert!(oracle.undo());
        assert_equivalent(&candidate, &oracle);
    }
    while candidate.redo() {
        assert!(oracle.redo());
        assert_equivalent(&candidate, &oracle);
    }
}

#[test]
fn noop_and_cancel_leave_no_patch_history_or_dirty_state() {
    let mut editor = Editor::new(document());
    editor.set_region_history_enabled(true);
    let original = editor
        .document
        .find_layer(&editor.active_layer)
        .unwrap()
        .image
        .clone()
        .unwrap();

    editor.brush.opacity = 0.0;
    assert!(editor.begin_stroke(255.5, 255.5, 1.0, PaintTool::Brush));
    assert!(!editor.finish_stroke());
    editor.brush.opacity = 1.0;
    assert!(editor.begin_stroke(-100.0, -100.0, 1.0, PaintTool::Pencil));
    assert!(!editor.finish_stroke());
    assert!(
        original.shares_pixels_with(
            editor
                .document
                .find_layer(&editor.active_layer)
                .unwrap()
                .image
                .as_ref()
                .unwrap()
        )
    );
    assert_eq!(editor.history_stats().detached_raster_bytes, 0);
    assert!(editor.begin_stroke(255.5, 255.5, 1.0, PaintTool::Brush));
    assert!(editor.continue_stroke(270.5, 270.5, 0.6));
    editor.cancel_stroke();

    let current = editor
        .document
        .find_layer(&editor.active_layer)
        .unwrap()
        .image
        .as_ref()
        .unwrap();
    assert_eq!(original, *current);
    assert!(original.pixels().all(|pixel| pixel.0 == [0; 4]));
    assert!(!editor.is_dirty());
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(editor.redo_depth(), 0);
    let stats = editor.history_stats();
    assert_eq!(stats.raster_patch_entries, 0);
    assert_eq!(stats.raster_patch_tiles, 0);
    assert_eq!(stats.raster_patch_bytes, 0);
}

#[test]
fn mixed_patch_and_snapshot_history_round_trips_exactly() {
    let (mut candidate, mut oracle) = editors();
    let candidate_paint = candidate.active_layer.clone();
    let oracle_paint = oracle.active_layer.clone();
    let points = [(40.5, 40.5, 1.0), (280.5, 260.5, 0.7)];
    assert!(stroke(
        &mut candidate,
        PaintTool::Brush,
        [200, 80, 20, 255],
        13.0,
        0.7,
        0.4,
        &points
    ));
    assert!(stroke(
        &mut oracle,
        PaintTool::Brush,
        [200, 80, 20, 255],
        13.0,
        0.7,
        0.4,
        &points
    ));
    assert!(candidate.rename_layer(&candidate_paint, "Painted"));
    assert!(oracle.rename_layer(&oracle_paint, "Painted"));

    candidate.select_rectangle(30.0, 30.0, 40.0, 40.0);
    oracle.select_rectangle(30.0, 30.0, 40.0, 40.0);
    assert!(stroke(
        &mut candidate,
        PaintTool::Pencil,
        [30, 220, 80, 255],
        9.0,
        0.8,
        1.0,
        &[(45.5, 45.5, 1.0)]
    ));
    assert!(stroke(
        &mut oracle,
        PaintTool::Pencil,
        [30, 220, 80, 255],
        9.0,
        0.8,
        1.0,
        &[(45.5, 45.5, 1.0)]
    ));
    candidate.clear_selection();
    oracle.clear_selection();
    let candidate_other = candidate.document.layers[0].id.clone();
    let oracle_other = oracle.document.layers[0].id.clone();
    assert!(candidate.delete_layer(&candidate_other));
    assert!(oracle.delete_layer(&oracle_other));
    assert_equivalent(&candidate, &oracle);

    while candidate.undo() {
        assert!(oracle.undo());
        assert_equivalent(&candidate, &oracle);
    }
    while candidate.redo() {
        assert!(oracle.redo());
        assert_equivalent(&candidate, &oracle);
    }
}

#[test]
fn saved_revision_and_ui_state_survive_patch_undo_redo() {
    let (mut candidate, mut oracle) = editors();
    let candidate_paint = candidate.active_layer.clone();
    let oracle_paint = oracle.active_layer.clone();
    let points = [(12.5, 12.5, 1.0)];
    assert!(stroke(
        &mut candidate,
        PaintTool::Pencil,
        [255, 20, 30, 255],
        1.0,
        1.0,
        1.0,
        &points
    ));
    assert!(stroke(
        &mut oracle,
        PaintTool::Pencil,
        [255, 20, 30, 255],
        1.0,
        1.0,
        1.0,
        &points
    ));
    candidate.mark_saved();
    oracle.mark_saved();
    assert!(!candidate.is_dirty() && !oracle.is_dirty());

    let candidate_other = candidate.document.layers[0].id.clone();
    let oracle_other = oracle.document.layers[0].id.clone();
    assert!(candidate.select_layer(&candidate_other));
    assert!(oracle.select_layer(&oracle_other));
    candidate.select_rectangle(4.0, 5.0, 9.0, 11.0);
    oracle.select_rectangle(4.0, 5.0, 9.0, 11.0);
    let selection_before_undo = candidate.selection.clone();
    assert!(candidate.undo());
    assert!(oracle.undo());
    assert_equivalent(&candidate, &oracle);
    assert_eq!(candidate.active_layer, candidate_paint);
    assert!(candidate.selection.is_none());
    assert!(candidate.is_dirty());
    assert!(candidate.redo());
    assert!(oracle.redo());
    assert_equivalent(&candidate, &oracle);
    assert_eq!(candidate.active_layer, candidate_other);
    assert_eq!(candidate.selection, selection_before_undo);
    assert!(!candidate.is_dirty());
    assert_ne!(candidate_paint, candidate_other);
    assert_ne!(oracle_paint, oracle_other);
}

#[test]
fn patch_budget_and_immutable_pixel_owners_are_preserved() {
    let mut editor = Editor::new(document());
    editor.set_region_history_enabled(true);
    let frozen = editor.document.clone();
    editor.set_history_limit(700 * 1024);
    editor.brush.size = 1.0;
    editor.brush.color = [90, 140, 230, 255];
    for &(x, y) in &[(10.5, 10.5), (270.5, 10.5), (10.5, 270.5), (270.5, 270.5)] {
        assert!(editor.begin_stroke(x, y, 1.0, PaintTool::Pencil));
        assert!(editor.finish_stroke());
        assert!(editor.history_bytes() <= 700 * 1024);
    }
    let stats = editor.history_stats();
    assert!(stats.raster_patch_entries > 0);
    assert!(
        stats.raster_patch_entries < 4,
        "budget should evict old tile entries"
    );
    assert!(stats.raster_patch_bytes <= editor.history_bytes());
    let frozen_image = frozen
        .find_layer(&editor.active_layer)
        .unwrap()
        .image
        .as_ref()
        .unwrap();
    for &(x, y) in &[(10, 10), (270, 10), (10, 270), (270, 270)] {
        assert_eq!(frozen_image.get_pixel(x, y).0, [0; 4]);
    }

    editor.set_history_limit(0);
    assert_eq!(editor.history_bytes(), 0);
    let stats = editor.history_stats();
    assert_eq!(stats.raster_patch_entries, 0);
    assert_eq!(stats.raster_patch_tiles, 0);
    assert_eq!(stats.raster_patch_bytes, 0);
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
}

#[test]
fn uniquely_owned_small_stroke_does_not_report_a_full_raster_detach() {
    let mut editor = Editor::new(Document::new(WIDTH, HEIGHT));
    editor.set_region_history_enabled(true);
    editor.brush.size = 1.0;
    editor.brush.color = [1, 2, 3, 255];
    assert!(editor.begin_stroke(1.5, 1.5, 1.0, PaintTool::Pencil));
    assert!(editor.finish_stroke());
    let stats = editor.history_stats();
    assert_eq!(stats.raster_patch_entries, 1);
    assert_eq!(stats.raster_patch_tiles, 1);
    assert_eq!(stats.detached_raster_bytes, 0);
}

#[test]
fn eraser_ignores_foreground_alpha_and_keeps_compact_reversible_history() {
    for alpha in [0, 127, 255] {
        let before = image::RgbaImage::from_pixel(WIDTH, HEIGHT, image::Rgba([240, 60, 20, 192]));
        let mut document = Document::new(WIDTH, HEIGHT);
        // Keep the oracle in a separate allocation so it does not force the
        // candidate's copy-on-write image to detach when the stroke begins.
        document.layers[0].image = Some(before.clone().into());
        let mut editor = Editor::new(document);
        editor.set_region_history_enabled(true);
        editor.brush.color = [170, 90, 230, alpha];
        editor.brush.size = 1.;
        editor.brush.hardness = 1.;
        editor.brush.opacity = 0.5;
        assert!(editor.begin_stroke(8.5, 8.5, 1., PaintTool::Eraser));
        assert!(editor.finish_stroke(), "foreground alpha {alpha}");
        let mut after = before.clone();
        after.put_pixel(8, 8, image::Rgba([240, 60, 20, 96]));
        assert_eq!(editor.document.layers[0].image.as_deref(), Some(&after));
        let stats = editor.history_stats();
        assert_eq!(stats.raster_patch_entries, 1);
        assert_eq!(stats.raster_patch_tiles, 1);
        assert!(stats.raster_patch_bytes < before.as_raw().len());
        assert_eq!(stats.detached_raster_bytes, 0);
        assert_eq!(editor.undo_depth(), 1);
        for _ in 0..2 {
            assert!(editor.undo());
            assert_eq!(editor.document.layers[0].image.as_deref(), Some(&before));
            assert!(!editor.is_dirty());
            assert!(editor.redo());
            assert_eq!(editor.document.layers[0].image.as_deref(), Some(&after));
            assert!(editor.is_dirty());
        }
        assert_eq!(editor.history_stats().detached_raster_bytes, 0);
        assert_eq!(editor.history_stats().raster_patch_entries, 1);
    }
}

#[test]
fn image_buffers_with_trailing_bytes_keep_reversible_snapshot_history() {
    let mut document = Document::new(WIDTH, HEIGHT);
    // ImageBuffer permits trailing bytes. Region swap requires an exact layout,
    // so this accepted but unusual buffer must select the snapshot fallback.
    let mut bytes = vec![0; (WIDTH * HEIGHT * 4) as usize];
    bytes.extend_from_slice(&[17, 39, 83, 251]);
    document.layers[0].image = Some(
        image::RgbaImage::from_raw(WIDTH, HEIGHT, bytes)
            .unwrap()
            .into(),
    );
    let original = document.layers[0].image.clone().unwrap();
    let mut editor = Editor::new(document);
    editor.brush.size = 1.;
    editor.brush.color = [90, 10, 60, 255];
    assert!(editor.begin_stroke(4.5, 4.5, 1., PaintTool::Pencil));
    editor.cancel_stroke();
    assert_eq!(editor.document.layers[0].image.as_ref().unwrap(), &original);
    assert!(editor.begin_stroke(4.5, 4.5, 1., PaintTool::Pencil));
    assert!(editor.finish_stroke());
    assert_eq!(editor.history_stats().raster_patch_entries, 0);
    assert!(editor.undo());
    assert_eq!(editor.document.layers[0].image.as_ref().unwrap(), &original);
    assert!(editor.redo());
    assert_eq!(
        editor.document.layers[0]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(4, 4)
            .0,
        [90, 10, 60, 255]
    );
}
