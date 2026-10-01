use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode, SoftMask, TargetColourUniformity},
    document,
    editor::Editor,
    model::Document,
    precision::{Rgba16Image, TiledImage16},
};
use serde::Deserialize;
use std::sync::{Arc, atomic::AtomicBool};

fn settings() -> TargetColourUniformity {
    TargetColourUniformity {
        target_rgb: [214, 126, 82],
        hue_range_degrees: 35.,
        hue_falloff_degrees: 20.,
        hue_uniformity: 0.6,
        saturation_uniformity: 0.7,
        lightness_uniformity: 0.,
    }
}

fn exact_state() -> LayerState {
    let pixels = Rgba16Image::from_fn(4, 2, |x, y| {
        Rgba([
            45_001 + x as u16 * 911,
            18_003 + y as u16 * 503,
            8_001 + x as u16 * 107,
            20_003 + y as u16 * 30_001,
        ])
    });
    let mut state = LayerState::from_image(&RgbaImage::new(4, 2), "uniformity").unwrap();
    let exact = Arc::new(TiledImage16::from_rgba16(&pixels).unwrap());
    state.source = exact.clone();
    state.result = exact;
    state
}

#[test]
fn target_colour_applies_undoes_redoes_and_survives_reopen() {
    let mut document = Document::new(4, 2);
    let id = document.layers[0].id.clone();
    let state = exact_state();
    let original = state.source.to_rgba16();
    document.layers[0].image = Some(state.proxy().unwrap().into());
    document.layers[0].advanced = Some(Arc::new(state));
    let mut editor = Editor::new(document);

    let mut edited = editor.editable_state(&id).unwrap();
    edited.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Target colour uniformity".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::TargetColourUniformity(settings()),
        soft_mask: None,
    });
    let edited = edited.evaluate(&AtomicBool::new(false)).unwrap();
    assert_ne!(edited.result.to_rgba16(), original);
    assert_eq!(edited.source.to_rgba16(), original);
    assert!(
        edited
            .result
            .to_rgba16()
            .pixels()
            .zip(original.pixels())
            .all(|(actual, before)| actual[3] == before[3])
    );
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), edited)])
            .unwrap()
    );
    let applied = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap()
        .result
        .to_rgba16();
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
        original
    );
    assert!(editor.redo());
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
        applied
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("uniformity.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    let reopened = reopened.find_layer(&id).unwrap().advanced.as_ref().unwrap();
    assert_eq!(reopened.source.to_rgba16(), original);
    assert_eq!(reopened.result.to_rgba16(), applied);
    assert_eq!(
        reopened.recipe.nodes[0].operation,
        AdvancedOperation::TargetColourUniformity(settings())
    );
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum PreUniformityReader {
    Denoise { radius: u8, strength: f32 },
}

#[test]
fn serialized_variant_is_explicit_and_old_readers_refuse_it() {
    let value =
        serde_json::to_value(AdvancedOperation::TargetColourUniformity(settings())).unwrap();
    assert_eq!(value["type"], "targetColourUniformity");
    assert_eq!(value["targetRgb"], serde_json::json!([214, 126, 82]));
    assert!(serde_json::from_value::<PreUniformityReader>(value).is_err());
}

#[test]
fn transparent_hidden_rgb_respects_selection_mask_and_noop() {
    let pixels = Rgba16Image::from_vec(
        3,
        1,
        vec![
            200 * 257,
            90 * 257,
            50 * 257,
            0, // selected by the warm target
            10 * 257,
            30 * 257,
            230 * 257,
            0, // outside the target hue
            190 * 257,
            80 * 257,
            40 * 257,
            0, // selected, but excluded by the node mask
        ],
    )
    .unwrap();
    let mut state = LayerState::from_rgba16(&pixels, "transparent uniformity").unwrap();
    state.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Target colour uniformity".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::TargetColourUniformity(settings()),
        soft_mask: Some(SoftMask::new(3, 1, vec![255, 255, 0]).unwrap()),
    });
    let evaluated = state.evaluate(&AtomicBool::new(false)).unwrap();
    let result = evaluated.result.to_rgba16();

    assert_ne!(
        &result.get_pixel(0, 0).0[..3],
        &pixels.get_pixel(0, 0).0[..3]
    );
    assert!(
        result.get_pixel(0, 0).0[..3]
            .iter()
            .any(|value| *value != 0)
    );
    assert_eq!(result.get_pixel(0, 0)[3], 0);
    assert_eq!(result.get_pixel(1, 0), pixels.get_pixel(1, 0));
    assert_eq!(result.get_pixel(2, 0), pixels.get_pixel(2, 0));
    assert_eq!(evaluated.source.to_rgba16(), pixels);
    assert_eq!(state.source.to_rgba16(), pixels);

    let mut no_op = settings();
    no_op.hue_uniformity = 0.;
    no_op.saturation_uniformity = 0.;
    no_op.lightness_uniformity = 0.;
    let mut no_op_state = LayerState::from_rgba16(&pixels, "transparent no-op").unwrap();
    no_op_state.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Target colour no-op".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::TargetColourUniformity(no_op),
        soft_mask: None,
    });
    let no_op_result = no_op_state.evaluate(&AtomicBool::new(false)).unwrap();
    assert_eq!(no_op_result.result.to_rgba16(), pixels);
    assert_eq!(no_op_result.source.to_rgba16(), pixels);
}
