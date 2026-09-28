//! Pure-Rust ports of the preserved premultiplied-RGBA adjustment kernels.
//! The public model stores straight-alpha pixels, so `apply` premultiplies before
//! running the byte-compatible math and converts the result back afterwards.
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const OUTER_KEYS: &[&str] = &[
    "kind",
    "hue",
    "saturation",
    "lightness",
    "colorize",
    "hsvSettings",
    "levels",
    "curves",
    "exposureSettings",
    "gradientMapSettings",
    "grainSettings",
    "blackWhiteSettings",
    "colorBalanceSettings",
    "blurRadius",
    "motionAngle",
    "motionDistance",
    "noiseAmount",
    "noiseGaussian",
    "noiseMonochromatic",
    "noiseSeed",
];

#[derive(Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct Grain {
    amount: f64,
    size: f64,
    roughness: f64,
    seed: u32,
}
impl Default for Grain {
    fn default() -> Self {
        Self {
            amount: 25.,
            size: 1.5,
            roughness: 50.,
            seed: 0,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct BlackWhite {
    reds: f64,
    yellows: f64,
    greens: f64,
    cyans: f64,
    blues: f64,
    magentas: f64,
    tint: bool,
    tint_hue: f64,
    tint_saturation: f64,
}
impl Default for BlackWhite {
    fn default() -> Self {
        Self {
            reds: 40.,
            yellows: 60.,
            greens: 40.,
            cyans: 60.,
            blues: 20.,
            magentas: 80.,
            tint: false,
            tint_hue: 40.,
            tint_saturation: 20.,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct ColorBalance {
    shadow_cyan_red: f64,
    shadow_magenta_green: f64,
    shadow_yellow_blue: f64,
    mid_cyan_red: f64,
    mid_magenta_green: f64,
    mid_yellow_blue: f64,
    highlight_cyan_red: f64,
    highlight_magenta_green: f64,
    highlight_yellow_blue: f64,
    preserve_luminosity: bool,
}
impl Default for ColorBalance {
    fn default() -> Self {
        Self {
            shadow_cyan_red: 0.,
            shadow_magenta_green: 0.,
            shadow_yellow_blue: 0.,
            mid_cyan_red: 0.,
            mid_magenta_green: 0.,
            mid_yellow_blue: 0.,
            highlight_cyan_red: 0.,
            highlight_magenta_green: 0.,
            highlight_yellow_blue: 0.,
            preserve_luminosity: true,
        }
    }
}

/// Apply one supported live adjustment. `None` means the kind belongs to another
/// kernel; malformed supported adjustments return an error rather than degrading.
pub fn apply(image: &RgbaImage, adjustment: &Value) -> Result<Option<RgbaImage>> {
    let object = adjustment
        .as_object()
        .context("adjustment must be object")?;
    ensure!(
        object.keys().all(|key| OUTER_KEYS.contains(&key.as_str())),
        "unknown adjustment field"
    );
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .context("adjustment kind missing")?;
    let mut output = premultiply(image);
    match kind {
        "Grain" => {
            let settings: Grain = decode_settings(object.get("grainSettings"))?;
            ensure!(
                settings.amount.is_finite() && (0. ..=100.).contains(&settings.amount),
                "invalid grain amount"
            );
            ensure!(
                settings.size.is_finite() && (0.5..=20.).contains(&settings.size),
                "invalid grain size"
            );
            ensure!(
                settings.roughness.is_finite() && (0. ..=100.).contains(&settings.roughness),
                "invalid grain roughness"
            );
            if settings.amount == 0. {
                return Ok(Some(image.clone()));
            }
            grain(&mut output, settings);
        }
        "Black & White" => {
            let settings: BlackWhite = decode_settings(object.get("blackWhiteSettings"))?;
            let values = [
                settings.reds,
                settings.yellows,
                settings.greens,
                settings.cyans,
                settings.blues,
                settings.magentas,
            ];
            ensure!(
                values
                    .into_iter()
                    .all(|v| v.is_finite() && (-200. ..=300.).contains(&v)),
                "invalid black and white weights"
            );
            ensure!(
                settings.tint_hue.is_finite() && (0. ..=360.).contains(&settings.tint_hue),
                "invalid tint hue"
            );
            ensure!(
                settings.tint_saturation.is_finite()
                    && (0. ..=100.).contains(&settings.tint_saturation),
                "invalid tint saturation"
            );
            black_white(&mut output, settings);
        }
        "Color Balance" => {
            let settings: ColorBalance = decode_settings(object.get("colorBalanceSettings"))?;
            let values = [
                settings.shadow_cyan_red,
                settings.shadow_magenta_green,
                settings.shadow_yellow_blue,
                settings.mid_cyan_red,
                settings.mid_magenta_green,
                settings.mid_yellow_blue,
                settings.highlight_cyan_red,
                settings.highlight_magenta_green,
                settings.highlight_yellow_blue,
            ];
            ensure!(
                values
                    .into_iter()
                    .all(|v| v.is_finite() && (-100. ..=100.).contains(&v)),
                "invalid color balance value"
            );
            if values.into_iter().all(|value| value == 0.) {
                return Ok(Some(image.clone()));
            }
            color_balance(&mut output, settings);
        }
        _ => return Ok(None),
    }
    Ok(Some(unpremultiply(&output)))
}

fn decode_settings<T: DeserializeOwned + Default>(value: Option<&Value>) -> Result<T> {
    value.filter(|v| !v.is_null()).map_or_else(
        || Ok(T::default()),
        |v| serde_json::from_value(v.clone()).context("invalid adjustment settings"),
    )
}
fn premultiply(image: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        let a = u16::from(p[3]);
        Rgba([
            ((u16::from(p[0]) * a + 127) / 255) as u8,
            ((u16::from(p[1]) * a + 127) / 255) as u8,
            ((u16::from(p[2]) * a + 127) / 255) as u8,
            p[3],
        ])
    })
}
fn unpremultiply(image: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        let a = u32::from(p[3]);
        if a == 0 {
            return Rgba([0; 4]);
        }
        Rgba([
            ((u32::from(p[0]) * 255 + a / 2) / a).min(255) as u8,
            ((u32::from(p[1]) * 255 + a / 2) / a).min(255) as u8,
            ((u32::from(p[2]) * 255 + a / 2) / a).min(255) as u8,
            p[3],
        ])
    })
}

fn mix32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}
fn lattice(ix: i64, iy: i64, seed: u32) -> f32 {
    let h = mix32(
        (ix as u32).wrapping_mul(0x9e37_79b1) ^ mix32((iy as u32).wrapping_mul(0x85eb_ca77) ^ seed),
    );
    (h & 0xffff) as f32 / 65535. + (h >> 16) as f32 / 65535. - 1.
}
fn grain_field(u: f64, v: f64, scale: f64, seed: u32) -> f32 {
    let cx = (u / scale).floor();
    let cy = (v / scale).floor();
    let mut tx = (u / scale - cx) as f32;
    let mut ty = (v / scale - cy) as f32;
    tx = tx * tx * (3. - 2. * tx);
    ty = ty * ty * (3. - 2. * ty);
    let ix = cx as i64;
    let iy = cy as i64;
    let n00 = lattice(ix, iy, seed);
    let n10 = lattice(ix + 1, iy, seed);
    let n01 = lattice(ix, iy + 1, seed);
    let n11 = lattice(ix + 1, iy + 1, seed);
    let top = n00 + (n10 - n00) * tx;
    let bottom = n01 + (n11 - n01) * tx;
    (top + (bottom - top) * ty) * 1.6
}
fn grain(image: &mut RgbaImage, s: Grain) {
    if s.amount <= 0. {
        return;
    }
    let strength = (s.amount.min(100.) / 100.) as f32 * 0.35 * 255.;
    let rough = (s.roughness.clamp(0., 100.) / 100.) as f32;
    let fine_seed = mix32(s.seed ^ 0xa511_e9b3);
    let detail = (s.size * 0.35).max(0.5);
    for (x, y, p) in image.enumerate_pixels_mut() {
        let a = p[3];
        if a == 0 {
            continue;
        }
        let u = x as f64 + 0.5;
        let v = y as f64 + 0.5;
        let smooth = grain_field(u, v, s.size, s.seed);
        let fine = grain_field(u, v, detail, fine_seed);
        let noise = smooth + (fine - smooth) * rough;
        let un = if a == 255 { 1. } else { 255. / a as f32 };
        let rgb = [p[0] as f32 * un, p[1] as f32 * un, p[2] as f32 * un];
        let level = ((0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]) / 255.).min(1.);
        let delta = noise * strength * (0.4 + 2.4 * level * (1. - level));
        let coverage = a as f32 / 255.;
        for c in 0..3 {
            p[c] = ((rgb[c] + delta).clamp(0., 255.) * coverage + 0.5) as u8;
        }
    }
}

