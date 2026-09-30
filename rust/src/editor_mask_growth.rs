//! Bounded mask grids retain the original bitmap and its document-space mapping.
use super::{LayerPlacement, Transform, metadata_placement, set_metadata_placement};
use crate::{
    effects,
    model::{Document, Layer},
    shared_image::SharedImage,
};
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};

pub(super) const MAX_GROWN_MASK_PIXELS: u64 = 16_777_216;
pub(super) type Bounds = (f32, f32, f32, f32);

/// The pre-stroke mask lives at this offset in the expanded bitmap. No second
/// expanded snapshot is needed for opacity accumulation or cancellation.
pub(super) struct Source {
    pub offset: (u32, u32),
    pub outside: u8,
    pub clamp_edges: bool,
    pub clamp_all: bool,
    pub solid: Option<Rgba<u8>>,
    pub source_size: (u32, u32),
}

impl Source {
    pub fn pixel(&self, original: &RgbaImage, x: u32, y: u32) -> Rgba<u8> {
        let x = i64::from(x) - i64::from(self.offset.0);
        let y = i64::from(y) - i64::from(self.offset.1);
        if let Some(solid) = self.solid {
            let halo = if self.clamp_edges { 3 } else { 0 };
            return if self.clamp_all
                || (x >= -halo
                    && y >= -halo
                    && x < i64::from(self.source_size.0) + halo
                    && y < i64::from(self.source_size.1) + halo)
            {
                solid
            } else {
                Rgba([self.outside, self.outside, self.outside, 255])
            };
        }
        sample_original(
            original,
            x,
            y,
            self.outside,
            self.clamp_edges,
            self.clamp_all,
        )
    }
}

pub(super) struct Growth {
    pub image: SharedImage,
    pub placement: LayerPlacement,
    pub source: Source,
    pub old_width: u32,
    pub sampling: String,
}

pub(super) fn placement(layer: &Layer, width: u32, height: u32) -> Option<LayerPlacement> {
    if (!(layer.is_group() || layer.image.is_none())
        || layer.metadata.get("maskOutsideCoverage").is_some())
        && let Some(explicit) = metadata_placement(layer, "maskPlacement")
    {
        return Some(explicit);
    }
    if layer.is_group() || layer.image.is_none() {
        let mask = layer.mask.as_ref()?;
        let size = layer.metadata.pointer("/transform/size");
        let w = size
            .and_then(|v| v.get(0))
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::from(mask.width())) as f32;
        let h = size
            .and_then(|v| v.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::from(mask.height())) as f32;
        let result = LayerPlacement {
            x: layer.offset_x,
            y: layer.offset_y,
            width: w * layer.scale_x.abs(),
            height: h * layer.scale_y.abs(),
            rotation: layer.rotation,
            flip_x: layer.scale_x < 0.,
            flip_y: layer.scale_y < 0.,
        };
        return result.is_valid().then_some(result);
    }
    super::placement_of(layer, width, height)
}

pub(super) fn transform(placement: LayerPlacement, width: u32, height: u32) -> Transform {
    let sx = placement.width / width as f32 * if placement.flip_x { -1. } else { 1. };
    let sy = placement.height / height as f32 * if placement.flip_y { -1. } else { 1. };
    let (sin, cos) = placement.rotation.to_radians().sin_cos();
    let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    let (cx, cy) = (width as f32 * 0.5, height as f32 * 0.5);
    Transform {
        a,
        b,
        c,
        d,
        tx: placement.x + placement.width * 0.5 - a * cx - c * cy,
        ty: placement.y + placement.height * 0.5 - b * cx - d * cy,
    }
}

pub(super) fn canvas_bounds(
    document: &Document,
    selection: &Option<super::Selection>,
) -> Result<Option<Bounds>> {
    ensure!(
        crate::model::valid_dimensions(document.width, document.height),
        "Invalid canvas size"
    );
    if let Some(selection) = selection {
        ensure!(
            selection.width == document.width
                && selection.height == document.height
                && selection.mask.len() == document.width as usize * document.height as usize,
            "Selection dimensions do not match the canvas"
        );
        Ok(selection
            .bounds()
            .map(|(x, y, w, h)| (x as f32, y as f32, (x + w) as f32, (y + h) as f32)))
    } else {
        Ok(Some((
            0.,
            0.,
            document.width as f32,
            document.height as f32,
        )))
    }
}

pub(super) fn contains(transform: Transform, width: u32, height: u32, bounds: Bounds) -> bool {
    corners(bounds).into_iter().all(|(x, y)| {
        transform.local(x, y).is_some_and(|(x, y)| {
            x >= -0.0001
                && y >= -0.0001
                && x <= width as f32 + 0.0001
                && y <= height as f32 + 0.0001
        })
    })
}

fn corners((x0, y0, x1, y1): Bounds) -> [(f32, f32); 4] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn mask_pixels(layers: &[Layer]) -> u64 {
    layers.iter().fold(0u64, |total, layer| {
        total
            .saturating_add(
                layer
                    .mask
                    .as_ref()
                    .map_or(0, |mask| u64::from(mask.width()) * u64::from(mask.height())),
            )
            .saturating_add(mask_pixels(&layer.children))
    })
}

