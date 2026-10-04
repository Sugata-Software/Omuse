//! Transactional ordinary-folder release.
//!
//! A folder's children already carry their own placement, lock and appearance
//! state. Releasing a plain folder therefore only changes hierarchy; folders
//! with compositing or placement semantics are rejected rather than flattened
//! into a visually different document.

use super::*;
use anyhow::{Result, bail};

impl Editor {
    /// Explain why releasing this folder could change appearance, discard a
    /// retained source, or violate a lock. Shared with menus and command search.
    pub fn ungroup_layer_unavailable(&self, id: &str) -> Option<&'static str> {
        if self.floating.is_some() {
            return Some("Commit or cancel the floating selection first");
        }
        let Some(folder) = self.document.find_layer(id) else {
            return Some("Select a folder first");
        };
        if !folder.is_group() {
            return Some("Selected layer is not a folder");
        }
        if folder.locked {
            return Some("Unlock the folder before ungrouping");
        }
        if has_locked_ancestor(&self.document.layers, id, false).unwrap_or(true) {
            return Some("Unlock the folder's parent before ungrouping");
        }
        if folder.children.iter().any(contains_locked) {
            return Some("Unlock the folder's children before ungrouping");
        }
        if folder.opacity != 1.0 || !folder.blend_mode.eq_ignore_ascii_case("normal") {
            return Some("Cannot ungroup a folder with opacity or blend effects");
        }
        if !folder.visible {
            return Some("Show the folder before ungrouping");
        }
        if folder.mask.is_some() {
            return Some("Cannot ungroup a folder with a mask");
        }
        if folder.metadata.get("effects").is_some_and(|effects| {
            !effects.is_null()
                && effects
                    .as_object()
                    .is_none_or(|effects| effects.values().any(|effect| !effect.is_null()))
        }) {
            return Some("Cannot ungroup a folder with effects");
        }
        if folder.offset_x != 0.0
            || folder.offset_y != 0.0
            || folder.rotation != 0.0
            || folder.scale_x != 1.0
            || folder.scale_y != 1.0
        {
            return Some("Cannot ungroup a folder with placement transforms");
        }
        if folder.children.is_empty() {
            return Some("Folder has no children");
        }
        if folder.image.is_some()
            || folder.advanced.is_some()
            || folder.vector_scene.is_some()
            || ["adjustment", "text", "shape", "rustVectorScene"]
                .iter()
                .any(|key| {
                    folder
                        .metadata
                        .get(*key)
                        .is_some_and(|value| !value.is_null())
                })
        {
            return Some("Cannot ungroup a folder with retained artwork or an adjustment");
        }
        if folder
            .metadata
            .get("maskSourceID")
            .is_some_and(|value| !value.is_null())
        {
            return Some("Unlink the folder's clipping mask before ungrouping");
        }
        if references_source(&self.document.layers, id) {
            return Some("Another layer uses this folder as a live-mask source; unlink it first");
        }
        let (parent, index) = folder_position(&self.document.layers, id)?;
        let siblings = parent
            .as_deref()
            .and_then(|parent| self.document.find_layer(parent))
            .map_or(self.document.layers.as_slice(), |parent| {
                parent.children.as_slice()
            });
        // Plain folders draw directly into the parent's backdrop. Ordinary
        // blends and adjustments on their children therefore remain safe.
        // Clipping stacks, however, are detected within each sibling slice:
        // splicing can join previously independent masks onto a stack on either
        // side. Compare the actual stack memberships without rendering/cloning.
        let mut before = clipping_stack_edges(siblings);
        before.extend(clipping_stack_edges(&folder.children));
        let after = clipping_stack_edges(
            siblings[..index]
                .iter()
                .chain(&folder.children)
                .chain(&siblings[index + 1..]),
        );
        (before != after)
            .then_some("Ungroup would change a clipping stack; unlink the crossing masks first")
    }

    /// Release one ordinary folder in place and return its former children in
    /// document order. The whole operation is one undo entry.
    pub fn ungroup_layer(&mut self, id: &str) -> Result<Vec<String>> {
        if let Some(reason) = self.ungroup_layer_unavailable(id) {
            bail!(reason);
        }
        self.finish_stroke();
        let (parent, index) = folder_position(&self.document.layers, id)
            .ok_or_else(|| anyhow::anyhow!("Folder disappeared while ungrouping"))?;
        let before = self.snapshot();
        let mut children = remove_layer(&mut self.document.layers, id)
            .ok_or_else(|| anyhow::anyhow!("Folder disappeared while ungrouping"))?
            .children;
        let child_ids = children
            .iter()
            .map(|child| child.id.clone())
            .collect::<Vec<_>>();
        let siblings = target_siblings_mut(&mut self.document.layers, parent.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Folder parent disappeared while ungrouping"))?;
        for (offset, child) in children.drain(..).enumerate() {
            siblings.insert(index + offset, child);
        }
        self.active_layer = child_ids[0].clone();
        self.commit(before);
        Ok(child_ids)
    }
}

