//! Deterministic, bounded bitmap-to-editable-vector tracing.
//!
//! Tracing deliberately works on a bounded RGBA8 copy. Fully transparent RGB
//! is ignored, while visible RGB is averaged in straight-alpha form with alpha
//! weighting. Flat regions are converted to compound even-odd paths, so holes
//! and disconnected islands survive without consuming one scene object each.

use crate::{
    vector_path::{Anchor, FillRule, Point, Subpath, VectorPath},
    vector_scene::{
        MAX_SCENE_ANCHORS, MAX_SCENE_SUBPATHS, VECTOR_SCENE_VERSION, VectorObject, VectorScene,
    },
};
use anyhow::{Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
};

const TRANSPARENT: u16 = u16::MAX;
const MAX_WORKING_PIXELS: u64 = 4_194_304;
const MAX_RAW_CONTOUR_POINTS: usize = 2_000_000;
const MAX_TRACE_WORK: u64 = 600_000_000;
const MAX_SIMPLIFICATION_WORK: u64 = 100_000_000;
const CANCEL_GRANULARITY: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TraceMode {
    Color,
    Grayscale,
    Monochrome,
}

/// User-facing tracing controls. `detail`, `smoothing`, and
/// `corner_preservation` are normalized to `0..=1`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TraceOptions {
    pub mode: TraceMode,
    /// Maximum palette entries for colour or grayscale tracing.
    pub colors: u8,
    /// Pixels darker than this value become monochrome foreground.
    pub monochrome_threshold: u8,
    /// Zero retains the rectilinear contour; one applies the strongest fit.
    pub detail: f32,
    /// Zero emits linear anchors; non-zero adds bounded cubic handles.
    pub smoothing: f32,
    /// Higher values keep more turns as handle-free corners.
    pub corner_preservation: f32,
    /// Connected regions smaller than this area are merged into a neighbour.
    pub speckle_area: u32,
    pub omit_white: bool,
    /// Omit the most frequent visible colour along the image border.
    pub omit_background: bool,
    /// Maximum working-image edge. A hard four-megapixel bound also applies.
    pub max_dimension: u32,
    /// Maximum anchors in the returned editable scene.
    pub max_points: usize,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            mode: TraceMode::Color,
            colors: 8,
            monochrome_threshold: 128,
            detail: 0.55,
            smoothing: 0.45,
            corner_preservation: 0.7,
            speckle_area: 4,
            omit_white: false,
            omit_background: false,
            max_dimension: 1_024,
            max_points: 50_000,
        }
    }
}

