//! Small vector interactions on the shared canvas, with transactional transforms.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_scene::VectorScene;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TransformStep {
    pub(super) dx: f32,
    pub(super) dy: f32,
    pub(super) scale: f32,
    pub(super) degrees: f32,
}

impl TransformStep {
    pub(super) fn apply(self, source: &VectorScene, selected: &[usize]) -> Result<VectorScene> {
        use omuse::vector_scene_ops as ops;
        ensure!(
            [self.dx, self.dy, self.scale, self.degrees]
                .iter()
                .all(|v| v.is_finite()),
            "Transform values must be finite"
        );
        ensure!(
            (0.0001..=100.).contains(&self.scale),
            "Scale must be between 0.01 and 10,000%"
        );
        let center = ops::bounds(source, selected)?
            .context("Selection has no geometry")?
            .center();
        let result = ops::scale(source, selected, self.scale, self.scale, center)?;
        let result = ops::rotate(&result, selected, self.degrees, center)?;
        ops::translate(&result, selected, self.dx, self.dy)
    }
    pub(super) fn is_identity(self) -> bool {
        self.dx == 0. && self.dy == 0. && self.scale == 1. && self.degrees.rem_euclid(360.) == 0.
    }
}

fn transformed_copy(
    source: &VectorScene,
    selected: &BTreeSet<usize>,
    step: TransformStep,
    scope_depth: usize,
) -> Result<(VectorScene, BTreeSet<usize>)> {
    source.validate()?;
    ensure!(!selected.is_empty(), "Select an object first");
    ensure!(
        source
            .objects
            .len()
            .checked_add(selected.len())
            .is_some_and(|count| count <= omuse::vector_scene::MAX_SCENE_OBJECTS),
        "Artwork has reached its object limit"
    );
    let mut result = step.apply(source, &selected.iter().copied().collect::<Vec<_>>())?;
    let mut groups = HashMap::new();
    let mut copies = selected
        .iter()
        .map(|index| {
            let mut object = result.objects[*index].clone();
            object.id = uuid::Uuid::new_v4().to_string();
            object.name = format!("{} copy", object.name);
            for group in object.groups.iter_mut().skip(scope_depth) {
                group.id = groups
                    .entry(group.id.clone())
                    .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone();
            }
            object
        })
        .collect::<Vec<_>>();
    omuse::vector_clip::repair_clones(
        source,
        &selected.iter().copied().collect::<Vec<_>>(),
        &mut copies,
    )?;
    // Keep originals byte-for-byte; the transform above is used only to construct copies.
    result = source.clone();
    let prefix = &source.objects[*selected.first().unwrap()].groups;
    ensure!(
        scope_depth <= prefix.len(),
        "Isolation group changed; select the artwork again"
    );
    let start = if scope_depth == 0 {
        result.objects.len()
    } else {
        result
            .objects
            .iter()
            .rposition(|object| {
                object.groups.len() >= scope_depth
                    && object.groups[..scope_depth] == prefix[..scope_depth]
            })
            .context("Isolation group changed")?
            + 1
    };
    let count = copies.len();
    result.objects.splice(start..start, copies);
    result.validate()?;
    let selection = (start..start + count).collect();
    Ok((result, selection))
}

fn swap_paints(source: &VectorScene, selected: &BTreeSet<usize>) -> Result<VectorScene> {
    ensure!(!selected.is_empty(), "Select an object first");
    let mut result = source.clone();
    for index in selected {
        let object = result
            .objects
            .get_mut(*index)
            .context("Object selection changed")?;
        ensure!(
            object.fill_gradient.is_none(),
            "Gradient fills cannot become strokes; choose a solid fill before swapping"
        );
        let old_fill = object.fill;
        let old_stroke = object.stroke;
        object.fill = old_stroke.map(|stroke| stroke.color);
        object.stroke = old_fill.map(|color| StrokeStyle {
            color,
            width: old_stroke.map_or(1., |stroke| stroke.width),
        });
        if object.stroke.is_none() {
            object.stroke_options = None;
        }
    }
    result.validate()?;
    Ok(result)
}

