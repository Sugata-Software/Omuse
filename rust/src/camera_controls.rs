//! Typed Camera Raw form fields, shared by every section of the development dialog.
use anyhow::{Result, ensure};
use serde_json::Value;
pub const SECTIONS: &[(&str, &str)] = &[
    ("", "Light & color"),
    ("__effects", "Effects"),
    ("curve", "Curves"),
    ("mixer", "Color mixer"),
    ("grading", "Color grading"),
    ("detail", "Detail"),
    ("optics", "Optics"),
    ("geometry", "Geometry"),
    ("calibration", "Calibration"),
];
#[derive(Clone, Debug)]
pub struct Field {
    pub path: String,
    pub label: String,
    pub value: Value,
}
fn label(key: &str) -> String {
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}
pub fn fields(draft: &Value, section: usize) -> Vec<Field> {
    fn walk(v: &Value, path: &str, name: &str, out: &mut Vec<Field>) {
        match v {
            Value::Object(map) => {
                for (k, v) in map {
                    walk(
                        v,
                        &format!("{path}/{k}"),
                        &if name.is_empty() {
                            label(k)
                        } else {
                            format!("{name} · {}", label(k))
                        },
                        out,
                    )
                }
            }
            Value::Array(values)
                if values
                    .first()
                    .is_some_and(|v| v.get("x").is_some() && v.get("y").is_some()) =>
            {
                out.push(Field {
                    path: path.into(),
                    label: format!("{name} (x,y; x,y — 0 to 1)"),
                    value: v.clone(),
                })
            }
            Value::Array(values) => {
                for (i, v) in values.iter().enumerate() {
                    walk(v, &format!("{path}/{i}"), &format!("{name} {}", i + 1), out)
                }
            }
            Value::Number(_) | Value::Bool(_) | Value::String(_) => out.push(Field {
                path: path.into(),
                label: name.into(),
                value: v.clone(),
            }),
            _ => {}
        }
    }
    let mut out = Vec::new();
    let key = SECTIONS[section.min(SECTIONS.len() - 1)].0;
    const EFFECTS: &[&str] = &[
        "texture",
        "clarity",
        "dehaze",
        "glow",
        "glowStyle",
        "glowRange",
        "glowSpread",
        "glowWarmth",
        "vignetteAmount",
        "vignetteStyle",
        "vignetteMidpoint",
        "vignetteRoundness",
        "vignetteFeather",
        "vignetteHighlights",
        "grainAmount",
        "grainSize",
        "grainRoughness",
    ];
    if key.is_empty() {
        if let Some(map) = draft.as_object() {
            for (k, v) in map {
                if k != "toneMapping"
                    && !EFFECTS.contains(&k.as_str())
                    && !v.is_object()
                    && !v.is_array()
                {
                    walk(v, &format!("/{k}"), &label(k), &mut out)
                }
            }
        }
    } else if key == "__effects" {
        if let Some(map) = draft.as_object() {
            for k in EFFECTS {
                if let Some(v) = map.get(*k) {
                    walk(v, &format!("/{k}"), &label(k), &mut out)
                }
            }
        }
    } else if let Some(v) = draft.get(key) {
        walk(v, &format!("/{key}"), "", &mut out)
    }
    out
}
pub fn display(value: &Value) -> String {
    match value {
        Value::Array(points) => points
            .iter()
            .map(|p| format!("{},{}", p["x"], p["y"]))
            .collect::<Vec<_>>()
            .join("; "),
        Value::String(v) => v.clone(),
        _ => value.to_string(),
    }
}
pub fn store(draft: &mut Value, field: &Field, text: &str) -> Result<()> {
    let value = match &field.value {
        Value::Bool(_) => Value::Bool(match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => true,
            "false" | "0" | "no" => false,
            _ => anyhow::bail!("{}: enter true or false", field.label),
        }),
        Value::String(_) => Value::String(text.trim().into()),
        Value::Array(_) => {
            let points = crate::adjustment_controls::parse_curve(text)?;
            ensure!(
                points
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p["x"].as_f64().unwrap() <= 1. && p["y"].as_f64().unwrap() <= 1.),
                "Camera Raw curve coordinates must be between 0 and 1"
            );
            points
        }
        Value::Number(n) if n.is_u64() => serde_json::json!(text.trim().parse::<u64>()?),
        Value::Number(n) if n.is_i64() => serde_json::json!(text.trim().parse::<i64>()?),
        _ => {
            let n = text.trim().parse::<f64>()?;
            ensure!(n.is_finite(), "{} must be finite", field.label);
            serde_json::json!(n)
        }
    };
    *draft
        .pointer_mut(&field.path)
        .ok_or_else(|| anyhow::anyhow!("Missing field {}", field.label))? = value;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_default_forms_roundtrip() {
        for settings in [
            omuse::camera_raw::Settings::default(),
            omuse::camera_raw::Settings::for_new_edit(),
        ] {
            let mut draft = serde_json::to_value(settings).unwrap();
            let original = draft.clone();
            for i in 0..SECTIONS.len() {
                for f in fields(&draft, i) {
                    assert_ne!(f.path, "/toneMapping");
                    store(&mut draft, &f, &display(&f.value)).unwrap();
                }
            }
            assert_eq!(draft, original);
            let settings = serde_json::from_value(draft).unwrap();
            omuse::camera_raw::validate(&settings).unwrap();
        }
    }
    #[test]
    fn curve_units_are_not_adjustment_byte_units() {
        let mut d = serde_json::json!({"curve":{"rgb":[{"x":0.,"y":0.},{"x":1.,"y":1.}]}});
        let f = fields(&d, 2).remove(0);
        assert!(store(&mut d, &f, "0,0;255,255").is_err());
        store(&mut d, &f, "0,0;0.5,0.6;1,1").unwrap();
    }
}
