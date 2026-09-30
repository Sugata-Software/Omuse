//! Create a synthetic Studio visual QA project and a matching PNG preview.
//!
//! Usage:
//!
//! ```text
//! cargo run --release --example studio_preview -- /tmp/omuse-studio-preview
//! ```
//!
//! The output directory must be new. The example creates `chromatic-form-01.omuse`
//! and `chromatic-form-01.png`, reopens the project, and verifies its pixels before
//! checking the exported PNG. It uses only procedural pixels and system fonts.

use anyhow::{Context, Result, bail, ensure};
use image::{Rgba, RgbaImage};
use omuse::{
    document,
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint, ObjectSize, TextAlignment},
    raster,
};
use serde_json::json;
use std::{
    env,
    f32::consts::TAU,
    fs,
    path::{Path, PathBuf},
};

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 900;

fn over(destination: &mut Rgba<u8>, source: [u8; 4]) {
    let source_alpha = f32::from(source[3]) / 255.;
    if source_alpha <= 0. {
        return;
    }
    let destination_alpha = f32::from(destination[3]) / 255.;
    let alpha = source_alpha + destination_alpha * (1. - source_alpha);
    if alpha <= 0. {
        return;
    }
    for channel in 0..3 {
        destination[channel] = ((f32::from(source[channel]) * source_alpha
            + f32::from(destination[channel]) * destination_alpha * (1. - source_alpha))
            / alpha)
            .round() as u8;
    }
    destination[3] = (alpha * 255.).round() as u8;
}

fn distance_to_segment(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    let (vx, vy) = (end.0 - start.0, end.1 - start.1);
    let length = vx * vx + vy * vy;
    let t = if length <= f32::EPSILON {
        0.
    } else {
        ((point.0 - start.0) * vx + (point.1 - start.1) * vy) / length
    }
    .clamp(0., 1.);
    (point.0 - start.0 - t * vx).hypot(point.1 - start.1 - t * vy)
}

fn segment(image: &mut RgbaImage, start: (f32, f32), end: (f32, f32), color: [u8; 4], radius: f32) {
    let left = (start.0.min(end.0) - radius - 2.).floor().max(0.) as u32;
    let top = (start.1.min(end.1) - radius - 2.).floor().max(0.) as u32;
    let right = (start.0.max(end.0) + radius + 2.)
        .ceil()
        .min(image.width() as f32) as u32;
    let bottom = (start.1.max(end.1) + radius + 2.)
        .ceil()
        .min(image.height() as f32) as u32;
    for y in top..bottom {
        for x in left..right {
            let distance = distance_to_segment((x as f32 + 0.5, y as f32 + 0.5), start, end);
            let coverage = (radius + 1. - distance).clamp(0., 1.);
            if coverage > 0. {
                let mut source = color;
                source[3] = (f32::from(color[3]) * coverage).round() as u8;
                over(image.get_pixel_mut(x, y), source);
            }
        }
    }
}

fn path(image: &mut RgbaImage, points: &[(f32, f32)], color: [u8; 4], radius: f32) {
    for pair in points.windows(2) {
        segment(image, pair[0], pair[1], color, radius);
    }
}

fn background() -> Layer {
    let mut layer = Layer::paint("Background // Graphite Navy", WIDTH, HEIGHT);
    let pixels = layer.image.as_mut().expect("paint layer has pixels");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let nx = x as f32 / WIDTH as f32;
            let ny = y as f32 / HEIGHT as f32;
            let glow = (-(((nx - 0.72) / 0.43).powi(2) + ((ny - 0.48) / 0.62).powi(2)) * 1.8).exp();
            let grain = ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) & 7) as f32;
            pixels.put_pixel(
                x,
                y,
                Rgba([
                    (7. + 12. * glow + grain * 0.18) as u8,
                    (12. + 24. * glow + grain * 0.22) as u8,
                    (25. + 38. * glow + grain * 0.35) as u8,
                    255,
                ]),
            );
        }
    }
    // A quiet registration grid gives the dark field a measured editorial edge.
    for x in (32..WIDTH).step_by(48) {
        for y in 0..HEIGHT {
            let pixel = pixels.get_pixel_mut(x, y);
            over(pixel, [45, 82, 112, 10]);
        }
    }
    for y in (32..HEIGHT).step_by(48) {
        for x in 0..WIDTH {
            let pixel = pixels.get_pixel_mut(x, y);
            over(pixel, [45, 82, 112, 8]);
        }
    }
    layer.metadata["studioPreview"] = json!({"role": "background", "palette": "graphite-navy"});
    layer
}

