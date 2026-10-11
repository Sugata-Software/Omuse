//! Disposable grid and radial repeats on the existing vector canvas.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_repeat::{RepeatSpec, generate};
use omuse::vector_scene::{MAX_SCENE_ANCHORS, MAX_SCENE_OBJECTS, MAX_SCENE_SUBPATHS, VectorScene};
use std::collections::{BTreeSet, HashSet};

/// Copies are appended in motif order. Original stack/group membership is left
/// intact; every copied group receives its own identity from the repeat engine.
fn repeat_scene(
    mut source: VectorScene,
    selected: &BTreeSet<usize>,
    spec: &RepeatSpec,
    namespace: &str,
    cancel: &AtomicBool,
) -> Result<(VectorScene, BTreeSet<usize>, usize)> {
    ensure!(!cancel.load(Ordering::Relaxed), "Vector repeat cancelled");
    source.validate()?;
    ensure!(
        !selected.is_empty() && selected.len() <= omuse::vector_repeat::MAX_REPEAT_SOURCES,
        "Select 1–64 objects to repeat"
    );
    let objects = selected
        .iter()
        .map(|i| {
            source
                .objects
                .get(*i)
                .cloned()
                .context("Object selection changed")
        })
        .collect::<Result<Vec<_>>>()?;
    let ids = objects
        .iter()
        .map(|o| uuid::Uuid::parse_str(&o.id))
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    ensure!(
        ids.len() == objects.len(),
        "Selected object identities must be unique"
    );
    let instances = spec.instance_count()? as usize;
    ensure!(
        instances > 1,
        "Choose at least two instances, including the original"
    );
    let extra = instances - 1;
    let anchors = |scene: &[omuse::vector_scene::VectorObject]| {
        scene
            .iter()
            .flat_map(|o| &o.path.subpaths)
            .map(|s| s.anchors.len())
            .sum::<usize>()
    };
    let subpaths = |scene: &[omuse::vector_scene::VectorObject]| {
        scene.iter().map(|o| o.path.subpaths.len()).sum::<usize>()
    };
    for (used, motif, limit, label) in [
        (
            source.objects.len(),
            objects.len(),
            MAX_SCENE_OBJECTS,
            "object",
        ),
        (
            anchors(&source.objects),
            anchors(&objects),
            MAX_SCENE_ANCHORS,
            "anchor",
        ),
        (
            subpaths(&source.objects),
            subpaths(&objects),
            MAX_SCENE_SUBPATHS,
            "subpath",
        ),
    ] {
        ensure!(
            motif
                .checked_mul(extra)
                .and_then(|n| used.checked_add(n))
                .is_some_and(|n| n <= limit),
            "Repeat would exceed the document's {limit}-{label} limit; use fewer instances"
        );
    }
    let snapshot = generate(&objects, spec, namespace, cancel)?;
    let original_groups = source
        .objects
        .iter()
        .flat_map(|o| &o.groups)
        .map(|g| uuid::Uuid::parse_str(&g.id))
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    for group in snapshot.objects[objects.len()..]
        .iter()
        .flat_map(|o| &o.groups)
    {
        ensure!(
            !original_groups.contains(&uuid::Uuid::parse_str(&group.id)?),
            "Repeat group identity collides with the document; try again"
        );
    }
    let start = source.objects.len();
    source
        .objects
        .extend(snapshot.objects.into_iter().skip(objects.len()));
    source
        .validate()
        .context("Repeat does not fit the current artwork")?;
    ensure!(!cancel.load(Ordering::Relaxed), "Vector repeat cancelled");
    let count = source.objects.len() - start;
    let chosen = (start..source.objects.len()).collect();
    Ok((source, chosen, count))
}