fn contains_locked(layer: &Layer) -> bool {
    layer.locked || layer.children.iter().any(contains_locked)
}

fn references_source(layers: &[Layer], id: &str) -> bool {
    layers.iter().any(|layer| {
        (layer.id != id
            && layer
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
                == Some(id))
            || references_source(&layer.children, id)
    })
}

/// Match the sibling-stack admission and skip behaviour in raster::draw_layers
/// and raster16::render_layers. Hidden clipped children still belong to a stack.
fn clipping_stack_edges<'a>(
    layers: impl IntoIterator<Item = &'a Layer>,
) -> std::collections::BTreeSet<(&'a str, &'a str)> {
    let mut edges = std::collections::BTreeSet::new();
    let mut layers = layers.into_iter().peekable();
    while let Some(base) = layers.next() {
        if !base.visible
            || !base.opacity.is_finite()
            || base.opacity <= 0.0
            || base.is_group()
            || ["adjustment", "maskSourceID"].iter().any(|key| {
                base.metadata
                    .get(*key)
                    .is_some_and(|value| !value.is_null())
            })
        {
            continue;
        }
        while layers.peek().is_some_and(|layer| {
            layer
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
                == Some(base.id.as_str())
        }) {
            let child = layers.next().unwrap();
            edges.insert((base.id.as_str(), child.id.as_str()));
        }
    }
    edges
}

fn folder_position(layers: &[Layer], id: &str) -> Option<(Option<String>, usize)> {
    for (index, layer) in layers.iter().enumerate() {
        if layer.id == id {
            return Some((None, index));
        }
        if let Some((parent, child_index)) = folder_position(&layer.children, id) {
            return Some((parent.or_else(|| Some(layer.id.clone())), child_index));
        }
    }
    None
}

