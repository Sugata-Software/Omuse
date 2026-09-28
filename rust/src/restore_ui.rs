//! Local restoration controls with background, revision-bound previews.
use super::create_ui::{note, section};
use super::*;
use omuse::restoration::Settings;
use std::sync::atomic::{AtomicBool, Ordering};

struct Draft {
    document: Document,
    restored_layer_id: String,
    before: Arc<RenderImage>,
    after: Arc<RenderImage>,
    epoch: u64,
    revision: u64,
}
pub(super) struct RestoreState {
    denoise: Entity<InputState>,
    sharpen: Entity<InputState>,
    scale: u32,
    enlarge_page: bool,
    pending: Option<Arc<AtomicBool>>,
    generation: u64,
    draft: Option<Draft>,
}
impl RestoreState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        Self {
            denoise: cx.new(|cx| InputState::new(window, cx).default_value("0.35")),
            sharpen: cx.new(|cx| InputState::new(window, cx).default_value("0.25")),
            scale: 1,
            enlarge_page: false,
            pending: None,
            generation: 0,
            draft: None,
        }
    }
    pub(super) fn release(&mut self, cx: &mut App) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.pending.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(draft) = self.draft.take() {
            cx.drop_image(draft.before, None);
            cx.drop_image(draft.after, None);
        }
    }
}
impl Drop for RestoreState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.pending {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl EditorView {
    pub(super) fn create_restoration_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.create.restore;
        let busy = state.pending.is_some();
        let mut body=section("PHOTO RESTORATION · ON THIS COMPUTER",cx)
            .child(note("Denoise and sharpen the selected photo. Enlargement interpolates pixels; it does not recover missing detail. Originals remain in the project.",cx))
            .child(note("Denoise · 0–1",cx)).child(input("restore-denoise",&state.denoise,window,cx))
            .child(note("Sharpen · 0–2",cx)).child(input("restore-sharpen",&state.sharpen,window,cx));
        let mut sizes = div().flex().gap_1();
        for scale in [1, 2, 4] {
            sizes = sizes.child(
                button(
                    SharedString::from(format!("restore-scale-{scale}")),
                    SharedString::from(format!("{scale}× pixels")),
                    ButtonVariant::Secondary,
                    cx,
                )
                .flex_1()
                .selected(state.scale == scale)
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.create.restore.scale = scale;
                    cx.notify();
                })),
            );
        }
        body = body
            .child(sizes)
            .child(
                button(
                    "restore-page-size",
                    "Enlarge the page too",
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(state.enlarge_page)
                .disabled(busy || state.scale == 1)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.create.restore.enlarge_page = !this.create.restore.enlarge_page;
                    cx.notify();
                })),
            )
            .child(
                button(
                    "restore-preview",
                    "Preview restoration",
                    ButtonVariant::Primary,
                    cx,
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.start_restoration(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                button(
                    "restore-local-remove",
                    "Remove using nearby pixels…",
                    ButtonVariant::Secondary,
                    cx,
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_pro(advanced_ui::Kind::Remove, window, cx)
                })),
            );
        if busy {
            body = body.child(note("Preparing a local draft… You can keep editing.", cx));
        }
        if let Some(draft) = &state.draft {
            let stale =
                draft.epoch != self.create.epoch || draft.revision != self.editor.revision();
            for (label, image) in [
                ("Original", &draft.before),
                ("Restoration draft", &draft.after),
            ] {
                body = body.child(note(label, cx)).child(
                    gpui_kit::img(image.clone())
                        .w_full()
                        .h(px(160.))
                        .object_fit(gpui_kit::ObjectFit::Contain),
                );
            }
            body = body
                .child(note(
                    if stale {
                        "Artwork changed. Preview again before keeping."
                    } else {
                        "Compare the draft. Keeping it is one undo step."
                    },
                    cx,
                ))
                .child(
                    button(
                        "restore-keep",
                        "Keep restoration",
                        ButtonVariant::Primary,
                        cx,
                    )
                    .disabled(stale || busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.keep_restoration(cx);
                        this.create_error(result, cx);
                    })),
                );
        }
        if busy || state.draft.is_some() {
            body = body.child(
                button(
                    "restore-discard",
                    if busy {
                        "Cancel restoration"
                    } else {
                        "Discard draft"
                    },
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.create.restore.release(cx);
                    this.status =
                        "Restoration draft discarded; source artwork is unchanged.".into();
                    cx.notify();
                })),
            );
        }
        body.into_any_element()
    }
    fn start_restoration(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.restore.pending.is_none(),
            "A restoration is already running"
        );
        self.finish_interaction(cx);
        let settings = Settings {
            denoise: self
                .create
                .restore
                .denoise
                .read(cx)
                .value()
                .trim()
                .parse()?,
            sharpen: self
                .create
                .restore
                .sharpen
                .read(cx)
                .value()
                .trim()
                .parse()?,
            scale: self.create.restore.scale,
            enlarge_page: self.create.restore.enlarge_page,
        };
        self.create.restore.release(cx);
        let generation = self.create.restore.generation;
        let epoch = self.create.epoch;
        let revision = self.editor.revision();
        let source = self.editor.document.clone();
        let layer = self.editor.active_layer.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.create.restore.pending = Some(cancel.clone());
        self.status = "Preparing restoration on this computer…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| -> anyhow::Result<_> {
                        let prepared =
                            omuse::restoration::prepare(&source, &layer, settings, &cancel)?;
                        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Restoration cancelled");
                        // The reference compositor has no cancellation hook;
                        // `prepare` limits this draft path to 16 MP. Check at
                        // each bounded composite boundary so no stale result is
                        // promoted after the non-interruptible work completes.
                        let before =
                            image::imageops::thumbnail(&raster::composite(&source), 640, 640);
                        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Restoration cancelled");
                        let after = image::imageops::thumbnail(
                            &raster::composite(&prepared.document),
                            640,
                            640,
                        );
                        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Restoration cancelled");
                        Ok((prepared, before, after))
                    })();
                    result.map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update(cx, |this, cx| {
                if this.create.restore.generation != generation {
                    return;
                }
                this.create.restore.pending = None;
                match result {
                    Ok((prepared, before, after)) => {
                        this.create.restore.draft = Some(Draft {
                            document: prepared.document,
                            restored_layer_id: prepared.restored_layer_id,
                            before: render_image(&before),
                            after: render_image(&after),
                            epoch,
                            revision,
                        });
                        this.status = "Restoration ready to compare in Create → Assets.".into();
                    }
                    Err(error) => this.status = format!("Restoration: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        Ok(())
    }
    fn keep_restoration(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let draft = self
            .create
            .restore
            .draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Preview a restoration first"))?;
        anyhow::ensure!(
            draft.epoch == self.create.epoch && draft.revision == self.editor.revision(),
            "Artwork changed. Preview the restoration again."
        );
        let document = draft.document.clone();
        let restored_layer_id = draft.restored_layer_id.clone();
        self.editor.replace_document_transaction(document)?;
        anyhow::ensure!(
            self.editor.select_layer(&restored_layer_id),
            "Restoration draft lost its visible layer"
        );
        self.layer_selection.clear();
        self.layer_selection
            .click(restored_layer_id, SelectionAction::Replace);
        self.create.restore.release(cx);
        self.changed(cx);
        self.status="Restoration kept. The original photo is retained as a hidden layer; Undo restores the previous document.".into();
        Ok(())
    }
}
