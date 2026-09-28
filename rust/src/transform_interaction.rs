//! Pure canvas-space transform overlay geometry and pointer-drag state.
use omuse::editor::{Editor, LayerPlacement};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CanvasPoint {
    pub x: f32,
    pub y: f32,
}
impl CanvasPoint {
    fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

pub const HANDLE_UNITS: [CanvasPoint; 8] = [
    CanvasPoint { x: 0., y: 0. },
    CanvasPoint { x: 0.5, y: 0. },
    CanvasPoint { x: 1., y: 0. },
    CanvasPoint { x: 1., y: 0.5 },
    CanvasPoint { x: 1., y: 1. },
    CanvasPoint { x: 0.5, y: 1. },
    CanvasPoint { x: 0., y: 1. },
    CanvasPoint { x: 0., y: 0.5 },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitTarget {
    Resize(usize),
    Rotate,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformGeometry {
    pub handles: [CanvasPoint; 8],
    pub rotation_handle: CanvasPoint,
    pub placement: LayerPlacement,
}
impl TransformGeometry {
    /// `rotation_handle_distance` is in canvas units; callers divide their
    /// desired screen-pixel distance by canvas zoom.
    pub fn new(placement: LayerPlacement, rotation_handle_distance: f32) -> Option<Self> {
        if !placement.is_valid()
            || !rotation_handle_distance.is_finite()
            || rotation_handle_distance < 0.
        {
            return None;
        }
        let handles = HANDLE_UNITS.map(|unit| placement_point(placement, unit));
        let radians = placement.rotation.to_radians();
        let top = handles[1];
        let rotation_handle = CanvasPoint {
            x: top.x + radians.sin() * rotation_handle_distance,
            y: top.y - radians.cos() * rotation_handle_distance,
        };
        Some(Self {
            handles,
            rotation_handle,
            placement,
        })
    }
    /// Geometric TL/TR/BR/BL order. Distortion applies source reflections in
    /// the editor, so these corners must not be reordered by the flip flags.
    pub fn corners(&self) -> [CanvasPoint; 4] {
        [
            self.handles[0],
            self.handles[2],
            self.handles[4],
            self.handles[6],
        ]
    }
    pub fn hit_test(&self, point: CanvasPoint, tolerance: f32) -> Option<HitTarget> {
        if !tolerance.is_finite() || tolerance < 0. {
            return None;
        }
        if point.distance(self.rotation_handle) <= tolerance {
            return Some(HitTarget::Rotate);
        }
        if let Some(index) = self
            .handles
            .iter()
            .position(|handle| point.distance(*handle) <= tolerance)
        {
            return Some(HitTarget::Resize(index));
        }
        self.placement
            .contains(point.x, point.y)
            .then_some(HitTarget::Move)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Move,
    Resize(usize),
    Rotate,
    Distort(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragModifiers {
    /// Constrains moves to one axis and rotations to 15-degree increments.
    pub shift: bool,
    /// Resize around the center rather than the opposite handle.
    pub from_center: bool,
    /// The default proportional-resize preference. Shift temporarily inverts it.
    pub lock_ratio: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformDrag {
    pub original: LayerPlacement,
    pub start: CanvasPoint,
    pub mode: DragMode,
    pub original_corners: Option<[CanvasPoint; 4]>,
}
impl TransformDrag {
    pub fn new(original: LayerPlacement, start: CanvasPoint, mode: DragMode) -> Option<Self> {
        if !original.is_valid() || !start.x.is_finite() || !start.y.is_finite() {
            return None;
        }
        Some(Self {
            original,
            start,
            mode,
            original_corners: None,
        })
    }
    pub fn with_corners(mut self, corners: [CanvasPoint; 4]) -> Option<Self> {
        if corners.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            self.original_corners = Some(corners);
            Some(self)
        } else {
            None
        }
    }
    pub fn updated(&self, point: CanvasPoint, modifiers: DragModifiers) -> LayerPlacement {
        if !point.x.is_finite() || !point.y.is_finite() {
            return self.original;
        }
        let mut result = self.original;
        match self.mode {
            DragMode::Distort(_) => return result,
            DragMode::Move => {
                let (mut dx, mut dy) = (point.x - self.start.x, point.y - self.start.y);
                if modifiers.shift {
                    if dx.abs() >= dy.abs() {
                        dy = 0.
                    } else {
                        dx = 0.
                    }
                }
                result.x += dx;
                result.y += dy;
            }
            DragMode::Rotate => {
                let center = result.center();
                let delta = (point.y - center.1).atan2(point.x - center.0)
                    - (self.start.y - center.1).atan2(self.start.x - center.0);
                result.rotation += delta.to_degrees();
                if modifiers.shift {
                    result.rotation = (result.rotation / 15.).round() * 15.;
                }
            }
            DragMode::Resize(index) => {
                if index >= HANDLE_UNITS.len() {
                    return self.original;
                }
                let handle = HANDLE_UNITS[index];
                let anchor_unit = if modifiers.from_center {
                    CanvasPoint { x: 0.5, y: 0.5 }
                } else {
                    CanvasPoint {
                        x: 1. - handle.x,
                        y: 1. - handle.y,
                    }
                };
                let anchor = placement_point(self.original, anchor_unit);
                let initial = placement_point(self.original, handle);
                let dx = initial.x + point.x - self.start.x - anchor.x;
                let dy = initial.y + point.y - self.start.y - anchor.y;
                let (sin, cos) = self.original.rotation.to_radians().sin_cos();
                let span = if modifiers.from_center { 2. } else { 1. };
                let local_x = (dx * cos + dy * sin) * span;
                let local_y = (-dx * sin + dy * cos) * span;
                let sx = handle.x * 2. - 1.;
                let sy = handle.y * 2. - 1.;
                let raw_width = if sx == 0. {
                    self.original.width
                } else {
                    local_x * sx
                };
                let raw_height = if sy == 0. {
                    self.original.height
                } else {
                    local_y * sy
                };
                let mirrored_x = raw_width < 0.;
                let mirrored_y = raw_height < 0.;
                let mut width = raw_width.abs().max(1.);
                let mut height = raw_height.abs().max(1.);
                if modifiers.lock_ratio != modifiers.shift {
                    let factor = if sx == 0. {
                        height / self.original.height
                    } else if sy == 0. {
                        width / self.original.width
                    } else {
                        // Project onto the aspect-ratio ray in the pointer's
                        // quadrant. Orientation is carried by the flip flags;
                        // signed spans here would collapse a reflected drag.
                        (raw_width.abs() * self.original.width
                            + raw_height.abs() * self.original.height)
                            / (self.original.width * self.original.width
                                + self.original.height * self.original.height)
                    }
                    .max(1. / self.original.width.min(self.original.height));
                    width = self.original.width * factor;
                    height = self.original.height * factor;
                }
                result.width = width;
                result.height = height;
                if mirrored_x {
                    result.flip_x = !result.flip_x;
                }
                if mirrored_y {
                    result.flip_y = !result.flip_y;
                }
                let offset_x = (0.5 - anchor_unit.x) * width * if mirrored_x { -1. } else { 1. };
                let offset_y = (0.5 - anchor_unit.y) * height * if mirrored_y { -1. } else { 1. };
                let center = CanvasPoint {
                    x: anchor.x + offset_x * cos - offset_y * sin,
                    y: anchor.y + offset_x * sin + offset_y * cos,
                };
                result.x = center.x - width / 2.;
                result.y = center.y - height / 2.;
            }
        }
        if result.is_valid() {
            result
        } else {
            self.original
        }
    }
    pub fn distorted_corners(
        &self,
        point: CanvasPoint,
        constrain_axis: bool,
    ) -> Option<[CanvasPoint; 4]> {
        let mut corners = self.original_corners?;
        let (mut dx, mut dy) = (point.x - self.start.x, point.y - self.start.y);
        if constrain_axis {
            if dx.abs() >= dy.abs() {
                dy = 0.
            } else {
                dx = 0.
            }
        }
        let moved: Vec<usize> = match self.mode {
            DragMode::Distort(index) if index < 8 && index % 2 == 0 => vec![index / 2],
            DragMode::Distort(1) => vec![0, 1],
            DragMode::Distort(3) => vec![1, 2],
            DragMode::Distort(5) => vec![2, 3],
            DragMode::Distort(7) => vec![3, 0],
            DragMode::Move => vec![0, 1, 2, 3],
            _ => return None,
        };
        for index in moved {
            corners[index].x += dx;
            corners[index].y += dy;
        }
        Some(corners)
    }
}

pub fn selection_bounds(editor: &Editor, ids: &[String]) -> Option<LayerPlacement> {
    if let [id] = ids {
        return editor.layer_placement(id);
    }
    let mut points = Vec::new();
    for id in ids {
        let placement = editor.layer_placement(id)?;
        points.extend(
            [
                CanvasPoint { x: 0., y: 0. },
                CanvasPoint { x: 1., y: 0. },
                CanvasPoint { x: 1., y: 1. },
                CanvasPoint { x: 0., y: 1. },
            ]
            .map(|unit| placement_point(placement, unit)),
        );
    }
    let min_x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let max_y = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let result = LayerPlacement {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(1.),
        height: (max_y - min_y).max(1.),
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    result.is_valid().then_some(result)
}

pub fn commit_drag(
    editor: &mut Editor,
    ids: &[String],
    original_box: LayerPlacement,
    draft: LayerPlacement,
) -> bool {
    let contains_group = ids.iter().any(|id| {
        editor
            .document
            .find_layer(id)
            .is_some_and(|layer| layer.is_group())
    });
    if contains_group
        && draft.width == original_box.width
        && draft.height == original_box.height
        && draft.rotation == original_box.rotation
        && draft.flip_x == original_box.flip_x
        && draft.flip_y == original_box.flip_y
    {
        return editor.move_layers(ids, draft.x - original_box.x, draft.y - original_box.y);
    }
    if ids.len() == 1 {
        editor.set_layer_placement(&ids[0], draft)
    } else {
        editor.transform_layers(ids, original_box, draft)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionAction {
    Replace,
    Add,
    Toggle,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayerSelection {
    pub ids: Vec<String>,
    pub primary: Option<String>,
}
impl LayerSelection {
    pub fn click(&mut self, id: String, action: SelectionAction) {
        match action {
            SelectionAction::Replace => self.ids = vec![id.clone()],
            SelectionAction::Add => {
                if !self.ids.contains(&id) {
                    self.ids.push(id.clone());
                }
            }
            SelectionAction::Toggle => {
                if let Some(index) = self.ids.iter().position(|value| value == &id) {
                    self.ids.remove(index);
                } else {
                    self.ids.push(id.clone());
                }
            }
        }
        self.primary = if self.ids.contains(&id) {
            Some(id)
        } else {
            self.ids.last().cloned()
        };
    }
    pub fn clear(&mut self) {
        self.ids.clear();
        self.primary = None;
    }
}

fn placement_point(value: LayerPlacement, unit: CanvasPoint) -> CanvasPoint {
    let x = (unit.x - 0.5) * value.width;
    let y = (unit.y - 0.5) * value.height;
    let (sin, cos) = value.rotation.to_radians().sin_cos();
    let center = value.center();
    CanvasPoint {
        x: center.0 + x * cos - y * sin,
        y: center.1 + x * sin + y * cos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omuse::model::Document;
    fn placement() -> LayerPlacement {
        LayerPlacement {
            x: 10.,
            y: 20.,
            width: 8.,
            height: 4.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        }
    }
    #[test]
    fn rotated_handles_and_hit_targets_are_canvas_correct() {
        let mut p = placement();
        p.rotation = 90.;
        let geometry = TransformGeometry::new(p, 10.).unwrap();
        assert!((geometry.handles[0].x - 16.).abs() < 0.001);
        assert!((geometry.handles[0].y - 18.).abs() < 0.001);
        assert_eq!(
            geometry.hit_test(geometry.handles[4], 1.),
            Some(HitTarget::Resize(4))
        );
        assert_eq!(
            geometry.hit_test(geometry.rotation_handle, 1.),
            Some(HitTarget::Rotate)
        );
    }
    #[test]
    fn move_rotate_and_resize_match_preserved_modifier_rules() {
        let p = placement();
        let moved = TransformDrag::new(p, CanvasPoint { x: 0., y: 0. }, DragMode::Move)
            .unwrap()
            .updated(
                CanvasPoint { x: 6., y: 2. },
                DragModifiers {
                    shift: true,
                    ..Default::default()
                },
            );
        assert_eq!((moved.x, moved.y), (16., 20.));
        let rotated = TransformDrag::new(p, CanvasPoint { x: 14., y: 16. }, DragMode::Rotate)
            .unwrap()
            .updated(
                CanvasPoint { x: 18., y: 22. },
                DragModifiers {
                    shift: true,
                    ..Default::default()
                },
            );
        assert_eq!(rotated.rotation, 90.);
        let resized = TransformDrag::new(p, CanvasPoint { x: 18., y: 24. }, DragMode::Resize(4))
            .unwrap()
            .updated(CanvasPoint { x: 22., y: 26. }, DragModifiers::default());
        assert_eq!((resized.width, resized.height), (12., 6.));
    }
    #[test]
    fn moving_one_rotated_flipped_layer_preserves_its_transform_and_undo() {
        let mut editor = Editor::new(Document::new(80, 60));
        let id = editor.active_layer.clone();
        let original = LayerPlacement {
            rotation: 37.,
            flip_x: true,
            flip_y: true,
            ..placement()
        };
        assert!(editor.set_layer_placement(&id, original));
        let original = editor.layer_placement(&id).unwrap();
        let bounds = selection_bounds(&editor, std::slice::from_ref(&id)).unwrap();
        assert_eq!(bounds, original, "a single layer needs its oriented bounds");
        let start = CanvasPoint { x: 14., y: 22. };
        let draft = TransformDrag::new(bounds, start, DragMode::Move)
            .unwrap()
            .updated(CanvasPoint { x: 21., y: 19. }, DragModifiers::default());
        let depth = editor.undo_depth();
        assert!(commit_drag(
            &mut editor,
            std::slice::from_ref(&id),
            bounds,
            draft
        ));
        let moved = editor.layer_placement(&id).unwrap();
        assert_eq!(
            moved,
            LayerPlacement {
                x: original.x + 7.,
                y: original.y - 3.,
                ..original
            }
        );
        assert_eq!(editor.undo_depth(), depth + 1);
        assert!(editor.undo());
        assert_eq!(editor.layer_placement(&id).unwrap(), original);
        assert!(editor.redo());
        assert_eq!(editor.layer_placement(&id).unwrap(), moved);
    }
    #[test]
    fn proportional_corner_drag_can_cross_its_anchor_without_collapsing() {
        for rotation in [0., 37., 90.] {
            for from_center in [false, true] {
                for (mirror_x, mirror_y) in [(true, false), (false, true), (true, true)] {
                    let original = LayerPlacement {
                        rotation,
                        flip_x: true,
                        ..placement()
                    };
                    let start = placement_point(original, HANDLE_UNITS[4]);
                    let anchor_unit = if from_center {
                        CanvasPoint { x: 0.5, y: 0.5 }
                    } else {
                        HANDLE_UNITS[0]
                    };
                    let anchor = placement_point(original, anchor_unit);
                    let distance = if from_center { 1. } else { 2. };
                    let local_x = original.width * distance * if mirror_x { -1. } else { 1. };
                    let local_y = original.height * distance * if mirror_y { -1. } else { 1. };
                    let (sin, cos) = rotation.to_radians().sin_cos();
                    let target = CanvasPoint {
                        x: anchor.x + local_x * cos - local_y * sin,
                        y: anchor.y + local_x * sin + local_y * cos,
                    };
                    let draft = TransformDrag::new(original, start, DragMode::Resize(4))
                        .unwrap()
                        .updated(
                            target,
                            DragModifiers {
                                lock_ratio: true,
                                from_center,
                                shift: false,
                            },
                        );
                    assert!((draft.width - 16.).abs() < 0.0001, "{draft:?}");
                    assert!((draft.height - 8.).abs() < 0.0001, "{draft:?}");
                    assert_eq!(draft.flip_x, original.flip_x ^ mirror_x);
                    assert_eq!(draft.flip_y, original.flip_y ^ mirror_y);
                    let kept_anchor = placement_point(
                        draft,
                        CanvasPoint {
                            x: if mirror_x {
                                1. - anchor_unit.x
                            } else {
                                anchor_unit.x
                            },
                            y: if mirror_y {
                                1. - anchor_unit.y
                            } else {
                                anchor_unit.y
                            },
                        },
                    );
                    assert!(kept_anchor.distance(anchor) < 0.0001);
                }
            }
        }
    }
    #[test]
    fn distortion_moves_the_visible_handle_on_a_flipped_layer() {
        for (flip_x, flip_y) in [(true, false), (false, true), (true, true)] {
            let p = LayerPlacement {
                rotation: 37.,
                flip_x,
                flip_y,
                ..placement()
            };
            let start = placement_point(p, HANDLE_UNITS[0]);
            let corners = TransformGeometry::new(p, 0.).unwrap().corners();
            let result = TransformDrag::new(p, start, DragMode::Distort(0))
                .unwrap()
                .with_corners(corners)
                .unwrap()
                .distorted_corners(
                    CanvasPoint {
                        x: start.x + 3.,
                        y: start.y - 2.,
                    },
                    false,
                )
                .unwrap();
            for index in 0..4 {
                let expected = if index == 0 {
                    CanvasPoint {
                        x: corners[index].x + 3.,
                        y: corners[index].y - 2.,
                    }
                } else {
                    corners[index]
                };
                assert_eq!(result[index], expected, "flip_x={flip_x}, flip_y={flip_y}");
            }
        }
    }
    #[test]
    fn distortion_preserves_reflected_pixel_orientation_and_undo() {
        use image::{Rgba, RgbaImage};
        for (flip_x, flip_y) in [(true, false), (false, true), (true, true)] {
            let mut document = Document::new(20, 16);
            // Every pixel differs: a double reflection cannot pass unnoticed.
            document.layers[0].image = Some(
                RgbaImage::from_fn(8, 6, |x, y| Rgba([(x * 30) as u8, (y * 40) as u8, 83, 255]))
                    .into(),
            );
            let mut editor = Editor::new(document);
            let id = editor.active_layer.clone();
            let original = LayerPlacement {
                x: 4.,
                y: 3.,
                width: 8.,
                height: 6.,
                rotation: 0.,
                flip_x,
                flip_y,
            };
            assert!(editor.set_layer_placement(&id, original));
            let before = omuse::raster::composite(&editor.document);
            let geometry = TransformGeometry::new(
                selection_bounds(&editor, std::slice::from_ref(&id)).unwrap(),
                0.,
            )
            .unwrap();
            let drag = TransformDrag::new(original, geometry.handles[0], DragMode::Distort(0))
                .unwrap()
                .with_corners(geometry.corners())
                .unwrap();
            let corners = drag.distorted_corners(geometry.handles[0], false).unwrap();
            let depth = editor.undo_depth();
            assert!(
                editor
                    .distort_layer(&id, corners.map(|p| (p.x, p.y)))
                    .unwrap()
            );
            assert_eq!(
                omuse::raster::composite(&editor.document),
                before,
                "flip_x={flip_x}, flip_y={flip_y}"
            );
            assert_eq!(editor.undo_depth(), depth + 1);
            assert!(editor.undo());
            assert_eq!(editor.layer_placement(&id), Some(original));
            assert_eq!(omuse::raster::composite(&editor.document), before);
            assert!(editor.redo());
            assert_eq!(omuse::raster::composite(&editor.document), before);
        }
    }
    #[test]
    fn group_bounds_include_rotated_corners_and_commit_as_one_editor_step() {
        let mut editor = Editor::new(Document::new(20, 20));
        let first = editor.active_layer.clone();
        let second = editor.add_layer("Second");
        editor.set_layer_placement(
            &first,
            LayerPlacement {
                x: 0.,
                y: 0.,
                width: 4.,
                height: 2.,
                rotation: 90.,
                flip_x: false,
                flip_y: false,
            },
        );
        editor.set_layer_placement(
            &second,
            LayerPlacement {
                x: 10.,
                y: 4.,
                width: 2.,
                height: 2.,
                rotation: 0.,
                flip_x: false,
                flip_y: false,
            },
        );
        let ids = vec![first, second];
        let bounds = selection_bounds(&editor, &ids).unwrap();
        for (actual, expected) in [bounds.x, bounds.y, bounds.width, bounds.height]
            .into_iter()
            .zip([1., -1., 11., 7.])
        {
            assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
        }
        let depth = editor.undo_depth();
        let draft = LayerPlacement {
            x: bounds.x + 5.,
            ..bounds
        };
        assert!(commit_drag(&mut editor, &ids, bounds, draft));
        assert_eq!(editor.undo_depth(), depth + 1);
    }
    #[test]
    fn moving_a_selected_group_moves_descendants_and_linked_masks_as_one_step() {
        let mut editor = Editor::new(Document::new(20, 20));
        let child = editor.active_layer.clone();
        assert!(editor.add_mask(&child, true));
        let child_before = editor.layer_placement(&child).unwrap();
        let mask_before = editor.mask_placement(&child).unwrap();
        let group = editor
            .group_layers(std::slice::from_ref(&child), "Group")
            .unwrap();
        let ids = vec![group];
        let bounds = selection_bounds(&editor, &ids).unwrap();
        let depth = editor.undo_depth();
        let draft = LayerPlacement {
            x: bounds.x + 5.,
            y: bounds.y + 3.,
            ..bounds
        };

        assert!(commit_drag(&mut editor, &ids, bounds, draft));
        let child_after = editor.layer_placement(&child).unwrap();
        let mask_after = editor.mask_placement(&child).unwrap();
        assert_eq!(
            (child_after.x, child_after.y),
            (child_before.x + 5., child_before.y + 3.)
        );
        assert_eq!(
            (mask_after.x, mask_after.y),
            (mask_before.x + 5., mask_before.y + 3.)
        );
        assert_eq!(editor.undo_depth(), depth + 1);
        assert!(editor.undo());
        assert_eq!(editor.layer_placement(&child), Some(child_before));
        assert_eq!(editor.mask_placement(&child), Some(mask_before));
    }
    #[test]
    fn floating_selection_translation_keeps_its_existing_commit_and_cancel_path() {
        let mut editor = Editor::new(Document::new(8, 8));
        let source = editor.active_layer.clone();
        editor
            .document
            .find_layer_mut(&source)
            .unwrap()
            .image
            .as_mut()
            .unwrap()
            .put_pixel(2, 2, image::Rgba([200, 30, 10, 255]));
        editor.select_rectangle(2., 2., 1., 1.);
        let original = editor.document.find_layer(&source).unwrap().image.clone();

        let floating = editor.begin_floating_selection().unwrap().unwrap();
        let placement = editor.layer_placement(&floating).unwrap();
        assert!(commit_drag(
            &mut editor,
            std::slice::from_ref(&floating),
            placement,
            LayerPlacement {
                x: placement.x + 2.,
                ..placement
            },
        ));
        assert!(editor.cancel_floating_selection());
        assert_eq!(editor.document.layers.len(), 1);
        assert_eq!(editor.document.find_layer(&source).unwrap().image, original);

        let floating = editor.begin_floating_selection().unwrap().unwrap();
        let placement = editor.layer_placement(&floating).unwrap();
        assert!(commit_drag(
            &mut editor,
            std::slice::from_ref(&floating),
            placement,
            LayerPlacement {
                x: placement.x + 2.,
                ..placement
            },
        ));
        assert!(editor.commit_floating_selection().unwrap());
        assert_eq!(editor.undo_depth(), 1);
        let image = editor
            .document
            .find_layer(&source)
            .unwrap()
            .image
            .as_ref()
            .unwrap();
        assert_eq!(image.get_pixel(2, 2).0, [0; 4]);
        assert_eq!(image.get_pixel(4, 2).0, [200, 30, 10, 255]);
    }
    #[test]
    fn additive_and_toggle_selection_keep_a_primary() {
        let mut selection = LayerSelection::default();
        selection.click("a".into(), SelectionAction::Replace);
        selection.click("b".into(), SelectionAction::Add);
        assert_eq!(selection.primary.as_deref(), Some("b"));
        selection.click("b".into(), SelectionAction::Toggle);
        assert_eq!(selection.ids, vec!["a"]);
        assert_eq!(selection.primary.as_deref(), Some("a"));
    }
}