fn has_locked_ancestor(layers: &[Layer], id: &str, inherited_lock: bool) -> Option<bool> {
    for layer in layers {
        let locked = inherited_lock || layer.locked;
        if layer.id == id {
            return Some(inherited_lock);
        }
        if let Some(found) = has_locked_ancestor(&layer.children, id, locked) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Document;

    #[test]
    fn ungroup_preserves_child_order_and_is_one_undo_step() {
        let mut editor = Editor::new(Document::new(8, 8));
        let first = editor.add_layer("First");
        let second = editor.add_layer("Second");
        let group = editor
            .group_layers(&[first.clone(), second.clone()], "Folder")
            .unwrap();
        let before_undo = editor.document.clone();
        let released = editor.ungroup_layer(&group).unwrap();
        assert_eq!(released.len(), 2);
        assert!(editor.document.find_layer(&group).is_none());
        assert_eq!(editor.document.layers.len(), 3);
        assert!(editor.undo());
        assert_eq!(
            editor
                .document
                .layers
                .iter()
                .map(|layer| layer.id.as_str())
                .collect::<Vec<_>>(),
            before_undo
                .layers
                .iter()
                .map(|layer| layer.id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(editor.undo());
    }

    #[test]
    fn nested_ungroup_uses_immediate_parent_and_rejects_locked_ancestor() {
        let mut editor = Editor::new(Document::new(8, 8));
        let first = editor.add_layer("First");
        let second = editor.add_layer("Second");
        let inner = editor
            .group_layers(&[first.clone(), second.clone()], "Inner")
            .unwrap();
        let middle = editor
            .group_layers(std::slice::from_ref(&inner), "Middle")
            .unwrap();
        let outer = editor
            .group_layers(std::slice::from_ref(&middle), "Outer")
            .unwrap();
        let sibling = editor.add_layer("Sibling");
        let before = editor.document.clone();
        let released = editor.ungroup_layer(&inner).unwrap();
        assert_eq!(released, vec![first.clone(), second.clone()]);
        let middle_layer = editor.document.find_layer(&middle).unwrap();
        assert_eq!(
            middle_layer
                .children
                .iter()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>(),
            vec![first.as_str(), second.as_str()]
        );
        assert!(editor.document.find_layer(&sibling).is_some());
        assert!(editor.undo());
        assert_eq!(format!("{:?}", editor.document), format!("{:?}", before));
        editor.document.find_layer_mut(&outer).unwrap().locked = true;
        assert!(
            editor
                .ungroup_layer(&inner)
                .unwrap_err()
                .to_string()
                .contains("parent")
        );
    }

    fn pixel(name: &str, rgba: [u8; 4]) -> Layer {
        let mut layer = Layer::paint(name, 1, 1);
        layer.image = Some(RgbaImage::from_pixel(1, 1, Rgba(rgba)).into());
        layer
    }

    fn document(layers: Vec<Layer>) -> Document {
        let mut document = Document::new(1, 1);
        document.layers = layers;
        document
    }

    fn assert_rejected_without_change(document: Document, id: &str, reason: &str) {
        let mut editor = Editor::new(document);
        let before = format!("{:?}", editor.document);
        let revision = editor.revision();
        assert!(
            editor
                .ungroup_layer_unavailable(id)
                .unwrap()
                .contains(reason)
        );
        assert!(
            editor
                .ungroup_layer(id)
                .unwrap_err()
                .to_string()
                .contains(reason)
        );
        assert_eq!(format!("{:?}", editor.document), before);
        assert_eq!(editor.revision(), revision);
        assert!(!editor.undo());
    }

    #[test]
    fn ungroup_rejects_both_clipping_boundaries_and_extension_of_existing_stack() {
        // The unguarded splice independently demonstrates a visible regression
        // with valid documents in both renderers, rather than only testing the
        // admission helper's internal representation.
        for base_inside in [false, true] {
            for existing_stack in [false, true] {
                let base = pixel("Translucent blue", [0, 0, 255, 128]);
                let mut red = pixel("Red clipped across folder boundary", [255, 0, 0, 255]);
                red.metadata["maskSourceID"] = serde_json::json!(base.id);
                let mut earlier_clip = pixel("Existing stack member", [0, 255, 0, 255]);
                earlier_clip.metadata["maskSourceID"] = serde_json::json!(base.id);
                let mut base_stack = vec![base];
                if existing_stack {
                    base_stack.push(earlier_clip);
                }
                let mut folder = Layer::group("Boundary");
                let id = folder.id.clone();
                let mut before = if base_inside {
                    folder.children = base_stack;
                    document(vec![folder, red])
                } else {
                    folder.children = vec![red];
                    base_stack.push(folder);
                    document(base_stack)
                };
                assert!(crate::raster::validate(&before).is_empty());
                let preview = crate::raster::composite(&before);
                let precision = crate::raster::composite16(&before).unwrap();
                let refused = before.clone();
                let index = before
                    .layers
                    .iter()
                    .position(|layer| layer.id == id)
                    .unwrap();
                let children = before.layers.remove(index).children;
                before.layers.splice(index..index, children);
                assert!(crate::raster::validate(&before).is_empty());
                assert_ne!(preview, crate::raster::composite(&before));
                assert_ne!(precision, crate::raster::composite16(&before).unwrap());
                assert_rejected_without_change(refused, &id, "clipping stack");
            }
        }
    }

    #[test]
    fn ungroup_rejects_the_folders_own_live_clipping_mask() {
        let base = pixel("Translucent blue", [0, 0, 255, 128]);
        let mut folder = Layer::group("Clipped folder");
        folder.metadata["maskSourceID"] = serde_json::json!(base.id);
        folder.children.push(pixel("Red", [255, 0, 0, 255]));
        let id = folder.id.clone();
        let before = document(vec![base, folder]);
        assert!(crate::raster::validate(&before).is_empty());
        let mut unguarded = before.clone();
        let children = unguarded.layers.pop().unwrap().children;
        unguarded.layers.extend(children);
        assert_ne!(
            crate::raster::composite(&before),
            crate::raster::composite(&unguarded)
        );
        assert_ne!(
            crate::raster::composite16(&before).unwrap(),
            crate::raster::composite16(&unguarded).unwrap()
        );
        assert_rejected_without_change(before, &id, "folder's clipping mask");
    }

    #[test]
    fn ungroup_does_not_leave_dangling_incoming_folder_references() {
        for dependent_inside in [false, true] {
            let mut folder = Layer::group("Referenced folder");
            let id = folder.id.clone();
            folder.children.push(pixel("Child", [20, 40, 60, 255]));
            let mut dependent = pixel("Dependent", [255; 4]);
            dependent.metadata["maskSourceID"] = serde_json::json!(id);
            let layers = if dependent_inside {
                folder.children.push(dependent);
                vec![folder]
            } else {
                vec![folder, dependent]
            };
            // Group sources are not renderable today, but must not be silently
            // converted from a retained dependency into a dangling reference.
            assert_rejected_without_change(document(layers), &id, "live-mask source");
        }
    }

    #[test]
    fn ungroup_rejects_adjustment_folder_and_retained_source_artwork() {
        let mut folder = Layer::group("Adjustment folder");
        let id = folder.id.clone();
        folder.children.push(pixel("Red", [255, 0, 0, 255]));
        folder.metadata["adjustment"] = serde_json::json!({"kind": "Invert"});
        let before = document(vec![pixel("Blue", [0, 0, 255, 255]), folder.clone()]);
        assert!(crate::raster::validate(&before).is_empty());
        let mut unguarded = before.clone();
        let children = unguarded.layers.pop().unwrap().children;
        unguarded.layers.extend(children);
        assert_ne!(
            crate::raster::composite(&before),
            crate::raster::composite(&unguarded)
        );
        assert_rejected_without_change(before, &id, "retained artwork or an adjustment");
        folder.metadata["adjustment"] = serde_json::Value::Null;
        folder.image = Some(RgbaImage::new(1, 1).into());
        assert_rejected_without_change(document(vec![folder]), &id, "retained artwork");
    }

    #[test]
    fn ungroup_rejects_empty_folder_and_locked_grandchild_in_editor_api() {
        let mut folder = Layer::group("Folder");
        let id = folder.id.clone();
        assert_rejected_without_change(document(vec![folder.clone()]), &id, "no children");
        let mut child = Layer::group("Child folder");
        let mut locked = pixel("Locked grandchild", [255; 4]);
        locked.locked = true;
        child.children.push(locked);
        folder.children.push(child);
        assert_rejected_without_change(document(vec![folder]), &id, "children");
    }

    #[test]
    fn ungroup_preserves_internal_clipping_and_safe_external_masks_under_parent() {
        let mut source = pixel("Independent mask source", [255, 255, 255, 90]);
        source.visible = false;
        let mut masked = pixel("Independent masked child", [200, 100, 50, 255]);
        masked.metadata["maskSourceID"] = serde_json::json!(source.id);
        let mut base = pixel("Multiply base", [100, 150, 220, 170]);
        base.blend_mode = "Multiply".into();
        let mut clipped = pixel("Screen child", [180, 60, 130, 140]);
        clipped.blend_mode = "Screen".into();
        clipped.metadata["maskSourceID"] = serde_json::json!(base.id);
        let mut folder = Layer::group("Safe folder");
        let id = folder.id.clone();
        folder.children = vec![masked, base, clipped];
        let mut parent = Layer::group("Masked translucent parent");
        parent.opacity = 0.7;
        parent.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([170, 170, 170, 255])).into());
        parent.children = vec![folder];
        let before = document(vec![pixel("Backdrop", [60, 80, 100, 230]), source, parent]);
        assert!(crate::raster::validate(&before).is_empty());
        let preview = crate::raster::composite(&before);
        let precision = crate::raster::composite16(&before).unwrap();
        let mut editor = Editor::new(before);
        assert!(editor.ungroup_layer_unavailable(&id).is_none());
        editor.ungroup_layer(&id).unwrap();
        assert_eq!(preview, crate::raster::composite(&editor.document));
        assert_eq!(
            precision,
            crate::raster::composite16(&editor.document).unwrap()
        );
        assert!(editor.undo());
        assert!(!editor.undo());
    }

    #[test]
    fn ungroup_preserves_child_adjustments_and_live_effects() {
        let mut styled = pixel("Styled child", [120, 160, 200, 180]);
        styled.metadata["effects"] = serde_json::json!({"stroke": {"size": 1.0}});
        let mut adjustment = Layer::group("Invert child");
        adjustment.metadata = serde_json::json!({"adjustment": {"kind": "Invert"}});
        let mut folder = Layer::group("Plain folder");
        let id = folder.id.clone();
        folder.children = vec![styled, adjustment];
        let before = document(vec![pixel("Backdrop", [30, 60, 90, 255]), folder]);
        assert!(crate::raster::validate(&before).is_empty());
        let preview = crate::raster::composite(&before);
        let mut editor = Editor::new(before);
        editor.ungroup_layer(&id).unwrap();
        assert_eq!(preview, crate::raster::composite(&editor.document));
    }
}
