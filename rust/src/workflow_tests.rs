use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn click(cx: &mut VisualTestContext, id: &'static str) {
    if id.starts_with("workflow-") {
        let viewport = cx
            .debug_bounds("dialog-body")
            .expect("workflow scroll area");
        let mut reachable = false;
        for _ in 0..30 {
            let bounds = cx.debug_bounds(id).unwrap();
            if bounds.origin.y >= viewport.origin.y
                && bounds.bottom_right().y <= viewport.bottom_right().y
            {
                reachable = true;
                break;
            }
            let delta = if bounds.origin.y < viewport.origin.y {
                80.
            } else {
                -80.
            };
            cx.simulate_event(gpui_kit::ScrollWheelEvent {
                position: viewport.center(),
                delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
                modifiers: Modifiers::default(),
                touch_phase: gpui_kit::TouchPhase::Moved,
            });
            draw(cx);
        }
        assert!(
            reachable,
            "Workflow control {id} is not reachable by scrolling"
        );
    }
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}
fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<EditorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let tmp = tempfile::tempdir().unwrap();
    let recovery = Recovery::at(tmp.path().join("recovery"));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = recovery;
        let mut document = Document::new(16, 12);
        document.layers[0].image =
            Some(image::RgbaImage::from_pixel(16, 12, image::Rgba([4, 5, 6, 255])).into());
        view.editor = Editor::new(document);
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    (view, cx, tmp)
}

#[gpui_kit::test]
fn source_import_duplicate_undo_and_package_roundtrip(cx: &mut TestAppContext) {
    let (view, cx, tmp) = setup(cx);
    let source = tmp.path().join("source.png");
    image::RgbaImage::from_pixel(7, 5, image::Rgba([20, 80, 140, 211]))
        .save(&source)
        .unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workflow(WorkflowKind::Source, window, cx)
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.path_input.update(cx, |input, cx| {
                input.set_value(source.to_string_lossy(), window, cx)
            });
        });
    });
    // The real modal footer remains reachable at the minimum supported window.
    draw(cx);
    for id in [
        "workflow-path",
        "workflow-import",
        "cancel-dialog",
        "confirm-dialog",
    ] {
        let b = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"));
        assert!(
            b.origin.x >= px(0.)
                && b.origin.y >= px(0.)
                && b.origin.x + b.size.width <= px(800.)
                && b.origin.y + b.size.height <= px(600.)
        );
    }
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let source_id = view.update(cx, |view, _| {
        view.editor
            .document
            .find_layer(&view.editor.active_layer)
            .unwrap()
            .advanced
            .as_ref()
            .unwrap()
            .recipe
            .source_id
            .clone()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workflow(WorkflowKind::Source, window, cx)
        })
    });
    draw(cx);
    click(cx, "workflow-duplicate");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let (count, all_linked) = view.update(cx, |view, _| {
        (
            view.editor.document.layers.len(),
            view.editor
                .document
                .layers
                .iter()
                .all(|l| l.advanced.as_ref().unwrap().recipe.source_id == source_id),
        )
    });
    assert_eq!(count, 2);
    assert!(all_linked);
    view.update(cx, |view, cx| {
        assert!(view.editor.undo());
        view.changed(cx);
    });
    assert_eq!(
        view.update(cx, |view, _| view.editor.document.layers.len()),
        1
    );
    let package = tmp.path().join("workflow.omuse");
    let document = view.update(cx, |view, _| view.editor.document.clone());
    omuse::document::save(&document, &package).unwrap();
    let reopened = omuse::document::open(&package).unwrap();
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .recipe
            .source_id,
        source_id
    );
}

fn open_workflow(view: &Entity<EditorView>, cx: &mut VisualTestContext, kind: WorkflowKind) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_workflow(kind, window, cx)));
    draw(cx);
}
fn set_path(view: &Entity<EditorView>, cx: &mut VisualTestContext, path: &std::path::Path) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.path_input.update(cx, |input, cx| {
                input.set_value(path.to_string_lossy(), window, cx)
            })
        })
    });
}

