//! Bounded native retouch with a shared-pixel capture, cancellable worker and
//! stale-safe commit. No live editor or Undo stack is moved into the worker.
use super::*;
use anyhow::{Context, Result, ensure};
use std::sync::{Arc, atomic::AtomicBool};

/// Immutable input for one background retouch job. Preparing this request shares
/// raster allocations and copies selection coverage fallibly; pixel work starts
/// only in `compute`. The caller owns admission of concurrent worker jobs.
pub struct RetouchRequest {
    before: Snapshot,
    fence: RetouchFence,
    target: String,
    mask: bool,
    mode: RetouchMode,
    points: Vec<(f32, f32)>,
    brush: Brush,
}

/// A completed job can be applied only to the editor state that requested it.
/// Dropping this value, including after cancellation, has no document effect.
pub struct PreparedRetouch {
    before: Snapshot,
    fence: RetouchFence,
    target: String,
    output: Option<RgbaImage>,
    mask_metadata: Option<serde_json::Value>,
}

struct RetouchFence {
    instance: u64,
    next_revision: u64,
    selection_revision: u64,
}

impl Editor {
    /// Capture the active raster or existing mask without copying its pixels.
    /// Finish other gestures first. This method never changes history or artwork.
    pub fn prepare_retouch(
        &self,
        points: Vec<(f32, f32)>,
        mode: RetouchMode,
        mask: bool,
    ) -> Result<RetouchRequest> {
        self.prepare_retouch_target(&self.active_layer, points, mode, mask)
    }

    fn prepare_retouch_target(
        &self,
        target: &str,
        points: Vec<(f32, f32)>,
        mode: RetouchMode,
        mask: bool,
    ) -> Result<RetouchRequest> {
        ensure!(self.stroke.is_none(), "Finish the active stroke first");
        ensure!(
            self.floating.is_none(),
            "Commit or cancel the floating selection first"
        );
        ensure!(
            points.len() <= 100_000,
            "Retouch stroke has too many points"
        );
        ensure!(
            !locked_in_tree(&self.document.layers, target, false),
            "Retouch layer is locked"
        );
        let layer = self
            .document
            .find_layer(target)
            .context("Layer not found")?;
        let image = if mask {
            layer.mask.as_ref().context("Layer has no mask")?
        } else {
            ensure!(
                !is_live_object(layer),
                "Retouch requires an 8-bit raster layer"
            );
            layer
                .image
                .as_ref()
                .context("Retouch requires a raster layer")?
        };
        ensure!(
            u64::from(image.width()) * u64::from(image.height()) <= 16_777_216,
            "Retouch supports images and masks up to 16 million pixels"
        );
        ensure!(
            self.selection.as_ref().is_none_or(|selection| {
                selection.width == self.document.width
                    && selection.height == self.document.height
                    && selection.mask.len()
                        == self.document.width as usize * self.document.height as usize
            }),
            "Selection dimensions do not match the canvas"
        );
        let selection_bytes = self.selection.as_ref().map_or(0, |s| s.mask.len());
        ensure_working_memory(
            points
                .capacity()
                .saturating_mul(std::mem::size_of::<(f32, f32)>())
                .saturating_add(selection_bytes)
                .saturating_add(image.as_raw().len()),
        )?;
        let selection = copy_selection(&self.selection)?;
        Ok(RetouchRequest {
            before: Snapshot {
                document: self.document.clone(),
                active_layer: self.active_layer.clone(),
                selection,
                revision: self.revision,
                // Compute this document-wide accounting on the worker.
                bytes: 0,
            },
            fence: RetouchFence {
                instance: self.instance_id,
                next_revision: self.next_revision,
                selection_revision: self.selection_revision,
            },
            target: target.to_owned(),
            mask,
            mode,
            points,
            brush: self.brush.clone(),
        })
    }

    /// Commit completed pixels once, or reject cancellation/stale inputs before
    /// any mutation. Public selection/document edits are checked as well as the
    /// normal editor revision, and pixel identity checks do not scan rasters.
    pub fn apply_prepared_retouch(
        &mut self,
        prepared: PreparedRetouch,
        cancelled: &AtomicBool,
    ) -> Result<bool> {
        crate::retouch_brush::check_cancelled(cancelled)?;
        ensure!(self.stroke.is_none(), "Finish the active stroke first");
        ensure!(
            self.floating.is_none(),
            "Commit or cancel the floating selection first"
        );
        ensure!(
            self.instance_id == prepared.fence.instance
                && self.revision == prepared.before.revision
                && self.next_revision == prepared.fence.next_revision
                && self.selection_revision == prepared.fence.selection_revision
                && self.active_layer == prepared.before.active_layer
                && self.selection == prepared.before.selection
                && same_document(&self.document, &prepared.before.document),
            "Retouch result is stale; the document, selection or active layer changed"
        );
        let Some(output) = prepared.output else {
            return Ok(false);
        };
        self.undo
            .try_reserve(1)
            .map_err(|_| anyhow::anyhow!("Not enough memory for retouch Undo history"))?;
        crate::retouch_brush::check_cancelled(cancelled)?;
        let layer = self
            .document
            .find_layer_mut(&prepared.target)
            .context("Retouch target no longer exists")?;
        if let Some(metadata) = prepared.mask_metadata {
            layer.mask = Some(output.into());
            layer.metadata = metadata;
        } else {
            layer.image = Some(output.into());
        }
        self.commit(prepared.before);
        Ok(true)
    }

