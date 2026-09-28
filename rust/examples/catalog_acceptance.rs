//! Render every native template at its original, square, story and wide sizes.
//! Each image is rendered after saving and reopening the editable document.
//! Usage: cargo run --release --example catalog_acceptance -- <new-directory>
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use omuse::{
    create, document,
    model::{Document, Layer},
    objects, raster, social_preview,
};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};

fn live_text(layers: &[Layer]) -> Result<Vec<(String, objects::LiveTextStyle)>> {
    let mut found = Vec::new();
    for layer in layers {
        if let Some(style) = objects::live_text(layer)? {
            found.push((layer.id.clone(), style));
        }
        found.extend(live_text(&layer.children)?);
    }
    Ok(found)
}

fn layer_state(layers: &[Layer]) -> Vec<Value> {
    layers
        .iter()
        .map(|layer| {
            json!({
                "id": layer.id,
                "name": layer.name,
                "visible": layer.visible,
                "locked": layer.locked,
                "opacity": layer.opacity,
                "blendMode": layer.blend_mode,
                "offset": [layer.offset_x, layer.offset_y],
                "rotation": layer.rotation,
                "scale": [layer.scale_x, layer.scale_y],
                "imageDimensions": layer.image.as_ref().map(|image| [image.width(), image.height()]),
                "maskDimensions": layer.mask.as_ref().map(|mask| [mask.width(), mask.height()]),
                "hasEditableSource": layer.advanced.is_some(),
                "metadata": layer.metadata,
                "children": layer_state(&layer.children),
            })
        })
        .collect()
}

fn document_state(document: &Document) -> Value {
    json!({
        "name": document.name,
        "dimensions": [document.width, document.height],
        "background": document.background,
        "metadata": document.metadata,
        "layers": layer_state(&document.layers),
    })
}

fn pixel_difference(expected: &RgbaImage, reopened: &RgbaImage) -> Value {
    let (expected_width, expected_height) = expected.dimensions();
    let (reopened_width, reopened_height) = reopened.dimensions();
    let shared_width = expected_width.min(reopened_width);
    let shared_height = expected_height.min(reopened_height);
    let mut changed_pixels = 0u64;
    let mut first_difference = None;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut max_channel_delta = [0u8; 4];

    for y in 0..shared_height {
        for x in 0..shared_width {
            let before = expected.get_pixel(x, y).0;
            let after = reopened.get_pixel(x, y).0;
            if before == after {
                continue;
            }
            changed_pixels += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            for channel in 0..4 {
                max_channel_delta[channel] =
                    max_channel_delta[channel].max(before[channel].abs_diff(after[channel]));
            }
            if first_difference.is_none() {
                first_difference = Some(json!({
                    "position": [x, y],
                    "expected": before,
                    "reopened": after,
                }));
            }
        }
    }

    // Differing dimensions are themselves a whole-image difference. The catalog
    // currently checks equal dimensions first, but retain an unambiguous count
    // should a future regression reach this diagnostic path.
    let expected_pixels = u64::from(expected_width) * u64::from(expected_height);
    let reopened_pixels = u64::from(reopened_width) * u64::from(reopened_height);
    if expected_pixels != reopened_pixels {
        changed_pixels = changed_pixels.saturating_add(expected_pixels.abs_diff(reopened_pixels));
    }

    json!({
        "expectedDimensions": [expected_width, expected_height],
        "reopenedDimensions": [reopened_width, reopened_height],
        "changedPixelsInSharedBounds": changed_pixels,
        "changedBounds": if min_x == u32::MAX { Value::Null } else { json!({
            "left": min_x,
            "top": min_y,
            "right": max_x,
            "bottom": max_y,
        }) },
        "maxChannelDelta": max_channel_delta,
        "firstDifference": first_difference,
    })
}

fn write_roundtrip_diagnostics(
    output: &std::path::Path,
    name: &str,
    artwork: &Document,
    reopened: &Document,
    expected: &RgbaImage,
    rendered: &RgbaImage,
) -> Result<()> {
    let expected_path = output.join(format!("{name}.expected.png"));
    let reopened_path = output.join(format!("{name}.reopened.png"));
    expected
        .save(&expected_path)
        .with_context(|| format!("Writing {}", expected_path.display()))?;
    rendered
        .save(&reopened_path)
        .with_context(|| format!("Writing {}", reopened_path.display()))?;
    let diagnostic_path = output.join(format!("{name}.roundtrip-diff.json"));
    fs::write(
        &diagnostic_path,
        serde_json::to_vec_pretty(&json!({
            "templateVariant": name,
            "pixelDifference": pixel_difference(expected, rendered),
            "beforeSave": document_state(artwork),
            "afterReopen": document_state(reopened),
            "expectedPng": expected_path.file_name().and_then(|path| path.to_str()),
            "reopenedPng": reopened_path.file_name().and_then(|path| path.to_str()),
        }))?,
    )
    .with_context(|| format!("Writing {}", diagnostic_path.display()))?;
    Ok(())
}

fn main() -> Result<()> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("Usage: catalog_acceptance <new-output-directory>")?;
    ensure!(!output.exists(), "Use a fresh output directory");
    fs::create_dir_all(&output)?;
    let brand = create::sugata_brand_kit();
    let mut evidence = Vec::new();
    for template in create::templates() {
        let original = create::instantiate_template(template.id, Some(&brand))?;
        for (variant, width, height) in [
            ("original", original.width, original.height),
            ("square", 1080, 1080),
            ("story", 1080, 1920),
            ("wide", 1200, 630),
        ] {
            let mut artwork =
                create::resize_layout(&original, width, height, create::ResizeStrategy::Adapt)?;
            artwork.metadata["omuseContent"] = json!({
                "caption": format!("{} template qualification", template.name),
                "altText": format!("Editable {} design in the Sugata palette", template.name),
            });
            let name = format!("{}-{variant}", template.id);
            let package = output.join(format!("{name}.comp"));
            let text = live_text(&artwork.layers)?;
            ensure!(!text.is_empty(), "Template {name} has no native text");
            let expected = raster::composite(&artwork);
            ensure!(
                expected.dimensions() == (width, height),
                "Invalid rendered dimensions for {name}"
            );
            document::save(&artwork, &package)?;
            let reopened = document::open(&package)?;
            ensure!(
                serde_json::to_value(live_text(&reopened.layers)?)? == serde_json::to_value(text)?,
                "Native text changed after reopening {name}"
            );
            let rendered = raster::composite(&reopened);
            if rendered != expected {
                write_roundtrip_diagnostics(
                    &output, &name, &artwork, &reopened, &expected, &rendered,
                )?;
            }
            ensure!(
                rendered == expected,
                "Rendered pixels changed after reopening {name}; wrote expected, reopened, and round-trip diagnostics"
            );
            rendered.save(output.join(format!("{name}.png")))?;
            let mut findings = Vec::new();
            social_preview::inspect_document(
                &reopened,
                &name,
                social_preview::SafeAreaPreset::CanvasMargin,
                &mut findings,
            )?;
            evidence.push(json!({"template":template.id,"variant":variant,"dimensions":[width,height],"nativeTextAndPixelsPreserved":true,"preflight":findings}));
        }
    }
    ensure!(
        evidence.len() == 80,
        "Expected all twenty templates at four sizes"
    );
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&json!({"status":"passed","variants":evidence}))?,
    )?;
    println!(
        "Validated 80 editable template variants: {}",
        output.display()
    );
    Ok(())
}
