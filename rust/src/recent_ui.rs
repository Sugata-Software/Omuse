//! Recent-project navigation uses the normal unsaved-work guard.
use super::*;
use gpui_kit::{Keystroke, ScrollHandle};
use omuse::recent_projects::RecentProjects;
#[cfg(all(test, feature = "ui-test"))]
#[path = "recent_upgrade_tests.rs"]
mod tests;

enum Update {
    Refresh,
    Note(PathBuf),
    Clear,
}
pub(super) struct RecentState {
    pub history: RecentProjects,
    query: Entity<InputState>,
    selected: usize,
    scroll: ScrollHandle,
    store: Option<PathBuf>,
    pending: std::collections::VecDeque<Update>,
    running: bool,
    message: String,
}
impl RecentState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find a recent project…"));
        cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.recent.selected = 0;
                cx.notify();
            }
        })
        .detach();
        Self {
            history: RecentProjects::default(),
            query,
            selected: 0,
            scroll: ScrollHandle::new(),
            store: (!cfg!(test)).then(|| omuse::identity::config_file_path("recent-projects.json")),
            pending: Default::default(),
            running: false,
            message: String::new(),
        }
    }
}
impl EditorView {
    pub(super) fn refresh_recent(&mut self, cx: &mut Context<Self>) {
        if !self
            .recent
            .pending
            .iter()
            .any(|op| matches!(op, Update::Refresh))
        {
            self.recent.pending.push_back(Update::Refresh);
        }
        self.start_recent_update(cx);
    }
    pub(super) fn note_recent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.recent
            .pending
            .retain(|op| !matches!(op,Update::Note(p) if *p==path));
        self.recent.pending.push_back(Update::Note(path));
        while self.recent.pending.len() > 12 {
            if let Some(i) = self
                .recent
                .pending
                .iter()
                .position(|op| matches!(op, Update::Note(_)))
            {
                self.recent.pending.remove(i);
            } else {
                break;
            }
        }
        self.start_recent_update(cx);
    }
    fn clear_recent(&mut self, cx: &mut Context<Self>) {
        self.recent.pending.clear();
        self.recent.pending.push_back(Update::Clear);
        self.start_recent_update(cx);
    }
    fn start_recent_update(&mut self, cx: &mut Context<Self>) {
        if self.recent.running {
            return;
        }
        let Some(op) = self.recent.pending.pop_front() else {
            return;
        };
        self.recent.running = true;
        let store = self.recent.store.clone();
        let memory = self.recent.history.clone();
        let task = cx.background_executor().spawn(async move {
            let _guard = if let Some(path) = &store {
                std::fs::create_dir_all(path.parent().unwrap())?;
                Some(omuse::save_guard::SaveGuard::acquire(path)?)
            } else {
                None
            };
            let mut history = if matches!(op, Update::Clear) {
                RecentProjects::default()
            } else if let Some(path) = &store {
                RecentProjects::load(path)?
            } else {
                memory
            };
            match op {
                Update::Refresh => history.refresh(),
                Update::Note(path) => history.note(&path)?,
                Update::Clear => history.paths.clear(),
            }
            if let Some(path) = &store {
                history.save(path)?;
            }
            Ok::<_, anyhow::Error>(history)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.recent.running = false;
                match result {
                    Ok(history) => {
                        this.recent.history = history;
                        this.recent.message.clear();
                    }
                    Err(error) => this.recent.message = format!("History unavailable: {error:#}"),
                }
                this.recent.selected = this
                    .recent
                    .selected
                    .min(this.recent_results(cx).len().saturating_sub(1));
                this.start_recent_update(cx);
                cx.notify();
            });
        })
        .detach();
    }
    fn recent_results(&self, cx: &App) -> Vec<PathBuf> {
        let query = self.recent.query.read(cx).value().to_lowercase();
        self.recent
            .history
            .paths
            .iter()
            .filter(|p| p.to_string_lossy().to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
    pub(super) fn open_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Dialog::Recent;
        self.dialog_generation += 1;
        self.recent.query.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.recent.selected = 0;
        self.refresh_recent(cx);
        cx.notify();
    }
    fn choose_recent(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Dialog::None;
        self.request(Pending::OpenPath(path), window, cx);
    }
    pub(super) fn recent_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialog != Dialog::Recent
            || !self.modal_focus.contains_focused(window, cx)
            || key.modifiers.control
            || key.modifiers.alt
            || key.modifiers.platform
        {
            return false;
        }
        let results = self.recent_results(cx);
        match key.key.as_str() {
            "up" => self.recent.selected = self.recent.selected.saturating_sub(1),
            "down" => {
                self.recent.selected =
                    (self.recent.selected + 1).min(results.len().saturating_sub(1))
            }
            "enter" => {
                if let Some(path) = results.get(self.recent.selected) {
                    self.choose_recent(path.clone(), window, cx);
                }
            }
            _ => return false,
        }
        self.recent.scroll.scroll_to_item(self.recent.selected);
        cx.notify();
        true
    }
    pub(super) fn recent_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut rows = div()
            .id("recent-project-list")
            .flex()
            .flex_col()
            .gap_1()
            .min_h_0()
            .max_h(px(300.))
            .overflow_y_scroll()
            .track_scroll(&self.recent.scroll);
        let results = self.recent_results(cx);
        if results.is_empty() {
            rows = rows.child(div().p_4().text_color(t.secondary).child(
                if self.recent.history.paths.is_empty() {
                    "Your saved projects will appear here."
                } else {
                    "No matching projects."
                },
            ));
        }
        for (index, path) in results.into_iter().enumerate() {
            let selected = index == self.recent.selected;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let location = path
                .parent()
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            rows = rows.child(
                div()
                    .id(("recent-project", index))
                    .debug_selector(move || format!("recent-project-{index}"))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded(control_radius())
                    .cursor_pointer()
                    .bg(if selected {
                        t.selected_fill()
                    } else {
                        t.normal_fill()
                    })
                    .hover(|s| s.bg(t.hover_fill()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_recent(path.clone(), window, cx)
                    }))
                    .child(div().truncate().child(name))
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.secondary)
                            .truncate()
                            .child(location),
                    ),
            );
        }
        let body = div()
            .id("recent-projects")
            .debug_selector(|| "recent-projects".into())
            .w(px((f32::from(window.viewport_size().width) - 40.)
                .min(620.)
                .max(240.)))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 40.).max(240.)
            ))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(t.surface)
            .border_1()
            .border_color(t.control_border())
            .rounded(px(8.))
            .child(div().text_lg().child("Recent projects"))
            .child(input("recent-search", &self.recent.query, window, cx))
            .child(rows)
            .when(!self.recent.message.is_empty(), |b| {
                b.child(
                    div()
                        .text_sm()
                        .text_color(t.danger)
                        .child(self.recent.message.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        button("clear-recent", "Clear history", ButtonVariant::Outline, cx)
                            .disabled(
                                self.recent.history.paths.is_empty()
                                    && self.recent.message.is_empty(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.clear_recent(cx))),
                    )
                    .child(
                        button("close-recent", "Close", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.dialog = Dialog::None;
                                this.focus.focus(window, cx);
                                cx.notify();
                            }),
                        ),
                    ),
            );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .child(body.focus_trap("recent-focus", &self.modal_focus))
            .into_any_element()
    }
}
