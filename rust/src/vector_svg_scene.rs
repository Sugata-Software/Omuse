//! Bounded SVG exchange for multi-object vector scenes.
//!
//! Imported transforms are flattened into each object's path.  This keeps the
//! scene representation deterministic while retaining the SVG group ancestry
//! needed to reconstruct editable organization.

use crate::{
    vector_path::{Anchor, FillRule, Point, StrokeStyle, Subpath, VectorPath},
    vector_scene::{
        GradientFill, GradientKind, GradientSpread, GradientStop, StrokeCap, StrokeJoin,
        StrokeOptions, VectorGroup, VectorObject, VectorScene, style,
    },
    vector_svg,
};
use anyhow::{Context, Result, bail, ensure};
use resvg::{tiny_skia, usvg};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = MAX_INPUT_BYTES;
const MAX_XML_DEPTH: usize = 48;
const MAX_TEXT_CHARACTERS: usize = 4_096;

/// Appearance conversions are explicit: outlined text and strokes remain
/// editable geometry, but are no longer live text or width-controlled strokes.
#[derive(Debug)]
pub struct ImportedScene {
    pub scene: VectorScene,
    pub warnings: Vec<String>,
}

pub fn import_scene(path: &Path) -> Result<VectorScene> {
    decode_scene(&vector_svg::read_bounded(path)?)
}

pub fn decode_scene(input: &[u8]) -> Result<VectorScene> {
    Ok(decode_scene_with_report(input)?.scene)
}

pub fn import_scene_with_report(path: &Path) -> Result<ImportedScene> {
    decode_scene_with_report(&vector_svg::read_bounded(path)?)
}

pub fn decode_scene_with_report(input: &[u8]) -> Result<ImportedScene> {
    ensure!(
        input.len() <= MAX_INPUT_BYTES,
        "Editable SVG input exceeds 4 MiB"
    );
    let text = std::str::from_utf8(input).context("Editable SVG must be UTF-8 XML")?;
    // Reject external resources and unsupported compositing before parsing.
    // Text and stroke conversion are bounded and reported to the caller.
    scene_preflight(text)?;
    let (normalized, hidden_ids) = normalize_hidden(text)?;
    let group_names = group_names(&normalized)?;
    let shape_ids = shape_ids(&normalized)?;
    let font_notices = Arc::new(Mutex::new(Vec::new()));
    let options = if normalized.contains("<text ") {
        text_options(font_notices.clone())
    } else {
        usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_str(&normalized, &options).context("Cannot parse editable SVG scene")?;
    let width = dimension(tree.size().width(), "width")?;
    let height = dimension(tree.size().height(), "height")?;
    let mut objects = Vec::new();
    let mut group_stack = Vec::new();
    let mut generated = 0usize;
    let mut warnings = Vec::new();
    collect_nodes(
        tree.root(),
        &group_names,
        &shape_ids,
        &hidden_ids,
        1.0,
        false,
        true,
        &mut group_stack,
        &mut objects,
        &mut generated,
        tiny_skia::Transform::identity(),
        &mut warnings,
    )?;
    for notice in font_notices.lock().unwrap().iter() {
        warning(&mut warnings, notice.clone());
    }
    ensure!(
        !objects.is_empty(),
        "Editable SVG scene has no painted paths"
    );
    let scene = VectorScene {
        version: if objects
            .iter()
            .any(|object| object.fill_gradient.is_some() || object.stroke_options.is_some())
        {
            3
        } else if objects.iter().any(|object| !object.groups.is_empty()) {
            2
        } else {
            1
        },
        width,
        height,
        objects,
    };
    scene.validate()?;
    Ok(ImportedScene { scene, warnings })
}

fn warning(warnings: &mut Vec<String>, message: String) {
    if !warnings.contains(&message) && warnings.len() < 64 {
        warnings.push(message);
    }
}

fn text_options(notices: Arc<Mutex<Vec<String>>>) -> usvg::Options<'static> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            db.load_font_data(include_bytes!("../assets/fonts/Outfit.ttf").to_vec());
            db.set_sans_serif_family("Outfit");
            db.set_serif_family("Outfit");
            Arc::new(db)
        })
        .clone();
    let selector = usvg::FontResolver::default_font_selector();
    let mut options = usvg::Options {
        fontdb,
        font_family: "Outfit".into(),
        ..Default::default()
    };
    options.font_resolver.select_font = Box::new(move |font, db| {
        let selected = selector(font, db);
        if let Some(face) = selected.and_then(|id| db.face(id)) {
            let requested = font
                .families()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let resolved = face.families.first().map_or("Unknown", |f| f.0.as_str());
            warning(
                &mut notices.lock().unwrap(),
                format!(
                    "SVG font {requested} resolved to {resolved}. Outlines use the fonts available on this machine."
                ),
            );
        }
        selected
    });
    options
}

