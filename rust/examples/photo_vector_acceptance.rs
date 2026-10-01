//! Reproducible synthetic artwork for the unreleased photo/vector foundation.
//! Run with a new destination directory; no user artwork is read or modified.
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode, TargetColourUniformity},
    document,
    editor::Editor,
    model::Document,
    precision::Rgba16Image,
    raster, vector_svg,
};
use std::{env, fs, path::PathBuf, sync::Arc, sync::atomic::AtomicBool};

fn document_with_state(state: LayerState) -> Result<Document> {
    let (width, height) = state.source.dimensions();
    let mut document = Document::new(width, height);
    document.layers[0].image = Some(state.proxy()?.into());
    document.layers[0].advanced = Some(Arc::new(state));
    Ok(document)
}

fn main() -> Result<()> {
    let directory = PathBuf::from(
        env::args_os()
            .nth(1)
            .context("Pass a new output directory")?,
    );
    fs::create_dir(&directory).context("The output directory must not already exist")?;
    let cancel = AtomicBool::new(false);
    // Fine, non-byte-aligned samples, varying tone, and an unaffected cool band.
    // These are synthetic swatches, not evidence of quality on photographs.
    let source = Rgba16Image::from_fn(320, 192, |x, y| {
        let texture = ((x * 19 + y * 31) % 1_100) as u16;
        let shade = y as u16 * 41;
        if x < 240 {
            Rgba([
                43_001 + shade + texture,
                18_003 + shade,
                9_001 + texture,
                65_535,
            ])
        } else {
            Rgba([8_003 + texture, 23_007 + shade, 49_003 + texture, 65_535])
        }
    });
    let original = LayerState::from_rgba16(&source, "Synthetic colour swatches")?;
    let before = document_with_state(original.clone())?;
    raster::export(&before, &directory.join("colour-before.png"))?;
    let mut edited = original;
    edited.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Target colour uniformity".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::TargetColourUniformity(TargetColourUniformity {
            target_rgb: [214, 126, 82],
            hue_range_degrees: 35.,
            hue_falloff_degrees: 20.,
            hue_uniformity: 0.7,
            saturation_uniformity: 0.5,
            lightness_uniformity: 0.,
        }),
        soft_mask: None,
    });
    let edited = edited.evaluate(&cancel)?;
    ensure!(
        edited.source.to_rgba16() == source,
        "Source samples changed"
    );
    let result = edited.result.to_rgba16();
    ensure!(result != source, "Uniformity made no visible change");
    for y in 0..192 {
        for x in 240..320 {
            ensure!(
                result.get_pixel(x, y) == source.get_pixel(x, y),
                "Cool band changed"
            );
        }
    }
    let mut editor = Editor::new(before);
    let id = editor.active_layer.clone();
    ensure!(
        editor.replace_editable_states(vec![(id, edited)])?,
        "Apply failed"
    );
    ensure!(
        editor.undo_depth() == 1 && editor.undo() && editor.redo(),
        "Undo/Redo failed"
    );
    let colour_path = directory.join("colour-uniformity.omuse");
    document::save(&editor.document, &colour_path)?;
    let reopened = document::open(&colour_path)?;
    ensure!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .result
            .to_rgba16()
            == result,
        "Saved uniformity result changed"
    );
    raster::export(&reopened, &directory.join("colour-after.png"))?;
    raster::export16(&reopened, &directory.join("colour-after-16.png"))?;

    let artwork = vector_svg::decode(
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="192">
      <path d="M40 36 C100 0 240 0 280 36 L280 156 C220 192 100 192 40 156 Z
               M110 64 L210 64 L210 128 L110 128 Z"
        fill="#D58049" fill-rule="evenodd" stroke="#473A36" stroke-width="4"
        stroke-linecap="round" stroke-linejoin="round"/>
    </svg>"##,
    )?;
    let svg_path = directory.join("compound-path.svg");
    vector_svg::export(&svg_path, &artwork)?;
    ensure!(
        vector_svg::import(&svg_path)? == artwork,
        "SVG round trip changed artwork"
    );
    let mut state = LayerState::from_image(&RgbaImage::new(320, 192), "Editable compound path")?;
    state.recipe.vector = Some(artwork.path.clone());
    state.recipe.vector_fill = artwork.fill.unwrap();
    state.recipe.vector_stroke = artwork.stroke;
    let vector_document = document_with_state(state.evaluate(&cancel)?)?;
    let vector_path = directory.join("vector-path.omuse");
    document::save(&vector_document, &vector_path)?;
    let reopened = document::open(&vector_path)?;
    ensure!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .recipe
            .vector
            .as_ref()
            == Some(&artwork.path),
        "Saved vector geometry changed"
    );
    raster::export(&reopened, &directory.join("vector-path.png"))?;
    fs::write(
        directory.join("results.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope": "synthetic artwork; no photographic-quality or release claim",
            "checks": ["source samples preserved", "unselected cool band exact", "one Undo and Redo",
                "native precision save/reopen", "editable SVG round trip", "retained compound geometry"],
            "olderReader": "The new colour node must be refused by 0.6.0; vector-only geometry remains readable. Run production readers separately."
        }))?,
    )?;
    println!("Acceptance artwork: {}", directory.display());
    Ok(())
}
