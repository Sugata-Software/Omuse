use image::{Rgba, RgbaImage};
use omuse::{
    advanced::{Component, LayerState},
    advanced_ops::{AdvancedOperation, FilterNode},
    document,
    editor::Editor,
    filters::Filter,
    model::{Document, Layer},
    objects::{LiveShapeKind, LiveShapeStyle, LiveTextStyle},
    precision::{Rgba16Image, TiledImage16},
    raster,
    vector_path::{Anchor, Point, Subpath, VectorPath},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn source16(width: u32, height: u32) -> Rgba16Image {
    Rgba16Image::from_fn(width, height, |x, y| {
        Rgba([
            701 + x as u16 * 911 + y as u16 * 37,
            12_345 + x as u16 * 1_003,
            45_678 - y as u16 * 701,
            65_535,
        ])
    })
}

fn state_with_exact_source(width: u32, height: u32) -> LayerState {
    let proxy = RgbaImage::from_pixel(width, height, Rgba([20, 30, 40, 255]));
    let mut state = LayerState::from_image(&proxy, "precision fixture").unwrap();
    let exact = Arc::new(TiledImage16::from_rgba16(&source16(width, height)).unwrap());
    state.source = exact.clone();
    state.result = exact;
    state
}

fn filter_node(filter: Filter) -> FilterNode {
    FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "precision filter".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::Filter(filter),
        soft_mask: None,
    }
}

fn document_with_state(state: LayerState) -> (Document, String) {
    let mut doc = Document::new(4, 3);
    let id = doc.layers[0].id.clone();
    doc.layers[0].image = Some(RgbaImage::from_pixel(4, 3, Rgba([20, 30, 40, 255])).into());
    doc.layers[0].advanced = Some(Arc::new(state));
    (doc, id)
}

#[test]
fn live_object_setters_reject_advanced_sources_atomically() {
    let (document, id) = document_with_state(state_with_exact_source(4, 3));
    let mut editor = Editor::new(document);
    let before = editor.document.find_layer(&id).unwrap().clone();
    let before_revision = editor.revision();
    let before_history = editor.undo_depth();

    assert!(!editor.set_live_text(&id, LiveTextStyle::default()).unwrap());
    assert!(
        !editor
            .set_live_shape(
                &id,
                LiveShapeStyle {
                    kind: LiveShapeKind::Rectangle,
                    red: 1.,
                    green: 0.,
                    blue: 0.,
                    corner_radius: 0.,
                    line_width: None,
                    start: None,
                    end: None,
                },
                4,
                3,
            )
            .unwrap()
    );

    let after = editor.document.find_layer(&id).unwrap();
    assert_eq!(after.image, before.image);
    assert_eq!(after.metadata, before.metadata);
    assert!(Arc::ptr_eq(
        after.advanced.as_ref().unwrap(),
        before.advanced.as_ref().unwrap()
    ));
    assert_eq!(editor.revision(), before_revision);
    assert_eq!(editor.undo_depth(), before_history);
}

