//! Document-level tracing checks. Small deterministic pixel fixtures keep the
//! renderer, persistence and history checks independent of photographic claims.
use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    document,
    editor::{Editor, LayerPlacement, PaintTool},
    image_trace::{self, TraceMode, TraceOptions},
    image_trace_layer::{self, PreparedTrace},
    model::{Document, Layer},
    precision::Rgba16Image,
    raster,
};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

fn pixels() -> RgbaImage {
    RgbaImage::from_fn(32, 24, |x, y| {
        if (4..28).contains(&x) && (4..20).contains(&y) {
            if (12..20).contains(&x) && (10..14).contains(&y) {
                Rgba([177, 19, 231, 0])
            } else if x >= 20 && y < 12 {
                Rgba([216, 107, 67, 255])
            } else {
                Rgba([42, 113, 179, 255])
            }
        } else {
            Rgba([177, 19, 231, 0])
        }
    })
}

fn options() -> TraceOptions {
    TraceOptions {
        mode: TraceMode::Color,
        colors: 4,
        detail: 0.,
        smoothing: 0.,
        corner_preservation: 1.,
        speckle_area: 0,
        max_dimension: 32,
        max_points: 4_096,
        ..TraceOptions::default()
    }
}

fn editor() -> (Editor, String) {
    let mut document = Document::new(32, 24);
    document.layers[0].name = "Retained source".into();
    document.layers[0].image = Some(pixels().into());
    let source = document.layers[0].id.clone();
    (Editor::new(document), source)
}

fn prepare_for(
    editor: &Editor,
    source: &str,
    target: Option<&str>,
    options: &TraceOptions,
    cancel: &AtomicBool,
) -> anyhow::Result<PreparedTrace> {
    let pixels = image_trace_layer::source(&editor.document, source)?;
    let trace = image_trace::trace(&pixels, options, &AtomicBool::new(false))?;
    image_trace_layer::prepare(
        &editor.document,
        source,
        target,
        trace,
        options,
        editor.revision(),
        editor.instance_id(),
        cancel,
    )
}

fn layer_state(layer: &Layer) -> Value {
    json!({
        "id": layer.id, "name": layer.name, "visible": layer.visible,
        "locked": layer.locked, "opacity": layer.opacity, "blend": layer.blend_mode,
        "placement": [layer.offset_x, layer.offset_y, layer.rotation, layer.scale_x, layer.scale_y],
        "image": layer.image.as_ref().map(|p| (p.dimensions(), p.as_raw())),
        "mask": layer.mask.as_ref().map(|p| (p.dimensions(), p.as_raw())),
        "scene": layer.vector_scene.as_deref(), "metadata": layer.metadata,
        "advanced": layer.advanced.as_ref().map(|state| json!({
            "recipe": state.recipe,
            "source": state.source.to_rgba16().as_raw(),
            "result": state.result.to_rgba16().as_raw(),
        })),
        "children": layer.children.iter().map(layer_state).collect::<Vec<_>>(),
    })
}

fn document_state(document: &Document) -> Value {
    json!({
        "dimensions": [document.width, document.height], "name": document.name,
        "background": document.background, "metadata": document.metadata,
        "layers": document.layers.iter().map(layer_state).collect::<Vec<_>>(),
    })
}

fn editor_state(editor: &Editor) -> Value {
    json!({
        "document": document_state(&editor.document), "active": editor.active_layer,
        "revision": editor.revision(), "selectionRevision": editor.selection_revision(),
        "undo": editor.undo_depth(), "redo": editor.redo_depth(), "dirty": editor.is_dirty(),
    })
}

