//! Safe SVG interchange for one editable Omuse path.
//!
//! This is deliberately not a general SVG document importer. It accepts one
//! rendered shape with solid fill and/or a uniform round stroke, normalizes
//! standard SVG shape syntax through `usvg`, and retains its cubic geometry as
//! a [`VectorPath`]. Text, images, paint servers and compositing effects are
//! rejected instead of being silently flattened.

use crate::{
    model::valid_dimensions,
    vector_path::{Anchor, FillRule, Point, StrokeStyle, Subpath, VectorPath},
};
use anyhow::{Context, Result, bail, ensure};
use resvg::{tiny_skia, usvg};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_XML_NODES: u32 = 12_000;
const MAX_XML_DEPTH: usize = 48;

/// A single editable path and its explicit SVG canvas and paint.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgArtwork {
    pub width: u32,
    pub height: u32,
    pub path: VectorPath,
    pub fill: Option<[u8; 4]>,
    pub stroke: Option<StrokeStyle>,
}

/// Read an editable SVG path from a bounded regular file. Symlinks and special
/// files are refused so importing cannot unexpectedly traverse another path or
/// block on a device/FIFO.
pub fn import(path: &Path) -> Result<SvgArtwork> {
    let file = open_read_nofollow_nonblock(path).context("Cannot safely open editable SVG")?;
    let metadata = file
        .metadata()
        .context("Cannot inspect opened editable SVG")?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_INPUT_BYTES,
        "Editable SVG input must be a regular file no larger than 4 MiB"
    );
    let expected_len = metadata.len();
    let mut input = Vec::with_capacity(expected_len as usize);
    let mut bounded = file.take(MAX_INPUT_BYTES + 1);
    bounded
        .read_to_end(&mut input)
        .context("Cannot read editable SVG")?;
    ensure!(
        input.len() as u64 <= MAX_INPUT_BYTES,
        "Editable SVG input exceeds 4 MiB"
    );
    let final_len = bounded
        .get_ref()
        .metadata()
        .context("Cannot recheck opened editable SVG")?
        .len();
    ensure!(
        input.len() as u64 == expected_len && final_len == expected_len,
        "Editable SVG changed while it was being read"
    );
    decode(&input)
}

/// Decode one editable SVG path from UTF-8 XML.
pub fn decode(input: &[u8]) -> Result<SvgArtwork> {
    ensure!(
        input.len() as u64 <= MAX_INPUT_BYTES,
        "Editable SVG input exceeds 4 MiB"
    );
    let text = std::str::from_utf8(input).context("Editable SVG must be UTF-8 XML")?;
    preflight(text)?;

    // No resources directory or custom resolver is supplied. The XML
    // preflight rejects every resource-bearing element/attribute as well.
    let tree = usvg::Tree::from_str(text, &usvg::Options::default())
        .context("Cannot parse editable SVG geometry")?;
    let width = integral_dimension(tree.size().width(), "width")?;
    let height = integral_dimension(tree.size().height(), "height")?;
    ensure!(
        valid_dimensions(width, height),
        "Editable SVG canvas exceeds Omuse dimension or 100-megapixel limits"
    );

    let mut paths = Vec::new();
    collect_paths(tree.root(), &mut paths)?;
    ensure!(
        paths.len() == 1,
        "Editable SVG must contain exactly one painted shape/path; found {}",
        paths.len()
    );
    let source = paths[0];
    ensure!(source.is_visible(), "Editable SVG path is hidden");
    ensure!(
        source.paint_order() == usvg::PaintOrder::FillAndStroke,
        "Stroke-before-fill paint order is unsupported"
    );

    let fill = source.fill().map(solid_fill).transpose()?;
    let (stroke, stroke_scale) = source
        .stroke()
        .map(|style| solid_stroke(style, source.abs_transform()))
        .transpose()?
        .map_or((None, 1.0), |(style, scale)| (Some(style), scale));
    ensure!(
        fill.is_some() || stroke.is_some(),
        "Editable SVG path must have a solid fill or stroke"
    );

    let mut path = convert_path(source.data(), source.abs_transform())?;
    path.fill_rule = source
        .fill()
        .map_or(FillRule::NonZero, |fill| match fill.rule() {
            usvg::FillRule::NonZero => FillRule::NonZero,
            usvg::FillRule::EvenOdd => FillRule::EvenOdd,
        });
    path.validate()
        .map_err(|error| anyhow::anyhow!("Invalid editable SVG path geometry: {error}"))?;

    let stroke = stroke.map(|mut value| {
        value.width *= stroke_scale;
        value
    });
    if let Some(stroke) = stroke {
        ensure!(
            stroke.width.is_finite() && stroke.width > 0.0 && stroke.width <= 100_000.0,
            "Editable SVG stroke width is outside Omuse limits"
        );
    }

    Ok(SvgArtwork {
        width,
        height,
        path,
        fill,
        stroke,
    })
}

