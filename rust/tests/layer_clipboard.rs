use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode, SoftMask},
    editor::{Editor, PaintTool},
    model::{Document, Layer},
    objects::{self, LiveShapeKind, LiveShapeStyle, LiveTextStyle, ObjectPoint},
    raster,
};
use serde_json::json;
use std::sync::Arc;

fn document(width: u32, height: u32, layers: Vec<Layer>) -> Document {
    Document {
        width,
        height,
        name: "Clipboard test".into(),
        background: [0; 4],
        layers,
        metadata: json!({}),
    }
}

fn paint(name: &str, width: u32, height: u32, color: [u8; 4]) -> Layer {
    let mut layer = Layer::paint(name, width, height);
    layer.image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
    layer
}

fn layer_ids(layers: &[Layer]) -> Vec<String> {
    layers.iter().map(|layer| layer.id.clone()).collect()
}

#[test]
fn copied_folder_preserves_nested_masks_locks_and_preview_after_source_changes() {
    let mut base = paint("Mask source", 8, 6, [255; 4]);
    base.visible = false;
    for y in 0..6 {
        for x in 4..8 {
            base.image.as_mut().unwrap().put_pixel(x, y, Rgba([0; 4]));
        }
    }
    let base_id = base.id.clone();
    let mut color = paint("Clipped color", 8, 6, [210, 60, 20, 255]);
    color.locked = true;
    color.mask = Some(RgbaImage::from_pixel(8, 6, Rgba([200, 200, 200, 255])).into());
    color.metadata["maskSourceID"] = json!(base.id);
    color.metadata["applicationNote"] = json!({"tags": ["retain", "editable"]});
    let child_id = color.id.clone();
    let mut nested = Layer::group("Nested");
    nested.children = vec![base, color];
    let mut folder = Layer::group("Locked folder");
    folder.locked = true;
    folder.opacity = 0.75;
    folder.mask = Some(RgbaImage::from_pixel(8, 6, Rgba([128, 128, 128, 255])).into());
    folder.children.push(nested);
    let folder_id = folder.id.clone();
    let expected = raster::composite(&document(8, 6, vec![folder.clone()]));
    assert!(expected.get_pixel(1, 1)[3] > 0 && expected.get_pixel(1, 1)[3] < 255);
    assert_eq!(expected.get_pixel(7, 1)[3], 0);

    let mut source = Editor::new(document(
        8,
        6,
        vec![paint("Excluded", 8, 6, [0, 255, 0, 255]), folder],
    ));
    source.document.background = [255, 0, 255, 255];
    let clipboard = source
        .copy_layers(&[child_id, folder_id.clone(), folder_id])
        .unwrap();
    assert_eq!(clipboard.root_count(), 1);
    assert_eq!(clipboard.layer_count(), 4);
    assert_eq!(source.undo_depth(), 0);
    assert!(!source.is_dirty());
    assert_eq!(clipboard.preview().unwrap(), expected);

    source
        .document
        .find_layer_mut(&base_id)
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .fill(0);
    assert_eq!(clipboard.preview().unwrap(), expected);
    drop(source);

    let mut target = Editor::new(document(8, 6, vec![]));
    let pasted = target.paste_layers(&clipboard).unwrap();
    let folder = target.document.find_layer(&pasted[0]).unwrap();
    assert!(folder.locked);
    assert_eq!(folder.opacity, 0.75);
    let children = &folder.children[0].children;
    assert!(!children[0].visible);
    assert!(children[1].locked);
    assert_eq!(children[1].metadata["maskSourceID"], children[0].id);
    assert_eq!(
        children[1].metadata["applicationNote"]["tags"][1],
        "editable"
    );
    assert!(target.document.find_layer(&base_id).is_none());
    assert_eq!(raster::composite(&target.document), expected);
    assert_eq!(target.undo_depth(), 1);
    assert!(target.undo());
    assert!(target.document.layers.is_empty());
    assert!(target.redo());
    assert_eq!(raster::composite(&target.document), expected);
}

