//! Presentation-only comparison state. Retained pixels are bounded previews;
//! none of these controls dispatches a provider request or changes Keep eligibility.
use super::ai_review::{self, Difference, DifferenceSummary, ReviewMode};
use super::*;
use gpui_kit::Focusable;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreviewFrame {
    width: u32,
    height: u32,
    page: Option<String>,
}
impl PreviewFrame {
    fn comparable(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && (self.page.is_none() || other.page.is_none() || self.page == other.page)
    }
}

#[derive(Clone)]
pub(super) struct PinnedPreview {
    id: String,
    group_id: String,
    source: SourceIdentity,
    index: u8,
    frame: Option<PreviewFrame>,
    pixels: Arc<image::RgbaImage>,
    image: Arc<RenderImage>,
}
impl PinnedPreview {
    fn matches(&self, proposal: &AiProposal) -> bool {
        self.group_id == proposal.group_id
            && self.source == proposal.source
            && proposal.variation_total > 1
    }
}

#[derive(Default)]
pub(super) struct AiReviewState {
    pub mode: ReviewMode,
    pub show_all_changes: bool,
    pub show_full_response: bool,
    pub composer_expanded: std::cell::Cell<bool>,
    pub pin: Option<PinnedPreview>,
    pixels: Option<Arc<image::RgbaImage>>,
    frame: Option<PreviewFrame>,
    original: Option<DisplayDifference>,
    pinned: Option<DisplayDifference>,
}
struct DisplayDifference {
    image: Option<Arc<RenderImage>>,
    summary: Option<DifferenceSummary>,
    notice: Option<&'static str>,
}
impl DisplayDifference {
    fn from_result(result: Result<Difference, &'static str>) -> Self {
        match result {
            Ok(value) => Self {
                image: Some(render_image(&value.highlight)),
                summary: Some(value.summary),
                notice: None,
            },
            Err(notice) => Self {
                image: None,
                summary: None,
                notice: Some(notice),
            },
        }
    }
}

pub(super) struct PreparedReview {
    pin: Option<PinnedPreview>,
    frame: Option<PreviewFrame>,
    original: Option<Result<Difference, &'static str>>,
    pinned: Option<Result<Difference, &'static str>>,
}

fn proposal_frame(proposal: &AiProposal) -> Option<PreviewFrame> {
    if let Some(project) = &proposal.project {
        let page = project
            .page_summaries()
            .into_iter()
            .find(|page| page.id == project.active_page_id())?;
        return Some(PreviewFrame {
            width: page.width,
            height: page.height,
            page: Some(page.id),
        });
    }
    if let Some(document) = &proposal.document {
        return Some(PreviewFrame {
            width: document.width,
            height: document.height,
            page: proposal.source.page_id.clone(),
        });
    }
    let index = proposal
        .provenance
        .get("workflowPreviewAsset")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    proposal.assets.get(index).map(|asset| PreviewFrame {
        width: asset.width,
        height: asset.height,
        page: proposal.source.page_id.clone(),
    })
}
fn measure(
    before: &image::RgbaImage,
    after: &image::RgbaImage,
    before_frame: Option<&PreviewFrame>,
    after_frame: Option<&PreviewFrame>,
) -> Result<Difference, &'static str> {
    if !before_frame
        .zip(after_frame)
        .is_some_and(|(before, after)| before.comparable(after))
    {
        return Err(
            "The canvas size or active page differs. Inspect the images side by side; pixel highlighting is unavailable.",
        );
    }
    ai_review::preview_difference(before, after)
}

/// Called only inside the existing local result-preparation worker. At most
/// two 640×640 difference images and one pinned preview survive publication.
pub(super) fn prepare(prepared: &mut PreparedAiProposal, pin: Option<PinnedPreview>) {
    let proposal = &prepared.proposal;
    let pin = pin.filter(|pin| pin.matches(proposal));
    let frame = proposal_frame(proposal);
    let before_frame = proposal
        .source_document
        .as_ref()
        .map(|document| PreviewFrame {
            width: document.width,
            height: document.height,
            page: proposal.source.page_id.clone(),
        });
    let original = prepared
        .before_preview_pixels
        .as_ref()
        .zip(prepared.preview_pixels.as_ref())
        .map(|(before, after)| measure(before, after, before_frame.as_ref(), frame.as_ref()));
    let pinned = pin
        .as_ref()
        .filter(|pin| pin.id != proposal.id)
        .zip(prepared.preview_pixels.as_ref())
        .map(|(pin, after)| measure(&pin.pixels, after, pin.frame.as_ref(), frame.as_ref()));
    prepared.review = Some(PreparedReview {
        pin,
        frame,
        original,
        pinned,
    });
}

