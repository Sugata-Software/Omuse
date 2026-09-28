//! Typed, fail-closed Camera Raw pipeline. The Basic stage ports the preserved kernel.
use anyhow::{Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}
fn linear_curve() -> Vec<CurvePoint> {
    vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 1., y: 1. }]
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct CurveSettings {
    pub shadows: f32,
    pub darks: f32,
    pub lights: f32,
    pub highlights: f32,
    pub shadow_split: f32,
    pub dark_split: f32,
    pub light_split: f32,
    pub rgb: Vec<CurvePoint>,
    pub red: Vec<CurvePoint>,
    pub green: Vec<CurvePoint>,
    pub blue: Vec<CurvePoint>,
    pub refine_saturation: f32,
}
impl Default for CurveSettings {
    fn default() -> Self {
        Self {
            shadows: 0.,
            darks: 0.,
            lights: 0.,
            highlights: 0.,
            shadow_split: 25.,
            dark_split: 50.,
            light_split: 75.,
            rgb: linear_curve(),
            red: linear_curve(),
            green: linear_curve(),
            blue: linear_curve(),
            refine_saturation: 0.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct PointColor {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
    pub hue_shift: f32,
    pub saturation_shift: f32,
    pub luminance_shift: f32,
    pub hue_range: f32,
    pub saturation_range: f32,
    pub luminance_range: f32,
    pub visualize: bool,
}
impl Default for PointColor {
    fn default() -> Self {
        Self {
            hue: 0.,
            saturation: 0.,
            luminance: 0.,
            hue_shift: 0.,
            saturation_shift: 0.,
            luminance_shift: 0.,
            hue_range: 30.,
            saturation_range: 0.4,
            luminance_range: 0.4,
            visualize: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MixerSettings {
    pub hue: Vec<f32>,
    pub saturation: Vec<f32>,
    pub luminance: Vec<f32>,
    pub points: Vec<PointColor>,
}
impl Default for MixerSettings {
    fn default() -> Self {
        Self {
            hue: vec![0.; 8],
            saturation: vec![0.; 8],
            luminance: vec![0.; 8],
            points: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GradeWheel {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GradingSettings {
    pub shadows: GradeWheel,
    pub midtones: GradeWheel,
    pub highlights: GradeWheel,
    pub global: GradeWheel,
    pub blending: f32,
    pub balance: f32,
}
impl Default for GradingSettings {
    fn default() -> Self {
        Self {
            shadows: Default::default(),
            midtones: Default::default(),
            highlights: Default::default(),
            global: Default::default(),
            blending: 50.,
            balance: 0.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct DetailSettings {
    pub sharpen_amount: f32,
    pub sharpen_radius: f32,
    pub sharpen_detail: f32,
    pub sharpen_masking: f32,
    pub noise_luminance: f32,
    pub noise_luminance_detail: f32,
    pub noise_luminance_contrast: f32,
    pub noise_color: f32,
    pub noise_color_detail: f32,
    pub noise_color_smoothness: f32,
}
impl Default for DetailSettings {
    fn default() -> Self {
        Self {
            sharpen_amount: 0.,
            sharpen_radius: 10.,
            sharpen_detail: 25.,
            sharpen_masking: 0.,
            noise_luminance: 0.,
            noise_luminance_detail: 50.,
            noise_luminance_contrast: 0.,
            noise_color: 0.,
            noise_color_detail: 50.,
            noise_color_smoothness: 50.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct OpticsSettings {
    pub remove_chromatic_aberration: bool,
    pub enable_lens_profile: bool,
    pub profile_distortion: f32,
    pub profile_vignetting: f32,
    pub distortion: f32,
    pub purple_amount: f32,
    pub purple_hue_low: f32,
    pub purple_hue_high: f32,
    pub green_amount: f32,
    pub green_hue_low: f32,
    pub green_hue_high: f32,
    pub vignette_amount: f32,
    pub vignette_midpoint: f32,
}
impl Default for OpticsSettings {
    fn default() -> Self {
        Self {
            remove_chromatic_aberration: false,
            enable_lens_profile: false,
            profile_distortion: 100.,
            profile_vignetting: 100.,
            distortion: 0.,
            purple_amount: 0.,
            purple_hue_low: 270.,
            purple_hue_high: 310.,
            green_amount: 0.,
            green_hue_low: 60.,
            green_hue_high: 120.,
            vignette_amount: 0.,
            vignette_midpoint: 50.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UprightMode {
    #[default]
    #[serde(rename = "Off")]
    Off,
    #[serde(rename = "Guided")]
    Guided,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    #[default]
    #[serde(rename = "Perspective")]
    Perspective,
    #[serde(rename = "Rectilinear")]
    Rectilinear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum WhiteBalance {
    #[default]
    #[serde(rename = "Custom")]
    Custom,
    #[serde(rename = "Auto")]
    Auto,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum GlowStyle {
    #[default]
    #[serde(rename = "Diffusion")]
    Diffusion,
    #[serde(rename = "Bloom")]
    Bloom,
    #[serde(rename = "Halation")]
    Halation,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum VignetteStyle {
    #[default]
    #[serde(rename = "Highlight Priority")]
    HighlightPriority,
    #[serde(rename = "Color Priority")]
    ColorPriority,
    #[serde(rename = "Paint Overlay")]
    PaintOverlay,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GeometryGuide {
    pub start_x: f32,
    pub start_y: f32,
    pub end_x: f32,
    pub end_y: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GeometrySettings {
    pub upright: UprightMode,
    pub projection: Projection,
    pub vertical: f32,
    pub horizontal: f32,
    pub rotate: f32,
    pub aspect: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub constrain_crop: bool,
    pub guides: Vec<GeometryGuide>,
}
impl Default for GeometrySettings {
    fn default() -> Self {
        Self {
            upright: Default::default(),
            projection: Default::default(),
            vertical: 0.,
            horizontal: 0.,
            rotate: 0.,
            aspect: 0.,
            scale: 0.,
            offset_x: 0.,
            offset_y: 0.,
            constrain_crop: false,
            guides: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct CalibrationSettings {
    pub process: u8,
    pub shadow_tint: f32,
    pub red_hue: f32,
    pub red_saturation: f32,
    pub green_hue: f32,
    pub green_saturation: f32,
    pub blue_hue: f32,
    pub blue_saturation: f32,
}
impl Default for CalibrationSettings {
    fn default() -> Self {
        Self {
            process: 6,
            shadow_tint: 0.,
            red_hue: 0.,
            red_saturation: 0.,
            green_hue: 0.,
            green_saturation: 0.,
            blue_hue: 0.,
            blue_saturation: 0.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub white_balance: WhiteBalance,
    pub temperature: f32,
    pub tint: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub glow: f32,
    pub glow_style: GlowStyle,
    pub glow_range: f32,
    pub glow_spread: f32,
    pub glow_warmth: f32,
    pub vignette_amount: f32,
    pub vignette_style: VignetteStyle,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,
    pub vignette_highlights: f32,
    pub grain_amount: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,
    pub curve: CurveSettings,
    pub mixer: MixerSettings,
    pub grading: GradingSettings,
    pub detail: DetailSettings,
    pub optics: OpticsSettings,
    pub geometry: GeometrySettings,
    pub calibration: CalibrationSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            white_balance: WhiteBalance::Custom,
            temperature: 0.,
            tint: 0.,
            exposure: 0.,
            contrast: 0.,
            highlights: 0.,
            shadows: 0.,
            whites: 0.,
            blacks: 0.,
            vibrance: 0.,
            saturation: 0.,
            texture: 0.,
            clarity: 0.,
            dehaze: 0.,
            glow: 0.,
            glow_style: GlowStyle::Diffusion,
            glow_range: 0.,
            glow_spread: 0.,
            glow_warmth: 0.,
            vignette_amount: 0.,
            vignette_style: VignetteStyle::HighlightPriority,
            vignette_midpoint: 50.,
            vignette_roundness: 0.,
            vignette_feather: 50.,
            vignette_highlights: 0.,
            grain_amount: 0.,
            grain_size: 25.,
            grain_roughness: 50.,
            curve: Default::default(),
            mixer: Default::default(),
            grading: Default::default(),
            detail: Default::default(),
            optics: Default::default(),
            geometry: Default::default(),
            calibration: Default::default(),
        }
    }
}
pub fn unsupported(s: &Settings) -> Vec<&str> {
    let _ = s;
    vec![]
}
pub fn validate(s: &Settings) -> Result<()> {
    ensure!(
        s.exposure.is_finite() && (-5.0..=5.0).contains(&s.exposure),
        "exposure must be -5 to 5"
    );
    for v in [
        s.temperature,
        s.tint,
        s.contrast,
        s.highlights,
        s.shadows,
        s.whites,
        s.blacks,
        s.vibrance,
        s.saturation,
        s.texture,
        s.clarity,
        s.dehaze,
        s.glow_range,
        s.glow_spread,
        s.glow_warmth,
        s.vignette_amount,
        s.vignette_roundness,
    ] {
        ensure!(
            v.is_finite() && (-100.0..=100.0).contains(&v),
            "Camera Raw value must be -100 to 100"
        )
    }
    for v in [
        s.glow,
        s.vignette_midpoint,
        s.vignette_feather,
        s.vignette_highlights,
        s.grain_amount,
        s.grain_size,
        s.grain_roughness,
    ] {
        finite_range(v, 0.0..=100.0, "Camera Raw unit value")?;
    }
    validate_curve(&s.curve)?;
    validate_mixer(&s.mixer)?;
    validate_grading(&s.grading)?;
    validate_detail(&s.detail)?;
    validate_optics(&s.optics)?;
    validate_geometry(&s.geometry)?;
    ensure!(
        (1..=6).contains(&s.calibration.process),
        "calibration process must be 1-6"
    );
    for v in [
        s.calibration.shadow_tint,
        s.calibration.red_hue,
        s.calibration.red_saturation,
        s.calibration.green_hue,
        s.calibration.green_saturation,
        s.calibration.blue_hue,
        s.calibration.blue_saturation,
    ] {
        ensure!(
            v.is_finite() && (-100.0..=100.0).contains(&v),
            "calibration value must be -100 to 100"
        );
    }
    let u = unsupported(s);
    ensure!(
        u.is_empty(),
        "Camera Raw stages not implemented exactly: {}",
        u.join(", ")
    );
    Ok(())
}
fn finite_range(v: f32, range: std::ops::RangeInclusive<f32>, name: &str) -> Result<()> {
    ensure!(
        v.is_finite() && range.contains(&v),
        "{name} is out of range"
    );
    Ok(())
}
fn validate_points(points: &[CurvePoint], name: &str) -> Result<()> {
    ensure!(
        (2..=32).contains(&points.len()),
        "{name} must contain 2-32 points"
    );
    ensure!(
        points.first()
            == Some(&CurvePoint {
                x: 0.,
                y: points[0].y
            })
            && points.last().is_some_and(|p| p.x == 1.),
        "{name} must span x=0 to x=1"
    );
    for point in points {
        finite_range(point.x, 0.0..=1.0, name)?;
        finite_range(point.y, 0.0..=1.0, name)?;
    }
    ensure!(
        points.windows(2).all(|p| p[0].x < p[1].x),
        "{name} x values must strictly increase"
    );
    Ok(())
}
fn validate_curve(s: &CurveSettings) -> Result<()> {
    for v in [
        s.shadows,
        s.darks,
        s.lights,
        s.highlights,
        s.refine_saturation,
    ] {
        finite_range(v, -100.0..=100.0, "curve amount")?;
    }
    finite_range(s.shadow_split, 5.0..=90.0, "shadowSplit")?;
    finite_range(s.dark_split, s.shadow_split + 2.0..=95.0, "darkSplit")?;
    finite_range(s.light_split, s.dark_split + 2.0..=98.0, "lightSplit")?;
    for (name, points) in [
        ("rgb", &s.rgb),
        ("red", &s.red),
        ("green", &s.green),
        ("blue", &s.blue),
    ] {
        validate_points(points, name)?;
    }
    Ok(())
}
fn validate_mixer(s: &MixerSettings) -> Result<()> {
    ensure!(
        s.hue.len() == 8 && s.saturation.len() == 8 && s.luminance.len() == 8,
        "mixer arrays must each contain 8 values"
    );
    for v in s.hue.iter().chain(&s.saturation).chain(&s.luminance) {
        finite_range(*v, -100.0..=100.0, "mixer value")?;
    }
    ensure!(s.points.len() <= 8, "mixer supports at most 8 point colors");
    for p in &s.points {
        finite_range(p.hue, 0.0..=360.0, "point hue")?;
        finite_range(p.saturation, 0.0..=1.0, "point saturation")?;
        finite_range(p.luminance, 0.0..=1.0, "point luminance")?;
        for v in [p.hue_shift, p.saturation_shift, p.luminance_shift] {
            finite_range(v, -100.0..=100.0, "point shift")?;
        }
        finite_range(p.hue_range, 5.0..=180.0, "point hueRange")?;
        finite_range(p.saturation_range, 0.05..=1.0, "point saturationRange")?;
        finite_range(p.luminance_range, 0.05..=1.0, "point luminanceRange")?;
    }
    Ok(())
}
fn validate_grading(s: &GradingSettings) -> Result<()> {
    for wheel in [s.shadows, s.midtones, s.highlights, s.global] {
        finite_range(wheel.hue, 0.0..=360.0, "grade hue")?;
        finite_range(wheel.saturation, 0.0..=100.0, "grade saturation")?;
        finite_range(wheel.luminance, -100.0..=100.0, "grade luminance")?;
    }
    finite_range(s.blending, 0.0..=100.0, "grading blending")?;
    finite_range(s.balance, -100.0..=100.0, "grading balance")?;
    Ok(())
}
fn validate_detail(s: &DetailSettings) -> Result<()> {
    finite_range(s.sharpen_amount, 0.0..=150.0, "sharpenAmount")?;
    for v in [
        s.sharpen_radius,
        s.sharpen_detail,
        s.sharpen_masking,
        s.noise_luminance,
        s.noise_luminance_detail,
        s.noise_luminance_contrast,
        s.noise_color,
        s.noise_color_detail,
        s.noise_color_smoothness,
    ] {
        finite_range(v, 0.0..=100.0, "detail value")?;
    }
    Ok(())
}
fn validate_optics(s: &OpticsSettings) -> Result<()> {
    for v in [
        s.profile_distortion,
        s.profile_vignetting,
        s.purple_amount,
        s.green_amount,
        s.vignette_midpoint,
    ] {
        finite_range(v, 0.0..=100.0, "optics value")?;
    }
    for v in [s.distortion, s.vignette_amount] {
        finite_range(v, -100.0..=100.0, "optics signed value")?;
    }
    for v in [
        s.purple_hue_low,
        s.purple_hue_high,
        s.green_hue_low,
        s.green_hue_high,
    ] {
        finite_range(v, 0.0..=360.0, "optics hue")?;
    }
    ensure!(
        s.purple_hue_low <= s.purple_hue_high && s.green_hue_low <= s.green_hue_high,
        "optics hue ranges must be ordered"
    );
    Ok(())
}
fn validate_geometry(s: &GeometrySettings) -> Result<()> {
    for v in [
        s.vertical,
        s.horizontal,
        s.aspect,
        s.scale,
        s.offset_x,
        s.offset_y,
    ] {
        finite_range(v, -100.0..=100.0, "geometry value")?;
    }
    finite_range(s.rotate, -45.0..=45.0, "geometry rotation")?;
    ensure!(s.guides.len() <= 16, "geometry supports at most 16 guides");
    for guide in &s.guides {
        for v in [guide.start_x, guide.start_y, guide.end_x, guide.end_y] {
            finite_range(v, 0.0..=1.0, "geometry guide coordinate")?;
        }
    }
    Ok(())
}
fn c(v: f64) -> f64 {
    v.clamp(0., 1.)
}
fn li(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn sr(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn y(a: [f64; 3]) -> f64 {
    0.2126 * a[0] + 0.7152 * a[1] + 0.0722 * a[2]
}
fn sl(a: &mut [f64; 3], t: f64) {
    let q = y(*a);
    if q < 1e-8 {
        if t > q {
            *a = [c(t); 3]
        }
    } else {
        for x in a {
            *x = c(*x * c(t) / q)
        }
    }
}
fn rgb_hsl(rgb: [f64; 3]) -> (f64, f64, f64) {
    let (mx, mn) = (
        rgb.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        rgb.iter().copied().fold(f64::INFINITY, f64::min),
    );
    let l = (mx + mn) * 0.5;
    let d = mx - mn;
    if d < 1e-6 {
        return (0., 0., l);
    }
    let s = d / (1.0_f64 - (2.0_f64 * l - 1.0_f64).abs());
    let mut h = if mx == rgb[0] {
        ((rgb[1] - rgb[2]) / d) % 6.
    } else if mx == rgb[1] {
        (rgb[2] - rgb[0]) / d + 2.
    } else {
        (rgb[0] - rgb[1]) / d + 4.
    };
    h /= 6.;
    if h < 0. {
        h += 1.
    }
    (h, s, l)
}
fn hue(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0. {
        t += 1.
    }
    if t > 1. {
        t -= 1.
    }
    if t < 1. / 6. {
        p + (q - p) * 6. * t
    } else if t < 0.5 {
        q
    } else if t < 2. / 3. {
        p + (q - p) * (2. / 3. - t) * 6.
    } else {
        p
    }
}
fn hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s <= 1e-6 {
        return [l; 3];
    }
    let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
    let p = 2. * l - q;
    [hue(p, q, h + 1. / 3.), hue(p, q, h), hue(p, q, h - 1. / 3.)]
}
fn calibrate(mut rgb: [f64; 3], s: &CalibrationSettings) -> [f64; 3] {
    if *s == CalibrationSettings::default() {
        return rgb;
    }
    let vs = match s.process {
        1 => 0.55,
        2 => 0.65,
        3 => 0.75,
        4 => 0.85,
        5 => 0.92,
        _ => 1.,
    };
    let (mut h, mut sat, l) = rgb_hsl(rgb);
    if l < 0.35 && s.shadow_tint != 0. {
        h += f64::from(s.shadow_tint) / 100. * vs * 0.06;
    }
    let (mx, mn) = (
        rgb.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        rgb.iter().copied().fold(f64::INFINITY, f64::min),
    );
    if mx - mn > 1e-5 {
        let (dh, ds) = if rgb[0] >= rgb[1] && rgb[0] >= rgb[2] {
            (s.red_hue, s.red_saturation)
        } else if rgb[1] >= rgb[0] && rgb[1] >= rgb[2] {
            (s.green_hue, s.green_saturation)
        } else {
            (s.blue_hue, s.blue_saturation)
        };
        h += f64::from(dh) / 100. * (15. / 360.) * vs;
        sat = c(sat * (1. + f64::from(ds) / 100. * 0.45 * vs));
    }
    h = h.rem_euclid(1.);
    rgb = hsl(h, sat, l);
    rgb
}
fn curve_value(x: f64, points: &[CurvePoint]) -> f64 {
    let i = points
        .windows(2)
        .position(|p| x < f64::from(p[1].x))
        .unwrap_or(points.len() - 2);
    let differences: Vec<f64> = points
        .windows(2)
        .map(|p| f64::from((p[1].y - p[0].y) / (p[1].x - p[0].x)))
        .collect();
    let slope = |j: usize| {
        if j == 0 {
            differences[0]
        } else if j == points.len() - 1 {
            differences[differences.len() - 1]
        } else if differences[j - 1] * differences[j] <= 0. {
            0.
        } else {
            2. / (1. / differences[j - 1] + 1. / differences[j])
        }
    };
    let h = f64::from(points[i + 1].x - points[i].x);
    let t = ((x - f64::from(points[i].x)) / h).clamp(0., 1.);
    let value = (2. * t.powi(3) - 3. * t.powi(2) + 1.) * f64::from(points[i].y)
        + (t.powi(3) - 2. * t.powi(2) + t) * h * slope(i)
        + (-2. * t.powi(3) + 3. * t.powi(2)) * f64::from(points[i + 1].y)
        + (t.powi(3) - t.powi(2)) * h * slope(i + 1);
    c(value)
}
fn parametric(tone: f64, s: &CurveSettings) -> f64 {
    let shadow = f64::from(s.shadow_split) / 100.;
    let dark = f64::from(s.dark_split) / 100.;
    let light = f64::from(s.light_split) / 100.;
    let (amount, lo, hi) = if tone < shadow {
        (s.shadows, 0., shadow)
    } else if tone < dark {
        (s.darks, shadow, dark)
    } else if tone < light {
        (s.lights, dark, light)
    } else {
        (s.highlights, light, 1.)
    };
    let span = (hi - lo).max(0.02);
    let weight = (1. - (tone - (lo + hi) / 2.).abs() / (span / 2.)).max(0.);
    c(tone + f64::from(amount) / 100. * weight * 0.22)
}
fn circular_distance(a: f64, b: f64) -> f64 {
    let distance = (a - b).abs();
    if distance > 0.5 {
        1. - distance
    } else {
        distance
    }
}
fn point_weight(h: f64, s: f64, l: f64, point: &PointColor) -> f64 {
    let hue_half = (f64::from(point.hue_range) / 360.).max(0.01);
    let sat_half = f64::from(point.saturation_range).max(0.01);
    let lum_half = f64::from(point.luminance_range).max(0.01);
    let hue_weight = 1. - circular_distance(h, f64::from(point.hue) / 360.) / hue_half;
    let sat_weight = 1. - (s - f64::from(point.saturation)).abs() / sat_half;
    let lum_weight = 1. - (l - f64::from(point.luminance)).abs() / lum_half;
    if hue_weight < 0. || sat_weight < 0. || lum_weight < 0. {
        0.
    } else {
        hue_weight * sat_weight * lum_weight
    }
}
fn box_blur_f32(source: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let mut temp = vec![0.; source.len()];
    let mut output = vec![0.; source.len()];
    let window = (radius * 2 + 1) as f64;
    for row in 0..height {
        for column in 0..width {
            let mut sum = 0.;
            for k in 0..=radius * 2 {
                let x = (column + k).saturating_sub(radius).min(width - 1);
                sum += f64::from(source[row * width + x]);
            }
            temp[row * width + column] = (sum / window) as f32;
        }
    }
    for column in 0..width {
        for row in 0..height {
            let mut sum = 0.;
            for k in 0..=radius * 2 {
                let y = (row + k).saturating_sub(radius).min(height - 1);
                sum += f64::from(temp[y * width + column]);
            }
            output[row * width + column] = (sum / window) as f32;
        }
    }
    output
}
// Reconstruct the preserved C stage's exact premultiplied byte boundary.
// Every legal premultiplied byte <= alpha round-trips through straight RGBA8,
// but p/255 is not the same normalized color as round(p*alpha/255)/alpha.
fn stage_rgb(pixel: &image::Rgba<u8>) -> [f64; 3] {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        return [0.; 3];
    }
    std::array::from_fn(|c| f64::from((u32::from(pixel[c]) * alpha + 127) / 255) / f64::from(alpha))
}
fn write_stage_rgb(pixel: &mut image::Rgba<u8>, rgb: [f64; 3]) {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        return;
    }
    for channel in 0..3 {
        let q = (c(rgb[channel]) * f64::from(alpha)).round() as u32;
        pixel[channel] = ((q * 255 + alpha / 2) / alpha).min(255) as u8;
    }
}
fn apply_effects(image: &mut RgbaImage, s: &Settings) {
    if s.texture == 0.
        && s.clarity == 0.
        && s.dehaze == 0.
        && s.glow == 0.
        && s.vignette_amount == 0.
    {
        return;
    }
    let (width, height) = (image.width() as usize, image.height() as usize);
    let luma: Vec<f32> = image
        .pixels()
        .map(|p| {
            if p[3] == 0 {
                0.
            } else {
                y(stage_rgb(p)) as f32
            }
        })
        .collect();
    let fine = if s.texture != 0. {
        Some(box_blur_f32(&luma, width, height, 1))
    } else {
        None
    };
    let coarse = if s.clarity != 0. {
        Some(box_blur_f32(&luma, width, height, 4))
    } else {
        None
    };
    let glow = if s.glow > 0. {
        let threshold = 0.55 + 0.4 * f64::from(s.glow_range) / 100.;
        let source: Vec<f32> = luma
            .iter()
            .map(|v| c((f64::from(*v) - threshold) / (1. - threshold).max(0.05)) as f32)
            .collect();
        let base = if s.glow_style == GlowStyle::Bloom {
            2.
        } else {
            5.
        };
        Some(box_blur_f32(
            &source,
            width,
            height,
            (base * (1. + f64::from(s.glow_spread) / 100.))
                .round()
                .clamp(1., 64.) as usize,
        ))
    } else {
        None
    };
    for (index, p) in image.pixels_mut().enumerate() {
        if p[3] == 0 {
            continue;
        }
        let mut rgb = stage_rgb(p);
        let tone = y(rgb);
        let mut detail = 0.;
        if let Some(v) = &fine {
            detail += f64::from(s.texture) / 100. * (tone - f64::from(v[index]));
        }
        if let Some(v) = &coarse {
            detail += f64::from(s.clarity) / 100. * (tone - f64::from(v[index]));
        }
        if detail != 0. {
            sl(&mut rgb, c(tone + detail));
        }
        if s.dehaze != 0. {
            let d = f64::from(s.dehaze) / 100.;
            let old = y(rgb);
            let contrast = 1. + 0.8 * d;
            let pivot = 0.45 - 0.1 * d.max(0.);
            let mut target = c(pivot + (old - 0.45) * contrast);
            if d < 0. {
                target = c(target + (-d) * (1. - target) * 0.45)
            } else {
                target = c(target - d * (0.4 - target).max(0.))
            }
            sl(&mut rgb, target);
            let lum = y(rgb);
            let saturation = 1. + 0.7 * d;
            for channel in &mut rgb {
                *channel = c(lum + (*channel - lum) * saturation);
            }
        }
        if let Some(glow) = &glow {
            let warmth = f64::from(s.glow_warmth) / 100.;
            let (red, green, blue, gain) = match s.glow_style {
                GlowStyle::Halation => (1., 0.35 - 0.3 * warmth, 0.2 - 0.2 * warmth, 1.),
                GlowStyle::Bloom => (
                    0.75 + 0.25 * warmth,
                    0.6 + 0.2 * warmth,
                    0.75 - 0.6 * warmth,
                    1.4,
                ),
                GlowStyle::Diffusion => (
                    0.75 + 0.25 * warmth,
                    0.6 + 0.2 * warmth,
                    0.75 - 0.6 * warmth,
                    1.,
                ),
            };
            let add = f64::from(glow[index]) * f64::from(s.glow) / 100. * gain;
            rgb[0] = c(rgb[0] + add * red);
            rgb[1] = c(rgb[1] + add * green);
            rgb[2] = c(rgb[2] + add * blue);
        }
        if s.vignette_amount != 0. {
            let x = index % width;
            let y0 = index / width;
            let nx = (x as f64 + 0.5) / width as f64 * 2. - 1.;
            let ny = (y0 as f64 + 0.5) / height as f64 * 2. - 1.;
            let square = nx.abs().max(ny.abs());
            let circle = nx.hypot(ny) / 2f64.sqrt();
            let shape = (1. - f64::from(s.vignette_roundness) / 100.) * 0.5;
            let distance = circle + (square - circle) * shape;
            let start = f64::from(s.vignette_midpoint) / 100. * 0.85;
            let soft = (f64::from(s.vignette_feather) / 100.).max(0.05);
            let t = c((distance - start) / soft);
            let mask = t * t * (3. - 2. * t);
            let mut effect = f64::from(s.vignette_amount) / 100. * mask;
            if effect < 0. && s.vignette_style == VignetteStyle::HighlightPriority {
                let bright = c((y(rgb) - 0.45) / 0.55);
                effect *= 1. - f64::from(s.vignette_highlights) / 100. * bright;
            }
            if effect < 0. {
                for channel in &mut rgb {
                    *channel *= 1. + effect
                }
            } else {
                for channel in &mut rgb {
                    *channel += (1. - *channel) * effect
                }
            }
            if s.vignette_style == VignetteStyle::ColorPriority && mask > 0. {
                let lum = y(rgb);
                let sat = 1. - 0.75 * mask * (f64::from(s.vignette_amount) / 100.).abs();
                for channel in &mut rgb {
                    *channel = c(lum + (*channel - lum) * sat)
                }
            }
        }
        write_stage_rgb(p, rgb);
    }
}
fn mix32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^ (value >> 16)
}
fn lattice(x: i64, y0: i64, seed: u32) -> f64 {
    let hash = mix32(
        (x as u32).wrapping_mul(0x9E3779B1) ^ mix32((y0 as u32).wrapping_mul(0x85EBCA77) ^ seed),
    );
    f64::from(hash & 0xffff) / 65535. + f64::from(hash >> 16) / 65535. - 1.
}
fn grain_field(u: f64, v: f64, scale: f64, seed: u32) -> f64 {
    let (cx, cy) = ((u / scale).floor(), (v / scale).floor());
    let mut tx = u / scale - cx;
    let mut ty = v / scale - cy;
    tx = tx * tx * (3. - 2. * tx);
    ty = ty * ty * (3. - 2. * ty);
    let (ix, iy) = (cx as i64, cy as i64);
    let top = lattice(ix, iy, seed) + (lattice(ix + 1, iy, seed) - lattice(ix, iy, seed)) * tx;
    let bottom = lattice(ix, iy + 1, seed)
        + (lattice(ix + 1, iy + 1, seed) - lattice(ix, iy + 1, seed)) * tx;
    (top + (bottom - top) * ty) * 1.6
}
fn apply_grain(image: &mut RgbaImage, s: &Settings) {
    if s.grain_amount <= 0. {
        return;
    }
    let size = 0.5 + f64::from(s.grain_size) / 100. * 19.5;
    let detail = (size * 0.35).max(0.5);
    let rough = f64::from(s.grain_roughness) / 100.;
    let fine_seed = mix32(0xA511E9B3);
    let strength = f64::from(s.grain_amount) / 100. * 0.35 * 255.;
    for (x, y0, p) in image.enumerate_pixels_mut() {
        if p[3] == 0 {
            continue;
        }
        let u = f64::from(x) + 0.5;
        let v = f64::from(y0) + 0.5;
        let smooth = grain_field(u, v, size, 0);
        let fine = grain_field(u, v, detail, fine_seed);
        let noise = smooth + (fine - smooth) * rough;
        let level = y([
            f64::from(p[0]) / 255.,
            f64::from(p[1]) / 255.,
            f64::from(p[2]) / 255.,
        ]);
        let delta = noise * strength * (0.4 + 2.4 * level * (1. - level));
        for channel in 0..3 {
            p[channel] = (f64::from(p[channel]) + delta).clamp(0., 255.).round() as u8;
        }
    }
}
fn edge_at_f32(
    luma: &[f32],
    width: usize,
    height: usize,
    x: usize,
    y0: usize,
    radius: usize,
) -> f32 {
    let center = luma[y0 * width + x];
    let mut sum = 0f32;
    let mut count = 0;
    for dy in [-(radius as isize), 0, radius as isize] {
        for dx in [-(radius as isize), 0, radius as isize] {
            if dx == 0 && dy == 0 {
                continue;
            }
            let sx = x as isize + dx;
            let sy = y0 as isize + dy;
            if sx >= 0 && sy >= 0 && (sx as usize) < width && (sy as usize) < height {
                sum += (luma[sy as usize * width + sx as usize] - center).abs();
                count += 1;
            }
        }
    }
    if count > 0 { sum / count as f32 } else { 0. }
}

fn write_premultiplied_pixel(pixel: &mut image::Rgba<u8>, rgb: [f64; 3]) {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        pixel.0[..3].fill(0);
        return;
    }
    for channel in 0..3 {
        let premultiplied = (rgb[channel].clamp(0., 1.) * f64::from(alpha))
            .round()
            .clamp(0., f64::from(alpha)) as u32;
        pixel[channel] = premultiplied as u8;
    }
}

fn apply_detail(image: &mut RgbaImage, s: &DetailSettings) {
    if s.sharpen_amount == 0. && s.noise_luminance == 0. && s.noise_color == 0. {
        return;
    }
    for pixel in image.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in 0..3 {
            pixel[channel] = ((u32::from(pixel[channel]) * alpha + 127) / 255) as u8;
        }
    }
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut luma: Vec<f32> = image
        .pixels()
        .map(|p| {
            if p[3] == 0 {
                0.
            } else {
                let alpha = f64::from(p[3]);
                y([
                    f64::from(p[0]) / alpha,
                    f64::from(p[1]) / alpha,
                    f64::from(p[2]) / alpha,
                ]) as f32
            }
        })
        .collect();
    if s.noise_luminance > 0. {
        let radius = (1. + f64::from(s.noise_luminance) / 50.)
            .round()
            .clamp(1., 64.) as usize;
        let blurred = box_blur_f32(&luma, width, height, radius);
        let original = luma.clone();
        for (index, p) in image.pixels_mut().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let x = index % width;
            let y0 = index / width;
            let edge = edge_at_f32(&original, width, height, x, y0, 1);
            let local = f64::from(s.noise_luminance) / 100.
                * (1.
                    - f64::from(s.noise_luminance_detail) / 100. * f64::from((edge * 6.).min(1.)));
            let mut target = (f64::from(original[index]) * (1. - local)
                + f64::from(blurred[index]) * local) as f32;
            target = (f64::from(target)
                + f64::from(s.noise_luminance_contrast) / 100.
                    * 0.25
                    * f64::from(original[index] - blurred[index])) as f32;
            luma[index] = target;
            let mut rgb = [
                f64::from(p[0]) / f64::from(p[3]),
                f64::from(p[1]) / f64::from(p[3]),
                f64::from(p[2]) / f64::from(p[3]),
            ];
            sl(&mut rgb, f64::from(target));
            write_premultiplied_pixel(p, rgb);
        }
    }
    if s.noise_color > 0. {
        let radius = (1. + f64::from(s.noise_color_smoothness) / 40.)
            .round()
            .clamp(1., 64.) as usize;
        let chroma: Vec<f32> = image
            .pixels()
            .map(|p| {
                if p[3] == 0 {
                    0.
                } else {
                    let alpha = f64::from(p[3]);
                    rgb_hsl([
                        f64::from(p[0]) / alpha,
                        f64::from(p[1]) / alpha,
                        f64::from(p[2]) / alpha,
                    ])
                    .1 as f32
                }
            })
            .collect();
        let blurred = box_blur_f32(&chroma, width, height, radius);
        for (index, p) in image.pixels_mut().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let edge = (chroma[index] - blurred[index]).abs();
            let local = f64::from(s.noise_color) / 100.
                * (1. - f64::from(s.noise_color_detail) / 100. * f64::from((edge * 4.).min(1.)));
            let saturation = chroma[index] * (1. - local) as f32 + blurred[index] * local as f32;
            let (h, _, l) = rgb_hsl([
                f64::from(p[0]) / f64::from(p[3]),
                f64::from(p[1]) / f64::from(p[3]),
                f64::from(p[2]) / f64::from(p[3]),
            ]);
            let rgb = hsl(h, f64::from(saturation), l);
            write_premultiplied_pixel(p, rgb);
        }
    }
    if s.sharpen_amount > 0. {
        luma = image
            .pixels()
            .map(|p| {
                if p[3] == 0 {
                    0.
                } else {
                    let alpha = f64::from(p[3]);
                    y([
                        f64::from(p[0]) / alpha,
                        f64::from(p[1]) / alpha,
                        f64::from(p[2]) / alpha,
                    ]) as f32
                }
            })
            .collect();
        let radius = (0.5 + f64::from(s.sharpen_radius) / 100. * 2.5)
            .round()
            .clamp(1., 64.) as usize;
        let blurred = box_blur_f32(&luma, width, height, radius);
        let detail = f64::from(s.sharpen_detail) / 100.;
        let threshold = f64::from(s.sharpen_masking) / 100. * 0.35;
        for (index, p) in image.pixels_mut().enumerate() {
            if p[3] == 0 {
                continue;
            }
            let mask = c((f64::from(edge_at_f32(
                &luma,
                width,
                height,
                index % width,
                index / width,
                radius,
            )) * (0.5 + detail)
                - threshold)
                / (0.35 - threshold * 0.5).max(0.04));
            let target = c(f64::from(luma[index])
                + f64::from(luma[index] - blurred[index]) * f64::from(s.sharpen_amount) / 100.
                    * mask
                    * (0.5 + detail));
            let mut rgb = [
                f64::from(p[0]) / f64::from(p[3]),
                f64::from(p[1]) / f64::from(p[3]),
                f64::from(p[2]) / f64::from(p[3]),
            ];
            sl(&mut rgb, target);
            write_premultiplied_pixel(p, rgb);
        }
    }
    for pixel in image.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        if alpha != 0 {
            for channel in 0..3 {
                pixel[channel] =
                    ((u32::from(pixel[channel]) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}
fn lens_distort(image: &RgbaImage, k: f64) -> RgbaImage {
    let (width, height) = (image.width(), image.height());
    let (cx, cy) = (f64::from(width) * 0.5, f64::from(height) * 0.5);
    let diagonal = cx * cx + cy * cy;
    let mut output = RgbaImage::new(width, height);
    for y0 in 0..height {
        for x0 in 0..width {
            let dx = f64::from(x0) + 0.5 - cx;
            let dy = f64::from(y0) + 0.5 - cy;
            let scale = 1. - k * (dx * dx + dy * dy) / diagonal;
            let sx = cx + dx * scale - 0.5;
            let sy = cy + dy * scale - 0.5;
            let bx = sx.floor() as i64;
            let by = sy.floor() as i64;
            let fx = sx - sx.floor();
            let fy = sy - sy.floor();
            let mut sums = [0.; 4];
            for j in 0..2 {
                for i in 0..2 {
                    let px = bx + i;
                    let py = by + j;
                    if px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
                        continue;
                    }
                    let weight =
                        (if i == 0 { 1. - fx } else { fx }) * (if j == 0 { 1. - fy } else { fy });
                    let pixel = image.get_pixel(px as u32, py as u32);
                    let alpha = f64::from(pixel[3]);
                    for channel in 0..3 {
                        // The preserved kernel resamples premultiplied bytes. The Camera Raw
                        // pipeline stores straight RGBA between stages, so recreate that byte
                        // boundary here and unpremultiply the result below.
                        let premultiplied = (f64::from(pixel[channel]) * alpha / 255.)
                            .round()
                            .min(alpha);
                        sums[channel] += weight * premultiplied;
                    }
                    sums[3] += weight * alpha;
                }
            }
            let alpha = sums[3].round().clamp(0., 255.) as u32;
            let mut straight = [0u8; 4];
            straight[3] = alpha as u8;
            if alpha != 0 {
                for channel in 0..3 {
                    let premultiplied = sums[channel].round().clamp(0., f64::from(alpha)) as u32;
                    straight[channel] = ((premultiplied * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
            output.put_pixel(x0, y0, image::Rgba(straight));
        }
    }
    output
}
fn hue_degrees(rgb: [f64; 3]) -> f64 {
    rgb_hsl(rgb).0 * 360.
}
fn hue_in_range(hue: f64, low: f64, high: f64) -> bool {
    if low <= high {
        hue >= low && hue <= high
    } else {
        hue >= low || hue <= high
    }
}
fn apply_optics(image: &mut RgbaImage, s: &OpticsSettings) {
    if *s == OpticsSettings::default() {
        return;
    }
    let distortion = f64::from(s.distortion) / 100. * 0.35
        + if s.enable_lens_profile {
            f64::from(s.profile_distortion) / 100. * 0.35
        } else {
            0.
        };
    if distortion != 0. {
        *image = lens_distort(image, distortion)
    }
    let (width, height) = (image.width(), image.height());
    if s.remove_chromatic_aberration {
        let source = image.clone();
        let (cx, cy) = (f64::from(width) * 0.5, f64::from(height) * 0.5);
        let max_radius = cx.hypot(cy);
        for y0 in 0..height {
            for x0 in 0..width {
                let alpha = image.get_pixel(x0, y0)[3];
                if alpha == 0 {
                    continue;
                }
                let dx = f64::from(x0) + 0.5 - cx;
                let dy = f64::from(y0) + 0.5 - cy;
                let shift = 0.45 * (dx.hypot(dy) / max_radius).powi(2) * 2.5;
                let rx = (f64::from(x0) - shift)
                    .round()
                    .clamp(0., f64::from(width - 1)) as u32;
                let bx = (f64::from(x0) + shift)
                    .round()
                    .clamp(0., f64::from(width - 1)) as u32;
                let p = image.get_pixel_mut(x0, y0);
                write_stage_rgb(
                    p,
                    [
                        stage_rgb(source.get_pixel(rx, y0))[0],
                        stage_rgb(source.get_pixel(x0, y0))[1],
                        stage_rgb(source.get_pixel(bx, y0))[2],
                    ],
                );
            }
        }
    }
    let vignette = f64::from(s.vignette_amount)
        + if s.enable_lens_profile {
            f64::from(s.profile_vignetting) / 100. * 35.
        } else {
            0.
        };
    for y0 in 0..height {
        for x0 in 0..width {
            let p = image.get_pixel_mut(x0, y0);
            if p[3] == 0 {
                continue;
            }
            let mut rgb = stage_rgb(p);
            let hue = hue_degrees(rgb);
            let max = rgb.iter().copied().fold(0., f64::max);
            let min = rgb.iter().copied().fold(1., f64::min);
            if max - min > 1e-6 {
                let saturation = (max - min) / max;
                let mut reduce: f64 = 0.;
                if s.purple_amount > 0.
                    && hue_in_range(
                        hue,
                        f64::from(s.purple_hue_low),
                        f64::from(s.purple_hue_high),
                    )
                {
                    reduce = reduce.max(f64::from(s.purple_amount) / 100.)
                }
                if s.green_amount > 0.
                    && hue_in_range(hue, f64::from(s.green_hue_low), f64::from(s.green_hue_high))
                {
                    reduce = reduce.max(f64::from(s.green_amount) / 100.)
                }
                if reduce > 0. {
                    let lum = y(rgb);
                    let factor = 1. - reduce * saturation;
                    for channel in &mut rgb {
                        *channel = c(lum + (*channel - lum) * factor);
                    }
                }
            }
            if vignette != 0. {
                let nx = (f64::from(x0) + 0.5) / f64::from(width) * 2. - 1.;
                let ny = (f64::from(y0) + 0.5) / f64::from(height) * 2. - 1.;
                let distance = nx.hypot(ny) / 2f64.sqrt();
                let start = f64::from(s.vignette_midpoint) / 100. * 0.85;
                let t = c((distance - start) / 0.35);
                let mask = t * t * (3. - 2. * t);
                let lift = vignette / 100. * mask;
                if lift > 0. {
                    for channel in &mut rgb {
                        *channel = c(*channel + (1. - *channel) * lift)
                    }
                } else {
                    for channel in &mut rgb {
                        *channel *= 1. + lift
                    }
                }
            }
            write_stage_rgb(p, rgb);
        }
    }
}
fn solve_homography(from: [[f64; 2]; 4], to: [[f64; 2]; 4]) -> Option<[f64; 8]> {
    let mut a = [[0.; 9]; 8];
    for i in 0..4 {
        let (x, y) = (from[i][0], from[i][1]);
        let (u, v) = (to[i][0], to[i][1]);
        a[i * 2] = [x, y, 1., 0., 0., 0., -u * x, -u * y, u];
        a[i * 2 + 1] = [0., 0., 0., x, y, 1., -v * x, -v * y, v];
    }
    for column in 0..8 {
        let pivot =
            (column..8).max_by(|&x, &y| a[x][column].abs().total_cmp(&a[y][column].abs()))?;
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        let divisor = a[column][column];
        for j in column..9 {
            a[column][j] /= divisor;
        }
        for row in 0..8 {
            if row == column {
                continue;
            }
            let factor = a[row][column];
            for j in column..9 {
                a[row][j] -= factor * a[column][j];
            }
        }
    }
    Some(std::array::from_fn(|i| a[i][8]))
}
fn sample_bilinear(image: &RgbaImage, x: f64, y0: f64) -> image::Rgba<u8> {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    let bx = x.floor() as i64;
    let by = y0.floor() as i64;
    let (fx, fy) = (x - x.floor(), y0 - y0.floor());
    let mut sum = [0.; 4];
    for j in 0..2 {
        for i in 0..2 {
            let (px, py) = (bx + i, by + j);
            if px < 0 || py < 0 || px >= width || py >= height {
                continue;
            }
            let weight = (if i == 0 { 1. - fx } else { fx }) * (if j == 0 { 1. - fy } else { fy });
            let pixel = image.get_pixel(px as u32, py as u32);
            let alpha = f64::from(pixel[3]) / 255.;
            for channel in 0..3 {
                sum[channel] += weight * f64::from(pixel[channel]) * alpha;
            }
            sum[3] += weight * f64::from(pixel[3]);
        }
    }
    let alpha = sum[3].round().clamp(0., 255.) as u8;
    if alpha == 0 {
        return image::Rgba([0; 4]);
    }
    let premultiplied = [
        sum[0].round().clamp(0., f64::from(alpha)) as u32,
        sum[1].round().clamp(0., f64::from(alpha)) as u32,
        sum[2].round().clamp(0., f64::from(alpha)) as u32,
    ];
    image::Rgba([
        ((premultiplied[0] * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8,
        ((premultiplied[1] * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8,
        ((premultiplied[2] * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8,
        alpha,
    ])
}
fn apply_geometry(image: &RgbaImage, s: &GeometrySettings) -> RgbaImage {
    let guides: Vec<_> = s
        .guides
        .iter()
        .copied()
        .filter(|g| f64::from(g.end_x - g.start_x).hypot(f64::from(g.end_y - g.start_y)) > 0.01)
        .collect();
    let uses_guides = s.upright == UprightMode::Guided && !guides.is_empty();
    if !uses_guides
        && s.vertical == 0.
        && s.horizontal == 0.
        && s.rotate == 0.
        && s.aspect == 0.
        && s.scale == 0.
        && s.offset_x == 0.
        && s.offset_y == 0.
    {
        return image.clone();
    }
    let (mut vertical, mut horizontal, mut rotate) = (
        f64::from(s.vertical),
        f64::from(s.horizontal),
        f64::from(s.rotate),
    );
    if uses_guides {
        let first = guides[0];
        let angle = f64::from(first.end_y - first.start_y)
            .atan2(f64::from(first.end_x - first.start_x))
            .to_degrees();
        let mut correction = -angle;
        if correction > 45. {
            correction -= 90.
        } else if correction < -45. {
            correction += 90.
        }
        rotate += correction;
        if let Some(second) = guides.get(1) {
            let angle = f64::from(second.end_y - second.start_y)
                .atan2(f64::from(second.end_x - second.start_x))
                .to_degrees();
            if angle.abs() > 45. {
                vertical += if angle > 0. { 25. } else { -25. }
            } else {
                horizontal += if angle > 0. { 25. } else { -25. }
            }
        }
    }
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let strength = if s.projection == Projection::Perspective {
        1.
    } else {
        0.55
    };
    let v = vertical / 100. * w * 0.18 * strength;
    let hz = horizontal / 100. * h * 0.18 * strength;
    let aspect = 1. + f64::from(s.aspect) / 200.;
    let zoom = 1. + f64::from(s.scale) / 100.;
    let shift_x = f64::from(s.offset_x) / 100. * w * 0.15;
    let shift_y = f64::from(s.offset_y) / 100. * h * 0.15;
    let mut corners = [
        [-v + shift_x, 0. - shift_y],
        [w + v + shift_x, 0. - shift_y],
        [w + hz + shift_x, h + shift_y],
        [-hz + shift_x, h + shift_y],
    ];
    let center = [w / 2. + shift_x, h / 2. - shift_y];
    let radians = -rotate.to_radians();
    for point in &mut corners {
        let (dx, dy) = (point[0] - center[0], point[1] - center[1]);
        point[0] = center[0] + dx * radians.cos() - dy * radians.sin();
        point[1] = center[1] + dx * radians.sin() + dy * radians.cos();
        point[0] = center[0] + (point[0] - center[0]) * aspect * zoom;
        point[1] = center[1] + (point[1] - center[1]) / aspect * zoom;
    }
    let source = [[0., 0.], [w, 0.], [w, h], [0., h]];
    let Some(map) = solve_homography(corners, source) else {
        return image.clone();
    };
    let mut output = RgbaImage::new(image.width(), image.height());
    for y0 in 0..image.height() {
        for x0 in 0..image.width() {
            let x = f64::from(x0) + 0.5;
            let y = f64::from(y0) + 0.5;
            let denominator = map[6] * x + map[7] * y + 1.;
            let sx = (map[0] * x + map[1] * y + map[2]) / denominator - 0.5;
            let sy = (map[3] * x + map[4] * y + map[5]) / denominator - 0.5;
            output.put_pixel(x0, y0, sample_bilinear(image, sx, sy));
        }
    }
    if !s.constrain_crop {
        return output;
    }
    let mut min_x = image.width();
    let mut min_y = image.height();
    let mut max_x = 0;
    let mut max_y = 0;
    for (x, y0, p) in output.enumerate_pixels() {
        if p[3] > 0 {
            min_x = min_x.min(x);
            min_y = min_y.min(y0);
            max_x = max_x.max(x + 1);
            max_y = max_y.max(y0 + 1);
        }
    }
    if min_x >= max_x || min_y >= max_y {
        return output;
    }
    let crop =
        image::imageops::crop_imm(&output, min_x, min_y, max_x - min_x, max_y - min_y).to_image();
    let scale = (w / f64::from(crop.width())).min(h / f64::from(crop.height()));
    let (nw, nh) = (
        (f64::from(crop.width()) * scale).round() as u32,
        (f64::from(crop.height()) * scale).round() as u32,
    );
    let resized = image::imageops::resize(&crop, nw, nh, image::imageops::FilterType::Triangle);
    let mut fitted = RgbaImage::new(image.width(), image.height());
    image::imageops::overlay(
        &mut fitted,
        &resized,
        i64::from((image.width() - nw) / 2),
        i64::from((image.height() - nh) / 2),
    );
    fitted
}
fn apply_curve_color(mut rgb: [f64; 3], s: &Settings) -> [f64; 3] {
    let tone = y(rgb);
    let mapped = curve_value(parametric(tone, &s.curve), &s.curve.rgb);
    sl(&mut rgb, mapped);
    if s.curve.refine_saturation != 0. && tone > 1e-4 {
        let factor = 1. + f64::from(s.curve.refine_saturation) / 100. * (mapped / tone - 1.);
        let lum = y(rgb);
        for channel in &mut rgb {
            *channel = c(lum + (*channel - lum) * factor);
        }
    }
    rgb[0] = curve_value(rgb[0], &s.curve.red);
    rgb[1] = curve_value(rgb[1], &s.curve.green);
    rgb[2] = curve_value(rgb[2], &s.curve.blue);
    let (mut h, mut saturation, mut luminance) = rgb_hsl(rgb);
    const CENTERS: [f64; 8] = [
        0.,
        30. / 360.,
        60. / 360.,
        120. / 360.,
        180. / 360.,
        240. / 360.,
        270. / 360.,
        300. / 360.,
    ];
    let mut hue_delta = 0.;
    let mut saturation_delta = 0.;
    let mut luminance_delta = 0.;
    let mut weight_sum = 0.;
    for (index, center) in CENTERS.iter().enumerate() {
        let weight = 1. - circular_distance(h, *center) / (40. / 360.);
        if weight <= 0. {
            continue;
        }
        hue_delta += f64::from(s.mixer.hue[index]) / 100. * weight * (30. / 360.);
        saturation_delta += f64::from(s.mixer.saturation[index]) / 100. * weight;
        luminance_delta += f64::from(s.mixer.luminance[index]) / 100. * weight * 0.25;
        weight_sum += weight;
    }
    if weight_sum > 1. {
        hue_delta /= weight_sum;
        saturation_delta /= weight_sum;
        luminance_delta /= weight_sum;
    }
    h = (h + hue_delta).rem_euclid(1.);
    saturation = c(saturation * (1. + saturation_delta));
    luminance = c(luminance + luminance_delta);
    for point in &s.mixer.points {
        let weight = point_weight(h, saturation, luminance, point);
        if weight <= 0. {
            continue;
        }
        h += f64::from(point.hue_shift) / 100. * weight * (30. / 360.);
        saturation = c(saturation * (1. + f64::from(point.saturation_shift) / 100. * weight));
        luminance = c(luminance + f64::from(point.luminance_shift) / 100. * weight * 0.25);
    }
    rgb = hsl(h.rem_euclid(1.), saturation, luminance);
    let split = 0.5 - f64::from(s.grading.balance) / 100. * 0.2;
    let reach = 0.12 + f64::from(s.grading.blending) / 100. * 0.38;
    let lum = y(rgb);
    let mut weights = [
        c((split + reach - lum) / (reach * 2.).max(0.05)),
        c(1. - (lum - split).abs() / (0.35 + reach)),
        c((lum - (split - reach)) / (reach * 2.).max(0.05)),
        1.,
    ];
    let sum = weights[0] + weights[1] + weights[2];
    if sum > 1e-4 {
        for weight in &mut weights[..3] {
            *weight /= sum;
        }
    }
    for (wheel, weight) in [
        s.grading.shadows,
        s.grading.midtones,
        s.grading.highlights,
        s.grading.global,
    ]
    .iter()
    .zip(weights)
    {
        let grade_saturation = f64::from(wheel.saturation) / 100.;
        let grade_luminance = f64::from(wheel.luminance) / 100.;
        if weight <= 0. || (grade_saturation <= 0. && grade_luminance == 0.) {
            continue;
        }
        let color = hsl(f64::from(wheel.hue) / 360., 1., 0.5);
        for channel in 0..3 {
            rgb[channel] =
                c(rgb[channel] + (color[channel] - 0.5) * grade_saturation * weight * 0.85);
        }
        if grade_luminance != 0. {
            let target = c(y(rgb) + grade_luminance * 0.25 * weight);
            sl(&mut rgb, target);
        }
    }
    rgb
}
pub fn apply(i: &RgbaImage, s: &Settings) -> Result<RgbaImage> {
    ensure!(
        crate::model::valid_dimensions(i.width(), i.height())
            && u64::from(i.width()) * u64::from(i.height()) <= 16_777_216,
        "Camera Raw supports images up to 16 million pixels"
    );
    validate(s)?;
    if *s == Settings::default() {
        return Ok(i.clone());
    }
    let mut o = apply_geometry(i, &s.geometry);
    let (w, m) = (f64::from(s.temperature) / 100., f64::from(s.tint) / 100.);
    let g = [
        1. + 0.35 * w + 0.15 * m,
        1. - 0.3 * m,
        1. - 0.35 * w + 0.15 * m,
    ];
    for p in o.pixels_mut() {
        if p[3] == 0 {
            p.0 = [0; 4];
            continue;
        }
        // Every preserved C stage reads and writes premultiplied bytes. Recreate
        // that quantization boundary before calibration and again before Basic;
        // feeding straight-alpha channels directly can diverge dramatically at
        // low alpha after nonlinear exposure and tone operations.
        let alpha = u32::from(p[3]);
        let mut premultiplied = [0u8; 3];
        for channel in 0..3 {
            premultiplied[channel] = ((u32::from(p[channel]) * alpha + 127) / 255).min(alpha) as u8;
        }
        let mut a = calibrate(
            [
                f64::from(premultiplied[0]) / f64::from(alpha),
                f64::from(premultiplied[1]) / f64::from(alpha),
                f64::from(premultiplied[2]) / f64::from(alpha),
            ],
            &s.calibration,
        );
        for channel in 0..3 {
            premultiplied[channel] = (a[channel] * f64::from(alpha))
                .round()
                .clamp(0., f64::from(alpha)) as u8;
            a[channel] = f64::from(premultiplied[channel]) / f64::from(alpha);
        }
        for k in 0..3 {
            a[k] = c(li(a[k]) * g[k] * 2f64.powf(f64::from(s.exposure)));
            a[k] = c(0.5 + (sr(a[k]) - 0.5) * (1. + f64::from(s.contrast) / 100.))
        }
        // The C kernel rescales and clamps RGB after each tone control. Applying
        // all four controls to luminance before one rescale loses those stage
        // boundaries and can turn saturated highlights into full clipping.
        let mut luminance = y(a);
        let t = c((luminance - 0.5) / 0.5).powi(2);
        luminance = c(if s.highlights >= 0. {
            luminance + f64::from(s.highlights) / 100. * t * (1. - luminance)
        } else {
            luminance + f64::from(s.highlights) / 100. * t * (luminance - 0.5)
        });
        sl(&mut a, luminance);
        luminance = y(a);
        let t = c((0.5 - luminance) / 0.5).powi(2);
        luminance = c(if s.shadows >= 0. {
            luminance + f64::from(s.shadows) / 100. * t * (0.5 - luminance)
        } else {
            luminance + f64::from(s.shadows) / 100. * t * luminance
        });
        sl(&mut a, luminance);
        luminance = y(a);
        if luminance > 0.75 {
            luminance = c(0.75 + (luminance - 0.75) * (1. + f64::from(s.whites) / 100.));
        }
        sl(&mut a, luminance);
        luminance = y(a);
        if luminance < 0.25 {
            luminance = c(0.25 + (luminance - 0.25) * (1. - f64::from(s.blacks) / 100.));
        }
        sl(&mut a, luminance);
        let l = y(a);
        let mx = a.iter().copied().fold(0., f64::max);
        let mn = a.iter().copied().fold(1., f64::min);
        let chroma = mx - mn;
        let sat = if mx > 1e-8 { chroma / mx } else { 0. };
        let mut hue = 0.;
        if chroma > 1e-8 {
            hue = if a[0] >= a[1] && a[0] >= a[2] {
                60. * ((a[1] - a[2]) / chroma % 6.)
            } else if a[1] >= a[0] && a[1] >= a[2] {
                60. * ((a[2] - a[0]) / chroma + 2.)
            } else {
                60. * ((a[0] - a[1]) / chroma + 4.)
            };
            if hue < 0. {
                hue += 360.;
            }
        }
        let mut skin = 0.;
        if (10. ..=50.).contains(&hue) {
            skin = if hue <= 30. {
                (hue - 10.) / 20.
            } else {
                (50. - hue) / 20.
            };
            skin *= c((sat - 0.15) / 0.35);
        }
        let vibrance = f64::from(s.vibrance) / 100.;
        let mut amount = vibrance * (1. - sat);
        if vibrance > 0. {
            amount *= 1. - 0.7 * skin;
        }
        let vf = 1. + amount;
        for x in &mut a {
            *x = c(l + (*x - l) * vf)
        }
        let l = y(a);
        let sf = 1. + f64::from(s.saturation) / 100.;
        for x in &mut a {
            *x = c(l + (*x - l) * sf)
        }
        for channel in 0..3 {
            premultiplied[channel] = (a[channel] * f64::from(alpha))
                .round()
                .clamp(0., f64::from(alpha)) as u8;
            a[channel] = f64::from(premultiplied[channel]) / f64::from(alpha);
        }
        a = apply_curve_color(a, s);
        for k in 0..3 {
            let q = (a[k] * f64::from(alpha))
                .round()
                .clamp(0., f64::from(alpha)) as u32;
            p[k] = ((q * 255 + alpha / 2) / alpha).min(255) as u8
        }
    }
    apply_effects(&mut o, s);
    apply_grain(&mut o, s);
    apply_optics(&mut o, &s.optics);
    apply_detail(&mut o, &s.detail);
    Ok(o)
}
/// Preview-only clipping overlay. Never write this result into artwork.
/// Uses the preserved C kernel's premultiplied thresholds and shadow-first order.
pub fn clipping_preview(image: &RgbaImage, shadows: bool, highlights: bool) -> RgbaImage {
    let mut out = image.clone();
    if !shadows && !highlights {
        return out;
    }
    for pixel in out.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        let mut rgb = [0.; 3];
        for channel in 0..3 {
            rgb[channel] =
                f64::from((u32::from(pixel[channel]) * alpha + 127) / 255) / f64::from(alpha);
        }
        if shadows && rgb.iter().any(|v| *v <= 0.5 / 255.) {
            rgb[0] *= 0.35;
            rgb[1] *= 0.35;
            rgb[2] = rgb[2] * 0.35 + 0.65;
        }
        if highlights && rgb.iter().any(|v| *v >= 254.5 / 255.) {
            rgb[0] = rgb[0] * 0.35 + 0.65;
            rgb[1] *= 0.35;
            rgb[2] *= 0.35;
        }
        for channel in 0..3 {
            let premultiplied = (rgb[channel] * f64::from(alpha)).round() as u32;
            pixel[channel] = ((premultiplied * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn clipping_preview_preserves_source_and_alpha() {
        let source =
            RgbaImage::from_raw(3, 1, vec![0, 0, 0, 128, 255, 255, 255, 255, 17, 31, 47, 0])
                .unwrap();
        let preview = clipping_preview(&source, true, true);
        assert_eq!(preview.get_pixel(0, 0).0, [0, 0, 165, 128]);
        assert_eq!(preview.get_pixel(1, 0).0, [255, 89, 89, 255]);
        assert_eq!(preview.get_pixel(2, 0), source.get_pixel(2, 0));
        assert_eq!(clipping_preview(&source, false, false), source);
        assert_eq!(source.get_pixel(0, 0).0, [0, 0, 0, 128]);
    }
    #[test]
    fn identity() {
        let i = RgbaImage::from_pixel(1, 1, Rgba([20, 80, 160, 128]));
        assert_eq!(apply(&i, &Settings::default()).unwrap(), i)
    }
    #[test]
    fn geometry_filters_short_guides_before_using_first_two() {
        let image = RgbaImage::from_fn(12, 10, |x, y| {
            Rgba([(x * 17) as u8, (y * 23) as u8, 90, 255])
        });
        let short = GeometryGuide {
            start_x: 0.2,
            start_y: 0.2,
            end_x: 0.205,
            end_y: 0.2,
        };
        let first = GeometryGuide {
            start_x: 0.1,
            start_y: 0.2,
            end_x: 0.9,
            end_y: 0.35,
        };
        let second = GeometryGuide {
            start_x: 0.3,
            start_y: 0.1,
            end_x: 0.4,
            end_y: 0.9,
        };
        let mut with_short = GeometrySettings {
            upright: UprightMode::Guided,
            guides: vec![short, first, short, second],
            ..Default::default()
        };
        let expected = GeometrySettings {
            upright: UprightMode::Guided,
            guides: vec![first, second],
            ..Default::default()
        };
        assert_eq!(
            apply_geometry(&image, &with_short),
            apply_geometry(&image, &expected)
        );
        with_short.guides = vec![short];
        assert_eq!(apply_geometry(&image, &with_short), image);
    }
    #[test]
    fn geometry_interpolates_premultiplied_without_transparent_color_halos() {
        let image = RgbaImage::from_fn(16, 16, |x, _| {
            if x < 8 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 0])
            }
        });
        let settings = GeometrySettings {
            rotate: 7.,
            ..Default::default()
        };
        let output = apply_geometry(&image, &settings);
        let edges: Vec<_> = output.pixels().filter(|p| p[3] > 0 && p[3] < 255).collect();
        assert!(!edges.is_empty());
        assert!(edges.iter().all(|p| p[0] >= 254 && p[1] == 0 && p[2] == 0));
    }
    #[test]
    fn basic_and_fail_closed() {
        let i = RgbaImage::from_pixel(1, 1, Rgba([80, 120, 160, 255]));
        let s = Settings {
            exposure: 1.,
            temperature: 20.,
            highlights: -30.,
            vibrance: 25.,
            ..Default::default()
        };
        assert_ne!(apply(&i, &s).unwrap(), i);
        let mut u = s;
        u.exposure = f32::NAN;
        assert!(apply(&i, &u).is_err())
    }
    #[test]
    fn calibration_process_is_enabled() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([180, 60, 40, 255]));
        let mut settings = Settings::default();
        settings.calibration.red_hue = 50.;
        settings.calibration.red_saturation = 25.;
        assert_ne!(apply(&image, &settings).unwrap(), image);
    }
    #[test]
    fn curve_stage_is_enabled() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([96, 128, 160, 255]));
        let mut settings = Settings::default();
        settings.curve.rgb = vec![
            CurvePoint { x: 0., y: 0. },
            CurvePoint { x: 0.5, y: 0.7 },
            CurvePoint { x: 1., y: 1. },
        ];
        assert_ne!(apply(&image, &settings).unwrap(), image);
    }
    #[test]
    fn mixer_stage_is_enabled() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([220, 40, 30, 255]));
        let mut settings = Settings::default();
        settings.mixer.hue[0] = 80.;
        assert_ne!(apply(&image, &settings).unwrap(), image);
    }
    #[test]
    fn grading_stage_is_enabled() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        let mut settings = Settings::default();
        settings.grading.global = GradeWheel {
            hue: 210.,
            saturation: 50.,
            luminance: 10.,
        };
        assert_ne!(apply(&image, &settings).unwrap(), image);
    }
    #[test]
    fn color_pipeline_matches_preserved_c_fixture() {
        // Generated by compiling AdjustPixels.c and invoking
        // adjust_camera_raw_curve_color with the same identity LUTs/settings.
        let image = RgbaImage::from_pixel(1, 1, Rgba([220, 40, 30, 255]));
        let mut settings = Settings::default();
        settings.mixer.hue[0] = 80.;
        settings.grading.global = GradeWheel {
            hue: 210.,
            saturation: 50.,
            luminance: 10.,
        };
        assert_eq!(
            apply(&image, &settings).unwrap().get_pixel(0, 0).0,
            [175, 102, 89, 255]
        );
    }
    #[test]
    fn color_schema_validation_is_strict() {
        let mut settings = Settings::default();
        settings.mixer.hue.pop();
        assert!(validate(&settings).is_err());
        let mut settings = Settings::default();
        settings.curve.rgb[1].x = 0.9;
        assert!(validate(&settings).is_err());
        let mut settings = Settings::default();
        settings.grading.global.hue = 361.;
        assert!(validate(&settings).is_err());
    }
    #[test]
    fn remaining_portable_stages_are_enabled() {
        let mut image = RgbaImage::new(9, 9);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 25) as u8, (y * 25) as u8, ((x + y) * 12) as u8, 255]);
        }
        let mut effects = Settings::default();
        effects.texture = 50.;
        effects.dehaze = 20.;
        assert_ne!(apply(&image, &effects).unwrap(), image);
        let mut detail = Settings::default();
        detail.detail.sharpen_amount = 100.;
        assert_ne!(apply(&image, &detail).unwrap(), image);
        let mut optics = Settings::default();
        optics.optics.distortion = 20.;
        assert_ne!(apply(&image, &optics).unwrap(), image);
        let mut geometry = Settings::default();
        geometry.geometry.rotate = 10.;
        assert_ne!(apply(&image, &geometry).unwrap(), image);
    }
    #[test]
    fn exact_detail_and_optics_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.detail.sharpen_radius, 10.);
        assert_eq!(settings.detail.sharpen_detail, 25.);
        assert_eq!(settings.detail.noise_color_smoothness, 50.);
        assert_eq!(settings.optics.profile_distortion, 100.);
        assert_eq!(settings.optics.purple_hue_low, 270.);
        assert_eq!(settings.optics.green_hue_high, 120.);
    }
    #[test]
    fn strict() {
        assert!(serde_json::from_value::<Settings>(serde_json::json!({"future":1})).is_err())
    }
}