impl TraceOptions {
    /// Validate retained settings before a trace job is started.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=32).contains(&self.colors),
            "trace colors must be 1..32"
        );
        ensure!(
            self.detail.is_finite() && (0.0..=1.0).contains(&self.detail),
            "trace detail must be 0..1"
        );
        ensure!(
            self.smoothing.is_finite() && (0.0..=1.0).contains(&self.smoothing),
            "trace smoothing must be 0..1"
        );
        ensure!(
            self.corner_preservation.is_finite() && (0.0..=1.0).contains(&self.corner_preservation),
            "trace corner preservation must be 0..1"
        );
        ensure!(
            self.speckle_area <= 1_000_000,
            "trace speckle area exceeds one million pixels"
        );
        ensure!(
            (16..=4_096).contains(&self.max_dimension),
            "trace processing edge must be 16..4096 pixels"
        );
        ensure!(
            (4..=MAX_SCENE_ANCHORS).contains(&self.max_points),
            "trace point budget must be 4..{MAX_SCENE_ANCHORS}"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceStats {
    pub objects: usize,
    pub subpaths: usize,
    pub anchors: usize,
    pub working_width: u32,
    pub working_height: u32,
    pub points_before: usize,
    pub points_after: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceResult {
    pub scene: VectorScene,
    pub stats: TraceStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct ColorAccum {
    count: u64,
    alpha: u64,
    red_alpha: u64,
    green_alpha: u64,
    blue_alpha: u64,
}

impl ColorAccum {
    fn add(&mut self, color: [u8; 4], count: u64) {
        let alpha = u64::from(color[3]).saturating_mul(count);
        self.count = self.count.saturating_add(count);
        self.alpha = self.alpha.saturating_add(alpha);
        self.red_alpha = self
            .red_alpha
            .saturating_add(u64::from(color[0]).saturating_mul(alpha));
        self.green_alpha = self
            .green_alpha
            .saturating_add(u64::from(color[1]).saturating_mul(alpha));
        self.blue_alpha = self
            .blue_alpha
            .saturating_add(u64::from(color[2]).saturating_mul(alpha));
    }

    fn color(self) -> [u8; 4] {
        if self.count == 0 || self.alpha == 0 {
            return [0; 4];
        }
        [
            ((self.red_alpha + self.alpha / 2) / self.alpha).min(255) as u8,
            ((self.green_alpha + self.alpha / 2) / self.alpha).min(255) as u8,
            ((self.blue_alpha + self.alpha / 2) / self.alpha).min(255) as u8,
            ((self.alpha + self.count / 2) / self.count).min(255) as u8,
        ]
    }
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    color: [u8; 4],
    count: u64,
    key: u32,
}

#[derive(Debug)]
struct SmallComponent {
    pixels: Vec<usize>,
    replacement: u16,
}

/// Trace a bitmap into an editable, geometry-authoritative vector scene.
pub fn trace(
    source: &RgbaImage,
    options: &TraceOptions,
    cancel: &AtomicBool,
) -> Result<TraceResult> {
    validate_options(source, options)?;
    check_cancel(cancel)?;
    let (width, height, pixels) = working_pixels(source, options.max_dimension, cancel)?;
    let pixel_count = u64::from(width) * u64::from(height);
    let palette_work = match options.mode {
        TraceMode::Monochrome => 1,
        _ => u64::from(options.colors).saturating_mul(8),
    };
    ensure!(
        pixel_count.saturating_mul(palette_work.saturating_add(36)) <= MAX_TRACE_WORK,
        "trace settings exceed the bounded work budget"
    );

    let (palette, mut labels) = quantize(&pixels, options, cancel)?;
    ensure!(
        !palette.is_empty(),
        "the image has no visible pixels to trace"
    );
    suppress_speckles(&mut labels, width, height, options.speckle_area, cancel)?;
    let background = border_background(&labels, width, height, palette.len());
    let mut skipped = vec![false; palette.len()];
    if options.omit_white {
        for (index, color) in palette.iter().enumerate() {
            skipped[index] = color[3] > 0 && color[..3].iter().all(|channel| *channel >= 248);
        }
    }
    if options.omit_background
        && let Some(index) = background
    {
        skipped[index] = true;
    }

    let mut objects = Vec::new();
    let mut points_before = 0usize;

    let traced = trace_regions(
        &labels,
        width,
        height,
        palette.len(),
        &skipped,
        options,
        cancel,
    )?;
    for (palette_index, mut path, raw_points) in traced {
        points_before = points_before
            .checked_add(raw_points)
            .ok_or_else(|| anyhow::anyhow!("trace point count overflow"))?;
        if path.subpaths.is_empty() {
            continue;
        }
        fit_path(&mut path, options, cancel)?;
        let name = match options.mode {
            TraceMode::Color => format!("Colour {}", palette_index + 1),
            TraceMode::Grayscale => format!("Gray {}", palette_index + 1),
            TraceMode::Monochrome => "Monochrome".into(),
        };
        objects.push(vector_object(
            objects.len(),
            name,
            path,
            palette[palette_index],
        ));
    }
    ensure!(
        !objects.is_empty(),
        "trace settings omitted every visible region"
    );

    let subpaths = objects
        .iter()
        .map(|object| object.path.subpaths.len())
        .sum::<usize>();
    let anchors = objects
        .iter()
        .flat_map(|object| &object.path.subpaths)
        .map(|subpath| subpath.anchors.len())
        .sum::<usize>();
    ensure!(
        subpaths <= MAX_SCENE_SUBPATHS,
        "trace produced too many editable subpaths"
    );
    ensure!(
        anchors <= options.max_points && anchors <= MAX_SCENE_ANCHORS,
        "trace produced {anchors} anchors, exceeding the {} point budget",
        options.max_points.min(MAX_SCENE_ANCHORS)
    );
    check_cancel(cancel)?;
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width,
        height,
        objects,
    };
    scene.validate()?;
    Ok(TraceResult {
        stats: TraceStats {
            objects: scene.objects.len(),
            subpaths,
            anchors,
            working_width: width,
            working_height: height,
            points_before,
            points_after: anchors,
        },
        scene,
    })
}

fn validate_options(source: &RgbaImage, options: &TraceOptions) -> Result<()> {
    ensure!(
        source.width() > 0 && source.height() > 0,
        "trace source dimensions must be positive"
    );
    options.validate()
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "image trace cancelled");
    Ok(())
}

fn periodic_cancel(cancel: &AtomicBool, step: usize) -> Result<()> {
    if step % CANCEL_GRANULARITY == 0 {
        check_cancel(cancel)?;
    }
    Ok(())
}

fn working_pixels(
    source: &RgbaImage,
    max_dimension: u32,
    cancel: &AtomicBool,
) -> Result<(u32, u32, Vec<[u8; 4]>)> {
    let source_width = source.width();
    let source_height = source.height();
    let edge_scale = f64::from(max_dimension) / f64::from(source_width.max(source_height));
    let pixel_scale =
        (MAX_WORKING_PIXELS as f64 / (f64::from(source_width) * f64::from(source_height))).sqrt();
    let scale = edge_scale.min(pixel_scale).min(1.0);
    let width = (f64::from(source_width) * scale).round().max(1.0) as u32;
    let height = (f64::from(source_height) * scale).round().max(1.0) as u32;
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_WORKING_PIXELS,
        "trace working image exceeds four megapixels"
    );
    if width == source_width && height == source_height {
        let mut pixels = Vec::with_capacity((u64::from(width) * u64::from(height)) as usize);
        for (index, pixel) in source.pixels().enumerate() {
            periodic_cancel(cancel, index)?;
            pixels.push(pixel.0);
        }
        return Ok((width, height, pixels));
    }

    let mut pixels = Vec::with_capacity((u64::from(width) * u64::from(height)) as usize);
    let sx = f64::from(source_width) / f64::from(width);
    let sy = f64::from(source_height) / f64::from(height);
    for y in 0..height {
        check_cancel(cancel)?;
        let source_y = (f64::from(y) + 0.5) * sy - 0.5;
        let y0 = source_y.floor().clamp(0.0, f64::from(source_height - 1)) as u32;
        let y1 = (y0 + 1).min(source_height - 1);
        let fy = (source_y - f64::from(y0)).clamp(0.0, 1.0);
        for x in 0..width {
            let source_x = (f64::from(x) + 0.5) * sx - 0.5;
            let x0 = source_x.floor().clamp(0.0, f64::from(source_width - 1)) as u32;
            let x1 = (x0 + 1).min(source_width - 1);
            let fx = (source_x - f64::from(x0)).clamp(0.0, 1.0);
            let samples = [
                (source.get_pixel(x0, y0).0, (1.0 - fx) * (1.0 - fy)),
                (source.get_pixel(x1, y0).0, fx * (1.0 - fy)),
                (source.get_pixel(x0, y1).0, (1.0 - fx) * fy),
                (source.get_pixel(x1, y1).0, fx * fy),
            ];
            let mut alpha = 0.0;
            let mut premultiplied = [0.0; 3];
            for (sample, weight) in samples {
                let a = f64::from(sample[3]) * weight;
                alpha += a;
                for channel in 0..3 {
                    premultiplied[channel] += f64::from(sample[channel]) * a;
                }
            }
            let mut pixel = [0; 4];
            pixel[3] = alpha.round().clamp(0.0, 255.0) as u8;
            if alpha > 0.0 {
                for channel in 0..3 {
                    pixel[channel] =
                        (premultiplied[channel] / alpha).round().clamp(0.0, 255.0) as u8;
                }
            }
            pixels.push(pixel);
        }
    }
    Ok((width, height, pixels))
}

