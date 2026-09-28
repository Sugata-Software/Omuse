//! Native raster object insertion. Shapes and text become ordinary image layers,
//! so .comp files remain compatible with the preserved editor. Source parameters
//! are retained as provenance, not advertised as an editable Mac text/shape object.
use crate::model::{Layer, valid_dimensions};
use anyhow::{Context, Result, ensure};
use image::Rgba;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPoint {
    pub x: f32,
    pub y: f32,
}
impl Serialize for ObjectPoint {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        [self.x, self.y].serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ObjectPoint {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let [x, y] = <[f32; 2]>::deserialize(deserializer)?;
        Ok(Self { x, y })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectSize {
    pub width: f32,
    pub height: f32,
}
impl Serialize for ObjectSize {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        [self.width, self.height].serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ObjectSize {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let [width, height] = <[f32; 2]>::deserialize(deserializer)?;
        Ok(Self { width, height })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignment {
    #[default]
    #[serde(rename = "Left")]
    Left,
    #[serde(rename = "Center")]
    Center,
    #[serde(rename = "Right")]
    Right,
}

/// A non-overlapping UTF-8 byte range with optional overrides over the base
/// live-text style. Byte ranges make the persisted representation unambiguous;
/// validation requires both ends to fall on Unicode scalar boundaries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RichTextRun {
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 4]>,
}

impl RichTextRun {
    fn has_override(&self) -> bool {
        self.font_name.is_some()
            || self.font_size.is_some()
            || self.weight.is_some()
            || self.italic.is_some()
            || self.color.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTextStyle {
    #[serde(default = "default_text_content")]
    pub content: String,
    #[serde(default = "default_font_name")]
    pub font_name: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub red: f32,
    #[serde(default)]
    pub green: f32,
    #[serde(default)]
    pub blue: f32,
    #[serde(default)]
    pub alignment: TextAlignment,
    #[serde(default)]
    pub tracking: f32,
    #[serde(default)]
    pub leading: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_size: Option<ObjectSize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<RichTextRun>,
}
fn default_text_content() -> String {
    "Text".into()
}
fn default_font_name() -> String {
    "Helvetica".into()
}
fn default_font_size() -> f32 {
    72.
}
impl Default for LiveTextStyle {
    fn default() -> Self {
        Self {
            content: default_text_content(),
            font_name: default_font_name(),
            font_size: 72.,
            red: 0.,
            green: 0.,
            blue: 0.,
            alignment: TextAlignment::Left,
            tracking: 0.,
            leading: 0.,
            box_size: None,
            runs: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveShapeKind {
    #[serde(rename = "Rectangle")]
    Rectangle,
    #[serde(rename = "Ellipse")]
    Ellipse,
    #[serde(rename = "Line")]
    Line,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveShapeStyle {
    pub kind: LiveShapeKind,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub corner_radius: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ObjectPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ObjectPoint>,
}

fn component(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

pub fn validate_text_style(style: &LiveTextStyle) -> Result<()> {
    ensure!(
        style.content.encode_utf16().count() <= 100_000,
        "Text exceeds 100000 UTF-16 code units"
    );
    ensure!(
        !style.font_name.trim().is_empty() && style.font_name.len() <= 512,
        "Invalid font name"
    );
    ensure!(
        style.font_size.is_finite() && (1. ..=2000.).contains(&style.font_size),
        "Font size must be 1-2000 pixels"
    );
    ensure!(
        [style.red, style.green, style.blue]
            .into_iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(&v)),
        "Text color components must be between 0 and 1"
    );
    ensure!(
        style.tracking.is_finite() && (-100. ..=1000.).contains(&style.tracking),
        "Tracking must be -100-1000"
    );
    ensure!(
        style.leading.is_finite() && (0. ..=5000.).contains(&style.leading),
        "Leading must be 0-5000"
    );
    if let Some(size) = style.box_size {
        ensure!(
            size.width.is_finite()
                && size.height.is_finite()
                && (16. ..=30_000.).contains(&size.width)
                && (16. ..=30_000.).contains(&size.height)
                && f64::from(size.width) * f64::from(size.height) <= 100_000_000.,
            "Invalid text box size"
        );
    }
    ensure!(style.runs.len() <= 4_096, "Text has too many styled runs");
    let mut previous_end = 0usize;
    for run in &style.runs {
        ensure!(
            run.start < run.end
                && run.end <= style.content.len()
                && style.content.is_char_boundary(run.start)
                && style.content.is_char_boundary(run.end),
            "Rich text ranges must be non-empty UTF-8 byte ranges"
        );
        ensure!(
            run.start >= previous_end,
            "Rich text ranges must be sorted and non-overlapping"
        );
        ensure!(run.has_override(), "Rich text run has no style override");
        if let Some(font) = &run.font_name {
            ensure!(
                !font.trim().is_empty() && font.len() <= 512,
                "Invalid rich text font name"
            );
        }
        if let Some(size) = run.font_size {
            ensure!(
                size.is_finite() && (1. ..=2_000.).contains(&size),
                "Rich text font size must be 1-2000 pixels"
            );
        }
        if let Some(weight) = run.weight {
            ensure!(
                (1..=1_000).contains(&weight),
                "Rich text weight must be 1-1000"
            );
        }
        if let Some(color) = run.color {
            ensure!(
                color
                    .into_iter()
                    .all(|value| value.is_finite() && (0. ..=1.).contains(&value)),
                "Rich text color components must be between 0 and 1"
            );
        }
        previous_end = run.end;
    }
    Ok(())
}

/// Replace text content while retaining styled runs only when their byte ranges
/// still describe the same string. Arbitrary editing cannot safely infer how a
/// prior selection maps into new UTF-8, so changed content becomes base style.
pub fn set_text_content(style: &mut LiveTextStyle, content: impl Into<String>) {
    let content = content.into();
    if style.content != content {
        style.runs.clear();
        style.content = content;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichTextPatch {
    pub font_name: Option<String>,
    pub font_size: Option<f32>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub color: Option<[f32; 4]>,
}

impl RichTextPatch {
    fn is_empty(&self) -> bool {
        self.font_name.is_none()
            && self.font_size.is_none()
            && self.weight.is_none()
            && self.italic.is_none()
            && self.color.is_none()
    }

    fn apply_to(&self, run: &mut RichTextRun) {
        if let Some(value) = &self.font_name {
            run.font_name = Some(value.clone());
        }
        if let Some(value) = self.font_size {
            run.font_size = Some(value);
        }
        if let Some(value) = self.weight {
            run.weight = Some(value);
        }
        if let Some(value) = self.italic {
            run.italic = Some(value);
        }
        if let Some(value) = self.color {
            run.color = Some(value);
        }
    }
}

/// Convert character indices used by native controls into validated UTF-8 byte
/// offsets used by persistence.
pub fn character_range(
    content: &str,
    start_character: usize,
    end_character: usize,
) -> Result<std::ops::Range<usize>> {
    ensure!(
        start_character < end_character,
        "Text range must not be empty"
    );
    let character_count = content.chars().count();
    ensure!(
        end_character <= character_count,
        "Text range exceeds the content"
    );
    let byte_at = |index: usize| {
        if index == character_count {
            content.len()
        } else {
            content
                .char_indices()
                .nth(index)
                .map(|(byte, _)| byte)
                .unwrap()
        }
    };
    Ok(byte_at(start_character)..byte_at(end_character))
}

/// Select the Unicode alphanumeric word containing a character index. An
/// underscore belongs to a word; punctuation and whitespace select themselves.
pub fn word_range(content: &str, character: usize) -> Result<std::ops::Range<usize>> {
    let chars = content.char_indices().collect::<Vec<_>>();
    ensure!(character < chars.len(), "Word position exceeds the content");
    let word_character = |value: char| value.is_alphanumeric() || value == '_';
    let target_is_word = word_character(chars[character].1);
    let mut start = character;
    let mut end = character + 1;
    if target_is_word {
        while start > 0 && word_character(chars[start - 1].1) {
            start -= 1;
        }
        while end < chars.len() && word_character(chars[end].1) {
            end += 1;
        }
    }
    character_range(content, start, end)
}

pub fn apply_rich_text_patch_characters(
    style: &mut LiveTextStyle,
    start_character: usize,
    end_character: usize,
    patch: RichTextPatch,
) -> Result<()> {
    let range = character_range(&style.content, start_character, end_character)?;
    apply_rich_text_patch(style, range, patch)
}

pub fn apply_rich_text_patch_word(
    style: &mut LiveTextStyle,
    character: usize,
    patch: RichTextPatch,
) -> Result<()> {
    let range = word_range(&style.content, character)?;
    apply_rich_text_patch(style, range, patch)
}

pub fn apply_rich_text_patch(
    style: &mut LiveTextStyle,
    range: std::ops::Range<usize>,
    patch: RichTextPatch,
) -> Result<()> {
    validate_text_style(style)?;
    ensure!(!patch.is_empty(), "Choose at least one rich text attribute");
    ensure!(
        range.start < range.end
            && range.end <= style.content.len()
            && style.content.is_char_boundary(range.start)
            && style.content.is_char_boundary(range.end),
        "Rich text selection must be a valid UTF-8 byte range"
    );
    let mut boundaries = vec![range.start, range.end];
    for run in &style.runs {
        boundaries.extend([run.start, run.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut next: Vec<RichTextRun> = vec![];
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let existing = style
            .runs
            .iter()
            .find(|run| run.start <= start && run.end >= end);
        let selected = start < range.end && end > range.start;
        let Some(mut run) = existing.cloned().or_else(|| {
            selected.then_some(RichTextRun {
                start,
                end,
                font_name: None,
                font_size: None,
                weight: None,
                italic: None,
                color: None,
            })
        }) else {
            continue;
        };
        run.start = start;
        run.end = end;
        if selected {
            patch.apply_to(&mut run);
        }
        if run.has_override() {
            if let Some(previous) = next.last_mut()
                && same_rich_style(previous, &run)
                && previous.end == run.start
            {
                previous.end = run.end;
                continue;
            }
            next.push(run);
        }
    }
    let mut candidate = style.clone();
    candidate.runs = next;
    validate_text_style(&candidate)?;
    style.runs = candidate.runs;
    Ok(())
}

fn same_rich_style(left: &RichTextRun, right: &RichTextRun) -> bool {
    left.font_name == right.font_name
        && left.font_size == right.font_size
        && left.weight == right.weight
        && left.italic == right.italic
        && left.color == right.color
}
pub fn validate_shape_style(style: &LiveShapeStyle) -> Result<()> {
    ensure!(
        [style.red, style.green, style.blue]
            .into_iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(&v)),
        "Shape color components must be between 0 and 1"
    );
    ensure!(
        style.corner_radius.is_finite()
            && style.corner_radius >= 0.
            && style.corner_radius <= 1_000_000.,
        "Invalid corner radius"
    );
    match style.kind {
        LiveShapeKind::Line => {
            let width = style.line_width.context("Line is missing lineWidth")?;
            ensure!(
                width.is_finite() && width >= 0. && width <= 1_000_000.,
                "Invalid line width"
            );
            for p in [style.start, style.end].into_iter().flatten() {
                ensure!(
                    p.x.is_finite()
                        && p.y.is_finite()
                        && p.x.abs() <= 1_000_000.
                        && p.y.abs() <= 1_000_000.,
                    "Invalid line endpoint"
                );
            }
        }
        _ => ensure!(
            style.line_width.is_none() && style.start.is_none() && style.end.is_none(),
            "Only lines may contain line fields"
        ),
    }
    Ok(())
}
pub fn live_text(layer: &Layer) -> Result<Option<LiveTextStyle>> {
    let Some(value) = layer.metadata.get("text").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    ensure!(
        layer.image.is_some() && !layer.is_group(),
        "Live text requires cached pixels on a non-group layer"
    );
    let style = serde_json::from_value(value.clone()).context("Invalid live text metadata")?;
    validate_text_style(&style)?;
    Ok(Some(style))
}
pub fn live_shape(layer: &Layer) -> Result<Option<LiveShapeStyle>> {
    let Some(value) = layer.metadata.get("shape").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    ensure!(
        layer.image.is_some() && !layer.is_group(),
        "Live shape requires cached pixels on a non-group layer"
    );
    let style = serde_json::from_value(value.clone()).context("Invalid live shape metadata")?;
    validate_shape_style(&style)?;
    Ok(Some(style))
}
pub fn validate_live_object(layer: &Layer) -> Result<()> {
    let text = live_text(layer)?;
    let shape = live_shape(layer)?;
    ensure!(
        text.is_none() || shape.is_none(),
        "Layer cannot be both live text and a live shape"
    );
    Ok(())
}

pub fn detach_live_object(layer: &mut Layer) {
    if let Some(m) = layer.metadata.as_object_mut() {
        m.remove("text");
        m.remove("shape");
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Ellipse {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Polygon {
        points: Vec<(f32, f32)>,
    },
}

fn finite_coordinate(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.
}

impl Shape {
    fn bounds(&self) -> Result<(f32, f32, f32, f32)> {
        match self {
            Self::Rectangle {
                x,
                y,
                width,
                height,
            }
            | Self::Ellipse {
                x,
                y,
                width,
                height,
            } => {
                ensure!(
                    [*x, *y, *width, *height].into_iter().all(finite_coordinate)
                        && *width > 0.
                        && *height > 0.,
                    "Shape dimensions must be positive and finite"
                );
                Ok((*x, *y, *x + *width, *y + *height))
            }
            Self::Line { x1, y1, x2, y2 } => {
                ensure!(
                    [*x1, *y1, *x2, *y2].into_iter().all(finite_coordinate),
                    "Line coordinates must be finite"
                );
                Ok((x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2)))
            }
            Self::Polygon { points } => {
                ensure!(
                    (3..=256).contains(&points.len()),
                    "Polygon must have 3–256 vertices"
                );
                ensure!(
                    points
                        .iter()
                        .all(|(x, y)| finite_coordinate(*x) && finite_coordinate(*y)),
                    "Polygon coordinates must be finite"
                );
                Ok(points.iter().fold(
                    (
                        f32::INFINITY,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    ),
                    |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)),
                ))
            }
        }
    }

    fn coverage(&self, x: f32, y: f32, half_stroke: f32) -> (bool, bool) {
        match self {
            Self::Rectangle {
                x: left,
                y: top,
                width,
                height,
            } => {
                let right = left + width;
                let bottom = top + height;
                let inside = x >= *left && x <= right && y >= *top && y <= bottom;
                let outer = x >= left - half_stroke
                    && x <= right + half_stroke
                    && y >= top - half_stroke
                    && y <= bottom + half_stroke;
                let inner = x > left + half_stroke
                    && x < right - half_stroke
                    && y > top + half_stroke
                    && y < bottom - half_stroke;
                (inside, half_stroke > 0. && outer && !inner)
            }
            Self::Ellipse {
                x: left,
                y: top,
                width,
                height,
            } => {
                let rx = width * 0.5;
                let ry = height * 0.5;
                let dx = x - left - rx;
                let dy = y - top - ry;
                let inside_ellipse =
                    |a: f32, b: f32| a > 0. && b > 0. && (dx / a).powi(2) + (dy / b).powi(2) <= 1.;
                let inside = inside_ellipse(rx, ry);
                (
                    inside,
                    half_stroke > 0.
                        && inside_ellipse(rx + half_stroke, ry + half_stroke)
                        && !inside_ellipse(rx - half_stroke, ry - half_stroke),
                )
            }
            Self::Line { x1, y1, x2, y2 } => (
                false,
                distance_to_segment((x, y), (*x1, *y1), (*x2, *y2)) <= half_stroke,
            ),
            Self::Polygon { points } => {
                let mut inside = false;
                let mut stroke = false;
                let mut previous = points[points.len() - 1];
                for &current in points {
                    if (current.1 > y) != (previous.1 > y)
                        && x < (previous.0 - current.0) * (y - current.1) / (previous.1 - current.1)
                            + current.0
                    {
                        inside = !inside;
                    }
                    if half_stroke > 0.
                        && distance_to_segment((x, y), previous, current) <= half_stroke
                    {
                        stroke = true;
                    }
                    previous = current;
                }
                (inside, stroke)
            }
        }
    }
}

fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let length = vx * vx + vy * vy;
    let t = if length <= f32::EPSILON {
        0.
    } else {
        ((p.0 - a.0) * vx + (p.1 - a.1) * vy) / length
    }
    .clamp(0., 1.);
    (p.0 - a.0 - t * vx).hypot(p.1 - a.1 - t * vy)
}

fn over(destination: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    let sa = f32::from(source[3]) / 255.;
    let da = f32::from(destination[3]) / 255.;
    let alpha = sa + da * (1. - sa);
    if alpha <= 0. {
        return [0; 4];
    }
    let mut result = [0; 4];
    for c in 0..3 {
        result[c] = ((f32::from(source[c]) * sa + f32::from(destination[c]) * da * (1. - sa))
            / alpha)
            .round() as u8;
    }
    result[3] = (alpha * 255.).round() as u8;
    result
}

/// Insert antialiased pixels, clipped to the canvas. `stroke` is color and width
/// in document pixels. Polygon filling uses the even-odd rule; lines have round caps.
pub fn shape_layer(
    name: impl Into<String>,
    width: u32,
    height: u32,
    shape: Shape,
    fill: [u8; 4],
    stroke: Option<([u8; 4], f32)>,
) -> Result<Layer> {
    ensure!(
        valid_dimensions(width, height),
        "Canvas dimensions exceed supported bounds"
    );
    let bounds = shape.bounds()?;
    let half_stroke = match stroke {
        Some((_, width)) => {
            ensure!(
                width.is_finite() && width > 0. && width <= 1_000.,
                "Stroke width must be between 0 and 1000 pixels"
            );
            width * 0.5
        }
        None => 0.,
    };
    ensure!(
        !matches!(shape, Shape::Line { .. }) || stroke.is_some(),
        "A line needs a stroke color and width"
    );
    let x0 = (bounds.0 - half_stroke - 1.)
        .floor()
        .max(0.)
        .min(width as f32) as u32;
    let y0 = (bounds.1 - half_stroke - 1.)
        .floor()
        .max(0.)
        .min(height as f32) as u32;
    let x1 = (bounds.2 + half_stroke + 1.)
        .ceil()
        .max(0.)
        .min(width as f32) as u32;
    let y1 = (bounds.3 + half_stroke + 1.)
        .ceil()
        .max(0.)
        .min(height as f32) as u32;
    let area = u64::from(x1.saturating_sub(x0)) * u64::from(y1.saturating_sub(y0));
    let complexity = if let Shape::Polygon { points } = &shape {
        points.len() as u64
    } else {
        1
    };
    ensure!(
        area.saturating_mul(complexity) <= 128_000_000,
        "Shape is too complex at this size; use fewer vertices or a smaller shape"
    );
    let mut layer = Layer::paint(name, width, height);
    let pixels = layer.image.as_mut().unwrap();
    for y in y0..y1 {
        for x in x0..x1 {
            let mut premultiplied = [0.; 4];
            for sy in [0.25, 0.75] {
                for sx in [0.25, 0.75] {
                    let (inside, outline) =
                        shape.coverage(x as f32 + sx, y as f32 + sy, half_stroke);
                    let mut sample = if inside { fill } else { [0; 4] };
                    if outline {
                        if let Some((color, _)) = stroke {
                            sample = over(sample, color);
                        }
                    }
                    let alpha = f32::from(sample[3]) / 255.;
                    for c in 0..3 {
                        premultiplied[c] += f32::from(sample[c]) * alpha;
                    }
                    premultiplied[3] += alpha;
                }
            }
            let alpha = premultiplied[3];
            if alpha > 0. {
                pixels.put_pixel(
                    x,
                    y,
                    Rgba([
                        (premultiplied[0] / alpha).round() as u8,
                        (premultiplied[1] / alpha).round() as u8,
                        (premultiplied[2] / alpha).round() as u8,
                        (alpha * 255. / 4.).round() as u8,
                    ]),
                );
            }
        }
    }
    layer.metadata["compositorRustRasterSource"] =
        json!({"kind": "shape", "geometry": shape, "fill": fill, "stroke": stroke});
    Ok(layer)
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub family: String,
    pub size: f32,
    pub color: [u8; 4],
    pub x: f32,
    pub y: f32,
    pub tracking: f32,
    pub leading: f32,
    pub alignment: TextAlignment,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "sans-serif".into(),
            size: 32.,
            color: [0, 0, 0, 255],
            x: 0.,
            y: 0.,
            tracking: 0.,
            leading: 0.,
            alignment: TextAlignment::Left,
        }
    }
}

struct TextEngine {
    fonts: cosmic_text::FontSystem,
    cache: cosmic_text::SwashCache,
}
thread_local! { static TEXT_ENGINE: std::cell::RefCell<Option<TextEngine>> = const { std::cell::RefCell::new(None) }; }

/// Create's Sugata preset names Outfit. Keep the licensed, bundled face in the
/// same font database used for shaping, fitting and rasterization so a host's
/// font configuration cannot silently substitute a different family.
fn new_text_engine() -> TextEngine {
    let mut fonts = cosmic_text::FontSystem::new();
    fonts
        .db_mut()
        .load_font_data(include_bytes!("../assets/fonts/Outfit.ttf").to_vec());
    TextEngine {
        fonts,
        cache: cosmic_text::SwashCache::new(),
    }
}

fn cosmic_family(name: &str) -> cosmic_text::Family<'_> {
    match name {
        "sans-serif" => cosmic_text::Family::SansSerif,
        "serif" => cosmic_text::Family::Serif,
        "monospace" => cosmic_text::Family::Monospace,
        name => cosmic_text::Family::Name(name),
    }
}

fn cosmic_alignment(alignment: TextAlignment) -> cosmic_text::Align {
    match alignment {
        TextAlignment::Left => cosmic_text::Align::Left,
        TextAlignment::Center => cosmic_text::Align::Center,
        TextAlignment::Right => cosmic_text::Align::Right,
    }
}

fn set_cosmic_text<'a>(
    buffer: &mut cosmic_text::Buffer,
    text: &'a str,
    style: &'a TextStyle,
    runs: &'a [RichTextRun],
) {
    use cosmic_text::{Attrs, Color, Metrics, Shaping, Style, Weight};
    let color = Color::rgba(style.color[0], style.color[1], style.color[2], 255);
    // Project tracking (and AppKit's kern value) is in canvas pixels;
    // cosmic-text accepts ems.
    let default_attrs = Attrs::new()
        .family(cosmic_family(&style.family))
        .letter_spacing(style.tracking / style.size)
        .color(color);
    let alignment = cosmic_alignment(style.alignment);
    if runs.is_empty() {
        buffer.set_text(text, &default_attrs, Shaping::Advanced, Some(alignment));
        return;
    }

    let mut spans = Vec::with_capacity(runs.len() * 2 + 1);
    let mut cursor = 0usize;
    for run in runs {
        if cursor < run.start {
            spans.push((&text[cursor..run.start], default_attrs.clone()));
        }
        let mut attrs = default_attrs.clone();
        if let Some(font) = &run.font_name {
            attrs = attrs.family(cosmic_family(font));
        }
        if let Some(size) = run.font_size {
            let line_height = if style.leading > 0. {
                style.leading / style.size * size
            } else {
                size * 1.2
            };
            attrs = attrs.metrics(Metrics::new(size, line_height));
        }
        if let Some(weight) = run.weight {
            attrs = attrs.weight(Weight(weight));
        }
        if let Some(italic) = run.italic {
            attrs = attrs.style(if italic { Style::Italic } else { Style::Normal });
        }
        if let Some(run_color) = run.color {
            attrs = attrs.color(Color::rgba(
                component(run_color[0]),
                component(run_color[1]),
                component(run_color[2]),
                component(run_color[3]),
            ));
        }
        spans.push((&text[run.start..run.end], attrs));
        cursor = run.end;
    }
    if cursor < text.len() {
        spans.push((&text[cursor..], default_attrs.clone()));
    }
    buffer.set_rich_text(spans, &default_attrs, Shaping::Advanced, Some(alignment));
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutReport {
    pub content_width: f32,
    pub content_height: f32,
    pub available_width: f32,
    pub available_height: f32,
    pub line_count: usize,
    pub horizontal_overflow: bool,
    pub vertical_overflow: bool,
    pub missing_fonts: Vec<String>,
}

impl TextLayoutReport {
    pub fn overflows(&self) -> bool {
        self.horizontal_overflow || self.vertical_overflow
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextFitResult {
    pub style: LiveTextStyle,
    pub report: TextLayoutReport,
    pub fitted: bool,
    pub minimum_reached: bool,
}

fn available_text_box(style: &LiveTextStyle) -> (f32, f32) {
    const PADDING: f32 = 12.0;
    style.box_size.map_or((30_000.0, 30_000.0), |size| {
        (
            (size.width - PADDING).max(1.0),
            (size.height - PADDING).max(1.0),
        )
    })
}

fn named_font_available(fonts: &cosmic_text::FontSystem, family: &str) -> bool {
    if matches!(family, "sans-serif" | "serif" | "monospace") {
        return true;
    }
    fonts.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })
}

/// Shape all content with cosmic-text and report the real wrapped extents.
/// Height is deliberately unconstrained during measurement so text that would
/// be clipped by its live box remains visible to the overflow calculation.
pub fn text_layout_report(style: &LiveTextStyle) -> Result<TextLayoutReport> {
    use cosmic_text::{Buffer, Metrics};
    validate_text_style(style)?;
    let (available_width, available_height) = available_text_box(style);
    let legacy = TextStyle {
        family: style.font_name.clone(),
        size: style.font_size,
        color: [
            component(style.red),
            component(style.green),
            component(style.blue),
            255,
        ],
        x: 0.0,
        y: 0.0,
        tracking: style.tracking,
        leading: if style.leading > 0.0 {
            style.leading
        } else {
            style.font_size * 1.2
        },
        alignment: style.alignment,
    };
    TEXT_ENGINE.with(|engine| -> Result<TextLayoutReport> {
        let mut engine = engine
            .try_borrow_mut()
            .context("Text renderer is already in use")?;
        let engine = engine.get_or_insert_with(new_text_engine);
        ensure!(
            engine.fonts.db().faces().next().is_some(),
            "No system fonts are available"
        );
        let mut missing_fonts = vec![];
        for family in std::iter::once(style.font_name.as_str())
            .chain(style.runs.iter().filter_map(|run| run.font_name.as_deref()))
        {
            if !named_font_available(&engine.fonts, family)
                && !missing_fonts
                    .iter()
                    .any(|known: &String| known.eq_ignore_ascii_case(family))
            {
                missing_fonts.push(family.to_owned());
            }
        }
        let mut buffer = Buffer::new(&mut engine.fonts, Metrics::new(legacy.size, legacy.leading));
        buffer.set_size(Some(available_width), None);
        set_cosmic_text(&mut buffer, &style.content, &legacy, &style.runs);
        buffer.shape_until_scroll(&mut engine.fonts, false);
        let mut content_width = 0.0_f32;
        let mut content_height = 0.0_f32;
        let mut line_count = 0usize;
        for run in buffer.layout_runs() {
            content_width = content_width.max(run.line_w);
            content_height = content_height.max(run.line_top + run.line_height);
            line_count += 1;
        }
        // Empty text still occupies one line according to the live object's
        // metrics, which keeps fit/report behaviour stable while typing.
        if line_count == 0 {
            content_height = legacy.leading;
        }
        Ok(TextLayoutReport {
            content_width,
            content_height,
            available_width,
            available_height,
            line_count,
            horizontal_overflow: content_width > available_width + 0.5,
            vertical_overflow: content_height > available_height + 0.5,
            missing_fonts,
        })
    })
}

fn scaled_text_style(style: &LiveTextStyle, scale: f32) -> LiveTextStyle {
    let mut candidate = style.clone();
    candidate.font_size *= scale;
    if candidate.leading > 0.0 {
        candidate.leading *= scale;
    }
    for run in &mut candidate.runs {
        if let Some(size) = &mut run.font_size {
            *size *= scale;
        }
    }
    candidate
}

/// Fit live text into its box by scaling base and per-run font sizes together.
/// The search is bounded and never drops the base font below `minimum_size`.
pub fn fit_text_to_box(style: &LiveTextStyle, minimum_size: f32) -> Result<TextFitResult> {
    validate_text_style(style)?;
    ensure!(
        style.box_size.is_some(),
        "Text needs a fixed box before it can be fitted"
    );
    ensure!(
        minimum_size.is_finite() && (1.0..=style.font_size).contains(&minimum_size),
        "Minimum size must be between 1 and the current base font size"
    );
    let original_report = text_layout_report(style)?;
    if !original_report.overflows() {
        return Ok(TextFitResult {
            style: style.clone(),
            report: original_report,
            fitted: false,
            minimum_reached: false,
        });
    }

    let minimum_scale = minimum_size / style.font_size;
    let minimum = scaled_text_style(style, minimum_scale);
    let minimum_report = text_layout_report(&minimum)?;
    if minimum_report.overflows() {
        return Ok(TextFitResult {
            style: minimum,
            report: minimum_report,
            fitted: true,
            minimum_reached: true,
        });
    }

    let mut low = minimum_scale;
    let mut high = 1.0_f32;
    let mut best = minimum;
    let mut best_report = minimum_report;
    for _ in 0..12 {
        let scale = (low + high) * 0.5;
        let candidate = scaled_text_style(style, scale);
        let report = text_layout_report(&candidate)?;
        if report.overflows() {
            high = scale;
        } else {
            low = scale;
            best = candidate;
            best_report = report;
        }
    }
    Ok(TextFitResult {
        style: best,
        report: best_report,
        fitted: true,
        minimum_reached: (low - minimum_scale).abs() < 0.002,
    })
}

/// Rasterize Unicode text using Linux system fonts, advanced shaping and font
/// fallback. The result is an ordinary editable pixel layer, not a Mac text object.
pub fn text_layer(
    name: impl Into<String>,
    width: u32,
    height: u32,
    text: &str,
    style: &TextStyle,
) -> Result<Layer> {
    text_layer_with_runs(name, width, height, text, style, &[])
}

fn text_layer_with_runs(
    name: impl Into<String>,
    width: u32,
    height: u32,
    text: &str,
    style: &TextStyle,
    runs: &[RichTextRun],
) -> Result<Layer> {
    use cosmic_text::{Buffer, Color, Metrics};
    ensure!(
        valid_dimensions(width, height),
        "Canvas dimensions exceed supported bounds"
    );
    ensure!(
        !text.trim().is_empty() && text.len() <= 16_384,
        "Text must contain 1–16384 bytes"
    );
    ensure!(
        style.size.is_finite() && (1. ..=2000.).contains(&style.size),
        "Font size must be 1–512 pixels"
    );
    let maximum_size = runs
        .iter()
        .filter_map(|run| run.font_size)
        .fold(style.size, f32::max);
    ensure!(
        text.chars().count() as f64 * f64::from(maximum_size).powi(2) <= 64_000_000.,
        "Text exceeds the glyph rasterization budget; use fewer characters or a smaller font"
    );
    ensure!(
        finite_coordinate(style.x) && finite_coordinate(style.y),
        "Text coordinates must be finite"
    );
    ensure!(
        !style.family.trim().is_empty() && style.family.len() <= 512,
        "Invalid font family"
    );
    let mut layer = Layer::paint(name, width, height);
    let pixels = layer.image.as_mut().unwrap();
    TEXT_ENGINE.with(|engine| -> Result<()> {
        let mut engine = engine
            .try_borrow_mut()
            .context("Text renderer is already in use")?;
        let engine = engine.get_or_insert_with(new_text_engine);
        ensure!(
            engine.fonts.db().faces().next().is_some(),
            "No system fonts are available"
        );
        let mut buffer = Buffer::new(
            &mut engine.fonts,
            Metrics::new(
                style.size,
                if style.leading > 0. {
                    style.leading
                } else {
                    style.size * 1.2
                },
            ),
        );
        buffer.set_size(
            Some((width as f32 - style.x).max(1.)),
            Some((height as f32 - style.y).max(1.)),
        );
        let color = Color::rgba(style.color[0], style.color[1], style.color[2], 255);
        set_cosmic_text(&mut buffer, text, style, runs);
        buffer.draw(
            &mut engine.fonts,
            &mut engine.cache,
            color,
            |x, y, w, h, color| {
                let x = i64::from(x) + style.x.round() as i64;
                let y = i64::from(y) + style.y.round() as i64;
                for py in y.max(0)..(y + i64::from(h)).min(i64::from(height)) {
                    for px in x.max(0)..(x + i64::from(w)).min(i64::from(width)) {
                        let old = pixels.get_pixel(px as u32, py as u32).0;
                        pixels.put_pixel(px as u32, py as u32, Rgba(over(old, color.as_rgba())));
                    }
                }
            },
        );
        // Glyph bitmap caches can grow without bound across unrelated font sizes;
        // keep font discovery cached but release this insertion's raster cache.
        engine.cache = cosmic_text::SwashCache::new();
        Ok(())
    })?;
    // Swash's per-pixel API supplies coverage and intentionally ignores the base
    // color alpha. Apply object opacity once after all overlapping glyphs, so
    // antialiasing and overlapping letters cannot make translucent text opaque.
    for pixel in pixels.pixels_mut() {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(style.color[3]) + 127) / 255) as u8;
        if pixel[3] == 0 {
            *pixel = Rgba([0; 4]);
        }
    }
    layer.metadata["compositorRustRasterSource"] = json!({"kind": "text", "content": text, "family": style.family, "size": style.size, "color": style.color, "origin": [style.x, style.y]});
    Ok(layer)
}

pub fn live_text_layer(
    name: impl Into<String>,
    origin: ObjectPoint,
    style: LiveTextStyle,
) -> Result<Layer> {
    validate_text_style(&style)?;
    let mut layer = Layer::paint(name, 1, 1);
    layer.offset_x = origin.x;
    layer.offset_y = origin.y;
    set_live_text(&mut layer, style)?;
    Ok(layer)
}

pub fn live_shape_layer(
    name: impl Into<String>,
    origin: ObjectPoint,
    width: u32,
    height: u32,
    style: LiveShapeStyle,
) -> Result<Layer> {
    ensure!(
        finite_coordinate(origin.x) && finite_coordinate(origin.y),
        "Invalid shape origin"
    );
    let mut layer = Layer::paint(name, width, height);
    layer.offset_x = origin.x;
    layer.offset_y = origin.y;
    set_live_shape(&mut layer, style, width, height)?;
    Ok(layer)
}

pub fn set_live_text(layer: &mut Layer, style: LiveTextStyle) -> Result<()> {
    validate_text_style(&style)?;
    let padding = 12.;
    let lines = style.content.lines().collect::<Vec<_>>();
    let line_height = if style.leading > 0. {
        style.leading
    } else {
        style.font_size * 1.2
    };
    let maximum_size = style
        .runs
        .iter()
        .filter_map(|run| run.font_size)
        .fold(style.font_size, f32::max);
    let (width, height) = if let Some(size) = style.box_size {
        (size.width.ceil() as u32, size.height.ceil() as u32)
    } else {
        let longest = lines.iter().map(|s| s.chars().count()).max().unwrap_or(0) as f32;
        let width = (longest * (maximum_size * 0.75 + style.tracking.max(0.))
            + padding * 2.
            + maximum_size * 0.1)
            .ceil()
            .max(16.);
        let maximum_line_height = line_height / style.font_size * maximum_size;
        let height = (lines.len().max(1) as f32 * maximum_line_height + padding * 2.)
            .ceil()
            .max(16.);
        (width as u32, height as u32)
    };
    ensure!(
        valid_dimensions(width, height),
        "Text raster exceeds supported bounds"
    );
    let legacy = TextStyle {
        family: style.font_name.clone(),
        size: style.font_size,
        color: [
            component(style.red),
            component(style.green),
            component(style.blue),
            255,
        ],
        x: padding,
        y: padding,
        tracking: style.tracking,
        leading: line_height,
        alignment: style.alignment,
    };
    let rendered = text_layer_with_runs(
        layer.name.clone(),
        width,
        height,
        &style.content,
        &legacy,
        &style.runs,
    )?;
    layer.image = rendered.image;
    if !layer.metadata.is_object() {
        layer.metadata = json!({});
    }
    layer.metadata["text"] = serde_json::to_value(&style)?;
    layer.metadata.as_object_mut().unwrap().remove("shape");
    layer
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("compositorRustRasterSource");
    Ok(())
}

pub fn set_live_shape(
    layer: &mut Layer,
    style: LiveShapeStyle,
    width: u32,
    height: u32,
) -> Result<()> {
    validate_shape_style(&style)?;
    ensure!(
        valid_dimensions(width, height),
        "Shape raster exceeds supported bounds"
    );
    let color = [
        component(style.red),
        component(style.green),
        component(style.blue),
        255,
    ];
    let shape = match style.kind {
        LiveShapeKind::Rectangle => Shape::Rectangle {
            x: 0.,
            y: 0.,
            width: width as f32,
            height: height as f32,
        },
        LiveShapeKind::Ellipse => Shape::Ellipse {
            x: 0.,
            y: 0.,
            width: width as f32,
            height: height as f32,
        },
        LiveShapeKind::Line => {
            let inset = style.line_width.unwrap_or(1.).max(1.) * 0.5;
            let start = style.start.unwrap_or(ObjectPoint {
                x: inset / width as f32,
                y: inset / height as f32,
            });
            let end = style.end.unwrap_or(ObjectPoint {
                x: 1. - inset / width as f32,
                y: 1. - inset / height as f32,
            });
            Shape::Line {
                x1: start.x * width as f32,
                y1: start.y * height as f32,
                x2: end.x * width as f32,
                y2: end.y * height as f32,
            }
        }
    };
    let stroke = matches!(style.kind, LiveShapeKind::Line)
        .then_some((color, style.line_width.unwrap_or(1.).max(1.)));
    let mut image = shape_layer(layer.name.clone(), width, height, shape, color, stroke)?
        .image
        .unwrap();
    if style.kind == LiveShapeKind::Rectangle && style.corner_radius > 0. {
        let radius = style
            .corner_radius
            .min(width as f32 * 0.5)
            .min(height as f32 * 0.5);
        for y in 0..height {
            for x in 0..width {
                let dx = (radius - (x as f32 + 0.5).min(width as f32 - x as f32 - 0.5)).max(0.);
                let dy = (radius - (y as f32 + 0.5).min(height as f32 - y as f32 - 0.5)).max(0.);
                if dx * dx + dy * dy > radius * radius {
                    image.put_pixel(x, y, Rgba([0; 4]));
                }
            }
        }
    }
    layer.image = Some(image.into());
    if !layer.metadata.is_object() {
        layer.metadata = json!({});
    }
    layer.metadata["shape"] = serde_json::to_value(&style)?;
    layer.metadata.as_object_mut().unwrap().remove("text");
    layer
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("compositorRustRasterSource");
    Ok(())
}

pub fn rasterize_live_object(layer: &mut Layer) -> Result<bool> {
    if let Some(style) = live_text(layer)? {
        set_live_text(layer, style)?;
        return Ok(true);
    }
    if let Some(style) = live_shape(layer)? {
        let image = layer
            .image
            .as_ref()
            .context("Live shape has no cached pixels")?;
        let (w, h) = image.dimensions();
        set_live_shape(layer, style, w, h)?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_font_available_includes_bundled_outfit() {
        let engine = new_text_engine();
        assert!(named_font_available(&engine.fonts, "Outfit"));
    }

    #[test]
    fn rectangle_fill_stroke_and_alpha_are_separate() {
        let layer = shape_layer(
            "Rect",
            20,
            20,
            Shape::Rectangle {
                x: 4.,
                y: 4.,
                width: 12.,
                height: 12.,
            },
            [255, 0, 0, 128],
            Some(([0, 0, 255, 255], 2.)),
        )
        .unwrap();
        let p = layer.image.unwrap();
        assert_eq!(p.get_pixel(10, 10).0, [255, 0, 0, 128]);
        assert_eq!(p.get_pixel(4, 10).0, [0, 0, 255, 255]);
        assert_eq!(p.get_pixel(1, 1).0, [0; 4]);
    }
    #[test]
    fn ellipse_corners_clear_and_polygon_even_odd_fill() {
        let ellipse = shape_layer(
            "Ellipse",
            16,
            16,
            Shape::Ellipse {
                x: 2.,
                y: 2.,
                width: 12.,
                height: 12.,
            },
            [0, 255, 0, 255],
            None,
        )
        .unwrap()
        .image
        .unwrap();
        assert_eq!(ellipse.get_pixel(8, 8).0, [0, 255, 0, 255]);
        assert_eq!(ellipse.get_pixel(2, 2).0, [0; 4]);
        let polygon = shape_layer(
            "Triangle",
            16,
            16,
            Shape::Polygon {
                points: vec![(2., 2.), (14., 2.), (8., 14.)],
            },
            [255; 4],
            None,
        )
        .unwrap()
        .image
        .unwrap();
        assert_eq!(polygon.get_pixel(8, 6).0, [255; 4]);
        assert_eq!(polygon.get_pixel(2, 12).0, [0; 4]);
    }
    #[test]
    fn line_clips_and_degenerate_round_cap_is_bounded() {
        let line = shape_layer(
            "Line",
            10,
            10,
            Shape::Line {
                x1: -20.,
                y1: 5.,
                x2: 20.,
                y2: 5.,
            },
            [0; 4],
            Some(([255, 255, 0, 255], 2.)),
        )
        .unwrap()
        .image
        .unwrap();
        assert_eq!(line.get_pixel(0, 5).0, [255, 255, 0, 255]);
        assert_eq!(line.get_pixel(9, 5).0, [255, 255, 0, 255]);
        assert!(
            shape_layer(
                "Bad",
                10,
                10,
                Shape::Rectangle {
                    x: 0.,
                    y: 0.,
                    width: f32::NAN,
                    height: 5.
                },
                [255; 4],
                None
            )
            .is_err()
        );
        assert!(
            shape_layer(
                "Bad",
                10,
                10,
                Shape::Polygon { points: vec![] },
                [255; 4],
                None
            )
            .is_err()
        );
    }
    #[test]
    fn raster_sources_roundtrip_as_ordinary_compatible_layers() {
        let mut doc = crate::model::Document::new(32, 32);
        doc.layers = vec![
            shape_layer(
                "Rect",
                32,
                32,
                Shape::Rectangle {
                    x: 2.,
                    y: 2.,
                    width: 20.,
                    height: 20.,
                },
                [100, 200, 50, 255],
                None,
            )
            .unwrap(),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Shape.comp");
        crate::document::save(&doc, &path).unwrap();
        let loaded = crate::document::open(&path).unwrap();
        assert_eq!(loaded.layers[0].image, doc.layers[0].image);
        assert_eq!(
            loaded.layers[0].metadata["compositorRustRasterSource"],
            doc.layers[0].metadata["compositorRustRasterSource"]
        );
    }
    #[test]
    fn system_font_text_is_colored_clipped_and_saved_as_pixels() {
        let style = TextStyle {
            size: 24.,
            color: [200, 10, 30, 128],
            x: 3.,
            y: 2.,
            ..Default::default()
        };
        let layer = text_layer("Text", 180, 64, "Hello Linux", &style).unwrap();
        let image = layer.image.as_ref().unwrap();
        assert!(image.pixels().any(|p| p[3] > 0));
        assert!(image.pixels().all(|p| p[3] <= 128));
        assert!(
            image
                .pixels()
                .filter(|p| p[3] > 0)
                .all(|p| p[0] == 200 && p[1] == 10 && p[2] == 30)
        );
        let mut doc = crate::model::Document::new(180, 64);
        doc.layers = vec![layer];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Text.comp");
        crate::document::save(&doc, &path).unwrap();
        assert_eq!(
            crate::document::open(&path).unwrap().layers[0].image,
            doc.layers[0].image
        );
    }

    #[test]
    fn tracking_adds_canvas_pixels_independently_of_font_size() {
        fn ink_width(layer: &Layer) -> u32 {
            let pixels = layer.image.as_ref().unwrap();
            let mut left = pixels.width();
            let mut right = 0;
            for (x, _, pixel) in pixels.enumerate_pixels() {
                if pixel[3] != 0 {
                    left = left.min(x);
                    right = right.max(x);
                }
            }
            assert!(left <= right, "text must have visible glyphs");
            right - left + 1
        }
        for size in [24., 72.] {
            let mut style = TextStyle {
                family: "monospace".into(),
                size,
                ..Default::default()
            };
            let base = text_layer("Base", 1000, 160, "HHHHH", &style).unwrap();
            style.tracking = 2.;
            let tracked = text_layer("Tracked", 1000, 160, "HHHHH", &style).unwrap();
            let extra = ink_width(&tracked) as i32 - ink_width(&base) as i32;
            // Four inter-glyph gaps, with a pixel of raster rounding at each edge.
            assert!(
                (6..=10).contains(&extra),
                "font size {size}: 2px tracking added {extra}px"
            );
        }
    }

    #[test]
    fn rich_text_rasterizes_mixed_color_and_weight_and_roundtrips() {
        let mut style = LiveTextStyle {
            content: "Bold blue".into(),
            font_name: "sans-serif".into(),
            font_size: 42.,
            box_size: Some(ObjectSize {
                width: 360.,
                height: 100.,
            }),
            ..Default::default()
        };
        style.runs = vec![
            RichTextRun {
                start: 0,
                end: 4,
                font_name: None,
                font_size: None,
                weight: Some(900),
                italic: None,
                color: Some([0.9, 0.05, 0.05, 1.]),
            },
            RichTextRun {
                start: 5,
                end: 9,
                font_name: None,
                font_size: None,
                weight: Some(200),
                italic: Some(true),
                color: Some([0.05, 0.1, 0.9, 1.]),
            },
        ];
        let layer = live_text_layer("Rich", ObjectPoint { x: 4., y: 6. }, style.clone()).unwrap();
        let image = layer.image.as_ref().unwrap();
        assert!(
            image
                .pixels()
                .any(|pixel| pixel[3] > 0 && pixel[0] > pixel[2] * 2)
        );
        assert!(
            image
                .pixels()
                .any(|pixel| pixel[3] > 0 && pixel[2] > pixel[0] * 2)
        );

        let mut document = crate::model::Document::new(400, 140);
        document.layers = vec![layer];
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Rich.comp");
        crate::document::save(&document, &path).unwrap();
        let reopened = crate::document::open(&path).unwrap();
        assert_eq!(live_text(&reopened.layers[0]).unwrap(), Some(style));
        assert_eq!(reopened.layers[0].image, document.layers[0].image);
    }

    #[test]
    fn rich_text_rejects_invalid_utf8_overlap_and_attributes_atomically() {
        let mut style = LiveTextStyle {
            content: "Aé word".into(),
            ..Default::default()
        };
        style.runs = vec![RichTextRun {
            start: 1,
            end: 2,
            font_name: None,
            font_size: None,
            weight: Some(700),
            italic: None,
            color: None,
        }];
        assert!(validate_text_style(&style).is_err());

        style.runs.clear();
        let original = style.clone();
        assert!(
            apply_rich_text_patch_characters(
                &mut style,
                0,
                2,
                RichTextPatch {
                    weight: Some(0),
                    ..Default::default()
                },
            )
            .is_err()
        );
        assert_eq!(style, original);

        style.runs = vec![
            RichTextRun {
                start: 0,
                end: 3,
                font_name: None,
                font_size: None,
                weight: Some(600),
                italic: None,
                color: None,
            },
            RichTextRun {
                start: 2,
                end: 5,
                font_name: None,
                font_size: None,
                weight: Some(300),
                italic: None,
                color: None,
            },
        ];
        assert!(validate_text_style(&style).is_err());
    }

    #[test]
    fn character_and_word_patches_preserve_other_run_attributes() {
        let mut style = LiveTextStyle {
            content: "Warm café light".into(),
            ..Default::default()
        };
        apply_rich_text_patch_word(
            &mut style,
            6,
            RichTextPatch {
                color: Some([0.8, 0.2, 0.1, 1.]),
                ..Default::default()
            },
        )
        .unwrap();
        apply_rich_text_patch_characters(
            &mut style,
            5,
            9,
            RichTextPatch {
                weight: Some(800),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(style.runs.len(), 1);
        assert_eq!(style.runs[0].color, Some([0.8, 0.2, 0.1, 1.]));
        assert_eq!(style.runs[0].weight, Some(800));
        set_text_content(&mut style, "Edited café");
        assert!(style.runs.is_empty());
    }
}
