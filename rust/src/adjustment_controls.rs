//! User-facing controls for the persisted Mac adjustment records.
use serde_json::{Value, json};
pub const KINDS: &[&str] = &[
    "Exposure",
    "Hue/Saturation",
    "Levels",
    "Curves",
    "Gradient Map",
    "Grain",
    "Gaussian Blur",
    "Motion Blur",
    "Add Noise",
    "Invert",
    "Black & White",
    "Color Balance",
];
#[derive(Clone)]
pub struct Field {
    pub path: String,
    pub label: String,
    pub default: f64,
    pub boolean: bool,
}
pub fn fields(kind: usize) -> Vec<Field> {
    let mut out = Vec::new();
    let mut add = |path: &str, label: &str, default: f64| {
        out.push(Field {
            path: path.into(),
            label: label.into(),
            default,
            boolean: false,
        })
    };
    match kind {
        0 => {
            add("/exposureSettings/exposure", "Exposure (stops)", 0.);
            add("/exposureSettings/offset", "Linear-light offset", 0.);
            add("/exposureSettings/gamma", "Gamma", 1.);
        }
        1 => {
            add("/hue", "Hue (degrees)", 0.);
            add("/saturation", "Saturation (%)", 0.);
            add("/lightness", "Lightness (%)", 0.);
        }
        2 => {
            for (c, name) in ["RGB", "Red", "Green", "Blue"].iter().enumerate() {
                for (key, label, d) in [
                    ("black", "input black", 0.),
                    ("gamma", "gamma", 1.),
                    ("white", "input white", 255.),
                    ("outputBlack", "output black", 0.),
                    ("outputWhite", "output white", 255.),
                ] {
                    add(
                        &format!("/levels/ranges/{c}/{key}"),
                        &format!("{name} {label}"),
                        d,
                    );
                }
            }
        }
        3 => {}
        4 => {
            for (end, d) in [("shadows", 0.), ("highlights", 1.)] {
                for channel in ["red", "green", "blue"] {
                    add(
                        &format!("/gradientMapSettings/{end}/{channel}"),
                        &format!("{end} {channel} (0–1)"),
                        d,
                    );
                }
            }
        }
        5 => {
            add("/grainSettings/amount", "Amount (%)", 25.);
            add("/grainSettings/size", "Particle size", 1.5);
            add("/grainSettings/roughness", "Roughness (%)", 50.);
            add("/grainSettings/seed", "Pattern seed", 0.);
        }
        6 => add("/blurRadius", "Radius", 10.),
        7 => {
            add("/motionAngle", "Angle (degrees)", 0.);
            add("/motionDistance", "Distance", 10.);
        }
        8 => {
            add("/noiseAmount", "Amount (%)", 10.);
            add("/noiseSeed", "Pattern seed", 0.);
        }
        10 => {
            for (key, d) in [
                ("reds", 40.),
                ("yellows", 60.),
                ("greens", 40.),
                ("cyans", 60.),
                ("blues", 20.),
                ("magentas", 80.),
                ("tintHue", 40.),
                ("tintSaturation", 20.),
            ] {
                add(&format!("/blackWhiteSettings/{key}"), key, d);
            }
        }
        11 => {
            for tone in ["shadow", "mid", "highlight"] {
                for pair in ["CyanRed", "MagentaGreen", "YellowBlue"] {
                    add(
                        &format!("/colorBalanceSettings/{tone}{pair}"),
                        &format!("{tone} {pair}"),
                        0.,
                    );
                }
            }
        }
        _ => {}
    }
    for (path, label, default) in match kind {
        1 => vec![("/colorize", "Colorize", false)],
        4 => vec![("/gradientMapSettings/reversed", "Reverse gradient", false)],
        8 => vec![
            ("/noiseGaussian", "Gaussian noise", false),
            ("/noiseMonochromatic", "Monochrome noise", true),
        ],
        10 => vec![("/blackWhiteSettings/tint", "Tint", false)],
        11 => vec![(
            "/colorBalanceSettings/preserveLuminosity",
            "Preserve luminosity",
            true,
        )],
        _ => vec![],
    } {
        out.push(Field {
            path: path.into(),
            label: label.into(),
            default: if default { 1. } else { 0. },
            boolean: true,
        });
    }
    out
}
pub fn fresh(kind: usize) -> Value {
    let mut v = crate::adjustment_base();
    v["kind"] = KINDS[kind.min(KINDS.len() - 1)].into();
    for f in fields(kind) {
        set(
            &mut v,
            &f.path,
            if f.boolean {
                json!(f.default != 0.)
            } else if f.path.ends_with("Seed") || f.path.ends_with("/seed") {
                json!(f.default as u32)
            } else {
                json!(f.default)
            },
        );
    }
    v
}
pub fn set(value: &mut Value, path: &str, new: Value) {
    fn at(value: &mut Value, parts: &[&str], new: Value) {
        if parts.is_empty() {
            *value = new;
            return;
        }
        if let Ok(i) = parts[0].parse::<usize>() {
            if !value.is_array() {
                *value = json!([]);
            }
            let a = value.as_array_mut().unwrap();
            while a.len() <= i {
                a.push(Value::Null);
            }
            at(&mut a[i], &parts[1..], new);
        } else {
            if !value.is_object() {
                *value = json!({});
            }
            at(&mut value[parts[0]], &parts[1..], new);
        }
    }
    at(
        value,
        &path.trim_start_matches('/').split('/').collect::<Vec<_>>(),
        new,
    );
}
pub fn parse_curve(text: &str) -> anyhow::Result<Value> {
    let mut points = Vec::new();
    for part in text.split(';').filter(|s| !s.trim().is_empty()) {
        let (a, b) = part
            .trim()
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("Use x,y; x,y curve points"))?;
        let (x, y) = (a.trim().parse::<f64>()?, b.trim().parse::<f64>()?);
        anyhow::ensure!(
            x.is_finite()
                && y.is_finite()
                && (0. ..=255.).contains(&x)
                && (0. ..=255.).contains(&y),
            "Curve values must be 0–255"
        );
        if let Some(last) = points.last() {
            let last: &Value = last;
            anyhow::ensure!(
                last["x"].as_f64().unwrap() < x,
                "Curve X values must increase"
            );
        }
        points.push(json!({"x":x,"y":y}));
    }
    anyhow::ensure!((2..=256).contains(&points.len()), "Use 2–256 curve points");
    Ok(json!(points))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_ui_preset_is_valid_source_metadata() {
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([80, 130, 180, 255]));
        for kind in 0..KINDS.len() {
            let value = fresh(kind);
            let result = omuse::effects::apply_adjustment(&image, &value)
                .unwrap_or_else(|e| panic!("{}: {e:#}", KINDS[kind]));
            assert_eq!(result.dimensions(), image.dimensions());
        }
    }
    #[test]
    fn curves_reject_reversed_coordinates() {
        assert!(parse_curve("0,0; 128,100; 255,255").is_ok());
        assert!(parse_curve("255,0; 0,255").is_err());
        assert!(parse_curve("NaN,0; 255,255").is_err());
    }
}
