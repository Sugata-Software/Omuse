use image::{Rgba, RgbaImage};
use omuse::{
    editor::{Editor, PaintTool, StrokeDamage},
    model::{Document, Layer},
};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 520;

fn editors() -> (Editor, Editor) {
    let mut document = Document::new(WIDTH, HEIGHT);
    document
        .layers
        .insert(0, Layer::paint("Background", WIDTH, HEIGHT));
    let mut candidate = Editor::new(document.clone());
    let mut oracle = Editor::new(document);
    candidate.set_region_history_enabled(true);
    oracle.set_region_history_enabled(false);
    (candidate, oracle)
}

fn assert_layers(left: &Layer, right: &Layer) {
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
        assert_layers(left, right);
    }
}

fn assert_same(candidate: &Editor, oracle: &Editor) {
    assert_eq!(candidate.document.width, oracle.document.width);
    assert_eq!(candidate.document.height, oracle.document.height);
    assert_eq!(candidate.document.name, oracle.document.name);
    assert_eq!(candidate.document.background, oracle.document.background);
    assert_eq!(candidate.document.metadata, oracle.document.metadata);
    assert_eq!(
        candidate.document.layers.len(),
        oracle.document.layers.len()
    );
    for (left, right) in candidate
        .document
        .layers
        .iter()
        .zip(&oracle.document.layers)
    {
        assert_layers(left, right);
    }
    assert_eq!(candidate.active_layer, oracle.active_layer);
    assert_eq!(candidate.selection, oracle.selection);
    assert_eq!(candidate.selection_revision(), oracle.selection_revision());
    assert_eq!(candidate.is_dirty(), oracle.is_dirty());
    assert_eq!(candidate.can_undo(), oracle.can_undo());
    assert_eq!(candidate.can_redo(), oracle.can_redo());
    assert_eq!(candidate.undo_depth(), oracle.undo_depth());
    assert_eq!(candidate.redo_depth(), oracle.redo_depth());
    assert_eq!(
        omuse::raster::composite(&candidate.document),
        omuse::raster::composite(&oracle.document)
    );
}

fn configure(editor: &mut Editor, color: [u8; 4], size: f32, opacity: f32) {
    editor.brush.color = color;
    editor.brush.size = size;
    editor.brush.opacity = opacity;
    editor.brush.hardness = 0.65;
}

fn damage(editor: &mut Editor) -> Option<StrokeDamage> {
    editor.take_stroke_damage()
}

fn paint_line(editor: &mut Editor, tool: PaintTool, from: (f32, f32), to: (f32, f32)) {
    assert!(editor.begin_stroke(from.0, from.1, 0.55, tool));
    assert!(editor.continue_stroke(to.0, to.1, 0.9));
    assert!(editor.finish_stroke());
}

#[test]
fn damage_finish_and_cancel_match_the_snapshot_oracle() {
    let (mut candidate, mut oracle) = editors();
    for editor in [&mut candidate, &mut oracle] {
        configure(editor, [210, 45, 90, 230], 21.0, 0.72);
    }

    assert!(candidate.begin_stroke(250.5, 250.5, 0.45, PaintTool::Brush));
    assert!(oracle.begin_stroke(250.5, 250.5, 0.45, PaintTool::Brush));
    assert_eq!(damage(&mut candidate), damage(&mut oracle));
    assert_eq!(damage(&mut candidate), None);
    assert_eq!(damage(&mut oracle), None);
    assert!(candidate.continue_stroke(271.5, 270.5, 0.85));
    assert!(oracle.continue_stroke(271.5, 270.5, 0.85));
    assert_eq!(damage(&mut candidate), damage(&mut oracle));
    assert!(candidate.finish_stroke());
    assert!(oracle.finish_stroke());
    assert_eq!(damage(&mut candidate), None);
    assert_eq!(damage(&mut oracle), None);
    assert_same(&candidate, &oracle);

    assert!(candidate.begin_stroke(40.5, 40.5, 1.0, PaintTool::Pencil));
    assert!(oracle.begin_stroke(40.5, 40.5, 1.0, PaintTool::Pencil));
    assert_eq!(damage(&mut candidate), damage(&mut oracle));
    assert!(candidate.continue_stroke(300.5, 40.5, 0.7));
    assert!(oracle.continue_stroke(300.5, 40.5, 0.7));
    assert_eq!(damage(&mut candidate), damage(&mut oracle));
    candidate.cancel_stroke();
    oracle.cancel_stroke();
    assert_eq!(damage(&mut candidate), None);
    assert_eq!(damage(&mut oracle), None);
    assert_same(&candidate, &oracle);
}

