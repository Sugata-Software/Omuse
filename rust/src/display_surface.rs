//! Display-only tiling. Engine pixels and history remain ordinary RGBA images.
//! Each BGRA texture includes a one-pixel neighbor halo to support filtering
//! across tile seams. A terminal strip smaller than 64 pixels is joined to its
//! preceding 256-pixel core, so cores can extend to 319 pixels; earlier grid
//! boundaries stay on multiples of 256. Axes of at most 256 pixels stay whole.
//! For nonempty canvases, buffer bytes are exactly
//! `4 * (width + 2 * columns) * (height + 2 * rows)`, using the actual coalesced
//! axis tile counts, not simply ceil(dimension / 256). Empty canvases have none.
//! GPU residency and retirement belong to the UI owner.
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use omuse::model::PixelRect;
use std::sync::Arc;

pub(crate) const TILE_EDGE: u32 = 256;

#[derive(Clone)]
pub(crate) struct DisplayTile {
    /// Canvas-space core, excluding the texture's one-pixel halo on every side.
    pub rect: PixelRect,
    pub image: Arc<RenderImage>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplayUpdate {
    pub replaced_tiles: usize,
    /// Bytes in newly created BGRA surfaces, including halos; not GPU telemetry.
    pub uploaded_bytes: usize,
}

#[derive(Clone)]
pub(crate) struct DisplaySurface {
    width: u32,
    height: u32,
    tiles: Arc<Vec<DisplayTile>>,
}

impl DisplaySurface {
    pub(crate) fn new(source: &RgbaImage) -> Self {
        let (width, height) = source.dimensions();
        let mut tiles = Vec::new();
        if width != 0 && height != 0 {
            let columns = axis_tiles(width);
            let rows = axis_tiles(height);
            for (y, tile_height) in rows {
                for &(x, tile_width) in &columns {
                    let rect = PixelRect {
                        x,
                        y,
                        width: tile_width,
                        height: tile_height,
                    };
                    tiles.push(DisplayTile {
                        rect,
                        image: make_image(source, rect),
                    });
                }
            }
        }
        Self {
            width,
            height,
            tiles: Arc::new(tiles),
        }
    }

