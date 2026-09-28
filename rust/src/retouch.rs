//! Deterministic content-aware fill ported from the preserved compositor kernel.
use anyhow::{Result, ensure};
use image::{GrayImage, RgbaImage};
fn next_random(s: &mut u32) -> u32 {
    *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
    *s
}
fn score(px: &RgbaImage, known: &[bool], w: usize, h: usize, p: usize, q: usize, r: i32) -> f64 {
    let (px0, py0, qx, qy) = (p % w, p / w, q % w, q / w);
    let (mut sum, mut count) = (0f64, 0);
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y, sx, sy) = (
                px0 as i32 + dx,
                py0 as i32 + dy,
                qx as i32 + dx,
                qy as i32 + dy,
            );
            if x < 0
                || y < 0
                || sx < 0
                || sy < 0
                || x >= w as i32
                || y >= h as i32
                || sx >= w as i32
                || sy >= h as i32
                || !known[y as usize * w + x as usize]
            {
                continue;
            }
            let (a, b) = (
                px.get_pixel(x as u32, y as u32),
                px.get_pixel(sx as u32, sy as u32),
            );
            for c in 0..4 {
                let d = i32::from(a[c]) - i32::from(b[c]);
                sum += f64::from(d * d)
            }
            count += 1
        }
    }
    if count == 0 {
        f64::MAX
    } else {
        sum / f64::from(count)
    }
}
/// Fill nonzero mask pixels from surrounding unmasked opaque pixels.
/// Failure leaves the image unchanged; success is deterministic.
pub fn content_fill(px: &mut RgbaImage, mask: &GrayImage) -> Result<()> {
    ensure!(
        px.dimensions() == mask.dimensions() && px.width() > 0,
        "Content Fill image and mask dimensions must match"
    );
    let (w, h) = (px.width() as usize, px.height() as usize);
    let n = w
        .checked_mul(h)
        .ok_or_else(|| anyhow::anyhow!("Content Fill dimensions overflow"))?;
    let (mut known, mut target, mut valid, mut queued) = (
        vec![false; n],
        vec![false; n],
        vec![false; n],
        vec![false; n],
    );
    let (mut donors, mut queue, mut chosen) =
        (Vec::with_capacity(n), Vec::with_capacity(n), vec![None; n]);
    let radius = if w >= 5 && h >= 5 { 2 } else { 0 };
    let mut missing = 0;
    for y in 0..h {
        for x in 0..w {
            let p = y * w + x;
            target[p] = mask.get_pixel(x as u32, y as u32)[0] != 0;
            known[p] = !target[p] && px.get_pixel(x as u32, y as u32)[3] == 255;
            if target[p] {
                missing += 1
            }
        }
    }
    if missing == 0 {
        return Ok(());
    }
    for y in 0..h {
        for x in 0..w {
            let p = y * w + x;
            if !known[p] {
                continue;
            }
            let mut ok = true;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let (sx, sy) = (x as i32 + dx, y as i32 + dy);
                    if sx < 0
                        || sy < 0
                        || sx >= w as i32
                        || sy >= h as i32
                        || !known[sy as usize * w + sx as usize]
                    {
                        ok = false
                    }
                }
            }
            if ok {
                valid[p] = true;
                donors.push(p)
            }
        }
    }
    ensure!(
        !donors.is_empty(),
        "Not enough unselected opaque pixels to synthesize a fill"
    );
    for y in 0..h {
        for x in 0..w {
            let p = y * w + x;
            if target[p]
                && ((x > 0 && known[p - 1])
                    || (x + 1 < w && known[p + 1])
                    || (y > 0 && known[p - w])
                    || (y + 1 < h && known[p + w]))
            {
                queue.push(p);
                queued[p] = true
            }
        }
    }
    let (mut head, mut scan, mut seed) = (0usize, 0usize, 0x6d2b79f5u32);
    loop {
        while head < queue.len() {
            let p = queue[head];
            head += 1;
            let (x, y) = (p % w, p / w);
            let ns = [
                if x > 0 { Some(p - 1) } else { None },
                if x + 1 < w { Some(p + 1) } else { None },
                if y > 0 { Some(p - w) } else { None },
                if y + 1 < h { Some(p + w) } else { None },
            ];
            let (mut best, mut best_score) = (None, f64::MAX);
            for k in 0..28 {
                let cq = if k < 4 {
                    ns[k].and_then(|t| {
                        let b = chosen[t].unwrap_or(t);
                        b.checked_add(p)?.checked_sub(t)
                    })
                } else {
                    Some(donors[next_random(&mut seed) as usize % donors.len()])
                };
                let Some(q) = cq.filter(|q| *q < n && valid[*q]) else {
                    continue;
                };
                let s = score(px, &known, w, h, p, q, radius);
                if best.is_none() || s < best_score {
                    best = Some(q);
                    best_score = s
                }
            }
            let mut best = best.unwrap_or(donors[0]);
            let mut r = 64;
            while r >= 1 {
                let cx = best as i32 % w as i32
                    + (next_random(&mut seed) % (2 * r as u32 + 1)) as i32
                    - r;
                let cy = best as i32 / w as i32
                    + (next_random(&mut seed) % (2 * r as u32 + 1)) as i32
                    - r;
                if cx >= 0 && cy >= 0 && cx < w as i32 && cy < h as i32 {
                    let q = cy as usize * w + cx as usize;
                    if valid[q] {
                        let s = score(px, &known, w, h, p, q, radius);
                        if s < best_score {
                            best_score = s;
                            best = q
                        }
                    }
                }
                r /= 2
            }
            let donor = *px.get_pixel((best % w) as u32, (best / w) as u32);
            px.put_pixel(x as u32, y as u32, donor);
            known[p] = true;
            chosen[p] = Some(best);
            for q in ns.into_iter().flatten() {
                if target[q] && !known[q] && !queued[q] {
                    queued[q] = true;
                    queue.push(q)
                }
            }
        }
        while scan < n && (!target[scan] || known[scan]) {
            scan += 1
        }
        if scan >= n {
            break;
        }
        queue.push(scan);
        queued[scan] = true
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};
    #[test]
    fn unchanged_without_selection() {
        let mut p = RgbaImage::from_pixel(3, 3, Rgba([1, 2, 3, 255]));
        let b = p.clone();
        content_fill(&mut p, &GrayImage::new(3, 3)).unwrap();
        assert_eq!(p, b)
    }
    #[test]
    fn deterministic_and_opaque_donors() {
        let mut p = RgbaImage::from_pixel(11, 11, Rgba([20, 40, 60, 255]));
        p.put_pixel(0, 0, Rgba([255, 0, 0, 0]));
        let mut m = GrayImage::new(11, 11);
        m.put_pixel(5, 5, Luma([255]));
        let mut q = p.clone();
        content_fill(&mut p, &m).unwrap();
        content_fill(&mut q, &m).unwrap();
        assert_eq!(p, q);
        assert_eq!(p.get_pixel(5, 5).0, [20, 40, 60, 255])
    }
    #[test]
    fn failure_preserves_pixels() {
        let mut p = RgbaImage::from_pixel(5, 5, Rgba([0, 0, 0, 0]));
        let m = GrayImage::from_pixel(5, 5, Luma([255]));
        let b = p.clone();
        assert!(content_fill(&mut p, &m).is_err());
        assert_eq!(p, b);
        assert!(content_fill(&mut p, &GrayImage::new(4, 5)).is_err())
    }
}