pub fn encode_scene(scene: &VectorScene) -> Result<String> {
    scene.validate()?;
    let mut output = String::new();
    push(&mut output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")?;
    push(
        &mut output,
        &format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
            scene.width, scene.height, scene.width, scene.height
        ),
    )?;
    for object in &scene.objects {
        if let Some(gradient) = &object.fill_gradient {
            write_gradient(&mut output, &object.id, gradient)?;
        }
    }
    let mut open = 0usize;
    let mut previous_groups: Vec<VectorGroup> = Vec::new();
    for object in &scene.objects {
        let common = object
            .groups
            .iter()
            .take(open)
            .zip(previous_groups.iter())
            .take_while(|(left, right)| left == right)
            .count();
        while open > common {
            push(&mut output, "</g>\n")?;
            open -= 1;
        }
        previous_groups.clone_from(&object.groups);
        for group in &object.groups[open..] {
            push(
                &mut output,
                &format!(
                    "<g id=\"{}\" data-name=\"{}\">\n",
                    xml(&group.id),
                    xml(&group.name)
                ),
            )?;
            open += 1;
        }
        push(&mut output, "  <path")?;
        push(
            &mut output,
            &format!(
                " id=\"{}\" data-name=\"{}\"",
                xml(&object.id),
                xml(&object.name)
            ),
        )?;
        if object.transform != [1., 0., 0., 1., 0., 0.] {
            let [a, b, c, d, e, f] = object.transform;
            push(
                &mut output,
                &format!(
                    " transform=\"matrix({} {} {} {} {} {})\"",
                    number(a),
                    number(b),
                    number(c),
                    number(d),
                    number(e),
                    number(f)
                ),
            )?;
        }
        push(&mut output, " d=\"")?;
        write_path(&mut output, &object.path)?;
        push(&mut output, "\"")?;
        write_fill_stroke(&mut output, object)?;
        if object.opacity < 1.0 {
            push(
                &mut output,
                &format!(" opacity=\"{}\"", number(object.opacity)),
            )?;
        }
        if !object.visible {
            push(&mut output, " visibility=\"hidden\"")?;
        }
        push(&mut output, "/>\n")?;
    }
    while open > 0 {
        push(&mut output, "</g>\n")?;
        open -= 1;
    }
    push(&mut output, "</svg>\n")?;
    Ok(output)
}

pub fn prepare_scene_export(
    path: &Path,
    scene: &VectorScene,
) -> Result<vector_svg::PreparedExport> {
    let encoded = encode_scene(scene)?;
    vector_svg::prepare_encoded_export(path, &encoded)
}

fn collect_nodes(
    group: &usvg::Group,
    group_names: &HashMap<String, String>,
    shape_ids: &std::collections::HashSet<String>,
    hidden_ids: &std::collections::HashSet<String>,
    inherited_opacity: f32,
    hidden_ancestor: bool,
    is_root: bool,
    ancestry: &mut Vec<VectorGroup>,
    output: &mut Vec<VectorObject>,
    generated: &mut usize,
    extra_transform: tiny_skia::Transform,
    warnings: &mut Vec<String>,
) -> Result<()> {
    ensure!(
        group.blend_mode() == usvg::BlendMode::Normal
            && !group.isolate()
            && group.clip_path().is_none()
            && group.mask().is_none()
            && group.filters().is_empty(),
        "SVG group compositing and clipping are unsupported"
    );
    let shape_wrapper = group.children().len() == 1
        && matches!(&group.children()[0], usvg::Node::Path(path) if
            path.id() == group.id()
                || (group.id().is_empty() && shape_ids.contains(path.id()))
                || (path.id().is_empty() && shape_ids.contains(group.id())));
    // The normalization pass resolves visibility inheritance per shape. A
    // visible child can override a group's visibility:hidden (unlike display).
    let group_hidden = hidden_ancestor || (shape_wrapper && hidden_ids.contains(group.id()));
    let group_opacity = group.opacity().get();
    ensure!(
        group_opacity.is_finite() && (0.0..=1.0).contains(&group_opacity),
        "invalid SVG group opacity"
    );
    ensure!(
        group_opacity == 1.0 || painted_paths(group) <= 1,
        "SVG opacity over multiple painted objects cannot be preserved as independent editable objects"
    );
    let object_opacity = inherited_opacity * group_opacity;
    let pushed = if shape_wrapper || (group.id().is_empty() && is_root) {
        None
    } else {
        let name = group_names
            .get(group.id())
            .cloned()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Group {}", ancestry.len() + 1));
        ancestry.push(VectorGroup {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            name,
        });
        Some(())
    };
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_nodes(
                child,
                group_names,
                shape_ids,
                hidden_ids,
                object_opacity,
                group_hidden,
                false,
                ancestry,
                output,
                generated,
                extra_transform,
                warnings,
            )?,
            usvg::Node::Path(path) => {
                let transform = extra_transform.pre_concat(path.abs_transform());
                ensure!(
                    path.paint_order() == usvg::PaintOrder::FillAndStroke,
                    "SVG path paint order is unsupported"
                );
                let fill_gradient = path
                    .fill()
                    .map(|f| gradient_fill(f, transform))
                    .transpose()?
                    .flatten();
                let fill = if let Some(gradient) = &fill_gradient {
                    Some(gradient.stops[0].color)
                } else {
                    path.fill().map(solid_fill).transpose()?
                };
                let expand_stroke = path.stroke().is_some_and(|stroke| {
                    !matches!(stroke.paint(), usvg::Paint::Color(_))
                        || uniform_scale(transform).is_err()
                });
                if expand_stroke {
                    ensure!(
                        fill.is_none() || object_opacity == 1.,
                        "SVG fill and expanded stroke with object opacity cannot be separated faithfully"
                    );
                }
                let stroke_options = path
                    .stroke()
                    .filter(|_| !expand_stroke)
                    .map(|s| stroke_options(s, transform))
                    .transpose()?
                    .flatten();
                let stroke = path
                    .stroke()
                    .filter(|_| !expand_stroke)
                    .map(|s| solid_stroke(s, transform))
                    .transpose()?;
                ensure!(
                    fill.is_some() || stroke.is_some() || expand_stroke,
                    "SVG path must have a solid fill or stroke"
                );
                let mut geometry = convert_path(path.data(), transform)?;
                geometry.fill_rule = path.fill().map_or(FillRule::NonZero, |f| match f.rule() {
                    usvg::FillRule::NonZero => FillRule::NonZero,
                    usvg::FillRule::EvenOdd => FillRule::EvenOdd,
                });
                geometry.validate()?;
                let name = group_names
                    .get(path.id())
                    .cloned()
                    .unwrap_or_else(|| path.id().to_owned())
                    .if_empty_then(|| format!("Path {}", output.len() + 1));
                let visible = path.is_visible() && !group_hidden && !hidden_ids.contains(path.id());
                if fill.is_some() || stroke.is_some() {
                    *generated += geometry
                        .subpaths
                        .iter()
                        .map(|s| s.anchors.len())
                        .sum::<usize>();
                    ensure!(
                        *generated <= crate::vector_scene::MAX_SCENE_ANCHORS,
                        "SVG exceeds the editable anchor limit"
                    );
                    let id = uuid::Uuid::new_v4().to_string().to_uppercase();
                    output.push(VectorObject {
                        id,
                        name: name.clone(),
                        path: geometry,
                        transform: [1., 0., 0., 1., 0., 0.],
                        fill,
                        stroke,
                        fill_gradient,
                        stroke_options,
                        text_path: None,
                        opacity: object_opacity,
                        visible,
                        groups: ancestry.clone(),
                    });
                }
                if expand_stroke {
                    let outline =
                        expanded_stroke(path, transform, name, object_opacity, visible, ancestry)?;
                    *generated += outline
                        .path
                        .subpaths
                        .iter()
                        .map(|s| s.anchors.len())
                        .sum::<usize>();
                    ensure!(
                        *generated <= crate::vector_scene::MAX_SCENE_ANCHORS,
                        "SVG exceeds the editable anchor limit"
                    );
                    output.push(outline);
                    warning(warnings, "SVG gradient or nonuniformly transformed strokes were converted to editable filled outlines; stroke width and dash settings are no longer live.".into());
                }
                ensure!(
                    output.len() <= crate::vector_scene::MAX_SCENE_OBJECTS,
                    "SVG exceeds the editable object limit"
                );
            }
            usvg::Node::Image(_) => {
                bail!("Embedded bitmap content is unsupported for editable scene import")
            }
            usvg::Node::Text(text) => {
                ensure!(
                    text.layouted()
                        .iter()
                        .flat_map(|span| &span.positioned_glyphs)
                        .all(|glyph| glyph.id.0 != 0),
                    "SVG text contains glyphs unavailable in installed fonts; choose another font or supply outlined SVG"
                );
                ensure!(
                    text.flattened()
                        .children()
                        .iter()
                        .all(|child| matches!(child, usvg::Node::Path(_))),
                    "Colour, bitmap and SVG-in-font glyphs are unsupported for editable SVG text; supply outlined SVG"
                );
                let first = output.len();
                collect_nodes(
                    text.flattened(),
                    group_names,
                    shape_ids,
                    hidden_ids,
                    object_opacity,
                    group_hidden || hidden_ids.contains(text.id()),
                    false,
                    ancestry,
                    output,
                    generated,
                    extra_transform.pre_concat(text.abs_transform()),
                    warnings,
                )?;
                for object in &mut output[first..] {
                    object.name = format!(
                        "{} · outlines",
                        group_names.get(text.id()).map_or(text.id(), String::as_str)
                    );
                }
                warning(warnings, "SVG text was converted to editable outlines. Characters, font settings and text flow are not retained as live text.".into());
            }
        }
    }
    if pushed.is_some() {
        ancestry.pop();
    }
    Ok(())
}

