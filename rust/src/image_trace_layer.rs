//! Prepare a trace next to its retained bitmap, in the same layer-tree slot.
//! Preview and publication use the same validated document so placement,
//! surrounding layers, masks and effects cannot diverge between them.
use crate::{
    image_trace::{TraceOptions, TraceResult, TraceStats},
    model::{Document, Layer},
    shared_image::SharedImage,
};
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const TRACE_METADATA: &str = "omuseImageTrace";

pub struct PreparedTrace {
    pub(crate) document: Document,
    pub(crate) revision: u64,
    pub(crate) instance: u64,
    pub layer_id: String,
    pub stats: TraceStats,
    pub preview: RgbaImage,
    pub source_preview: RgbaImage,
}

/// Retained source pixels are never modified or detached during tracing.
pub fn source(document: &Document, id: &str) -> Result<SharedImage> {
    let layer = document
        .find_layer(id)
        .context("The original image layer is no longer available")?;
    ensure!(
        !layer.is_group()
            && layer.vector_scene.is_none()
            && crate::objects::live_text(layer)?.is_none()
            && crate::objects::live_shape(layer)?.is_none(),
        "Choose a pixel image layer to trace"
    );
    let pixels = layer
        .image
        .as_ref()
        .context("Choose an image layer to trace")?;
    ensure!(
        u64::from(pixels.width()) * u64::from(pixels.height())
            <= crate::vector_scene::MAX_SCENE_PIXELS,
        "Trace supports source images up to 16 megapixels; resize a copy before tracing this image"
    );
    Ok(pixels.clone())
}

/// A trace can be regenerated from its original image after save/reopen.
pub fn retained_settings(layer: &Layer) -> Option<(String, TraceOptions)> {
    if layer.vector_scene.is_none() {
        return None;
    }
    let value = layer.metadata.get(TRACE_METADATA)?;
    let id = value.get("sourceLayerID")?.as_str()?.to_owned();
    let options = serde_json::from_value(value.get("options")?.clone()).ok()?;
    Some((id, options))
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Image trace cancelled");
    Ok(())
}

fn locked(layers: &[Layer], id: &str, inherited: bool) -> Option<bool> {
    for layer in layers {
        let blocked = inherited || layer.locked;
        if layer.id == id {
            return Some(blocked);
        }
        if let Some(value) = locked(&layer.children, id, blocked) {
            return Some(value);
        }
    }
    None
}

/// Check the layer that would be changed before doing expensive trace work.
/// Retracing may read a locked original, but never modifies a locked target.
pub fn validate_target(document: &Document, id: &str) -> Result<()> {
    ensure!(
        locked(&document.layers, id, false) == Some(false),
        "Unlock this layer and its parent groups before tracing"
    );
    Ok(())
}

fn insert_above(layers: &mut Vec<Layer>, source: &str, layer: &mut Option<Layer>) -> bool {
    for index in 0..layers.len() {
        if layers[index].id == source {
            layers.insert(index + 1, layer.take().expect("insert trace once"));
            return true;
        }
        if insert_above(&mut layers[index].children, source, layer) {
            return true;
        }
    }
    false
}

