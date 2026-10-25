//! Read-only conservative retained-owner accounting for optional image-job
//! admission. Shared snapshots are counted repeatedly; no history is trimmed,
//! no stroke is finished, and no source/cache is loaded or changed.
use super::*;
use crate::create_project::resident_memory::{document_resident_bytes, layer_resident_bytes};
use anyhow::{Context, Result};
use std::mem::size_of;
fn selection_bytes(selection: &Option<Selection>) -> u64 {
    selection.as_ref().map_or(0, |s| {
        (size_of::<Selection>() as u64).saturating_add(s.mask.capacity() as u64)
    })
}
fn dynamics_bytes(s: &crate::brush_dynamics::Settings) -> u64 {
    let mut bytes = (size_of::<crate::brush_dynamics::Settings>() as u64).saturating_add(
        (s.pressure_curve.capacity() as u64)
            .saturating_mul(size_of::<crate::brush_dynamics::CurvePoint>() as u64),
    );
    if let crate::brush_dynamics::Tip::Custom(tip) = &s.tip {
        bytes = bytes.saturating_add(tip.pixels.capacity() as u64);
    }
    bytes
}
fn snapshot_bytes(snapshot: &Snapshot) -> Result<u64> {
    Ok((size_of::<Snapshot>() as u64)
        .saturating_add(snapshot.active_layer.capacity() as u64)
        .saturating_add(selection_bytes(&snapshot.selection))
        .saturating_add(document_resident_bytes(&snapshot.document)?))
}
fn entry_bytes(entry: &HistoryEntry) -> Result<u64> {
    let mut bytes = snapshot_bytes(entry.state())?;
    if let HistoryEntry::Raster(patch) = entry {
        let pixels=patch.pixels.try_borrow().context("A cached page raster history is being edited; retry memory admission after it finishes")?;
        bytes = bytes
            .saturating_add(pixels.owned_bytes() as u64)
            .saturating_add(patch.layer_id.capacity() as u64)
            .saturating_add(size_of::<RasterHistory>() as u64);
    }
    Ok(bytes)
}
impl Editor {
    pub fn resident_memory_estimate(&self) -> Result<u64> {
        let mut bytes = (size_of::<Self>() as u64)
            .saturating_add(document_resident_bytes(&self.document)?)
            .saturating_add(self.active_layer.capacity() as u64)
            .saturating_add(selection_bytes(&self.selection))
            .saturating_add(
                (self.undo.capacity() as u64).saturating_mul(size_of::<HistoryEntry>() as u64),
            )
            .saturating_add(
                (self.redo.capacity() as u64).saturating_mul(size_of::<HistoryEntry>() as u64),
            )
            .saturating_add(
                self.mask_paint_error
                    .as_ref()
                    .map_or(0, |s| s.capacity() as u64),
            )
            .saturating_add(
                self.layout_error
                    .as_ref()
                    .map_or(0, |s| s.capacity() as u64),
            );
        if let Some(dynamics) = &self.brush_dynamics {
            bytes = bytes.saturating_add(dynamics_bytes(dynamics));
        }
        for entry in self.undo.iter().chain(&self.redo) {
            bytes = bytes.saturating_add(entry_bytes(entry)?);
        }
        if let Some(stroke) = &self.stroke {
            bytes = bytes
                .saturating_add(size_of::<Stroke>() as u64)
                .saturating_add(entry_bytes(&stroke.before)?)
                .saturating_add(stroke.layer_id.capacity() as u64);
            if let Some(image) = &stroke.clone_canvas {
                bytes = bytes.saturating_add(image.as_raw().capacity() as u64);
            }
            if let Some(dynamics) = &stroke.dynamics {
                bytes = bytes.saturating_add(dynamics_bytes(dynamics.settings()));
            }
            let coverage = stroke.coverage.try_borrow().context(
                "A cached page stroke is updating; retry memory admission after it finishes",
            )?;
            bytes = bytes.saturating_add((coverage.capacity() as u64).saturating_mul(64));
        }
        if let Some(floating) = &self.floating {
            bytes = bytes
                .saturating_add(size_of::<FloatingSelection>() as u64)
                .saturating_add(snapshot_bytes(&floating.before)?)
                .saturating_add(selection_bytes(&floating.before_selection))
                .saturating_add(floating.source_id.capacity() as u64)
                .saturating_add(floating.floating_id.capacity() as u64)
                .saturating_add(floating.selection_mask.as_raw().capacity() as u64)
                .saturating_add(layer_resident_bytes(&floating.initial_floating, 0)?);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_counts_selection_history_redo_and_retained_snapshots_without_changing_them() {
        let mut editor = Editor::new(Document::new(8, 6));
        let base = editor.resident_memory_estimate().unwrap();
        let revision = editor.revision();
        let mut mask = Vec::with_capacity(4096);
        mask.resize(48, 128);
        editor.selection = Some(Selection {
            width: 8,
            height: 6,
            mask,
        });
        let selected = editor.resident_memory_estimate().unwrap();
        assert!(selected >= base + 4096);
        assert_eq!(editor.revision(), revision);
        assert!(editor.record_selection_change(None));
        let undo = editor.resident_memory_estimate().unwrap();
        assert!(undo > selected);
        assert_eq!(editor.undo_depth(), 1);
        assert_eq!(editor.revision(), revision);
        assert!(editor.undo());
        let redo = editor.resident_memory_estimate().unwrap();
        assert!(redo > base);
        assert_eq!(editor.redo_depth(), 1);
        assert!(editor.redo());
        assert_eq!(editor.undo_depth(), 1);
    }
    #[test]
    fn floating_owner_is_counted_without_committing_or_cancelling_it() {
        let mut editor = Editor::new(Document::new(8, 6));
        editor.document.layers[0].image = Some(RgbaImage::from_pixel(8, 6, Rgba([200; 4])).into());
        editor.select_rect(1, 1, 3, 2);
        let before = editor.resident_memory_estimate().unwrap();
        assert!(editor.begin_floating_selection().unwrap().is_some());
        let floating = editor.floating_selection_layer().unwrap().to_owned();
        let depth = editor.undo_depth();
        let revision = editor.revision();
        let estimated = editor.resident_memory_estimate().unwrap();
        assert!(estimated > before);
        assert_eq!(editor.floating_selection_layer(), Some(floating.as_str()));
        assert_eq!(editor.undo_depth(), depth);
        assert_eq!(editor.revision(), revision);
    }
    #[test]
    fn region_history_payload_is_charged_and_a_busy_payload_refuses_estimation() {
        let mut editor = Editor::new(Document::new(32, 32));
        let baseline = editor.resident_memory_estimate().unwrap();
        let id = editor.active_layer.clone();
        let mut patch = raster_patch::RasterPatch::new(32, 32);
        assert!(patch.capture(editor.document.layers[0].image.as_ref().unwrap(), 0, 0));
        let payload = patch.owned_bytes() as u64;
        let state = editor.snapshot_without_raster(&id);
        let expected_revision = editor.revision();
        editor.undo.push_back(HistoryEntry::Raster(RasterHistory {
            state,
            layer_id: id,
            expected_revision,
            pixels: std::cell::RefCell::new(patch),
        }));
        assert!(editor.resident_memory_estimate().unwrap() >= baseline + payload);
        let HistoryEntry::Raster(patch) = &editor.undo[0] else {
            panic!()
        };
        let _busy = patch.pixels.borrow_mut();
        assert!(
            editor
                .resident_memory_estimate()
                .unwrap_err()
                .to_string()
                .contains("history is being edited")
        );
        assert!(
            crate::camera_raw::native::ui_admission_for_editor(
                &editor,
                (32, 32),
                true,
                false,
                0,
                0
            )
            .unwrap_err()
            .to_string()
            .contains("history is being edited")
        );
    }
}
