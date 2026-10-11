//! Versioned texture-aware removal, independent of the preserved fill kernels.
//!
//! Uses immutable donor coordinates, texture-gradient matching, boundary-first
//! initialization and alternating patch-field refinement. Final pixels come from
//! a single best covering patch, avoiding a texture-blurring colour average.
//! This is an original bounded implementation informed by PhotoCraft's nonlocal
//! removal at 7722172585a01cbdb93c06f0f5ff2634fcb17999 and the Newson et al.
//! non-local inpainting / Barnes et al. PatchMatch approaches cited there.
//! It does not implement PhotoCraft's multiscale or gradient-domain solvers.
//!
//! Algorithm attribution: PhotoCraft, copyright (c) 2026 ArtCraft Team and the
//! PhotoCraft contributors, MIT. See `texture/UPSTREAM-LICENSE.txt`. All source
//! pixels remain immutable; callbacks receive original full-image coordinates,
//! allowing the caller to copy original 16-bit values without quantization.

use anyhow::{Result, ensure};
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
pub const MAX_REGION_PIXELS: usize = 4_000_000;
pub const MAX_TARGET_PIXELS: usize = 250_000;
pub const MAX_SCRATCH_BYTES: usize = 256 * 1024 * 1024;
/// Maximum compared patch cells, independent of image/region preparation.
/// The other stages are linear in the bounded source/region/selection sizes.
pub const MAX_PATCH_WORK: u64 = 256_000_000;
const PASSES: usize = 3;
const MISSING: u32 = u32::MAX;
const QUEUED: u32 = u32::MAX - 1;

#[derive(Clone, Copy)]
struct Region {
    x: u32,
    y: u32,
    w: usize,
    h: usize,
    selected: usize,
    random_candidates: usize,
}

impl Region {
    fn at(self, i: usize) -> (u32, u32) {
        (self.x + (i % self.w) as u32, self.y + (i / self.w) as u32)
    }

    fn full_index(self, i: usize, width: u32) -> usize {
        let (x, y) = self.at(i);
        y as usize * width as usize + x as usize
    }
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "texture removal cancelled");
    Ok(())
}

fn storage<T: Clone>(count: usize, value: T) -> Result<Vec<T>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| anyhow::anyhow!("Not enough memory for texture removal"))?;
    result.resize(count, value);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn region(
    width: u32,
    height: u32,
    target: &[u8],
    allowed: &[u8],
    search: u32,
    patch: u8,
    feather: f32,
    cancel: &AtomicBool,
) -> Result<Option<Region>> {
    check(cancel)?;
    let count = u64::from(width) * u64::from(height);
    ensure!(
        width > 0 && height > 0 && count <= MAX_SOURCE_PIXELS,
        "Texture removal needs a nonempty source of at most 64 megapixels"
    );
    ensure!(
        target.len() as u64 == count && allowed.len() == target.len(),
        "Texture removal masks must match the source dimensions"
    );
    ensure!(
        (1..=64).contains(&search) && patch <= 4,
        "Texture removal search radius must be 1–64 and patch radius 0–4"
    );
    ensure!(
        feather.is_finite() && (0.0..=1.0).contains(&feather),
        "Texture removal feather must be between zero and one"
    );
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    let mut selected = 0;
    for (i, &coverage) in target.iter().enumerate() {
        if i % 4096 == 0 {
            check(cancel)?;
        }
        if coverage == 0 {
            continue;
        }
        selected += 1;
        ensure!(
            selected <= MAX_TARGET_PIXELS,
            "Remove at most 250000 selected pixels at a time"
        );
        let (x, y) = (i as u32 % width, i as u32 / width);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if selected == 0 {
        return Ok(None);
    }
    let radius = u32::from(patch.max(1));
    // Candidate patch plus gradient feature support is retained around the
    // complete search area, including when the source is a large photograph.
    let margin = search + radius + 2;
    x0 = x0.saturating_sub(margin);
    y0 = y0.saturating_sub(margin);
    x1 = x1.saturating_add(margin).min(width - 1);
    y1 = y1.saturating_add(margin).min(height - 1);
    let (w, h) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
    let pixels = w
        .checked_mul(h)
        .ok_or_else(|| anyhow::anyhow!("Removal region overflow"))?;
    ensure!(
        pixels <= MAX_REGION_PIXELS,
        "Selected objects are too spread out: remove a smaller region at a time (4 million working pixels)"
    );
    // Conservative simultaneous storage: donor field, features, costs, valid
    // flags, source list, summed-area table, queue, deferred samples and rows.
    let bytes = pixels
        .checked_mul(32)
        .and_then(|v| v.checked_add(selected * 16))
        .and_then(|v| v.checked_add((w + h + 2) * 16))
        .ok_or_else(|| anyhow::anyhow!("Removal memory estimate overflow"))?;
    ensure!(
        bytes <= MAX_SCRATCH_BYTES,
        "Texture removal exceeds its 256 MiB working-memory limit"
    );
    let area = u64::from(radius * 2 + 1).pow(2);
    // Each search considers <=1 current +4 propagated +7 shrinking-radius
    // candidates, plus the random candidates. One final cost refresh follows.
    let comparisons = MAX_PATCH_WORK / selected as u64 / area;
    let candidates = comparisons.saturating_sub(1) / (PASSES as u64 + 1);
    ensure!(
        candidates >= 20,
        "Selection is too large for the minimum texture-search quality budget; use a smaller selection or patch"
    );
    let random_candidates = candidates.saturating_sub(12).min(24) as usize;
    Ok(Some(Region {
        x: x0,
        y: y0,
        w,
        h,
        selected,
        random_candidates,
    }))
}

/// Validate limits before scheduling a job. No working images are allocated.
#[allow(clippy::too_many_arguments)]
pub fn validate_inputs(
    width: u32,
    height: u32,
    target: &[u8],
    allowed: &[u8],
    search_radius: u32,
    patch_radius: u8,
    feather: f32,
) -> Result<()> {
    region(
        width,
        height,
        target,
        allowed,
        search_radius,
        patch_radius,
        feather,
        &AtomicBool::new(false),
    )?;
    Ok(())
}

struct Field<'a> {
    image: &'a RgbaImage,
    target: &'a [u8],
    allowed: &'a [u8],
    roi: Region,
    patch: i32,
    search: i32,
    donors: Vec<u32>,
    features: Vec<[f32; 2]>,
    valid: Vec<u8>,
    sources: Vec<u32>,
    source_rows: Vec<usize>,
}