fn painted_paths(group: &usvg::Group) -> usize {
    let mut count = 0;
    for node in group.children() {
        count += match node {
            usvg::Node::Group(group) => painted_paths(group),
            usvg::Node::Text(text) => painted_paths(text.flattened()),
            _ => 1,
        };
        if count >= 2 {
            return 2;
        }
    }
    count
}

fn expanded_stroke(
    path: &usvg::Path,
    transform: tiny_skia::Transform,
    name: String,
    opacity: f32,
    visible: bool,
    groups: &[VectorGroup],
) -> Result<VectorObject> {
    let stroke = path.stroke().context("Missing SVG stroke")?;
    let identity = tiny_skia::Transform::identity();
    let options = stroke_options(stroke, identity)?.unwrap_or_default();
    let resolution = transform
        .sx
        .hypot(transform.ky)
        .max(transform.kx.hypot(transform.sy))
        .max(1.);
    ensure!(
        resolution.is_finite() && resolution <= 64.,
        "SVG stroke expansion supports transforms up to 64 times source size"
    );
    ensure!(
        stroke.width().get() <= 100_000.,
        "SVG stroke width exceeds the editable limit"
    );
    let mut segments = 0usize;
    let mut length = 0f64;
    let mut previous = tiny_skia::Point::from_xy(0., 0.);
    let mut start = previous;
    for segment in path.data().segments() {
        segments += 1;
        ensure!(
            segments <= 4_096,
            "SVG stroke expansion exceeds the input segment limit"
        );
        let mut visit = |point: tiny_skia::Point| {
            length += f64::from(point.x - previous.x).hypot(f64::from(point.y - previous.y));
            previous = point;
        };
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => {
                previous = p;
                start = p;
            }
            tiny_skia::PathSegment::LineTo(p) => visit(p),
            tiny_skia::PathSegment::QuadTo(c, p) => {
                visit(c);
                visit(p);
            }
            tiny_skia::PathSegment::CubicTo(a, b, p) => {
                visit(a);
                visit(b);
                visit(p);
            }
            tiny_skia::PathSegment::Close => visit(start),
        }
    }
    let mut pen = options.stroke(stroke.width().get(), 1.);
    let dashed;
    let source = if let Some(dash) = pen.dash.take() {
        let smallest = options.dashes.iter().copied().fold(f32::INFINITY, f32::min);
        ensure!(
            length / f64::from(smallest) + segments as f64 <= 4_096.,
            "SVG stroke expansion exceeds the dash work limit"
        );
        dashed = path
            .data()
            .dash(&dash, resolution)
            .context("Cannot expand SVG stroke dashes")?;
        &dashed
    } else {
        path.data()
    };
    let outline = source
        .stroke(&pen, resolution)
        .context("Cannot expand SVG stroke geometry")?;
    let geometry = convert_path(&outline, transform)?;
    geometry.validate()?;
    let gradient = gradient_paint(stroke.paint(), stroke.opacity(), transform)?;
    let fill = if let Some(gradient) = &gradient {
        gradient.stops[0].color
    } else {
        color(stroke.paint(), stroke.opacity(), "stroke")?
    };
    Ok(VectorObject {
        id: uuid::Uuid::new_v4().to_string().to_uppercase(),
        name: format!("{name} · stroke outlines"),
        path: geometry,
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some(fill),
        stroke: None,
        fill_gradient: gradient,
        stroke_options: None,
        text_path: None,
        opacity,
        visible,
        groups: groups.to_vec(),
    })
}

