//! Exact, copy-on-write-aware comparison for guarded Create history changes.
//!
//! Collection history distinguishes structural changes from operations that
//! replace artwork.  The latter must refuse to discard canvas edits made after
//! the operation.  These comparisons deliberately include retained editing
//! state as well as rendered pixels, so an editable RAW or advanced recipe is
//! never mistaken for an unchanged canvas.

use crate::{
    advanced::LayerState,
    create_project::Project,
    model::{Document, Layer},
    precision::TiledImage16,
    shared_image::SharedImage,
};
use anyhow::{Context, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

/// Returns whether two documents carry the same user-visible and retained
/// editing state.  Snapshot sharing takes the cheap identity path; detached
/// image data is compared exactly.
pub fn documents_match(left: &Document, right: &Document) -> bool {
    left.width == right.width
        && left.height == right.height
        && left.name == right.name
        && left.background == right.background
        && left.metadata == right.metadata
        && layers_match(&left.layers, &right.layers)
}

/// Compares page documents without permanently materializing lazy pages in
/// either project. Page ids and their presentation order are part of the
/// comparison; title, brand, component and resource envelopes are intentionally
/// excluded because this guard answers only whether artwork changed.
pub fn project_documents_match(left: &mut Project, right: &mut Project) -> Result<bool> {
    let page_ids = left.page_ids();
    if page_ids != right.page_ids() {
        return Ok(false);
    }

    // `for_each_page_document` releases each lazy page after its callback.
    // Stream two temporary project clones over bounded channels so comparison
    // remains exact and never fills either retained project's lazy-page cache.
    let mut left_snapshot = left.clone();
    let mut right_snapshot = right.clone();
    std::thread::scope(|scope| {
        let (left_sender, left_receiver) = mpsc::sync_channel(1);
        let (right_sender, right_receiver) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let left_stopped = stopped.clone();
        let right_stopped = stopped.clone();
        let left_worker =
            scope.spawn(move || stream_pages(&mut left_snapshot, left_sender, left_stopped));
        let right_worker =
            scope.spawn(move || stream_pages(&mut right_snapshot, right_sender, right_stopped));

        let comparison = (|| -> Result<bool> {
            for expected_id in &page_ids {
                let (left_id, left_document) = receive_page(&left_receiver, "left")?;
                let (right_id, right_document) = receive_page(&right_receiver, "right")?;
                if left_id != *expected_id
                    || right_id != *expected_id
                    || !documents_match(&left_document, &right_document)
                {
                    return Ok(false);
                }
            }
            Ok(true)
        })();

        // A comparison mismatch may stop receiving before a worker has sent
        // its next page. Dropping receivers releases the bounded senders first.
        stopped.store(true, Ordering::Relaxed);
        drop(left_receiver);
        drop(right_receiver);
        left_worker
            .join()
            .map_err(|_| anyhow::anyhow!("left project comparison worker panicked"))??;
        right_worker
            .join()
            .map_err(|_| anyhow::anyhow!("right project comparison worker panicked"))??;
        comparison
    })
}

fn stream_pages(
    project: &mut Project,
    sender: mpsc::SyncSender<(String, Document)>,
    stopped: Arc<AtomicBool>,
) -> Result<()> {
    project.for_each_page_document(|page, document| {
        if sender.send((page.id.clone(), document.clone())).is_err() {
            if stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            anyhow::bail!("project comparison receiver stopped");
        }
        Ok(())
    })
}

fn receive_page(
    receiver: &mpsc::Receiver<(String, Document)>,
    side: &str,
) -> Result<(String, Document)> {
    receiver
        .recv()
        .with_context(|| format!("{side} project comparison ended before every page was read"))
}

fn layers_match(left: &[Layer], right: &[Layer]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| layer_matches(left, right))
}

fn layer_matches(left: &Layer, right: &Layer) -> bool {
    left.id == right.id
        && left.name == right.name
        && left.visible == right.visible
        && left.locked == right.locked
        && float_matches(left.opacity, right.opacity)
        && left.blend_mode == right.blend_mode
        && float_matches(left.offset_x, right.offset_x)
        && float_matches(left.offset_y, right.offset_y)
        && float_matches(left.rotation, right.rotation)
        && float_matches(left.scale_x, right.scale_x)
        && float_matches(left.scale_y, right.scale_y)
        && shared_images_match(&left.image, &right.image)
        && shared_images_match(&left.mask, &right.mask)
        && advanced_match(&left.advanced, &right.advanced)
        && left.metadata == right.metadata
        && layers_match(&left.children, &right.children)
}

fn float_matches(left: f32, right: f32) -> bool {
    left.to_bits() == right.to_bits()
}

fn shared_images_match(left: &Option<SharedImage>, right: &Option<SharedImage>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.shares_pixels_with(right)
                || (left.dimensions() == right.dimensions() && left.as_raw() == right.as_raw())
        }
        _ => false,
    }
}

fn advanced_match(left: &Option<Arc<LayerState>>, right: &Option<Arc<LayerState>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            Arc::ptr_eq(left, right)
                || (serializable_match(&left.recipe, &right.recipe)
                    && optional_bytes_match(&left.raw_bytes, &right.raw_bytes)
                    && tiled_images_match(&left.source, &right.source)
                    && tiled_images_match(&left.result, &right.result))
        }
        _ => false,
    }
}

