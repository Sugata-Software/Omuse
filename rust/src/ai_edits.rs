//! Local, reversible composition of provider images. The provider never owns
//! the source document or the decision about which pixels may be replaced.
use crate::{
    editor::{Editor, Selection},
    model::{Document, Layer},
    raster,
};
use anyhow::{Context, Result, ensure};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MAX_EDIT_PIXELS: u64 = 16_000_000;
pub const PRODUCT_PRESENTATION_METADATA_KEY: &str = "omuseProductPresentation";

/// Optional, local presentation layers for an already protected product.
/// Neither effect is enabled implicitly: callers must opt into a concrete
/// setting, and both outputs remain ordinary removable raster layers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<ProductShadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection: Option<ProductReflection>,
}

impl ProductPresentation {
    pub fn is_empty(&self) -> bool {
        self.shadow.is_none() && self.reflection.is_none()
    }

    fn validate(&self) -> Result<()> {
        if let Some(shadow) = &self.shadow {
            shadow.validate()?;
        }
        if let Some(reflection) = &self.reflection {
            reflection.validate()?;
        }
        Ok(())
    }
}

/// A bounded, monochrome shadow cast from the selected product silhouette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductShadow {
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur_px: u32,
    pub opacity: f32,
    pub color: [u8; 3],
}

impl Default for ProductShadow {
    fn default() -> Self {
        Self {
            offset_x: 0,
            offset_y: 18,
            blur_px: 16,
            opacity: 0.35,
            color: [0, 0, 0],
        }
    }
}

impl ProductShadow {
    fn validate(&self) -> Result<()> {
        ensure!(
            (-4096..=4096).contains(&self.offset_x)
                && (-4096..=4096).contains(&self.offset_y)
                && self.blur_px <= 256
                && self.opacity.is_finite()
                && (0.0..=1.0).contains(&self.opacity),
            "invalid product shadow settings"
        );
        Ok(())
    }
}

/// A vertically mirrored, gradually faded copy of the selected product.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductReflection {
    /// Fraction of the selected subject height to retain, from 5% through 100%.
    pub height_ratio: f32,
    pub gap_px: u32,
    pub opacity: f32,
}

impl Default for ProductReflection {
    fn default() -> Self {
        Self {
            height_ratio: 0.45,
            gap_px: 8,
            opacity: 0.28,
        }
    }
}

