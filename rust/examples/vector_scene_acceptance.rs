//! Reproducible allocation accounting for the first procedural vector-scene
//! slice. Run as an optimized example with a new output directory.
//!
//! The reported byte counts come from owned buffer capacities and tiled-image
//! accounting. They are not RSS measurements or rendering-speed claims.

use anyhow::{Context, Result, ensure};
use omuse::{
    advanced::LayerState,
    document,
    editor::Editor,
    model::Document,
    vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene},
};
use std::{env, fs, path::PathBuf, sync::Arc, sync::atomic::AtomicBool};

const EDGE: u32 = 4_096;

fn scene() -> Result<VectorScene> {
    let mut objects = vec![
        VectorObject::rectangle(
            "Midnight background",
            0.,
            0.,
            EDGE as f32,
            EDGE as f32,
            Some([16, 24, 42, 255]),
            None,
        )?,
        VectorObject::ellipse(
            "Coral ellipse",
            360.,
            420.,
            1_760.,
            1_360.,
            Some([235, 92, 76, 224]),
            None,
        )?,
        VectorObject::ellipse(
            "Gold ellipse",
            1_880.,
            1_960.,
            1_720.,
            1_400.,
            Some([244, 187, 70, 210]),
            None,
        )?,
        VectorObject::rectangle(
            "Teal panel",
            2_260.,
            380.,
            1_280.,
            1_020.,
            Some([32, 180, 166, 230]),
            None,
        )?,
        VectorObject::rectangle(
            "Violet panel",
            620.,
            2_340.,
            1_260.,
            1_080.,
            Some([139, 92, 246, 215]),
            None,
        )?,
    ];
    objects[3].transform = [1., 0.12, -0.18, 1., 80., -120.];
    objects[4].transform = [1., -0.1, 0.16, 1., -120., 140.];
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: EDGE,
        height: EDGE,
        objects,
    };
    scene.validate()?;
    Ok(scene)
}

fn main() -> Result<()> {
    let directory = PathBuf::from(
        env::args_os()
            .nth(1)
            .context("Pass a new output directory")?,
    );
    fs::create_dir(&directory).context("The output directory must not already exist")?;

    let cancel = AtomicBool::new(false);
    let scene = scene()?;
    let scene_heap_bytes = scene.retained_bytes();
    let cache = scene.render(&cancel)?;
    let cache_bytes = cache.as_raw().capacity();

    let mut editor = Editor::new(Document::new(EDGE, EDGE));
    let layer_id = editor.insert_vector_scene(
        "Multi-object vector",
        editor.revision(),
        scene.clone(),
        cache,
    )?;
    ensure!(
        editor.undo_depth() == 1,
        "Vector scene did not make one Undo step"
    );
    ensure!(editor.undo(), "Vector scene Undo failed");
    ensure!(
        editor.document.find_layer(&layer_id).is_none(),
        "Undo retained the inserted vector layer"
    );
    ensure!(editor.redo(), "Vector scene Redo failed");
    let expected_cache = editor
        .document
        .find_layer(&layer_id)
        .and_then(|layer| layer.image.clone())
        .context("Applied vector cache is missing")?;

    let project_path = directory.join("multi-object-vector.omuse");
    document::save(&editor.document, &project_path)?;
    let reopened = document::open(&project_path)?;
    let layer = reopened
        .find_layer(&layer_id)
        .context("Reopened vector layer is missing")?;
    ensure!(
        layer.vector_scene.as_deref() == Some(&scene),
        "Reopened vector geometry changed"
    );
    ensure!(
        layer.image.as_ref() == Some(&expected_cache),
        "Reopened derived vector cache changed"
    );

    // Allocate independent high-precision source and result stores, plus the
    // ordinary RGBA8 proxy, to make the former raster-backed representation
    // explicit instead of inferring it from pixel-format arithmetic.
    let mut old_state = LayerState::from_image(&expected_cache, "Raster-backed comparison source")?;
    let independent_result =
        LayerState::from_image(&expected_cache, "Raster-backed comparison result")?;
    old_state.result = independent_result.result;
    ensure!(
        !Arc::ptr_eq(&old_state.source, &old_state.result),
        "Comparison source and result unexpectedly share storage"
    );
    let old_proxy = old_state.proxy()?;
    let source_bytes = old_state.source.memory_bytes();
    let result_bytes = old_state.result.memory_bytes();
    let proxy_bytes = old_proxy.as_raw().capacity();
    let old_total_bytes = source_bytes
        .saturating_add(result_bytes)
        .saturating_add(proxy_bytes);
    let scene_total_bytes = scene_heap_bytes.saturating_add(cache_bytes);
    ensure!(
        scene_total_bytes < old_total_bytes,
        "Procedural scene accounting did not reduce retained bytes"
    );

    let measurements = serde_json::json!({
        "scope": "synthetic 4096x4096 artwork; scene allocation accounting only",
        "objects": scene.objects.len(),
        "procedural": {
            "sceneHeapBytes": scene_heap_bytes,
            "oneDerivedRgba8CacheBytes": cache_bytes,
            "totalBytes": scene_total_bytes,
        },
        "rasterBackedComparison": {
            "independentTiled16SourceBytes": source_bytes,
            "independentTiled16ResultBytes": result_bytes,
            "rgba8ProxyBytes": proxy_bytes,
            "totalBytes": old_total_bytes,
        },
        "checks": [
            "several coloured objects retained in one procedural scene layer",
            "one Undo and Redo",
            "project save and reopen preserve geometry and derived pixels"
        ],
        "limitations": [
            "the comparison counts the vector scene and its one cache, not the document's initial blank layer, Undo history, or process memory",
            "byte counts are owned allocation capacities, not process RSS",
            "this example makes no rendering-speed or artwork-quality claim"
        ]
    });
    fs::write(
        directory.join("vector-scene-measurements.json"),
        serde_json::to_vec_pretty(&measurements)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&measurements)?);
    Ok(())
}
