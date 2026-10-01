//! Deterministic synthetic tracing acceptance artwork. Run with a new output
//! directory. No photographs, provider accounts or existing projects are used.
//!
//! Fixtures are sampled from analytic shapes independently of the vector
//! renderer. Pixels/trace geometry are reproducible; project identifiers and
//! single-run timings may vary. These cases do not establish photo quality,
//! peak memory, interactive latency or performance against another editor.

use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::Editor,
    image_trace::{self, TraceMode, TraceOptions, TraceResult},
    model::Document,
    raster,
    vector_scene::VectorScene,
};
use serde_json::{Value, json};
use std::{env, fs, path::Path, sync::atomic::AtomicBool, time::Instant};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;
const NOISE: [(u32, u32); 3] = [(16, 18), (18, 18), (230, 169)];

struct Fixture {
    name: &'static str,
    source: RgbaImage,
    options: TraceOptions,
    alpha_silhouette: bool,
    minimum_iou: f64,
    maximum_matted_error: f64,
    probes: Vec<(u32, u32, [u8; 4], u8)>,
}

fn options(mode: TraceMode) -> TraceOptions {
    TraceOptions {
        mode,
        colors: 8,
        monochrome_threshold: 128,
        detail: 0.85,
        smoothing: 0.6,
        corner_preservation: 0.5,
        speckle_area: 4,
        omit_white: true,
        omit_background: false,
        max_dimension: WIDTH,
        max_points: 30_000,
    }
}

fn ellipse(x: f64, y: f64, cx: f64, cy: f64, rx: f64, ry: f64) -> bool {
    ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.
}

// Average in premultiplied space, then return straight RGBA. The generator
// never uses the production contour fitter or vector rasterizer.
fn sampled(samples: u32, pixel: impl Fn(f64, f64) -> [u8; 4]) -> RgbaImage {
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let mut alpha = 0u32;
        let mut colour = [0u32; 3];
        for sy in 0..samples {
            for sx in 0..samples {
                let p = pixel(
                    x as f64 + (sx as f64 + 0.5) / samples as f64,
                    y as f64 + (sy as f64 + 0.5) / samples as f64,
                );
                alpha += u32::from(p[3]);
                for c in 0..3 {
                    colour[c] += u32::from(p[c]) * u32::from(p[3]);
                }
            }
        }
        let n = samples * samples;
        let mut output = [0; 4];
        output[3] = ((alpha + n / 2) / n) as u8;
        if alpha != 0 {
            for c in 0..3 {
                output[c] = ((colour[c] + alpha / 2) / alpha) as u8;
            }
        }
        Rgba(output)
    })
}

