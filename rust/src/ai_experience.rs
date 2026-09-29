//! Task-first controls for the assistant. Choosing a task or a starter never
//! submits a request; the primary button and Ctrl+Enter share one dispatcher.
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AiTask {
    #[default]
    Design,
    Photo,
    Caption,
    Generate,
    Replace,
    Remove,
    Background,
    Expand,
}

impl AiTask {
    const ALL: [Self; 8] = [
        Self::Design,
        Self::Photo,
        Self::Caption,
        Self::Generate,
        Self::Replace,
        Self::Remove,
        Self::Background,
        Self::Expand,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Design => "Design & layout",
            Self::Photo => "Enhance photo",
            Self::Caption => "Caption & alt text",
            Self::Generate => "Generate image",
            Self::Replace => "Replace selection",
            Self::Remove => "Remove object",
            Self::Background => "New background",
            Self::Expand => "Expand canvas",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Design => "Create edit preview",
            Self::Photo => "Preview adjustments",
            Self::Caption => "Draft copy",
            Self::Generate => "Generate image",
            Self::Replace => "Preview replacement",
            Self::Remove => "Preview removal",
            Self::Background => "Preview background",
            Self::Expand => "Preview expansion",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::Design => {
                "Arrange editable text, shapes and layouts. Your original stays unchanged until you keep a preview."
            }
            Self::Photo => {
                "Tune the active photo layer with reversible tonal and colour adjustments. This changes the whole layer, not only a selection."
            }
            Self::Caption => {
                "Draft a caption and accurate alt text from this page. Review the copy before adding it to your content."
            }
            Self::Generate => {
                "Create a new image from a brief and optional references, then add it as a separate layer."
            }
            Self::Replace => {
                "Select the area to replace, then describe what should appear there. Pixels outside the selection stay protected."
            }
            Self::Remove => {
                "Select an object to remove. Omuse asks for a natural fill and protects the rest of the image."
            }
            Self::Background => {
                "Select the subject to keep, then describe a new background. The selected subject stays protected."
            }
            Self::Expand => {
                "Describe what should continue beyond the image. Choose the extra space on each edge below."
            }
        }
    }

    pub(super) fn capability(self) -> Capability {
        match self {
            Self::Design | Self::Photo | Self::Caption => Capability::AssistantStreaming,
            Self::Generate => Capability::ImageGeneration,
            _ => Capability::ImageEditing,
        }
    }

    pub(super) fn uses_assistant(self) -> bool {
        self.capability() == Capability::AssistantStreaming
    }

    fn starters(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Design => &[
                (
                    "Polish this layout",
                    "Improve this design's spacing, alignment and hierarchy. Preserve the wording, images and brand colours.",
                ),
                (
                    "Social post",
                    "Create an editable square social post with a strong headline, clear supporting copy and generous space. Use the existing content as the starting point.",
                ),
                (
                    "A fresh direction",
                    "Give this design a distinctive new visual direction while keeping its message. Use editable text and shapes.",
                ),
            ],
            Self::Photo => &[
                (
                    "Natural light",
                    "Brighten the active photo gently with exposure, brightness and contrast adjustments. Keep it natural and preserve the original pixels.",
                ),
                (
                    "Cinematic contrast",
                    "Give the active photo cinematic contrast with restrained saturation and balanced exposure. Use reversible adjustments.",
                ),
                (
                    "Clean product photo",
                    "Improve exposure, brightness, contrast and saturation on the active product photo. Keep the product's colours accurate and the treatment subtle.",
                ),
            ],
            Self::Caption => &[
                (
                    "Short & engaging",
                    "Write a concise, engaging caption and accurate alt text for this page. Keep the voice human and do not invent product claims.",
                ),
                (
                    "Launch announcement",
                    "Draft a clear launch caption using only facts in this page, plus useful alt text. Keep the call to action concise.",
                ),
            ],
            Self::Generate => &[
                (
                    "Editorial photograph",
                    "An editorial still life with sculptural forms, warm terracotta and cream tones, soft directional light and generous negative space. No text.",
                ),
                (
                    "Social backdrop",
                    "A refined abstract background for a social post: warm paper texture, muted teal, burnt orange accents, ample quiet space for a headline. No text.",
                ),
            ],
            Self::Replace => &[(
                "Match the scene",
                "Replace the selected area with a natural alternative that matches the scene's lighting, perspective and texture.",
            )],
            Self::Remove => &[(
                "Remove cleanly",
                "Remove the selected object and reconstruct a seamless, natural background.",
            )],
            Self::Background => &[(
                "Studio backdrop",
                "A warm neutral studio backdrop with soft directional lighting and a natural contact shadow. Keep the selected subject unchanged.",
            )],
            Self::Expand => &[(
                "Continue the scene",
                "Continue the scene naturally into the added space, matching its light, colours and perspective. No extra text or new focal subjects.",
            )],
        }
    }

    pub(super) fn from_result(proposal: &AiProposal) -> Self {
        if let Some(task) = proposal
            .provenance
            .get("omuseTask")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
        {
            return task;
        }
        match &proposal.intent {
            Some(ImageIntent::Generate) => Self::Generate,
            Some(ImageIntent::Replace) => Self::Replace,
            Some(ImageIntent::Background) => Self::Background,
            Some(ImageIntent::Expand { .. }) => Self::Expand,
            None => {
                if let Some(plan) = proposal
                    .plan_json
                    .as_deref()
                    .and_then(|json| CreativePlan::parse(json).ok())
                {
                    if !plan.operations.is_empty()
                        && plan
                            .operations
                            .iter()
                            .all(|op| matches!(op, CreativeOperation::AdjustPhoto { .. }))
                    {
                        return Self::Photo;
                    }
                    if !plan.operations.is_empty()
                        && plan
                            .operations
                            .iter()
                            .all(|op| matches!(op, CreativeOperation::SetContent { .. }))
                    {
                        return Self::Caption;
                    }
                }
                Self::Design
            }
        }
    }

    pub(super) fn validate_plan(
        self,
        plan: &CreativePlan,
        active_layer: &str,
    ) -> anyhow::Result<()> {
        if self == Self::Photo {
            anyhow::ensure!(
                !plan.operations.is_empty(),
                "No photo adjustments were returned. Your canvas is unchanged; try a more specific brief."
            );
        }
        if self == Self::Caption {
            anyhow::ensure!(
                plan.operations.len() == 1
                    && matches!(&plan.operations[0], CreativeOperation::SetContent { caption, alt_text } if !caption.trim().is_empty() && !alt_text.trim().is_empty()),
                "The provider did not return both a caption and alt text. Your artwork is unchanged; refine the brief and try again."
            );
        }
        for operation in &plan.operations {
            match self {
                Self::Photo => anyhow::ensure!(
                    matches!(operation, CreativeOperation::AdjustPhoto { layer_id, .. } if layer_id == active_layer),
                    "The photo assistant proposed changes outside the active photo layer. Nothing was applied; refine the brief or choose Design & layout."
                ),
                Self::Caption => anyhow::ensure!(
                    matches!(operation, CreativeOperation::SetContent { .. }),
                    "The caption assistant proposed artwork changes. Nothing was applied; ask for caption and alt text only."
                ),
                _ => {}
            }
        }
        Ok(())
    }
}

