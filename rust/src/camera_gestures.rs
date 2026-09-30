//! Bounded, deterministic controls for stage-aligned Camera Raw canvas samples.
//! These functions change a settings draft, never image pixels or editor history.
use crate::camera_raw::{self, CurvePoint, Settings};
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Curve(usize),
    Hue,
    Saturation,
    Luminance,
}

fn sampled(pixel: [u8; 4]) -> Result<[f64; 3]> {
    ensure!(
        pixel[3] != 0,
        "Choose a visible pixel; transparent pixels have no colour"
    );
    Ok(camera_raw::stage_rgb(&image::Rgba(pixel)))
}

/// Solve the pipeline's linear RGB temperature/tint gains for a neutral sample.
/// Calibration has already been applied to this WhiteBalance-stage reference.
/// The bool reports a requested correction outside the supported control range.
pub fn white_balance(settings: &Settings, pixel: [u8; 4]) -> Result<(Settings, bool)> {
    camera_raw::validate(settings)?;
    let rgb = sampled(pixel)?;
    ensure!(
        rgb.iter().all(|v| (0.02..0.98).contains(v)),
        "Pick a neutral midtone away from black or clipped highlights"
    );
    let [r, g, b] = rgb.map(camera_raw::li);
    let (a, b0, c, d) = (0.35 * r, 0.15 * r + 0.3 * g, -0.35 * b, 0.15 * b + 0.3 * g);
    let determinant = a * d - b0 * c;
    ensure!(
        determinant.abs() > 1e-12,
        "This sample is too dark for white balance"
    );
    let temperature = ((g - r) * d - b0 * (g - b)) / determinant * 100.;
    let tint = (a * (g - b) - (g - r) * c) / determinant * 100.;
    let limited = temperature.abs() > 100. || tint.abs() > 100.;
    let mut next = settings.clone();
    next.temperature = temperature.clamp(-100., 100.) as f32;
    next.tint = tint.clamp(-100., 100.) as f32;
    camera_raw::validate(&next)?;
    Ok((next, limited))
}

/// Pick only the green/purple hue bands supported by the defringe controls.
/// Sample after lens/aberration correction, before defringe and vignetting.
pub fn defringe(settings: &Settings, pixel: [u8; 4]) -> Result<Settings> {
    camera_raw::validate(settings)?;
    let rgb = sampled(pixel)?;
    let (h, saturation, _) = camera_raw::rgb_hsl(rgb);
    ensure!(
        saturation >= 0.08,
        "Choose a coloured green or purple fringe"
    );
    let hue = (h * 360.) as f32;
    let green = (35. ..=175.).contains(&hue);
    ensure!(
        green || (210. ..=335.).contains(&hue),
        "Choose a green or purple fringe"
    );
    let mut next = settings.clone();
    if green {
        next.optics.green_hue_low = (hue - 20.).max(0.);
        next.optics.green_hue_high = (hue + 20.).min(360.);
        next.optics.green_amount = next.optics.green_amount.max(50.);
    } else {
        next.optics.purple_hue_low = (hue - 20.).max(0.);
        next.optics.purple_hue_high = (hue + 20.).min(360.);
        next.optics.purple_amount = next.optics.purple_amount.max(50.);
    }
    camera_raw::validate(&next)?;
    Ok(next)
}

