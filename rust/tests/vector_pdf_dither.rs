//! PDF conversion qualification independent of editable SVG resource admission.
use omuse::{
    vector_path::{FillRule, Point},
    vector_scene::{
        GradientFill, GradientKind, GradientMethod, GradientSpread, GradientStop, VectorObject,
        VectorScene,
    },
};
use std::sync::atomic::AtomicBool;

fn scene() -> VectorScene {
    let mut painted = VectorObject::rectangle(
        "Dithered transformed photo shape",
        2.,
        2.,
        44.,
        36.,
        Some([10, 20, 30, 255]),
        None,
    )
    .unwrap();
    painted.transform = [1., 0., 0., 1., 3., 2.];
    painted.opacity = 0.6;
    painted.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 2., y: 2. },
            end: Point { x: 46., y: 2. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [35, 80, 180, 220],
            },
            GradientStop {
                offset: 1.,
                color: [215, 180, 50, 170],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
        method: GradientMethod::Classic,
        dither: true,
    });
    let mut mask = VectorObject::rectangle(
        "Transformed compound-hole mask",
        8.,
        7.,
        31.,
        27.,
        Some([0, 0, 0, 255]),
        None,
    )
    .unwrap();
    let hole =
        VectorObject::rectangle("Hole", 16., 14., 12., 11., Some([0, 0, 0, 255]), None).unwrap();
    mask.path.subpaths.extend(hole.path.subpaths);
    mask.path.fill_rule = FillRule::EvenOdd;
    mask.transform = [1., 0., 0., 1., 2., 1.];
    let source = VectorScene {
        version: 6,
        width: 64,
        height: 48,
        objects: vec![painted, mask],
    };
    let (mut clipped, _) = omuse::vector_clip::make(&source, &[0, 1]).unwrap();
    clipped.objects.push(
        VectorObject::rectangle(
            "Unrelated vector object",
            48.,
            12.,
            10.,
            20.,
            Some([230, 40, 60, 255]),
            None,
        )
        .unwrap(),
    );
    clipped
}
#[test]
fn bounded_dithered_pdf_keeps_clipping_and_vector_objects_without_loosening_svg_import() {
    let source = scene();
    let before = source.clone();
    let generated = omuse::vector_svg_scene::encode_scene(&source).unwrap();
    assert!(generated.contains("<image") && generated.contains("clip-path="));
    assert!(
        omuse::vector_svg_scene::decode_scene(generated.as_bytes()).is_err(),
        "editable SVG must continue to refuse embedded images"
    );
    let pdf = omuse::vector_pdf::encode_scene(&source, 72.).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF"));
    assert!(text.contains("/Subtype /Image"));
    assert!(text.contains("/MediaBox [0 0 64 48]"));
    assert_eq!(source, before, "validation cannot mutate the source scene");
    for unsafe_svg in [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><image href="https://example.invalid/a.png" width="64" height="48"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><image href="data:image/png;base64,AA==" width="64" height="48"/></svg>"##,
    ] {
        assert!(omuse::vector_svg_scene::decode_scene(unsafe_svg.as_bytes()).is_err());
    }
}
#[test]
fn clipped_perceptual_pdf_validates_geometry_without_editable_stop_admission() {
    let mut source = scene();
    let paint = source.objects[0].fill_gradient.as_mut().unwrap();
    paint.dither = false;
    paint.method = GradientMethod::Perceptual;
    paint.stops = (0..16)
        .map(|index| GradientStop {
            offset: index as f32 / 15.,
            color: if index % 2 == 0 {
                [0, 0, 255, 255]
            } else {
                [255, 255, 0, 255]
            },
        })
        .collect();
    let generated = omuse::vector_svg_scene::encode_scene(&source).unwrap();
    assert!(generated.matches("<stop ").count() > 16);
    assert!(omuse::vector_svg_scene::decode_scene(generated.as_bytes()).is_err());
    let pdf = omuse::vector_pdf::encode_scene(&source, 72.).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
}

#[test]
fn dither_does_not_bypass_invalid_empty_clipping_rejection() {
    let mut source = scene();
    let mask = source
        .objects
        .iter_mut()
        .find(|object| object.is_clip)
        .unwrap();
    mask.path.subpaths.clear();
    let error = omuse::vector_pdf::encode_scene(&source, 72.).unwrap_err();
    assert!(format!("{error:#}").contains("clipping"), "{error:#}");
}

/// Optional independent renderer qualification, executed explicitly on the host
/// when Poppler is available. Normal cargo runs do not imply it has executed.
#[test]
#[ignore = "requires Poppler pdftoppm; run explicitly for independent PDF pixels"]
fn poppler_pdf_pixels_match_clipped_dither_opacity_holes_and_unrelated_vector() {
    let source = scene();
    let native = source.render(&AtomicBool::new(false)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let pdf = directory.path().join("clipped-dither.pdf");
    std::fs::write(&pdf, omuse::vector_pdf::encode_scene(&source, 72.).unwrap()).unwrap();
    let prefix = directory.path().join("independent");
    let result = std::process::Command::new("pdftoppm")
        .args(["-r", "288", "-png", "-singlefile"])
        .arg(&pdf)
        .arg(&prefix)
        .output()
        .expect("Poppler must be installed for this explicit qualification");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let independent = image::open(prefix.with_extension("png")).unwrap().to_rgb8();
    if let Some(evidence) = std::env::var_os("OMUSE_PDF_TEST_EVIDENCE_DIR") {
        let evidence = std::path::PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::copy(&pdf, evidence.join("clipped-dither.pdf")).unwrap();
        native
            .save(evidence.join("native-transparent.png"))
            .unwrap();
        independent
            .save(evidence.join("poppler-white.png"))
            .unwrap();
    }
    assert_eq!(
        independent.dimensions(),
        (native.width() * 4, native.height() * 4)
    );
    // Poppler's native-resolution vector clip AA includes edge coverage on a
    // neighboring pixel. Sample each original pixel center at 4x resolution to
    // compare the actual image/clip geometry without changing exported artwork.
    // This covers every source pixel, including hole/outside/vector centers.
    let mut maximum = 0i32;
    let mut differences = 0usize;
    for (x, y, pixel) in native.enumerate_pixels() {
        for channel in 0..3 {
            let expected = (u32::from(pixel[channel]) * u32::from(pixel[3])
                + 255 * (255 - u32::from(pixel[3]))
                + 127)
                / 255;
            let error = (i32::from(independent.get_pixel(x * 4 + 2, y * 4 + 2)[channel])
                - expected as i32)
                .abs();
            maximum = maximum.max(error);
            if error > 3 {
                differences += 1;
            }
        }
    }
    println!(
        "Poppler all-source-pixel centers at 4x comparison: max channel error={maximum}; channels outside tolerance3={differences}/{}",
        64 * 48 * 3
    );
    assert_eq!(
        independent.get_pixel(2 * 4 + 2, 20 * 4 + 2).0,
        [255; 3],
        "nothing outside clipping silhouette"
    );
    assert_eq!(
        independent.get_pixel(23 * 4 + 2, 20 * 4 + 2).0,
        [255; 3],
        "compound hole remains empty"
    );
    assert_eq!(
        independent.get_pixel(52 * 4 + 2, 20 * 4 + 2).0,
        [230, 40, 60],
        "unrelated object retains vector appearance"
    );
    assert_eq!(
        differences, 0,
        "dither, clipping and opacity must agree with independent PDF pixels"
    );
}
