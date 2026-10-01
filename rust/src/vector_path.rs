//! Platform-independent cubic paths. Coordinates are canvas pixels, with y down.
//! Filling supports non-zero and even-odd winding; open subpaths are implicitly
//! closed for filling and remain open for stroking.
use anyhow::{Result, ensure};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

const MAX_SUBPATHS: usize = 4_096;
const MAX_ANCHORS: usize = 100_000;
const MAX_COORDINATE: f32 = 1_000_000.;
const MAX_FLAT_POINTS: usize = 2_000_000;
const MAX_RASTER_PIXELS: u64 = 16_777_216;
const MAX_STROKE_TESTS: u64 = 268_435_456;
const MAX_FILL_WORK: u64 = 300_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
    fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub position: Point,
    pub incoming: Option<Point>,
    pub outgoing: Option<Point>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subpath {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorPath {
    pub subpaths: Vec<Subpath>,
    pub fill_rule: FillRule,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeStyle {
    pub color: [u8; 4],
    pub width: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FlatSubpath {
    pub points: Vec<Point>,
    pub closed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Anchor {
        subpath: usize,
        anchor: usize,
    },
    Segment {
        subpath: usize,
        segment: usize,
        t: f32,
    },
}

impl VectorPath {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.subpaths.len() <= MAX_SUBPATHS,
            "too many vector subpaths"
        );
        let mut count = 0usize;
        for sub in &self.subpaths {
            ensure!(
                !sub.closed || sub.anchors.len() >= 2,
                "closed subpath needs two anchors"
            );
            count = count.saturating_add(sub.anchors.len());
            ensure!(count <= MAX_ANCHORS, "too many vector anchors");
            for a in &sub.anchors {
                for p in [Some(a.position), a.incoming, a.outgoing]
                    .into_iter()
                    .flatten()
                {
                    ensure!(
                        p.x.is_finite()
                            && p.y.is_finite()
                            && p.x.abs() <= MAX_COORDINATE
                            && p.y.abs() <= MAX_COORDINATE,
                        "invalid vector coordinate"
                    );
                }
            }
        }
        Ok(())
    }
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut lo = Point {
            x: f32::INFINITY,
            y: f32::INFINITY,
        };
        let mut hi = Point {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        };
        let mut any = false;
        for s in &self.subpaths {
            for a in &s.anchors {
                for p in [Some(a.position), a.incoming, a.outgoing]
                    .into_iter()
                    .flatten()
                {
                    any = true;
                    lo.x = lo.x.min(p.x);
                    lo.y = lo.y.min(p.y);
                    hi.x = hi.x.max(p.x);
                    hi.y = hi.y.max(p.y)
                }
            }
        }
        any.then_some((lo, hi))
    }
    pub fn move_anchor(&mut self, sub: usize, anchor: usize, to: Point) -> Result<()> {
        ensure!(to.x.is_finite() && to.y.is_finite(), "invalid point");
        let a = self
            .subpaths
            .get_mut(sub)
            .and_then(|s| s.anchors.get_mut(anchor))
            .ok_or_else(|| anyhow::anyhow!("anchor index out of range"))?;
        let d = Point {
            x: to.x - a.position.x,
            y: to.y - a.position.y,
        };
        a.position = to;
        for h in [&mut a.incoming, &mut a.outgoing] {
            if let Some(p) = h {
                p.x += d.x;
                p.y += d.y
            }
        }
        self.validate()
    }
    pub fn delete_anchor(&mut self, sub: usize, anchor: usize) -> Result<Anchor> {
        let s = self
            .subpaths
            .get_mut(sub)
            .ok_or_else(|| anyhow::anyhow!("subpath index out of range"))?;
        ensure!(anchor < s.anchors.len(), "anchor index out of range");
        Ok(s.anchors.remove(anchor))
    }
    /// Split a cubic segment exactly with De Casteljau and insert its on-curve node.
    pub fn insert_on_segment(&mut self, sub: usize, segment: usize, t: f32) -> Result<usize> {
        self.validate()?;
        ensure!(
            self.subpaths.iter().map(|s| s.anchors.len()).sum::<usize>() < MAX_ANCHORS,
            "too many vector anchors"
        );
        ensure!(
            t.is_finite() && (0.0..=1.0).contains(&t),
            "invalid split parameter"
        );
        let s = self
            .subpaths
            .get_mut(sub)
            .ok_or_else(|| anyhow::anyhow!("subpath index out of range"))?;
        let n = s.anchors.len();
        ensure!(
            n >= 2 && segment < if s.closed { n } else { n - 1 },
            "segment index out of range"
        );
        let j = (segment + 1) % n;
        let p0 = s.anchors[segment].position;
        let p1 = s.anchors[segment].outgoing.unwrap_or(p0);
        let p3 = s.anchors[j].position;
        let p2 = s.anchors[j].incoming.unwrap_or(p3);
        let a = p0.lerp(p1, t);
        let b = p1.lerp(p2, t);
        let c = p2.lerp(p3, t);
        let d = a.lerp(b, t);
        let e = b.lerp(c, t);
        let p = d.lerp(e, t);
        s.anchors[segment].outgoing = Some(a);
        s.anchors[j].incoming = Some(c);
        let at = segment + 1;
        s.anchors.insert(
            at,
            Anchor {
                position: p,
                incoming: Some(d),
                outgoing: Some(e),
            },
        );
        Ok(at)
    }
    pub fn flatten(
        &self,
        tolerance: f32,
        mut cancel: impl FnMut() -> bool,
    ) -> Result<Vec<FlatSubpath>> {
        self.validate()?;
        ensure!(
            tolerance.is_finite() && (0.01..=1024.).contains(&tolerance),
            "invalid flatten tolerance"
        );
        let mut out = Vec::new();
        let mut total: usize = 0;
        for s in &self.subpaths {
            if cancel() {
                anyhow::bail!("cancelled")
            };
            if s.anchors.is_empty() {
                out.push(FlatSubpath {
                    points: vec![],
                    closed: s.closed,
                });
                continue;
            }
            let mut pts = vec![s.anchors[0].position];
            let segments = if s.closed {
                s.anchors.len()
            } else {
                s.anchors.len().saturating_sub(1)
            };
            for i in 0..segments {
                let before = pts.len();
                let j = (i + 1) % s.anchors.len();
                flatten_cubic(
                    s.anchors[i].position,
                    s.anchors[i].outgoing.unwrap_or(s.anchors[i].position),
                    s.anchors[j].incoming.unwrap_or(s.anchors[j].position),
                    s.anchors[j].position,
                    tolerance,
                    0,
                    &mut pts,
                    &mut cancel,
                )?;
                total = total.saturating_add(pts.len() - before);
                ensure!(total <= MAX_FLAT_POINTS, "flattened path is too large");
            }
            out.push(FlatSubpath {
                points: pts,
                closed: s.closed,
            });
        }
        Ok(out)
    }
    pub fn hit_test(&self, p: Point, tolerance: f32) -> Result<Option<Hit>> {
        let flat = self.flatten((tolerance / 4.).max(0.1), || false)?;
        let mut best = (tolerance, None);
        for (si, s) in self.subpaths.iter().enumerate() {
            for (ai, a) in s.anchors.iter().enumerate() {
                let d = p.distance(a.position);
                if d <= best.0 {
                    best = (
                        d,
                        Some(Hit::Anchor {
                            subpath: si,
                            anchor: ai,
                        }),
                    )
                }
            }
            let f = &flat[si];
            for i in 0..f.points.len().saturating_sub(1) {
                let (d, t) = segment_distance(p, f.points[i], f.points[i + 1]);
                if d < best.0 {
                    best = (
                        d,
                        Some(Hit::Segment {
                            subpath: si,
                            segment: i,
                            t,
                        }),
                    )
                }
            }
        }
        Ok(best.1)
    }
}
fn flatten_cubic(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tol: f32,
    depth: u8,
    out: &mut Vec<Point>,
    cancel: &mut impl FnMut() -> bool,
) -> Result<()> {
    if cancel() {
        anyhow::bail!("cancelled")
    }
    ensure!(depth < 32, "curve subdivision exceeded limit");
    let d1 = point_line_distance(p1, p0, p3);
    let d2 = point_line_distance(p2, p0, p3);
    if d1.max(d2) <= tol {
        out.push(p3);
        return Ok(());
    }
    let a = p0.lerp(p1, 0.5);
    let b = p1.lerp(p2, 0.5);
    let c = p2.lerp(p3, 0.5);
    let d = a.lerp(b, 0.5);
    let e = b.lerp(c, 0.5);
    let m = d.lerp(e, 0.5);
    flatten_cubic(p0, a, d, m, tol, depth + 1, out, cancel)?;
    flatten_cubic(m, e, c, p3, tol, depth + 1, out, cancel)
}
fn point_line_distance(p: Point, a: Point, b: Point) -> f32 {
    let (_, t) = segment_distance(p, a, b);
    p.distance(a.lerp(b, t))
}
fn segment_distance(p: Point, a: Point, b: Point) -> (f32, f32) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let q = dx * dx + dy * dy;
    let t = if q <= 1e-12 {
        0.
    } else {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / q
    }
    .clamp(0., 1.);
    (p.distance(a.lerp(b, t)), t)
}