fn nested_editor() -> (Editor, String, String) {
    let mut document = Document::new(80, 64);
    document.layers[0].name = "Surrounding photograph".into();
    document.layers[0].image =
        Some(RgbaImage::from_pixel(80, 64, Rgba([210, 225, 240, 255])).into());
    let mut group = Layer::group("Masked folder");
    group.opacity = 0.85;
    group.mask = Some(
        RgbaImage::from_fn(80, 64, |x, _| {
            let value = if x < 8 { 100 } else { 235 };
            Rgba([value, value, value, 255])
        })
        .into(),
    );
    let mut below = Layer::paint("Sibling below", 8, 8);
    below.offset_x = 3.;
    below.offset_y = 4.;
    below.image = Some(RgbaImage::from_pixel(8, 8, Rgba([180, 110, 50, 255])).into());
    let mut source = Layer::paint("Placed source", 32, 24);
    source.image = Some(pixels().into());
    source.offset_x = 22.;
    source.offset_y = 18.;
    source.rotation = 17.;
    source.scale_x = -1.25;
    source.scale_y = 0.9;
    source.opacity = 0.72;
    source.blend_mode = "Multiply".into();
    source.mask = Some(
        RgbaImage::from_fn(32, 24, |x, _| {
            let value = if x < 9 { 100 } else { 255 };
            Rgba([value, value, value, 255])
        })
        .into(),
    );
    source.metadata = json!({
        "maskEnabled": true, "maskLinked": false, "maskOutsideCoverage": 0,
        "maskPlacement": {"origin": [20., 14.], "size": [40., 24.],
            "rotation": -11., "flipX": true, "flipY": false, "sampling": "Smooth"},
        "transform": {"sampling": "High quality"},
        "effects": {"shadow": {"distance": 2.}},
    });
    let mut above = Layer::paint("Sibling above", 6, 6);
    above.offset_x = 60.;
    above.offset_y = 41.;
    above.image = Some(RgbaImage::from_pixel(6, 6, Rgba([220, 80, 60, 220])).into());
    let source_id = source.id.clone();
    let group_id = group.id.clone();
    group.children = vec![below, source, above];
    document.layers.push(group);
    assert!(raster::validate(&document).is_empty());
    let mut editor = Editor::new(document);
    assert!(editor.select_layer(&source_id));
    (editor, source_id, group_id)
}

#[test]
fn initial_trace_preserves_source_and_commits_exact_preview_as_one_undo_step() {
    let (mut editor, source) = editor();
    let before_document = document_state(&editor.document);
    let before_pixels = raster::composite(&editor.document);
    let shared_source = image_trace_layer::source(&editor.document, &source).unwrap();
    let before_editor = editor_state(&editor);
    let options = options();
    let prepared = prepare_for(&editor, &source, None, &options, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        editor_state(&editor),
        before_editor,
        "Preparing a trace mutated the live editor"
    );
    assert_eq!(prepared.source_preview, before_pixels);
    assert_eq!(
        prepared.preview, before_pixels,
        "Unsmoothed flat tracing moved source boundaries or colours"
    );
    let expected_preview = prepared.preview.clone();
    let expected_id = prepared.layer_id.clone();
    assert_eq!(editor.apply_image_trace(prepared).unwrap(), expected_id);
    assert_eq!(editor.undo_depth(), 1);
    assert_eq!(editor.active_layer, expected_id);
    let original = editor.document.find_layer(&source).unwrap();
    assert!(!original.visible);
    assert!(
        original
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(&shared_source)
    );
    assert_eq!(original.image.as_deref(), Some(&pixels()));
    let traced = editor.document.find_layer(&expected_id).unwrap();
    assert!(traced.visible && traced.vector_scene.is_some());
    assert_eq!(
        image_trace_layer::retained_settings(traced),
        Some((source.clone(), options))
    );
    assert_eq!(raster::composite(&editor.document), expected_preview);
    let committed = document_state(&editor.document);
    assert!(editor.undo());
    assert_eq!(document_state(&editor.document), before_document);
    assert_eq!(editor.active_layer, source);
    assert!(editor.redo());
    assert_eq!(document_state(&editor.document), committed);
    assert_eq!(raster::composite(&editor.document), expected_preview);
}

