//! Contextual actions on the shared canvas. The bottom dock has a stable target
//! while zooming/resizing and wraps within the viewport. It never captures the
//! canvas outside its own surface, and never creates a second command path.
use super::*;
use gpui_omarchy::with_tooltip;

#[cfg(all(test, feature = "ui-test"))]
#[path = "canvas_context_ui_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanvasContext {
    Photo,
    Selection,
    Text,
    Artwork,
    Layers,
    Folder,
    VectorDraft,
    Floating,
}

impl CanvasContext {
    fn title(self) -> &'static str {
        match self {
            Self::Photo => "Photo",
            Self::Selection => "Selected pixels",
            Self::Text => "Text",
            Self::Artwork => "Artwork",
            Self::Layers => "Selected layers",
            Self::Folder => "Frame / group",
            Self::VectorDraft => "Vector edit",
            Self::Floating => "Floating pixels",
        }
    }

    pub(super) fn actions(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Photo => &[
                ("camera-raw", "Develop"),
                ("remove-background", "Cut out"),
                ("add-mask", "Add mask"),
                ("duplicate", "Duplicate"),
                ("ask-omuse", "Ask Omuse"),
            ],
            Self::Selection => &[
                ("controlled-removal", "Remove object"),
                ("content-fill", "Fill"),
                ("transform-selection", "Move pixels"),
                ("deselect", "Deselect"),
            ],
            Self::Text => &[
                ("edit-object", "Edit text"),
                ("transform", "Transform"),
                ("duplicate", "Duplicate"),
                ("ask-omuse", "Ask Omuse"),
            ],
            Self::Artwork => &[
                ("edit-object", "Edit artwork"),
                ("transform", "Transform"),
                ("duplicate", "Duplicate"),
                ("ask-omuse", "Ask Omuse"),
            ],
            Self::Layers => &[
                ("auto-layout", "Auto layout"),
                ("group", "Group"),
                ("duplicate", "Duplicate"),
            ],
            Self::Folder => &[
                ("auto-layout", "Auto layout"),
                ("ungroup", "Ungroup"),
                ("duplicate", "Duplicate"),
            ],
            Self::VectorDraft => &[
                ("tool-move", "Select objects"),
                ("vector-nodes", "Edit points"),
                ("duplicate", "Duplicate"),
                ("edit-object", "More controls"),
            ],
            Self::Floating => &[
                ("commit-selection", "Keep pixels"),
                ("cancel-selection", "Cancel"),
            ],
        }
    }
}

impl EditorView {
    pub(super) fn canvas_context(&self, cx: &App) -> Option<CanvasContext> {
        if !self.preferences.context_toolbar
            || self.busy
            || self.photo_io.is_some()
            || self.dialog != Dialog::None
            || self.inline_text.is_some()
            || self.crop.is_some()
            || self.image_trace_active()
            || self.mask_inspection.active()
            || self.paint_mask
            || self.create.phone_preview
            || self.drag_start.is_some()
            || self.transform_drag.is_some()
            || self.guide_drag.is_some()
            || self.tablet_painting
            || self.space_down
            || self.pan_pointer.is_some()
            || self.middle_pan_pointer.is_some()
            || self.numeric_scrubbing()
            || cx.has_active_drag()
        {
            return None;
        }
        if self.vector_scene_active() {
            return self
                .vector_context_ready()
                .then_some(CanvasContext::VectorDraft);
        }
        if self.editor.floating_selection_layer().is_some() {
            return Some(CanvasContext::Floating);
        }
        // Painting and sampling get an unobstructed canvas. Selection actions
        // appear after the gesture, not while the user is drawing its boundary.
        if !matches!(
            self.tool,
            Tool::Move
                | Tool::Text
                | Tool::Rectangle
                | Tool::Ellipse
                | Tool::Lasso
                | Tool::Wand
                | Tool::Object
        ) {
            return None;
        }
        let layer = self.editor.document.find_layer(&self.editor.active_layer)?;
        if !visible_layer(&self.editor.document.layers, &layer.id, true) {
            return None;
        }
        if self
            .editor
            .selection
            .as_ref()
            .and_then(Selection::bounds)
            .is_some()
            && layer.image.is_some()
        {
            return Some(CanvasContext::Selection);
        }
        if self.selected_layer_ids().len() > 1 {
            return Some(CanvasContext::Layers);
        }
        if layer.is_group() {
            return Some(CanvasContext::Folder);
        }
        if objects::live_text(layer).ok().flatten().is_some() {
            return Some(CanvasContext::Text);
        }
        if self.layer_action_unavailable("edit-object").is_none() {
            return Some(CanvasContext::Artwork);
        }
        layer.image.as_ref().map(|_| CanvasContext::Photo)
    }