/// Encode one editable Omuse path as a self-contained SVG document.
pub fn encode(artwork: &SvgArtwork) -> Result<String> {
    validate_artwork(artwork)?;
    let mut output = String::new();
    push_bounded(
        &mut output,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" ",
    )?;
    push_bounded(
        &mut output,
        &format!(
            "width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n  <path d=\"",
            artwork.width, artwork.height, artwork.width, artwork.height
        ),
    )?;
    for subpath in &artwork.path.subpaths {
        if subpath.anchors.is_empty() {
            continue;
        }
        let first = &subpath.anchors[0];
        push_command(&mut output, 'M', &[first.position])?;
        for index in 1..subpath.anchors.len() {
            push_segment(
                &mut output,
                &subpath.anchors[index - 1],
                &subpath.anchors[index],
            )?;
        }
        if subpath.closed {
            let last = subpath.anchors.last().expect("nonempty checked");
            if last.outgoing.is_some() || first.incoming.is_some() {
                push_segment(&mut output, last, first)?;
            }
            push_bounded(&mut output, " Z")?;
        }
    }

    let fill = artwork.fill.map_or_else(
        || " fill=\"none\"".to_owned(),
        |color| {
            format!(
                " fill=\"#{:02X}{:02X}{:02X}\" fill-opacity=\"{}\"",
                color[0],
                color[1],
                color[2],
                alpha_string(color[3])
            )
        },
    );
    let stroke = artwork.stroke.map_or_else(
        || " stroke=\"none\"".to_owned(),
        |style| {
            format!(
                " stroke=\"#{:02X}{:02X}{:02X}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
                style.color[0],
                style.color[1],
                style.color[2],
                alpha_string(style.color[3]),
                number(style.width)
            )
        },
    );
    let fill_rule = match artwork.path.fill_rule {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    };

    // All interpolated values are validated numbers or fixed-width hex, so no
    // unescaped user-controlled XML text enters the document. Building the
    // final document in place also prevents a second multi-megabyte copy.
    push_bounded(&mut output, "\"")?;
    push_bounded(&mut output, &fill)?;
    push_bounded(&mut output, &stroke)?;
    push_bounded(
        &mut output,
        &format!(" fill-rule=\"{fill_rule}\"/>\n</svg>\n"),
    )?;
    Ok(output)
}

/// A fully written and synced sibling file awaiting one atomic publication
/// step. Dropping it before publication removes the staged file.
#[derive(Debug)]
pub struct PreparedExport {
    destination: PathBuf,
    parent: PathBuf,
    staging: PathBuf,
}

impl PreparedExport {
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Publish without replacing an existing destination. This performs only
    /// the atomic hard-link commit so callers can recheck their document guard
    /// immediately before invoking it.
    pub fn publish(mut self) -> Result<PublishedExport> {
        fs::hard_link(&self.staging, &self.destination)
            .context("Cannot publish editable SVG without replacing an existing file")?;
        Ok(PublishedExport {
            destination: std::mem::take(&mut self.destination),
            parent: std::mem::take(&mut self.parent),
            staging: std::mem::take(&mut self.staging),
        })
    }
}