#[test]
fn paste_keeps_source_order_and_inserts_above_active_top_level_as_one_undo() {
    let a = paint("A", 3, 2, [20, 30, 40, 255]);
    let mut b = paint("B", 2, 1, [50, 60, 70, 255]);
    b.offset_x = 3.25;
    b.offset_y = 1.5;
    b.rotation = 25.0;
    b.scale_x = -1.5;
    b.scale_y = 2.0;
    b.opacity = 0.4;
    b.blend_mode = "Multiply".into();
    let originals = vec![a.clone(), b.clone()];
    let source = Editor::new(document(12, 10, originals.clone()));
    let clipboard = source
        .copy_layers(&[b.id.clone(), a.id.clone(), b.id.clone()])
        .unwrap();

    let child = paint("Active child", 12, 10, [0; 4]);
    let active_id = child.id.clone();
    let mut folder = Layer::group("Destination folder");
    folder.children.push(child);
    let top = paint("Existing top", 12, 10, [0; 4]);
    let top_id = top.id.clone();
    let mut destination = Editor::new(document(12, 10, vec![folder, top]));
    assert!(destination.select_layer(&active_id));
    destination.select_rectangle(2., 3., 2., 1.);
    let selection = destination.selection.clone();
    let before = layer_ids(&destination.document.layers);
    let before_render = raster::composite(&destination.document);
    let pasted = destination.paste_layers(&clipboard).unwrap();
    assert_eq!(destination.undo_depth(), 1);
    assert_eq!(destination.selection, selection);
    assert_eq!(destination.active_layer, pasted[1]);
    assert_eq!(destination.document.layers[3].id, top_id);
    assert_eq!(destination.document.layers[0].children.len(), 1);
    for (index, original) in originals.iter().enumerate() {
        let copy = &destination.document.layers[index + 1];
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.id, pasted[index]);
        assert_eq!(copy.name, original.name);
        assert_eq!(copy.offset_x, original.offset_x);
        assert_eq!(copy.offset_y, original.offset_y);
        assert_eq!(copy.rotation, original.rotation);
        assert_eq!(copy.scale_x, original.scale_x);
        assert_eq!(copy.scale_y, original.scale_y);
        assert_eq!(copy.opacity, original.opacity);
        assert_eq!(copy.blend_mode, original.blend_mode);
        assert!(
            copy.image
                .as_ref()
                .unwrap()
                .shares_pixels_with(original.image.as_ref().unwrap())
        );
    }
    assert!(destination.undo());
    assert_eq!(layer_ids(&destination.document.layers), before);
    assert_eq!(destination.active_layer, active_id);
    assert_eq!(destination.selection, selection);
    assert_eq!(raster::composite(&destination.document), before_render);
    assert!(!destination.is_dirty());
    assert!(destination.redo());
    assert_eq!(destination.document.layers[1].id, pasted[0]);
    assert_eq!(destination.document.layers[2].id, pasted[1]);
}