impl EditorView {
    fn ai_task_note(&self) -> Option<String> {
        if self.ai_local_only() {
            return Some("Local-only is on. Turn it off in Connections to send a request.".into());
        }
        if self.ai.task == AiTask::Photo
            && let Err(error) = omuse::creative_commands::validate_photo_target(
                &self.editor.document,
                &self.editor.active_layer,
            )
        {
            return Some(format!("Choose an unlocked photo layer: {error}"));
        }
        if let Err(note) = self.ai_route_for_task(self.ai.task) {
            return Some(note);
        }
        if let Some(note) = self.ai_sequence_note() {
            return Some(note);
        }
        match self.ai.task {
            AiTask::Replace | AiTask::Remove if !self.ai_has_edit_selection() => {
                Some("Make a selection on the canvas first. Only that area will be changed.".into())
            }
            AiTask::Background if !self.ai_has_background_selection() => Some(
                "Select the subject you want to keep first, leaving room for a new background."
                    .into(),
            ),
            _ => None,
        }
    }

    pub(super) fn submit_ai_task(&mut self, cx: &mut Context<Self>) {
        if let Some(note) = self.ai_task_note() {
            self.ai.activity = note;
            cx.notify();
            return;
        }
        if self.ai.prompt.read(cx).value().trim().is_empty() {
            self.ai.activity = "Describe what you want, or choose a starting point above.".into();
            cx.notify();
            return;
        }
        if let Err(error) = self.begin_ai_workflow(cx) {
            self.ai.activity = error.to_string();
            cx.notify();
            return;
        }
        let first = self
            .ai_route_for_task(self.ai.task)
            .is_ok_and(|route| route.first_use);
        let submission = if first {
            CapabilitySubmission::FirstUseQualification
        } else {
            CapabilitySubmission::Verified
        };
        match self.ai.task {
            AiTask::Design => self.start_ai_job_with_focus_for_submission(JobOperation::Assistant, None, submission, cx),
            AiTask::Photo => self.start_ai_job_with_focus_for_submission(JobOperation::Assistant,
                Some("Enhance the active photo layer using only adjust_photo operations. Use the current canvas preview to assess tone and colour. Preserve the original pixels, content, masks, geometry and all other layers. Choose restrained values and describe the adjustments in the summary."), submission, cx),
            AiTask::Caption => self.start_ai_job_with_focus_for_submission(JobOperation::Assistant,
                Some("Draft a useful social caption and accurate image description for this page. Use the supplied canvas preview and editable content, retain the meaning of existing user copy, and avoid guessing facts or hidden details. Return only a set_content operation for review; do not change the artwork."), submission, cx),
            AiTask::Remove => self.start_ai_remove_job(first, cx),
            AiTask::Expand => if let Err(error) = self.start_ai_expand(first, cx) { self.ai.activity = error.to_string(); cx.notify(); },
            task => {
                let intent = match task { AiTask::Generate => ImageIntent::Generate, AiTask::Replace => ImageIntent::Replace, AiTask::Background => ImageIntent::Background, _ => unreachable!() };
                if first { self.start_first_use_ai_image_job(intent, cx) } else { self.start_ai_image_job(intent, cx) }
            }
        }
        if !self.ai_busy() {
            self.stop_ai_workflow();
        }
    }