impl EditorView {
    pub(super) fn publish_ai_review(
        &mut self,
        prepared: &mut PreparedAiProposal,
        cx: &mut Context<Self>,
    ) {
        self.clear_ai_comparison(cx);
        if let Some(review) = prepared.review.take() {
            self.ai.review.pin = review.pin;
            self.ai.review.frame = review.frame;
            self.ai.review.original = review.original.map(DisplayDifference::from_result);
            self.ai.review.pinned = review.pinned.map(DisplayDifference::from_result);
        }
        if let Some(pixels) = prepared.preview_pixels.take() {
            prepared.proposal.preview = Some(render_image(&pixels));
            self.ai.review.pixels = Some(Arc::new(pixels));
        }
        if let Some(pixels) = prepared.before_preview_pixels.take() {
            prepared.proposal.before_preview = Some(render_image(&pixels));
        }
    }

    pub(super) fn clear_ai_comparison(&mut self, cx: &mut Context<Self>) {
        let previous = std::mem::take(&mut self.ai.review);
        for difference in [previous.original, previous.pinned].into_iter().flatten() {
            if let Some(image) = difference.image {
                cx.drop_image(image, None);
            }
        }
        if let Some(pin) = previous.pin {
            cx.drop_image(pin.image, None);
        }
    }

    fn pin_ai_review(&mut self, cx: &mut Context<Self>) {
        if self.ensure_ai_review_idle().is_err() {
            return;
        }
        let Some(proposal) = self
            .ai
            .result
            .as_ref()
            .filter(|proposal| proposal.variation_total > 1)
        else {
            return;
        };
        let Some((pixels, image)) = self
            .ai
            .review
            .pixels
            .as_ref()
            .zip(proposal.preview.as_ref())
        else {
            return;
        };
        let pin = PinnedPreview {
            id: proposal.id.clone(),
            group_id: proposal.group_id.clone(),
            source: proposal.source.clone(),
            index: proposal.variation_index,
            frame: self.ai.review.frame.clone(),
            pixels: pixels.clone(),
            image: image.clone(),
        };
        if let Some(old) = self.ai.review.pin.replace(pin) {
            cx.drop_image(old.image, None);
        }
        if let Some(difference) = self.ai.review.pinned.take().and_then(|value| value.image) {
            cx.drop_image(difference, None);
        }
        self.ai.review.mode = ReviewMode::Pair;
        self.ai.activity = "Comparison pinned. Choose another retained variation below; your artwork stays unchanged.".into();
        cx.notify();
    }

