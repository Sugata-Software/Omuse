use super::*;
use gpui_kit::TestAppContext;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const SMALL_EDGE: u32 = 520;
const WAIT_IDLE: Duration = Duration::from_secs(30);

struct DesktopRun {
    detached_bytes: usize,
    image_bytes: usize,
    patch_entries: usize,
    undo_depth: usize,
    final_hash: u64,
    recovery_hash: u64,
}

fn fixture(width: u32, height: u32) -> Document {
    let mut document = Document::new(width, height);
    document.layers[0].image = Some(
        image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([
                19u8.wrapping_add((x % 211) as u8),
                37u8.wrapping_add((y % 193) as u8),
                ((x / 7 + y / 11) % 251) as u8,
                255,
            ])
        })
        .into(),
    );
    document
}

fn hash(image: &image::RgbaImage) -> u64 {
    image
        .as_raw()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

fn wait_recovery(view: &Entity<EditorView>, cx: &mut gpui_kit::VisualTestContext) -> PathBuf {
    view.update(cx, |view, _| {
        view.recovery.wait_idle_for_test(WAIT_IDLE).unwrap()
    })
}

fn stroke(
    view: &Entity<EditorView>,
    cx: &mut gpui_kit::VisualTestContext,
    index: usize,
    measure: bool,
) -> (Duration, Duration) {
    let x = 40. + ((index * 31) % (SMALL_EDGE as usize - 100)) as f32;
    let y = 50. + ((index * 47) % (SMALL_EDGE as usize - 110)) as f32;
    let full_started = Instant::now();
    let begin_elapsed = view.update_in(cx, |view, window, cx| {
        let started = Instant::now();
        assert!(view.editor.begin_stroke(x, y, 1., PaintTool::Brush));
        assert!(view.editor.continue_stroke(x + 18., y + 9., 1.));
        let elapsed = started.elapsed();
        view.queue_stroke_frame(window, cx);
        elapsed
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let finish_elapsed = view.update(cx, |view, cx| {
        let started = Instant::now();
        assert!(view.editor.finish_stroke());
        let elapsed = started.elapsed();
        view.changed(cx);
        elapsed
    });
    let full_elapsed = full_started.elapsed();
    view.update(cx, |view, _| {
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
    });
    if measure {
        (begin_elapsed + finish_elapsed, full_elapsed)
    } else {
        (Duration::ZERO, Duration::ZERO)
    }
}

fn setup(
    cx: &mut TestAppContext,
    width: u32,
    height: u32,
    legacy: bool,
) -> (
    Entity<EditorView>,
    &mut gpui_kit::VisualTestContext,
    tempfile::TempDir,
    Document,
    image::RgbaImage,
) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let recovery = Recovery::at(temp.path().join("recovery"));
    let document = fixture(width, height);
    let retained = document.clone();
    let retained_pixels = raster::composite(&retained);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = recovery;
        view.editor = Editor::new(document);
        view.editor.set_region_history_enabled(true);
        view.editor.brush.size = 13.;
        view.editor.brush.color = [231, 45, 87, 255];
        view.dialog = Dialog::None;
        view.ensure_create().unwrap();
        view.create
            .session
            .as_mut()
            .unwrap()
            .retain_synced_documents_for_test = legacy;
        view.refresh(cx);
        view
    });
    (view, cx, temp, retained, retained_pixels)
}