fn black_white(image: &mut RgbaImage, s: BlackWhite) {
    let w = [s.reds, s.yellows, s.greens, s.cyans, s.blues, s.magentas].map(|v| (v / 100.) as f32);
    for p in image.pixels_mut() {
        let alpha = p[3] as f32;
        if alpha == 0. {
            continue;
        }
        let r = (p[0] as f32 * 255. / alpha).min(255.) / 255.;
        let g = (p[1] as f32 * 255. / alpha).min(255.) / 255.;
        let b = (p[2] as f32 * 255. / alpha).min(255.) / 255.;
        let mx = r.max(g.max(b));
        let mn = r.min(g.min(b));
        let md = r + g + b - mx - mn;
        let (primary, secondary) = if mx == r {
            (0, if g >= b { 1 } else { 5 })
        } else if mx == g {
            (2, if r >= b { 1 } else { 3 })
        } else {
            (4, if g >= r { 3 } else { 5 })
        };
        let gray = (mn + (md - mn) * w[secondary] + (mx - md) * w[primary]).clamp(0., 1.);
        let (mut out_r, mut out_g, mut out_b) = (gray, gray, gray);
        if s.tint && s.tint_saturation > 0. {
            let sat = s.tint_saturation / 100.;
            let c = (1. - (2. * gray as f64 - 1.).abs()) * sat;
            let hp = (s.tint_hue % 360.) / 60.;
            let xx = c * (1. - (hp % 2. - 1.).abs());
            let (r1, g1, b1) = if hp < 1. {
                (c, xx, 0.)
            } else if hp < 2. {
                (xx, c, 0.)
            } else if hp < 3. {
                (0., c, xx)
            } else if hp < 4. {
                (0., xx, c)
            } else if hp < 5. {
                (xx, 0., c)
            } else {
                (c, 0., xx)
            };
            let m = gray as f64 - c / 2.;
            out_r = (r1 + m).clamp(0., 1.) as f32;
            out_g = (g1 + m).clamp(0., 1.) as f32;
            out_b = (b1 + m).clamp(0., 1.) as f32;
        }
        p[0] = (out_r * alpha).round().clamp(0., alpha) as u8;
        p[1] = (out_g * alpha).round().clamp(0., alpha) as u8;
        p[2] = (out_b * alpha).round().clamp(0., alpha) as u8;
    }
}