fn solid_pixel(mask: &RgbaImage, place: LayerPlacement) -> Option<Rgba<u8>> {
    if mask.width() > 2
        || mask.height() > 2
        || (place.width <= mask.width() as f32 && place.height <= mask.height() as f32)
    {
        return None;
    }
    let pixel = *mask.get_pixel_checked(0, 0)?;
    (pixel[3] == 255 && mask.pixels().all(|p| *p == pixel)).then_some(pixel)
}

fn sample_original(
    mask: &RgbaImage,
    x: i64,
    y: i64,
    outside: u8,
    clamp_edges: bool,
    clamp_all: bool,
) -> Rgba<u8> {
    let w = i64::from(mask.width());
    let h = i64::from(mask.height());
    // Legacy implicit masks clamp at their edge. Keep the interpolation halo
    // intact when materializing independent placement, including rotated masks.
    if clamp_all
        || (x >= 0 && y >= 0 && x < w && y < h)
        || (clamp_edges && x >= -3 && y >= -3 && x < w + 3 && y < h + 3)
    {
        *mask.get_pixel(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32)
    } else {
        Rgba([outside, outside, outside, 255])
    }
}

/// Prepare off the live document. The old pixels are copied exactly; only a
/// solid 1/2-pixel placeholder is materialized at document-pixel resolution.
pub(super) fn prepare(
    document: &Document,
    id: &str,
    bounds: Bounds,
    allow_solid: bool,
) -> Result<Option<Growth>> {
    let layer = document
        .find_layer(id)
        .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
    let mask = layer
        .mask
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Layer has no mask"))?;
    effects::validate_mask_metadata(&layer.metadata)?;
    ensure!(
        crate::model::valid_dimensions(mask.width(), mask.height()),
        "Invalid mask size"
    );
    let place = placement(layer, document.width, document.height)
        .ok_or_else(|| anyhow::anyhow!("Invalid mask placement"))?;
    let solid = allow_solid.then(|| solid_pixel(mask, place)).flatten();
    let w = solid.map_or(mask.width(), |_| {
        place.width.ceil().max(mask.width() as f32) as u32
    });
    let h = solid.map_or(mask.height(), |_| {
        place.height.ceil().max(mask.height() as f32) as u32
    });
    let old_transform = transform(place, w, h);
    if solid.is_none() && contains(old_transform, w, h, bounds) {
        return Ok(None);
    }
    let mut limits = (0f64, 0f64, f64::from(w), f64::from(h));
    for (x, y) in corners(bounds) {
        let (x, y) = old_transform
            .local(x, y)
            .ok_or_else(|| anyhow::anyhow!("Invalid mask transform"))?;
        ensure!(
            x.is_finite() && y.is_finite(),
            "Mask growth exceeds supported transform bounds"
        );
        limits.0 = limits.0.min(f64::from(x).floor());
        limits.1 = limits.1.min(f64::from(y).floor());
        limits.2 = limits.2.max(f64::from(x).ceil());
        limits.3 = limits.3.max(f64::from(y).ceil());
    }
    let width = limits.2 - limits.0;
    let height = limits.3 - limits.1;
    ensure!(
        width >= 1.
            && height >= 1.
            && width <= f64::from(crate::model::MAX_DIMENSION)
            && height <= f64::from(crate::model::MAX_DIMENSION)
            && width * height <= MAX_GROWN_MASK_PIXELS as f64,
        "Mask growth exceeds the 16-million-pixel editing limit; use a smaller selection or mask scale"
    );
    let (width, height) = (width as u32, height as u32);
    let pixels = u64::from(width) * u64::from(height);
    ensure!(
        mask_pixels(&document.layers)
            .saturating_sub(u64::from(mask.width()) * u64::from(mask.height()))
            .saturating_add(pixels)
            <= crate::model::MAX_PIXELS,
        "Mask growth exceeds the project mask pixel budget"
    );
    let center = old_transform.world(
        (limits.0 + width as f64 * 0.5) as f32,
        (limits.1 + height as f64 * 0.5) as f32,
    );
    let next = LayerPlacement {
        x: center.0 - place.width / w as f32 * width as f32 * 0.5,
        y: center.1 - place.height / h as f32 * height as f32 * 0.5,
        width: place.width / w as f32 * width as f32,
        height: place.height / h as f32 * height as f32,
        ..place
    };
    ensure!(
        next.is_valid(),
        "Expanded mask placement exceeds supported bounds"
    );
    let outside = effects::mask_outside_coverage(&layer.metadata, mask);
    let clamp_edges = metadata_placement(layer, "maskPlacement").is_none();
    let source = Source {
        offset: ((-limits.0) as u32, (-limits.1) as u32),
        outside,
        clamp_edges,
        clamp_all: clamp_edges && (layer.is_group() || layer.image.is_none()),
        solid,
        source_size: (w, h),
    };
    let image = RgbaImage::from_fn(width, height, |x, y| source.pixel(mask, x, y));
    Ok(Some(Growth {
        image: image.into(),
        placement: next,
        source,
        old_width: mask.width(),
        sampling: layer
            .metadata
            .pointer("/maskPlacement/sampling")
            .or_else(|| layer.metadata.pointer("/transform/sampling"))
            .and_then(|v| v.as_str())
            .unwrap_or("High quality")
            .to_owned(),
    }))
}

