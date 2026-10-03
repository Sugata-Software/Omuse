use image::RgbaImage;
use omuse::{
    document,
    editor::{Adjustment, Editor, LayerPlacement, PaintTool},
    model::Document,
    objects, raster,
    vector_path::{Anchor, FillRule, Point, Subpath, VectorPath},
    vector_scene::VectorScene,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::atomic::AtomicBool,
};

fn triangle() -> VectorPath {
    VectorPath {
        fill_rule: FillRule::NonZero,
        subpaths: vec![Subpath {
            closed: true,
            anchors: [(2., 2.), (14., 3.), (7., 11.)]
                .into_iter()
                .map(|(x, y)| Anchor {
                    position: Point { x, y },
                    incoming: None,
                    outgoing: None,
                })
                .collect(),
        }],
    }
}

fn scene(color: [u8; 4]) -> VectorScene {
    VectorScene::from_path(16, 12, "Triangle", triangle(), Some(color), None).unwrap()
}

fn cache(scene: &VectorScene) -> RgbaImage {
    scene.render(&AtomicBool::new(false)).unwrap()
}

fn insert_scene(editor: &mut Editor, color: [u8; 4]) -> String {
    let scene = scene(color);
    let cache = cache(&scene);
    editor
        .insert_vector_scene("Scene", editor.revision(), scene, cache)
        .unwrap()
}

#[test]
fn scene_cache_is_guarded_and_rasterize_is_one_reversible_step() {
    let mut editor = Editor::new(Document::new(24, 18));
    let id = insert_scene(&mut editor, [210, 45, 30, 220]);
    assert!(editor.select_layer(&id));
    let layer = editor.document.find_layer(&id).unwrap();
    let pixels = layer.image.clone().unwrap();
    let geometry = layer.vector_scene.clone().unwrap();
    let revision = editor.revision();
    let depth = editor.undo_depth();

    assert!(!editor.begin_stroke(5., 5., 1., PaintTool::Brush));
    assert!(!editor.fill_selection([1, 2, 3, 255]));
    assert!(!editor.adjust(Adjustment::Invert));
    assert!(editor.add_mask(&id, true));
    let mask_depth = editor.undo_depth();
    assert!(!editor.remove_mask(&id, true));
    assert_eq!(editor.undo_depth(), mask_depth);
    assert_eq!(editor.revision(), revision + 1);
    let guarded = editor.document.find_layer(&id).unwrap();
    assert_eq!(guarded.image.as_ref(), Some(&pixels));
    assert_eq!(guarded.vector_scene.as_deref(), Some(geometry.as_ref()));

    assert!(editor.rasterize_layer(&id));
    let raster = editor.document.find_layer(&id).unwrap();
    assert!(raster.vector_scene.is_none());
    assert_eq!(raster.image.as_ref(), Some(&pixels));
    assert_eq!(editor.undo_depth(), mask_depth + 1);
    assert!(editor.undo());
    let restored = editor.document.find_layer(&id).unwrap();
    assert_eq!(restored.vector_scene.as_deref(), Some(geometry.as_ref()));
    assert_eq!(restored.image.as_ref(), Some(&pixels));
    assert!(editor.undo());
    assert!(editor.document.find_layer(&id).unwrap().mask.is_none());
    assert_eq!(editor.undo_depth(), depth);
}