    /// Synchronous compatibility API, using the same capture/compute/commit path.
    pub fn retouch_stroke(&mut self, points: &[(f32, f32)], mode: RetouchMode) -> Result<bool> {
        self.retouch_stroke_cancellable(points, mode, &AtomicBool::new(false))
    }

    pub fn retouch_stroke_cancellable(
        &mut self,
        points: &[(f32, f32)],
        mode: RetouchMode,
        cancelled: &AtomicBool,
    ) -> Result<bool> {
        crate::retouch_brush::check_cancelled(cancelled)?;
        self.finish_stroke();
        if self.editable_layer().is_none() {
            return Ok(false);
        }
        let request = self.prepare_retouch(copy_points(points)?, mode, false)?;
        self.apply_prepared_retouch(request.compute(cancelled)?, cancelled)
    }

    pub fn retouch_mask_stroke(
        &mut self,
        id: &str,
        points: &[(f32, f32)],
        mode: RetouchMode,
    ) -> Result<bool> {
        self.retouch_mask_stroke_cancellable(id, points, mode, &AtomicBool::new(false))
    }

    pub fn retouch_mask_stroke_cancellable(
        &mut self,
        id: &str,
        points: &[(f32, f32)],
        mode: RetouchMode,
        cancelled: &AtomicBool,
    ) -> Result<bool> {
        crate::retouch_brush::check_cancelled(cancelled)?;
        self.finish_stroke();
        let request = self.prepare_retouch_target(id, copy_points(points)?, mode, true)?;
        self.apply_prepared_retouch(request.compute(cancelled)?, cancelled)
    }
}

fn copy_selection(selection: &Option<Selection>) -> Result<Option<Selection>> {
    selection
        .as_ref()
        .map(|selection| {
            let mut mask = Vec::new();
            crate::retouch_brush::reserve(&mut mask, selection.mask.len(), "selection history")?;
            mask.extend_from_slice(&selection.mask);
            Ok(Selection {
                width: selection.width,
                height: selection.height,
                mask,
            })
        })
        .transpose()
}

fn copy_points(points: &[(f32, f32)]) -> Result<Vec<(f32, f32)>> {
    ensure!(
        points.len() <= 100_000,
        "Retouch stroke has too many points"
    );
    let mut owned = Vec::new();
    crate::retouch_brush::reserve(&mut owned, points.len(), "stroke capture")?;
    owned.extend_from_slice(points);
    Ok(owned)
}

fn ensure_working_memory(bytes: usize) -> Result<()> {
    ensure!(
        bytes <= crate::retouch_brush::MAX_WORKING_BYTES,
        "Retouch exceeds the working memory budget"
    );
    Ok(())
}

// An editor's document is public for import and fixture construction. Compare
// its structure and retained allocation identities as a second stale fence.
// SharedImage/Arc copy-on-write ensures direct mutations change that identity.
fn same_document(a: &Document, b: &Document) -> bool {
    a.width == b.width
        && a.height == b.height
        && a.name == b.name
        && a.background == b.background
        && a.metadata == b.metadata
        && same_layers(&a.layers, &b.layers)
}

fn same_arc<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn same_image(
    a: &Option<crate::shared_image::SharedImage>,
    b: &Option<crate::shared_image::SharedImage>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.shares_pixels_with(b),
        (None, None) => true,
        _ => false,
    }
}

fn same_layers(a: &[Layer], b: &[Layer]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.name == b.name
                && a.visible == b.visible
                && a.locked == b.locked
                && a.opacity.to_bits() == b.opacity.to_bits()
                && a.blend_mode == b.blend_mode
                && a.offset_x.to_bits() == b.offset_x.to_bits()
                && a.offset_y.to_bits() == b.offset_y.to_bits()
                && a.rotation.to_bits() == b.rotation.to_bits()
                && a.scale_x.to_bits() == b.scale_x.to_bits()
                && a.scale_y.to_bits() == b.scale_y.to_bits()
                && a.metadata == b.metadata
                && same_image(&a.image, &b.image)
                && same_image(&a.mask, &b.mask)
                && same_arc(&a.advanced, &b.advanced)
                && same_arc(&a.vector_scene, &b.vector_scene)
                && same_layers(&a.children, &b.children)
        })
}

