//! Pure, transactional operations on vector scene object selections.
//!
//! These helpers return a new scene and never partially mutate the caller's
//! scene. Group transforms are deliberately baked into each object's affine
//! transform; groups carry membership and names only.

use crate::vector_path::Point;
use crate::vector_scene::{VectorGroup, VectorObject, VectorScene};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionBounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl SelectionBounds {
    pub fn width(self) -> f32 {
        self.right - self.left
    }
    pub fn height(self) -> f32 {
        self.bottom - self.top
    }
    pub fn center(self) -> Point {
        Point {
            x: (self.left + self.right) * 0.5,
            y: (self.top + self.bottom) * 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignAxis {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
}

fn indices(scene: &VectorScene, selected: &[usize]) -> Result<Vec<usize>> {
    ensure!(!selected.is_empty(), "a selection is required");
    let mut result = selected.to_vec();
    result.sort_unstable();
    ensure!(
        result.windows(2).all(|pair| pair[0] != pair[1]),
        "selection contains duplicate object indices"
    );
    ensure!(
        result.iter().all(|index| *index < scene.objects.len()),
        "selection object index is outside the scene"
    );
    Ok(result)
}

fn common_parent(scene: &VectorScene, selected: &[usize]) -> Vec<VectorGroup> {
    let mut parent = scene.objects[selected[0]].groups.clone();
    for index in &selected[1..] {
        let path = &scene.objects[*index].groups;
        let common = parent
            .iter()
            .zip(path)
            .take_while(|(left, right)| left == right)
            .count();
        parent.truncate(common);
    }
    parent
}

/// For reorder, a fully selected group is itself the movable unit. Promote
/// through any such complete ancestry until the first partially selected
/// parent; this keeps the group block together among its siblings.
fn reorder_parent(scene: &VectorScene, selected: &[usize]) -> Vec<VectorGroup> {
    let mut parent = common_parent(scene, selected);
    let selected_set: HashSet<usize> = selected.iter().copied().collect();
    while let Some(group) = parent.last() {
        let depth = parent.len();
        let members: Vec<usize> = scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| {
                object.groups.len() >= depth
                    && object.groups[..depth - 1] == parent[..depth - 1]
                    && object.groups[depth - 1].id == group.id
            })
            .map(|(index, _)| index)
            .collect();
        if members.iter().all(|index| selected_set.contains(index)) {
            parent.pop();
        } else {
            break;
        }
    }
    parent
}

/// A group operation wraps children at the common parent boundary. Existing
/// child groups at that boundary must be selected as whole blocks; deeper
/// groups remain in each object's path after the new wrapper.
fn ensure_complete_boundary_groups(
    scene: &VectorScene,
    selected: &[usize],
    parent: &[VectorGroup],
) -> Result<()> {
    let depth = parent.len();
    let selected_set: HashSet<usize> = selected.iter().copied().collect();
    let ids: HashSet<&str> = selected
        .iter()
        .filter_map(|index| scene.objects[*index].groups.get(depth))
        .map(|group| group.id.as_str())
        .collect();
    for id in ids {
        ensure!(
            scene.objects.iter().enumerate().all(|(index, object)| {
                object.groups.len() <= depth
                    || object.groups[..depth] != parent[..]
                    || object.groups[depth].id != id
                    || selected_set.contains(&index)
            }),
            "a grouped selection must contain each selected group's members"
        );
    }
    Ok(())
}

/// Group selected objects, preserving their relative paint order and moving
/// the chosen block near the highest selected slot.
pub fn group(
    scene: &VectorScene,
    selected: &[usize],
    name: &str,
) -> Result<(VectorScene, Vec<usize>)> {
    let selected = indices(scene, selected)?;
    ensure!(
        !name.trim().is_empty() && name.len() <= 16_384,
        "invalid vector group name"
    );
    let parent = common_parent(scene, &selected);
    ensure_complete_boundary_groups(scene, &selected, &parent)?;
    let group = VectorGroup {
        id: uuid::Uuid::new_v4().to_string().to_uppercase(),
        name: name.to_owned(),
    };
    let selected_set: std::collections::HashSet<usize> = selected.iter().copied().collect();
    let chosen: Vec<VectorObject> = selected
        .iter()
        .map(|index| {
            let mut object = scene.objects[*index].clone();
            object
                .groups
                .splice(parent.len()..parent.len(), [group.clone()]);
            object
        })
        .collect();
    let remaining: Vec<VectorObject> = scene
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| !selected_set.contains(index))
        .map(|(_, object)| object.clone())
        .collect();
    let insertion = selected
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_sub(chosen.len().saturating_sub(1))
        .min(remaining.len());
    let mut objects = remaining;
    objects.splice(insertion..insertion, chosen);
    let result = VectorScene {
        version: scene
            .version
            .max(crate::vector_scene::VECTOR_SCENE_GROUP_VERSION),
        objects,
        ..scene.clone()
    };
    result.validate()?;
    Ok((result, (insertion..insertion + selected.len()).collect()))
}

/// Move complete group blocks, or individually selected ungrouped siblings,
/// within their shared parent. A selected child group must be selected as a
/// whole; no unit can cross its parent boundary.
pub fn reorder(
    scene: &VectorScene,
    selected: &[usize],
    forward: bool,
) -> Result<(VectorScene, Vec<usize>)> {
    let selected = indices(scene, selected)?;
    let parent = reorder_parent(scene, &selected);
    ensure_complete_boundary_groups(scene, &selected, &parent)?;
    let depth = parent.len();
    let selected_ids: HashSet<String> = selected
        .iter()
        .map(|i| scene.objects[*i].id.clone())
        .collect();
    let scope: Vec<usize> = scene
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| object.groups.len() >= depth && object.groups[..depth] == parent[..])
        .map(|(i, _)| i)
        .collect();
    ensure!(!scope.is_empty(), "selection parent has no objects");
    ensure!(
        scope.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "selection crosses a parent boundary"
    );

    let mut units: Vec<Vec<usize>> = Vec::new();
    let mut by_group: HashMap<String, usize> = HashMap::new();
    for index in &scope {
        let object = &scene.objects[*index];
        if let Some(group) = object.groups.get(depth) {
            if let Some(unit) = by_group.get(&group.id).copied() {
                units[unit].push(*index);
            } else {
                by_group.insert(group.id.clone(), units.len());
                units.push(vec![*index]);
            }
        } else {
            units.push(vec![*index]);
        }
    }
    let mut selected_units = vec![false; units.len()];
    for (unit_index, unit) in units.iter().enumerate() {
        selected_units[unit_index] = unit
            .iter()
            .any(|index| selected_ids.contains(&scene.objects[*index].id));
    }
    if forward {
        for i in (0..units.len().saturating_sub(1)).rev() {
            if selected_units[i] && !selected_units[i + 1] {
                units.swap(i, i + 1);
                selected_units.swap(i, i + 1);
            }
        }
    } else {
        for i in 1..units.len() {
            if selected_units[i] && !selected_units[i - 1] {
                units.swap(i, i - 1);
                selected_units.swap(i, i - 1);
            }
        }
    }
    let mut result = scene.clone();
    let replacement: Vec<VectorObject> = units
        .into_iter()
        .flatten()
        .map(|i| scene.objects[i].clone())
        .collect();
    let start = scope[0];
    result
        .objects
        .splice(start..=scope[scope.len() - 1], replacement);
    let selection_ids: HashSet<String> = selected
        .iter()
        .map(|i| scene.objects[*i].id.clone())
        .collect();
    let selection = result
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| selection_ids.contains(&object.id))
        .map(|(i, _)| i)
        .collect();
    result.validate()?;
    Ok((result, selection))
}

