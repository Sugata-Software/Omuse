//! Routing regressions that need the desktop view rather than the pure byte parser.

use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use std::{path::PathBuf, sync::Arc};

fn status(
    provider: ProviderId,
    connection: ConnectionState,
    capabilities: &[Capability],
) -> ProviderStatus {
    ProviderStatus {
        provider,
        display_name: provider.display_name(),
        connection,
        billing: ai::BillingMode::SubscriptionAllowance,
        version: Some("fixture".into()),
        capabilities: capabilities
            .iter()
            .copied()
            .map(|capability| ai::CapabilityStatus {
                capability,
                evidence: EvidenceLevel::Verified,
                detail: "fixture".into(),
            })
            .collect(),
        allowance: None,
        detail: "fixture".into(),
        client: Some(ai::ValidatedClient::fixture(
            provider,
            PathBuf::from(format!("/fixture/{provider:?}")),
        )),
    }
}

fn ready_routes() -> Vec<ProviderStatus> {
    vec![
        status(
            ProviderId::ClaudeCode,
            ConnectionState::Ready,
            &[Capability::AssistantStreaming],
        ),
        status(
            ProviderId::CodexSubscription,
            ConnectionState::Ready,
            &[
                Capability::AssistantStreaming,
                Capability::ImageGeneration,
                Capability::ImageEditing,
            ],
        ),
    ]
}

fn proposal(view: &EditorView) -> AiProposal {
    AiProposal {
        workspace: None,
        id: "review-kept".into(),
        group_id: "review-group".into(),
        source: view.ai_source_identity(),
        source_document: Some(view.editor.document.clone()),
        provider: ProviderId::CodexSubscription,
        operation: Operation::Assistant,
        intent: None,
        prompt: "original result prompt".into(),
        summary: "original result summary".into(),
        plan_json: None,
        document: Some(view.editor.document.clone()),
        project: None,
        assets: vec![],
        context_assets: vec![],
        variation_index: 1,
        variation_total: 1,
        selection: None,
        product_presentation: Default::default(),
        image_edit: false,
        provenance: serde_json::json!({"fixture": true}),
        before_preview: None,
        preview: None,
        error: None,
    }
}

fn setup_view(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    cx.update(crate::init_test_theme);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        view.dialog = Dialog::None;
        view.editor = Editor::new(Document::new(40, 30));
        view.inspector_tab = studio_ui::InspectorTab::Assistant;
        view.inspector_visible = true;
        view.ai.checking = true;
        view.ai.routing = RoutingPreferences::default();
        view.ai.routing_error = None;
        view.ai.providers = ready_routes();
        view.refresh(cx);
        view
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be rendered"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

fn reveal(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    let viewport = cx
        .debug_bounds("ai-body-viewport")
        .expect("AI body viewport");
    let visible = |bounds: Bounds<Pixels>| {
        bounds.origin.x >= viewport.origin.x
            && bounds.origin.y >= viewport.origin.y
            && bounds.bottom_right().x <= viewport.bottom_right().x
            && bounds.bottom_right().y <= viewport.bottom_right().y
    };
    if let Some(bounds) = cx.debug_bounds(selector).filter(|bounds| visible(*bounds)) {
        return bounds;
    }
    for _ in 0..24 {
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-80.))),
            modifiers: Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        draw(cx);
        if let Some(bounds) = cx.debug_bounds(selector).filter(|bounds| visible(*bounds)) {
            return bounds;
        }
    }
    panic!("{selector} must be reachable by scrolling");
}

fn reveal_and_click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = reveal(cx, selector);
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