impl RetouchRequest {
    /// Compute native pixels on a worker. Every large raster allocation and the
    /// existing bounded kernel run here; cancellation leaves the editor alone.
    pub fn compute(mut self, cancelled: &AtomicBool) -> Result<PreparedRetouch> {
        crate::retouch_brush::check_cancelled(cancelled)?;
        self.before.bytes = document_bytes(&self.before.document).saturating_add(
            self.before
                .selection
                .as_ref()
                .map_or(0, |s| s.mask.capacity()),
        );
        let (output, mask_metadata) = if self.mask {
            self.compute_mask(cancelled)?
        } else {
            let layer = self
                .before
                .document
                .find_layer(&self.target)
                .context("Layer not found")?;
            let original = layer.image.as_ref().context("Layer has no raster")?;
            let transform = Transform::for_layer(&self.before.document, &self.target)
                .context("Invalid layer transform")?;
            (
                self.native_retouch_output(
                    original,
                    &self.points,
                    self.mode,
                    transform,
                    crate::retouch_brush::EdgeMode::Constant([0; 4]),
                    0,
                    cancelled,
                )?,
                None,
            )
        };
        crate::retouch_brush::check_cancelled(cancelled)?;
        Ok(PreparedRetouch {
            before: self.before,
            fence: self.fence,
            target: self.target,
            output,
            mask_metadata,
        })
    }

    fn compute_mask(
        &self,
        cancelled: &AtomicBool,
    ) -> Result<(Option<RgbaImage>, Option<serde_json::Value>)> {
        let document = &self.before.document;
        let layer = document
            .find_layer(&self.target)
            .context("Layer not found")?;
        let original = layer.mask.as_ref().context("Layer has no mask")?;
        let transform =
            Transform::for_mask(document, &self.target).context("Invalid mask placement")?;
        let outside = crate::effects::mask_outside_coverage(&layer.metadata, original);
        let preserve_grid = if (layer.is_group() || layer.image.is_none())
            && layer.metadata.get("maskOutsideCoverage").is_none()
            && layer.metadata.get("maskPlacement").is_some()
        {
            mask_growth::placement(layer, document.width, document.height).map(|placement| {
                let sampling = layer
                    .metadata
                    .pointer("/transform/sampling")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("High quality")
                    .to_owned();
                (placement, sampling)
            })
        } else {
            None
        };
        // Imported masks may encode coverage with alpha or coloured RGB. Keep
        // the old coverage calculation, untouched bytes and infinite ground.
        let mut requires_normalization = false;
        for (i, p) in original.pixels().enumerate() {
            if i & 4095 == 0 {
                crate::retouch_brush::check_cancelled(cancelled)?;
            }
            if p[0] != p[1] || p[1] != p[2] || p[3] != 255 {
                requires_normalization = true;
                break;
            }
        }
        let normalized = if requires_normalization {
            ensure_working_memory(
                self.points
                    .capacity()
                    .saturating_mul(std::mem::size_of::<(f32, f32)>())
                    .saturating_add(
                        self.before
                            .selection
                            .as_ref()
                            .map_or(0, |s| s.mask.capacity()),
                    )
                    .saturating_add(original.as_raw().len().saturating_mul(2)),
            )?;
            let mut image = crate::retouch_brush::copy_image(original)?;
            for (i, p) in image.pixels_mut().enumerate() {
                if i & 4095 == 0 {
                    crate::retouch_brush::check_cancelled(cancelled)?;
                }
                let gray = ((0.2126 * f32::from(p[0])
                    + 0.7152 * f32::from(p[1])
                    + 0.0722 * f32::from(p[2]))
                    * f32::from(p[3])
                    / 255.)
                    .round() as u8;
                *p = Rgba([gray, gray, gray, 255]);
            }
            Some(image)
        } else {
            None
        };
        let source = normalized.as_ref().unwrap_or(original);
        let Some(mut output) = self.native_retouch_output(
            source,
            &self.points,
            self.mode,
            transform,
            crate::retouch_brush::EdgeMode::Constant([outside, outside, outside, 255]),
            normalized.as_ref().map_or(0, |i| i.as_raw().len()),
            cancelled,
        )?
        else {
            return Ok((None, None));
        };
        if normalized.is_some() {
            for (i, ((pixel, old), coverage)) in output
                .pixels_mut()
                .zip(original.pixels())
                .zip(source.pixels())
                .enumerate()
            {
                if i & 4095 == 0 {
                    crate::retouch_brush::check_cancelled(cancelled)?;
                }
                if pixel == coverage {
                    *pixel = *old;
                }
            }
        }
        // Prepare metadata on the worker so applying pixels performs no JSON
        // construction, including the special legacy folder-mask placement fix.
        let mut metadata_layer = layer.clone();
        if let Some((placement, sampling)) = preserve_grid {
            set_metadata_placement(&mut metadata_layer, "maskPlacement", placement);
            metadata_layer.metadata["maskPlacement"]["sampling"] = sampling.into();
        }
        metadata_layer.metadata["maskOutsideCoverage"] = serde_json::json!(outside);
        crate::retouch_brush::check_cancelled(cancelled)?;
        Ok((Some(output), Some(metadata_layer.metadata)))
    }

