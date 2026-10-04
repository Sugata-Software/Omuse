use omuse::{
    model::PixelRect,
    vector_path::{Anchor, Point, StrokeStyle, Subpath, VectorPath},
    vector_scene::{
        GradientFill, GradientKind, GradientSpread, GradientStop, StrokeCap, StrokeJoin,
        StrokeOptions, VectorObject, VectorScene,
    },
    vector_svg_scene,
};
use std::sync::atomic::AtomicBool;

fn gradient(kind: GradientKind) -> GradientFill {
    GradientFill {
        kind,
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [240, 30, 10, 255],
            },
            GradientStop {
                offset: 1.,
                color: [10, 30, 240, 255],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
    }
}
fn scene(width: u32, height: u32) -> VectorScene {
    let mut object = VectorObject::rectangle(
        "Paint",
        0.,
        0.,
        width as f32,
        height as f32,
        Some([240, 30, 10, 255]),
        None,
    )
    .unwrap();
    object.fill_gradient = Some(gradient(GradientKind::Linear {
        start: Point { x: 0., y: 0. },
        end: Point {
            x: width as f32,
            y: 0.,
        },
    }));
    VectorScene {
        version: 3,
        width,
        height,
        objects: vec![object],
    }
}
fn render(scene: &VectorScene) -> image::RgbaImage {
    scene.render(&AtomicBool::new(false)).unwrap()
}

#[test]
fn linear_gradient_is_continuous_across_tiles_and_region_exports() {
    let scene = scene(600, 24);
    let full = render(&scene);
    assert!(full.get_pixel(2, 12)[0] > 230);
    assert!(full.get_pixel(597, 12)[2] > 230);
    for x in [255, 256, 257, 511, 512] {
        assert!(full.get_pixel(x, 12)[0].abs_diff(full.get_pixel(x - 1, 12)[0]) <= 1);
    }
    let region = scene
        .render_region(
            PixelRect {
                x: 180,
                y: 5,
                width: 300,
                height: 10,
            },
            1.,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        region,
        image::imageops::crop_imm(&full, 180, 5, 300, 10).to_image()
    );
}

#[test]
fn radial_transform_and_alpha_keep_straight_color() {
    let mut scene = scene(80, 60);
    let fill = scene.objects[0].fill_gradient.as_mut().unwrap();
    fill.kind = GradientKind::Radial {
        center: Point { x: 20., y: 20. },
        focus: Point { x: 20., y: 20. },
        radius: 20.,
    };
    fill.transform = [1., 0., 0., 1., 20., 10.];
    fill.stops[0].color = [200, 80, 30, 128];
    fill.stops[1].color = [200, 80, 30, 0];
    let image = render(&scene);
    let center = image.get_pixel(40, 30);
    assert!((center[0] as i32 - 200).abs() <= 2 && (center[1] as i32 - 80).abs() <= 2);
    assert!((120..=128).contains(&center[3]));
    assert_eq!(image.get_pixel(75, 30)[3], 0);
}

