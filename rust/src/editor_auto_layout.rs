//! Auto-layout edits share the document's ordinary atomic history boundary.
use super::*;
use crate::auto_layout::{self as layout, FrameRule, ItemRule, ItemSize};
use anyhow::{Context, Result, ensure};
use std::sync::atomic::{AtomicBool, Ordering};

impl Editor {
    pub(super) fn commit_checked(&mut self, before: Snapshot) -> Result<bool> {
        if self.commit(before) {
            return Ok(true);
        }
        if let Some(error) = self.layout_error.as_ref() {
            anyhow::bail!(error.clone());
        }
        Ok(false)
    }
    /// A refused implicit reflow restores the complete pre-edit document. The
    /// UI can drain this message even for older bool-returning editor methods.
    pub fn take_layout_error(&mut self) -> Option<String> {
        self.layout_error.take()
    }

    fn layout_ready(&self, cancel: &AtomicBool) -> Result<()> {
        ensure!(!cancel.load(Ordering::Relaxed), "Auto-layout cancelled");
        ensure!(
            self.stroke.is_none() && self.floating.is_none(),
            "Finish the active edit before changing auto-layout"
        );
        Ok(())
    }
    fn publish_layout(&mut self, mut candidate: Document, cancel: &AtomicBool) -> Result<()> {
        self.layout_ready(cancel)?;
        layout::release_orphans(&mut candidate.layers, false);
        layout::reflow(&mut candidate, cancel)?;
        ensure!(
            crate::raster::validate(&candidate).is_empty(),
            "Layout produced invalid document placement"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "Auto-layout cancelled");
        let before = self.snapshot();
        self.document = candidate;
        ensure!(
            self.commit(before),
            "{}",
            self.take_layout_error()
                .unwrap_or_else(|| "Cannot commit auto-layout".into())
        );
        Ok(())
    }