    pub(crate) fn snapshot(&self) -> Arc<Vec<DisplayTile>> {
        self.tiles.clone()
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Compare before converting so a release-frame refresh reuses every
    /// unchanged RenderImage and its existing GPU identity.
    pub(crate) fn replace(&mut self, source: &RgbaImage) -> DisplayUpdate {
        if self.dimensions() != source.dimensions() {
            return self.reset(source);
        }
        self.update(source, None)
    }

    /// `damage` must cover every source pixel changed since the last update.
    /// Tiles intersecting that damage through their halo are included too.
    /// Different dimensions always reset the complete grid, even for no damage.
    pub(crate) fn update_region(&mut self, source: &RgbaImage, damage: PixelRect) -> DisplayUpdate {
        if self.dimensions() != source.dimensions() {
            return self.reset(source);
        }
        let Some(damage) = damage.clipped(self.width, self.height) else {
            return DisplayUpdate::default();
        };
        self.update(source, Some(damage))
    }

    fn reset(&mut self, source: &RgbaImage) -> DisplayUpdate {
        *self = Self::new(source);
        DisplayUpdate {
            replaced_tiles: self.tiles.len(),
            uploaded_bytes: self.tiles.iter().map(|tile| byte_count(tile.rect)).sum(),
        }
    }

    fn update(&mut self, source: &RgbaImage, damage: Option<PixelRect>) -> DisplayUpdate {
        let mut updated = DisplayUpdate::default();
        for index in 0..self.tiles.len() {
            let tile = &self.tiles[index];
            if damage.is_some_and(|damage| !halo_intersects(tile.rect, damage))
                || matches_source(tile, source)
            {
                continue;
            }
            let rect = tile.rect;
            let image = make_image(source, rect);
            Arc::make_mut(&mut self.tiles)[index].image = image;
            updated.replaced_tiles += 1;
            updated.uploaded_bytes += byte_count(rect);
        }
        updated
    }
}

/// Preserve the regular grid except for a small terminal remainder.
fn axis_tiles(length: u32) -> Vec<(u32, u32)> {
    let mut tiles = Vec::new();
    let mut origin = 0;
    while origin < length {
        let remaining = length - origin;
        let extent = if remaining < TILE_EDGE + TILE_EDGE / 4 {
            remaining
        } else {
            TILE_EDGE
        };
        tiles.push((origin, extent));
        origin += extent;
    }
    tiles
}

fn byte_count(rect: PixelRect) -> usize {
    (rect.width as usize + 2) * (rect.height as usize + 2) * 4
}

fn halo_intersects(tile: PixelRect, damage: PixelRect) -> bool {
    // Use half-open ranges and wide sums; externally supplied damage may end
    // beyond u32::MAX. The caller clips it before this comparison.
    let left = u64::from(tile.x.saturating_sub(1));
    let top = u64::from(tile.y.saturating_sub(1));
    let right = u64::from(tile.x) + u64::from(tile.width) + 1;
    let bottom = u64::from(tile.y) + u64::from(tile.height) + 1;
    left < u64::from(damage.x) + u64::from(damage.width)
        && u64::from(damage.x) < right
        && top < u64::from(damage.y) + u64::from(damage.height)
        && u64::from(damage.y) < bottom
}

fn source_coordinate(origin: u32, halo_coordinate: u32, limit: u32) -> u32 {
    // Called only for nonempty sources. Signed/wide arithmetic also supports
    // the fixed outer halo at canvas origin without wrapping.
    (i64::from(origin) + i64::from(halo_coordinate) - 1).clamp(0, i64::from(limit) - 1) as u32
}

/// Compare equal-length complete pixels without allocating converted storage.
/// Red/blue swapping and an OR reduction have no branch per pixel, allowing
/// the compiler to vectorize long unchanged rows using safe, unaligned loads.
fn bgra_matches_rgba(bgra: &[u8], rgba: &[u8]) -> bool {
    debug_assert_eq!(bgra.len(), rgba.len());
    debug_assert_eq!(rgba.len() % 4, 0);
    bgra.chunks_exact(4)
        .zip(rgba.chunks_exact(4))
        .fold(0u32, |difference, (stored, source)| {
            let stored = u32::from_le_bytes(stored.try_into().unwrap());
            let rgba = u32::from_le_bytes(source.try_into().unwrap());
            let swapped =
                (rgba & 0xff00_ff00) | ((rgba & 0x0000_00ff) << 16) | ((rgba & 0x00ff_0000) >> 16);
            difference | (stored ^ swapped)
        })
        == 0
}

fn matches_source(tile: &DisplayTile, source: &RgbaImage) -> bool {
    let Some(stored) = tile.image.as_bytes(0) else {
        return false;
    };
    if stored.len() != byte_count(tile.rect) {
        return false;
    }
    let rect = tile.rect;
    let stride = (rect.width as usize + 2) * 4;
    let source_stride = source.width() as usize * 4;
    let core_start = rect.x as usize * 4;
    let core_bytes = rect.width as usize * 4;
    let left = rect.x.saturating_sub(1) as usize * 4;
    let right = (rect.x + rect.width).min(source.width() - 1) as usize * 4;
    let pixels = source.as_raw();
    for (y, row) in stored.chunks_exact(stride).enumerate() {
        // Clamp once per row. The core is contiguous and already in bounds;
        // only the two side-halo pixels require special edge handling.
        let sy = source_coordinate(rect.y, y as u32, source.height()) as usize;
        let source_row = &pixels[sy * source_stride..(sy + 1) * source_stride];
        if !bgra_matches_rgba(&row[..4], &source_row[left..left + 4])
            || !bgra_matches_rgba(
                &row[4..4 + core_bytes],
                &source_row[core_start..core_start + core_bytes],
            )
            || !bgra_matches_rgba(&row[4 + core_bytes..], &source_row[right..right + 4])
        {
            return false;
        }
    }
    true
}

fn make_image(source: &RgbaImage, rect: PixelRect) -> Arc<RenderImage> {
    // Fill reserved storage by copying rows, avoiding a zero-fill pass over
    // pixels that will all be overwritten. No uninitialized bytes are exposed.
    let mut buffer = Vec::with_capacity(byte_count(rect));
    let source_stride = source.width() as usize * 4;
    let core_start = rect.x as usize * 4;
    let core_bytes = rect.width as usize * 4;
    let left = rect.x.saturating_sub(1) as usize * 4;
    let right = (rect.x + rect.width).min(source.width() - 1) as usize * 4;
    let pixels = source.as_raw();
    for y in 0..rect.height + 2 {
        let sy = source_coordinate(rect.y, y, source.height()) as usize;
        let source_row = &pixels[sy * source_stride..(sy + 1) * source_stride];
        buffer.extend_from_slice(&source_row[left..left + 4]);
        buffer.extend_from_slice(&source_row[core_start..core_start + core_bytes]);
        buffer.extend_from_slice(&source_row[right..right + 4]);
    }
    let mut bgra = RgbaImage::from_raw(rect.width + 2, rect.height + 2, buffer)
        .expect("complete display tile rows");
    // Match the prior full-image conversion's contiguous channel-swap pass.
    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn fixture(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                (x % 251) as u8,
                (y % 239) as u8,
                ((x + y) % 233) as u8,
                ((x * 3 + y) % 256) as u8,
            ])
        })
    }

    fn reconstruct(surface: &DisplaySurface) -> RgbaImage {
        let mut image = RgbaImage::new(surface.width, surface.height);
        for tile in surface.tiles.iter() {
            let stored = tile.image.as_bytes(0).unwrap();
            let stride = (tile.rect.width as usize + 2) * 4;
            for y in 0..tile.rect.height {
                for x in 0..tile.rect.width {
                    let offset = (y as usize + 1) * stride + (x as usize + 1) * 4;
                    let p = &stored[offset..offset + 4];
                    image.put_pixel(
                        tile.rect.x + x,
                        tile.rect.y + y,
                        Rgba([p[2], p[1], p[0], p[3]]),
                    );
                }
            }
        }
        image
    }

    fn assert_halos(surface: &DisplaySurface, source: &RgbaImage) {
        for tile in surface.tiles.iter() {
            let bytes = tile.image.as_bytes(0).unwrap();
            assert_eq!(bytes.len(), byte_count(tile.rect));
            for y in 0..tile.rect.height + 2 {
                for x in 0..tile.rect.width + 2 {
                    // Independently map texture coordinates back into the canvas.
                    let sx = (tile.rect.x as i64 + x as i64 - 1)
                        .max(0)
                        .min(source.width() as i64 - 1) as u32;
                    let sy = (tile.rect.y as i64 + y as i64 - 1)
                        .max(0)
                        .min(source.height() as i64 - 1) as u32;
                    let p = source.get_pixel(sx, sy).0;
                    let i = (y as usize * (tile.rect.width as usize + 2) + x as usize) * 4;
                    assert_eq!(&bytes[i..i + 4], &[p[2], p[1], p[0], p[3]]);
                }
            }
        }
    }

    #[test]
    fn exact_bgra_and_neighbor_halos_clamp_at_outer_canvas_edges() {
        for (w, h) in [(1, 1), (257, 259), (512, 256)] {
            let source = fixture(w, h);
            let surface = DisplaySurface::new(&source);
            assert_eq!(surface.dimensions(), (w, h));
            assert_eq!(reconstruct(&surface), source);
            assert_halos(&surface, &source);
        }
    }

    #[test]
    fn seam_and_corner_edits_replace_all_neighbor_halos_only() {
        for (x, y, expected) in [
            (12, 15, vec![0]),
            (255, 15, vec![0, 1]),
            (256, 15, vec![0, 1]),
            (255, 255, vec![0, 1, 3, 4]),
            (256, 256, vec![0, 1, 3, 4]),
            (0, 0, vec![0]),
            (599, 529, vec![5]),
        ] {
            let mut source = fixture(600, 530);
            let mut surface = DisplaySurface::new(&source);
            let before = surface.snapshot();
            source.put_pixel(x, y, Rgba([255, 254, 253, 252]));
            let update = surface.update_region(
                &source,
                PixelRect {
                    x,
                    y,
                    width: 1,
                    height: 1,
                },
            );
            assert_eq!(update.replaced_tiles, expected.len());
            assert_eq!(
                update.uploaded_bytes,
                expected
                    .iter()
                    .map(|&i| byte_count(before[i].rect))
                    .sum::<usize>()
            );
            for (index, tile) in surface.tiles.iter().enumerate() {
                assert_eq!(
                    Arc::ptr_eq(&tile.image, &before[index].image),
                    !expected.contains(&index)
                );
                assert_eq!(
                    tile.image.id == before[index].image.id,
                    !expected.contains(&index)
                );
            }
            assert_eq!(reconstruct(&surface), source);
            assert_halos(&surface, &source);
        }
    }

    #[test]
    fn terminal_coalescing_preserves_grid_and_threshold_boundaries() {
        for (length, expected) in [
            (0, vec![]),
            (1, vec![(0, 1)]),
            (256, vec![(0, 256)]),
            (257, vec![(0, 257)]),
            (319, vec![(0, 319)]),
            (320, vec![(0, 256), (256, 64)]),
            (512, vec![(0, 256), (256, 256)]),
            (513, vec![(0, 256), (256, 257)]),
            (769, vec![(0, 256), (256, 256), (512, 257)]),
        ] {
            assert_eq!(axis_tiles(length), expected);
        }
        for length in 1..=30_000 {
            let tiles = axis_tiles(length);
            assert_eq!(tiles.iter().map(|(_, size)| *size).sum::<u32>(), length);
            assert!(tiles.iter().all(|(origin, size)| origin % TILE_EDGE == 0
                && *size > 0
                && *size < TILE_EDGE + TILE_EDGE / 4));
            assert!(
                tiles[..tiles.len() - 1]
                    .iter()
                    .all(|(_, size)| *size == TILE_EDGE)
            );
        }
    }

    #[test]
    fn coalesced_native_fixtures_keep_exact_halos_and_joined_boundary_damage() {
        for (width, height, expected_tiles) in [(257, 257, 1), (1025, 769, 12)] {
            let mut source = fixture(width, height);
            let mut surface = DisplaySurface::new(&source);
            assert_eq!(surface.tiles.len(), expected_tiles);
            assert_eq!(reconstruct(&surface), source);
            assert_halos(&surface, &source);
            let columns = axis_tiles(width).len();
            let rows = axis_tiles(height).len();
            assert_eq!(
                surface
                    .tiles
                    .iter()
                    .map(|t| byte_count(t.rect))
                    .sum::<usize>(),
                4 * (width as usize + 2 * columns) * (height as usize + 2 * rows)
            );
            // Both sides of the former 256/1024 and 256/768 boundaries now
            // belong to a single final core, including the last canvas pixel.
            for (step, (x, y)) in [
                (width - 2, height - 2),
                (width - 1, height - 2),
                (width - 2, height - 1),
                (width - 1, height - 1),
            ]
            .into_iter()
            .enumerate()
            {
                let before = surface.snapshot();
                source.put_pixel(x, y, Rgba([251, 252, step as u8, 255]));
                let update = surface.update_region(
                    &source,
                    PixelRect {
                        x,
                        y,
                        width: 1,
                        height: 1,
                    },
                );
                assert_eq!(update.replaced_tiles, 1);
                for (index, tile) in surface.tiles.iter().enumerate() {
                    assert_eq!(
                        Arc::ptr_eq(&tile.image, &before[index].image),
                        index + 1 != expected_tiles
                    );
                }
                assert_eq!(reconstruct(&surface), source);
                assert_halos(&surface, &source);
            }
        }
    }

    #[test]
    fn random_damage_sequences_reconstruct_exactly_and_keep_old_snapshots() {
        let mut source = fixture(600, 530);
        let mut surface = DisplaySurface::new(&source);
        let original = surface.snapshot();
        let original_first_bytes = original[0].image.as_bytes(0).unwrap().to_vec();
        let mut state = 0x52a9_184du32;
        let mut next = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            state
        };
        for step in 0..40 {
            let x = next() % 600;
            let y = next() % 530;
            let w = (next() % 37 + 1).min(600 - x);
            let h = (next() % 29 + 1).min(530 - y);
            for py in y..y + h {
                for px in x..x + w {
                    source.put_pixel(px, py, Rgba([step, 100, 170, 230]));
                }
            }
            let update = surface.update_region(
                &source,
                PixelRect {
                    x,
                    y,
                    width: w,
                    height: h,
                },
            );
            assert!(update.replaced_tiles > 0);
            assert_eq!(reconstruct(&surface), source);
            assert_halos(&surface, &source);
        }
        assert_eq!(original[0].image.as_bytes(0).unwrap(), original_first_bytes);
    }

    #[test]
    fn full_replace_reuses_identical_images_and_only_replaces_actual_changes() {
        let mut source = fixture(513, 300);
        let mut surface = DisplaySurface::new(&source);
        let before = surface.snapshot();
        assert_eq!(surface.replace(&source), DisplayUpdate::default());
        assert!(Arc::ptr_eq(&before, &surface.snapshot()));
        source.put_pixel(290, 100, Rgba([254, 253, 252, 251]));
        assert_eq!(surface.replace(&source).replaced_tiles, 1);
        for (i, tile) in surface.tiles.iter().enumerate() {
            assert_eq!(Arc::ptr_eq(&tile.image, &before[i].image), i != 1);
        }
        let after = surface.snapshot();
        assert_eq!(surface.replace(&source), DisplayUpdate::default());
        assert!(Arc::ptr_eq(&after, &surface.snapshot()));
        assert_eq!(reconstruct(&surface), source);
    }

    #[test]
    fn empty_overflow_and_unchanged_damage_do_not_invalidate_images() {
        let source = fixture(300, 270);
        let mut surface = DisplaySurface::new(&source);
        let before = surface.snapshot();
        for rect in [
            PixelRect {
                x: 0,
                y: 0,
                width: 0,
                height: 2,
            },
            PixelRect {
                x: u32::MAX,
                y: u32::MAX,
                width: u32::MAX,
                height: u32::MAX,
            },
            PixelRect {
                x: 255,
                y: 255,
                width: 3,
                height: 3,
            },
            PixelRect {
                x: 0,
                y: 0,
                width: u32::MAX,
                height: u32::MAX,
            },
        ] {
            assert_eq!(
                surface.update_region(&source, rect),
                DisplayUpdate::default()
            );
            assert!(Arc::ptr_eq(&before, &surface.snapshot()));
        }
    }

    #[test]
    fn dimension_changes_reset_grid_even_with_empty_damage() {
        let mut surface = DisplaySurface::new(&fixture(300, 260));
        let before = surface.snapshot();
        let source = fixture(3, 4);
        let empty = PixelRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(
            surface.update_region(&source, empty),
            DisplayUpdate {
                replaced_tiles: 1,
                uploaded_bytes: 5 * 6 * 4
            }
        );
        assert_eq!(surface.dimensions(), (3, 4));
        assert!(!Arc::ptr_eq(&surface.tiles[0].image, &before[0].image));
        assert_halos(&surface, &source);
        for (w, h) in [(0, 0), (0, 5), (7, 0)] {
            assert_eq!(
                surface.replace(&RgbaImage::new(w, h)),
                DisplayUpdate::default()
            );
            assert_eq!(surface.dimensions(), (w, h));
            assert!(surface.snapshot().is_empty());
        }
        assert_eq!(surface.replace(&source).replaced_tiles, 1);
        assert_eq!(reconstruct(&surface), source);
    }

    #[test]
    #[ignore = "CPU benchmark; run explicitly with --ignored --nocapture in release mode"]
    fn benchmark_regional_conversion_against_full_clone_swap() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };
        fn median(samples: &mut [Duration]) -> Duration {
            samples.sort_unstable();
            let middle = samples.len() / 2;
            (samples[middle - 1] + samples[middle]) / 2
        }
        let mut source = fixture(2048, 2048);
        let mut surface = DisplaySurface::new(&source);
        let mut regional_samples = Vec::new();
        let mut legacy_samples = Vec::new();
        let mut creation_samples = Vec::new();
        let mut unchanged_samples = Vec::new();
        let mut replaced = 0;
        let mut regional_bytes = 0;
        let mut legacy_bytes = 0;
        let mut creation_tiles = 0;
        let mut creation_bytes = 0;
        const WARMUP: u32 = 8;
        const STEPS: u32 = 80;
        for step in 0..WARMUP + STEPS {
            let rect = PixelRect {
                x: 20 + (step * 73) % 1950,
                y: 20 + (step * 47) % 1950,
                width: 24,
                height: 24,
            };
            // Mutations are deliberately outside every timed section.
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    source.put_pixel(x, y, Rgba([step as u8, 71, 211, 255]));
                }
            }
            let start = Instant::now();
            let update = black_box(surface.update_region(black_box(&source), rect));
            let regional_time = start.elapsed();

            // Compare a same-pixels mouse-release refresh separately: it scans
            // the complete source but should create no new RenderImage at all.
            let snapshot = surface.snapshot();
            let start = Instant::now();
            let unchanged = black_box(surface.replace(black_box(&source)));
            let unchanged_time = start.elapsed();
            assert_eq!(unchanged, DisplayUpdate::default());
            assert!(Arc::ptr_eq(&snapshot, &surface.snapshot()));

            // Exact previous render_image path: full RGBA clone, red/blue
            // swap, new Frame, new RenderImage and Arc. No GPU work is timed.
            let start = Instant::now();
            let mut bgra = black_box(&source).clone();
            for pixel in bgra.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            let image = Arc::new(RenderImage::new(vec![Frame::new(bgra)]));
            black_box(&image);
            let legacy_time = start.elapsed();

            // General redraw baseline: building every tile, including byte
            // conversion and halos, from the same 2048-square source.
            let start = Instant::now();
            let rebuilt = DisplaySurface::new(black_box(&source));
            black_box(&rebuilt);
            let creation_time = start.elapsed();
            if step >= WARMUP {
                regional_samples.push(regional_time);
                unchanged_samples.push(unchanged_time);
                legacy_samples.push(legacy_time);
                creation_samples.push(creation_time);
                replaced += update.replaced_tiles;
                regional_bytes += update.uploaded_bytes;
                legacy_bytes += image.as_bytes(0).unwrap().len();
                creation_tiles += rebuilt.tiles.len();
                creation_bytes += rebuilt
                    .tiles
                    .iter()
                    .map(|tile| byte_count(tile.rect))
                    .sum::<usize>();
            }
        }
        assert_eq!(reconstruct(&surface), source);
        let regional = median(&mut regional_samples);
        let legacy = median(&mut legacy_samples);
        let creation = median(&mut creation_samples);
        let unchanged = median(&mut unchanged_samples);
        println!(
            "display conversion 2048x2048, {STEPS} measured edits of 24x24 after {WARMUP} warmups: median regional={regional:?}, legacy_full_clone_swap={legacy:?}, full_tile_creation={creation:?}, unchanged_replace={unchanged:?}; regional_replaced_tiles={replaced}, regional_created_bytes={regional_bytes}, legacy_created_bytes={legacy_bytes}, full_created_tiles={creation_tiles}, full_created_bytes={creation_bytes}, unchanged_created_bytes=0; CPU conversion/allocation only; no GPU work"
        );
    }
}