fn fixtures() -> Vec<Fixture> {
    let mut logo = sampled(1, |x, y| {
        let dx = (x - 128.) / 82.;
        let dy = (y - 96.) / 64.;
        let outer = dx.hypot(dy) <= 1. + 0.045 * (dy.atan2(dx) * 7.).sin();
        let hole = ellipse(x, y, 128., 96., 30., 24.);
        if outer && !hole {
            [0, 0, 0, 255]
        } else {
            [255; 4]
        }
    });
    for (x, y) in NOISE {
        logo.put_pixel(x, y, Rgba([0, 0, 0, 255]));
    }
    let flat = sampled(1, |x, y| {
        let mut p = [255; 4];
        if ellipse(x, y, 88., 85., 61., 55.) {
            p = [36, 111, 174, 255];
        }
        if ellipse(x, y, 159., 94., 65., 48.) {
            p = [220, 97, 61, 255];
        }
        let corner_x = x - x.clamp(77., 171.);
        let corner_y = y - y.clamp(140., 155.);
        if corner_x * corner_x + corner_y * corner_y <= 100. {
            p = [36, 149, 122, 255];
        }
        p
    });
    let mut transparent = sampled(1, |x, y| {
        let dx = (x - 105.) / 67.;
        let dy = (y - 100.) / 54.;
        let edge = 1. + 0.14 * (dy.atan2(dx) * 3.).sin();
        if dx.hypot(dy) <= edge {
            [68, 127, 158, 160]
        } else if ellipse(x, y, 202., 47., 18., 22.) {
            [224, 160, 64, 232]
        } else {
            [0; 4]
        }
    });
    // Hidden RGB must not turn into traceable shapes or tint visible colours.
    for (x, y, p) in transparent.enumerate_pixels_mut() {
        if p[3] == 0 {
            *p = Rgba([(x * 7 % 256) as u8, 231, (y * 11 % 256) as u8, 0]);
        }
    }
    let antialiased = sampled(4, |x, y| {
        let wave = 88. + 30. * ((x - 24.) / 208. * std::f64::consts::TAU).sin();
        let swash = (24. ..=232.).contains(&x) && (y - wave).abs() <= 6.;
        let ring = ellipse(x, y, 176., 144., 33., 23.) && !ellipse(x, y, 176., 144., 24., 14.);
        if swash || ring {
            [22, 22, 22, 255]
        } else {
            [255; 4]
        }
    });
    let mut mono_options = options(TraceMode::Monochrome);
    mono_options.speckle_area = 9;
    let mut transparent_options = options(TraceMode::Color);
    transparent_options.omit_white = false;
    let mut gray_options = options(TraceMode::Grayscale);
    gray_options.speckle_area = 0;
    gray_options.detail = 0.55;
    vec![
        Fixture {
            name: "curved-logo-hole-speckles",
            source: logo,
            options: mono_options,
            alpha_silhouette: false,
            minimum_iou: 0.92,
            maximum_matted_error: 8.,
            probes: vec![
                (128, 96, [0; 4], 0),
                (68, 96, [0, 0, 0, 255], 24),
                (16, 18, [0; 4], 0),
                (18, 18, [0; 4], 0),
                (230, 169, [0; 4], 0),
            ],
        },
        Fixture {
            name: "overlapping-flat-colours",
            source: flat,
            options: options(TraceMode::Color),
            alpha_silhouette: false,
            minimum_iou: 0.92,
            maximum_matted_error: 9.,
            probes: vec![
                (54, 82, [36, 111, 174, 255], 24),
                (135, 77, [220, 97, 61, 255], 24),
                (150, 147, [36, 149, 122, 255], 24),
                (4, 4, [0; 4], 0),
            ],
        },
        Fixture {
            name: "transparent-curves-hidden-rgb",
            source: transparent,
            options: transparent_options,
            alpha_silhouette: true,
            minimum_iou: 0.92,
            maximum_matted_error: 8.,
            probes: vec![
                (105, 100, [68, 127, 158, 160], 24),
                (202, 47, [224, 160, 64, 232], 24),
                (4, 4, [0; 4], 0),
            ],
        },
        Fixture {
            name: "antialiased-grayscale-drawing",
            source: antialiased,
            options: gray_options,
            alpha_silhouette: false,
            minimum_iou: 0.87,
            maximum_matted_error: 8.,
            probes: vec![(76, 118, [22, 22, 22, 255], 32), (176, 144, [0; 4], 0)],
        },
    ]
}

fn on_white(pixel: [u8; 4]) -> [u8; 3] {
    std::array::from_fn(|c| {
        ((u32::from(pixel[c]) * u32::from(pixel[3]) + 255 * u32::from(255 - pixel[3]) + 127) / 255)
            as u8
    })
}

fn foreground(pixel: [u8; 4], alpha: bool) -> bool {
    if alpha {
        pixel[3] >= 96
    } else {
        // A fixed white-matte luminance cut, independent of trace labels.
        let rgb = on_white(pixel);
        (u32::from(rgb[0]) * 2126 + u32::from(rgb[1]) * 7152 + u32::from(rgb[2]) * 722)
            < 190 * 10_000
    }
}