fn procedural_artwork() -> Layer {
    let mut layer = Layer::paint("Procedural Artwork // Contour Orbit Study", WIDTH, HEIGHT);
    let pixels = layer.image.as_mut().expect("paint layer has pixels");

    // Long contour ribbons carry the eye from the type block into the centre-right field.
    for band in 0..13 {
        let phase = band as f32 * 0.34;
        let mut points = Vec::with_capacity(190);
        for step in 0..190 {
            let t = step as f32 / 189.;
            let x = 320. + t * 820.;
            let y = 430.
                + (t * TAU * 1.25 + phase).sin() * 94.
                + (t * TAU * 2.9 + phase * 1.7).sin() * 25.
                + (band as f32 - 6.) * 12.;
            points.push((x, y));
        }
        let cool = (band as f32 / 12.).clamp(0., 1.);
        let color = [
            (52. + 20. * cool) as u8,
            (196. + 44. * cool) as u8,
            (232. + 22. * cool) as u8,
            138,
        ];
        path(pixels, &points, [35, 164, 221, 22], 11.);
        path(
            pixels,
            &points,
            color,
            if band % 4 == 0 { 2.1 } else { 1.25 },
        );
    }

    // Tilted, slightly perturbed orbital loops avoid a generic geometric-circle look.
    for orbit in 0..5 {
        let mut points = Vec::with_capacity(150);
        let phase = orbit as f32 * 0.58;
        for step in 0..150 {
            let angle = step as f32 / 149. * TAU;
            let wobble = 1. + 0.055 * (angle * 3. + phase).sin();
            let rx = (270. + orbit as f32 * 28.) * wobble;
            let ry = (132. + orbit as f32 * 23.) * wobble;
            let x = 855. + rx * angle.cos() * 0.98 - ry * angle.sin() * 0.17;
            let y = 452. + rx * angle.cos() * 0.17 + ry * angle.sin() * 0.98;
            points.push((x, y));
        }
        path(pixels, &points, [44, 185, 214, 20], 8.);
        path(
            pixels,
            &points,
            if orbit % 2 == 0 {
                [103, 232, 224, 130]
            } else {
                [71, 137, 244, 112]
            },
            1.25,
        );
    }

    // Small instrument marks make the image read as a study rather than a wallpaper.
    for mark in 0..7 {
        let x = 690. + mark as f32 * 73.;
        let y = 174. + (mark as f32 * 0.9).sin() * 18.;
        segment(pixels, (x, y), (x + 30., y), [129, 216, 231, 125], 1.);
        segment(pixels, (x, y), (x, y + 7.), [129, 216, 231, 75], 1.);
    }
    layer.metadata["studioPreview"] =
        json!({"role": "procedural-artwork", "seed": 20260927, "transparent": true});
    layer
}

fn live_text_layer(name: &str, style: LiveTextStyle, offset: (f32, f32)) -> Result<Layer> {
    objects::live_text_layer(
        name,
        ObjectPoint {
            x: offset.0,
            y: offset.1,
        },
        style,
    )
}

fn headline() -> Result<Layer> {
    let width = 590;
    let height = 170;
    live_text_layer(
        "Live Headline // FORM / 01",
        LiveTextStyle {
            runs: vec![],
            content: "FORM / 01".into(),
            font_name: "sans-serif".into(),
            font_size: 92.,
            red: 239. / 255.,
            green: 245. / 255.,
            blue: 252. / 255.,
            alignment: TextAlignment::Left,
            tracking: 2.5,
            leading: 102.,
            box_size: Some(ObjectSize {
                width: width as f32,
                height: height as f32,
            }),
        },
        (58., 72.),
    )
}

