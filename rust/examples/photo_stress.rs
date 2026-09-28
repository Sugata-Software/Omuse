//! Bounded, synthetic large-photo stress qualification.
//!
//! This is deliberately a process-level release harness rather than a benchmark
//! test.  `scripts/photo-stress.py` runs one size per process and applies the
//! host safety limits.  The pixels are made by resizing and tiling a supplied
//! photograph, so the result must not be described as native camera input.

use anyhow::{Context, Result, bail, ensure};
use image::{RgbaImage, imageops::FilterType};
use omuse::{
    create_project::Project,
    editor::{Editor, PaintTool},
    model::{Document, Layer, MAX_PIXELS},
    raster::{self, ExportOptions},
};
use serde_json::{Value, json};
use std::{
    fs,
    hint::black_box,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const HISTORY_LIMIT: usize = 256 * 1024 * 1024;
const COMPOSITE_SAMPLES: usize = 5;
const MIN_EDITS: usize = 80;
const MAX_EDITS: usize = 120;

struct Arguments {
    input: PathBuf,
    output: PathBuf,
    width: u32,
    height: u32,
    edits: usize,
}

fn main() -> Result<()> {
    let args = arguments()?;
    ensure!(
        (MIN_EDITS..=MAX_EDITS).contains(&args.edits),
        "--edits must be between {MIN_EDITS} and {MAX_EDITS}"
    );
    ensure!(
        u64::from(args.width) * u64::from(args.height) * 2 <= MAX_PIXELS,
        "two full-canvas layers exceed the supported live pixel budget"
    );
    ensure!(
        args.width >= 64 && args.height >= 64,
        "stress canvases must be at least 64x64 pixels"
    );
    ensure!(
        args.input.is_file(),
        "input photograph is not a regular file"
    );
    fs::create_dir(&args.output)
        .with_context(|| format!("create fresh evidence directory {}", args.output.display()))?;

    let started = Instant::now();
    checkpoint("start", started, json!({}))?;
    let source_bytes = fs::metadata(&args.input)?.len();
    let (fixture, tile_width, tile_height) = synthetic_fixture(
        image::open(&args.input)
            .with_context(|| format!("decode input photograph {}", args.input.display()))?
            .to_rgba8(),
        args.width,
        args.height,
    )?;
    let fixture_hash = fnv1a64(fixture.as_raw());
    checkpoint(
        "synthetic-fixture-ready",
        started,
        json!({
            "fixtureKind": "synthetic-size-stress-fixture",
            "sourceBytes": source_bytes,
            "tileWidth": tile_width,
            "tileHeight": tile_height,
            "pixelBytes": fixture.as_raw().len(),
            "pixelFnv1a64": &fixture_hash,
        }),
    )?;

    let mut document = Document::new(args.width, args.height);
    document.name = format!("Synthetic {}x{} photo stress", args.width, args.height);
    document.metadata["photoStressFixture"] = json!({
        "kind": "synthetic-size-stress-fixture",
        "method": "supplied photograph resized with preserved aspect ratio, then tiled",
        "source": args.input.to_string_lossy(),
        "tileWidth": tile_width,
        "tileHeight": tile_height,
        "warning": "Synthetic size fixture; not evidence of physical input latency or camera decoding",
    });
    let mut overlay = document.layers.pop().context("new document has no layer")?;
    overlay.name = "Brush edits".into();
    let overlay_id = overlay.id.clone();
    let mut photograph = Layer::group("Synthetic tiled photograph");
    photograph.metadata = json!({
        "photoStressFixture": "synthetic-size-stress-fixture",
        "pixelFormat": "8-bit straight-alpha RGBA",
    });
    photograph.image = Some(fixture.into());
    let photograph_id = photograph.id.clone();
    document.layers = vec![photograph, overlay];
    ensure!(
        raster::validate(&document).is_empty(),
        "synthetic fixture document is invalid"
    );

    let mut editor = Editor::new(document);
    editor.set_history_limit(HISTORY_LIMIT);
    ensure!(editor.select_layer(&overlay_id), "select brush layer");
    checkpoint(
        "two-layer-document-ready",
        started,
        json!({
            "layers": 2,
            "logicalLayerPixels": u64::from(args.width) * u64::from(args.height) * 2,
            "historyLimitBytes": HISTORY_LIMIT,
        }),
    )?;

    let metadata_started = Instant::now();
    let shared_before: Vec<_> = editor
        .document
        .layers
        .iter()
        .map(|layer| layer.image.as_ref().unwrap().clone())
        .collect();
    for edit in 0..10 {
        let id = if edit % 2 == 0 {
            &photograph_id
        } else {
            &overlay_id
        };
        ensure!(
            editor.rename_layer(id, &format!("Stress metadata {edit}")),
            "metadata edit {edit} was rejected"
        );
    }
    let metadata_shared = editor
        .document
        .layers
        .iter()
        .zip(&shared_before)
        .all(|(layer, before)| layer.image.as_ref().unwrap().shares_pixels_with(before));
    ensure!(
        metadata_shared,
        "metadata-only edits copied a raster allocation"
    );
    drop(shared_before);
    checkpoint(
        "metadata-edits-complete",
        started,
        json!({
            "edits": 10,
            "elapsedMs": milliseconds(metadata_started.elapsed()),
            "pixelAllocationsShared": true,
            "historyBytes": editor.history_bytes(),
        }),
    )?;

    editor.brush.size = 24.0;
    editor.brush.opacity = 1.0;
    editor.brush.hardness = 1.0;
    let brush_started = Instant::now();
    let mut stroke_ms = Vec::with_capacity(args.edits);
    let mut maximum_history_bytes = editor.history_bytes();
    for edit in 0..args.edits {
        const MARGIN: u32 = 16;
        const SEGMENT_X: u32 = 10;
        const SEGMENT_Y: u32 = 6;
        let x_span = args.width - MARGIN * 2 - SEGMENT_X;
        let y_span = args.height - MARGIN * 2 - SEGMENT_Y;
        let x = (MARGIN + ((edit as u64 * 7_919 + 17) % u64::from(x_span)) as u32) as f32 + 0.5;
        let y = (MARGIN + ((edit as u64 * 104_729 + 29) % u64::from(y_span)) as u32) as f32 + 0.5;
        editor.brush.color = [
            31u8.wrapping_add((edit as u8).wrapping_mul(17)),
            79u8.wrapping_add((edit as u8).wrapping_mul(29)),
            151u8.wrapping_add((edit as u8).wrapping_mul(37)),
            255,
        ];
        let stroke_started = Instant::now();
        ensure!(
            editor.begin_stroke(x, y, 1.0, PaintTool::Brush),
            "brush edit {edit} did not start"
        );
        ensure!(
            editor.continue_stroke(x + SEGMENT_X as f32, y + SEGMENT_Y as f32, 1.0),
            "brush edit {edit} segment did not continue"
        );
        ensure!(
            editor.finish_stroke(),
            "brush edit {edit} did not change pixels"
        );
        let this_stroke_ms = milliseconds(stroke_started.elapsed());
        stroke_ms.push(this_stroke_ms);
        maximum_history_bytes = maximum_history_bytes.max(editor.history_bytes());
        ensure!(
            editor.history_bytes() <= HISTORY_LIMIT,
            "history exceeded the configured 256 MiB limit"
        );
        if edit == 0 || (edit + 1) % 20 == 0 || edit + 1 == args.edits {
            let history_stats = editor.history_stats();
            checkpoint(
                "brush-progress",
                started,
                json!({
                    "completedEdits": edit + 1,
                    "historyBytes": editor.history_bytes(),
                    "rasterPatchEntries": history_stats.raster_patch_entries,
                    "rasterPatchTiles": history_stats.raster_patch_tiles,
                    "rasterPatchBytes": history_stats.raster_patch_bytes,
                    "detachedRasterBytes": history_stats.detached_raster_bytes,
                    "undoDepth": editor.undo_depth(),
                    "lastStrokeMs": this_stroke_ms,
                }),
            )?;
        }
    }
    let brush_ms = milliseconds(brush_started.elapsed());
    let mut sorted_stroke_ms = stroke_ms.clone();
    sorted_stroke_ms.sort_by(f64::total_cmp);
    let stroke_median_ms = percentile(&sorted_stroke_ms, 0.50);
    let stroke_p95_ms = percentile(&sorted_stroke_ms, 0.95);
    let stroke_max_ms = *sorted_stroke_ms.last().context("no brush stroke timings")?;

    let mut composite_ms = Vec::with_capacity(COMPOSITE_SAMPLES);
    let mut expected_composite: Option<RgbaImage> = None;
    for sample_index in 0..COMPOSITE_SAMPLES {
        let sample_started = Instant::now();
        let composite = raster::composite(&editor.document);
        let elapsed = milliseconds(sample_started.elapsed());
        ensure!(
            composite.dimensions() == (args.width, args.height),
            "composite sample has wrong dimensions"
        );
        black_box(&composite);
        if let Some(expected) = &expected_composite {
            ensure!(
                composite == *expected,
                "composite sample {sample_index} changed pixels"
            );
        } else {
            expected_composite = Some(composite);
        }
        composite_ms.push(elapsed);
        checkpoint(
            "composite-sample",
            started,
            json!({"sample": sample_index + 1, "elapsedMs": elapsed}),
        )?;
    }
    let expected_composite = expected_composite.context("no composite samples")?;
    let composite_hash = fnv1a64(expected_composite.as_raw());

    let mut undo_count = 0usize;
    while editor.undo() {
        undo_count += 1;
    }
    ensure!(undo_count > 0, "history retained no undo step");
    let mut redo_count = 0usize;
    while editor.redo() {
        redo_count += 1;
    }
    ensure!(
        redo_count == undo_count,
        "redo did not restore every retained step"
    );
    let redone = raster::composite(&editor.document);
    ensure!(
        redone == expected_composite,
        "undo/redo did not restore the exact final composite"
    );
    drop(redone);

    let document_before_rejection = document_fingerprint(&editor.document);
    let history_before_rejection = (
        editor.history_bytes(),
        editor.undo_depth(),
        editor.redo_depth(),
        editor.revision(),
        editor.active_layer.clone(),
    );
    let pixels_per_layer = u64::from(args.width) * u64::from(args.height);
    let current_pixels = pixels_per_layer * 2;
    let clones_needed = ((MAX_PIXELS - current_pixels) / pixels_per_layer + 1) as usize;
    let mut rejected = Layer::group("Must be rejected: over live pixel budget");
    let source = editor
        .document
        .find_layer(&photograph_id)
        .context("photograph layer missing")?
        .clone();
    rejected.children = (0..clones_needed)
        .map(|_| {
            let mut copy = source.clone();
            copy.id = uuid::Uuid::new_v4().to_string().to_uppercase();
            copy
        })
        .collect();
    ensure!(
        current_pixels + pixels_per_layer * clones_needed as u64 > MAX_PIXELS,
        "rejection fixture does not exceed the live pixel budget"
    );
    ensure!(
        editor.import_layer(rejected).is_empty(),
        "over-budget layer tree was accepted"
    );
    ensure!(
        document_fingerprint(&editor.document) == document_before_rejection,
        "over-budget rejection changed the document"
    );
    ensure!(
        (
            editor.history_bytes(),
            editor.undo_depth(),
            editor.redo_depth(),
            editor.revision(),
            editor.active_layer.clone(),
        ) == history_before_rejection,
        "over-budget rejection changed history or editor state"
    );
    checkpoint(
        "over-budget-layer-rejected",
        started,
        json!({
            "attemptedLogicalPixels": current_pixels + pixels_per_layer * clones_needed as u64,
            "documentFingerprint": document_before_rejection,
            "historyBytes": editor.history_bytes(),
        }),
    )?;

    let history_stats = editor.history_stats();
    let editor_history_bytes = editor.history_bytes();

    let package = args.output.join("stress.omuse");
    let exported_png = args.output.join("stress.png");
    let final_document = editor.document.clone();
    let mut project = Project::new("Synthetic large-photo stress", final_document);
    drop(editor);
    let save_started = Instant::now();
    project.save(&package)?;
    let save_ms = milliseconds(save_started.elapsed());
    checkpoint("package-saved", started, json!({"elapsedMs": save_ms}))?;
    ensure!(
        package.join("project.json").is_file(),
        "native Omuse project manifest is missing"
    );
    drop(project);

    let reopen_started = Instant::now();
    let mut reopened_project = Project::open(&package)?;
    let reopened = reopened_project.active_document()?.clone();
    drop(reopened_project);
    let reopened_composite = raster::composite(&reopened);
    let reopen_ms = milliseconds(reopen_started.elapsed());
    ensure!(
        reopened_composite == expected_composite,
        "save/reopen changed composite pixels"
    );
    drop(reopened_composite);

    let export_started = Instant::now();
    raster::export_with_options(&reopened, &exported_png, ExportOptions::default())?;
    let export_ms = milliseconds(export_started.elapsed());
    let exported = image::open(&exported_png)?.to_rgba8();
    ensure!(
        exported == expected_composite,
        "PNG export did not decode to the exact composite pixels"
    );
    let png_bytes = fs::metadata(&exported_png)?.len();
    let png_file_hash = fnv1a64(&fs::read(&exported_png)?);
    let package_bytes = tree_bytes(&package)?;
    checkpoint(
        "reopen-and-export-complete",
        started,
        json!({
            "reopenAndCompositeMs": reopen_ms,
            "pngExportMs": export_ms,
            "packageTreeBytes": package_bytes,
            "pngBytes": png_bytes,
        }),
    )?;

    let final_rss = rss();
    emit(&json!({
        "event": "result",
        "status": "passed",
        "fixture": {
            "kind": "synthetic-size-stress-fixture",
            "source": args.input,
            "sourceBytes": source_bytes,
            "method": "aspect-preserving resize to a bounded tile, repeated to the requested canvas",
            "tile": {"width": tile_width, "height": tile_height},
            "pixelFormat": "8-bit straight-alpha RGBA",
            "width": args.width,
            "height": args.height,
            "megapixels": f64::from(args.width) * f64::from(args.height) / 1_000_000.0,
            "pixelBytes": u64::from(args.width) * u64::from(args.height) * 4,
            "pixelFnv1a64": &fixture_hash,
        },
        "workload": {
            "layers": 2,
            "brushEdits": args.edits,
            "brush": {"tool": "Brush", "sizePx": 24, "continuedSegmentPx": [10, 6]},
            "metadataEdits": 10,
            "historyLimitBytes": HISTORY_LIMIT,
            "compositeSamples": COMPOSITE_SAMPLES,
        },
        "checks": {
            "metadataChangedWithoutPixelCopies": true,
            "maximumHistoryBytes": maximum_history_bytes,
            "historyWithinLimit": maximum_history_bytes <= HISTORY_LIMIT,
            "undoCount": undo_count,
            "redoCount": redo_count,
            "undoRedoExact": true,
            "overBudgetLayerRejected": true,
            "rejectionLeftDocumentAndHistoryIntact": true,
            "saveReopenExact": true,
            "nativeOmuseProjectRoundTrip": true,
            "pngExportExact": true,
        },
        "history": {
            "historyBytes": editor_history_bytes,
            "rasterPatchEntries": history_stats.raster_patch_entries,
            "rasterPatchTiles": history_stats.raster_patch_tiles,
            "rasterPatchBytes": history_stats.raster_patch_bytes,
            "detachedRasterBytes": history_stats.detached_raster_bytes,
        },
        "timingsMs": {
            "brushEdits": brush_ms,
            "brushStroke": {
                "first": stroke_ms[0],
                "median": stroke_median_ms,
                "p95": stroke_p95_ms,
                "max": stroke_max_ms,
            },
            "composite": composite_ms,
            "save": save_ms,
            "reopenAndComposite": reopen_ms,
            "pngExport": export_ms,
            "total": milliseconds(started.elapsed()),
        },
        "artifacts": {
            "compositePixelBytes": expected_composite.as_raw().len(),
            "compositePixelFnv1a64": composite_hash,
            "package": {"path": package, "treeBytes": package_bytes},
            "png": {"path": exported_png, "bytes": png_bytes, "fileFnv1a64": png_file_hash},
        },
        "memory": {
            "currentRssBytes": final_rss.0,
            "peakRssBytes": final_rss.1,
        },
        "limitations": [
            "Synthetic size stress fixture; this does not measure physical-input latency or camera decoding",
            "Bounded single workflow run; this is not a multi-hour soak test",
            "Timings are wall-clock samples and depend on the current host load",
        ],
    }))?;
    Ok(())
}

fn arguments() -> Result<Arguments> {
    let mut input = None;
    let mut output = None;
    let mut width = None;
    let mut height = None;
    let mut edits = 100usize;
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag.to_string_lossy();
        match flag.as_ref() {
            "--input" => input = Some(PathBuf::from(next_value(&mut args, &flag)?)),
            "--output" => output = Some(PathBuf::from(next_value(&mut args, &flag)?)),
            "--width" => width = Some(next_value(&mut args, &flag)?.to_string_lossy().parse()?),
            "--height" => height = Some(next_value(&mut args, &flag)?.to_string_lossy().parse()?),
            "--edits" => edits = next_value(&mut args, &flag)?.to_string_lossy().parse()?,
            "--help" | "-h" => {
                println!(
                    "usage: photo_stress --input PHOTO --output FRESH_DIR --width PX --height PX [--edits 80..120]"
                );
                std::process::exit(0);
            }
            _ => bail!("unknown argument {flag}"),
        }
    }
    Ok(Arguments {
        input: input.context("--input is required")?,
        output: output.context("--output is required")?,
        width: width.context("--width is required")?,
        height: height.context("--height is required")?,
        edits,
    })
}