fn bounded_run(cx: &mut TestAppContext, legacy: bool) -> DesktopRun {
    let (view, cx, _temp, retained, retained_pixels) = setup(cx, SMALL_EDGE, SMALL_EDGE, legacy);
    assert!((SMALL_EDGE as usize) * (SMALL_EDGE as usize) * 4 >= 1024 * 1024);

    // The warmup consumes the intentional detach from the immutable owner above.
    stroke(&view, cx, 0, false);
    wait_recovery(&view, cx);
    let before = view.update(cx, |view, _| {
        view.editor.history_stats().detached_raster_bytes
    });

    for index in 1..=2 {
        wait_recovery(&view, cx);
        stroke(&view, cx, index, false);
    }
    wait_recovery(&view, cx);

    let (detached_bytes, image_bytes, patch_entries, undo_depth, final_pixels) =
        view.update(cx, |view, _| {
            let stats = view.editor.history_stats();
            let mut snapshot = view.content_snapshot().unwrap();
            let current = snapshot.active_document().unwrap();
            assert_eq!(
                (current.width, current.height),
                (view.editor.document.width, view.editor.document.height)
            );
            assert_eq!(raster::composite(current), view.pixels);
            (
                stats.detached_raster_bytes - before,
                view.editor.document.layers[0]
                    .image
                    .as_ref()
                    .unwrap()
                    .as_raw()
                    .len(),
                stats.raster_patch_entries,
                view.editor.undo_depth(),
                view.pixels.clone(),
            )
        });
    if legacy {
        assert_eq!(detached_bytes, image_bytes * 2);
    } else {
        assert_eq!(detached_bytes, 0);
    }
    assert!(patch_entries >= 3);

    view.update(cx, |view, cx| {
        assert!(view.editor.undo());
        view.changed(cx);
        assert!(view.editor.redo());
        view.changed(cx);
        assert_eq!(view.pixels, final_pixels);
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
    });
    let recovery_path = wait_recovery(&view, cx);
    let mut recovered = omuse::create_project::Project::open(&recovery_path).unwrap();
    let recovery_pixels = raster::composite(recovered.active_document().unwrap());
    assert_eq!(recovery_pixels, final_pixels);
    assert_eq!(raster::composite(&retained), retained_pixels);

    DesktopRun {
        detached_bytes,
        image_bytes,
        patch_entries,
        undo_depth,
        final_hash: hash(&final_pixels),
        recovery_hash: hash(&recovery_pixels),
    }
}

#[gpui_kit::test]
fn desktop_create_history_releases_idle_raster_owners_without_weakening_snapshots(
    cx: &mut TestAppContext,
) {
    let current = bounded_run(cx, false);
    let legacy = bounded_run(cx, true);
    assert_eq!(current.detached_bytes, 0);
    assert_eq!(legacy.detached_bytes, legacy.image_bytes * 2);
    assert_eq!(current.final_hash, legacy.final_hash);
    assert_eq!(current.recovery_hash, legacy.recovery_hash);
    assert_eq!(current.undo_depth, legacy.undo_depth);
    assert_eq!(current.patch_entries, legacy.patch_entries);
}

fn percentile_ms(samples: &mut [Duration], quantile: f64) -> f64 {
    samples.sort_unstable();
    let index = ((samples.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(samples.len() - 1);
    samples[index].as_secs_f64() * 1000.
}

fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kib = status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    })?;
    kib.checked_mul(1024)
}

