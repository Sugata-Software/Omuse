//! Pure settled previews for editable vector-scene creation and replacement.

use crate::{
    document::validate_vector_scene_budget,
    model::{Document, Layer, MAX_LAYERS, MAX_PIXELS, valid_dimensions},
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

/// Render a transient, scale-aware preview. At scale 1 this is exactly the
/// settled renderer. At larger scales only documents made of plain raster and
/// vector-scene layers are admitted; masks, effects, groups, advanced recipes,
/// and non-Normal blending deliberately fall back to the settled renderer.
pub fn composite_at_scale(
    document: &Document,
    layer_id: Option<&str>,
    artwork: &VectorScene,
    requested_scale: f32,
    cancel: &AtomicBool,
) -> Result<RgbaImage> {
    ensure!(
        requested_scale.is_finite() && requested_scale >= 1.,
        "Preview scale must be at least 1"
    );
    check_cancel(cancel)?;
    if requested_scale <= 1. {
        return composite(document, layer_id, artwork, cancel);
    }
    let scale = requested_scale.ceil().clamp(2., 4.);
    let width = scaled_dimension(document.width, scale)?;
    let height = scaled_dimension(document.height, scale)?;
    if !valid_dimensions(width, height) || u64::from(width) * u64::from(height) > 16_000_000 {
        return composite(document, layer_id, artwork, cancel);
    }
    if !scale_preview_supported(document, layer_id) {
        return composite(document, layer_id, artwork, cancel);
    }

    let mut candidate = document.clone();
    candidate.width = width;
    candidate.height = height;
    candidate.background = document.background;
    let scaled_artwork = match scale_scene(artwork, scale) {
        Ok(scene) => scene,
        Err(_) => return composite(document, layer_id, artwork, cancel),
    };
    if !scaled_vector_budget_supported(document, layer_id, &scaled_artwork, scale) {
        return composite(document, layer_id, artwork, cancel);
    }
    let scaled_cache = match scaled_artwork.render(cancel) {
        Ok(cache) => cache,
        Err(error) if cancel.load(Ordering::Relaxed) => return Err(error),
        // The settled artwork can be valid while its supersampled preview
        // exceeds a geometry/work limit. Keep the visible settled preview.
        Err(_) => return composite(document, layer_id, artwork, cancel),
    };
    for layer in &mut candidate.layers {
        if layer_id.is_some_and(|id| id == layer.id) {
            continue;
        }
        if let Err(error) = scale_layer(layer, scale, cancel) {
            if cancel.load(Ordering::Relaxed) {
                return Err(error);
            }
            return composite(document, layer_id, artwork, cancel);
        }
    }
    match layer_id {
        None => {
            ensure!(
                tree_count(&candidate.layers) < MAX_LAYERS,
                "Vector scene preview exceeds the document layer limit"
            );
            let mut layer = Layer::group("Vector artwork");
            layer.metadata = serde_json::json!({});
            layer.image = Some(scaled_cache.into());
            layer.vector_scene = Some(Arc::new(scaled_artwork.clone()));
            candidate.layers.push(layer);
        }
        Some(id) => {
            let existing = document
                .find_layer(id)
                .ok_or_else(|| anyhow::anyhow!("Vector scene preview layer not found"))?;
            ensure!(
                existing.vector_scene.is_some()
                    && existing.advanced.is_none()
                    && existing.mask.is_none(),
                "Vector scene preview target is unsupported"
            );
            let layer = candidate
                .find_layer_mut(id)
                .expect("preview target was found in source clone");
            layer.offset_x *= scale;
            layer.offset_y *= scale;
            layer.image = Some(scaled_cache.into());
            layer.vector_scene = Some(Arc::new(scaled_artwork));
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
        "Invalid scaled vector scene preview: {}",
        errors.join("; ")
    );
    let preview = crate::raster::composite(&candidate);
    check_cancel(cancel)?;
    ensure!(
        preview.dimensions() == (width, height),
        "Scaled vector scene preview could not be composited"
    );
    Ok(preview)
}

fn scaled_dimension(value: u32, scale: f32) -> Result<u32> {
    let scaled = (f64::from(value) * f64::from(scale)).round();
    ensure!(
        scaled.is_finite() && scaled >= 1. && scaled <= f64::from(u32::MAX),
        "Scaled preview dimension is out of range"
    );
    Ok(scaled as u32)
}

fn scale_scene(source: &VectorScene, scale: f32) -> Result<VectorScene> {
    let mut scene = source.clone();
    scene.width = scaled_dimension(source.width, scale)?;
    scene.height = scaled_dimension(source.height, scale)?;
    for object in &mut scene.objects {
        for subpath in &mut object.path.subpaths {
            for anchor in &mut subpath.anchors {
                anchor.position.x *= scale;
                anchor.position.y *= scale;
                if let Some(point) = &mut anchor.incoming {
                    point.x *= scale;
                    point.y *= scale;
                }
                if let Some(point) = &mut anchor.outgoing {
                    point.x *= scale;
                    point.y *= scale;
                }
            }
        }
        object.transform[4] *= scale;
        object.transform[5] *= scale;
        if let Some(stroke) = &mut object.stroke {
            stroke.width *= scale;
        }
        // Path coordinates are scaled in local space above. Paints, dash
        // lengths and retained text layouts must use that same local space,
        // including when an object still has its own rotation or translation.
        let local_scale = [scale, 0., 0., scale, 0., 0.];
        if let Some(gradient) = &mut object.fill_gradient {
            gradient.bake_transform(local_scale);
        }
        if let Some(options) = &mut object.stroke_options {
            options.scale(scale);
        }
        if let Some(text) = &mut object.text_path {
            text.bake_transform(local_scale);
        }
    }
    scene.validate()?;
    Ok(scene)
}

fn scale_preview_supported(document: &Document, target: Option<&str>) -> bool {
    if target.is_some_and(|id| document.find_layer(id).is_none()) {
        return false;
    }
    fn supported(layer: &Layer) -> bool {
        layer.advanced.is_none()
            && layer.mask.is_none()
            && !layer.is_group()
            && layer.children.is_empty()
            && matches!(layer.blend_mode.as_str(), "Normal")
            && layer.metadata.as_object().is_some_and(|m| {
                m.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "id" | "name"
                            | "isVisible"
                            | "locked"
                            | "opacity"
                            | "blendMode"
                            | "parentID"
                            | "imageFile"
                            | "maskFile"
                            | "isGroup"
                            | "transform"
                            | "sourceColorProfile"
                    )
                })
            })
    }
    document.layers.iter().all(supported)
}

