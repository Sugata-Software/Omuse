//! One reversible Shape Builder gesture on the main canvas.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::{vector_builder::ShapeBuilder, vector_scene::VectorScene};
use std::collections::BTreeSet;

pub(super) struct BuilderDraft {
    pub(super) regions: Arc<ShapeBuilder>,
    pub(super) chosen: Vec<usize>,
    source: VectorScene,
    selected: BTreeSet<usize>,
    dragging: bool,
    erase: bool,
    previous: Option<VectorPoint>,
}

impl EditorView {
    pub(super) fn start_scene_builder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
            if scene.builder.take().is_some() {
                self.status = "Shape Builder closed · Artwork unchanged".into();
                cx.notify();
                return;
            }
        }
        let prepared = (|| -> Result<_> {
            self.update_vector_style(cx)?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            ensure!(
                (2..=8).contains(&scene.selected_objects.len()),
                "Select 2–8 filled paths for Shape Builder"
            );
            ensure!(
                scene.selected_objects.last().unwrap() - scene.selected_objects.first().unwrap()
                    + 1
                    == scene.selected_objects.len(),
                "Select consecutive paths in the object stack so Shape Builder preserves stacking order"
            );
            let source = draft.scene_snapshot()?;
            let objects = scene
                .selected_objects
                .iter()
                .map(|i| source.objects.get(*i).cloned().context("Selection changed"))
                .collect::<Result<Vec<_>>>()?;
            Ok((
                source,
                objects,
                scene.selected_objects.clone(),
                draft.cancel.clone(),
            ))
        })();
        let (source, objects, selected, token) = match prepared {
            Ok(p) => p,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        let task_token = token.clone();
        let task = cx
            .background_executor()
            .spawn(async move { omuse::vector_builder::prepare(&objects, &task_token) });
        self.busy = true;
        self.status = "Finding editable shape regions… Escape cancels".into();
        self.focus.focus(window, cx);
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.vector_draft.as_ref().is_some_and(|d| Arc::ptr_eq(&d.cancel, &token)) { return; }
                this.busy = false;
                if !this.vector_scene_current() || !this.vector_draft.as_ref().is_some_and(|d| d.scene_snapshot().is_ok_and(|s| s == source) && d.scene.as_ref().is_some_and(|s| s.selected_objects == selected)) {
                    this.status = "Artwork changed; Shape Builder preparation discarded".into(); cx.notify(); return;
                }
                match result {
                    Ok(regions) => {
                        let scene = this.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
                        scene.mode = scene::SceneMode::Select;
                        scene.builder = Some(BuilderDraft { regions: Arc::new(regions), chosen: Vec::new(), source, selected, dragging: false, erase: false, previous: None });
                        this.status = "Shape Builder · Drag across regions to merge · Alt-drag erases · One gesture, then Undo to compare".into();
                    }
                    Err(error) => this.status = format!("Shape Builder: {error:#}"),
                }
                this.focus.focus(window, cx); cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn builder_pointer(
        &mut self,
        point: VectorPoint,
        down: bool,
        alt: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(builder) = self
            .vector_draft
            .as_mut()
            .and_then(|d| d.scene.as_mut())
            .and_then(|s| s.builder.as_mut())
        else {
            return false;
        };
        if down {
            builder.chosen.clear();
            builder.dragging = true;
            builder.erase = alt;
            builder.previous = None;
        }
        if builder.dragging {
            let crossed = if let Some(previous) = builder.previous {
                builder.regions.hit_test_segment(previous, point)
            } else {
                Ok(builder.regions.hit_test(point).into_iter().collect())
            };
            let crossed = match crossed {
                Ok(crossed) => crossed,
                Err(error) => {
                    builder.chosen.clear();
                    builder.dragging = false;
                    builder.previous = None;
                    self.status = format!("Shape Builder gesture cancelled: {error:#}");
                    cx.notify();
                    return true;
                }
            };
            builder.previous = Some(point);
            for id in crossed {
                if !builder.chosen.contains(&id) {
                    builder.chosen.push(id);
                }
            }
            self.status = format!(
                "Shape Builder · {} {} regions · Release to preview",
                if builder.erase { "Erase" } else { "Merge" },
                builder.chosen.len()
            );
            cx.notify();
        }
        true
    }

    pub(super) fn finish_builder_gesture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.busy {
            return false;
        }
        let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) else {
            return false;
        };
        if !scene.builder.as_ref().is_some_and(|b| b.dragging) {
            return false;
        }
        let mut builder = scene.builder.take().unwrap();
        if builder.chosen.is_empty() {
            builder.dragging = false;
            scene.builder = Some(builder);
            return true;
        }
        let token = self.vector_draft.as_ref().unwrap().cancel.clone();
        let task_token = token.clone();
        let original = builder.source.clone();
        let original_selection = builder.selected.clone();
        let task = cx.background_executor().spawn(async move {
            let mut objects = if builder.erase {
                builder.regions.erase(&builder.chosen, &task_token)?
            } else {
                builder.regions.merge(&builder.chosen, &task_token)?
            };
            let mut groups = builder.source.objects[*builder.selected.first().unwrap()]
                .groups
                .clone();
            for index in &builder.selected {
                let common = groups
                    .iter()
                    .zip(&builder.source.objects[*index].groups)
                    .take_while(|(a, b)| a == b)
                    .count();
                groups.truncate(common);
            }
            for object in &mut objects {
                object.groups = groups.clone();
            }
            let insertion = builder.selected.last().unwrap() + 1 - builder.selected.len();
            let count = objects.len();
            let mut artwork = builder.source;
            artwork.objects = artwork
                .objects
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !builder.selected.contains(i))
                .map(|(_, o)| o)
                .collect();
            artwork.objects.splice(insertion..insertion, objects);
            if artwork.objects.is_empty() {
                artwork
                    .objects
                    .push(scene::new_object("Path 1", VectorPath::default(), [0; 4]));
            }
            artwork.validate()?;
            Ok::<_, anyhow::Error>((artwork, (insertion..insertion + count).collect()))
        });
        self.busy = true;
        self.status = "Building editable shapes… Escape cancels".into();
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this
                    .vector_draft
                    .as_ref()
                    .is_some_and(|d| Arc::ptr_eq(&d.cancel, &token))
                {
                    return;
                }
                this.busy = false;
                if !this.vector_scene_current()
                    || !this.vector_draft.as_ref().is_some_and(|d| {
                        d.scene_snapshot().is_ok_and(|s| s == original)
                            && d.scene
                                .as_ref()
                                .is_some_and(|s| s.selected_objects == original_selection)
                    })
                {
                    this.status = "Artwork changed; Shape Builder result discarded".into();
                    cx.notify();
                    return;
                }
                match result.and_then(|(artwork, selected)| {
                    this.replace_scene_artwork(artwork, selected, window, cx)
                }) {
                    Ok(()) => this.status =
                        "Shape Builder preview · Ctrl+Z restores originals · Enter keeps artwork"
                            .into(),
                    Err(error) => this.status = format!("Shape Builder: {error:#}"),
                }
                this.start_scene_preview(cx);
                this.focus.focus(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }
}
