use omuse::{
    advanced::LayerState,
    camera_raw::Settings,
    editor::Editor,
    model::Document,
    xmp_preset::{self, METADATA_KEY, SavedProvenance, SourceKind, TargetIdentity},
};
use std::sync::{Arc, atomic::AtomicBool};
fn xml() -> Vec<u8> {
    br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:c="http://ns.adobe.com/camera-raw-settings/1.0/" c:Exposure2012="0.75" c:Saturation="13" c:Temperature="6500" c:CameraProfile="Unresolved commercial profile" c:ProcessVersion="11.0"/></rdf:RDF>"#.to_vec()
}
fn fixture() -> (Editor, String, TargetIdentity, Settings) {
    let pixels = image::RgbaImage::from_fn(12, 8, |x, y| {
        image::Rgba([
            (19 + x * 13 + y * 3) as u8,
            (31 + x * 5 + y * 9) as u8,
            (221 - x * 7 - y * 11) as u8,
            [0, 1, 31, 127, 254, 255][x as usize % 6],
        ])
    });
    let state = LayerState::from_image(&pixels, "synthetic retained original").unwrap();
    let mut document = Document::new(12, 8);
    let id = document.layers[0].id.clone();
    document.layers[0].image = Some(pixels.into());
    document.layers[0].advanced = Some(Arc::new(state.clone()));
    let mut editor = Editor::new(document);
    editor.active_layer = id.clone();
    let base = Settings::for_native_edit();
    let value = serde_json::to_value(&base).unwrap();
    let target = TargetIdentity {
        kind: SourceKind::Rendered,
        layer_id: id.clone(),
        source_id: state.recipe.source_id.clone(),
        editor_instance: editor.instance_id(),
        revision: editor.revision(),
        working_space: format!("{:?}", state.recipe.working_space),
        processing: "Native16V1".into(),
        tone_mapping: value
            .get("toneMapping")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Legacy")
            .into(),
        camera_make: None,
        camera_model: None,
        camera_profile: None,
    };
    (editor, id, target, base)
}
#[test]
fn imported_native_grade_and_inert_report_commit_as_one_undo_and_reopen_without_xml() {
    let (mut editor, id, target, base) = fixture();
    let before = editor.document.clone();
    let original = xml();
    let plan = xmp_preset::parse(&original, target.clone()).unwrap();
    let settings = plan.reviewed_settings(&base, &target).unwrap();
    let state = editor.editable_state(&id).unwrap();
    let mask = editor.selection_for_layer(&id).unwrap();
    let (prepared, _) = state
        .with_native_camera_raw(settings.clone(), mask, &AtomicBool::new(false), None)
        .unwrap();
    let provenance = SavedProvenance::new(
        plan.report().clone(),
        &serde_json::to_value(&settings).unwrap(),
        &settings,
    )
    .unwrap();
    assert!(
        editor
            .replace_editable_states_with_xmp_provenance(
                vec![(id.clone(), prepared.clone())],
                &provenance
            )
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), 1);
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
        prepared.result.to_rgba16()
    );
    let report = editor.document.find_layer(&id).unwrap().metadata[METADATA_KEY].clone();
    assert_eq!(
        report["mapping"]["sourceSha256"],
        omuse::asset_library::sha256_hex(&original)
    );
    assert_eq!(report["modifiedAfterImport"], false);
    assert!(editor.undo());
    assert_eq!(
        editor.document.find_layer(&id).unwrap().image,
        before.find_layer(&id).unwrap().image
    );
    assert!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .metadata
            .get(METADATA_KEY)
            .is_none()
    );
    assert!(editor.redo());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("portable.omuse");
    omuse::document::save(&editor.document, &path).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 19);
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(
        reopened.find_layer(&id).unwrap().metadata[METADATA_KEY],
        report
    );
    assert_eq!(
        reopened
            .find_layer(&id)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .evaluate(&AtomicBool::new(false))
            .unwrap()
            .result
            .to_rgba16(),
        prepared.result.to_rgba16()
    );
}
#[test]
fn stale_wrong_node_or_oversized_provenance_refuse_without_pixel_metadata_or_history_change() {
    let (mut editor, id, target, base) = fixture();
    let before = editor.document.clone();
    let plan = xmp_preset::parse(&xml(), target.clone()).unwrap();
    let settings = plan.reviewed_settings(&base, &target).unwrap();
    let state = editor.editable_state(&id).unwrap();
    let (prepared, _) = state
        .with_native_camera_raw(settings.clone(), None, &AtomicBool::new(false), None)
        .unwrap();
    let provenance = SavedProvenance::new(
        plan.report().clone(),
        &serde_json::to_value(&settings).unwrap(),
        &settings,
    )
    .unwrap();
    let mut stale = provenance.clone();
    stale.mapping.target.revision += 1;
    assert!(
        editor
            .replace_editable_states_with_xmp_provenance(
                vec![(id.clone(), prepared.clone())],
                &stale
            )
            .is_err()
    );
    let mut wrong = provenance.clone();
    wrong.native_settings_at_apply.exposure += 0.1;
    assert!(
        editor
            .replace_editable_states_with_xmp_provenance(
                vec![(id.clone(), prepared.clone())],
                &wrong
            )
            .is_err()
    );
    let mut huge = provenance.clone();
    huge.mapping.rows[0].input = "x".repeat(1025);
    assert!(
        editor
            .replace_editable_states_with_xmp_provenance(vec![(id.clone(), prepared)], &huge)
            .is_err()
    );
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(
        editor.document.find_layer(&id).unwrap().image,
        before.find_layer(&id).unwrap().image
    );
    assert!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .metadata
            .get(METADATA_KEY)
            .is_none()
    );
}

