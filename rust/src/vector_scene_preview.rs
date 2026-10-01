//! Pure settled previews for editable vector-scene creation and replacement.

use crate::{
    document::validate_vector_scene_budget,
    model::{Document, Layer, MAX_LAYERS, MAX_PIXELS},
    vector_scene::VectorScene,
};
use anyhow::{Result, ensure};
use image::RgbaImage;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Render `artwork` and composite it through the ordinary document renderer
/// without mutating the source document or editor history.
///
/// `None` previews the scene as a new top-level layer above existing artwork.
/// `Some(id)` replaces only the cache and geometry of an existing vector-scene
/// layer, preserving its placement, mask, effects, opacity, blend and tree slot.
pub fn composite(
    document: &Document,
    layer_id: Option<&str>,
    artwork: &VectorScene,
    cancel: &AtomicBool,
) -> Result<RgbaImage> {
    check_cancel(cancel)?;
    let cache = artwork.render(cancel)?;
    check_cancel(cancel)?;

    let mut candidate = document.clone();
    match layer_id {
        None => {
            ensure!(
                tree_count(&candidate.layers) < MAX_LAYERS,
                "Vector scene preview exceeds the document layer limit"
            );
            let mut layer = Layer::group("Vector artwork");
            layer.metadata = serde_json::json!({});
            layer.image = Some(cache.into());
            layer.vector_scene = Some(Arc::new(artwork.clone()));
            candidate.layers.push(layer);
        }
        Some(id) => {
            let existing = document
                .find_layer(id)
                .ok_or_else(|| anyhow::anyhow!("Vector scene preview layer not found"))?;
            ensure!(
                !existing.is_group()
                    && existing.vector_scene.is_some()
                    && existing.advanced.is_none()
                    && crate::objects::live_text(existing)?.is_none()
                    && crate::objects::live_shape(existing)?.is_none(),
                "Vector scene preview target is not an editable vector scene"
            );
            ensure!(
                existing
                    .image
                    .as_ref()
                    .is_some_and(|image| image.dimensions() == cache.dimensions()),
                "Vector scene preview must retain the existing cache dimensions"
            );
            let layer = candidate
                .find_layer_mut(id)
                .expect("preview target was found in source clone");
            layer.image = Some(cache.into());
            layer.vector_scene = Some(Arc::new(artwork.clone()));
        }
    }

    check_cancel(cancel)?;
    ensure!(
        tree_pixels(&candidate.layers) <= MAX_PIXELS,
        "Vector scene preview exceeds the document pixel limit"
    );
    validate_vector_scene_budget(&candidate.layers)?;
    let errors = crate::raster::validate(&candidate);
    ensure!(
        errors.is_empty(),
        "Invalid vector scene preview: {}",
        errors.join("; ")
    );
    check_cancel(cancel)?;

    let preview = crate::raster::composite(&candidate);
    check_cancel(cancel)?;
    ensure!(
        preview.dimensions() == (candidate.width, candidate.height),
        "Vector scene preview could not be composited"
    );
    Ok(preview)
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "vector scene preview cancelled"
    );
    Ok(())
}

fn tree_count(layers: &[Layer]) -> usize {
    layers.iter().fold(0usize, |total, layer| {
        total
            .saturating_add(1)
            .saturating_add(tree_count(&layer.children))
    })
}

