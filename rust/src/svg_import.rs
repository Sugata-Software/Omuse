//! Self-contained SVG/SVGZ raster import. Rendering never resolves document
//! URLs or filesystem paths; compressed input, references and render surfaces
//! have explicit budgets before the renderer runs.
use crate::model::{Layer, valid_dimensions};
use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use resvg::{tiny_skia, usvg};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

const MAX_INPUT: u64 = 16 * 1024 * 1024;
const MAX_NODES: u32 = 30_000;
const MAX_DEPTH: usize = 64;
const MAX_PIXELS: u64 = 25_000_000;
const MAX_WORK_PIXELS: u64 = 100_000_000;

pub fn matches(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("svg") || s.eq_ignore_ascii_case("svgz"))
}

pub fn import(path: &Path) -> Result<Layer> {
    import_with_size(path, None)
}

/// Rasterize directly at the requested size, preserving sharp vector edges.
/// The UI keeps the aspect ratio; callers that need a different ratio can
/// explicitly choose one. No resampling of a low-resolution intermediate.
pub fn import_with_size(path: &Path, size: Option<(u32, u32)>) -> Result<Layer> {
    let data = read_file(path)?;
    let image = rasterize_with_size(&data, size)?;
    let mut layer = Layer::group(
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("SVG artwork"),
    );
    layer.metadata = json!({
        "sourceFormat": if data.starts_with(&[0x1f, 0x8b]) { "SVGZ" } else { "SVG" },
        "sourceRasterSize": [image.width(), image.height()],
        "psdConversions": [format!("SVG artwork was rasterized at {} × {} pixels (96 DPI for intrinsic physical units). Vector paths and text are now pixels; keep the original SVG to edit them. External resources and animation are not imported.", image.width(), image.height())]
    });
    layer.image = Some(image.into());
    Ok(layer)
}

/// Inspect SVG dimensions without allocating the output pixel surface.
pub fn intrinsic_size(path: &Path) -> Result<(f32, f32)> {
    let tree = parse_tree(&read_file(path)?)?;
    Ok((tree.size().width(), tree.size().height()))
}

pub fn validate_size(width: u32, height: u32) -> Result<()> {
    ensure!(
        valid_dimensions(width, height) && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "SVG raster exceeds 25 megapixels or the 30000-pixel side limit; choose a smaller import size"
    );
    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_INPUT,
        "SVG input must be a regular file no larger than 16 MiB"
    );
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(MAX_INPUT + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() as u64 <= MAX_INPUT, "SVG input exceeds 16 MiB");
    Ok(data)
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

fn parse_tree(input: &[u8]) -> Result<usvg::Tree> {
    ensure!(input.len() as u64 <= MAX_INPUT, "SVG input exceeds 16 MiB");
    let expanded;
    let data = if input.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(input)
            .take(MAX_INPUT + 1)
            .read_to_end(&mut out)
            .context("Invalid SVGZ gzip stream")?;
        ensure!(
            out.len() as u64 <= MAX_INPUT,
            "Expanded SVGZ exceeds 16 MiB"
        );
        expanded = out;
        expanded.as_slice()
    } else {
        input
    };
    let text = std::str::from_utf8(data).context("SVG must contain UTF-8 XML")?;
    preflight(text)?;
    // A failed image resolver normally disappears silently in usvg. Retain the
    // reason and reject the entire import instead of showing incomplete art.
    let image_error = Arc::new(Mutex::new(None::<String>));
    let image_budget = Arc::new(Mutex::new(0u64));
    let error = image_error.clone();
    let budget = image_budget.clone();
    let mut options = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_string: Box::new(|_, _| None),
        resolve_data: Box::new(move |mime, data, _| {
            let result = embedded_image(mime, data, &budget);
            match result {
                Ok(kind) => Some(kind),
                Err(reason) => {
                    *error.lock().unwrap() = Some(reason.to_string());
                    None
                }
            }
        }),
    };
    let tree = usvg::Tree::from_str(text, &options).context("Cannot parse SVG artwork")?;
    if let Some(error) = image_error.lock().unwrap().take() {
        bail!("{error}");
    }
    Ok(tree)
}

#[cfg(test)]
fn rasterize(input: &[u8]) -> Result<RgbaImage> {
    rasterize_with_size(input, None)
}