impl ProductReflection {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.height_ratio.is_finite()
                && (0.05..=1.0).contains(&self.height_ratio)
                && self.gap_px <= 4096
                && self.opacity.is_finite()
                && (0.0..=1.0).contains(&self.opacity),
            "invalid product reflection settings"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageIntent {
    #[default]
    Generate,
    Replace,
    Background,
    Expand {
        left: u32,
        top: u32,
        right: u32,
        bottom: u32,
    },
}
pub struct PreparedInput {
    pub canvas: RgbaImage,
    pub mask: Option<GrayImage>,
}

fn validate_document(source: &Document) -> Result<()> {
    ensure!(
        u64::from(source.width) * u64::from(source.height) <= MAX_EDIT_PIXELS,
        "AI editing supports at most 16 million canvas pixels"
    );
    let errors = raster::validate(source);
    ensure!(
        errors.is_empty(),
        "Invalid source artwork: {}",
        errors.join("; ")
    );
    Ok(())
}
fn selection_mask(selection: &Selection, width: u32, height: u32) -> Result<GrayImage> {
    ensure!(
        selection.width == width && selection.height == height,
        "The selection no longer matches the canvas"
    );
    ensure!(
        selection.mask.len() == width as usize * height as usize,
        "Invalid selection mask length"
    );
    GrayImage::from_raw(width, height, selection.mask.clone()).context("Invalid selection mask")
}
fn expanded_dimensions(
    source: &Document,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) -> Result<(u32, u32)> {
    ensure!(
        left <= 4096 && top <= 4096 && right <= 4096 && bottom <= 4096,
        "Expand by at most 4096 pixels per edge"
    );
    ensure!(
        left + top + right + bottom > 0,
        "Choose a canvas edge to expand"
    );
    let width = source
        .width
        .checked_add(left)
        .and_then(|v| v.checked_add(right))
        .context("Canvas width overflow")?;
    let height = source
        .height
        .checked_add(top)
        .and_then(|v| v.checked_add(bottom))
        .context("Canvas height overflow")?;
    ensure!(
        crate::model::valid_dimensions(width, height)
            && u64::from(width) * u64::from(height) <= MAX_EDIT_PIXELS,
        "Expanded canvas exceeds the 16 million pixel AI edit limit"
    );
    Ok((width, height))
}

/// These live adjustments evaluate canvas coordinates or neighbouring canvas
/// pixels. Layer-local filters/effects keep their source dimensions when the
/// canvas grows; pointwise live adjustments likewise need no extra render.
fn has_canvas_dependent_adjustments(layers: &[Layer]) -> bool {
    layers.iter().any(|layer| {
        layer.visible
            && layer.opacity > 0.
            && (matches!(
                layer
                    .metadata
                    .pointer("/adjustment/kind")
                    .and_then(Value::as_str),
                Some("Gaussian Blur" | "Motion Blur" | "Add Noise" | "Grain")
            ) || has_canvas_dependent_adjustments(&layer.children))
    })
}

fn expanded_source_document(
    source: Document,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
) -> Result<Document> {
    let mut editor = Editor::new(source);
    editor.set_history_limit(0);
    ensure!(
        editor.crop_canvas(-(left as i32), -(top as i32), width, height),
        "Canvas expansion could not preserve the current layer geometry"
    );
    let mut result = editor.document;
    fn shift_frame_metadata(layers: &mut [Layer], left: u32, top: u32) -> Result<()> {
        for layer in layers {
            if let Some(mut frame) = crate::create::frame_spec(layer)? {
                frame.bounds.x += left as f32;
                frame.bounds.y += top as f32;
                layer.metadata["omuseCreate"]["frame"] = serde_json::to_value(frame)?;
            }
            shift_frame_metadata(&mut layer.children, left, top)?;
        }
        Ok(())
    }
    shift_frame_metadata(&mut result.layers, left, top)?;
    Ok(result)
}

fn check_expanded_adjustments(
    source: &Document,
    original: &RgbaImage,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
) -> Result<()> {
    if !has_canvas_dependent_adjustments(&source.layers) {
        return Ok(());
    }
    let expanded = expanded_source_document(source.clone(), left, top, width, height)?;
    let pixels = raster::composite(&expanded);
    ensure!(
        pixels.dimensions() == (width, height),
        "Canvas expansion could not evaluate the existing live adjustments"
    );
    ensure!(
        original
            .enumerate_pixels()
            .all(|(x, y, pixel)| { pixels.get_pixel(x + left, y + top) == pixel }),
        "Expanding this canvas would change the original pixels because a live spatial or noise adjustment is reevaluated. Keep the current canvas size, or explicitly flatten an unlocked duplicate before expanding."
    );
    Ok(())
}

fn protect_locked_objects(source: &Document, mask: &mut GrayImage) -> Result<()> {
    ensure!(
        mask.dimensions() == (source.width, source.height),
        "The editable mask no longer matches the canvas"
    );
    fn show_locked(layers: &mut [Layer], locked_ancestor: bool) -> bool {
        let mut any_visible = false;
        for layer in layers {
            let locked = locked_ancestor || layer.locked;
            let protected_children = show_locked(&mut layer.children, locked);
            layer.visible &= locked || protected_children;
            any_visible |= layer.visible;
        }
        any_visible
    }
    let mut protected = source.clone();
    protected.background = [0; 4];
    // Keep the complete tree, including hidden/unlocked live-mask sources.
    // Removing those sources invalidates a locked clipped layer's dependency
    // graph. Only locked contributions and their visible ancestors render;
    // an unlocked clipping base still supplies alpha without being protected
    // across its entire footprint.
    if !show_locked(&mut protected.layers, false) {
        return Ok(());
    }
    let pixels = raster::composite(&protected);
    ensure!(
        pixels.dimensions() == (source.width, source.height),
        "Locked artwork protection could not be evaluated"
    );
    for (mask, pixel) in mask.pixels_mut().zip(pixels.pixels()) {
        if pixel[3] > 0 {
            mask[0] = 0;
        }
    }
    Ok(())
}

pub fn prepare_input(
    source: &Document,
    selection: Option<&Selection>,
    intent: &ImageIntent,
) -> Result<PreparedInput> {
    validate_document(source)?;
    if let ImageIntent::Expand {
        left,
        top,
        right,
        bottom,
    } = *intent
    {
        let (width, height) = expanded_dimensions(source, left, top, right, bottom)?;
        let original = raster::composite(source);
        check_expanded_adjustments(source, &original, left, top, width, height)?;
        let mut canvas = RgbaImage::new(width, height);
        image::imageops::replace(&mut canvas, &original, i64::from(left), i64::from(top));
        let mask = GrayImage::from_fn(width, height, |x, y| {
            Luma([
                if x >= left && x < left + source.width && y >= top && y < top + source.height {
                    0
                } else {
                    255
                },
            ])
        });
        return Ok(PreparedInput {
            canvas,
            mask: Some(mask),
        });
    }
    let mut mask = match intent {
        ImageIntent::Generate => None,
        ImageIntent::Replace => Some(match selection {
            Some(selection) => selection_mask(selection, source.width, source.height)?,
            None => GrayImage::from_pixel(source.width, source.height, Luma([255])),
        }),
        ImageIntent::Background => {
            let selection = selection.context(
                "Select the subject you want to protect before replacing its background",
            )?;
            let mut mask = selection_mask(selection, source.width, source.height)?;
            ensure!(
                mask.pixels().any(|p| p[0] > 0) && mask.pixels().any(|p| p[0] < 255),
                "Select a subject with some background outside it"
            );
            for pixel in mask.pixels_mut() {
                pixel[0] = 255 - pixel[0];
            }
            Some(mask)
        }
        ImageIntent::Expand { .. } => unreachable!(),
    };
    if let Some(mask) = mask.as_mut() {
        protect_locked_objects(source, mask)?;
        ensure!(
            mask.pixels().any(|pixel| pixel[0] > 0),
            "The editable region is empty or protected by locked objects"
        );
    }
    Ok(PreparedInput {
        canvas: raster::composite(source),
        mask,
    })
}

/// Normalize resolution only when the provider kept the requested framing.
/// A different aspect ratio needs deliberate alignment rather than distortion.
fn aligned_image(image: &RgbaImage, width: u32, height: u32) -> Result<RgbaImage> {
    ensure!(
        image.width() > 0
            && image.height() > 0
            && u64::from(image.width()) * u64::from(image.height()) <= MAX_EDIT_PIXELS,
        "Generated image dimensions exceed the editing limit"
    );
    let ratio = image.width() as f64 / image.height() as f64;
    let expected = width as f64 / height as f64;
    ensure!(
        (ratio / expected - 1.).abs() <= 0.015,
        "Generated image framing differs from the requested canvas. Keep it as a separate asset or request the original aspect ratio."
    );
    Ok(if image.dimensions() == (width, height) {
        image.clone()
    } else {
        image::imageops::resize(image, width, height, image::imageops::FilterType::Lanczos3)
    })
}

pub fn prepare_result(
    source: &Document,
    selection: Option<&Selection>,
    intent: &ImageIntent,
    generated: &RgbaImage,
    mut provenance: Value,
) -> Result<Document> {
    let input = prepare_input(source, selection, intent)?;
    let mut result = source.clone();
    let mut layer = Layer::paint(
        match intent {
            ImageIntent::Generate => "Generated image",
            ImageIntent::Replace => "AI replacement",
            ImageIntent::Background => "New background · protected subject",
            ImageIntent::Expand { .. } => "Generated canvas surround",
        },
        1,
        1,
    );
    if *intent == ImageIntent::Generate {
        ensure!(
            generated.width() > 0
                && generated.height() > 0
                && u64::from(generated.width()) * u64::from(generated.height()) <= MAX_EDIT_PIXELS,
            "Generated image exceeds the editing limit"
        );
        let scale = (source.width as f32 / generated.width() as f32)
            .min(source.height as f32 / generated.height() as f32)
            .min(1.);
        layer.image = Some(generated.clone().into());
        layer.scale_x = scale;
        layer.scale_y = scale;
        layer.offset_x = (source.width as f32 - generated.width() as f32 * scale) * 0.5;
        layer.offset_y = (source.height as f32 - generated.height() as f32 * scale) * 0.5;
    } else {
        layer.image =
            Some(aligned_image(generated, input.canvas.width(), input.canvas.height())?.into());
        if let Some(mask) = input.mask {
            layer.mask = Some(
                RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
                    let value = mask.get_pixel(x, y)[0];
                    Rgba([value, value, value, 255])
                })
                .into(),
            );
        }
        if let ImageIntent::Expand { left, top, .. } = *intent {
            result = expanded_source_document(
                result,
                left,
                top,
                input.canvas.width(),
                input.canvas.height(),
            )?;
            if !provenance.is_object() {
                provenance = serde_json::json!({});
            }
            provenance["originalRectangle"] =
                serde_json::json!([left, top, source.width, source.height]);
        }
    }
    if !provenance.is_object() {
        provenance = serde_json::json!({});
    }
    provenance["intent"] = serde_json::to_value(intent)?;
    provenance["sourceDimensions"] = serde_json::json!([source.width, source.height]);
    provenance["providerDimensions"] = serde_json::json!([generated.width(), generated.height()]);
    layer.metadata["omuseGenerated"] = provenance;
    result.layers.push(layer);
    let errors = raster::validate(&result);
    ensure!(
        errors.is_empty(),
        "Generated draft is invalid: {}",
        errors.join("; ")
    );
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
struct SubjectBounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl SubjectBounds {
    fn height(self) -> u32 {
        self.bottom - self.top + 1
    }
}