impl Drop for PreparedExport {
    fn drop(&mut self) {
        remove_staging(&self.staging);
    }
}

/// An export whose destination is already visible. Finishing makes the parent
/// directory durable and removes the now-unneeded staged sibling.
#[derive(Debug)]
pub struct PublishedExport {
    destination: PathBuf,
    parent: PathBuf,
    staging: PathBuf,
}

impl PublishedExport {
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn finish(self) -> Result<PathBuf> {
        fs::File::open(&self.parent)
            .and_then(|directory| directory.sync_all())
            .context("Cannot finish publishing editable SVG")?;
        Ok(self.destination.clone())
    }
}

impl Drop for PublishedExport {
    fn drop(&mut self) {
        remove_staging(&self.staging);
    }
}

fn remove_staging(path: &Path) {
    if !path.as_os_str().is_empty() {
        let _ = fs::remove_file(path);
    }
}

/// Encode, write and sync a staged sibling without making the destination
/// visible. Existing destinations are not touched during preparation.
pub fn prepare_export(path: &Path, artwork: &SvgArtwork) -> Result<PreparedExport> {
    let encoded = encode(artwork)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .context("Editable SVG destination needs a file name")?;
    let staging = parent.join(format!(
        ".omuse-vector-svg-{}-{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let mut file = create_staging(&staging).context("Cannot stage editable SVG")?;
    let prepared = PreparedExport {
        destination: path.to_path_buf(),
        parent: parent.to_path_buf(),
        staging,
    };
    file.write_all(encoded.as_bytes())
        .context("Cannot write staged editable SVG")?;
    file.sync_all()
        .context("Cannot finish staged editable SVG")?;
    Ok(prepared)
}

/// Export to a brand-new regular file. Existing destinations are never
/// replaced. The staged API lets interactive callers fence publication with a
/// current document/draft guard.
pub fn export(path: &Path, artwork: &SvgArtwork) -> Result<()> {
    prepare_export(path, artwork)?.publish()?.finish()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_read_nofollow_nonblock(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    // O_NOFOLLOW prevents final-component link traversal. O_NONBLOCK makes a
    // raced FIFO/device open return without waiting; fstat below then rejects
    // every non-regular descriptor. O_CLOEXEC keeps it out of child processes.
    OpenOptions::new()
        .read(true)
        .custom_flags(0x20000 | 0x800 | 0x80000)
        .open(path)
}

#[cfg(not(target_os = "linux"))]
fn open_read_nofollow_nonblock(path: &Path) -> std::io::Result<fs::File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Editable SVG input is not a regular file",
        ));
    }
    OpenOptions::new().read(true).open(path)
}

fn create_staging(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x80000);
    }
    options.open(path)
}

fn validate_artwork(artwork: &SvgArtwork) -> Result<()> {
    ensure!(
        valid_dimensions(artwork.width, artwork.height),
        "Editable SVG canvas exceeds Omuse dimension or 100-megapixel limits"
    );
    artwork.path.validate()?;
    ensure!(
        artwork
            .path
            .subpaths
            .iter()
            .any(|subpath| subpath.anchors.len() >= 2),
        "Editable SVG path has no drawable segments"
    );
    ensure!(
        artwork.fill.is_some() || artwork.stroke.is_some(),
        "Editable SVG path must have a fill or stroke"
    );
    if let Some(stroke) = artwork.stroke {
        ensure!(
            stroke.width.is_finite() && stroke.width > 0.0 && stroke.width <= 100_000.0,
            "Invalid editable SVG stroke width"
        );
    }
    Ok(())
}

