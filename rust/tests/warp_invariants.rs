//! Independent protection invariants; no production sampler in expected values.
use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{self, AdvancedOperation, FilterNode, SoftMask, WarpMesh, WarpPin},
    advanced16,
    editor::Editor,
    model::Document,
    precision::{Rgba16Image, TiledImage16},
};
use std::sync::{Arc, atomic::AtomicBool};
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn mesh() -> WarpMesh {
    WarpMesh {
        columns: 2,
        rows: 2,
        points: vec![[0.125, 0.], [1.125, 0.], [0.125, 1.], [1.125, 1.]],
        pins: vec![],
        freeze_mask: None,
        protect_mask: None,
    }
}
fn node(w: WarpMesh) -> FilterNode {
    FilterNode {
        id: "06a9721f-bafe-45c3-9582-dcd55e330a23".into(),
        name: "Protected warp".into(),
        enabled: true,
        opacity: 1.,
        soft_mask: None,
        operation: AdvancedOperation::Warp(w),
    }
}
#[test]
fn full_freeze_and_protect_keep_hidden_and_partial_alpha_exact_in_both_kernels() {
    let bytes = RgbaImage::from_fn(9, 7, |x, y| {
        Rgba([
            17 + x as u8 * 13,
            31 + y as u8 * 17,
            79,
            [0, 1, 127, 254, 255][((x + y) % 5) as usize],
        ])
    });
    let words = Rgba16Image::from_fn(9, 7, |x, y| {
        let p = bytes.get_pixel(x, y);
        Rgba([
            p[0] as u16 * 257 + 1,
            p[1] as u16 * 257 + 3,
            21007,
            [0, 1, 257, 32768, 65535][((x + y) % 5) as usize],
        ])
    });
    for frozen in [false, true] {
        let mut warp = mesh();
        let mask = SoftMask::new(9, 7, vec![255; 63]).unwrap();
        if frozen {
            warp.freeze_mask = Some(mask)
        } else {
            warp.protect_mask = Some(mask)
        }
        warp.pins = vec![
            WarpPin {
                source: [0.3, 0.4],
                target: [0.6, 0.2],
                radius: 0.8,
                strength: 0.75,
            },
            WarpPin {
                source: [0.7, 0.6],
                target: [0.4, 0.8],
                radius: 0.6,
                strength: 0.5,
            },
        ];
        assert_eq!(
            advanced_ops::evaluate(&bytes, &[node(warp.clone())]).unwrap(),
            bytes,
            "byte fully frozen={frozen}"
        );
        let master = TiledImage16::from_rgba16(&words).unwrap();
        assert_eq!(
            advanced16::evaluate(&master, &AdvancedOperation::Warp(warp), &idle())
                .unwrap()
                .unwrap()
                .to_rgba16(),
            words
        );
    }
}
#[test]
fn soft_freeze_and_protection_follow_distinct_independent_translation_oracles() {
    let bytes = RgbaImage::from_fn(9, 7, |x, y| {
        Rgba([30 + x as u8 * 10, 40 + y as u8 * 10, 79, 255])
    });
    let words = Rgba16Image::from_fn(9, 7, |x, y| {
        Rgba([
            10001 + x as u16 * 1000,
            20003 + y as u16 * 1100,
            30007,
            65535,
        ])
    });
    for frozen in [false, true] {
        for amount in [0u8, 64, 128, 192, 255] {
            let mut warp = mesh();
            let mask = SoftMask::new(9, 7, vec![amount; 63]).unwrap();
            if frozen {
                warp.freeze_mask = Some(mask)
            } else {
                warp.protect_mask = Some(mask)
            }
            let output = advanced_ops::evaluate(&bytes, &[node(warp.clone())]).unwrap();
            let master = TiledImage16::from_rgba16(&words).unwrap();
            let result = advanced16::evaluate(&master, &AdvancedOperation::Warp(warp), &idle())
                .unwrap()
                .unwrap();
            // Interior mesh translation is exactly one source pixel. Freeze scales the
            // inverse displacement; Protect blends the displaced and original pixels.
            let coverage = 1. - f64::from(amount) / 255.;
            for y in 1..6 {
                for x in 3..7 {
                    let expected_byte =
                        (f64::from(bytes.get_pixel(x, y)[0]) - 10. * coverage).round() as u8;
                    let expected_word =
                        (f64::from(words.get_pixel(x, y)[0]) - 1000. * coverage).round() as u16;
                    assert!(output.get_pixel(x, y)[0].abs_diff(expected_byte) <= 1);
                    assert!(result.get_pixel(x, y).0[0].abs_diff(expected_word) <= 1);
                    assert_eq!(output.get_pixel(x, y)[1], bytes.get_pixel(x, y)[1]);
                    assert_eq!(result.get_pixel(x, y).0[1], words.get_pixel(x, y)[1]);
                }
            }
        }
    }
}
#[test]
fn protected_pin_recipe_is_immutable_cancelled_reversible_and_portable() {
    let words = Rgba16Image::from_fn(9, 7, |x, y| {
        Rgba([
            10001 + x as u16 * 919,
            20003 + y as u16 * 733,
            30007,
            [0, 1, 32768, 65535][((x + y) % 4) as usize],
        ])
    });
    let mut warp = mesh();
    let mask: Vec<u8> = (0..63)
        .map(|i| {
            if i % 9 < 3 {
                255
            } else if i % 9 == 3 {
                128
            } else {
                0
            }
        })
        .collect();
    warp.protect_mask = Some(SoftMask::new(9, 7, mask.clone()).unwrap());
    warp.pins = vec![
        WarpPin {
            source: [0.2, 0.3],
            target: [0.4, 0.2],
            radius: 0.7,
            strength: 0.8,
        },
        WarpPin {
            source: [0.7, 0.8],
            target: [0.6, 0.6],
            radius: 0.4,
            strength: 0.5,
        },
    ];
    let state = LayerState::from_rgba16(&words, "Protected pixels").unwrap();
    let mut doc = Document::new(9, 7);
    doc.layers[0].image = Some(state.proxy().unwrap().into());
    doc.layers[0].advanced = Some(Arc::new(state));
    let mut editor = Editor::new(doc);
    let id = editor.active_layer.clone();
    let source = editor.editable_state(&id).unwrap();
    let mut request = source.clone();
    request.recipe.nodes.push(node(warp));
    assert!(request.evaluate(&AtomicBool::new(true)).is_err());
    assert_eq!(source.result.to_rgba16(), words);
    assert_eq!(editor.undo_depth(), 0);
    let result = request.evaluate(&idle()).unwrap();
    let expected = result.result.to_rgba16();
    for (i, (a, b)) in words.pixels().zip(expected.pixels()).enumerate() {
        if mask[i] == 255 {
            assert_eq!(a, b)
        }
    }
    editor
        .replace_editable_states(vec![(id.clone(), result)])
        .unwrap();
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(
        editor.editable_state(&id).unwrap().result.to_rgba16(),
        words
    );
    assert!(editor.redo());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protected.omuse");
    omuse::document::save(&editor.document, &path).unwrap();
    let opened = omuse::document::open(&path).unwrap();
    let saved = opened.find_layer(&id).unwrap().advanced.as_ref().unwrap();
    assert_eq!(saved.source.to_rgba16(), words);
    assert_eq!(saved.result.to_rgba16(), expected);
    assert_eq!(
        saved.evaluate(&idle()).unwrap().result.to_rgba16(),
        expected
    );
}

#[test]
fn identity_warp_does_not_normalize_hidden_pixel_data() {
    let image = RgbaImage::from_fn(9, 7, |x, y| {
        Rgba([
            11 + x as u8 * 13,
            23 + y as u8 * 17,
            37,
            [0, 1, 127, 255][((x + y) % 4) as usize],
        ])
    });
    let mut warp = mesh();
    warp.points = vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]];
    assert_eq!(
        advanced_ops::evaluate(&image, &[node(warp)]).unwrap(),
        image
    );
}
