//! Numeric Camera Raw controls use the same units and limits as its validator.
//! Curves, booleans and enumerated choices retain their existing controls.
use super::Spec;
use crate::camera_controls::Field;
use serde_json::Value;
use std::sync::OnceLock;

pub(super) fn field_spec(field: &Field) -> Option<Spec> {
    field.value.as_f64()?;
    let path = field.path.as_str();
    if path.starts_with("/curve/") || path == "/calibration/process" {
        return None;
    }
    let (min, max, step, decimals) = match path {
        "/exposure" => (-5., 5., 0.05, 3),
        "/temperature" | "/tint" | "/contrast" | "/highlights" | "/shadows" | "/whites"
        | "/blacks" | "/vibrance" | "/saturation" | "/texture" | "/clarity" | "/dehaze"
        | "/glowRange" | "/glowSpread" | "/glowWarmth" | "/vignetteAmount"
        | "/vignetteRoundness" => (-100., 100., 1., 2),
        "/glow"
        | "/vignetteMidpoint"
        | "/vignetteFeather"
        | "/vignetteHighlights"
        | "/grainAmount"
        | "/grainSize"
        | "/grainRoughness" => (0., 100., 1., 2),
        p if p.starts_with("/mixer/points/") => match p.rsplit('/').next()? {
            "hue" => (0., 360., 1., 2),
            "saturation" | "luminance" => (0., 1., 0.01, 3),
            "hueShift" | "saturationShift" | "luminanceShift" => (-100., 100., 1., 2),
            "hueRange" => (5., 180., 1., 2),
            "saturationRange" | "luminanceRange" => (0.05, 1., 0.01, 3),
            _ => return None,
        },
        p if p.starts_with("/mixer/hue/")
            || p.starts_with("/mixer/saturation/")
            || p.starts_with("/mixer/luminance/") =>
        {
            (-100., 100., 1., 2)
        }
        "/grading/blending" => (0., 100., 1., 2),
        "/grading/balance" => (-100., 100., 1., 2),
        p if p.starts_with("/grading/") => match p.rsplit('/').next()? {
            "hue" => (0., 360., 1., 2),
            "saturation" => (0., 100., 1., 2),
            "luminance" => (-100., 100., 1., 2),
            _ => return None,
        },
        "/detail/sharpenAmount" => (0., 150., 1., 2),
        p if p.starts_with("/detail/") => (0., 100., 1., 2),
        "/optics/distortion" | "/optics/vignetteAmount" => (-100., 100., 1., 2),
        "/optics/purpleHueLow"
        | "/optics/purpleHueHigh"
        | "/optics/greenHueLow"
        | "/optics/greenHueHigh" => (0., 360., 1., 2),
        p if p.starts_with("/optics/") => (0., 100., 1., 2),
        "/geometry/rotate" => (-45., 45., 0.1, 3),
        p if p.starts_with("/geometry/guides/") => (0., 1., 0.01, 3),
        p if p.starts_with("/geometry/") || p.starts_with("/calibration/") => (-100., 100., 1., 2),
        _ => return None,
    };
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    let defaults = DEFAULTS.get_or_init(|| {
        let mut value = serde_json::to_value(omuse::camera_raw::Settings::default()).unwrap();
        value["mixer"]["points"] = serde_json::json!([omuse::camera_raw::PointColor::default()]);
        value["geometry"]["guides"] =
            serde_json::json!([omuse::camera_raw::GeometryGuide::default()]);
        value
    });
    let default_path = if path.starts_with("/mixer/points/") {
        format!("/mixer/points/0/{}", path.rsplit('/').next()?)
    } else if path.starts_with("/geometry/guides/") {
        format!("/geometry/guides/0/{}", path.rsplit('/').next()?)
    } else {
        path.to_owned()
    };
    let default = defaults.pointer(&default_path)?.as_f64()?;
    Some(Spec::new(min, max, step, default, decimals))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_numeric_defaults_are_valid_and_every_supported_field_has_bounded_units() {
        let mut settings = omuse::camera_raw::Settings::default();
        settings.mixer.points.push(Default::default());
        settings.geometry.guides.push(Default::default());
        let value = serde_json::to_value(&settings).unwrap();
        for section in 0..crate::camera_controls::SECTIONS.len() {
            for field in crate::camera_controls::fields(&value, section) {
                let spec = field_spec(&field);
                if !field.value.is_number()
                    || field.path.starts_with("/curve/")
                    || field.path == "/calibration/process"
                {
                    assert!(spec.is_none(), "{}", field.path);
                    continue;
                }
                let spec =
                    spec.unwrap_or_else(|| panic!("Missing numeric limits for {}", field.path));
                assert_eq!(
                    spec.parse(&spec.default.to_string()).unwrap(),
                    spec.default,
                    "{}",
                    field.path
                );
                assert!(
                    spec.parse(&(spec.min - spec.step).to_string()).is_err(),
                    "{}",
                    field.path
                );
                assert!(
                    spec.parse(&(spec.max + spec.step).to_string()).is_err(),
                    "{}",
                    field.path
                );
                let mut reset = value.clone();
                crate::camera_controls::store(&mut reset, &field, &spec.default.to_string())
                    .unwrap();
                omuse::camera_raw::validate(&serde_json::from_value(reset).unwrap()).unwrap();
            }
        }
    }

    #[test]
    fn camera_exposure_and_geometry_ranges_reject_values_the_engine_rejects() {
        let settings = omuse::camera_raw::Settings::default();
        let draft = serde_json::to_value(settings).unwrap();
        for (section, path, valid, invalid) in [
            (0, "/exposure", 5., 5.1),
            (7, "/geometry/rotate", -45., -45.1),
        ] {
            let field = crate::camera_controls::fields(&draft, section)
                .into_iter()
                .find(|f| f.path == path)
                .unwrap();
            let spec = field_spec(&field).unwrap();
            assert!(spec.parse(&valid.to_string()).is_ok());
            assert!(spec.parse(&invalid.to_string()).is_err());
            let mut value = draft.clone();
            crate::camera_controls::store(&mut value, &field, &invalid.to_string()).unwrap();
            assert!(omuse::camera_raw::validate(&serde_json::from_value(value).unwrap()).is_err());
        }
    }
}
