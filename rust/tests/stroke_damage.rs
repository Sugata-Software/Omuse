use image::{Rgba, RgbaImage};
use omuse::{
    editor::{Editor, PaintTool, Selection, StrokeDamage},
    model::{Document, PixelRect},
};

fn editor() -> Editor {
    let mut e = Editor::new(Document::new(24, 20));
    e.brush.size = 1.;
    e.brush.hardness = 1.;
    e.brush.color = [210, 45, 70, 255];
    e
}
fn pixels(e: &Editor) -> RgbaImage {
    e.document
        .find_layer(&e.active_layer)
        .unwrap()
        .image
        .as_ref()
        .unwrap()
        .to_image()
}
fn changed_rect(before: &RgbaImage, after: &RgbaImage) -> Option<PixelRect> {
    let (mut left, mut top, mut right, mut bottom) = (before.width(), before.height(), 0, 0);
    for (x, y, old) in before.enumerate_pixels() {
        if old != after.get_pixel(x, y) {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (right > left && bottom > top).then(|| PixelRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}
fn damage(e: &mut Editor, expected: PixelRect, mask_target: bool) {
    assert_eq!(
        e.take_stroke_damage(),
        Some(StrokeDamage {
            layer_id: e.active_layer.clone(),
            local_rect: expected,
            mask_target,
        })
    );
    assert_eq!(e.take_stroke_damage(), None);
}

#[test]
fn drain_reaccumulate_and_finish_have_exact_local_bounds() {
    let mut e = editor();
    assert_eq!(e.take_stroke_damage(), None);
    assert!(e.begin_stroke(2.5, 3.5, 1., PaintTool::Pencil));
    damage(
        &mut e,
        PixelRect {
            x: 2,
            y: 3,
            width: 1,
            height: 1,
        },
        false,
    );
    let before = pixels(&e);
    assert!(e.continue_stroke(7.5, 3.5, 1.));
    let expected = changed_rect(&before, &pixels(&e)).unwrap();
    damage(&mut e, expected, false);
    assert!(e.continue_stroke(7.5, 3.5, 1.));
    assert_eq!(e.take_stroke_damage(), None);
    assert!(e.continue_stroke(7.5, 7.5, 1.));
    assert!(e.finish_stroke());
    assert_eq!(e.take_stroke_damage(), None);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn sparse_selection_reports_changed_pixels_instead_of_brush_extent() {
    let mut e = editor();
    e.brush.size = 18.;
    let mut selection = Selection {
        width: 24,
        height: 20,
        mask: vec![0; 24 * 20],
    };
    selection.mask[5 * 24 + 6] = 255;
    selection.mask[9 * 24 + 11] = 255;
    e.selection = Some(selection);
    assert!(e.begin_stroke(9., 8., 1., PaintTool::Brush));
    damage(
        &mut e,
        PixelRect {
            x: 6,
            y: 5,
            width: 6,
            height: 5,
        },
        false,
    );
    assert_eq!(pixels(&e).pixels().filter(|p| p[3] != 0).count(), 2);
}

#[test]
fn unchanged_stamps_preserve_shared_allocations_and_produce_no_damage() {
    for kind in 0..7 {
        let mut e = editor();
        let mut pressure = 1.;
        let mut tool = PaintTool::Brush;
        match kind {
            0 => e.brush.opacity = 0.,
            1 => e.brush.color[3] = 0,
            2 => pressure = 0.,
            3 => tool = PaintTool::Eraser,
            4..=6 => {
                e.document.layers[0].image =
                    Some(RgbaImage::from_pixel(24, 20, Rgba(e.brush.color)).into());
            }
            _ => unreachable!(),
        }
        let original = e.document.layers[0].image.clone().unwrap();
        if kind >= 5 {
            assert!(e.begin_clone_stroke((2.5, 2.5), (8.5, 8.5), kind == 6));
        } else {
            assert!(e.begin_stroke(8.5, 8.5, pressure, tool));
        }
        assert_eq!(e.take_stroke_damage(), None, "case {kind}");
        assert!(
            original.shares_pixels_with(e.document.layers[0].image.as_ref().unwrap()),
            "case {kind} detached"
        );
        assert!(!e.finish_stroke());
        assert_eq!(e.undo_depth(), 0);
    }
}

#[test]
fn outside_image_and_missing_clone_samples_do_not_detach() {
    let mut e = editor();
    let original = e.document.layers[0].image.clone().unwrap();
    assert!(e.begin_stroke(-100., -100., 1., PaintTool::Brush));
    assert!(e.continue_stroke(-80., -80., 1.));
    assert_eq!(e.take_stroke_damage(), None);
    assert!(!e.finish_stroke());
    assert!(e.begin_clone_stroke((-10., -10.), (8.5, 8.5), false));
    assert_eq!(e.take_stroke_damage(), None);
    assert!(original.shares_pixels_with(e.document.layers[0].image.as_ref().unwrap()));
}

#[test]
fn eraser_damage_excludes_already_transparent_pixels() {
    let mut e = editor();
    e.document.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(7, 9, Rgba([11, 12, 13, 255]));
    e.brush.size = 12.;
    assert!(e.begin_stroke(8., 8., 1., PaintTool::Eraser));
    damage(
        &mut e,
        PixelRect {
            x: 7,
            y: 9,
            width: 1,
            height: 1,
        },
        false,
    );
    assert!(e.finish_stroke());
    assert_eq!(pixels(&e).get_pixel(7, 9).0, [0; 4]);
}

#[test]
fn mask_damage_identifies_mask_and_preserves_source_image_sharing() {
    let mut e = editor();
    let id = e.active_layer.clone();
    assert!(e.add_mask(&id, true));
    let original = e.document.layers[0].image.clone().unwrap();
    e.brush.color = [0, 0, 0, 255];
    assert!(e.begin_mask_stroke(4.5, 6.5, 1., PaintTool::Pencil));
    damage(
        &mut e,
        PixelRect {
            x: 4,
            y: 6,
            width: 1,
            height: 1,
        },
        true,
    );
    assert!(original.shares_pixels_with(e.document.layers[0].image.as_ref().unwrap()));
    e.cancel_stroke();
    assert_eq!(e.take_stroke_damage(), None);
    assert_eq!(
        e.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(4, 6)
            .0,
        [255; 4]
    );
    let original_mask = e.document.layers[0].mask.clone().unwrap();
    e.brush.color = [255; 4];
    assert!(e.begin_mask_stroke(4.5, 6.5, 1., PaintTool::Brush));
    assert_eq!(e.take_stroke_damage(), None);
    assert!(original_mask.shares_pixels_with(e.document.layers[0].mask.as_ref().unwrap()));
}

#[test]
fn transformed_layer_damage_stays_in_source_pixel_coordinates() {
    let mut e = editor();
    e.document.layers[0].offset_x = 5.;
    e.document.layers[0].offset_y = -2.;
    assert!(e.begin_stroke(8.5, 4.5, 1., PaintTool::Pencil));
    damage(
        &mut e,
        PixelRect {
            x: 3,
            y: 6,
            width: 1,
            height: 1,
        },
        false,
    );
}

#[test]
fn accumulated_damage_matches_interpolated_stamps_and_frozen_clone_source() {
    for heal in [false, true] {
        let mut e = editor();
        // Transparent target makes heal retain the frozen source colors.
        for x in 1..7 {
            e.document.layers[0].image.as_mut().unwrap().put_pixel(
                x,
                2,
                Rgba([x as u8 * 20, 50, 60, 255]),
            );
        }
        let before = pixels(&e);
        assert!(e.begin_clone_stroke((1.5, 2.5), (2.5, 2.5), heal));
        assert!(e.continue_clone_stroke((7.5, 2.5)));
        let after = pixels(&e);
        damage(&mut e, changed_rect(&before, &after).unwrap(), false);
        if !heal {
            assert_eq!(after.get_pixel(3, 2), before.get_pixel(2, 2));
            assert_eq!(after.get_pixel(7, 2), before.get_pixel(6, 2));
        }
        e.cancel_clone_stroke();
        assert_eq!(e.take_stroke_damage(), None);
        assert_eq!(pixels(&e), before);
        assert_eq!(e.undo_depth(), 0);
    }
}

#[test]
fn smoothing_slack_has_no_damage_and_release_discards_pending_bounds() {
    for tool in [PaintTool::Brush, PaintTool::Eraser] {
        let mut e = editor();
        if tool == PaintTool::Eraser {
            e.document.layers[0].image =
                Some(RgbaImage::from_pixel(24, 20, Rgba(e.brush.color)).into());
        }
        e.brush.smoothing = 5.;
        assert!(e.begin_stroke(2.5, 3.5, 1., tool));
        assert!(e.take_stroke_damage().is_some());
        let before_slack = e.document.layers[0].image.clone().unwrap();
        assert!(e.continue_stroke(5.5, 3.5, 1.));
        assert_eq!(e.take_stroke_damage(), None);
        assert!(before_slack.shares_pixels_with(e.document.layers[0].image.as_ref().unwrap()));
        assert_eq!(pixels(&e), before_slack.to_image());
        let before = pixels(&e);
        assert!(e.continue_stroke(12.5, 3.5, 1.));
        let expected = changed_rect(&before, &pixels(&e)).unwrap();
        damage(&mut e, expected, false);
        assert!(e.finish_stroke());
        assert_eq!(e.take_stroke_damage(), None);
        assert_eq!(
            pixels(&e).get_pixel(12, 3).0,
            if tool == PaintTool::Eraser {
                [0; 4]
            } else {
                e.brush.color
            }
        );
    }
}

#[test]
fn cancellation_and_replacement_do_not_leak_old_stroke_damage() {
    let mut e = editor();
    let original = e.document.layers[0].image.clone().unwrap();
    assert!(e.begin_stroke(2.5, 3.5, 1., PaintTool::Pencil));
    e.cancel_stroke();
    assert_eq!(e.take_stroke_damage(), None);
    assert!(original.shares_pixels_with(e.document.layers[0].image.as_ref().unwrap()));
    assert!(e.begin_stroke(2.5, 3.5, 1., PaintTool::Pencil));
    assert!(e.begin_stroke(18.5, 13.5, 1., PaintTool::Pencil));
    damage(
        &mut e,
        PixelRect {
            x: 18,
            y: 13,
            width: 1,
            height: 1,
        },
        false,
    );
    assert!(e.finish_stroke());
    assert_eq!(e.take_stroke_damage(), None);
    assert_eq!(e.undo_depth(), 2);
}
