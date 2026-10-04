//! Opt-in qualification with licensed, external photographs; never runs on user files.
use anyhow::{Context, Result, ensure};
use image::{ImageFormat, RgbaImage};
use omuse::{
    create_project::Project,
    document,
    editor::{Adjustment, Editor, Selection},
    model::Document,
    raster::{self, ExportOptions},
    retouch_brush::RetouchMode,
    spot_heal::SpotHealingMode,
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

/// These checks establish transactions and protected-pixel boundaries on a real
/// photo. Before/after images remain necessary for judging photographic quality.
fn checked_history(editor: &mut Editor, before: &Document, depth: usize) -> Result<()> {
    ensure!(
        editor.undo_depth() == depth + 1,
        "Photo stroke was not one Undo step"
    );
    let after = editor.document.clone();
    ensure!(editor.undo(), "Photo stroke Undo unavailable");
    ensure!(
        editor.document.layers[0].image == before.layers[0].image
            && editor.document.layers[0].mask == before.layers[0].mask,
        "Photo stroke Undo changed source or mask pixels"
    );
    ensure!(
        raster::composite(&editor.document) == raster::composite(before),
        "Photo stroke Undo changed rendered pixels"
    );
    ensure!(editor.redo(), "Photo stroke Redo unavailable");
    ensure!(
        editor.document.layers[0].image == after.layers[0].image
            && editor.document.layers[0].mask == after.layers[0].mask,
        "Photo stroke Redo changed source or mask pixels"
    );
    ensure!(
        raster::composite(&editor.document) == raster::composite(&after),
        "Photo stroke Redo changed rendered pixels"
    );
    Ok(())
}

fn sampled_selection(selection: &Selection, x: f32, y: f32) -> f32 {
    let (px, py) = (x - 0.5, y - 0.5);
    let (left, top) = (px.floor() as i64, py.floor() as i64);
    let (fx, fy) = (px - px.floor(), py - py.floor());
    let value = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= i64::from(selection.width) || y >= i64::from(selection.height) {
            0.
        } else {
            f32::from(selection.mask[y as usize * selection.width as usize + x as usize])
        }
    };
    ((1. - fx) * value(left, top) + fx * value(left + 1, top)) * (1. - fy)
        + ((1. - fx) * value(left, top + 1) + fx * value(left + 1, top + 1)) * fy
}

fn stroke_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0. {
        0.
    } else {
        ((point.0 - start.0) * dx + (point.1 - start.1) * dy) / length
    }
    .clamp(0., 1.);
    (point.0 - start.0 - t * dx).hypot(point.1 - start.1 - t * dy)
}

