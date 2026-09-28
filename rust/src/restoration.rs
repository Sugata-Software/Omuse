//! Local photo restoration drafts. Originals remain embedded as hidden layers;
//! interpolation is explicitly distinguished from generated or recovered detail.
use crate::{
    advanced::{Component, LayerState},
    advanced_ops::{self, AdvancedOperation, FilterNode},
    create,
    filters::Filter,
    model::{Document, Layer},
    objects,
    precision::{Rgba16, TiledImage16},
    raster,
};
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const MAX_PIXELS: u64 = 16_000_000;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub denoise: f32,
    pub sharpen: f32,
    pub scale: u32,
    pub enlarge_page: bool,
}

/// A prepared document remains separate from the live editor until the user
/// accepts it. The retained ID lets the UI select the visible derivative while
/// editor undo restores the source selection captured by its transaction.
pub struct Prepared {
    pub document: Document,
    pub restored_layer_id: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            denoise: 0.35,
            sharpen: 0.25,
            scale: 1,
            enlarge_page: false,
        }
    }
}
impl Settings {
    fn validate(self) -> Result<()> {
        ensure!(
            self.denoise.is_finite() && (0.0..=1.0).contains(&self.denoise),
            "Denoise must be between 0 and 1"
        );
        ensure!(
            self.sharpen.is_finite() && (0.0..=2.0).contains(&self.sharpen),
            "Sharpen must be between 0 and 2"
        );
        ensure!(
            matches!(self.scale, 1 | 2 | 4),
            "Choose 1×, 2× or 4× resolution"
        );
        Ok(())
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Restoration cancelled");
    Ok(())
}
fn dimensions(width: u32, height: u32, scale: u32) -> Result<(u32, u32)> {
    let width = width.checked_mul(scale).context("Image width overflow")?;
    let height = height.checked_mul(scale).context("Image height overflow")?;
    ensure!(
        crate::model::valid_dimensions(width, height)
            && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Restoration output is limited to 16 million pixels"
    );
    Ok((width, height))
}

fn restoration_nodes(settings: Settings) -> Vec<FilterNode> {
    let mut nodes = Vec::new();
    let mut push = |name: &str, operation| {
        nodes.push(FilterNode {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            enabled: true,
            opacity: 1.,
            operation,
            soft_mask: None,
        });
    };
    if settings.denoise > 0. {
        push(
            "Local denoise",
            AdvancedOperation::Denoise {
                radius: 2,
                strength: settings.denoise,
            },
        );
    }
    if settings.sharpen > 0. {
        push(
            "Gentle sharpening",
            AdvancedOperation::Filter(Filter::UnsharpMask {
                sigma: 1.,
                amount: settings.sharpen,
                threshold: 0.02,
            }),
        );
    }
    nodes
}

/// Bilinear interpolation of premultiplied channels. It is bounded, cancellable
/// between rows and does not pull invisible RGB colours into cutout edges.
fn enlarge(source: &RgbaImage, width: u32, height: u32, cancel: &AtomicBool) -> Result<RgbaImage> {
    let mut output = RgbaImage::new(width, height);
    for y in 0..height {
        check_cancel(cancel)?;
        let sy = ((y as f64 + 0.5) * source.height() as f64 / height as f64 - 0.5)
            .clamp(0., source.height().saturating_sub(1) as f64);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(source.height() - 1);
        let dy = sy - y0 as f64;
        for x in 0..width {
            let sx = ((x as f64 + 0.5) * source.width() as f64 / width as f64 - 0.5)
                .clamp(0., source.width().saturating_sub(1) as f64);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(source.width() - 1);
            let dx = sx - x0 as f64;
            let mut accum = [0.; 4];
            for (xx, yy, weight) in [
                (x0, y0, (1. - dx) * (1. - dy)),
                (x1, y0, dx * (1. - dy)),
                (x0, y1, (1. - dx) * dy),
                (x1, y1, dx * dy),
            ] {
                let pixel = source.get_pixel(xx, yy);
                let alpha = pixel[3] as f64 / 255.;
                for c in 0..3 {
                    accum[c] += pixel[c] as f64 * alpha * weight;
                }
                accum[3] += alpha * weight;
            }
            let mut pixel = [0; 4];
            if accum[3] > 0. {
                for c in 0..3 {
                    pixel[c] = (accum[c] / accum[3]).round().clamp(0., 255.) as u8;
                }
            }
            pixel[3] = (accum[3] * 255.).round().clamp(0., 255.) as u8;
            output.put_pixel(x, y, Rgba(pixel));
        }
    }
    Ok(output)
}

/// The editable-source path must retain all 16-bit samples. Just like the
/// byte path above, weights are applied to premultiplied channels so invisible
/// edge colours cannot contaminate translucent output.
fn enlarge16(
    source: &TiledImage16,
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<TiledImage16> {
    let mut output = TiledImage16::new(width, height, source.working_space())?;
    for y in 0..height {
        check_cancel(cancel)?;
        let sy = ((y as f64 + 0.5) * source.height() as f64 / height as f64 - 0.5)
            .clamp(0., source.height().saturating_sub(1) as f64);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(source.height() - 1);
        let dy = sy - y0 as f64;
        for x in 0..width {
            let sx = ((x as f64 + 0.5) * source.width() as f64 / width as f64 - 0.5)
                .clamp(0., source.width().saturating_sub(1) as f64);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(source.width() - 1);
            let dx = sx - x0 as f64;
            let mut accum = [0.; 4];
            for (xx, yy, weight) in [
                (x0, y0, (1. - dx) * (1. - dy)),
                (x1, y0, dx * (1. - dy)),
                (x0, y1, (1. - dx) * dy),
                (x1, y1, dx * dy),
            ] {
                let pixel = source.get_pixel(xx, yy).0;
                let alpha = pixel[3] as f64 / 65_535.;
                for channel in 0..3 {
                    accum[channel] += pixel[channel] as f64 * alpha * weight;
                }
                accum[3] += alpha * weight;
            }
            let mut pixel = [0; 4];
            if accum[3] > 0. {
                for channel in 0..3 {
                    pixel[channel] = (accum[channel] / accum[3]).round().clamp(0., 65_535.) as u16;
                }
            }
            pixel[3] = (accum[3] * 65_535.).round().clamp(0., 65_535.) as u16;
            output.set_pixel(x, y, Rgba16(pixel))?;
        }
    }
    Ok(output)
}

pub fn prepare(
    source: &Document,
    layer_id: &str,
    settings: Settings,
    cancel: &AtomicBool,
) -> Result<Prepared> {
    settings.validate()?;
    check_cancel(cancel)?;
    dimensions(source.width, source.height, 1)?;
    ensure!(
        raster::validate(source).is_empty(),
        "The source document cannot be rendered safely"
    );
    fn eligible<'a>(layers: &'a [Layer], id: &str, protected: bool) -> Result<Option<&'a Layer>> {
        for layer in layers {
            let protected = protected || layer.locked || !layer.visible;
            if layer.id == id {
                ensure!(!protected, "Select a visible unlocked image");
                return Ok(Some(layer));
            }
            if let Some(found) = eligible(&layer.children, id, protected)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }
    let original =
        eligible(&source.layers, layer_id, false)?.context("Select an image to restore")?;
    ensure!(
        !original.is_group()
            && objects::live_text(original)?.is_none()
            && objects::live_shape(original)?.is_none()
            && create::frame_spec(original)?.is_none(),
        "Restore a photo layer; native text, shapes and frames keep their own editing controls"
    );
    ensure!(
        original
            .advanced
            .as_ref()
            .is_none_or(|state| state.recipe.vector.is_none()),
        "Restore vector-masked sources in the editable filter stack"
    );
    let image = original
        .image
        .as_ref()
        .context("The selected layer has no pixels")?;
    let (width, height) = dimensions(image.width(), image.height(), settings.scale)?;
    if settings.enlarge_page {
        dimensions(source.width, source.height, settings.scale)?;
    }
    enum RestoredPixels {
        Raster(RgbaImage),
        Advanced {
            state: Arc<LayerState>,
            proxy: RgbaImage,
        },
    }

    let nodes = restoration_nodes(settings);
    let restored_pixels = if let Some(original_state) = &original.advanced {
        // Editable sources own 16-bit source/result tiles. Clone that state so
        // the hidden source is never touched, append the typed operations, and
        // run the established high-precision evaluator rather than its 8-bit
        // display proxy.
        let mut state = (**original_state).clone();
        if !nodes.is_empty() {
            state.recipe.nodes.extend(nodes);
            state = state.evaluate(cancel)?;
        }
        // The derived layer owns its retained 16-bit master. It must not claim
        // the source photo's external/RAW link or duplicate potentially large
        // RAW bytes; those remain solely with the hidden original layer.
        state.raw_bytes = None;
        state.recipe.source_id = uuid::Uuid::new_v4().to_string();
        state.recipe.source_name = format!("{} · restored", original.name);
        state.recipe.linked_path = None;
        state.recipe.raw_extension = None;
        state.recipe.raw_settings = None;
        if settings.scale > 1 {
            let enlarged = Arc::new(enlarge16(&state.result, width, height, cancel)?);
            // Enlargement is a baked derivative: retain the exact enlarged
            // master, but not external/raw/vector recipe links that no longer
            // describe those pixels. Blend If stays as ordinary layer metadata
            // below, so its compositing meaning is preserved.
            state.source = enlarged.clone();
            state.result = enlarged;
            state.recipe.nodes.clear();
            state.recipe.blend_if = None;
            state.recipe.vector = None;
            state.recipe.vector_is_mask = false;
            state.recipe.vector_fill = [0, 0, 0, 255];
            state.recipe.vector_stroke = None;
            state.recipe.component = Component::Image;
        }
        state.validate()?;
        let proxy = state.proxy()?;
        RestoredPixels::Advanced {
            state: Arc::new(state),
            proxy,
        }
    } else {
        let mut pixels = advanced_ops::evaluate_cancellable(image, &nodes, cancel)?;
        if settings.scale > 1 {
            pixels = enlarge(&pixels, width, height, cancel)?;
        }
        RestoredPixels::Raster(pixels)
    };
    check_cancel(cancel)?;
    let mut result = source.clone();
    if settings.enlarge_page && settings.scale > 1 {
        let (w, h) = dimensions(source.width, source.height, settings.scale)?;
        create::resize_layout_in_place(&mut result, w, h, create::ResizeStrategy::ScaleToFit)?;
    }
    fn insert(
        layers: &mut Vec<Layer>,
        id: &str,
        pixels: &mut Option<RestoredPixels>,
        settings: Settings,
    ) -> Result<Option<String>> {
        for index in 0..layers.len() {
            if layers[index].id == id {
                let mut restored = layers[index].clone();
                restored.id = uuid::Uuid::new_v4().to_string().to_uppercase();
                restored.name = format!("{} · restored", restored.name);
                if let Some(blend) = crate::advanced::layer_blend_if(&restored)? {
                    restored.metadata[crate::advanced::RASTER_BLEND_IF_KEY] =
                        serde_json::to_value(blend)?;
                }
                match pixels.take().context("Restoration pixels were consumed")? {
                    RestoredPixels::Raster(pixels) => {
                        restored.image = Some(pixels.into());
                        restored.advanced = None;
                    }
                    RestoredPixels::Advanced { state, proxy } => {
                        restored.image = Some(proxy.into());
                        restored.advanced = Some(state);
                    }
                }
                restored.scale_x /= settings.scale as f32;
                restored.scale_y /= settings.scale as f32;
                restored.metadata["omuseRestoration"] = serde_json::json!({"version":1,"sourceLayerId":id,"settings":settings,"engine":"local-denoise-unsharp-premultiplied-bilinear","generatedDetail":false});
                layers[index].visible = false;
                let restored_id = restored.id.clone();
                layers.insert(index + 1, restored);
                return Ok(Some(restored_id));
            }
            if let Some(restored_id) = insert(&mut layers[index].children, id, pixels, settings)? {
                return Ok(Some(restored_id));
            }
        }
        Ok(None)
    }
    let restored_layer_id = insert(
        &mut result.layers,
        layer_id,
        &mut Some(restored_pixels),
        settings,
    )?
    .context("Source layer disappeared")?;
    check_cancel(cancel)?;
    ensure!(
        raster::validate(&result).is_empty(),
        "Restored artwork exceeds the document rendering limits"
    );
    Ok(Prepared {
        document: result,
        restored_layer_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enlargement_preserves_original_pixels_native_text_and_one_undo() {
        let mut source = Document::new(16, 12);
        source.layers[0].image =
            Some(RgbaImage::from_pixel(16, 12, Rgba([100, 50, 30, 255])).into());
        let id = source.layers[0].id.clone();
        let mut text = Layer::paint("Copy", 1, 1);
        objects::set_live_text(
            &mut text,
            objects::LiveTextStyle {
                content: "Native".into(),
                font_size: 8.,
                ..Default::default()
            },
        )
        .unwrap();
        let text_id = text.id.clone();
        source.layers.push(text);
        let original = source.find_layer(&id).unwrap().image.clone();
        let prepared = prepare(
            &source,
            &id,
            Settings {
                denoise: 0.,
                sharpen: 0.,
                scale: 2,
                enlarge_page: true,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let restored_id = prepared.restored_layer_id.clone();
        let result = prepared.document;
        assert_eq!((result.width, result.height), (32, 24));
        assert_eq!(result.find_layer(&id).unwrap().image, original);
        assert!(!result.find_layer(&id).unwrap().visible);
        assert_eq!(
            objects::live_text(result.find_layer(&text_id).unwrap())
                .unwrap()
                .unwrap()
                .content,
            "Native"
        );
        assert_eq!(
            result.layers[1].image.as_ref().unwrap().dimensions(),
            (32, 24)
        );
        let mut editor = crate::editor::Editor::new(source.clone());
        let original_active = editor.active_layer.clone();
        editor.replace_document_transaction(result).unwrap();
        assert!(editor.select_layer(&restored_id));
        assert!(editor.undo());
        assert_eq!(editor.active_layer, original_active);
        assert!(crate::create_history::documents_match(
            &editor.document,
            &source
        ));
    }
    #[test]
    fn transparent_colour_does_not_bleed_and_cancel_has_no_result() {
        let image = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 0])
            }
        });
        let scaled = enlarge(&image, 8, 4, &AtomicBool::new(false)).unwrap();
        assert!(
            scaled
                .pixels()
                .all(|p| p[3] == 0 || (p[0] == 255 && p[1] == 0))
        );
        let source = Document::new(8, 8);
        assert!(
            prepare(
                &source,
                &source.layers[0].id,
                Settings::default(),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(dimensions(3000, 2000, 2).is_err());
    }
    #[test]
    fn parent_lock_and_native_objects_are_protected() {
        let mut source = Document::new(8, 8);
        let id = source.layers[0].id.clone();
        let mut group = Layer::group("Protected");
        group.locked = true;
        group.children = source.layers;
        source.layers = vec![group];
        assert!(prepare(&source, &id, Settings::default(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn advanced_no_op_restoration_retains_16_bit_gradient_and_survives_reopen() {
        let mut source = Document::new(3, 1);
        let master = image::ImageBuffer::from_fn(3, 1, |x, _| {
            Rgba([
                16_001 + x as u16,
                32_003 + x as u16,
                48_005 + x as u16,
                65_535,
            ])
        });
        let mut state = LayerState::from_image(&RgbaImage::new(3, 1), "High precision").unwrap();
        let master = Arc::new(TiledImage16::from_rgba16(&master).unwrap());
        state.source = master.clone();
        state.result = master.clone();
        source.layers[0].image = Some(state.proxy().unwrap().into());
        source.layers[0].advanced = Some(Arc::new(state));
        let source_id = source.layers[0].id.clone();

        let prepared = prepare(
            &source,
            &source_id,
            Settings {
                denoise: 0.,
                sharpen: 0.,
                scale: 1,
                enlarge_page: false,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let restored = prepared
            .document
            .find_layer(&prepared.restored_layer_id)
            .unwrap();
        assert!(!prepared.document.find_layer(&source_id).unwrap().visible);
        assert_eq!(
            restored.advanced.as_ref().unwrap().result.to_rgba16(),
            (*master).to_rgba16()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("high-precision-restoration.comp");
        crate::document::save(&prepared.document, &path).unwrap();
        let reopened = crate::document::open(&path).unwrap();
        assert_eq!(
            reopened
                .find_layer(&prepared.restored_layer_id)
                .unwrap()
                .advanced
                .as_ref()
                .unwrap()
                .result
                .to_rgba16(),
            (*master).to_rgba16()
        );
    }
}
