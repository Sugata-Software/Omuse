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
                        (local_x * sx * self.original.width + local_y * sy * self.original.height)
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