/// Apply the total vertical drag to the initial settings, avoiding accumulated
/// rounding and event-rate dependence. Positive delta raises the chosen control.
pub fn targeted(
    settings: &Settings,
    pixel: [u8; 4],
    target: Target,
    delta: f32,
) -> Result<Settings> {
    camera_raw::validate(settings)?;
    ensure!(delta.is_finite(), "Drag distance must be finite");
    let rgb = sampled(pixel)?;
    if delta == 0. {
        return Ok(settings.clone());
    }
    let delta = delta.clamp(-1., 1.);
    let mut next = settings.clone();
    if let Target::Curve(channel) = target {
        ensure!(channel <= 3, "Choose an RGB, red, green or blue curve");
        let x = if channel == 0 {
            camera_raw::parametric(
                0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2],
                &settings.curve,
            )
        } else {
            camera_raw::apply_master_curve(rgb, settings)[channel - 1]
        }
        .clamp(0., 1.) as f32;
        let points = match channel {
            0 => &mut next.curve.rgb,
            1 => &mut next.curve.red,
            2 => &mut next.curve.green,
            _ => &mut next.curve.blue,
        };
        let y = (camera_raw::curve_value(f64::from(x), points) as f32 + delta).clamp(0., 1.);
        if let Some(index) = points
            .iter()
            .enumerate()
            .filter(|(_, p)| (p.x - x).abs() <= 0.015)
            .min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map(|(i, _)| i)
        {
            points[index].y = y;
        } else {
            ensure!(
                points.len() < 32,
                "This curve has 32 points; remove one before targeting another tone"
            );
            let index = points.partition_point(|point| point.x < x);
            points.insert(index, CurvePoint { x, y });
        }
    } else {
        let (h, saturation, _) = camera_raw::rgb_hsl(rgb);
        ensure!(
            saturation >= 0.02,
            "Choose a coloured area for a targeted HSL adjustment"
        );
        let hue = h * 360.;
        let values = match target {
            Target::Hue => &mut next.mixer.hue,
            Target::Saturation => &mut next.mixer.saturation,
            _ => &mut next.mixer.luminance,
        };
        for (index, center) in [0., 30., 60., 120., 180., 240., 270., 300.]
            .iter()
            .enumerate()
        {
            let distance = (hue - center).abs();
            let weight = (1. - distance.min(360. - distance) / 40.).max(0.);
            values[index] = (values[index] + delta * 100. * weight as f32).clamp(-100., 100.);
        }
    }
    camera_raw::validate(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camera_raw::{SampleStage, apply, apply_with_sample};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn white_balance_solves_actual_linear_gain_contract_and_bounds_extremes() {
        for alpha in [255, 128] {
            let pixel = [151, 139, 124, alpha];
            let (settings, limited) = white_balance(&Settings::default(), pixel).unwrap();
            assert!(!limited);
            let output = apply(
                &image::RgbaImage::from_pixel(1, 1, image::Rgba(pixel)),
                &settings,
            )
            .unwrap();
            let p = output.get_pixel(0, 0);
            assert!(
                p[0].abs_diff(p[1]) <= 2 && p[1].abs_diff(p[2]) <= 2,
                "{p:?}"
            );
            assert_eq!(p[3], alpha);
        }
        let (neutral, limited) = white_balance(&Settings::default(), [128, 128, 128, 255]).unwrap();
        assert_eq!((neutral.temperature, neutral.tint), (0., 0.));
        assert!(!limited);
        assert!(
            white_balance(&Settings::default(), [220, 30, 30, 255])
                .unwrap()
                .1
        );
        for p in [[128, 128, 128, 0], [0, 0, 0, 255], [255, 255, 255, 255]] {
            assert!(white_balance(&Settings::default(), p).is_err());
        }
    }

    #[test]
    fn sampled_stages_preserve_final_grade_and_optics_pick_matches_real_pipeline() {
        let source = image::RgbaImage::from_fn(12, 8, |x, y| {
            image::Rgba([
                (70 + x * 5) as u8,
                (40 + y * 4) as u8,
                150,
                if x == 0 { 0 } else { 192 },
            ])
        });
        let settings = Settings {
            temperature: 15.,
            exposure: 0.25,
            glow: 15.,
            grain_amount: 10.,
            optics: camera_raw::OpticsSettings {
                purple_amount: 30.,
                ..Default::default()
            },
            ..Default::default()
        };
        let expected = apply(&source, &settings).unwrap();
        for stage in [
            SampleStage::WhiteBalance,
            SampleStage::Curve,
            SampleStage::Mixer,
            SampleStage::PointColor,
            SampleStage::Optics,
        ] {
            let (output, sample) =
                apply_with_sample(&source, &settings, &AtomicBool::new(false), stage).unwrap();
            assert_eq!(output, expected);
            assert_eq!(sample.dimensions(), source.dimensions());
            assert_eq!(sample.get_pixel(0, 0)[3], 0);
            if stage == SampleStage::Optics {
                let mut before = settings.clone();
                before.optics = Default::default();
                before.detail = Default::default();
                assert_eq!(sample, apply(&source, &before).unwrap());
            }
        }
        let next = defringe(&Settings::default(), [170, 40, 190, 255]).unwrap();
        let original = image::RgbaImage::from_pixel(1, 1, image::Rgba([170, 40, 190, 255]));
        let reduced = apply(&original, &next).unwrap();
        let chroma = |p: &image::Rgba<u8>| {
            (*p.0[..3].iter().max().unwrap()) - (*p.0[..3].iter().min().unwrap())
        };
        assert!(chroma(reduced.get_pixel(0, 0)) < chroma(original.get_pixel(0, 0)));
        assert!(defringe(&next, [128, 128, 128, 255]).is_err());
        assert!(defringe(&next, [255, 0, 0, 255]).is_err());
    }

    #[test]
    fn defringe_reference_includes_lens_and_chromatic_aberration_corrections() {
        let source = image::RgbaImage::from_fn(32, 24, |x, y| {
            image::Rgba([
                (x * 7) as u8,
                (y * 10) as u8,
                if x % 2 == 0 { 190 } else { 35 },
                if x < 2 { 128 } else { 255 },
            ])
        });
        let settings = Settings {
            optics: camera_raw::OpticsSettings {
                distortion: 30.,
                remove_chromatic_aberration: true,
                purple_amount: 65.,
                green_amount: 30.,
                vignette_amount: -40.,
                ..Default::default()
            },
            ..Default::default()
        };
        let (graded, sample) = apply_with_sample(
            &source,
            &settings,
            &AtomicBool::new(false),
            SampleStage::Optics,
        )
        .unwrap();
        let mut reference = settings.clone();
        reference.optics.purple_amount = 0.;
        reference.optics.green_amount = 0.;
        reference.optics.vignette_amount = 0.;
        assert_eq!(sample, apply(&source, &reference).unwrap());
        assert_ne!(sample, source);
        assert_eq!(graded, apply(&source, &settings).unwrap());
    }

    #[test]
    fn targeted_hsl_uses_engine_hue_weights_and_total_drag_is_repeatable() {
        let original = Settings::default();
        let next = targeted(&original, [255, 0, 0, 255], Target::Saturation, -0.5).unwrap();
        assert_eq!(
            next.mixer.saturation,
            vec![-50., -12.5, 0., 0., 0., 0., 0., 0.]
        );
        let output = apply(
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])),
            &next,
        )
        .unwrap();
        assert!(output.get_pixel(0, 0)[1] > 0);
        assert_eq!(
            targeted(&original, [255, 0, 0, 255], Target::Saturation, -0.5).unwrap(),
            next
        );
        assert_eq!(
            targeted(&original, [80, 100, 120, 255], Target::Hue, 0.).unwrap(),
            original
        );
        assert!(targeted(&original, [128, 128, 128, 255], Target::Hue, 0.5).is_err());
        assert!(targeted(&original, [1, 2, 3, 0], Target::Luminance, 0.5).is_err());
        assert!(targeted(&original, [1, 2, 3, 255], Target::Hue, f32::NAN).is_err());
    }

    #[test]
    fn targeted_curves_use_the_actual_channel_input_and_keep_ordered_bounded_points() {
        let original = Settings::default();
        let next = targeted(&original, [128, 128, 128, 255], Target::Curve(0), 0.2).unwrap();
        assert_eq!(next.curve.rgb.len(), 3);
        assert!((next.curve.rgb[1].x - 128. / 255.).abs() < 1e-5);
        let output = apply(
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 128, 255])),
            &next,
        )
        .unwrap();
        assert!(output.get_pixel(0, 0)[0] >= 177);
        assert!(next.curve.red == original.curve.red && next.mixer == original.mixer);
        let huge = targeted(&next, [128, 128, 128, 255], Target::Curve(0), 10.).unwrap();
        assert!(huge.curve.rgb.windows(2).all(|p| p[0].x < p[1].x));
        assert!(huge.curve.rgb.iter().all(|p| (0. ..=1.).contains(&p.y)));
        assert!(targeted(&original, [1, 2, 3, 255], Target::Curve(4), 0.1).is_err());
    }
}