fn subject_bounds(subject: &GrayImage) -> Result<SubjectBounds> {
    let mut bounds = None::<SubjectBounds>;
    for (x, y, pixel) in subject.enumerate_pixels() {
        if pixel[0] == 0 {
            continue;
        }
        bounds = Some(match bounds {
            Some(current) => SubjectBounds {
                left: current.left.min(x),
                top: current.top.min(y),
                right: current.right.max(x),
                bottom: current.bottom.max(y),
            },
            None => SubjectBounds {
                left: x,
                top: y,
                right: x,
                bottom: y,
            },
        });
    }
    bounds.context("Select a visible product before adding a shadow or reflection")
}

/// Effects are always absent over the product itself and anything the editor
/// already protects through a locked layer. This mirrors the provider-input
/// rule for background replacement, so local finishing cannot overpaint a
/// locked logo, header, or protected group outside the selected product.
fn clear_protected_effect_pixels(
    surface: &mut RgbaImage,
    subject: &GrayImage,
    editable_effect_pixels: &GrayImage,
) {
    for ((pixel, selection), editable) in surface
        .pixels_mut()
        .zip(subject.pixels())
        .zip(editable_effect_pixels.pixels())
    {
        if selection[0] > 0 || editable[0] == 0 {
            *pixel = Rgba([0; 4]);
        }
    }
}

fn presentation_layer(name: &str, image: RgbaImage, kind: &str, settings: Value) -> Layer {
    let width = image.width();
    let height = image.height();
    let mut layer = Layer::paint(name, width, height);
    layer.image = Some(image.into());
    layer.metadata[PRODUCT_PRESENTATION_METADATA_KEY] = json!({
        "version": 1,
        "kind": kind,
        "settings": settings,
    });
    layer
}

fn product_shadow_layer(
    source: &Document,
    subject: &GrayImage,
    editable_effect_pixels: &GrayImage,
    settings: &ProductShadow,
) -> Result<Layer> {
    let silhouette = RgbaImage::from_fn(source.width, source.height, |x, y| {
        Rgba([0, 0, 0, subject.get_pixel(x, y)[0]])
    });
    let blurred = image::imageops::blur(&silhouette, settings.blur_px as f32);
    let mut image = RgbaImage::new(source.width, source.height);
    for (x, y, pixel) in blurred.enumerate_pixels() {
        let target_x = i64::from(x) + i64::from(settings.offset_x);
        let target_y = i64::from(y) + i64::from(settings.offset_y);
        if !(0..i64::from(source.width)).contains(&target_x)
            || !(0..i64::from(source.height)).contains(&target_y)
        {
            continue;
        }
        let alpha = (f32::from(pixel[3]) * settings.opacity).round() as u8;
        image.put_pixel(
            target_x as u32,
            target_y as u32,
            Rgba([
                settings.color[0],
                settings.color[1],
                settings.color[2],
                alpha,
            ]),
        );
    }
    clear_protected_effect_pixels(&mut image, subject, editable_effect_pixels);
    Ok(presentation_layer(
        "Product shadow",
        image,
        "shadow",
        serde_json::to_value(settings)?,
    ))
}