#[test]
fn prepared_replacement_is_revision_fenced_exclusive_and_atomic() {
    let mut editor = Editor::new(Document::new(24, 18));
    let id = insert_scene(&mut editor, [20, 40, 60, 255]);
    let before = editor.document.find_layer(&id).unwrap().clone();
    let replacement = scene([80, 100, 120, 255]);
    let rendered = cache(&replacement);

    let error = editor
        .replace_vector_scene(
            &id,
            editor.revision() + 1,
            replacement.clone(),
            rendered.clone(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("changed"));
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);

    // Geometry is authoritative editing state. A semantic scene change must
    // commit even when its prepared compatibility cache is byte-identical.
    let mut geometry_only = (**before.vector_scene.as_ref().unwrap()).clone();
    geometry_only.objects[0].name = "Semantic rename".into();
    let same_pixels = before.image.as_ref().unwrap().to_image();
    let geometry_revision = editor.revision();
    assert!(
        editor
            .replace_vector_scene(&id, geometry_revision, geometry_only.clone(), same_pixels,)
            .unwrap()
    );
    assert_eq!(editor.revision(), geometry_revision + 1);
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(&geometry_only)
    );
    assert!(editor.undo());
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);

    let invalid = RgbaImage::new(15, 12);
    assert!(
        editor
            .replace_vector_scene(&id, editor.revision(), replacement.clone(), invalid)
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);
    let mut resized_scene = replacement.clone();
    resized_scene.width = 8;
    resized_scene.height = 8;
    let resized_cache = cache(&resized_scene);
    assert!(
        editor
            .replace_vector_scene(&id, editor.revision(), resized_scene, resized_cache)
            .unwrap_err()
            .to_string()
            .contains("retain the existing cache dimensions")
    );
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);
    let revision = editor.revision();
    assert!(
        editor
            .replace_vector_scene(&id, revision, replacement.clone(), rendered.clone())
            .unwrap()
    );
    assert_eq!(
        editor.document.find_layer(&id).unwrap().image.as_deref(),
        Some(&rendered)
    );
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(&replacement)
    );
    assert!(editor.undo());
    assert_eq!(editor.document.find_layer(&id).unwrap().image, before.image);
}

#[test]
fn scene_project_binds_geometry_to_cache_and_ordinary_projects_remain_v10() {
    let directory = tempfile::tempdir().unwrap();
    let ordinary_path = directory.path().join("ordinary.omuse");
    let ordinary = Document::new(24, 18);
    document::save(&ordinary, &ordinary_path).unwrap();
    let ordinary_manifest: Value =
        serde_json::from_slice(&std::fs::read(ordinary_path.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(ordinary_manifest["version"], 10);
    // External format 11 also carries ordinary raster/font-run projects.
    // Reading those must preserve their cached pixels without inventing a scene.
    let mut external_v11 = ordinary_manifest.clone();
    external_v11["version"] = json!(11);
    std::fs::write(
        ordinary_path.join("manifest.json"),
        serde_json::to_vec_pretty(&external_v11).unwrap(),
    )
    .unwrap();
    let reopened_ordinary = document::open(&ordinary_path).unwrap();
    assert_eq!(reopened_ordinary.layers[0].image, ordinary.layers[0].image);
    assert!(reopened_ordinary.layers[0].vector_scene.is_none());

    // Omuse's grouped/styled/text scene formats still require real scene data.
    for version in 12..=14 {
        let mut invalid_scene = ordinary_manifest.clone();
        invalid_scene["version"] = json!(version);
        std::fs::write(
            ordinary_path.join("manifest.json"),
            serde_json::to_vec_pretty(&invalid_scene).unwrap(),
        )
        .unwrap();
        assert!(
            document::open(&ordinary_path)
                .unwrap_err()
                .to_string()
                .contains("require at least one vector scene")
        );
    }

    let mut editor = Editor::new(Document::new(24, 18));
    let id = insert_scene(&mut editor, [11, 91, 201, 173]);
    let expected_scene = editor
        .document
        .find_layer(&id)
        .unwrap()
        .vector_scene
        .clone()
        .unwrap();
    let expected_cache = editor
        .document
        .find_layer(&id)
        .unwrap()
        .image
        .clone()
        .unwrap();
    let path = directory.path().join("scene.omuse");
    document::save(&editor.document, &path).unwrap();

    let manifest_path = path.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["version"], 11);
    let record = manifest["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"] == id)
        .unwrap();
    assert_eq!(record["imageFile"], format!("{id}.png"));
    assert_eq!(record["rustVectorScene"]["version"], 1);
    assert_eq!(record["rustVectorScene"]["cacheWidth"], 16);
    assert_eq!(
        record["rustVectorScene"]["cacheSha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        record["rustVectorScene"]["sceneSha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let expected_scene_digest = record["rustVectorScene"]["sceneSha256"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(path.join("images").join(format!("{id}.png")).is_file());
    assert!(
        path.join("images")
            .join(format!("{id}.vector-scene.json.z"))
            .is_file()
    );

    let reopened = document::open(&path).unwrap();
    let layer = reopened.find_layer(&id).unwrap();
    assert_eq!(layer.vector_scene.as_deref(), Some(expected_scene.as_ref()));
    assert_eq!(layer.image.as_ref(), Some(&expected_cache));

    let clean_manifest = manifest.clone();
    manifest["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == id)
        .unwrap()["rustVectorScene"]["cacheSha256"] = json!("0".repeat(64));
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("digest")
    );

    let mut unknown_descriptor = clean_manifest.clone();
    unknown_descriptor["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == id)
        .unwrap()["rustVectorScene"]["futureField"] = json!(true);
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&unknown_descriptor).unwrap(),
    )
    .unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("descriptor")
    );

    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&clean_manifest).unwrap(),
    )
    .unwrap();
    let sidecar = path
        .join("images")
        .join(format!("{id}.vector-scene.json.z"));
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(std::fs::File::open(&sidecar).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(
        expected_scene_digest,
        omuse::asset_library::sha256_hex(&decoded)
    );
    let mut sidecar_json: Value = serde_json::from_slice(&decoded).unwrap();
    sidecar_json["futureField"] = json!(true);
    let invalid_geometry = serde_json::to_vec(&sidecar_json).unwrap();
    let file = std::fs::File::create(&sidecar).unwrap();
    let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::fast());
    encoder.write_all(&invalid_geometry).unwrap();
    encoder.finish().unwrap();
    let mut invalid_geometry_manifest = clean_manifest;
    invalid_geometry_manifest["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == id)
        .unwrap()["rustVectorScene"]["sceneSha256"] =
        json!(omuse::asset_library::sha256_hex(&invalid_geometry));
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&invalid_geometry_manifest).unwrap(),
    )
    .unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("geometry")
    );
}