#[test]
fn photo_promotion_preserves_valid_live_clip_disabled_bitmap_and_placement() {
    let (mut editor, id, _, _) = fixture();
    editor.document.find_layer_mut(&id).unwrap().advanced = None;
    attach_live_clip_and_disabled_bitmap(&mut editor, &id);
    let before = editor.document.clone();
    let composed = omuse::raster::composite(&before);
    assert_eq!(composed.get_pixel(9, 4)[3], 0);
    assert!(composed.get_pixel(2, 4)[3] > 0);
    let state = editor.editable_state(&id).unwrap();
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), state)])
            .unwrap()
    );
    assert_eq!(editor.undo_depth(), 1);
    assert_eq!(omuse::raster::composite(&editor.document), composed);
    assert_live_links_and_bitmap(&editor.document, &before, &id);
    assert!(editor.undo());
    assert_eq!(omuse::raster::composite(&editor.document), composed);
}
fn attach_live_clip_and_disabled_bitmap(editor: &mut Editor, id: &str) {
    let mut source = omuse::model::Layer::paint("Valid hidden half-image clip", 12, 8);
    source.image = Some(
        image::RgbaImage::from_fn(12, 8, |x, _| {
            image::Rgba([255, 255, 255, if x < 6 { 255 } else { 0 }])
        })
        .into(),
    );
    source.visible = false;
    let layer = editor.document.find_layer_mut(id).unwrap();
    layer.mask = Some(image::RgbaImage::from_pixel(12, 8, image::Rgba([0, 0, 0, 255])).into());
    layer.metadata["maskEnabled"] = false.into();
    layer.metadata["maskSourceID"] = serde_json::json!(source.id);
    layer.metadata["maskPlacement"] =
        serde_json::json!({"origin":[1,1],"size":[10,6],"rotation":0,"flipX":false,"flipY":false});
    editor.document.layers.push(source);
}
fn assert_live_links_and_bitmap(actual: &Document, before: &Document, id: &str) {
    let (a, b) = (
        actual.find_layer(id).unwrap(),
        before.find_layer(id).unwrap(),
    );
    assert_eq!(a.mask, b.mask);
    for key in ["maskEnabled", "maskSourceID", "maskPlacement"] {
        assert_eq!(a.metadata[key], b.metadata[key], "{key}");
    }
    assert!(
        actual
            .find_layer(a.metadata["maskSourceID"].as_str().unwrap())
            .is_some(),
        "live link identifies an actual layer"
    );
}
/// Opaque synthetic payload retention only. This does not qualify a RAW decoder,
/// camera sample, profile colour transform or Adobe rendering.
#[test]
fn imported_native_grade_preserves_live_clip_prior_byte_node_and_opaque_raw_source_metadata() {
    let (mut editor, id, mut target, base) = fixture();
    attach_live_clip_and_disabled_bitmap(&mut editor, &id);
    let layer = editor.document.find_layer_mut(&id).unwrap();
    let state = Arc::make_mut(layer.advanced.as_mut().unwrap());
    state.raw_bytes = Some(Arc::new(
        b"opaque synthetic RAW payload; never decoded".to_vec(),
    ));
    state.recipe.raw_extension = Some("dng".into());
    state.recipe.raw_settings = Some(omuse::raw_import::DevelopSettings {
        temperature: 5500.,
        tint: 7.,
        as_shot_temperature: 5400.,
        as_shot_tint: 3.,
        ..Default::default()
    });
    state.recipe.nodes.push(omuse::advanced_ops::FilterNode {
        id: "49AD586E-B6F4-4611-93C5-DB5BB9A5C8C2".into(),
        name: "Existing byte-compatible node".into(),
        enabled: false,
        opacity: 1.,
        operation: omuse::advanced_ops::AdvancedOperation::CameraRaw(Settings::default()),
        soft_mask: None,
    });
    state.validate().unwrap();
    target.kind = SourceKind::DecodedRaw;
    let before = editor.document.clone();
    let old_state = before
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap()
        .clone();
    let composed = omuse::raster::composite(&before);
    assert_eq!(composed.get_pixel(9, 4)[3], 0);
    assert!(composed.get_pixel(2, 4)[3] > 0);
    let plan = xmp_preset::parse(&xml(), target.clone()).unwrap();
    let settings = plan.reviewed_settings(&base, &target).unwrap();
    assert_eq!(settings.temperature, base.temperature);
    let (prepared, _) = old_state
        .with_native_camera_raw(settings.clone(), None, &AtomicBool::new(false), None)
        .unwrap();
    let provenance = SavedProvenance::new(
        plan.report().clone(),
        &serde_json::to_value(&settings).unwrap(),
        &settings,
    )
    .unwrap();
    assert!(
        editor
            .replace_editable_states_with_xmp_provenance(vec![(id.clone(), prepared)], &provenance)
            .unwrap()
    );
    assert_live_links_and_bitmap(&editor.document, &before, &id);
    let kept = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(&kept.source, &old_state.source));
    assert!(Arc::ptr_eq(
        kept.raw_bytes.as_ref().unwrap(),
        old_state.raw_bytes.as_ref().unwrap()
    ));
    assert_eq!(kept.recipe.raw_settings, old_state.recipe.raw_settings);
    assert_eq!(kept.recipe.nodes[0], old_state.recipe.nodes[0]);
    let after = omuse::raster::composite(&editor.document);
    for (a, b) in composed.pixels().zip(after.pixels()) {
        assert_eq!(a[3], b[3], "live mask/alpha footprint must not change");
    }
    assert_ne!(after.get_pixel(2, 4), composed.get_pixel(2, 4));
    assert_eq!(editor.undo_depth(), 1);
    let temp = tempfile::tempdir().unwrap();
    let saved = temp.path().join("source-preserved.omuse");
    omuse::document::save(&editor.document, &saved).unwrap();
    let reopened = omuse::document::open(&saved).unwrap();
    assert_live_links_and_bitmap(&reopened, &before, &id);
    let reopened_state = reopened.find_layer(&id).unwrap().advanced.as_ref().unwrap();
    assert_eq!(reopened_state.raw_bytes, old_state.raw_bytes);
    assert_eq!(
        reopened_state.recipe.raw_settings,
        old_state.recipe.raw_settings
    );
    assert_eq!(reopened_state.recipe.nodes[0], old_state.recipe.nodes[0]);
    assert!(editor.undo());
    assert_eq!(omuse::raster::composite(&editor.document), composed);
    assert_live_links_and_bitmap(&editor.document, &before, &id);
}