fn product_reflection_layer(
    source: &Document,
    subject: &GrayImage,
    editable_effect_pixels: &GrayImage,
    settings: &ProductReflection,
) -> Result<Layer> {
    let bounds = subject_bounds(subject)?;
    let height = ((bounds.height() as f32 * settings.height_ratio).ceil() as u32).max(1);
    let start_y = bounds
        .bottom
        .checked_add(1)
        .and_then(|value| value.checked_add(settings.gap_px))
        .context("product reflection position overflows the canvas")?;
    ensure!(
        start_y < source.height && height <= source.height - start_y,
        "Move the product up or expand the canvas before adding this reflection"
    );
    let source_pixels = raster::composite(source);
    let mut image = RgbaImage::new(source.width, source.height);
    for destination_offset in 0..height {
        let source_offset =
            destination_offset as u64 * u64::from(bounds.height()) / u64::from(height);
        let source_y = bounds.bottom - source_offset as u32;
        let fade = 1.0 - destination_offset as f32 / height as f32;
        for x in bounds.left..=bounds.right {
            let selection = subject.get_pixel(x, source_y)[0];
            if selection == 0 {
                continue;
            }
            let pixel = source_pixels.get_pixel(x, source_y);
            let alpha =
                (f32::from(pixel[3]) * f32::from(selection) / 255.0 * settings.opacity * fade)
                    .round() as u8;
            image.put_pixel(
                x,
                start_y + destination_offset,
                Rgba([pixel[0], pixel[1], pixel[2], alpha]),
            );
        }
    }
    clear_protected_effect_pixels(&mut image, subject, editable_effect_pixels);
    Ok(presentation_layer(
        "Product reflection",
        image,
        "reflection",
        serde_json::to_value(settings)?,
    ))
}

/// Add only the presentation layers explicitly requested by the caller. The
/// input document and its existing source layers are cloned unchanged; each
/// output layer removes every selected-subject pixel before it is composited.
pub fn apply_product_presentation(
    source: &Document,
    selection: &Selection,
    presentation: &ProductPresentation,
) -> Result<Document> {
    validate_document(source)?;
    presentation.validate()?;
    ensure!(
        !presentation.is_empty(),
        "Choose a shadow or reflection before applying product presentation"
    );
    let subject = selection_mask(selection, source.width, source.height)?;
    subject_bounds(&subject)?;
    let mut editable_effect_pixels =
        GrayImage::from_pixel(source.width, source.height, Luma([255]));
    protect_locked_objects(source, &mut editable_effect_pixels)?;
    let mut result = source.clone();
    if let Some(shadow) = &presentation.shadow {
        result.layers.push(product_shadow_layer(
            source,
            &subject,
            &editable_effect_pixels,
            shadow,
        )?);
    }
    if let Some(reflection) = &presentation.reflection {
        result.layers.push(product_reflection_layer(
            source,
            &subject,
            &editable_effect_pixels,
            reflection,
        )?);
    }
    let errors = raster::validate(&result);
    ensure!(
        errors.is_empty(),
        "Product presentation draft is invalid: {}",
        errors.join("; ")
    );
    Ok(result)
}

/// Apply a provider background result, then add the user-selected local
/// presentation layers. An empty presentation is deliberately a no-op.
pub fn prepare_product_background_result(
    source: &Document,
    subject: &Selection,
    generated: &RgbaImage,
    provenance: Value,
    presentation: &ProductPresentation,
) -> Result<Document> {
    let background = prepare_result(
        source,
        Some(subject),
        &ImageIntent::Background,
        generated,
        provenance,
    )?;
    if presentation.is_empty() {
        return Ok(background);
    }
    apply_product_presentation(&background, subject, presentation)
}