#[gpui_kit::test]
#[ignore = "manual desktop history benchmark; run one size and mode in release mode"]
fn benchmark_desktop_create_history(cx: &mut TestAppContext) {
    const STROKES: usize = 12;
    let megapixels = std::env::var("OMUSE_DESKTOP_STRESS_MP").unwrap_or_default();
    let (width, height) = match megapixels.as_str() {
        "12" => (4000, 3000),
        "24" => (6000, 4000),
        "48" => (8000, 6000),
        _ => panic!("OMUSE_DESKTOP_STRESS_MP must be exactly 12, 24, or 48"),
    };
    let mode = std::env::var("OMUSE_DESKTOP_STRESS_MODE").unwrap_or_default();
    let legacy = match mode.as_str() {
        "legacy" => true,
        "current" => false,
        _ => panic!("OMUSE_DESKTOP_STRESS_MODE must be legacy or current"),
    };
    let (view, cx, _temp, retained, retained_pixels) = setup(cx, width, height, legacy);

    stroke(&view, cx, 0, false);
    wait_recovery(&view, cx);
    let before = view.update(cx, |view, _| view.editor.history_stats());
    let warmup_pixels = view.update(cx, |view, _| view.pixels.clone());
    let warmup_hash = hash(&warmup_pixels);
    let mut paint_core = Vec::with_capacity(STROKES);
    let mut desktop_wall = Vec::with_capacity(STROKES);
    for index in 1..=STROKES {
        // Recovery waiting is deliberately outside both timing samples.
        wait_recovery(&view, cx);
        let (core, wall) = stroke(&view, cx, index, true);
        paint_core.push(core);
        desktop_wall.push(wall);
    }
    let recovery_path = wait_recovery(&view, cx);
    let (after, undo_depth, final_pixels) = view.update(cx, |view, _| {
        (
            view.editor.history_stats(),
            view.editor.undo_depth(),
            view.pixels.clone(),
        )
    });
    assert_eq!(
        final_pixels,
        view.update(cx, |view, _| raster::composite(&view.editor.document))
    );

    for _ in 0..STROKES {
        view.update(cx, |view, cx| {
            assert!(view.editor.undo());
            view.changed(cx);
        });
    }
    let undo_hash = view.update(cx, |view, _| {
        assert_eq!(view.pixels, warmup_pixels);
        assert_eq!(view.pixels, raster::composite(&view.editor.document));
        hash(&view.pixels)
    });
    drop(warmup_pixels);
    for _ in 0..STROKES {
        view.update(cx, |view, cx| {
            assert!(view.editor.redo());
            view.changed(cx);
        });
    }
    let final_after_redo = view.update(cx, |view, _| view.pixels.clone());
    assert_eq!(final_after_redo, final_pixels);
    let final_recovery_path = wait_recovery(&view, cx);
    assert_eq!(final_recovery_path, recovery_path);
    let mut recovered = omuse::create_project::Project::open(&final_recovery_path).unwrap();
    let recovery_pixels = raster::composite(recovered.active_document().unwrap());
    assert_eq!(recovery_pixels, final_pixels);
    assert_eq!(raster::composite(&retained), retained_pixels);

    let paint_median = percentile_ms(&mut paint_core.clone(), 0.50);
    let paint_p95 = percentile_ms(&mut paint_core.clone(), 0.95);
    let paint_max = paint_core.iter().max().unwrap().as_secs_f64() * 1000.;
    let wall_median = percentile_ms(&mut desktop_wall.clone(), 0.50);
    let wall_p95 = percentile_ms(&mut desktop_wall.clone(), 0.95);
    let wall_max = desktop_wall.iter().max().unwrap().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::json!({
            "event": "desktop-history-result",
            "mode": mode,
            "width": width,
            "height": height,
            "megapixels": (u64::from(width) * u64::from(height)) as f64 / 1_000_000.,
            "strokes": STROKES,
            "strokeLocalityPx": [SMALL_EDGE, SMALL_EDGE],
            "recoveryWaitIncludedInTimings": false,
            "timingsArePhysicalInputLatency": false,
            "paintCoreMs": {"median": paint_median, "p95": paint_p95, "max": paint_max},
            "desktopFrameAndChangedWallMs": {"median": wall_median, "p95": wall_p95, "max": wall_max},
            "history": {
                "detachedRasterBytes": after.detached_raster_bytes - before.detached_raster_bytes,
                "rasterPatchEntries": after.raster_patch_entries,
                "rasterPatchTiles": after.raster_patch_tiles,
                "rasterPatchBytes": after.raster_patch_bytes,
                "undoDepth": undo_depth,
            },
            "hashes": {
                "warmup": format!("{warmup_hash:016x}"),
                "undo": format!("{undo_hash:016x}"),
                "final": format!("{:016x}", hash(&final_pixels)),
                "redo": format!("{:016x}", hash(&final_after_redo)),
                "recovery": format!("{:016x}", hash(&recovery_pixels)),
                "retainedSnapshot": format!("{:016x}", hash(&retained_pixels)),
            },
            "peakRssBytes": peak_rss_bytes(),
            "limitations": [
                "Wall-clock process samples; not physical-input latency",
                "Recovery was idle before each stroke to isolate long-lived Create ownership",
                "Recovery serialization and exact composite verification are outside timing samples",
                "Outstanding recovery or save snapshots still correctly force copy-on-write",
            ],
        })
    );
}