fn compare(fixture: &Fixture, rendered: &RgbaImage, directory: &Path) -> Result<Value> {
    ensure!(
        rendered.dimensions() == fixture.source.dimensions(),
        "Trace dimensions changed"
    );
    let mut intersection = 0u64;
    let mut union = 0u64;
    let mut rgb_error = 0u64;
    let mut alpha_error = 0u64;
    let difference = RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let original = fixture.source.get_pixel(x, y).0;
        let actual = rendered.get_pixel(x, y).0;
        let expected_mask = foreground(original, fixture.alpha_silhouette);
        let actual_mask = foreground(actual, fixture.alpha_silhouette);
        intersection += u64::from(expected_mask && actual_mask);
        union += u64::from(expected_mask || actual_mask);
        let a = on_white(original);
        let b = on_white(actual);
        rgb_error += (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
        alpha_error += u64::from(original[3].abs_diff(actual[3]));
        Rgba(match (expected_mask, actual_mask) {
            (true, false) => [210, 57, 57, 255],
            (false, true) => [50, 120, 230, 255],
            (true, true) => [45, 50, 55, 255],
            (false, false) => [245, 245, 245, 255],
        })
    });
    difference.save(directory.join("silhouette-difference.png"))?;
    ensure!(union > 0, "The silhouette check had no foreground");
    let iou = intersection as f64 / union as f64;
    let mean_rgb_error = rgb_error as f64 / (f64::from(WIDTH) * f64::from(HEIGHT) * 3.);
    let mean_alpha_error = alpha_error as f64 / (f64::from(WIDTH) * f64::from(HEIGHT));
    ensure!(
        iou >= fixture.minimum_iou,
        "{} silhouette IoU {iou:.4} is below {}",
        fixture.name,
        fixture.minimum_iou
    );
    ensure!(
        mean_rgb_error <= fixture.maximum_matted_error,
        "{} white-matte RGB mean error {mean_rgb_error:.3} exceeds {}",
        fixture.name,
        fixture.maximum_matted_error
    );
    if fixture.alpha_silhouette {
        ensure!(
            mean_alpha_error <= 8.,
            "{} alpha mean error {mean_alpha_error:.3} exceeds 8",
            fixture.name
        );
    }
    for &(x, y, expected, tolerance) in &fixture.probes {
        let actual = rendered.get_pixel(x, y).0;
        if expected[3] == 0 {
            ensure!(
                actual[3] == 0,
                "{} expected clear pixel at ({x},{y}), got {actual:?}",
                fixture.name
            );
        } else {
            ensure!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= tolerance),
                "{} interior colour/alpha at ({x},{y}): {actual:?}, expected {expected:?} ±{tolerance}",
                fixture.name
            );
        }
    }
    Ok(json!({
        "silhouetteIou": iou,
        "minimumIou": fixture.minimum_iou,
        "whiteMatteMeanAbsoluteRgbError": mean_rgb_error,
        "maximumWhiteMatteError": fixture.maximum_matted_error,
        "meanAbsoluteAlphaError": mean_alpha_error,
        "maximumAlphaError": if fixture.alpha_silhouette { Some(8.) } else { None },
        "alphaErrorIsInformational": !fixture.alpha_silhouette,
        "interiorAndHoleProbes": fixture.probes.len(),
        "differenceLegend": "red: missing foreground; blue: extra foreground; dark: agreement",
    }))
}

fn geometry(scene: &VectorScene) -> (usize, usize, usize) {
    let mut subpaths = 0;
    let mut anchors = 0;
    let mut curved_segments = 0;
    for object in &scene.objects {
        for subpath in &object.path.subpaths {
            subpaths += 1;
            anchors += subpath.anchors.len();
            let n = subpath.anchors.len();
            let segments = if subpath.closed {
                n
            } else {
                n.saturating_sub(1)
            };
            for i in 0..segments {
                let a = &subpath.anchors[i];
                let b = &subpath.anchors[(i + 1) % n];
                let dx = b.position.x - a.position.x;
                let dy = b.position.y - a.position.y;
                let length = dx.hypot(dy).max(1.);
                // Handles on the chord are still straight. Require actual
                // curvature, not merely populated Option fields.
                let curved = [a.outgoing, b.incoming].into_iter().flatten().any(|p| {
                    ((p.x - a.position.x) * dy - (p.y - a.position.y) * dx).abs() / length > 0.01
                });
                curved_segments += usize::from(curved);
            }
        }
    }
    (subpaths, anchors, curved_segments)
}

fn option_json(options: &TraceOptions) -> Value {
    json!({
        "mode": match options.mode {
            TraceMode::Color => "color",
            TraceMode::Grayscale => "grayscale",
            TraceMode::Monochrome => "monochrome",
        },
        "colors": options.colors,
        "monochromeThreshold": options.monochrome_threshold,
        "detail": options.detail,
        "smoothing": options.smoothing,
        "cornerPreservation": options.corner_preservation,
        "speckleArea": options.speckle_area,
        "omitWhite": options.omit_white,
        "omitBackground": options.omit_background,
        "maxDimension": options.max_dimension,
        "maxPoints": options.max_points,
    })
}