#[test]
fn scene_project_rejects_valid_sidecars_swapped_between_equal_sized_layers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("swapped.omuse");
    let mut editor = Editor::new(Document::new(24, 18));
    let first = insert_scene(&mut editor, [230, 30, 40, 255]);
    let second = insert_scene(&mut editor, [20, 80, 220, 255]);
    document::save(&editor.document, &path).unwrap();

    let first_sidecar = path
        .join("images")
        .join(format!("{first}.vector-scene.json.z"));
    let second_sidecar = path
        .join("images")
        .join(format!("{second}.vector-scene.json.z"));
    let first_bytes = std::fs::read(&first_sidecar).unwrap();
    let second_bytes = std::fs::read(&second_sidecar).unwrap();
    std::fs::write(&first_sidecar, second_bytes).unwrap();
    std::fs::write(&second_sidecar, first_bytes).unwrap();

    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("geometry digest")
    );
}

#[test]
fn duplicate_and_clipboard_keep_geometry_cache_and_bounded_accounting() {
    let mut source = Editor::new(Document::new(24, 18));
    let id = insert_scene(&mut source, [180, 70, 20, 255]);
    let scene = source
        .document
        .find_layer(&id)
        .unwrap()
        .vector_scene
        .clone()
        .unwrap();
    let cache = source
        .document
        .find_layer(&id)
        .unwrap()
        .image
        .clone()
        .unwrap();
    let duplicate = source.duplicate_layer(&id).unwrap();
    let duplicated = source.document.find_layer(&duplicate).unwrap();
    assert_eq!(duplicated.vector_scene.as_deref(), Some(scene.as_ref()));
    assert_eq!(duplicated.image.as_ref(), Some(&cache));

    let clipboard = source.copy_layers(&[id]).unwrap();
    assert!(clipboard.retained_bytes() >= cache.as_raw().len() + scene.retained_bytes());
    assert!(clipboard.retained_bytes() < 2 * 1024 * 1024);
    let mut target = Editor::new(Document::new(24, 18));
    let pasted = target.paste_layers(&clipboard).unwrap();
    let layer = target.document.find_layer(&pasted[0]).unwrap();
    assert_eq!(layer.vector_scene.as_deref(), Some(scene.as_ref()));
    assert_eq!(layer.image.as_ref(), Some(&cache));
    assert_ne!(pasted[0], duplicate);
}