    /// Deterministic native smoke fixture, never a provider qualification.
    /// The CLI smoke runner supplies a separate private profile/history root.
    pub(in crate::ui) fn prepare_native_ai_review_inspection(
        &mut self,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            omuse::identity::env_var("OMUSE_NATIVE_PANEL").as_deref() == Ok("ai-review"),
            "AI comparison fixture is restricted to the explicit native smoke panel"
        );
        self.ensure_ai_review_idle()?;
        anyhow::ensure!(
            u64::from(self.editor.document.width) * u64::from(self.editor.document.height)
                <= 1_000_000,
            "Native comparison fixture canvas is too large"
        );
        self.clear_ai_result(cx);
        self.ai.connections_visible = false;
        self.ai.checking = true; // keep read-only smoke inspection out of connection discovery
        self.ai.route_picker_task = None;
        self.inspector_visible = true;
        self.inspector_tab = studio_ui::InspectorTab::Assistant;
        let project = self.content_snapshot()?;
        let source = self.ai_source_identity();
        let group_id = uuid::Uuid::new_v4().to_string();
        let mut history = HistoryStore::open(&self.ai.history_root)?;
        for (index, color) in [(1, [239, 227, 211, 255]), (2, [206, 230, 223, 255])] {
            let plan = CreativePlan {
                summary: "Local comparison fixture · alternate canvas colour".into(),
                operations: vec![CreativeOperation::SetBackground { color }],
            };
            let entry = history.persist(NewProposal {
                id: uuid::Uuid::new_v4().to_string(),
                group_id: group_id.clone(),
                variation_index: index,
                variation_total: 2,
                source: source.clone(),
                operation: Operation::Assistant,
                provider: ProviderId::CodexSubscription,
                prompt: "Local synthetic review fixture. No provider request was made.".into(),
                summary: plan.summary.clone(),
                plan_json: Some(serde_json::to_string(&plan)?),
                assets: vec![],
                context_assets: vec![],
                image_edit: false,
                provenance: serde_json::json!({"qualificationFixture": true}),
            })?;
            self.ai.history.push(entry.clone());
            let mut prepared = prepare_saved_ai_proposal(
                entry,
                self.ai.history_root.clone(),
                self.editor.document.clone(),
                project.clone(),
                None,
                true,
            )?;
            let pin = self.ai.review.pin.clone();
            prepare(&mut prepared, pin);
            self.publish_ai_review(&mut prepared, cx);
            self.ai.result = Some(prepared.proposal);
            if index == 1 {
                self.pin_ai_review(cx);
            }
        }
        self.ai.history.truncate(ai_history::MAX_ENTRIES);
        self.ai.history_loaded = true;
        anyhow::ensure!(
            self.ai
                .result
                .as_ref()
                .is_some_and(|proposal| self.ai_proposal_source_matches(proposal))
                && self.ai.review.pin.is_some()
                && self
                    .ai
                    .review
                    .pinned
                    .as_ref()
                    .is_some_and(|difference| difference.summary.is_some()),
            "Local native comparison was not fully prepared"
        );
        self.ai.activity = "Local comparison fixture · no provider request or billing. Keep and Undo use the normal editing path.".into();
        cx.notify();
        Ok(())
    }

    fn ai_review_can_collapse_composer(&self) -> bool {
        self.ai.result.is_some() && !self.ai_busy() && self.ai.workflow.is_none()
    }

    fn ai_review_composer_is_collapsed(&self, window: &Window, cx: &App) -> bool {
        // Once visible input has focus, keep its controls mounted until an
        // explicit collapse. Otherwise mouse-down can blur the input and
        // remove Send/Collapse before their mouse-up activation arrives.
        if self.ai.prompt.read(cx).focus_handle(cx).is_focused(window) {
            self.ai.review.composer_expanded.set(true);
        }
        self.ai_review_can_collapse_composer()
            && !self.ai.review.composer_expanded.get()
            && self.ai.route_picker_task.is_none()
    }

    fn collapse_ai_review_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ai_review_can_collapse_composer() {
            return;
        }
        self.ai.review.composer_expanded.set(false);
        self.ai.route_picker_task = None;
        // The hidden textarea must not retain the AiPrompt > Input key context.
        // Focus a stable review target; Tab can reach the visible disclosure.
        self.ai.review_footer_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn ai_review_composer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        if self.ai_review_composer_is_collapsed(window, cx) {
            let has_draft = !self.ai.prompt.read(cx).value().trim().is_empty();
            return div()
                .id("ai-review-footer")
                .debug_selector(|| "ai-review-footer".into())
                .track_focus(&self.ai.review_footer_focus)
                .key_context("AiReview")
                .flex()
                .flex_col()
                .flex_shrink_0()
                .min_w_0()
                .gap_1()
                .p_2()
                .bg(t.background)
                .border_t_1()
                .border_color(t.divider())
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .min_w_0()
                        .child(
                            button(
                                "ai-review-refine",
                                "Refine result",
                                ButtonVariant::Primary,
                                cx,
                            )
                            .debug_selector(|| "ai-review-refine".into())
                            .flex_1()
                            .min_w_0()
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.prepare_ai_follow_up(false, cx);
                                    this.focus_ai_prompt(window, cx);
                                },
                            )),
                        )
                        .child(
                            button(
                                "ai-review-compose",
                                "Open composer",
                                ButtonVariant::Secondary,
                                cx,
                            )
                            .debug_selector(|| "ai-review-compose".into())
                            .flex_1()
                            .min_w_0()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.focus_ai_prompt(window, cx)),
                            ),
                        ),
                )
                .child(
                    div()
                        .id("ai-activity")
                        .debug_selector(|| "ai-activity".into())
                        .min_h(px(14.))
                        .max_h(px(28.))
                        .overflow_y_scroll()
                        .child(
                            label(
                                if has_draft {
                                    format!("Draft kept · {}", self.ai.activity)
                                } else {
                                    self.ai.activity.clone()
                                },
                                cx,
                            )
                            .text_size(px(10.)),
                        ),
                )
                .into_any_element();
        }
        let composer = self.ai_composer(window, cx);
        if !self.ai_review_can_collapse_composer() {
            return composer;
        }
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .child(
                div()
                    .p_2()
                    .bg(t.background)
                    .border_t_1()
                    .border_color(t.divider())
                    .child(
                        button(
                            "ai-review-collapse",
                            "Collapse composer · keep draft",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .debug_selector(|| "ai-review-collapse".into())
                        .w_full()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.collapse_ai_review_composer(window, cx)
                        })),
                    ),
            )
            .child(composer)
            .into_any_element()
    }

    pub(super) fn ai_review_visuals(
        &self,
        proposal: &AiProposal,
        ai_busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = cx.omarchy().clone();
        let pin = self
            .ai
            .review
            .pin
            .as_ref()
            .filter(|pin| pin.matches(proposal));
        let comparing_pin = pin.filter(|pin| pin.id != proposal.id);
        let before = comparing_pin
            .map(|pin| &pin.image)
            .or(proposal.before_preview.as_ref());
        let before_label = comparing_pin
            .map(|pin| format!("Pinned · variation {}", pin.index))
            .unwrap_or_else(|| "Before".into());
        let after_label = if proposal.variation_total > 1 {
            format!("Variation {} · reviewing", proposal.variation_index)
        } else {
            "After · proposal".into()
        };
        let difference = if comparing_pin.is_some() {
            self.ai.review.pinned.as_ref()
        } else {
            self.ai.review.original.as_ref()
        };
        let has_pair = before.is_some() && proposal.preview.is_some();
        let mode = if has_pair {
            self.ai.review.mode
        } else {
            ReviewMode::After
        };
        let mut section = div().flex().flex_col().min_w_0().gap_2();
        if has_pair {
            for row in [
                [
                    (ReviewMode::Pair, "Compare"),
                    (ReviewMode::Changes, "Changes"),
                ],
                [
                    (ReviewMode::Before, "Left image"),
                    (ReviewMode::After, "Right image"),
                ],
            ] {
                let mut controls = div().flex().min_w_0().gap_1();
                for (choice, title) in row {
                    let id = match choice {
                        ReviewMode::Pair => "ai-preview-pair",
                        ReviewMode::Before => "ai-preview-before",
                        ReviewMode::After => "ai-preview-after",
                        ReviewMode::Changes => "ai-preview-changes",
                    };
                    controls = controls.child(
                        button(id, title, ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w_0()
                            .selected(mode == choice)
                            .disabled(
                                choice == ReviewMode::Changes
                                    && difference
                                        .and_then(|difference| difference.image.as_ref())
                                        .is_none(),
                            )
                            .debug_selector(move || id.into())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ai.review.mode = choice;
                                cx.notify();
                            })),
                    );
                }
                section = section.child(controls);
            }
        }
        match mode {
            ReviewMode::Pair => {
                let mut pair = div()
                    .flex()
                    .min_w_0()
                    .gap_2()
                    .debug_selector(|| "ai-comparison-pair".into());
                if let Some(image) = before {
                    pair = pair.child(preview_pane(&before_label, image.clone(), 172., cx));
                }
                if let Some(image) = &proposal.preview {
                    pair = pair.child(preview_pane(&after_label, image.clone(), 172., cx));
                }
                section = section.child(pair);
            }
            ReviewMode::Before => {
                if let Some(image) = before {
                    section = section.child(preview_pane(&before_label, image.clone(), 240., cx));
                }
            }
            ReviewMode::After => {
                if let Some(image) = &proposal.preview {
                    section = section.child(preview_pane(&after_label, image.clone(), 240., cx));
                }
            }
            ReviewMode::Changes => {
                if let Some(image) = difference.and_then(|difference| difference.image.as_ref()) {
                    section = section.child(preview_pane(
                        "Visible preview differences",
                        image.clone(),
                        240.,
                        cx,
                    ));
                }
            }
        }
        if let Some(difference) = difference {
            if let Some(summary) = &difference.summary {
                section = section.child(label(summary.description(), cx));
                if mode == ReviewMode::Changes {
                    section = section.child(label("Bright areas mark preview differences greater than 8/255, measured over light and dark backgrounds. Not a full-resolution or quality score.", cx));
                }
            } else if let Some(notice) = difference.notice {
                section = section.child(label(notice, cx));
            }
        } else if proposal.preview.is_some() && before.is_none() {
            section = section.child(label("The original preview is unavailable for this saved result. The current canvas is not used as its Before image.", cx));
        }
        if proposal.variation_total > 1 && self.ai.review.pixels.is_some() {
            let pinned_here = pin.is_some_and(|pin| pin.id == proposal.id);
            section = section.child(
                button(
                    "ai-pin-variation",
                    if pinned_here {
                        "Pinned · choose another variation"
                    } else {
                        "Pin this variation for comparison"
                    },
                    ButtonVariant::Secondary,
                    cx,
                )
                .disabled(ai_busy || pinned_here)
                .selected(pinned_here)
                .debug_selector(|| "ai-pin-variation".into())
                .on_click(cx.listener(|this, _, _, cx| this.pin_ai_review(cx))),
            );
        }
        if pin.is_some() {
            section = section.child(
                button(
                    "ai-unpin-variation",
                    "Compare with original",
                    ButtonVariant::Secondary,
                    cx,
                )
                .disabled(ai_busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(pin) = this.ai.review.pin.take() {
                        cx.drop_image(pin.image, None);
                    }
                    if let Some(image) = this.ai.review.pinned.take().and_then(|value| value.image)
                    {
                        cx.drop_image(image, None);
                    }
                    this.ai.review.mode = ReviewMode::Pair;
                    cx.notify();
                })),
            );
        }
        section.border_color(t.control_border())
    }
}

