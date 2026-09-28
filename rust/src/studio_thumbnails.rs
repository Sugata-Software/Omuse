//! Small native previews for the layer studio.
//!
//! The cache owns only a fixed-size BGRA sample and the `RenderImage` made
//! from it. It deliberately does not retain a `SharedImage` (or any clone of
//! the source image), so painting a layer list cannot keep a full layer alive.

use gpui_kit::{App, RenderImage, Window};
use image::{Frame, RgbaImage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const THUMBNAIL_EDGE: u32 = 32;
const MAX_ENTRIES: usize = 256;

struct Entry {
    /// The complete 32x32 BGRA sample used to decide whether the texture can
    /// be reused. Keeping this small byte vector avoids comparing source
    /// images and makes changes below the preview's resolution free to reuse.
    bytes: Vec<u8>,
    image: Arc<RenderImage>,
}

/// GPU-backed 32x32 previews keyed by layer id.
///
/// Call [`Self::retain`] after each layer-list submission and [`Self::release`]
/// when the owning view is torn down. The caller remains responsible for
/// ensuring a returned image is no longer submitted before it is evicted.
#[derive(Default)]
pub struct LayerThumbnails {
    entries: HashMap<String, Entry>,
}

impl LayerThumbnails {
    /// Return the cached preview for `id`, replacing its GPU image only when
    /// the bounded nearest-neighbour sample actually changed.
    ///
    /// A new id is rejected once the bounded cache is full. This keeps every
    /// image already returned during the current layer-list paint alive until
    /// the caller can run [`Self::retain`] after submission.
    pub fn image(
        &mut self,
        id: &str,
        source: &RgbaImage,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<RenderImage>> {
        let bytes = thumbnail_bytes(source);
        if let Some(entry) = self.entries.get(id) {
            if entry.bytes == bytes {
                return Some(Arc::clone(&entry.image));
            }
        } else if self.entries.len() >= MAX_ENTRIES {
            return None;
        }

        let image = Arc::new(RenderImage::new(vec![Frame::new(
            RgbaImage::from_raw(THUMBNAIL_EDGE, THUMBNAIL_EDGE, bytes.clone())
                .expect("thumbnail buffer has exactly 32x32 pixels"),
        )]));
        if let Some(previous) = self.entries.insert(
            id.to_owned(),
            Entry {
                bytes,
                image: Arc::clone(&image),
            },
        ) {
            // The caller has moved this id to the replacement image for the
            // current submission, so the old GPU texture is safe to retire.
            cx.drop_image(previous.image, Some(window));
        }

        Some(image)
    }

    /// Retire previews whose layer ids are no longer visible.
    pub fn retain(&mut self, visible_ids: &HashSet<String>, window: &mut Window, cx: &mut App) {
        let disappeared: Vec<String> = self
            .entries
            .keys()
            .filter(|id| !visible_ids.contains(*id))
            .cloned()
            .collect();
        for id in disappeared {
            if let Some(entry) = self.entries.remove(&id) {
                cx.drop_image(entry.image, Some(window));
            }
        }
    }

    /// Release every cached texture, for use when the owning view is dropped.
    pub fn release(&mut self, cx: &mut App) {
        for (_, entry) in self.entries.drain() {
            cx.drop_image(entry.image, None);
        }
    }
}

/// Build a centred aspect-fit preview in BGRA order.
///
/// Sampling work is capped at 1024 destination pixels regardless of source
/// size. Empty images produce a transparent texture, which keeps callers from
/// needing a separate placeholder path.
fn thumbnail_bytes(source: &RgbaImage) -> Vec<u8> {
    let (source_width, source_height) = source.dimensions();
    let mut output = vec![0; (THUMBNAIL_EDGE * THUMBNAIL_EDGE * 4) as usize];
    if source_width == 0 || source_height == 0 {
        return output;
    }

    let (width, height) = fit_dimensions(source_width, source_height);
    let left = (THUMBNAIL_EDGE - width) / 2;
    let top = (THUMBNAIL_EDGE - height) / 2;
    let source_pixels = source.as_raw();
    for y in 0..height {
        // Mapping pixel centres gives stable nearest-neighbour samples and
        // avoids a source-size-dependent loop or intermediate image.
        let source_y = (((2 * u64::from(y) + 1) * u64::from(source_height))
            / (2 * u64::from(height)))
        .min(u64::from(source_height - 1)) as usize;
        for x in 0..width {
            let source_x = (((2 * u64::from(x) + 1) * u64::from(source_width))
                / (2 * u64::from(width)))
            .min(u64::from(source_width - 1)) as usize;
            let source_index = (source_y * source_width as usize + source_x) * 4;
            let output_index = (((top + y) * THUMBNAIL_EDGE + left + x) * 4) as usize;
            let pixel = &source_pixels[source_index..source_index + 4];
            output[output_index..output_index + 4]
                .copy_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    output
}

fn fit_dimensions(source_width: u32, source_height: u32) -> (u32, u32) {
    debug_assert!(source_width > 0 && source_height > 0);
    if source_width >= source_height {
        let height = ((u64::from(source_height) * u64::from(THUMBNAIL_EDGE)
            + u64::from(source_width / 2))
            / u64::from(source_width))
        .clamp(1, u64::from(THUMBNAIL_EDGE)) as u32;
        (THUMBNAIL_EDGE, height)
    } else {
        let width = ((u64::from(source_width) * u64::from(THUMBNAIL_EDGE)
            + u64::from(source_height / 2))
            / u64::from(source_height))
        .clamp(1, u64::from(THUMBNAIL_EDGE)) as u32;
        (width, THUMBNAIL_EDGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn aspect_fit_preserves_wide_shape_and_transparent_padding() {
        let source = RgbaImage::from_pixel(8, 2, Rgba([10, 20, 30, 255]));
        let bytes = thumbnail_bytes(&source);

        assert_eq!(fit_dimensions(8, 2), (32, 8));
        assert_eq!(&bytes[..32 * 12 * 4], vec![0u8; 32 * 12 * 4].as_slice());
        assert_eq!(
            &bytes[(12 * 32 * 4)..(20 * 32 * 4)],
            &vec![30, 20, 10, 255].repeat(32 * 8)
        );
        assert_eq!(&bytes[(20 * 32 * 4)..], vec![0u8; 32 * 12 * 4].as_slice());
    }

    #[test]
    fn aspect_fit_preserves_tall_shape() {
        let source = RgbaImage::from_pixel(2, 8, Rgba([10, 20, 30, 255]));
        assert_eq!(fit_dimensions(source.width(), source.height()), (8, 32));
        let bytes = thumbnail_bytes(&source);
        assert_eq!(&bytes[..12 * 4], vec![0u8; 12 * 4].as_slice());
        let expected = vec![30, 20, 10, 255].repeat(8);
        assert_eq!(&bytes[12 * 4..20 * 4], expected.as_slice());
    }

    #[test]
    fn nearest_sample_swaps_rgba_to_bgra_and_keeps_alpha() {
        let mut source = RgbaImage::new(2, 1);
        source.put_pixel(0, 0, Rgba([1, 2, 3, 4]));
        source.put_pixel(1, 0, Rgba([5, 6, 7, 8]));
        let bytes = thumbnail_bytes(&source);

        // A 2x1 source fits to 32x16; the first visible row starts at y=8.
        let first = (8 * 32) * 4;
        assert_eq!(&bytes[first..first + 4], &[3, 2, 1, 4]);
        assert_eq!(&bytes[first + 16 * 4..first + 16 * 4 + 4], &[7, 6, 5, 8]);
    }
}