#[test]
fn vector_mask_rebuild_is_explicit_and_preserves_raster_mask_edits() {
    let (document, id) = document_with_state(state_with_exact_source(4, 3));
    let mut editor = Editor::new(document);
    let path = VectorPath {
        subpaths: vec![Subpath {
            closed: true,
            anchors: vec![
                Anchor {
                    position: Point { x: 0., y: 0. },
                    incoming: None,
                    outgoing: None,
                },
                Anchor {
                    position: Point { x: 4., y: 0. },
                    incoming: None,
                    outgoing: None,
                },
                Anchor {
                    position: Point { x: 0., y: 3. },
                    incoming: None,
                    outgoing: None,
                },
            ],
        }],
        fill_rule: Default::default(),
    };
    let mut vector = editor.editable_state(&id).unwrap();
    vector.recipe.vector = Some(path.clone());
    vector.recipe.vector_is_mask = true;
    let vector = vector.evaluate(&AtomicBool::new(false)).unwrap();
    assert!(editor.replace_vector_state(&id, vector).unwrap());

    let corrected = RgbaImage::from_pixel(4, 3, Rgba([77, 77, 77, 255]));
    let placement = json!({
        "origin": [0.25, 0.5],
        "size": [4.0, 3.0],
        "rotation": 13.0,
        "flipX": true,
        "flipY": false
    });
    let layer = editor.document.find_layer_mut(&id).unwrap();
    layer.mask = Some(corrected.clone().into());
    layer.metadata["maskEnabled"] = json!(true);
    layer.metadata["maskLinked"] = json!(true);
    layer.metadata["maskPlacement"] = placement.clone();

    let mut updated = editor.editable_state(&id).unwrap();
    updated
        .recipe
        .nodes
        .push(filter_node(Filter::Exposure { stops: 0.5 }));
    let updated = updated.evaluate(&AtomicBool::new(false)).unwrap();
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), updated)])
            .unwrap()
    );
    let layer = editor.document.find_layer(&id).unwrap();
    assert_eq!(layer.mask.as_deref(), Some(&corrected));
    assert_eq!(layer.metadata["maskPlacement"], placement);

    let layer = editor.document.find_layer_mut(&id).unwrap();
    layer.mask = None;
    for key in ["maskEnabled", "maskLinked", "maskPlacement"] {
        layer.metadata.as_object_mut().unwrap().remove(key);
    }
    let mut updated = editor.editable_state(&id).unwrap();
    updated
        .recipe
        .nodes
        .push(filter_node(Filter::Exposure { stops: -0.25 }));
    let updated = updated.evaluate(&AtomicBool::new(false)).unwrap();
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), updated)])
            .unwrap()
    );
    assert!(editor.document.find_layer(&id).unwrap().mask.is_none());

    let mut explicit = editor.editable_state(&id).unwrap();
    explicit.recipe.vector = Some(path.clone());
    explicit.recipe.vector_is_mask = true;
    let explicit = explicit.evaluate(&AtomicBool::new(false)).unwrap();
    assert!(editor.replace_vector_state(&id, explicit).unwrap());
    let expected = omuse::vector_path::rasterize_mask(&path, 4, 3, 0.25, || false).unwrap();
    let expected = RgbaImage::from_fn(expected.width(), expected.height(), |x, y| {
        let value = expected.get_pixel(x, y)[0];
        Rgba([value, value, value, 255])
    });
    assert_eq!(
        editor.document.find_layer(&id).unwrap().mask.as_deref(),
        Some(&expected)
    );
    assert!(editor.undo());
    assert!(editor.document.find_layer(&id).unwrap().mask.is_none());
}

#[test]
fn exact_source_survives_edit_reorder_save_undo_redo_and_reopen() {
    let mut editor = Editor::new(document_with_state(state_with_exact_source(4, 3)).0);
    let id = editor.document.layers[0].id.clone();
    editor.document.layers[0].mask =
        Some(RgbaImage::from_pixel(4, 3, Rgba([128, 128, 128, 255])).into());
    editor.document.layers[0].metadata["maskEnabled"] = json!(true);
    editor.document.layers[0].metadata["maskLinked"] = json!(true);
    editor.document.layers[0].metadata["maskPlacement"] =
        json!({"origin":[0.25,0.5],"size":[4.0,3.0],"rotation":13.0,"flipX":true,"flipY":false});
    editor.document.layers[0].offset_x = 1.25;
    editor.document.layers[0].rotation = 19.;
    let extra = Layer::paint("reorder companion", 4, 3);
    editor.document.layers.push(extra);
    assert!(editor.reorder_layer(&id, None, 1));
    let original = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap()
        .source
        .to_rgba16();

    let mut prepared = editor.editable_state(&id).unwrap();
    prepared
        .recipe
        .nodes
        .push(filter_node(Filter::Exposure { stops: 0.5 }));
    let evaluated = prepared.evaluate(&AtomicBool::new(false)).unwrap();
    assert_ne!(evaluated.result.to_rgba16(), original);
    assert_eq!(
        prepared.source.to_rgba16(),
        original,
        "editing must retain immutable source"
    );
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), evaluated)])
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), 2);
    assert_eq!(
        editor.document.find_layer(&id).unwrap().metadata["maskLinked"],
        json!(true)
    );
    assert_eq!(editor.document.find_layer(&id).unwrap().offset_x, 1.25);
    assert!(editor.undo());
    assert!(editor.document.find_layer(&id).unwrap().advanced.is_some());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .as_ref()
            .unwrap()
            .source
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
            .source
            .to_rgba16(),
        original
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Exact.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    let layer = reopened.find_layer(&id).unwrap();
    assert_eq!(
        layer.advanced.as_ref().unwrap().source.to_rgba16(),
        original
    );
    assert_eq!(
        layer.metadata["maskPlacement"],
        editor.document.find_layer(&id).unwrap().metadata["maskPlacement"]
    );
    assert_eq!(
        raster::composite(&reopened),
        raster::composite(&editor.document)
    );
}