fn scene_preflight(text: &str) -> Result<()> {
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 12_000,
            ..Default::default()
        },
    )
    .context("Invalid or excessive editable SVG XML")?;
    ensure!(
        doc.root_element().tag_name().name() == "svg"
            && doc.root_element().tag_name().namespace() == Some("http://www.w3.org/2000/svg"),
        "Expected an SVG document with the SVG namespace"
    );
    let text_characters = doc
        .descendants()
        .filter(|node| node.is_text() && node.ancestors().any(|parent| parent.has_tag_name("text")))
        .map(|node| node.text().unwrap_or_default().chars().count())
        .sum::<usize>();
    ensure!(
        text_characters <= MAX_TEXT_CHARACTERS,
        "Editable SVG text exceeds 4096 characters"
    );
    ensure!(
        doc.descendants()
            .filter(|node| node.has_tag_name("text"))
            .count()
            <= 128,
        "Editable SVG exceeds 128 text elements"
    );
    for node in doc.descendants().filter(|node| node.is_element()) {
        ensure!(
            node.ancestors().count() <= MAX_XML_DEPTH,
            "Editable SVG nesting exceeds 48 levels"
        );
        ensure!(
            node.tag_name().namespace() == Some("http://www.w3.org/2000/svg"),
            "Foreign XML namespaces are unsupported"
        );
        let tag = node.tag_name().name();
        ensure!(
            tag != "svg" || node == doc.root_element(),
            "Nested SVG viewports are unsupported"
        );
        ensure!(
            matches!(
                tag,
                "svg"
                    | "g"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "title"
                    | "desc"
                    | "metadata"
                    | "defs"
                    | "linearGradient"
                    | "radialGradient"
                    | "stop"
                    | "text"
                    | "tspan"
            ),
            "Unsupported SVG element <{tag}>"
        );
        for attribute in node.attributes() {
            let name = attribute.name();
            let value = attribute.value().to_ascii_lowercase();
            ensure!(
                (attribute.namespace().is_none()
                    || (attribute.namespace() == Some("http://www.w3.org/XML/1998/namespace")
                        && name == "space"))
                    && name != "href"
                    && name != "vector-effect",
                "SVG resource references are unsupported"
            );
            ensure!(
                !matches!(
                    name,
                    "mix-blend-mode" | "isolation" | "clip-path" | "mask" | "filter"
                ),
                "SVG compositing and clipping are unsupported"
            );
            ensure!(
                !value.contains("@import"),
                "SVG resource references are unsupported"
            );
            if value.contains("url(") {
                ensure!(
                    matches!(name, "fill" | "stroke" | "style"),
                    "Only local fill or stroke gradient references are supported"
                );
                for reference in attribute.value().split("url(").skip(1) {
                    let reference = reference
                        .split(')')
                        .next()
                        .context("Invalid gradient reference")?
                        .trim();
                    let id = reference
                        .strip_prefix('#')
                        .context("External gradient references are unsupported")?;
                    ensure!(
                        !id.is_empty()
                            && doc
                                .descendants()
                                .any(|target| target.attribute("id") == Some(id)
                                    && matches!(
                                        target.tag_name().name(),
                                        "linearGradient" | "radialGradient"
                                    )),
                        "Gradient reference must name a local linear or radial gradient"
                    );
                }
                ensure!(
                    !value.contains("url (") && attribute.value().contains("url("),
                    "Invalid gradient reference syntax"
                );
            }
            if name == "color-interpolation" {
                ensure!(
                    value == "srgb",
                    "Only sRGB gradient interpolation is supported"
                );
            }
            ensure!(
                !matches!(
                    name,
                    "inline-size"
                        | "shape-inside"
                        | "shape-subtract"
                        | "font-feature-settings"
                        | "font-variation-settings"
                ),
                "SVG text layout feature is unsupported; supply outlined text"
            );
            if tag == "tspan" {
                ensure!(
                    !matches!(name, "visibility" | "display" | "opacity"),
                    "Per-span SVG visibility or opacity is unsupported; split the text into separate elements"
                );
            }
            if name == "style" {
                for declaration in value.split(';').filter(|part| !part.trim().is_empty()) {
                    let property = declaration
                        .split_once(':')
                        .map_or(declaration, |(property, _)| property)
                        .trim();
                    ensure!(
                        matches!(
                            property,
                            "fill"
                                | "fill-opacity"
                                | "fill-rule"
                                | "stroke"
                                | "stroke-opacity"
                                | "stroke-width"
                                | "stroke-linecap"
                                | "stroke-linejoin"
                                | "stroke-miterlimit"
                                | "stroke-dasharray"
                                | "stroke-dashoffset"
                                | "stop-color"
                                | "stop-opacity"
                                | "paint-order"
                                | "opacity"
                                | "display"
                                | "visibility"
                                | "transform"
                                | "font-family"
                                | "font-size"
                                | "font-style"
                                | "font-weight"
                                | "font-stretch"
                                | "font-variant"
                                | "letter-spacing"
                                | "word-spacing"
                                | "text-anchor"
                                | "text-decoration"
                                | "text-rendering"
                                | "dominant-baseline"
                                | "alignment-baseline"
                                | "baseline-shift"
                                | "writing-mode"
                        ),
                        "Unsupported SVG CSS property"
                    );
                    ensure!(
                        tag != "tspan" || !matches!(property, "visibility" | "display" | "opacity"),
                        "Per-span SVG visibility or opacity is unsupported; split the text into separate elements"
                    );
                }
            }
        }
    }
    Ok(())
}