#[test]
fn bounded_analysis_keeps_nested_placement_masks_effects_and_original_extent() {
    let (mut editor, source, folder) = nested_editor();
    let original = editor.document.find_layer(&source).unwrap().clone();
    let group = editor.document.find_layer(&folder).unwrap().clone();
    let original_preview = raster::composite(&editor.document);
    let mut options = options();
    options.max_dimension = 16;
    let prepared = prepare_for(&editor, &source, None, &options, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        (prepared.stats.working_width, prepared.stats.working_height),
        (16, 12)
    );
    assert_eq!(prepared.source_preview, original_preview);
    let preview = prepared.preview.clone();
    let id = editor.apply_image_trace(prepared).unwrap();
    let parent = editor.document.find_layer(&folder).unwrap();
    assert_eq!(
        parent
            .children
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            group.children[0].id.as_str(),
            source.as_str(),
            id.as_str(),
            group.children[2].id.as_str()
        ]
    );
    assert_eq!(parent.opacity, group.opacity);
    assert_eq!(parent.mask, group.mask);
    assert_eq!(parent.metadata, group.metadata);
    let traced = editor.document.find_layer(&id).unwrap();
    assert_eq!(
        (
            traced.offset_x,
            traced.offset_y,
            traced.rotation,
            traced.scale_x,
            traced.scale_y
        ),
        (
            original.offset_x,
            original.offset_y,
            original.rotation,
            original.scale_x,
            original.scale_y
        )
    );
    assert_eq!(
        (traced.opacity, &traced.blend_mode),
        (original.opacity, &original.blend_mode)
    );
    assert!(
        traced
            .mask
            .as_ref()
            .unwrap()
            .shares_pixels_with(original.mask.as_ref().unwrap())
    );
    for key in [
        "maskEnabled",
        "maskLinked",
        "maskPlacement",
        "maskOutsideCoverage",
        "effects",
        "transform",
    ] {
        assert_eq!(
            traced.metadata.get(key),
            original.metadata.get(key),
            "Trace changed {key}"
        );
    }
    assert_eq!(traced.image.as_ref().unwrap().dimensions(), (32, 24));
    let scene = traced.vector_scene.as_ref().unwrap();
    assert_eq!((scene.width, scene.height), (32, 24));
    assert!(
        scene
            .objects
            .iter()
            .all(|o| o.transform == [2., 0., 0., 2., 0., 0.])
    );
    assert_eq!(
        traced.image.as_ref().unwrap().get_pixel(5, 5).0,
        [42, 113, 179, 255]
    );
    assert_eq!(
        traced.image.as_ref().unwrap().get_pixel(15, 11)[3],
        0,
        "Scaled trace lost its source-space hole"
    );
    assert_eq!(raster::composite(&editor.document), preview);
    assert_eq!(
        preview, original_preview,
        "Bounded tracing changed an aligned flat fixture's nested composite"
    );
}

#[test]
fn retrace_replaces_geometry_in_place_and_preserves_later_target_styling() {
    let (mut editor, source, _) = nested_editor();
    let prepared =
        prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false)).unwrap();
    let id = editor.apply_image_trace(prepared).unwrap();
    assert!(editor.rename_layer(&id, "Retouched vector"));
    assert!(editor.set_opacity(&id, 0.43));
    assert!(editor.set_blend_mode(&id, "Screen"));
    assert!(editor.set_layer_placement(
        &id,
        LayerPlacement {
            x: 11.,
            y: 15.,
            width: 44.,
            height: 30.,
            rotation: -9.,
            flip_x: false,
            flip_y: true,
        }
    ));
    assert!(
        editor
            .set_layer_effects(&id, json!({"shadow": {"distance": 4.}}))
            .unwrap()
    );
    assert!(
        editor.set_locked(&source, true),
        "Read-only original may be locked for retracing"
    );
    let before = document_state(&editor.document);
    let old_target = editor.document.find_layer(&id).unwrap().clone();
    let old_source = layer_state(editor.document.find_layer(&source).unwrap());
    let depth = editor.undo_depth();
    let mut new_options = options();
    new_options.mode = TraceMode::Monochrome;
    new_options.monochrome_threshold = 110;
    let prepared = prepare_for(
        &editor,
        &source,
        Some(&id),
        &new_options,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut comparison = editor.document.clone();
    comparison.find_layer_mut(&source).unwrap().visible = true;
    comparison.find_layer_mut(&id).unwrap().visible = false;
    assert_eq!(prepared.source_preview, raster::composite(&comparison));
    let preview = prepared.preview.clone();
    assert_eq!(editor.apply_image_trace(prepared).unwrap(), id);
    assert_eq!(editor.undo_depth(), depth + 1);
    let target = editor.document.find_layer(&id).unwrap();
    assert_eq!(target.name, old_target.name);
    assert_eq!(
        (
            target.offset_x,
            target.offset_y,
            target.rotation,
            target.scale_x,
            target.scale_y
        ),
        (
            old_target.offset_x,
            old_target.offset_y,
            old_target.rotation,
            old_target.scale_x,
            old_target.scale_y
        )
    );
    assert_eq!(
        (target.opacity, &target.blend_mode),
        (old_target.opacity, &old_target.blend_mode)
    );
    assert_eq!(target.mask, old_target.mask);
    for key in [
        "maskPlacement",
        "maskLinked",
        "maskOutsideCoverage",
        "effects",
        "transform",
    ] {
        assert_eq!(target.metadata.get(key), old_target.metadata.get(key));
    }
    assert_ne!(target.vector_scene, old_target.vector_scene);
    assert_eq!(
        image_trace_layer::retained_settings(target),
        Some((source.clone(), new_options))
    );
    assert_eq!(
        layer_state(editor.document.find_layer(&source).unwrap()),
        old_source
    );
    assert_eq!(raster::composite(&editor.document), preview);
    assert!(editor.undo());
    assert_eq!(document_state(&editor.document), before);
    assert!(editor.redo());
    assert_eq!(raster::composite(&editor.document), preview);
}

