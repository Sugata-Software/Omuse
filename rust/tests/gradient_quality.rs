use omuse::{
    compatibility_report::CompatibilityReport,
    model::{Document, PixelRect},
    vector_path::{Point, StrokeStyle},
    vector_scene::{
        GradientFill, GradientKind, GradientMethod, GradientSpread, GradientStop, VectorObject,
        VectorScene,
    },
    vector_svg_scene,
};
use std::{path::Path, sync::atomic::AtomicBool};

fn scene() -> VectorScene {
    let mut object =
        VectorObject::rectangle("Quality", 0., 0., 256., 64., Some([0, 0, 255, 255]), None)
            .unwrap();
    object.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 0., y: 0. },
            end: Point { x: 256., y: 0. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [0, 0, 255, 255],
            },
            GradientStop {
                offset: 1.,
                color: [255, 255, 0, 255],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
        method: GradientMethod::Classic,
        dither: false,
    });
    VectorScene {
        version: 6,
        width: 256,
        height: 64,
        objects: vec![object],
    }
}
fn svg_pixels(svg: &str) -> image::RgbaImage {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).unwrap();
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(tree.size().width() as u32, tree.size().height() as u32)
            .unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    image::RgbaImage::from_fn(pixmap.width(), pixmap.height(), |x, y| {
        let p = pixmap.pixel(x, y).unwrap().demultiply();
        image::Rgba([p.red(), p.green(), p.blue(), p.alpha()])
    })
}

#[test]
fn native_paint_guards_preserve_classic_files_and_reopen_extended_master_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paints.omuse");
    let mut artwork = scene();
    let mut document = Document::new(artwork.width, artwork.height);
    document.layers[0].image = Some(artwork.render(&AtomicBool::new(false)).unwrap().into());
    document.layers[0].vector_scene = Some(std::sync::Arc::new(artwork.clone()));
    omuse::document::save(&document, &path).unwrap();
    let read_manifest = || {
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(path.join("manifest.json")).unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        read_manifest()["version"],
        16,
        "ordinary paint keeps the prior file identity"
    );
    assert_eq!(
        omuse::document::open(&path).unwrap().layers[0]
            .vector_scene
            .as_ref()
            .unwrap()
            .version,
        5
    );
    let paint = artwork.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.dither = true;
    let expected = artwork.render(&AtomicBool::new(false)).unwrap();
    document.layers[0].image = Some(expected.clone().into());
    document.layers[0].vector_scene = Some(std::sync::Arc::new(artwork));
    omuse::document::save(&document, &path).unwrap();
    assert_eq!(read_manifest()["version"], 18);
    let reopened = omuse::document::open(&path).unwrap();
    assert_eq!(
        reopened.layers[0]
            .vector_scene
            .as_ref()
            .unwrap()
            .render(&AtomicBool::new(false))
            .unwrap(),
        expected
    );
    let mut downgraded = read_manifest();
    downgraded["version"] = serde_json::json!(16);
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&downgraded).unwrap(),
    )
    .unwrap();
    let error = omuse::document::open(&path).unwrap_err().to_string();
    assert!(error.contains("format 18"), "{error}");
}

#[test]
fn classic_defaults_do_not_change_old_serialized_paints_or_pixels() {
    let s = scene();
    let paint = s.objects[0].fill_gradient.as_ref().unwrap();
    let encoded = serde_json::to_string(paint).unwrap();
    assert!(!encoded.contains("method") && !encoded.contains("dither"));
    let restored: GradientFill = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, paint.clone());
    assert!(!restored.requires_extended_format());
    let mut old = s.clone();
    old.version = 3;
    assert_eq!(
        old.render(&AtomicBool::new(false)).unwrap(),
        s.render(&AtomicBool::new(false)).unwrap()
    );
}