#[test]
fn clipboard_preserves_live_text_shapes_advanced_pixels_and_independent_mutation() {
    let style = LiveTextStyle {
        content: "Omuse".into(),
        font_size: 12.0,
        ..Default::default()
    };
    let text =
        objects::live_text_layer("Title", ObjectPoint { x: 3., y: 2. }, style.clone()).unwrap();
    let shape_style = LiveShapeStyle {
        kind: LiveShapeKind::Rectangle,
        red: 0.9,
        green: 0.3,
        blue: 0.1,
        corner_radius: 2.,
        line_width: None,
        start: None,
        end: None,
    };
    let shape = objects::live_shape_layer(
        "Shape",
        ObjectPoint { x: 8., y: 20. },
        20,
        16,
        shape_style.clone(),
    )
    .unwrap();
    let mut precision = paint("Editable source", 4, 4, [123, 70, 50, 255]);
    let mut state = LayerState::from_image(precision.image.as_ref().unwrap(), "Original").unwrap();
    let exact = omuse::precision::Rgba16([12_345, 23_456, 34_567, 65_535]);
    Arc::make_mut(&mut state.result)
        .set_pixel(0, 0, exact)
        .unwrap();
    precision.image = Some(state.proxy().unwrap().into());
    let state = Arc::new(state);
    precision.advanced = Some(state.clone());
    precision.metadata["provenance"] = json!({"retain": true});
    let mut source = Editor::new(document(128, 96, vec![text, shape, precision]));
    let clipboard = source
        .copy_layers(&layer_ids(&source.document.layers))
        .unwrap();
    let expected = clipboard.preview().unwrap();
    let mut destination = Editor::new(document(128, 96, vec![]));
    let pasted = destination.paste_layers(&clipboard).unwrap();
    assert_eq!(
        objects::live_text(&destination.document.layers[0]).unwrap(),
        Some(style)
    );
    assert_eq!(
        objects::live_shape(&destination.document.layers[1]).unwrap(),
        Some(shape_style)
    );
    assert!(Arc::ptr_eq(
        destination.document.layers[2].advanced.as_ref().unwrap(),
        &state
    ));
    assert_eq!(
        destination.document.layers[2]
            .advanced
            .as_ref()
            .unwrap()
            .result
            .get_pixel(0, 0),
        exact
    );
    assert_eq!(
        destination.document.layers[2].metadata["provenance"]["retain"],
        true
    );
    assert_eq!(raster::composite(&destination.document), expected);

    let precision_id = source.document.layers[2].id.clone();
    let source_image = source.document.layers[2].image.as_ref().unwrap().clone();
    destination
        .document
        .find_layer_mut(&pasted[2])
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .put_pixel(0, 0, Rgba([1, 2, 3, 4]));
    assert_eq!(
        source.document.layers[2].image.as_ref().unwrap(),
        &source_image
    );
    let edited = Arc::make_mut(destination.document.layers[2].advanced.as_mut().unwrap());
    Arc::make_mut(&mut edited.result)
        .set_pixel(0, 0, omuse::precision::Rgba16([0; 4]))
        .unwrap();
    assert_eq!(state.result.get_pixel(0, 0), exact);
    source
        .document
        .find_layer_mut(&precision_id)
        .unwrap()
        .image
        .as_mut()
        .unwrap()
        .put_pixel(1, 1, Rgba([9; 4]));
    assert_eq!(clipboard.preview().unwrap(), expected);
    let again = destination.paste_layers(&clipboard.clone()).unwrap();
    assert_ne!(pasted, again);
    assert_eq!(
        destination
            .document
            .find_layer(&again[2])
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .result
            .get_pixel(0, 0),
        exact
    );
    assert_eq!(
        destination
            .document
            .find_layer(&again[2])
            .unwrap()
            .image
            .as_ref()
            .unwrap(),
        &source_image
    );
}

#[test]
fn cross_document_live_links_never_reuse_a_destination_identity() {
    let base = paint("Base", 4, 3, [100, 110, 120, 255]);
    let mut clipped = paint("Clipped", 4, 3, [150, 70, 30, 255]);
    clipped.metadata["maskSourceID"] = json!(base.id);
    let source = Editor::new(document(4, 3, vec![base.clone(), clipped.clone()]));
    let clipboard = source.copy_layers(&[clipped.id, base.id.clone()]).unwrap();
    let mut unrelated = paint("Same old identity, unrelated pixels", 4, 3, [0; 4]);
    unrelated.id = base.id.clone();
    let mut destination = Editor::new(document(4, 3, vec![unrelated]));
    let pasted = destination.paste_layers(&clipboard).unwrap();
    assert_ne!(pasted[0], base.id);
    assert_eq!(
        destination.document.layers[2].metadata["maskSourceID"],
        pasted[0]
    );
    assert_eq!(
        raster::composite(&destination.document).get_pixel(1, 1).0,
        [150, 70, 30, 255]
    );
    assert!(destination.select_layer(&pasted[0]));
    assert!(destination.fill_selection([0; 4]));
    assert_eq!(
        raster::composite(&destination.document).get_pixel(1, 1)[3],
        0
    );
    assert_eq!(clipboard.preview().unwrap().get_pixel(1, 1)[3], 255);
}