fn next_value(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    flag: &str,
) -> Result<std::ffi::OsString> {
    args.next().context(format!("{flag} requires a value"))
}

fn synthetic_fixture(source: RgbaImage, width: u32, height: u32) -> Result<(RgbaImage, u32, u32)> {
    ensure!(
        source.width() > 0 && source.height() > 0,
        "empty input photograph"
    );
    let tile_width = width.min(2_048);
    let tile_height = ((u64::from(source.height()) * u64::from(tile_width)
        + u64::from(source.width()) / 2)
        / u64::from(source.width()))
    .clamp(1, u64::from(height)) as u32;
    let tile = image::imageops::resize(&source, tile_width, tile_height, FilterType::Triangle);
    let fixture = RgbaImage::from_fn(width, height, |x, y| {
        *tile.get_pixel(x % tile_width, y % tile_height)
    });
    Ok((fixture, tile_width, tile_height))
}

fn checkpoint(name: &str, started: Instant, details: Value) -> Result<()> {
    let memory = rss();
    emit(&json!({
        "event": "checkpoint",
        "name": name,
        "elapsedMs": milliseconds(started.elapsed()),
        "currentRssBytes": memory.0,
        "peakRssBytes": memory.1,
        "details": details,
    }))
}

fn emit(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    io::stdout().flush()?;
    Ok(())
}