#[gpui_kit::test]
fn choosing_a_task_pin_preserves_the_entire_current_draft(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.task = AiTask::Design;
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("Keep this exact brief", window, cx)
        });
        view.ai.result = Some(proposal(view));
        view.ai.follow_up = Some(AiFollowUp {
            workspace: None,
            result_id: "follow-up-result".into(),
            result_name: "Follow-up result".into(),
            result_assets: vec![PathBuf::from("/fixture/result.png")],
            another_direction: true,
            plan_json: Some("{\"summary\":\"kept\",\"operations\":[]}".into()),
        });
        view.ai.references = vec![
            PathBuf::from("/fixture/reference-a.png"),
            PathBuf::from("/fixture/reference-b.png"),
        ];
        view.ai.transcript = vec![
            ("You".into(), "Earlier brief".into()),
            ("Omuse".into(), "Earlier answer".into()),
        ];

        view.select_ai_route(ProviderChoice::Pinned(ProviderId::ClaudeCode), cx);

        assert_eq!(
            view.ai.routing.choice(TaskKind::Design),
            ProviderChoice::Pinned(ProviderId::ClaudeCode)
        );
        assert_eq!(view.ai.prompt.read(cx).value(), "Keep this exact brief");
        assert_eq!(view.ai.result.as_ref().unwrap().id, "review-kept");
        assert_eq!(
            view.ai.follow_up.as_ref().unwrap().result_id,
            "follow-up-result"
        );
        assert_eq!(view.ai.references.len(), 2);
        assert_eq!(view.ai.transcript[1].1, "Earlier answer");
        assert!(view.ai.running.is_none());
        assert!(view.ai.activity.contains("nothing has been sent"));
    });
}

#[gpui_kit::test]
fn busy_request_refuses_a_route_change(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update(cx, |view, cx| {
        view.ai.task = AiTask::Generate;
        view.ai.route_picker_task = Some(AiTask::Generate);
        view.ai.preparing_image = true;
        view.ai.activity = "Preparing the current request".into();

        view.select_ai_route(ProviderChoice::Pinned(ProviderId::ClaudeCode), cx);

        assert_eq!(
            view.ai.routing.choice(TaskKind::Generate),
            ProviderChoice::Auto
        );
        assert_eq!(view.ai.route_picker_task, Some(AiTask::Generate));
        assert_eq!(view.ai.activity, "Preparing the current request");
        assert!(view.ai.preparing_image);
    });
}

#[gpui_kit::test]
fn unsupported_claude_pins_never_fall_back_or_start_a_request(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        for task in [AiTask::Caption, AiTask::Generate] {
            view.ai.task = task;
            view.ai
                .routing
                .set_choice(task.kind(), ProviderChoice::Pinned(ProviderId::ClaudeCode));
            view.ai.prompt.update(cx, |prompt, cx| {
                prompt.set_value("Do not send this fixture", window, cx)
            });

            let error = view.ai_route_for_task(task).unwrap_err();
            assert!(error.contains("selected provider"), "{error}");
            view.submit_ai_task(cx);

            assert!(view.ai.running.is_none(), "{task:?} unexpectedly started");
            assert!(view.ai.preparing_work_dir.is_none());
            assert!(!view.ai.preparing_image);
            assert!(view.ai.workflow.is_none());
            assert_eq!(
                view.ai.routing.choice(task.kind()),
                ProviderChoice::Pinned(ProviderId::ClaudeCode)
            );
        }
    });
}

#[gpui_kit::test]
fn signed_out_pin_does_not_use_a_different_ready_provider(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.providers = vec![
            status(
                ProviderId::CodexSubscription,
                ConnectionState::SignedOut,
                &[Capability::AssistantStreaming],
            ),
            status(
                ProviderId::ClaudeCode,
                ConnectionState::Ready,
                &[Capability::AssistantStreaming],
            ),
        ];
        view.ai.task = AiTask::Design;
        view.ai.routing.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::CodexSubscription),
        );
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("A valid design brief", window, cx)
        });

        let error = view.ai_route_for_task(AiTask::Design).unwrap_err();
        assert!(error.contains("needs sign-in"), "{error}");
        view.submit_ai_task(cx);

        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());
        assert!(view.ai.workflow.is_none());
        assert_eq!(
            view.ai.routing.choice(TaskKind::Design),
            ProviderChoice::Pinned(ProviderId::CodexSubscription)
        );
    });
}