    /// Selection limits writes, never the sampling source. This preserves the
    /// established ability to pull neighbouring colour across a selection edge.
    fn native_retouch_output(
        &self,
        source: &RgbaImage,
        points: &[(f32, f32)],
        mode: RetouchMode,
        transform: Transform,
        edge: crate::retouch_brush::EdgeMode,
        reserved_bytes: usize,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> anyhow::Result<Option<RgbaImage>> {
        anyhow::ensure!(
            points.len() <= 100_000,
            "Retouch stroke has too many points"
        );
        anyhow::ensure!(
            self.before.selection.as_ref().is_none_or(|selection| {
                selection.width == self.before.document.width
                    && selection.height == self.before.document.height
                    && selection.mask.len()
                        == self.before.document.width as usize
                            * self.before.document.height as usize
            }),
            "Selection dimensions do not match the canvas"
        );
        // Admit all retained caller buffers before allocating the converted
        // point vector, including a result raster reserved by the kernel next.
        // A near-limit selection must not briefly exceed the cap before refusal.
        ensure_working_memory(
            reserved_bytes
                .saturating_add(
                    self.before
                        .selection
                        .as_ref()
                        .map_or(0, |s| s.mask.capacity()),
                )
                .saturating_add(
                    self.points
                        .capacity()
                        .saturating_mul(std::mem::size_of::<(f32, f32)>()),
                )
                .saturating_add(
                    points
                        .len()
                        .saturating_mul(std::mem::size_of::<crate::retouch_brush::StrokePoint>()),
                )
                .saturating_add(source.as_raw().len()),
        )?;
        let mut stroke_points = Vec::new();
        crate::retouch_brush::reserve(&mut stroke_points, points.len(), "stroke points")?;
        stroke_points.extend(
            points
                .iter()
                .map(|&(x, y)| crate::retouch_brush::StrokePoint { x, y }),
        );
        let reserved_bytes = reserved_bytes
            .saturating_add(
                stroke_points.capacity() * std::mem::size_of::<crate::retouch_brush::StrokePoint>(),
            )
            .saturating_add(
                self.before
                    .selection
                    .as_ref()
                    .map_or(0, |s| s.mask.capacity()),
            )
            .saturating_add(self.points.capacity() * std::mem::size_of::<(f32, f32)>());
        let mut output = crate::retouch_brush::apply_native_cancellable(
            source,
            &stroke_points,
            self.brush.size,
            self.brush.hardness.min(0.98),
            self.brush.opacity,
            mode,
            crate::retouch_brush::NativeOptions {
                transform: [
                    transform.a,
                    transform.b,
                    transform.c,
                    transform.d,
                    transform.tx,
                    transform.ty,
                ],
                blur_radius: self.brush.blur_radius,
                edge,
                reserved_bytes,
            },
            cancelled,
        )?;
        let mut changed = false;
        for y in 0..output.height() {
            crate::retouch_brush::check_cancelled(cancelled)?;
            for x in 0..output.width() {
                let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
                let original = source.get_pixel(x, y).0;
                let pixel = output.get_pixel_mut(x, y);
                if pixel.0 == original {
                    continue;
                }
                let coverage = if wx < 0.
                    || wy < 0.
                    || wx >= self.before.document.width as f32
                    || wy >= self.before.document.height as f32
                {
                    0.
                } else {
                    self.before
                        .selection
                        .as_ref()
                        .map_or(1., |s| f32::from(s.sampled_coverage(wx, wy)) / 255.)
                };
                // Keep hidden RGB byte-exact outside the edited coverage.
                pixel.0 = if coverage == 0. {
                    original
                } else {
                    blend_coverage(original, pixel.0, coverage)
                };
                changed |= pixel.0 != original;
            }
        }
        crate::retouch_brush::check_cancelled(cancelled)?;
        Ok(changed.then_some(output))
    }
}