fn tonal_weights(v: f32) -> (f32, f32, f32) {
    let a = 0.25;
    let b = 0.333;
    let scale = 0.7;
    let shadow = (((v - b) / -a + 0.5).clamp(0., 1.)) * scale;
    let m1 = ((v - b) / a + 0.5).clamp(0., 1.);
    let m2 = ((v + b - 1.) / -a + 0.5).clamp(0., 1.);
    let highlight = (((v + b - 1.) / a + 0.5).clamp(0., 1.)) * scale;
    (shadow, m1 * m2 * scale, highlight)
}
fn color_balance(image: &mut RgbaImage, s: ColorBalance) {
    let shadows = [
        s.shadow_cyan_red,
        s.shadow_magenta_green,
        s.shadow_yellow_blue,
    ]
    .map(|v| (v / 100.) as f32);
    let mids = [s.mid_cyan_red, s.mid_magenta_green, s.mid_yellow_blue].map(|v| (v / 100.) as f32);
    let highs = [
        s.highlight_cyan_red,
        s.highlight_magenta_green,
        s.highlight_yellow_blue,
    ]
    .map(|v| (v / 100.) as f32);
    for p in image.pixels_mut() {
        let alpha = p[3] as f32;
        if alpha == 0. {
            continue;
        }
        let mut c = [0.; 3];
        for i in 0..3 {
            c[i] = (p[i] as f32 * 255. / alpha).min(255.) / 255.;
        }
        let before = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        for i in 0..3 {
            let (sw, mw, hw) = tonal_weights(c[i]);
            c[i] = (c[i] + shadows[i] * sw + mids[i] * mw + highs[i] * hw).clamp(0., 1.);
        }
        if s.preserve_luminosity {
            let after = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
            if after > 0.0001 {
                let ratio = before / after;
                for value in &mut c {
                    *value = (*value * ratio).clamp(0., 1.);
                }
            }
        }
        for i in 0..3 {
            p[i] = (c[i] * alpha).round().clamp(0., alpha) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_is_none_and_invalid_is_strict() {
        let image = RgbaImage::new(1, 1);
        assert!(
            apply(&image, &serde_json::json!({"kind":"Invert"}))
                .unwrap()
                .is_none()
        );
        assert!(
            apply(
                &image,
                &serde_json::json!({"kind":"Grain","grainSettings":{"size":0.1}})
            )
            .is_err()
        );
        assert!(
            apply(
                &image,
                &serde_json::json!({"kind":"Color Balance","colorBalanceSettings":{"future":1}})
            )
            .is_err()
        );
    }
    #[test]
    fn neutral_color_balance_is_identity_and_alpha_survives() {
        let image = RgbaImage::from_vec(2, 1, vec![200, 30, 80, 127, 10, 20, 30, 0]).unwrap();
        let output = apply(&image, &serde_json::json!({"kind":"Color Balance"}))
            .unwrap()
            .unwrap();
        assert_eq!(output, image);
    }
    #[test]
    fn black_white_keeps_neutral_gray_and_tint_colors_it() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([120, 120, 120, 128]));
        let gray = apply(&image, &serde_json::json!({"kind":"Black & White"}))
            .unwrap()
            .unwrap();
        assert_eq!(gray.get_pixel(0, 0), image.get_pixel(0, 0));
        let tinted = apply(
            &image,
            &serde_json::json!({"kind":"Black & White","blackWhiteSettings":{"tint":true}}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(tinted.get_pixel(0, 0)[3], 128);
        assert_ne!(tinted.get_pixel(0, 0)[0], tinted.get_pixel(0, 0)[2]);
    }
    #[test]
    fn grain_is_seeded_and_preserves_alpha() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([128, 128, 128, 96]));
        let value = serde_json::json!({"kind":"Grain","grainSettings":{"seed":42}});
        let a = apply(&image, &value).unwrap().unwrap();
        let b = apply(&image, &value).unwrap().unwrap();
        assert_eq!(a, b);
        assert!(a.pixels().all(|p| p[3] == 96));
        assert_ne!(a, image);
    }

    /// Fixtures were generated by compiling the preserved AdjustPixels.c and
    /// converting its premultiplied result back through this module's boundary.
    #[test]
    fn preserved_c_byte_fixtures_match() {
        let image = RgbaImage::from_vec(2, 1, vec![201, 30, 80, 127, 255, 64, 32, 255]).unwrap();
        let bw = apply(&image, &serde_json::json!({"kind":"Black & White"}))
            .unwrap()
            .unwrap();
        assert_eq!(bw.into_raw(), vec![118, 118, 118, 127, 128, 128, 128, 255]);

        let tint = RgbaImage::from_pixel(1, 1, Rgba([120, 120, 120, 128]));
        let tint = apply(
            &tint,
            &serde_json::json!({"kind":"Black & White","blackWhiteSettings":{"tint":true}}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(tint.into_raw(), vec![143, 128, 96, 128]);

        let balance = RgbaImage::from_pixel(1, 1, Rgba([201, 30, 80, 127]));
        let balance = apply(
            &balance,
            &serde_json::json!({"kind":"Color Balance","colorBalanceSettings":{
                "shadowCyanRed":20,"shadowMagentaGreen":-10,"shadowYellowBlue":30,
                "midCyanRed":-20,"midMagentaGreen":40,"midYellowBlue":-30,
                "highlightCyanRed":10,"highlightMagentaGreen":20,"highlightYellowBlue":-40
            }}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(balance.into_raw(), vec![229, 12, 92, 127]);

        let grain = RgbaImage::from_vec(2, 1, vec![128, 128, 128, 96, 128, 64, 32, 255]).unwrap();
        let grain = apply(
            &grain,
            &serde_json::json!({"kind":"Grain","grainSettings":{"seed":42}}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(grain.into_raw(), vec![133, 133, 133, 96, 120, 56, 24, 255]);
    }
}