fn neighbours(i: usize, w: usize, h: usize) -> [Option<usize>; 4] {
    let (x, y) = (i % w, i / w);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < w).then(|| i + 1),
        (y > 0).then(|| i - w),
        (y + 1 < h).then(|| i + w),
    ]
}

#[derive(Clone, Copy, Default)]
struct SourceRange {
    start: usize,
    end: usize,
}

impl Field<'_> {
    fn pixel(&self, i: usize) -> [u8; 4] {
        let (x, y) = self.roi.at(i);
        self.image.get_pixel(x, y).0
    }

    fn is_target(&self, i: usize) -> bool {
        self.target[self.roi.full_index(i, self.image.width())] > 0
    }

    fn luma(&self, i: usize) -> f32 {
        let p = self.pixel(i);
        (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
            * f32::from(p[3])
            / 255.0
    }

    fn prepare(&mut self, cancel: &AtomicBool) -> Result<()> {
        let Region { w, h, .. } = self.roi;
        // Features never look at the removed object's colours. Copied context
        // carries its donor's features, retaining evidence of texture inside.
        for y in 0..h {
            check(cancel)?;
            for x in 0..w {
                if x % 256 == 0 {
                    check(cancel)?;
                }
                let i = y * w + x;
                if self.is_target(i) {
                    continue;
                }
                let (mut sum, mut count) = ([0.0; 2], [0u32; 2]);
                for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        let j = yy * w + xx;
                        if self.is_target(j) {
                            continue;
                        }
                        for (axis, q) in [
                            (0, (xx + 1 < w).then(|| j + 1)),
                            (1, (yy + 1 < h).then(|| j + w)),
                        ] {
                            if let Some(q) = q.filter(|&q| !self.is_target(q)) {
                                sum[axis] += (self.luma(q) - self.luma(j)).abs();
                                count[axis] += 1;
                            }
                        }
                    }
                }
                self.features[i] = [
                    sum[0] / count[0].max(1) as f32,
                    sum[1] / count[1].max(1) as f32,
                ];
            }
        }
        let stride = w + 1;
        let mut invalid = storage(stride * (h + 1), 0u32)?;
        for y in 0..h {
            check(cancel)?;
            let mut row = 0;
            for x in 0..w {
                if x % 4096 == 0 {
                    check(cancel)?;
                }
                let i = y * w + x;
                let full = self.roi.full_index(i, self.image.width());
                row += u32::from(
                    self.target[full] > 0 || self.allowed[full] == 0 || self.pixel(i)[3] == 0,
                );
                invalid[(y + 1) * stride + x + 1] = invalid[y * stride + x + 1] + row;
            }
        }
        // Full source patches including feature support must be unselected
        // and allowed. This prevents both object leakage and forbidden donors.
        let margin = self.patch as usize + 2;
        for y in 0..h {
            check(cancel)?;
            self.source_rows[y] = self.sources.len();
            if y < margin || y + margin >= h {
                continue;
            }
            for x in margin..w.saturating_sub(margin) {
                if x % 4096 == 0 {
                    check(cancel)?;
                }
                let (x0, y0, x1, y1) = (x - margin, y - margin, x + margin + 1, y + margin + 1);
                if invalid[y1 * stride + x1] + invalid[y0 * stride + x0]
                    == invalid[y0 * stride + x1] + invalid[y1 * stride + x0]
                {
                    let i = y * w + x;
                    self.valid[i] = 1;
                    self.sources.push(i as u32);
                }
            }
        }
        self.source_rows[h] = self.sources.len();
        ensure!(
            !self.sources.is_empty(),
            "No fully allowed source patches: enlarge the sampling area or reduce the patch radius"
        );
        Ok(())
    }

    fn score(&self, target: usize, source: usize, cutoff: f32) -> f32 {
        let (x, y) = ((target % self.roi.w) as i32, (target / self.roi.w) as i32);
        let (sx, sy) = ((source % self.roi.w) as i32, (source / self.roi.w) as i32);
        let (mut sum, mut count) = (0.0, 0u32);
        let full = (self.patch * 2 + 1).pow(2) as f32;
        for dy in -self.patch..=self.patch {
            for dx in -self.patch..=self.patch {
                let (tx, ty) = (x + dx, y + dy);
                if tx < 0 || ty < 0 || tx >= self.roi.w as i32 || ty >= self.roi.h as i32 {
                    continue;
                }
                let donor = self.donors[ty as usize * self.roi.w + tx as usize];
                if donor >= self.donors.len() as u32 {
                    continue;
                }
                let j = (sy + dy) as usize * self.roi.w + (sx + dx) as usize;
                let (a, b) = (self.pixel(donor as usize), self.pixel(j));
                for c in 0..3 {
                    let d = (f32::from(a[c]) * f32::from(a[3]) - f32::from(b[c]) * f32::from(b[3]))
                        / 255.0;
                    sum += d * d;
                }
                sum += (f32::from(a[3]) - f32::from(b[3])).powi(2);
                for axis in 0..2 {
                    sum += 12.0
                        * (self.features[donor as usize][axis] - self.features[j][axis]).powi(2);
                }
                count += 1;
            }
            if sum > cutoff * full {
                return f32::INFINITY;
            }
        }
        if count == 0 {
            f32::INFINITY
        } else {
            sum / count as f32
        }
    }

    fn search(&self, i: usize, pass: u32) -> Result<u32> {
        let Region {
            w,
            h,
            random_candidates,
            ..
        } = self.roi;
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        let (x0, x1) = (
            (x - self.search).max(0),
            (x + self.search).min(w as i32 - 1),
        );
        let (y0, y1) = (
            (y - self.search).max(0),
            (y + self.search).min(h as i32 - 1),
        );
        let mut ranges = [SourceRange::default(); 129];
        let mut total = 0;
        for (slot, yy) in (y0..=y1).enumerate() {
            let row =
                &self.sources[self.source_rows[yy as usize]..self.source_rows[yy as usize + 1]];
            let low = (yy as usize * w + x0 as usize) as u32;
            let high = (yy as usize * w + x1 as usize) as u32;
            let begin = row.partition_point(|&v| v < low);
            let end = row.partition_point(|&v| v <= high);
            total += end - begin;
            ranges[slot] = SourceRange {
                start: self.source_rows[yy as usize] + begin,
                end: total,
            };
        }
        ensure!(
            total > 0,
            "No fully allowed source patch within the radius at {:?}; enlarge the sampling area or search radius",
            self.roi.at(i)
        );
        let mut best = MISSING;
        let mut best_score = f32::INFINITY;
        let mut best_distance = u64::MAX;
        let mut consider = |q: u32| {
            if q >= (w * h) as u32 || self.valid[q as usize] == 0 {
                return;
            }
            let (sx, sy) = ((q as usize % w) as i32, (q as usize / w) as i32);
            if sx < x0 || sy < y0 || sx > x1 || sy > y1 {
                return;
            }
            let cost = self.score(i, q as usize, best_score);
            let distance = u64::from(x.abs_diff(sx)).pow(2) + u64::from(y.abs_diff(sy)).pow(2);
            if cost < best_score || (cost == best_score && (distance, q) < (best_distance, best)) {
                best = q;
                best_score = cost;
                best_distance = distance;
            }
        };
        consider(self.donors[i]);
        for j in neighbours(i, w, h).into_iter().flatten() {
            let q = self.donors[j];
            if q >= (w * h) as u32 {
                continue;
            }
            let sx = (q as usize % w) as i32 + x - (j % w) as i32;
            let sy = (q as usize / w) as i32 + y - (j / w) as i32;
            if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                consider(sy as u32 * w as u32 + sx as u32);
            }
        }
        let mut rng =
            (i as u32).wrapping_mul(0x9e3779b9) ^ pass.wrapping_mul(0x85ebca6b) ^ 0x61c88647;
        for k in 0..random_candidates {
            // Stratify the entire eligible source set; sparse allowed regions
            // cannot be missed by random rejection or a raster stride alias.
            let lo = k * total / random_candidates;
            let hi = ((k + 1) * total / random_candidates).max(lo + 1);
            let rank = (lo + random(&mut rng) as usize % (hi - lo)).min(total - 1);
            let rows = &ranges[..(y1 - y0 + 1) as usize];
            let slot = rows.partition_point(|r| r.end <= rank);
            let previous = if slot == 0 { 0 } else { rows[slot - 1].end };
            consider(self.sources[rows[slot].start + rank - previous]);
        }
        // End the borrow held by `consider` before random local refinement.
        let initial_best = best;
        let mut radius = self.search;
        while radius >= 1 && initial_best != MISSING {
            let sx = (initial_best as usize % w) as i32
                + (random(&mut rng) % (radius as u32 * 2 + 1)) as i32
                - radius;
            let sy = (initial_best as usize / w) as i32
                + (random(&mut rng) % (radius as u32 * 2 + 1)) as i32
                - radius;
            if sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 {
                let q = sy as usize * w + sx as usize;
                if self.valid[q] > 0 {
                    let cost = self.score(i, q, best_score);
                    if cost < best_score {
                        best = q as u32;
                        best_score = cost;
                    }
                }
            }
            radius /= 2;
        }
        ensure!(
            best != MISSING && best_score.is_finite(),
            "Texture removal needs known surrounding context at {:?}",
            self.roi.at(i)
        );
        Ok(best)
    }
}