fn photo_retouch(
    document: &Document,
    output: &Path,
    measurements: &mut Vec<Value>,
) -> Result<Value> {
    let output = output.join("retouch");
    fs::create_dir(&output)?;
    let (width, height) = (document.width as f32, document.height as f32);
    let shorter = width.min(height);
    let id = document.layers[0].id.clone();
    let native = document.layers[0]
        .image
        .as_ref()
        .context("Photo has no editable raster")?;
    let mut trials = Vec::new();
    let original_hash = fingerprint(native.as_raw());

    // A recorded fixed local target, corresponding to the small mission patch
    // on the unrotated NASA portrait fixture. Other inputs exercise local repair
    // at the same fractional location; no semantic or quality verdict is made.
    let mut heal = Editor::new(document.clone());
    let heal_point = (width * 0.332, height * 0.748);
    heal.brush.size = (shorter * 0.18).clamp(6., 96.);
    heal.brush.hardness = 0.9;
    heal.brush.opacity = 1.;
    let heal_radius = heal.brush.size * 0.5;
    save_png(
        &raster::composite(&heal.document),
        &output.join("spot-heal-before.png"),
    )?;
    timed(measurements, "photo_spot_heal_content_aware", || {
        ensure!(
            heal.spot_heal_stroke(&[heal_point], SpotHealingMode::ContentAware, 0x4f4d5553)?,
            "Local photo repair did not change the chosen region"
        );
        Ok(())
    })?;
    let healed = heal.document.layers[0].image.as_ref().unwrap();
    let mut healed_pixels = 0;
    for (x, y, old) in native.enumerate_pixels() {
        let new = healed.get_pixel(x, y);
        if (x as f32 + 0.5 - heal_point.0).hypot(y as f32 + 0.5 - heal_point.1) > heal_radius + 0.01
        {
            ensure!(new == old, "Spot Heal changed a pixel outside its brush");
        }
        healed_pixels += usize::from(new != old);
    }
    checked_history(&mut heal, document, 0)?;
    save_png(
        &raster::composite(&heal.document),
        &output.join("spot-heal-after.png"),
    )?;
    trials.push(json!({"tool":"Spot Heal","mode":"Content aware","target_canvas":heal_point,"diameter":heal.brush.size,"changed_native_pixels":healed_pixels,"undo_redo":"exact","quality":"requires visual review; fixed local region, not a semantic-removal score"}));
    drop(heal);

    for transformed in [false, true] {
        for (name, mode) in [
            ("blur", RetouchMode::Blur),
            ("smudge", RetouchMode::Smudge),
            ("liquify", RetouchMode::Liquify),
        ] {
            let variant = if transformed {
                "transformed-soft-selection"
            } else {
                "flat"
            };
            let stem = format!("{variant}-{name}");
            let mut work_doc = document.clone();
            if transformed {
                let layer = &mut work_doc.layers[0];
                // Retain every source pixel while displaying a smaller,
                // anisotropically scaled, rotated and horizontally flipped layer.
                layer.scale_x = -0.62;
                layer.scale_y = 0.72;
                layer.rotation = 17.;
                layer.offset_x = width * 0.19 + 0.25;
                layer.offset_y = height * 0.14 + 0.375;
            }
            let before = work_doc.clone();
            let mut editor = Editor::new(work_doc);
            editor.brush.size = (shorter * 0.075).clamp(8., 64.);
            editor.brush.hardness = 0.65;
            editor.brush.opacity = 0.75;
            editor.brush.blur_radius = 3.5;
            let path = [
                editor
                    .layer_to_canvas(&id, width * 0.40, height * 0.48)
                    .context("Missing photo placement")?,
                editor
                    .layer_to_canvas(&id, width * 0.53, height * 0.48)
                    .context("Missing photo placement")?,
            ];
            if transformed {
                editor.select_ellipse(width * 0.50, height * 0.25, width * 0.30, height * 0.50);
                ensure!(
                    editor.feather_selection((shorter * 0.025).clamp(1., 32.)),
                    "Photo selection feather failed"
                );
                ensure!(
                    editor
                        .selection
                        .as_ref()
                        .unwrap()
                        .mask
                        .iter()
                        .any(|&v| v > 0 && v < 255),
                    "Photo selection has no fractional coverage"
                );
            }
            let selection = editor.selection.clone();
            let depth = editor.undo_depth();
            save_png(
                &raster::composite(&editor.document),
                &output.join(format!("{stem}-before.png")),
            )?;
            let cancelled = std::sync::atomic::AtomicBool::new(true);
            let error = editor
                .retouch_stroke_cancellable(&path, mode, &cancelled)
                .expect_err("Cancelled photo stroke was accepted");
            ensure!(
                error.to_string().contains("cancelled")
                    && editor.undo_depth() == depth
                    && editor.document.layers[0].image == before.layers[0].image,
                "Cancelled photo stroke changed pixels/history"
            );
            timed(measurements, &format!("photo_{stem}"), || {
                ensure!(
                    editor.retouch_stroke(&path, mode)?,
                    "Photo retouch did not change the chosen region"
                );
                Ok(())
            })?;
            let after = editor.document.layers[0].image.as_ref().unwrap();
            ensure!(
                after.dimensions() == native.dimensions(),
                "Native retouch resized the source raster"
            );
            let mut changed = 0usize;
            let mut protected = 0usize;
            let mut changed_soft_edge = 0usize;
            for (x, y, old) in native.enumerate_pixels() {
                let point = editor
                    .layer_to_canvas(&id, x as f32 + 0.5, y as f32 + 0.5)
                    .unwrap();
                let selected = selection
                    .as_ref()
                    .map_or(255., |s| sampled_selection(s, point.0, point.1));
                let outside = stroke_distance(point, path[0], path[1])
                    > editor.brush.size * 0.5 + 0.01
                    || selected == 0.;
                if outside {
                    ensure!(
                        after.get_pixel(x, y) == old,
                        "Photo {stem} changed a protected source pixel"
                    );
                    protected += 1;
                }
                changed += usize::from(after.get_pixel(x, y) != old);
                changed_soft_edge +=
                    usize::from(selected > 0. && selected < 255. && after.get_pixel(x, y) != old);
                if mode == RetouchMode::Blur {
                    ensure!(
                        after.get_pixel(x, y)[3] == old[3],
                        "Photo Blur changed source alpha"
                    );
                }
            }
            ensure!(
                changed > 0 && protected > 0,
                "Photo retouch boundary fixture was ineffective"
            );
            ensure!(
                !transformed || changed_soft_edge > 0,
                "Photo retouch did not exercise a partially selected pixel"
            );
            let after_hash = fingerprint(after.as_raw());
            save_png(after, &output.join(format!("{stem}-native-after.png")))?;
            checked_history(&mut editor, &before, depth)?;
            ensure!(
                editor.selection == selection,
                "Photo stroke Undo/Redo changed its selection"
            );
            save_png(
                &raster::composite(&editor.document),
                &output.join(format!("{stem}-after.png")),
            )?;
            trials.push(json!({"tool":name,"variant":variant,"native_dimensions":native.dimensions(),"path_canvas":path,"diameter":editor.brush.size,"blur_radius_canvas":editor.brush.blur_radius,"changed_native_pixels":changed,"protected_native_pixels":protected,"changed_soft_selection_pixels":changed_soft_edge,"cancel":"atomic","undo_redo":"exact","native_before_fnv1a64":original_hash,"native_after_fnv1a64":after_hash}));
        }
    }

    let mut setup = Editor::new(document.clone());
    setup.select_ellipse(width * 0.20, height * 0.15, width * 0.60, height * 0.72);
    ensure!(
        setup.feather_selection((shorter * 0.025).clamp(1., 32.)),
        "Photo mask feather failed"
    );
    ensure!(
        setup.add_mask(&id, true),
        "Photo soft selection mask failed"
    );
    setup.clear_selection();
    let before = setup.document;
    let before_mask = before.layers[0]
        .mask
        .as_ref()
        .context("Photo mask missing")?;
    ensure!(
        before_mask.pixels().any(|p| p[0] > 0 && p[0] < 255),
        "Photo mask lacks soft coverage"
    );
    save_png(&raster::composite(&before), &output.join("mask-before.png"))?;
    save_png(before_mask, &output.join("mask-native-before.png"))?;
    let mut mask = Editor::new(before.clone());
    mask.brush.size = (shorter * 0.12).clamp(8., 96.);
    mask.brush.hardness = 0.65;
    mask.brush.opacity = 0.7;
    let path = [(width * 0.20, height * 0.51), (width * 0.28, height * 0.51)];
    timed(measurements, "photo_soft_mask_liquify", || {
        ensure!(
            mask.retouch_mask_stroke(&id, &path, RetouchMode::Liquify)?,
            "Photo soft mask retouch made no change"
        );
        Ok(())
    })?;
    ensure!(
        mask.document.layers[0].image == before.layers[0].image,
        "Photo mask retouch altered artwork"
    );
    checked_history(&mut mask, &before, 0)?;
    let final_pixels = raster::composite(&mask.document);
    save_png(&final_pixels, &output.join("mask-after.png"))?;
    save_png(
        mask.document.layers[0].mask.as_ref().unwrap(),
        &output.join("mask-native-after.png"),
    )?;
    let package = output.join("soft-mask-retouch.omuse");
    timed(measurements, "photo_soft_mask_save", || {
        document::save(&mask.document, &package)
    })?;
    let reopened = timed(measurements, "photo_soft_mask_reopen", || {
        document::open(&package)
    })?;
    ensure!(
        reopened.layers[0].image == mask.document.layers[0].image
            && reopened.layers[0].mask == mask.document.layers[0].mask,
        "Photo mask save/reopen changed source pixels"
    );
    ensure!(
        raster::composite(&reopened) == final_pixels,
        "Photo mask save/reopen changed composite"
    );
    for extension in ["png", "tiff"] {
        let path = output.join(format!("soft-mask-export.{extension}"));
        timed(
            measurements,
            &format!("photo_soft_mask_export_{extension}"),
            || raster::export_with_options(&reopened, &path, ExportOptions::default()),
        )?;
        ensure!(
            image::open(&path)?.to_rgba8() == final_pixels,
            "Photo mask lossless export changed pixels"
        );
    }
    trials.push(json!({"tool":"mask Liquify","mask":"feathered ellipse from selection","path_canvas":path,"artwork":"unchanged","undo_redo":"exact","save_reopen":"source, mask and composite exact","exports":"PNG and TIFF exact"}));
    Ok(
        json!({"status":"passed","working_dimensions":native.dimensions(),"cases":trials,"quality":"Before/after images require human review; numerical success does not establish general photographic reconstruction quality"}),
    )
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
    let retouch = photo_retouch(&editor.document, output, &mut measurements)?;
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
        "checks":["import","save/reopen","retained source and mask","selected adjustment boundaries","undo/redo pixel equality","explicit pixel conversion when needed","real-photo Spot Heal and native retouch","transformed soft selection boundaries","soft mask retouch save/reopen/export","crop","resize","PNG/TIFF/WebP exact export","JPEG dimensions"],
        "retouch":retouch,"measurements":measurements,"memory":memory()
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