#[gpui_kit::test]
fn auto_routing_and_task_overrides_survive_a_preferences_round_trip(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update(cx, |view, _| {
        view.ai.routing.assistant = ProviderId::ClaudeCode;
        view.ai.routing.image_provider = ProviderId::CodexSubscription;

        assert_eq!(
            view.ai_route_for_task(AiTask::Design).unwrap().provider,
            ProviderId::ClaudeCode
        );
        assert_eq!(
            view.ai_route_for_task(AiTask::Caption).unwrap().provider,
            ProviderId::CodexSubscription,
            "caption visual input must override the preferred text-only assistant"
        );

        view.ai.routing.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::CodexSubscription),
        );
        view.ai.routing.set_choice(
            TaskKind::Generate,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        let encoded = serde_json::to_vec(&view.ai.routing).unwrap();
        let loaded = RoutingPreferences::from_json_bounded(&encoded).unwrap();

        assert_eq!(loaded.assistant, ProviderId::ClaudeCode);
        assert_eq!(loaded.image_provider, ProviderId::CodexSubscription);
        assert_eq!(
            loaded.choice(TaskKind::Design),
            ProviderChoice::Pinned(ProviderId::CodexSubscription)
        );
        assert_eq!(
            loaded.choice(TaskKind::Generate),
            ProviderChoice::Pinned(ProviderId::ClaudeCode)
        );
        assert_eq!(loaded.choice(TaskKind::Caption), ProviderChoice::Auto);
    });
}

#[gpui_kit::test]
fn unreadable_preferences_block_mutation_until_the_explicit_reset(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update(cx, |view, cx| {
        view.ai.routing.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::CodexSubscription),
        );
        view.ai.routing_error = Some("Saved AI choices could not be read".into());
        view.ai.route_picker_task = Some(AiTask::Design);
        view.ai.activity = "Unreadable choices remain protected".into();

        view.select_ai_route(ProviderChoice::Pinned(ProviderId::ClaudeCode), cx);
        view.save_ai_preferences();

        assert_eq!(
            view.ai.routing.choice(TaskKind::Design),
            ProviderChoice::Pinned(ProviderId::CodexSubscription)
        );
        assert_eq!(view.ai.activity, "Unreadable choices remain protected");
        assert!(view.ai.routing_error.is_some());
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    reveal_and_click(cx, "ai-reset-routing");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.ai.routing_error.is_none());
        assert_eq!(
            view.ai.routing.choice(TaskKind::Design),
            ProviderChoice::Auto
        );
    });
}

#[gpui_kit::test]
fn route_and_follow_on_choosers_are_reachable_at_the_minimum_window(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update(cx, |view, _| view.ai.task = AiTask::Generate);
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);

    click(cx, "ai-choose-provider");
    assert!(cx.debug_bounds("ai-route-auto").is_some());
    reveal(cx, "ai-route-ClaudeCode");
    cx.update(|_, cx| assert_eq!(view.read(cx).ai.route_picker_task, Some(AiTask::Generate)));

    click(cx, "ai-choose-provider");
    reveal_and_click(cx, "ai-finish-layout");
    reveal_and_click(cx, "ai-finish-caption");
    reveal_and_click(cx, "ai-step-provider-1");

    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.ai.finish_with_layout);
        assert!(view.ai.finish_with_caption);
        assert_eq!(view.ai.route_picker_task, Some(AiTask::Design));
    });
    reveal(cx, "ai-route-ClaudeCode");
}

#[gpui_kit::test]
fn pending_preparation_freezes_provider_and_owns_private_references(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("completed-result");
    std::fs::create_dir(&workspace).unwrap();
    let reference = workspace.join("reference.png");
    std::fs::write(&reference, b"private reference").unwrap();
    let next_work_dir = root.path().join("next-request");

    view.update(cx, |view, _| {
        view.ai.reference_workspaces = vec![Arc::new(PrivateAiWorkspace::adopt(workspace.clone()))];
        let document = view.editor.document.clone();
        let pending = PendingAssistantRequest {
            workflow: None,
            _reference_workspaces: view
                .ai_reference_workspaces_for(std::slice::from_ref(&reference)),
            client: ai::ValidatedClient::fixture(
                ProviderId::ClaudeCode,
                PathBuf::from("/fixture/claude"),
            ),
            provider: ProviderId::ClaudeCode,
            provider_version: Some("fixture".into()),
            source: view.ai_source_identity(),
            source_document: document.clone(),
            source_project: omuse::create_project::Project::new("fixture", document),
            active_layer: view.editor.active_layer.clone(),
            task: AiTask::Design,
            brief: "Frozen brief".into(),
            follow_up_context: String::new(),
            reference_paths: vec![reference.clone()],
            include_canvas_preview: false,
            submission: CapabilitySubmission::Verified,
            work_dir: next_work_dir,
        };

        view.ai.reference_workspaces.clear();
        view.ai.routing.assistant = ProviderId::CodexSubscription;
        assert!(
            reference.is_file(),
            "pending preparation must retain its source"
        );
        assert_eq!(pending.provider, ProviderId::ClaudeCode);
        assert_eq!(pending.client.provider(), ProviderId::ClaudeCode);
        assert_eq!(pending.reference_paths, vec![reference.clone()]);

        drop(pending);
        assert!(
            !workspace.exists(),
            "the last pending owner should clean the private source workspace"
        );
    });
}