    fn canvas_context_disabled(&self, context: CanvasContext, id: &str) -> bool {
        if context == CanvasContext::VectorDraft {
            return !self.vector_context_ready();
        }
        if matches!(
            id,
            "ask-omuse" | "deselect" | "commit-selection" | "cancel-selection"
        ) {
            return false;
        }
        self.layer_action_unavailable(id).is_some()
            || self.layer_action_unavailable("duplicate").is_some()
            || self.studio_action_disabled(id)
    }

    pub(super) fn run_canvas_context(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(context) = self.canvas_context(cx) else {
            return;
        };
        // A queued click must not act on a different context after a gesture,
        // modal, selection change or background completion.
        if !context.actions().iter().any(|(action, _)| *action == id)
            || self.canvas_context_disabled(context, id)
        {
            return;
        }
        // A command can remove its initiating button (Ungroup, Deselect,
        // Keep). Start from canvas focus so the next shortcut has a live
        // target; commands opening text or dialogs may then choose their own.
        self.focus.focus(window, cx);
        self.command(id, window, cx);
    }

    pub(super) fn canvas_context_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(context) = self.canvas_context(cx) else {
            return div().into_any_element();
        };
        let theme = cx.omarchy().clone();
        let mut surface = div()
            .id("canvas-context-toolbar")
            .debug_selector(|| "canvas-context-toolbar".into())
            .occlude()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap_1()
            .max_w_full()
            .p_1()
            .rounded(px(6.))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_2()
                    .text_size(px(11.))
                    .text_color(theme.foreground)
                    .child(context.title()),
            );
        for &(id, label) in context.actions() {
            let chord = self.shortcuts.chord(id);
            let source = (
                self.editor.instance_id(),
                self.editor.revision(),
                self.editor.selection_revision(),
                self.create.epoch,
                self.selected_layer_ids(),
            );
            let tooltip = if chord.is_empty() {
                label.to_owned()
            } else {
                format!("{label} · {}", shortcuts::display_chord(chord))
            };
            surface = surface.child(with_tooltip(
                inspector_ui::panel_button(
                    format!("canvas-context-{id}"),
                    label,
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(move || format!("canvas-context-{id}"))
                .disabled(self.canvas_context_disabled(context, id))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if source
                        == (
                            this.editor.instance_id(),
                            this.editor.revision(),
                            this.editor.selection_revision(),
                            this.create.epoch,
                            this.selected_layer_ids(),
                        )
                    {
                        this.run_canvas_context(id, window, cx);
                    }
                })),
                tooltip,
            ));
        }
        surface = surface.child(with_tooltip(
            inspector_ui::panel_button("canvas-context-hide", "×", ButtonVariant::Secondary, cx)
                .accessibility_label("Hide canvas actions")
                .debug_selector(|| "canvas-context-hide".into())
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.canvas_context(cx).is_some() {
                        this.focus.focus(window, cx);
                        this.command("context-toolbar", window, cx);
                    }
                })),
            "Hide canvas actions · restore with Ctrl+K → Canvas actions",
        ));
        div()
            .absolute()
            .left(px(12.))
            .right(px(12.))
            .bottom(px(12.))
            .flex()
            .justify_center()
            .child(surface)
            .into_any_element()
    }
}

fn visible_layer(layers: &[Layer], id: &str, parents_visible: bool) -> bool {
    layers.iter().any(|layer| {
        let visible = parents_visible && layer.visible;
        if layer.id == id {
            visible
        } else {
            visible_layer(&layer.children, id, visible)
        }
    })
}