#[gpui_kit::test]
fn record_save_load_apply_recipe_collision_and_undo(cx: &mut TestAppContext) {
    let (view, cx, tmp) = setup(cx);
    open_workflow(&view, cx, WorkflowKind::Automation);
    click(cx, "workflow-record");
    assert!(view.update(cx, |v, _| v.macro_recording));
    click(cx, "inspector-tab-develop");
    cx.update(|window, cx| {
        view.update(cx, |v, cx| v.command("brighter", window, cx));
    });
    view.update(cx, |v, _| {
        assert!(v.macro_recording);
        assert_eq!(v.recorded_recipe.steps.len(), 1);
    });
    open_workflow(&view, cx, WorkflowKind::Automation);
    click(cx, "workflow-record");
    assert!(!view.update(cx, |v, _| v.macro_recording));
    let recipe_path = tmp.path().join("bright.recipe.json");
    open_workflow(&view, cx, WorkflowKind::Automation);
    set_path(&view, cx, &recipe_path);
    click(cx, "workflow-save-recipe");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let saved = std::fs::read(&recipe_path).unwrap();
    view.update(cx, |v, _| v.recorded_recipe = Recipe::default());
    open_workflow(&view, cx, WorkflowKind::Automation);
    set_path(&view, cx, &recipe_path);
    click(cx, "workflow-load-recipe");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    assert_eq!(view.update(cx, |v, _| v.recorded_recipe.steps.len()), 1);
    open_workflow(&view, cx, WorkflowKind::Automation);
    click(cx, "workflow-apply-recipe");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert_eq!(v.editor.document.layers.len(), 2);
        assert_eq!(
            v.editor
                .document
                .layers
                .iter()
                .filter(|layer| !layer.visible)
                .count(),
            1
        );
        assert!(v.editor.undo());
        assert_eq!(v.editor.document.layers.len(), 1);
    });
    open_workflow(&view, cx, WorkflowKind::Automation);
    set_path(&view, cx, &recipe_path);
    click(cx, "workflow-save-recipe");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        std::fs::read(&recipe_path).unwrap(),
        saved,
        "collision must preserve existing recipe"
    );
    assert!(view.update(cx, |v, _| {
        v.status.contains("existing recipes are preserved")
    }));
}

#[gpui_kit::test]
fn colour_proof_is_display_only_and_working_space_is_persisted(cx: &mut TestAppContext) {
    let (view, cx, tmp) = setup(cx);
    let (revision, undo, pixels) = view.update(cx, |v, _| {
        (v.editor.revision(), v.editor.undo_depth(), v.pixels.clone())
    });
    open_workflow(&view, cx, WorkflowKind::Colour);
    view.update(cx, |v, cx| {
        assert_eq!(v.detail_inputs[1].read(cx).value().as_ref(), "0");
        assert_eq!(v.detail_inputs[2].read(cx).value().as_ref(), "0");
    });
    click(cx, "workflow-proof");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert!(v.proof_settings.enabled);
        assert_eq!(v.editor.revision(), revision);
        assert_eq!(v.editor.undo_depth(), undo);
        assert_eq!(v.pixels, pixels);
    });
    open_workflow(&view, cx, WorkflowKind::Colour);
    click(cx, "workflow-linear");
    click(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let package = tmp.path().join("linear.omuse");
    let document = view.update(cx, |v, _| {
        let layer = v
            .editor
            .document
            .find_layer(&v.editor.active_layer)
            .unwrap();
        assert_eq!(
            layer.advanced.as_ref().unwrap().recipe.working_space,
            WorkingSpace::LinearSrgb
        );
        v.editor.document.clone()
    });
    omuse::document::save(&document, &package).unwrap();
    let reopened = omuse::document::open(&package).unwrap();
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .recipe
            .working_space,
        WorkingSpace::LinearSrgb
    );
}

#[gpui_kit::test]
fn pending_source_import_is_cancelled_by_dirty_close_and_focus_returns(cx: &mut TestAppContext) {
    let (view, cx, tmp) = setup(cx);
    let source = tmp.path().join("large.png");
    image::RgbaImage::from_fn(1024, 1024, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
    })
    .save(&source)
    .unwrap();
    view.update(cx, |v, _| v.editor.mark_unsaved());
    open_workflow(&view, cx, WorkflowKind::Source);
    set_path(&view, cx, &source);
    // Start inside one UI update so the close event observes the pending job;
    // simulate_click is allowed to advance the background executor.
    let token = cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.run_workflow(true, window, cx);
            assert!(v.busy);
            v.workflow_draft.as_ref().unwrap().cancel.clone()
        })
    });
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    draw(cx);
    view.update(cx, |v, _| {
        assert_eq!(v.dialog, Dialog::Unsaved);
        assert!(v.workflow_draft.is_none());
        assert!(!v.busy);
        assert!(token.load(Ordering::Relaxed));
        assert!(v.editor.document.layers[0].advanced.is_none());
        assert_eq!(v.editor.undo_depth(), 0);
    });
    click(cx, "cancel-unsaved");
    cx.update(|window, cx| {
        let v = view.read(cx);
        assert_eq!(v.dialog, Dialog::None);
        assert!(v.focus.is_focused(window));
    });
}

