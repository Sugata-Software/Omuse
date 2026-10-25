//! One snap index per main-canvas node/handle gesture. Legacy dialogs bypass it.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_snap::{
    AnchorId, AnchorTarget, MAX_SNAP_ANCHORS, MAX_SNAP_GUIDES, SnapGrid, SnapGuide, SnapIndex,
};

#[derive(Default)]
pub(super) struct ContactSnapUi {
    pub(super) options: omuse::vector_snap::ContactOptions,
    pub(super) indicator: Option<omuse::vector_snap::ContactResult>,
    cache: Option<ContactCache>,
}

struct ContactCache {
    requested: u64,
    active: usize,
    from: VectorPoint,
    isolation: Vec<String>,
    indices: std::result::Result<(SnapIndex, omuse::vector_snap::ContactIndex), String>,
}

impl EditorView {
    pub(super) fn clear_contact_snap_cache(&mut self) {
        if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
            scene.contacts.cache = None;
            scene.contacts.indicator = None;
        }
    }

    pub(super) fn preview_pen_contacts(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let before = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .and_then(|s| s.contacts.indicator);
        if let Some(point) = self.vector_canvas_point(event.position) {
            self.snap_pen_construction_point(point, event.modifiers.shift || event.modifiers.alt);
        } else if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
            scene.contacts.indicator = None;
        }
        let after = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .and_then(|s| s.contacts.indicator);
        if before != after {
            cx.notify();
        }
    }

    pub(super) fn snap_pen_construction_point(
        &mut self,
        point: VectorPoint,
        bypass: bool,
    ) -> VectorPoint {
        if let Some(scene) = self.vector_draft.as_mut().and_then(|d| d.scene.as_mut()) {
            scene.contacts.indicator = None;
        }
        if bypass || !self.preferences.snapping || !self.vector_scene_current() {
            return point;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return point;
        };
        let Some(scene) = draft.scene.as_ref().filter(|s| {
            s.mode == super::scene::SceneMode::Pen
                && (s.contacts.options.tangent || s.contacts.options.perpendicular)
        }) else {
            return point;
        };
        let Some(subpath) = draft.path.subpaths.last().filter(|s| !s.closed) else {
            return point;
        };
        let Some(previous) = subpath
            .anchors
            .last()
            .filter(|a| a.outgoing.is_none_or(|p| p == a.position))
        else {
            return point;
        };
        let from = previous.position;
        let (requested, active, options, is_new) = (
            scene.requested,
            scene.active,
            scene.contacts.options,
            scene.is_new,
        );
        let transform = (|| -> Result<[f32; 6]> {
            if is_new {
                return Ok([1., 0., 0., 1., 0., 0.]);
            }
            let p = self
                .editor
                .layer_to_canvas(&draft.layer, 0., 0.)
                .context("Layer placement unavailable")?;
            let x = self
                .editor
                .layer_to_canvas(&draft.layer, 1., 0.)
                .context("Layer placement unavailable")?;
            let y = self
                .editor
                .layer_to_canvas(&draft.layer, 0., 1.)
                .context("Layer placement unavailable")?;
            Ok([x.0 - p.0, x.1 - p.1, y.0 - p.0, y.1 - p.1, p.0, p.1])
        })();
        let transform = match transform {
            Ok(t) => t,
            Err(_) => return point,
        };
        let world = |p: VectorPoint| VectorPoint {
            x: transform[0] * p.x + transform[2] * p.y + transform[4],
            y: transform[1] * p.x + transform[3] * p.y + transform[5],
        };
        let pointer = world(point);
        let world_from = world(from);
        let cached = scene.contacts.cache.as_ref().is_some_and(|c| {
            c.requested == requested
                && c.active == active
                && c.from == from
                && c.isolation == scene.isolation
        });
        if !cached {
            let prepared = (|| -> Result<_> {
                let mut source = draft.scene_snapshot()?;
                for (index, object) in source.objects.iter_mut().enumerate() {
                    if index == active
                        || object.clip_to.is_some()
                        || !super::vector_isolation_ui::in_scope(&scene.isolation, object)
                    {
                        object.visible = false;
                    }
                }
                ensure!(
                    self.editor
                        .document
                        .metadata
                        .get("guides")
                        .and_then(serde_json::Value::as_array)
                        .is_none_or(|guides| guides.len() <= MAX_SNAP_GUIDES),
                    "Contact scene exceeds4096 guides"
                );
                let guides = if self.show_guides {
                    self.editor
                        .guides()
                        .into_iter()
                        .enumerate()
                        .map(|(id, g)| SnapGuide {
                            id,
                            axis: g.axis,
                            position: g.position,
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                let grid = self.show_grid.then_some(SnapGrid {
                    settings: omuse::canvas_grid::GridSettings {
                        spacing: self.preferences.grid_spacing,
                        subdivisions: self.preferences.grid_subdivisions,
                    },
                    origin: VectorPoint { x: 0., y: 0. },
                });
                Ok((
                    SnapIndex::from_scene(&source, transform, &guides, grid, &[], &draft.cancel)?,
                    omuse::vector_snap::ContactIndex::from_scene(
                        &source,
                        transform,
                        &draft.cancel,
                    )?,
                ))
            })()
            .map_err(|error| format!("{error:#}"));
            if let Err(error) = &prepared {
                self.status = format!("Pen contacts unavailable: {error}");
            }
            let scene = self.vector_draft.as_mut().unwrap().scene.as_mut().unwrap();
            scene.contacts.cache = Some(ContactCache {
                requested,
                active,
                from,
                isolation: scene.isolation.clone(),
                indices: prepared,
            });
        }
        let draft = self.vector_draft.as_ref().unwrap();
        let scene = draft.scene.as_ref().unwrap();
        let Some(Ok((ordinary, contact))) = scene.contacts.cache.as_ref().map(|c| &c.indices)
        else {
            return point;
        };
        let ordinary = ordinary.resolve(pointer, self.zoom, 6.).ok();
        let contact = contact.resolve(world_from, pointer, self.zoom, 6., options, &draft.cancel);
        let ordinary_distance = ordinary
            .as_ref()
            .filter(|r| r.anchor.is_some() || r.vertical.is_some() || r.horizontal.is_some())
            .map(|r| (r.point.x - pointer.x).hypot(r.point.y - pointer.y));
        let (result, indicator) = match contact {
            Ok(Some(result))
                if ordinary.as_ref().is_none_or(|r| r.anchor.is_none())
                    && ordinary_distance.is_none_or(|d| {
                        (result.point.x - pointer.x).hypot(result.point.y - pointer.y) < d
                    }) =>
            {
                (result.point, Some(result))
            }
            Ok(_) => (
                ordinary
                    .filter(|r| {
                        r.anchor.is_some() || r.vertical.is_some() || r.horizontal.is_some()
                    })
                    .map_or(pointer, |r| r.point),
                None,
            ),
            Err(error) => {
                self.status = format!("Pen contact query: {error:#}");
                (ordinary.map_or(pointer, |r| r.point), None)
            }
        };
        let local = if is_new {
            Some(result)
        } else {
            self.editor
                .canvas_to_layer(&draft.layer, result.x, result.y)
                .map(|(x, y)| VectorPoint { x, y })
        };
        if let Some(local) = local.filter(|p| vector_point_in_bounds(*p)) {
            self.vector_draft
                .as_mut()
                .unwrap()
                .scene
                .as_mut()
                .unwrap()
                .contacts
                .indicator = indicator;
            local
        } else {
            point
        }
    }

    pub(super) fn vector_contact_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(scene) = self
            .vector_draft
            .as_ref()
            .and_then(|d| d.scene.as_ref())
            .filter(|s| s.mode == super::scene::SceneMode::Pen)
        else {
            return div().into_any_element();
        };
        let mut controls = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(inspector_ui::panel_note(
                "Pen contacts · straight segments · Shift/Alt bypass",
                cx,
            ));
        let mut buttons = div().flex().flex_wrap().gap_1();
        for (id, label, tangent, selected) in [
            (
                "vector-contact-tangent",
                "Tangent",
                true,
                scene.contacts.options.tangent,
            ),
            (
                "vector-contact-perpendicular",
                "Perpendicular",
                false,
                scene.contacts.options.perpendicular,
            ),
        ] {
            buttons = buttons.child(
                button(id, label, ButtonVariant::Outline, cx)
                    .selected(selected)
                    .disabled(self.busy)
                    .debug_selector(move || id.into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(scene) =
                            this.vector_draft.as_mut().and_then(|d| d.scene.as_mut())
                        {
                            if tangent {
                                scene.contacts.options.tangent = !scene.contacts.options.tangent;
                            } else {
                                scene.contacts.options.perpendicular =
                                    !scene.contacts.options.perpendicular;
                            }
                        }
                        this.clear_contact_snap_cache();
                        cx.notify();
                    })),
            );
        }
        controls = controls.child(buttons);
        if !self.preferences.snapping {
            controls = controls.child(inspector_ui::panel_note(
                "Enable Snapping to use Pen contacts",
                cx,
            ));
        }
        controls.into_any_element()
    }

    /// Parent resets both draft snap fields in begin_drag and on pointer-up.
    /// Pass Shift as bypass before applying the existing 45-degree constraint.
    pub(super) fn snap_vector_drag_point(
        &mut self,
        point: VectorPoint,
        bypass: bool,
    ) -> VectorPoint {
        if let Some(draft) = self.vector_draft.as_mut() {
            draft.snap_indicator = None;
        }
        if bypass || !self.preferences.snapping || !self.vector_scene_current() {
            return point;
        }
        let Some(draft) = self.vector_draft.as_ref() else {
            return point;
        };
        if draft.scene.is_none() || draft.drag.is_none() {
            return point;
        }
        if draft.snap_cache.is_none() {
            let prepared = self
                .prepare_vector_snap_cache()
                .map_err(|error| format!("{error:#}"));
            if let Err(error) = &prepared {
                self.status = format!("Snapping unavailable for this drag: {error}");
            }
            self.vector_draft.as_mut().unwrap().snap_cache = Some(prepared);
        }
        let draft = self.vector_draft.as_ref().unwrap();
        let Some(Ok(index)) = &draft.snap_cache else {
            return point;
        };
        let is_new = draft.scene.as_ref().unwrap().is_new;
        let canvas = if is_new {
            Some(point)
        } else {
            self.editor
                .layer_to_canvas(&draft.layer, point.x, point.y)
                .map(|(x, y)| VectorPoint { x, y })
        };
        let resolved = canvas
            .context("Layer placement is unavailable")
            .and_then(|canvas| index.resolve(canvas, self.zoom, 6.));
        match resolved {
            Ok(result) => {
                if result.anchor.is_none()
                    && result.vertical.is_none()
                    && result.horizontal.is_none()
                {
                    return point;
                }
                let local = if is_new {
                    Some(result.point)
                } else {
                    self.editor
                        .canvas_to_layer(&draft.layer, result.point.x, result.point.y)
                        .map(|(x, y)| VectorPoint { x, y })
                };
                if let Some(local) = local.filter(|p| vector_point_in_bounds(*p)) {
                    self.vector_draft.as_mut().unwrap().snap_indicator = Some(result.point);
                    local
                } else {
                    self.vector_snap_failed(
                        "Snapped point is outside the layer's editable coordinates".into(),
                    );
                    point
                }
            }
            Err(error) => {
                self.vector_snap_failed(format!("{error:#}"));
                point
            }
        }
    }

    fn vector_snap_failed(&mut self, error: String) {
        if let Some(draft) = self.vector_draft.as_mut() {
            draft.snap_cache = Some(Err(error.clone()));
            draft.snap_indicator = None;
        }
        self.status = format!("Snapping unavailable for this drag: {error}");
    }

    fn prepare_vector_snap_cache(&self) -> Result<SnapIndex> {
        let draft = self.vector_draft.as_ref().context("Vector editor closed")?;
        let scene = draft
            .scene
            .as_ref()
            .context("Vector snapping requires the main canvas")?;
        let drag = draft.drag.as_ref().context("Start a node or handle drag")?;
        let snapshot = draft.scene_snapshot()?;
        let excluded = AnchorId {
            object: scene.active,
            subpath: drag.subpath,
            anchor: drag.anchor,
        };
        ensure!(
            snapshot
                .objects
                .get(excluded.object)
                .and_then(|o| o.path.subpaths.get(excluded.subpath))
                .and_then(|s| s.anchors.get(excluded.anchor))
                .is_some(),
            "Dragged anchor no longer exists"
        );
        let mut targets = Vec::new();
        let mut count = 0usize;
        for (object_index, object) in snapshot.objects.iter().enumerate() {
            ensure!(
                !draft.cancel.load(Ordering::Relaxed),
                "Vector gesture cancelled"
            );
            for (subpath_index, subpath) in object.path.subpaths.iter().enumerate() {
                count = count.saturating_add(subpath.anchors.len());
                ensure!(
                    count <= MAX_SNAP_ANCHORS,
                    "Snap scene exceeds 100000 anchors"
                );
                for (anchor_index, anchor) in subpath.anchors.iter().enumerate() {
                    if anchor_index % 256 == 0 {
                        ensure!(
                            !draft.cancel.load(Ordering::Relaxed),
                            "Vector gesture cancelled"
                        );
                    }
                    if !object.visible || object.opacity == 0. {
                        continue;
                    }
                    // Canvas drafts normally bake these transforms. Applying
                    // them explicitly also covers a valid unbaked snapshot.
                    let [a, b, c, d, e, f] = object.transform.map(f64::from);
                    let (x, y) = (f64::from(anchor.position.x), f64::from(anchor.position.y));
                    let local = VectorPoint {
                        x: (a * x + c * y + e) as f32,
                        y: (b * x + d * y + f) as f32,
                    };
                    let point = if scene.is_new {
                        local
                    } else {
                        let (x, y) = self
                            .editor
                            .layer_to_canvas(&draft.layer, local.x, local.y)
                            .context("Layer placement is unavailable")?;
                        VectorPoint { x, y }
                    };
                    targets.push(AnchorTarget {
                        id: AnchorId {
                            object: object_index,
                            subpath: subpath_index,
                            anchor: anchor_index,
                        },
                        point,
                    });
                }
            }
        }
        let guides = if self.show_guides {
            ensure!(
                self.editor
                    .document
                    .metadata
                    .get("guides")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(|guides| guides.len() <= MAX_SNAP_GUIDES),
                "Snap scene exceeds 4096 guides"
            );
            self.editor
                .guides()
                .into_iter()
                .enumerate()
                .map(|(id, guide)| SnapGuide {
                    id,
                    axis: guide.axis,
                    position: guide.position,
                })
                .collect()
        } else {
            Vec::new()
        };
        let grid = self.show_grid.then_some(SnapGrid {
            settings: omuse::canvas_grid::GridSettings {
                spacing: self.preferences.grid_spacing,
                subdivisions: self.preferences.grid_subdivisions,
            },
            origin: VectorPoint { x: 0., y: 0. },
        });
        SnapIndex::build(&targets, &guides, grid, &[excluded], &draft.cancel)
    }
}
