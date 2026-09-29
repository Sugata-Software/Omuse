//! User-owned routing. Resolving a route never sends a request or mutates a draft.
use super::*;

impl AiTask {
    pub(super) fn kind(self) -> TaskKind {
        match self {
            Self::Design => TaskKind::Design,
            Self::Photo => TaskKind::Photo,
            Self::Caption => TaskKind::Caption,
            Self::Generate => TaskKind::Generate,
            Self::Replace => TaskKind::Replace,
            Self::Remove => TaskKind::Remove,
            Self::Background => TaskKind::Background,
            Self::Expand => TaskKind::Expand,
        }
    }
}

impl EditorView {
    pub(super) fn ai_route_for_task(&self, task: AiTask) -> Result<ResolvedRoute, String> {
        if let Some(error) = &self.ai.routing_error {
            return Err(error.clone());
        }
        resolve_route(
            task.kind(),
            !effective_reference_paths(self.ai.follow_up.as_ref(), &self.ai.references).is_empty(),
            &self.ai.routing,
            &self.ai.providers,
        )
        .map_err(|error| format!("{error} Check Connections, or choose another provider."))
    }

    pub(super) fn ai_task_provider(&self) -> Option<ProviderId> {
        // The active job owns a frozen client. Discovery and future preferences
        // must not relabel a submitted request as work by another provider.
        self.ai
            .running
            .as_ref()
            .map(|job| job.client.provider())
            .or_else(|| {
                self.ai_workflow_current_route()
                    .map(|(_, provider)| provider)
            })
            .or_else(|| {
                self.ai_route_for_task(self.ai.task)
                    .ok()
                    .map(|route| route.provider)
            })
    }

    pub(super) fn select_ai_route(&mut self, choice: ProviderChoice, cx: &mut Context<Self>) {
        if self.ai_busy() || self.ai.routing_error.is_some() {
            return;
        }
        let task = self.ai.route_picker_task.unwrap_or(self.ai.task);
        self.ai.routing.set_choice(task.kind(), choice);
        self.ai.route_picker_task = None;
        self.ai.activity = match self.ai_route_for_task(self.ai.task) {
            Ok(route) => format!(
                "{}. Your brief and review are unchanged; nothing has been sent.",
                route.reason
            ),
            Err(note) => {
                format!("{note} Your brief and review are unchanged; nothing has been sent.")
            }
        };
        self.save_ai_preferences();
        cx.notify();
    }

    pub(super) fn ai_route_button(&self, cx: &mut Context<Self>) -> Div {
        let task = self
            .ai_workflow_current_route()
            .map(|(task, _)| task)
            .unwrap_or(self.ai.task);
        let choice = self.ai.routing.choice(task.kind());
        let name = match self.ai_task_provider() {
            Some(provider) => provider.display_name(),
            None => match choice {
                ProviderChoice::Pinned(provider) => provider.display_name(),
                ProviderChoice::Auto => "Choose a connection",
            },
        };
        let mode = if choice == ProviderChoice::Auto {
            "Auto"
        } else {
            "Pinned"
        };
        div().flex().flex_col().gap_1().child(
            button(
                "ai-choose-provider",
                SharedString::from(format!("{mode} · {name} ▾")),
                ButtonVariant::Secondary,
                cx,
            )
            .w_full()
            .accessibility_label(format!("Provider for {}: {mode}, {name}", task.label()))
            .justify_start()
            .disabled(self.ai_busy())
            .selected(self.ai.route_picker_task == Some(self.ai.task))
            .debug_selector(|| "ai-choose-provider".into())
            .on_click(cx.listener(|this, _, _, cx| {
                if this.ai_busy() {
                    return;
                }
                this.ai.connections_visible = false;
                this.ai.route_picker_task = if this.ai.route_picker_task == Some(this.ai.task) {
                    None
                } else {
                    Some(this.ai.task)
                };
                cx.notify();
            })),
        )
    }

    pub(super) fn ai_route_picker(&self, cx: &mut Context<Self>) -> Div {
        let task = self.ai.route_picker_task.unwrap_or(self.ai.task);
        let choice = self.ai.routing.choice(task.kind());
        let mut panel = panel_section(format!("PROVIDER FOR {}", task.label().to_uppercase()), cx)
            .border_color(cx.omarchy().accent.opacity(0.5))
            .child(label("Saved for this task. Auto uses your preferred capable subscription. A pinned provider is never replaced automatically.", cx));
        for option in [ProviderChoice::Auto]
            .into_iter()
            .chain(ProviderId::ALL.map(ProviderChoice::Pinned))
        {
            let (id, title) = match option {
                ProviderChoice::Auto => ("ai-route-auto".to_string(), "Auto"),
                ProviderChoice::Pinned(provider) => {
                    (format!("ai-route-{provider:?}"), provider.display_name())
                }
            };
            let mut preferences = self.ai.routing.clone();
            preferences.set_choice(task.kind(), option);
            let description = match resolve_route(
                task.kind(),
                !effective_reference_paths(self.ai.follow_up.as_ref(), &self.ai.references)
                    .is_empty(),
                &preferences,
                &self.ai.providers,
            ) {
                Ok(route) => format!(
                    "{}{}",
                    route.reason,
                    if route.first_use {
                        " · first use will test support"
                    } else {
                        ""
                    }
                ),
                Err(error) => error.to_string(),
            };
            let selector = id.clone();
            panel = panel
                .child(
                    button(SharedString::from(id), title, ButtonVariant::Secondary, cx)
                        .w_full()
                        .selected(choice == option)
                        .disabled(self.ai_busy() || self.ai.routing_error.is_some())
                        .debug_selector(move || selector.clone())
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_ai_route(option, cx)),
                        ),
                )
                .child(label(description, cx));
        }
        if let Some(error) = &self.ai.routing_error {
            panel = panel.child(label(error.clone(), cx)).child(
                button(
                    "ai-reset-routing",
                    "Reset unreadable choices",
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(|| "ai-reset-routing".into())
                .disabled(self.ai_busy())
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.ai_busy() {
                        return;
                    }
                    this.ai.routing = RoutingPreferences::default();
                    this.ai.routing_error = None;
                    this.save_ai_preferences();
                    cx.notify();
                })),
            );
        }
        panel.child(label("Switching providers keeps your brief, references and review. Each new request uses the displayed subscription; failed requests are never retried with another provider automatically.", cx))
    }
}