#[test]
fn save_reopen_retains_source_precision_scene_and_regeneratable_trace_settings() {
    let (mut editor, source) = editor();
    let input = pixels();
    let precise = Rgba16Image::from_fn(32, 24, |x, y| {
        let pixel = input.get_pixel(x, y).0;
        Rgba([
            u16::from(pixel[0]) * 257 + 17,
            u16::from(pixel[1]) * 257,
            u16::from(pixel[2]) * 257,
            u16::from(pixel[3]) * 257,
        ])
    });
    let retained = Arc::new(LayerState::from_rgba16(&precise, "Original precision").unwrap());
    let original = editor.document.find_layer_mut(&source).unwrap();
    original.image = Some(retained.proxy().unwrap().into());
    original.advanced = Some(retained.clone());
    let settings = options();
    let prepared = prepare_for(&editor, &source, None, &settings, &AtomicBool::new(false)).unwrap();
    let expected = prepared.preview.clone();
    let id = editor.apply_image_trace(prepared).unwrap();
    assert!(Arc::ptr_eq(
        editor
            .document
            .find_layer(&source)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap(),
        &retained
    ));
    assert!(editor.document.find_layer(&id).unwrap().advanced.is_none());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trace.omuse");
    document::save(&editor.document, &path).unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 11);
    let reopened = document::open(&path).unwrap();
    assert_eq!(raster::composite(&reopened), expected);
    let original = reopened.find_layer(&source).unwrap();
    assert!(!original.visible);
    assert_eq!(
        original.advanced.as_ref().unwrap().source.to_rgba16(),
        precise
    );
    let trace = reopened.find_layer(&id).unwrap();
    assert_eq!(
        trace.vector_scene,
        editor.document.find_layer(&id).unwrap().vector_scene
    );
    assert_eq!(trace.image, editor.document.find_layer(&id).unwrap().image);
    let (retained_id, retained_options) = image_trace_layer::retained_settings(trace).unwrap();
    assert_eq!((&retained_id, &retained_options), (&source, &settings));
    let reopened_editor = Editor::new(reopened);
    let regenerated = prepare_for(
        &reopened_editor,
        &retained_id,
        Some(&id),
        &retained_options,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(regenerated.preview, expected);
}

#[test]
fn stale_revision_is_rejected_without_touching_newer_document_or_history() {
    let (mut editor, source) = editor();
    let prepared =
        prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false)).unwrap();
    assert!(editor.rename_layer(&source, "Changed after preparation"));
    let before = editor_state(&editor);
    assert!(
        editor
            .apply_image_trace(prepared)
            .unwrap_err()
            .to_string()
            .contains("changed")
    );
    assert_eq!(editor_state(&editor), before);
}

