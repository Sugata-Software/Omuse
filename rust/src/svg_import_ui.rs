//! Choose SVG raster dimensions before a guarded background open/import.
use super::*;
use anyhow::{Context as _, Result, ensure};
use std::sync::atomic::Ordering;

pub(super) struct SvgImportDraft {
    path: PathBuf,
    import: bool,
    intrinsic: Option<(f32, f32)>,
    source_stamp: Option<u64>,
    width: Entity<InputState>,
}

impl EditorView {
    pub(super) fn svg_source_stamp(path: &std::path::Path) -> Result<u64> {
        ensure!(
            std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()),
            "The SVG source is unavailable or is no longer a regular file. Cancel and reopen it."
        );
        omuse::save_guard::package_stamp(path)
            .context("The SVG source cannot be inspected. Cancel and reopen it.")
    }

    pub(super) fn begin_svg_import(
        &mut self,
        path: PathBuf,
        import: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.photo_io.is_some() {
            self.status = "The previous image operation is finishing. Try again shortly.".into();
            cx.notify();
            return;
        }
        let width = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe_in(
            &width,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } if this.dialog == Dialog::SvgImport => this.apply_svg_import(window, cx),
                _ => {}
            },
        )
        .detach();
        self.svg_import_draft = Some(SvgImportDraft {
            path: path.clone(),
            import,
            intrinsic: None,
            source_stamp: None,
            width,
        });
        self.dialog = Dialog::SvgImport;
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.modal_focus.focus(window, cx);
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        self.status = "Reading SVG dimensions…".into();
        cx.spawn_in(window, async move |view, cx| {
            let worker_cancel = cancel.clone();
            let result = cx.background_executor().spawn(async move {
                let result = (|| -> Result<_> {
                    ensure!(!worker_cancel.load(Ordering::Relaxed), "SVG import cancelled");
                    let stamp = Self::svg_source_stamp(&path)?;
                    let intrinsic = omuse::svg_import::intrinsic_size(&path)?;
                    ensure!(Self::svg_source_stamp(&path)? == stamp,
                        "The SVG source changed while reading. Reload dimensions to review it again.");
                    ensure!(!worker_cancel.load(Ordering::Relaxed), "SVG import cancelled");
                    Ok((intrinsic, stamp))
                })();
                result.map_err(|error| format!("{error:#}"))
            }).await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_photo_io(&cancel, identity) { cx.notify(); return; }
                match result {
                    Ok((intrinsic, stamp)) => {
                        if let Some(draft) = this.svg_import_draft.as_mut() {
                            draft.intrinsic = Some(intrinsic);
                            draft.source_stamp = Some(stamp);
                            // Large intrinsic artwork starts with an explicit
                            // suggested size shown in the form, never a silent
                            // crop or resize after clicking Import.
                            let scale = (4096. / intrinsic.0.max(intrinsic.1)).min(1.);
                            let width = (intrinsic.0 * scale).round().max(1.) as u32;
                            draft.width.update(cx, |state, cx| state.set_value(width.to_string(), window, cx));
                            this.status = "Choose the raster size. Aspect ratio stays locked; the original SVG is preserved.".into();
                        }
                    }
                    Err(error) => { this.status = format!("SVG import failed: {error}"); }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn svg_size(&self, cx: &Context<Self>) -> Result<(u32, u32)> {
        let draft = self
            .svg_import_draft
            .as_ref()
            .context("SVG import closed")?;
        let intrinsic = draft.intrinsic.context("Wait for SVG dimensions to load")?;
        let width = draft
            .width
            .read(cx)
            .value()
            .trim()
            .parse::<u32>()
            .context("Enter a whole-number width in pixels")?;
        ensure!(width > 0, "Width must be at least 1 pixel");
        let height = (f64::from(width) * f64::from(intrinsic.1) / f64::from(intrinsic.0))
            .round()
            .max(1.);
        ensure!(
            height.is_finite() && height <= f64::from(u32::MAX),
            "SVG height exceeds supported bounds"
        );
        let height = height as u32;
        omuse::svg_import::validate_size(width, height)?;
        Ok((width, height))
    }

    pub(super) fn apply_svg_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let size = match self.svg_size(cx) {
            Ok(size) => size,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let Some(draft) = self.svg_import_draft.as_ref() else {
            return;
        };
        let Some(stamp) = draft.source_stamp else {
            self.status = "Reload dimensions before importing the SVG.".into();
            cx.notify();
            return;
        };
        self.open_photo_background_sized(
            draft.path.clone(),
            draft.import,
            Some((size, stamp)),
            window,
            cx,
        );
    }

    pub(super) fn svg_import_body(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let Some(draft) = self.svg_import_draft.as_ref() else {
            return div().child("SVG import closed").into_any_element();
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .debug_selector(|| "svg-import-controls".into())
            .child(
                div()
                    .text_color(t.secondary)
                    .child(draft.path.file_name().map_or_else(
                        || "SVG artwork".into(),
                        |name| name.to_string_lossy().into_owned(),
                    )),
            )
            .child(
                button(
                    "svg-refresh-source",
                    "Reload dimensions",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(draft) = &this.svg_import_draft {
                        let path = draft.path.clone();
                        let import = draft.import;
                        this.begin_svg_import(path, import, window, cx);
                    }
                })),
            );
        if let Some(intrinsic) = draft.intrinsic {
            let mut presets = div().flex().flex_wrap().gap_2();
            for (index, (label, value)) in [
                ("Original", intrinsic.0),
                ("2×", intrinsic.0 * 2.),
                ("4×", intrinsic.0 * 4.),
                ("2048 px", 2048.),
            ]
            .into_iter()
            .enumerate()
            {
                presets = presets.child(
                    button(("svg-size", index), label, ButtonVariant::Outline, cx)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(draft) = &this.svg_import_draft {
                                draft.width.update(cx, |state, cx| {
                                    state.set_value(
                                        (value.round().max(1.) as u32).to_string(),
                                        window,
                                        cx,
                                    )
                                });
                            }
                            cx.notify();
                        })),
                );
            }
            body = body.child(div().text_sm().child(format!("Intrinsic size: {:.2} × {:.2} px", intrinsic.0, intrinsic.1)))
                .child(presets)
                .child(div().child("Width in pixels").child(if self.busy {
                    div().child(draft.width.read(cx).value().to_string()).into_any_element()
                } else {
                    input("svg-import-width", &draft.width, window, cx).into_any_element()
                }))
                .child(div().text_sm().child(match self.svg_size(cx) {
                    Ok((w, h)) => format!("Output: {w} × {h} px · {:.1} megapixels · Aspect ratio locked", f64::from(w) * f64::from(h) / 1_000_000.),
                    Err(error) => error.to_string(),
                }))
                .child(div().text_sm().text_color(t.secondary).child("Vector shapes and text become one raster layer at this size. The SVG file stays unchanged."));
        } else {
            body = body.child(div().text_sm().child(if self.busy {
                "Inspecting artwork…"
            } else {
                "Cancel to choose a different SVG file."
            }));
        }
        body.into_any_element()
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    #[gpui_kit::test]
    fn changed_svg_source_requires_new_dimensions_and_preserves_current_artwork(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("art.svg");
        std::fs::write(&path, "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='6'><rect width='8' height='6' fill='red'/></svg>").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(10, 10));
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.dialog = Dialog::None;
            view.refresh(cx);
            view
        });
        let before = view.read_with(cx, |view, _| view.pixels.clone());
        let revision = view.read_with(cx, |view, _| view.editor.revision());
        view.update_in(cx, |view, window, cx| {
            view.begin_svg_import(path.clone(), false, window, cx)
        });
        cx.run_until_parked();
        // Same path now refers to different dimensions. The old aspect ratio
        // must never be applied to these new bytes.
        let replacement = temp.path().join("replacement.svg");
        let new_source = "<svg xmlns='http://www.w3.org/2000/svg' width='6' height='8'><rect width='6' height='8' fill='blue'/></svg>";
        std::fs::write(&replacement, new_source).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        view.update_in(cx, |view, window, cx| view.apply_svg_import(window, cx));
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.dialog, Dialog::SvgImport);
            assert!(!view.busy);
            assert_eq!(view.editor.revision(), revision);
            assert_eq!(view.pixels, before);
            assert!(view.status.contains("source changed"));
            view.begin_svg_import(path.clone(), false, window, cx);
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            let draft = view.svg_import_draft.as_ref().unwrap();
            assert_eq!(draft.intrinsic, Some((6., 8.)));
            draft
                .width
                .update(cx, |input, cx| input.set_value("60", window, cx));
            view.apply_svg_import(window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert_eq!(
                (view.editor.document.width, view.editor.document.height),
                (60, 80)
            );
            assert_eq!(view.pixels.get_pixel(30, 40).0, [0, 0, 255, 255]);
            assert!(view.svg_import_draft.is_none());
        });
        assert_eq!(std::fs::read_to_string(path).unwrap(), new_source);
    }

    #[gpui_kit::test]
    fn svg_open_chooses_dimensions_and_never_edits_the_source(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("art.svg");
        let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='6'><rect width='8' height='6' fill='#ff8000'/></svg>";
        std::fs::write(&path, source).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.dialog = Dialog::Open;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(path.clone(), false, window, cx)
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert!(view.dialog == Dialog::SvgImport);
            assert!(!view.busy);
            let draft = view.svg_import_draft.as_ref().unwrap();
            assert_eq!(draft.intrinsic, Some((8., 6.)));
            draft
                .width
                .update(cx, |input, cx| input.set_value("80", window, cx));
            view.apply_svg_import(window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.busy);
            assert!(view.path.is_none());
            assert!(view.svg_import_draft.is_none());
            assert_eq!(
                (view.editor.document.width, view.editor.document.height),
                (80, 60)
            );
            assert_eq!(view.pixels.get_pixel(40, 30).0, [255, 128, 0, 255]);
            assert!(!view.import_notes.is_empty());
        });
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }

    #[gpui_kit::test]
    fn cancelled_svg_probe_cannot_replace_artwork(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("art.svg");
        std::fs::write(
            &path,
            "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='6'/>",
        )
        .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view
        });
        let before = view.read_with(cx, |view, _| view.pixels.clone());
        view.update_in(cx, |view, window, cx| {
            view.open_photo_background(path.clone(), false, window, cx);
            assert!(view.cancel_photo_io());
            view.dialog = Dialog::None;
            view.dialog_generation += 1;
            view.svg_import_draft = None;
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(view.photo_io.is_none());
            assert!(view.svg_import_draft.is_none());
            assert!(view.dialog == Dialog::None);
            assert_eq!(view.pixels, before);
        });
    }
}