fn group_names(text: &str) -> Result<HashMap<String, String>> {
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 12_000,
            ..Default::default()
        },
    )?;
    Ok(doc
        .descendants()
        .filter(|node| {
            node.is_element()
                && matches!(
                    node.tag_name().name(),
                    "g" | "path"
                        | "rect"
                        | "circle"
                        | "ellipse"
                        | "line"
                        | "polyline"
                        | "polygon"
                        | "text"
                )
        })
        .filter_map(|node| {
            let id = node.attribute("id")?.to_owned();
            let name = node.attribute("data-name").unwrap_or(&id).to_owned();
            Some((id, name))
        })
        .collect())
}

fn shape_ids(text: &str) -> Result<std::collections::HashSet<String>> {
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 12_000,
            ..Default::default()
        },
    )?;
    Ok(doc
        .descendants()
        .filter(|node| {
            node.is_element()
                && matches!(
                    node.tag_name().name(),
                    "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon"
                )
        })
        .filter_map(|node| node.attribute("id").map(str::to_owned))
        .collect())
}

/// Produce a deliberately small, normalized XML copy for usvg.
///
/// usvg is allowed to optimize away `display:none` nodes and anonymous groups.
/// We retain the original visibility in `hidden_ids`, make those nodes paintable
/// in the temporary tree, and assign stable IDs to otherwise anonymous paint
/// nodes.  This uses roxmltree's parsed attributes rather than textual replaces,
/// so quoting and whitespace in the source are immaterial.
fn normalize_hidden(text: &str) -> Result<(String, std::collections::HashSet<String>)> {
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 12_000,
            ..Default::default()
        },
    )?;
    let mut normalized = String::with_capacity(text.len());
    let mut hidden_ids = std::collections::HashSet::new();
    let mut generated = 0usize;
    let mut used_ids = std::collections::HashSet::new();
    for node in doc.descendants().filter(|node| node.is_element()) {
        if let Some(id) = node.attribute("id") {
            ensure!(used_ids.insert(id.to_owned()), "SVG contains duplicate IDs");
        }
    }
    serialize_svg_node(
        doc.root_element(),
        &mut normalized,
        &mut hidden_ids,
        &mut generated,
        &mut used_ids,
        false,
        false,
    )?;
    ensure!(
        normalized.len() <= MAX_INPUT_BYTES,
        "Editable SVG input exceeds 4 MiB"
    );
    Ok((normalized, hidden_ids))
}

fn serialize_svg_node(
    node: roxmltree::Node<'_, '_>,
    out: &mut String,
    hidden_ids: &mut std::collections::HashSet<String>,
    generated: &mut usize,
    used_ids: &mut std::collections::HashSet<String>,
    display_hidden_ancestor: bool,
    visibility_hidden_ancestor: bool,
) -> Result<()> {
    ensure!(node.is_element(), "SVG root must be an element");
    let tag = node.tag_name().name();
    let display_hidden = node_property(node, "display").is_some_and(|value| value == "none");
    let own_visibility = node_property(node, "visibility");
    let effective_visibility_hidden = match own_visibility.as_deref() {
        Some("visible") => false,
        Some("hidden" | "collapse") => true,
        _ => visibility_hidden_ancestor,
    };
    let hidden = display_hidden_ancestor || display_hidden || effective_visibility_hidden;
    let mut id = node.attribute("id").map(str::to_owned);
    if id.is_none() && tag != "svg" {
        loop {
            *generated += 1;
            let candidate = format!("omuse-generated-node-{}", *generated);
            if used_ids.insert(candidate.clone()) {
                id = Some(candidate);
                break;
            }
        }
    }
    if hidden {
        if let Some(id) = id.as_ref() {
            hidden_ids.insert(id.clone());
        }
    }

    out.push('<');
    out.push_str(tag);
    let mut has_svg_namespace = false;
    let mut wrote_visible_style = false;
    for attribute in node.attributes() {
        let name = attribute.name();
        if name == "xmlns" {
            has_svg_namespace = true;
        }
        if hidden && matches!(name, "display" | "visibility") {
            continue;
        }
        if hidden && name == "style" {
            let retained = retained_style(attribute.value());
            if !retained.is_empty() {
                out.push_str(" style=\"");
                out.push_str(&xml(&retained));
                out.push_str(";display:inline;visibility:visible\"");
            } else {
                out.push_str(" style=\"display:inline;visibility:visible\"");
            }
            wrote_visible_style = true;
            continue;
        }
        out.push(' ');
        if attribute.namespace() == Some("http://www.w3.org/XML/1998/namespace") {
            out.push_str("xml:");
        }
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(&xml(attribute.value()));
        out.push('"');
    }
    if tag == "svg" && !has_svg_namespace {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }
    if node.attribute("id").is_none() && tag != "svg" {
        out.push_str(" id=\"");
        out.push_str(&xml(id.as_deref().unwrap_or_default()));
        out.push('"');
        if node.attribute("data-name").is_none() {
            out.push_str(&format!(
                " data-name=\"{} {}\"",
                if tag == "g" {
                    "Group"
                } else if tag == "text" {
                    "Text"
                } else {
                    "Path"
                },
                *generated
            ));
        }
    }
    if hidden && !wrote_visible_style {
        out.push_str(" style=\"display:inline;visibility:visible\"");
    }
    out.push('>');
    for child in node.children() {
        if child.is_element() {
            serialize_svg_node(
                child,
                out,
                hidden_ids,
                generated,
                used_ids,
                display_hidden_ancestor || display_hidden,
                effective_visibility_hidden,
            )?;
        } else if let Some(value) = child.text() {
            out.push_str(&xml(value));
        }
    }
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    Ok(())
}

