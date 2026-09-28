//! Reversible raster regions alongside the existing full document snapshots.
//!
//! A region entry keeps the complete editor-state snapshot with only the
//! painted raster removed. All other pixels remain immutable shared owners.
//! This retains snapshot semantics without forcing the painted raster to copy
//! merely because a stroke began.
use super::{Editor, HistoryEntry, RasterHistory, Snapshot, is_live_object};
use crate::model::Document;
use crate::shared_image::SharedImage;
use image::{Rgba, RgbaImage};

impl HistoryEntry {
    pub(super) fn state(&self) -> &Snapshot {
        match self {
            Self::Snapshot(state) => state,
            Self::Raster(patch) => &patch.state,
        }
    }

    pub(super) fn extra_bytes(&self) -> usize {
        match self {
            Self::Snapshot(_) => 0,
            Self::Raster(patch) => patch
                .pixels
                .borrow()
                .owned_bytes()
                .saturating_add(patch.layer_id.capacity())
                .saturating_add(std::mem::size_of::<RasterHistory>()),
        }
    }

    /// Resolve a snapshot's source once per dab, not once per affected pixel.
    pub(super) fn original_image(&self, layer_id: &str, mask_target: bool) -> Option<&RgbaImage> {
        let Self::Snapshot(state) = self else {
            return None;
        };
        let layer = state.document.find_layer(layer_id)?;
        let image = if mask_target {
            &layer.mask
        } else {
            &layer.image
        };
        image.as_deref()
    }

    pub(super) fn original_pixel(
        &self,
        original: Option<&RgbaImage>,
        current: &RgbaImage,
        x: u32,
        y: u32,
    ) -> Option<Rgba<u8>> {
        match self {
            Self::Snapshot(_) => original?.get_pixel_checked(x, y).copied(),
            Self::Raster(patch) => patch.pixels.borrow().original_pixel(current, x, y),
        }
    }

    pub(super) fn capture(&self, image: &RgbaImage, x: u32, y: u32) -> bool {
        match self {
            Self::Snapshot(_) => true,
            Self::Raster(patch) => patch.pixels.borrow_mut().capture(image, x, y),
        }
    }

    pub(super) fn can_apply(&self, document: &Document, revision: u64) -> bool {
        match self {
            Self::Snapshot(_) => true,
            Self::Raster(patch) => {
                revision == patch.expected_revision
                    && (document.width, document.height)
                        == (patch.state.document.width, patch.state.document.height)
                    && patch
                        .state
                        .document
                        .find_layer(&patch.layer_id)
                        .is_some_and(|layer| layer.image.is_none() && !is_live_object(layer))
                    && document.find_layer(&patch.layer_id).is_some_and(|layer| {
                        !is_live_object(layer)
                            && layer
                                .image
                                .as_ref()
                                .is_some_and(|image| patch.pixels.borrow().validate(image))
                    })
            }
        }
    }
}

impl Editor {
    pub(super) fn stroke_history(&self) -> HistoryEntry {
        if self.region_history_enabled
            && self.selection.is_none()
            && self.floating.is_none()
            && let Some(image) = self.editable_layer().and_then(|layer| layer.image.as_ref())
            && image.as_raw().len() >= super::REGION_HISTORY_MIN_BYTES
            && super::raster_patch::RasterPatch::new(image.width(), image.height()).validate(image)
        {
            HistoryEntry::Raster(RasterHistory {
                state: self.snapshot_without_raster(&self.active_layer),
                layer_id: self.active_layer.clone(),
                expected_revision: self.revision,
                pixels: std::cell::RefCell::new(super::raster_patch::RasterPatch::new(
                    image.width(),
                    image.height(),
                )),
            })
        } else {
            HistoryEntry::Snapshot(self.snapshot())
        }
    }

    pub(super) fn snapshot_without_raster(&self, layer_id: &str) -> Snapshot {
        let mut state = self.snapshot();
        // The caller has already checked that the target exists. Dropping only
        // this cloned owner before writing is what avoids a whole-raster copy.
        state.document.find_layer_mut(layer_id).unwrap().image = None;
        state
    }