#[gpui_kit::test]
fn beginning_a_three_step_workflow_preflights_and_freezes_the_first_route(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.task = AiTask::Generate;
        view.ai.finish_with_layout = true;
        view.ai.finish_with_caption = true;
        view.ai.routing.set_choice(
            TaskKind::Generate,
            ProviderChoice::Pinned(ProviderId::CodexSubscription),
        );
        view.ai.routing.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        view.ai.routing.set_choice(
            TaskKind::Caption,
            ProviderChoice::Pinned(ProviderId::CodexSubscription),
        );
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("Build a complete campaign draft", window, cx)
        });

        view.begin_ai_workflow(cx).unwrap();
        assert_eq!(
            view.ai_workflow_current_route(),
            Some((AiTask::Generate, ProviderId::CodexSubscription))
        );
        assert_eq!(
            view.ai_workflow_progress().as_deref(),
            Some("Step 1/3 · Generate image · ChatGPT via Codex")
        );

        // Discovery and preferences can change while a request is running.
        // The disclosed workflow must retain the route captured at preflight.
        view.ai.routing.set_choice(
            TaskKind::Generate,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        view.ai.providers.clear();
        assert_eq!(
            view.ai_workflow_current_route(),
            Some((AiTask::Generate, ProviderId::CodexSubscription))
        );
        assert_eq!(
            view.ai_workflow_progress().as_deref(),
            Some("Step 1/3 · Generate image · ChatGPT via Codex")
        );
        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());
    });
}

#[gpui_kit::test]
fn unsupported_follow_on_pin_fails_preflight_before_any_work_starts(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.task = AiTask::Generate;
        view.ai.finish_with_caption = true;
        view.ai.routing.set_choice(
            TaskKind::Caption,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("Generate then caption", window, cx)
        });

        let error = view.begin_ai_workflow(cx).unwrap_err().to_string();

        assert!(error.contains("Caption & alt text step"), "{error}");
        assert!(error.contains("selected provider"), "{error}");
        assert!(view.ai.workflow.is_none());
        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());
        assert!(!view.ai.preparing_image);
    });
}

#[gpui_kit::test]
fn enabling_local_only_midsequence_discards_the_remote_workflow(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.task = AiTask::Generate;
        view.ai.finish_with_caption = true;
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("Generate then caption", window, cx)
        });
        view.begin_ai_workflow(cx).unwrap();
        assert!(view.ai.workflow.is_some());

        view.set_ai_local_only(true, cx).unwrap();

        assert!(view.ai_local_only());
        assert!(view.ai.workflow.is_none());
        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());
        assert!(view.ai.activity.contains("Local-only"));
    });
}

#[gpui_kit::test]
fn stopped_or_stale_first_result_never_dispatches_the_next_step(cx: &mut TestAppContext) {
    let (view, cx) = setup_view(cx);
    view.update_in(cx, |view, window, cx| {
        view.ai.task = AiTask::Generate;
        view.ai.finish_with_caption = true;
        view.ai.prompt.update(cx, |prompt, cx| {
            prompt.set_value("Generate then caption", window, cx)
        });

        view.begin_ai_workflow(cx).unwrap();
        let mut stopped = proposal(view);
        assert!(!view.continue_ai_workflow(&mut stopped, false, cx));
        assert!(view.ai.workflow.is_none());
        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());

        view.begin_ai_workflow(cx).unwrap();
        let mut stale = proposal(view);
        view.editor = Editor::new(Document::new(41, 31));
        assert!(!view.continue_ai_workflow(&mut stale, true, cx));
        assert!(view.ai.workflow.is_none());
        assert!(view.ai.running.is_none());
        assert!(view.ai.preparing_work_dir.is_none());
        assert!(!view.ai.preparing_image);
        assert!(view.ai.activity.contains("Sequence stopped"));
    });
}
