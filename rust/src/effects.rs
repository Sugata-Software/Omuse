//! Strict, non-destructive layer appearance support.
use crate::filters::{self, Filter};
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LayerEffects {
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub color_overlay: Option<ColorEffect>,
    pub inner_shadow: Option<Shadow>,
    pub outer_glow: Option<Glow>,
    pub inner_glow: Option<Glow>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stroke {
    pub enabled: Option<bool>,
    pub size: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub opacity: f32,
    pub inside: bool,
}
impl Default for Stroke {
    fn default() -> Self {
        Self {
            enabled: None,
            size: 4.,
            red: 0.,
            green: 0.,
            blue: 0.,
            opacity: 1.,
            inside: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shadow {
    pub enabled: Option<bool>,
    pub angle: f32,
    pub distance: f32,
    pub blur: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub opacity: f32,
}
impl Default for Shadow {
    fn default() -> Self {
        Self {
            enabled: None,
            angle: 90.,
            distance: 20.,
            blur: 20.,
            red: 0.,
            green: 0.,
            blue: 0.,
            opacity: 0.5,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorEffect {
    pub enabled: Option<bool>,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub opacity: f32,
}
impl Default for ColorEffect {
    fn default() -> Self {
        Self {
            enabled: None,
            red: 0.,
            green: 0.,
            blue: 0.,
            opacity: 1.,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Glow {
    pub enabled: Option<bool>,
    pub size: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub opacity: f32,
}
impl Default for Glow {
    fn default() -> Self {
        Self {
            enabled: None,
            size: 20.,
            red: 1.,
            green: 1.,
            blue: 1.,
            opacity: 0.75,
        }
    }
}
fn on(v: Option<bool>) -> bool {
    v.unwrap_or(true)
}
fn valid_color(c: [f32; 3], a: f32) -> Result<()> {
    ensure!(
        c.into_iter()
            .chain([a])
            .all(|v| v.is_finite() && (0.0..=1.).contains(&v)),
        "effect color/opacity must be 0...1"
    );
    Ok(())
}
impl LayerEffects {
    pub fn parse(v: &Value) -> Result<Self> {
        let x: Self = serde_json::from_value(v.clone()).context("Invalid layer effects")?;
        x.validate()?;
        Ok(x)
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(x) = &self.stroke {
            ensure!(
                x.size.is_finite() && (0.0..=500.).contains(&x.size),
                "invalid stroke size"
            );
            valid_color([x.red, x.green, x.blue], x.opacity)?
        }
        for x in [&self.shadow, &self.inner_shadow].into_iter().flatten() {
            ensure!(
                x.angle.is_finite()
                    && (-360.0..=360.).contains(&x.angle)
                    && x.distance.is_finite()
                    && (0.0..=5000.).contains(&x.distance)
                    && x.blur.is_finite()
                    && (0.0..=500.).contains(&x.blur),
                "invalid shadow geometry"
            );
            valid_color([x.red, x.green, x.blue], x.opacity)?
        }
        if let Some(x) = &self.color_overlay {
            valid_color([x.red, x.green, x.blue], x.opacity)?
        }
        for x in [&self.outer_glow, &self.inner_glow].into_iter().flatten() {
            ensure!(
                x.size.is_finite() && (0.0..=500.).contains(&x.size),
                "invalid glow size"
            );
            valid_color([x.red, x.green, x.blue], x.opacity)?
        }
        Ok(())
    }
}
/// Render a derived surface. The source image is never modified.
pub fn render(
    src: &RgbaImage,
    mask: Option<&RgbaImage>,
    fx: &LayerEffects,
) -> Result<(RgbaImage, u32)> {
    fx.validate()?;
    let shown = masked(src, mask);
    let mut margin = 0f32;
    if let Some(x) = &fx.stroke {
        if on(x.enabled) && !x.inside {
            margin = margin.max(x.size)
        }
    }
    if let Some(x) = &fx.shadow {
        if on(x.enabled) {
            margin = margin.max(x.distance + x.blur * 3.)
        }
    }
    if let Some(x) = &fx.outer_glow {
        if on(x.enabled) {
            margin = margin.max(x.size * 3.)
        }
    }
    let pad = margin.ceil() as u32 + 2;
    let w = src
        .width()
        .checked_add(pad * 2)
        .context("effect overflow")?;
    let h = src
        .height()
        .checked_add(pad * 2)
        .context("effect overflow")?;
    ensure!(
        crate::model::valid_dimensions(w, h),
        "effect surface too large"
    );
    let mut shape = RgbaImage::new(w, h);
    for (x, y, p) in shown.enumerate_pixels() {
        shape.put_pixel(x + pad, y + pad, Rgba([p[3]; 4]))
    }
    let mut out = RgbaImage::new(w, h);
    if let Some(x) = &fx.shadow {
        if on(x.enabled) {
            let r = x.angle.to_radians();
            paint_blur(
                &mut out,
                &shape,
                x.blur,
                (-r.cos() * x.distance).round() as i32,
                (r.sin() * x.distance).round() as i32,
                [x.red, x.green, x.blue],
                x.opacity,
                false,
            )
        }
    }
    if let Some(x) = &fx.outer_glow {
        if on(x.enabled) {
            paint_blur(
                &mut out,
                &shape,
                x.size,
                0,
                0,
                [x.red, x.green, x.blue],
                x.opacity,
                true,
            )
        }
    }
    if let Some(x) = &fx.stroke {
        if on(x.enabled) && !x.inside {
            paint_stroke(&mut out, &shape, x)
        }
    }
    for (x, y, p) in shown.enumerate_pixels() {
        over(&mut out, x + pad, y + pad, p.0)
    }
    if let Some(x) = &fx.color_overlay {
        if on(x.enabled) {
            inside(&mut out, &shape, [x.red, x.green, x.blue], x.opacity, None)
        }
    }
    if let Some(x) = &fx.inner_glow {
        if on(x.enabled) {
            inside(
                &mut out,
                &shape,
                [x.red, x.green, x.blue],
                x.opacity,
                Some(x.size),
            )
        }
    }
    if let Some(x) = &fx.inner_shadow {
        if on(x.enabled) {
            inner_shadow(&mut out, &shape, x)
        }
    }
    if let Some(x) = &fx.stroke {
        if on(x.enabled) && x.inside {
            paint_stroke(&mut out, &shape, x)
        }
    }
    Ok((out, pad))
}
fn masked(s: &RgbaImage, m: Option<&RgbaImage>) -> RgbaImage {
    let mut o = s.clone();
    if let Some(m) = m {
        for (x, y, p) in o.enumerate_pixels_mut() {
            let q = m.get_pixel(
                (u64::from(x) * u64::from(m.width()) / u64::from(s.width())) as u32,
                (u64::from(y) * u64::from(m.height()) / u64::from(s.height())) as u32,
            );
            let c = (0.2126 * q[0] as f32 + 0.7152 * q[1] as f32 + 0.0722 * q[2] as f32) / 255.
                * q[3] as f32
                / 255.;
            p[3] = (p[3] as f32 * c).round() as u8
        }
    }
    o
}
fn blur(a: &RgbaImage, s: f32) -> RgbaImage {
    let mut b = a.clone();
    let _ = filters::apply(&mut b, &Filter::GaussianBlur { sigma: s });
    b
}
fn paint_blur(
    o: &mut RgbaImage,
    a: &RgbaImage,
    s: f32,
    dx: i32,
    dy: i32,
    c: [f32; 3],
    opacity: f32,
    outside: bool,
) {
    for (x, y, p) in blur(a, s).enumerate_pixels() {
        let xx = x as i64 + dx as i64;
        let yy = y as i64 + dy as i64;
        if xx < 0 || yy < 0 || xx >= o.width() as i64 || yy >= o.height() as i64 {
            continue;
        }
        let own = a.get_pixel(xx as u32, yy as u32)[0] as f32 / 255.;
        let v = p[0] as f32 / 255.;
        over(
            o,
            xx as u32,
            yy as u32,
            rgba(c, opacity * if outside { (v - own).max(0.) } else { v }),
        )
    }
}
fn paint_stroke(o: &mut RgbaImage, a: &RgbaImage, s: &Stroke) {
    let r = s.size.ceil() as i32;
    for y in 0..a.height() {
        for x in 0..a.width() {
            let own = a.get_pixel(x, y)[0] as f32 / 255.;
            let mut n = if s.inside { own } else { 0. };
            for yy in -r..=r {
                for xx in -r..=r {
                    if xx * xx + yy * yy > r * r {
                        continue;
                    }
                    let (px, py) = (x as i64 + xx as i64, y as i64 + yy as i64);
                    if px >= 0 && py >= 0 && px < a.width() as i64 && py < a.height() as i64 {
                        let v = a.get_pixel(px as u32, py as u32)[0] as f32 / 255.;
                        n = if s.inside { n.min(v) } else { n.max(v) }
                    }
                }
            }
            over(
                o,
                x,
                y,
                rgba(
                    [s.red, s.green, s.blue],
                    s.opacity * if s.inside { own - n } else { n - own },
                ),
            )
        }
    }
}
fn inside(o: &mut RgbaImage, a: &RgbaImage, c: [f32; 3], opacity: f32, b: Option<f32>) {
    let soft = b.map(|s| blur(a, s));
    for (x, y, p) in a.enumerate_pixels() {
        let own = p[0] as f32 / 255.;
        let cov = soft
            .as_ref()
            .map_or(own, |v| (own - v.get_pixel(x, y)[0] as f32 / 255.).max(0.));
        over(o, x, y, rgba(c, opacity * cov))
    }
}
fn inner_shadow(o: &mut RgbaImage, a: &RgbaImage, s: &Shadow) {
    let b = blur(a, s.blur);
    let r = s.angle.to_radians();
    let (dx, dy) = (
        (-r.cos() * s.distance).round() as i64,
        (r.sin() * s.distance).round() as i64,
    );
    for (x, y, p) in a.enumerate_pixels() {
        let (xx, yy) = (x as i64 - dx, y as i64 - dy);
        let v = if xx >= 0 && yy >= 0 && xx < b.width() as i64 && yy < b.height() as i64 {
            b.get_pixel(xx as u32, yy as u32)[0] as f32 / 255.
        } else {
            0.
        };
        over(
            o,
            x,
            y,
            rgba(
                [s.red, s.green, s.blue],
                s.opacity * p[0] as f32 / 255. * (1. - v),
            ),
        )
    }
}
fn rgba(c: [f32; 3], a: f32) -> [u8; 4] {
    [
        (c[0] * 255.).round() as u8,
        (c[1] * 255.).round() as u8,
        (c[2] * 255.).round() as u8,
        (a.clamp(0., 1.) * 255.).round() as u8,
    ]
}
fn over(o: &mut RgbaImage, x: u32, y: u32, s: [u8; 4]) {
    if s[3] == 0 {
        return;
    }
    let d = o.get_pixel(x, y).0;
    let (sa, da) = (s[3] as f32 / 255., d[3] as f32 / 255.);
    let a = sa + da * (1. - sa);
    let mut r = [0; 4];
    for i in 0..3 {
        r[i] = ((s[i] as f32 * sa + d[i] as f32 * da * (1. - sa)) / a).round() as u8
    }
    r[3] = (a * 255.).round() as u8;
    o.put_pixel(x, y, Rgba(r))
}

pub fn adjustment_for_filter(f: &Filter) -> Result<Value> {
    let range = json!({"black":0,"gamma":1,"white":255,"outputBlack":0,"outputWhite":255});
    let line = json!([{"x":0,"y":0},{"x":255,"y":255}]);
    let kind = match f {
        Filter::Hsl { .. } => "Hue/Saturation",
        Filter::Levels { .. } => "Levels",
        Filter::Curves { .. } => "Curves",
        Filter::Exposure { .. } => "Exposure",
        Filter::GaussianBlur { .. } => "Gaussian Blur",
        Filter::Noise { .. } => "Add Noise",
        Filter::Invert => "Invert",
        _ => anyhow::bail!("filter has no lossless live-adjustment mapping"),
    };
    let mut v = json!({"kind":kind,"hue":0,"saturation":0,"lightness":0,"colorize":false,
  "levels":{"channel":"RGB","ranges":[range.clone(),range.clone(),range.clone(),range]},
  "curves":{"channel":"RGB","channels":[line.clone(),line.clone(),line.clone(),line]}});
    let o = v.as_object_mut().unwrap();
    match f {
        Filter::Hsl {
            hue_degrees,
            saturation,
            lightness,
        } => {
            o.insert("hue".into(), json!(hue_degrees));
            o.insert("saturation".into(), json!(saturation * 100.));
            o.insert("lightness".into(), json!(lightness * 100.));
        }
        Filter::Levels {
            black,
            white,
            gamma,
        } => {
            o["levels"]["ranges"][0] = json!({"black":black*255.,"gamma":gamma,"white":white*255.,"outputBlack":0,"outputWhite":255});
        }
        Filter::Curves { points } => {
            o["curves"]["channels"][0] = json!(
                points
                    .iter()
                    .map(|(x, y)| json!({"x":x*255.,"y":y*255.}))
                    .collect::<Vec<_>>()
            );
        }
        Filter::Exposure { stops } => {
            o.insert(
                "exposureSettings".into(),
                json!({"exposure":stops,"offset":0,"gamma":1}),
            );
        }
        Filter::GaussianBlur { sigma } => {
            o.insert("blurRadius".into(), json!(sigma));
        }
        Filter::Noise {
            amount,
            seed,
            monochrome,
        } => {
            o.insert("noiseAmount".into(), json!(amount * 100.));
            o.insert("noiseGaussian".into(), json!(false));
            o.insert("noiseMonochromatic".into(), json!(monochrome));
            o.insert(
                "noiseSeed".into(),
                json!(u32::try_from(*seed).unwrap_or(u32::MAX)),
            );
        }
        Filter::Invert => {}
        _ => unreachable!(),
    }
    Ok(v)
}

fn special_adjustment(
    kind: &str,
    image: &RgbaImage,
    o: &serde_json::Map<String, Value>,
) -> Result<Option<RgbaImage>> {
    if matches!(kind, "Grain" | "Black & White" | "Color Balance") {
        anyhow::bail!("{kind} adjustment exact preserved-kernel parity is not implemented");
    }
    let mut out = image.clone();
    match kind {
        "Gradient Map" => {
            let defaults = json!({"shadows":{"red":0,"green":0,"blue":0},"highlights":{"red":1,"green":1,"blue":1},"reversed":false});
            let s = o
                .get("gradientMapSettings")
                .unwrap_or(&defaults)
                .as_object()
                .context("gradientMapSettings must be an object")?;
            ensure!(
                s.keys()
                    .all(|k| matches!(k.as_str(), "shadows" | "highlights" | "reversed")),
                "unknown Gradient Map setting"
            );
            let get = |name: &str| -> Result<[f32; 3]> {
                let c = s
                    .get(name)
                    .and_then(Value::as_object)
                    .context("gradient color missing")?;
                ensure!(
                    c.len() == 3
                        && c.keys()
                            .all(|k| matches!(k.as_str(), "red" | "green" | "blue")),
                    "unknown gradient color field"
                );
                let values = ["red", "green", "blue"].map(|k| {
                    c.get(k)
                        .and_then(Value::as_f64)
                        .context("gradient color channel missing")
                        .map(|v| v as f32)
                });
                let [red, green, blue] = values;
                let values = [red?, green?, blue?];
                ensure!(
                    values
                        .into_iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
                    "invalid gradient color"
                );
                Ok(values)
            };
            let (mut a, mut b) = (get("shadows")?, get("highlights")?);
            if s.get("reversed").and_then(Value::as_bool) == Some(true) {
                std::mem::swap(&mut a, &mut b)
            }
            for p in out.pixels_mut() {
                let t = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.;
                for c in 0..3 {
                    p[c] = ((a[c] + (b[c] - a[c]) * t).clamp(0., 1.) * 255.).round() as u8
                }
            }
        }
        "Black & White" => {
            let s = o
                .get("blackWhiteSettings")
                .and_then(Value::as_object)
                .context("blackWhiteSettings missing")?;
            let w = ["reds", "yellows", "greens", "cyans", "blues", "magentas"].map(|k| {
                s.get(k).and_then(Value::as_f64).unwrap_or(match k {
                    "reds" | "greens" => 40.,
                    "yellows" | "cyans" => 60.,
                    "blues" => 20.,
                    _ => 80.,
                }) as f32
                    / 100.
            });
            for p in out.pixels_mut() {
                let (r, g, b) = (p[0] as f32 / 255., p[1] as f32 / 255., p[2] as f32 / 255.);
                let sum = r + g + b;
                let gray = if sum == 0. {
                    0.
                } else {
                    (r * w[0]
                        + r.min(g) * w[1]
                        + g * w[2]
                        + g.min(b) * w[3]
                        + b * w[4]
                        + b.min(r) * w[5])
                        / (r + r.min(g) + g + g.min(b) + b + b.min(r)).max(0.001)
                }
                .clamp(0., 1.);
                let v = (gray * 255.).round() as u8;
                p[0] = v;
                p[1] = v;
                p[2] = v
            }
        }
        "Color Balance" => {
            let s = o
                .get("colorBalanceSettings")
                .and_then(Value::as_object)
                .context("colorBalanceSettings missing")?;
            for p in out.pixels_mut() {
                let old = [p[0] as f32 / 255., p[1] as f32 / 255., p[2] as f32 / 255.];
                let lum = 0.2126 * old[0] + 0.7152 * old[1] + 0.0722 * old[2];
                let weights = [
                    (1. - 2. * lum).max(0.),
                    1. - (2. * lum - 1.).abs(),
                    (2. * lum - 1.).max(0.),
                ];
                for c in 0..3 {
                    let names = match c {
                        0 => ["shadowCyanRed", "midCyanRed", "highlightCyanRed"],
                        1 => [
                            "shadowMagentaGreen",
                            "midMagentaGreen",
                            "highlightMagentaGreen",
                        ],
                        _ => ["shadowYellowBlue", "midYellowBlue", "highlightYellowBlue"],
                    };
                    let shift = (0..3)
                        .map(|i| {
                            weights[i]
                                * s.get(names[i]).and_then(Value::as_f64).unwrap_or(0.) as f32
                                / 100.
                        })
                        .sum::<f32>();
                    p[c] = ((old[c] + shift).clamp(0., 1.) * 255.).round() as u8
                }
                if s.get("preserveLuminosity")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
                {
                    let now = 0.2126 * p[0] as f32 / 255.
                        + 0.7152 * p[1] as f32 / 255.
                        + 0.0722 * p[2] as f32 / 255.;
                    if now > 0. {
                        let q = lum / now;
                        for c in 0..3 {
                            p[c] = ((p[c] as f32 / 255. * q).clamp(0., 1.) * 255.).round() as u8
                        }
                    }
                }
            }
        }
        "Motion Blur" => {
            let angle = o
                .get("motionAngle")
                .and_then(Value::as_f64)
                .unwrap_or(0.)
                .to_radians();
            let distance = o
                .get("motionDistance")
                .and_then(Value::as_f64)
                .unwrap_or(10.);
            ensure!(
                (-90f64..=90.).contains(&angle.to_degrees()) && (1.0..=2000.).contains(&distance),
                "invalid Motion Blur settings"
            );
            let source = out.clone();
            let steps = distance.ceil().clamp(1., 128.) as i32;
            for (x, y, p) in out.enumerate_pixels_mut() {
                let mut sum = [0u64; 4];
                let mut n = 0;
                for i in -steps..=steps {
                    let t = i as f64 / steps as f64 * distance / 2.;
                    let xx = x as i64 + (angle.cos() * t).round() as i64;
                    let yy = y as i64 + (angle.sin() * t).round() as i64;
                    if xx >= 0
                        && yy >= 0
                        && xx < source.width() as i64
                        && yy < source.height() as i64
                    {
                        let q = source.get_pixel(xx as u32, yy as u32);
                        for c in 0..4 {
                            sum[c] += u64::from(q[c]) * if c < 3 { u64::from(q[3]) } else { 1 }
                        }
                        n += 1
                    }
                }
                p[3] = (sum[3] / n) as u8;
                if sum[3] == 0 {
                    p[0] = 0;
                    p[1] = 0;
                    p[2] = 0;
                } else {
                    for c in 0..3 {
                        p[c] = (sum[c] / sum[3]).min(255) as u8;
                    }
                }
            }
        }
        "Grain" => {
            let s = o
                .get("grainSettings")
                .and_then(Value::as_object)
                .context("grainSettings missing")?;
            let amount = s.get("amount").and_then(Value::as_f64).unwrap_or(25.) as f32 / 100.;
            let rough = s.get("roughness").and_then(Value::as_f64).unwrap_or(50.) as f32 / 100.;
            let mut state = s.get("seed").and_then(Value::as_u64).unwrap_or(0) ^ 0x9e3779b97f4a7c15;
            for p in out.pixels_mut() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let n =
                    ((state >> 40) as f32 / 16777215. - 0.5) * 2. * amount * (0.5 + 0.5 * rough);
                let lum =
                    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.;
                let n = n * (1. - (2. * lum - 1.).abs());
                for c in 0..3 {
                    p[c] = ((p[c] as f32 / 255. + n).clamp(0., 1.) * 255.).round() as u8
                }
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(out))
}
pub fn apply_adjustment(image: &RgbaImage, v: &Value) -> Result<RgbaImage> {
    let o = v.as_object().context("adjustment must be object")?;
    const KEYS: &[&str] = &[
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
    ensure!(
        o.keys().all(|k| KEYS.contains(&k.as_str())),
        "unknown adjustment field"
    );
    let kind = o
        .get("kind")
        .and_then(Value::as_str)
        .context("adjustment kind missing")?;
    if let Some(result) = crate::adjustment_kernels::apply(image, v)? {
        return Ok(result);
    }
    if let Some(result) = special_adjustment(kind, image, o)? {
        return Ok(result);
    }
    let filter = match kind {
        "Invert" => Some(Filter::Invert),
        "Gaussian Blur" => Some(Filter::GaussianBlur {
            sigma: o.get("blurRadius").and_then(Value::as_f64).unwrap_or(10.) as f32,
        }),
        "Add Noise" => Some(Filter::Noise {
            amount: (o.get("noiseAmount").and_then(Value::as_f64).unwrap_or(10.) / 100.)
                .clamp(0., 1.) as f32,
            seed: o.get("noiseSeed").and_then(Value::as_u64).unwrap_or(0),
            monochrome: o
                .get("noiseMonochromatic")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "Hue/Saturation" => Some(Filter::Hsl {
            hue_degrees: o.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            saturation: o.get("saturation").and_then(Value::as_f64).unwrap_or(0.0) as f32 / 100.0,
            lightness: o.get("lightness").and_then(Value::as_f64).unwrap_or(0.0) as f32 / 100.0,
        }),
        "Levels" => {
            let r = o
                .get("levels")
                .and_then(|v| v.get("ranges"))
                .and_then(Value::as_array)
                .and_then(|v| v.first())
                .and_then(Value::as_object)
                .context("invalid Levels settings")?;
            Some(Filter::Levels {
                black: r.get("black").and_then(Value::as_f64).unwrap_or(0.0) as f32 / 255.0,
                white: r.get("white").and_then(Value::as_f64).unwrap_or(255.0) as f32 / 255.0,
                gamma: r.get("gamma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            })
        }
        "Curves" => {
            let points = o
                .get("curves")
                .and_then(|v| v.get("channels"))
                .and_then(Value::as_array)
                .and_then(|v| v.first())
                .and_then(Value::as_array)
                .context("invalid Curves settings")?
                .iter()
                .map(|p| {
                    Ok((
                        p.get("x")
                            .and_then(Value::as_f64)
                            .context("curve x missing")? as f32
                            / 255.0,
                        p.get("y")
                            .and_then(Value::as_f64)
                            .context("curve y missing")? as f32
                            / 255.0,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            Some(Filter::Curves { points })
        }
        "Exposure" => None,
        _ => anyhow::bail!("unsupported adjustment kind: {kind}"),
    };
    if matches!(kind, "Grain" | "Black & White" | "Color Balance") {
        anyhow::bail!("{kind} adjustment exact preserved-kernel parity is not implemented");
    }
    let mut out = image.clone();
    if let Some(f) = filter {
        filters::apply(&mut out, &f)?;
        return Ok(out);
    }
    let s = o
        .get("exposureSettings")
        .and_then(Value::as_object)
        .context("exposureSettings missing")?;
    ensure!(
        s.keys()
            .all(|k| matches!(k.as_str(), "exposure" | "offset" | "gamma")),
        "unknown Exposure setting"
    );
    let (stops, offset, gamma) = (
        s.get("exposure").and_then(Value::as_f64).unwrap_or(0.) as f32,
        s.get("offset").and_then(Value::as_f64).unwrap_or(0.) as f32,
        s.get("gamma").and_then(Value::as_f64).unwrap_or(1.) as f32,
    );
    ensure!(
        (-20.0..=20.).contains(&stops)
            && (-0.5..=0.5).contains(&offset)
            && (0.01..=9.99).contains(&gamma),
        "invalid Exposure settings"
    );
    for p in out.pixels_mut() {
        if p[3] == 0 {
            continue;
        }
        for c in 0..3 {
            let e = p[c] as f32 / 255.;
            let l = if e <= 0.04045 {
                e / 12.92
            } else {
                ((e + 0.055) / 1.055).powf(2.4)
            };
            let l = (l * 2f32.powf(stops) + offset).max(0.).powf(1. / gamma);
            let e = if l <= 0.0031308 {
                l * 12.92
            } else {
                1.055 * l.powf(1. / 2.4) - 0.055
            };
            p[c] = (e.clamp(0., 1.) * 255.).round() as u8
        }
    }
    Ok(out)
}
pub fn validate_layer_metadata(v: &Value) -> Result<()> {
    if let Some(x) = v.get("effects").filter(|v| !v.is_null()) {
        LayerEffects::parse(x)?;
    }
    if let Some(x) = v.get("adjustment").filter(|v| !v.is_null()) {
        apply_adjustment(&RgbaImage::from_pixel(1, 1, Rgba([0; 4])), x)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_only() {
        let s = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
        let b = s.clone();
        let f = LayerEffects::parse(&json!({"stroke":{"size":1.0}})).unwrap();
        let (r, _) = render(&s, None, &f).unwrap();
        assert_eq!(s, b);
        assert!(r.pixels().filter(|p| p[3] > 0).count() > 1)
    }
    #[test]
    fn strict() {
        assert!(LayerEffects::parse(&json!({"stroke":{"future":1}})).is_err());
        assert!(
            apply_adjustment(&RgbaImage::new(1, 1), &json!({"kind":"Invert","future":1})).is_err()
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MaskPlacement {
    origin: [f32; 2],
    size: [f32; 2],
    #[serde(default)]
    rotation: f32,
    #[serde(default)]
    flip_x: bool,
    #[serde(default)]
    flip_y: bool,
    #[serde(default)]
    sampling: Option<String>,
}
#[derive(Clone)]
pub struct MaskSampler<'a> {
    mask: &'a RgbaImage,
    placed: Option<MaskPlacement>,
    outside: f32,
    sampling: String,
}

/// Legacy masks infer their infinite white/black ground from edge coverage.
/// Once painted beyond their bitmap, retain that ground explicitly so editing
/// an edge cannot unexpectedly reveal or hide distant artwork.
pub fn mask_outside_coverage(metadata: &Value, mask: &RgbaImage) -> u8 {
    if let Some(value) = metadata.get("maskOutsideCoverage").and_then(Value::as_u64)
        && matches!(value, 0 | 255)
    {
        return value as u8;
    }
    let mut total = 0f64;
    let mut count = 0u64;
    let mut add = |p: &Rgba<u8>| {
        total += (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
            * f64::from(p[3])
            / (255. * 255.);
        count += 1;
    };
    if mask.height() > 0 {
        for x in 0..mask.width() {
            add(mask.get_pixel(x, 0));
            if mask.height() > 1 {
                add(mask.get_pixel(x, mask.height() - 1));
            }
        }
    }
    if mask.width() > 0 {
        for y in 1..mask.height().saturating_sub(1) {
            add(mask.get_pixel(0, y));
            if mask.width() > 1 {
                add(mask.get_pixel(mask.width() - 1, y));
            }
        }
    }
    if count == 0 || total * 2. >= count as f64 {
        255
    } else {
        0
    }
}

impl<'a> MaskSampler<'a> {
    /// Parse placement and compute the edge-majority background once per render
    /// pass. Metadata validation reports malformed placements before rendering.
    pub fn new(metadata: &Value, mask: &'a RgbaImage) -> Self {
        Self::make(metadata, mask, true)
    }
    /// Folder masks are placed by the folder transform in the source renderer;
    /// an image-layer-only mask placement must not move them independently.
    pub fn new_folder(metadata: &Value, mask: &'a RgbaImage) -> Self {
        // Legacy folders use their own transform; grown masks have an explicit
        // independent grid while the folder and its children stay in place.
        Self::make(
            metadata,
            mask,
            metadata.get("maskOutsideCoverage").is_some(),
        )
    }
    fn make(metadata: &Value, mask: &'a RgbaImage, allow_placement: bool) -> Self {
        let placed = metadata
            .get("maskPlacement")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value::<MaskPlacement>(v.clone()).ok());
        let placed = allow_placement.then_some(placed).flatten();
        let sampling = placed
            .as_ref()
            .and_then(|p| p.sampling.as_deref())
            .or_else(|| {
                metadata
                    .get("transform")
                    .and_then(|t| t.get("sampling"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("High quality")
            .to_owned();
        Self {
            mask,
            placed,
            sampling,
            outside: f32::from(mask_outside_coverage(metadata, mask)) / 255.,
        }
    }
    pub fn coverage(
        &self,
        canvas_x: f64,
        canvas_y: f64,
        local_u: f64,
        local_v: f64,
        source_w: f64,
        source_h: f64,
    ) -> f32 {
        let point = self
            .placed
            .as_ref()
            .and_then(|p| {
                let (w, h) = (p.size[0] as f64, p.size[1] as f64);
                if w <= 0. || h <= 0. {
                    return None;
                }
                let (cx, cy) = (p.origin[0] as f64 + w / 2., p.origin[1] as f64 + h / 2.);
                let (dx, dy) = (canvas_x - cx, canvas_y - cy);
                let (s, c) = (p.rotation as f64).to_radians().sin_cos();
                let mut u = (dx * c + dy * s) / w + 0.5;
                let mut v = (-dx * s + dy * c) / h + 0.5;
                if p.flip_x {
                    u = 1.0 - u
                }
                if p.flip_y {
                    v = 1.0 - v
                }
                Some((u * self.mask.width() as f64, v * self.mask.height() as f64))
            })
            .unwrap_or((
                local_u / source_w * self.mask.width() as f64,
                local_v / source_h * self.mask.height() as f64,
            ));
        let outside = if self.placed.is_some() {
            self.outside
        } else {
            0.
        };
        let value = |x: i64, y: i64| {
            if x < 0
                || y < 0
                || x >= i64::from(self.mask.width())
                || y >= i64::from(self.mask.height())
            {
                if self.placed.is_some() {
                    return outside;
                }
            }
            let p = self.mask.get_pixel(
                x.clamp(0, i64::from(self.mask.width()) - 1) as u32,
                y.clamp(0, i64::from(self.mask.height()) - 1) as u32,
            );
            (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.
                * p[3] as f32
                / 255.
        };
        if self.sampling == "Nearest" {
            return value(point.0.floor() as i64, point.1.floor() as i64);
        }
        let (px, py) = (point.0 - 0.5, point.1 - 0.5);
        if self.sampling == "Smooth" {
            let (ix, iy) = (px.floor() as i64, py.floor() as i64);
            let (fx, fy) = ((px - px.floor()) as f32, (py - py.floor()) as f32);
            return value(ix, iy) * (1. - fx) * (1. - fy)
                + value(ix + 1, iy) * fx * (1. - fy)
                + value(ix, iy + 1) * (1. - fx) * fy
                + value(ix + 1, iy + 1) * fx * fy;
        }
        fn sinc(x: f64) -> f64 {
            if x.abs() < 1e-9 {
                1.
            } else {
                let p = std::f64::consts::PI * x;
                p.sin() / p
            }
        }
        let (ix, iy) = (px.floor() as i64, py.floor() as i64);
        let mut sum = 0.;
        let mut weight = 0.;
        for y in iy - 2..=iy + 3 {
            let dy = py - y as f64;
            let wy = sinc(dy) * sinc(dy / 3.);
            for x in ix - 2..=ix + 3 {
                let dx = px - x as f64;
                let w = wy * sinc(dx) * sinc(dx / 3.);
                sum += f64::from(value(x, y)) * w;
                weight += w;
            }
        }
        if weight.abs() < 1e-9 {
            outside
        } else {
            (sum / weight).clamp(0., 1.) as f32
        }
    }
}
/// Convenience sampler for isolated calls. Render loops should construct one
/// `MaskSampler` and reuse it for all pixels.
pub fn mask_coverage(
    metadata: &Value,
    mask: &RgbaImage,
    canvas_x: f64,
    canvas_y: f64,
    local_u: f64,
    local_v: f64,
    source_w: f64,
    source_h: f64,
) -> f32 {
    MaskSampler::new(metadata, mask)
        .coverage(canvas_x, canvas_y, local_u, local_v, source_w, source_h)
}
pub fn validate_mask_metadata(v: &Value) -> Result<()> {
    if let Some(value) = v.get("maskOutsideCoverage") {
        ensure!(
            matches!(value.as_u64(), Some(0 | 255)),
            "maskOutsideCoverage must be 0 (black) or 255 (white)"
        );
    }
    if let Some(raw) = v.get("maskPlacement").filter(|v| !v.is_null()) {
        let p: MaskPlacement =
            serde_json::from_value(raw.clone()).context("invalid maskPlacement")?;
        ensure!(
            p.origin
                .into_iter()
                .chain(p.size)
                .chain([p.rotation])
                .all(f32::is_finite)
                && p.size[0] > 0.
                && p.size[1] > 0.,
            "invalid maskPlacement geometry"
        );
        ensure!(
            p.sampling
                .as_deref()
                .is_none_or(|s| matches!(s, "Nearest" | "Smooth" | "High quality")),
            "unsupported mask sampling"
        );
    }
    if let Some(x) = v.get("maskLinked") {
        ensure!(x.is_boolean(), "maskLinked must be boolean");
    }
    Ok(())
}

#[cfg(test)]
mod mask_sampling_tests {
    use super::*;

    #[test]
    fn smooth_and_high_quality_interpolate_mask_edges() {
        let mask = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255; 4])
            }
        });
        for sampling in ["Smooth", "High quality"] {
            let metadata = json!({"maskPlacement": {
                "origin": [0, 0], "size": [2, 1], "sampling": sampling
            }});
            validate_mask_metadata(&metadata).unwrap();
            let sampler = MaskSampler::new(&metadata, &mask);
            let value = sampler.coverage(1., 0.5, 1., 0.5, 2., 1.);
            assert!((0.35..0.65).contains(&value), "{sampling}: {value}");
        }
    }

    #[test]
    fn legacy_linear_mask_sampling_is_rejected() {
        assert!(
            validate_mask_metadata(&json!({"maskPlacement": {
                "origin": [0, 0], "size": [1, 1], "sampling": "Linear"
            }}))
            .is_err()
        );
    }
}

#[cfg(test)]
mod adjustment_tests {
    use super::*;
    #[test]
    fn generated_adjustments_round_trip_without_touching_source() {
        for f in [
            Filter::Invert,
            Filter::GaussianBlur { sigma: 3. },
            Filter::Exposure { stops: 1. },
            Filter::Hsl {
                hue_degrees: 20.,
                saturation: 0.2,
                lightness: -0.1,
            },
            Filter::Levels {
                black: 0.1,
                white: 0.9,
                gamma: 1.2,
            },
        ] {
            let value = adjustment_for_filter(&f).unwrap();
            let source = RgbaImage::from_pixel(2, 1, Rgba([64, 128, 192, 200]));
            let before = source.clone();
            assert!(apply_adjustment(&source, &value).is_ok());
            assert_eq!(source, before);
        }
    }
}
