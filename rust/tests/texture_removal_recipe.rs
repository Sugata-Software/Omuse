//! Persisted opt-in removal must retain original high-precision samples.
use image::Rgba;
use omuse::{
    advanced::LayerState,
    advanced_ops::{
        AdvancedOperation, ContentAwareAlgorithm, ContentAwareReplace, FilterNode, SoftMask,
    },
    advanced16, document,
    editor::Editor,
    model::Document,
    precision::{Rgba16Image, TiledImage16, WorkingSpace},
    retouch::texture,
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn texture_recipe_preserves_original_16_bit_donors_undo_cancel_and_saved_reopen() {
    let pixels = Rgba16Image::from_fn(64, 56, |x, y| {
        if (24..34).contains(&x) && (20..30).contains(&y) {
            Rgba([65003, 1003, 58007, 65535])
        } else {
            let base = if (x + y) % 3 == 0 { 7001 } else { 36003 };
            Rgba([
                base + x as u16 * 17,
                base + y as u16 * 23,
                base + (x + y) as u16 * 11,
                if (x, y) == (0, 0) { 12345 } else { 65535 },
            ])
        }
    });
    let source = Arc::new(TiledImage16::from_rgba16(&pixels).unwrap());
    let matching = source.to_rgba8_in(WorkingSpace::Srgb).unwrap();
    let mut target = vec![0; 64 * 56];
    let mut allowed = vec![255; target.len()];
    for y in 20..30 {
        for x in 24..34 {
            target[y * 64 + x] = 255;
            allowed[y * 64 + x] = 0;
        }
    }
    let settings = ContentAwareReplace {
        algorithm: ContentAwareAlgorithm::TextureV2,
        target_mask: SoftMask::new(64, 56, target.clone()).unwrap(),
        allowed_source_mask: SoftMask::new(64, 56, allowed.clone()).unwrap(),
        search_radius: 24,
        patch_radius: 2,
        feather: 0.0,
    };
    let mut serialized = serde_json::to_value(&settings).unwrap();
    assert_eq!(serialized["algorithm"], "textureV2");
    let roundtrip: ContentAwareReplace = serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(roundtrip, settings);
    serialized.as_object_mut().unwrap().remove("algorithm");
    assert_eq!(
        serde_json::from_value::<ContentAwareReplace>(serialized.clone())
            .unwrap()
            .algorithm,
        ContentAwareAlgorithm::Legacy,
        "Old missing-version recipes retain their original algorithm"
    );
    serialized["algorithm"] = serde_json::json!("textureV999");
    assert!(serde_json::from_value::<ContentAwareReplace>(serialized).is_err());

    let mut expected = pixels.clone();
    let mut donors = Vec::new();
    texture::visit_samples(
        &matching,
        &target,
        &allowed,
        24,
        2,
        0.0,
        &AtomicBool::new(false),
        |x, y, sx, sy, coverage| {
            assert_eq!(coverage, 1.0);
            assert_eq!(target[(sy * 64 + sx) as usize], 0);
            assert_ne!(allowed[(sy * 64 + sx) as usize], 0);
            expected.put_pixel(x, y, *pixels.get_pixel(sx, sy));
            donors.push((sx, sy));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(donors.len(), 100);
    assert!(
        donors
            .iter()
            .any(|&(x, y)| pixels.get_pixel(x, y)[0] % 257 != 0)
    );
    let operation = AdvancedOperation::ContentAwareReplace(settings);
    let result = advanced16::evaluate(&source, &operation, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.to_rgba16(),
        expected,
        "Copy original words, not an 8-bit preview expanded to 16 bits"
    );
    assert_eq!(source.to_rgba16(), pixels);
    for (i, (before, after)) in pixels.pixels().zip(expected.pixels()).enumerate() {
        if target[i] == 0 {
            assert_eq!(before, after, "Unmasked 16-bit pixel {i}");
        }
    }

    let mut state = LayerState::from_image(&matching, "Texture removal precision fixture").unwrap();
    state.source = source.clone();
    state.result = source.clone();
    let mut doc = Document::new(64, 56);
    doc.layers[0].image = Some(matching.into());
    doc.layers[0].advanced = Some(Arc::new(state));
    let mut editor = Editor::new(doc);
    let id = editor.active_layer.clone();
    let mut requested = editor.editable_state(&id).unwrap();
    requested.recipe.nodes.push(FilterNode {
        id: "texture-removal-v2".into(),
        name: "Texture removal".into(),
        enabled: true,
        opacity: 1.0,
        operation,
        soft_mask: None,
    });
    let revision = editor.revision();
    let history = editor.undo_depth();
    assert!(requested.evaluate(&AtomicBool::new(true)).is_err());
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.undo_depth(), history);
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16(),
        pixels
    );
    let evaluated = requested.evaluate(&AtomicBool::new(false)).unwrap();
    assert_eq!(evaluated.result.to_rgba16(), expected);
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), evaluated)])
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), history + 1);
    assert!(editor.undo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16(),
        pixels
    );
    assert!(editor.redo());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Texture-v2.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    let saved = reopened.find_layer(&id).unwrap().advanced.as_ref().unwrap();
    assert_eq!(saved.source.to_rgba16(), pixels);
    assert_eq!(saved.result.to_rgba16(), expected);
    assert!(matches!(&saved.recipe.nodes[0].operation,
        AdvancedOperation::ContentAwareReplace(s) if s.algorithm == ContentAwareAlgorithm::TextureV2));
    assert_eq!(saved.recipe.nodes, requested.recipe.nodes);
    assert_eq!(
        saved
            .evaluate(&AtomicBool::new(false))
            .unwrap()
            .result
            .to_rgba16(),
        expected
    );
}