fn preview_pane(title: &str, image: Arc<RenderImage>, height: f32, cx: &App) -> Div {
    let t = cx.omarchy();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(label(title.to_owned(), cx))
        .child(
            div()
                .w_full()
                .h(px(height))
                .flex_shrink_0()
                .overflow_hidden()
                .rounded(px(6.))
                .border_1()
                .border_color(t.control_border())
                .bg(t.inset)
                .child(
                    gpui_kit::img(image)
                        .size_full()
                        .object_fit(gpui_kit::ObjectFit::Contain),
                ),
        )
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Focusable, TestAppContext};

    #[test]
    fn resized_canvas_and_changed_page_do_not_compare_as_matching_thumbnails() {
        let pixels = image::RgbaImage::new(64, 64);
        let original = PreviewFrame {
            width: 1000,
            height: 1000,
            page: Some("a".into()),
        };
        let resized = PreviewFrame {
            width: 2000,
            height: 2000,
            page: Some("a".into()),
        };
        let another_page = PreviewFrame {
            width: 1000,
            height: 1000,
            page: Some("b".into()),
        };
        assert!(measure(&pixels, &pixels, Some(&original), Some(&resized)).is_err());
        assert!(measure(&pixels, &pixels, Some(&original), Some(&another_page)).is_err());
        assert!(measure(&pixels, &pixels, None, Some(&original)).is_err());
    }

    #[gpui_kit::test]
    fn retained_comparison_stays_local_and_cannot_make_a_stale_result_applicable(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temporary = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temporary.path().join("recovery"));
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(40, 30));
            view.ai.checking = true;
            view.ai.history_root = temporary.path().join("history");
            view.inspector_tab = studio_ui::InspectorTab::Assistant;
            view.inspector_visible = true;
            view.refresh(cx);
            view
        });
        let ids = view.update_in(cx, |view, _, cx| {
            let mut store = HistoryStore::open(&view.ai.history_root).unwrap();
            let mut ids = Vec::new();
            let group_id = uuid::Uuid::new_v4().to_string();
            for index in 1..=2 {
                let path = temporary.path().join(format!("variation-{index}.png"));
                image::RgbaImage::from_pixel(12, 8, image::Rgba([index * 70, 20, 30, 255]))
                    .save(&path)
                    .unwrap();
                let byte_len = std::fs::metadata(&path).unwrap().len();
                let entry = store
                    .persist(NewProposal {
                        id: uuid::Uuid::new_v4().to_string(),
                        group_id: group_id.clone(),
                        variation_index: index,
                        variation_total: 2,
                        source: view.ai_source_identity(),
                        operation: Operation::GenerateImage,
                        provider: ProviderId::CodexSubscription,
                        prompt: "Local retained fixture".into(),
                        summary: format!("Fixture {index}"),
                        plan_json: None,
                        assets: vec![ai::ResultAsset {
                            path,
                            media_type: "image/png".into(),
                            width: 12,
                            height: 8,
                            byte_len,
                            provider_item_id: None,
                        }],
                        context_assets: vec![],
                        image_edit: false,
                        provenance: serde_json::json!({"intent": ImageIntent::Generate}),
                    })
                    .unwrap();
                ids.push(entry.id.clone());
                view.ai.history.push(entry);
            }
            view.select_ai_history(&ids[0], cx);
            ids
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            let proposal = view.ai.result.as_ref().unwrap();
            assert_eq!(proposal.id, ids[0]);
            assert_eq!(proposal.intent, Some(ImageIntent::Generate));
            assert!(
                proposal.document.is_some(),
                "saved image preparation failed: {:?}",
                proposal.error
            );
            assert_eq!(view.ai.review.mode, ReviewMode::Pair);
            assert!(view.ai.review.original.as_ref().unwrap().summary.is_some());
            view.ai.prompt.read(cx).focus_handle(cx).focus(window, cx);
            view.pin_ai_review(cx);
            assert!(view.ai.prompt.read(cx).focus_handle(cx).is_focused(window));
            view.select_ai_history(&ids[1], cx);
            assert!(view.ai_busy());
        });
        cx.run_until_parked();
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-comparison-pair").is_some());
        view.update_in(cx, |view, _, cx| {
            assert_eq!(view.ai.result.as_ref().unwrap().id, ids[1]);
            assert_eq!(view.ai.review.pin.as_ref().unwrap().id, ids[0]);
            assert!(
                view.ai
                    .review
                    .pinned
                    .as_ref()
                    .unwrap()
                    .summary
                    .as_ref()
                    .unwrap()
                    .changed_pixels
                    > 0
            );
            assert!(view.ai.running.is_none());
            assert!(!view.ai_busy());
            assert_eq!(view.editor.undo_depth(), 0);
            let proposal = view.ai.result.as_ref().unwrap();
            assert!(view.ai_proposal_source_matches(proposal));
            let mut pin = view.ai.review.pin.clone().unwrap();
            pin.source.session_id = "another-window".into();
            assert!(!pin.matches(proposal));
            pin.source = proposal.source.clone();
            pin.group_id = "another-batch".into();
            assert!(!pin.matches(proposal));
            view.editor.add_layer("Newer user edit");
            let revision = view.editor.revision();
            view.ai.review.mode = ReviewMode::Changes;
            view.apply_ai_plan(cx);
            assert_eq!(view.editor.revision(), revision);
            assert!(view.ai.activity.contains("canvas changed"));
            assert!(view.ai.result.is_some());
            assert!(view.ai.review.pin.is_some());
            view.clear_ai_result(cx);
            assert!(view.ai.review.pin.is_none());
            assert!(view.ai.review.pixels.is_none());
            assert!(view.ai.review.original.is_none());
            assert!(view.ai.review.pinned.is_none());
        });
    }

    #[gpui_kit::test]
    fn compact_review_preserves_draft_refinement_focus_and_busy_controls(cx: &mut TestAppContext) {
        use gpui_kit::Modifiers;
        cx.update(crate::init_test_theme);
        cx.update(bind_ai_keys);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(40, 30));
            view.ai.checking = true;
            view.inspector_visible = true;
            view.inspector_tab = studio_ui::InspectorTab::Assistant;
            view.ai.prompt.update(cx, |prompt, cx| {
                prompt.set_value("Keep my draft", window, cx)
            });
            view.ai.references = vec![PathBuf::from("retained-reference.png")];
            let mut candidate = view.editor.document.clone();
            candidate.background = [60, 80, 90, 255];
            view.ai.result = Some(super::super::tests::assistant_proposal(&view, candidate));
            view.refresh(cx);
            view.focus.focus(window, cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let compact_body = cx.debug_bounds("ai-body-viewport").unwrap();
        let inspector = cx.debug_bounds("ai-inspector").unwrap();
        let footer = cx
            .debug_bounds("ai-review-footer")
            .expect("completed review gets compact footer");
        assert!(footer.bottom_right().y <= inspector.bottom_right().y);
        assert!(footer.size.height < px(90.));
        assert!(cx.debug_bounds("ai-prompt").is_none());
        assert!(cx.debug_bounds("ai-connections").is_some());
        let prompt_id = view.read_with(cx, |view, _| view.ai.prompt.entity_id());
        // Connections uses all available height and returning preserves the
        // deliberate compact review state, including its unsent draft.
        let connections = cx.debug_bounds("ai-connections").unwrap();
        cx.simulate_click(connections.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-review-footer").is_none());
        assert!(cx.debug_bounds("ai-prompt").is_none());
        let connection_body = cx.debug_bounds("ai-body-viewport").unwrap();
        assert!(
            connection_body.size.height >= compact_body.size.height + footer.size.height - px(2.)
        );
        let back = cx.debug_bounds("ai-connections").unwrap();
        cx.simulate_click(back.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-review-footer").is_some());
        assert!(cx.debug_bounds("ai-prompt").is_none());
        let open = cx.debug_bounds("ai-review-compose").unwrap();
        cx.simulate_click(open.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-review-footer").is_none());
        assert!(cx.debug_bounds("ai-prompt").is_some());
        assert!(
            cx.debug_bounds("ai-body-viewport").unwrap().size.height < compact_body.size.height
        );
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.ai.prompt.entity_id(), prompt_id);
            assert_eq!(view.ai.prompt.read(cx).value(), "Keep my draft");
            assert!(view.ai.prompt.read(cx).focus_handle(cx).is_focused(window));
            // Receiving an idle result must not hide a prompt being edited.
            view.ai.review.composer_expanded.set(false);
            assert!(!view.ai_review_composer_is_collapsed(window, cx));
        });
        cx.simulate_input(" · refine colour");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let collapse = cx.debug_bounds("ai-review-collapse").unwrap();
        cx.simulate_click(collapse.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-prompt").is_none());
        let activity = view.read_with(cx, |view, _| view.ai.activity.clone());
        cx.simulate_keystrokes("ctrl-enter");
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.ai.activity, activity, "hidden draft must not submit");
            assert!(!view.ai_busy());
            assert!(view.ai.review_footer_focus.is_focused(window));
            assert!(!view.ai.prompt.read(cx).focus_handle(cx).is_focused(window));
            assert!(view.ai.prompt.read(cx).value().contains("refine colour"));
            assert_eq!(
                view.ai.references,
                vec![PathBuf::from("retained-reference.png")]
            );
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let refine = cx.debug_bounds("ai-review-refine").unwrap();
        cx.simulate_click(refine.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update_in(cx, |view, window, cx| {
            assert!(view.ai.follow_up.is_some());
            assert!(view.ai.review.composer_expanded.get());
            assert!(view.ai.prompt.read(cx).focus_handle(cx).is_focused(window));
            assert!(view.ai.prompt.read(cx).value().contains("refine colour"));
            assert_eq!(view.ai.prompt.entity_id(), prompt_id);
            view.collapse_ai_review_composer(window, cx);
            view.ai.preparing_work_dir = Some(PathBuf::from("not-executed-fixture-workspace"));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ai-review-footer").is_none());
        assert!(cx.debug_bounds("ai-review-collapse").is_none());
        assert!(cx.debug_bounds("ai-prompt").is_some());
        assert!(
            cx.debug_bounds("ai-cancel").is_some(),
            "active request Stop must remain visible"
        );
        view.update_in(cx, |view, _, _| view.ai.preparing_work_dir = None);
    }

    #[gpui_kit::test]
    fn superseded_history_comparison_cannot_republish_its_pin(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, _, cx| {
            view.ai.preparing_image = true;
            let generation = view.ai.preparation_generation;
            view.clear_ai_result(cx);
            view.ai.preparing_image = true; // a newer task owns busy state
            assert!(!view.complete_ai_result_preparation(generation));
            assert!(view.ai.preparing_image);
            assert!(view.ai.review.pin.is_none());
            assert!(view.ai.result.is_none());
        });
    }
}