#[test]
fn missing_external_or_invalid_dependencies_reject_without_history_or_dirty_changes() {
    let base = paint("External mask", 4, 3, [255; 4]);
    let mut clipped = paint("Dependent", 4, 3, [20, 30, 40, 255]);
    clipped.metadata["maskSourceID"] = json!(base.id);
    let clipped_id = clipped.id.clone();
    let base_id = base.id.clone();
    let mut source = Editor::new(document(4, 3, vec![base, clipped]));
    let before = raster::composite(&source.document);
    let error = source
        .copy_layers(&[clipped_id.clone()])
        .unwrap_err()
        .to_string();
    assert!(error.contains("live mask source"), "{error}");
    assert!(source.copy_layers(&[]).is_err());
    assert!(
        source
            .copy_layers(&[clipped_id.clone(), "missing".into()])
            .is_err()
    );
    assert_eq!(source.undo_depth(), 0);
    assert!(!source.is_dirty());
    assert_eq!(raster::composite(&source.document), before);
    source.document.layers[0].metadata["maskSourceID"] = json!(clipped_id);
    assert!(
        source
            .copy_layers(&[base_id, clipped_id])
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
}

#[test]
fn floating_selections_reject_copy_and_paste_without_committing_them() {
    let mut editor = Editor::new(document(
        8,
        8,
        vec![paint("Source", 8, 8, [50, 80, 120, 255])],
    ));
    let clipboard = editor.copy_layers(&[editor.active_layer.clone()]).unwrap();
    editor.select_rectangle(2., 2., 2., 2.);
    let floating = editor.begin_floating_selection().unwrap().unwrap();
    let before = raster::composite(&editor.document);
    let ids = layer_ids(&editor.document.layers);
    assert!(editor.copy_layers(&[floating.clone()]).is_err());
    assert!(editor.paste_layers(&clipboard).is_err());
    assert_eq!(editor.floating_selection_layer(), Some(floating.as_str()));
    assert_eq!(layer_ids(&editor.document.layers), ids);
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(raster::composite(&editor.document), before);
    assert!(editor.cancel_floating_selection());
    assert_eq!(
        raster::composite(&editor.document),
        clipboard.preview().unwrap()
    );
}

#[test]
fn paste_checks_destination_pixel_and_layer_budgets_before_committing_a_stroke() {
    let source = Editor::new(document(1, 1, vec![paint("Copy", 1, 1, [10, 20, 30, 255])]));
    let clipboard = source.copy_layers(&[source.active_layer.clone()]).unwrap();
    let shared = paint("Large", 1000, 1000, [0; 4]).image.unwrap();
    let layers = (0..100)
        .map(|_| {
            let mut layer = Layer::paint("Shared large raster", 1, 1);
            layer.image = Some(shared.clone());
            layer
        })
        .collect();
    let mut destination = Editor::new(document(1, 1, layers));
    destination.brush.size = 1.;
    assert!(destination.begin_stroke(0.5, 0.5, 1., PaintTool::Pencil));
    let active = destination.active_layer.clone();
    let error = destination
        .paste_layers(&clipboard)
        .unwrap_err()
        .to_string();
    assert!(error.contains("pixel limit"), "{error}");
    assert_eq!(destination.document.layers.len(), 100);
    assert_eq!(destination.active_layer, active);
    assert_eq!(
        destination.undo_depth(),
        0,
        "rejected paste must leave the pending stroke alone"
    );
    destination.cancel_stroke();
    assert!(!destination.is_dirty());

    let layers = (0..omuse::model::MAX_LAYERS)
        .map(|_| Layer::group("Folder"))
        .collect();
    let mut full = Editor::new(document(1, 1, layers));
    let error = full.paste_layers(&clipboard).unwrap_err().to_string();
    assert!(error.contains("layer limit"), "{error}");
    assert_eq!(full.document.layers.len(), omuse::model::MAX_LAYERS);
    assert_eq!(full.undo_depth(), 0);
    assert!(!full.is_dirty());
}

#[test]
fn destination_mask_surface_limit_is_checked_transactionally() {
    let base = paint("Base", 1, 1, [255; 4]);
    let mut layers = vec![base.clone()];
    for _ in 0..2 {
        let mut dependent = paint("Linked", 1, 1, [255; 4]);
        dependent.metadata["maskSourceID"] = json!(base.id);
        layers.push(dependent);
    }
    let source = Editor::new(document(1, 1, layers));
    let clipboard = source
        .copy_layers(&layer_ids(&source.document.layers))
        .unwrap();
    // Canvas dimensions, not the tiny copied assets, determine mask surfaces.
    let mut destination = Editor::new(document(10_000, 10_000, vec![]));
    let error = destination
        .paste_layers(&clipboard)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Live mask dependency surfaces"), "{error}");
    assert!(destination.document.layers.is_empty());
    assert_eq!(destination.undo_depth(), 0);
    assert!(!destination.is_dirty());
}

#[test]
fn payload_limit_accounts_for_retained_raster_metadata_and_advanced_capacity() {
    const OVER_LIMIT: usize = 256 * 1024 * 1024 + 1;
    // Reserve rather than fill: test retained allocation accounting without
    // writing hundreds of megabytes of test pixels or JSON into physical RAM.
    let mut bytes = Vec::with_capacity(OVER_LIMIT);
    bytes.extend_from_slice(&[1, 2, 3, 255]);
    let mut layer = Layer::paint("Reserved raster", 1, 1);
    layer.image = Some(RgbaImage::from_raw(1, 1, bytes).unwrap().into());
    let source = Editor::new(document(1, 1, vec![layer]));
    assert!(
        source
            .copy_layers(&[source.active_layer.clone()])
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    drop(source);

    let mut value = String::with_capacity(OVER_LIMIT);
    value.push('x');
    let mut layer = paint("Reserved metadata", 1, 1, [255; 4]);
    layer.metadata["retained"] = serde_json::Value::String(value);
    let source = Editor::new(document(1, 1, vec![layer]));
    assert!(
        source
            .copy_layers(&[source.active_layer.clone()])
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    drop(source);

    let mut layer = paint("Reserved editable mask", 1, 1, [255; 4]);
    let mut state = LayerState::from_image(layer.image.as_ref().unwrap(), "Original").unwrap();
    let mut coverage = Vec::with_capacity(OVER_LIMIT);
    coverage.push(255);
    state.recipe.nodes.push(FilterNode {
        id: "invert".into(),
        name: "Invert".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::Filter(omuse::filters::Filter::Invert),
        soft_mask: Some(SoftMask::new(1, 1, coverage).unwrap()),
    });
    layer.advanced = Some(Arc::new(state));
    let source = Editor::new(document(1, 1, vec![layer]));
    assert!(
        source
            .copy_layers(&[source.active_layer.clone()])
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    assert_eq!(source.undo_depth(), 0);
    assert!(!source.is_dirty());
}

#[test]
fn repeated_shared_rasters_are_charged_once_and_remain_shared_after_paste() {
    let image = paint("Source", 256, 256, [255; 4]).image.unwrap();
    let layers = (0..8)
        .map(|index| {
            let mut layer = Layer::paint(format!("Copy {index}"), 1, 1);
            layer.image = Some(image.clone());
            layer
        })
        .collect();
    let source = Editor::new(document(256, 256, layers));
    let clipboard = source
        .copy_layers(&layer_ids(&source.document.layers))
        .unwrap();
    assert!(clipboard.retained_bytes() > image.as_raw().len());
    assert!(clipboard.retained_bytes() < image.as_raw().len() * 2);
    let mut destination = Editor::new(document(256, 256, vec![]));
    destination.paste_layers(&clipboard).unwrap();
    assert!(
        destination.document.layers.iter().all(|layer| layer
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(&image))
    );
}

#[test]
fn excessive_tree_or_metadata_depth_and_duplicate_ids_are_rejected_before_clone() {
    let mut root = Layer::group("Deep");
    for _ in 0..64 {
        let mut parent = Layer::group("Parent");
        parent.children.push(root);
        root = parent;
    }
    let source = Editor::new(document(1, 1, vec![root]));
    assert!(
        source
            .copy_layers(&[source.document.layers[0].id.clone()])
            .unwrap_err()
            .to_string()
            .contains("64 levels")
    );

    let mut metadata = json!(true);
    for _ in 0..130 {
        metadata = json!([metadata]);
    }
    let mut layer = paint("Deep metadata", 1, 1, [255; 4]);
    layer.metadata["nested"] = metadata;
    let source = Editor::new(document(1, 1, vec![layer]));
    assert!(
        source
            .copy_layers(&[source.active_layer.clone()])
            .unwrap_err()
            .to_string()
            .contains("128 levels")
    );

    let layer = paint("Duplicate identity", 1, 1, [255; 4]);
    let source = Editor::new(document(1, 1, vec![layer.clone(), layer]));
    assert!(
        source
            .copy_layers(&[source.active_layer.clone()])
            .unwrap_err()
            .to_string()
            .contains("duplicated")
    );
}

#[test]
fn preview_refuses_oversized_interoperability_canvas_but_internal_layers_remain_exact() {
    let layer = paint("Tiny asset on large canvas", 1, 1, [8, 16, 32, 255]);
    let source = Editor::new(document(5000, 4000, vec![layer.clone()]));
    let clipboard = source.copy_layers(&[source.active_layer.clone()]).unwrap();
    assert!(
        clipboard
            .preview()
            .unwrap_err()
            .to_string()
            .contains("16 megapixel")
    );
    let mut destination = Editor::new(document(2, 2, vec![]));
    destination.paste_layers(&clipboard).unwrap();
    assert_eq!(destination.document.layers[0].image, layer.image);
    assert_eq!(
        raster::composite(&destination.document).get_pixel(0, 0).0,
        [8, 16, 32, 255]
    );
}