fn save_project(
    source: &RgbaImage,
    traced: &TraceResult,
    rendered: &RgbaImage,
    directory: &Path,
) -> Result<()> {
    let mut document = Document::new(source.width(), source.height());
    document.name = "Synthetic tracing acceptance".into();
    let original = &mut document.layers[0];
    original.name = "Original source (hidden; show to compare)".into();
    original.visible = false;
    original.locked = true;
    original.image = Some(source.clone().into());
    let source_id = original.id.clone();
    let mut editor = Editor::new(document);
    let vector_id = editor.insert_vector_scene(
        "Editable traced artwork",
        editor.revision(),
        traced.scene.clone(),
        rendered.clone(),
    )?;
    ensure!(
        editor.undo_depth() == 1,
        "Tracing insertion was not one document Undo step"
    );
    ensure!(editor.undo(), "Traced artwork Undo failed");
    ensure!(
        editor.document.find_layer(&vector_id).is_none(),
        "Undo retained traced artwork"
    );
    ensure!(
        editor
            .document
            .find_layer(&source_id)
            .and_then(|l| l.image.as_deref())
            == Some(source),
        "Undo changed the retained original source"
    );
    ensure!(editor.redo(), "Traced artwork Redo failed");
    let project = directory.join("editable-trace.omuse");
    document::save(&editor.document, &project)?;
    let reopened = document::open(&project)?;
    let original = reopened
        .find_layer(&source_id)
        .context("Saved source layer is missing")?;
    ensure!(
        !original.visible && original.locked && original.image.as_deref() == Some(source),
        "Save/reopen changed source pixels or comparison-layer state"
    );
    let artwork = reopened
        .find_layer(&vector_id)
        .context("Saved vector layer is missing")?;
    ensure!(
        artwork.vector_scene.as_deref() == Some(&traced.scene),
        "Saved vector geometry changed"
    );
    ensure!(
        artwork.image.as_deref() == Some(rendered),
        "Saved vector cache changed"
    );
    ensure!(
        raster::composite(&reopened) == *rendered,
        "Reopened document composite differs from trace"
    );
    Ok(())
}