fn luminance(color: [u8; 4]) -> u8 {
    ((54 * u32::from(color[0]) + 183 * u32::from(color[1]) + 19 * u32::from(color[2]) + 128) / 256)
        as u8
}

fn trace_color(color: [u8; 4], mode: TraceMode) -> [u8; 4] {
    match mode {
        TraceMode::Color => color,
        TraceMode::Grayscale => {
            let value = luminance(color);
            [value, value, value, color[3]]
        }
        TraceMode::Monochrome => [0, 0, 0, color[3]],
    }
}

fn color_key(color: [u8; 4], mode: TraceMode) -> u32 {
    match mode {
        TraceMode::Color => {
            (u32::from(color[0] >> 3) << 14)
                | (u32::from(color[1] >> 3) << 9)
                | (u32::from(color[2] >> 3) << 4)
                | u32::from(color[3] >> 4)
        }
        TraceMode::Grayscale => (u32::from(color[0] >> 2) << 4) | u32::from(color[3] >> 4),
        TraceMode::Monochrome => 0,
    }
}

fn color_distance(a: [u8; 4], b: [u8; 4]) -> u64 {
    let premultiply = |channel: u8, alpha: u8| u64::from(channel) * u64::from(alpha) / 255;
    let channels = [
        premultiply(a[0], a[3]).abs_diff(premultiply(b[0], b[3])),
        premultiply(a[1], a[3]).abs_diff(premultiply(b[1], b[3])),
        premultiply(a[2], a[3]).abs_diff(premultiply(b[2], b[3])),
        u64::from(a[3].abs_diff(b[3])) * 2,
    ];
    channels.iter().map(|value| value * value).sum()
}

fn quantize(
    pixels: &[[u8; 4]],
    options: &TraceOptions,
    cancel: &AtomicBool,
) -> Result<(Vec<[u8; 4]>, Vec<u16>)> {
    if options.mode == TraceMode::Monochrome {
        let mut labels = vec![TRANSPARENT; pixels.len()];
        let mut foreground = ColorAccum::default();
        for (index, color) in pixels.iter().copied().enumerate() {
            periodic_cancel(cancel, index)?;
            if color[3] > 0 && luminance(color) < options.monochrome_threshold {
                labels[index] = 0;
                foreground.add([0, 0, 0, color[3]], 1);
            }
        }
        return if foreground.count == 0 {
            Ok((Vec::new(), labels))
        } else {
            Ok((vec![foreground.color()], labels))
        };
    }

    let mut bins: BTreeMap<u32, ColorAccum> = BTreeMap::new();
    for (index, original) in pixels.iter().copied().enumerate() {
        periodic_cancel(cancel, index)?;
        if original[3] == 0 {
            continue;
        }
        let color = trace_color(original, options.mode);
        bins.entry(color_key(color, options.mode))
            .or_default()
            .add(color, 1);
    }
    let samples = bins
        .into_iter()
        .map(|(key, accum)| Sample {
            color: accum.color(),
            count: accum.count,
            key,
        })
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Ok((Vec::new(), vec![TRANSPARENT; pixels.len()]));
    }
    let count = usize::from(options.colors).min(samples.len());
    let mut ranked = samples.clone();
    ranked.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    let mut palette = Vec::with_capacity(count);
    palette.push(ranked[0].color);
    while palette.len() < count {
        let Some(next) = samples
            .iter()
            .filter(|sample| !palette.contains(&sample.color))
            .max_by(|a, b| {
                let score = |sample: &Sample| {
                    let distance = palette
                        .iter()
                        .map(|color| color_distance(sample.color, *color))
                        .min()
                        .unwrap_or(0);
                    u128::from(distance) * u128::from(sample.count.isqrt().max(1))
                };
                score(a).cmp(&score(b)).then_with(|| b.key.cmp(&a.key))
            })
        else {
            break;
        };
        palette.push(next.color);
    }
    for iteration in 0..8 {
        check_cancel(cancel)?;
        let mut accumulators = vec![ColorAccum::default(); palette.len()];
        for (index, sample) in samples.iter().enumerate() {
            periodic_cancel(cancel, iteration * samples.len() + index)?;
            let nearest = nearest_color(sample.color, &palette);
            accumulators[nearest].add(sample.color, sample.count);
        }
        let mut changed = false;
        for (center, accumulator) in palette.iter_mut().zip(accumulators) {
            if accumulator.count > 0 {
                let color = accumulator.color();
                changed |= color != *center;
                *center = color;
            }
        }
        if !changed {
            break;
        }
    }
    let mut deduplicated = Vec::with_capacity(palette.len());
    for color in palette {
        if !deduplicated.contains(&color) {
            deduplicated.push(color);
        }
    }
    let mut labels = Vec::with_capacity(pixels.len());
    for (index, original) in pixels.iter().copied().enumerate() {
        periodic_cancel(cancel, index)?;
        if original[3] == 0 {
            labels.push(TRANSPARENT);
        } else {
            labels.push(nearest_color(trace_color(original, options.mode), &deduplicated) as u16);
        }
    }
    Ok((deduplicated, labels))
}