fn preflight(text: &str) -> Result<()> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )
    .context("Invalid or excessive editable SVG XML (DTDs/entities are unsupported)")?;
    let root = document.root_element();
    ensure!(
        root.tag_name().name() == "svg"
            && root.tag_name().namespace() == Some("http://www.w3.org/2000/svg"),
        "Expected an SVG document with the SVG namespace"
    );

    for node in document.descendants() {
        ensure!(
            !node.is_pi(),
            "SVG processing instructions and external stylesheets are unsupported"
        );
        if !node.is_element() {
            continue;
        }
        ensure!(
            node.tag_name().namespace() == Some("http://www.w3.org/2000/svg"),
            "Foreign XML namespaces are unsupported in editable SVG"
        );
        ensure!(
            node.namespaces()
                .all(|namespace| namespace.uri() == "http://www.w3.org/2000/svg"),
            "Foreign XML namespace declarations are unsupported in editable SVG"
        );
        ensure!(
            node.ancestors().take(MAX_XML_DEPTH + 2).count() <= MAX_XML_DEPTH,
            "Editable SVG nesting exceeds 48 levels"
        );
        let tag = node.tag_name().name();
        match tag {
            "text" | "tspan" | "textPath" => {
                bail!("SVG text is unsupported for editable path import")
            }
            "image" | "video" | "audio" | "foreignObject" => {
                bail!("Embedded bitmap/media content is unsupported for editable path import")
            }
            "linearGradient" | "radialGradient" | "pattern" | "mesh" | "meshgradient" => {
                bail!("SVG gradients and patterns are unsupported for editable path import")
            }
            "filter" => {
                bail!("SVG filters and effects are unsupported for editable path import")
            }
            _ if tag.starts_with("fe") => {
                bail!("SVG filters and effects are unsupported for editable path import")
            }
            "mask" | "clipPath" => {
                bail!("SVG masks and clipping paths are unsupported for editable path import")
            }
            "script" | "animate" | "animateMotion" | "animateTransform" | "set" => {
                bail!("SVG scripts and animation are unsupported for editable path import")
            }
            "use" | "symbol" | "marker" => {
                bail!("Referenced SVG content and markers are unsupported for editable path import")
            }
            "style" => {
                bail!(
                    "SVG style sheets are unsupported; use inline style or presentation attributes"
                )
            }
            "svg" | "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline"
            | "polygon" | "title" | "desc" | "metadata" | "defs" => {}
            _ => bail!("Unsupported SVG element <{tag}> in editable path import"),
        }
        for attribute in node.attributes() {
            let name = attribute.name();
            let value = attribute.value();
            ensure!(
                attribute.namespace().is_none(),
                "Foreign namespaced SVG attributes are unsupported"
            );
            ensure!(
                name != "href" && name != "vector-effect",
                "SVG resource references and non-scaling strokes are unsupported"
            );
            ensure!(
                !matches!(
                    name,
                    "opacity" | "mix-blend-mode" | "isolation" | "clip-path" | "mask" | "filter"
                ),
                "SVG object opacity, blending, masks, clipping and effects are unsupported"
            );
            ensure!(
                !value.to_ascii_lowercase().contains("url("),
                "SVG resource references and paint servers are unsupported"
            );
            ensure!(
                !value.to_ascii_lowercase().contains("@import"),
                "External SVG style imports are unsupported"
            );
            if name == "style" {
                for declaration in value.split(';') {
                    let property = declaration
                        .split_once(':')
                        .map_or(declaration, |(property, _)| property)
                        .trim()
                        .to_ascii_lowercase();
                    ensure!(
                        !matches!(
                            property.as_str(),
                            "opacity"
                                | "mix-blend-mode"
                                | "isolation"
                                | "clip-path"
                                | "mask"
                                | "filter"
                                | "vector-effect"
                        ),
                        "SVG object opacity, blending, masks, clipping, effects and non-scaling strokes are unsupported"
                    );
                }
            }
        }
    }
    Ok(())
}

