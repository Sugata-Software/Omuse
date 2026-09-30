use image::{Rgba, RgbaImage};
use omuse::{
    dither::{self, Palette, PixelShape, Settings, Style},
    filters::{self, Filter},
};
use std::sync::atomic::AtomicBool;

fn image() -> RgbaImage {
    RgbaImage::from_fn(37, 29, |x, y| {
        Rgba([
            (x * 7 % 256) as u8,
            (y * 11 % 256) as u8,
            ((x + y) * 5 % 256) as u8,
            [0, 1, 64, 128, 254, 255][(x + y) as usize % 6],
        ])
    })
}
#[test]
fn every_dither_style_and_palette_is_repeatable_preserves_alpha_and_hidden_rgb() {
    let original = image();
    for style in Style::ALL {
        for palette in [Palette::BlackWhite, Palette::TwoColors, Palette::Original] {
            let settings = Settings {
                style,
                palette,
                pixel_size: 3,
                cell_size: 4,
                ..Default::default()
            };
            let mut a = original.clone();
            let mut b = original.clone();
            dither::apply(&mut a, &settings, &AtomicBool::new(false)).unwrap();
            filters::apply(&mut b, &Filter::Dither(settings)).unwrap();
            assert_eq!(a, b, "{style:?} {palette:?}");
            assert_ne!(
                a, original,
                "Effect must actually render {style:?} {palette:?}"
            );
            for (before, after) in original.pixels().zip(a.pixels()) {
                assert_eq!(before[3], after[3], "{style:?} {palette:?}");
                if before[3] == 0 {
                    assert_eq!(before, after);
                }
            }
        }
    }
}
#[test]
fn ordered_black_and_white_endpoints_and_tone_count_are_exact() {
    for style in [
        Style::Atkinson,
        Style::FloydSteinberg,
        Style::Bayer2,
        Style::Bayer4,
        Style::Bayer8,
    ] {
        for value in [0, 255] {
            let mut pixels = RgbaImage::from_pixel(17, 13, Rgba([value, value, value, 128]));
            let before = pixels.clone();
            dither::apply(
                &mut pixels,
                &Settings {
                    style,
                    pixel_size: 1,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(pixels, before, "{style:?}, {value}");
        }
        let mut pixels = image();
        dither::apply(
            &mut pixels,
            &Settings {
                style,
                pixel_size: 1,
                levels: 4,
                ..Default::default()
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            pixels
                .pixels()
                .filter(|p| p[3] != 0)
                .all(|p| [0, 85, 170, 255].contains(&p[0]) && p[0] == p[1] && p[1] == p[2])
        );
    }
}
#[test]
fn two_color_output_uses_the_requested_palette_including_dot_gaps() {
    let mut pixels = image();
    let settings = Settings {
        style: Style::Bayer4,
        palette: Palette::TwoColors,
        dark: [22, 64, 99],
        light: [231, 144, 81],
        pixel_size: 4,
        pixel_shape: PixelShape::Dot,
        ..Default::default()
    };
    filters::apply(&mut pixels, &Filter::Dither(settings.clone())).unwrap();
    assert!(
        pixels
            .pixels()
            .filter(|p| p[3] != 0)
            .all(|p| p.0[..3] == settings.dark || p.0[..3] == settings.light)
    );
}
#[test]
fn block_sampling_ignores_hidden_colour_and_keeps_partial_edge_alpha_exact() {
    let mut pixels = RgbaImage::from_fn(5, 3, |x, _| {
        if x == 4 {
            Rgba([255, 0, 0, 1])
        } else {
            Rgba([0, 0, 255, 0])
        }
    });
    pixels.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    let original = pixels.clone();
    let settings = Settings {
        palette: Palette::Original,
        pixel_size: 4,
        levels: 8,
        ..Default::default()
    };
    dither::apply(&mut pixels, &settings, &AtomicBool::new(false)).unwrap();
    assert_eq!(pixels.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(pixels.get_pixel(4, 0).0, [255, 0, 0, 1]);
    for (before, after) in original.pixels().zip(pixels.pixels()) {
        assert_eq!(before[3], after[3]);
        if before[3] == 0 {
            assert_eq!(before, after);
        }
    }
}
#[test]
fn transparent_hidden_pixels_cannot_change_diffusion_output() {
    let mut source_a = image();
    let mut source_b = source_a.clone();
    for p in source_a.pixels_mut().filter(|p| p[3] == 0) {
        p.0[..3].copy_from_slice(&[0, 0, 0]);
    }
    for p in source_b.pixels_mut().filter(|p| p[3] == 0) {
        p.0[..3].copy_from_slice(&[255, 0, 255]);
    }
    for style in [Style::Atkinson, Style::FloydSteinberg] {
        let mut a = source_a.clone();
        let mut b = source_b.clone();
        let settings = Settings {
            style,
            pixel_size: 1,
            palette: Palette::Original,
            ..Default::default()
        };
        dither::apply(&mut a, &settings, &AtomicBool::new(false)).unwrap();
        dither::apply(&mut b, &settings, &AtomicBool::new(false)).unwrap();
        for (a, b) in a.pixels().zip(b.pixels()) {
            if a[3] != 0 {
                assert_eq!(a, b);
            }
        }
    }
}
#[test]
fn dither_rejects_invalid_settings_and_cancel_without_changing_source() {
    let invalid = [
        Settings {
            pixel_size: 0,
            ..Default::default()
        },
        Settings {
            pixel_size: 33,
            ..Default::default()
        },
        Settings {
            cell_size: 65,
            ..Default::default()
        },
        Settings {
            levels: 1,
            ..Default::default()
        },
        Settings {
            angle: f32::NAN,
            ..Default::default()
        },
        Settings {
            diffusion: 1.01,
            ..Default::default()
        },
        Settings {
            characters: "\n".into(),
            ..Default::default()
        },
        Settings {
            characters: "é".into(),
            ..Default::default()
        },
        Settings {
            characters: "A".repeat(65),
            ..Default::default()
        },
    ];
    for settings in invalid {
        let mut pixels = image();
        let before = pixels.clone();
        assert!(dither::apply(&mut pixels, &settings, &AtomicBool::new(false)).is_err());
        assert_eq!(pixels, before);
    }
    let mut pixels = image();
    let before = pixels.clone();
    assert!(dither::apply(&mut pixels, &Settings::default(), &AtomicBool::new(true)).is_err());
    assert_eq!(pixels, before);
}
#[test]
fn ascii_character_ramp_is_order_independent_and_custom_glyphs_are_real() {
    let source = RgbaImage::from_pixel(64, 48, Rgba([150, 150, 150, 255]));
    let settings = Settings {
        style: Style::Ascii,
        pixel_size: 1,
        cell_size: 8,
        characters: " .#@".into(),
        ..Default::default()
    };
    let mut a = source.clone();
    let mut b = source.clone();
    dither::apply(&mut a, &settings, &AtomicBool::new(false)).unwrap();
    dither::apply(
        &mut b,
        &Settings {
            characters: "@#. ".into(),
            ..settings.clone()
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(a, b);
    assert!(a.pixels().any(|p| p[0] == 0) && a.pixels().any(|p| p[0] == 255));
    dither::apply(
        &mut b,
        &Settings {
            characters: "0".into(),
            ..settings
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_ne!(a, b);
}
#[test]
fn new_finishing_modes_leave_legacy_recipe_serialization_unchanged() {
    let legacy: Filter =
        serde_json::from_str(r#"{"Bloom":{"sigma":8.0,"amount":0.7,"threshold":0.6}}"#).unwrap();
    assert_eq!(
        legacy,
        Filter::Bloom {
            sigma: 8.,
            amount: 0.7,
            threshold: 0.6
        }
    );
    for filter in [
        Filter::Dither(Settings::default()),
        Filter::BloomGlow {
            sigma: 4.,
            amount: 1.,
            threshold: 0.2,
        },
        Filter::VignetteOverlay {
            opacity: 0.5,
            midpoint: 0.2,
            feather: 0.6,
            color: [1, 2, 3],
        },
        Filter::LocalContrast {
            sigma: 3.,
            shadows: 0.1,
            midtones: 0.2,
            highlights: 0.3,
        },
    ] {
        let json = serde_json::to_vec(&filter).unwrap();
        assert_eq!(serde_json::from_slice::<Filter>(&json).unwrap(), filter);
    }
}
#[test]
fn bloom_glow_fills_transparent_margin_without_hidden_colour_halo() {
    let mut source = RgbaImage::from_pixel(25, 25, Rgba([0, 0, 255, 0]));
    for y in 9..16 {
        for x in 9..16 {
            source.put_pixel(x, y, Rgba([255, 180, 100, 255]));
        }
    }
    let mut existing = source.clone();
    filters::apply(
        &mut existing,
        &Filter::Bloom {
            sigma: 3.,
            amount: 1.,
            threshold: 0.1,
        },
    )
    .unwrap();
    assert_eq!(existing.get_pixel(8, 12).0, [0, 0, 255, 0]);
    filters::apply(
        &mut source,
        &Filter::BloomGlow {
            sigma: 3.,
            amount: 1.,
            threshold: 0.1,
        },
    )
    .unwrap();
    let glow = source.get_pixel(8, 12);
    assert!(glow[3] > 0 && glow[3] < 255);
    assert!(
        glow[0] > glow[1] && glow[1] > glow[2],
        "Hidden blue must not tint the glow: {glow:?}"
    );
    assert_eq!(source.get_pixel(12, 12)[3], 255);
}
#[test]
fn empty_layer_vignette_is_coloured_alpha_overlay_and_selection_safe() {
    let mut document = omuse::model::Document::new(21, 21);
    document.layers[0].image = Some(RgbaImage::from_pixel(21, 21, Rgba([220, 40, 70, 0])).into());
    let mut editor = omuse::editor::Editor::new(document);
    editor.selection = Some(omuse::editor::Selection {
        width: 21,
        height: 21,
        mask: (0..441)
            .map(|i| if i % 21 < 10 { 0 } else { 255 })
            .collect(),
    });
    let before = editor.document.layers[0].image.clone().unwrap();
    assert!(editor.apply_filter(&Filter::VignetteOverlay {
        opacity: 0.8,
        midpoint: 0.2,
        feather: 0.6,
        color: [24, 60, 99]
    }));
    let result = editor.document.layers[0].image.as_ref().unwrap();
    assert_eq!(result.get_pixel(0, 0), before.get_pixel(0, 0));
    assert_eq!(result.get_pixel(10, 10), before.get_pixel(10, 10));
    assert_eq!(result.get_pixel(20, 0).0[..3], [24, 60, 99]);
    assert!(result.get_pixel(20, 0)[3] > 128);
    assert_eq!(editor.undo_depth(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document.layers[0].image.as_ref().unwrap(), &before);
}
#[test]
fn local_contrast_is_spatial_and_preserves_flat_fields_and_alpha() {
    let effect = Filter::LocalContrast {
        sigma: 2.,
        shadows: 0.7,
        midtones: 0.7,
        highlights: 0.7,
    };
    let mut flat = RgbaImage::from_pixel(15, 11, Rgba([100, 150, 180, 128]));
    let before = flat.clone();
    filters::apply(&mut flat, &effect).unwrap();
    assert_eq!(flat, before);
    let mut edge = RgbaImage::from_fn(17, 9, |x, _| Rgba([if x < 8 { 80 } else { 170 }; 4]));
    let original = edge.clone();
    filters::apply(&mut edge, &effect).unwrap();
    assert!(edge.get_pixel(7, 4)[0] < 80);
    assert!(edge.get_pixel(8, 4)[0] > 170);
    for (before, after) in original.pixels().zip(edge.pixels()) {
        assert_eq!(before[3], after[3]);
    }
}
#[test]
fn new_finishing_neutral_settings_and_invalid_values_are_transactional() {
    for filter in [
        Filter::BloomGlow {
            sigma: 2.,
            amount: 0.,
            threshold: 0.2,
        },
        Filter::BloomGlow {
            sigma: 2.,
            amount: 1.,
            threshold: 1.,
        },
        Filter::VignetteOverlay {
            opacity: 0.,
            midpoint: 0.2,
            feather: 0.6,
            color: [1, 2, 3],
        },
        Filter::LocalContrast {
            sigma: 0.,
            shadows: 1.,
            midtones: 1.,
            highlights: 1.,
        },
    ] {
        let mut pixels = image();
        let before = pixels.clone();
        filters::apply(&mut pixels, &filter).unwrap();
        assert_eq!(pixels, before);
    }
    for filter in [
        Filter::BloomGlow {
            sigma: 129.,
            amount: 1.,
            threshold: 0.2,
        },
        Filter::VignetteOverlay {
            opacity: 1.,
            midpoint: 0.2,
            feather: 0.,
            color: [1, 2, 3],
        },
        Filter::LocalContrast {
            sigma: 2.,
            shadows: f32::NAN,
            midtones: 1.,
            highlights: 1.,
        },
    ] {
        let mut pixels = image();
        let before = pixels.clone();
        assert!(filters::apply(&mut pixels, &filter).is_err());
        assert_eq!(pixels, before);
    }
}

#[test]
fn finishing_effects_never_silently_reduce_a_sixteen_bit_master() {
    use omuse::precision::{Rgba16Image, TiledRgba16};
    let pixels = Rgba16Image::from_pixel(3, 2, Rgba([12345, 23456, 45678, 54321]));
    let mut master = TiledRgba16::from_rgba16(&pixels).unwrap();
    for filter in [
        Filter::Dither(Settings::default()),
        Filter::BloomGlow {
            sigma: 3.,
            amount: 1.,
            threshold: 0.2,
        },
        Filter::VignetteOverlay {
            opacity: 0.5,
            midpoint: 0.2,
            feather: 0.5,
            color: [2, 3, 4],
        },
        Filter::LocalContrast {
            sigma: 3.,
            shadows: 0.3,
            midtones: 0.3,
            highlights: 0.3,
        },
    ] {
        let error = master.apply_filter(&filter).unwrap_err();
        assert!(error.to_string().contains("8-bit paint layer"));
        assert_eq!(master.to_rgba16(), pixels);
    }
}