    /// Swap an entry with the current editor state. All fallible checks precede
    /// any raster write or state replacement; callers move deques only on success.
    pub(super) fn apply_history_entry(&mut self, entry: &mut HistoryEntry) -> bool {
        if !entry.can_apply(&self.document, self.revision) {
            return false;
        }
        match entry {
            HistoryEntry::Snapshot(previous) => {
                let current = self.snapshot();
                self.restore(std::mem::replace(previous, current));
            }
            HistoryEntry::Raster(patch) => {
                let current = self.snapshot_without_raster(&patch.layer_id);
                let mut image: SharedImage = self
                    .document
                    .find_layer_mut(&patch.layer_id)
                    .unwrap()
                    .image
                    .take()
                    .unwrap();
                if patch.pixels.borrow().tile_count() != 0 {
                    let allocation = image.allocation_id();
                    let detached_bytes = image.as_raw().len();
                    // Preflight above covered every tile. No fallible operation
                    // remains after taking the image from the live document.
                    let swapped = patch.pixels.borrow_mut().swap(&mut image);
                    debug_assert!(swapped);
                    if allocation != image.allocation_id() {
                        self.detached_raster_bytes =
                            self.detached_raster_bytes.saturating_add(detached_bytes);
                    }
                }
                patch
                    .state
                    .document
                    .find_layer_mut(&patch.layer_id)
                    .unwrap()
                    .image = Some(image);
                self.restore(std::mem::replace(&mut patch.state, current));
                patch.expected_revision = self.revision;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::PaintTool;

    fn editor() -> Editor {
        let mut editor = Editor::new(Document::new(520, 520));
        editor.brush.size = 24.;
        editor.brush.color = [230, 40, 70, 255];
        editor
    }

    #[test]
    fn overflow_rolls_back_all_tiles_without_changing_history_or_saved_revision() {
        let mut editor = editor();
        assert!(editor.begin_stroke(128., 128., 1., PaintTool::Brush));
        assert!(editor.finish_stroke());
        editor.mark_saved();
        let before = editor.document.clone();
        let undo = editor.undo_depth();
        let redo = editor.redo_depth();
        assert!(editor.begin_stroke(254., 254., 1., PaintTool::Brush));
        assert!(editor.continue_stroke(510., 510., 1.));
        // Exercise the same abort path as bounded stroke-coverage exhaustion
        // without allocating millions of coverage entries in a unit test.
        editor.stroke.as_ref().unwrap().overflow.set(true);
        assert!(!editor.finish_stroke());
        assert_eq!(editor.document.layers[0].image, before.layers[0].image);
        assert_eq!(editor.undo_depth(), undo);
        assert_eq!(editor.redo_depth(), redo);
        assert!(!editor.is_dirty());
        assert!(editor.take_stroke_damage().is_none());
    }

    #[test]
    fn invalid_live_geometry_or_revision_cannot_consume_a_patch_entry() {
        let mut editor = editor();
        assert!(editor.begin_stroke(128., 128., 1., PaintTool::Brush));
        assert!(editor.finish_stroke());
        let painted = editor.document.layers[0].image.take().unwrap();
        editor.document.layers[0].image = Some(RgbaImage::new(2, 3).into());
        let bytes = editor.history_bytes();
        assert!(!editor.undo());
        assert_eq!(editor.undo_depth(), 1);
        assert_eq!(editor.redo_depth(), 0);
        assert_eq!(editor.history_bytes(), bytes);
        assert_eq!(
            editor.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .dimensions(),
            (2, 3)
        );
        editor.document.layers[0].image = Some(painted);
        let saved_revision = editor.revision;
        editor.revision += 1;
        assert!(!editor.undo());
        assert_eq!(editor.undo_depth(), 1);
        editor.revision = saved_revision;
        assert!(editor.undo());
        assert!(editor.redo());
    }

    #[test]
    fn cancelling_an_unchanged_region_stroke_keeps_the_raster_allocation() {
        let mut editor = editor();
        let frozen = editor.document.layers[0].image.clone().unwrap();
        editor.brush.opacity = 0.;
        assert!(editor.begin_stroke(250., 250., 1., PaintTool::Brush));
        editor.cancel_stroke();
        assert!(frozen.shares_pixels_with(editor.document.layers[0].image.as_ref().unwrap()));
        assert_eq!(editor.history_stats().detached_raster_bytes, 0);
        assert_eq!(editor.undo_depth(), 0);
        assert!(!editor.is_dirty());
    }
}
