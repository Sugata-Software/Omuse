//! Immutable snapshots share pixels; mutable access detaches only the edited image.
//!
//! This is whole-image copy-on-write, not tiled storage. All mutation goes through
//! `Arc::make_mut`, so undo, recovery and background jobs retain stable pixels.
use image::RgbaImage;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedImage(Arc<RgbaImage>);

impl SharedImage {
    /// A concrete copy for APIs that require an owned, mutable ImageBuffer.
    pub fn to_image(&self) -> RgbaImage {
        (**self).clone()
    }

    /// Avoid copying when this is the only owner.
    pub fn into_image(self) -> RgbaImage {
        Arc::unwrap_or_clone(self.0)
    }

    /// Share immutable pixels with background preview consumers.
    pub fn as_arc(&self) -> Arc<RgbaImage> {
        self.0.clone()
    }

    /// Identity of the retained allocation, used only for live memory accounting.
    pub(crate) fn allocation_id(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    pub fn shares_pixels_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl From<RgbaImage> for SharedImage {
    fn from(image: RgbaImage) -> Self {
        Self(Arc::new(image))
    }
}
impl Deref for SharedImage {
    type Target = RgbaImage;
    fn deref(&self) -> &RgbaImage {
        &self.0
    }
}
impl DerefMut for SharedImage {
    fn deref_mut(&mut self) -> &mut RgbaImage {
        Arc::make_mut(&mut self.0)
    }
}
impl PartialEq<RgbaImage> for SharedImage {
    fn eq(&self, other: &RgbaImage) -> bool {
        **self == *other
    }
}
impl PartialEq<SharedImage> for RgbaImage {
    fn eq(&self, other: &SharedImage) -> bool {
        *self == **other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn reads_share_and_every_mutation_detaches() {
        let original: SharedImage = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255])).into();
        let mut edited = original.clone();
        assert!(original.shares_pixels_with(&edited));
        edited.put_pixel(2, 3, Rgba([1, 2, 3, 4]));
        assert!(!original.shares_pixels_with(&edited));
        assert_eq!(original.get_pixel(2, 3).0, [10, 20, 30, 255]);
        assert_eq!(edited.get_pixel(2, 3).0, [1, 2, 3, 4]);
        let frozen = edited.clone();
        let edited_bytes: &mut [u8] = edited.as_mut();
        edited_bytes[0] = 99;
        assert_eq!(frozen.as_raw()[0], 10);
        assert_eq!(edited.as_raw()[0], 99);
    }

    #[test]
    fn owned_conversion_reuses_unique_allocation_and_preserves_shared_snapshot() {
        let original: SharedImage = RgbaImage::from_pixel(2, 2, Rgba([5, 6, 7, 8])).into();
        let ptr = original.as_raw().as_ptr();
        let owned = original.into_image();
        assert_eq!(owned.as_raw().as_ptr(), ptr);
        let snapshot: SharedImage = owned.into();
        let mut owned = snapshot.clone().into_image();
        owned.put_pixel(0, 0, Rgba([0; 4]));
        assert_eq!(snapshot.get_pixel(0, 0).0, [5, 6, 7, 8]);
    }
}
