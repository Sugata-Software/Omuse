//! Shared layer prerequisites for menus, shortcut search, and command dispatch.
use super::*;
use std::collections::HashSet;

const ACTIONS: &[(&str, &str)] = &[
    ("edit-object", "Edit text / shape"),
    ("rasterize", "Convert to pixels"),
    ("transform", "Transform…"),
    ("rename", "Rename"),
    ("group", "Group selected layers"),
    ("duplicate", "Duplicate selected layers"),
    ("delete", "Delete selected layers"),
    ("lock", "Lock layer"),
    ("visibility", "Hide layer"),
    ("clipping", "Create clipping mask"),
    ("add-mask", "Add mask"),
    ("remove-mask", "Delete mask"),
    ("clear-effects", "Delete layer effects"),
    ("nest", "Move into group"),
    ("unnest", "Move to top level"),
    ("merge", "Merge down"),
];

fn context<'a>(layers: &'a [Layer], id: &str, path: &mut Vec<&'a Layer>) -> bool {
    for layer in layers {
        path.push(layer);
        if layer.id == id || context(&layer.children, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn contains_locked(layer: &Layer) -> bool {
    layer.locked || layer.children.iter().any(contains_locked)
}

fn visit<'a>(layers: &'a [Layer], result: &mut Vec<&'a Layer>) {
    for layer in layers {
        result.push(layer);
        visit(&layer.children, result);
    }
}

fn descendants<'a>(layer: &'a Layer, ids: &mut HashSet<&'a str>) {
    ids.insert(&layer.id);
    for child in &layer.children {
        descendants(child, ids);
    }
}

fn has_live_object(layer: &Layer) -> bool {
    objects::live_text(layer).ok().flatten().is_some()
        || objects::live_shape(layer).ok().flatten().is_some()
}