#[test]
fn perceptual_blue_yellow_has_the_expected_light_blue_grey_midpoint() {
    let mut s = scene();
    s.objects[0].fill_gradient.as_mut().unwrap().method = GradientMethod::Perceptual;
    let image = s.render(&AtomicBool::new(false)).unwrap();
    let p = image.get_pixel(127, 32).0;
    for (got, want) in p[..3].iter().zip([108i32, 171, 199]) {
        assert!((i32::from(*got) - want).abs() <= 3, "{p:?}");
    }
    assert_eq!(p[3], 255);
    let classic = scene()
        .render(&AtomicBool::new(false))
        .unwrap()
        .get_pixel(127, 32)
        .0;
    assert!((i32::from(classic[0]) - i32::from(classic[2])).abs() <= 2);
}

#[test]
fn perceptual_expansion_is_bounded_and_preserves_original_stop_data() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.stops = (0..16)
        .map(|i| GradientStop {
            offset: i as f32 / 15.,
            color: [
                (i * 17) as u8,
                ((15 - i) * 17) as u8,
                if i % 2 == 0 { 255 } else { 0 },
                if i % 3 == 0 { 0 } else { 255 },
            ],
        })
        .collect();
    let original = paint.clone();
    let expanded = paint.expanded_stops().unwrap();
    assert!(expanded.len() <= 4096 && expanded.len() > 16);
    assert!(expanded.windows(2).all(|w| w[0].0 <= w[1].0));
    assert!(expanded.iter().flat_map(|s| s.1).all(f32::is_finite));
    assert_eq!(*paint, original);
}

#[test]
fn transparent_stop_colours_cannot_fringe_and_dither_keeps_alpha() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.stops[0].color = [255, 0, 0, 0];
    paint.stops[1].color = [0, 0, 255, 255];
    let smooth = s.render(&AtomicBool::new(false)).unwrap();
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let noisy = s.render(&AtomicBool::new(false)).unwrap();
    for (a, b) in smooth.pixels().zip(noisy.pixels()) {
        assert_eq!(a[3], b[3]);
        if a[3] > 16 {
            assert!(a[0] <= 2 && a[1] <= 2, "{a:?}");
        }
        if b[3] == 0 {
            assert_eq!(b.0, [0; 4]);
        }
    }
}

#[test]
fn classic_dither_preserves_straight_rgba_shader_appearance_at_transparent_stops() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.stops[0].color = [255, 0, 0, 0];
    paint.stops[1].color = [0, 0, 255, 255];
    let classic = s.render(&AtomicBool::new(false)).unwrap();
    let middle = classic.get_pixel(127, 32).0;
    assert!(middle[0] > 120 && middle[2] > 120, "{middle:?}");
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let dithered = s.render(&AtomicBool::new(false)).unwrap();
    for (a, b) in classic.pixels().zip(dithered.pixels()) {
        assert_eq!(a[3], b[3]);
        for c in 0..3 {
            // Recover the actual premultiplied 8-bit sample. Comparing raw
            // floats would count straight-RGBA demultiplication rounding twice.
            let associated =
                |p: &image::Rgba<u8>| (f32::from(p[c]) * f32::from(p[3]) / 255.).round();
            assert!(
                (associated(a) - associated(b)).abs() <= 1.,
                "{a:?} -> {b:?}"
            );
        }
    }
}

#[test]
fn perceptual_transparent_hidden_red_to_black_has_no_native_or_svg_fringe() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.stops[0].color = [255, 0, 0, 0];
    paint.stops[1].color = [0, 0, 0, 255];
    let original = paint.stops.clone();
    let expanded = paint.expanded_stops().unwrap();
    assert_eq!(expanded[0].1, [0.; 4]);
    let native = s.render(&AtomicBool::new(false)).unwrap();
    let independent = svg_pixels(&vector_svg_scene::encode_scene(&s).unwrap());
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let dithered = s.render(&AtomicBool::new(false)).unwrap();
    for image in [&native, &independent, &dithered] {
        for p in image.pixels() {
            assert_eq!(&p.0[..3], &[0, 0, 0], "{p:?}");
        }
    }
    assert_eq!(s.objects[0].fill_gradient.as_ref().unwrap().stops, original);
}

