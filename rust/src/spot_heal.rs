//! Direct Rust port of `Compositor/Rendering/HealPixels.c`.
use anyhow::{Result, ensure};
use image::{GrayImage, RgbaImage};

const OUTSIDE: u8 = 0;
const RING: u8 = 1;
const HOLE: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotHealingMode {
    ContentAware,
    CreateTexture,
    ProximityMatch,
}

/// Heal pixels covered by `coverage`. The public image uses straight alpha;
/// the preserved kernel is evaluated in premultiplied RGBA, like the Mac renderer.
pub fn apply(
    image: &mut RgbaImage,
    coverage: &GrayImage,
    opacity: f32,
    mode: SpotHealingMode,
    seed: u32,
) -> Result<bool> {
    ensure!(
        image.dimensions() == coverage.dimensions(),
        "Spot healing coverage dimensions differ from the image"
    );
    ensure!(
        opacity.is_finite() && (0.0..=1.0).contains(&opacity),
        "Spot healing opacity must be between 0 and 1"
    );
    let pixels = u64::from(image.width()) * u64::from(image.height());
    ensure!(
        pixels <= 16_777_216,
        "Spot healing supports images up to 16 million pixels"
    );
    let Some(bounds) = coverage_bounds(coverage) else {
        return Ok(false);
    };
    if opacity == 0.0 {
        return Ok(false);
    }
    let before = image.clone();
    let mut rgba = Vec::with_capacity(pixels as usize * 4);
    for p in image.pixels() {
        let a = p[3];
        rgba.extend_from_slice(&[
            ((u16::from(p[0]) * u16::from(a) + 127) / 255) as u8,
            ((u16::from(p[1]) * u16::from(a) + 127) / 255) as u8,
            ((u16::from(p[2]) * u16::from(a) + 127) / 255) as u8,
            a,
        ]);
    }
    let premultiplied_before = rgba.clone();
    kernel(
        &mut rgba,
        coverage.as_raw(),
        image.width() as usize,
        image.height() as usize,
        opacity,
        mode,
        seed,
        bounds,
    )?;
    for (index, (p, q)) in image.pixels_mut().zip(rgba.chunks_exact(4)).enumerate() {
        if coverage.as_raw()[index] == 0 {
            continue;
        }
        if q == &premultiplied_before[index * 4..index * 4 + 4] {
            continue;
        }
        let a = q[3];
        p.0 = if a == 0 {
            [0; 4]
        } else {
            [
                ((u32::from(q[0]) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
                ((u32::from(q[1]) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
                ((u32::from(q[2]) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
                a,
            ]
        };
    }
    Ok(*image != before)
}

fn coverage_bounds(c: &GrayImage) -> Option<[usize; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (c.width() as usize, c.height() as usize, 0, 0);
    for (x, y, p) in c.enumerate_pixels() {
        if p[0] != 0 {
            x0 = x0.min(x as usize);
            y0 = y0.min(y as usize);
            x1 = x1.max(x as usize + 1);
            y1 = y1.max(y as usize + 1);
        }
    }
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}
fn unit(k: u32) -> f64 {
    f64::from(hash(k) >> 8) / 16_777_216.0
}
fn score(
    r: &[u8],
    role: &[u8],
    wx0: isize,
    wy0: isize,
    ww: usize,
    wh: usize,
    dx: isize,
    dy: isize,
    w: usize,
    h: usize,
) -> f64 {
    if dx.abs() < ww as isize && dy.abs() < wh as isize {
        return f64::INFINITY;
    }
    if wx0 + dx < 0
        || wy0 + dy < 0
        || wx0 + ww as isize + dx > w as isize
        || wy0 + wh as isize + dy > h as isize
    {
        return f64::INFINITY;
    }
    let (mut sum, mut n) = (0f64, 0usize);
    for y in 0..wh {
        for x in 0..ww {
            if role[y * ww + x] != RING {
                continue;
            }
            let ti = ((wy0 + y as isize) as usize * w + (wx0 + x as isize) as usize) * 4;
            let si =
                (((wy0 + y as isize + dy) as usize) * w + (wx0 + x as isize + dx) as usize) * 4;
            for c in 0..4 {
                let d = f64::from(r[ti + c]) - f64::from(r[si + c]);
                sum += d * d;
            }
            n += 1;
        }
    }
    if n == 0 {
        f64::INFINITY
    } else {
        sum / n as f64
    }
}
fn solve(v: &mut [f32], role: &[u8], w: usize, h: usize, depth: u8) {
    let mut iterations = 300;
    if w > 32 && h > 32 && depth < 16 {
        let (cw, ch) = ((w + 1) / 2, (h + 1) / 2);
        let mut cv = vec![0f32; cw * ch * 4];
        let mut cr = vec![0u8; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                let (mut known, mut hole) = (0, 0);
                let (mut ks, mut hs) = ([0f32; 4], [0f32; 4]);
                for j in 0..2 {
                    for i in 0..2 {
                        let (fx, fy) = (x * 2 + i, y * 2 + j);
                        if fx >= w || fy >= h {
                            continue;
                        }
                        let p = fy * w + fx;
                        if role[p] == RING {
                            known += 1;
                            for c in 0..4 {
                                ks[c] += v[p * 4 + c]
                            }
                        } else if role[p] == HOLE {
                            hole += 1;
                            for c in 0..4 {
                                hs[c] += v[p * 4 + c]
                            }
                        }
                    }
                }
                let q = y * cw + x;
                if known > 0 {
                    cr[q] = RING;
                    for c in 0..4 {
                        cv[q * 4 + c] = ks[c] / known as f32
                    }
                } else if hole > 0 {
                    cr[q] = HOLE;
                    for c in 0..4 {
                        cv[q * 4 + c] = hs[c] / hole as f32
                    }
                }
            }
        }
        solve(&mut cv, &cr, cw, ch, depth + 1);
        for y in 0..h {
            for x in 0..w {
                let (p, q) = (y * w + x, (y / 2) * cw + x / 2);
                if role[p] == HOLE && cr[q] == HOLE {
                    v[p * 4..p * 4 + 4].copy_from_slice(&cv[q * 4..q * 4 + 4]);
                }
            }
        }
        iterations = 40;
    }
    for _ in 0..iterations {
        for y in 0..h {
            for x in 0..w {
                let p = y * w + x;
                if role[p] != HOLE {
                    continue;
                }
                let mut s = [0f32; 4];
                let mut n = 0f32;
                for (nx, ny) in [
                    (x as isize - 1, y as isize),
                    (x as isize + 1, y as isize),
                    (x as isize, y as isize - 1),
                    (x as isize, y as isize + 1),
                ] {
                    if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                        continue;
                    }
                    let q = ny as usize * w + nx as usize;
                    if role[q] == OUTSIDE {
                        continue;
                    }
                    for c in 0..4 {
                        s[c] += v[q * 4 + c]
                    }
                    n += 1.;
                }
                if n > 0. {
                    for c in 0..4 {
                        v[p * 4 + c] += 1.8f32 * (s[c] / n - v[p * 4 + c]);
                    }
                }
            }
        }
    }
}
fn kernel(
    r: &mut [u8],
    cov: &[u8],
    w: usize,
    h: usize,
    opacity: f32,
    mode: SpotHealingMode,
    seed: u32,
    b: [usize; 4],
) -> Result<()> {
    let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
    let ring = (bw.max(bh) / 8).clamp(2, 16);
    let (wx0, wy0) = (b[0].saturating_sub(ring), b[1].saturating_sub(ring));
    let (wx1, wy1) = ((b[2] + ring).min(w), (b[3] + ring).min(h));
    let (ww, wh) = (wx1 - wx0, wy1 - wy0);
    let wn = ww * wh;
    let (mut role, mut near, mut prefix, mut value) = (
        vec![0u8; wn],
        vec![0u8; wn],
        vec![0isize; ww.max(wh) + 1],
        vec![0f32; wn * 4],
    );
    for y in 0..wh {
        for x in 0..ww {
            if cov[(wy0 + y) * w + wx0 + x] != 0 {
                role[y * ww + x] = HOLE
            }
        }
    }
    for y in 0..wh {
        prefix[0] = 0;
        for x in 0..ww {
            prefix[x + 1] = prefix[x] + isize::from(role[y * ww + x] == HOLE)
        }
        for x in 0..ww {
            let (lo, hi) = (x.saturating_sub(ring), (x + ring + 1).min(ww));
            near[y * ww + x] = u8::from(prefix[hi] - prefix[lo] > 0)
        }
    }
    for x in 0..ww {
        prefix[0] = 0;
        for y in 0..wh {
            prefix[y + 1] = prefix[y] + isize::from(near[y * ww + x] != 0)
        }
        for y in 0..wh {
            let (lo, hi) = (y.saturating_sub(ring), (y + ring + 1).min(wh));
            if role[y * ww + x] == OUTSIDE && prefix[hi] - prefix[lo] > 0 {
                role[y * ww + x] = RING
            }
        }
    }
    let rc = role.iter().filter(|&&q| q == RING).count();
    if rc == 0 {
        return Ok(());
    }
    let (mut ox, mut oy, mut have) = (0isize, 0isize, false);
    if mode != SpotHealingMode::CreateTexture {
        let factors = [1.05, 1.35, 1.75, 2.25, 2.8];
        let count = if mode == SpotHealingMode::ProximityMatch {
            2
        } else {
            5
        };
        let mut best = f64::INFINITY;
        for (fi, f) in factors[..count].iter().enumerate() {
            for a in 0..24 {
                let angle = a as f64 * std::f64::consts::PI / 12.;
                let dx = (angle.cos() * f * ww as f64).round() as isize;
                let dy = (angle.sin() * f * wh as f64).round() as isize;
                let mut sc = score(r, &role, wx0 as isize, wy0 as isize, ww, wh, dx, dy, w, h);
                sc *= if mode == SpotHealingMode::ProximityMatch {
                    1. + 0.6 * fi as f64
                } else {
                    1. + 0.1 * fi as f64
                };
                if sc < best {
                    best = sc;
                    ox = dx;
                    oy = dy
                }
            }
        }
        if best.is_finite() {
            let (cx, cy) = (ox, oy);
            let mut refined = score(r, &role, wx0 as isize, wy0 as isize, ww, wh, cx, cy, w, h);
            for j in -3..=3 {
                for i in -3..=3 {
                    let sc = score(
                        r,
                        &role,
                        wx0 as isize,
                        wy0 as isize,
                        ww,
                        wh,
                        cx + i,
                        cy + j,
                        w,
                        h,
                    );
                    if sc < refined {
                        refined = sc;
                        ox = cx + i;
                        oy = cy + j
                    }
                }
            }
            have = true
        }
    }
    let (mut mean, mut detail) = ([0f64; 4], [0f64; 3]);
    for y in 0..wh {
        for x in 0..ww {
            let p = y * ww + x;
            if role[p] != RING {
                continue;
            }
            let (ix, iy) = (wx0 + x, wy0 + y);
            let ti = (iy * w + ix) * 4;
            let si =
                have.then(|| ((iy as isize + oy) as usize * w + (ix as isize + ox) as usize) * 4);
            for c in 0..4 {
                value[p * 4 + c] = f32::from(r[ti + c]) - si.map_or(0f32, |q| f32::from(r[q + c]));
                mean[c] += f64::from(value[p * 4 + c]);
            }
            if !have {
                for c in 0..3 {
                    let (mut around, mut n) = (0f64, 0usize);
                    for (nx, ny) in [
                        (ix as isize - 1, iy as isize),
                        (ix as isize + 1, iy as isize),
                        (ix as isize, iy as isize - 1),
                        (ix as isize, iy as isize + 1),
                    ] {
                        if nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize {
                            around += f64::from(r[(ny as usize * w + nx as usize) * 4 + c]);
                            n += 1
                        }
                    }
                    if n > 0 {
                        let d = f64::from(r[ti + c]) - around / n as f64;
                        detail[c] += d * d
                    }
                }
            }
        }
    }
    for m in &mut mean {
        *m /= rc as f64
    }
    for p in 0..wn {
        if role[p] == HOLE {
            for c in 0..4 {
                value[p * 4 + c] = mean[c] as f32
            }
        }
    }
    solve(&mut value, &role, ww, wh, 0);
    for d in &mut detail {
        *d = (*d / rc as f64).sqrt() * 0.9
    }
    for y in 0..wh {
        for x in 0..ww {
            let p = y * ww + x;
            if role[p] != HOLE {
                continue;
            }
            let (ix, iy) = (wx0 + x, wy0 + y);
            let ti = (iy * w + ix) * 4;
            let si =
                have.then(|| ((iy as isize + oy) as usize * w + (ix as isize + ox) as usize) * 4);
            let amount = f64::from(cov[iy * w + ix]) / 255. * f64::from(opacity);
            let grain = if have {
                0.
            } else {
                let key = hash(seed ^ hash((iy * w + ix) as u32));
                (-2. * (1. - unit(key)).ln()).sqrt()
                    * (2. * std::f64::consts::PI * unit(key ^ 0x68e31da4)).cos()
            };
            let mut out = [0f64; 4];
            for c in 0..4 {
                let healed = si.map_or(0f64, |q| f64::from(r[q + c]))
                    + f64::from(value[p * 4 + c])
                    + if c < 3 { grain * detail[c] } else { 0. };
                out[c] = f64::from(r[ti + c]) + (healed - f64::from(r[ti + c])) * amount;
            }
            r[ti + 3] = out[3].clamp(0., 255.).round() as u8;
            for c in 0..3 {
                r[ti + c] = out[c].clamp(0., f64::from(r[ti + 3])).round() as u8
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    fn fixture() -> (RgbaImage, GrayImage) {
        let mut image = RgbaImage::new(48, 48);
        let mut coverage = GrayImage::new(48, 48);
        for y in 0..48 {
            for x in 0..48 {
                image.put_pixel(
                    x,
                    y,
                    Rgba([
                        ((x * 17 + y * 3 + x * y % 19) % 256) as u8,
                        ((x * 5 + y * 13 + x * y % 23) % 256) as u8,
                        ((x * 11 + y * 7 + x * y % 29) % 256) as u8,
                        255,
                    ]),
                );
                if (x as i32 - 24).pow(2) + (y as i32 - 23).pow(2) <= 16 {
                    coverage.put_pixel(x, y, Luma([(80 + ((x + y) * 13) % 176) as u8]));
                }
            }
        }
        (image, coverage)
    }

    fn fnv(bytes: &[u8]) -> u64 {
        bytes.iter().fold(1_469_598_103_934_665_603, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211)
        })
    }

    #[test]
    fn all_modes_match_preserved_c_fixture() {
        let expected = [
            (SpotHealingMode::ContentAware, 3_909_187_360_276_412_737),
            (SpotHealingMode::CreateTexture, 10_762_953_494_576_293_206),
            (SpotHealingMode::ProximityMatch, 3_909_187_360_276_412_737),
        ];
        for (mode, checksum) in expected {
            let (mut image, coverage) = fixture();
            assert!(apply(&mut image, &coverage, 0.75, mode, 0x1234_5678).unwrap());
            assert_eq!(fnv(image.as_raw()), checksum, "{mode:?}");
        }
    }

    #[test]
    fn empty_coverage_is_an_exact_noop() {
        let (mut image, _) = fixture();
        let before = image.clone();
        assert!(
            !apply(
                &mut image,
                &GrayImage::new(48, 48),
                1.0,
                SpotHealingMode::ContentAware,
                1
            )
            .unwrap()
        );
        assert_eq!(image, before);
    }

    #[test]
    fn uncovered_low_alpha_and_hidden_rgb_are_bit_exact() {
        let (mut image, mut coverage) = fixture();
        image.put_pixel(0, 0, Rgba([201, 17, 99, 37]));
        image.put_pixel(1, 0, Rgba([77, 88, 99, 0]));
        coverage.put_pixel(24, 23, Luma([255]));
        let outside = [*image.get_pixel(0, 0), *image.get_pixel(1, 0)];
        apply(
            &mut image,
            &coverage,
            1.0,
            SpotHealingMode::CreateTexture,
            7,
        )
        .unwrap();
        assert_eq!([*image.get_pixel(0, 0), *image.get_pixel(1, 0)], outside);
    }
}
