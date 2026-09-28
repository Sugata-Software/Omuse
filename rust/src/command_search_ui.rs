//! Theme-aware command discovery: the same catalog powers search, execution and remapping.
use super::*;
use gpui_kit::{Focusable, FontWeight, Keystroke, ScrollHandle};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SearchFilter {
    #[default]
    All,
    Bound,
    Unbound,
    Gestures,
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, VisualTestContext};

    fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
        cx.update(crate::init_test_theme);
        cx.update(|cx| install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.shortcuts = Shortcuts::default();
            view.editor = Editor::new(Document::new(32, 24));
            view.focus.focus(window, cx);
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        draw(cx);
        (view, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui_kit::test]
    fn palette_opens_by_shortcut_searches_executes_and_restores_canvas_focus(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = setup(cx);
        cx.simulate_keystrokes("ctrl-k");
        draw(cx);
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::CommandSearch));
        cx.simulate_input("eraser");
        draw(cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                view.command_search_results(cx)[0].definition.id,
                "tool-eraser"
            );
            assert_eq!(view.tool, Tool::Brush, "Typing must not select tools");
        });
        cx.simulate_keystrokes("enter");
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::None);
            assert_eq!(view.tool, Tool::Eraser);
            assert!(view.focus.is_focused(window));
        });
        cx.simulate_keystrokes("b");
        cx.update(|_, cx| assert_eq!(view.read(cx).tool, Tool::Brush));
    }

    #[gpui_kit::test]
    fn palette_command_uses_normal_undo_and_dialog_guards(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        cx.simulate_keystrokes("ctrl-k");
        cx.simulate_input("new layer");
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).editor.document.layers.len(), 2);
            assert_eq!(view.read(cx).editor.undo_depth(), 1);
        });
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.document.layers.len(), 1));
        cx.simulate_keystrokes("ctrl-k");
        cx.simulate_input("resize image");
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::ResizeImage));
        cx.simulate_keystrokes("b ctrl-j");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.document.layers.len(), 1));
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    }

    #[gpui_kit::test]
    fn palette_empty_and_unavailable_results_never_edit_artwork(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        cx.simulate_keystrokes("ctrl-k");
        cx.simulate_input("no such command 78162");
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.command_search_results(cx).is_empty());
            assert_eq!(view.dialog, Dialog::CommandSearch);
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("crop");
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::CommandSearch);
            assert!(view.command_search.message.contains("selection"));
            assert!(
                view.editor.selection.is_none(),
                "Ctrl+A belongs to the search field"
            );
            assert_eq!(
                (view.editor.document.width, view.editor.document.height),
                (32, 24)
            );
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            assert_eq!(view.read(cx).dialog, Dialog::None);
            assert!(view.read(cx).focus.is_focused(window));
        });
    }

    #[gpui_kit::test]
    fn palette_arrows_scroll_selected_result_at_minimum_window(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        cx.simulate_keystrokes("ctrl-k");
        draw(cx);
        let panel = cx.debug_bounds("command-search").unwrap();
        assert!(panel.origin.x >= px(0.) && panel.origin.y >= px(0.));
        assert!(panel.origin.x + panel.size.width <= px(800.));
        assert!(panel.origin.y + panel.size.height <= px(600.));
        cx.simulate_keystrokes("pagedown pagedown down");
        draw(cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.command_search.selected, 13);
        });
        let row = cx.debug_bounds("command-result-selected").unwrap();
        let list = cx.debug_bounds("command-results").unwrap();
        assert!(
            row.origin.y >= list.origin.y,
            "Selected row must be visible"
        );
        assert!(row.origin.y + row.size.height <= list.origin.y + list.size.height);
        cx.simulate_input("eraser");
        draw(cx);
        cx.update(|_, cx| assert_eq!(view.read(cx).command_search.selected, 0));
    }

    #[gpui_kit::test]
    fn remapped_brush_keys_and_palette_show_the_same_effective_binding(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        view.update_in(cx, |view, _, cx| {
            let previous = view.shortcuts.clone();
            view.shortcuts.assign("brush-smaller", "alt-9").unwrap();
            install_shortcuts(&view.shortcuts, &previous, cx);
            view.editor.brush.size = 20.;
        });
        cx.simulate_keystrokes("[");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.brush.size, 20.));
        cx.simulate_keystrokes("alt-9");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.brush.size, 16.));
        cx.simulate_keystrokes("ctrl-k");
        cx.simulate_input("Alt+9");
        draw(cx);
        cx.update(|_, cx| {
            let results = view.read(cx).command_search_results(cx);
            assert_eq!(results[0].definition.id, "brush-smaller");
            assert_eq!(results[0].chord, "alt-9");
        });
        cx.simulate_keystrokes("escape");
        cx.simulate_keystrokes("alt-9");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.brush.size, 12.8));
    }
    #[gpui_kit::test]
    fn familiar_tool_chords_and_brush_modifiers_dispatch(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        for (chord, expected) in [
            ("v", Tool::Move),
            ("h", Tool::Hand),
            ("b", Tool::Brush),
            ("shift-b", Tool::Pencil),
            ("e", Tool::Eraser),
            ("g", Tool::Gradient),
            ("shift-g", Tool::Fill),
            ("m", Tool::Rectangle),
            ("shift-m", Tool::Ellipse),
            ("l", Tool::Lasso),
            ("w", Tool::Wand),
            ("i", Tool::Picker),
            ("s", Tool::Clone),
            ("j", Tool::SpotHeal),
            ("shift-j", Tool::Heal),
            ("u", Tool::ShapeRect),
            ("shift-u", Tool::ShapeEllipse),
            ("t", Tool::Text),
        ] {
            cx.simulate_keystrokes(chord);
            cx.update(|_, cx| assert_eq!(view.read(cx).tool, expected, "{chord}"));
        }
        cx.simulate_keystrokes("b");
        view.update(cx, |view, _| {
            view.editor.brush.hardness = 0.5;
        });
        // Linux XKB reports the produced symbol and consumes Shift for punctuation.
        cx.simulate_keystrokes("{ 3");
        cx.update(|_, cx| {
            assert!((view.read(cx).editor.brush.hardness - 0.4).abs() < 0.0001);
            assert!((view.read(cx).editor.brush.opacity - 0.3).abs() < 0.0001);
        });
        cx.simulate_keystrokes("}");
        cx.update(|_, cx| assert!((view.read(cx).editor.brush.hardness - 0.5).abs() < 0.0001));
        let snapping = cx.update(|_, cx| view.read(cx).preferences.snapping);
        cx.simulate_keystrokes("ctrl-:");
        cx.update(|_, cx| assert_ne!(view.read(cx).preferences.snapping, snapping));
    }

    #[gpui_kit::test]
    fn sidebar_typing_is_isolated_and_search_can_open_from_an_input(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        view.update_in(cx, |view, window, cx| {
            view.command("create-workspace", window, cx);
        });
        draw(cx);
        view.update_in(cx, |view, window, cx| {
            view.create.fields[0].update(cx, |field, cx| {
                field.set_value("", window, cx);
                field.focus(window, cx);
            });
        });
        draw(cx);
        cx.simulate_keystrokes("b v e x");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.tool, Tool::Brush);
            assert_eq!(view.create.fields[0].read(cx).value().as_ref(), "bvex");
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.simulate_keystrokes("ctrl-k");
        draw(cx);
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::CommandSearch));
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            assert!(view.read(cx).focus.is_focused(window));
            assert_eq!(
                view.read(cx).create.fields[0].read(cx).value().as_ref(),
                "bvex"
            );
        });
    }

    #[gpui_kit::test]
    fn remapped_save_commits_inline_text_and_old_binding_stays_inactive(cx: &mut TestAppContext) {
        let (view, cx) = setup(cx);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Keys.comp");
        view.update_in(cx, |view, window, cx| {
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.path = Some(path.clone());
            let previous = view.shortcuts.clone();
            view.shortcuts.assign("save", "ctrl-alt-s").unwrap();
            view.shortcuts.assign("save-as", "alt-shift-s").unwrap();
            install_shortcuts(&view.shortcuts, &previous, cx);
            view.begin_inline_text(None, (4., 4.), None, window, cx);
        });
        draw(cx);
        cx.simulate_input("Custom save");
        cx.simulate_keystrokes("ctrl-s");
        cx.update(|_, cx| assert!(view.read(cx).inline_text.is_some()));
        assert!(!path.exists());
        cx.simulate_keystrokes("ctrl-alt-s");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.inline_text.is_none());
            assert!(!view.editor.is_dirty());
            assert_eq!(view.editor.undo_depth(), 1);
            assert_eq!(
                objects::live_text(
                    document::open(&path)
                        .unwrap()
                        .find_layer(&view.editor.active_layer)
                        .unwrap()
                )
                .unwrap()
                .unwrap()
                .content,
                "Custom save"
            );
        });
        view.update_in(cx, |view, window, cx| {
            let previous = view.shortcuts.clone();
            view.shortcuts.assign("save", "f6").unwrap();
            install_shortcuts(&view.shortcuts, &previous, cx);
            view.begin_inline_text(
                Some(view.editor.active_layer.clone()),
                (4., 4.),
                None,
                window,
                cx,
            );
        });
        draw(cx);
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("Function key save");
        cx.simulate_keystrokes("f6");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.inline_text.is_none());
            assert_eq!(
                objects::live_text(
                    document::open(&path)
                        .unwrap()
                        .find_layer(&view.editor.active_layer)
                        .unwrap()
                )
                .unwrap()
                .unwrap()
                .content,
                "Function key save"
            );
        });
        let saved = std::fs::read(path.join("manifest.json")).unwrap();
        view.update_in(cx, |view, window, cx| {
            view.begin_inline_text(
                Some(view.editor.active_layer.clone()),
                (4., 4.),
                None,
                window,
                cx,
            );
        });
        draw(cx);
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("Save elsewhere");
        cx.simulate_keystrokes("ctrl-shift-s");
        cx.update(|_, cx| assert!(view.read(cx).inline_text.is_some()));
        cx.simulate_keystrokes("alt-shift-s");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.inline_text.is_none());
            assert_eq!(view.dialog, Dialog::Save);
            assert!(view.editor.is_dirty());
            assert_eq!(view.editor.undo_depth(), 3);
        });
        assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), saved);
    }

    #[gpui_kit::test]
    fn shortcut_editor_clear_restore_conflict_and_cancel_keep_active_bindings_safe(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = setup(cx);
        cx.simulate_keystrokes("ctrl-alt-k");
        draw(cx);
        cx.simulate_input("brush-smaller");
        draw(cx);
        let clear = cx.debug_bounds("clear-shortcut-brush-smaller").unwrap();
        cx.simulate_click(clear.center(), gpui_kit::Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).shortcut_draft.chord("brush-smaller"), "");
            assert_eq!(view.read(cx).shortcuts.chord("brush-smaller"), "[");
        });
        draw(cx);
        let restore = cx.debug_bounds("default-shortcut-brush-smaller").unwrap();
        cx.simulate_click(restore.center(), gpui_kit::Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).shortcut_draft.chord("brush-smaller"), "["));
        draw(cx);
        let record = cx.debug_bounds("record-brush-smaller").unwrap();
        cx.simulate_click(record.center(), gpui_kit::Modifiers::default());
        cx.simulate_keystrokes("ctrl-s");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.recording.as_deref(), Some("brush-smaller"));
            assert!(view.status.contains("Already assigned"), "{}", view.status);
            assert_eq!(view.shortcut_draft.chord("brush-smaller"), "[");
            assert_eq!(view.dialog, Dialog::Shortcuts);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert!(view.read(cx).recording.is_none());
            assert_eq!(view.read(cx).dialog, Dialog::Shortcuts);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::None);
            assert!(view.focus.is_focused(window));
            assert_eq!(view.shortcuts.chord("brush-smaller"), "[");
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }
}