fn rasterize_with_size(input: &[u8], requested: Option<(u32, u32)>) -> Result<RgbaImage> {
    if let Some((width, height)) = requested {
        validate_size(width, height)?;
    }
    let tree = parse_tree(input)?;
    let size = tree.size();
    let (width, height) =
        requested.unwrap_or((size.width().ceil() as u32, size.height().ceil() as u32));
    validate_size(width, height)?;
    let scale = (width as f32 / size.width(), height as f32 / size.height());
    let mut work = u64::from(width) * u64::from(height);
    bound_render(tree.root(), 0, scale, &mut work)?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).context("Cannot allocate SVG raster")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale.0, scale.1),
        &mut pixmap.as_mut(),
    );
    let mut bytes = pixmap.take();
    // tiny-skia produces premultiplied RGBA; the editor stores straight RGBA.
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = if alpha == 0 {
                0
            } else {
                ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8
            };
        }
    }
    RgbaImage::from_raw(width, height, bytes).context("Invalid SVG raster size")
}

fn embedded_image(mime: &str, data: Arc<Vec<u8>>, budget: &Mutex<u64>) -> Result<usvg::ImageKind> {
    ensure!(
        data.len() as u64 <= MAX_INPUT,
        "Embedded SVG image exceeds 16 MiB"
    );
    let mut reader = ImageReader::new(Cursor::new(data.as_slice())).with_guessed_format()?;
    let format = reader.format().context("Unsupported embedded SVG image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .context("Invalid embedded SVG image")?;
    let (w, h) = decoder.dimensions();
    let pixels = u64::from(w) * u64::from(h);
    ensure!(
        valid_dimensions(w, h) && pixels <= MAX_PIXELS,
        "Embedded SVG image exceeds 25 megapixels or dimension limits"
    );
    let mut used = budget.lock().unwrap();
    ensure!(
        pixels <= MAX_PIXELS.saturating_sub(*used),
        "SVG embedded-image pixel budget exceeded"
    );
    *used += pixels;
    drop(used);
    // Only these formats are decoded by resvg. In particular do not delegate
    // nested SVG data URLs back into a resolver with fresh recursion budgets.
    match (format, mime) {
        (image::ImageFormat::Png, "image/png" | "text/plain")
        | (image::ImageFormat::Jpeg, "image/jpeg" | "image/jpg" | "text/plain")
        | (image::ImageFormat::Gif, "image/gif" | "text/plain")
        | (image::ImageFormat::WebP, "image/webp" | "text/plain") => {}
        _ => bail!(
            "SVG embedded images must be PNG, JPEG, GIF or WebP with a matching media type; nested SVG is unsupported"
        ),
    }
    let profile = decoder
        .icc_profile()
        .context("Invalid embedded SVG image profile")?;
    let image = DynamicImage::from_decoder(decoder)
        .context("Cannot decode embedded SVG image")?
        .to_rgba8();
    let image = crate::color_management::to_srgb(&image, profile.as_deref())?.image;
    // Give the renderer a validated, static sRGB PNG. A truncated embedded
    // image must fail the import, not silently vanish after a header probe.
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image).write_to(&mut encoded, image::ImageFormat::Png)?;
    Ok(usvg::ImageKind::PNG(Arc::new(encoded.into_inner())))
}

