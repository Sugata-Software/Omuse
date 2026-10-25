//! Retained clipping actions in the shared vector workspace.
use super::*;
use anyhow::{Context as _, Result};
use omuse::vector_scene::VectorScene;
use std::collections::BTreeSet;

/// Resolve one relationship only: never silently drop unrelated selected art.
pub(super) fn edit_targets(
    artwork: &VectorScene,
    selected: &BTreeSet<usize>,
) -> Option<(usize, BTreeSet<usize>)> {
    let mut masks = BTreeSet::new();
    for index in selected {
        let object = artwork.objects.get(*index)?;
        let id = if object.is_clip {
            &object.id
        } else {
            object.clip_to.as_ref()?
        };
        masks.insert(id);
    }
    if masks.len() != 1 {
        return None;
    }
    let id = *masks.first()?;
    let mask = artwork
        .objects
        .iter()
        .position(|o| o.is_clip && &o.id == id)?;
    let contents: BTreeSet<_> = artwork
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| o.clip_to.as_ref() == Some(id))
        .map(|(i, _)| i)
        .collect();
    (!contents.is_empty()).then_some((mask, contents))
}

impl EditorView {
    fn clip_edit_ready(&self) -> bool {
        !self.busy
            && self.vector_scene_current()
            && self.vector_draft.as_ref().is_some_and(|draft| {
                draft.drag.is_none()
                    && draft.scene.as_ref().is_some_and(|scene| {
                        scene.drag_origin.is_none()
                            && scene.gradient_drag.is_none()
                            && scene.marquee.is_none()
                            && scene.builder.is_none()
                    })
            })
    }
    pub(super) fn scene_clip_edit(
        &mut self,
        mask: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.clip_edit_ready() {
            return;
        }
        if let Err(error) = self.update_vector_style(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let Some((mask_index, contents)) = edit_targets(&scene.artwork, &scene.selected_objects)
        else {
            self.status = "Select one clipping mask or its contents".into();
            cx.notify();
            return;
        };
        let selected = if mask {
            [mask_index].into_iter().collect()
        } else {
            contents
        };
        let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
        while !selected.iter().all(|i| {
            super::vector_isolation_ui::in_scope(&scene.isolation, &scene.artwork.objects[*i])
        }) {
            scene.isolation.pop();
        }
        scene.mode = if mask {
            scene::SceneMode::Nodes
        } else {
            scene::SceneMode::Select
        };
        scene.gradient_edit = false;
        self.set_scene_selection(selected, mask.then_some(mask_index), window, cx);
        self.tool = Tool::Move;
        self.status = if mask {
            "Editing clipping mask · Drag its nodes · Edit contents switches back"
        } else {
            "Editing clipped contents · Full geometry remains editable · Ctrl-click selects one object"
        }.into();
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn scene_clip_edit_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let scene = self.vector_draft.as_ref().unwrap().scene.as_ref().unwrap();
        let available = edit_targets(&scene.artwork, &scene.selected_objects).is_some();
        let mut row = div().flex().gap_1();
        for (id, label, mask) in [
            ("vector-clip-edit-mask", "Edit mask", true),
            ("vector-clip-edit-contents", "Edit contents", false),
        ] {
            row = row.child(
                button(id, label, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .disabled(!available || !self.clip_edit_ready())
                    .flex_1()
                    .min_w_0()
                    .on_click(cx.listener(move |this, _, w, cx| this.scene_clip_edit(mask, w, cx))),
            );
        }
        row.into_any_element()
    }

    pub(super) fn scene_clip_action(
        &mut self,
        release: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<()> {
            self.update_vector_style(cx)?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            let selected: Vec<_> = scene.selected_objects.iter().copied().collect();
            let source = draft.scene_snapshot()?;
            let (artwork, selected) = if release {
                omuse::vector_clip::release(&source, &selected)?
            } else {
                omuse::vector_clip::make(&source, &selected)?
            };
            self.replace_scene_artwork(artwork, selected.into_iter().collect(), window, cx)?;
            self.status = if release {
                "Clipping released · Original shapes restored · Ctrl+Z to undo".into()
            } else {
                "Editable clipping created · Select the mask in Objects to edit its points · Ctrl+Z to undo".into()
            };
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Vector clipping: {error:#}");
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