/// All expensive work is suitable for a background thread. The source image is
/// kept; a new trace hides it without deleting it. Retrace replaces only the
/// existing trace geometry/cache and retains that layer's later styling.
pub fn prepare(
    document: &Document,
    source_id: &str,
    target_id: Option<&str>,
    mut trace: TraceResult,
    options: &TraceOptions,
    revision: u64,
    instance: u64,
    cancel: &AtomicBool,
) -> Result<PreparedTrace> {
    check_cancel(cancel)?;
    options.validate()?;
    let source_pixels = source(document, source_id)?;
    validate_target(document, target_id.unwrap_or(source_id))?;
    let original = document
        .find_layer(source_id)
        .context("Original image not found")?;
    trace.scene.validate()?;
    ensure!(
        !trace.scene.objects.is_empty(),
        "No artwork remained; adjust the trace settings"
    );
    ensure!(
        trace.scene.objects.iter().all(|o| o.stroke.is_none()),
        "Trace output must use filled paths"
    );
    // Analyse at the requested working resolution but keep the source-sized
    // cache so existing effect radii and masks retain their exact coordinate system.
    let sx = source_pixels.width() as f32 / trace.scene.width as f32;
    let sy = source_pixels.height() as f32 / trace.scene.height as f32;
    for object in &mut trace.scene.objects {
        let [a, b, c, d, e, f] = object.transform;
        object.transform = [a * sx, b * sy, c * sx, d * sy, e * sx, f * sy];
    }
    trace.scene.width = source_pixels.width();
    trace.scene.height = source_pixels.height();
    trace.scene.validate()?;
    let cache = trace.scene.render(cancel)?;
    check_cancel(cancel)?;
    let mut candidate = document.clone();
    let mut result_layer = if let Some(id) = target_id {
        let target = document
            .find_layer(id)
            .context("The vector trace layer is no longer available")?;
        ensure!(
            retained_settings(target).is_some_and(|(source, _)| source == source_id),
            "This vector layer no longer belongs to the original image"
        );
        ensure!(
            target
                .image
                .as_ref()
                .is_some_and(|image| image.dimensions() == cache.dimensions()),
            "Original image size changed; create a new trace to retain placement"
        );
        target.clone()
    } else {
        let mut layer = original.clone();
        layer.id = uuid::Uuid::new_v4().to_string().to_uppercase();
        layer.name = format!("{} · Vector", original.name);
        layer
    };
    result_layer.advanced = None;
    result_layer.image = Some(cache.into());
    result_layer.vector_scene = Some(Arc::new(trace.scene));
    result_layer.visible = true;
    result_layer.locked = false;
    let blend_if = crate::advanced::layer_blend_if(original)?;
    if !result_layer.metadata.is_object() {
        result_layer.metadata = serde_json::json!({});
    }
    if let Some(metadata) = result_layer.metadata.as_object_mut() {
        metadata.remove("rustEditableAsset");
        metadata.remove("rustVectorScene");
        if target_id.is_none() {
            if let Some(settings) = blend_if {
                metadata.insert(
                    crate::advanced::RASTER_BLEND_IF_KEY.into(),
                    serde_json::to_value(settings)?,
                );
            }
        }
        metadata.insert(
            TRACE_METADATA.into(),
            serde_json::json!({"version":1, "sourceLayerID":source_id, "options":options}),
        );
    }
    let layer_id = result_layer.id.clone();
    if let Some(id) = target_id {
        *candidate
            .find_layer_mut(id)
            .context("Trace layer disappeared")? = result_layer;
    } else {
        ensure!(
            insert_above(&mut candidate.layers, source_id, &mut Some(result_layer)),
            "Could not place traced artwork"
        );
        candidate
            .find_layer_mut(source_id)
            .expect("source was found")
            .visible = false;
    }
    fn budget(layers: &[Layer]) -> (usize, u64) {
        layers
            .iter()
            .fold((0usize, 0u64), |(count, pixels), layer| {
                let (children, child_pixels) = budget(&layer.children);
                let own_pixels = [layer.image.as_ref(), layer.mask.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|image| u64::from(image.width()) * u64::from(image.height()))
                    .sum::<u64>();
                (
                    count.saturating_add(1).saturating_add(children),
                    pixels
                        .saturating_add(own_pixels)
                        .saturating_add(child_pixels),
                )
            })
    }
    let (layers, pixels) = budget(&candidate.layers);
    ensure!(
        layers <= crate::model::MAX_LAYERS && pixels <= crate::model::MAX_PIXELS,
        "Retaining the source and vector cache would exceed the project layer or pixel budget"
    );
    crate::document::validate_vector_scene_budget(&candidate.layers)?;
    crate::advanced::validate_document_budget(&candidate)?;
    let errors = crate::raster::validate(&candidate);
    ensure!(
        errors.is_empty(),
        "Cannot keep trace: {}",
        errors.join("; ")
    );
    check_cancel(cancel)?;
    let preview = crate::raster::composite(&candidate);
    check_cancel(cancel)?;
    ensure!(
        preview.dimensions() == (document.width, document.height),
        "Trace preview exceeded rendering limits"
    );
    let mut comparison = document.clone();
    comparison
        .find_layer_mut(source_id)
        .expect("original was found")
        .visible = true;
    if let Some(id) = target_id {
        comparison
            .find_layer_mut(id)
            .expect("trace was found")
            .visible = false;
    }
    let source_preview = crate::raster::composite(&comparison);
    check_cancel(cancel)?;
    ensure!(
        source_preview.dimensions() == (document.width, document.height),
        "Source comparison exceeded rendering limits"
    );
    Ok(PreparedTrace {
        document: candidate,
        revision,
        instance,
        layer_id,
        stats: trace.stats,
        preview,
        source_preview,
    })
}