#[test]
fn background_certified_promotion_keeps_original_cache_and_rejects_cancel_or_replacement() {
    let (mut editor, id, _, base) = fixture();
    editor.document.find_layer_mut(&id).unwrap().advanced = None;
    attach_live_clip_and_disabled_bitmap(&mut editor, &id);
    let before = editor.document.clone();
    let layer = editor.document.find_layer(&id).unwrap();
    let image = layer.image.as_ref().unwrap().as_arc();
    let target = TargetIdentity {
        kind: SourceKind::Rendered,
        layer_id: id.clone(),
        source_id: "captured-paint".into(),
        editor_instance: editor.instance_id(),
        revision: editor.revision(),
        working_space: "Srgb".into(),
        processing: "ByteCompatible".into(),
        tone_mapping: serde_json::to_value(&base)
            .unwrap()
            .get("toneMapping")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Legacy")
            .into(),
        camera_make: None,
        camera_model: None,
        camera_profile: None,
    };
    assert!(
        xmp_preset::PreparedPhotoPromotion::prepare(
            target.clone(),
            image.clone(),
            "photo".into(),
            None,
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert_eq!(editor.undo_depth(), 0);
    let prepared = xmp_preset::PreparedPhotoPromotion::prepare(
        target.clone(),
        image.clone(),
        "photo".into(),
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(editor.commit_prepared_photo_promotion(prepared).unwrap());
    assert!(Arc::ptr_eq(
        &editor
            .document
            .find_layer(&id)
            .unwrap()
            .image
            .as_ref()
            .unwrap()
            .as_arc(),
        &image
    ));
    assert_live_links_and_bitmap(&editor.document, &before, &id);
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    let mut replacement = Editor::new(before.clone());
    let prepared = xmp_preset::PreparedPhotoPromotion::prepare(
        target,
        image,
        "photo".into(),
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        replacement
            .commit_prepared_photo_promotion(prepared)
            .is_err()
    );
    assert_eq!(replacement.undo_depth(), 0);
    assert!(
        replacement
            .document
            .find_layer(&id)
            .unwrap()
            .advanced
            .is_none()
    );
}