/// Remove the outermost group(s) containing the selected objects.
pub fn ungroup(scene: &VectorScene, selected: &[usize]) -> Result<(VectorScene, Vec<usize>)> {
    let selected = expand_selection(scene, selected)?;
    let ids: std::collections::HashSet<String> = selected
        .iter()
        .filter_map(|index| {
            scene.objects[*index]
                .groups
                .first()
                .map(|group| group.id.clone())
        })
        .collect();
    ensure!(!ids.is_empty(), "selection is not grouped");
    let mut result = scene.clone();
    for object in &mut result.objects {
        if object
            .groups
            .first()
            .is_some_and(|group| ids.contains(&group.id))
        {
            object.groups.remove(0);
        }
    }
    result.validate()?;
    Ok((result, selected))
}

/// Expand selected objects to all objects in their outermost groups.
pub fn expand_selection(scene: &VectorScene, selected: &[usize]) -> Result<Vec<usize>> {
    let selected = indices(scene, selected)?;
    let ids: std::collections::HashSet<&str> = selected
        .iter()
        .filter_map(|index| {
            scene.objects[*index]
                .groups
                .first()
                .map(|group| group.id.as_str())
        })
        .collect();
    let expanded: Vec<usize> = scene
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            object
                .groups
                .first()
                .filter(|group| ids.contains(group.id.as_str()))
                .map(|_| index)
        })
        .collect();
    let mut result: std::collections::BTreeSet<usize> = selected.into_iter().collect();
    result.extend(expanded);
    Ok(result.into_iter().collect())
}