impl EditorView {
    /// Read-only admission checks. The editor still validates each operation
    /// atomically, including resource budgets and dependency consistency.
    pub(super) fn layer_action_unavailable(&self, id: &str) -> Option<&'static str> {
        if !ACTIONS.iter().any(|(action, _)| *action == id)
            && !matches!(
                id,
                "apply-mask"
                    | "invert-mask"
                    | "mask-enable"
                    | "mask-link"
                    | "mask-transform"
                    | "mask-paint"
                    | "effects"
            )
        {
            return None;
        }
        if self.busy {
            return Some("Wait for the current operation to finish");
        }
        if self.editor.floating_selection_layer().is_some() {
            return Some("Commit or cancel the floating selection first");
        }
        if id == "mask-paint" && self.paint_mask {
            return None;
        }
        let document = &self.editor.document;
        let mut path = Vec::new();
        if !context(&document.layers, &self.editor.active_layer, &mut path) {
            return Some("Select a layer first");
        }
        let layer = *path.last().unwrap();
        // These are organisational changes, and remain useful on locked art.
        if matches!(id, "lock" | "visibility" | "rename") {
            return None;
        }
        if path.iter().any(|layer| layer.locked) {
            return Some("Unlock this layer and its parent groups first");
        }
        let roots = if matches!(id, "group" | "duplicate" | "delete" | "nest" | "unnest") {
            let roots = self.editor.selected_layer_roots(&self.selected_layer_ids());
            if roots.is_empty() {
                return Some("Select one or more layers first");
            }
            for selected in &roots {
                let mut selected_path = Vec::new();
                if !context(&document.layers, selected, &mut selected_path)
                    || selected_path.iter().any(|layer| layer.locked)
                    || selected_path
                        .last()
                        .is_some_and(|layer| contains_locked(layer))
                {
                    return Some("Unlock all selected layers, children, and parent groups first");
                }
            }
            roots
        } else {
            Vec::new()
        };
        let below = || {
            layer_position(&document.layers, &layer.id, None).and_then(|(parent, index, _)| {
                let siblings = parent
                    .as_ref()
                    .and_then(|parent| document.find_layer(parent))
                    .map_or(&document.layers[..], |parent| &parent.children);
                index.checked_sub(1).and_then(|index| siblings.get(index))
            })
        };
        match id {
            "edit-object" => {
                if !has_live_object(layer) {
                    Some("Select an editable text or shape layer")
                } else if path.iter().any(|layer| !layer.visible) {
                    Some("Show this layer and its parent groups before editing")
                } else {
                    None
                }
            }
            "rasterize" if layer.advanced.is_none() && !has_live_object(layer) => {
                Some("This layer already contains pixels or has no editable source")
            }
            "transform" if layer.is_group() => Some(
                "Select a child layer for numeric transforms; use Move to reposition the group",
            ),
            "transform" if layer.image.is_none() => {
                Some("Select a pixel layer, text, or shape to transform")
            }
            "add-mask" if layer.mask.is_some() => Some("This layer already has a mask"),
            "remove-mask" | "apply-mask" | "invert-mask" | "mask-enable" | "mask-link"
            | "mask-transform"
                if layer.mask.is_none() =>
            {
                Some("Add a layer mask first")
            }
            "mask-paint" if !self.paint_mask && layer.mask.is_none() => {
                Some("Add a layer mask first")
            }
            "apply-mask" if layer.advanced.is_some() || has_live_object(layer) => {
                Some("Convert a copy to pixels before baking its mask")
            }
            "apply-mask" if layer.image.is_none() => Some("Apply mask requires a pixel layer"),
            "effects" if layer.image.is_none() => {
                Some("Select a pixel, text, or shape layer for effects")
            }
            "clear-effects"
                if layer.image.is_none()
                    || !layer
                        .metadata
                        .get("effects")
                        .and_then(|effects| effects.as_object())
                        .is_some_and(|effects| effects.values().any(|value| !value.is_null())) =>
            {
                Some("This layer has no layer effects to delete")
            }
            "clipping"
                if layer
                    .metadata
                    .get("maskSourceID")
                    .and_then(|value| value.as_str())
                    .is_none()
                    && below().is_none_or(|below| below.image.is_none()) =>
            {
                Some("Place a pixel layer immediately below to use as the clipping source")
            }
            "delete" => {
                let mut removed = HashSet::new();
                for id in &roots {
                    descendants(document.find_layer(id).unwrap(), &mut removed);
                }
                let mut all = Vec::new();
                visit(&document.layers, &mut all);
                all.iter()
                    .any(|other| {
                        !removed.contains(other.id.as_str())
                            && other
                                .metadata
                                .get("maskSourceID")
                                .and_then(|value| value.as_str())
                                .is_some_and(|source| removed.contains(source))
                    })
                    .then_some(
                        "Select dependent live-mask layers too, or unlink their mask source first",
                    )
            }
            "nest" => {
                let mut excluded = HashSet::new();
                for id in &roots {
                    descendants(document.find_layer(id).unwrap(), &mut excluded);
                }
                let mut all = Vec::new();
                visit(&document.layers, &mut all);
                let valid = all
                    .iter()
                    .filter(|target| target.is_group() && !excluded.contains(target.id.as_str()))
                    .any(|target| {
                        let mut target_path = Vec::new();
                        context(&document.layers, &target.id, &mut target_path)
                            && target_path.iter().all(|layer| !layer.locked)
                    });
                (!valid).then_some("Create or unlock another group to move these layers into")
            }
            "unnest"
                if roots.iter().all(|id| {
                    layer_position(&document.layers, id, None)
                        .is_none_or(|(parent, _, _)| parent.is_none())
                }) =>
            {
                Some("Selected layers are already at the top level")
            }
            "merge" => {
                if let Some(reason) = self.editor.merge_down_preservation_reason() {
                    return Some(reason);
                }
                let Some(below) = below() else {
                    return Some("Select a layer with another layer immediately below");
                };
                if below.locked {
                    return Some("Unlock the layer below before merging");
                }
                if below.image.is_none()
                    || layer.image.is_none()
                    || below.is_group()
                    || layer.is_group()
                {
                    return Some("Merge down needs two adjacent image layers");
                }
                if !below.visible || path.iter().any(|layer| !layer.visible) {
                    return Some("Show both layers and their parent groups before merging");
                }
                if [below, layer]
                    .iter()
                    .any(|layer| !layer.blend_mode.eq_ignore_ascii_case("normal"))
                {
                    return Some("Set both layers to Normal blending before merging");
                }
                if path[..path.len() - 1].iter().any(|parent| {
                    parent.opacity != 1.
                        || (parent.mask.is_some()
                            && parent
                                .metadata
                                .get("maskEnabled")
                                .and_then(|value| value.as_bool())
                                != Some(false))
                }) {
                    return Some(
                        "Use a parent group at full opacity with its mask disabled before merging",
                    );
                }
                None
            }
            _ => None,
        }
    }

    fn layer_action_label(&self, id: &str, fallback: &'static str) -> &'static str {
        let Some(layer) = self.editor.document.find_layer(&self.editor.active_layer) else {
            return fallback;
        };
        match id {
            "lock" if layer.locked => "Unlock layer",
            "visibility" if !layer.visible => "Show layer",
            "clipping"
                if layer
                    .metadata
                    .get("maskSourceID")
                    .and_then(|value| value.as_str())
                    .is_some() =>
            {
                "Release clipping mask"
            }
            "edit-object" if objects::live_text(layer).ok().flatten().is_some() => "Edit text…",
            "edit-object" if objects::live_shape(layer).ok().flatten().is_some() => "Edit shape…",
            _ => fallback,
        }
    }

    /// Opening the chooser is harmless even when the current parent is the
    /// only folder. Explain unchanged/cyclic/locked choices at the destination,
    /// instead of preventing the user from inspecting the hierarchy.
    pub(super) fn layer_nest_target_unavailable(&self, target: &str) -> Option<&'static str> {
        let document = &self.editor.document;
        let mut target_path = Vec::new();
        if !context(&document.layers, target, &mut target_path)
            || !target_path.last().is_some_and(|layer| layer.is_group())
        {
            return Some("Choose an existing group");
        }
        if target_path.iter().any(|layer| layer.locked) {
            return Some("Unlock this group and its parents first");
        }
        let roots = self.editor.selected_layer_roots(&self.selected_layer_ids());
        if roots.is_empty() {
            return Some("Select layers to move first");
        }
        for id in &roots {
            let mut selected_path = Vec::new();
            context(&document.layers, id, &mut selected_path);
            if selected_path.iter().any(|layer| layer.locked)
                || selected_path
                    .last()
                    .is_some_and(|layer| contains_locked(layer))
            {
                return Some("Unlock selected layers and their children first");
            }
            if target_path.iter().any(|layer| &layer.id == id) {
                return Some("A group cannot be moved into itself or its children");
            }
        }
        roots
            .iter()
            .all(|id| {
                layer_position(&document.layers, id, None)
                    .is_some_and(|(parent, _, _)| parent.as_deref() == Some(target))
            })
            .then_some("Selected layers are already in this group")
    }

    pub(super) fn run_layer_action(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog != Dialog::LayerMenu || !ACTIONS.iter().any(|(action, _)| *action == id) {
            return;
        }
        if let Some(reason) = self.command_search_unavailable(id) {
            self.status = reason.into();
            cx.notify();
            return;
        }
        self.dialog = Dialog::None;
        self.focus.focus(window, cx);
        self.command(id, window, cx);
    }

    pub(super) fn layer_actions_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.omarchy().clone();
        let layer_name = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .map_or("No selected layer", |layer| layer.name.as_str());
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .text_sm()
                .text_color(theme.secondary)
                .child(layer_name.to_owned()),
        );
        for &(id, fallback) in ACTIONS {
            let reason = self.command_search_unavailable(id);
            let mut row = div()
                .flex()
                .flex_col()
                .items_start()
                .gap_1()
                .child(div().child(self.layer_action_label(id, fallback)));
            if let Some(reason) = reason {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(theme.secondary)
                        .whitespace_normal()
                        .child(reason),
                );
            }
            body = body.child(
                button(
                    SharedString::from(format!("layer-action-{id}")),
                    "",
                    ButtonVariant::Outline,
                    cx,
                )
                .debug_selector(move || format!("layer-action-{id}"))
                .accessibility_label(self.layer_action_label(id, fallback))
                .disabled(reason.is_some())
                .w_full()
                .h_auto()
                .min_h(px(36.))
                .py_2()
                .justify_start()
                .child(row)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.run_layer_action(id, window, cx)),
                ),
            );
        }
        body.child(
            button("dismiss-layer-menu", "Close", ButtonVariant::Outline, cx).on_click(
                cx.listener(|this, _, window, cx| {
                    this.dialog = Dialog::None;
                    this.focus.focus(window, cx);
                    cx.notify();
                }),
            ),
        )
        .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn layer_actions_merge_explains_external_mask_dependencies_and_does_not_commit(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            let mut document = Document::new(6, 6);
            let upper = Layer::paint("Upper", 6, 6);
            let id = upper.id.clone();
            let mut dependent = Layer::paint("Dependent", 6, 6);
            dependent.metadata["maskSourceID"] = serde_json::json!(document.layers[0].id);
            document.layers.extend([upper, dependent]);
            view.editor = Editor::new(document);
            view.select_layer_ids(vec![id]);
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            assert!(
                view.command_search_unavailable("merge")
                    .unwrap()
                    .contains("live-mask source")
            );
            let revision = view.editor.revision();
            view.command("merge", window, cx);
            assert_eq!(view.editor.revision(), revision);
            assert_eq!(view.editor.document.layers.len(), 3);
            assert!(view.status.contains("live-mask source"));
            assert!(raster::validate(&view.editor.document).is_empty());
        });
    }

    #[gpui_kit::test]
    fn layer_actions_nest_chooser_explains_current_parent_and_rejects_cycles(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(6, 6));
            view
        });
        view.update_in(cx, |view, window, cx| {
            let child = view.editor.active_layer.clone();
            let parent = view
                .editor
                .group_layers(std::slice::from_ref(&child), "Parent")
                .unwrap();
            view.select_layer_ids(vec![parent.clone()]);
            let revision = view.editor.revision();
            assert!(
                view.command_search_unavailable("transform")
                    .unwrap()
                    .contains("child layer")
            );
            view.command("transform", window, cx);
            assert_eq!(view.dialog, Dialog::None);
            assert_eq!(view.editor.revision(), revision);
            view.select_layer_ids(vec![child]);
            let revision = view.editor.revision();
            assert!(view.layer_action_unavailable("nest").is_none());
            view.command("nest", window, cx);
            assert_eq!(view.dialog, Dialog::Nest);
            assert_eq!(
                view.layer_nest_target_unavailable(&parent),
                Some("Selected layers are already in this group")
            );
            assert_eq!(view.editor.revision(), revision);
            let other = view.editor.add_group("Other");
            assert!(view.layer_nest_target_unavailable(&other).is_none());
            view.select_layer_ids(vec![parent.clone()]);
            assert!(
                view.layer_nest_target_unavailable(&parent)
                    .unwrap()
                    .contains("itself")
            );
        });
    }

    #[gpui_kit::test]
    fn layer_actions_mask_prerequisites_share_search_and_command_guards(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(6, 6));
            view
        });
        view.update_in(cx, |view, window, cx| {
            assert_eq!(
                view.command_search_unavailable("remove-mask"),
                Some("Add a layer mask first")
            );
            assert!(view.command_search_unavailable("add-mask").is_none());
            let revision = view.editor.revision();
            view.command("remove-mask", window, cx);
            assert_eq!(view.editor.revision(), revision);
            assert_eq!(view.status, "Add a layer mask first");
            view.command("add-mask", window, cx);
            assert!(view.command_search_unavailable("add-mask").is_some());
            assert!(view.command_search_unavailable("remove-mask").is_none());
            view.dialog = Dialog::LayerMenu;
            view.run_layer_action("remove-mask", window, cx);
            assert_eq!(view.dialog, Dialog::None);
            assert!(
                view.editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .unwrap()
                    .mask
                    .is_none()
            );
            assert!(view.editor.undo());
            assert!(
                view.editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .unwrap()
                    .mask
                    .is_some()
            );
        });
    }

    #[gpui_kit::test]
    fn layer_actions_locked_parent_and_stale_menu_never_mutate_artwork(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(6, 6));
            view
        });
        view.update_in(cx, |view, window, cx| {
            let child = view.editor.active_layer.clone();
            let group = view
                .editor
                .group_layers(std::slice::from_ref(&child), "Protected")
                .unwrap();
            view.select_layer_ids(vec![child.clone()]);
            view.dialog = Dialog::LayerMenu;
            assert!(view.command_search_unavailable("delete").is_none());
            view.editor.set_locked(&group, true);
            let revision = view.editor.revision();
            view.run_layer_action("delete", window, cx);
            assert_eq!(view.dialog, Dialog::LayerMenu);
            assert_eq!(view.editor.revision(), revision);
            assert!(view.editor.document.find_layer(&child).is_some());
            for action in ["duplicate", "group", "transform", "add-mask", "unnest"] {
                assert!(
                    view.command_search_unavailable(action)
                        .unwrap()
                        .contains("Unlock"),
                    "{action}"
                );
            }
            assert!(view.command_search_unavailable("visibility").is_none());
            assert!(view.command_search_unavailable("lock").is_none());
            view.select_layer_ids(vec![group.clone()]);
            view.run_layer_action("lock", window, cx);
            assert!(!view.editor.document.find_layer(&group).unwrap().locked);
            view.editor.document.find_layer_mut(&child).unwrap().locked = true;
            assert!(
                view.command_search_unavailable("delete")
                    .unwrap()
                    .contains("children")
            );
        });
    }

    #[gpui_kit::test]
    fn layer_actions_distinguish_raster_text_group_and_live_mask_dependencies(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 12));
            view
        });
        view.update_in(cx, |view, _window, _cx| {
            let source = view.editor.active_layer.clone();
            assert!(view.command_search_unavailable("edit-object").is_some());
            assert!(view.command_search_unavailable("rasterize").is_some());
            assert!(view.command_search_unavailable("merge").is_some());
            assert!(view.command_search_unavailable("nest").is_some());
            let text = objects::live_text_layer(
                "Editable",
                objects::ObjectPoint { x: 0., y: 0. },
                objects::LiveTextStyle {
                    content: "Hello".into(),
                    font_size: 10.,
                    ..Default::default()
                },
            )
            .unwrap();
            let text = view.editor.insert_layer(text);
            view.select_layer_ids(vec![text.clone()]);
            assert!(view.command_search_unavailable("edit-object").is_none());
            assert!(view.command_search_unavailable("rasterize").is_none());
            assert_eq!(view.layer_action_label("edit-object", "Edit"), "Edit text…");
            view.editor.add_mask(&text, true);
            assert!(
                view.command_search_unavailable("apply-mask")
                    .unwrap()
                    .contains("Convert")
            );
            view.editor
                .set_live_mask_source(&text, Some(&source))
                .unwrap();
            view.select_layer_ids(vec![source.clone()]);
            assert!(
                view.command_search_unavailable("delete")
                    .unwrap()
                    .contains("dependent")
            );
            view.select_layer_ids(vec![source, text]);
            assert!(view.command_search_unavailable("delete").is_none());
        });
    }
}