pub(super) struct CommandSearchState {
    query: Entity<InputState>,
    selected: usize,
    filter: SearchFilter,
    scroll: ScrollHandle,
    message: String,
}

impl CommandSearchState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search commands, tools, categories or keys…")
        });
        cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.command_search.selected = 0;
                this.command_search.message.clear();
                this.command_search.scroll.scroll_to_item(0);
                cx.notify();
            }
        })
        .detach();
        Self {
            query,
            selected: 0,
            filter: SearchFilter::All,
            scroll: ScrollHandle::new(),
            message: String::new(),
        }
    }
}

impl EditorView {
    pub(super) fn open_command_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_search.selected = 0;
        self.command_search.filter = SearchFilter::All;
        self.command_search.message.clear();
        self.command_search.scroll.scroll_to_item(0);
        self.space_down = false;
        self.dialog = Dialog::CommandSearch;
        self.command_search.query.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn command_search_results(&self, cx: &App) -> Vec<shortcuts::CommandMatch> {
        let filter = self.command_search.filter;
        if filter == SearchFilter::Gestures {
            return vec![];
        }
        self.shortcuts
            .search(&self.command_search.query.read(cx).value())
            .into_iter()
            .filter(|entry| match filter {
                SearchFilter::Bound => !entry.chord.is_empty(),
                SearchFilter::Unbound => entry.chord.is_empty(),
                _ => true,
            })
            .collect()
    }