#[test]
fn mode_switch_finishes_the_active_stroke_and_branching_clears_redo() {
    let (mut candidate, mut oracle) = editors();
    for editor in [&mut candidate, &mut oracle] {
        configure(editor, [40, 190, 80, 255], 5.0, 0.8);
        assert!(editor.begin_stroke(12.5, 12.5, 1.0, PaintTool::Pencil));
        assert!(editor.continue_stroke(280.5, 12.5, 0.7));
    }
    candidate.set_region_history_enabled(false);
    oracle.set_region_history_enabled(false);
    assert_same(&candidate, &oracle);
    assert_eq!(candidate.history_stats().raster_patch_entries, 1);

    paint_line(
        &mut candidate,
        PaintTool::Brush,
        (20.5, 80.5),
        (300.5, 80.5),
    );
    paint_line(&mut oracle, PaintTool::Brush, (20.5, 80.5), (300.5, 80.5));
    assert_eq!(candidate.history_stats().raster_patch_entries, 1);
    assert_same(&candidate, &oracle);
    assert!(candidate.undo());
    assert!(oracle.undo());
    assert!(candidate.can_redo() && oracle.can_redo());

    candidate.set_region_history_enabled(true);
    oracle.set_region_history_enabled(false);
    configure(&mut candidate, [30, 70, 240, 255], 3.0, 1.0);
    configure(&mut oracle, [30, 70, 240, 255], 3.0, 1.0);
    paint_line(
        &mut candidate,
        PaintTool::Pencil,
        (50.5, 120.5),
        (270.5, 120.5),
    );
    paint_line(
        &mut oracle,
        PaintTool::Pencil,
        (50.5, 120.5),
        (270.5, 120.5),
    );
    assert!(!candidate.can_redo() && !oracle.can_redo());
    assert_same(&candidate, &oracle);
}

#[test]
fn selected_mask_and_layer_lifetime_fallbacks_mix_with_patches() {
    let (mut candidate, mut oracle) = editors();
    let candidate_paint = candidate.active_layer.clone();
    let oracle_paint = oracle.active_layer.clone();
    configure(&mut candidate, [230, 90, 20, 255], 7.0, 0.75);
    configure(&mut oracle, [230, 90, 20, 255], 7.0, 0.75);
    paint_line(
        &mut candidate,
        PaintTool::Brush,
        (10.5, 10.5),
        (270.5, 270.5),
    );
    paint_line(&mut oracle, PaintTool::Brush, (10.5, 10.5), (270.5, 270.5));
    let patch_entries = candidate.history_stats().raster_patch_entries;

    candidate.select_rectangle(100.0, 100.0, 30.0, 30.0);
    oracle.select_rectangle(100.0, 100.0, 30.0, 30.0);
    paint_line(
        &mut candidate,
        PaintTool::Pencil,
        (105.5, 105.5),
        (125.5, 125.5),
    );
    paint_line(
        &mut oracle,
        PaintTool::Pencil,
        (105.5, 105.5),
        (125.5, 125.5),
    );
    assert_eq!(
        candidate.history_stats().raster_patch_entries,
        patch_entries
    );
    candidate.clear_selection();
    oracle.clear_selection();

    assert!(candidate.add_mask(&candidate_paint, true));
    assert!(oracle.add_mask(&oracle_paint, true));
    for editor in [&mut candidate, &mut oracle] {
        editor.brush.color = [0, 0, 0, 255];
        assert!(editor.begin_mask_stroke(210.5, 210.5, 1.0, PaintTool::Pencil));
        assert!(editor.continue_stroke(220.5, 220.5, 0.8));
        assert!(editor.finish_stroke());
    }
    assert_eq!(
        candidate.history_stats().raster_patch_entries,
        patch_entries
    );
    assert_same(&candidate, &oracle);

    let candidate_background = candidate.document.layers[0].id.clone();
    let oracle_background = oracle.document.layers[0].id.clone();
    assert!(candidate.delete_layer(&candidate_background));
    assert!(oracle.delete_layer(&oracle_background));
    assert_same(&candidate, &oracle);
    assert!(candidate.undo());
    assert!(oracle.undo());
    assert!(
        candidate
            .document
            .find_layer(&candidate_background)
            .is_some()
    );
    assert!(oracle.document.find_layer(&oracle_background).is_some());
    assert_same(&candidate, &oracle);

    while candidate.undo() {
        assert!(oracle.undo());
        assert_same(&candidate, &oracle);
    }
    while candidate.redo() {
        assert!(oracle.redo());
        assert_same(&candidate, &oracle);
    }
}

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }
}