fn nearest_color(color: [u8; 4], palette: &[[u8; 4]]) -> usize {
    let mut nearest = 0;
    let mut distance = u64::MAX;
    for (index, candidate) in palette.iter().copied().enumerate() {
        let candidate_distance = color_distance(color, candidate);
        if candidate_distance < distance {
            nearest = index;
            distance = candidate_distance;
        }
    }
    nearest
}

fn neighbours(index: usize, width: usize, height: usize) -> [Option<usize>; 4] {
    let x = index % width;
    let y = index / width;
    [
        (x > 0).then_some(index - 1),
        (x + 1 < width).then_some(index + 1),
        (y > 0).then_some(index - width),
        (y + 1 < height).then_some(index + width),
    ]
}

fn suppress_speckles(
    labels: &mut [u16],
    width: u32,
    height: u32,
    minimum_area: u32,
    cancel: &AtomicBool,
) -> Result<()> {
    if minimum_area <= 1 {
        return Ok(());
    }
    let width = width as usize;
    let height = height as usize;
    let mut visited = vec![false; labels.len()];
    let mut replacements = Vec::new();
    let mut processed = 0usize;
    for start in 0..labels.len() {
        periodic_cancel(cancel, start)?;
        let label = labels[start];
        if visited[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut pixels = Vec::new();
        let mut adjacent: BTreeMap<u16, usize> = BTreeMap::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            processed = processed.saturating_add(1);
            periodic_cancel(cancel, processed)?;
            pixels.push(index);
            for neighbour in neighbours(index, width, height) {
                match neighbour {
                    Some(neighbour) if labels[neighbour] == label => {
                        if !visited[neighbour] {
                            visited[neighbour] = true;
                            queue.push_back(neighbour);
                        }
                    }
                    Some(neighbour) => {
                        *adjacent.entry(labels[neighbour]).or_default() += 1;
                    }
                    None => *adjacent.entry(TRANSPARENT).or_default() += 1,
                }
            }
        }
        if pixels.len() < minimum_area as usize {
            let mut replacement = None;
            for (candidate, count) in adjacent {
                if candidate == label {
                    continue;
                }
                if replacement.is_none_or(|(best, best_count)| {
                    count > best_count || (count == best_count && candidate < best)
                }) {
                    replacement = Some((candidate, count));
                }
            }
            if let Some((replacement, _)) = replacement {
                replacements.push(SmallComponent {
                    pixels,
                    replacement,
                });
            }
        }
    }
    for replacement in replacements {
        check_cancel(cancel)?;
        for index in replacement.pixels {
            labels[index] = replacement.replacement;
        }
    }
    Ok(())
}

fn border_background(labels: &[u16], width: u32, height: u32, palette_len: usize) -> Option<usize> {
    let width = width as usize;
    let height = height as usize;
    let mut counts = vec![0usize; palette_len];
    let mut border_samples = 0usize;
    let mut count = |label: u16| {
        border_samples += 1;
        if label != TRANSPARENT {
            counts[label as usize] += 1;
        }
    };
    for x in 0..width {
        count(labels[x]);
        if height > 1 {
            count(labels[(height - 1) * width + x]);
        }
    }
    for y in 1..height.saturating_sub(1) {
        count(labels[y * width]);
        if width > 1 {
            count(labels[y * width + width - 1]);
        }
    }
    let candidate = counts
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count > 0)
        .max_by(|(a_index, a_count), (b_index, b_count)| {
            a_count.cmp(b_count).then_with(|| b_index.cmp(a_index))
        })
        .map(|(index, count)| (index, count));
    candidate
        .filter(|(_, count)| count.saturating_mul(2) >= border_samples)
        .map(|(index, _)| index)
}

fn encode_vertex(x: u32, y: u32, stride: u32) -> u64 {
    u64::from(y) * u64::from(stride) + u64::from(x)
}

fn decode_vertex(vertex: u64, stride: u32) -> (u32, u32) {
    (
        (vertex % u64::from(stride)) as u32,
        (vertex / u64::from(stride)) as u32,
    )
}

fn add_edge(edges: &mut BTreeMap<u64, Vec<u64>>, from: u64, to: u64) {
    edges.entry(from).or_default().push(to);
}

