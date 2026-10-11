//! Retained, bounded flow layout for mixed native layers. Independently authored.
//!
//! Frames are ordinary pass-through folders. Their direct children retain their
//! image/text/vector sources; layout only changes placement. Nested frames and
//! rotated members are deliberately refused by version 1, rather than guessed.
use crate::model::{Document, Layer};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

pub const FRAME_KEY: &str = "omuseAutoLayout";
pub const ITEM_KEY: &str = "omuseLayoutItem";
pub const MAX_FRAMES: usize = 64;
pub const MAX_MEMBERS: usize = 256;
pub const MAX_TOTAL_MEMBERS: usize = 2048;
const MAX_EXTENT: f32 = 300_000.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flow {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum FrameSize {
    Hug,
    Fixed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ItemSize {
    Hug,
    Fixed(f32),
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameRule {
    pub version: u32,
    pub flow: Flow,
    pub width: FrameSize,
    pub height: FrameSize,
    /// Top, right, bottom, left, in canvas pixels.
    pub padding: [f32; 4],
    pub gap: f32,
    pub wrap: bool,
    pub align: Align,
}
impl Default for FrameRule {
    fn default() -> Self {
        Self {
            version: 1,
            flow: Flow::Horizontal,
            width: FrameSize::Hug,
            height: FrameSize::Hug,
            padding: [16.; 4],
            gap: 16.,
            wrap: false,
            align: Align::Start,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemRule {
    pub version: u32,
    pub width: ItemSize,
    pub height: ItemSize,
    /// Stable intrinsic scale, independent of a previous Fill/Fixed result.
    pub basis_scale: [f32; 2],
}
impl ItemRule {
    pub fn natural(layer: &Layer) -> Self {
        Self {
            version: 1,
            width: ItemSize::Hug,
            height: ItemSize::Hug,
            basis_scale: [layer.scale_x, layer.scale_y],
        }
    }
}

fn number(value: f32, minimum: f32) -> bool {
    value.is_finite() && (minimum..=MAX_EXTENT).contains(&value)
}
impl FrameRule {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported auto-layout rule version");
        ensure!(
            self.padding.iter().all(|v| number(*v, 0.)) && number(self.gap, 0.),
            "Layout padding and gap must be finite, nonnegative and at most 300000 pixels"
        );
        for size in [self.width, self.height] {
            if let FrameSize::Fixed(value) = size {
                ensure!(number(value, 1.), "Invalid fixed frame size");
            }
        }
        let main = if self.flow == Flow::Horizontal {
            self.width
        } else {
            self.height
        };
        ensure!(
            !self.wrap || matches!(main, FrameSize::Fixed(_)),
            "Wrapping needs a fixed size in the flow direction"
        );
        Ok(())
    }
}
impl ItemRule {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported layout item version");
        ensure!(
            self.basis_scale
                .iter()
                .all(|v| v.is_finite() && (0.001..=1000.).contains(&v.abs())),
            "Invalid intrinsic layout scale"
        );
        for size in [self.width, self.height] {
            if let ItemSize::Fixed(value) = size {
                ensure!(number(value, 1.), "Invalid fixed item size");
            }
        }
        Ok(())
    }
}

fn small_rule(value: &Value) -> Result<()> {
    fn visit(value: &Value, count: &mut usize, depth: usize) -> bool {
        *count += 1;
        if *count > 64 || depth > 4 {
            return false;
        }
        match value {
            Value::Array(v) => v.len() <= 4 && v.iter().all(|v| visit(v, count, depth + 1)),
            Value::Object(v) => {
                v.len() <= 12
                    && v.iter()
                        .all(|(k, v)| k.len() <= 32 && visit(v, count, depth + 1))
            }
            Value::String(v) => v.len() <= 32,
            _ => true,
        }
    }
    ensure!(
        visit(value, &mut 0, 0),
        "Layout rule exceeds metadata limits"
    );
    Ok(())
}
pub fn frame_rule(layer: &Layer) -> Result<Option<FrameRule>> {
    layer
        .metadata
        .get(FRAME_KEY)
        .map(|v| {
            small_rule(v)?;
            let rule: FrameRule =
                serde_json::from_value(v.clone()).context("Invalid auto-layout frame")?;
            rule.validate()?;
            Ok(rule)
        })
        .transpose()
}
pub fn item_rule(layer: &Layer) -> Result<Option<ItemRule>> {
    layer
        .metadata
        .get(ITEM_KEY)
        .map(|v| {
            small_rule(v)?;
            let rule: ItemRule =
                serde_json::from_value(v.clone()).context("Invalid auto-layout item")?;
            rule.validate()?;
            Ok(rule)
        })
        .transpose()
}
pub fn set_frame_rule(layer: &mut Layer, rule: &FrameRule) -> Result<()> {
    rule.validate()?;
    ensure!(layer.metadata.is_object(), "Invalid layer metadata");
    layer.metadata[FRAME_KEY] = serde_json::to_value(rule)?;
    Ok(())
}
pub fn set_item_rule(layer: &mut Layer, rule: &ItemRule) -> Result<()> {
    rule.validate()?;
    ensure!(layer.metadata.is_object(), "Invalid layer metadata");
    layer.metadata[ITEM_KEY] = serde_json::to_value(rule)?;
    Ok(())
}
pub fn has_layout(layers: &[Layer]) -> bool {
    layers.iter().any(|l| {
        l.metadata.get(FRAME_KEY).is_some()
            || l.metadata.get(ITEM_KEY).is_some()
            || has_layout(&l.children)
    })
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Auto-layout cancelled");
    Ok(())
}

#[derive(Clone, Debug)]
struct MemberPlacement {
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rule: ItemRule,
    frozen: bool,
}
#[derive(Clone, Debug)]
struct FramePlacement {
    id: String,
    width: f32,
    height: f32,
    members: Vec<MemberPlacement>,
}
#[derive(Debug)]
pub struct LayoutPlan {
    frames: Vec<FramePlacement>,
}
impl LayoutPlan {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn member_count(&self) -> usize {
        self.frames.iter().map(|f| f.members.len()).sum()
    }
}

/// Compute every placement before mutating anything. O(document layers + frame
/// members), no pixels are copied or rendered, no iterative constraint solver.
pub fn plan(document: &Document, cancel: &AtomicBool) -> Result<LayoutPlan> {
    cancelled(cancel)?;
    let mut frames = Vec::new();
    frames.try_reserve(MAX_FRAMES)?;
    let mut seen = std::collections::HashSet::new();
    seen.try_reserve(crate::model::MAX_LAYERS)?;
    fn walk(
        layers: &[Layer],
        depth: usize,
        parent_frame: bool,
        locked: bool,
        seen: &mut std::collections::HashSet<String>,
        frames: &mut Vec<FramePlacement>,
        total: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<()> {
        ensure!(depth <= 64, "Auto-layout hierarchy is too deep");
        for layer in layers {
            cancelled(cancel)?;
            ensure!(
                layer.id.len() <= 64
                    && seen.len() < crate::model::MAX_LAYERS
                    && seen.insert(layer.id.clone()),
                "Auto-layout hierarchy has duplicate identities or exceeds the layer limit"
            );
            let frame = frame_rule(layer)?;
            if frame.is_some() {
                ensure!(
                    !parent_frame,
                    "Nested auto-layout frames are not supported by version 1"
                );
                ensure!(
                    frames.len() < MAX_FRAMES,
                    "At most 64 auto-layout frames are supported"
                );
                *total += layer.children.len();
                ensure!(
                    *total <= MAX_TOTAL_MEMBERS,
                    "Auto-layout exceeds 2048 total members"
                );
                frames.push(plan_frame(
                    layer,
                    &frame.unwrap(),
                    locked || layer.locked,
                    cancel,
                )?);
            }
            ensure!(
                parent_frame || item_rule(layer)?.is_none(),
                "Layout item must belong directly to a frame"
            );
            walk(
                &layer.children,
                depth + 1,
                layer.metadata.get(FRAME_KEY).is_some(),
                locked || layer.locked,
                seen,
                frames,
                total,
                cancel,
            )?;
        }
        Ok(())
    }
    walk(
        &document.layers,
        0,
        false,
        false,
        &mut seen,
        &mut frames,
        &mut 0,
        cancel,
    )?;
    Ok(LayoutPlan { frames })
}

fn plan_frame(
    layer: &Layer,
    rule: &FrameRule,
    locked: bool,
    cancel: &AtomicBool,
) -> Result<FramePlacement> {
    ensure!(
        layer.is_group()
            && layer.image.is_none()
            && layer.advanced.is_none()
            && layer.vector_scene.is_none()
            && layer.mask.is_none()
            && layer.rotation == 0.
            && layer.scale_x == 1.
            && layer.scale_y == 1.,
        "Layout frames must be unrotated, unscaled folders without retained artwork or masks"
    );
    ensure!(
        layer.metadata.is_object()
            && layer
                .metadata
                .get("maskSourceID")
                .is_none_or(Value::is_null),
        "Unlink the frame's clipping mask before using auto-layout"
    );
    ensure!(
        layer.children.len() <= MAX_MEMBERS,
        "A layout frame supports at most 256 direct layers"
    );
    ensure!(
        layer.offset_x.is_finite()
            && layer.offset_y.is_finite()
            && layer.offset_x.abs() <= 1_000_000.
            && layer.offset_y.abs() <= 1_000_000.,
        "Invalid frame origin"
    );
    let horizontal = rule.flow == Flow::Horizontal;
    let (main_size, cross_size) = if horizontal {
        (rule.width, rule.height)
    } else {
        (rule.height, rule.width)
    };
    let (main_start, main_end, cross_start, cross_end) = if horizontal {
        (
            rule.padding[3],
            rule.padding[1],
            rule.padding[0],
            rule.padding[2],
        )
    } else {
        (
            rule.padding[0],
            rule.padding[2],
            rule.padding[3],
            rule.padding[1],
        )
    };
    let available = |s: FrameSize, a: f32, b: f32| -> Result<Option<f32>> {
        match s {
            FrameSize::Hug => Ok(None),
            FrameSize::Fixed(n) => {
                ensure!(n >= a + b + 1., "Frame padding leaves no room for content");
                Ok(Some(n - a - b))
            }
        }
    };
    let main_available = available(main_size, main_start, main_end)?;
    let cross_available = available(cross_size, cross_start, cross_end)?;
    struct Item<'a> {
        layer: &'a Layer,
        rule: ItemRule,
        main: f32,
        cross: f32,
        fill_main: bool,
        fill_cross: bool,
    }
    let mut items = Vec::new();
    items.try_reserve(layer.children.len())?;
    for child in &layer.children {
        cancelled(cancel)?;
        ensure!(child.metadata.is_object(), "Invalid layout member metadata");
        ensure!(
            !child.is_group() && child.children.is_empty() && child.image.is_some(),
            "Frames accept direct photo, text and vector layers; nested folders and adjustments are unsupported"
        );
        ensure!(
            child.rotation.rem_euclid(360.) == 0.,
            "Unrotate the layer before adding it to auto-layout"
        );
        ensure!(
            !child
                .metadata
                .get("clippingMask")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && child
                    .metadata
                    .get("maskSourceID")
                    .is_none_or(Value::is_null),
            "Unlink clipping masks before adding layers to auto-layout"
        );
        let item = item_rule(child)?.unwrap_or_else(|| ItemRule::natural(child));
        item.validate()?;
        let image = child.image.as_ref().unwrap();
        let natural = [
            image.width() as f32 * item.basis_scale[0].abs(),
            image.height() as f32 * item.basis_scale[1].abs(),
        ];
        let dimensions = [item.width, item.height];
        let mut size = [0.; 2];
        for axis in 0..2 {
            size[axis] = match dimensions[axis] {
                ItemSize::Hug => natural[axis],
                ItemSize::Fixed(n) => n,
                ItemSize::Fill => 1.,
            };
            ensure!(
                number(size[axis], 1.),
                "Layout member size exceeds supported bounds"
            );
        }
        let (m, c, ms, cs) = if horizontal {
            (size[0], size[1], item.width, item.height)
        } else {
            (size[1], size[0], item.height, item.width)
        };
        ensure!(
            !matches!(ms, ItemSize::Fill) || main_available.is_some(),
            "Fill inside a hugging frame creates a size cycle; fix the frame width/height first"
        );
        ensure!(
            !matches!(cs, ItemSize::Fill) || cross_available.is_some(),
            "Fill inside a hugging frame creates a size cycle; fix the frame width/height first"
        );
        ensure!(
            !rule.wrap || !matches!(ms, ItemSize::Fill) && !matches!(cs, ItemSize::Fill),
            "Wrapping currently supports Hug and Fixed items; turn wrapping off before using Fill"
        );
        if child.visible {
            items.push(Item {
                layer: child,
                rule: item,
                main: m,
                cross: c,
                fill_main: matches!(ms, ItemSize::Fill),
                fill_cross: matches!(cs, ItemSize::Fill),
            });
        }
    }
    let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
    rows.try_reserve(items.len().max(1))?;
    let mut start = 0;
    let mut used = 0.;
    for (i, item) in items.iter().enumerate() {
        let next = used + if i > start { rule.gap } else { 0. } + item.main;
        if rule.wrap && i > start && next > main_available.unwrap() {
            rows.push(start..i);
            start = i;
            used = item.main;
        } else {
            used = next;
        }
    }
    if !items.is_empty() {
        rows.push(start..items.len());
    }
    let mut placements = Vec::new();
    placements.try_reserve(items.len())?;
    let mut cross_cursor = cross_start;
    let mut largest_main = 0_f32;
    for row in rows {
        cancelled(cancel)?;
        let gaps = rule.gap * row.len().saturating_sub(1) as f32;
        let fill = items[row.clone()].iter().filter(|i| i.fill_main).count();
        let fixed = items[row.clone()]
            .iter()
            .filter(|i| !i.fill_main)
            .map(|i| i.main)
            .sum::<f32>()
            + gaps;
        let fill_size = if fill > 0 {
            (main_available.unwrap() - fixed) / fill as f32
        } else {
            0.
        };
        ensure!(
            fill == 0 || number(fill_size, 1.),
            "Frame is too small for its fixed items, gaps and Fill members"
        );
        let natural_cross = items[row.clone()]
            .iter()
            .map(|i| i.cross)
            .fold(1_f32, f32::max);
        let row_cross = if rule.wrap {
            natural_cross
        } else {
            cross_available.unwrap_or(natural_cross)
        };
        let mut cursor = main_start;
        for item in &items[row] {
            let main = if item.fill_main { fill_size } else { item.main };
            let cross = if item.fill_cross {
                row_cross
            } else {
                item.cross
            };
            let align = match rule.align {
                Align::Start => 0.,
                Align::Center => (row_cross - cross) * 0.5,
                Align::End => row_cross - cross,
            };
            let (x, y, w, h) = if horizontal {
                (cursor, cross_cursor + align, main, cross)
            } else {
                (cross_cursor + align, cursor, cross, main)
            };
            let (x, y) = (layer.offset_x + x, layer.offset_y + y);
            ensure!(
                x.is_finite() && y.is_finite() && x.abs() <= 1_000_000. && y.abs() <= 1_000_000.,
                "Layout position exceeds supported bounds"
            );
            let image = item.layer.image.as_ref().unwrap();
            ensure!(
                [w / image.width() as f32, h / image.height() as f32]
                    .iter()
                    .all(|s| (0.001..=1000.).contains(s)),
                "Layout would exceed layer scale limits"
            );
            if locked || item.layer.locked {
                ensure!(
                    x == item.layer.offset_x
                        && y == item.layer.offset_y
                        && w / image.width() as f32 == item.layer.scale_x.abs()
                        && h / image.height() as f32 == item.layer.scale_y.abs(),
                    "Auto-layout would move or resize a locked layer; unlock it first"
                );
            }
            placements.push(MemberPlacement {
                id: item.layer.id.clone(),
                x,
                y,
                width: w,
                height: h,
                rule: item.rule.clone(),
                frozen: locked || item.layer.locked,
            });
            cursor += main + rule.gap;
        }
        largest_main = largest_main.max((cursor - main_start - rule.gap).max(0.));
        cross_cursor += row_cross + rule.gap;
    }
    let natural_main = (main_start + largest_main + main_end).max(1.);
    let natural_cross =
        (cross_cursor - if items.is_empty() { 0. } else { rule.gap } + cross_end).max(1.);
    let main = match main_size {
        FrameSize::Hug => natural_main,
        FrameSize::Fixed(n) => n,
    };
    let cross = match cross_size {
        FrameSize::Hug => natural_cross,
        FrameSize::Fixed(n) => n,
    };
    ensure!(
        number(main, 1.) && number(cross, 1.),
        "Layout frame exceeds supported size bounds"
    );
    let (width, height) = if horizontal {
        (main, cross)
    } else {
        (cross, main)
    };
    if locked && let Some((w, h)) = extent(layer) {
        ensure!(
            width == w && height == h,
            "Auto-layout would resize a locked frame; unlock it first"
        );
    }
    Ok(FramePlacement {
        id: layer.id.clone(),
        width,
        height,
        members: placements,
    })
}

/// Apply a freshly computed plan to its document. Callers stage a document or
/// retain an undo snapshot, so cancellation after any step cannot publish it.
pub fn reflow(document: &mut Document, cancel: &AtomicBool) -> Result<bool> {
    if !has_layout(&document.layers) {
        cancelled(cancel)?;
        return Ok(false);
    }
    let plan = plan(document, cancel)?;
    cancelled(cancel)?;
    // The plan contains at most 2048 members; no cancellation is observed after
    // this final fence, making publication of the bounded placement pass atomic.
    let mut changed = false;
    for frame in plan.frames {
        for p in frame.members {
            if p.frozen {
                continue;
            }
            let layer = document
                .find_layer_mut(&p.id)
                .context("Layout member disappeared")?;
            let image = layer.image.as_ref().unwrap();
            let old = crate::editor::LayerPlacement {
                x: layer.offset_x,
                y: layer.offset_y,
                width: image.width() as f32 * layer.scale_x.abs(),
                height: image.height() as f32 * layer.scale_y.abs(),
                rotation: layer.rotation,
                flip_x: layer.scale_x < 0.,
                flip_y: layer.scale_y < 0.,
            };
            let next = crate::editor::LayerPlacement {
                x: p.x,
                y: p.y,
                width: p.width,
                height: p.height,
                rotation: 0.,
                flip_x: layer.scale_x < 0.,
                flip_y: layer.scale_y < 0.,
            };
            if old != next {
                crate::editor::carry_mask_placement(layer, old, next);
                layer.offset_x = p.x;
                layer.offset_y = p.y;
                layer.rotation = 0.;
                let image = layer.image.as_ref().unwrap();
                layer.scale_x = p.width / image.width() as f32 * layer.scale_x.signum();
                layer.scale_y = p.height / image.height() as f32 * layer.scale_y.signum();
                changed = true;
            }
            if item_rule(layer)?.as_ref() != Some(&p.rule) {
                set_item_rule(layer, &p.rule)?;
                changed = true;
            }
        }
        let group = document.find_layer_mut(&frame.id).unwrap();
        let extent = json!([frame.width, frame.height]);
        if group.metadata.get("omuseLayoutExtent") != Some(&extent) {
            group.metadata["omuseLayoutExtent"] = extent.clone();
            changed = true;
        }
        if !group
            .metadata
            .get("transform")
            .is_some_and(Value::is_object)
        {
            group.metadata["transform"] = json!({});
            changed = true;
        }
        if group.metadata.pointer("/transform/size") != Some(&extent) {
            group.metadata["transform"]["size"] = extent;
            changed = true;
        }
    }
    Ok(changed)
}

/// Explicit reparenting out of a frame releases that layer's layout rule while
/// preserving its current placement. No IDs or sources are rewritten.
pub(crate) fn release_orphans(layers: &mut [Layer], inside_frame: bool) {
    for layer in layers {
        if !inside_frame {
            if let Some(m) = layer.metadata.as_object_mut() {
                m.remove(ITEM_KEY);
            }
        }
        let frame = layer.metadata.get(FRAME_KEY).is_some();
        release_orphans(&mut layer.children, frame);
    }
}

pub fn extent(layer: &Layer) -> Option<(f32, f32)> {
    let v = layer.metadata.get("omuseLayoutExtent")?.as_array()?;
    let (w, h) = (v.first()?.as_f64()? as f32, v.get(1)?.as_f64()? as f32);
    (number(w, 1.) && number(h, 1.)).then_some((w, h))
}

/// Files retain both their rules and derived placement. Refuse stale geometry
/// instead of making an unrelated later edit silently rearrange the artwork.
pub fn validate_settled(document: &Document) -> Result<()> {
    if !has_layout(&document.layers) {
        return Ok(());
    }
    let planned = plan(document, &AtomicBool::new(false))?;
    for frame in planned.frames {
        let group = document.find_layer(&frame.id).unwrap();
        ensure!(
            extent(group) == Some((frame.width, frame.height)),
            "Auto-layout frame extent is stale; reflow before saving"
        );
        let expected = json!([frame.width, frame.height]);
        ensure!(
            group.metadata.pointer("/transform/size") == Some(&expected),
            "Auto-layout frame transform extent is stale"
        );
        for member in frame.members {
            let layer = document.find_layer(&member.id).unwrap();
            let image = layer.image.as_ref().unwrap();
            ensure!(
                layer.offset_x == member.x
                    && layer.offset_y == member.y
                    && layer.scale_x.abs() == member.width / image.width() as f32
                    && layer.scale_y.abs() == member.height / image.height() as f32,
                "Auto-layout member placement is stale; reflow before saving"
            );
        }
    }
    Ok(())
}