pub fn bounds(scene: &VectorScene, selected: &[usize]) -> Result<Option<SelectionBounds>> {
    let selected = indices(scene, selected)?;
    let mut result: Option<SelectionBounds> = None;
    for index in selected {
        for subpath in &scene.objects[index].path.subpaths {
            for anchor in &subpath.anchors {
                for point in [Some(anchor.position), anchor.incoming, anchor.outgoing]
                    .into_iter()
                    .flatten()
                {
                    let [a, b, c, d, e, f] = scene.objects[index].transform;
                    let point = Point {
                        x: a * point.x + c * point.y + e,
                        y: b * point.x + d * point.y + f,
                    };
                    result = Some(match result {
                        None => SelectionBounds {
                            left: point.x,
                            top: point.y,
                            right: point.x,
                            bottom: point.y,
                        },
                        Some(mut b) => {
                            b.left = b.left.min(point.x);
                            b.top = b.top.min(point.y);
                            b.right = b.right.max(point.x);
                            b.bottom = b.bottom.max(point.y);
                            b
                        }
                    });
                }
            }
        }
    }
    Ok(result)
}

fn apply_matrix(scene: &VectorScene, selected: &[usize], matrix: [f32; 6]) -> Result<VectorScene> {
    let selected = indices(scene, selected)?;
    let mut result = scene.clone();
    for index in selected {
        let old = result.objects[index].transform;
        result.objects[index].transform = [
            matrix[0] * old[0] + matrix[2] * old[1],
            matrix[1] * old[0] + matrix[3] * old[1],
            matrix[0] * old[2] + matrix[2] * old[3],
            matrix[1] * old[2] + matrix[3] * old[3],
            matrix[0] * old[4] + matrix[2] * old[5] + matrix[4],
            matrix[1] * old[4] + matrix[3] * old[5] + matrix[5],
        ];
    }
    result.validate()?;
    Ok(result)
}

pub fn translate(scene: &VectorScene, selected: &[usize], dx: f32, dy: f32) -> Result<VectorScene> {
    ensure!(
        dx.is_finite() && dy.is_finite(),
        "translation must be finite"
    );
    apply_matrix(scene, selected, [1., 0., 0., 1., dx, dy])
}

pub fn scale(
    scene: &VectorScene,
    selected: &[usize],
    sx: f32,
    sy: f32,
    anchor: Point,
) -> Result<VectorScene> {
    ensure!(
        sx.is_finite() && sy.is_finite() && sx.abs() >= 1e-6 && sy.abs() >= 1e-6,
        "scale is outside supported bounds"
    );
    apply_matrix(
        scene,
        selected,
        [sx, 0., 0., sy, anchor.x * (1. - sx), anchor.y * (1. - sy)],
    )
}

pub fn rotate(
    scene: &VectorScene,
    selected: &[usize],
    degrees: f32,
    anchor: Point,
) -> Result<VectorScene> {
    ensure!(degrees.is_finite(), "rotation must be finite");
    let radians = degrees.to_radians();
    let (s, c) = radians.sin_cos();
    apply_matrix(
        scene,
        selected,
        [
            c,
            s,
            -s,
            c,
            anchor.x * (1. - c) + anchor.y * s,
            anchor.y * (1. - c) - anchor.x * s,
        ],
    )
}