#[test]
fn seeded_mixed_sequences_remain_byte_and_state_exact() {
    for seed in [7_u64, 0x5eed, 0xc0ffee] {
        let (mut candidate, mut oracle) = editors();
        let mut rng = Rng(seed);
        let candidate_id = candidate.active_layer.clone();
        let oracle_id = oracle.active_layer.clone();
        candidate.mark_saved();
        oracle.mark_saved();

        for step in 0..18 {
            match rng.next() % 6 {
                0 | 1 => {
                    let x = 4.5 + (rng.next() % 490) as f32;
                    let y = 4.5 + (rng.next() % 498) as f32;
                    let x2 = (x + (rng.next() % 40) as f32 - 20.0).clamp(0.5, 511.5);
                    let y2 = (y + (rng.next() % 40) as f32 - 20.0).clamp(0.5, 519.5);
                    let color = [
                        rng.next() as u8,
                        rng.next() as u8,
                        rng.next() as u8,
                        180 + (rng.next() % 76) as u8,
                    ];
                    let size = 1.0 + (rng.next() % 15) as f32;
                    let tool = if step % 4 == 0 {
                        PaintTool::Eraser
                    } else if step % 2 == 0 {
                        PaintTool::Pencil
                    } else {
                        PaintTool::Brush
                    };
                    for editor in [&mut candidate, &mut oracle] {
                        configure(editor, color, size, 0.55);
                    }
                    assert!(candidate.begin_stroke(x, y, 0.55, tool));
                    assert!(oracle.begin_stroke(x, y, 0.55, tool));
                    assert!(candidate.continue_stroke(x2, y2, 0.9));
                    assert!(oracle.continue_stroke(x2, y2, 0.9));
                    assert_eq!(candidate.finish_stroke(), oracle.finish_stroke());
                }
                2 => {
                    let name = format!("Layer {seed:x}-{step}");
                    assert_eq!(
                        candidate.rename_layer(&candidate_id, &name),
                        oracle.rename_layer(&oracle_id, &name)
                    );
                }
                3 => {
                    let opacity = 0.2 + (rng.next() % 70) as f32 / 100.0;
                    assert_eq!(
                        candidate.set_opacity(&candidate_id, opacity),
                        oracle.set_opacity(&oracle_id, opacity)
                    );
                }
                4 if candidate.can_undo() && oracle.can_undo() => {
                    assert_eq!(candidate.undo(), oracle.undo());
                }
                5 if candidate.can_redo() && oracle.can_redo() => {
                    assert_eq!(candidate.redo(), oracle.redo());
                }
                _ => {
                    candidate.select_rectangle(2.0, 2.0, 12.0, 12.0);
                    oracle.select_rectangle(2.0, 2.0, 12.0, 12.0);
                    candidate.clear_selection();
                    oracle.clear_selection();
                }
            }
            assert_same(&candidate, &oracle);
            if step == 8 {
                candidate.mark_saved();
                oracle.mark_saved();
                assert_same(&candidate, &oracle);
            }
        }

        while candidate.undo() {
            assert!(oracle.undo());
            assert_same(&candidate, &oracle);
        }
        while candidate.redo() {
            assert!(oracle.redo());
            assert_same(&candidate, &oracle);
        }
    }
}

#[test]
fn frozen_documents_remain_immutable_through_sequence_undo_and_redo() {
    let (mut candidate, _) = editors();
    let frozen = candidate.document.clone();
    let id = candidate.active_layer.clone();
    let frozen_pixels: RgbaImage = frozen
        .find_layer(&id)
        .unwrap()
        .image
        .as_ref()
        .unwrap()
        .to_image();
    configure(&mut candidate, [15, 100, 240, 255], 17.0, 0.8);
    paint_line(
        &mut candidate,
        PaintTool::Brush,
        (240.5, 240.5),
        (280.5, 280.5),
    );
    assert!(candidate.rename_layer(&id, "After paint"));
    assert!(candidate.undo());
    assert!(candidate.undo());
    assert!(candidate.redo());
    assert!(candidate.redo());
    assert_eq!(
        frozen.find_layer(&id).unwrap().image.as_ref().unwrap(),
        &frozen_pixels
    );
    assert_eq!(frozen.find_layer(&id).unwrap().name, "Layer 1");
    assert!(
        frozen
            .find_layer(&id)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .pixels()
            .all(|pixel| *pixel == Rgba([0; 4]))
    );
}