fn trace_regions(
    labels: &[u16],
    width: u32,
    height: u32,
    palette_len: usize,
    skipped: &[bool],
    options: &TraceOptions,
    cancel: &AtomicBool,
) -> Result<Vec<(usize, VectorPath, usize)>> {
    let usize_width = width as usize;
    let usize_height = height as usize;
    let stride = width + 1;
    let mut visited = vec![false; labels.len()];
    let mut paths = (0..palette_len)
        .map(|_| VectorPath {
            subpaths: Vec::new(),
            fill_rule: FillRule::EvenOdd,
        })
        .collect::<Vec<_>>();
    let mut raw_counts = vec![0usize; palette_len];
    let mut raw_total = 0usize;
    let mut output_total = 0usize;
    let mut subpath_total = 0usize;
    let mut processed = 0usize;
    let mut simplification_work = 0u64;

    for start in 0..labels.len() {
        periodic_cancel(cancel, start)?;
        let label = labels[start];
        if label == TRANSPARENT || visited[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut component = Vec::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            processed = processed.saturating_add(1);
            periodic_cancel(cancel, processed)?;
            component.push(index);
            for neighbour in neighbours(index, usize_width, usize_height)
                .into_iter()
                .flatten()
            {
                if !visited[neighbour] && labels[neighbour] == label {
                    visited[neighbour] = true;
                    queue.push_back(neighbour);
                }
            }
        }
        if skipped[label as usize] {
            continue;
        }
        let mut edges: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        let mut component_edges = 0usize;
        for (step, index) in component.into_iter().enumerate() {
            periodic_cancel(cancel, step)?;
            let x = (index % usize_width) as u32;
            let y = (index / usize_width) as u32;
            let differs = |x: i64, y: i64| {
                x < 0
                    || y < 0
                    || x >= i64::from(width)
                    || y >= i64::from(height)
                    || labels[y as usize * usize_width + x as usize] != label
            };
            if differs(i64::from(x), i64::from(y) - 1) {
                component_edges = component_edges.saturating_add(1);
                ensure!(
                    raw_total.saturating_add(component_edges) <= MAX_RAW_CONTOUR_POINTS,
                    "trace contour exceeds the two-million-point raw geometry budget"
                );
                add_edge(
                    &mut edges,
                    encode_vertex(x, y, stride),
                    encode_vertex(x + 1, y, stride),
                );
            }
            if differs(i64::from(x) + 1, i64::from(y)) {
                component_edges = component_edges.saturating_add(1);
                ensure!(
                    raw_total.saturating_add(component_edges) <= MAX_RAW_CONTOUR_POINTS,
                    "trace contour exceeds the two-million-point raw geometry budget"
                );
                add_edge(
                    &mut edges,
                    encode_vertex(x + 1, y, stride),
                    encode_vertex(x + 1, y + 1, stride),
                );
            }
            if differs(i64::from(x), i64::from(y) + 1) {
                component_edges = component_edges.saturating_add(1);
                ensure!(
                    raw_total.saturating_add(component_edges) <= MAX_RAW_CONTOUR_POINTS,
                    "trace contour exceeds the two-million-point raw geometry budget"
                );
                add_edge(
                    &mut edges,
                    encode_vertex(x + 1, y + 1, stride),
                    encode_vertex(x, y + 1, stride),
                );
            }
            if differs(i64::from(x) - 1, i64::from(y)) {
                component_edges = component_edges.saturating_add(1);
                ensure!(
                    raw_total.saturating_add(component_edges) <= MAX_RAW_CONTOUR_POINTS,
                    "trace contour exceeds the two-million-point raw geometry budget"
                );
                add_edge(
                    &mut edges,
                    encode_vertex(x, y + 1, stride),
                    encode_vertex(x, y, stride),
                );
            }
        }
        let edge_count = edges.values().map(Vec::len).sum::<usize>();
        raw_total = raw_total
            .checked_add(edge_count)
            .ok_or_else(|| anyhow::anyhow!("trace contour size overflow"))?;
        ensure!(
            raw_total <= MAX_RAW_CONTOUR_POINTS,
            "trace contour exceeds the two-million-point raw geometry budget"
        );
        raw_counts[label as usize] = raw_counts[label as usize].saturating_add(edge_count);
        let loops = edge_loops(edges, stride, cancel)?;
        for points in loops {
            let points =
                simplify_closed(&points, options.detail, cancel, &mut simplification_work)?;
            if points.len() >= 3 {
                output_total = output_total
                    .checked_add(points.len())
                    .ok_or_else(|| anyhow::anyhow!("trace point count overflow"))?;
                ensure!(
                    output_total <= options.max_points.min(MAX_SCENE_ANCHORS),
                    "trace produced more anchors than the {} point budget",
                    options.max_points.min(MAX_SCENE_ANCHORS)
                );
                subpath_total = subpath_total.saturating_add(1);
                ensure!(
                    subpath_total <= MAX_SCENE_SUBPATHS,
                    "trace produced too many editable subpaths"
                );
                paths[label as usize].subpaths.push(Subpath {
                    anchors: points
                        .into_iter()
                        .map(|position| Anchor {
                            position,
                            incoming: None,
                            outgoing: None,
                        })
                        .collect(),
                    closed: true,
                });
            }
        }
    }
    Ok(paths
        .into_iter()
        .enumerate()
        .filter(|(_, path)| !path.subpaths.is_empty())
        .map(|(index, path)| (index, path, raw_counts[index]))
        .collect())
}

fn edge_direction(from: u64, to: u64, stride: u32) -> u8 {
    let (fx, fy) = decode_vertex(from, stride);
    let (tx, ty) = decode_vertex(to, stride);
    if tx > fx {
        0
    } else if ty > fy {
        1
    } else if tx < fx {
        2
    } else {
        3
    }
}