fn serializable_match<T: serde::Serialize>(left: &T, right: &T) -> bool {
    serde_json::to_value(left)
        .ok()
        .zip(serde_json::to_value(right).ok())
        .is_some_and(|(left, right)| left == right)
}

fn optional_bytes_match(left: &Option<Arc<Vec<u8>>>, right: &Option<Arc<Vec<u8>>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Arc::ptr_eq(left, right) || **left == **right,
        _ => false,
    }
}

fn tiled_images_match(left: &Arc<TiledImage16>, right: &Arc<TiledImage16>) -> bool {
    if Arc::ptr_eq(left, right) {
        return true;
    }
    if left.dimensions() != right.dimensions() || left.space() != right.space() {
        return false;
    }
    let (width, height) = left.dimensions();
    if left.tile_size() != right.tile_size() {
        return tiled_region_matches(left, right, 0, 0, width, height);
    }
    let tile_size = left.tile_size();
    for top in (0..height).step_by(tile_size as usize) {
        for left_edge in (0..width).step_by(tile_size as usize) {
            if left.shares_tile_with(right, left_edge, top) {
                continue;
            }
            let tile_width = (width - left_edge).min(tile_size);
            let tile_height = (height - top).min(tile_size);
            if !tiled_region_matches(left, right, left_edge, top, tile_width, tile_height) {
                return false;
            }
        }
    }
    true
}

fn tiled_region_matches(
    left: &TiledImage16,
    right: &TiledImage16,
    left_edge: u32,
    top: u32,
    width: u32,
    height: u32,
) -> bool {
    for y in top..top + height {
        for x in left_edge..left_edge + width {
            if left.get_pixel(x, y) != right.get_pixel(x, y) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{advanced::LayerState, precision::Rgba16};
    use image::{Rgba, RgbaImage};

    #[test]
    fn layer_edits_are_not_mistaken_for_unchanged_artwork() {
        let document = Document::new(8, 8);
        let mut renamed = document.clone();
        renamed.layers[0].name = "Retouched layer".into();
        assert!(!documents_match(&document, &renamed));

        let mut painted = document.clone();
        painted.layers[0]
            .image
            .as_mut()
            .unwrap()
            .put_pixel(2, 3, Rgba([18, 52, 86, 255]));
        assert!(!documents_match(&document, &painted));
    }

    #[test]
    fn hidden_metadata_is_semantic_document_state() {
        let document = Document::new(8, 8);
        let mut document_metadata = document.clone();
        document_metadata.metadata["futureDocumentField"] = serde_json::json!({"kept": true});
        assert!(!documents_match(&document, &document_metadata));

        let mut layer_metadata = document.clone();
        layer_metadata.layers[0].metadata["futureLayerField"] = serde_json::json!("preserve me");
        assert!(!documents_match(&document, &layer_metadata));
    }

    #[test]
    fn advanced_recipe_raw_source_and_result_are_all_preserved() {
        let mut document = Document::new(4, 4);
        let image = RgbaImage::from_pixel(4, 4, Rgba([12, 34, 56, 255]));
        let mut state = LayerState::from_image(&image, "Advanced source").unwrap();
        state.recipe.raw_extension = Some("raw".into());
        state.recipe.raw_settings = Some(Default::default());
        state.raw_bytes = Some(Arc::new(vec![1, 2, 3]));
        document.layers[0].advanced = Some(Arc::new(state));
        let snapshot = document.clone();
        assert!(documents_match(&document, &snapshot));

        let mut recipe_changed = document.clone();
        Arc::make_mut(recipe_changed.layers[0].advanced.as_mut().unwrap())
            .recipe
            .source_name = "Changed recipe".into();
        assert!(!documents_match(&document, &recipe_changed));

        let mut raw_changed = document.clone();
        Arc::make_mut(raw_changed.layers[0].advanced.as_mut().unwrap()).raw_bytes =
            Some(Arc::new(vec![9, 8, 7]));
        assert!(!documents_match(&document, &raw_changed));

        let mut source_changed = document.clone();
        Arc::make_mut(
            &mut Arc::make_mut(source_changed.layers[0].advanced.as_mut().unwrap()).source,
        )
        .set_pixel(0, 0, Rgba16([1, 2, 3, 4]))
        .unwrap();
        assert!(!documents_match(&document, &source_changed));

        let mut result_changed = document.clone();
        Arc::make_mut(
            &mut Arc::make_mut(result_changed.layers[0].advanced.as_mut().unwrap()).result,
        )
        .set_pixel(0, 0, Rgba16([5, 6, 7, 8]))
        .unwrap();
        assert!(!documents_match(&document, &result_changed));
    }

    #[test]
    fn project_comparison_requires_page_order_and_exact_page_content() {
        let mut left = Project::new("Campaign", Document::new(8, 8));
        left.add_blank_page("Second", 8, 8).unwrap();
        let mut same = left.clone();
        assert!(project_documents_match(&mut left, &mut same).unwrap());

        let ids = same.page_ids();
        same.reorder_page(&ids[1], 0).unwrap();
        assert!(!project_documents_match(&mut left, &mut same).unwrap());
    }
}
