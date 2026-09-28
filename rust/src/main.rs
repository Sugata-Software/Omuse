mod adjustment_controls;
mod camera_canvas;
mod camera_controls;
mod curve_editor;
mod display_surface;
mod preferences;
mod recovery;
mod shortcuts;
mod startup;
mod studio_icons;
mod studio_thumbnails;
mod transform_interaction;
fn adjustment_base() -> serde_json::Value {
    omuse::effects::adjustment_for_filter(&omuse::filters::Filter::Invert)
        .expect("default adjustment")
}
mod ui;

use gpui_kit::{AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use std::path::PathBuf;

// A real inotify callback cannot wake GPUI's deterministic test executor.
// Initialize the same controls, then stop system watching before yielding.
// Native acceptance tests use normal init and exercise the actual watcher.
#[cfg(all(test, feature = "ui-test"))]
fn init_test_theme(cx: &mut gpui_kit::App) {
    gpui_omarchy::init(cx);
    gpui_omarchy::Theme::tokyo_night().apply(cx);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--batch") {
        if !(4..=6).contains(&args.len()) {
            eprintln!(
                "Usage: omuse --batch RECIPE.json INPUT_FOLDER OUTPUT_FOLDER [png|jpg|webp|tiff] [CANCEL_FILE]"
            );
            std::process::exit(2);
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if args
            .get(5)
            .is_some_and(|path| std::path::Path::new(path).exists())
        {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher = args.get(5).map(|path| {
            let path = PathBuf::from(path);
            let cancel = cancel.clone();
            let finished = finished.clone();
            std::thread::spawn(move || {
                while !finished.load(std::sync::atomic::Ordering::Relaxed) {
                    if path.exists() {
                        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            })
        });
        let result =
            omuse::recipes::Recipe::load(std::path::Path::new(&args[1])).and_then(|recipe| {
                omuse::recipes::batch(
                    &recipe,
                    std::path::Path::new(&args[2]),
                    std::path::Path::new(&args[3]),
                    args.get(4).map(String::as_str).unwrap_or("png"),
                    &cancel,
                    |done, total| {
                        eprintln!("{}", serde_json::json!({"completed": done, "total": total}));
                    },
                )
            });
        finished.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(watcher) = watcher {
            let _ = watcher.join();
        }
        match result {
            Ok(report) => {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                if report.cancelled {
                    std::process::exit(130);
                }
                if report.items.iter().any(|item| item.error.is_some()) {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("Batch failed: {error:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        println!(
            "Omuse {}\n\nUsage: omuse [PROJECT.omuse | PROJECT.comp | IMAGE]\n       omuse --export INPUT.comp OUTPUT.png\n       omuse --batch RECIPE.json INPUT_FOLDER OUTPUT_FOLDER [png|jpg|webp|tiff] [CANCEL_FILE]\n       omuse --self-test [EVIDENCE_DIRECTORY]\n       omuse --ui-smoke EVIDENCE_DIRECTORY\n\nNative Linux editor using GPUI and gpui-omarchy. The previous editor remains separate.",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    if args.first().map(String::as_str) == Some("--version") {
        println!("Omuse {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.first().map(String::as_str) == Some("--export") {
        if args.len() != 3 {
            eprintln!("Usage: omuse --export INPUT.comp OUTPUT.png");
            std::process::exit(2);
        }
        let result = ui::EditorView::open_content(std::path::Path::new(&args[1]))
            .and_then(|(doc, _)| omuse::raster::export(&doc, std::path::Path::new(&args[2])));
        if let Err(error) = result {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().map(String::as_str) == Some("--self-test") {
        if let Err(error) = ui::self_test(args.get(1).map(PathBuf::from)) {
            eprintln!("Self-test failed: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    let native_journey = if args.first().map(String::as_str) == Some("--ui-smoke") {
        Some(
            args.get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("omuse-native-journey")),
        )
    } else {
        None
    };
    let path = if native_journey.is_some() {
        None
    } else {
        args.first().map(PathBuf::from)
    };
    gpui_kit::application()
        .with_assets(studio_icons::StudioAssets)
        .run(move |cx| {
            gpui_omarchy::init(cx);
            ui::bind_keys(cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Omuse".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Maximized(Bounds::centered(
                    None,
                    size(px(1440.), px(920.)),
                    cx,
                ))),
                window_min_size: Some(size(px(800.), px(600.))),
                app_id: Some("omuse".to_owned()),
                ..Default::default()
            };
            match cx.open_window(options, |window, cx| {
                cx.new(|cx| {
                    startup::StartupView::new(path.clone(), native_journey.clone(), window, cx)
                })
            }) {
                Ok(_) => cx.activate(true),
                Err(error) => {
                    eprintln!("Unable to open the editor: {error:#}");
                    cx.quit();
                }
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        });
}