fn take_edge(
    edges: &mut BTreeMap<u64, Vec<u64>>,
    from: u64,
    incoming: Option<u8>,
    stride: u32,
) -> Option<u64> {
    let choices = edges.get_mut(&from)?;
    let selected = if let Some(incoming) = incoming {
        choices
            .iter()
            .enumerate()
            .min_by_key(|(_, to)| {
                let direction = edge_direction(from, **to, stride);
                match (direction + 4 - incoming) % 4 {
                    1 => 0,
                    0 => 1,
                    3 => 2,
                    _ => 3,
                }
            })
            .map(|(index, _)| index)?
    } else {
        choices
            .iter()
            .enumerate()
            .min_by_key(|(_, to)| **to)
            .map(|(index, _)| index)?
    };
    let to = choices.remove(selected);
    if choices.is_empty() {
        edges.remove(&from);
    }
    Some(to)
}

fn edge_loops(
    mut edges: BTreeMap<u64, Vec<u64>>,
    stride: u32,
    cancel: &AtomicBool,
) -> Result<Vec<Vec<Point>>> {
    let edge_count = edges.values().map(Vec::len).sum::<usize>();
    let mut consumed = 0usize;
    let mut loops = Vec::new();
    while let Some((&start, _)) = edges.first_key_value() {
        check_cancel(cancel)?;
        let mut current = start;
        let mut incoming = None;
        let mut points = Vec::new();
        loop {
            periodic_cancel(cancel, consumed)?;
            let (x, y) = decode_vertex(current, stride);
            points.push(Point {
                x: x as f32,
                y: y as f32,
            });
            let next = take_edge(&mut edges, current, incoming, stride)
                .ok_or_else(|| anyhow::anyhow!("trace contour is not closed"))?;
            incoming = Some(edge_direction(current, next, stride));
            current = next;
            consumed += 1;
            ensure!(
                consumed <= edge_count,
                "trace contour walk exceeded its edge budget"
            );
            if current == start {
                break;
            }
        }
        loops.push(points);
    }
    ensure!(
        consumed == edge_count,
        "trace did not consume every contour edge"
    );
    Ok(loops)
}

fn cross(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
}

fn remove_collinear(points: &[Point], cancel: &AtomicBool) -> Result<Vec<Point>> {
    if points.len() <= 3 {
        return Ok(points.to_vec());
    }
    let mut result = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        periodic_cancel(cancel, index)?;
        let previous = points[(index + points.len() - 1) % points.len()];
        let point = points[index];
        let next = points[(index + 1) % points.len()];
        if cross(previous, point, next).abs() > 1e-6 {
            result.push(point);
        }
    }
    if result.len() < 3 {
        Ok(points.to_vec())
    } else {
        Ok(result)
    }
}

fn point_segment_distance(point: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return (point.x - a.x).hypot(point.y - a.y);
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    (point.x - (a.x + dx * t)).hypot(point.y - (a.y + dy * t))
}

fn rdp_open(
    points: &[Point],
    tolerance: f32,
    cancel: &AtomicBool,
    work: &mut u64,
) -> Result<Vec<Point>> {
    if points.len() <= 2 {
        return Ok(points.to_vec());
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut ranges = vec![(0usize, points.len() - 1)];
    while let Some((first, last)) = ranges.pop() {
        if last <= first + 1 {
            continue;
        }
        let mut farthest = first;
        let mut distance = tolerance;
        for index in first + 1..last {
            *work = work.saturating_add(1);
            ensure!(
                *work <= MAX_SIMPLIFICATION_WORK,
                "trace simplification exceeds its bounded work budget"
            );
            periodic_cancel(cancel, *work as usize)?;
            let candidate = point_segment_distance(points[index], points[first], points[last]);
            if candidate > distance {
                farthest = index;
                distance = candidate;
            }
        }
        if farthest != first {
            keep[farthest] = true;
            ranges.push((first, farthest));
            ranges.push((farthest, last));
        }
    }
    Ok(points
        .iter()
        .copied()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(point))
        .collect())
}

fn simplify_closed(
    points: &[Point],
    detail: f32,
    cancel: &AtomicBool,
    work: &mut u64,
) -> Result<Vec<Point>> {
    let points = remove_collinear(points, cancel)?;
    if points.len() <= 4 || detail <= 0.0 {
        return Ok(points);
    }
    let first = 0usize;
    let opposite = (1..points.len())
        .max_by(|a, b| {
            let distance = |index: usize| {
                let dx = points[index].x - points[first].x;
                let dy = points[index].y - points[first].y;
                dx * dx + dy * dy
            };
            distance(*a).total_cmp(&distance(*b))
        })
        .unwrap_or(points.len() / 2);
    let tolerance = 0.25 + detail * detail * 4.0;
    let first_half = rdp_open(&points[..=opposite], tolerance, cancel, work)?;
    let mut second_source = points[opposite..].to_vec();
    second_source.push(points[0]);
    let second_half = rdp_open(&second_source, tolerance, cancel, work)?;
    let mut result = first_half;
    result.pop();
    result.extend(second_half);
    if result.last() == result.first() {
        result.pop();
    }
    Ok(if result.len() < 3 { points } else { result })
}