#[test]
fn perceptual_transparent_middle_stop_retains_both_adjacent_colour_limits() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.stops = vec![
        GradientStop {
            offset: 0.,
            color: [0, 0, 255, 255],
        },
        GradientStop {
            offset: 0.5,
            color: [255, 0, 0, 0],
        },
        GradientStop {
            offset: 1.,
            color: [0, 255, 0, 255],
        },
    ];
    let expanded = paint.expanded_stops().unwrap();
    let middle = expanded
        .iter()
        .filter(|(offset, _)| *offset == 0.5)
        .collect::<Vec<_>>();
    assert_eq!(middle.len(), 2);
    assert!(middle[0].1[2] > 0.99 && middle[0].1[3] == 0.);
    assert!(middle[1].1[1] > 0.99 && middle[1].1[3] == 0.);
    let native = s.render(&AtomicBool::new(false)).unwrap();
    let independent = svg_pixels(&vector_svg_scene::encode_scene(&s).unwrap());
    for image in [&native, &independent] {
        for x in 1..127 {
            let p = image.get_pixel(x, 32);
            assert!(p[0] <= 1 && p[1] <= 1, "left{x}: {p:?}");
        }
        for x in 129..255 {
            let p = image.get_pixel(x, 32);
            assert!(p[0] <= 1 && p[2] <= 1, "right{x}: {p:?}");
        }
    }
}

#[test]
fn dither_is_repeatable_and_region_consistent_after_transform() {
    let mut s = scene();
    s.objects[0].transform = [1., 0., 0., 1., 0.5, 0.5];
    let smooth = s.render(&AtomicBool::new(false)).unwrap();
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let noisy = s.render(&AtomicBool::new(false)).unwrap();
    assert_eq!(noisy, s.render(&AtomicBool::new(false)).unwrap());
    assert!(
        smooth
            .pixels()
            .zip(noisy.pixels())
            .filter(|(a, b)| a != b)
            .count()
            > 100
    );
    let region = s
        .render_region(
            PixelRect {
                x: 35,
                y: 11,
                width: 77,
                height: 27,
            },
            1.,
            &AtomicBool::new(false),
        )
        .unwrap();
    for (x, y, p) in region.enumerate_pixels() {
        assert_eq!(p, noisy.get_pixel(x + 35, y + 11), "region mismatch{x},{y}");
    }
}

#[test]
fn dither_does_not_modify_the_stroke_painted_over_the_fill() {
    let mut s = scene();
    s.objects[0].stroke = Some(StrokeStyle {
        color: [34, 56, 78, 255],
        width: 8.,
    });
    let smooth = s.render(&AtomicBool::new(false)).unwrap();
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let noisy = s.render(&AtomicBool::new(false)).unwrap();
    assert_eq!(smooth.get_pixel(2, 25), noisy.get_pixel(2, 25));
    assert_eq!(noisy.get_pixel(2, 25).0, [34, 56, 78, 255]);
}

#[test]
fn perceptual_svg_preserves_vector_appearance_with_an_explicit_conversion_report() {
    let mut s = scene();
    s.objects[0].fill_gradient.as_mut().unwrap().method = GradientMethod::Perceptual;
    let native = s.render(&AtomicBool::new(false)).unwrap();
    let svg = vector_svg_scene::encode_scene(&s).unwrap();
    assert!(!svg.contains("<image"));
    assert!(svg.matches("<stop ").count() > 2);
    let independent = svg_pixels(&svg);
    for x in 0..256 {
        for c in 0..4 {
            assert!(
                (i32::from(native.get_pixel(x, 32)[c])
                    - i32::from(independent.get_pixel(x, 32)[c]))
                .abs()
                    <= 3,
                "x{x}"
            );
        }
    }
    let report = CompatibilityReport::vector_export(Path::new("quality.svg"), Some(&s), false);
    assert!(report.converted.iter().any(|s| s.contains("Perceptual")));
}

