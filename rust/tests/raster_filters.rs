use image::{Rgba, RgbaImage};
use omuse::filters::{self, Filter};
fn pixel() -> RgbaImage {
    RgbaImage::from_raw(
        3,
        1,
        vec![120, 80, 210, 255, 19, 73, 111, 128, 9, 99, 200, 0],
    )
    .unwrap()
}
#[test]
fn neutral_settings_are_exact_identity() {
    let filters = [
        Filter::Exposure { stops: 0.0 },
        Filter::Levels {
            black: 0.0,
            white: 1.0,
            gamma: 1.0,
        },
        Filter::Curves {
            points: vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
        },
        Filter::Hsl {
            hue_degrees: 0.0,
            saturation: 0.0,
            lightness: 0.0,
        },
        Filter::ColorBalance {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        },
        Filter::GaussianBlur { sigma: 0.0 },
        Filter::UnsharpMask {
            sigma: 2.0,
            amount: 0.0,
            threshold: 0.0,
        },
        Filter::Noise {
            amount: 0.0,
            seed: 42,
            monochrome: false,
        },
        Filter::Vignette {
            amount: 0.0,
            midpoint: 0.5,
            feather: 0.5,
        },
        Filter::Bloom {
            sigma: 2.0,
            amount: 0.0,
            threshold: 0.5,
        },
        Filter::TonalContrast {
            shadows: 0.0,
            midtones: 0.0,
            highlights: 0.0,
        },
    ];
    for filter in filters {
        let mut img = pixel();
        let before = img.clone();
        filters::apply(&mut img, &filter).unwrap();
        assert_eq!(img, before, "{filter:?}");
    }
}
#[test]
fn all_filters_preserve_alpha_and_fully_transparent_rgb() {
    let cases = [
        Filter::Exposure { stops: 1.0 },
        Filter::Levels {
            black: 0.1,
            white: 0.8,
            gamma: 1.5,
        },
        Filter::Curves {
            points: vec![(0.0, 1.0), (1.0, 0.0)],
        },
        Filter::Hsl {
            hue_degrees: 60.0,
            saturation: 0.2,
            lightness: 0.1,
        },
        Filter::ColorBalance {
            red: 0.2,
            green: -0.1,
            blue: 0.0,
        },
        Filter::GaussianBlur { sigma: 2.0 },
        Filter::UnsharpMask {
            sigma: 2.0,
            amount: 1.0,
            threshold: 0.0,
        },
        Filter::Noise {
            amount: 0.2,
            seed: 42,
            monochrome: false,
        },
        Filter::Vignette {
            amount: 0.7,
            midpoint: 0.1,
            feather: 0.5,
        },
        Filter::Bloom {
            sigma: 2.0,
            amount: 1.0,
            threshold: 0.1,
        },
        Filter::TonalContrast {
            shadows: 0.3,
            midtones: -0.3,
            highlights: 0.2,
        },
        Filter::Invert,
        Filter::Grayscale,
    ];
    for filter in cases {
        let mut img = pixel();
        filters::apply(&mut img, &filter).unwrap();
        assert_eq!(
            img.pixels().map(|p| p[3]).collect::<Vec<_>>(),
            vec![255, 128, 0],
            "{filter:?}"
        );
        assert_eq!(img.get_pixel(2, 0).0, [9, 99, 200, 0], "{filter:?}");
    }
}
#[test]
fn invalid_settings_do_not_mutate_any_pixel() {
    let invalid = [
        Filter::Exposure { stops: f32::NAN },
        Filter::Levels {
            black: 0.8,
            white: 0.2,
            gamma: 1.0,
        },
        Filter::Levels {
            black: 0.0,
            white: 1.0,
            gamma: 0.0,
        },
        Filter::Curves {
            points: vec![(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)],
        },
        Filter::Curves {
            points: vec![(0.0, 0.0), (1.0, f32::INFINITY)],
        },
        Filter::GaussianBlur { sigma: -1.0 },
        Filter::Noise {
            amount: 1.1,
            seed: 0,
            monochrome: false,
        },
        Filter::Vignette {
            amount: 0.5,
            midpoint: 0.5,
            feather: 0.0,
        },
    ];
    for filter in invalid {
        let mut img = pixel();
        let before = img.clone();
        assert!(filters::apply(&mut img, &filter).is_err());
        assert_eq!(img, before);
    }
}
#[test]
fn levels_curves_and_hue_have_known_results() {
    let mut img = RgbaImage::from_pixel(1, 1, Rgba([64, 128, 192, 127]));
    filters::apply(
        &mut img,
        &Filter::Levels {
            black: 64.0 / 255.0,
            white: 192.0 / 255.0,
            gamma: 1.0,
        },
    )
    .unwrap();
    assert_eq!(img.get_pixel(0, 0).0, [0, 128, 255, 127]);
    let mut img = RgbaImage::from_pixel(1, 1, Rgba([12, 100, 234, 255]));
    filters::apply(
        &mut img,
        &Filter::Curves {
            points: vec![(0.0, 1.0), (1.0, 0.0)],
        },
    )
    .unwrap();
    assert_eq!(img.get_pixel(0, 0).0, [243, 155, 21, 255]);
    let mut img = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128]));
    filters::apply(
        &mut img,
        &Filter::Hsl {
            hue_degrees: 120.0,
            saturation: 0.0,
            lightness: 0.0,
        },
    )
    .unwrap();
    assert_eq!(img.get_pixel(0, 0).0, [0, 255, 0, 128]);
}
#[test]
fn exposure_operates_on_linear_light() {
    let mut img = RgbaImage::from_pixel(1, 1, Rgba([128, 0, 255, 255]));
    filters::apply(&mut img, &Filter::Exposure { stops: 1.0 }).unwrap();
    assert_eq!(img.get_pixel(0, 0).0, [176, 0, 255, 255]);
}
#[test]
fn blur_preserves_constant_field_and_ignores_transparent_rgb() {
    let mut constant = RgbaImage::from_pixel(9, 7, Rgba([30, 80, 150, 128]));
    let before = constant.clone();
    filters::apply(&mut constant, &Filter::GaussianBlur { sigma: 3.0 }).unwrap();
    assert_eq!(constant, before);
    let mut edge =
        RgbaImage::from_raw(3, 1, vec![255, 0, 0, 255, 255, 0, 0, 128, 0, 0, 255, 0]).unwrap();
    filters::apply(&mut edge, &Filter::GaussianBlur { sigma: 3.0 }).unwrap();
    assert_eq!(edge.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(edge.get_pixel(1, 0).0, [255, 0, 0, 128]);
}
#[test]
fn blur_spreads_impulse_and_unsharp_increases_edge_contrast() {
    let mut img = RgbaImage::from_pixel(9, 1, Rgba([0, 0, 0, 255]));
    img.put_pixel(4, 0, Rgba([255, 255, 255, 255]));
    filters::apply(&mut img, &Filter::GaussianBlur { sigma: 1.5 }).unwrap();
    assert!(img.get_pixel(4, 0)[0] < 255);
    assert!(img.get_pixel(3, 0)[0] > 0);
    assert_eq!(img.get_pixel(3, 0), img.get_pixel(5, 0));
    let mut edge = RgbaImage::from_raw(
        3,
        1,
        vec![50, 50, 50, 255, 100, 100, 100, 255, 150, 150, 150, 255],
    )
    .unwrap();
    filters::apply(
        &mut edge,
        &Filter::UnsharpMask {
            sigma: 2.0,
            amount: 1.0,
            threshold: 0.0,
        },
    )
    .unwrap();
    assert!(edge.get_pixel(0, 0)[0] < 50);
    assert!(edge.get_pixel(2, 0)[0] > 150);
}
#[test]
fn noise_is_repeatable_and_monochrome_channels_match() {
    let mut a = RgbaImage::from_pixel(10, 10, Rgba([128; 4]));
    let mut b = a.clone();
    let f = Filter::Noise {
        amount: 0.2,
        seed: 1234,
        monochrome: true,
    };
    filters::apply(&mut a, &f).unwrap();
    filters::apply(&mut b, &f).unwrap();
    assert_eq!(a, b);
    assert!(a.pixels().any(|p| p[0] != 128));
    assert!(a.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
}
#[test]
fn vignette_changes_edges_more_than_center_and_bloom_threshold_is_respected() {
    let mut img = RgbaImage::from_pixel(9, 9, Rgba([128, 128, 128, 255]));
    filters::apply(
        &mut img,
        &Filter::Vignette {
            amount: 1.0,
            midpoint: 0.2,
            feather: 0.6,
        },
    )
    .unwrap();
    assert_eq!(img.get_pixel(4, 4)[0], 128);
    assert!(img.get_pixel(0, 0)[0] < 32);
    let mut img = RgbaImage::from_pixel(9, 9, Rgba([128, 128, 128, 255]));
    let before = img.clone();
    filters::apply(
        &mut img,
        &Filter::Bloom {
            sigma: 2.0,
            amount: 1.0,
            threshold: 1.0,
        },
    )
    .unwrap();
    assert_eq!(img, before);
    filters::apply(
        &mut img,
        &Filter::Bloom {
            sigma: 2.0,
            amount: 1.0,
            threshold: 0.0,
        },
    )
    .unwrap();
    assert!(img.get_pixel(4, 4)[0] > 128);
}