#[test]
fn locked_parent_and_invalid_state_are_atomic_and_cancellation_is_non_mutating() {
    let mut doc = Document::new(3, 2);
    let child = doc.layers.remove(0);
    let child_id = child.id.clone();
    let mut group = Layer::group("locked parent");
    let group_id = group.id.clone();
    group.children.push(child);
    doc.layers.push(group);
    let mut editor = Editor::new(doc);
    editor.select_layer(&child_id);
    assert!(editor.set_locked(&group_id, true));
    let before_child = editor.document.find_layer(&child_id).unwrap().image.clone();
    let before_group_locked = editor.document.find_layer(&group_id).unwrap().locked;
    let before_history = editor.undo_depth();
    let state = editor.editable_state(&child_id).unwrap();
    let error = editor
        .replace_editable_states(vec![(child_id.clone(), state.clone())])
        .unwrap_err();
    assert!(error.to_string().contains("parents"));
    assert_eq!(
        editor.document.find_layer(&child_id).unwrap().image,
        before_child
    );
    assert_eq!(
        editor.document.find_layer(&group_id).unwrap().locked,
        before_group_locked
    );
    assert_eq!(editor.undo_depth(), before_history);

    editor.set_locked(&group_id, false);
    let mut invalid = state;
    invalid.result = Arc::new(TiledImage16::from_rgba16(&Rgba16Image::new(1, 1)).unwrap());
    let before_child = editor.document.find_layer(&child_id).unwrap().image.clone();
    let before_history = editor.undo_depth();
    assert!(
        editor
            .replace_editable_states(vec![(child_id.clone(), invalid)])
            .is_err()
    );
    assert_eq!(
        editor.document.find_layer(&child_id).unwrap().image,
        before_child
    );
    assert_eq!(editor.undo_depth(), before_history);

    let mut canceled = editor.editable_state(&child_id).unwrap();
    canceled
        .recipe
        .nodes
        .push(filter_node(Filter::GaussianBlur { sigma: 2. }));
    let cancel = AtomicBool::new(true);
    let source_before = canceled.source.to_rgba16();
    assert!(canceled.evaluate(&cancel).is_err());
    assert_eq!(canceled.source.to_rgba16(), source_before);
    assert_eq!(cancel.load(Ordering::Relaxed), true);
}

#[test]
fn frequency_components_can_be_composited_without_an_8_bit_source() {
    let source = source16(5, 5);
    let base = Arc::new(TiledImage16::from_rgba16(&source).unwrap());
    let mut low = state_with_exact_source(5, 5);
    low.recipe.component = Component::LowFrequency { sigma: 1.0 };
    low.result = base.clone();
    low = low.evaluate(&AtomicBool::new(false)).unwrap();
    let mut high = state_with_exact_source(5, 5);
    high.recipe.component = Component::HighFrequency { sigma: 1.0 };
    high.result = base.clone();
    high = high.evaluate(&AtomicBool::new(false)).unwrap();

    let mut document = Document::new(5, 5);
    document.layers[0].image = Some(low.proxy().unwrap().into());
    document.layers[0].advanced = Some(Arc::new(low));
    let mut high_layer = Layer::paint("high", 5, 5);
    high_layer.image = Some(high.proxy().unwrap().into());
    high_layer.advanced = Some(Arc::new(high));
    high_layer.blend_mode = "Linear Light".into();
    document.layers.push(high_layer);
    let rendered = raster::composite16(&document).unwrap();
    let mut max_error = 0u16;
    for (actual, expected) in rendered.pixels().zip(source.pixels()) {
        for c in 0..3 {
            max_error = max_error.max(actual[c].abs_diff(expected[c]));
        }
    }
    assert!(max_error <= 2, "frequency reconstruction error {max_error}");
}