fn node_property(node: roxmltree::Node<'_, '_>, property: &str) -> Option<String> {
    let style = node.attribute("style").and_then(|style| {
        style.split(';').find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim().eq_ignore_ascii_case(property)).then(|| value.trim().to_ascii_lowercase())
        })
    });
    style.or_else(|| {
        node.attribute(property)
            .map(|value| value.trim().to_ascii_lowercase())
    })
}

fn retained_style(style: &str) -> String {
    style
        .split(';')
        .filter(|declaration| {
            let Some((name, _)) = declaration.split_once(':') else {
                return false;
            };
            !matches!(
                name.trim().to_ascii_lowercase().as_str(),
                "display" | "visibility"
            )
        })
        .map(str::trim)
        .filter(|declaration| !declaration.is_empty())
        .collect::<Vec<_>>()
        .join(";")
}

fn solid_fill(fill: &usvg::Fill) -> Result<[u8; 4]> {
    color(fill.paint(), fill.opacity(), "fill")
}
fn solid_stroke(stroke: &usvg::Stroke, transform: tiny_skia::Transform) -> Result<StrokeStyle> {
    let scale = uniform_scale(transform)?;
    Ok(StrokeStyle {
        color: color(stroke.paint(), stroke.opacity(), "stroke")?,
        width: stroke.width().get() * scale,
    })
}
fn color(paint: &usvg::Paint, opacity: usvg::Opacity, role: &str) -> Result<[u8; 4]> {
    let usvg::Paint::Color(c) = paint else {
        bail!("SVG {role} must be a solid color")
    };
    Ok([c.red, c.green, c.blue, (opacity.get() * 255.).round() as u8])
}
fn uniform_scale(t: tiny_skia::Transform) -> Result<f32> {
    let x = t.sx.hypot(t.ky);
    let y = t.kx.hypot(t.sy);
    let dot = t.sx * t.kx + t.ky * t.sy;
    let tol = x.max(y).max(1.0) * 1e-5;
    ensure!(
        x.is_finite()
            && y.is_finite()
            && x > 0.0
            && (x - y).abs() <= tol
            && dot.abs() <= tol * x.max(y),
        "Non-uniform or skewed transforms on SVG strokes are unsupported"
    );
    Ok((x + y) * 0.5)
}
fn dimension(value: f32, label: &str) -> Result<u32> {
    ensure!(
        value.is_finite()
            && value > 0.
            && value.round() <= u32::MAX as f32
            && (value - value.round()).abs() <= 1e-4,
        "Invalid SVG {label}"
    );
    Ok(value.round() as u32)
}