/// Remove only presentation layers created by this module. This is useful for
/// an explicit "remove presentation" action; normal editor undo is still a
/// single transaction because application returns one replacement document.
pub fn remove_product_presentation_layers(document: &mut Document) -> Result<usize> {
    fn owned(layer: &Layer) -> bool {
        layer
            .metadata
            .get(PRODUCT_PRESENTATION_METADATA_KEY)
            .and_then(Value::as_object)
            .is_some_and(|value| {
                value.get("version").and_then(Value::as_u64) == Some(1)
                    && matches!(
                        value.get("kind").and_then(Value::as_str),
                        Some("shadow" | "reflection")
                    )
                    && value.get("settings").is_some_and(Value::is_object)
            })
    }
    fn contains_locked_layer(layers: &[Layer], locked_ancestor: bool) -> bool {
        layers.iter().any(|layer| {
            let locked = locked_ancestor || layer.locked;
            locked || contains_locked_layer(&layer.children, locked)
        })
    }
    fn contains_protected_presentation(layers: &[Layer], locked_ancestor: bool) -> bool {
        layers.iter().any(|layer| {
            let locked = locked_ancestor || layer.locked;
            (owned(layer) && (locked || contains_locked_layer(&layer.children, false)))
                || contains_protected_presentation(&layer.children, locked)
        })
    }
    ensure!(
        !contains_protected_presentation(&document.layers, false),
        "Unlock local product finishing before removing it"
    );
    fn remove(layers: &mut Vec<Layer>) -> usize {
        let mut removed = 0;
        layers.retain_mut(|layer| {
            removed += remove(&mut layer.children);
            if owned(layer) {
                removed += 1;
            }
            !owned(layer)
        });
        removed
    }
    Ok(remove(&mut document.layers))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Document {
        let mut doc = Document::new(8, 6);
        doc.layers[0].image = Some(
            RgbaImage::from_fn(8, 6, |x, y| Rgba([x as u8 * 20, y as u8 * 30, 80, 255])).into(),
        );
        doc
    }

    fn adjustment_layer(settings: Value) -> Layer {
        let mut layer = Layer::group(settings["kind"].as_str().unwrap());
        layer.metadata = json!({"isGroup": false, "adjustment": settings});
        layer
    }

    fn assert_expanded_original_matches(source: &Document, intent: ImageIntent) -> Document {
        let ImageIntent::Expand {
            left,
            top,
            right,
            bottom,
        } = intent
        else {
            panic!("expected expansion intent");
        };
        let before = raster::composite(source);
        let width = source.width + left + right;
        let height = source.height + top + bottom;
        prepare_input(source, None, &intent).unwrap();
        let result = prepare_result(
            source,
            None,
            &intent,
            &RgbaImage::from_pixel(width, height, Rgba([7, 8, 9, 255])),
            Value::Null,
        )
        .unwrap();
        let after = raster::composite(&result);
        assert_eq!(after.dimensions(), (width, height));
        for (x, y, pixel) in before.enumerate_pixels() {
            assert_eq!(
                after.get_pixel(x + left, y + top),
                pixel,
                "original pixel ({x}, {y})"
            );
        }
        result
    }
    #[test]
    fn replacement_and_background_preserve_exact_protected_pixels_and_originals() {
        let source = source();
        let before = raster::composite(&source);
        let selection = Selection {
            width: 8,
            height: 6,
            mask: (0..48)
                .map(|index| if index % 8 < 4 { 255 } else { 0 })
                .collect(),
        };
        let generated = RgbaImage::from_pixel(16, 12, Rgba([220, 20, 90, 255]));
        for intent in [ImageIntent::Replace, ImageIntent::Background] {
            let result =
                prepare_result(&source, Some(&selection), &intent, &generated, Value::Null)
                    .unwrap();
            assert!(crate::create_history::documents_match(
                &source,
                &Document {
                    layers: result.layers[..source.layers.len()].to_vec(),
                    ..source.clone()
                }
            ));
            let after = raster::composite(&result);
            for y in 0..6 {
                for x in 0..8 {
                    if (intent == ImageIntent::Replace && x >= 4)
                        || (intent == ImageIntent::Background && x < 4)
                    {
                        assert_eq!(after.get_pixel(x, y), before.get_pixel(x, y));
                    }
                }
            }
        }
    }

    #[test]
    fn locked_clipped_pixels_are_protected_without_protecting_the_unlocked_base() {
        let mut source = source();
        let mut clipped = Layer::paint("Locked clipped detail", 2, 2);
        clipped.image = Some(RgbaImage::from_pixel(2, 2, Rgba([220, 40, 30, 255])).into());
        clipped.offset_x = 1.;
        clipped.offset_y = 1.;
        clipped.locked = true;
        clipped.metadata["maskSourceID"] = json!(source.layers[0].id);
        source.layers.push(clipped);
        let mut independent = Layer::paint("Independent locked detail", 1, 1);
        independent.image = Some(RgbaImage::from_pixel(1, 1, Rgba([30, 220, 40, 255])).into());
        independent.offset_x = 6.;
        independent.offset_y = 4.;
        independent.locked = true;
        source.layers.push(independent);
        assert!(raster::validate(&source).is_empty());
        let before = raster::composite(&source);
        let generated = RgbaImage::from_pixel(8, 6, Rgba([180, 20, 210, 255]));
        for intent in [ImageIntent::Replace, ImageIntent::Background] {
            let selection = Selection {
                width: 8,
                height: 6,
                mask: (0..48)
                    .map(|index| {
                        if intent == ImageIntent::Replace || index == 0 {
                            255
                        } else {
                            0
                        }
                    })
                    .collect(),
            };
            let prepared = prepare_input(&source, Some(&selection), &intent).unwrap();
            let mask = prepared.mask.unwrap();
            for (x, y) in [(1, 1), (2, 2), (6, 4)] {
                assert_eq!(mask.get_pixel(x, y)[0], 0);
            }
            assert_eq!(mask.get_pixel(4, 3)[0], 255);
            let result =
                prepare_result(&source, Some(&selection), &intent, &generated, Value::Null)
                    .unwrap();
            let after = raster::composite(&result);
            for (x, y) in [(1, 1), (2, 2), (6, 4)] {
                assert_eq!(after.get_pixel(x, y), before.get_pixel(x, y));
            }
            assert_eq!(after.get_pixel(4, 3), generated.get_pixel(4, 3));
            assert!(crate::create_history::documents_match(
                &source,
                &Document {
                    layers: result.layers[..source.layers.len()].to_vec(),
                    ..source.clone()
                }
            ));
        }
    }

    #[test]
    fn inherited_locks_keep_hidden_mask_dependencies_and_visible_group_coverage() {
        let mut source = source();
        let mut first_mask = Layer::paint("Hidden horizontal mask source", 8, 6);
        first_mask.image = Some(
            RgbaImage::from_fn(8, 6, |x, _| {
                Rgba([255, 255, 255, if x < 4 { 255 } else { 0 }])
            })
            .into(),
        );
        first_mask.visible = false;
        let mut second_mask = Layer::paint("Hidden chained mask source", 8, 6);
        second_mask.image = Some(
            RgbaImage::from_fn(8, 6, |_, y| {
                Rgba([255, 255, 255, if y < 3 { 255 } else { 0 }])
            })
            .into(),
        );
        second_mask.visible = false;
        second_mask.metadata["maskSourceID"] = json!(first_mask.id);
        let mut detail = Layer::paint("Detail protected by its parent", 8, 6);
        detail.image = Some(RgbaImage::from_pixel(8, 6, Rgba([30, 40, 230, 255])).into());
        detail.metadata["maskSourceID"] = json!(second_mask.id);
        let mut group = Layer::group("Locked masked group");
        group.locked = true;
        group.opacity = 0.5;
        group.mask = Some(
            RgbaImage::from_fn(8, 6, |_, y| {
                let value = if y < 2 { 255 } else { 0 };
                Rgba([value, value, value, 255])
            })
            .into(),
        );
        group.children.push(detail);
        source.layers.extend([first_mask, second_mask, group]);
        assert!(raster::validate(&source).is_empty());
        let before = raster::composite(&source);
        let mask = prepare_input(&source, None, &ImageIntent::Replace)
            .unwrap()
            .mask
            .unwrap();
        assert_eq!(mask.get_pixel(1, 1)[0], 0);
        for (x, y) in [(5, 1), (1, 2), (1, 4)] {
            assert_eq!(mask.get_pixel(x, y)[0], 255);
        }
        let generated = RgbaImage::from_pixel(8, 6, Rgba([180, 20, 210, 255]));
        let result = prepare_result(
            &source,
            None,
            &ImageIntent::Replace,
            &generated,
            Value::Null,
        )
        .unwrap();
        let after = raster::composite(&result);
        assert_eq!(after.get_pixel(1, 1), before.get_pixel(1, 1));
        for (x, y) in [(5, 1), (1, 2), (1, 4)] {
            assert_eq!(after.get_pixel(x, y), generated.get_pixel(x, y));
        }
        // A hidden locked ancestor must not protect its visible children.
        source.layers.last_mut().unwrap().visible = false;
        let mask = prepare_input(&source, None, &ImageIntent::Replace)
            .unwrap()
            .mask
            .unwrap();
        assert!(mask.pixels().all(|pixel| pixel[0] == 255));
    }

    #[test]
    fn invalid_locked_protection_render_is_an_error_without_changing_the_mask() {
        let mut source = source();
        source.layers[0].locked = true;
        source.layers[0].metadata["maskSourceID"] = json!(uuid::Uuid::new_v4().to_string());
        let mut mask = GrayImage::from_pixel(source.width, source.height, Luma([255]));
        let before = mask.clone();
        let error = protect_locked_objects(&source, &mut mask).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("protection could not be evaluated")
        );
        assert_eq!(mask, before);
    }

    #[test]
    fn expansion_preserves_original_rectangle_and_has_single_undo() {
        let source = source();
        let before = raster::composite(&source);
        let intent = ImageIntent::Expand {
            left: 2,
            top: 3,
            right: 2,
            bottom: 3,
        };
        let generated = RgbaImage::from_pixel(12, 12, Rgba([7, 8, 9, 255]));
        let result = prepare_result(&source, None, &intent, &generated, Value::Null).unwrap();
        let after = raster::composite(&result);
        for y in 0..6 {
            for x in 0..8 {
                assert_eq!(after.get_pixel(x + 2, y + 3), before.get_pixel(x, y));
            }
        }
        let mut editor = Editor::new(source);
        editor.replace_document_transaction(result).unwrap();
        assert!(editor.undo());
        assert_eq!(raster::composite(&editor.document), before);
    }

    #[test]
    fn expansion_rejects_gaussian_motion_noise_and_grain_that_change_original_pixels() {
        let intent = ImageIntent::Expand {
            left: 2,
            top: 2,
            right: 2,
            bottom: 2,
        };
        for settings in [
            json!({"kind": "Gaussian Blur", "blurRadius": 2.}),
            json!({"kind": "Motion Blur", "motionAngle": 0., "motionDistance": 4.}),
            json!({"kind": "Add Noise", "noiseAmount": 30., "noiseSeed": 17}),
            json!({"kind": "Grain", "grainSettings": {"amount": 40., "seed": 17}}),
        ] {
            let mut source = source();
            source.background = [255; 4];
            let mut adjustment = adjustment_layer(settings.clone());
            // Include a nested, clipped adjustment: its visible output is
            // masked, but its kernel still evaluates the whole canvas.
            adjustment.metadata["maskSourceID"] = json!(source.layers[0].id);
            let mut group = Layer::group("Live adjustment group");
            group.children.push(adjustment);
            source.layers.push(group);
            let unchanged = source.clone();
            let before = raster::composite(&source);
            assert_eq!(before.dimensions(), (8, 6));
            let expanded = expanded_source_document(source.clone(), 2, 2, 12, 10).unwrap();
            let reevaluated = raster::composite(&expanded);
            assert_eq!(reevaluated.dimensions(), (12, 10));
            assert!(
                before
                    .enumerate_pixels()
                    .any(|(x, y, pixel)| { reevaluated.get_pixel(x + 2, y + 2) != pixel }),
                "fixture must reproduce changed protected pixels for {settings}"
            );
            let error = prepare_input(&source, None, &intent)
                .err()
                .expect("reject before sending");
            assert!(
                error
                    .to_string()
                    .contains("would change the original pixels")
            );
            assert!(error.to_string().contains("flatten an unlocked duplicate"));
            assert!(
                prepare_result(
                    &source,
                    None,
                    &intent,
                    &RgbaImage::from_pixel(12, 10, Rgba([7, 8, 9, 255])),
                    Value::Null
                )
                .is_err()
            );
            assert!(crate::create_history::documents_match(&source, &unchanged));
        }
    }

    #[test]
    fn expansion_keeps_pointwise_adjustments_editable_and_preserves_original_pixels() {
        let mut source = source();
        let mut group = Layer::group("Pointwise live adjustments");
        group.children.extend([
            adjustment_layer(json!({"kind": "Invert"})),
            adjustment_layer(json!({"kind": "Exposure", "exposureSettings": {
                "exposure": 0.5, "offset": 0.02, "gamma": 1.2
            }})),
            adjustment_layer(json!({"kind": "Hue/Saturation", "hue": 12., "saturation": 10.})),
        ]);
        let group_id = group.id.clone();
        source.layers.push(group);
        let result = assert_expanded_original_matches(
            &source,
            ImageIntent::Expand {
                left: 2,
                top: 2,
                right: 2,
                bottom: 2,
            },
        );
        let kept = result.find_layer(&group_id).unwrap();
        assert_eq!(kept.children.len(), 3);
        for (actual, original) in kept
            .children
            .iter()
            .zip(&source.find_layer(&group_id).unwrap().children)
        {
            assert_eq!(
                actual.metadata["adjustment"],
                original.metadata["adjustment"]
            );
        }
    }

    #[test]
    fn expansion_allows_inactive_and_actually_unchanged_spatial_adjustments() {
        let mut inactive = source();
        inactive.background = [255; 4];
        let mut hidden = Layer::group("Hidden blur");
        hidden.visible = false;
        hidden.children.push(adjustment_layer(
            json!({"kind": "Gaussian Blur", "blurRadius": 2.}),
        ));
        let mut transparent = adjustment_layer(json!({"kind": "Add Noise", "noiseAmount": 30.}));
        transparent.opacity = 0.;
        inactive.layers.extend([
            hidden,
            transparent,
            adjustment_layer(json!({"kind": "Gaussian Blur", "blurRadius": 0.})),
            adjustment_layer(json!({"kind": "Add Noise", "noiseAmount": 0.})),
            adjustment_layer(json!({"kind": "Grain", "grainSettings": {"amount": 0.}})),
        ]);
        assert_expanded_original_matches(
            &inactive,
            ImageIntent::Expand {
                left: 2,
                top: 2,
                right: 2,
                bottom: 2,
            },
        );

        // Grain is tied to x/y coordinates, so adding only right/bottom
        // margins leaves its original pixels unchanged and should be allowed.
        let mut grain = source();
        grain
            .layers
            .push(adjustment_layer(json!({"kind": "Grain", "grainSettings": {
                "amount": 40., "seed": 17
            }})));
        assert_expanded_original_matches(
            &grain,
            ImageIntent::Expand {
                left: 0,
                top: 0,
                right: 2,
                bottom: 2,
            },
        );
    }

    #[test]
    fn expansion_preserves_layer_local_live_filters_and_effects() {
        use crate::{
            advanced::LayerState,
            advanced_ops::{AdvancedOperation, FilterNode},
            filters::Filter,
        };
        use std::sync::{Arc, atomic::AtomicBool};
        let mut filtered = source();
        let mut state =
            LayerState::from_image(filtered.layers[0].image.as_ref().unwrap(), "Photo").unwrap();
        state.recipe.nodes.push(FilterNode {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Local Gaussian blur".into(),
            enabled: true,
            opacity: 1.,
            operation: AdvancedOperation::Filter(Filter::GaussianBlur { sigma: 2. }),
            soft_mask: None,
        });
        let state = Arc::new(state.evaluate(&AtomicBool::new(false)).unwrap());
        filtered.layers[0].image = Some(state.proxy().unwrap().into());
        filtered.layers[0].advanced = Some(state.clone());
        let intent = ImageIntent::Expand {
            left: 2,
            top: 2,
            right: 2,
            bottom: 2,
        };
        let result = assert_expanded_original_matches(&filtered, intent.clone());
        assert!(Arc::ptr_eq(
            result.layers[0].advanced.as_ref().unwrap(),
            &state
        ));

        let mut effected = source();
        effected.layers[0].metadata["effects"] = json!({
            "shadow": {"distance": 1., "blur": 2., "opacity": 0.5}
        });
        let result = assert_expanded_original_matches(&effected, intent);
        assert_eq!(
            result.layers[0].metadata["effects"],
            effected.layers[0].metadata["effects"]
        );
    }
    #[test]
    fn masks_and_incompatible_framing_are_rejected_without_mutation() {
        let source = source();
        let selection = Selection {
            width: 8,
            height: 6,
            mask: vec![255; 3],
        };
        assert!(prepare_input(&source, Some(&selection), &ImageIntent::Replace).is_err());
        assert!(prepare_input(&source, None, &ImageIntent::Background).is_err());
        assert!(
            prepare_result(
                &source,
                None,
                &ImageIntent::Replace,
                &RgbaImage::new(8, 8),
                Value::Null
            )
            .is_err()
        );
    }
    #[test]
    fn expansion_keeps_frame_crop_geometry_editable() {
        use crate::create::{self, FrameBounds, FrameSpec};
        let mut source = Document::new(40, 30);
        let frame = FrameSpec::new(FrameBounds {
            x: 4.,
            y: 3.,
            width: 24.,
            height: 18.,
        });
        let id = create::add_image_frame(
            &mut source,
            "Frame",
            RgbaImage::from_pixel(12, 12, Rgba([120, 60, 40, 255])),
            frame,
        )
        .unwrap();
        let intent = ImageIntent::Expand {
            left: 5,
            top: 5,
            right: 5,
            bottom: 5,
        };
        let mut expanded = prepare_result(
            &source,
            None,
            &intent,
            &RgbaImage::from_pixel(50, 40, Rgba([4, 5, 6, 255])),
            Value::Null,
        )
        .unwrap();
        let layer = expanded.find_layer_mut(&id).unwrap();
        let shifted = create::frame_spec(layer).unwrap().unwrap();
        assert_eq!(shifted.bounds.x, 9.);
        assert_eq!(shifted.bounds.y, 8.);
        let before = layer.mask.clone();
        create::set_frame_crop(layer, shifted.crop).unwrap();
        assert_eq!(layer.mask, before);
    }

    fn product_selection() -> Selection {
        Selection {
            width: 8,
            height: 6,
            // A three-pixel-high product with room underneath for its reflection.
            mask: (0..48)
                .map(|index| {
                    let x = index % 8;
                    let y = index / 8;
                    if (2..=5).contains(&x) && y <= 2 {
                        255
                    } else {
                        0
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn product_presentation_is_reversible_and_never_paints_selected_source_pixels() {
        let source = source();
        let source_pixels = raster::composite(&source);
        let selection = product_selection();
        let presentation = ProductPresentation {
            shadow: Some(ProductShadow {
                offset_x: 0,
                offset_y: 1,
                blur_px: 0,
                opacity: 1.0,
                color: [0, 0, 0],
            }),
            reflection: Some(ProductReflection {
                height_ratio: 1.0,
                gap_px: 0,
                opacity: 1.0,
            }),
        };

        let result = apply_product_presentation(&source, &selection, &presentation).unwrap();
        assert!(crate::create_history::documents_match(
            &source,
            &Document {
                layers: result.layers[..source.layers.len()].to_vec(),
                ..source.clone()
            }
        ));
        assert_eq!(result.layers.len(), source.layers.len() + 2);
        assert_eq!(result.layers[source.layers.len()].name, "Product shadow");
        assert_eq!(
            result.layers[source.layers.len() + 1].name,
            "Product reflection"
        );

        let after = raster::composite(&result);
        for y in 0..source.height {
            for x in 0..source.width {
                if selection.mask[(y * source.width + x) as usize] > 0 {
                    assert_eq!(after.get_pixel(x, y), source_pixels.get_pixel(x, y));
                }
            }
        }
        // The first row below the product is a direct, opaque vertical mirror.
        assert_eq!(after.get_pixel(3, 3), source_pixels.get_pixel(3, 2));

        let mut explicitly_removed = result.clone();
        assert_eq!(
            remove_product_presentation_layers(&mut explicitly_removed).unwrap(),
            2
        );
        assert!(crate::create_history::documents_match(
            &source,
            &explicitly_removed
        ));

        let mut editor = Editor::new(source.clone());
        editor.replace_document_transaction(result).unwrap();
        assert!(editor.undo());
        assert!(crate::create_history::documents_match(
            &source,
            &editor.document
        ));
    }

    #[test]
    fn product_background_presentation_is_explicit_and_preserves_the_subject() {
        let source = source();
        let source_pixels = raster::composite(&source);
        let selection = product_selection();
        let generated = RgbaImage::from_pixel(8, 6, Rgba([220, 20, 90, 255]));

        let no_presentation = prepare_product_background_result(
            &source,
            &selection,
            &generated,
            Value::Null,
            &ProductPresentation::default(),
        )
        .unwrap();
        assert_eq!(no_presentation.layers.len(), source.layers.len() + 1);
        assert!(no_presentation.layers.iter().all(|layer| {
            layer
                .metadata
                .get(PRODUCT_PRESENTATION_METADATA_KEY)
                .is_none()
        }));

        let presented = prepare_product_background_result(
            &source,
            &selection,
            &generated,
            Value::Null,
            &ProductPresentation {
                shadow: Some(ProductShadow {
                    blur_px: 0,
                    offset_y: 1,
                    opacity: 1.0,
                    ..ProductShadow::default()
                }),
                reflection: None,
            },
        )
        .unwrap();
        let after = raster::composite(&presented);
        for y in 0..source.height {
            for x in 0..source.width {
                if selection.mask[(y * source.width + x) as usize] > 0 {
                    assert_eq!(after.get_pixel(x, y), source_pixels.get_pixel(x, y));
                }
            }
        }
    }

    #[test]
    fn product_presentation_rejects_out_of_canvas_reflections_without_mutating_source() {
        let source = source();
        let selection = product_selection();
        let presentation = ProductPresentation {
            shadow: None,
            reflection: Some(ProductReflection {
                height_ratio: 1.0,
                gap_px: 3,
                opacity: 0.5,
            }),
        };
        assert!(apply_product_presentation(&source, &selection, &presentation).is_err());
        assert_eq!(source.layers.len(), 1);
        assert!(
            source.layers[0]
                .metadata
                .get(PRODUCT_PRESENTATION_METADATA_KEY)
                .is_none()
        );
    }

    #[test]
    fn product_presentation_never_overpaints_locked_pixels_or_removes_locked_effects() {
        let mut source = source();
        let mut locked_group = Layer::group("Locked header");
        locked_group.locked = true;
        let mut locked_logo = Layer::paint("Locked logo", 8, 6);
        locked_logo.image = Some(
            RgbaImage::from_fn(8, 6, |x, y| {
                if (x, y) == (3, 3) {
                    Rgba([12, 230, 90, 255])
                } else {
                    Rgba([0; 4])
                }
            })
            .into(),
        );
        // The dependency lies outside the locked group. Local finishing must
        // keep it available without protecting the whole unlocked source.
        locked_logo.metadata["maskSourceID"] = json!(source.layers[0].id);
        locked_group.children.push(locked_logo);
        source.layers.push(locked_group);
        let before = raster::composite(&source);
        let selection = product_selection();
        let presentation = ProductPresentation {
            shadow: Some(ProductShadow {
                offset_x: 0,
                offset_y: 1,
                blur_px: 0,
                opacity: 1.0,
                color: [0, 0, 0],
            }),
            reflection: Some(ProductReflection {
                height_ratio: 1.0,
                gap_px: 0,
                opacity: 1.0,
            }),
        };
        let mut result = apply_product_presentation(&source, &selection, &presentation).unwrap();
        let after = raster::composite(&result);
        // Both the offset shadow and the mirrored product would otherwise be
        // composited at this locked child under its locked ancestor.
        assert_eq!(after.get_pixel(3, 3), before.get_pixel(3, 3));

        let result_before_remove = result.clone();
        result.layers.last_mut().unwrap().locked = true;
        assert!(remove_product_presentation_layers(&mut result).is_err());
        // The refusal happens before an unprotected sibling could be removed.
        assert_eq!(result.layers.len(), result_before_remove.layers.len());
        assert!(result.layers.iter().any(|layer| {
            layer
                .metadata
                .get(PRODUCT_PRESENTATION_METADATA_KEY)
                .is_some()
        }));
    }
}
