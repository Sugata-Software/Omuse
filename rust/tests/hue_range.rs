use image::{Rgba, RgbaImage};
use omuse::range_mask::{self, RangeKind, RangeSettings};

fn settings(tolerance: f32, feather: f32) -> RangeSettings {
    RangeSettings {
        kind: RangeKind::Hue {
            rgb: [255, 0, 0],
            tolerance_degrees: tolerance,
            feather_degrees: feather,
            minimum_saturation: 0.1,
        },
        invert: false,
    }
}

#[test]
fn hue_wraps_through_red_and_selects_across_lightness_without_selecting_greys() {
    let colors = [
        [255, 0, 25, 255],
        [255, 25, 0, 255],
        [90, 0, 0, 255],
        [255, 180, 180, 255],
        [128, 128, 128, 255],
        [0, 0, 255, 255],
    ];
    let source = RgbaImage::from_fn(colors.len() as u32, 1, |x, _| Rgba(colors[x as usize]));
    assert_eq!(
        range_mask::mask(&source, settings(10., 0.))
            .unwrap()
            .as_raw(),
        &[255, 255, 255, 255, 0, 0]
    );
}

#[test]
fn circular_falloff_is_analytic_and_alpha_is_applied_once() {
    let source = RgbaImage::from_fn(4, 1, |x, _| {
        Rgba(match x {
            0 => [255, 255, 0, 255], // yellow is exactly +60 degrees
            1 => [255, 0, 255, 255], // magenta is exactly -60 degrees
            2 => [255, 255, 0, 128],
            _ => [255, 0, 0, 0],
        })
    });
    assert_eq!(
        range_mask::mask(&source, settings(30., 60.))
            .unwrap()
            .as_raw(),
        &[128, 128, 64, 0]
    );
    let mut inverted = settings(30., 60.);
    inverted.invert = true;
    assert_eq!(
        range_mask::mask(&source, inverted).unwrap().as_raw(),
        &[128, 128, 64, 0]
    );
}

#[test]
fn saturation_protection_and_invalid_parameters_are_explicit() {
    let source = RgbaImage::from_fn(2, 1, |x, _| {
        Rgba(if x == 0 {
            [134, 122, 122, 255]
        } else {
            [255, 0, 0, 255]
        })
    });
    assert_eq!(
        range_mask::mask(&source, settings(20., 10.))
            .unwrap()
            .as_raw(),
        &[0, 255]
    );
    for tolerance in [-1., 181., f32::NAN, f32::INFINITY] {
        assert!(range_mask::mask(&source, settings(tolerance, 10.)).is_err());
    }
    assert!(
        range_mask::validate(RangeSettings {
            kind: RangeKind::Hue {
                rgb: [128, 128, 128],
                tolerance_degrees: 20.,
                feather_degrees: 10.,
                minimum_saturation: 0.,
            },
            invert: false
        })
        .unwrap_err()
        .to_string()
        .contains("grey")
    );
}