    /// Frame selected contiguous sibling leaves without changing their stacking
    /// order. Selection across unrelated folders is explicitly refused.
    pub fn create_layout_frame(
        &mut self,
        ids: &[String],
        rule: FrameRule,
        cancel: &AtomicBool,
    ) -> Result<String> {
        self.layout_ready(cancel)?;
        rule.validate()?;
        ensure!(
            !ids.is_empty() && ids.len() <= layout::MAX_MEMBERS,
            "Select 1–256 photo, text or vector layers"
        );
        let roots =
            selected_roots(&self.document.layers, ids).context("Layout selection disappeared")?;
        ensure!(
            roots.len() == ids.len(),
            "Select distinct sibling layers, without their folders"
        );
        ensure!(
            roots_editable(&self.document.layers, &roots),
            "Unlock selected layers and their parent before framing"
        );
        ensure!(
            tree_count(&self.document.layers) < crate::model::MAX_LAYERS,
            "Document layer limit reached"
        );
        let first_path = tree_path(&self.document.layers, &roots[0]).unwrap();
        let parent = first_path
            .len()
            .checked_sub(2)
            .map(|i| first_path[i].clone());
        ensure!(
            parent
                .as_deref()
                .and_then(|id| self.document.find_layer(id))
                .is_none_or(|l| l.metadata.get(layout::FRAME_KEY).is_none()),
            "Nested auto-layout frames are not supported"
        );
        for id in &roots {
            let path = tree_path(&self.document.layers, id).unwrap();
            ensure!(
                path.len() == first_path.len()
                    && path[..path.len() - 1] == first_path[..first_path.len() - 1],
                "Frame layers from the same folder"
            );
            let layer = self.document.find_layer(id).unwrap();
            ensure!(
                !layer.is_group() && layer.image.is_some(),
                "Select photo, text or vector layers; nested folders are unsupported"
            );
        }
        // Moving a clipping boundary into another sibling slice changes its
        // compositing meaning even if its position and pixels are unchanged.
        fn referenced(layers: &[Layer], ids: &[String]) -> bool {
            layers.iter().any(|l| {
                l.metadata
                    .get("maskSourceID")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| ids.iter().any(|s| s == id))
                    || referenced(&l.children, ids)
            })
        }
        ensure!(
            !referenced(&self.document.layers, &roots),
            "Unlink clipping masks that reference the selected layers before framing"
        );
        let mut candidate = self.document.clone();
        let siblings = target_siblings_mut(&mut candidate.layers, parent.as_deref()).unwrap();
        let first = siblings.iter().position(|l| l.id == roots[0]).unwrap();
        ensure!(
            siblings
                .get(first..first + roots.len())
                .is_some_and(|slice| slice.iter().map(|l| &l.id).eq(roots.iter())),
            "Select contiguous sibling layers to preserve stacking order"
        );
        let mut frame = Layer::group("Auto-layout frame");
        frame.offset_x = siblings[first..first + roots.len()]
            .iter()
            .map(|l| l.offset_x)
            .fold(f32::INFINITY, f32::min);
        frame.offset_y = siblings[first..first + roots.len()]
            .iter()
            .map(|l| l.offset_y)
            .fold(f32::INFINITY, f32::min);
        frame.children = siblings.drain(first..first + roots.len()).collect();
        layout::set_frame_rule(&mut frame, &rule)?;
        for child in &mut frame.children {
            let item = ItemRule::natural(child);
            layout::set_item_rule(child, &item)?;
        }
        let id = frame.id.clone();
        siblings.insert(first, frame);
        self.publish_layout(candidate, cancel)?;
        self.active_layer = id.clone();
        Ok(id)
    }

    /// Enable or update retained layout on one ordinary folder.
    pub fn set_layout_frame(
        &mut self,
        id: &str,
        rule: FrameRule,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        self.layout_ready(cancel)?;
        rule.validate()?;
        ensure!(
            !locked_in_tree(&self.document.layers, id, false),
            "Unlock the frame and its parent first"
        );
        let layer = self.document.find_layer(id).context("Frame not found")?;
        ensure!(layer.is_group(), "Choose a folder or frame");
        if layout::frame_rule(layer)?.as_ref() == Some(&rule) {
            return Ok(false);
        }
        let new = layout::frame_rule(layer)?.is_none();
        let mut candidate = self.document.clone();
        let layer = candidate.find_layer_mut(id).unwrap();
        if new {
            ensure!(
                layer.children.iter().all(|c| !c.locked),
                "Unlock the folder's children first"
            );
            if !layer.children.is_empty() {
                layer.offset_x = layer
                    .children
                    .iter()
                    .map(|l| l.offset_x)
                    .fold(f32::INFINITY, f32::min);
                layer.offset_y = layer
                    .children
                    .iter()
                    .map(|l| l.offset_y)
                    .fold(f32::INFINITY, f32::min);
            }
            for child in &mut layer.children {
                let item = ItemRule::natural(child);
                layout::set_item_rule(child, &item)?;
            }
        }
        layout::set_frame_rule(layer, &rule)?;
        self.publish_layout(candidate, cancel)?;
        Ok(true)
    }

    pub fn set_layout_item(
        &mut self,
        id: &str,
        width: ItemSize,
        height: ItemSize,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        self.layout_ready(cancel)?;
        ensure!(
            !locked_in_tree(&self.document.layers, id, false),
            "Unlock the layout member first"
        );
        let path = tree_path(&self.document.layers, id).context("Layout member not found")?;
        let parent = path
            .len()
            .checked_sub(2)
            .and_then(|i| self.document.find_layer(&path[i]));
        ensure!(
            parent.is_some_and(|p| p.metadata.get(layout::FRAME_KEY).is_some()),
            "Select a direct member of an auto-layout frame"
        );
        let layer = self.document.find_layer(id).unwrap();
        let mut rule = layout::item_rule(layer)?.unwrap_or_else(|| ItemRule::natural(layer));
        if rule.width == width && rule.height == height {
            return Ok(false);
        }
        rule.width = width;
        rule.height = height;
        rule.validate()?;
        let mut candidate = self.document.clone();
        layout::set_item_rule(candidate.find_layer_mut(id).unwrap(), &rule)?;
        self.publish_layout(candidate, cancel)?;
        Ok(true)
    }

    /// Release the rules, retaining the folder, exact visible geometry and all
    /// editable sources. This is the explicit static-copy compatibility path.
    pub fn remove_layout_frame(&mut self, id: &str, cancel: &AtomicBool) -> Result<bool> {
        self.layout_ready(cancel)?;
        ensure!(
            !locked_in_tree(&self.document.layers, id, false),
            "Unlock the frame first"
        );
        let layer = self.document.find_layer(id).context("Frame not found")?;
        if layout::frame_rule(layer)?.is_none() {
            return Ok(false);
        }
        ensure!(
            layer.children.iter().all(|c| !c.locked),
            "Unlock the frame's children before releasing rules"
        );
        let mut candidate = self.document.clone();
        let layer = candidate.find_layer_mut(id).unwrap();
        let metadata = layer
            .metadata
            .as_object_mut()
            .context("Invalid frame metadata")?;
        metadata.remove(layout::FRAME_KEY);
        metadata.remove("omuseLayoutExtent");
        layer.offset_x = 0.;
        layer.offset_y = 0.;
        for child in &mut layer.children {
            if let Some(m) = child.metadata.as_object_mut() {
                m.remove(layout::ITEM_KEY);
            }
        }
        self.publish_layout(candidate, cancel)?;
        Ok(true)
    }
}