fn scaled_vector_budget_supported(
    document: &Document,
    target: Option<&str>,
    artwork: &VectorScene,
    scale: f32,
) -> bool {
    let mut pixels = u64::from(artwork.width) * u64::from(artwork.height);
    let mut layers = document.layers.clone();
    for layer in &mut layers {
        if target.is_some_and(|id| id == layer.id) {
            layer.vector_scene = None;
        }
    }
    let mut layer = Layer::group("Preview budget");
    layer.vector_scene = Some(Arc::new(artwork.clone()));
    layers.push(layer);
    if validate_vector_scene_budget(&layers).is_err() {
        return false;
    }
    for layer in &document.layers {
        if target.is_some_and(|id| id == layer.id) {
            continue;
        }
        let Some(scene) = layer.vector_scene.as_ref() else {
            pixels = pixels.saturating_add(layer.image.as_ref().map_or(0, |image| {
                u64::from(image.width()) * u64::from(image.height())
            }));
            continue;
        };
        let Ok(scaled) = scale_scene(scene, scale) else {
            return false;
        };
        pixels = pixels.saturating_add(u64::from(scaled.width) * u64::from(scaled.height));
    }
    pixels <= MAX_PIXELS
}

fn scale_layer(layer: &mut Layer, scale: f32, cancel: &AtomicBool) -> Result<()> {
    ensure!(
        layer.children.is_empty(),
        "Scaled preview does not support groups"
    );
    layer.offset_x *= scale;
    layer.offset_y *= scale;
    if let Some(scene) = layer.vector_scene.as_ref() {
        let scaled = scale_scene(scene, scale)?;
        layer.image = Some(scaled.render(cancel)?.into());
        layer.vector_scene = Some(Arc::new(scaled));
    } else if layer.image.is_some() {
        // Preserve source photo pixels; the larger transient document scales
        // their placement and transform rather than blurring an enlarged copy.
        layer.scale_x *= scale;
        layer.scale_y *= scale;
    }
    Ok(())
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
    fn scaled_preview_preserves_gradient_dash_and_retained_text_coordinates() {
        use crate::{
            model::PixelRect,
            vector_path::{Anchor, Point, StrokeStyle, Subpath, VectorPath},
            vector_scene::{
                GradientFill, GradientKind, GradientSpread, GradientStop, StrokeOptions,
                TextOnPath, TextPathAlignment, text,
            },
        };
        let cancel = AtomicBool::new(false);
        let mut object = VectorObject::rectangle(
            "Styled",
            12.,
            12.,
            75.,
            40.,
            Some([240, 140, 60, 255]),
            Some(StrokeStyle {
                color: [20, 60, 90, 255],
                width: 3.,
            }),
        )
        .unwrap();
        object.transform = [0.96, 0.28, -0.28, 0.96, 12., 1.];
        object.fill_gradient = Some(GradientFill {
            kind: GradientKind::Linear {
                start: Point { x: 12., y: 12. },
                end: Point { x: 87., y: 52. },
            },
            stops: vec![
                GradientStop {
                    offset: 0.,
                    color: [240, 140, 60, 150],
                },
                GradientStop {
                    offset: 1.,
                    color: [30, 90, 130, 255],
                },
            ],
            spread: GradientSpread::Pad,
            transform: [1., 0., 0.2, 1., -2., 3.],
        });
        object.stroke_options = Some(StrokeOptions {
            dashes: vec![6., 4.],
            dash_offset: 2.,
            ..Default::default()
        });
        let recipe = TextOnPath {
            text: "Omuse".into(),
            font_family: "Outfit".into(),
            font_size: 16.,
            letter_spacing: 1.,
            start_offset: 0.5,
            alignment: TextPathAlignment::Center,
            guide: VectorPath {
                fill_rule: Default::default(),
                subpaths: vec![Subpath {
                    closed: false,
                    anchors: vec![
                        Anchor {
                            position: Point { x: 10., y: 85. },
                            incoming: None,
                            outgoing: None,
                        },
                        Anchor {
                            position: Point { x: 150., y: 85. },
                            incoming: None,
                            outgoing: None,
                        },
                    ],
                }],
            },
            transform: [1., 0., 0., 1., 0., 0.],
            resolved_fonts: Vec::new(),
        };
        let type_object = text::update_object(
            &VectorObject::new(
                "Text",
                VectorPath::default(),
                Some([240, 220, 150, 255]),
                None,
            ),
            &recipe,
            &cancel,
        )
        .unwrap();
        let source = VectorScene {
            version: 4,
            width: 160,
            height: 100,
            objects: vec![object, type_object],
        };
        let scaled = scale_scene(&source, 3.).unwrap();
        assert_eq!(
            scaled.objects[0].stroke_options.as_ref().unwrap().dashes,
            [18., 12.]
        );
        assert_eq!(
            scaled.objects[0]
                .stroke_options
                .as_ref()
                .unwrap()
                .dash_offset,
            6.
        );
        let (_, reflowed) =
            text::shape(scaled.objects[1].text_path.as_ref().unwrap(), &cancel).unwrap();
        assert_eq!(reflowed, scaled.objects[1].path);
        let expected = source
            .render_region(
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 160,
                    height: 100,
                },
                3.,
                &cancel,
            )
            .unwrap();
        let actual = scaled.render(&cancel).unwrap();
        let differing = actual
            .pixels()
            .zip(expected.pixels())
            .filter(|(a, b)| a.0.iter().zip(b.0).any(|(a, b)| a.abs_diff(b) > 3))
            .count();
        assert!(
            differing < 30,
            "Scaled preview changed paints or dashes at {differing} pixels"
        );
    }

    #[test]
    fn high_resolution_work_budget_falls_back_to_settled_artwork() {
        let mut artwork = crate::vector_svg_scene::decode_scene(br##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480"><ellipse cx="320" cy="240" rx="192" ry="144" fill="#D58049" stroke="#547A8E" stroke-width="4"/></svg>"##).unwrap();
        artwork.objects[0].stroke_options = None;
        let cancel = AtomicBool::new(false);
        let too_large = scale_scene(&artwork, 4.).unwrap();
        assert!(
            too_large
                .render(&cancel)
                .unwrap_err()
                .to_string()
                .contains("work limit")
        );
        let preview =
            composite_at_scale(&Document::new(640, 480), None, &artwork, 4., &cancel).unwrap();
        assert_eq!(preview.dimensions(), (640, 480));
        assert!(preview.pixels().any(|p| p[0] > 180 && p[1] < 160));
        assert!(
            composite_at_scale(
                &Document::new(640, 480),
                None,
                &artwork,
                4.,
                &AtomicBool::new(true)
            )
            .is_err()
        );
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
