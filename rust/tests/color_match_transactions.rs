use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode, SoftMask},
    color_match::{self, Settings},
    document,
    editor::Editor,
    model::Document,
    precision::{Rgba16Image, WorkingSpace},
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn reference_grade_preserves_original_precision_masks_and_one_undo_through_reopen() {
    let cancel = AtomicBool::new(false);
    let pixels = Rgba16Image::from_fn(4, 2, |x, y| {
        Rgba([
            17001 + x as u16 * 5501,
            9003 + y as u16 * 5501,
            22007 + x as u16 * 4003,
            if x == 0 { 0 } else { 32003 + y as u16 * 20001 },
        ])
    });
    let state = LayerState::from_rgba16(&pixels, "Sixteen-bit original").unwrap();
    let mut document = Document::new(4, 2);
    let id = document.layers[0].id.clone();
    document.layers[0].image = Some(state.proxy().unwrap().into());
    document.layers[0].advanced = Some(Arc::new(state));
    document.layers[0].offset_x = -0.75;
    document.layers[0].scale_y = -1.25;
    document.layers[0].rotation = 21.;
    let mut editor = Editor::new(document);
    let reference = RgbaImage::from_pixel(7, 3, Rgba([225, 167, 107, 255]));
    let effect = Settings {
        version: 1,
        reference: color_match::statistics8(&reference, &cancel).unwrap(),
        amount: 0.63,
        preserve_lightness: true,
    };
    let mut draft = editor.editable_state(&id).unwrap();
    let full = color_match::apply16(&draft.source, &effect, &cancel)
        .unwrap()
        .to_rgba16();
    let mask = vec![255, 0, 128, 255, 0, 128, 255, 255];
    draft.recipe.nodes.push(FilterNode {
        id: "reference-colour".into(),
        name: "Match reference colour".into(),
        enabled: true,
        opacity: 0.75,
        operation: AdvancedOperation::ReferenceColourMatch(effect.clone()),
        soft_mask: Some(SoftMask::new(4, 2, mask.clone()).unwrap()),
    });
    let draft = draft.evaluate(&cancel).unwrap();
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(
        editor.editable_state(&id).unwrap().result.to_rgba16(),
        pixels
    );
    let expected = Rgba16Image::from_fn(4, 2, |x, y| {
        let old = pixels.get_pixel(x, y).0;
        let candidate = full.get_pixel(x, y).0;
        let t = f64::from(0.75f32 * f32::from(mask[(y * 4 + x) as usize]) / 255.);
        Rgba([
            ((1. - t) * f64::from(old[0]) + t * f64::from(candidate[0])).round() as u16,
            ((1. - t) * f64::from(old[1]) + t * f64::from(candidate[1])).round() as u16,
            ((1. - t) * f64::from(old[2]) + t * f64::from(candidate[2])).round() as u16,
            old[3],
        ])
    });
    assert_eq!(draft.result.to_rgba16(), expected);
    assert_ne!(expected, pixels);
    editor
        .replace_editable_states(vec![(id.clone(), draft)])
        .unwrap();
    assert_eq!(editor.undo_depth(), 1);
    let layer = editor.document.find_layer(&id).unwrap();
    assert_eq!(
        (layer.offset_x, layer.scale_y, layer.rotation),
        (-0.75, -1.25, 21.)
    );
    assert!(editor.undo());
    assert_eq!(
        editor.editable_state(&id).unwrap().result.to_rgba16(),
        pixels
    );
    assert!(editor.redo());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reference-grade.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    let state = reopened.find_layer(&id).unwrap().advanced.as_ref().unwrap();
    assert_eq!(state.source.to_rgba16(), pixels);
    assert_eq!(state.result.to_rgba16(), expected);
    assert_eq!(
        state.recipe.nodes[0].operation,
        AdvancedOperation::ReferenceColourMatch(effect)
    );
    assert_eq!(state.recipe.nodes[0].soft_mask.as_ref().unwrap().data, mask);
    assert_eq!(
        state.evaluate(&cancel).unwrap().result.to_rgba16(),
        expected
    );
    assert_eq!(state.result.working_space(), WorkingSpace::Srgb);
}

#[test]
fn alpha_preserving_nodes_round_midpoints_identically_at_every_alpha() {
    // Half-strength inversion is exactly the midpoint of the 16-bit channel
    // range. A colour-only adjustment must round it up for every alpha,
    // including hidden RGB, without an alpha-dependent one-unit bias.
    let pixels = Rgba16Image::from_fn(256, 256, |x, y| {
        Rgba([28003, 9003, 33011, (y * 256 + x) as u16])
    });
    let mut state = LayerState::from_rgba16(&pixels, "Every alpha").unwrap();
    state.recipe.nodes.push(FilterNode {
        id: "half-invert".into(),
        name: "Half-strength invert".into(),
        enabled: true,
        opacity: 0.5,
        operation: AdvancedOperation::Filter(omuse::filters::Filter::Invert),
        soft_mask: None,
    });
    let result = state.evaluate(&AtomicBool::new(false)).unwrap();
    for (index, pixel) in result.result.to_rgba16().pixels().enumerate() {
        assert_eq!(
            pixel.0,
            [32768, 32768, 32768, index as u16],
            "half-strength inversion at alpha {index}"
        );
    }
    assert_eq!(state.source.to_rgba16(), pixels);
}

#[test]
fn cancelling_or_rejecting_a_reference_draft_never_changes_pixels_or_history() {
    let source = Rgba16Image::from_pixel(2, 2, Rgba([10001, 20003, 30007, 50001]));
    let state = LayerState::from_rgba16(&source, "Original").unwrap();
    let mut document = Document::new(2, 2);
    document.layers[0].image = Some(state.proxy().unwrap().into());
    document.layers[0].advanced = Some(Arc::new(state));
    let editor = Editor::new(document);
    let mut draft = editor.editable_state(&editor.active_layer).unwrap();
    let settings = Settings {
        version: 1,
        reference: color_match::statistics16(&draft.source, &AtomicBool::new(false)).unwrap(),
        amount: 1.,
        preserve_lightness: false,
    };
    draft.recipe.nodes.push(FilterNode {
        id: "reference".into(),
        name: "Reference".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::ReferenceColourMatch(settings),
        soft_mask: None,
    });
    assert!(draft.evaluate(&AtomicBool::new(true)).is_err());
    if let AdvancedOperation::ReferenceColourMatch(settings) = &mut draft.recipe.nodes[0].operation
    {
        settings.reference.deviation[0] = f32::INFINITY;
    }
    assert!(draft.evaluate(&AtomicBool::new(false)).is_err());
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(
        editor
            .editable_state(&editor.active_layer)
            .unwrap()
            .result
            .to_rgba16(),
        source
    );
}