#[test]
fn matching_revision_in_another_editor_instance_does_not_accept_prepared_trace() {
    let (origin, source) = editor();
    let prepared =
        prepare_for(&origin, &source, None, &options(), &AtomicBool::new(false)).unwrap();
    let mut replacement = Editor::new(origin.document.clone());
    assert_eq!(replacement.revision(), origin.revision());
    assert_ne!(replacement.instance_id(), origin.instance_id());
    let before = editor_state(&replacement);
    assert!(replacement.apply_image_trace(prepared).is_err());
    assert_eq!(editor_state(&replacement), before);
}

#[test]
fn cancelled_preparation_and_locked_initial_sources_or_ancestors_are_inert() {
    let (editor, source) = editor();
    let before = editor_state(&editor);
    let error = prepare_for(&editor, &source, None, &options(), &AtomicBool::new(true))
        .err()
        .unwrap();
    assert!(error.to_string().to_lowercase().contains("cancel"));
    assert_eq!(editor_state(&editor), before);
    for lock_parent in [false, true] {
        let (mut editor, source, parent) = nested_editor();
        assert!(editor.set_locked(if lock_parent { &parent } else { &source }, true));
        let before = editor_state(&editor);
        let error = prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false))
            .err()
            .unwrap();
        assert!(error.to_string().contains("Unlock"));
        assert_eq!(editor_state(&editor), before);
    }
}

#[test]
fn locked_retrace_targets_and_target_ancestors_are_inert() {
    for lock_parent in [false, true] {
        let (mut editor, source, parent) = nested_editor();
        let prepared =
            prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false)).unwrap();
        let target = editor.apply_image_trace(prepared).unwrap();
        assert!(editor.set_locked(if lock_parent { &parent } else { &target }, true));
        let before = editor_state(&editor);
        let error = prepare_for(
            &editor,
            &source,
            Some(&target),
            &options(),
            &AtomicBool::new(false),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("Unlock"));
        assert_eq!(editor_state(&editor), before);
    }
}

#[test]
fn an_active_uncommitted_brush_stroke_blocks_publication_without_ending_it() {
    let (mut editor, source) = editor();
    let prepared =
        prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false)).unwrap();
    let revision = editor.revision();
    editor.brush.size = 3.;
    editor.brush.color = [250, 230, 30, 255];
    assert!(editor.begin_stroke(7., 7., 1., PaintTool::Brush));
    assert_eq!(editor.revision(), revision);
    let before = editor_state(&editor);
    assert!(
        editor
            .apply_image_trace(prepared)
            .unwrap_err()
            .to_string()
            .contains("active edit")
    );
    assert_eq!(editor_state(&editor), before);
    assert!(
        editor.finish_stroke(),
        "Rejected trace unexpectedly ended or discarded the brush edit"
    );
}

#[test]
fn unrelated_retrace_targets_are_rejected_without_hiding_or_replacing_source() {
    let (mut editor, source) = editor();
    let other = editor.add_layer("Unrelated pixel layer");
    let before = editor_state(&editor);
    assert!(
        prepare_for(
            &editor,
            &source,
            Some(&other),
            &options(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(editor_state(&editor), before);
    let prepared =
        prepare_for(&editor, &source, None, &options(), &AtomicBool::new(false)).unwrap();
    let trace = editor.apply_image_trace(prepared).unwrap();
    editor.document.find_layer_mut(&other).unwrap().image = Some(pixels().into());
    let before = editor_state(&editor);
    assert!(
        prepare_for(
            &editor,
            &other,
            Some(&trace),
            &options(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(editor_state(&editor), before);
}

#[test]
fn valid_null_source_metadata_still_retains_trace_settings_and_source_identity() {
    let (mut editor, source) = editor();
    editor.document.find_layer_mut(&source).unwrap().metadata = Value::Null;
    assert!(raster::validate(&editor.document).is_empty());
    let options = options();
    let prepared = prepare_for(&editor, &source, None, &options, &AtomicBool::new(false)).unwrap();
    let target = editor.apply_image_trace(prepared).unwrap();
    assert_eq!(
        image_trace_layer::retained_settings(editor.document.find_layer(&target).unwrap()),
        Some((source.clone(), options))
    );
    assert_eq!(
        editor.document.find_layer(&source).unwrap().metadata,
        Value::Null
    );
}
