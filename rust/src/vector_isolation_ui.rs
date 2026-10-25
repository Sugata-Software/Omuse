//! View-only group isolation. Saved artwork and exports retain their paint.
use super::scene::{SceneDraft, SceneMode};
use super::*;
use omuse::vector_scene::VectorObject;
use std::collections::BTreeSet;

pub(super) fn in_scope(scope: &[String], object: &VectorObject) -> bool {
    object.groups.len() >= scope.len()
        && scope
            .iter()
            .zip(&object.groups)
            .all(|(id, group)| id == &group.id)
}

pub(super) fn members(scene: &SceneDraft, index: usize) -> BTreeSet<usize> {
    let Some(object) = scene
        .artwork
        .objects
        .get(index)
        .filter(|o| in_scope(&scene.isolation, o))
    else {
        return BTreeSet::new();
    };
    let depth = scene.isolation.len();
    match object.groups.get(depth) {
        Some(group) => scene
            .artwork
            .objects
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                in_scope(&scene.isolation, item)
                    && item.groups.get(depth).is_some_and(|g| g.id == group.id)
            })
            .map(|(i, _)| i)
            .collect(),
        None => [index].into_iter().collect(),
    }
}

impl EditorView {
    pub(in crate::ui) fn vector_isolation_depth(&self) -> usize {
        self.vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .map_or(0, |s| s.isolation.len())
    }
    pub(super) fn reconcile_vector_isolation(&mut self) {
        let Some(draft) = self.vector_draft.as_mut() else {
            return;
        };
        let Some(scene) = draft.scene.as_mut() else {
            return;
        };
        while !scene.isolation.is_empty()
            && !scene
                .artwork
                .objects
                .iter()
                .any(|o| in_scope(&scene.isolation, o))
        {
            scene.isolation.pop();
        }
        scene.selected_objects.retain(|i| {
            scene
                .artwork
                .objects
                .get(*i)
                .is_some_and(|o| in_scope(&scene.isolation, o))
        });
        if !scene
            .artwork
            .objects
            .get(scene.active)
            .is_some_and(|o| in_scope(&scene.isolation, o))
        {
            if let Some(index) = scene.selected_objects.iter().next().copied().or_else(|| {
                scene
                    .artwork
                    .objects
                    .iter()
                    .position(|o| in_scope(&scene.isolation, o))
            }) {
                scene.active = index;
                let object = &scene.artwork.objects[index];
                draft.path = object.path.clone();
                draft.fill = object.fill.unwrap_or([0; 4]);
                draft.stroke = object.stroke;
                draft.selected = None;
                draft.drag = None;
            }
        }
    }

    pub(super) fn enter_vector_isolation(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.busy || self.update_vector_style(cx).is_err() {
            return false;
        }
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return false;
        };
        let Some(object) = scene
            .artwork
            .objects
            .get(index)
            .filter(|o| in_scope(&scene.isolation, o))
        else {
            return false;
        };
        let Some(group) = object.groups.get(scene.isolation.len()) else {
            return false;
        };
        scene.isolation.push(group.id.clone());
        scene.gradient_edit = false;
        scene.mode = SceneMode::Select;
        let selected = members(scene, index);
        self.set_scene_selection(selected, Some(index), window, cx);
        self.vector_scene_changed(cx);
        self.status =
            "Group isolated · Double-click a nested group to enter · Escape steps out".into();
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(in crate::ui) fn leave_vector_isolation(
        &mut self,
        depth: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.busy {
            return false;
        }
        let Some(scene) = self.vector_draft.as_ref().and_then(|d| d.scene.as_ref()) else {
            return false;
        };
        if scene.isolation.len() <= depth {
            return false;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return true;
        }
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        scene.isolation.truncate(depth);
        scene.drag_origin = None;
        scene.gradient_drag = None;
        scene.marquee = None;
        self.vector_scene_changed(cx);
        self.status = "Left isolated group · Artwork changes retained".into();
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn vector_isolation_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let mut row = div().flex().items_center().gap_1().flex_wrap();
        if scene.isolation.is_empty() {
            return row.into_any_element();
        }
        row = row.child(
            button(
                "vector-isolation-root",
                "Artwork",
                ButtonVariant::Outline,
                cx,
            )
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, w, cx| {
                this.leave_vector_isolation(0, w, cx);
            })),
        );
        for (index, id) in scene.isolation.iter().enumerate() {
            let name = scene
                .artwork
                .objects
                .iter()
                .flat_map(|o| o.groups.iter())
                .find(|g| &g.id == id)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| "Group".into());
            let depth = index + 1;
            let key = format!("vector-isolation-{depth}");
            row = row.child(
                button(
                    SharedString::from(key),
                    SharedString::from(name),
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(depth == scene.isolation.len())
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.leave_vector_isolation(depth, w, cx);
                })),
            );
        }
        row.into_any_element()
    }
}
