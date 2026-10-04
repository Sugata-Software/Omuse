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