pub(super) fn apply(layer: &mut Layer, growth: &Growth) {
    layer.mask = Some(growth.image.clone());
    set_metadata_placement(layer, "maskPlacement", growth.placement);
    layer.metadata["maskPlacement"]["sampling"] = serde_json::json!(growth.sampling);
    layer.metadata["maskOutsideCoverage"] = serde_json::json!(growth.source.outside);
}

impl super::Editor {
    /// Consume the explanation for a rejected mask stroke. UI callers should
    /// show this after begin/continue/finish, including rejected growth midway.
    pub fn take_mask_paint_error(&mut self) -> Option<String> {
        self.mask_paint_error.take()
    }

    pub(super) fn ensure_mask_dab_grid(&mut self, x: f32, y: f32, radius: f32) -> Result<()> {
        let Some(stroke) = &self.stroke else {
            return Ok(());
        };
        if !stroke.mask_target || stroke.mask_growth.is_some() || stroke.overflow.get() {
            return Ok(());
        }
        let Some(bounds) = stroke.mask_paint_bounds else {
            return Ok(());
        };
        let dab = (
            (x - radius).max(bounds.0),
            (y - radius).max(bounds.1),
            (x + radius).min(bounds.2),
            (y + radius).min(bounds.3),
        );
        if dab.0 >= dab.2 || dab.1 >= dab.3 {
            return Ok(());
        }
        let id = stroke.layer_id.clone();
        let layer = self
            .document
            .find_layer(&id)
            .ok_or_else(|| anyhow::anyhow!("Mask layer no longer exists"))?;
        let mask = layer
            .mask
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Layer has no mask"))?;
        effects::validate_mask_metadata(&layer.metadata)?;
        let place = placement(layer, self.document.width, self.document.height)
            .ok_or_else(|| anyhow::anyhow!("Invalid mask placement"))?;
        let allow_solid = stroke.coverage.borrow().is_empty();
        let grow = (allow_solid && solid_pixel(mask, place).is_some())
            || !contains(
                transform(place, mask.width(), mask.height()),
                mask.width(),
                mask.height(),
                dab,
            );
        let outside = effects::mask_outside_coverage(&layer.metadata, mask);
        if grow && let Some(growth) = prepare(&self.document, &id, bounds, allow_solid)? {
            let stroke = self.stroke.as_mut().unwrap();
            let old = std::mem::take(stroke.coverage.get_mut());
            let new_width = growth.image.width() as usize;
            for (index, amount) in old {
                let x = index % growth.old_width as usize + growth.source.offset.0 as usize;
                let y = index / growth.old_width as usize + growth.source.offset.1 as usize;
                stroke.coverage.get_mut().insert(y * new_width + x, amount);
            }
            if let Some(previous) = stroke.damage.as_mut() {
                previous.x += growth.source.offset.0;
                previous.y += growth.source.offset.1;
            }
            apply(self.document.find_layer_mut(&id).unwrap(), &growth);
            stroke.mask_growth = Some(growth.source);
        } else {
            self.document.find_layer_mut(&id).unwrap().metadata["maskOutsideCoverage"] =
                serde_json::json!(outside);
        }
        Ok(())
    }

    pub(super) fn growing_mask_work_editor(&self, id: &str) -> Result<super::Editor> {
        let mut work = self.mask_work_editor(id)?;
        let Some(bounds) = canvas_bounds(&self.document, &self.selection)? else {
            return Ok(work);
        };
        if let Some(growth) = prepare(&self.document, id, bounds, true)? {
            let layer = work.document.find_layer_mut(&work.active_layer).unwrap();
            layer.image = Some(growth.image);
            super::restore_placement_bounds(layer, growth.placement);
            set_metadata_placement(layer, "maskEditGrownPlacement", growth.placement);
            layer.metadata["maskEditGrownPlacement"]["sampling"] =
                serde_json::json!(growth.sampling);
            layer.metadata["maskOutsideCoverage"] = serde_json::json!(growth.source.outside);
        } else {
            let source = self.document.find_layer(id).unwrap();
            if metadata_placement(source, "maskPlacement").is_some() {
                work.document
                    .find_layer_mut(&work.active_layer)
                    .unwrap()
                    .metadata["maskOutsideCoverage"] = serde_json::json!(
                    effects::mask_outside_coverage(&source.metadata, source.mask.as_ref().unwrap())
                );
            }
        }
        // Fill and gradient reach only selected canvas pixels, preserving any
        // pre-existing mask pixels retained beyond the current document.
        if work.selection.is_none() {
            work.select_all();
        }
        Ok(work)
    }
}