    pub(super) fn ai_task_controls(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let busy = self.ai_busy();
        let mut tasks = div().flex().flex_wrap().gap_1();
        for task in AiTask::ALL {
            tasks = tasks.child(
                button(
                    SharedString::from(format!("ai-task-{task:?}")),
                    task.label(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(self.ai.task == task)
                .disabled(busy)
                .debug_selector(move || format!("ai-task-{task:?}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.ai.task = task;
                    this.ai.follow_up = None;
                    this.ai.activity =
                        "Describe the change you want. Nothing is sent until you submit.".into();
                    this.focus_ai_prompt(window, cx);
                    cx.notify();
                })),
            );
        }
        let mut section = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("What would you like to create?"),
            )
            .child(tasks)
            .child(label(self.ai.task.help(), cx));
        let mut starters = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label("STARTING POINTS · customise before sending", cx));
        for (index, &(name, brief)) in self.ai.task.starters().iter().enumerate() {
            starters = starters.child(
                button(
                    SharedString::from(format!("ai-starter-{index}")),
                    name,
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .justify_start()
                .disabled(busy)
                .debug_selector(move || format!("ai-starter-{index}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.ai
                        .prompt
                        .update(cx, |prompt, cx| prompt.set_value(brief, window, cx));
                    this.ai.follow_up = None;
                    this.ai.activity = "Brief added. Make it yours, then submit when ready.".into();
                    this.focus_ai_prompt(window, cx);
                    cx.notify();
                })),
            );
        }
        section = section.child(self.ai_workflow_controls(cx)).child(starters);
        if self.ai.task == AiTask::Expand {
            section = section
                .child(label(
                    "Add space in pixels · left / top / right / bottom",
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(input("ai-expand-left", &self.ai.expand_left, window, cx))
                        .child(input("ai-expand-top", &self.ai.expand_top, window, cx))
                        .child(input("ai-expand-right", &self.ai.expand_right, window, cx))
                        .child(input(
                            "ai-expand-bottom",
                            &self.ai.expand_bottom,
                            window,
                            cx,
                        )),
                );
        }
        if !self.ai.task.uses_assistant() {
            let mut choices = div().flex().gap_1();
            for count in [1u8, 2, 4] {
                choices = choices.child(
                    button(
                        SharedString::from(format!("ai-variations-{count}")),
                        SharedString::from(count.to_string()),
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .selected(self.ai.variation_count == count)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ai.variation_count = count;
                        cx.notify();
                    })),
                );
            }
            section = section
                .child(label(
                    format!(
                        "{} variation{} · each uses a separate request",
                        self.ai.variation_count,
                        if self.ai.variation_count == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ),
                    cx,
                ))
                .child(choices);
        }
        if self.ai.task == AiTask::Background && self.ai_has_background_selection() && !busy {
            section = section.child(self.render_product_controls(window, cx));
        }
        section
    }

    pub(super) fn ai_composer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let busy = self.ai_busy();
        let route = self.ai_route_for_task(self.ai.task);
        let note = self.ai_task_note();
        let first = route.as_ref().is_ok_and(|route| route.first_use);
        let mut composer = div()
            .id("ai-composer")
            .key_context("AiPrompt")
            .on_action(cx.listener(|this, _: &SubmitAiPrompt, _, cx| this.submit_ai_prompt(cx)))
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_t_1()
            .border_color(t.divider())
            .flex_shrink_0()
            .child(self.ai_route_button(cx))
            .child(
                gpui_omarchy::textarea("ai-prompt", &self.ai.prompt, window, cx)
                    .debug_selector(|| "ai-prompt".into())
                    .min_h(px(56.))
                    .max_h(px(88.)),
            );
        if let Some(progress) = self.ai_workflow_progress() {
            composer = composer.child(label(progress, cx));
        }
        let active = self.ai.running.is_some()
            || self.ai.preparing_work_dir.is_some()
            || self.ai.workflow.is_some();
        let primary = if active {
            button("ai-cancel", "Stop request", ButtonVariant::Secondary, cx)
                .debug_selector(|| "ai-cancel".into())
                .on_click(cx.listener(|this, _, _, cx| this.stop_ai_request(cx)))
        } else {
            button(
                "ai-plan",
                if busy {
                    "Preparing preview…"
                } else {
                    self.ai.task.action()
                },
                ButtonVariant::Primary,
                cx,
            )
            .debug_selector(|| "ai-plan".into())
            .disabled(busy || note.is_some())
            .on_click(cx.listener(|this, _, _, cx| this.submit_ai_prompt(cx)))
        };
        if active && let Some(started) = self.ai.request_started {
            composer = composer.child(label(
                format!(
                    "{}s elapsed · your canvas is unchanged",
                    started.elapsed().as_secs()
                ),
                cx,
            ));
        }
        composer = composer
            .child(
                div().flex().flex_wrap().gap_1().child(primary).child(
                    button(
                        "ai-add-reference",
                        "References…",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .disabled(busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.choose_ai_references(window, cx)),
                    ),
                ),
            )
            .child(label("Ctrl+Enter sends · Enter adds a new line", cx))
            .child(
                div()
                    .id("ai-activity")
                    .debug_selector(|| "ai-activity".into())
                    .min_h(px(18.))
                    .max_h(px(48.))
                    .overflow_y_scroll()
                    .child(label(self.ai.activity.clone(), cx)),
            );
        let mut details = div()
            .id("ai-composer-details")
            .debug_selector(|| "ai-composer-details".into())
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(56.))
            .overflow_y_scroll();
        if let Some(note) = note {
            details = details.child(label(note, cx));
        }
        if first {
            details = details.child(label("First use with this connection: your request will test support and may use subscription allowance.", cx));
        } else {
            details = details.child(label(
                "Uses your subscription. Every change is reviewed before Keep.",
                cx,
            ));
        }
        composer.child(details).into_any_element()
    }

    fn stop_ai_request(&mut self, cx: &mut Context<Self>) {
        self.stop_ai_workflow();
        if let Some(job) = &self.ai.running {
            job.handle.cancel();
        }
        if self.ai.preparing_work_dir.is_some() {
            self.ai.dispatch_generation = self.ai.dispatch_generation.wrapping_add(1);
            self.ai.preparing_image = false;
            if let Some(path) = self.ai.preparing_work_dir.take() {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        self.ai.variation_batch = None;
        self.ai.activity = "Stopping… remaining variations will not be sent. The active request may already have used allowance.".into();
        cx.notify();
    }
}
