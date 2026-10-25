//! PDF export from Omuse's bounded scene/path representation. Dithered fills
//! may embed explicitly reported, bounded object rasterizations; other objects
//! retain vector geometry. No arbitrary PDF, SVG resource or font is read.
use crate::{
    vector_scene::VectorScene,
    vector_svg::{self, PreparedExport, SvgArtwork},
    vector_svg_scene,
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub fn encode_scene(scene: &VectorScene, dpi: f32) -> Result<Vec<u8>> {
    let svg = vector_svg_scene::encode_scene(scene)?;
    if scene.objects.iter().any(|object| object.is_clip) {
        // Normalizers may discard a degenerate clip. Reject conversion rather
        // than risk publishing contents outside their retained silhouette.
        // Paint conversion is deliberately richer than editable SVG import:
        // dither embeds bounded pixels; perceptual interpolation expands stops.
        // Validate only the unchanged clipping geometry using solid fills on a
        // companion scene. Import still refuses arbitrary images/resources and
        // excessive editable stops. Actual PDF paints never use this companion.
        let validation_svg;
        let validation_input = if scene
            .objects
            .iter()
            .any(|object| object.fill_gradient.is_some())
        {
            let mut geometry = scene.clone();
            for object in &mut geometry.objects {
                if object.fill_gradient.take().is_some() {
                    object.fill = Some([0, 0, 0, 255]);
                }
            }
            validation_svg = vector_svg_scene::encode_scene(&geometry)?;
            validation_svg.as_str()
        } else {
            svg.as_str()
        };
        vector_svg_scene::decode_scene(validation_input.as_bytes())
            .context("Cannot preserve this clipping silhouette in vector PDF")?;
    }
    encode_svg(&svg, dpi)
}

pub fn encode_path(artwork: &SvgArtwork, dpi: f32) -> Result<Vec<u8>> {
    encode_svg(&vector_svg::encode(artwork)?, dpi)
}

fn encode_svg(svg: &str, dpi: f32) -> Result<Vec<u8>> {
    ensure!(
        dpi.is_finite() && (1. ..=9600.).contains(&dpi),
        "PDF resolution must be 1–9600 DPI"
    );
    let xml = roxmltree::Document::parse(svg).context("Cannot inspect generated vector artwork")?;
    ensure!(
        !xml.descendants()
            .any(|node| matches!(node.attribute("spreadMethod"), Some("repeat" | "reflect"))),
        "PDF export cannot preserve repeating or reflected gradients. Choose Pad, or export SVG to keep this gradient."
    );
    let tree = svg2pdf::usvg::Tree::from_str(svg, &svg2pdf::usvg::Options::default())
        .context("Cannot prepare vector PDF")?;
    ensure!(
        tree.size().width() * 72. / dpi <= 14_400. && tree.size().height() * 72. / dpi <= 14_400.,
        "PDF page exceeds 200 inches. Increase document resolution before exporting."
    );
    let result = svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions {
            embed_text: false,
            ..Default::default()
        },
        svg2pdf::PageOptions { dpi },
    )
    .map_err(|error| anyhow::anyhow!("Cannot encode vector PDF: {error}"))?;
    ensure!(
        result.len() <= 16 * 1024 * 1024,
        "Vector PDF exceeds 16 MiB"
    );
    Ok(result)
}

pub fn prepare_scene_export(path: &Path, scene: &VectorScene, dpi: f32) -> Result<PreparedExport> {
    vector_svg::prepare_bytes_export(path, &encode_scene(scene, dpi)?)
}

pub fn prepare_path_export(path: &Path, artwork: &SvgArtwork, dpi: f32) -> Result<PreparedExport> {
    vector_svg::prepare_bytes_export(path, &encode_path(artwork, dpi)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_path::{Anchor, FillRule, Point, Subpath, VectorPath};

    fn scene() -> VectorScene {
        VectorScene::from_path(
            144,
            72,
            "Curve",
            VectorPath {
                subpaths: vec![Subpath {
                    anchors: vec![
                        Anchor {
                            position: Point { x: 8., y: 60. },
                            incoming: None,
                            outgoing: Some(Point { x: 20., y: 8. }),
                        },
                        Anchor {
                            position: Point { x: 136., y: 60. },
                            incoming: Some(Point { x: 120., y: 8. }),
                            outgoing: None,
                        },
                    ],
                    closed: true,
                }],
                fill_rule: FillRule::EvenOdd,
            },
            Some([200, 80, 40, 160]),
            None,
        )
        .unwrap()
    }

    #[test]
    fn pdf_retains_curved_geometry_transparency_and_resolution_without_images() {
        let bytes = encode_scene(&scene(), 144.).unwrap();
        let pdf = String::from_utf8_lossy(&bytes);
        assert!(pdf.starts_with("%PDF-1.7"));
        assert!(pdf.contains("/MediaBox [0 0 72 36]"));
        assert!(pdf.contains("/ExtGState"));
        assert!(!pdf.contains("/Subtype /Image"));
        assert!(pdf.ends_with("%%EOF\n") || pdf.ends_with("%%EOF"));
    }

    #[test]
    fn pdf_publication_is_atomic_and_never_replaces_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Art.pdf");
        let prepared = prepare_scene_export(&path, &scene(), 72.).unwrap();
        assert!(!path.exists());
        std::fs::write(&path, b"keep").unwrap();
        assert!(prepared.publish().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        let good = temp.path().join("New.pdf");
        prepare_scene_export(&good, &scene(), 72.)
            .unwrap()
            .publish()
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            std::fs::read(good).unwrap(),
            encode_scene(&scene(), 72.).unwrap()
        );
    }

    #[test]
    fn invalid_resolution_cannot_create_an_output_file() {
        for dpi in [0., f32::NAN, f32::INFINITY, 9601.] {
            assert!(encode_scene(&scene(), dpi).is_err());
        }
    }
}