fn tree_pixels(layers: &[Layer]) -> u64 {
    layers.iter().fold(0u64, |total, layer| {
        total
            .saturating_add(layer.image.as_ref().map_or(0, |image| {
                u64::from(image.width()) * u64::from(image.height())
            }))
            .saturating_add(
                layer
                    .mask
                    .as_ref()
                    .map_or(0, |mask| u64::from(mask.width()) * u64::from(mask.height())),
            )
            .saturating_add(tree_pixels(&layer.children))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::Editor, model::Document, vector_scene::VectorObject};
    use image::{Rgba, RgbaImage};

    fn scene(width: u32, height: u32, color: [u8; 4]) -> VectorScene {
        VectorScene {
            version: crate::vector_scene::VECTOR_SCENE_VERSION,
            width,
            height,
            objects: vec![
                VectorObject::rectangle(
                    "Artwork",
                    0.,
                    0.,
                    width as f32,
                    height as f32,
                    Some(color),
                    None,
                )
                .unwrap(),
            ],
        }
    }

    #[test]
    fn new_scene_matches_committed_top_level_composite_over_photo() {
        let mut document = Document::new(8, 6);
        let photo =
            RgbaImage::from_fn(8, 6, |x, y| Rgba([(x * 21) as u8, (y * 31) as u8, 90, 255]));
        document.layers[0].image = Some(photo.into());
        let artwork = scene(5, 4, [230, 40, 30, 180]);
        let cancel = AtomicBool::new(false);

        let preview = composite(&document, None, &artwork, &cancel).unwrap();
        let mut committed = Editor::new(document.clone());
        committed
            .insert_vector_scene(
                "Vector artwork",
                committed.revision(),
                artwork.clone(),
                artwork.render(&cancel).unwrap(),
            )
            .unwrap();
        assert_eq!(preview, crate::raster::composite(&committed.document));
        assert_eq!(document.layers.len(), 1);
    }

    #[test]
    fn replacement_matches_commit_with_transform_mask_opacity_and_top_occlusion() {
        let cancel = AtomicBool::new(false);
        let original = scene(6, 5, [30, 190, 80, 255]);
        let replacement = scene(6, 5, [220, 70, 35, 230]);
        let mut editor = Editor::new(Document::new(14, 10));
        let id = editor
            .insert_vector_scene(
                "Scene",
                editor.revision(),
                original.clone(),
                original.render(&cancel).unwrap(),
            )
            .unwrap();
        {
            let layer = editor.document.find_layer_mut(&id).unwrap();
            layer.offset_x = 2.5;
            layer.offset_y = 1.25;
            layer.rotation = 13.;
            layer.scale_x = 1.2;
            layer.scale_y = 1.2;
            layer.opacity = 0.65;
            layer.mask = Some(
                RgbaImage::from_fn(6, 5, |x, _| {
                    let coverage = if x < 4 { 255 } else { 0 };
                    Rgba([coverage, coverage, coverage, 255])
                })
                .into(),
            );
            layer.metadata["maskEnabled"] = serde_json::json!(true);
            layer.metadata["maskLinked"] = serde_json::json!(true);
            layer.metadata["effects"] = serde_json::json!({
                "colorOverlay": {
                    "red": 0.1,
                    "green": 0.2,
                    "blue": 0.8,
                    "opacity": 0.25
                }
            });
        }
        let position = editor
            .document
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .unwrap();
        let scene_layer = editor.document.layers.remove(position);
        let mut group = Layer::group("Scene group");
        group.children.push(scene_layer);
        editor.document.layers.push(group);
        let mut top = Layer::paint("Occlusion", 4, 3);
        top.offset_x = 5.;
        top.offset_y = 3.;
        top.image = Some(RgbaImage::from_pixel(4, 3, Rgba([25, 35, 210, 255])).into());
        editor.document.layers.push(top);
        let source = editor.document.clone();

        let preview = composite(&source, Some(&id), &replacement, &cancel).unwrap();
        let mut committed = Editor::new(source.clone());
        committed
            .replace_vector_scene(
                &id,
                committed.revision(),
                replacement.clone(),
                replacement.render(&cancel).unwrap(),
            )
            .unwrap();
        assert_eq!(preview, crate::raster::composite(&committed.document));
        assert_eq!(preview.get_pixel(6, 4).0, [25, 35, 210, 255]);
        assert_eq!(
            source.find_layer(&id).unwrap().vector_scene.as_deref(),
            Some(&original)
        );
        assert_eq!(source.layers[source.layers.len() - 2].children[0].id, id);
        assert_eq!(source.layers.last().unwrap().name, "Occlusion");
    }

    #[test]
    fn cancellation_and_invalid_targets_leave_the_source_untouched() {
        let cancel = AtomicBool::new(false);
        let original = scene(4, 4, [15, 25, 35, 255]);
        let replacement = scene(4, 4, [200, 100, 50, 255]);
        let mut editor = Editor::new(Document::new(8, 8));
        let id = editor
            .insert_vector_scene(
                "Scene",
                editor.revision(),
                original.clone(),
                original.render(&cancel).unwrap(),
            )
            .unwrap();
        let before_cache = editor.document.find_layer(&id).unwrap().image.clone();
        let before_layers = editor.document.layers.len();

        assert!(composite(&editor.document, Some("missing"), &replacement, &cancel).is_err());
        let raster_id = editor.document.layers[0].id.clone();
        assert!(composite(&editor.document, Some(&raster_id), &replacement, &cancel).is_err());
        let cancelled = AtomicBool::new(true);
        assert!(
            composite(&editor.document, Some(&id), &replacement, &cancelled)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );

        let layer = editor.document.find_layer(&id).unwrap();
        assert_eq!(layer.vector_scene.as_deref(), Some(&original));
        assert_eq!(layer.image, before_cache);
        assert_eq!(editor.document.layers.len(), before_layers);
    }
}