pub(crate) fn convert_path(
    data: &tiny_skia::Path,
    transform: tiny_skia::Transform,
) -> Result<VectorPath> {
    let mut all = Vec::new();
    let mut current: Option<Subpath> = None;
    for segment in data.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => {
                if let Some(s) = current.take() {
                    all.push(s)
                };
                current = Some(Subpath {
                    anchors: vec![Anchor {
                        position: mapped(p, transform),
                        incoming: None,
                        outgoing: None,
                    }],
                    closed: false,
                });
            }
            tiny_skia::PathSegment::LineTo(p) => {
                current
                    .as_mut()
                    .context("SVG path segment appears before move")?
                    .anchors
                    .push(Anchor {
                        position: mapped(p, transform),
                        incoming: None,
                        outgoing: None,
                    });
            }
            tiny_skia::PathSegment::QuadTo(c, p) => {
                let s = current
                    .as_mut()
                    .context("SVG path segment appears before move")?;
                let from = s
                    .anchors
                    .last()
                    .context("SVG quadratic path has no start")?
                    .position;
                let c = mapped(c, transform);
                let to = mapped(p, transform);
                s.anchors.last_mut().unwrap().outgoing = Some(Point {
                    x: from.x + (c.x - from.x) * 2. / 3.,
                    y: from.y + (c.y - from.y) * 2. / 3.,
                });
                s.anchors.push(Anchor {
                    position: to,
                    incoming: Some(Point {
                        x: to.x + (c.x - to.x) * 2. / 3.,
                        y: to.y + (c.y - to.y) * 2. / 3.,
                    }),
                    outgoing: None,
                });
            }
            tiny_skia::PathSegment::CubicTo(c1, c2, p) => {
                let s = current
                    .as_mut()
                    .context("SVG path segment appears before move")?;
                s.anchors.last_mut().unwrap().outgoing = Some(mapped(c1, transform));
                s.anchors.push(Anchor {
                    position: mapped(p, transform),
                    incoming: Some(mapped(c2, transform)),
                    outgoing: None,
                });
            }
            tiny_skia::PathSegment::Close => {
                let mut s = current.take().context("SVG close before move")?;
                if s.anchors.len() > 1
                    && s.anchors.last().unwrap().position == s.anchors[0].position
                {
                    let closing = s.anchors.pop().unwrap();
                    // tiny-skia represents a curved closing segment as the
                    // incoming control on the repeated endpoint.  That point
                    // belongs to the first anchor after the duplicate is
                    // removed; dropping it changes the rendered loop.
                    if let Some(control) = closing.incoming {
                        s.anchors[0].incoming = Some(control);
                    }
                }
                s.closed = true;
                all.push(s);
            }
        }
    }
    if let Some(s) = current {
        all.push(s)
    };
    Ok(VectorPath {
        subpaths: all,
        fill_rule: FillRule::NonZero,
    })
}
fn mapped(mut p: tiny_skia::Point, t: tiny_skia::Transform) -> Point {
    t.map_point(&mut p);
    Point { x: p.x, y: p.y }
}