    fn command_search_gestures(&self, cx: &App) -> Vec<(&'static str, &'static str)> {
        let query = self.command_search.query.read(cx).value().to_lowercase();
        shortcuts::GESTURES
            .iter()
            .copied()
            .filter(|(label, keys)| {
                let text = format!("{label} {keys}").to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            })
            .collect()
    }

    // Specific missing prerequisites are visible before a command is run. The
    // normal command handler remains the authority for document validation.
    fn command_search_unavailable(&self, id: &str) -> Option<&'static str> {
        if self.busy {
            return Some("Wait for the current operation to finish");
        }
        let floating = self.editor.floating_selection_layer().is_some();
        if floating
            && (id.starts_with("tool-")
                || matches!(
                    id,
                    "new" | "open" | "save" | "save-as" | "export" | "import" | "quit"
                ))
        {
            return Some("Commit or cancel the floating selection first");
        }
        match id {
            "undo" if !self.can_undo_or_collection() => Some("Nothing to undo"),
            "redo" if !self.can_redo_or_collection() => Some("Nothing to redo"),
            "commit-selection" | "cancel-selection" if !floating => Some("No floating selection"),
            "crop" if self.selection_box.is_none() => Some("Draw a rectangular selection first"),
            "deselect"
            | "feather-selection"
            | "grow-selection"
            | "shrink-selection"
            | "transform-selection"
                if self.editor.selection.is_none() =>
            {
                Some("Make a selection first")
            }
            "transform-selection" if self.paint_mask => {
                Some("Use Place mask while painting a mask")
            }
            "previous-page" | "next-page" if self.create.session.is_none() => {
                Some("Open or create a page collection first")
            }
            _ => None,
        }
    }

    fn run_search_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog != Dialog::CommandSearch || shortcuts::definition(id).is_none() {
            return;
        }
        if let Some(index) = self
            .command_search_results(cx)
            .iter()
            .position(|entry| entry.definition.id == id)
        {
            self.command_search.selected = index;
        }
        if let Some(reason) = self.command_search_unavailable(id) {
            self.command_search.message = reason.into();
            cx.notify();
            return;
        }
        self.dialog = Dialog::None;
        self.focus.focus(window, cx);
        self.command(id, window, cx);
        // Dialogs and the assistant composer retain their own input focus.
        // Other commands return directly to the canvas key context.
        if self.dialog == Dialog::None && self.inline_text.is_none() && id != "ask-omuse" {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn command_search_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialog != Dialog::CommandSearch || !self.modal_focus.contains_focused(window, cx) {
            return false;
        }
        let modifiers = key.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.shift {
            return false;
        }
        if key.key == "escape" {
            self.dialog = Dialog::None;
            self.focus.focus(window, cx);
            cx.notify();
            return true;
        }
        // Let Tab and focused buttons keep their normal activation behaviour.
        if !self
            .command_search
            .query
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            return false;
        }
        let results = self.command_search_results(cx);
        let count = if self.command_search.filter == SearchFilter::Gestures {
            self.command_search_gestures(cx).len()
        } else {
            results.len()
        };
        let selected = self.command_search.selected.min(count.saturating_sub(1));
        match key.key.as_str() {
            "up" => self.command_search.selected = selected.saturating_sub(1),
            "down" => self.command_search.selected = (selected + 1).min(count.saturating_sub(1)),
            "pageup" => self.command_search.selected = selected.saturating_sub(6),
            "pagedown" => {
                self.command_search.selected = (selected + 6).min(count.saturating_sub(1))
            }
            "enter" => {
                if let Some(entry) = results.get(selected) {
                    self.run_search_command(entry.definition.id, window, cx);
                }
                return true;
            }
            _ => return false,
        }
        self.command_search.message.clear();
        self.command_search
            .scroll
            .scroll_to_item(self.command_search.selected);
        cx.notify();
        true
    }

    pub(super) fn command_search_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.omarchy().clone();
        let results = self.command_search_results(cx);
        let gestures = self.command_search_gestures(cx);
        let is_gestures = self.command_search.filter == SearchFilter::Gestures;
        let count = if is_gestures {
            gestures.len()
        } else {
            results.len()
        };
        self.command_search.selected = self.command_search.selected.min(count.saturating_sub(1));
        let active = results.get(self.command_search.selected);
        let active_id = active.map(|entry| entry.definition.id);
        let active_reason = active_id.and_then(|id| self.command_search_unavailable(id));
        let detail = if !self.command_search.message.is_empty() {
            self.command_search.message.clone()
        } else if let Some(reason) = active_reason {
            reason.into()
        } else if let Some(entry) = active {
            shortcuts::contextual_notes(entry.definition.id).into()
        } else if is_gestures {
            "Gesture reference · use these directly on the canvas".into()
        } else {
            "No matches. Try a tool, category, or shortcut such as Ctrl+S.".into()
        };
        let mut tabs = div().flex().items_center().gap_1();
        for (filter, id, label) in [
            (SearchFilter::All, "search-all", "All commands"),
            (SearchFilter::Bound, "search-bound", "With shortcuts"),
            (SearchFilter::Unbound, "search-unbound", "Unbound"),
            (SearchFilter::Gestures, "search-gestures", "Gestures"),
        ] {
            tabs = tabs.child(
                button(id, label, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .selected(self.command_search.filter == filter)
                    .h(px(28.))
                    .px_2()
                    .text_size(px(11.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.command_search.filter = filter;
                        this.command_search.selected = 0;
                        this.command_search.message.clear();
                        this.command_search.scroll.scroll_to_item(0);
                        this.command_search
                            .query
                            .update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    })),
            );
        }
        tabs = tabs.child(div().flex_1()).child(
            div()
                .text_size(px(11.))
                .text_color(theme.secondary)
                .child(format!(
                    "{count} / {}",
                    if is_gestures {
                        shortcuts::GESTURES.len()
                    } else {
                        shortcuts::catalog().len()
                    }
                )),
        );
        let mut list = div()
            .id("command-results")
            .debug_selector(|| "command-results".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.command_search.scroll)
            .flex()
            .flex_col()
            .gap_1();
        if is_gestures {
            for (index, (label, keys)) in gestures.iter().enumerate() {
                list = list.child(
                    div()
                        .p_3()
                        .rounded(px(4.))
                        .flex_shrink_0()
                        .bg(if index == self.command_search.selected {
                            theme.accent.opacity(0.10)
                        } else {
                            theme.inset
                        })
                        .child(div().text_color(theme.bright).child(*label))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme.secondary)
                                .child(*keys),
                        ),
                );
            }
        } else {
            for (index, entry) in results.iter().enumerate() {
                let id = entry.definition.id;
                let reason = self.command_search_unavailable(id);
                let selected = index == self.command_search.selected;
                let chord = if entry.chord.is_empty() {
                    "Unbound".into()
                } else {
                    shortcuts::display_chord(&entry.chord)
                };
                list = list.child(
                    button(
                        SharedString::from(format!("command-result-{id}")),
                        "",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .debug_selector(move || {
                        if selected {
                            "command-result-selected".into()
                        } else {
                            format!("command-result-{id}")
                        }
                    })
                    .accessibility_label(entry.definition.label)
                    .selected(selected)
                    .w_full()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_3()
                    .py_1()
                    .justify_start()
                    .rounded(px(4.))
                    .bg(if selected {
                        theme.accent.opacity(0.12)
                    } else {
                        theme.surface
                    })
                    .border_color(if selected {
                        theme.accent
                    } else {
                        theme.divider()
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(if reason.is_some() {
                                        theme.secondary
                                    } else {
                                        theme.bright
                                    })
                                    .child(entry.definition.label),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme.secondary)
                                    .child(reason.unwrap_or(entry.definition.category)),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(theme.mono_font.clone())
                            .text_size(px(11.))
                            .text_color(theme.secondary)
                            .child(chord),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run_search_command(id, window, cx)
                    })),
                );
            }
        }
        if count == 0 {
            list = list.child(
                div()
                    .p_4()
                    .text_color(theme.secondary)
                    .child("No matching commands or gestures"),
            );
        }
        let panel = div()
            .id("command-search")
            .debug_selector(|| "command-search".into())
            .w(px(
                (f32::from(window.viewport_size().width) - 48.).clamp(240., 700.)
            ))
            .h(px(
                (f32::from(window.viewport_size().height) - 48.).clamp(220., 580.)
            ))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(8.))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.control_border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        crate::studio_icons::glyph("search")
                            .size(px(18.))
                            .text_color(theme.accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(17.))
                            .text_color(theme.bright)
                            .child("Commands & shortcuts"),
                    )
                    .child(
                        button("close-command-search", "Esc", ButtonVariant::Secondary, cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dialog = Dialog::None;
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(input(
                "command-query",
                &self.command_search.query,
                window,
                cx,
            ))
            .child(tabs)
            .child(list)
            .child(
                div()
                    .min_h(px(32.))
                    .text_size(px(11.))
                    .text_color(theme.secondary)
                    .child(detail),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.divider())
                    .pt_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.))
                            .text_color(theme.secondary)
                            .child("↑ ↓ choose · Enter run · Esc close"),
                    )
                    .child(
                        button(
                            "customize-search-shortcut",
                            "Edit shortcut",
                            ButtonVariant::Outline,
                            cx,
                        )
                        .disabled(active_id.is_none())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let Some(id) = active_id else { return };
                                this.dialog = Dialog::None;
                                this.command("shortcuts", window, cx);
                                this.path_input
                                    .update(cx, |input, cx| input.set_value(id, window, cx));
                            },
                        )),
                    )
                    .child(
                        button("run-search-command", "Run", ButtonVariant::Primary, cx)
                            .debug_selector(|| "run-search-command".into())
                            .disabled(active_id.is_none() || active_reason.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(id) = active_id {
                                    this.run_search_command(id, window, cx);
                                }
                            })),
                    ),
            );
        div()
            .id("modal-shield")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .child(panel.focus_trap("modal-focus", &self.modal_focus))
            .into_any_element()
    }

    /// Used by the disposable native-window journey, after opening the palette.
    pub(super) fn native_command_query(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.command_search
            .query
            .update(cx, |input, cx| input.set_value(query, window, cx));
    }
}