fn rss() -> (Option<u64>, Option<u64>) {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return (None, None);
    };
    fn field(status: &str, name: &str) -> Option<u64> {
        status.lines().find_map(|line| {
            let value = line.strip_prefix(name)?.split_whitespace().next()?;
            value.parse::<u64>().ok()?.checked_mul(1_024)
        })
    }
    (field(&status, "VmRSS:"), field(&status, "VmHWM:"))
}

fn milliseconds(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let index = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len().saturating_sub(1));
    sorted[index]
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn document_fingerprint(document: &Document) -> String {
    fn add(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    fn visit_layer(hash: &mut u64, layer: &Layer) {
        for value in [&layer.id, &layer.name, &layer.blend_mode] {
            add(hash, value.as_bytes());
            add(hash, &[0]);
        }
        add(hash, &[layer.visible as u8, layer.locked as u8]);
        for value in [
            layer.opacity,
            layer.offset_x,
            layer.offset_y,
            layer.rotation,
            layer.scale_x,
            layer.scale_y,
        ] {
            add(hash, &value.to_bits().to_le_bytes());
        }
        add(hash, layer.metadata.to_string().as_bytes());
        for image in [&layer.image, &layer.mask] {
            if let Some(image) = image {
                add(hash, &image.width().to_le_bytes());
                add(hash, &image.height().to_le_bytes());
                add(hash, image.as_raw());
            } else {
                add(hash, &[0xff]);
            }
        }
        add(hash, &(layer.advanced.is_some() as u8).to_le_bytes());
        for child in &layer.children {
            visit_layer(hash, child);
        }
    }
    let mut hash = 0xcbf29ce484222325u64;
    add(&mut hash, &document.width.to_le_bytes());
    add(&mut hash, &document.height.to_le_bytes());
    add(&mut hash, document.name.as_bytes());
    add(&mut hash, &document.background);
    add(&mut hash, document.metadata.to_string().as_bytes());
    for item in &document.layers {
        visit_layer(&mut hash, item);
    }
    format!("{hash:016x}")
}

fn tree_bytes(path: &Path) -> Result<u64> {
    fn walk(path: &Path, total: &mut u64) -> Result<()> {
        for item in fs::read_dir(path)? {
            let item = item?;
            let kind = item.file_type()?;
            ensure!(!kind.is_symlink(), "package contains a symbolic link");
            if kind.is_dir() {
                walk(&item.path(), total)?;
            } else if kind.is_file() {
                *total = total.saturating_add(item.metadata()?.len());
            }
        }
        Ok(())
    }
    let mut total = 0;
    walk(path, &mut total)?;
    Ok(total)
}
