use image::{Rgba, RgbaImage};

const TILE_SIZE: u32 = 256;
const CHANNELS: usize = 4;

pub(crate) struct RasterPatch {
    width: u32,
    height: u32,
    tiles: Vec<Tile>,
}

struct Tile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl RasterPatch {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            tiles: Vec::new(),
        }
    }

    pub(crate) fn original_pixel(&self, image: &RgbaImage, x: u32, y: u32) -> Option<Rgba<u8>> {
        if image.dimensions() != (self.width, self.height) || x >= self.width || y >= self.height {
            return None;
        }
        let tile_x = x / TILE_SIZE;
        let tile_y = y / TILE_SIZE;
        if let Ok(index) = self.find(tile_x, tile_y) {
            let tile = &self.tiles[index];
            let local_x = (x - tile_x * TILE_SIZE) as usize;
            let local_y = (y - tile_y * TILE_SIZE) as usize;
            let offset = local_y
                .checked_mul(tile.width as usize)?
                .checked_add(local_x)?
                .checked_mul(CHANNELS)?;
            let end = offset.checked_add(CHANNELS)?;
            let pixel: [u8; CHANNELS] = tile.bytes.get(offset..end)?.try_into().ok()?;
            Some(Rgba(pixel))
        } else {
            image.get_pixel_checked(x, y).copied()
        }
    }

    /// Ensure a tile's current bytes are captured. Returns false only for invalid input.
    pub(crate) fn capture(&mut self, image: &RgbaImage, x: u32, y: u32) -> bool {
        if image.dimensions() != (self.width, self.height) || x >= self.width || y >= self.height {
            return false;
        }
        let tile_x = x / TILE_SIZE;
        let tile_y = y / TILE_SIZE;
        let index = match self.find(tile_x, tile_y) {
            Ok(_) => return true,
            Err(index) => index,
        };
        let start_x = tile_x * TILE_SIZE;
        let start_y = tile_y * TILE_SIZE;
        let width = TILE_SIZE.min(self.width - start_x);
        let height = TILE_SIZE.min(self.height - start_y);
        let byte_len = match (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(CHANNELS))
        {
            Some(len) => len,
            None => return false,
        };
        let source = image.as_raw();
        let stride = match (self.width as usize).checked_mul(CHANNELS) {
            Some(stride) => stride,
            None => return false,
        };
        let row_len = width as usize * CHANNELS;
        let mut bytes = Vec::with_capacity(byte_len);
        for row in 0..height as usize {
            let source_start = (start_y as usize + row) * stride + start_x as usize * CHANNELS;
            let Some(source_row) = source.get(source_start..source_start + row_len) else {
                return false;
            };
            bytes.extend_from_slice(source_row);
        }
        self.tiles.insert(
            index,
            Tile {
                x: tile_x,
                y: tile_y,
                width,
                height,
                bytes,
            },
        );
        true
    }

    pub(crate) fn validate(&self, image: &RgbaImage) -> bool {
        if image.dimensions() != (self.width, self.height) {
            return false;
        }
        let Some(expected_image_len) = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(CHANNELS))
        else {
            return false;
        };
        if image.as_raw().len() != expected_image_len {
            return false;
        }

        let mut previous = None;
        for tile in &self.tiles {
            let key = (tile.y, tile.x);
            if previous.is_some_and(|prior| prior >= key) {
                return false;
            }
            previous = Some(key);
            let Some(start_x) = tile.x.checked_mul(TILE_SIZE) else {
                return false;
            };
            let Some(start_y) = tile.y.checked_mul(TILE_SIZE) else {
                return false;
            };
            if start_x >= self.width || start_y >= self.height {
                return false;
            }
            let expected_width = TILE_SIZE.min(self.width - start_x);
            let expected_height = TILE_SIZE.min(self.height - start_y);
            let expected_len = (expected_width as usize)
                .checked_mul(expected_height as usize)
                .and_then(|pixels| pixels.checked_mul(CHANNELS));
            if tile.width != expected_width
                || tile.height != expected_height
                || expected_len != Some(tile.bytes.len())
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn swap(&mut self, image: &mut RgbaImage) -> bool {
        if !self.validate(image) {
            return false;
        }
        let stride = self.width as usize * CHANNELS;
        let pixels: &mut [u8] = image.as_mut();
        for tile in &mut self.tiles {
            let start_x = tile.x as usize * TILE_SIZE as usize;
            let start_y = tile.y as usize * TILE_SIZE as usize;
            let row_len = tile.width as usize * CHANNELS;
            for row in 0..tile.height as usize {
                let live_start = (start_y + row) * stride + start_x * CHANNELS;
                let stored_start = row * row_len;
                let live = &mut pixels[live_start..live_start + row_len];
                let stored = &mut tile.bytes[stored_start..stored_start + row_len];
                live.swap_with_slice(stored);
            }
        }
        true
    }

    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(
                self.tiles
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Tile>()),
            )
            .saturating_add(self.tiles.iter().fold(0usize, |bytes, tile| {
                bytes.saturating_add(tile.bytes.capacity())
            }))
    }

    pub(crate) fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    fn find(&self, x: u32, y: u32) -> Result<usize, usize> {
        self.tiles
            .binary_search_by_key(&(y, x), |tile| (tile.y, tile.x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterned(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([x as u8, y as u8, (x ^ y) as u8, x.wrapping_add(y) as u8])
        })
    }

    #[test]
    fn captures_clipped_edges_and_preserves_hidden_bytes() {
        let image = patterned(257, 259);
        let mut patch = RasterPatch::new(257, 259);
        assert!(patch.capture(&image, 256, 258));
        assert_eq!(patch.tile_count(), 1);
        assert_eq!(patch.tiles[0].bytes.len(), 3 * 4);
        assert_eq!(
            patch.original_pixel(&image, 256, 258),
            Some(*image.get_pixel(256, 258))
        );
    }

    #[test]
    fn original_reads_survive_multiple_captures_and_live_changes() {
        let mut image = patterned(520, 300);
        let mut patch = RasterPatch::new(520, 300);
        let first = *image.get_pixel(4, 5);
        let second = *image.get_pixel(519, 299);
        assert!(patch.capture(&image, 4, 5));
        *image.get_pixel_mut(4, 5) = Rgba([9, 8, 7, 6]);
        assert!(patch.capture(&image, 519, 299));
        *image.get_pixel_mut(519, 299) = Rgba([1, 2, 3, 4]);
        assert!(patch.capture(&image, 5, 6));
        assert_eq!(patch.original_pixel(&image, 4, 5), Some(first));
        assert_eq!(patch.original_pixel(&image, 519, 299), Some(second));
        assert_eq!(
            patch.original_pixel(&image, 300, 10),
            Some(*image.get_pixel(300, 10))
        );
    }

    #[test]
    fn repeated_swaps_are_byte_exact() {
        let before = patterned(300, 270);
        let mut live = before.clone();
        let mut patch = RasterPatch::new(300, 270);
        assert!(patch.capture(&live, 10, 10));
        assert!(patch.capture(&live, 299, 269));
        *live.get_pixel_mut(10, 10) = Rgba([0, 1, 2, 3]);
        *live.get_pixel_mut(299, 269) = Rgba([4, 5, 6, 7]);
        let after = live.clone();
        assert!(patch.swap(&mut live));
        assert_eq!(live, before);
        assert!(patch.swap(&mut live));
        assert_eq!(live, after);
        assert!(patch.swap(&mut live));
        assert_eq!(live, before);
    }

    #[test]
    fn malformed_patches_fail_atomically() {
        let base = patterned(300, 270);
        let cases = [0, 1, 2, 3];
        for case in cases {
            let mut patch = RasterPatch::new(300, 270);
            patch.capture(&base, 10, 10);
            match case {
                0 => patch.width = 301,
                1 => patch.tiles[0].bytes.pop().map(|_| ()).unwrap(),
                2 => patch.tiles[0].x = 99,
                3 => patch.tiles.push(Tile {
                    x: patch.tiles[0].x,
                    y: patch.tiles[0].y,
                    width: patch.tiles[0].width,
                    height: patch.tiles[0].height,
                    bytes: patch.tiles[0].bytes.clone(),
                }),
                _ => unreachable!(),
            }
            let mut live = base.clone();
            let unchanged = live.clone();
            assert!(!patch.swap(&mut live));
            assert_eq!(live, unchanged);
        }

        let mut out_of_order = RasterPatch::new(300, 270);
        out_of_order.capture(&base, 299, 10);
        out_of_order.tiles.push(Tile {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
            bytes: vec![0; 256 * 256 * 4],
        });
        let mut live = base.clone();
        let unchanged = live.clone();
        assert!(!out_of_order.swap(&mut live));
        assert_eq!(live, unchanged);

        let mut wrong_dimensions = patterned(299, 270);
        let unchanged = wrong_dimensions.clone();
        let mut patch = RasterPatch::new(300, 270);
        patch.capture(&base, 10, 10);
        assert!(!patch.swap(&mut wrong_dimensions));
        assert_eq!(wrong_dimensions, unchanged);
    }

    #[test]
    fn reads_and_no_ops_do_not_allocate() {
        let image = patterned(8, 8);
        let mut patch = RasterPatch::new(8, 8);
        let bytes = patch.owned_bytes();
        assert_eq!(
            patch.original_pixel(&image, 3, 4),
            Some(*image.get_pixel(3, 4))
        );
        assert_eq!(patch.original_pixel(&image, 8, 0), None);
        assert!(!patch.capture(&image, 8, 0));
        assert!(!patch.capture(&RgbaImage::new(7, 8), 0, 0));
        assert_eq!(patch.tile_count(), 0);
        assert_eq!(patch.owned_bytes(), bytes);
    }
}