#[test]
fn resize_preserves_exact_advanced_source_and_rejects_shear_atomically() {
    let (document, id) = document_with_state(state_with_exact_source(4, 3));
    let mut editor = Editor::new(document);
    let original = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap()
        .source
        .to_rgba16();

    assert!(editor.resize_image_with_options(8, 6, 144., "Nearest"));
    assert_eq!((editor.document.width, editor.document.height), (8, 6));
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        original,
        "canvas resize must retain the immutable 16-bit source"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Resized.omuse");
    document::save(&editor.document, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!((reopened.width, reopened.height), (8, 6));
    assert_eq!(
        reopened
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        original
    );

    assert!(editor.undo());
    assert_eq!((editor.document.width, editor.document.height), (4, 3));
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        original
    );
    assert!(editor.redo());
    assert_eq!((editor.document.width, editor.document.height), (8, 6));
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        original
    );

    // A rotated advanced layer cannot represent a non-uniform canvas resize
    // with its placement alone: that operation would introduce a shear. The
    // failure must leave both the document and history untouched.
    let (document, id) = document_with_state(state_with_exact_source(4, 3));
    let mut editor = Editor::new(document);
    editor.document.find_layer_mut(&id).unwrap().rotation = 17.;
    let before_dimensions = (editor.document.width, editor.document.height);
    let before_source = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap()
        .source
        .to_rgba16();
    let before_rotation = editor.document.find_layer(&id).unwrap().rotation;
    let before_history = editor.undo_depth();
    assert!(!editor.resize_image_with_options(8, 4, 144., "Nearest"));
    assert_eq!(
        (editor.document.width, editor.document.height),
        before_dimensions
    );
    assert_eq!(
        editor.document.find_layer(&id).unwrap().rotation,
        before_rotation
    );
    assert_eq!(editor.undo_depth(), before_history);
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        before_source
    );
}

#[cfg(unix)]
#[test]
fn malformed_advanced_assets_are_rejected_without_replacing_existing_project() {
    use std::{fs, io::Write, os::unix::fs::symlink};
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("images");
    fs::create_dir(&assets).unwrap();
    let state = state_with_exact_source(2, 2);
    state.save_assets(&assets, "ASSET").unwrap();
    let recipe_path = assets.join("ASSET.editable.json.z");
    let source_path = assets.join("ASSET.source16.png");
    let original_source = fs::read(&source_path).unwrap();

    fs::remove_file(&source_path).unwrap();
    symlink(assets.join("ASSET.result16.png"), &source_path).unwrap();
    assert!(LayerState::load_assets(&assets, "ASSET").is_err());
    fs::remove_file(&source_path).unwrap();
    fs::write(&source_path, &original_source).unwrap();
    fs::write(&source_path, b"not a png").unwrap();
    assert!(LayerState::load_assets(&assets, "ASSET").is_err());
    fs::write(&source_path, &original_source).unwrap();

    for corruption in 0..3 {
        let mut recipe = state.recipe.clone();
        match corruption {
            0 => recipe.version = 99,
            1 => recipe.vector_is_mask = true,
            _ => {
                recipe.vector_stroke = Some(omuse::vector_path::StrokeStyle {
                    color: [255; 4],
                    width: 2.,
                })
            }
        }
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        compressed
            .write_all(&serde_json::to_vec(&recipe).unwrap())
            .unwrap();
        fs::write(&recipe_path, compressed.finish().unwrap()).unwrap();
        assert!(LayerState::load_assets(&assets, "ASSET").is_err());
    }
    assert!(
        fs::metadata(assets.join("ASSET.result16.png"))
            .unwrap()
            .is_file()
    );

    let project = directory.path().join("Existing.omuse");
    let valid_state = state_with_exact_source(2, 2);
    let expected = valid_state.source.to_rgba16();
    let mut valid_doc = Document::new(2, 2);
    valid_doc.layers[0].image = Some(RgbaImage::from_pixel(2, 2, Rgba([20, 30, 40, 255])).into());
    valid_doc.layers[0].advanced = Some(Arc::new(valid_state));
    document::save(&valid_doc, &project).unwrap();
    let mut invalid_doc = valid_doc.clone();
    let mut invalid_state = (**invalid_doc.layers[0].advanced.as_ref().unwrap()).clone();
    invalid_state.recipe.version = 99;
    invalid_doc.layers[0].advanced = Some(Arc::new(invalid_state));
    assert!(document::save(&invalid_doc, &project).is_err());
    assert_eq!(
        document::open(&project).unwrap().layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        expected
    );

    let manifest_path = project.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["layers"][0]["rustEditableAsset"] = json!("../escape.editable.json.z");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(document::open(&project).is_err());
}