#[test]
fn dither_export_preserves_clipping_opacity_and_other_vector_objects() {
    let mut s = scene();
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    s.objects[0].opacity = 0.6;
    let mask =
        VectorObject::rectangle("Mask", 20., 10., 100., 40., Some([0, 0, 0, 255]), None).unwrap();
    s.objects.push(mask);
    let (mut s, _) = omuse::vector_clip::make(&s, &[0, 1]).unwrap();
    s.objects.push(
        VectorObject::rectangle(
            "Still vector",
            170.,
            20.,
            30.,
            20.,
            Some([255, 0, 0, 255]),
            None,
        )
        .unwrap(),
    );
    let native = s.render(&AtomicBool::new(false)).unwrap();
    let svg = vector_svg_scene::encode_scene(&s).unwrap();
    assert!(svg.contains("<image") && svg.contains("Still vector") && svg.contains("<path"));
    let exported = svg_pixels(&svg);
    assert_eq!(exported.get_pixel(5, 30).0, [0; 4]);
    assert_eq!(exported.get_pixel(180, 30).0, [255, 0, 0, 255]);
    for x in 25..115 {
        for c in 0..4 {
            assert!(
                (i32::from(native.get_pixel(x, 30)[c]) - i32::from(exported.get_pixel(x, 30)[c]))
                    .abs()
                    <= 2,
                "x{x},c{c}"
            );
        }
    }
    let report = CompatibilityReport::vector_export(Path::new("dither.pdf"), Some(&s), true);
    assert!(
        report
            .converted
            .iter()
            .any(|s| s.contains("embedded pixels"))
    );
    assert!(
        omuse::vector_pdf::encode_scene(&s, 96.)
            .unwrap()
            .starts_with(b"%PDF")
    );
}

#[test]
fn dither_export_refuses_oversized_conversion_before_rendering() {
    let mut s = scene();
    s.width = 4096;
    s.height = 2048;
    s.objects[0].fill_gradient.as_mut().unwrap().dither = true;
    let error = vector_svg_scene::encode_scene(&s).unwrap_err();
    assert!(error.to_string().contains("export budget"));
}

#[test]
fn dither_recovers_fractional_ramp_means_instead_of_adding_noise_after_quantization() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.dither = true;
    paint.stops[0].color = [100, 100, 100, 255];
    paint.stops[1].color = [101, 101, 101, 255];
    let pixels = s.render(&AtomicBool::new(false)).unwrap();
    let mut mean_error = 0f64;
    for x in 0..256 {
        let mean = (0..64)
            .map(|y| f64::from(pixels.get_pixel(x, y)[0]))
            .sum::<f64>()
            / 64.;
        let expected = 100. + (f64::from(x) + 0.5) / 256.;
        let error = (mean - expected).abs();
        assert!(error < 0.26, "x{x} {mean} vs {expected}");
        mean_error += error;
    }
    assert!(
        mean_error / 256. < 0.07,
        "mean quantization error {}",
        mean_error / 256.
    );
}

#[test]
fn extended_gradient_master_survives_native_save_and_reopen() {
    let mut s = scene();
    let paint = s.objects[0].fill_gradient.as_mut().unwrap();
    paint.method = GradientMethod::Perceptual;
    paint.dither = true;
    let mut document = Document::new(256, 64);
    let layer = &mut document.layers[0];
    layer.vector_scene = Some(std::sync::Arc::new(s.clone()));
    layer.image = Some(s.render(&AtomicBool::new(false)).unwrap().into());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Quality.omuse");
    omuse::document::save(&document, &path).unwrap();
    let reopened = omuse::document::open(&path).unwrap();
    let saved = reopened.layers[0].vector_scene.as_ref().unwrap();
    assert_eq!(saved.objects[0].fill_gradient, s.objects[0].fill_gradient);
    assert_eq!(
        saved.render(&AtomicBool::new(false)).unwrap(),
        s.render(&AtomicBool::new(false)).unwrap()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest["version"].as_u64().unwrap() >= 18);
}