fn random(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    *state
}

/// Compute every match before invoking the first callback. A failed search or
/// cancelled synthesis emits no samples. Callers must stage callback writes and
/// publish only after success, since cancellation/callback errors can also occur
/// during the final copy. Unselected pixels never receive callbacks.
#[allow(clippy::too_many_arguments)]
pub fn visit_samples(
    image: &RgbaImage,
    target: &[u8],
    allowed: &[u8],
    search_radius: u32,
    patch_radius: u8,
    feather: f32,
    cancel: &AtomicBool,
    mut sample: impl FnMut(u32, u32, u32, u32, f32) -> Result<()>,
) -> Result<()> {
    let Some(roi) = region(
        image.width(),
        image.height(),
        target,
        allowed,
        search_radius,
        patch_radius,
        feather,
        cancel,
    )?
    else {
        return Ok(());
    };
    let n = roi.w * roi.h;
    let mut donors = storage(n, MISSING)?;
    for (i, value) in donors.iter_mut().enumerate() {
        if i % 4096 == 0 {
            check(cancel)?;
        }
        if target[roi.full_index(i, image.width())] == 0 {
            *value = i as u32;
        }
    }
    let mut sources = Vec::new();
    sources
        .try_reserve_exact(n)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal sources"))?;
    let mut field = Field {
        image,
        target,
        allowed,
        roi,
        patch: i32::from(patch_radius.max(1)),
        search: search_radius as i32,
        donors,
        features: storage(n, [0.0; 2])?,
        valid: storage(n, 0u8)?,
        sources,
        source_rows: storage(roi.h + 1, 0usize)?,
    };
    field.prepare(cancel)?;
    let mut front = Vec::<u32>::new();
    front
        .try_reserve_exact(roi.selected)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal boundary"))?;
    for i in 0..n {
        if i % 4096 == 0 {
            check(cancel)?;
        }
        if field.donors[i] == MISSING
            && neighbours(i, roi.w, roi.h)
                .into_iter()
                .flatten()
                .any(|j| field.donors[j] < n as u32)
        {
            front.push(i as u32);
            field.donors[i] = QUEUED;
        }
    }
    let mut cursor = 0;
    while cursor < front.len() {
        check(cancel)?;
        let i = front[cursor] as usize;
        cursor += 1;
        field.donors[i] = field.search(i, 0)?;
        for j in neighbours(i, roi.w, roi.h).into_iter().flatten() {
            if field.donors[j] == MISSING {
                field.donors[j] = QUEUED;
                front.push(j as u32);
            }
        }
    }
    ensure!(
        front.len() == roi.selected,
        "Texture removal needs unselected surrounding context"
    );
    for pass in 0..PASSES {
        for index in 0..n {
            let i = if pass % 2 == 0 { n - 1 - index } else { index };
            if index % 4096 == 0 {
                check(cancel)?;
            }
            if field.is_target(i) {
                check(cancel)?;
                field.donors[i] = field.search(i, pass as u32 + 1)?;
            }
        }
    }
    let mut costs = storage(n, f32::INFINITY)?;
    for &i in &front {
        check(cancel)?;
        costs[i as usize] =
            field.score(i as usize, field.donors[i as usize] as usize, f32::INFINITY);
    }
    let mut samples = Vec::<(u32, u32, f32)>::new();
    samples
        .try_reserve_exact(roi.selected)
        .map_err(|_| anyhow::anyhow!("Not enough memory for removal result"))?;
    for i in 0..n {
        if i % 4096 == 0 {
            check(cancel)?;
        }
        if !field.is_target(i) {
            continue;
        }
        let (x, y) = ((i % roi.w) as i32, (i / roi.w) as i32);
        let (mut best, mut cost) = (field.donors[i], costs[i]);
        // Copy a whole patch's contribution, rather than averaging unrelated
        // donor colours and erasing their natural grain.
        for dy in -field.patch..=field.patch {
            for dx in -field.patch..=field.patch {
                let (cx, cy) = (x + dx, y + dy);
                if cx < 0 || cy < 0 || cx >= roi.w as i32 || cy >= roi.h as i32 {
                    continue;
                }
                let j = cy as usize * roi.w + cx as usize;
                if costs[j] < cost {
                    let centre = field.donors[j];
                    let sx = (centre as usize % roi.w) as i32 - dx;
                    let sy = (centre as usize / roi.w) as i32 - dy;
                    // Full allowed source patches make this safe; retain an
                    // explicit check before resolving a persisted donor.
                    if sx < 0 || sy < 0 || sx >= roi.w as i32 || sy >= roi.h as i32 {
                        continue;
                    }
                    let q = sy as usize * roi.w + sx as usize;
                    let full = roi.full_index(q, image.width());
                    if target[full] == 0 && allowed[full] > 0 {
                        best = q as u32;
                        cost = costs[j];
                    }
                }
            }
        }
        let full = roi.full_index(i, image.width());
        let donor_full = roi.full_index(best as usize, image.width());
        let confidence = (1.0 - cost / (255.0 * 255.0 * 4.0)).clamp(0.0, 1.0);
        let amount = f32::from(target[full]) / 255.0 * f32::from(allowed[donor_full]) / 255.0
            * (1.0 - feather + feather * confidence);
        samples.push((i as u32, best, amount));
    }
    check(cancel)?;
    for (i, q, amount) in samples {
        check(cancel)?;
        let (x, y) = roi.at(i as usize);
        let (sx, sy) = roi.at(q as usize);
        sample(x, y, sx, sy, amount)?;
    }
    check(cancel)
}

#[cfg(test)]
#[path = "texture/tests.rs"]
mod tests;