fn fit_path(path: &mut VectorPath, options: &TraceOptions, cancel: &AtomicBool) -> Result<()> {
    if options.smoothing <= 0.0 {
        return Ok(());
    }
    let corner_threshold = (15.0 + (1.0 - options.corner_preservation) * 120.0).to_radians();
    let mut processed = 0usize;
    for subpath in &mut path.subpaths {
        check_cancel(cancel)?;
        let positions = subpath
            .anchors
            .iter()
            .map(|anchor| anchor.position)
            .collect::<Vec<_>>();
        let count = positions.len();
        if count < 3 {
            continue;
        }
        for index in 0..count {
            processed = processed.saturating_add(1);
            periodic_cancel(cancel, processed)?;
            let previous = positions[(index + count - 1) % count];
            let point = positions[index];
            let next = positions[(index + 1) % count];
            let incoming = (point.x - previous.x, point.y - previous.y);
            let outgoing = (next.x - point.x, next.y - point.y);
            let incoming_length = incoming.0.hypot(incoming.1);
            let outgoing_length = outgoing.0.hypot(outgoing.1);
            if incoming_length <= f32::EPSILON || outgoing_length <= f32::EPSILON {
                continue;
            }
            let dot = ((incoming.0 * outgoing.0 + incoming.1 * outgoing.1)
                / (incoming_length * outgoing_length))
                .clamp(-1.0, 1.0);
            let turn = dot.acos();
            if turn >= corner_threshold {
                continue;
            }
            let tangent = (next.x - previous.x, next.y - previous.y);
            let tangent_length = tangent.0.hypot(tangent.1);
            if tangent_length <= f32::EPSILON {
                continue;
            }
            let unit = (tangent.0 / tangent_length, tangent.1 / tangent_length);
            let incoming_handle = incoming_length * options.smoothing / 3.0;
            let outgoing_handle = outgoing_length * options.smoothing / 3.0;
            subpath.anchors[index].incoming = Some(Point {
                x: point.x - unit.0 * incoming_handle,
                y: point.y - unit.1 * incoming_handle,
            });
            subpath.anchors[index].outgoing = Some(Point {
                x: point.x + unit.0 * outgoing_handle,
                y: point.y + unit.1 * outgoing_handle,
            });
        }
    }
    Ok(())
}