fn collect_paths<'a>(group: &'a usvg::Group, output: &mut Vec<&'a usvg::Path>) -> Result<()> {
    ensure!(
        group.opacity() == usvg::Opacity::ONE
            && group.blend_mode() == usvg::BlendMode::Normal
            && !group.isolate()
            && group.clip_path().is_none()
            && group.mask().is_none()
            && group.filters().is_empty(),
        "SVG opacity groups, blending, masks, clipping and effects are unsupported"
    );
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_paths(child, output)?,
            usvg::Node::Path(path) => output.push(path),
            usvg::Node::Image(_) => {
                bail!("Embedded bitmap content is unsupported for editable path import")
            }
            usvg::Node::Text(_) => bail!("SVG text is unsupported for editable path import"),
        }
    }
    Ok(())
}

fn solid_fill(fill: &usvg::Fill) -> Result<[u8; 4]> {
    color_with_opacity(fill.paint(), fill.opacity(), "fill")
}

fn solid_stroke(
    stroke: &usvg::Stroke,
    transform: tiny_skia::Transform,
) -> Result<(StrokeStyle, f32)> {
    ensure!(
        stroke.dasharray().is_none(),
        "Dashed SVG strokes are unsupported for editable path import"
    );
    ensure!(
        stroke.linecap() == usvg::LineCap::Round && stroke.linejoin() == usvg::LineJoin::Round,
        "Editable SVG strokes must use round line caps and joins"
    );
    let scale = uniform_scale(transform)?;
    Ok((
        StrokeStyle {
            color: color_with_opacity(stroke.paint(), stroke.opacity(), "stroke")?,
            width: stroke.width().get(),
        },
        scale,
    ))
}

fn color_with_opacity(paint: &usvg::Paint, opacity: usvg::Opacity, role: &str) -> Result<[u8; 4]> {
    let usvg::Paint::Color(color) = paint else {
        bail!("SVG {role} must be a solid color; gradients and patterns are unsupported")
    };
    Ok([
        color.red,
        color.green,
        color.blue,
        (opacity.get() * 255.0).round() as u8,
    ])
}

fn uniform_scale(transform: tiny_skia::Transform) -> Result<f32> {
    let x = transform.sx.hypot(transform.ky);
    let y = transform.kx.hypot(transform.sy);
    let dot = transform.sx * transform.kx + transform.ky * transform.sy;
    let tolerance = x.max(y).max(1.0) * 1.0e-5;
    ensure!(
        x.is_finite()
            && y.is_finite()
            && x > 0.0
            && (x - y).abs() <= tolerance
            && dot.abs() <= tolerance * x.max(y),
        "Non-uniform or skewed transforms on SVG strokes are unsupported"
    );
    Ok((x + y) * 0.5)
}

fn convert_path(data: &tiny_skia::Path, transform: tiny_skia::Transform) -> Result<VectorPath> {
    let mut subpaths: Vec<Subpath> = Vec::new();
    let mut current: Option<Subpath> = None;

    for segment in data.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(position) => {
                finish_open_subpath(&mut subpaths, current.take());
                current = Some(Subpath {
                    anchors: vec![Anchor {
                        position: mapped(position, transform),
                        incoming: None,
                        outgoing: None,
                    }],
                    closed: false,
                });
            }
            tiny_skia::PathSegment::LineTo(position) => {
                let sub = current
                    .as_mut()
                    .context("SVG path segment appears before its move command")?;
                sub.anchors.push(Anchor {
                    position: mapped(position, transform),
                    incoming: None,
                    outgoing: None,
                });
            }
            tiny_skia::PathSegment::QuadTo(control, position) => {
                let sub = current
                    .as_mut()
                    .context("SVG path segment appears before its move command")?;
                let from = sub
                    .anchors
                    .last()
                    .context("SVG quadratic path has no starting point")?
                    .position;
                let control = mapped(control, transform);
                let to = mapped(position, transform);
                sub.anchors.last_mut().expect("checked").outgoing = Some(Point {
                    x: from.x + (control.x - from.x) * (2.0 / 3.0),
                    y: from.y + (control.y - from.y) * (2.0 / 3.0),
                });
                sub.anchors.push(Anchor {
                    position: to,
                    incoming: Some(Point {
                        x: to.x + (control.x - to.x) * (2.0 / 3.0),
                        y: to.y + (control.y - to.y) * (2.0 / 3.0),
                    }),
                    outgoing: None,
                });
            }
            tiny_skia::PathSegment::CubicTo(control1, control2, position) => {
                let sub = current
                    .as_mut()
                    .context("SVG path segment appears before its move command")?;
                ensure!(
                    !sub.anchors.is_empty(),
                    "SVG cubic path has no starting point"
                );
                sub.anchors.last_mut().expect("checked").outgoing =
                    Some(mapped(control1, transform));
                sub.anchors.push(Anchor {
                    position: mapped(position, transform),
                    incoming: Some(mapped(control2, transform)),
                    outgoing: None,
                });
            }
            tiny_skia::PathSegment::Close => {
                let mut sub = current
                    .take()
                    .context("SVG close command appears before its move command")?;
                normalize_closing_anchor(&mut sub);
                sub.closed = true;
                subpaths.push(sub);
            }
        }
    }
    finish_open_subpath(&mut subpaths, current);
    Ok(VectorPath {
        subpaths,
        fill_rule: FillRule::NonZero,
    })
}