impl EditorView {
    pub(super) fn scene_repeat_action(
        &mut self,
        radial: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let prepared = (|| -> Result<_> {
            ensure!(
                self.vector_scene_current(),
                "Open current vector artwork first"
            );
            self.update_vector_style(cx)?;
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let scene = draft.scene.as_ref().context("Open vector artwork first")?;
            let number = |index: usize, label: &str| -> Result<f32> {
                let value = self.detail_inputs[index]
                    .read(cx)
                    .value()
                    .parse::<f32>()
                    .with_context(|| format!("Enter a number for {label}"))?;
                ensure!(value.is_finite(), "{label} must be a finite number");
                Ok(value)
            };
            let count = |index: usize, label: &str| -> Result<u32> {
                self.detail_inputs[index]
                    .read(cx)
                    .value()
                    .parse::<u32>()
                    .with_context(|| format!("Enter a whole number from 1 to 256 for {label}"))
            };
            let spec = if radial {
                let orientation = self.detail_inputs[28].read(cx).value();
                ensure!(
                    matches!(orientation.as_ref(), "rotate" | "keep"),
                    "Choose a radial orientation"
                );
                RepeatSpec::Radial {
                    count: count(24, "radial instances")?,
                    center: VectorPoint {
                        x: number(26, "centre X")?,
                        y: number(27, "centre Y")?,
                    },
                    angle_step_degrees: number(25, "angle step")?,
                    rotate_copies: orientation.as_ref() == "rotate",
                }
            } else {
                RepeatSpec::Grid {
                    columns: count(20, "columns")?,
                    rows: count(21, "rows")?,
                    column_step: VectorPoint {
                        x: number(22, "column step")?,
                        y: 0.,
                    },
                    row_step: VectorPoint {
                        x: 0.,
                        y: number(23, "row step")?,
                    },
                }
            };
            spec.instance_count()?;
            Ok((
                draft.scene_snapshot()?,
                scene.selected_objects.clone(),
                draft.cancel.clone(),
                spec,
            ))
        })();
        let (source, selected, token, spec) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let original = source.clone();
        let original_selection = selected.clone();
        let task_token = token.clone();
        let namespace = uuid::Uuid::new_v4().to_string();
        let task = cx
            .background_executor()
            .spawn(async move { repeat_scene(source, &selected, &spec, &namespace, &task_token) });
        self.busy = true;
        self.status = "Preparing editable repeats… Escape cancels".into();
        self.focus.focus(window, cx);
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.vector_draft.as_ref().is_some_and(|d| Arc::ptr_eq(&d.cancel, &token)) { return; }
                this.busy = false;
                let unchanged = this.vector_scene_current() && this.vector_draft.as_ref().is_some_and(|d| {
                    d.scene_snapshot().is_ok_and(|s| s == original)
                        && d.scene.as_ref().is_some_and(|s| s.selected_objects == original_selection)
                });
                if !unchanged {
                    this.status = "Artwork or selection changed; repeat result discarded".into();
                    cx.notify(); return;
                }
                match result.and_then(|(artwork, chosen, count)| {
                    this.replace_scene_artwork(artwork, chosen, window, cx)?;
                    Ok(count)
                }) {
                    Ok(count) => this.status = format!("Repeat preview · {count} editable copies · Originals retained · Ctrl+Z restores · Enter keeps edits"),
                    Err(error) => this.status = format!("Repeat: {error:#}"),
                }
                this.start_scene_preview(cx);
                this.focus.focus(window, cx);
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn vector_repeat_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .is_some_and(|s| !s.selected_objects.is_empty());
        let disabled = self.busy || !selected;
        let mut grid = div().flex().flex_wrap().gap_2();
        for (index, label) in [
            (20, "Columns"),
            (21, "Rows"),
            (22, "Column step · px"),
            (23, "Row step · px"),
        ] {
            grid = grid.child(self.vector_repeat_input(index, label, window, cx));
        }
        let mut radial = div().flex().flex_wrap().gap_2();
        for (index, label) in [
            (24, "Instances"),
            (25, "Angle step · °"),
            (26, "Centre X · px"),
            (27, "Centre Y · px"),
        ] {
            radial = radial.child(self.vector_repeat_input(index, label, window, cx));
        }
        let mut orientation = div().flex().gap_1();
        for (name, label) in [("rotate", "Rotate copies"), ("keep", "Keep upright")] {
            let id = format!("vector-repeat-orientation-{name}");
            orientation = orientation.child(
                button(
                    SharedString::from(id.clone()),
                    label,
                    ButtonVariant::Outline,
                    cx,
                )
                .selected(self.detail_inputs[28].read(cx).value().as_ref() == name)
                .disabled(self.busy)
                .flex_1()
                .min_w_0()
                .debug_selector(move || id.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.detail_inputs[28]
                        .update(cx, |input, cx| input.set_value(name, window, cx));
                    cx.notify();
                })),
            );
        }
        div().flex().flex_col().gap_2()
            .child(inspector_ui::panel_note("Grid repeat · Counts include the original", cx))
            .child(grid)
            .child(button("vector-repeat-grid", "Preview grid", ButtonVariant::Outline, cx)
                .disabled(disabled).debug_selector(|| "vector-repeat-grid".into())
                .on_click(cx.listener(|this, _, window, cx| this.scene_repeat_action(false, window, cx))))
            .child(inspector_ui::panel_note("Radial repeat · Positive angles turn clockwise", cx))
            .child(radial).child(orientation)
            .child(button("vector-repeat-radial", "Preview radial", ButtonVariant::Outline, cx)
                .disabled(disabled).debug_selector(|| "vector-repeat-radial".into())
                .on_click(cx.listener(|this, _, window, cx| this.scene_repeat_action(true, window, cx))))
            .child(inspector_ui::panel_note("New copies appear above the originals. Undo to adjust and repeat again; accepted copies are edited individually.", cx))
            .into_any_element()
    }

    fn vector_repeat_input(
        &self,
        index: usize,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = format!("vector-repeat-field-{index}");
        div()
            .flex_1()
            .min_w(px(100.))
            .flex()
            .flex_col()
            .gap_1()
            .child(inspector_ui::panel_note(label, cx))
            .child(
                input(
                    SharedString::from(id.clone()),
                    &self.detail_inputs[index],
                    window,
                    cx,
                )
                .debug_selector(move || id.clone()),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omuse::vector_scene::{VECTOR_SCENE_VERSION, VectorGroup, VectorObject};
    const NAMESPACE: &str = "5b2c5456-86e9-4474-b037-5b336fb5a0da";
    fn source() -> VectorScene {
        VectorScene {
            version: VECTOR_SCENE_VERSION,
            width: 100,
            height: 100,
            objects: (0..3)
                .map(|i| {
                    VectorObject::rectangle(
                        format!("Object {i}"),
                        10.,
                        10.,
                        10.,
                        10.,
                        Some([255; 4]),
                        None,
                    )
                    .unwrap()
                })
                .collect(),
        }
    }
    fn spec(columns: u32) -> RepeatSpec {
        RepeatSpec::Grid {
            columns,
            rows: 1,
            column_step: VectorPoint { x: 20., y: 0. },
            row_step: VectorPoint { x: 0., y: 20. },
        }
    }
    #[test]
    fn repeat_appends_copies_without_reordering_originals_or_splitting_groups() {
        let mut source = source();
        let group = VectorGroup {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Group".into(),
        };
        for object in &mut source.objects {
            object.groups = vec![group.clone()];
        }
        let originals = source.objects.clone();
        let (result, chosen, count) = repeat_scene(
            source,
            &[0, 2].into_iter().collect(),
            &spec(2),
            NAMESPACE,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(&result.objects[..3], &originals);
        assert_eq!(count, 2);
        assert_eq!(chosen, [3, 4].into_iter().collect());
        assert_eq!(result.objects[3].groups, result.objects[4].groups);
        assert_ne!(result.objects[3].groups[0].id, group.id);
        result.validate().unwrap();
    }
    #[test]
    fn repeat_rejects_stale_empty_and_noop_selections() {
        let cancel = AtomicBool::new(false);
        for selected in [BTreeSet::new(), [3].into_iter().collect()] {
            assert!(repeat_scene(source(), &selected, &spec(2), NAMESPACE, &cancel).is_err());
        }
        assert!(
            repeat_scene(
                source(),
                &[0].into_iter().collect(),
                &spec(1),
                NAMESPACE,
                &cancel
            )
            .is_err()
        );
    }
    #[test]
    fn repeat_document_budget_counts_unselected_objects() {
        let mut source = source();
        source.objects = (0..900)
            .map(|_| source.objects[0].clone())
            .enumerate()
            .map(|(i, mut o)| {
                o.id = uuid::Uuid::new_v4().to_string();
                o.name = format!("Object {i}");
                o
            })
            .collect();
        let error = repeat_scene(
            source,
            &[0].into_iter().collect(),
            &spec(256),
            NAMESPACE,
            &AtomicBool::new(false),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("document's 1024-object"), "{error}");
    }
    #[test]
    fn repeat_cancelled_before_expansion_never_returns_artwork() {
        assert!(
            repeat_scene(
                source(),
                &[0].into_iter().collect(),
                &spec(2),
                NAMESPACE,
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }
}