fn vector_object(index: usize, name: String, path: VectorPath, fill: [u8; 4]) -> VectorObject {
    const TRACE_NAMESPACE: u128 = 0x4f4d_5553_4554_5241_4345_0000_0000_0000;
    VectorObject {
        id: uuid::Uuid::from_u128(TRACE_NAMESPACE | index as u128).to_string(),
        name,
        path,
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        fill: Some(fill),
        stroke: None,
        opacity: 1.0,
        visible: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn options(mode: TraceMode) -> TraceOptions {
        TraceOptions {
            mode,
            colors: 8,
            max_dimension: 256,
            ..Default::default()
        }
    }

    fn render(result: &TraceResult) -> RgbaImage {
        result.scene.render(&AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn monochrome_preserves_a_hole_in_rendered_output() {
        let mut image = RgbaImage::new(48, 48);
        for y in 5..43 {
            for x in 5..43 {
                if !(16..32).contains(&x) || !(16..32).contains(&y) {
                    image.put_pixel(x, y, Rgba([20, 20, 20, 255]));
                }
            }
        }
        let result = trace(
            &image,
            &TraceOptions {
                mode: TraceMode::Monochrome,
                smoothing: 0.0,
                detail: 0.4,
                omit_background: true,
                ..options(TraceMode::Monochrome)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = render(&result);
        assert!(rendered.get_pixel(8, 8)[3] > 240);
        assert_eq!(rendered.get_pixel(24, 24)[3], 0);
        assert!(result.stats.subpaths >= 2);
    }

    #[test]
    fn color_trace_ignores_hidden_rgb_and_keeps_straight_alpha() {
        let mut image = RgbaImage::from_pixel(40, 24, Rgba([255, 0, 0, 0]));
        for y in 3..21 {
            for x in 3..18 {
                image.put_pixel(x, y, Rgba([20, 210, 40, 255]));
            }
            for x in 23..37 {
                image.put_pixel(x, y, Rgba([30, 60, 220, 128]));
            }
        }
        let result = trace(
            &image,
            &TraceOptions {
                omit_background: true,
                smoothing: 0.0,
                detail: 0.3,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(result.scene.objects.iter().all(|object| {
            object
                .fill
                .is_none_or(|fill| !(fill[0] > 200 && fill[1] < 40 && fill[2] < 40))
        }));
        let rendered = render(&result);
        assert_eq!(rendered.get_pixel(8, 10).0, [20, 210, 40, 255]);
        let blue = rendered.get_pixel(30, 10).0;
        assert_eq!(&blue[..3], &[30, 60, 220]);
        assert!((120..=136).contains(&blue[3]));
    }

    #[test]
    fn stronger_detail_reduces_points_and_smoothing_emits_real_cubics() {
        let mut image = RgbaImage::new(128, 128);
        let center = 63.5f32;
        for y in 0..128 {
            for x in 0..128 {
                let distance = (x as f32 - center).hypot(y as f32 - center);
                if distance < 46.0 {
                    image.put_pixel(x, y, Rgba([40, 120, 220, 255]));
                }
            }
        }
        let baseline = trace(
            &image,
            &TraceOptions {
                detail: 0.0,
                smoothing: 0.0,
                omit_background: true,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let fitted = trace(
            &image,
            &TraceOptions {
                detail: 0.85,
                smoothing: 0.6,
                corner_preservation: 0.5,
                omit_background: true,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let medium = trace(
            &image,
            &TraceOptions {
                detail: 0.45,
                smoothing: 0.0,
                omit_background: true,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(medium.stats.anchors <= baseline.stats.anchors);
        assert!(fitted.stats.anchors <= medium.stats.anchors);
        assert!(fitted.stats.anchors < baseline.stats.anchors);
        assert!(fitted.stats.points_before > fitted.stats.points_after);
        assert!(fitted.scene.objects.iter().any(|object| {
            object
                .path
                .subpaths
                .iter()
                .flat_map(|subpath| &subpath.anchors)
                .any(|anchor| anchor.incoming.is_some() || anchor.outgoing.is_some())
        }));

        let expected = image.pixels().filter(|pixel| pixel[3] > 0).count();
        let rendered = render(&fitted);
        let mut intersection = 0usize;
        let mut union = 0usize;
        for (source, traced) in image.pixels().zip(rendered.pixels()) {
            let source = source[3] > 0;
            let traced = traced[3] > 0;
            intersection += usize::from(source && traced);
            union += usize::from(source || traced);
        }
        assert!(expected > 0 && intersection as f32 / union as f32 > 0.88);
    }

    #[test]
    fn grayscale_suppresses_speckles_and_is_deterministic() {
        let mut image = RgbaImage::from_pixel(64, 48, Rgba([255, 255, 255, 255]));
        for y in 8..40 {
            for x in 10..54 {
                image.put_pixel(x, y, Rgba([70, 130, 190, 255]));
            }
        }
        image.put_pixel(3, 3, Rgba([0, 0, 0, 255]));
        let options = TraceOptions {
            mode: TraceMode::Grayscale,
            colors: 4,
            speckle_area: 3,
            omit_white: true,
            smoothing: 0.0,
            ..options(TraceMode::Grayscale)
        };
        let first = trace(&image, &options, &AtomicBool::new(false)).unwrap();
        let second = trace(&image, &options, &AtomicBool::new(false)).unwrap();
        assert_eq!(first, second);
        let rendered = render(&first);
        assert_eq!(rendered.get_pixel(3, 3)[3], 0);
        let gray = rendered.get_pixel(20, 20).0;
        assert_eq!(gray[0], gray[1]);
        assert_eq!(gray[1], gray[2]);
    }

    #[test]
    fn cancellation_and_output_budget_refuse_without_truncation() {
        let image = RgbaImage::from_pixel(32, 32, Rgba([0, 0, 0, 255]));
        let cancelled = AtomicBool::new(true);
        assert!(
            trace(&image, &options(TraceMode::Color), &cancelled)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );

        let mut checker = RgbaImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                checker.put_pixel(
                    x,
                    y,
                    if (x + y) % 2 == 0 {
                        Rgba([0, 0, 0, 255])
                    } else {
                        Rgba([255, 255, 255, 255])
                    },
                );
            }
        }
        let error = trace(
            &checker,
            &TraceOptions {
                colors: 2,
                detail: 0.0,
                smoothing: 0.0,
                speckle_area: 0,
                max_points: 16,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.to_string().contains("point budget"), "{error:#}");
    }

    #[test]
    fn processing_resolution_is_bounded_and_reported() {
        let image = RgbaImage::from_pixel(800, 400, Rgba([10, 40, 90, 255]));
        let result = trace(
            &image,
            &TraceOptions {
                max_dimension: 200,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            (result.stats.working_width, result.stats.working_height),
            (200, 100)
        );
        assert_eq!((result.scene.width, result.scene.height), (200, 100));
    }

    #[test]
    fn retained_options_are_strict_and_validated() {
        let options = TraceOptions::default();
        let json = serde_json::to_string(&options).unwrap();
        assert_eq!(
            serde_json::from_str::<TraceOptions>(&json).unwrap(),
            options
        );
        let with_unknown = json.replacen('{', r#"{"futureField":true,"#, 1);
        assert!(serde_json::from_str::<TraceOptions>(&with_unknown).is_err());

        let mut invalid = options;
        invalid.detail = f32::NAN;
        assert!(
            invalid
                .validate()
                .unwrap_err()
                .to_string()
                .contains("detail")
        );
    }

    #[test]
    fn border_touching_foreground_is_not_promoted_to_full_canvas_background() {
        let mut image = RgbaImage::new(40, 40);
        for y in 8..32 {
            for x in 0..12 {
                image.put_pixel(x, y, Rgba([220, 30, 50, 255]));
            }
        }
        let result = trace(
            &image,
            &TraceOptions {
                smoothing: 0.0,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = render(&result);
        assert_eq!(rendered.get_pixel(4, 20).0, [220, 30, 50, 255]);
        assert_eq!(rendered.get_pixel(30, 20)[3], 0);
    }

    #[test]
    fn majority_border_keeps_transparent_holes_and_semitransparent_regions_separate() {
        let background = [40, 90, 160, 128];
        let foreground = [220, 40, 30, 128];
        let mut image = RgbaImage::from_pixel(48, 48, Rgba(background));
        for y in 15..33 {
            for x in 15..33 {
                image.put_pixel(x, y, Rgba([0; 4]));
            }
        }
        for y in 4..12 {
            for x in 4..12 {
                image.put_pixel(x, y, Rgba(foreground));
            }
        }
        let result = trace(
            &image,
            &TraceOptions {
                colors: 3,
                detail: 0.0,
                smoothing: 0.0,
                speckle_area: 0,
                ..options(TraceMode::Color)
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = render(&result);
        assert_eq!(rendered.get_pixel(24, 24).0, [0; 4]);
        assert_eq!(rendered.get_pixel(40, 40).0, background);
        assert_eq!(rendered.get_pixel(8, 8).0, foreground);
    }
}