fn preflight(text: &str) -> Result<()> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES,
            ..Default::default()
        },
    )
    .context("Invalid or excessive SVG XML (DTD/entity declarations are unsupported)")?;
    let root = document.root_element();
    ensure!(
        root.tag_name().name() == "svg"
            && root.tag_name().namespace() == Some("http://www.w3.org/2000/svg"),
        "Expected an SVG document with the SVG namespace"
    );
    let mut ids = HashMap::new();
    for node in document.descendants() {
        ensure!(
            !node.is_pi() || node.pi().is_none_or(|p| p.target != "xml-stylesheet"),
            "External SVG stylesheets are unsupported; embed styles before importing"
        );
        if !node.is_element() {
            continue;
        }
        ensure!(
            node.ancestors().take(MAX_DEPTH + 2).count() <= MAX_DEPTH,
            "SVG nesting exceeds 64 levels"
        );
        ensure!(
            !matches!(
                node.tag_name().name(),
                "script"
                    | "foreignObject"
                    | "animate"
                    | "animateMotion"
                    | "animateTransform"
                    | "set"
                    | "audio"
                    | "video"
            ),
            "SVG scripts, foreign content and animation are unsupported; export a static SVG first"
        );
        if let Some(id) = node.attribute("id") {
            ensure!(
                ids.insert(id, node).is_none(),
                "SVG contains duplicate object IDs"
            );
        }
        for attribute in node.attributes() {
            if attribute.name() == "href" {
                let value = attribute.value().trim();
                ensure!(
                    value.starts_with('#')
                        || (node.tag_name().name() == "image" && value.starts_with("data:")),
                    "SVG external resources are unsupported; embed linked images and use local references before importing"
                );
            }
            if attribute.name() == "style"
                || attribute.value().to_ascii_lowercase().contains("url(")
            {
                local_css(attribute.value())?;
            }
        }
        if node.tag_name().name() == "image" {
            let href = node
                .attribute("href")
                .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
                .context("SVG image has no embedded image data")?;
            validate_data_url(href.trim())?;
        }
        if node.tag_name().name() == "style" {
            for child in node.children().filter(|n| n.is_text()) {
                local_css(child.text().unwrap_or_default())?;
            }
        }
    }
    // <use> and filter images can expand a tiny XML input exponentially. Count their expanded
    // subtree before invoking usvg, memoizing repeated subtrees and rejecting
    // cycles instead of relying on the renderer's recursion handling.
    fn expanded<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
        ids: &HashMap<&'input str, roxmltree::Node<'a, 'input>>,
        seen: &mut HashSet<roxmltree::NodeId>,
        memo: &mut HashMap<roxmltree::NodeId, u64>,
        depth: usize,
    ) -> Result<u64> {
        ensure!(
            depth <= MAX_DEPTH,
            "SVG reference nesting exceeds 64 levels"
        );
        if let Some(value) = memo.get(&node.id()) {
            return Ok(*value);
        }
        ensure!(
            seen.insert(node.id()),
            "SVG contains cyclic object references"
        );
        let mut count = 1u64;
        for child in node.children().filter(|n| n.is_element()) {
            count += expanded(child, ids, seen, memo, depth + 1)?;
            ensure!(
                count <= u64::from(MAX_NODES),
                "Expanded SVG exceeds 30000 objects"
            );
        }
        if matches!(node.tag_name().name(), "use" | "feImage") {
            let href = node
                .attribute("href")
                .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(id) = href.and_then(|h| h.strip_prefix('#')) {
                let target = ids.get(id).context("SVG references a missing object")?;
                count += expanded(*target, ids, seen, memo, depth + 1)?;
                ensure!(
                    count <= u64::from(MAX_NODES),
                    "Expanded SVG exceeds 30000 objects"
                );
            }
        }
        seen.remove(&node.id());
        memo.insert(node.id(), count);
        Ok(count)
    }
    expanded(root, &ids, &mut HashSet::new(), &mut HashMap::new(), 0)?;
    Ok(())
}

fn local_css(css: &str) -> Result<()> {
    let lower = css.to_ascii_lowercase();
    ensure!(
        !lower.contains("@import") && !lower.contains("@font-face") && !lower.contains('\\'),
        "SVG external or escaped CSS resources are unsupported; embed styles and outline custom web fonts before importing"
    );
    for tail in lower.split("url(").skip(1) {
        let end = tail.find(')').context("Malformed SVG resource URL")?;
        let target = tail[..end].trim().trim_matches(['\'', '"']).trim();
        ensure!(
            target.starts_with('#'),
            "SVG external CSS resources are unsupported; use local references"
        );
    }
    Ok(())
}

fn validate_data_url(url: &str) -> Result<()> {
    let (header, data) = url
        .split_once(',')
        .context("Malformed SVG image data URL")?;
    ensure!(
        matches!(
            header,
            "data:image/png;base64"
                | "data:image/jpeg;base64"
                | "data:image/jpg;base64"
                | "data:image/gif;base64"
                | "data:image/webp;base64"
        ),
        "SVG images must be embedded PNG, JPEG, GIF or WebP base64 data; linked and nested SVG images are unsupported"
    );
    let mut count = 0usize;
    let mut padding = 0usize;
    let mut last_value = 0u8;
    for byte in data.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            padding += 1;
            ensure!(padding <= 2, "Invalid SVG image base64 padding");
        } else {
            ensure!(
                padding == 0 && (byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/')),
                "Invalid SVG image base64 data"
            );
            last_value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                _ => 63,
            };
        }
        count += 1;
    }
    ensure!(
        count > 0 && count % 4 == 0,
        "Invalid SVG image base64 length"
    );
    ensure!(
        (padding != 1 || last_value & 3 == 0) && (padding != 2 || last_value & 15 == 0),
        "Invalid SVG image base64 padding bits"
    );
    Ok(())
}

