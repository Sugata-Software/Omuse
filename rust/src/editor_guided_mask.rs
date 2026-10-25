//! Canonical guarded mask preparation and commit helpers. Heavy coverage and
//! selection projection run off the UI thread; publication rechecks captured owners.
use super::*;
use crate::shared_image::SharedImage;
use anyhow::{Context, Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct SubjectMaskRequest {
    instance: u64,
    revision: u64,
    selection_revision: u64,
    layer: String,
    source: SharedImage,
    old_mask: Option<SharedImage>,
    old_advanced: Option<Arc<crate::advanced::LayerState>>,
    old_metadata: serde_json::Value,
    placement: [u32; 5],
    document: Document,
    subject: Arc<GrayImage>,
}
pub struct PreparedSubjectMask {
    instance: u64,
    revision: u64,
    selection_revision: u64,
    layer: String,
    source: SharedImage,
    old_mask: Option<SharedImage>,
    old_advanced: Option<Arc<crate::advanced::LayerState>>,
    old_metadata: serde_json::Value,
    placement: [u32; 5],
    mask: SharedImage,
}
impl SubjectMaskRequest {
    pub fn prepare(self, cancel: &AtomicBool) -> Result<(PreparedSubjectMask, Document)> {
        ensure!(
            !cancel.load(Ordering::Acquire),
            "Mask preparation cancelled"
        );
        let mut preview = Editor::new(self.document);
        ensure!(
            preview.apply_subject_mask_cancellable(&self.layer, &self.subject, false, cancel)?,
            "Unlock the image layer and parent groups"
        );
        let mask = preview
            .document
            .find_layer(&self.layer)
            .and_then(|l| l.mask.clone())
            .context("Prepared mask is missing")?;
        ensure!(
            !cancel.load(Ordering::Acquire),
            "Mask preparation cancelled"
        );
        Ok((
            PreparedSubjectMask {
                instance: self.instance,
                revision: self.revision,
                selection_revision: self.selection_revision,
                layer: self.layer,
                source: self.source,
                old_mask: self.old_mask,
                old_advanced: self.old_advanced,
                old_metadata: self.old_metadata,
                placement: self.placement,
                mask,
            },
            preview.document,
        ))
    }
}
pub struct PreparedSubjectSelection {
    guard: PreparedSubjectMask,
    selection: Selection,
}
impl SubjectMaskRequest {
    pub fn prepare_selection(self, cancel: &AtomicBool) -> Result<PreparedSubjectSelection> {
        ensure!(
            !cancel.load(Ordering::Acquire),
            "Selection preparation cancelled"
        );
        let mut editor = Editor::new(self.document);
        ensure!(
            editor.apply_subject_mask_cancellable(&self.layer, &self.subject, true, cancel)?,
            "Selection projection failed"
        );
        let selection = editor
            .selection
            .take()
            .context("Prepared selection missing")?;
        ensure!(
            !cancel.load(Ordering::Acquire),
            "Selection preparation cancelled"
        );
        // Guard's mask is only a dimensions witness; no mask is applied by this action.
        let witness = self.source.clone();
        Ok(PreparedSubjectSelection {
            guard: PreparedSubjectMask {
                instance: self.instance,
                revision: self.revision,
                selection_revision: self.selection_revision,
                layer: self.layer,
                source: self.source,
                old_mask: self.old_mask,
                old_advanced: self.old_advanced,
                old_metadata: self.old_metadata,
                placement: self.placement,
                mask: witness,
            },
            selection,
        })
    }
}
impl Editor {
    pub fn mask_review_ready(&self) -> Result<()> {
        ensure!(
            self.stroke.is_none() && self.floating.is_none(),
            "Finish or cancel the current stroke or floating selection before guided masking"
        );
        Ok(())
    }
    pub fn image_local_point(&self, id: &str, x: f32, y: f32) -> Option<(f32, f32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let p = Transform::for_layer(&self.document, id)?.local(x, y)?;
        (p.0.is_finite() && p.1.is_finite()).then_some(p)
    }
    pub fn image_world_point(&self, id: &str, x: f32, y: f32) -> Option<(f32, f32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let p = Transform::for_layer(&self.document, id)?.world(x, y);
        (p.0.is_finite() && p.1.is_finite()).then_some(p)
    }
    /// Same retained allocation accounting as history, for conservative optional
    /// model admission. This is an estimate, not a process/allocator RSS limit.
    pub fn mask_review_memory_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        document_bytes(&self.document)
            .saturating_add(retained_image_bytes(&self.document.layers, &mut seen))
            .saturating_add(self.history_bytes())
            .saturating_add(self.selection.as_ref().map_or(0, |s| s.mask.capacity()))
    }
    pub fn subject_mask_request(
        &self,
        id: &str,
        subject: Arc<GrayImage>,
    ) -> Result<SubjectMaskRequest> {
        ensure!(
            self.stroke.is_none() && self.floating.is_none(),
            "Finish or cancel the current stroke or floating selection before guided masking"
        );
        let layer = self
            .document
            .find_layer(id)
            .context("Choose an image layer")?;
        let source = layer.image.clone().context("Choose an image layer")?;
        ensure!(
            source.dimensions() == subject.dimensions(),
            "The editable mask dimensions changed"
        );
        ensure!(
            u64::from(source.width()) * u64::from(source.height()) <= 16_777_216
                && u64::from(self.document.width) * u64::from(self.document.height) <= 16_777_216,
            "Mask review supports up to 16 megapixels"
        );
        ensure!(
            !locked_in_tree(&self.document.layers, id, false),
            "Unlock the layer and parent groups"
        );
        Ok(SubjectMaskRequest {
            instance: self.instance_id(),
            revision: self.revision(),
            selection_revision: self.selection_revision(),
            layer: id.to_owned(),
            source,
            old_mask: layer.mask.clone(),
            old_advanced: layer.advanced.clone(),
            old_metadata: layer.metadata.clone(),
            placement: [
                layer.offset_x.to_bits(),
                layer.offset_y.to_bits(),
                layer.rotation.to_bits(),
                layer.scale_x.to_bits(),
                layer.scale_y.to_bits(),
            ],
            document: self.document.clone(),
            subject,
        })
    }
    fn validate_prepared_subject(&self, prepared: &PreparedSubjectMask) -> Result<()> {
        ensure!(
            self.stroke.is_none() && self.floating.is_none(),
            "Finish or cancel the current stroke or floating selection before applying a prepared mask"
        );
        ensure!(
            self.instance_id() == prepared.instance
                && self.revision() == prepared.revision
                && self.active_layer == prepared.layer
                && self.selection_revision() == prepared.selection_revision,
            "The prepared mask source changed"
        );
        let source = self
            .document
            .find_layer(&prepared.layer)
            .and_then(|l| l.image.as_ref())
            .context("The source image is no longer available")?;
        ensure!(
            source.shares_pixels_with(&prepared.source)
                && source.dimensions() == prepared.mask.dimensions(),
            "The prepared mask pixels or dimensions changed"
        );
        let layer = self.document.find_layer(&prepared.layer).unwrap();
        ensure!(
            match (&layer.mask, &prepared.old_mask) {
                (None, None) => true,
                (Some(a), Some(b)) => a.shares_pixels_with(b),
                _ => false,
            },
            "The prior mask changed"
        );
        ensure!(
            match (&layer.advanced, &prepared.old_advanced) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            },
            "The retained source or recipe changed"
        );
        ensure!(
            layer.metadata == prepared.old_metadata
                && [
                    layer.offset_x.to_bits(),
                    layer.offset_y.to_bits(),
                    layer.rotation.to_bits(),
                    layer.scale_x.to_bits(),
                    layer.scale_y.to_bits()
                ] == prepared.placement,
            "The layer placement or mask settings changed"
        );
        Ok(())
    }
    pub fn keep_prepared_subject_mask(&mut self, prepared: PreparedSubjectMask) -> Result<bool> {
        self.validate_prepared_subject(&prepared)?;
        ensure!(
            !locked_in_tree(&self.document.layers, &prepared.layer, false),
            "Unlock the layer and parent groups"
        );
        let before = self.snapshot();
        let layer = self.document.find_layer_mut(&prepared.layer).unwrap();
        layer.mask = Some(prepared.mask);
        metadata_bool(layer, "maskEnabled", true);
        metadata_bool(layer, "maskLinked", true);
        remove_metadata(layer, "maskPlacement");
        remove_metadata(layer, "maskOutsideCoverage");
        self.commit_checked(before)
    }
    /// Project only after explicit Use as selection, on a worker.
    pub fn prepare_subject_selection(
        &self,
        id: &str,
        subject: Arc<GrayImage>,
    ) -> Result<SubjectMaskRequest> {
        self.subject_mask_request(id, subject)
    }
    pub fn use_prepared_subject_selection(
        &mut self,
        prepared: PreparedSubjectSelection,
    ) -> Result<bool> {
        self.validate_prepared_subject(&prepared.guard)?;
        ensure!(
            prepared.selection.width == self.document.width
                && prepared.selection.height == self.document.height
                && prepared.selection.mask.len()
                    == self.document.width as usize * self.document.height as usize,
            "Prepared selection dimensions changed"
        );
        let previous = self.selection.take();
        self.selection = Some(prepared.selection);
        self.record_selection_change(previous);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_cancellation_before_or_during_rows_leaves_every_editor_owner_unchanged() {
        for as_selection in [false, true] {
            for precancel in [false, true] {
                let mut doc = Document::new(8, 8);
                doc.layers[0].image =
                    Some(RgbaImage::from_pixel(8, 8, Rgba([100, 130, 180, 255])).into());
                doc.layers[0].mask =
                    Some(RgbaImage::from_pixel(8, 8, Rgba([70, 70, 70, 255])).into());
                doc.layers[0].metadata["maskEnabled"] = serde_json::json!(false);
                let mut e = Editor::new(doc);
                e.select_rect(1, 1, 2, 2);
                let selection = e.selection.clone();
                let image = e.document.layers[0].image.clone().unwrap();
                let mask = e.document.layers[0].mask.clone().unwrap();
                let metadata = e.document.layers[0].metadata.clone();
                let revision = e.revision();
                let id = e.active_layer.clone();
                let cancel = AtomicBool::new(precancel);
                assert!(
                    e.apply_subject_mask_rows(
                        &id,
                        &GrayImage::from_pixel(8, 8, image::Luma([200])),
                        as_selection,
                        &cancel,
                        |row| {
                            if row == 3 {
                                cancel.store(true, Ordering::Release);
                            }
                        }
                    )
                    .is_err()
                );
                assert_eq!(e.undo_depth(), 0);
                assert_eq!(e.revision(), revision);
                assert_eq!(e.selection, selection);
                assert!(
                    e.document.layers[0]
                        .image
                        .as_ref()
                        .unwrap()
                        .shares_pixels_with(&image)
                );
                assert!(
                    e.document.layers[0]
                        .mask
                        .as_ref()
                        .unwrap()
                        .shares_pixels_with(&mask)
                );
                assert_eq!(e.document.layers[0].metadata, metadata);
            }
        }
    }
    #[test]
    fn floating_selection_never_admits_or_commits_prepared_guided_work() {
        let mut doc = Document::new(20, 12);
        doc.layers[0].image =
            Some(RgbaImage::from_pixel(20, 12, Rgba([100, 130, 180, 255])).into());
        let mut e = Editor::new(doc);
        let id = e.active_layer.clone();
        let mask = Arc::new(GrayImage::from_pixel(20, 12, image::Luma([200])));
        let (p, _) = e
            .subject_mask_request(&id, mask.clone())
            .unwrap()
            .prepare(&AtomicBool::new(false))
            .unwrap();
        e.select_rect(1, 1, 5, 5);
        assert!(e.begin_floating_selection().unwrap().is_some());
        let depth = e.undo_depth();
        let selection = e.selection.clone();
        let floating = e.floating_selection_layer().map(str::to_owned);
        assert!(e.subject_mask_request(&id, mask).is_err());
        assert!(e.keep_prepared_subject_mask(p).is_err());
        assert_eq!(e.undo_depth(), depth);
        assert_eq!(e.selection, selection);
        assert_eq!(e.floating_selection_layer().map(str::to_owned), floating);
    }
    #[test]
    fn prepared_mask_is_one_undo_and_stale_sources_are_refused() {
        let mut doc = Document::new(20, 12);
        doc.layers[0].image =
            Some(RgbaImage::from_pixel(20, 12, Rgba([100, 130, 180, 255])).into());
        let mut e = Editor::new(doc);
        let id = e.active_layer.clone();
        let mask = Arc::new(GrayImage::from_pixel(20, 12, image::Luma([100])));
        let (p, _) = e
            .subject_mask_request(&id, mask.clone())
            .unwrap()
            .prepare(&AtomicBool::new(false))
            .unwrap();
        assert!(e.keep_prepared_subject_mask(p).unwrap());
        assert_eq!(e.undo_depth(), 1);
        assert!(e.undo());
        assert!(e.document.layers[0].mask.is_none());
        let (p, _) = e
            .subject_mask_request(&id, mask)
            .unwrap()
            .prepare(&AtomicBool::new(false))
            .unwrap();
        let mut other = Editor::new(e.document.clone());
        assert!(other.keep_prepared_subject_mask(p).is_err());
        assert!(other.document.layers[0].mask.is_none());
    }
    #[test]
    fn prepared_selection_is_one_selection_undo_with_exact_source_and_mask() {
        let mut doc = Document::new(20, 12);
        doc.layers[0].image =
            Some(RgbaImage::from_pixel(20, 12, Rgba([100, 130, 180, 255])).into());
        let mut e = Editor::new(doc);
        e.select_rect(1, 1, 2, 2);
        let prior = e.selection.clone();
        let image = e.document.layers[0].image.clone().unwrap();
        let id = e.active_layer.clone();
        let p = e
            .subject_mask_request(
                &id,
                Arc::new(GrayImage::from_pixel(20, 12, image::Luma([100]))),
            )
            .unwrap()
            .prepare_selection(&AtomicBool::new(false))
            .unwrap();
        assert!(e.use_prepared_subject_selection(p).unwrap());
        assert_eq!(e.undo_depth(), 1);
        assert!(
            e.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .shares_pixels_with(&image)
        );
        assert!(e.document.layers[0].mask.is_none());
        assert!(e.undo());
        assert_eq!(e.selection, prior);
    }
}
