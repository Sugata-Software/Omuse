//! Opt-in qualification with licensed, external photographs; never runs on user files.
use anyhow::{Context, Result, ensure};
use image::{ImageFormat, RgbaImage};
use omuse::{
    create_project::Project,
    document,
    editor::{Adjustment, Editor},
    raster::{self, ExportOptions},
};
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::Path, time::Instant};

fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn memory() -> Value {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|tail| tail.split_whitespace().next())
                .and_then(|n| n.parse::<u64>().ok())
        })
    };
    json!({"rss_kib": field("VmRSS:"), "peak_rss_kib": field("VmHWM:")})
}

fn timed<T>(rows: &mut Vec<Value>, name: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
    let started = Instant::now();
    let result = work().with_context(|| name.to_owned())?;
    let row = json!({"stage":name,"milliseconds":started.elapsed().as_secs_f64()*1000.,"memory":memory()});
    println!("{row}");
    std::io::stdout().flush()?;
    rows.push(row);
    Ok(result)
}

fn save_png(image: &RgbaImage, path: &Path) -> Result<()> {
    image.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

fn run(input: &Path, output: &Path, require_precision: bool) -> Result<()> {
    ensure!(!output.exists(), "Use a fresh result directory");
    fs::create_dir_all(output)?;
    let mut measurements = Vec::new();
    let doc = timed(&mut measurements, "import", || document::open(input))?;
    let id = doc
        .layers
        .first()
        .context("Imported image has no layer")?
        .id
        .clone();
    let imported = raster::composite(&doc);
    save_png(&imported, &output.join("imported.png"))?;
    let initial_hash = fingerprint(imported.as_raw());
    let initial_size = (doc.width, doc.height);
    let original_state = doc.layers[0].advanced.clone();
    ensure!(
        !require_precision || original_state.is_some(),
        "High-precision input lost its editable 16-bit source"
    );
    if let Some(state) = &original_state {
        fs::write(output.join("retained16.png"), state.source.to_png16()?)?;
    }
    let original_package = output.join("original.omuse");
    let mut project = Project::new("Photo qualification", doc);
    timed(&mut measurements, "save_original", || {
        project.save(&original_package)
    })?;
    drop(project);
    let reopened = timed(&mut measurements, "reopen_original", || {
        let mut project = Project::open(&original_package)?;
        Ok(project.active_document()?.clone())
    })?;
    ensure!(
        raster::composite(&reopened) == imported,
        "Original package pixels changed"
    );
    if let Some(state) = &original_state {
        let actual = reopened.layers[0]
            .advanced
            .as_ref()
            .context("Reopened source lost precision")?;
        ensure!(
            actual.source.to_rgba16() == state.source.to_rgba16(),
            "Retained 16-bit master changed"
        );
        ensure!(
            actual.raw_bytes == state.raw_bytes,
            "Embedded RAW bytes changed"
        );
        let reference = raster::composite16(&reopened)?;
        for extension in ["png", "tiff"] {
            let path = output.join(format!("original16.{extension}"));
            timed(&mut measurements, &format!("export16_{extension}"), || {
                raster::export16(&reopened, &path)
            })?;
            ensure!(
                image::open(path)?.to_rgba16() == reference,
                "16-bit export samples changed"
            );
        }
    }
    drop(original_state);
    drop(imported);
    let mut editor = Editor::new(reopened);
    // This is an explicit pixel-editing conversion. Retained sources and
    // high-precision exports above are qualified before that conversion.
    if editor.document.layers[0].advanced.is_some() {
        ensure!(
            editor.rasterize_layer(&id),
            "Explicit conversion to pixels failed"
        );
    }
    let width = editor.document.width.min(1024);
    let height = ((u64::from(editor.document.height) * u64::from(width)
        / u64::from(editor.document.width)) as u32)
        .max(8);
    if (width, height) != (editor.document.width, editor.document.height) {
        ensure!(editor.resize_image(width, height), "Working resize failed");
    }
    let before = raster::composite(&editor.document);
    editor.select_rectangle(
        (width / 4) as f32,
        (height / 4) as f32,
        (width / 2) as f32,
        (height / 2) as f32,
    );
    ensure!(
        editor.adjust(Adjustment::Brightness(0.1)),
        "Selected brightness failed"
    );
    let adjusted = raster::composite(&editor.document);
    for (x, y, pixel) in before.enumerate_pixels() {
        if x < width / 4
            || x >= width / 4 + width / 2
            || y < height / 4
            || y >= height / 4 + height / 2
        {
            ensure!(
                adjusted.get_pixel(x, y) == pixel,
                "Adjustment changed an unselected pixel"
            );
        }
    }
    ensure!(editor.undo(), "Adjustment undo unavailable");
    ensure!(
        raster::composite(&editor.document) == before,
        "Adjustment undo changed pixels"
    );
    ensure!(editor.redo(), "Adjustment redo unavailable");
    ensure!(
        raster::composite(&editor.document) == adjusted,
        "Adjustment redo changed pixels"
    );
    ensure!(editor.add_mask(&id, true), "Selection mask failed");
    let margin = width.min(height) / 16;
    ensure!(
        editor.crop_canvas(
            margin as i32,
            margin as i32,
            width - 2 * margin,
            height - 2 * margin
        ),
        "Crop failed"
    );
    ensure!(
        editor.resize_image((width / 2).max(8), (height / 2).max(8)),
        "Final resize failed"
    );
    let final_pixels = raster::composite(&editor.document);
    let final_hash = fingerprint(final_pixels.as_raw());
    ensure!(
        editor.undo() && editor.redo(),
        "Resize undo/redo unavailable"
    );
    ensure!(
        raster::composite(&editor.document) == final_pixels,
        "Resize undo/redo changed pixels"
    );
    let source = editor.document.layers[0].image.clone();
    let mask = editor.document.layers[0].mask.clone();
    let edited_package = output.join("edited.omuse");
    let mut edited = Project::new("Edited photo qualification", editor.document);
    timed(&mut measurements, "save_edited", || {
        edited.save(&edited_package)
    })?;
    drop(edited);
    let final_doc = timed(&mut measurements, "reopen_edited", || {
        let mut edited = Project::open(&edited_package)?;
        Ok(edited.active_document()?.clone())
    })?;
    ensure!(
        raster::composite(&final_doc) == final_pixels,
        "Edited package pixels changed"
    );
    ensure!(
        final_doc.layers[0].image == source && final_doc.layers[0].mask == mask,
        "Edited source or mask changed"
    );
    save_png(&final_pixels, &output.join("expected.png"))?;
    let options = ExportOptions {
        jpeg_quality: 100,
        matte: [17, 23, 29],
    };
    for extension in ["png", "tiff", "webp", "jpg"] {
        let path = output.join(format!("export.{extension}"));
        timed(&mut measurements, &format!("export_{extension}"), || {
            raster::export_with_options(&final_doc, &path, options)
        })?;
        let actual = image::open(path)?.to_rgba8();
        ensure!(
            actual.dimensions() == final_pixels.dimensions(),
            "Export dimensions changed"
        );
        if extension != "jpg" {
            ensure!(actual == final_pixels, "Lossless export pixels changed");
        }
    }
    let report = json!({
        "status":"passed","input":input,"source_dimensions":initial_size,
        "retained_16_bit":require_precision,"initial_pixel_fnv1a64":initial_hash,
        "final_pixel_fnv1a64":final_hash,"jpeg_matte":options.matte,
        "checks":["import","save/reopen","retained source and mask","selected adjustment boundaries","undo/redo pixel equality","explicit pixel conversion when needed","crop","resize","PNG/TIFF/WebP exact export","JPEG dimensions"],
        "measurements":measurements,"memory":memory()
    });
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [input, output] => run(Path::new(input), Path::new(output), false),
        [input, output, flag] if flag == "--precision" => {
            run(Path::new(input), Path::new(output), true)
        }
        _ => anyhow::bail!("usage: photo_qualification INPUT FRESH_OUTPUT [--precision]"),
    }
}
