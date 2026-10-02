//! Advisory content guides and deterministic export preflight. Platform chrome
//! varies with device, caption and placement, so these are composition guides.
use crate::{
    create_project::Project,
    model::{Document, Layer},
    objects,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAX_ISSUES: usize = 512;
const MAX_TEXT_OBJECTS: usize = 128;
const MAX_VISUAL_OBJECTS: usize = 512;
const MAX_TEXT_PAIRS: usize = 4_096;
const TEXT_OVERLAP_TOLERANCE: f32 = 1.0;
const MIN_TEXT_SPACING: f32 = 8.0;
const ALPHA_SCAN_PIXELS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeAreaPreset {
    #[default]
    CanvasMargin,
    Story,
    VerticalVideo,
}
impl SafeAreaPreset {
    pub const ALL: [Self; 3] = [Self::CanvasMargin, Self::Story, Self::VerticalVideo];

    pub fn label(self) -> &'static str {
        match self {
            Self::CanvasMargin => "Canvas · 5%",
            Self::Story => "Story · title guide",
            Self::VerticalVideo => "Vertical video · title guide",
        }
    }

    pub fn insets(self) -> [f32; 4] {
        match self {
            Self::CanvasMargin => [0.05; 4],
            Self::Story => [0.06, 0.15, 0.06, 0.22],
            Self::VerticalVideo => [0.06, 0.15, 0.16, 0.35],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreflightIssue {
    pub page: String,
    pub layer: Option<String>,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}
impl Bounds {
    fn of(points: &[Point; 4]) -> Self {
        Self {
            left: points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
            top: points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
            right: points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
            bottom: points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
        }
    }

    fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

#[derive(Clone, Copy)]
struct Quad {
    points: [Point; 4],
    bounds: Bounds,
}

#[derive(Clone, Copy)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}
impl Affine {
    const IDENTITY: Self = Self {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 1.,
        tx: 0.,
        ty: 0.,
    };

    // Returns self(local(point)).
    fn compose(self, local: Self) -> Self {
        Self {
            a: self.a * local.a + self.c * local.b,
            b: self.b * local.a + self.d * local.b,
            c: self.a * local.c + self.c * local.d,
            d: self.b * local.c + self.d * local.d,
            tx: self.a * local.tx + self.c * local.ty + self.tx,
            ty: self.b * local.tx + self.d * local.ty + self.ty,
        }
    }

    fn point(self, x: f32, y: f32) -> Point {
        Point {
            x: self.a * x + self.c * y + self.tx,
            y: self.b * x + self.d * y + self.ty,
        }
    }

    // Mirrors the raster renderer: rotation is clockwise around scaled bounds
    // and negative scale flips without moving the unrotated bounds.
    fn for_leaf(layer: &Layer, width: u32, height: u32) -> Option<Self> {
        if width == 0
            || height == 0
            || ![
                layer.offset_x,
                layer.offset_y,
                layer.scale_x,
                layer.scale_y,
                layer.rotation,
            ]
            .into_iter()
            .all(f32::is_finite)
            || layer.scale_x == 0.
            || layer.scale_y == 0.
        {
            return None;
        }
        let (width, height) = (width as f32, height as f32);
        let (sin, cos) = layer.rotation.rem_euclid(360.).to_radians().sin_cos();
        let (a, b, c, d) = (
            cos * layer.scale_x,
            sin * layer.scale_x,
            -sin * layer.scale_y,
            cos * layer.scale_y,
        );
        let cx = layer.offset_x + width * layer.scale_x.abs() * 0.5;
        let cy = layer.offset_y + height * layer.scale_y.abs() * 0.5;
        Some(Self {
            a,
            b,
            c,
            d,
            tx: cx - a * width * 0.5 - c * height * 0.5,
            ty: cy - b * width * 0.5 - d * height * 0.5,
        })
    }
}

impl Quad {
    fn transformed(transform: Affine, width: u32, height: u32) -> Option<Self> {
        let points = [
            transform.point(0., 0.),
            transform.point(width as f32, 0.),
            transform.point(width as f32, height as f32),
            transform.point(0., height as f32),
        ];
        points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite())
            .then(|| Self {
                bounds: Bounds::of(&points),
                points,
            })
    }

    fn canvas(document: &Document) -> Self {
        let points = [
            Point { x: 0., y: 0. },
            Point {
                x: document.width as f32,
                y: 0.,
            },
            Point {
                x: document.width as f32,
                y: document.height as f32,
            },
            Point {
                x: 0.,
                y: document.height as f32,
            },
        ];
        Self {
            bounds: Bounds::of(&points),
            points,
        }
    }
}

struct TextObject {
    name: String,
    quad: Quad,
    visual_index: usize,
    color: Option<[f32; 3]>,
}
struct Geometry {
    text: Vec<TextObject>,
    visual: Vec<Quad>,
    text_complete: bool,
    visual_complete: bool,
}
impl Default for Geometry {
    fn default() -> Self {
        Self {
            text: vec![],
            visual: vec![],
            text_complete: true,
            visual_complete: true,
        }
    }
}

pub fn inspect_project(
    project: &mut Project,
    preset: SafeAreaPreset,
) -> Result<Vec<PreflightIssue>> {
    let mut issues = vec![];
    project.for_each_page_document(|page, document| {
        inspect_document(document, &page.name, preset, &mut issues)?;
        Ok(())
    })?;
    Ok(issues)
}

pub fn inspect_document(
    document: &Document,
    page: &str,
    preset: SafeAreaPreset,
    issues: &mut Vec<PreflightIssue>,
) -> Result<()> {
    if issues.len() >= MAX_ISSUES {
        return Ok(());
    }
    let mut geometry = Geometry::default();
    collect(
        &document.layers,
        document,
        page,
        preset,
        Affine::IDENTITY,
        1.,
        false,
        issues,
        &mut geometry,
    )?;
    inspect_text_relationships(page, issues, &geometry);
    inspect_text_contrast(document, page, issues, &geometry);
    if (!geometry.text_complete || !geometry.visual_complete) && issues.len() < MAX_ISSUES {
        push_issue(
            issues,
            page,
            None,
            "content_check_limited",
            "The content check reached its object limit; review dense text layouts manually."
                .into(),
        );
    }
    if document.metadata["omuseContent"]["altText"]
        .as_str()
        .is_none_or(|text| text.trim().is_empty())
    {
        push_issue(
            issues,
            page,
            None,
            "missing_alt_text",
            "Add an image description for the exported content pack.".into(),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect(
    layers: &[Layer],
    document: &Document,
    page: &str,
    preset: SafeAreaPreset,
    parent_transform: Affine,
    inherited_opacity: f32,
    inherited_mask: bool,
    issues: &mut Vec<PreflightIssue>,
    geometry: &mut Geometry,
) -> Result<()> {
    for layer in layers.iter().filter(|layer| layer.visible) {
        if issues.len() >= MAX_ISSUES {
            return Ok(());
        }
        if !layer.opacity.is_finite() || layer.opacity <= 0. {
            continue;
        }
        let opacity = inherited_opacity * layer.opacity.clamp(0., 1.);
        if opacity <= 0. {
            continue;
        }
        let masked = inherited_mask || (mask_enabled(layer) && layer.mask.is_some());
        if is_adjustment(layer) {
            push_visual(geometry, Quad::canvas(document));
            continue;
        }
        if layer.is_group() {
            // Folders are pass-through in raster.rs. Their masks and opacity
            // inherit, while their geometric transform does not affect children.
            collect(
                &layer.children,
                document,
                page,
                preset,
                parent_transform,
                opacity,
                masked,
                issues,
                geometry,
            )?;
            continue;
        }
        let Some(image) = &layer.image else {
            continue;
        };
        if !visually_nonempty(image) {
            continue;
        }
        let Some(local) = Affine::for_leaf(layer, image.width(), image.height()) else {
            continue;
        };
        let Some(quad) = Quad::transformed(
            parent_transform.compose(local),
            image.width(),
            image.height(),
        ) else {
            continue;
        };
        let visual_index = geometry.visual.len();
        push_visual(geometry, quad);
        let Some(style) = objects::live_text(layer)? else {
            continue;
        };
        inspect_text_layout(layer, &style, quad, document, page, preset, issues)?;
        if geometry.text.len() >= MAX_TEXT_OBJECTS {
            geometry.text_complete = false;
            continue;
        }
        geometry.text.push(TextObject {
            name: layer.name.clone(),
            quad,
            visual_index,
            color: contrast_color(layer, &style, opacity, masked),
        });
    }
    Ok(())
}

fn inspect_text_layout(
    layer: &Layer,
    style: &objects::LiveTextStyle,
    quad: Quad,
    document: &Document,
    page: &str,
    preset: SafeAreaPreset,
    issues: &mut Vec<PreflightIssue>,
) -> Result<()> {
    let report = objects::text_layout_report(style)?;
    if report.overflows() {
        push_issue(
            issues,
            page,
            Some(&layer.name),
            "text_overflow",
            "Text exceeds its editable box. Shorten it or use Fit text.".into(),
        );
    }
    if !report.missing_fonts.is_empty() {
        push_issue(
            issues,
            page,
            Some(&layer.name),
            "missing_font",
            format!(
                "Substitute font in use: {}",
                report.missing_fonts.join(", ")
            ),
        );
    }
    let [left, top, right, bottom] = preset.insets();
    let bounds = quad.bounds;
    let (width, height) = (document.width as f32, document.height as f32);
    if bounds.left < 0. || bounds.top < 0. || bounds.right > width || bounds.bottom > height {
        push_issue(
            issues,
            page,
            Some(&layer.name),
            "text_off_canvas",
            "Text crosses the canvas edge and may be clipped.".into(),
        );
    } else if bounds.left < width * left
        || bounds.top < height * top
        || bounds.right > width * (1. - right)
        || bounds.bottom > height * (1. - bottom)
    {
        push_issue(
            issues,
            page,
            Some(&layer.name),
            "text_outside_guide",
            "Text falls outside the selected content guide. Review the phone preview.".into(),
        );
    }
    Ok(())
}

fn inspect_text_relationships(page: &str, issues: &mut Vec<PreflightIssue>, geometry: &Geometry) {
    if !geometry.text_complete {
        return;
    }
    let mut pairs = 0usize;
    for (index, left) in geometry.text.iter().enumerate() {
        for right in geometry.text.iter().skip(index + 1) {
            if pairs >= MAX_TEXT_PAIRS {
                push_issue(
                    issues,
                    page,
                    None,
                    "content_check_limited",
                    "The content check reached its pair limit; review dense text layouts manually."
                        .into(),
                );
                return;
            }
            pairs += 1;
            if !left.quad.bounds.intersects(right.quad.bounds)
                && bounds_distance(left.quad.bounds, right.quad.bounds) >= MIN_TEXT_SPACING
            {
                continue;
            }
            if quads_overlap(left.quad, right.quad, TEXT_OVERLAP_TOLERANCE) {
                push_issue(
                    issues,
                    page,
                    Some(&right.name),
                    "text_overlap",
                    format!(
                        "Text overlaps “{}”. Separate the text boxes before export.",
                        left.name
                    ),
                );
            } else if quad_distance(left.quad, right.quad) < MIN_TEXT_SPACING {
                push_issue(
                    issues,
                    page,
                    Some(&right.name),
                    "text_tight_spacing",
                    format!(
                        "Text is within {MIN_TEXT_SPACING:.0} px of “{}”. Review the spacing.",
                        left.name
                    ),
                );
            }
            if issues.len() >= MAX_ISSUES {
                return;
            }
        }
    }
}

fn inspect_text_contrast(
    document: &Document,
    page: &str,
    issues: &mut Vec<PreflightIssue>,
    geometry: &Geometry,
) {
    if document.background[3] != 255 || !geometry.visual_complete {
        return;
    }
    let background = [
        document.background[0] as f32 / 255.,
        document.background[1] as f32 / 255.,
        document.background[2] as f32 / 255.,
    ];
    for text in &geometry.text {
        let Some(color) = text.color else {
            continue;
        };
        // Any other intersecting leaf makes the visible backdrop unknown.
        if geometry.visual.iter().enumerate().any(|(index, visual)| {
            index != text.visual_index && text.quad.bounds.intersects(visual.bounds)
        }) {
            continue;
        }
        let ratio = contrast_ratio(color, background);
        if ratio < 4.5 {
            push_issue(
                issues,
                page,
                Some(&text.name),
                "text_low_contrast",
                format!(
                    "Text contrast is {ratio:.1}:1 against the plain document background. Review it at the intended size."
                ),
            );
        }
        if issues.len() >= MAX_ISSUES {
            return;
        }
    }
}

fn push_visual(geometry: &mut Geometry, quad: Quad) {
    if geometry.visual.len() < MAX_VISUAL_OBJECTS {
        geometry.visual.push(quad);
    } else {
        geometry.visual_complete = false;
    }
}

fn push_issue(
    issues: &mut Vec<PreflightIssue>,
    page: &str,
    layer: Option<&str>,
    code: &str,
    detail: String,
) {
    if issues.len() < MAX_ISSUES {
        issues.push(PreflightIssue {
            page: page.into(),
            layer: layer.map(str::to_owned),
            code: code.into(),
            detail,
        });
    }
}

fn is_adjustment(layer: &Layer) -> bool {
    layer
        .metadata
        .get("adjustment")
        .is_some_and(|value| !value.is_null())
}

fn mask_enabled(layer: &Layer) -> bool {
    layer
        .metadata
        .get("maskEnabled")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
}

fn visually_nonempty(image: &crate::shared_image::SharedImage) -> bool {
    u64::from(image.width()) * u64::from(image.height()) > ALPHA_SCAN_PIXELS
        || image.pixels().any(|pixel| pixel[3] != 0)
}

fn contrast_color(
    layer: &Layer,
    style: &objects::LiveTextStyle,
    opacity: f32,
    masked: bool,
) -> Option<[f32; 3]> {
    // Rich runs can supply different colours. Do not invent one ratio for them.
    if style.runs.iter().any(|run| run.color.is_some())
        || masked
        || layer.blend_mode != "Normal"
        || layer
            .metadata
            .get("effects")
            .is_some_and(|effects| !effects.is_null())
        || (opacity - 1.).abs() > f32::EPSILON
    {
        return None;
    }
    [style.red, style.green, style.blue]
        .into_iter()
        .all(|component| component.is_finite() && (0. ..=1.).contains(&component))
        .then_some([style.red, style.green, style.blue])
}

fn contrast_ratio(left: [f32; 3], right: [f32; 3]) -> f32 {
    fn luminance(color: [f32; 3]) -> f32 {
        fn linear(component: f32) -> f32 {
            if component <= 0.04045 {
                component / 12.92
            } else {
                ((component + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
    }
    let (left, right) = (luminance(left), luminance(right));
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn quads_overlap(left: Quad, right: Quad, tolerance: f32) -> bool {
    for quad in [left, right] {
        for index in 0..4 {
            let start = quad.points[index];
            let end = quad.points[(index + 1) % 4];
            let (axis_x, axis_y) = (start.y - end.y, end.x - start.x);
            let length = axis_x.hypot(axis_y);
            if !length.is_finite() || length <= f32::EPSILON {
                return false;
            }
            let (axis_x, axis_y) = (axis_x / length, axis_y / length);
            let project = |quad: Quad| {
                quad.points
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), point| {
                        let value = point.x * axis_x + point.y * axis_y;
                        (min.min(value), max.max(value))
                    })
            };
            let (left_min, left_max) = project(left);
            let (right_min, right_max) = project(right);
            if left_max.min(right_max) - left_min.max(right_min) <= tolerance {
                return false;
            }
        }
    }
    true
}

fn quad_distance(left: Quad, right: Quad) -> f32 {
    if quads_overlap(left, right, 0.) || quads_edges_intersect(left, right) {
        return 0.;
    }
    let mut distance = f32::INFINITY;
    for point in left.points {
        for index in 0..4 {
            distance = distance.min(point_segment_distance(
                point,
                right.points[index],
                right.points[(index + 1) % 4],
            ));
        }
    }
    for point in right.points {
        for index in 0..4 {
            distance = distance.min(point_segment_distance(
                point,
                left.points[index],
                left.points[(index + 1) % 4],
            ));
        }
    }
    distance
}

fn bounds_distance(left: Bounds, right: Bounds) -> f32 {
    let dx = if left.right < right.left {
        right.left - left.right
    } else if right.right < left.left {
        left.left - right.right
    } else {
        0.
    };
    let dy = if left.bottom < right.top {
        right.top - left.bottom
    } else if right.bottom < left.top {
        left.top - right.bottom
    } else {
        0.
    };
    dx.hypot(dy)
}

fn quads_edges_intersect(left: Quad, right: Quad) -> bool {
    (0..4).any(|li| {
        (0..4).any(|ri| {
            segments_intersect(
                left.points[li],
                left.points[(li + 1) % 4],
                right.points[ri],
                right.points[(ri + 1) % 4],
            )
        })
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    fn cross(a: Point, b: Point, c: Point) -> f32 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    fn on_segment(a: Point, b: Point, point: Point) -> bool {
        point.x >= a.x.min(b.x) - f32::EPSILON
            && point.x <= a.x.max(b.x) + f32::EPSILON
            && point.y >= a.y.min(b.y) - f32::EPSILON
            && point.y <= a.y.max(b.y) + f32::EPSILON
    }
    let (ab_c, ab_d) = (cross(a, b, c), cross(a, b, d));
    let (cd_a, cd_b) = (cross(c, d, a), cross(c, d, b));
    ((ab_c > 0. && ab_d < 0. || ab_c < 0. && ab_d > 0.)
        && (cd_a > 0. && cd_b < 0. || cd_a < 0. && cd_b > 0.))
        || (ab_c.abs() <= f32::EPSILON && on_segment(a, b, c))
        || (ab_d.abs() <= f32::EPSILON && on_segment(a, b, d))
        || (cd_a.abs() <= f32::EPSILON && on_segment(c, d, a))
        || (cd_b.abs() <= f32::EPSILON && on_segment(c, d, b))
}

fn point_segment_distance(point: Point, start: Point, end: Point) -> f32 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let length = dx * dx + dy * dy;
    if length <= f32::EPSILON {
        return (point.x - start.x).hypot(point.y - start.y);
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length).clamp(0., 1.);
    (point.x - (start.x + t * dx)).hypot(point.y - (start.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn text_layer(name: &str, x: f32, y: f32, width: f32, height: f32, color: [f32; 3]) -> Layer {
        let mut layer = Layer::paint(name, 1, 1);
        layer.offset_x = x;
        layer.offset_y = y;
        objects::set_live_text(
            &mut layer,
            objects::LiveTextStyle {
                content: "Readable text".into(),
                font_name: "sans-serif".into(),
                font_size: 16.,
                red: color[0],
                green: color[1],
                blue: color[2],
                box_size: Some(objects::ObjectSize { width, height }),
                ..Default::default()
            },
        )
        .unwrap();
        layer
    }

    fn issue_codes(document: &Document) -> Vec<String> {
        let mut issues = vec![];
        inspect_document(document, "Page", SafeAreaPreset::CanvasMargin, &mut issues).unwrap();
        issues.into_iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn preflight_reports_overflow_missing_fonts_and_hidden_layers_are_ignored() {
        let mut doc = Document::new(120, 100);
        let mut layer = Layer::paint("Headline", 1, 1);
        objects::set_live_text(
            &mut layer,
            objects::LiveTextStyle {
                content: "A long headline that needs room".into(),
                font_name: "Omuse deliberately absent fixture font".into(),
                font_size: 42.,
                // Tall enough to show the first line with any host's fallback
                // font (preflight skips fully transparent layers), yet far
                // shorter than the wrapped headline.
                box_size: Some(objects::ObjectSize {
                    width: 80.,
                    height: 72.,
                }),
                ..Default::default()
            },
        )
        .unwrap();
        doc.layers.push(layer);
        let codes = issue_codes(&doc);
        assert!(codes.iter().any(|code| code == "text_overflow"));
        assert!(codes.iter().any(|code| code == "missing_font"));
        doc.layers.last_mut().unwrap().visible = false;
        let codes = issue_codes(&doc);
        assert!(codes.iter().all(|code| code != "text_overflow"));
        assert!(codes.iter().all(|code| code != "missing_font"));
    }

    #[test]
    fn nested_rotation_and_flip_match_reference_raster_bounds() {
        let mut doc = Document::new(160, 160);
        doc.layers.clear();
        doc.background = [255, 255, 255, 255];

        let mut folder = Layer::group("Pass-through folder");
        folder.offset_x = 500.;
        folder.offset_y = 500.;
        folder.scale_x = -2.;
        folder.scale_y = 2.;
        folder.rotation = 135.;

        let mut child = text_layer("Nested", 25., 25., 30., 20., [0., 0., 0.]);
        child.scale_x = -1.25;
        child.scale_y = 1.5;
        child.rotation = 25.;
        folder.children.push(child);
        doc.layers.push(folder);

        let codes = issue_codes(&doc);
        assert!(!codes.iter().any(|code| code == "text_off_canvas"));
        assert!(!codes.iter().any(|code| code == "text_outside_guide"));
    }

    #[test]
    fn preflight_warns_for_overlapping_and_tightly_spaced_text() {
        let mut doc = Document::new(240, 180);
        doc.layers.clear();
        doc.background = [255, 255, 255, 255];
        doc.layers
            .push(text_layer("Heading", 32., 32., 100., 30., [0., 0., 0.]));
        doc.layers
            .push(text_layer("Overlay", 80., 42., 100., 30., [0., 0., 0.]));
        doc.layers
            .push(text_layer("Caption", 32., 90., 80., 20., [0., 0., 0.]));
        doc.layers
            .push(text_layer("Credit", 32., 116., 80., 20., [0., 0., 0.]));

        let codes = issue_codes(&doc);
        assert!(codes.iter().any(|code| code == "text_overlap"));
        assert!(codes.iter().any(|code| code == "text_tight_spacing"));
    }

    #[test]
    fn contrast_is_reported_only_against_a_known_plain_background() {
        let mut doc = Document::new(180, 120);
        doc.layers.clear();
        doc.background = [255, 255, 255, 255];
        doc.layers.push(text_layer(
            "Low contrast",
            20.,
            25.,
            100.,
            30.,
            [0.7, 0.7, 0.7],
        ));
        assert!(
            issue_codes(&doc)
                .iter()
                .any(|code| code == "text_low_contrast")
        );

        let mut unknown = Layer::paint("Photograph", 1, 1);
        unknown.image = Some(RgbaImage::from_pixel(180, 120, Rgba([30, 40, 50, 255])).into());
        doc.layers.push(unknown);
        assert!(
            !issue_codes(&doc)
                .iter()
                .any(|code| code == "text_low_contrast")
        );

        let mut with_effect = Document::new(180, 120);
        with_effect.layers.clear();
        with_effect.background = [255, 255, 255, 255];
        let mut styled = text_layer("Effect", 20., 25., 100., 30., [0.7, 0.7, 0.7]);
        styled.metadata["effects"] = serde_json::json!({});
        with_effect.layers.push(styled);
        assert!(
            !issue_codes(&with_effect)
                .iter()
                .any(|code| code == "text_low_contrast")
        );
    }

    #[test]
    fn rich_text_colour_runs_do_not_claim_a_single_contrast_ratio() {
        let mut doc = Document::new(180, 120);
        doc.layers.clear();
        doc.background = [255, 255, 255, 255];
        let mut text = text_layer("Mixed", 20., 25., 100., 30., [0.7, 0.7, 0.7]);
        let mut style = objects::live_text(&text).unwrap().unwrap();
        style.runs.push(objects::RichTextRun {
            start: 0,
            end: 5,
            font_name: None,
            font_size: None,
            weight: None,
            italic: None,
            color: Some([0.1, 0.1, 0.1, 1.]),
        });
        objects::set_live_text(&mut text, style).unwrap();
        doc.layers.push(text);

        assert!(
            !issue_codes(&doc)
                .iter()
                .any(|code| code == "text_low_contrast")
        );
    }
}