fn main() -> Result<()> {
    let directory = env::args_os()
        .nth(1)
        .context("Pass a new output directory")?;
    let directory = Path::new(&directory);
    fs::create_dir(directory).context("The output directory must not already exist")?;
    let cancel = AtomicBool::new(false);
    let fixtures = fixtures();
    let mut results = Vec::new();
    for fixture in &fixtures {
        let folder = directory.join(fixture.name);
        fs::create_dir(&folder)?;
        fixture.source.save(folder.join("source.png"))?;
        let start = Instant::now();
        let traced = image_trace::trace(&fixture.source, &fixture.options, &cancel)
            .with_context(|| format!("Trace {}", fixture.name))?;
        let trace_ms = start.elapsed().as_secs_f64() * 1_000.;
        traced.scene.validate()?;
        let (subpaths, anchors, curves) = geometry(&traced.scene);
        ensure!(
            !traced.scene.objects.is_empty() && curves > 0,
            "{} produced no curved artwork",
            fixture.name
        );
        ensure!(
            anchors <= fixture.options.max_points,
            "Trace exceeded its point budget"
        );
        ensure!(
            traced.stats.objects == traced.scene.objects.len()
                && traced.stats.subpaths == subpaths
                && traced.stats.anchors == anchors,
            "Reported trace counts differ from independently counted geometry"
        );
        let start = Instant::now();
        let rendered = traced.scene.render(&cancel)?;
        let render_ms = start.elapsed().as_secs_f64() * 1_000.;
        rendered.save(folder.join("trace.png"))?;
        let white = RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
            let rgb = on_white(rendered.get_pixel(x, y).0);
            Rgba([rgb[0], rgb[1], rgb[2], 255])
        });
        white.save(folder.join("trace-on-white.png"))?;
        let metrics = compare(fixture, &rendered, &folder)?;
        let repeat = image_trace::trace(&fixture.source, &fixture.options, &cancel)?;
        ensure!(
            repeat.scene == traced.scene,
            "Repeated trace geometry was not deterministic"
        );
        if fixture.alpha_silhouette {
            let mut no_hidden_rgb = fixture.source.clone();
            for p in no_hidden_rgb.pixels_mut().filter(|p| p[3] == 0) {
                *p = Rgba([0; 4]);
            }
            let hidden_check = image_trace::trace(&no_hidden_rgb, &fixture.options, &cancel)?;
            ensure!(
                hidden_check.scene.render(&cancel)? == rendered,
                "Hidden transparent RGB changed traced pixels"
            );
        }
        save_project(&fixture.source, &traced, &rendered, &folder)?;
        results.push(json!({
            "fixture": fixture.name,
            "options": option_json(&fixture.options),
            "objects": traced.scene.objects.len(), "subpaths": subpaths,
            "anchors": anchors, "nonlinearCubicSegments": curves,
            "pointsBefore": traced.stats.points_before, "pointsAfter": traced.stats.points_after,
            "workingWidth": traced.stats.working_width, "workingHeight": traced.stats.working_height,
            "singleRunTraceMs": trace_ms, "singleRunRenderMs": render_ms,
            "retainedSceneBytes": traced.scene.retained_bytes(),
            "metrics": metrics,
            "checks": ["repeated geometry exact", "one Undo and Redo", "source bytes retained",
                "save/reopen preserves geometry and derived pixels", "reopened composite equals trace"],
        }));
    }

    let logo = &fixtures[0];
    let mut baseline_options = options(TraceMode::Monochrome);
    baseline_options.detail = 0.;
    baseline_options.smoothing = 0.;
    baseline_options.speckle_area = logo.options.speckle_area;
    let baseline = image_trace::trace(&logo.source, &baseline_options, &cancel)?;
    let simplified = image_trace::trace(&logo.source, &logo.options, &cancel)?;
    let (_, before, baseline_curves) = geometry(&baseline.scene);
    let (_, after, curves) = geometry(&simplified.scene);
    baseline
        .scene
        .render(&cancel)?
        .save(directory.join("logo-linear-contour.png"))?;
    ensure!(
        baseline_curves == 0,
        "Zero smoothing generated nonlinear segments"
    );
    ensure!(
        curves >= 4,
        "Simplified logo has too few genuinely curved segments"
    );
    ensure!(
        after * 2 <= before,
        "Strong simplification did not halve logo anchors: {before} → {after}"
    );

    let mut bounded_options = options(TraceMode::Monochrome);
    bounded_options.max_dimension = 128;
    let bounded = image_trace::trace(&logo.source, &bounded_options, &cancel)?;
    ensure!(
        bounded.scene.width <= 128
            && bounded.scene.height <= 128
            && bounded.stats.working_width == bounded.scene.width
            && bounded.stats.working_height == bounded.scene.height,
        "Trace ignored its working resolution bound"
    );
    bounded
        .scene
        .render(&cancel)?
        .save(directory.join("logo-bounded-resolution.png"))?;
    ensure!(
        image_trace::trace(&logo.source, &logo.options, &AtomicBool::new(true)).is_err(),
        "Pre-cancelled tracing unexpectedly succeeded"
    );
    let mut limited_options = baseline_options.clone();
    limited_options.max_points = 4;
    ensure!(
        image_trace::trace(&logo.source, &limited_options, &cancel).is_err(),
        "Curved logo tracing unexpectedly bypassed a four-anchor budget"
    );

    let report = json!({
        "scope": "four synthetic fixtures, analytic source pixels; no general photographic-quality or release claim",
        "fixtureDimensions": [WIDTH, HEIGHT],
        "fixtures": results,
        "simplification": {
            "linearOptions": option_json(&baseline_options), "strongOptions": option_json(&logo.options),
            "linearAnchors": before, "strongAnchors": after,
            "anchorReductionFraction": 1. - after as f64 / before as f64,
            "minimumReductionFraction": 0.5, "strongNonlinearCubicSegments": curves,
        },
        "boundedResolution": [bounded.scene.width, bounded.scene.height],
        "preCancelledTraceRejected": true,
        "insufficientPointBudgetRejected": true,
        "limitations": [
            "synthetic logos, flat colours and drawings do not qualify arbitrary photographs or complex artwork",
            "the silhouette and white-matte error limits apply only to these named fixtures",
            "timings are individual observations, not latency percentiles or cross-editor benchmarks",
            "retainedSceneBytes excludes derived caches, source images, history, transient allocations and process memory",
            "RGBA8 inputs and vector display caches do not establish high-precision tracing",
            "source and trace images are deterministic; project identifiers and timings may vary"
        ],
    });
    fs::write(
        directory.join("image-trace-measurements.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