#[test]
fn style_versions_and_limits_fail_closed() {
    let mut scene = scene(32, 32);
    for version in [1, 2, 5] {
        scene.version = version;
        assert!(scene.validate().is_err());
    }
    scene.version = 3;
    scene.objects[0].fill_gradient.as_mut().unwrap().stops[1].offset = -0.2;
    assert!(scene.validate().is_err());
    let mut scene = scene.clone();
    scene.objects[0].fill_gradient = None;
    scene.objects[0].stroke_options = Some(StrokeOptions::default());
    assert!(scene.validate().is_err());
    let invalid = StrokeOptions {
        dashes: vec![0.01, 5.],
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
    let old =
        VectorObject::rectangle("Legacy", 0., 0., 20., 20., Some([1, 2, 3, 255]), None).unwrap();
    let json = serde_json::to_string(&old).unwrap();
    assert!(!json.contains("fill_gradient") && !json.contains("stroke_options"));
}

fn line() -> VectorScene {
    let mut object = VectorObject::new(
        "Line",
        VectorPath {
            subpaths: vec![Subpath {
                anchors: vec![
                    Anchor {
                        position: Point { x: 10., y: 20. },
                        incoming: None,
                        outgoing: None,
                    },
                    Anchor {
                        position: Point { x: 90., y: 20. },
                        incoming: None,
                        outgoing: None,
                    },
                ],
                closed: false,
            }],
            fill_rule: Default::default(),
        },
        None,
        Some(StrokeStyle {
            color: [250, 100, 20, 255],
            width: 8.,
        }),
    );
    object.stroke_options = Some(StrokeOptions {
        cap: StrokeCap::Butt,
        join: StrokeJoin::Miter,
        dashes: vec![10., 10.],
        ..Default::default()
    });
    VectorScene {
        version: 3,
        width: 100,
        height: 40,
        objects: vec![object],
    }
}

#[test]
fn cap_dash_and_hit_test_agree() {
    let mut scene = line();
    let image = render(&scene);
    assert_eq!(image.get_pixel(7, 20)[3], 0);
    assert!(image.get_pixel(14, 20)[3] > 250);
    assert_eq!(image.get_pixel(25, 20)[3], 0);
    assert_eq!(scene.hit_test(Point { x: 25., y: 20. }, 0.).unwrap(), None);
    assert_eq!(
        scene.hit_test(Point { x: 14., y: 20. }, 0.).unwrap(),
        Some(0)
    );
    scene.objects[0].stroke_options.as_mut().unwrap().cap = StrokeCap::Square;
    assert!(render(&scene).get_pixel(7, 20)[3] > 250);
    scene.objects[0]
        .stroke_options
        .as_mut()
        .unwrap()
        .dash_offset = 10.;
    assert_eq!(render(&scene).get_pixel(14, 20)[3], 0);
}

#[test]
fn ordinary_dashed_artwork_can_render_at_fourfold_canvas_zoom() {
    let scene = vector_svg_scene::decode_scene(br##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480"><defs><radialGradient id="g"><stop stop-color="#C1E1DA"/><stop offset="1" stop-color="#223847"/></radialGradient></defs><rect x="128" y="96" width="384" height="288" fill="#D58049" stroke="#F2DAAF" stroke-width="4" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="16 10"/><ellipse cx="397" cy="202" rx="192" ry="144" fill="url(#g)" stroke="#F2DAAF" stroke-width="4" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="16 10"/></svg>"##).unwrap();
    let pixels = scene
        .render_region(
            PixelRect {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            },
            4.,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(pixels.dimensions(), (2560, 1920));
    assert!(pixels.get_pixel(1600, 800)[3] > 0);
}

#[test]
fn svg_round_trip_retains_gradient_transform_stops_and_strokes() {
    let input=br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="30" cy="25" fx="25" fy="22" r="20" gradientTransform="translate(10 5)" spreadMethod="reflect"><stop offset="0" stop-color="#EFAA30" stop-opacity="0.5"/><stop offset="0.4" stop-color="#70AACC"/><stop offset="1" stop-color="#172F40"/></radialGradient></defs><path d="M 10 10 L 90 10 L 90 70 L 10 70 Z" fill="url(#g)" stroke="#DDAA77" stroke-width="3" stroke-linecap="square" stroke-linejoin="bevel" stroke-dasharray="6 4" stroke-dashoffset="2"/></svg>"##;
    let scene = vector_svg_scene::decode_scene(input).unwrap();
    let tree = resvg::usvg::Tree::from_data(input, &resvg::usvg::Options::default()).unwrap();
    let mut reference = resvg::tiny_skia::Pixmap::new(100, 80).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut reference.as_mut(),
    );
    let rendered = render(&scene);
    let differing = rendered
        .pixels()
        .zip(reference.pixels())
        .filter(|(a, b)| {
            let b = b.demultiply();
            a.0.iter()
                .zip([b.red(), b.green(), b.blue(), b.alpha()])
                .any(|(a, b)| a.abs_diff(b) > 3)
        })
        .count();
    assert!(
        differing < 40,
        "Gradient/stroke appearance differs from independent SVG rasterizer: {differing} pixels"
    );
    assert_eq!(scene.version, 3);
    let out = vector_svg_scene::encode_scene(&scene).unwrap();
    let again = vector_svg_scene::decode_scene(out.as_bytes()).unwrap();
    assert_eq!(
        scene.objects[0].fill_gradient,
        again.objects[0].fill_gradient
    );
    assert_eq!(
        scene.objects[0].stroke_options,
        again.objects[0].stroke_options
    );
    assert_eq!(render(&scene), render(&again));
    for invalid in [
        r##"url(https://example.com/g.svg#g)"##,
        r##"url(#missing)"##,
    ] {
        let invalid = String::from_utf8(input.to_vec())
            .unwrap()
            .replace("url(#g)", invalid);
        assert!(vector_svg_scene::decode_scene(invalid.as_bytes()).is_err());
    }
}

#[test]
fn grouped_styled_scenes_do_not_downgrade_and_boolean_keeps_paint() {
    let mut scene = scene(60, 40);
    scene.objects.push(
        VectorObject::rectangle("Second", 20., 5., 30., 25., Some([50, 60, 70, 255]), None)
            .unwrap(),
    );
    let (grouped, _) = omuse::vector_scene_ops::group(&scene, &[0, 1], "Group").unwrap();
    assert_eq!(grouped.version, 3);
    let result = omuse::vector_boolean::combine(
        &scene.objects,
        omuse::vector_boolean::BooleanOperation::Union,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result[0].fill_gradient, scene.objects[0].fill_gradient);
    assert!(scene.render(&AtomicBool::new(true)).is_err());
}

#[test]
fn dash_expansion_is_bounded_before_rasterization() {
    let mut scene = line();
    scene.objects[0].path.subpaths[0].anchors[0].position.x = -900_000.;
    scene.objects[0].path.subpaths[0].anchors[1].position.x = 900_000.;
    scene.objects[0].stroke_options.as_mut().unwrap().dashes = vec![0.1, 0.1];
    assert!(
        scene
            .render(&AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("segment budget")
    );
    assert!(scene.hit_test(Point { x: 30., y: 20. }, 0.).is_err());
}
