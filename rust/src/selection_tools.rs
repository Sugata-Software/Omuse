use crate::editor::Selection;
use anyhow::{Result, ensure};
use image::{GrayImage, RgbaImage};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Subtract,
}

pub fn combine(base: Option<&Selection>, incoming: &Selection, mode: SelectionMode) -> Selection {
    let compatible = base.filter(|b| b.width == incoming.width && b.height == incoming.height);
    let mask = incoming
        .mask
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            let old = compatible.and_then(|b| b.mask.get(i)).copied().unwrap_or(0);
            match mode {
                SelectionMode::Replace => value,
                SelectionMode::Add => {
                    let a = u16::from(old);
                    let b = u16::from(value);
                    (a + b - (a * b + 127) / 255).min(255) as u8
                }
                SelectionMode::Subtract => {
                    ((u16::from(old) * (255 - u16::from(value)) + 127) / 255) as u8
                }
            }
        })
        .collect();
    Selection {
        width: incoming.width,
        height: incoming.height,
        mask,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WandSettings {
    pub tolerance: u8,
    pub sample_radius: u8,
    pub contiguous: bool,
    pub all_layers: bool,
    pub anti_alias: bool,
}
impl Default for WandSettings {
    fn default() -> Self {
        Self {
            tolerance: 32,
            sample_radius: 0,
            contiguous: true,
            all_layers: false,
            anti_alias: true,
        }
    }
}

pub fn wand(image: &RgbaImage, x: u32, y: u32, settings: &WandSettings) -> Result<Selection> {
    let (width, height) = image.dimensions();
    ensure!(
        width > 0 && height > 0 && x < width && y < height,
        "wand seed is outside image"
    );
    ensure!(
        settings.sample_radius <= 2,
        "wand sample radius must be 0, 1, or 2"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= crate::model::MAX_PIXELS,
        "wand image exceeds pixel limit"
    );
    let radius = u32::from(settings.sample_radius);
    let (x0, x1) = (
        x.saturating_sub(radius),
        x.saturating_add(radius).min(width - 1),
    );
    let (y0, y1) = (
        y.saturating_sub(radius),
        y.saturating_add(radius).min(height - 1),
    );
    // The preserved WandPixels kernel compares CGContext premultiplied bytes.
    let premultiplied = |x: u32, y: u32| {
        let p = image.get_pixel(x, y).0;
        [
            ((u16::from(p[0]) * u16::from(p[3]) + 127) / 255) as u8,
            ((u16::from(p[1]) * u16::from(p[3]) + 127) / 255) as u8,
            ((u16::from(p[2]) * u16::from(p[3]) + 127) / 255) as u8,
            p[3],
        ]
    };
    let mut sums = [0u64; 4];
    let mut samples = 0u64;
    for sy in y0..=y1 {
        for sx in x0..=x1 {
            let p = premultiplied(sx, sy);
            for c in 0..4 {
                sums[c] += u64::from(p[c]);
            }
            samples += 1;
        }
    }
    let reference: [u8; 4] = std::array::from_fn(|c| ((sums[c] + samples / 2) / samples) as u8);
    let matches = |px: u32, py: u32| {
        premultiplied(px, py)
            .iter()
            .zip(reference)
            .all(|(&a, b)| a.abs_diff(b) <= settings.tolerance)
    };
    let mut mask = vec![0; width as usize * height as usize];
    if settings.contiguous {
        let mut queue = VecDeque::new();
        queue.push_back((x, y));
        while let Some((px, py)) = queue.pop_front() {
            let index = py as usize * width as usize + px as usize;
            if mask[index] != 0 || !matches(px, py) {
                continue;
            }
            mask[index] = 255;
            if px > 0 {
                queue.push_back((px - 1, py));
            }
            if px + 1 < width {
                queue.push_back((px + 1, py));
            }
            if py > 0 {
                queue.push_back((px, py - 1));
            }
            if py + 1 < height {
                queue.push_back((px, py + 1));
            }
        }
    } else {
        for py in 0..height {
            for px in 0..width {
                if matches(px, py) {
                    mask[py as usize * width as usize + px as usize] = 255;
                }
            }
        }
    }
    Ok(Selection {
        width,
        height,
        mask,
    })
}

/// Select the thresholded foreground component under a point from the native
/// subject detector's grayscale output. This is the bounded fallback for
/// macOS instance selection: U2NETP supplies foreground probability rather
/// than distinct instance labels, so touching subjects remain one component.
pub fn object_from_subject_mask(
    subject: &GrayImage,
    x: u32,
    y: u32,
    edge_offset: i8,
) -> Result<Selection> {
    let (width, height) = subject.dimensions();
    ensure!(
        width > 0 && height > 0 && x < width && y < height,
        "object-selection point is outside mask"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= 16_777_216,
        "Object selection supports masks up to 16 million pixels"
    );
    ensure!(
        (-10..=10).contains(&edge_offset),
        "object-selection edge offset must be -10 through 10"
    );
    let index = |px: u32, py: u32| py as usize * width as usize + px as usize;
    let mut mask = vec![0u8; width as usize * height as usize];
    if subject.get_pixel(x, y)[0] >= 128 {
        let mut queue = VecDeque::new();
        queue.push_back((x, y));
        mask[index(x, y)] = 255;
        while let Some((px, py)) = queue.pop_front() {
            for (nx, ny) in [
                (px.wrapping_sub(1), py),
                (px.saturating_add(1), py),
                (px, py.wrapping_sub(1)),
                (px, py.saturating_add(1)),
            ] {
                if nx >= width || ny >= height {
                    continue;
                }
                let at = index(nx, ny);
                if mask[at] == 0 && subject.get_pixel(nx, ny)[0] >= 128 {
                    mask[at] = 255;
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    for _ in 0..edge_offset.unsigned_abs() {
        let previous = mask;
        let mut next = previous.clone();
        for py in 0..height {
            for px in 0..width {
                let at = index(px, py);
                if edge_offset > 0 && previous[at] != 0 {
                    let mut keep = true;
                    for ny in py.saturating_sub(1)..=py.saturating_add(1).min(height - 1) {
                        for nx in px.saturating_sub(1)..=px.saturating_add(1).min(width - 1) {
                            keep &= previous[index(nx, ny)] != 0;
                        }
                    }
                    next[at] = if keep { 255 } else { 0 };
                } else if edge_offset < 0 && previous[at] == 0 {
                    let mut fill = false;
                    for ny in py.saturating_sub(1)..=py.saturating_add(1).min(height - 1) {
                        for nx in px.saturating_sub(1)..=px.saturating_add(1).min(width - 1) {
                            fill |= previous[index(nx, ny)] != 0;
                        }
                    }
                    if fill {
                        next[at] = 255;
                    }
                }
            }
        }
        mask = next;
    }
    Ok(Selection {
        width,
        height,
        mask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn global_and_contiguous_regions_differ() {
        let image = RgbaImage::from_fn(5, 1, |x, _| {
            Rgba(if x == 2 {
                [0, 0, 0, 255]
            } else {
                [200, 0, 0, 255]
            })
        });
        let mut s = WandSettings {
            tolerance: 0,
            ..Default::default()
        };
        assert_eq!(
            wand(&image, 0, 0, &s).unwrap().mask,
            vec![255, 255, 0, 0, 0]
        );
        s.contiguous = false;
        assert_eq!(
            wand(&image, 0, 0, &s).unwrap().mask,
            vec![255, 255, 0, 255, 255]
        );
    }
    #[test]
    fn averaging_and_alpha_are_part_of_match() {
        let image =
            RgbaImage::from_raw(3, 1, vec![100, 0, 0, 0, 150, 0, 0, 255, 150, 0, 0, 128]).unwrap();
        let s = WandSettings {
            sample_radius: 1,
            tolerance: 0,
            contiguous: false,
            ..Default::default()
        };
        // Premultiplied RGB average is (0 + 150 + 75)/3 = 75;
        // averaged alpha is 128, matching only the third source pixel.
        assert_eq!(wand(&image, 1, 0, &s).unwrap().mask, vec![0, 0, 255]);
    }
    #[test]
    fn fractional_selection_algebra() {
        let a = Selection {
            width: 1,
            height: 1,
            mask: vec![128],
        };
        let b = Selection {
            width: 1,
            height: 1,
            mask: vec![128],
        };
        assert_eq!(combine(Some(&a), &b, SelectionMode::Add).mask, vec![192]);
        assert_eq!(
            combine(Some(&a), &b, SelectionMode::Subtract).mask,
            vec![64]
        );
    }
    #[test]
    fn object_mode_chooses_clicked_component() {
        let mut mask = GrayImage::new(6, 2);
        for &(x, y) in &[(0, 0), (1, 0), (4, 0), (4, 1)] {
            mask.get_pixel_mut(x, y)[0] = 255;
        }
        let selected = object_from_subject_mask(&mask, 4, 0, 0).unwrap();
        assert_eq!(selected.mask, vec![0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 0]);
        assert!(
            object_from_subject_mask(&mask, 3, 0, 0)
                .unwrap()
                .mask
                .iter()
                .all(|&v| v == 0)
        );
    }
    #[test]
    fn object_edge_offset_matches_source_morphology() {
        let mask = GrayImage::from_pixel(5, 5, image::Luma([255]));
        let eroded = object_from_subject_mask(&mask, 2, 2, 1).unwrap();
        // Source morphology treats the image boundary as bounded foreground.
        assert!(eroded.mask.iter().all(|&v| v == 255));
        let mut point = GrayImage::new(5, 5);
        point.get_pixel_mut(2, 2)[0] = 255;
        let expanded = object_from_subject_mask(&point, 2, 2, -1).unwrap();
        assert_eq!(expanded.mask.iter().filter(|&&v| v == 255).count(), 9);
    }
}