fn normalize_closing_anchor(sub: &mut Subpath) {
    if sub.anchors.len() < 2 {
        return;
    }
    let first = sub.anchors[0].position;
    let last = sub.anchors.last().expect("length checked");
    if last.position == first {
        let closing = sub.anchors.pop().expect("length checked");
        sub.anchors[0].incoming = closing.incoming;
    }
}

fn finish_open_subpath(output: &mut Vec<Subpath>, subpath: Option<Subpath>) {
    if let Some(subpath) = subpath {
        output.push(subpath);
    }
}

fn mapped(mut point: tiny_skia::Point, transform: tiny_skia::Transform) -> Point {
    transform.map_point(&mut point);
    Point {
        x: point.x,
        y: point.y,
    }
}

fn integral_dimension(value: f32, label: &str) -> Result<u32> {
    ensure!(value.is_finite() && value > 0.0, "Invalid SVG {label}");
    let rounded = value.round();
    ensure!(
        (value - rounded).abs() <= 1.0e-4 && rounded <= u32::MAX as f32,
        "Editable SVG {label} must resolve to a whole number of pixels"
    );
    Ok(rounded as u32)
}

fn push_segment(output: &mut String, from: &Anchor, to: &Anchor) -> Result<()> {
    match (from.outgoing, to.incoming) {
        (None, None) => push_command(output, 'L', &[to.position]),
        (outgoing, incoming) => push_command(
            output,
            'C',
            &[
                outgoing.unwrap_or(from.position),
                incoming.unwrap_or(to.position),
                to.position,
            ],
        ),
    }
}

fn push_command(output: &mut String, command: char, points: &[Point]) -> Result<()> {
    push_bounded(output, " ")?;
    let command = match command {
        'M' => "M",
        'L' => "L",
        'C' => "C",
        _ => unreachable!("internal SVG command"),
    };
    push_bounded(output, command)?;
    for point in points {
        push_bounded(output, " ")?;
        push_bounded(output, &number(point.x))?;
        push_bounded(output, " ")?;
        push_bounded(output, &number(point.y))?;
    }
    Ok(())
}

fn push_bounded(output: &mut String, value: &str) -> Result<()> {
    ensure!(
        value.len() <= MAX_INPUT_BYTES as usize - output.len(),
        "Encoded editable SVG exceeds the 4 MiB reimport limit; reduce path complexity"
    );
    output.push_str(value);
    Ok(())
}

fn number(value: f32) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    }
}

fn alpha_string(alpha: u8) -> String {
    if alpha == 255 {
        "1".to_owned()
    } else if alpha == 0 {
        "0".to_owned()
    } else {
        (f32::from(alpha) / 255.0).to_string()
    }
}