fn bound_render(
    group: &usvg::Group,
    depth: usize,
    scale: (f32, f32),
    work: &mut u64,
) -> Result<()> {
    ensure!(
        depth <= MAX_DEPTH,
        "Expanded SVG group nesting exceeds 64 levels"
    );
    let bounds = group.abs_layer_bounding_box();
    let area = (f64::from(bounds.width()) * f64::from(scale.0)).ceil()
        * (f64::from(bounds.height()) * f64::from(scale.1)).ceil();
    ensure!(
        area.is_finite() && area <= MAX_WORK_PIXELS as f64,
        "SVG drawing or filter bounds exceed the rendering budget"
    );
    let surfaces = 1 + group
        .filters()
        .iter()
        .map(|f| 4 * f.primitives().len() as u64)
        .sum::<u64>();
    let pixels = (area as u64)
        .checked_mul(surfaces)
        .context("SVG filter budget overflow")?;
    ensure!(
        pixels <= MAX_WORK_PIXELS.saturating_sub(*work),
        "SVG group/filter surface budget exceeded"
    );
    *work += pixels;
    for node in group.children() {
        if let usvg::Node::Group(child) = node {
            bound_render(child, depth + 1, scale, work)?;
        }
        let mut result = Ok(());
        node.subroots(|root| {
            if result.is_ok() {
                result = bound_render(root, depth + 1, scale, work);
            }
        });
        result?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn svg(body: &str) -> String {
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='8' height='6'>{body}</svg>")
    }
    #[test]
    fn vector_pixels_use_intrinsic_dimensions_and_straight_alpha() {
        let image = rasterize(
            svg("<rect width='8' height='6' fill='#ff8000' fill-opacity='.5'/>").as_bytes(),
        )
        .unwrap();
        assert_eq!(image.dimensions(), (8, 6));
        assert_eq!(image.get_pixel(4, 3).0, [255, 128, 0, 128]);
        let box_only = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 23 17'><path fill='red' d='M0 0H23V17H0z'/></svg>";
        assert_eq!(rasterize(box_only).unwrap().dimensions(), (23, 17));
    }
    #[test]
    fn svgz_uses_identical_pixels_and_limits_expansion() {
        fn gzip(data: &[u8]) -> Vec<u8> {
            let mut writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            writer.write_all(data).unwrap();
            writer.finish().unwrap()
        }
        let source = svg("<rect width='8' height='6' fill='rebeccapurple'/>");
        assert_eq!(
            rasterize(source.as_bytes()).unwrap(),
            rasterize(&gzip(source.as_bytes())).unwrap()
        );
        assert!(
            rasterize(&gzip(&vec![b' '; MAX_INPUT as usize + 1]))
                .unwrap_err()
                .to_string()
                .contains("Expanded SVGZ")
        );
        assert!(rasterize(&[0x1f, 0x8b, 0]).is_err());
    }
    #[test]
    fn local_references_work_but_external_resources_and_entities_are_rejected() {
        let image = rasterize(svg("<defs><rect id='r' width='4' height='6' fill='lime'/></defs><use href='#r' x='2'/>").as_bytes()).unwrap();
        assert_eq!(image.get_pixel(3, 3).0, [0, 255, 0, 255]);
        for body in [
            "<image href='/etc/passwd'/>",
            "<image href='https://example.org/a.png'/>",
            "<style>@import url('file:///tmp/a.css');</style>",
            "<rect style='fill:url(https://example.org/a.svg#p)'/>",
            "<script>alert(1)</script>",
            "<foreignObject/>",
            "<animate/>",
        ] {
            assert!(rasterize(svg(body).as_bytes()).is_err(), "{body}");
        }
        assert!(rasterize(b"<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><svg xmlns='http://www.w3.org/2000/svg'>&x;</svg>").is_err());
    }
    #[test]
    fn cyclic_and_exponential_references_fail_before_rendering() {
        assert!(
            rasterize(svg("<g id='a'><use href='#a'/></g>").as_bytes())
                .unwrap_err()
                .to_string()
                .contains("cyclic")
        );
        let mut body = "<defs><g id='a0'><rect width='1' height='1'/></g>".to_string();
        for n in 1..20 {
            body.push_str(&format!(
                "<g id='a{n}'><use href='#a{}'/><use href='#a{}'/></g>",
                n - 1,
                n - 1
            ));
        }
        body.push_str("</defs><use href='#a19'/>");
        assert!(
            rasterize(svg(&body).as_bytes())
                .unwrap_err()
                .to_string()
                .contains("30000 objects")
        );
    }
    #[test]
    fn oversized_surface_and_embedded_images_are_rejected() {
        assert!(
            rasterize(b"<svg xmlns='http://www.w3.org/2000/svg' width='30000' height='30000'/>")
                .unwrap_err()
                .to_string()
                .contains("25 megapixels")
        );
        // A valid PNG header with enormous dimensions is rejected before decode.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&30_000u32.to_be_bytes());
        png.extend_from_slice(&30_000u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert!(embedded_image("image/png", Arc::new(png), &Mutex::new(0)).is_err());
        assert!(
            embedded_image(
                "image/svg+xml",
                Arc::new(svg("").into_bytes()),
                &Mutex::new(0)
            )
            .is_err()
        );
    }
    #[test]
    fn imported_svg_saves_and_reopens_as_omuse_without_changing_original() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("design.SVG");
        let original = svg("<rect width='8' height='6' fill='#ff8000'/>");
        fs::write(&source, &original).unwrap();
        let doc = crate::document::open(&source).unwrap();
        assert!(!crate::import_report::conversion_notes(&doc).is_empty());
        let path = temp.path().join("design.omuse");
        crate::document::save(&doc, &path).unwrap();
        let reopened = crate::document::open(&path).unwrap();
        assert_eq!(reopened.layers[0].image, doc.layers[0].image);
        assert_eq!(fs::read_to_string(source).unwrap(), original);
    }

    #[test]
    fn chosen_svg_size_renders_vectors_directly_and_can_reduce_a_large_viewport() {
        let source = svg("<rect width='4' height='6' fill='red'/>");
        let image = rasterize_with_size(source.as_bytes(), Some((80, 60))).unwrap();
        assert_eq!(image.dimensions(), (80, 60));
        assert_eq!(image.get_pixel(39, 30).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(40, 30).0, [0, 0, 0, 0]);
        let large = b"<svg xmlns='http://www.w3.org/2000/svg' width='90000' height='90000'><rect width='90000' height='90000' fill='blue'/></svg>";
        assert!(rasterize(large).is_err());
        assert_eq!(
            rasterize_with_size(large, Some((90, 90)))
                .unwrap()
                .get_pixel(45, 45)
                .0,
            [0, 0, 255, 255]
        );
        assert!(rasterize_with_size(source.as_bytes(), Some((0, 60))).is_err());
    }

    #[test]
    fn embedded_images_render_and_invalid_data_never_disappears_silently() {
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let data = png.into_inner();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in data.chunks(3) {
            let a = u32::from(chunk[0]);
            let b = u32::from(*chunk.get(1).unwrap_or(&0));
            let c = u32::from(*chunk.get(2).unwrap_or(&0));
            let bits = (a << 16) | (b << 8) | c;
            for index in 0..4 {
                encoded.push(if index > chunk.len() {
                    '='
                } else {
                    alphabet[((bits >> (18 - index * 6)) & 63) as usize] as char
                });
            }
        }
        let image = rasterize(
            svg(&format!(
                "<image width='8' height='6' href='data:image/png;base64,{encoded}'/>"
            ))
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(image.get_pixel(4, 3).0, [255, 0, 0, 255]);
        for href in [
            "data:image/png;base64,###=",
            "data:image/png;base64,AAA",
            "data:image/png;base64,AB==",
            "data:image/png;base64,AAAA",
            "#image",
        ] {
            assert!(
                rasterize(svg(&format!("<image width='8' height='6' href='{href}'/>")).as_bytes())
                    .is_err(),
                "{href}"
            );
        }
        assert!(
            embedded_image(
                "image/png",
                Arc::new(data[..data.len() / 2].to_vec()),
                &Mutex::new(0)
            )
            .is_err()
        );
    }
}