fn regular_shape(
    width: u32,
    height: u32,
    sides: u32,
    inner_ratio: Option<f32>,
) -> Result<VectorPath> {
    ensure!(
        (3..=64).contains(&sides),
        "Choose 3–64 sides or star points"
    );
    ensure!(
        inner_ratio.is_none_or(|ratio| ratio.is_finite() && (0.01..=0.99).contains(&ratio)),
        "Star inner radius must be 1–99%"
    );
    let center = VectorPoint {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    let radius = width.min(height) as f32 * 0.3;
    let count = if inner_ratio.is_some() {
        sides * 2
    } else {
        sides
    };
    let anchors = (0..count)
        .map(|i| {
            let angle =
                -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / count as f32;
            let radius = radius
                * if i % 2 == 1 {
                    inner_ratio.unwrap_or(1.)
                } else {
                    1.
                };
            Anchor {
                position: VectorPoint {
                    x: center.x + radius * angle.cos(),
                    y: center.y + radius * angle.sin(),
                },
                incoming: None,
                outgoing: None,
            }
        })
        .collect();
    Ok(VectorPath {
        subpaths: vec![Subpath {
            anchors,
            closed: true,
        }],
        fill_rule: Default::default(),
    })
}

impl EditorView {
    pub(super) fn scene_transform_step(&self, cx: &Context<Self>) -> Result<TransformStep> {
        let value = |i: usize| {
            self.detail_inputs[i]
                .read(cx)
                .value()
                .parse::<f32>()
                .context("Enter numeric transform values")
        };
        Ok(TransformStep {
            dx: value(4)?,
            dy: value(5)?,
            scale: value(6)? / 100.,
            degrees: value(7)?,
        })
    }

    pub(super) fn scene_quick_action(
        &mut self,
        action: &str,
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
            let selected = scene.selected_objects.clone();
            let source = draft.scene_snapshot()?;
            if action == "swap" {
                let artwork = swap_paints(&source, &selected)?;
                self.replace_scene_artwork(artwork, selected, window, cx)?;
                self.status = "Fill and stroke swapped · Ctrl+Z restores".into();
                return Ok(());
            }
            let step = if action == "transform-copy" {
                self.scene_transform_step(cx)?
            } else {
                scene
                    .last_transform
                    .context("Transform or move artwork first, then repeat")?
            };
            ensure!(
                !step.is_identity(),
                "Set a move, rotation or scale before repeating"
            );
            let (artwork, chosen) = if action == "again" {
                (
                    step.apply(&source, &selected.iter().copied().collect::<Vec<_>>())?,
                    selected,
                )
            } else {
                transformed_copy(&source, &selected, step, scene.isolation.len())?
            };
            self.replace_scene_artwork(artwork, chosen, window, cx)?;
            self.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .last_transform = Some(step);
            self.reset_scene_transform_inputs(window, cx);
            self.status = if action == "again" {
                "Transform repeated · Ctrl+Z restores"
            } else {
                "Transformed copy added · Ctrl+Shift+D repeats · Ctrl+Z restores"
            }
            .into();
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Vector: {error:#}");
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn scene_add_regular_shape(
        &mut self,
        star: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<()> {
            self.update_vector_style(cx)?;
            let sides = self.detail_inputs[29]
                .read(cx)
                .value()
                .parse::<u32>()
                .context("Enter a whole number of sides or star points")?;
            let inner = if star {
                Some(
                    self.detail_inputs[30]
                        .read(cx)
                        .value()
                        .parse::<f32>()
                        .context("Enter the inner radius percentage")?
                        / 100.,
                )
            } else {
                None
            };
            let draft = self
                .vector_draft
                .as_ref()
                .context("Open vector artwork first")?;
            let mut artwork = draft.scene_snapshot()?;
            ensure!(
                artwork.objects.len() < omuse::vector_scene::MAX_SCENE_OBJECTS,
                "Artwork has reached its object limit"
            );
            let path = regular_shape(artwork.width, artwork.height, sides, inner)?;
            let mut object =
                super::scene::new_object(if star { "Star" } else { "Polygon" }, path, draft.fill);
            object.stroke = draft.stroke;
            if let Some(scene) = draft.scene.as_ref() {
                object.stroke_options = scene.artwork.objects[scene.active].stroke_options.clone();
                object.groups = scene.artwork.objects[scene.active]
                    .groups
                    .iter()
                    .take(scene.isolation.len())
                    .cloned()
                    .collect();
            }
            let reuse = artwork.objects.len() == 1
                && artwork.objects[0]
                    .path
                    .subpaths
                    .iter()
                    .all(|subpath| subpath.anchors.is_empty());
            let at = if reuse {
                artwork.objects[0] = object;
                0
            } else {
                let at = if let Some(scene) = draft
                    .scene
                    .as_ref()
                    .filter(|scene| !scene.isolation.is_empty())
                {
                    artwork
                        .objects
                        .iter()
                        .rposition(|object| {
                            super::vector_isolation_ui::in_scope(&scene.isolation, object)
                        })
                        .map_or(artwork.objects.len(), |index| index + 1)
                } else {
                    artwork.objects.len()
                };
                artwork.objects.insert(at, object);
                at
            };
            self.replace_scene_artwork(artwork, [at].into_iter().collect(), window, cx)?;
            self.set_scene_mode(super::scene::SceneMode::Select, window, cx);
            self.status = format!(
                "{} added · Move to position · Nodes to edit · Ctrl+Z restores",
                if star { "Star" } else { "Polygon" }
            );
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Add shape: {error:#}");
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn vector_quick_shape_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut fields = div().flex().flex_wrap().gap_2();
        for (index, id, label) in [
            (29, "vector-shape-points", "Sides / star points · 3–64"),
            (30, "vector-star-inner", "Star inner radius · %"),
        ] {
            fields = fields.child(
                div()
                    .flex_1()
                    .min_w(px(100.))
                    .child(inspector_ui::panel_note(label, cx))
                    .child(
                        input(id, &self.detail_inputs[index], window, cx)
                            .debug_selector(move || id.into()),
                    ),
            );
        }
        let mut actions = div().flex().gap_1();
        for (star, id, label) in [
            (false, "vector-add-polygon", "Polygon"),
            (true, "vector-add-star", "Star"),
        ] {
            actions =
                actions.child(
                    button(id, label, ButtonVariant::Outline, cx)
                        .disabled(self.busy)
                        .flex_1()
                        .debug_selector(move || id.into())
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.scene_add_regular_shape(star, w, cx)
                        })),
                );
        }
        div().flex().flex_col().gap_2().child(fields).child(actions).child(inspector_ui::panel_note("Arrow keys nudge · Shift+Arrow moves 10 px · Nodes: Shift snaps direction · Alt breaks handle symmetry", cx)).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omuse::vector_scene::VectorObject;
    fn source() -> VectorScene {
        VectorScene {
            version: 1,
            width: 100,
            height: 100,
            objects: vec![
                VectorObject::rectangle("Square", 10., 10., 10., 10., Some([255, 0, 0, 255]), None)
                    .unwrap(),
            ],
        }
    }
    #[test]
    fn repeat_copy_chains_geometry_without_mutating_original() {
        let source = source();
        let step = TransformStep {
            dx: 15.,
            dy: 2.,
            scale: 1.,
            degrees: 0.,
        };
        let (first, selected) =
            transformed_copy(&source, &[0].into_iter().collect(), step, 0).unwrap();
        assert_eq!(first.objects[0], source.objects[0]);
        let (second, selected) = transformed_copy(&first, &selected, step, 0).unwrap();
        assert_eq!(selected, [2].into_iter().collect());
        assert_eq!(&second.objects[..2], &first.objects);
        let bounds = omuse::vector_scene_ops::bounds(&second, &[2])
            .unwrap()
            .unwrap();
        assert_eq!((bounds.left, bounds.top), (40., 14.));
        assert_ne!(second.objects[1].id, second.objects[2].id);
    }
    #[test]
    fn copy_inside_nested_isolation_keeps_parent_contiguous() {
        use omuse::vector_scene::VectorGroup;
        let mut source = source();
        let parent = VectorGroup {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Parent".into(),
        };
        let nested = VectorGroup {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Nested".into(),
        };
        source.version = 2;
        source.objects[0].groups = vec![parent.clone(), nested.clone()];
        let mut sibling = source.objects[0].clone();
        sibling.id = uuid::Uuid::new_v4().to_string();
        sibling.groups = vec![parent.clone()];
        source.objects.push(sibling.clone());
        let step = TransformStep {
            dx: 5.,
            dy: 0.,
            scale: 1.,
            degrees: 0.,
        };
        let (result, chosen) =
            transformed_copy(&source, &[0].into_iter().collect(), step, 2).unwrap();
        assert_eq!(chosen, [1].into_iter().collect());
        assert_eq!(result.objects[1].groups, vec![parent, nested]);
        assert_eq!(result.objects[2], sibling);
        result.validate().unwrap();
    }
    #[test]
    fn transformed_clip_copy_uses_its_own_mask() {
        let mut source = source();
        source.objects.push(
            VectorObject::rectangle("Mask", 10., 10., 5., 10., Some([255; 4]), None).unwrap(),
        );
        let (source, _) = omuse::vector_clip::make(&source, &[0, 1]).unwrap();
        let step = TransformStep {
            dx: 20.,
            dy: 0.,
            scale: 1.,
            degrees: 0.,
        };
        let (result, _) =
            transformed_copy(&source, &[0, 1].into_iter().collect(), step, 0).unwrap();
        assert_eq!(
            result.objects[2].clip_to.as_deref(),
            Some(result.objects[3].id.as_str())
        );
        assert_eq!(&result.objects[..2], &source.objects);
        result.validate().unwrap();
    }
    #[test]
    fn copy_rejects_stale_empty_and_non_finite_transforms() {
        let source = source();
        let mut step = TransformStep {
            dx: 15.,
            dy: 0.,
            scale: 1.,
            degrees: 0.,
        };
        assert!(transformed_copy(&source, &BTreeSet::new(), step, 0).is_err());
        assert!(transformed_copy(&source, &[1].into_iter().collect(), step, 0).is_err());
        step.dx = f32::NAN;
        assert!(transformed_copy(&source, &[0].into_iter().collect(), step, 0).is_err());
    }
    #[test]
    fn solid_fill_swap_is_reversible_and_preserves_geometry() {
        let source = source();
        let selected = [0].into_iter().collect();
        let swapped = swap_paints(&source, &selected).unwrap();
        assert_eq!(swapped.objects[0].path, source.objects[0].path);
        assert_eq!(swapped.objects[0].fill, None);
        assert_eq!(swapped.objects[0].stroke.unwrap().color, [255, 0, 0, 255]);
        assert_eq!(swap_paints(&swapped, &selected).unwrap(), source);
    }
    #[test]
    fn swapping_a_mixed_selection_never_discards_a_gradient() {
        use omuse::vector_scene::{GradientFill, GradientKind, GradientSpread, GradientStop};
        let mut source = source();
        let mut object = source.objects[0].clone();
        object.id = uuid::Uuid::new_v4().to_string();
        object.fill_gradient = Some(GradientFill {
            kind: GradientKind::Linear {
                start: VectorPoint { x: 10., y: 10. },
                end: VectorPoint { x: 20., y: 10. },
            },
            stops: vec![
                GradientStop {
                    offset: 0.,
                    color: [255, 0, 0, 255],
                },
                GradientStop {
                    offset: 1.,
                    color: [0, 0, 255, 255],
                },
            ],
            spread: GradientSpread::Pad,
            transform: [1., 0., 0., 1., 0., 0.],
            method: Default::default(),
            dither: false,
        });
        source.version = 3;
        source.objects.push(object);
        let original = source.clone();
        assert!(
            swap_paints(&source, &[0, 1].into_iter().collect())
                .unwrap_err()
                .to_string()
                .contains("Gradient fills")
        );
        assert_eq!(source, original);
    }
    #[test]
    fn regular_shapes_have_bounded_editable_closed_points() {
        for (sides, inner) in [(3, None), (64, None), (5, Some(0.5)), (64, Some(0.01))] {
            let path = regular_shape(200, 100, sides, inner).unwrap();
            assert_eq!(
                path.subpaths
                    .iter()
                    .map(|subpath| subpath.anchors.len())
                    .sum::<usize>(),
                sides as usize * if inner.is_some() { 2 } else { 1 }
            );
            assert!(path.subpaths[0].closed);
            path.validate().unwrap();
            assert!(
                path.subpaths[0]
                    .anchors
                    .iter()
                    .all(|a| a.incoming.is_none() && a.outgoing.is_none())
            );
        }
        assert!(regular_shape(100, 100, 2, None).is_err());
        assert!(regular_shape(100, 100, 65, None).is_err());
        assert!(regular_shape(100, 100, 5, Some(f32::NAN)).is_err());
    }
}