#[test]
fn merge_preload_cancellation_canvas_expansion_and_shift_validation() {
    let folder = tempfile::tempdir().unwrap();
    let cancelled = AtomicBool::new(true);
    let result = merge_folder(
        Action::Focus,
        folder.path(),
        std::path::Path::new(""),
        "64",
        &cancelled,
        CancellationToken::new(),
        (800, 600),
    );
    assert!(result.err().unwrap().to_string().contains("cancelled"));
    assert_eq!(expanded_canvas((800, 600), (320, 900)), (800, 900));
    assert_eq!(expanded_canvas((800, 600), (320, 240)), (800, 600));
    assert!(parse_max_shift("sixty-four").is_err());
    assert_eq!(parse_max_shift(" 128 ").unwrap(), 128);
}

#[test]
fn cancelled_precision_export_preserves_existing_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let destination = tmp.path().join("existing.png");
    std::fs::write(&destination, b"preserve-existing").unwrap();
    let document = Document::new(8, 8);
    let cancel = AtomicBool::new(true);
    assert!(omuse::raster::export16_cancellable(&document, &destination, &cancel).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"preserve-existing");
}

#[gpui_kit::test]
fn all_three_merge_actions_import_pixels_and_commit_one_undo(cx: &mut TestAppContext) {
    let (view, cx, tmp) = setup(cx);
    let folder = tmp.path().join("merge-inputs");
    std::fs::create_dir(&folder).unwrap();
    let pixels = image::RgbaImage::from_fn(16, 12, |x, y| {
        image::Rgba([
            20 + (x * 11) as u8,
            30 + (y * 15) as u8,
            80 + ((x * y) % 90) as u8,
            255,
        ])
    });
    for name in ["a.png", "b.png"] {
        pixels.save(folder.join(name)).unwrap();
    }
    for action in ["workflow-focus", "workflow-hdr", "workflow-panorama"] {
        open_workflow(&view, cx, WorkflowKind::Merge);
        view.update(cx, |v, cx| {
            assert_eq!(v.detail_inputs[0].read(cx).value().as_ref(), "0,0");
            assert_eq!(v.detail_inputs[1].read(cx).value().as_ref(), "64");
        });
        set_path(&view, cx, &folder);
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.detail_inputs[1].update(cx, |input, cx| input.set_value("0", window, cx))
            })
        });
        draw(cx);
        click(cx, action);
        click(cx, "confirm-dialog");
        cx.run_until_parked();
        draw(cx);
        view.update(cx, |v, _| {
            assert_eq!(v.dialog, Dialog::None, "{action}: {}", v.status);
            assert!(!v.busy);
            assert_eq!(
                (v.editor.document.width, v.editor.document.height),
                (16, 12)
            );
            assert_eq!(v.editor.document.layers.len(), 2);
            let state = v
                .editor
                .document
                .find_layer(&v.editor.active_layer)
                .unwrap()
                .advanced
                .as_ref()
                .unwrap();
            let result = state.proxy().unwrap();
            assert_eq!(result.dimensions(), pixels.dimensions());
            for (a, b) in result.pixels().zip(pixels.pixels()) {
                for c in 0..4 {
                    assert!(a[c].abs_diff(b[c]) <= 1, "{action}: {a:?} vs {b:?}");
                }
            }
            assert!(v.editor.undo());
            assert_eq!(v.editor.document.layers.len(), 1);
        });
    }
}

#[gpui_kit::test]
fn batch_report_retains_failures_and_renders_a_bounded_detail_list(cx: &mut TestAppContext) {
    let (view, cx, _tmp) = setup(cx);
    let report = omuse::recipes::BatchReport {
        items: (0..22)
            .map(|index| omuse::recipes::BatchItem {
                input: std::path::PathBuf::from(format!("photo-{index}.png")),
                output: std::path::PathBuf::from(format!("photo-{index}.png.jpg")),
                error: (index == 3 || index == 21).then(|| "decoder rejected input".into()),
            })
            .collect(),
        cancelled: true,
    };
    view.update(cx, |view, _| {
        let (status, changed) = view
            .commit_workflow(workflow_ui::Outcome::Batch(report.clone()))
            .unwrap();
        assert_eq!(
            status,
            "Batch finished: 20 succeeded, 2 failed, 1 cancelled"
        );
        assert!(!changed);
        let retained = view.last_batch_report.as_ref().unwrap();
        assert_eq!(retained.items.len(), 22);
        assert_eq!(
            retained.items[3].error.as_deref(),
            Some("decoder rejected input")
        );
    });

    open_workflow(&view, cx, WorkflowKind::Automation);
    assert!(cx.debug_bounds("workflow-batch-report").is_some());
    assert!(cx.debug_bounds("workflow-batch-report-row-0").is_some());
    assert!(cx.debug_bounds("workflow-batch-report-row-20").is_none());
}