fn caption() -> Result<Layer> {
    let width = 560;
    let height = 100;
    live_text_layer(
        "Live Caption // Chromatic Field Study",
        LiveTextStyle {
            runs: vec![],
            content: "CHROMATIC FIELD STUDY\nPERTH / 2026 — GENERATIVE EDITION".into(),
            font_name: "sans-serif".into(),
            font_size: 17.,
            red: 139. / 255.,
            green: 177. / 255.,
            blue: 201. / 255.,
            alignment: TextAlignment::Left,
            tracking: 2.1,
            leading: 27.,
            box_size: Some(ObjectSize {
                width: width as f32,
                height: height as f32,
            }),
        },
        (62., 250.),
    )
}

fn make_document() -> Result<Document> {
    let mut document = Document::new(WIDTH, HEIGHT);
    document.name = "Chromatic Form 01 // Studio Preview".into();
    document.background = [0, 0, 0, 0];
    document.metadata["studioPreview"] = json!({
        "title": "FORM / 01",
        "edition": "Chromatic Field Study",
        "fixture": "procedural-cpu",
        "created": "2026-09-27",
    });
    document.layers.clear();
    document.layers.push(background());
    document.layers.push(procedural_artwork());
    document.layers.push(headline()?);
    document.layers.push(caption()?);
    Ok(document)
}

fn verify_project(path: &Path, expected: &Document) -> Result<Document> {
    let reopened = document::open(path).with_context(|| format!("reopen {}", path.display()))?;
    ensure!(
        reopened.width == WIDTH && reopened.height == HEIGHT,
        "reopened dimensions differ"
    );
    ensure!(
        reopened.layers.len() == 4,
        "expected four preview layers after reopen"
    );
    ensure!(
        reopened.layers.iter().map(|layer| layer.name.as_str()).eq([
            "Background // Graphite Navy",
            "Procedural Artwork // Contour Orbit Study",
            "Live Headline // FORM / 01",
            "Live Caption // Chromatic Field Study",
        ]),
        "reopened layer names differ from the Studio fixture"
    );
    ensure!(
        reopened.layers[2].metadata.get("text").is_some()
            && reopened.layers[3].metadata.get("text").is_some(),
        "live text metadata was not retained"
    );
    let before = raster::composite(expected);
    let after = raster::composite(&reopened);
    ensure!(
        before == after,
        "reopened composite pixels differ from the fixture"
    );
    Ok(reopened)
}

fn run(root: &Path) -> Result<()> {
    if root.exists() {
        bail!(
            "output directory already exists; choose a fresh path: {}",
            root.display()
        );
    }
    fs::create_dir(root)
        .with_context(|| format!("create fresh output directory {}", root.display()))?;
    let project = root.join("chromatic-form-01.omuse");
    let preview = root.join("chromatic-form-01.png");
    let document = make_document()?;
    document::save(&document, &project).context("save Studio preview project")?;
    let reopened = verify_project(&project, &document)?;
    raster::export(&reopened, &preview).context("export Studio preview PNG")?;
    let exported = image::open(&preview)
        .with_context(|| format!("open exported PNG {}", preview.display()))?
        .into_rgba8();
    ensure!(
        exported.dimensions() == (WIDTH, HEIGHT),
        "exported PNG dimensions differ"
    );
    ensure!(
        exported.pixels().any(|pixel| pixel[3] > 0),
        "exported PNG is completely transparent"
    );
    ensure!(
        exported.get_pixel(80, 110).0 != exported.get_pixel(900, 450).0,
        "exported PNG lacks expected field contrast"
    );
    println!(
        "created {} and {} ({}x{}, {} layers)",
        project.display(),
        preview.display(),
        WIDTH,
        HEIGHT,
        reopened.layers.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(root) = args.next() else {
        bail!("usage: cargo run --release --example studio_preview -- OUTPUT_DIR");
    };
    if args.next().is_some() {
        bail!("usage: cargo run --release --example studio_preview -- OUTPUT_DIR");
    }
    run(&PathBuf::from(root))
}