pub fn rasterize_mask(
    path: &VectorPath,
    width: u32,
    height: u32,
    tolerance: f32,
    mut cancel: impl FnMut() -> bool,
) -> Result<GrayImage> {
    ensure!(
        crate::model::valid_dimensions(width, height),
        "invalid raster dimensions"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_RASTER_PIXELS,
        "vector raster exceeds 16 megapixels"
    );
    let flat = path.flatten(tolerance, || cancel())?;
    let mut out = GrayImage::new(width, height);
    let Some((x0, y0, x1, y1)) = clipped_bounds(&flat, width, height, 0.) else {
        return Ok(out);
    };
    let segment_count: u64 = flat.iter().map(|s| s.points.len() as u64).sum();
    let area = u64::from(x1 - x0) * u64::from(y1 - y0);
    let scanlines = u64::from(y1 - y0) * 4;
    ensure!(
        area.saturating_mul(16)
            .saturating_add(scanlines.saturating_mul(segment_count))
            <= MAX_FILL_WORK,
        "vector fill raster workload is too large"
    );
    for y in y0..y1 {
        if cancel() {
            anyhow::bail!("cancelled")
        }
        let mut coverage = vec![0u8; (x1 - x0) as usize];
        for sy in 0..4 {
            let sample_y = y as f32 + (sy as f32 + 0.5) / 4.;
            let mut crossings = scanline_crossings(&flat, sample_y);
            crossings.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut crossing_index = 0;
            let mut winding = 0i32;
            for x in x0..x1 {
                for sx in 0..4 {
                    let sample_x = x as f32 + (sx as f32 + 0.5) / 4.;
                    while crossing_index < crossings.len()
                        && crossings[crossing_index].0 <= sample_x
                    {
                        match path.fill_rule {
                            FillRule::EvenOdd => winding ^= 1,
                            FillRule::NonZero => winding += i32::from(crossings[crossing_index].1),
                        }
                        crossing_index += 1;
                    }
                    if winding != 0 {
                        coverage[(x - x0) as usize] += 1;
                    }
                }
            }
        }
        for x in x0..x1 {
            out.put_pixel(
                x,
                y,
                Luma([(u16::from(coverage[(x - x0) as usize]) * 255 / 16) as u8]),
            )
        }
    }
    Ok(out)
}
pub fn rasterize_rgba(
    path: &VectorPath,
    width: u32,
    height: u32,
    fill: Option<[u8; 4]>,
    stroke: Option<StrokeStyle>,
    tolerance: f32,
    mut cancel: impl FnMut() -> bool,
) -> Result<RgbaImage> {
    if let Some(s) = stroke {
        ensure!(
            s.width.is_finite() && s.width > 0. && s.width <= 100_000.,
            "invalid stroke width"
        )
    }
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_RASTER_PIXELS,
        "vector raster exceeds 16 megapixels"
    );
    let flat = path.flatten(tolerance, || cancel())?;
    let mask = if fill.is_some() {
        Some(rasterize_mask(path, width, height, tolerance, || cancel())?)
    } else {
        None
    };
    let mut out = RgbaImage::new(width, height);
    let pad = stroke.map_or(0., |s| s.width * 0.5 + 1.);
    let Some((x0, y0, x1, y1)) = clipped_bounds(&flat, width, height, pad) else {
        return Ok(out);
    };
    let segment_count: u64 = flat.iter().map(|s| segments(s).count() as u64).sum();
    if stroke.is_some() {
        ensure!(
            u64::from(x1 - x0)
                .saturating_mul(u64::from(y1 - y0))
                .saturating_mul(16)
                .saturating_mul(segment_count)
                <= MAX_STROKE_TESTS,
            "vector stroke raster workload is too large"
        );
    }
    for y in y0..y1 {
        if cancel() {
            anyhow::bail!("cancelled")
        }
        for x in x0..x1 {
            let mut pixel = [0; 4];
            if let (Some(color), Some(m)) = (fill, mask.as_ref()) {
                pixel = coverage_color(color, m.get_pixel(x, y)[0])
            }
            if let Some(s) = stroke {
                let mut covered = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let p = Point {
                            x: x as f32 + (sx as f32 + 0.5) / 4.,
                            y: y as f32 + (sy as f32 + 0.5) / 4.,
                        };
                        covered += usize::from(flat.iter().any(|sub| {
                            segments(sub).any(|(a, b)| segment_distance(p, a, b).0 <= s.width * 0.5)
                        }));
                    }
                }
                pixel = over(pixel, coverage_color(s.color, (covered * 255 / 16) as u8));
            }
            out.put_pixel(x, y, Rgba(pixel))
        }
    }
    Ok(out)
}
fn clipped_bounds(
    paths: &[FlatSubpath],
    width: u32,
    height: u32,
    padding: f32,
) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for p in paths.iter().flat_map(|s| &s.points) {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    if !min_x.is_finite() {
        return None;
    }
    let x0 = (min_x - padding).floor().clamp(0., width as f32) as u32;
    let y0 = (min_y - padding).floor().clamp(0., height as f32) as u32;
    let x1 = (max_x + padding).ceil().clamp(0., width as f32) as u32;
    let y1 = (max_y + padding).ceil().clamp(0., height as f32) as u32;
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}
fn scanline_crossings(paths: &[FlatSubpath], y: f32) -> Vec<(f32, i8)> {
    let mut crossings = Vec::new();
    for sub in paths {
        if sub.points.len() < 3 {
            continue;
        }
        for index in 0..sub.points.len() {
            let a = sub.points[index];
            let b = sub.points[(index + 1) % sub.points.len()];
            if (a.y <= y && b.y > y) || (a.y > y && b.y <= y) {
                crossings.push((
                    a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y),
                    if b.y > a.y { 1 } else { -1 },
                ));
            }
        }
    }
    crossings
}
fn segments(s: &FlatSubpath) -> impl Iterator<Item = (Point, Point)> + '_ {
    let n = s.points.len();
    (0..if s.closed { n } else { n.saturating_sub(1) })
        .map(move |i| (s.points[i], s.points[(i + 1) % n]))
}
fn coverage_color(mut c: [u8; 4], m: u8) -> [u8; 4] {
    c[3] = ((u16::from(c[3]) * u16::from(m) + 127) / 255) as u8;
    c
}
fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = src[3] as f32 / 255.;
    let da = dst[3] as f32 / 255.;
    let a = sa + da * (1. - sa);
    if a <= 0. {
        return [0; 4];
    }
    let mut o = [0; 4];
    for c in 0..3 {
        o[c] = ((src[c] as f32 * sa + dst[c] as f32 * da * (1. - sa)) / a).round() as u8
    }
    o[3] = (a * 255.).round() as u8;
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(x: f32, y: f32) -> Anchor {
        Anchor {
            position: Point { x, y },
            incoming: None,
            outgoing: None,
        }
    }
    #[test]
    fn even_odd_hole_and_bounds() {
        let p = VectorPath {
            subpaths: vec![
                Subpath {
                    anchors: vec![a(1., 1.), a(9., 1.), a(9., 9.), a(1., 9.)],
                    closed: true,
                },
                Subpath {
                    anchors: vec![a(3., 3.), a(7., 3.), a(7., 7.), a(3., 7.)],
                    closed: true,
                },
            ],
            fill_rule: FillRule::EvenOdd,
        };
        assert_eq!(
            p.bounds(),
            Some((Point { x: 1., y: 1. }, Point { x: 9., y: 9. }))
        );
        let m = rasterize_mask(&p, 10, 10, 0.1, || false).unwrap();
        assert_eq!(m.get_pixel(5, 5)[0], 0);
        assert_eq!(m.get_pixel(2, 2)[0], 255)
    }
    #[test]
    fn cubic_split_preserves_curve() {
        fn cubic(points: [Point; 4], t: f32) -> Point {
            let a = points[0].lerp(points[1], t);
            let b = points[1].lerp(points[2], t);
            let c = points[2].lerp(points[3], t);
            a.lerp(b, t).lerp(b.lerp(c, t), t)
        }
        let mut p = VectorPath {
            subpaths: vec![Subpath {
                anchors: vec![
                    Anchor {
                        position: Point { x: 0., y: 0. },
                        incoming: None,
                        outgoing: Some(Point { x: 0., y: 10. }),
                    },
                    Anchor {
                        position: Point { x: 10., y: 0. },
                        incoming: Some(Point { x: 10., y: 10. }),
                        outgoing: None,
                    },
                ],
                closed: false,
            }],
            fill_rule: FillRule::NonZero,
        };
        let original = [
            p.subpaths[0].anchors[0].position,
            p.subpaths[0].anchors[0].outgoing.unwrap(),
            p.subpaths[0].anchors[1].incoming.unwrap(),
            p.subpaths[0].anchors[1].position,
        ];
        let before = p.flatten(0.05, || false).unwrap();
        let split = 0.37;
        assert_eq!(p.insert_on_segment(0, 0, split).unwrap(), 1);
        let after = p.flatten(0.05, || false).unwrap();
        assert_eq!(before[0].points.first(), after[0].points.first());
        assert_eq!(before[0].points.last(), after[0].points.last());
        let anchors = &p.subpaths[0].anchors;
        let left = [
            anchors[0].position,
            anchors[0].outgoing.unwrap(),
            anchors[1].incoming.unwrap(),
            anchors[1].position,
        ];
        let right = [
            anchors[1].position,
            anchors[1].outgoing.unwrap(),
            anchors[2].incoming.unwrap(),
            anchors[2].position,
        ];
        for step in 0..=100 {
            let t = step as f32 / 100.;
            let expected = cubic(original, t);
            let actual = if t <= split {
                cubic(left, t / split)
            } else {
                cubic(right, (t - split) / (1. - split))
            };
            assert!(
                (expected.x - actual.x).abs() < 1e-4,
                "{t}: {expected:?} {actual:?}"
            );
            assert!(
                (expected.y - actual.y).abs() < 1e-4,
                "{t}: {expected:?} {actual:?}"
            );
        }
    }
    #[test]
    fn cancellation_and_validation() {
        let p = VectorPath {
            subpaths: vec![Subpath {
                anchors: vec![a(0., 0.), a(1., 1.)],
                closed: false,
            }],
            fill_rule: FillRule::NonZero,
        };
        assert!(p.flatten(0.1, || true).is_err());
    }
}
