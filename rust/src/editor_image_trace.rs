use super::*;

impl Editor {
    /// Publish exactly the previewed candidate as one Undo transaction.
    pub fn apply_image_trace(
        &mut self,
        prepared: crate::image_trace_layer::PreparedTrace,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(
            self.instance_id() == prepared.instance && self.revision() == prepared.revision,
            "Document changed while tracing; reopen Image trace"
        );
        anyhow::ensure!(
            self.stroke.is_none() && self.floating.is_none(),
            "Finish the active edit first"
        );
        anyhow::ensure!(
            tree_count(&prepared.document.layers) <= crate::model::MAX_LAYERS
                && tree_pixels(&prepared.document.layers) <= crate::model::MAX_PIXELS,
            "Trace exceeds the project budget"
        );
        crate::document::validate_vector_scene_budget(&prepared.document.layers)?;
        let errors = crate::raster::validate(&prepared.document);
        anyhow::ensure!(
            errors.is_empty(),
            "Invalid trace document: {}",
            errors.join("; ")
        );
        let before = self.snapshot();
        self.document = prepared.document;
        self.active_layer = prepared.layer_id.clone();
        self.commit(before);
        Ok(prepared.layer_id)
    }
}
