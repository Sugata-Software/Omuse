//! One snap index per main-canvas node/handle gesture. Legacy dialogs bypass it.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::vector_snap::{
    AnchorId, AnchorTarget, MAX_SNAP_ANCHORS, MAX_SNAP_GUIDES, SnapGrid, SnapGuide, SnapIndex,
};

impl EditorView {
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