fn selection_units(scene: &VectorScene, selected: &[usize]) -> Result<Vec<Vec<usize>>> {
    let selected = indices(scene, selected)?;
    let selected_set: HashSet<_> = selected.iter().copied().collect();
    let mut units = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for index in selected {
        if let Some(group) = scene.objects[index].groups.first() {
            let members: Vec<usize> = scene
                .objects
                .iter()
                .enumerate()
                .filter_map(|(member, object)| {
                    object
                        .groups
                        .first()
                        .filter(|candidate| candidate.id == group.id)
                        .map(|_| member)
                })
                .collect();
            ensure!(!members.is_empty(), "group has no members");
            if members.iter().all(|member| selected_set.contains(member)) {
                if seen.insert(group.id.clone()) {
                    units.push(members);
                }
            } else {
                units.push(vec![index]);
            }
        } else {
            units.push(vec![index]);
        }
    }
    Ok(units)
}

fn unit_bounds(scene: &VectorScene, unit: &[usize]) -> Result<SelectionBounds> {
    bounds(scene, unit)?.ok_or_else(|| anyhow::anyhow!("selection has no geometry"))
}

fn translate_unit(scene: &VectorScene, unit: &[usize], dx: f32, dy: f32) -> Result<VectorScene> {
    apply_matrix(scene, unit, [1., 0., 0., 1., dx, dy])
}

pub fn align(scene: &VectorScene, selected: &[usize], axis: AlignAxis) -> Result<VectorScene> {
    let units = selection_units(scene, selected)?;
    ensure!(units.len() >= 2, "at least two objects are required");
    let all_indices: Vec<usize> = units.iter().flatten().copied().collect();
    let all = unit_bounds(scene, &all_indices)?;
    let mut result = scene.clone();
    for unit in units {
        let current = unit_bounds(scene, &unit)?;
        let (dx, dy) = match axis {
            AlignAxis::Left => (all.left - current.left, 0.),
            AlignAxis::HorizontalCenter => (all.center().x - current.center().x, 0.),
            AlignAxis::Right => (all.right - current.right, 0.),
            AlignAxis::Top => (0., all.top - current.top),
            AlignAxis::VerticalCenter => (0., all.center().y - current.center().y),
            AlignAxis::Bottom => (0., all.bottom - current.bottom),
        };
        result = translate_unit(&result, &unit, dx, dy)?;
    }
    Ok(result)
}

pub fn distribute(
    scene: &VectorScene,
    selected: &[usize],
    axis: DistributeAxis,
) -> Result<VectorScene> {
    let units = selection_units(scene, selected)?;
    ensure!(units.len() >= 3, "at least three objects are required");
    let mut order = units;
    order.sort_by(|a, b| {
        let Ok(aa) = unit_bounds(scene, a) else {
            return std::cmp::Ordering::Equal;
        };
        let Ok(bb) = unit_bounds(scene, b) else {
            return std::cmp::Ordering::Equal;
        };
        let av = if axis == DistributeAxis::Horizontal {
            aa.left
        } else {
            aa.top
        };
        let bv = if axis == DistributeAxis::Horizontal {
            bb.left
        } else {
            bb.top
        };
        av.total_cmp(&bv)
    });
    let first = unit_bounds(scene, &order[0])?;
    let last = unit_bounds(scene, &order[order.len() - 1])?;
    let span = if axis == DistributeAxis::Horizontal {
        last.right - first.left
    } else {
        last.bottom - first.top
    };
    let mut total = 0.;
    for unit in &order {
        let b = unit_bounds(scene, unit)?;
        total += if axis == DistributeAxis::Horizontal {
            b.width()
        } else {
            b.height()
        };
    }
    let gap = (span - total) / (order.len() - 1) as f32;
    ensure!(
        gap.is_finite() && gap >= 0.,
        "objects overlap too much to distribute"
    );
    let mut result = scene.clone();
    let mut cursor = if axis == DistributeAxis::Horizontal {
        first.left
    } else {
        first.top
    };
    for unit in order {
        let b = unit_bounds(&result, &unit)?;
        let delta = if axis == DistributeAxis::Horizontal {
            cursor - b.left
        } else {
            cursor - b.top
        };
        result = translate_unit(
            &result,
            &unit,
            if axis == DistributeAxis::Horizontal {
                delta
            } else {
                0.
            },
            if axis == DistributeAxis::Vertical {
                delta
            } else {
                0.
            },
        )?;
        let b = unit_bounds(&result, &unit)?;
        cursor += if axis == DistributeAxis::Horizontal {
            b.width() + gap
        } else {
            b.height() + gap
        };
    }
    Ok(result)
}