pub(crate) fn write_path(out: &mut String, path: &VectorPath) -> Result<()> {
    for sub in &path.subpaths {
        if sub.anchors.is_empty() {
            continue;
        };
        push(
            out,
            &format!(
                "M {} {}",
                number(sub.anchors[0].position.x),
                number(sub.anchors[0].position.y)
            ),
        )?;
        for i in 1..sub.anchors.len() {
            let from = &sub.anchors[i - 1];
            let to = &sub.anchors[i];
            match (from.outgoing, to.incoming) {
                (None, None) => push(
                    out,
                    &format!(" L {} {}", number(to.position.x), number(to.position.y)),
                )?,
                (a, b) => {
                    let c1 = a.unwrap_or(from.position);
                    let c2 = b.unwrap_or(to.position);
                    push(
                        out,
                        &format!(
                            " C {} {} {} {} {} {}",
                            number(c1.x),
                            number(c1.y),
                            number(c2.x),
                            number(c2.y),
                            number(to.position.x),
                            number(to.position.y)
                        ),
                    )?;
                }
            }
        }
        if sub.closed && sub.anchors.len() > 1 {
            let from = sub.anchors.last().unwrap();
            let to = &sub.anchors[0];
            match (from.outgoing, to.incoming) {
                (None, None) => {}
                (a, b) => {
                    let c1 = a.unwrap_or(from.position);
                    let c2 = b.unwrap_or(to.position);
                    push(
                        out,
                        &format!(
                            " C {} {} {} {} {} {}",
                            number(c1.x),
                            number(c1.y),
                            number(c2.x),
                            number(c2.y),
                            number(to.position.x),
                            number(to.position.y)
                        ),
                    )?;
                }
            }
        }
        if sub.closed {
            push(out, " Z")?;
        }
    }
    Ok(())
}
fn write_fill_stroke(out: &mut String, object: &VectorObject) -> Result<()> {
    if object.fill_gradient.is_some() {
        push(
            out,
            &format!(" fill=\"url(#gradient-{})\"", xml(&object.id)),
        )?;
    } else {
        match object.fill {
            Some(c) => push(
                out,
                &format!(
                    " fill=\"#{:02X}{:02X}{:02X}\" fill-opacity=\"{}\"",
                    c[0],
                    c[1],
                    c[2],
                    alpha(c[3])
                ),
            )?,
            None => push(out, " fill=\"none\"")?,
        };
    }
    match object.stroke {
        Some(s) => push(
            out,
            &format!(
                " stroke=\"#{:02X}{:02X}{:02X}\" stroke-opacity=\"{}\" stroke-width=\"{}\"",
                s.color[0],
                s.color[1],
                s.color[2],
                alpha(s.color[3]),
                number(s.width)
            ),
        )?,
        None => push(out, " stroke=\"none\"")?,
    };
    if object.stroke.is_some() {
        let options = object.stroke_options.clone().unwrap_or_default();
        let cap = match options.cap {
            StrokeCap::Butt => "butt",
            StrokeCap::Round => "round",
            StrokeCap::Square => "square",
        };
        let join = match options.join {
            StrokeJoin::Miter => "miter",
            StrokeJoin::Round => "round",
            StrokeJoin::Bevel => "bevel",
        };
        push(
            out,
            &format!(
                " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"",
                number(options.miter_limit)
            ),
        )?;
        if !options.dashes.is_empty() {
            push(
                out,
                &format!(
                    " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                    options
                        .dashes
                        .iter()
                        .map(|v| number(*v))
                        .collect::<Vec<_>>()
                        .join(" "),
                    number(options.dash_offset)
                ),
            )?;
        }
    }
    push(
        out,
        match object.path.fill_rule {
            FillRule::NonZero => " fill-rule=\"nonzero\"",
            FillRule::EvenOdd => " fill-rule=\"evenodd\"",
        },
    )
}
fn push(out: &mut String, value: &str) -> Result<()> {
    ensure!(
        out.len().saturating_add(value.len()) <= MAX_OUTPUT_BYTES,
        "Encoded editable SVG exceeds the 4 MiB reimport limit"
    );
    out.push_str(value);
    Ok(())
}
fn number(v: f32) -> String {
    if v == 0. { "0".into() } else { v.to_string() }
}
fn alpha(v: u8) -> String {
    format!("{:.6}", f32::from(v) / 255.)
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
trait EmptyThen {
    fn if_empty_then<F: FnOnce() -> String>(self, f: F) -> String;
}
impl EmptyThen for String {
    fn if_empty_then<F: FnOnce() -> String>(self, f: F) -> String {
        if self.is_empty() { f() } else { self }
    }
}

fn gradient_fill(
    fill: &usvg::Fill,
    transform: tiny_skia::Transform,
) -> Result<Option<GradientFill>> {
    gradient_paint(fill.paint(), fill.opacity(), transform)
}

fn gradient_paint(
    paint: &usvg::Paint,
    opacity: usvg::Opacity,
    transform: tiny_skia::Transform,
) -> Result<Option<GradientFill>> {
    let (base, kind): (&usvg::BaseGradient, GradientKind) = match paint {
        usvg::Paint::Color(_) => return Ok(None),
        usvg::Paint::LinearGradient(g) => (
            g,
            GradientKind::Linear {
                start: Point {
                    x: g.x1(),
                    y: g.y1(),
                },
                end: Point {
                    x: g.x2(),
                    y: g.y2(),
                },
            },
        ),
        usvg::Paint::RadialGradient(g) => (
            g,
            GradientKind::Radial {
                center: Point {
                    x: g.cx(),
                    y: g.cy(),
                },
                focus: Point {
                    x: g.fx(),
                    y: g.fy(),
                },
                radius: g.r().get(),
            },
        ),
        _ => bail!("Only solid, linear and radial fills are supported for editable SVG"),
    };
    let gradient = GradientFill {
        kind,
        stops: base
            .stops()
            .iter()
            .map(|s| GradientStop {
                offset: s.offset().get(),
                color: [
                    s.color().red,
                    s.color().green,
                    s.color().blue,
                    (s.opacity().get() * opacity.get() * 255.).round() as u8,
                ],
            })
            .collect(),
        spread: match base.spread_method() {
            usvg::SpreadMethod::Pad => GradientSpread::Pad,
            usvg::SpreadMethod::Repeat => GradientSpread::Repeat,
            usvg::SpreadMethod::Reflect => GradientSpread::Reflect,
        },
        transform: style::matrix(transform.pre_concat(base.transform())),
    };
    gradient.validate()?;
    Ok(Some(gradient))
}

fn stroke_options(
    stroke: &usvg::Stroke,
    transform: tiny_skia::Transform,
) -> Result<Option<StrokeOptions>> {
    let scale = uniform_scale(transform)?;
    let mut dashes = stroke.dasharray().map_or_else(Vec::new, |v| v.to_vec());
    if dashes.len() % 2 == 1 {
        dashes.extend(dashes.clone());
    }
    let options = StrokeOptions {
        cap: match stroke.linecap() {
            usvg::LineCap::Butt => StrokeCap::Butt,
            usvg::LineCap::Round => StrokeCap::Round,
            usvg::LineCap::Square => StrokeCap::Square,
        },
        join: match stroke.linejoin() {
            usvg::LineJoin::Miter => StrokeJoin::Miter,
            usvg::LineJoin::Round => StrokeJoin::Round,
            usvg::LineJoin::Bevel => StrokeJoin::Bevel,
            _ => bail!("SVG stroke join is unsupported"),
        },
        miter_limit: stroke.miterlimit().get(),
        dashes: dashes.into_iter().map(|v| v * scale).collect(),
        dash_offset: stroke.dashoffset() * scale,
    };
    options.validate()?;
    Ok((options != StrokeOptions::default()).then_some(options))
}

fn write_gradient(out: &mut String, id: &str, gradient: &GradientFill) -> Result<()> {
    let (tag, geometry) = match gradient.kind {
        GradientKind::Linear { start, end } => (
            "linearGradient",
            format!(
                "x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                number(start.x),
                number(start.y),
                number(end.x),
                number(end.y)
            ),
        ),
        GradientKind::Radial {
            center,
            focus,
            radius,
        } => (
            "radialGradient",
            format!(
                "cx=\"{}\" cy=\"{}\" fx=\"{}\" fy=\"{}\" r=\"{}\"",
                number(center.x),
                number(center.y),
                number(focus.x),
                number(focus.y),
                number(radius)
            ),
        ),
    };
    let spread = match gradient.spread {
        GradientSpread::Pad => "pad",
        GradientSpread::Repeat => "repeat",
        GradientSpread::Reflect => "reflect",
    };
    let transform = gradient
        .transform
        .iter()
        .map(|v| number(*v))
        .collect::<Vec<_>>()
        .join(" ");
    push(
        out,
        &format!(
            "<defs><{tag} id=\"gradient-{}\" gradientUnits=\"userSpaceOnUse\" {geometry} spreadMethod=\"{spread}\" gradientTransform=\"matrix({transform})\">",
            xml(id)
        ),
    )?;
    for stop in &gradient.stops {
        let [r, g, b, a] = stop.color;
        push(
            out,
            &format!(
                "<stop offset=\"{}\" stop-color=\"#{r:02X}{g:02X}{b:02X}\" stop-opacity=\"{}\"/>",
                number(stop.offset),
                alpha(a)
            ),
        )?;
    }
    push(out, &format!("</{tag}></defs>\n"))
}