#[test]
fn transforms_crop_and_image_resize_preserve_scene_and_mask_sources() {
    let mut editor = Editor::new(Document::new(24, 18));
    let id = insert_scene(&mut editor, [95, 135, 175, 255]);
    let scene = editor
        .document
        .find_layer(&id)
        .unwrap()
        .vector_scene
        .clone()
        .unwrap();
    let cache = editor
        .document
        .find_layer(&id)
        .unwrap()
        .image
        .clone()
        .unwrap();
    assert!(editor.add_mask(&id, true));
    assert!(editor.set_mask_linked(&id, false));
    let detached = LayerPlacement {
        x: 3.,
        y: 4.,
        width: 13.,
        height: 9.,
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    assert!(editor.set_mask_placement(&id, detached));
    let mask = editor
        .document
        .find_layer(&id)
        .unwrap()
        .mask
        .clone()
        .unwrap();
    assert!(editor.crop_canvas(2, 1, 22, 17));
    let cropped = editor.document.find_layer(&id).unwrap();
    assert_eq!(cropped.vector_scene.as_deref(), Some(scene.as_ref()));
    assert_eq!(cropped.image.as_ref(), Some(&cache));
    assert_eq!(cropped.mask.as_ref(), Some(&mask));
    assert_eq!(editor.mask_placement(&id).unwrap().x, 1.);
    assert_eq!(editor.mask_placement(&id).unwrap().y, 3.);

    assert!(editor.resize_image_with_options(44, 34, 144., "High quality"));
    let resized = editor.document.find_layer(&id).unwrap();
    assert_eq!(resized.vector_scene.as_deref(), Some(scene.as_ref()));
    assert_eq!(resized.image.as_ref(), Some(&cache));
    assert_eq!(resized.mask.as_ref(), Some(&mask));
    let resized_mask = editor.mask_placement(&id).unwrap();
    assert!((resized_mask.x - 2.).abs() < 1.0e-4);
    assert!((resized_mask.y - 6.).abs() < 1.0e-4);

    let placement = editor.layer_placement(&id).unwrap();
    assert!(editor.set_layer_placement(
        &id,
        LayerPlacement {
            rotation: 17.,
            ..placement
        }
    ));
    let revision = editor.revision();
    let history = editor.undo_depth();
    assert!(!editor.resize_image_with_options(88, 34, 144., "Nearest"));
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.undo_depth(), history);
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(scene.as_ref())
    );
}

#[test]
fn direct_invalid_cache_is_not_rendered_and_explicit_detach_clears_scene_identity() {
    let scene = scene([30, 60, 90, 255]);
    let rendered = cache(&scene);
    let mut document = Document::new(24, 18);
    {
        let layer = &mut document.layers[0];
        layer.image = Some(RgbaImage::new(15, 12).into());
        layer.vector_scene = Some(std::sync::Arc::new(scene.clone()));
    }
    let errors = raster::validate(&document);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("exclusive matching cached pixels"))
    );
    assert_eq!(raster::composite(&document).dimensions(), (0, 0));

    let layer = &mut document.layers[0];
    layer.image = Some(rendered.clone().into());
    layer.metadata["rustVectorScene"] = json!({"stale": true});
    objects::detach_live_object(layer);
    assert!(layer.vector_scene.is_none());
    assert!(layer.metadata.get("rustVectorScene").is_none());
    assert_eq!(layer.image.as_deref(), Some(&rendered));
}
