//! Native, UI-independent editing operations and bounded document history.
//!
//! Canvas coordinates are used by all pointer and selection operations. Layers
//! retain their transforms; editing maps the pointer into their source image.
use crate::brush_dynamics::{Dab, InputPoint};
use crate::model::{Document, Layer, PixelRect};
use crate::retouch_brush::RetouchMode;
use crate::spot_heal::SpotHealingMode;
use image::{GrayImage, Rgba, RgbaImage};
use std::collections::VecDeque;
#[path = "editor_advanced.rs"]
mod advanced;
#[path = "editor_dynamics.rs"]
mod editor_dynamics;
#[path = "editor_history.rs"]
mod editor_history;
#[path = "raster_patch.rs"]
mod raster_patch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasGuide {
    pub id: String,
    pub axis: GuideAxis,
    pub position: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimBasedOn {
    Transparent,
    TopLeft,
    BottomRight,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrimOptions {
    pub based_on: TrimBasedOn,
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    pub tolerance: u8,
}
impl Default for TrimOptions {
    fn default() -> Self {
        Self {
            based_on: TrimBasedOn::Transparent,
            top: true,
            bottom: true,
            left: true,
            right: true,
            tolerance: 0,
        }
    }
}

const DEFAULT_HISTORY_BYTES: usize = 256 * 1024 * 1024;
const MAX_HISTORY_STEPS: usize = 100;
// Small rasters keep the cheaper single-allocation snapshot path. Region
// history targets the full-image copies that dominate larger photo strokes.
const REGION_HISTORY_MIN_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintTool {
    Brush,
    Pencil,
    Eraser,
}

#[derive(Clone, Debug)]
pub struct Brush {
    pub color: [u8; 4],
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    /// Brush/eraser pulled-string length in screen points. Zero preserves unsmoothed input exactly.
    pub smoothing: f32,
}
impl Default for Brush {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            size: 16.0,
            opacity: 1.0,
            hardness: 0.85,
            smoothing: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Adjustment {
    /// Add a fraction of full intensity, in -1..=1.
    Brightness(f32),
    /// Contrast change in -1..=1, with zero preserving the image.
    Contrast(f32),
    /// Saturation change in -1..=1, with -1 producing grayscale.
    Saturation(f32),
    Invert,
    Grayscale,
    Blur(f32),
    Sharpen(f32),
}

/// Unrotated bounds in canvas pixels. Rotation is clockwise around the center;
/// flips affect orientation without changing the bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}
impl LayerPlacement {
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height, self.rotation]
            .iter()
            .all(|v| v.is_finite())
            && (1.0..=300_000.0).contains(&self.width)
            && (1.0..=300_000.0).contains(&self.height)
            && self.x.abs() <= 1_000_000.0
            && self.y.abs() <= 1_000_000.0
    }
    pub fn center(self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
    pub fn point(self, unit_x: f32, unit_y: f32) -> (f32, f32) {
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let x = (unit_x - 0.5) * self.width * if self.flip_x { -1.0 } else { 1.0 };
        let y = (unit_y - 0.5) * self.height * if self.flip_y { -1.0 } else { 1.0 };
        let center = self.center();
        (center.0 + x * cos - y * sin, center.1 + x * sin + y * cos)
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        let center = self.center();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (x, y) = (x - center.0, y - center.1);
        (x * cos + y * sin).abs() <= self.width * 0.5
            && (-x * sin + y * cos).abs() <= self.height * 0.5
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub width: u32,
    pub height: u32,
    /// Row-major coverage, 0 excluded and 255 included.
    pub mask: Vec<u8>,
}
impl Selection {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && (x as u32) < self.width
            && (y as u32) < self.height
            && self
                .mask
                .get(y as usize * self.width as usize + x as usize)
                .copied()
                .unwrap_or(0)
                != 0
    }
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut left, mut top, mut right, mut bottom) = (self.width, self.height, 0, 0);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.contains(x as i32, y as i32) {
                    found = true;
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x);
                    bottom = bottom.max(y);
                }
            }
        }
        found.then_some((
            left,
            top,
            right.saturating_sub(left) + 1,
            bottom.saturating_sub(top) + 1,
        ))
    }
}

#[derive(Clone)]
struct Snapshot {
    document: Document,
    active_layer: String,
    selection: Option<Selection>,
    revision: u64,
    bytes: usize,
}

enum HistoryEntry {
    Snapshot(Snapshot),
    Raster(RasterHistory),
}

struct RasterHistory {
    state: Snapshot,
    layer_id: String,
    expected_revision: u64,
    pixels: std::cell::RefCell<raster_patch::RasterPatch>,
}

/// Diagnostic counters for bounded history and isolated performance checks.
/// Detached bytes are cumulative copies during painting and region undo/redo,
/// not process memory or physical input latency.
#[derive(Clone, Copy, Debug, Default)]
pub struct HistoryStats {
    pub raster_patch_entries: usize,
    pub raster_patch_tiles: usize,
    pub raster_patch_bytes: usize,
    pub detached_raster_bytes: usize,
}
/// Source-image pixels changed since the active stroke's last damage read.
/// Finishing or cancelling the stroke discards pending damage; callers should
/// refresh the final/restored document after either operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrokeDamage {
    pub layer_id: String,
    pub local_rect: PixelRect,
    pub mask_target: bool,
}

struct Stroke {
    before: HistoryEntry,
    last: (f32, f32, f32),
    tool: PaintTool,
    brush: Brush,
    layer_id: String,
    changed: bool,
    damage: Option<PixelRect>,
    clone: Option<CloneStroke>,
    clone_canvas: Option<RgbaImage>,
    mask_target: bool,
    coverage: std::cell::RefCell<std::collections::HashMap<usize, f32>>,
    overflow: std::cell::Cell<bool>,
    smoothing_anchor: (f32, f32),
    pointer: (f32, f32, f32),
    last_input: InputPoint,
    dynamics: Option<editor_dynamics::StrokeState>,
}
impl Stroke {
    fn uses_smoothing(&self) -> bool {
        self.clone.is_none()
            && matches!(self.tool, PaintTool::Brush | PaintTool::Eraser)
            && self.brush.smoothing > 0.
    }
}
#[derive(Clone, Copy)]
struct CloneStroke {
    offset: (f32, f32),
    heal: bool,
}
struct FloatingSelection {
    before: Snapshot,
    before_selection: Option<Selection>,
    source_id: String,
    floating_id: String,
    selection_mask: image::GrayImage,
    initial_floating: Layer,
}

pub struct Editor {
    /// Prefer mutation methods so undo and dirty tracking remain accurate.
    pub document: Document,
    pub active_layer: String,
    pub brush: Brush,
    pub brush_dynamics: Option<crate::brush_dynamics::Settings>,
    pub selection: Option<Selection>,
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
    region_history_enabled: bool,
    detached_raster_bytes: usize,
    history_limit: usize,
    revision: u64,
    next_revision: u64,
    selection_revision: u64,
    saved_revision: u64,
    stroke: Option<Stroke>,
    floating: Option<FloatingSelection>,
}

impl Editor {
    pub fn new(document: Document) -> Self {
        let active_layer = first_paint(&document.layers).unwrap_or_default();
        Self {
            document,
            active_layer,
            brush: Brush::default(),
            brush_dynamics: None,
            selection: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            region_history_enabled: true,
            detached_raster_bytes: 0,
            history_limit: DEFAULT_HISTORY_BYTES,
            revision: 0,
            next_revision: 1,
            selection_revision: 0,
            saved_revision: 0,
            stroke: None,
            floating: None,
        }
    }
    /// Drain the exact bounding rectangle of pixels actually changed by stamps.
    pub fn take_stroke_damage(&mut self) -> Option<StrokeDamage> {
        let stroke = self.stroke.as_mut()?;
        Some(StrokeDamage {
            local_rect: stroke.damage.take()?,
            layer_id: stroke.layer_id.clone(),
            mask_target: stroke.mask_target,
        })
    }

    pub fn selection_revision(&self) -> u64 {
        self.selection_revision
    }
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
            || self.stroke.as_ref().is_some_and(|s| s.changed)
            || self.floating.is_some()
    }
    /// Recovery opens have no on-disk saved revision until an explicit save.
    pub fn mark_unsaved(&mut self) {
        self.saved_revision = u64::MAX;
    }
    pub fn mark_saved(&mut self) {
        self.finish_stroke();
        self.saved_revision = self.revision;
    }
    /// Commit a fully prepared native edit as one undo step. Validation happens
    /// before the live document changes; callers can prepare work off-thread.
    pub fn replace_document_transaction(&mut self, document: Document) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.floating.is_none(),
            "Commit the floating selection first"
        );
        let errors = crate::raster::validate(&document);
        anyhow::ensure!(errors.is_empty(), "Invalid document: {}", errors.join("; "));
        self.finish_stroke();
        let before = self.snapshot();
        let resized =
            (self.document.width, self.document.height) != (document.width, document.height);
        self.document = document;
        if self.document.find_layer(&self.active_layer).is_none() {
            self.active_layer = first_paint(&self.document.layers).unwrap_or_default();
        }
        if resized {
            self.selection = None;
            self.selection_revision = self.selection_revision.wrapping_add(1);
        }
        self.commit(before);
        Ok(())
    }
    pub fn can_undo(&self) -> bool {
        self.floating.is_some()
            || !self.undo.is_empty()
            || self.stroke.as_ref().is_some_and(|s| s.changed)
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.stroke.is_none()
    }
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }
    pub fn history_bytes(&self) -> usize {
        // Charge allocations retained only by history once, not once per snapshot.
        // Current-document pixels already belong to the live document budget.
        let mut seen = std::collections::HashSet::new();
        retained_image_bytes(&self.document.layers, &mut seen);
        self.undo
            .iter()
            .chain(&self.redo)
            .fold(0usize, |total, entry| {
                let snapshot = entry.state();
                total
                    .saturating_add(snapshot.bytes)
                    .saturating_add(entry.extra_bytes())
                    .saturating_add(retained_image_bytes(&snapshot.document.layers, &mut seen))
            })
    }
    /// Compatibility/qualification switch; existing history remains reversible.
    /// Finish the current stroke before changing which history future strokes use.
    pub fn set_region_history_enabled(&mut self, enabled: bool) {
        self.finish_stroke();
        self.region_history_enabled = enabled;
    }
    pub fn history_stats(&self) -> HistoryStats {
        let mut stats = HistoryStats {
            detached_raster_bytes: self.detached_raster_bytes,
            ..Default::default()
        };
        for entry in self.undo.iter().chain(&self.redo) {
            if let HistoryEntry::Raster(patch) = entry {
                let pixels = patch.pixels.borrow();
                stats.raster_patch_entries += 1;
                stats.raster_patch_tiles += pixels.tile_count();
                stats.raster_patch_bytes = stats
                    .raster_patch_bytes
                    .saturating_add(pixels.owned_bytes());
            }
        }
        stats
    }
    /// Snapshots that cannot fit are evicted, including a single oversized one.
    /// The application can use can_undo() to expose that limit honestly.
    pub fn set_history_limit(&mut self, bytes: usize) {
        self.history_limit = bytes;
        self.trim_history();
    }
    pub fn select_layer(&mut self, id: &str) -> bool {
        if self.document.find_layer(id).is_none() {
            return false;
        }
        self.finish_stroke();
        self.active_layer = id.to_owned();
        true
    }
    pub fn undo(&mut self) -> bool {
        self.finish_stroke();
        if self.floating.is_some() {
            return self.cancel_floating_selection();
        }
        if self
            .undo
            .back()
            .is_none_or(|entry| !entry.can_apply(&self.document, self.revision))
        {
            return false;
        }
        let mut previous = self.undo.pop_back().unwrap();
        if !self.apply_history_entry(&mut previous) {
            self.undo.push_back(previous);
            return false;
        }
        self.redo.push_back(previous);
        self.trim_history();
        true
    }
    pub fn redo(&mut self) -> bool {
        self.finish_stroke();
        if self
            .redo
            .back()
            .is_none_or(|entry| !entry.can_apply(&self.document, self.revision))
        {
            return false;
        }
        let mut next = self.redo.pop_back().unwrap();
        if !self.apply_history_entry(&mut next) {
            self.redo.push_back(next);
            return false;
        }
        self.undo.push_back(next);
        self.trim_history();
        true
    }
    fn snapshot(&self) -> Snapshot {
        let selection_bytes = self.selection.as_ref().map_or(0, |s| s.mask.capacity());
        Snapshot {
            document: self.document.clone(),
            active_layer: self.active_layer.clone(),
            selection: self.selection.clone(),
            revision: self.revision,
            bytes: document_bytes(&self.document).saturating_add(selection_bytes),
        }
    }
    fn restore(&mut self, snapshot: Snapshot) {
        self.selection_revision = self.selection_revision.wrapping_add(1);
        self.document = snapshot.document;
        self.active_layer = snapshot.active_layer;
        self.selection = snapshot.selection;
        self.revision = snapshot.revision;
    }
    fn commit(&mut self, before: Snapshot) {
        self.commit_history(HistoryEntry::Snapshot(before));
    }
    fn commit_history(&mut self, mut before: HistoryEntry) {
        if self.floating.is_some() {
            return;
        }
        if let HistoryEntry::Raster(patch) = &mut before {
            patch.expected_revision = self.next_revision;
        }
        self.undo.push_back(before);
        self.redo.clear();
        self.revision = self.next_revision;
        self.next_revision = self.next_revision.saturating_add(1);
        self.trim_history();
    }
    fn trim_history(&mut self) {
        while self.undo.len() + self.redo.len() > MAX_HISTORY_STEPS
            || self.history_bytes() > self.history_limit
        {
            if self.undo.len() >= self.redo.len() && !self.undo.is_empty() {
                self.undo.pop_front();
            } else if !self.redo.is_empty() {
                self.redo.pop_front();
            } else {
                break;
            }
        }
    }
    fn editable_layer(&self) -> Option<&Layer> {
        if locked_in_tree(&self.document.layers, &self.active_layer, false) {
            return None;
        }
        self.document
            .find_layer(&self.active_layer)
            .filter(|l| l.image.is_some() && !is_live_object(l))
    }

    fn valid_dynamics(&self) -> bool {
        self.brush_dynamics
            .as_ref()
            .is_none_or(|settings| settings.validate().is_ok())
    }

    fn valid_input(&self, input: InputPoint) -> bool {
        [input.x, input.y, input.pressure, input.tilt_x, input.tilt_y]
            .iter()
            .all(|value| value.is_finite())
            && self.brush_dynamics.as_ref().is_none_or(|_| {
                (0.0..=1.0).contains(&input.pressure)
                    && (-1.0..=1.0).contains(&input.tilt_x)
                    && (-1.0..=1.0).contains(&input.tilt_y)
            })
    }

    fn new_dynamics(&self, first: InputPoint) -> Option<editor_dynamics::StrokeState> {
        let settings = self.brush_dynamics.clone()?;
        editor_dynamics::StrokeState::new(
            settings,
            editor_dynamics::seed_for_layer(&self.active_layer) ^ self.revision,
            first,
        )
        .ok()
    }

    pub fn begin_stroke(&mut self, x: f32, y: f32, pressure: f32, tool: PaintTool) -> bool {
        self.begin_stylus_stroke(
            InputPoint {
                x,
                y,
                pressure,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
            tool,
        )
    }

    /// Begin a stroke with measured stylus data. Mouse adapters should use
    /// `begin_stroke`, which supplies zero tilt and the adapter's pressure.
    pub fn begin_stylus_stroke(&mut self, input: InputPoint, tool: PaintTool) -> bool {
        if !self.valid_dynamics() || !self.valid_input(input) {
            return false;
        }
        self.finish_stroke();
        if ![
            input.x,
            input.y,
            input.pressure,
            self.brush.size,
            self.brush.opacity,
            self.brush.hardness,
            self.brush.smoothing,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.editable_layer().is_none()
            || self.brush.size <= 0.0
            || !(0.0..=100.0).contains(&self.brush.smoothing)
        {
            return false;
        }
        let before = self.stroke_history();
        let dynamics = self.new_dynamics(input);
        self.stroke = Some(Stroke {
            before,
            last: (input.x, input.y, input.pressure.clamp(0.0, 1.0)),
            tool,
            brush: self.brush.clone(),
            layer_id: self.active_layer.clone(),
            changed: false,
            damage: None,
            clone: None,
            clone_canvas: None,
            mask_target: false,
            coverage: Default::default(),
            overflow: Default::default(),
            smoothing_anchor: (input.x, input.y),
            pointer: (input.x, input.y, input.pressure.clamp(0., 1.)),
            last_input: InputPoint {
                pressure: input.pressure.clamp(0.0, 1.0),
                ..input
            },
            dynamics,
        });
        let dabs = self
            .stroke
            .as_mut()
            .and_then(|stroke| {
                stroke
                    .dynamics
                    .as_mut()
                    .map(editor_dynamics::StrokeState::start)
            })
            .unwrap_or_default();
        if dabs.is_empty() {
            self.stamp(input.x, input.y, input.pressure.clamp(0.0, 1.0));
        } else {
            for dab in dabs {
                self.stamp_dab(dab);
            }
        }
        true
    }
    /// Paint a layer mask without altering source pixels, including independently placed masks.
    pub fn begin_mask_stroke(&mut self, x: f32, y: f32, pressure: f32, tool: PaintTool) -> bool {
        self.begin_stylus_mask_stroke(
            InputPoint {
                x,
                y,
                pressure,
                tilt_x: 0.,
                tilt_y: 0.,
            },
            tool,
        )
    }
    pub fn begin_stylus_mask_stroke(&mut self, input: InputPoint, tool: PaintTool) -> bool {
        if !self.valid_dynamics() || !self.valid_input(input) {
            return false;
        }
        let (x, y, pressure) = (input.x, input.y, input.pressure);
        self.finish_stroke();
        if ![
            x,
            y,
            pressure,
            self.brush.size,
            self.brush.opacity,
            self.brush.hardness,
            self.brush.smoothing,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.brush.size <= 0.
            || !(0.0..=100.0).contains(&self.brush.smoothing)
            || locked_in_tree(&self.document.layers, &self.active_layer, false)
            || self
                .document
                .find_layer(&self.active_layer)
                .is_none_or(|l| l.mask.is_none())
        {
            return false;
        }
        let dynamics = self.new_dynamics(input);
        self.stroke = Some(Stroke {
            before: HistoryEntry::Snapshot(self.snapshot()),
            last: (x, y, pressure.clamp(0., 1.)),
            tool,
            brush: self.brush.clone(),
            layer_id: self.active_layer.clone(),
            changed: false,
            damage: None,
            clone: None,
            clone_canvas: None,
            mask_target: true,
            coverage: Default::default(),
            overflow: Default::default(),
            smoothing_anchor: (x, y),
            pointer: (x, y, pressure.clamp(0., 1.)),
            last_input: input,
            dynamics,
        });
        let dabs = self
            .stroke
            .as_mut()
            .and_then(|stroke| {
                stroke
                    .dynamics
                    .as_mut()
                    .map(editor_dynamics::StrokeState::start)
            })
            .unwrap_or_default();
        if dabs.is_empty() {
            self.stamp(x, y, pressure.clamp(0., 1.));
        } else {
            for dab in dabs {
                self.stamp_dab(dab);
            }
        }
        true
    }
    /// Source pixels are frozen in the single undo snapshot for the entire drag.
    pub fn begin_clone_stroke(
        &mut self,
        source: (f32, f32),
        destination: (f32, f32),
        heal: bool,
    ) -> bool {
        self.begin_clone_stroke_with_source(source, destination, heal, None)
    }
    /// Freeze the complete rendered canvas as the clone/heal source.
    pub fn begin_clone_stroke_from_canvas(
        &mut self,
        source: (f32, f32),
        destination: (f32, f32),
        heal: bool,
    ) -> bool {
        self.finish_stroke();
        if self.editable_layer().is_none() {
            return false;
        }
        let canvas = crate::raster::composite(&self.document);
        self.begin_clone_stroke_with_source(source, destination, heal, Some(canvas))
    }
    fn begin_clone_stroke_with_source(
        &mut self,
        source: (f32, f32),
        destination: (f32, f32),
        heal: bool,
        clone_canvas: Option<RgbaImage>,
    ) -> bool {
        self.finish_stroke();
        if ![
            source.0,
            source.1,
            destination.0,
            destination.1,
            self.brush.size,
            self.brush.opacity,
            self.brush.hardness,
            self.brush.smoothing,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.brush.size <= 0.0
            || !(0.0..=100.0).contains(&self.brush.smoothing)
            || self.editable_layer().is_none()
        {
            return false;
        }
        let offset = (source.0 - destination.0, source.1 - destination.1);
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return false;
        }
        self.stroke = Some(Stroke {
            before: HistoryEntry::Snapshot(self.snapshot()),
            last: (destination.0, destination.1, 1.0),
            tool: PaintTool::Brush,
            brush: self.brush.clone(),
            layer_id: self.active_layer.clone(),
            changed: false,
            damage: None,
            clone: Some(CloneStroke { offset, heal }),
            clone_canvas,
            mask_target: false,
            coverage: Default::default(),
            overflow: Default::default(),
            smoothing_anchor: destination,
            pointer: (destination.0, destination.1, 1.),
            last_input: InputPoint {
                x: destination.0,
                y: destination.1,
                pressure: 1.0,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
            dynamics: None,
        });
        self.stamp(destination.0, destination.1, 1.0);
        true
    }
    pub fn continue_clone_stroke(&mut self, destination: (f32, f32)) -> bool {
        if self.stroke.as_ref().is_none_or(|s| s.clone.is_none()) {
            return false;
        }
        self.continue_stroke(destination.0, destination.1, 1.0)
    }
    pub fn finish_clone_stroke(&mut self) -> bool {
        if self.stroke.as_ref().is_none_or(|s| s.clone.is_none()) {
            return false;
        }
        self.finish_stroke()
    }
    pub fn cancel_clone_stroke(&mut self) {
        if self.stroke.as_ref().is_some_and(|s| s.clone.is_some()) {
            self.cancel_stroke();
        }
    }
    pub fn continue_stroke(&mut self, x: f32, y: f32, pressure: f32) -> bool {
        self.continue_stylus_stroke_at_zoom(
            InputPoint {
                x,
                y,
                pressure,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
            1.0,
        )
    }
    pub fn continue_stylus_stroke(&mut self, input: InputPoint) -> bool {
        self.continue_stylus_stroke_at_zoom(input, 1.0)
    }
    /// Continue with pulled-string smoothing measured in screen points at `zoom`.
    pub fn continue_stroke_at_zoom(&mut self, x: f32, y: f32, pressure: f32, zoom: f32) -> bool {
        self.continue_stylus_stroke_at_zoom(
            InputPoint {
                x,
                y,
                pressure,
                tilt_x: 0.0,
                tilt_y: 0.0,
            },
            zoom,
        )
    }
    pub fn continue_stylus_stroke_at_zoom(&mut self, input: InputPoint, zoom: f32) -> bool {
        if !self.valid_input(input) || !zoom.is_finite() || zoom <= 0. {
            return false;
        }
        let Some(stroke) = self.stroke.as_mut() else {
            return false;
        };
        let pressure = input.pressure.clamp(0., 1.);
        stroke.pointer = (input.x, input.y, pressure);
        let (mut target_x, mut target_y) = (input.x, input.y);
        if stroke.uses_smoothing() {
            let radius = stroke.brush.smoothing / zoom.max(0.01);
            let (dx, dy) = (
                input.x - stroke.smoothing_anchor.0,
                input.y - stroke.smoothing_anchor.1,
            );
            let distance = dx.hypot(dy);
            if distance <= radius {
                return true;
            }
            let step = (distance - radius) / distance;
            target_x = stroke.smoothing_anchor.0 + dx * step;
            target_y = stroke.smoothing_anchor.1 + dy * step;
            stroke.smoothing_anchor = (target_x, target_y);
        }
        self.append_stylus_point(InputPoint {
            x: target_x,
            y: target_y,
            pressure,
            tilt_x: input.tilt_x,
            tilt_y: input.tilt_y,
        })
    }
    fn append_stylus_point(&mut self, input: InputPoint) -> bool {
        let (lx, ly, lp, spacing) = match self.stroke.as_ref() {
            Some(stroke) => (
                stroke.last.0,
                stroke.last.1,
                stroke.last.2,
                (stroke.brush.size.clamp(0.1, 4096.0) * 0.12).max(0.5),
            ),
            None => return false,
        };
        if let Some(dynamics) = self
            .stroke
            .as_mut()
            .and_then(|stroke| stroke.dynamics.as_mut())
        {
            let dabs = match dynamics.append(input) {
                Ok(dabs) => dabs,
                Err(_) => return false,
            };
            for dab in dabs {
                self.stamp_dab(dab);
            }
            let stroke = self.stroke.as_mut().unwrap();
            stroke.last = (input.x, input.y, input.pressure.clamp(0.0, 1.0));
            stroke.last_input = input;
            return true;
        }
        let distance = (input.x - lx).hypot(input.y - ly);
        let steps = (distance / spacing).ceil().clamp(1.0, 8192.0) as u32;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            self.stamp(
                lx + (input.x - lx) * t,
                ly + (input.y - ly) * t,
                lp + (input.pressure - lp) * t,
            );
        }
        self.stroke.as_mut().unwrap().last = (input.x, input.y, input.pressure);
        self.stroke.as_mut().unwrap().last_input = input;
        true
    }
    fn stamp(&mut self, x: f32, y: f32, pressure: f32) {
        self.stamp_with_dab(x, y, pressure, None);
    }
    fn stamp_dab(&mut self, dab: Dab) {
        self.stamp_with_dab(dab.x, dab.y, dab.opacity, Some(dab));
    }
    fn stamp_with_dab(&mut self, x: f32, y: f32, pressure: f32, dynamic_dab: Option<Dab>) {
        let Some(stroke) = &self.stroke else {
            return;
        };
        if pressure <= 0.0 {
            return;
        }
        let brush = &stroke.brush;
        let dynamic_settings = stroke
            .dynamics
            .as_ref()
            .map(|state| state.settings().clone());
        let radius = dynamic_dab
            .map(|dab| (dab.size.max(0.1) / 2.0).max(0.5))
            .unwrap_or_else(|| (brush.size.clamp(0.1, 4096.0) * pressure.max(0.05) / 2.0).max(0.5));
        let opacity = dynamic_dab
            .map(|dab| dab.opacity * brush.opacity.clamp(0.0, 1.0))
            .unwrap_or_else(|| brush.opacity.clamp(0.0, 1.0) * pressure);
        let hardness = if stroke.tool == PaintTool::Pencil {
            1.0
        } else {
            dynamic_dab.map_or(brush.hardness.clamp(0.0, 1.0), |dab| dab.hardness)
        };
        let Some(transform) = (if stroke.mask_target {
            Transform::for_mask(&self.document, &stroke.layer_id)
        } else {
            Transform::for_layer(&self.document, &stroke.layer_id)
        }) else {
            return;
        };
        let Some(layer) = self.document.find_layer_mut(&stroke.layer_id) else {
            return;
        };
        let Some((cx, cy)) = transform.local(x, y) else {
            return;
        };
        let local_radius = transform.inverse_radius(radius);
        let Some(image) = (if stroke.mask_target {
            layer.mask.as_mut()
        } else {
            layer.image.as_mut()
        }) else {
            return;
        };
        let left = ((cx - local_radius).floor() as i64).clamp(0, image.width() as i64) as u32;
        let right = ((cx + local_radius).ceil() as i64).clamp(0, image.width() as i64) as u32;
        let top = ((cy - local_radius).floor() as i64).clamp(0, image.height() as i64) as u32;
        let bottom = ((cy + local_radius).ceil() as i64).clamp(0, image.height() as i64) as u32;
        let frozen = stroke
            .clone
            .and_then(|_| stroke.before.state().document.find_layer(&stroke.layer_id))
            .and_then(|layer| layer.image.as_ref());
        let sample = |wx: f32, wy: f32| -> Option<[u8; 4]> {
            let clone = stroke.clone?;
            if let Some(canvas) = &stroke.clone_canvas {
                let (sx, sy) = (wx + clone.offset.0, wy + clone.offset.1);
                if sx < 0.0
                    || sy < 0.0
                    || sx >= canvas.width() as f32
                    || sy >= canvas.height() as f32
                {
                    return None;
                }
                return Some(canvas.get_pixel(sx.floor() as u32, sy.floor() as u32).0);
            }
            let frozen = frozen?;
            let (sx, sy) = transform.local(wx + clone.offset.0, wy + clone.offset.1)?;
            if sx < 0.0 || sy < 0.0 || sx >= frozen.width() as f32 || sy >= frozen.height() as f32 {
                None
            } else {
                Some(frozen.get_pixel(sx as u32, sy as u32).0)
            }
        };
        let mut tone = [0.0; 3];
        if stroke.clone.is_some_and(|clone| clone.heal) {
            let mut count = 0.0;
            for py in top..bottom {
                for px in left..right {
                    let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
                    if (wx - x).hypot(wy - y) > radius {
                        continue;
                    }
                    let Some(source) = sample(wx, wy) else {
                        continue;
                    };
                    let target = if let Some(canvas) = &stroke.clone_canvas {
                        let (tx, ty) = (wx.floor() as i64, wy.floor() as i64);
                        if tx < 0
                            || ty < 0
                            || tx >= canvas.width() as i64
                            || ty >= canvas.height() as i64
                        {
                            continue;
                        }
                        canvas.get_pixel(tx as u32, ty as u32).0
                    } else {
                        let Some(layer) = frozen else {
                            continue;
                        };
                        if px >= layer.width() || py >= layer.height() {
                            continue;
                        }
                        layer.get_pixel(px, py).0
                    };
                    if source[3] == 0 || target[3] == 0 {
                        continue;
                    }
                    for c in 0..3 {
                        tone[c] += target[c] as f32 - source[c] as f32;
                    }
                    count += 1.0;
                }
            }
            if count > 0.0 {
                for value in &mut tone {
                    *value /= count;
                }
            }
        }
        let mut damage_bounds = (image.width(), image.height(), 0, 0);
        let mut changed = false;
        let image_width = image.width() as usize;
        let original = stroke
            .before
            .original_image(&stroke.layer_id, stroke.mask_target);
        let initial_allocation = image.allocation_id();
        let detached_bytes = image.as_raw().len();
        for py in top..bottom {
            for px in left..right {
                let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
                if !selected(&self.selection, wx, wy) {
                    continue;
                }
                let distance = (wx - x).hypot(wy - y) / radius;
                if distance > 1.0 {
                    continue;
                }
                let coverage = selection_coverage(&self.selection, wx, wy)
                    * if let (Some(settings), Some(dab)) = (&dynamic_settings, dynamic_dab) {
                        if dab.opacity <= 0.0 {
                            0.0
                        } else {
                            (editor_dynamics::sample_dab(
                                settings,
                                &dab,
                                (wx - x) / radius,
                                (wy - y) / radius,
                            ) / dab.opacity.max(0.000_001))
                            .clamp(0.0, 1.0)
                        }
                    } else if distance <= hardness || hardness >= 1.0 {
                        1.0
                    } else {
                        (1.0 - distance) / (1.0 - hardness)
                    };
                // SharedImage detaches on mutable access, so compute first.
                // A transparent/no-op stamp must keep the snapshot allocation.
                let old = *image.get_pixel(px, py);
                let Some(base) = stroke.before.original_pixel(original, image, px, py) else {
                    stroke.overflow.set(true);
                    continue;
                };
                let mut pixel = old;
                if stroke.mask_target {
                    let target = if stroke.tool == PaintTool::Eraser {
                        0.
                    } else {
                        brush.color[0] as f32
                    };
                    let amount = stroke_coverage(
                        stroke,
                        py as usize * image_width + px as usize,
                        opacity * coverage * brush.color[3] as f32 / 255.,
                    );
                    if stroke.overflow.get() {
                        continue;
                    }
                    let old_gray = base[0] as f32 * base[3] as f32 / 255.;
                    let gray = (old_gray + (target - old_gray) * amount)
                        .round()
                        .clamp(0., 255.) as u8;
                    pixel.0 = [gray, gray, gray, 255];
                } else if stroke.clone.is_some() {
                    let Some(mut color) = sample(wx, wy) else {
                        continue;
                    };
                    for c in 0..3 {
                        color[c] = (color[c] as f32 + tone[c]).round().clamp(0.0, 255.0) as u8;
                    }
                    let amount = stroke_coverage(
                        stroke,
                        py as usize * image_width + px as usize,
                        opacity * coverage,
                    );
                    if stroke.overflow.get() {
                        continue;
                    }
                    pixel.0 = over(base.0, color, amount);
                } else if stroke.tool == PaintTool::Eraser {
                    let amount = stroke_coverage(
                        stroke,
                        py as usize * image_width + px as usize,
                        opacity * coverage,
                    );
                    if stroke.overflow.get() {
                        continue;
                    }
                    pixel.0 = base.0;
                    pixel.0[3] = (pixel.0[3] as f32 * (1.0 - amount)).round() as u8;
                    if pixel.0[3] == 0 {
                        pixel.0 = [0; 4];
                    }
                } else {
                    let amount = stroke_coverage(
                        stroke,
                        py as usize * image_width + px as usize,
                        opacity * coverage,
                    );
                    if stroke.overflow.get() {
                        continue;
                    }
                    pixel.0 = over(base.0, brush.color, amount);
                }
                if pixel != old {
                    if !stroke.before.capture(image, px, py) {
                        stroke.overflow.set(true);
                        continue;
                    }
                    image.put_pixel(px, py, pixel);
                    changed = true;
                    damage_bounds.0 = damage_bounds.0.min(px);
                    damage_bounds.1 = damage_bounds.1.min(py);
                    damage_bounds.2 = damage_bounds.2.max(px + 1);
                    damage_bounds.3 = damage_bounds.3.max(py + 1);
                }
            }
        }
        if image.allocation_id() != initial_allocation {
            self.detached_raster_bytes = self.detached_raster_bytes.saturating_add(detached_bytes);
        }
        if changed {
            let stroke = self.stroke.as_mut().unwrap();
            stroke.changed = true;
            let (mut left, mut top, mut right, mut bottom) = damage_bounds;
            if let Some(previous) = stroke.damage {
                left = left.min(previous.x);
                top = top.min(previous.y);
                right = right.max(previous.x + previous.width);
                bottom = bottom.max(previous.y + previous.height);
            }
            stroke.damage = Some(PixelRect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            });
        }
    }
    pub fn finish_stroke(&mut self) -> bool {
        if let Some(stroke) = &self.stroke
            && stroke.uses_smoothing()
            && (stroke.pointer.0 != stroke.smoothing_anchor.0
                || stroke.pointer.1 != stroke.smoothing_anchor.1)
        {
            let pointer = stroke.pointer;
            let last_input = stroke.last_input;
            self.append_stylus_point(InputPoint {
                x: pointer.0,
                y: pointer.1,
                pressure: pointer.2,
                tilt_x: last_input.tilt_x,
                tilt_y: last_input.tilt_y,
            });
        }
        let dabs = self
            .stroke
            .as_mut()
            .and_then(|stroke| {
                stroke
                    .dynamics
                    .as_mut()
                    .map(editor_dynamics::StrokeState::finish)
            })
            .unwrap_or_default();
        for dab in dabs {
            self.stamp_dab(dab);
        }
        let Some(mut stroke) = self.stroke.take() else {
            return false;
        };
        if stroke.overflow.get() {
            self.apply_history_entry(&mut stroke.before);
            return false;
        }
        if stroke.changed {
            self.commit_history(stroke.before);
        }
        stroke.changed
    }
    pub fn cancel_stroke(&mut self) {
        if let Some(mut stroke) = self.stroke.take() {
            self.apply_history_entry(&mut stroke.before);
        }
    }

    pub fn add_layer(&mut self, name: &str) -> String {
        if self.floating.is_some()
            || tree_count(&self.document.layers) >= crate::model::MAX_LAYERS
            || tree_pixels(&self.document.layers).saturating_add(
                u64::from(self.document.width).saturating_mul(u64::from(self.document.height)),
            ) > crate::model::MAX_PIXELS
        {
            return String::new();
        }
        self.finish_stroke();
        let before = self.snapshot();
        let layer = Layer::paint(name, self.document.width, self.document.height);
        let id = layer.id.clone();
        self.document.layers.push(layer);
        self.active_layer = id.clone();
        self.commit(before);
        id
    }
    pub fn add_group(&mut self, name: &str) -> String {
        if self.floating.is_some() || tree_count(&self.document.layers) >= crate::model::MAX_LAYERS
        {
            return String::new();
        }
        self.finish_stroke();
        let before = self.snapshot();
        let layer = Layer::group(name);
        let id = layer.id.clone();
        self.document.layers.push(layer);
        self.active_layer = id.clone();
        self.commit(before);
        id
    }
    pub fn import_layer(&mut self, layer: Layer) -> String {
        self.insert_layer(layer)
    }
    pub fn insert_layer(&mut self, layer: Layer) -> String {
        let mut layer = layer;
        regenerate_ids(&mut layer);
        let added_layers = 1 + tree_count(&layer.children);
        if self.floating.is_some()
            || !valid_insert_tree(&layer)
            || tree_count(&self.document.layers).saturating_add(added_layers)
                > crate::model::MAX_LAYERS
            || tree_pixels(&self.document.layers).saturating_add(layer_pixels(&layer))
                > crate::model::MAX_PIXELS
        {
            return String::new();
        }
        // Validate the proposed whole tree before ending an active stroke or
        // taking an undo snapshot. This keeps a rejected import fully inert.
        let mut proposed = self.document.clone();
        proposed.layers.push(layer.clone());
        if !crate::raster::validate(&proposed).is_empty() {
            return String::new();
        }
        self.finish_stroke();
        let before = self.snapshot();
        let id = layer.id.clone();
        self.document.layers.push(layer);
        self.active_layer = id.clone();
        self.commit(before);
        id
    }
    pub fn delete_layer(&mut self, id: &str) -> bool {
        self.finish_stroke();
        if self.document.find_layer(id).is_none()
            || locked_in_tree(&self.document.layers, id, false)
        {
            return false;
        }
        let before = self.snapshot();
        remove_layer(&mut self.document.layers, id);
        if self.document.layers.is_empty() {
            self.document.layers.push(Layer::paint(
                "Layer 1",
                self.document.width,
                self.document.height,
            ));
        }
        if self.document.find_layer(&self.active_layer).is_none() {
            self.active_layer = first_paint(&self.document.layers)
                .unwrap_or_else(|| self.document.layers[0].id.clone());
        }
        self.commit(before);
        true
    }
    pub fn duplicate_layer(&mut self, id: &str) -> Option<String> {
        self.finish_stroke();
        let mut copy = self.document.find_layer(id)?.clone();
        let before = self.snapshot();
        regenerate_ids(&mut copy);
        copy.name.push_str(" copy");
        let new_id = copy.id.clone();
        insert_after(&mut self.document.layers, id, copy);
        self.active_layer = new_id.clone();
        self.commit(before);
        Some(new_id)
    }
    /// Canonical selected roots in bottom-to-top hierarchy order; selected folders carry descendants.
    pub fn selected_layer_roots(&self, ids: &[String]) -> Vec<String> {
        selected_roots(&self.document.layers, ids).unwrap_or_default()
    }
    /// Wrap selected roots at their nearest common parent, at the highest selected branch.
    pub fn group_layers(&mut self, ids: &[String], name: &str) -> Option<String> {
        self.finish_stroke();
        if self.floating.is_some() {
            return None;
        }
        let roots = selected_roots(&self.document.layers, ids)?;
        if roots.is_empty()
            || tree_count(&self.document.layers) >= crate::model::MAX_LAYERS
            || !roots_editable(&self.document.layers, &roots)
        {
            return None;
        }
        let paths: Vec<_> = roots
            .iter()
            .map(|id| tree_path(&self.document.layers, id).unwrap())
            .collect();
        let mut parent_depth = 0;
        while paths.iter().all(|path| path.len() > parent_depth + 1)
            && paths
                .iter()
                .all(|path| path[parent_depth] == paths[0][parent_depth])
        {
            parent_depth += 1;
        }
        let parent = parent_depth.checked_sub(1).map(|i| paths[0][i].clone());
        if roots
            .iter()
            .any(|id| parent_depth + 1 + tree_depth(self.document.find_layer(id).unwrap()) > 64)
        {
            return None;
        }
        let name = name.trim();
        if name.len() > 1024 {
            return None;
        }
        let name = if name.is_empty() {
            fn named(layers: &[Layer], name: &str) -> bool {
                layers
                    .iter()
                    .any(|l| l.name == name || named(&l.children, name))
            }
            let mut n = 1;
            while named(&self.document.layers, &format!("Folder {n}")) {
                n += 1;
            }
            format!("Folder {n}")
        } else {
            name.to_owned()
        };
        let siblings = parent.as_deref().map_or(&self.document.layers[..], |id| {
            &self.document.find_layer(id).unwrap().children
        });
        let branches: std::collections::HashSet<_> = paths
            .iter()
            .map(|path| path[parent_depth].as_str())
            .collect();
        let highest = siblings
            .iter()
            .rposition(|layer| branches.contains(layer.id.as_str()))?;
        let insertion = siblings[..=highest]
            .iter()
            .filter(|layer| !roots.contains(&layer.id))
            .count();
        let before = self.snapshot();
        let mut group = Layer::group(name);
        for id in &roots {
            group
                .children
                .push(remove_layer(&mut self.document.layers, id).unwrap());
        }
        let id = group.id.clone();
        let siblings = target_siblings_mut(&mut self.document.layers, parent.as_deref()).unwrap();
        siblings.insert(insertion.min(siblings.len()), group);
        self.active_layer = id.clone();
        self.commit(before);
        Some(id)
    }
    /// Duplicate each selected tree immediately above itself as one transaction.
    pub fn duplicate_layers(&mut self, ids: &[String]) -> Vec<String> {
        self.duplicate_layer_batch(ids, None)
    }
    /// Duplicate selected roots into a group/root at a bottom-to-top insertion index.
    pub fn duplicate_layers_to(
        &mut self,
        ids: &[String],
        parent: Option<&str>,
        index: usize,
    ) -> Vec<String> {
        self.duplicate_layer_batch(ids, Some((parent, index)))
    }
    fn duplicate_layer_batch(
        &mut self,
        ids: &[String],
        destination: Option<(Option<&str>, usize)>,
    ) -> Vec<String> {
        self.finish_stroke();
        if self.floating.is_some() {
            return Vec::new();
        }
        let Some(roots) = selected_roots(&self.document.layers, ids) else {
            return Vec::new();
        };
        if roots.is_empty() || !roots_editable(&self.document.layers, &roots) {
            return Vec::new();
        }
        let added: usize = roots
            .iter()
            .map(|id| 1 + tree_count(&self.document.find_layer(id).unwrap().children))
            .sum();
        let added_pixels: u64 = roots
            .iter()
            .map(|id| layer_pixels(self.document.find_layer(id).unwrap()))
            .sum();
        if tree_pixels(&self.document.layers).saturating_add(added_pixels)
            > crate::model::MAX_PIXELS
        {
            return Vec::new();
        }
        if tree_count(&self.document.layers).saturating_add(added) > crate::model::MAX_LAYERS {
            return Vec::new();
        }
        if let Some((parent, _)) = destination {
            // Match the Mac drag contract: reject the original folder and its
            // descendants even though a copied tree would have fresh IDs.
            if parent.is_some_and(|parent| {
                roots.iter().any(|id| {
                    parent == id
                        || find(&self.document.find_layer(id).unwrap().children, parent).is_some()
                })
            }) {
                return Vec::new();
            }
            let parent_depth = match valid_parent(&self.document.layers, parent) {
                Some(depth) => depth,
                None => return Vec::new(),
            };
            if roots
                .iter()
                .any(|id| parent_depth + tree_depth(self.document.find_layer(id).unwrap()) > 64)
            {
                return Vec::new();
            }
        }
        let mut copies: Vec<Layer> = roots
            .iter()
            .map(|id| self.document.find_layer(id).unwrap().clone())
            .collect();
        // One ID map spans the entire batch, including links between separately selected trees.
        let mapping = regenerate_forest_ids(&mut copies);
        let mut clipping = contiguous_clips(&self.document.layers);
        let copied_clipping: Vec<_> = clipping
            .iter()
            .filter_map(|id| mapping.get(id).cloned())
            .collect();
        clipping.extend(copied_clipping);
        for copy in &mut copies {
            copy.name.push_str(" copy");
        }
        let copied_ids: Vec<String> = copies.iter().map(|layer| layer.id.clone()).collect();
        let before = self.snapshot();
        if let Some((parent, index)) = destination {
            let target = target_siblings_mut(&mut self.document.layers, parent).unwrap();
            let index = index.min(target.len());
            target.splice(index..index, copies);
        } else {
            for (id, copy) in roots.iter().zip(copies) {
                insert_after(&mut self.document.layers, id, copy);
            }
        }
        if destination.is_some()
            && (reconcile_drop_clipping(&mut self.document.layers, &copied_ids, clipping).is_err()
                || !crate::raster::validate(&self.document).is_empty())
        {
            self.restore(before);
            return Vec::new();
        }
        self.active_layer = copied_ids.last().unwrap().clone();
        self.commit(before);
        copied_ids
    }
    /// Delete selected trees atomically. External live-mask links require an explicit unlink choice.
    pub fn delete_layers(&mut self, ids: &[String]) -> bool {
        self.delete_layers_with_unlink(ids, false)
    }
    pub fn delete_layers_with_unlink(&mut self, ids: &[String], unlink_live_masks: bool) -> bool {
        self.finish_stroke();
        if self.floating.is_some() {
            return false;
        }
        let Some(roots) = selected_roots(&self.document.layers, ids) else {
            return false;
        };
        if roots.is_empty() || !roots_editable(&self.document.layers, &roots) {
            return false;
        }
        let removed = descendant_set(&self.document.layers, &roots);
        fn dependents(
            layers: &[Layer],
            removed: &std::collections::HashSet<String>,
            output: &mut Vec<String>,
        ) {
            for layer in layers {
                if !removed.contains(&layer.id)
                    && layer
                        .metadata
                        .get("maskSourceID")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|source| removed.contains(source))
                {
                    output.push(layer.id.clone());
                }
                dependents(&layer.children, removed, output);
            }
        }
        let mut linked = Vec::new();
        dependents(&self.document.layers, &removed, &mut linked);
        if !linked.is_empty()
            && (!unlink_live_masks
                || linked
                    .iter()
                    .any(|id| locked_in_tree(&self.document.layers, id, false)))
        {
            return false;
        }
        let before = self.snapshot();
        for id in &roots {
            remove_layer(&mut self.document.layers, id);
        }
        for id in linked {
            remove_metadata(self.document.find_layer_mut(&id).unwrap(), "maskSourceID");
        }
        if self.document.layers.is_empty() {
            self.document.layers.push(Layer::paint(
                "Layer 1",
                self.document.width,
                self.document.height,
            ));
        }
        if self.document.find_layer(&self.active_layer).is_none() {
            self.active_layer = first_paint(&self.document.layers)
                .unwrap_or_else(|| self.document.layers[0].id.clone());
        }
        self.commit(before);
        true
    }
    /// Move selected roots as a contiguous block. Index is measured AFTER their removal.
    pub fn reparent_layers(&mut self, ids: &[String], parent: Option<&str>, index: usize) -> bool {
        self.finish_stroke();
        if self.floating.is_some() {
            return false;
        }
        let Some(roots) = selected_roots(&self.document.layers, ids) else {
            return false;
        };
        if roots.is_empty() || !roots_editable(&self.document.layers, &roots) {
            return false;
        }
        let Some(parent_depth) = valid_parent(&self.document.layers, parent) else {
            return false;
        };
        for id in &roots {
            let layer = self.document.find_layer(id).unwrap();
            if parent.is_some_and(|parent| parent == id || find(&layer.children, parent).is_some())
                || parent_depth + tree_depth(layer) > 64
            {
                return false;
            }
        }
        let before = self.snapshot();
        let old_order = hierarchy_signature(&self.document.layers);
        let clipping = contiguous_clips(&self.document.layers);
        let moving: Vec<_> = roots
            .iter()
            .map(|id| remove_layer(&mut self.document.layers, id).unwrap())
            .collect();
        let target = target_siblings_mut(&mut self.document.layers, parent).unwrap();
        let index = index.min(target.len());
        target.splice(index..index, moving);
        let clipping_changed =
            match reconcile_drop_clipping(&mut self.document.layers, &roots, clipping) {
                Ok(changed) => changed,
                Err(()) => {
                    self.restore(before);
                    return false;
                }
            };
        // Adoption must not introduce a cycle through an arbitrary live-mask link.
        if !crate::raster::validate(&self.document).is_empty() {
            self.restore(before);
            return false;
        }
        if hierarchy_signature(&self.document.layers) == old_order && !clipping_changed {
            self.restore(before);
            return false;
        }
        self.active_layer = roots.last().unwrap().clone();
        self.commit(before);
        true
    }
    /// UI drop helper: -1 below, +1 above, 0 inside target group. None means root top.
    /// Returns selected roots/copies on success, empty on rejected/no-op drops.
    pub fn drop_layers(
        &mut self,
        ids: &[String],
        target: Option<&str>,
        placement: i8,
        copy: bool,
    ) -> Vec<String> {
        if !(-1..=1).contains(&placement) {
            return Vec::new();
        }
        let Some(roots) = selected_roots(&self.document.layers, ids) else {
            return Vec::new();
        };
        if roots.is_empty() {
            return Vec::new();
        }
        let (parent, mut index) = if let Some(target) = target {
            let Some(layer) = self.document.find_layer(target) else {
                return Vec::new();
            };
            if placement == 0 {
                if !layer.is_group() {
                    return Vec::new();
                }
                (Some(target.to_owned()), layer.children.len())
            } else {
                if !copy && roots.iter().any(|id| id == target) {
                    return Vec::new();
                }
                let path = tree_path(&self.document.layers, target).unwrap();
                let parent = path.len().checked_sub(2).map(|i| path[i].clone());
                let siblings = parent.as_deref().map_or(&self.document.layers[..], |id| {
                    &self.document.find_layer(id).unwrap().children
                });
                (
                    parent,
                    siblings.iter().position(|l| l.id == target).unwrap()
                        + usize::from(placement > 0),
                )
            }
        } else {
            (None, self.document.layers.len())
        };
        if !copy {
            let siblings = parent.as_deref().map_or(&self.document.layers[..], |id| {
                &self.document.find_layer(id).unwrap().children
            });
            index -= siblings[..index.min(siblings.len())]
                .iter()
                .filter(|l| roots.contains(&l.id))
                .count();
        }
        if copy {
            self.duplicate_layers_to(&roots, parent.as_deref(), index)
        } else if self.reparent_layers(&roots, parent.as_deref(), index) {
            roots
        } else {
            Vec::new()
        }
    }
    /// Translate selected trees once, including linked masks but leaving detached leaf masks fixed.
    pub fn move_layers(&mut self, ids: &[String], dx: f32, dy: f32) -> bool {
        self.finish_stroke();
        if self.floating.is_some() || !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0)
        {
            return false;
        }
        let Some(roots) = selected_roots(&self.document.layers, ids) else {
            return false;
        };
        if roots.is_empty() || !roots_editable(&self.document.layers, &roots) {
            return false;
        }
        fn valid(layer: &Layer, dx: f32, dy: f32) -> bool {
            let x = layer.offset_x + dx;
            let y = layer.offset_y + dy;
            x.is_finite()
                && y.is_finite()
                && x.abs() <= 1_000_000.0
                && y.abs() <= 1_000_000.0
                && layer.children.iter().all(|l| valid(l, dx, dy))
        }
        if roots
            .iter()
            .any(|id| !valid(self.document.find_layer(id).unwrap(), dx, dy))
        {
            return false;
        }
        let before = self.snapshot();
        fn shift(layer: &mut Layer, dx: f32, dy: f32, dw: u32, dh: u32) {
            let old = placement_of(layer, dw, dh);
            layer.offset_x += dx;
            layer.offset_y += dy;
            let new = placement_of(layer, dw, dh);
            if let (Some(old), Some(new)) = (old, new) {
                carry_mask_placement(layer, old, new);
            }
            for child in &mut layer.children {
                shift(child, dx, dy, dw, dh);
            }
        }
        let (dw, dh) = (self.document.width, self.document.height);
        for id in &roots {
            shift(self.document.find_layer_mut(id).unwrap(), dx, dy, dw, dh);
        }
        self.commit(before);
        true
    }
    /// Copy/move an editable bitmap mask in document space. Clipping links stay on their original layers.
    pub fn copy_layer_mask(
        &mut self,
        source: &str,
        target: &str,
        copy: bool,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        if self.floating.is_some()
            || source == target
            || locked_in_tree(&self.document.layers, target, false)
            || (!copy && locked_in_tree(&self.document.layers, source, false))
        {
            return Ok(false);
        }
        let from = self
            .document
            .find_layer(source)
            .ok_or_else(|| anyhow::anyhow!("Mask source layer not found"))?;
        let Some(mask) = &from.mask else {
            return Ok(false);
        };
        let to = self
            .document
            .find_layer(target)
            .ok_or_else(|| anyhow::anyhow!("Mask target layer not found"))?;
        if to.is_group() {
            return Ok(false);
        }
        crate::effects::validate_mask_metadata(&from.metadata)?;
        let folder_mask = from.is_group()
            || from
                .metadata
                .get("adjustment")
                .is_some_and(|value| !value.is_null());
        let explicit = (!folder_mask)
            .then(|| from.metadata.get("maskPlacement"))
            .flatten()
            .filter(|value| !value.is_null());
        let placement = if folder_mask {
            // Folder/adjustment masks follow their transform extent, which may
            // differ from both document and mask bitmap dimensions.
            let size = from
                .metadata
                .get("transform")
                .and_then(|v| v.get("size"))
                .and_then(serde_json::Value::as_array);
            let width = size
                .and_then(|v| v.first())
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(f64::from(mask.width())) as f32;
            let height = size
                .and_then(|v| v.get(1))
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(f64::from(mask.height())) as f32;
            LayerPlacement {
                x: from.offset_x,
                y: from.offset_y,
                width: width * from.scale_x.abs(),
                height: height * from.scale_y.abs(),
                rotation: from.rotation,
                flip_x: from.scale_x < 0.0,
                flip_y: from.scale_y < 0.0,
            }
        } else {
            self.mask_placement(source)
                .ok_or_else(|| anyhow::anyhow!("Invalid source mask placement"))?
        };
        anyhow::ensure!(placement.is_valid(), "Invalid source mask placement");
        let sampling = explicit
            .and_then(|v| v.get("sampling"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                from.metadata
                    .get("transform")
                    .and_then(|v| v.get("sampling"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("High quality");
        let mask = mask.clone();
        let mut metadata = if to.metadata.is_object() {
            to.metadata.clone()
        } else {
            serde_json::json!({})
        };
        for key in ["maskEnabled", "maskLinked"] {
            if let Some(value) = from.metadata.get(key) {
                metadata[key] = value.clone();
            } else {
                metadata.as_object_mut().unwrap().remove(key);
            }
        }
        // Preserve explicit placement fields (including sampling); materialize implicit placement.
        if let Some(value) = explicit {
            metadata["maskPlacement"] = value.clone();
        } else {
            metadata["maskPlacement"] = serde_json::json!({"origin":[placement.x,placement.y],"size":[placement.width,placement.height],"rotation":placement.rotation,"flipX":placement.flip_x,"flipY":placement.flip_y});
        }
        metadata["maskPlacement"]["sampling"] = serde_json::json!(sampling);
        // Image-less adjustments sample masks through their layer transform,
        // unlike paint layers which use the independent maskPlacement above.
        // Canonicalize both representations so editing and save/load retain
        // the same document-space coverage without replacing other metadata.
        let adjustment_target = to.image.is_none()
            && to
                .metadata
                .get("adjustment")
                .is_some_and(|value| !value.is_null());
        let scale_x = if placement.flip_x { -1.0 } else { 1.0 };
        let scale_y = if placement.flip_y { -1.0 } else { 1.0 };
        if adjustment_target {
            if !metadata["transform"].is_object() {
                metadata["transform"] = serde_json::json!({});
            }
            for key in ["origin", "size", "rotation", "flipX", "flipY", "sampling"] {
                metadata["transform"][key] = metadata["maskPlacement"][key].clone();
            }
        }
        let transform_unchanged = !adjustment_target
            || (to.offset_x == placement.x
                && to.offset_y == placement.y
                && to.rotation == placement.rotation
                && to.scale_x == scale_x
                && to.scale_y == scale_y);
        crate::effects::validate_mask_metadata(&metadata)?;
        let added = u64::from(mask.width()) * u64::from(mask.height());
        let removed = to
            .mask
            .as_ref()
            .map_or(0, |i| u64::from(i.width()) * u64::from(i.height()));
        anyhow::ensure!(
            !copy
                || tree_pixels(&self.document.layers)
                    .saturating_sub(removed)
                    .saturating_add(added)
                    <= crate::model::MAX_PIXELS,
            "Copied mask exceeds project image budget"
        );
        if copy && to.mask.as_ref() == Some(&mask) && to.metadata == metadata && transform_unchanged
        {
            return Ok(false);
        }
        let before = self.snapshot();
        let to = self.document.find_layer_mut(target).unwrap();
        to.mask = Some(mask.into());
        to.metadata = metadata;
        if adjustment_target {
            to.offset_x = placement.x;
            to.offset_y = placement.y;
            to.rotation = placement.rotation;
            to.scale_x = scale_x;
            to.scale_y = scale_y;
        }
        if !copy {
            let from = self.document.find_layer_mut(source).unwrap();
            from.mask = None;
            for key in ["maskEnabled", "maskLinked", "maskPlacement"] {
                remove_metadata(from, key);
            }
        }
        self.active_layer = target.to_owned();
        self.commit(before);
        Ok(true)
    }
    /// Copy/move an entire layer-effects stack; other destination metadata is preserved.
    pub fn copy_layer_effects(
        &mut self,
        source: &str,
        target: &str,
        copy: bool,
    ) -> anyhow::Result<bool> {
        self.transfer_effects(source, target, None, copy)
    }
    /// Copy/move one effect row while preserving all other destination effects.
    pub fn copy_layer_effect(
        &mut self,
        source: &str,
        target: &str,
        kind: &str,
        copy: bool,
    ) -> anyhow::Result<bool> {
        anyhow::ensure!(
            [
                "stroke",
                "shadow",
                "colorOverlay",
                "innerShadow",
                "outerGlow",
                "innerGlow"
            ]
            .contains(&kind),
            "Unknown layer effect kind"
        );
        self.transfer_effects(source, target, Some(kind), copy)
    }
    fn transfer_effects(
        &mut self,
        source: &str,
        target: &str,
        kind: Option<&str>,
        copy: bool,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        if self.floating.is_some()
            || source == target
            || locked_in_tree(&self.document.layers, target, false)
            || (!copy && locked_in_tree(&self.document.layers, source, false))
        {
            return Ok(false);
        }
        let from = self
            .document
            .find_layer(source)
            .ok_or_else(|| anyhow::anyhow!("Effect source layer not found"))?;
        let Some(original) = from.metadata.get("effects").filter(|v| !v.is_null()) else {
            return Ok(false);
        };
        crate::effects::LayerEffects::parse(original)?;
        let to = self
            .document
            .find_layer(target)
            .ok_or_else(|| anyhow::anyhow!("Effect target layer not found"))?;
        if to.is_group() || to.image.is_none() {
            return Ok(false);
        }
        let effects = if let Some(kind) = kind {
            let Some(effect) = original.get(kind).filter(|v| !v.is_null()) else {
                return Ok(false);
            };
            let mut effects = to
                .metadata
                .get("effects")
                .filter(|v| v.is_object())
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            effects[kind] = effect.clone();
            effects
        } else {
            original.clone()
        };
        crate::effects::LayerEffects::parse(&effects)?;
        if copy && to.metadata.get("effects") == Some(&effects) {
            return Ok(false);
        }
        let before = self.snapshot();
        let to = self.document.find_layer_mut(target).unwrap();
        if !to.metadata.is_object() {
            to.metadata = serde_json::json!({});
        }
        to.metadata["effects"] = effects;
        if !copy {
            let from = self.document.find_layer_mut(source).unwrap();
            if let Some(kind) = kind {
                if let Some(effects) = from
                    .metadata
                    .get_mut("effects")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    effects.remove(kind);
                }
            } else {
                remove_metadata(from, "effects");
            }
        }
        self.active_layer = target.to_owned();
        self.commit(before);
        Ok(true)
    }
    pub fn rename_layer(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.len() > 1024 {
            return false;
        }
        self.change_layer(id, |layer| {
            if layer.name == name {
                false
            } else {
                layer.name = name.to_owned();
                true
            }
        })
    }
    pub fn set_visibility(&mut self, id: &str, visible: bool) -> bool {
        self.change_layer(id, |layer| {
            let changed = layer.visible != visible;
            layer.visible = visible;
            changed
        })
    }
    pub fn set_locked(&mut self, id: &str, locked: bool) -> bool {
        self.change_layer(id, |layer| {
            let changed = layer.locked != locked;
            layer.locked = locked;
            changed
        })
    }
    /// Change interpolation without baking source pixels; one undo step.
    pub fn set_sampling(&mut self, id: &str, sampling: &str, mask: bool) -> anyhow::Result<bool> {
        anyhow::ensure!(
            matches!(sampling, "Nearest" | "Smooth" | "High quality"),
            "Invalid sampling mode"
        );
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        if mask
            && self
                .document
                .find_layer(id)
                .is_none_or(|layer| layer.mask.is_none())
        {
            return Ok(false);
        }
        let independent = mask
            && self.document.find_layer(id).is_some_and(|layer| {
                !layer.is_group()
                    && !layer
                        .metadata
                        .get("adjustment")
                        .is_some_and(|v| !v.is_null())
            });
        let placement = if independent {
            self.mask_placement(id)
        } else {
            None
        };
        Ok(self.change_layer(id, |layer| {
            if independent {
                let Some(placement) = placement else {
                    return false;
                };
                if layer
                    .metadata
                    .pointer("/maskPlacement/sampling")
                    .and_then(|v| v.as_str())
                    == Some(sampling)
                {
                    return false;
                }
                if !layer
                    .metadata
                    .get("maskPlacement")
                    .is_some_and(|v| v.is_object())
                {
                    set_metadata_placement(layer, "maskPlacement", placement);
                }
                layer.metadata["maskPlacement"]["sampling"] = sampling.into();
            } else {
                if layer
                    .metadata
                    .pointer("/transform/sampling")
                    .and_then(|v| v.as_str())
                    .unwrap_or("High quality")
                    == sampling
                {
                    return false;
                }
                if !layer.metadata.is_object() {
                    layer.metadata = serde_json::json!({});
                }
                if !layer
                    .metadata
                    .get("transform")
                    .is_some_and(|v| v.is_object())
                {
                    layer.metadata["transform"] = serde_json::json!({});
                }
                layer.metadata["transform"]["sampling"] = sampling.into();
            }
            true
        }))
    }
    pub fn set_opacity(&mut self, id: &str, opacity: f32) -> bool {
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        if !opacity.is_finite() {
            return false;
        }
        self.change_layer(id, |layer| {
            let opacity = opacity.clamp(0.0, 1.0);
            let changed = layer.opacity != opacity;
            layer.opacity = opacity;
            changed
        })
    }
    pub fn set_blend_mode(&mut self, id: &str, mode: &str) -> bool {
        if !crate::raster::supported_blend_mode(mode) {
            return false;
        }
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        self.change_layer(id, |layer| {
            let changed = layer.blend_mode != mode;
            layer.blend_mode = mode.to_owned();
            changed
        })
    }
    fn change_layer(&mut self, id: &str, change: impl FnOnce(&mut Layer) -> bool) -> bool {
        self.finish_stroke();
        if self.document.find_layer(id).is_none() {
            return false;
        }
        let before = self.snapshot();
        if !change(self.document.find_layer_mut(id).unwrap()) {
            return false;
        }
        self.commit(before);
        true
    }
    /// Move into a group or root at a bottom-to-top index after removal.
    /// Cycles, image parents, and changes under locked ancestors are rejected.
    pub fn reorder_layer(&mut self, id: &str, parent: Option<&str>, index: usize) -> bool {
        self.finish_stroke();
        let Some(layer) = self.document.find_layer(id) else {
            return false;
        };
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        if let Some(parent_id) = parent {
            fn subtree_depth(layer: &Layer) -> usize {
                1 + layer.children.iter().map(subtree_depth).max().unwrap_or(0)
            }
            fn depth(layers: &[Layer], id: &str, at: usize) -> Option<usize> {
                for layer in layers {
                    if layer.id == id {
                        return Some(at);
                    }
                    if let Some(found) = depth(&layer.children, id, at + 1) {
                        return Some(found);
                    }
                }
                None
            }
            if depth(&self.document.layers, parent_id, 1).unwrap_or(64) + subtree_depth(layer) > 64
            {
                return false;
            }
            if id == parent_id || find(&layer.children, parent_id).is_some() {
                return false;
            }
            let Some(target) = self.document.find_layer(parent_id) else {
                return false;
            };
            if target.image.is_some() || locked_in_tree(&self.document.layers, parent_id, false) {
                return false;
            }
        }
        let before = self.snapshot();
        let layer = remove_layer(&mut self.document.layers, id).unwrap();
        let target = if let Some(parent) = parent {
            &mut self.document.find_layer_mut(parent).unwrap().children
        } else {
            &mut self.document.layers
        };
        target.insert(index.min(target.len()), layer);
        self.commit(before);
        true
    }
    pub fn transform_layer(
        &mut self,
        id: &str,
        x: f32,
        y: f32,
        rotation: f32,
        scale_x: f32,
        scale_y: f32,
    ) -> bool {
        if ![x, y, rotation, scale_x, scale_y]
            .iter()
            .all(|v| v.is_finite())
            || scale_x.abs() < 0.001
            || scale_y.abs() < 0.001
            || scale_x.abs() > 1000.0
            || scale_y.abs() > 1000.0
            || locked_in_tree(&self.document.layers, id, false)
        {
            return false;
        }
        if self
            .document
            .find_layer(id)
            .is_none_or(|l| l.image.is_none())
        {
            return false;
        }
        let before_placement = self.layer_placement(id);
        let changed = self.change_layer(id, |l| {
            let old = (l.offset_x, l.offset_y, l.rotation, l.scale_x, l.scale_y);
            (l.offset_x, l.offset_y, l.rotation, l.scale_x, l.scale_y) =
                (x, y, rotation.rem_euclid(360.0), scale_x, scale_y);
            old != (l.offset_x, l.offset_y, l.rotation, l.scale_x, l.scale_y)
        });
        if changed {
            if let (Some(old), Some(new)) = (before_placement, self.layer_placement(id)) {
                carry_mask_placement(self.document.find_layer_mut(id).unwrap(), old, new);
            }
        }
        changed
    }

    pub fn layer_placement(&self, id: &str) -> Option<LayerPlacement> {
        placement_of(
            self.document.find_layer(id)?,
            self.document.width,
            self.document.height,
        )
    }
    pub fn layer_to_canvas(&self, id: &str, x: f32, y: f32) -> Option<(f32, f32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Transform::for_layer(&self.document, id).map(|transform| transform.world(x, y))
    }
    pub fn canvas_to_layer(&self, id: &str, x: f32, y: f32) -> Option<(f32, f32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Transform::for_layer(&self.document, id)?.local(x, y)
    }
    pub fn set_layer_placement(&mut self, id: &str, value: LayerPlacement) -> bool {
        if !value.is_valid() || locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        let Some(layer) = self.document.find_layer(id) else {
            return false;
        };
        let (pw, ph) = source_size(layer, self.document.width, self.document.height);
        self.transform_layer(
            id,
            value.x,
            value.y,
            value.rotation,
            value.width / pw as f32 * if value.flip_x { -1.0 } else { 1.0 },
            value.height / ph as f32 * if value.flip_y { -1.0 } else { 1.0 },
        )
    }

    /// Transform several paint leaves as one upright selection box.
    pub fn transform_layers(
        &mut self,
        ids: &[String],
        from: LayerPlacement,
        to: LayerPlacement,
    ) -> bool {
        self.finish_stroke();
        if ids.is_empty() || !from.is_valid() || !to.is_valid() {
            return false;
        }
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        if unique.iter().any(|id| {
            locked_in_tree(&self.document.layers, id, false)
                || self
                    .document
                    .find_layer(id)
                    .is_none_or(|l| l.image.is_none())
        }) {
            return false;
        }
        let originals: Vec<_> = unique
            .iter()
            .filter_map(|id| self.layer_placement(id).map(|p| (id.clone(), p)))
            .collect();
        if originals.len() != unique.len() {
            return false;
        }
        let before = self.snapshot();
        let mut changed = false;
        for (id, old) in originals {
            let next = placement_following(old, from, to);
            if !next.is_valid() {
                continue;
            }
            let (pw, ph) = source_size(
                self.document.find_layer(&id).unwrap(),
                self.document.width,
                self.document.height,
            );
            let layer = self.document.find_layer_mut(&id).unwrap();
            layer.offset_x = next.x;
            layer.offset_y = next.y;
            layer.rotation = next.rotation.rem_euclid(360.0);
            layer.scale_x = next.width / pw as f32 * if next.flip_x { -1.0 } else { 1.0 };
            layer.scale_y = next.height / ph as f32 * if next.flip_y { -1.0 } else { 1.0 };
            carry_mask_placement(layer, old, next);
            changed |= old != next;
        }
        if changed {
            self.commit(before);
        }
        changed
    }

    /// Destructively resample a raster layer into canvas-space corners ordered
    /// top-left, top-right, bottom-right, bottom-left. Live objects must first
    /// be explicitly rasterized. A linked mask is warped in the same transaction.
    pub fn distort_layer(&mut self, id: &str, corners: [(f32, f32); 4]) -> anyhow::Result<bool> {
        self.finish_stroke();
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let layer = self
            .document
            .find_layer(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        if is_live_object(layer) {
            return Ok(false);
        }
        let image = layer
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Distortion requires raster pixels"))?;
        anyhow::ensure!(
            corners
                .iter()
                .flat_map(|&(x, y)| [x, y])
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Invalid distortion corners"
        );
        anyhow::ensure!(
            quad_is_usable(corners),
            "Distortion corners cross or collapse"
        );
        let min_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .floor();
        let min_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor();
        let max_x = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil();
        let max_y = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil();
        let width = (max_x - min_x).max(1.0) as u32;
        let height = (max_y - min_y).max(1.0) as u32;
        anyhow::ensure!(
            crate::model::valid_dimensions(width, height),
            "Distortion exceeds pixel limits"
        );
        let flip_x = layer.scale_x < 0.0;
        let flip_y = layer.scale_y < 0.0;
        let warped = warp_quad(image, corners, min_x, min_y, width, height, flip_x, flip_y);
        let warped_mask = if mask_linked(layer) {
            layer.mask.as_ref().map(|mask| {
                warp_quad(mask, corners, min_x, min_y, width, height, flip_x, flip_y).into()
            })
        } else {
            layer.mask.clone()
        };
        let before = self.snapshot();
        let layer = self.document.find_layer_mut(id).unwrap();
        layer.image = Some(warped.into());
        layer.mask = warped_mask;
        layer.offset_x = min_x;
        layer.offset_y = min_y;
        layer.rotation = 0.0;
        layer.scale_x = 1.0;
        layer.scale_y = 1.0;
        if mask_linked(layer) {
            remove_metadata(layer, "maskPlacement");
        }
        self.commit(before);
        Ok(true)
    }

    pub fn set_mask_linked(&mut self, id: &str, linked: bool) -> bool {
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        self.change_layer(id, |layer| {
            if layer.mask.is_none() || mask_linked(layer) == linked {
                return false;
            }
            metadata_bool(layer, "maskLinked", linked);
            true
        })
    }
    pub fn mask_placement(&self, id: &str) -> Option<LayerPlacement> {
        let layer = self.document.find_layer(id)?;
        if layer.mask.is_none() {
            return None;
        }
        metadata_placement(layer, "maskPlacement").or_else(|| self.layer_placement(id))
    }
    pub fn set_mask_placement(&mut self, id: &str, value: LayerPlacement) -> bool {
        if !value.is_valid() || locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        let layer_place = self.layer_placement(id);
        self.change_layer(id, |layer| {
            if layer.mask.is_none() || mask_linked(layer) {
                return false;
            }
            let old = metadata_placement(layer, "maskPlacement").or(layer_place);
            if old == Some(value) {
                return false;
            }
            if layer_place == Some(value) {
                remove_metadata(layer, "maskPlacement");
            } else {
                set_metadata_placement(layer, "maskPlacement", value);
            }
            true
        })
    }

    pub fn set_live_text(
        &mut self,
        id: &str,
        style: crate::objects::LiveTextStyle,
    ) -> anyhow::Result<bool> {
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let Some(old_place) = self.layer_placement(id) else {
            return Ok(false);
        };
        let layer = self
            .document
            .find_layer(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        if layer.advanced.is_some() {
            return Ok(false);
        }
        let old = crate::objects::live_text(layer)?;
        if old.as_ref() == Some(&style) {
            return Ok(false);
        }
        // Rasterization can fail (for example because the resulting point text is too
        // large), so prepare the complete replacement away from the document. This
        // also keeps pixels and metadata atomic if a later placement check fails.
        let mut replacement = layer.clone();
        let old_mask_place = replacement
            .mask
            .as_ref()
            .filter(|mask| mask.width() > 1 || mask.height() > 1)
            .and_then(|_| metadata_placement(&replacement, "maskPlacement").or(Some(old_place)));
        crate::objects::set_live_text(&mut replacement, style)?;
        resize_placement_preserving_upper_left(&mut replacement, old_place)?;
        // Changing the source raster must not silently resize an implicitly placed
        // mask. Materialize its previous canvas placement, matching the Mac editor.
        if metadata_placement(&replacement, "maskPlacement").is_none()
            && let Some(mask_place) = old_mask_place
        {
            set_metadata_placement(&mut replacement, "maskPlacement", mask_place);
        }
        let before = self.snapshot();
        *self.document.find_layer_mut(id).unwrap() = replacement;
        self.commit(before);
        Ok(true)
    }
    pub fn set_live_shape(
        &mut self,
        id: &str,
        style: crate::objects::LiveShapeStyle,
        width: u32,
        height: u32,
    ) -> anyhow::Result<bool> {
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let Some(old_place) = self.layer_placement(id) else {
            return Ok(false);
        };
        let before = self.snapshot();
        let layer = self
            .document
            .find_layer_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        if layer.advanced.is_some() {
            return Ok(false);
        }
        let old = crate::objects::live_shape(layer)?;
        if old.as_ref() == Some(&style)
            && layer
                .image
                .as_ref()
                .is_some_and(|i| i.dimensions() == (width, height))
        {
            return Ok(false);
        }
        crate::objects::set_live_shape(layer, style, width, height)?;
        restore_placement_bounds(layer, old_place);
        self.commit(before);
        Ok(true)
    }
    /// Explicit conversion required before pixel painting a live object.
    pub fn rasterize_layer(&mut self, id: &str) -> bool {
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        let advanced_proxy = self.document.find_layer(id).and_then(|layer| {
            layer.advanced.as_ref().and_then(|state| {
                state
                    .proxy()
                    .ok()
                    .map(|proxy| (proxy, state.recipe.blend_if.clone()))
            })
        });
        let before = self.snapshot();
        let Some(layer) = self.document.find_layer_mut(id) else {
            return false;
        };
        if let Some((proxy, blend_if)) = advanced_proxy {
            layer.advanced = None;
            layer.image = Some(proxy.into());
            if let Some(metadata) = layer.metadata.as_object_mut() {
                metadata.remove("rustEditableAsset");
                metadata.remove(crate::advanced::RASTER_BLEND_IF_KEY);
                if let Some(settings) = blend_if {
                    metadata.insert(
                        crate::advanced::RASTER_BLEND_IF_KEY.into(),
                        serde_json::to_value(settings).expect("Blend If is serializable"),
                    );
                }
            }
            self.commit(before);
            return true;
        }
        if crate::objects::rasterize_live_object(layer).ok() != Some(true) {
            return false;
        }
        crate::objects::detach_live_object(layer);
        self.commit(before);
        true
    }

    pub fn set_layer_effects(
        &mut self,
        id: &str,
        effects: serde_json::Value,
    ) -> anyhow::Result<bool> {
        crate::effects::validate_layer_metadata(
            &serde_json::json!({ "effects": effects.clone() }),
        )?;
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let Some(layer) = self.document.find_layer(id) else {
            return Ok(false);
        };
        if layer.image.is_none() || layer.metadata.get("effects") == Some(&effects) {
            return Ok(false);
        }
        self.change_layer(id, |layer| {
            layer.metadata["effects"] = effects;
            true
        });
        Ok(true)
    }
    pub fn add_adjustment(&mut self, adjustment: serde_json::Value) -> anyhow::Result<String> {
        crate::effects::validate_layer_metadata(
            &serde_json::json!({ "adjustment": adjustment.clone() }),
        )?;
        let before = self.snapshot();
        let mut layer = Layer::group("Adjustment");
        layer.metadata = serde_json::json!({ "isGroup": false, "adjustment": adjustment });
        let id = layer.id.clone();
        self.document.layers.push(layer);
        self.active_layer = id.clone();
        self.commit(before);
        Ok(id)
    }
    pub fn set_adjustment(
        &mut self,
        id: &str,
        adjustment: serde_json::Value,
    ) -> anyhow::Result<bool> {
        crate::effects::validate_layer_metadata(
            &serde_json::json!({ "adjustment": adjustment.clone() }),
        )?;
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let Some(layer) = self.document.find_layer(id) else {
            return Ok(false);
        };
        if layer.metadata.get("adjustment") == Some(&adjustment) {
            return Ok(false);
        }
        Ok(self.change_layer(id, |layer| {
            layer.metadata["adjustment"] = adjustment;
            true
        }))
    }
    pub fn set_live_mask_source(&mut self, id: &str, source: Option<&str>) -> anyhow::Result<bool> {
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let Some(target) = self.document.find_layer(id) else {
            return Ok(false);
        };
        let old = target
            .metadata
            .get("maskSourceID")
            .and_then(serde_json::Value::as_str);
        if old == source {
            return Ok(false);
        }
        if let Some(source_id) = source {
            anyhow::ensure!(
                source_id != id && self.document.find_layer(source_id).is_some(),
                "Invalid live mask source"
            );
            let mut cursor = Some(source_id);
            for _ in 0..=crate::model::MAX_LAYERS {
                let Some(current) = cursor else {
                    break;
                };
                anyhow::ensure!(current != id, "Live mask dependency cycle");
                cursor = self
                    .document
                    .find_layer(current)
                    .and_then(|layer| layer.metadata.get("maskSourceID"))
                    .and_then(serde_json::Value::as_str);
            }
            anyhow::ensure!(cursor.is_none(), "Live mask dependency cycle");
        }
        Ok(self.change_layer(id, |layer| {
            match source {
                Some(source_id) => layer.metadata["maskSourceID"] = serde_json::json!(source_id),
                None => remove_metadata(layer, "maskSourceID"),
            }
            true
        }))
    }

    pub fn guides(&self) -> Vec<CanvasGuide> {
        parse_guides(&self.document.metadata)
    }
    pub fn add_guide(&mut self, axis: GuideAxis, position: f32) -> Option<String> {
        self.finish_stroke();
        if !position.is_finite() || position.abs() > 1_000_000.0 {
            return None;
        }
        let before = self.snapshot();
        let id = uuid::Uuid::new_v4().to_string().to_uppercase();
        let mut guides = self.guides();
        guides.push(CanvasGuide {
            id: id.clone(),
            axis,
            position,
        });
        set_guides(&mut self.document.metadata, &guides);
        self.commit(before);
        Some(id)
    }
    pub fn move_guide(&mut self, id: &str, position: f32) -> bool {
        self.finish_stroke();
        if !position.is_finite() || position.abs() > 1_000_000.0 {
            return false;
        }
        let mut guides = self.guides();
        let Some(guide) = guides.iter_mut().find(|guide| guide.id == id) else {
            return false;
        };
        if guide.position == position {
            return false;
        }
        let before = self.snapshot();
        guide.position = position;
        set_guides(&mut self.document.metadata, &guides);
        self.commit(before);
        true
    }
    pub fn remove_guide(&mut self, id: &str) -> bool {
        self.finish_stroke();
        let mut guides = self.guides();
        let count = guides.len();
        guides.retain(|guide| guide.id != id);
        if guides.len() == count {
            return false;
        }
        let before = self.snapshot();
        set_guides(&mut self.document.metadata, &guides);
        self.commit(before);
        true
    }

    pub fn flip_horizontal(&mut self) -> bool {
        let Some(layer) = self.document.find_layer(&self.active_layer) else {
            return false;
        };
        let (id, x, y, rotation, sx, sy) = (
            layer.id.clone(),
            layer.offset_x,
            layer.offset_y,
            layer.rotation,
            -layer.scale_x,
            layer.scale_y,
        );
        self.transform_layer(&id, x, y, rotation, sx, sy)
    }
    pub fn flip_vertical(&mut self) -> bool {
        let Some(layer) = self.document.find_layer(&self.active_layer) else {
            return false;
        };
        let (id, x, y, rotation, sx, sy) = (
            layer.id.clone(),
            layer.offset_x,
            layer.offset_y,
            layer.rotation,
            layer.scale_x,
            -layer.scale_y,
        );
        self.transform_layer(&id, x, y, rotation, sx, sy)
    }
    /// Finish one selection gesture without changing the saved document revision.
    pub fn record_selection_change(&mut self, previous: Option<Selection>) -> bool {
        if self.selection == previous || self.floating.is_some() {
            return false;
        }
        self.selection_revision = self.selection_revision.wrapping_add(1);
        let mut before = self.snapshot();
        before.bytes = before
            .bytes
            .saturating_sub(before.selection.as_ref().map_or(0, |s| s.mask.capacity()))
            .saturating_add(previous.as_ref().map_or(0, |s| s.mask.capacity()));
        before.selection = previous;
        self.undo.push_back(HistoryEntry::Snapshot(before));
        self.redo.clear();
        self.trim_history();
        true
    }
    pub fn clear_selection(&mut self) {
        self.finish_stroke();
        self.selection = None;
    }
    pub fn select_all(&mut self) {
        self.finish_stroke();
        self.selection = Some(Selection {
            width: self.document.width,
            height: self.document.height,
            mask: vec![255; self.document.width as usize * self.document.height as usize],
        });
    }
    pub fn select_rectangle(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.select_shape(x, y, width, height, false);
    }
    pub fn select_ellipse(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.select_shape(x, y, width, height, true);
    }
    fn select_shape(&mut self, x: f32, y: f32, width: f32, height: f32, ellipse: bool) {
        self.finish_stroke();
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return;
        }
        let (left, right) = (x.min(x + width), x.max(x + width));
        let (top, bottom) = (y.min(y + height), y.max(y + height));
        let mut selection = Selection {
            width: self.document.width,
            height: self.document.height,
            mask: vec![0; self.document.width as usize * self.document.height as usize],
        };
        if right > left && bottom > top {
            for py in (top.floor().max(0.0) as u32).min(selection.height)
                ..(bottom.ceil().max(0.0) as u32).min(selection.height)
            {
                for px in (left.floor().max(0.0) as u32).min(selection.width)
                    ..(right.ceil().max(0.0) as u32).min(selection.width)
                {
                    let cx = px as f32 + 0.5;
                    let cy = py as f32 + 0.5;
                    let inside = cx >= left && cx < right && cy >= top && cy < bottom;
                    let ellipse_inside = !ellipse
                        || ((cx - (left + right) / 2.0) / ((right - left) / 2.0)).powi(2)
                            + ((cy - (top + bottom) / 2.0) / ((bottom - top) / 2.0)).powi(2)
                            <= 1.0;
                    if inside && ellipse_inside {
                        selection.mask[py as usize * selection.width as usize + px as usize] = 255;
                    }
                }
            }
        }
        self.selection = Some(selection);
    }
    pub fn invert_selection(&mut self) {
        self.finish_stroke();
        if let Some(selection) = &mut self.selection {
            for v in &mut selection.mask {
                *v = 255 - *v;
            }
        } else {
            self.select_all();
        }
    }
    /// Feather selection coverage with a bounded Gaussian blur. Selection edits
    /// are UI state and intentionally do not enter document undo history.
    pub fn feather_selection(&mut self, radius: f32) -> bool {
        self.finish_stroke();
        if !radius.is_finite() || radius <= 0.0 || radius > 512.0 {
            return false;
        }
        let Some(selection) = &mut self.selection else {
            return false;
        };
        let Some(image) =
            image::GrayImage::from_raw(selection.width, selection.height, selection.mask.clone())
        else {
            return false;
        };
        let blurred = image::imageops::blur(&image, radius);
        let next = blurred.into_raw();
        if next == selection.mask {
            return false;
        }
        selection.mask = next;
        true
    }
    /// Positive pixels grow a selection, negative pixels contract it. Uses a
    /// circular neighborhood and preserves the strongest soft-edge coverage.
    pub fn resize_selection(&mut self, pixels: i32) -> bool {
        self.finish_stroke();
        if pixels == 0 || pixels.unsigned_abs() > 256 {
            return false;
        }
        let Some(selection) = &mut self.selection else {
            return false;
        };
        let radius = pixels.unsigned_abs() as i32;
        let mut output = vec![if pixels > 0 { 0 } else { 255 }; selection.mask.len()];
        let offsets: Vec<_> = (-radius..=radius)
            .flat_map(|dy| {
                (-radius..=radius)
                    .filter(move |dx| dx * dx + dy * dy <= radius * radius)
                    .map(move |dx| (dx, dy))
            })
            .collect();
        if selection.mask.len().saturating_mul(offsets.len()) > 128_000_000 {
            return false;
        }
        for y in 0..selection.height as i32 {
            for x in 0..selection.width as i32 {
                let mut value = if pixels > 0 { 0 } else { 255 };
                for &(dx, dy) in &offsets {
                    let sample = if x + dx < 0
                        || y + dy < 0
                        || x + dx >= selection.width as i32
                        || y + dy >= selection.height as i32
                    {
                        0
                    } else {
                        selection.mask
                            [(y + dy) as usize * selection.width as usize + (x + dx) as usize]
                    };
                    value = if pixels > 0 {
                        value.max(sample)
                    } else {
                        value.min(sample)
                    };
                }
                output[y as usize * selection.width as usize + x as usize] = value;
            }
        }
        if output == selection.mask {
            return false;
        }
        selection.mask = output;
        true
    }
    /// Wand selects contiguous similar source pixels on the active paint layer.
    pub fn wand_select(&mut self, x: i32, y: i32, tolerance: u8) -> bool {
        self.finish_stroke();
        let Some(layer) = self.document.find_layer(&self.active_layer) else {
            return false;
        };
        let Some(transform) = Transform::for_layer(&self.document, &self.active_layer) else {
            return false;
        };
        let Some((lx, ly)) = transform.local(x as f32 + 0.5, y as f32 + 0.5) else {
            return false;
        };
        let Some(image) = &layer.image else {
            return false;
        };
        let pixels = flood_pixels(
            image,
            lx.floor() as i32,
            ly.floor() as i32,
            tolerance,
            |_| true,
        );
        if pixels.is_empty() {
            return false;
        }
        let mut mask = vec![0; self.document.width as usize * self.document.height as usize];
        // Map each canvas pixel into the flood mask so enlarged layers select their full footprint.
        let mut source_mask = vec![false; image.width() as usize * image.height() as usize];
        for (px, py) in pixels {
            source_mask[py as usize * image.width() as usize + px as usize] = true;
        }
        for cy in 0..self.document.height {
            for cx in 0..self.document.width {
                let Some((sx, sy)) = transform.local(cx as f32 + 0.5, cy as f32 + 0.5) else {
                    continue;
                };
                if sx >= 0.0
                    && sy >= 0.0
                    && sx < image.width() as f32
                    && sy < image.height() as f32
                    && source_mask[sy as usize * image.width() as usize + sx as usize]
                {
                    mask[cy as usize * self.document.width as usize + cx as usize] = 255;
                }
            }
        }
        self.selection = Some(Selection {
            width: self.document.width,
            height: self.document.height,
            mask,
        });
        true
    }
    pub fn combine_selection(
        &mut self,
        previous: Option<Selection>,
        mode: crate::selection_tools::SelectionMode,
    ) {
        if let Some(incoming) = self.selection.as_ref() {
            self.selection = Some(crate::selection_tools::combine(
                previous.as_ref(),
                incoming,
                mode,
            ));
        }
    }
    /// Source-compatible canvas sample: the visible composite or active raw asset
    /// at its displayed transform, without opacity, blend, effects, or a mask.
    pub fn selection_sample(&self, all_layers: bool) -> anyhow::Result<RgbaImage> {
        anyhow::ensure!(
            valid_size(self.document.width, self.document.height),
            "Invalid selection canvas"
        );
        let sample = if all_layers {
            crate::raster::composite(&self.document)
        } else {
            let mut doc = Document::new(1, 1);
            doc.width = self.document.width;
            doc.height = self.document.height;
            doc.background = [0; 4];
            doc.layers.clear();
            if let Some(original) = self
                .document
                .find_layer(&self.active_layer)
                .filter(|layer| !layer.is_group() && layer.image.is_some())
            {
                let mut layer = shallow_layer(original);
                layer.advanced = None;
                layer.visible = true;
                layer.opacity = 1.;
                layer.blend_mode = "Normal".into();
                let transform = layer.metadata.get("transform").cloned();
                layer.metadata = serde_json::json!({});
                if let Some(transform) = transform {
                    layer.metadata["transform"] = transform;
                }
                doc.layers.push(layer);
            }
            crate::raster::composite(&doc)
        };
        anyhow::ensure!(
            sample.dimensions() == (self.document.width, self.document.height),
            "Selection sample exceeds renderer limits"
        );
        Ok(sample)
    }
    pub fn wand_select_with(
        &mut self,
        x: i32,
        y: i32,
        settings: &crate::selection_tools::WandSettings,
        mode: crate::selection_tools::SelectionMode,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        if x < 0 || y < 0 || x as u32 >= self.document.width || y as u32 >= self.document.height {
            return Ok(false);
        }
        let incoming = crate::selection_tools::wand(
            &self.selection_sample(settings.all_layers)?,
            x as u32,
            y as u32,
            settings,
        )?;
        let next = crate::selection_tools::combine(self.selection.as_ref(), &incoming, mode);
        let changed = self
            .selection
            .as_ref()
            .map(|old| {
                old.mask != next.mask || old.width != next.width || old.height != next.height
            })
            .unwrap_or(true);
        self.selection = Some(next);
        Ok(changed)
    }
    pub fn gradient_with(
        &mut self,
        start: (f32, f32),
        end: (f32, f32),
        from: [u8; 4],
        to: [u8; 4],
        settings: &crate::gradient_tools::GradientSettings,
    ) -> anyhow::Result<bool> {
        anyhow::ensure!(
            [start.0, start.1, end.0, end.1, settings.opacity]
                .iter()
                .all(|v| v.is_finite())
                && (0. ..=1.).contains(&settings.opacity),
            "Invalid gradient settings"
        );
        if (end.0 - start.0).hypot(end.1 - start.1) <= f32::EPSILON {
            return Ok(false);
        }
        Ok(self.map_active_pixels(|x, y, pixel| {
            over(
                pixel,
                crate::gradient_tools::sample((x, y), start, end, from, to, settings),
                1.,
            )
        }))
    }
    pub fn gradient_mask_with(
        &mut self,
        id: &str,
        start: (f32, f32),
        end: (f32, f32),
        from: u8,
        to: u8,
        settings: &crate::gradient_tools::GradientSettings,
    ) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let changed = work.gradient_with(
            start,
            end,
            [from, from, from, 255],
            [to, to, to, 255],
            settings,
        )?;
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    pub fn fill_at(&mut self, x: i32, y: i32, color: [u8; 4], tolerance: u8) -> bool {
        self.finish_stroke();
        let Some(layer) = self.editable_layer() else {
            return false;
        };
        let Some(transform) = Transform::for_layer(&self.document, &self.active_layer) else {
            return false;
        };
        let Some((lx, ly)) = transform.local(x as f32 + 0.5, y as f32 + 0.5) else {
            return false;
        };
        let image = layer.image.as_ref().unwrap();
        let pixels = flood_pixels(
            image,
            lx.floor() as i32,
            ly.floor() as i32,
            tolerance,
            |(px, py)| {
                let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
                selected(&self.selection, wx, wy)
            },
        );
        if pixels.is_empty()
            || pixels
                .iter()
                .all(|&(px, py)| image.get_pixel(px, py).0 == color)
        {
            return false;
        }
        let before = self.snapshot();
        let image = self
            .document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image
            .as_mut()
            .unwrap();
        for (px, py) in pixels {
            image.put_pixel(px, py, Rgba(color));
        }
        self.commit(before);
        true
    }
    pub fn fill_selection(&mut self, color: [u8; 4]) -> bool {
        self.map_active_pixels(|_, _, _| color)
    }
    pub fn clear_selected_pixels(&mut self) -> bool {
        self.fill_selection([0; 4])
    }
    pub fn gradient(
        &mut self,
        start: (f32, f32),
        end: (f32, f32),
        from: [u8; 4],
        to: [u8; 4],
    ) -> bool {
        if ![start.0, start.1, end.0, end.1]
            .iter()
            .all(|v| v.is_finite())
        {
            return false;
        }
        let (dx, dy) = (end.0 - start.0, end.1 - start.1);
        let length = dx * dx + dy * dy;
        if length <= f32::EPSILON || !length.is_finite() {
            return false;
        }
        self.map_active_pixels(|x, y, _| {
            let t = (((x - start.0) * dx + (y - start.1) * dy) / length).clamp(0.0, 1.0);
            let mut color = [0; 4];
            for c in 0..4 {
                color[c] = (from[c] as f32 * (1.0 - t) + to[c] as f32 * t).round() as u8;
            }
            color
        })
    }
    fn map_active_pixels(&mut self, mut apply: impl FnMut(f32, f32, [u8; 4]) -> [u8; 4]) -> bool {
        self.finish_stroke();
        if self.editable_layer().is_none() {
            return false;
        }
        let Some(transform) = Transform::for_layer(&self.document, &self.active_layer) else {
            return false;
        };
        let before = self.snapshot();
        let mut changed = false;
        let image = self
            .document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image
            .as_mut()
            .unwrap();
        for (px, py, pixel) in image.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
            if !selected(&self.selection, wx, wy) {
                continue;
            }
            let next = blend_coverage(
                pixel.0,
                apply(wx, wy, pixel.0),
                selection_coverage(&self.selection, wx, wy),
            );
            changed |= next != pixel.0;
            pixel.0 = next;
        }
        if changed {
            self.commit(before);
        }
        changed
    }
    /// Synthesize selected source pixels using the preserved deterministic patch-search kernel.
    pub fn content_aware_fill(&mut self) -> anyhow::Result<bool> {
        self.finish_stroke();
        let Some(selection) = &self.selection else {
            return Ok(false);
        };
        let Some(layer) = self.editable_layer() else {
            return Ok(false);
        };
        let Some(transform) = Transform::for_layer(&self.document, &self.active_layer) else {
            return Ok(false);
        };
        let original = layer.image.as_ref().unwrap();
        anyhow::ensure!(
            u64::from(original.width()) * u64::from(original.height()) <= 16_777_216,
            "Content-aware fill supports layers up to 16 million pixels"
        );
        let mask = image::GrayImage::from_fn(original.width(), original.height(), |x, y| {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            image::Luma([
                if selection.contains(wx.floor() as i32, wy.floor() as i32) {
                    255
                } else {
                    0
                },
            ])
        });
        anyhow::ensure!(
            mask.pixels().filter(|p| p[0] != 0).count() <= 250_000,
            "Fill up to 250000 selected pixels at a time"
        );
        let mut filled = original.clone();
        crate::retouch::content_fill(&mut filled, &mask)?;
        let mut output = original.clone();
        let mut changed = false;
        for (x, y, pixel) in output.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let coverage = selection_coverage(&self.selection, wx, wy);
            if coverage <= 0.0 {
                continue;
            }
            let next = blend_coverage(pixel.0, filled.get_pixel(x, y).0, coverage);
            changed |= next != pixel.0;
            pixel.0 = next;
        }
        if !changed {
            return Ok(false);
        }
        let before = self.snapshot();
        self.document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image = Some(output.into());
        self.commit(before);
        Ok(true)
    }

    /// Apply one canvas-space Blur, Smudge, or Liquify stroke to the active
    /// raster layer and map the result back through its transform in one undo step.
    pub fn retouch_stroke(
        &mut self,
        points: &[(f32, f32)],
        mode: RetouchMode,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        let Some(layer) = self.editable_layer() else {
            return Ok(false);
        };
        anyhow::ensure!(
            u64::from(self.document.width) * u64::from(self.document.height) <= 16_777_216,
            "Retouch strokes support canvases up to 16 million pixels"
        );
        anyhow::ensure!(
            points.len() <= 100_000,
            "Retouch stroke has too many points"
        );
        let transform = Transform::for_layer(&self.document, &self.active_layer)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        let original_pixels = layer.image.as_ref().unwrap().clone();
        anyhow::ensure!(
            u64::from(original_pixels.width()) * u64::from(original_pixels.height()) <= 16_777_216,
            "Retouch strokes support layers up to 16 million pixels"
        );
        let mut projected = shallow_layer(layer);
        projected.mask = None;
        projected.opacity = 1.0;
        projected.visible = true;
        projected.blend_mode = "normal".into();
        projected.metadata = serde_json::json!({});
        let canvas_before = crate::raster::composite(&Document {
            width: self.document.width,
            height: self.document.height,
            name: String::new(),
            background: [0; 4],
            layers: vec![projected],
            metadata: serde_json::Value::Null,
        });
        let stroke_points: Vec<_> = points
            .iter()
            .map(|&(x, y)| crate::retouch_brush::StrokePoint { x, y })
            .collect();
        let canvas_after = crate::retouch_brush::apply(
            &canvas_before,
            &stroke_points,
            self.brush.size,
            self.brush.hardness.min(0.98),
            self.brush.opacity,
            mode,
        )?;
        let mut output = original_pixels.clone();
        let mut changed = false;
        for (x, y, pixel) in output.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let coverage = selection_coverage(&self.selection, wx, wy);
            if coverage <= 0.0 {
                continue;
            }
            let (cx, cy) = (wx.floor() as i32, wy.floor() as i32);
            if cx < 0
                || cy < 0
                || cx as u32 >= self.document.width
                || cy as u32 >= self.document.height
            {
                continue;
            }
            let before = canvas_before.get_pixel(cx as u32, cy as u32).0;
            let after = canvas_after.get_pixel(cx as u32, cy as u32).0;
            if before == after {
                continue;
            }
            let next = blend_coverage(pixel.0, after, coverage);
            changed |= next != pixel.0;
            pixel.0 = next;
        }
        if !changed {
            return Ok(false);
        }
        let before = self.snapshot();
        self.document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image = Some(output.into());
        self.commit(before);
        Ok(true)
    }

    /// Apply the preserved source-free Spot Healing kernel in one undo step.
    /// Points and brush size are in canvas coordinates; selection coverage is
    /// sampled in canvas space before being mapped into the layer source.
    pub fn spot_heal_stroke(
        &mut self,
        points: &[(f32, f32)],
        mode: SpotHealingMode,
        seed: u32,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        let Some(layer) = self.editable_layer() else {
            return Ok(false);
        };
        anyhow::ensure!(!points.is_empty(), "Spot healing needs at least one point");
        anyhow::ensure!(
            points.len() <= 100_000,
            "Spot healing stroke has too many points"
        );
        anyhow::ensure!(
            points.iter().all(|(x, y)| x.is_finite() && y.is_finite()),
            "Spot healing points must be finite"
        );
        anyhow::ensure!(
            self.brush.size.is_finite() && (1.0..=4096.0).contains(&self.brush.size),
            "Spot healing brush size is invalid"
        );
        let transform = Transform::for_layer(&self.document, &self.active_layer)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        let original = layer.image.as_ref().unwrap().clone();
        let mut coverage = GrayImage::new(original.width(), original.height());
        let spacing = (self.brush.size * 0.12).max(0.5);
        let mut dabs = Vec::with_capacity(points.len());
        dabs.push(points[0]);
        for pair in points.windows(2) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            let distance = dx.hypot(dy);
            let steps = (distance / spacing).ceil().max(1.0) as usize;
            anyhow::ensure!(
                dabs.len().saturating_add(steps) <= 2_000_000,
                "Spot healing stroke is too long"
            );
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                dabs.push((pair[0].0 + dx * t, pair[0].1 + dy * t));
            }
        }
        anyhow::ensure!(
            u64::from(original.width())
                .saturating_mul(u64::from(original.height()))
                .saturating_mul(dabs.len() as u64)
                <= 200_000_000,
            "Spot healing stroke requires too much work"
        );
        let radius = self.brush.size * 0.5;
        let hardness = self.brush.hardness.clamp(0.0, 1.0);
        for (x, y, pixel) in coverage.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let selection = selection_coverage(&self.selection, wx, wy);
            if selection <= 0.0 {
                continue;
            }
            let mut amount = 0.0f32;
            for &(px, py) in &dabs {
                let distance = (wx - px).hypot(wy - py) / radius;
                if distance > 1.0 {
                    continue;
                }
                let dab = if distance <= hardness {
                    1.0
                } else {
                    1.0 - (distance - hardness) / (1.0 - hardness).max(0.001)
                };
                amount = amount.max(dab);
            }
            pixel.0 = [(amount * selection * 255.0).round() as u8];
        }
        let mut output = original.clone();
        if !crate::spot_heal::apply(&mut output, &coverage, self.brush.opacity, mode, seed)? {
            return Ok(false);
        }
        let before = self.snapshot();
        self.document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image = Some(output.into());
        self.commit(before);
        Ok(true)
    }

    /// Run a bounded image operation, then commit only selected source pixels.
    /// The operation cannot resize the layer and never receives live objects.
    pub fn apply_image_operation(
        &mut self,
        operation: impl FnOnce(&RgbaImage) -> anyhow::Result<RgbaImage>,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        let Some(layer) = self.editable_layer() else {
            return Ok(false);
        };
        let transform = Transform::for_layer(&self.document, &self.active_layer)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        let original = layer.image.as_ref().unwrap();
        let result = operation(original)?;
        anyhow::ensure!(
            result.dimensions() == original.dimensions(),
            "Image operation changed layer dimensions"
        );
        let mut output = original.clone();
        let mut changed = false;
        for (x, y, pixel) in output.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let coverage = selection_coverage(&self.selection, wx, wy);
            if coverage <= 0.0 {
                continue;
            }
            let next = blend_coverage(pixel.0, result.get_pixel(x, y).0, coverage);
            changed |= next != pixel.0;
            pixel.0 = next;
        }
        if !changed {
            return Ok(false);
        }
        let before = self.snapshot();
        self.document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image = Some(output.into());
        self.commit(before);
        Ok(true)
    }
    pub fn adjust(&mut self, adjustment: Adjustment) -> bool {
        match adjustment {
            Adjustment::Blur(sigma) | Adjustment::Sharpen(sigma) => {
                if !sigma.is_finite() || sigma <= 0.0 {
                    return false;
                }
                self.finish_stroke();
                let Some(layer) = self.editable_layer() else {
                    return false;
                };
                let Some(transform) = Transform::for_layer(&self.document, &self.active_layer)
                else {
                    return false;
                };
                let original = layer.image.as_ref().unwrap();
                let mut premultiplied = original.clone();
                for pixel in premultiplied.pixels_mut() {
                    let alpha = pixel[3] as f32 / 255.0;
                    for c in 0..3 {
                        pixel[c] = (pixel[c] as f32 * alpha).round() as u8;
                    }
                }
                let mut result = image::imageops::blur(&*premultiplied, sigma.min(64.0));
                for pixel in result.pixels_mut() {
                    if pixel[3] != 0 {
                        for c in 0..3 {
                            pixel[c] = (pixel[c] as f32 * 255.0 / pixel[3] as f32)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                if matches!(adjustment, Adjustment::Sharpen(_)) {
                    for (pixel, source) in result.pixels_mut().zip(original.pixels()) {
                        for c in 0..3 {
                            pixel[c] = (2 * source[c] as i16 - pixel[c] as i16).clamp(0, 255) as u8;
                        }
                        pixel[3] = source[3];
                    }
                }
                let before = self.snapshot();
                let image = self
                    .document
                    .find_layer_mut(&self.active_layer)
                    .unwrap()
                    .image
                    .as_mut()
                    .unwrap();
                let mut changed = false;
                for (px, py, pixel) in image.enumerate_pixels_mut() {
                    let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
                    let coverage = selection_coverage(&self.selection, wx, wy);
                    if coverage <= 0.0 {
                        continue;
                    }
                    let next = blend_coverage(pixel.0, result.get_pixel(px, py).0, coverage);
                    changed |= pixel.0 != next;
                    pixel.0 = next;
                }
                if changed {
                    self.commit(before);
                }
                changed
            }
            _ => {
                if let Adjustment::Brightness(v)
                | Adjustment::Contrast(v)
                | Adjustment::Saturation(v) = adjustment
                {
                    if !v.is_finite() {
                        return false;
                    }
                }
                self.map_active_pixels(|_, _, mut pixel| {
                    let gray = 0.2126 * pixel[0] as f32
                        + 0.7152 * pixel[1] as f32
                        + 0.0722 * pixel[2] as f32;
                    for value in &mut pixel[..3] {
                        let x = *value as f32;
                        let next = match adjustment {
                            Adjustment::Brightness(v) => x + v.clamp(-1.0, 1.0) * 255.0,
                            Adjustment::Contrast(v) => {
                                let v = v.clamp(-1.0, 0.999);
                                (x - 127.5) * (1.0 + v) / (1.0 - v) + 127.5
                            }
                            Adjustment::Saturation(v) => {
                                gray + (x - gray) * (1.0 + v.clamp(-1.0, 1.0))
                            }
                            Adjustment::Invert => 255.0 - x,
                            Adjustment::Grayscale => gray,
                            _ => x,
                        };
                        *value = next.round().clamp(0.0, 255.0) as u8;
                    }
                    pixel
                })
            }
        }
    }
    pub fn apply_filter(&mut self, filter: &crate::filters::Filter) -> bool {
        self.finish_stroke();
        if crate::filters::validate(filter).is_err() {
            return false;
        }
        let Some(layer) = self.editable_layer() else {
            return false;
        };
        let Some(transform) = Transform::for_layer(&self.document, &self.active_layer) else {
            return false;
        };
        let mut result = layer.image.as_ref().unwrap().clone();
        if crate::filters::apply(&mut result, filter).is_err() {
            return false;
        }
        let before = self.snapshot();
        let image = self
            .document
            .find_layer_mut(&self.active_layer)
            .unwrap()
            .image
            .as_mut()
            .unwrap();
        let mut changed = false;
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let coverage = selection_coverage(&self.selection, wx, wy);
            if coverage <= 0.0 {
                continue;
            }
            let next = blend_coverage(pixel.0, result.get_pixel(x, y).0, coverage);
            changed |= pixel.0 != next;
            pixel.0 = next;
        }
        if changed {
            self.commit(before);
        }
        changed
    }
    /// Polygon/lasso selection using the even-odd fill rule at pixel centers.
    pub fn select_polygon(&mut self, points: &[(f32, f32)]) -> bool {
        self.finish_stroke();
        if points.len() < 3
            || points.len() > 65_536
            || points
                .iter()
                .any(|&(x, y)| !x.is_finite() || !y.is_finite())
        {
            return false;
        }
        let mut mask = vec![0; self.document.width as usize * self.document.height as usize];
        let top = points
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let bottom = points
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0) as u32;
        let mut intersections = Vec::with_capacity(points.len());
        for y in top.min(self.document.height)..bottom.min(self.document.height) {
            intersections.clear();
            let py = y as f64 + 0.5;
            for index in 0..points.len() {
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                let (ax, ay, bx, by) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
                if (ay > py) != (by > py) {
                    intersections.push(ax + (py - ay) * (bx - ax) / (by - ay));
                }
            }
            intersections.sort_by(f64::total_cmp);
            for pair in intersections.chunks_exact(2) {
                let left = (pair[0] - 0.5).ceil().max(0.0) as u32;
                let right = (pair[1] - 0.5).ceil().max(0.0) as u32;
                for x in left.min(self.document.width)..right.min(self.document.width) {
                    mask[y as usize * self.document.width as usize + x as usize] = 255;
                }
            }
        }
        self.selection = Some(Selection {
            width: self.document.width,
            height: self.document.height,
            mask,
        });
        true
    }
    /// Move a folder as a transaction; pass-through group transforms never move children.
    pub fn move_group(&mut self, id: &str, dx: f32, dy: f32) -> bool {
        self.finish_stroke();
        if !dx.is_finite()
            || !dy.is_finite()
            || (dx == 0.0 && dy == 0.0)
            || locked_in_tree(&self.document.layers, id, false)
        {
            return false;
        }
        let Some(layer) = self.document.find_layer(id) else {
            return false;
        };
        fn can_move(layer: &Layer, dx: f32, dy: f32) -> bool {
            !layer.locked
                && (layer.offset_x + dx).is_finite()
                && (layer.offset_y + dy).is_finite()
                && layer.children.iter().all(|child| can_move(child, dx, dy))
        }
        if layer.image.is_some() || !can_move(layer, dx, dy) {
            return false;
        }
        let before = self.snapshot();
        fn shift(layer: &mut Layer, dx: f32, dy: f32) {
            layer.offset_x += dx;
            layer.offset_y += dy;
            for child in &mut layer.children {
                shift(child, dx, dy);
            }
        }
        shift(self.document.find_layer_mut(id).unwrap(), dx, dy);
        self.commit(before);
        true
    }
    /// Erase mask coverage at canvas coordinates, keeping editable mask RGB intact.
    pub fn erase_mask(&mut self, id: &str, x: f32, y: f32, radius: f32, opacity: f32) -> bool {
        self.finish_stroke();
        if ![x, y, radius, opacity].iter().all(|v| v.is_finite())
            || radius <= 0.0
            || !(0.0..=1.0).contains(&opacity)
            || locked_in_tree(&self.document.layers, id, false)
        {
            return false;
        }
        let Some(layer) = self.document.find_layer(id) else {
            return false;
        };
        let Some(mask) = &layer.mask else {
            return false;
        };
        // Detached leaf masks have their own canvas placement. Group masks use
        // their declared transform extent when no explicit placement exists.
        let (transform, local_width, local_height) =
            if layer.image.is_some() || metadata_placement(layer, "maskPlacement").is_some() {
                let Some(transform) = Transform::for_mask(&self.document, id) else {
                    return false;
                };
                (transform, mask.width(), mask.height())
            } else {
                let mut mask_layer = shallow_layer(layer);
                mask_layer.image = Some(mask.clone());
                if let Some(size) = layer
                    .metadata
                    .get("transform")
                    .and_then(|t| t.get("size"))
                    .and_then(serde_json::Value::as_array)
                {
                    if let (Some(w), Some(h)) = (
                        size.first().and_then(serde_json::Value::as_f64),
                        size.get(1).and_then(serde_json::Value::as_f64),
                    ) {
                        mask_layer.scale_x *= w as f32 / mask.width() as f32;
                        mask_layer.scale_y *= h as f32 / mask.height() as f32;
                    }
                }
                (
                    Transform::of(&mask_layer, mask.width(), mask.height()),
                    mask.width(),
                    mask.height(),
                )
            };
        let (mw, mh) = mask.dimensions();
        if mw == 0 || mh == 0 {
            return false;
        }
        let radius = radius.min(2048.0);
        let before = self.snapshot();
        let mut changed = false;
        let mask = self
            .document
            .find_layer_mut(id)
            .unwrap()
            .mask
            .as_mut()
            .unwrap();
        for (mx, my, pixel) in mask.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(
                (mx as f32 + 0.5) * local_width as f32 / mw as f32,
                (my as f32 + 0.5) * local_height as f32 / mh as f32,
            );
            if !selected(&self.selection, wx, wy) {
                continue;
            }
            let distance = (wx - x).hypot(wy - y) / radius;
            if distance > 1.0 {
                continue;
            }
            let next =
                (pixel[3] as f32 * (1.0 - opacity * (1.0 - distance * distance))).round() as u8;
            changed |= next != pixel[3];
            pixel[3] = next;
        }
        if changed {
            self.commit(before);
        }
        changed
    }
    /// Resize each layer over its full transformed extent, preserving off-canvas
    /// pixels. Cached live objects become raster layers, as in the source editor.
    pub fn resize_image(&mut self, width: u32, height: u32) -> bool {
        let resolution = self
            .document
            .metadata
            .get("resolution")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(72.);
        self.resize_image_with_options(width, height, resolution, "High quality")
    }
    pub fn resize_image_with_options(
        &mut self,
        width: u32,
        height: u32,
        resolution: f64,
        sampling: &str,
    ) -> bool {
        self.finish_stroke();
        if !valid_size(width, height)
            || !resolution.is_finite()
            || !(1.0..=9600.0).contains(&resolution)
            || !matches!(sampling, "Nearest" | "Smooth" | "High quality")
            || !crate::raster::validate(&self.document).is_empty()
        {
            return false;
        }
        let old_resolution = self
            .document
            .metadata
            .get("resolution")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(72.);
        if (width, height) == (self.document.width, self.document.height) {
            if old_resolution == resolution {
                return false;
            }
            let before = self.snapshot();
            if !self.document.metadata.is_object() {
                self.document.metadata = serde_json::json!({});
            }
            self.document.metadata["resolution"] = serde_json::json!(resolution);
            self.commit(before);
            return true;
        }
        fn locked(layers: &[Layer]) -> bool {
            layers.iter().any(|l| l.locked || locked(&l.children))
        }
        if locked(&self.document.layers) {
            return false;
        }
        let sx = width as f32 / self.document.width as f32;
        let sy = height as f32 / self.document.height as f32;
        fn extent(p: LayerPlacement, sx: f32, sy: f32) -> Option<LayerPlacement> {
            let corners = [
                p.point(0., 0.),
                p.point(1., 0.),
                p.point(1., 1.),
                p.point(0., 1.),
            ];
            let x = corners
                .iter()
                .map(|v| v.0 * sx)
                .fold(f32::INFINITY, f32::min)
                .floor();
            let y = corners
                .iter()
                .map(|v| v.1 * sy)
                .fold(f32::INFINITY, f32::min)
                .floor();
            let right = corners
                .iter()
                .map(|v| v.0 * sx)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil();
            let bottom = corners
                .iter()
                .map(|v| v.1 * sy)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil();
            let out = LayerPlacement {
                x,
                y,
                width: right - x,
                height: bottom - y,
                rotation: 0.,
                flip_x: false,
                flip_y: false,
            };
            (out.is_valid() && valid_size(out.width as u32, out.height as u32)).then_some(out)
        }
        fn scaled_placement(p: LayerPlacement, sx: f32, sy: f32) -> Option<LayerPlacement> {
            let (sin, cos) = p.rotation.to_radians().sin_cos();
            let width = (p.width * cos * sx).hypot(p.width * sin * sy);
            let height = (p.height * sin * sx).hypot(p.height * cos * sy);
            let rotation = (sin * sy).atan2(cos * sx).to_degrees();
            let center = p.center();
            let center = (center.0 * sx, center.1 * sy);
            let out = LayerPlacement {
                x: center.0 - width * 0.5,
                y: center.1 - height * 0.5,
                width,
                height,
                rotation,
                flip_x: p.flip_x,
                flip_y: p.flip_y,
            };
            out.is_valid().then_some(out)
        }
        fn plane(
            source: &RgbaImage,
            old: LayerPlacement,
            new: LayerPlacement,
            sx: f32,
            sy: f32,
            mask: bool,
            sampling: &str,
            budget: &mut (u64, u64, u64),
        ) -> Option<RgbaImage> {
            if mask && source.width() == 1 && source.height() == 1 {
                return Some(source.clone());
            }
            let count = new.width as u64 * new.height as u64;
            let used = if mask { &mut budget.1 } else { &mut budget.0 };
            *used = used.saturating_add(count);
            if *used > crate::model::MAX_PIXELS {
                return None;
            }
            let mut temporary = Layer::group("");
            temporary.offset_x = old.x;
            temporary.offset_y = old.y;
            temporary.rotation = old.rotation;
            temporary.scale_x =
                old.width / source.width() as f32 * if old.flip_x { -1. } else { 1. };
            temporary.scale_y =
                old.height / source.height() as f32 * if old.flip_y { -1. } else { 1. };
            let transform = Transform::of(&temporary, source.width(), source.height());
            let scale_x = (transform.a * sx).hypot(transform.b * sy).max(0.0001) as f64;
            let scale_y = (transform.c * sx).hypot(transform.d * sy).max(0.0001) as f64;
            let taps = |s: f64| (6. / s.clamp(3. / 32., 1.)).ceil() as u64 + 2;
            let work = match sampling {
                "Nearest" => count,
                "Smooth" => count.saturating_mul(4),
                _ => count
                    .saturating_mul(taps(scale_x))
                    .saturating_mul(taps(scale_y)),
            };
            budget.2 = budget.2.saturating_add(work);
            if budget.2 > 1_000_000_000 {
                return None;
            }
            let gray = |p: &Rgba<u8>| {
                ((54 * u32::from(p[0]) + 183 * u32::from(p[1]) + 19 * u32::from(p[2]) + 128) / 256
                    * u32::from(p[3])
                    + 127)
                    / 255
            };
            let background = if mask {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([0; 4])
            };
            let mut output = RgbaImage::from_pixel(new.width as u32, new.height as u32, background);
            for (x, y, pixel) in output.enumerate_pixels_mut() {
                let Some((u, v)) =
                    transform.local((new.x + x as f32 + 0.5) / sx, (new.y + y as f32 + 0.5) / sy)
                else {
                    continue;
                };
                if u < 0. || v < 0. || u >= source.width() as f32 || v >= source.height() as f32 {
                    continue;
                }
                let sampled = Rgba(match sampling {
                    "Nearest" => source.get_pixel(u.floor() as u32, v.floor() as u32).0,
                    "Smooth" => crate::raster::sample_linear(source, f64::from(u), f64::from(v)),
                    _ => crate::raster::sample_lanczos(
                        source,
                        f64::from(u),
                        f64::from(v),
                        scale_x,
                        scale_y,
                    ),
                });
                *pixel = if mask {
                    let v = gray(&sampled) as u8;
                    Rgba([v, v, v, 255])
                } else {
                    sampled
                };
            }
            Some(output)
        }
        fn resize(
            layers: &[Layer],
            ow: u32,
            oh: u32,
            sx: f32,
            sy: f32,
            sampling: &str,
            budget: &mut (u64, u64, u64),
        ) -> Option<Vec<Layer>> {
            let mut out = Vec::with_capacity(layers.len());
            for original in layers {
                let old = placement_of(original, ow, oh)?;
                if original.advanced.is_some() {
                    // Preserve embedded originals and recipes. Scaling a
                    // rotated layer non-uniformly can introduce shear, which
                    // LayerPlacement cannot represent without rasterization.
                    fn exact_scale(p: LayerPlacement, sx: f32, sy: f32) -> Option<LayerPlacement> {
                        let (sin, cos) = p.rotation.to_radians().sin_cos();
                        let dot = sin * cos * (sy * sy - sx * sx);
                        if dot.abs() > 0.00001 * (sx * sx + sy * sy) {
                            return None;
                        }
                        scaled_placement(p, sx, sy)
                    }
                    let new = exact_scale(old, sx, sy)?;
                    let mut layer = shallow_layer(original);
                    let image = layer.image.as_ref()?;
                    layer.offset_x = new.x;
                    layer.offset_y = new.y;
                    layer.rotation = new.rotation;
                    layer.scale_x =
                        new.width / image.width() as f32 * if new.flip_x { -1. } else { 1. };
                    layer.scale_y =
                        new.height / image.height() as f32 * if new.flip_y { -1. } else { 1. };
                    if let Some(mask) = metadata_placement(original, "maskPlacement") {
                        set_metadata_placement(
                            &mut layer,
                            "maskPlacement",
                            exact_scale(mask, sx, sy)?,
                        );
                    }
                    set_metadata_placement(&mut layer, "transform", new);
                    layer.metadata["transform"]["sampling"] = serde_json::json!(sampling);
                    out.push(layer);
                    continue;
                }
                let new = extent(old, sx, sy)?;
                let mut layer = shallow_layer(original);
                if let Some(image) = &original.image {
                    layer.image =
                        Some(plane(image, old, new, sx, sy, false, sampling, budget)?.into());
                    remove_metadata(&mut layer, "text");
                    remove_metadata(&mut layer, "shape");
                    remove_metadata(&mut layer, "compositorRustRasterSource");
                }
                if let Some(mask) = &original.mask {
                    let placed = metadata_placement(original, "maskPlacement");
                    let old_mask = placed.unwrap_or(old);
                    let new_mask = if placed.is_some() {
                        scaled_placement(old_mask, sx, sy)?
                    } else {
                        new
                    };
                    if placed.is_some() {
                        layer.mask = Some(mask.clone());
                        set_metadata_placement(&mut layer, "maskPlacement", new_mask);
                    } else {
                        layer.mask = Some(
                            plane(mask, old_mask, new_mask, sx, sy, true, sampling, budget)?.into(),
                        );
                        remove_metadata(&mut layer, "maskPlacement");
                    }
                }
                layer.offset_x = new.x;
                layer.offset_y = new.y;
                layer.rotation = 0.;
                layer.scale_x = 1.;
                layer.scale_y = 1.;
                set_metadata_placement(&mut layer, "transform", new);
                layer.metadata["transform"]["sampling"] = serde_json::json!(sampling);
                layer.children = resize(&original.children, ow, oh, sx, sy, sampling, budget)?;
                out.push(layer);
            }
            Some(out)
        }
        let Some(layers) = resize(
            &self.document.layers,
            self.document.width,
            self.document.height,
            sx,
            sy,
            sampling,
            &mut (0, 0, 0),
        ) else {
            return false;
        };
        let before = self.snapshot();
        self.document.layers = layers;
        self.document.width = width;
        self.document.height = height;
        if !self.document.metadata.is_object() {
            self.document.metadata = serde_json::json!({});
        }
        self.document.metadata["resolution"] = serde_json::json!(resolution);
        let mut guides = self.guides();
        for guide in &mut guides {
            guide.position *= if guide.axis == GuideAxis::Vertical {
                sx
            } else {
                sy
            };
        }
        if !guides.is_empty() {
            self.document.metadata["guides"]=serde_json::Value::Array(guides.iter().map(|g|serde_json::json!({"id":g.id,"axis":if g.axis==GuideAxis::Vertical{"vertical"}else{"horizontal"},"position":g.position})).collect());
        }
        self.selection = None;
        self.commit(before);
        true
    }
    pub fn select_rect(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.select_rectangle(x as f32, y as f32, width as f32, height as f32);
    }
    /// Create a grayscale mask. An active selection is projected into layer-local pixels.
    pub fn apply_subject_mask(
        &mut self,
        id: &str,
        subject: &image::GrayImage,
        as_selection: bool,
    ) -> anyhow::Result<bool> {
        self.finish_stroke();
        let layer = self
            .document
            .find_layer(id)
            .ok_or_else(|| anyhow::anyhow!("Layer no longer exists"))?;
        let image = layer
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Select an image layer"))?;
        anyhow::ensure!(
            image.dimensions() == subject.dimensions(),
            "Subject mask dimensions changed"
        );
        let transform = Transform::for_layer(&self.document, id)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        if as_selection {
            anyhow::ensure!(
                u64::from(self.document.width) * u64::from(self.document.height) <= 16_777_216,
                "Subject selection supports canvases up to 16 million pixels"
            );
            let mut mask = vec![0; self.document.width as usize * self.document.height as usize];
            for y in 0..self.document.height {
                for x in 0..self.document.width {
                    if let Some((u, v)) = transform.local(x as f32 + 0.5, y as f32 + 0.5) {
                        if u >= 0.
                            && v >= 0.
                            && u < subject.width() as f32
                            && v < subject.height() as f32
                        {
                            mask[y as usize * self.document.width as usize + x as usize] =
                                subject.get_pixel(u as u32, v as u32)[0];
                        }
                    }
                }
            }
            self.selection = Some(Selection {
                width: self.document.width,
                height: self.document.height,
                mask,
            });
            return Ok(true);
        }
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let old_mask = layer
            .mask
            .as_ref()
            .map(|mask| crate::effects::MaskSampler::new(&layer.metadata, mask));
        let mask = RgbaImage::from_fn(subject.width(), subject.height(), |x, y| {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let old = old_mask
                .as_ref()
                .map(|mask| {
                    mask.coverage(
                        wx as f64,
                        wy as f64,
                        x as f64 + 0.5,
                        y as f64 + 0.5,
                        subject.width() as f64,
                        subject.height() as f64,
                    )
                })
                .unwrap_or(1.);
            let v = (subject.get_pixel(x, y)[0] as f32 * old).round() as u8;
            Rgba([v, v, v, 255])
        });
        let before = self.snapshot();
        let layer = self.document.find_layer_mut(id).unwrap();
        layer.mask = Some(mask.into());
        metadata_bool(layer, "maskEnabled", true);
        metadata_bool(layer, "maskLinked", true);
        remove_metadata(layer, "maskPlacement");
        self.commit(before);
        Ok(true)
    }
    pub fn add_mask(&mut self, id: &str, reveal: bool) -> bool {
        self.finish_stroke();
        let Some(layer) = self.document.find_layer(id) else {
            return false;
        };
        if layer.mask.is_some() || locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        let (w, h) = layer
            .image
            .as_ref()
            .map_or((self.document.width, self.document.height), |i| {
                i.dimensions()
            });
        let Some(transform) = Transform::for_layer(&self.document, id) else {
            return false;
        };
        let mut mask = RgbaImage::new(w, h);
        for (x, y, pixel) in mask.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let value = if selected(&self.selection, wx, wy) == reveal {
                255
            } else {
                0
            };
            *pixel = Rgba([value, value, value, 255]);
        }
        let before = self.snapshot();
        let layer = self.document.find_layer_mut(id).unwrap();
        layer.mask = Some(mask.into());
        metadata_bool(layer, "maskEnabled", true);
        self.commit(before);
        true
    }

    /// Replace a layer mask from a mask aligned to the document canvas.
    ///
    /// The source is sampled at layer-local pixel centers after mapping those
    /// centers into canvas space. This deliberately replaces any existing
    /// mask, including an independent mask or a live mask link, so the result
    /// is a single explicit, linked mask transaction.
    pub fn replace_canvas_mask(
        &mut self,
        id: &str,
        canvas_mask: &image::GrayImage,
    ) -> anyhow::Result<bool> {
        anyhow::ensure!(
            self.floating.is_none(),
            "Cannot replace a mask while a floating selection is active"
        );
        anyhow::ensure!(
            crate::model::valid_dimensions(self.document.width, self.document.height),
            "Invalid document dimensions"
        );
        anyhow::ensure!(
            canvas_mask.dimensions() == (self.document.width, self.document.height),
            "Canvas mask dimensions must match the document"
        );
        anyhow::ensure!(
            crate::model::valid_dimensions(canvas_mask.width(), canvas_mask.height()),
            "Invalid canvas mask dimensions"
        );
        anyhow::ensure!(
            u64::from(canvas_mask.width()) * u64::from(canvas_mask.height()) <= 16_777_216,
            "Canvas masks support up to 16 million pixels"
        );

        let layer = self
            .document
            .find_layer(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        if locked_in_tree(&self.document.layers, id, false) {
            return Ok(false);
        }
        let (target_width, target_height) = layer
            .image
            .as_ref()
            .map_or((self.document.width, self.document.height), |image| {
                image.dimensions()
            });
        anyhow::ensure!(
            crate::model::valid_dimensions(target_width, target_height),
            "Invalid target mask dimensions"
        );
        anyhow::ensure!(
            u64::from(target_width) * u64::from(target_height) <= 16_777_216,
            "Target masks support up to 16 million pixels"
        );

        let transform = Transform::for_layer(&self.document, id)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        let determinant = transform.a * transform.d - transform.b * transform.c;
        anyhow::ensure!(
            [
                transform.a,
                transform.b,
                transform.c,
                transform.d,
                transform.tx,
                transform.ty,
                determinant,
            ]
            .iter()
            .all(|value| value.is_finite())
                && determinant.abs() >= 0.000000001,
            "Invalid layer transform"
        );

        fn sample_canvas(mask: &image::GrayImage, x: f32, y: f32) -> u8 {
            if !x.is_finite()
                || !y.is_finite()
                || x < 0.0
                || y < 0.0
                || x >= mask.width() as f32
                || y >= mask.height() as f32
            {
                return 0;
            }
            let px = x - 0.5;
            let py = y - 0.5;
            let ix = px.floor() as i64;
            let iy = py.floor() as i64;
            let fx = px - px.floor();
            let fy = py - py.floor();
            let mut value = 0.0f32;
            for (ox, oy, weight) in [
                (0i64, 0i64, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let sx = ix + ox;
                let sy = iy + oy;
                if sx >= 0
                    && sy >= 0
                    && sx < i64::from(mask.width())
                    && sy < i64::from(mask.height())
                {
                    value += f32::from(mask.get_pixel(sx as u32, sy as u32)[0]) * weight;
                }
            }
            value.round().clamp(0.0, 255.0) as u8
        }

        let projected = image::RgbaImage::from_fn(target_width, target_height, |x, y| {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let value = sample_canvas(canvas_mask, wx, wy);
            image::Rgba([value, value, value, 255])
        });
        let already_explicit = layer.mask.as_ref().is_some_and(|mask| **mask == projected)
            && layer
                .metadata
                .get("maskEnabled")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            && layer
                .metadata
                .get("maskLinked")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            && layer.metadata.get("maskPlacement").is_none()
            && layer.metadata.get("maskSourceID").is_none();
        if already_explicit {
            return Ok(false);
        }

        // Finish any in-progress stroke only after all fallible validation and
        // projection work has succeeded, so invalid input cannot leave a
        // partial mask transaction behind.
        self.finish_stroke();
        let before = self.snapshot();
        let layer = self
            .document
            .find_layer_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        layer.mask = Some(projected.into());
        metadata_bool(layer, "maskEnabled", true);
        metadata_bool(layer, "maskLinked", true);
        remove_metadata(layer, "maskPlacement");
        remove_metadata(layer, "maskSourceID");
        self.commit(before);
        Ok(true)
    }

    pub fn invert_mask(&mut self, id: &str) -> bool {
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        self.change_layer(id, |layer| {
            let Some(mask) = &mut layer.mask else {
                return false;
            };
            for pixel in mask.pixels_mut() {
                for c in 0..3 {
                    pixel[c] = 255 - pixel[c];
                }
            }
            true
        })
    }
    fn mask_work_editor(&self, id: &str) -> anyhow::Result<Editor> {
        anyhow::ensure!(
            !locked_in_tree(&self.document.layers, id, false),
            "Mask layer is locked"
        );
        let layer = self
            .document
            .find_layer(id)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        let mask = layer
            .mask
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Layer has no mask"))?;
        let placement = self
            .mask_placement(id)
            .ok_or_else(|| anyhow::anyhow!("Invalid mask placement"))?;
        let mut work = Layer::paint("Mask", mask.width(), mask.height());
        work.image = Some(mask.into());
        work.offset_x = placement.x;
        work.offset_y = placement.y;
        work.rotation = placement.rotation;
        work.scale_x = placement.width / work.image.as_ref().unwrap().width() as f32
            * if placement.flip_x { -1. } else { 1. };
        work.scale_y = placement.height / work.image.as_ref().unwrap().height() as f32
            * if placement.flip_y { -1. } else { 1. };
        let mut editor = Editor::new(Document {
            width: self.document.width,
            height: self.document.height,
            name: "Mask edit".into(),
            background: [0; 4],
            layers: vec![work],
            metadata: Default::default(),
        });
        editor.selection = self.selection.clone();
        editor.brush = self.brush.clone();
        Ok(editor)
    }
    fn commit_mask_work(&mut self, id: &str, work: Editor, before: Snapshot) -> bool {
        let Some(image) = work
            .document
            .find_layer(&work.active_layer)
            .and_then(|l| l.image.as_ref())
        else {
            return false;
        };
        let normalized = RgbaImage::from_fn(image.width(), image.height(), |x, y| {
            let p = image.get_pixel(x, y);
            let gray = ((0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
                * p[3] as f32
                / 255.)
                .round() as u8;
            Rgba([gray, gray, gray, 255])
        });
        let Some(layer) = self.document.find_layer_mut(id) else {
            return false;
        };
        if layer.mask.as_deref() == Some(&normalized) {
            return false;
        }
        layer.mask = Some(normalized.into());
        self.commit(before);
        true
    }
    pub fn fill_mask_selection(&mut self, id: &str, value: u8) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let changed = work.fill_selection([value, value, value, 255]);
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    /// Apply an image adjustment to a mask, blending its result through the
    /// exact fractional canvas selection before one document-history commit.
    pub fn adjust_mask_selection(
        &mut self,
        id: &str,
        adjustment: Adjustment,
    ) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let selection = work.selection.take();
        let transform = Transform::for_layer(&work.document, &work.active_layer)
            .ok_or_else(|| anyhow::anyhow!("Invalid mask placement"))?;
        let original = work
            .document
            .find_layer(&work.active_layer)
            .and_then(|layer| layer.image.clone())
            .ok_or_else(|| anyhow::anyhow!("Layer has no mask"))?;
        if !work.adjust(adjustment) {
            return Ok(false);
        }
        let adjusted = work
            .document
            .find_layer(&work.active_layer)
            .and_then(|layer| layer.image.clone())
            .unwrap();
        let mut result = original.clone();
        let mut changed = false;
        for (x, y, pixel) in result.enumerate_pixels_mut() {
            let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
            let coverage = selection_coverage(&selection, wx, wy);
            let next = blend_coverage(pixel.0, adjusted.get_pixel(x, y).0, coverage);
            changed |= next != pixel.0;
            pixel.0 = next;
        }
        if !changed {
            return Ok(false);
        }
        work.document
            .find_layer_mut(&work.active_layer)
            .unwrap()
            .image = Some(result.into());
        Ok(self.commit_mask_work(id, work, before))
    }
    pub fn flood_fill_mask(
        &mut self,
        id: &str,
        x: i32,
        y: i32,
        value: u8,
        tolerance: u8,
    ) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let changed = work.fill_at(x, y, [value, value, value, 255], tolerance);
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    pub fn gradient_mask(
        &mut self,
        id: &str,
        start: (f32, f32),
        end: (f32, f32),
        from: u8,
        to: u8,
    ) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let changed = work.gradient(start, end, [from, from, from, 255], [to, to, to, 255]);
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    pub fn retouch_mask_stroke(
        &mut self,
        id: &str,
        points: &[(f32, f32)],
        mode: RetouchMode,
    ) -> anyhow::Result<bool> {
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        let changed = work.retouch_stroke(points, mode)?;
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    pub fn clone_mask_stroke(
        &mut self,
        id: &str,
        source: (f32, f32),
        points: &[(f32, f32)],
        heal: bool,
    ) -> anyhow::Result<bool> {
        let Some(&first) = points.first() else {
            return Ok(false);
        };
        let before = self.snapshot();
        let mut work = self.mask_work_editor(id)?;
        if !work.begin_clone_stroke(source, first, heal) {
            return Ok(false);
        }
        for &point in &points[1..] {
            work.continue_clone_stroke(point);
        }
        let changed = work.finish_clone_stroke();
        Ok(changed && self.commit_mask_work(id, work, before))
    }
    pub fn copy_mask_selection(&self, id: &str) -> anyhow::Result<Option<RgbaImage>> {
        Ok(self.mask_work_editor(id)?.copy_selection())
    }
    pub fn cut_mask_selection(&mut self, id: &str) -> anyhow::Result<bool> {
        self.fill_mask_selection(id, 0)
    }
    pub fn set_mask_enabled(&mut self, id: &str, enabled: bool) -> bool {
        self.change_layer(id, |layer| {
            if layer.mask.is_none()
                || layer
                    .metadata
                    .get("maskEnabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true)
                    == enabled
            {
                return false;
            }
            metadata_bool(layer, "maskEnabled", enabled);
            true
        })
    }
    /// Removing with apply bakes mask coverage into source alpha (paint layers only).
    pub fn remove_mask(&mut self, id: &str, apply: bool) -> bool {
        if locked_in_tree(&self.document.layers, id, false) {
            return false;
        }
        if apply
            && self
                .document
                .find_layer(id)
                .is_some_and(|layer| layer.advanced.is_some())
        {
            return false;
        }
        self.change_layer(id, |layer| {
            let Some(mask) = &layer.mask else {
                return false;
            };
            if apply {
                let Some(image) = &mut layer.image else {
                    return false;
                };
                if mask.width() == 0 || mask.height() == 0 {
                    return false;
                }
                let (width, height) = image.dimensions();
                for (x, y, pixel) in image.enumerate_pixels_mut() {
                    let mx = (u64::from(x) * u64::from(mask.width()) / u64::from(width)) as u32;
                    let my = (u64::from(y) * u64::from(mask.height()) / u64::from(height)) as u32;
                    let m = mask.get_pixel(mx, my);
                    let coverage =
                        (0.2126 * m[0] as f32 + 0.7152 * m[1] as f32 + 0.0722 * m[2] as f32)
                            / 255.0
                            * m[3] as f32
                            / 255.0;
                    pixel[3] = (pixel[3] as f32 * coverage).round() as u8;
                }
            }
            layer.mask = None;
            metadata_bool(layer, "maskEnabled", true);
            true
        })
    }
    /// Merge adjacent normal paint layers; unsupported blend interactions are rejected.
    pub fn merge_down(&mut self) -> bool {
        self.finish_stroke();
        if locked_in_tree(&self.document.layers, &self.active_layer, false) {
            return false;
        }
        fn sibling_pair(layers: &[Layer], id: &str) -> Option<(Layer, Layer)> {
            if let Some(index) = layers.iter().position(|l| l.id == id) {
                return (index > 0).then(|| (layers[index - 1].clone(), layers[index].clone()));
            }
            layers.iter().find_map(|l| sibling_pair(&l.children, id))
        }
        let Some((below, above)) = sibling_pair(&self.document.layers, &self.active_layer) else {
            return false;
        };
        fn merge_parent_is_neutral(layers: &[Layer], id: &str) -> bool {
            for layer in layers {
                if layer.id == id {
                    return true;
                }
                if find(&layer.children, id).is_some() {
                    return layer.visible
                        && layer.opacity == 1.0
                        && (layer.mask.is_none()
                            || layer
                                .metadata
                                .get("maskEnabled")
                                .and_then(serde_json::Value::as_bool)
                                == Some(false))
                        && merge_parent_is_neutral(&layer.children, id);
                }
            }
            false
        }
        if !merge_parent_is_neutral(&self.document.layers, &self.active_layer) {
            return false;
        }

        if below.locked
            || !below.visible
            || !above.visible
            || below.image.is_none()
            || above.image.is_none()
            || !normal_blend(&below.blend_mode)
            || !normal_blend(&above.blend_mode)
        {
            return false;
        }
        let isolated = Document {
            width: self.document.width,
            height: self.document.height,
            name: self.document.name.clone(),
            background: [0; 4],
            layers: vec![below.clone(), above.clone()],
            metadata: self.document.metadata.clone(),
        };
        if !crate::raster::validate(&isolated).is_empty() {
            return false;
        }
        let image = crate::raster::composite(&isolated);
        let mut merged = Layer::paint(
            above.name.clone(),
            self.document.width,
            self.document.height,
        );
        merged.image = Some(image.into());
        let id = merged.id.clone();
        let before = self.snapshot();
        fn replace(layers: &mut Vec<Layer>, id: &str, merged: Layer) -> bool {
            if let Some(index) = layers.iter().position(|l| l.id == id) {
                layers.remove(index);
                layers[index - 1] = merged;
                return true;
            }
            for layer in layers {
                if find(&layer.children, id).is_some() {
                    return replace(&mut layer.children, id, merged);
                }
            }
            false
        }
        replace(&mut self.document.layers, &above.id, merged);
        self.active_layer = id;
        self.commit(before);
        true
    }
    /// Explicitly flatten visible artwork and background into one raster layer.
    pub fn flatten(&mut self) -> bool {
        self.finish_stroke();
        fn any_locked(layers: &[Layer]) -> bool {
            layers.iter().any(|l| l.locked || any_locked(&l.children))
        }
        if any_locked(&self.document.layers) {
            return false;
        }
        if !crate::raster::validate(&self.document).is_empty() {
            return false;
        }
        let image = crate::raster::composite(&self.document);
        let before = self.snapshot();
        let mut layer = Layer::paint("Flattened", self.document.width, self.document.height);
        layer.image = Some(image.into());
        self.active_layer = layer.id.clone();
        self.document.layers = vec![layer];
        self.document.background = [0; 4];
        self.commit(before);
        true
    }
    /// One-dab convenience API; continuous pointer drawing uses the clone stroke methods.
    pub fn clone_stamp(
        &mut self,
        source: (f32, f32),
        destination: (f32, f32),
        radius: f32,
        opacity: f32,
        heal: bool,
    ) -> bool {
        if !radius.is_finite() || radius <= 0.0 || !opacity.is_finite() {
            return false;
        }
        let brush = self.brush.clone();
        self.brush.size = radius.min(2048.0) * 2.0;
        self.brush.opacity = opacity;
        let began = self.begin_clone_stroke(source, destination, heal);
        self.brush = brush;
        began && self.finish_clone_stroke()
    }
    /// Copy the active layer's visible, transformed appearance, cropped to selection bounds.
    /// Elliptical/non-rectangular excluded pixels are transparent. The background is omitted.
    pub fn copy_selection(&self) -> Option<RgbaImage> {
        let layer = self.document.find_layer(&self.active_layer)?.clone();
        let (x, y, width, height) = if let Some(selection) = &self.selection {
            selection.bounds()?
        } else {
            (0, 0, self.document.width, self.document.height)
        };
        let isolated = Document {
            width: self.document.width,
            height: self.document.height,
            name: self.document.name.clone(),
            background: [0; 4],
            layers: vec![layer],
            metadata: self.document.metadata.clone(),
        };
        if !crate::raster::validate(&isolated).is_empty() {
            return None;
        }
        let rendered = crate::raster::composite(&isolated);
        let mut output = RgbaImage::new(width, height);
        for (px, py, pixel) in output.enumerate_pixels_mut() {
            let coverage = selection_coverage(
                &self.selection,
                (px + x) as f32 + 0.5,
                (py + y) as f32 + 0.5,
            );
            if coverage > 0. {
                *pixel = *rendered.get_pixel(px + x, py + y);
                pixel[3] = (f32::from(pixel[3]) * coverage).round() as u8;
            }
        }
        Some(output)
    }
    pub fn floating_selection_layer(&self) -> Option<&str> {
        self.floating
            .as_ref()
            .map(|state| state.floating_id.as_str())
    }
    /// Lift selected pixels into a transient layer. Existing layer transform APIs may edit
    /// that layer; their intermediate history entries are suppressed until commit/cancel.
    pub fn begin_floating_selection(&mut self) -> anyhow::Result<Option<String>> {
        self.finish_stroke();
        if self.floating.is_some() {
            return Ok(None);
        }
        let selection = self
            .selection
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Select pixels before transforming"))?;
        let (x, y, width, height) = selection
            .bounds()
            .ok_or_else(|| anyhow::anyhow!("Selection is empty"))?;
        let source = self
            .document
            .find_layer(&self.active_layer)
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))?;
        anyhow::ensure!(
            source.image.is_some()
                && !is_live_object(source)
                && !locked_in_tree(&self.document.layers, &source.id, false),
            "Selection source is not editable"
        );
        anyhow::ensure!(
            u64::from(width) * u64::from(height) <= 16_777_216
                && source
                    .image
                    .as_ref()
                    .is_some_and(
                        |image| u64::from(image.width()) * u64::from(image.height()) <= 16_777_216
                    ),
            "Floating selections support layers and selections up to 16 million pixels"
        );
        let source_id = source.id.clone();
        let source_pixels = source.image.as_ref().unwrap().clone();
        let before = self.snapshot();
        let before_selection = self.selection.clone();
        let source_transform = Transform::for_layer(&self.document, &source_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        let mut pixels = RgbaImage::new(width, height);
        for (px, py, pixel) in pixels.enumerate_pixels_mut() {
            let Some((u, v)) = source_transform.local((x + px) as f32 + 0.5, (y + py) as f32 + 0.5)
            else {
                continue;
            };
            if u >= 0.
                && v >= 0.
                && u < source_pixels.width() as f32
                && v < source_pixels.height() as f32
            {
                *pixel = *source_pixels.get_pixel(u as u32, v as u32);
            }
        }
        let mut selection_mask = image::GrayImage::new(width, height);
        for (py, px) in (0..height).flat_map(|py| (0..width).map(move |px| (py, px))) {
            let coverage = selection_coverage(
                &self.selection,
                (x + px) as f32 + 0.5,
                (y + py) as f32 + 0.5,
            );
            selection_mask.put_pixel(px, py, image::Luma([(coverage * 255.).round() as u8]));
            let alpha = pixels.get_pixel(px, py)[3];
            pixels.get_pixel_mut(px, py)[3] = (f32::from(alpha) * coverage).round() as u8;
        }
        let transform = Transform::for_layer(&self.document, &source_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid layer transform"))?;
        {
            let image = self
                .document
                .find_layer_mut(&source_id)
                .unwrap()
                .image
                .as_mut()
                .unwrap();
            for (px, py, pixel) in image.enumerate_pixels_mut() {
                let (wx, wy) = transform.world(px as f32 + 0.5, py as f32 + 0.5);
                let coverage = selection_coverage(&self.selection, wx, wy);
                if coverage > 0. {
                    *pixel = Rgba(blend_coverage(pixel.0, [0; 4], coverage));
                }
            }
        }
        let mut floating = Layer::paint("Floating Selection", width, height);
        floating.image = Some(pixels.into());
        floating.offset_x = x as f32;
        floating.offset_y = y as f32;
        let floating_id = floating.id.clone();
        let initial_floating = floating.clone();
        insert_after(&mut self.document.layers, &source_id, floating);
        self.active_layer = floating_id.clone();
        self.floating = Some(FloatingSelection {
            before,
            before_selection,
            source_id,
            floating_id: floating_id.clone(),
            selection_mask,
            initial_floating,
        });
        Ok(Some(floating_id))
    }
    pub fn cancel_floating_selection(&mut self) -> bool {
        let Some(state) = self.floating.take() else {
            return false;
        };
        self.restore(state.before);
        self.selection = state.before_selection;
        true
    }
    pub fn commit_floating_selection(&mut self) -> anyhow::Result<bool> {
        let Some(state) = self.floating.take() else {
            return Ok(false);
        };
        if self
            .document
            .find_layer(&state.floating_id)
            .is_some_and(|layer| {
                layer.image == state.initial_floating.image
                    && layer.offset_x == state.initial_floating.offset_x
                    && layer.offset_y == state.initial_floating.offset_y
                    && layer.rotation == state.initial_floating.rotation
                    && layer.scale_x == state.initial_floating.scale_x
                    && layer.scale_y == state.initial_floating.scale_y
            })
        {
            self.restore(state.before);
            self.selection = state.before_selection;
            return Ok(false);
        }
        let result = (|| -> anyhow::Result<()> {
            let floating = self
                .document
                .find_layer(&state.floating_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Floating layer missing"))?;
            let source = self
                .document
                .find_layer(&state.source_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Source layer missing"))?;
            let source_image = source
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Source pixels missing"))?;
            let floating_image = floating
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Floating pixels missing"))?;
            let source_t = Transform::of(&source, self.document.width, self.document.height);
            let floating_t = Transform::of(&floating, self.document.width, self.document.height);
            let corners = [
                (0., 0.),
                (floating_image.width() as f32, 0.),
                (
                    floating_image.width() as f32,
                    floating_image.height() as f32,
                ),
                (0., floating_image.height() as f32),
            ];
            let local: Vec<_> = corners
                .into_iter()
                .filter_map(|p| {
                    let w = floating_t.world(p.0, p.1);
                    source_t.local(w.0, w.1)
                })
                .collect();
            anyhow::ensure!(local.len() == 4, "Invalid floating transform");
            let min_x = local.iter().map(|p| p.0).fold(0., f32::min).floor() as i32;
            let min_y = local.iter().map(|p| p.1).fold(0., f32::min).floor() as i32;
            let max_x = local
                .iter()
                .map(|p| p.0)
                .fold(source_image.width() as f32, f32::max)
                .ceil() as i32;
            let max_y = local
                .iter()
                .map(|p| p.1)
                .fold(source_image.height() as f32, f32::max)
                .ceil() as i32;
            let (width, height) = ((max_x - min_x) as u32, (max_y - min_y) as u32);
            anyhow::ensure!(
                valid_size(width, height),
                "Floating selection exceeds pixel limits"
            );
            let mut merged = RgbaImage::new(width, height);
            for (py, px) in (0..height).flat_map(|py| (0..width).map(move |px| (py, px))) {
                let (old_x, old_y) = (px as i32 + min_x, py as i32 + min_y);
                let mut out = if old_x >= 0
                    && old_y >= 0
                    && (old_x as u32) < source_image.width()
                    && (old_y as u32) < source_image.height()
                {
                    source_image.get_pixel(old_x as u32, old_y as u32).0
                } else {
                    [0; 4]
                };
                let world = source_t.world(old_x as f32 + 0.5, old_y as f32 + 0.5);
                if let Some((u, v)) = floating_t.local(world.0, world.1)
                    && u >= 0.
                    && v >= 0.
                    && u < floating_image.width() as f32
                    && v < floating_image.height() as f32
                {
                    let src = floating_image.get_pixel(u as u32, v as u32).0;
                    out = alpha_over(src, out);
                }
                merged.put_pixel(px, py, Rgba(out));
            }
            let layer = self.document.find_layer_mut(&state.source_id).unwrap();
            let old_scale = (layer.scale_x, layer.scale_y);
            let (old_rotation, old_mask) = (layer.rotation, layer.mask.clone());
            layer.image = Some(merged.into());
            layer.rotation = old_rotation;
            layer.scale_x = old_scale.0;
            layer.scale_y = old_scale.1;
            let (cx, cy) = (width as f32 / 2., height as f32 / 2.);
            let (a, b, c, d) = (source_t.a, source_t.b, source_t.c, source_t.d);
            let origin = source_t.world(min_x as f32, min_y as f32);
            layer.offset_x = origin.0 - cx * old_scale.0.abs() + a * cx + c * cy;
            layer.offset_y = origin.1 - cy * old_scale.1.abs() + b * cx + d * cy;
            if let Some(mask) = old_mask
                && mask_linked(layer)
            {
                let mut grown = RgbaImage::from_pixel(width, height, Rgba([255; 4]));
                for (py, px) in (0..height).flat_map(|py| (0..width).map(move |px| (py, px))) {
                    let ox = px as i32 + min_x;
                    let oy = py as i32 + min_y;
                    if ox >= 0
                        && oy >= 0
                        && (ox as u32) < source_image.width()
                        && (oy as u32) < source_image.height()
                    {
                        let mx =
                            (ox as u32 * mask.width() / source_image.width()).min(mask.width() - 1);
                        let my = (oy as u32 * mask.height() / source_image.height())
                            .min(mask.height() - 1);
                        grown.put_pixel(px, py, *mask.get_pixel(mx, my));
                    }
                }
                layer.mask = Some(grown.into());
            }
            remove_layer(&mut self.document.layers, &state.floating_id);
            self.active_layer = state.source_id.clone();
            self.selection = Some(selection_from_floating(
                &state.selection_mask,
                floating_t,
                self.document.width,
                self.document.height,
            ));
            Ok(())
        })();
        if let Err(error) = result {
            self.restore(state.before);
            self.selection = state.before_selection;
            return Err(error);
        }
        self.commit(state.before);
        Ok(true)
    }
    /// UI must successfully publish copy_selection() to its clipboard before calling cut.
    pub fn cut_selection(&mut self) -> bool {
        self.finish_stroke();
        if self.editable_layer().is_none() {
            return false;
        }
        if self.selection.is_some() {
            return self.clear_selected_pixels();
        }
        // Pixels outside the canvas were not copied and must not be erased.
        self.select_all();
        let changed = self.clear_selected_pixels();
        self.selection = None;
        changed
    }

    /// Nine-point row-major anchor, with floor rounding matching the original Mac implementation.
    /// Fill is an opaque RGB extension layer, never a replacement document background.
    pub fn resize_canvas_anchored(
        &mut self,
        width: u32,
        height: u32,
        anchor: u8,
        fill: Option<[u8; 4]>,
    ) -> bool {
        if anchor > 8 || !valid_size(width, height) {
            return false;
        }
        let dx = ((i64::from(width) - i64::from(self.document.width)) * (anchor % 3) as i64)
            .div_euclid(2);
        let dy = ((i64::from(height) - i64::from(self.document.height)) * (anchor / 3) as i64)
            .div_euclid(2);
        self.resize_canvas_region(-(dx as i32), -(dy as i32), width, height, fill)
    }
    /// Compute the trim rectangle of visible composite pixels without changing the document.
    pub fn trim_bounds(&self, options: TrimOptions) -> Option<(u32, u32, u32, u32)> {
        if !options.top && !options.bottom && !options.left && !options.right {
            return None;
        }
        if !crate::raster::validate(&self.document).is_empty() {
            return None;
        }
        let image = crate::raster::composite(&self.document);
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return None;
        }
        // Swift compares the premultiplied RGBA raster, including alpha for sampled-color modes.
        let premultiplied = |p: [u8; 4]| -> [u8; 4] {
            let a = u32::from(p[3]);
            [
                (u32::from(p[0]) * a + 127).div_euclid(255) as u8,
                (u32::from(p[1]) * a + 127).div_euclid(255) as u8,
                (u32::from(p[2]) * a + 127).div_euclid(255) as u8,
                p[3],
            ]
        };
        let target = match options.based_on {
            TrimBasedOn::TopLeft => premultiplied(image.get_pixel(0, 0).0),
            TrimBasedOn::BottomRight => premultiplied(image.get_pixel(width - 1, height - 1).0),
            TrimBasedOn::Transparent => [0; 4],
        };
        let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
        for (x, y, pixel) in image.enumerate_pixels() {
            let trim = match options.based_on {
                TrimBasedOn::Transparent => pixel[3] == 0,
                _ => {
                    let p = premultiplied(pixel.0);
                    (0..4).all(|c| p[c].abs_diff(target[c]) <= options.tolerance)
                }
            };
            if !trim {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
        if right == 0 {
            return None;
        }
        if !options.left {
            left = 0;
        }
        if !options.top {
            top = 0;
        }
        if !options.right {
            right = width;
        }
        if !options.bottom {
            bottom = height;
        }
        Some((left, top, right - left, bottom - top))
    }
    pub fn trim_canvas(&mut self, options: TrimOptions) -> bool {
        self.finish_stroke();
        let Some((x, y, width, height)) = self.trim_bounds(options) else {
            return false;
        };
        self.crop_canvas(x as i32, y as i32, width, height)
    }
    fn resize_canvas_region(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        fill: Option<[u8; 4]>,
    ) -> bool {
        self.finish_stroke();
        if self.floating.is_some()
            || !valid_size(width, height)
            || x.unsigned_abs() > 1_000_000
            || y.unsigned_abs() > 1_000_000
            || (x == 0 && y == 0 && width == self.document.width && height == self.document.height)
        {
            return false;
        }
        let (old_width, old_height) = (self.document.width, self.document.height);
        let extension = fill.filter(|_| width > old_width || height > old_height);
        if extension.is_some() {
            fn pixels(layers: &[Layer]) -> u64 {
                layers
                    .iter()
                    .map(|l| {
                        l.image
                            .as_ref()
                            .map_or(0, |i| u64::from(i.width()) * u64::from(i.height()))
                            + l.mask
                                .as_ref()
                                .map_or(0, |i| u64::from(i.width()) * u64::from(i.height()))
                            + pixels(&l.children)
                    })
                    .sum()
            }
            if tree_count(&self.document.layers) >= crate::model::MAX_LAYERS
                || pixels(&self.document.layers)
                    .saturating_add(u64::from(width) * u64::from(height))
                    > crate::model::MAX_PIXELS
            {
                return false;
            }
        }
        fn valid(layers: &[Layer], x: f32, y: f32) -> bool {
            layers.iter().all(|l| {
                let nx = l.offset_x - x;
                let ny = l.offset_y - y;
                nx.is_finite()
                    && ny.is_finite()
                    && nx.abs() <= 1_000_000.0
                    && ny.abs() <= 1_000_000.0
                    && metadata_placement(l, "maskPlacement").is_none_or(|p| {
                        (p.x - x).abs() <= 1_000_000.0 && (p.y - y).abs() <= 1_000_000.0
                    })
                    && valid(&l.children, x, y)
            })
        }
        if !valid(&self.document.layers, x as f32, y as f32) {
            return false;
        }
        let before = self.snapshot();
        self.document.width = width;
        self.document.height = height;
        fn shift(layers: &mut [Layer], x: f32, y: f32) {
            for layer in layers {
                layer.offset_x -= x;
                layer.offset_y -= y;
                if let Some(mut placement) = metadata_placement(layer, "maskPlacement") {
                    placement.x -= x;
                    placement.y -= y;
                    set_metadata_placement(layer, "maskPlacement", placement);
                }
                shift(&mut layer.children, x, y);
            }
        }
        shift(&mut self.document.layers, x as f32, y as f32);
        let mut guides = self.guides();
        for guide in &mut guides {
            guide.position -= match guide.axis {
                GuideAxis::Horizontal => y as f32,
                GuideAxis::Vertical => x as f32,
            };
        }
        set_guides(&mut self.document.metadata, &guides);
        if let Some(mut color) = extension {
            color[3] = 255;
            let mut layer = Layer::paint("Canvas Extension", width, height);
            let image = layer.image.as_mut().unwrap();
            for (px, py, pixel) in image.enumerate_pixels_mut() {
                let ox = i64::from(px) + i64::from(x);
                let oy = i64::from(py) + i64::from(y);
                if ox < 0 || oy < 0 || ox >= i64::from(old_width) || oy >= i64::from(old_height) {
                    *pixel = Rgba(color);
                }
            }
            self.document.layers.insert(0, layer);
        }
        self.selection = None;
        self.commit(before);
        true
    }
    /// Change canvas bounds while preserving layer pixels and transforms.
    pub fn resize_canvas(&mut self, width: u32, height: u32) -> bool {
        self.crop_canvas(0, 0, width, height)
    }
    /// Crop the visible canvas without discarding pixels outside its new bounds.
    pub fn crop_canvas(&mut self, x: i32, y: i32, width: u32, height: u32) -> bool {
        self.resize_canvas_region(x, y, width, height, None)
    }
}

fn valid_size(width: u32, height: u32) -> bool {
    crate::model::valid_dimensions(width, height)
}

fn selected(selection: &Option<Selection>, x: f32, y: f32) -> bool {
    selection
        .as_ref()
        .is_none_or(|s| s.contains(x.floor() as i32, y.floor() as i32))
}
fn stroke_coverage(stroke: &Stroke, index: usize, amount: f32) -> f32 {
    const MAX_TOUCHED: usize = 4_000_000;
    let mut coverage = stroke.coverage.borrow_mut();
    if let Some(previous) = coverage.get_mut(&index) {
        *previous = previous.max(amount);
        return *previous;
    }
    if coverage.len() >= MAX_TOUCHED {
        stroke.overflow.set(true);
        return 0.;
    }
    coverage.insert(index, amount);
    amount
}
fn selection_coverage(selection: &Option<Selection>, x: f32, y: f32) -> f32 {
    let Some(selection) = selection else {
        return 1.0;
    };
    let (x, y) = (x.floor() as i32, y.floor() as i32);
    if x < 0 || y < 0 || x as u32 >= selection.width || y as u32 >= selection.height {
        return 0.0;
    }
    f32::from(selection.mask[y as usize * selection.width as usize + x as usize]) / 255.0
}
fn alpha_over(source: [u8; 4], destination: [u8; 4]) -> [u8; 4] {
    let sa = f32::from(source[3]) / 255.;
    let da = f32::from(destination[3]) / 255.;
    let oa = sa + da * (1. - sa);
    if oa <= 0. {
        return [0; 4];
    }
    let mut output = [0; 4];
    for channel in 0..3 {
        output[channel] = ((f32::from(source[channel]) * sa
            + f32::from(destination[channel]) * da * (1. - sa))
            / oa)
            .round()
            .clamp(0., 255.) as u8;
    }
    output[3] = (oa * 255.).round() as u8;
    output
}
fn selection_from_floating(
    mask: &image::GrayImage,
    transform: Transform,
    width: u32,
    height: u32,
) -> Selection {
    let mut output = vec![0; width as usize * height as usize];
    for y in 0..height {
        for x in 0..width {
            if let Some((u, v)) = transform.local(x as f32 + 0.5, y as f32 + 0.5)
                && u >= 0.
                && v >= 0.
                && u < mask.width() as f32
                && v < mask.height() as f32
            {
                output[y as usize * width as usize + x as usize] =
                    mask.get_pixel(u as u32, v as u32)[0];
            }
        }
    }
    Selection {
        width,
        height,
        mask: output,
    }
}
fn blend_coverage(from: [u8; 4], to: [u8; 4], coverage: f32) -> [u8; 4] {
    if coverage >= 1.0 {
        return to;
    }
    let from_alpha = f32::from(from[3]) / 255.0;
    let to_alpha = f32::from(to[3]) / 255.0;
    let alpha = from_alpha + (to_alpha - from_alpha) * coverage;
    let mut output = [0; 4];
    if alpha > 0.0 {
        for channel in 0..3 {
            let premultiplied = f32::from(from[channel]) / 255.0 * from_alpha * (1.0 - coverage)
                + f32::from(to[channel]) / 255.0 * to_alpha * coverage;
            output[channel] = (premultiplied / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    output[3] = (alpha * 255.0).round() as u8;
    output
}
fn over(destination: [u8; 4], source: [u8; 4], opacity: f32) -> [u8; 4] {
    let sa = source[3] as f32 / 255.0 * opacity.clamp(0.0, 1.0);
    let da = destination[3] as f32 / 255.0;
    let alpha = sa + da * (1.0 - sa);
    if alpha <= 0.0 {
        return [0; 4];
    }
    let mut output = [0; 4];
    for c in 0..3 {
        output[c] = ((source[c] as f32 * sa + destination[c] as f32 * da * (1.0 - sa)) / alpha)
            .round() as u8;
    }
    output[3] = (alpha * 255.0).round() as u8;
    output
}
fn first_paint(layers: &[Layer]) -> Option<String> {
    for layer in layers.iter().rev() {
        if layer.image.is_some() {
            return Some(layer.id.clone());
        }
        if let Some(id) = first_paint(&layer.children) {
            return Some(id);
        }
    }
    None
}
fn find<'a>(layers: &'a [Layer], id: &str) -> Option<&'a Layer> {
    for l in layers {
        if l.id == id {
            return Some(l);
        }
        if let Some(found) = find(&l.children, id) {
            return Some(found);
        }
    }
    None
}
fn locked_in_tree(layers: &[Layer], id: &str, parent_locked: bool) -> bool {
    for layer in layers {
        let locked = parent_locked || layer.locked;
        if layer.id == id {
            return locked;
        }
        if find(&layer.children, id).is_some() {
            return locked_in_tree(&layer.children, id, locked);
        }
    }
    false
}
fn remove_layer(layers: &mut Vec<Layer>, id: &str) -> Option<Layer> {
    if let Some(index) = layers.iter().position(|l| l.id == id) {
        return Some(layers.remove(index));
    }
    for layer in layers {
        if let Some(found) = remove_layer(&mut layer.children, id) {
            return Some(found);
        }
    }
    None
}
fn insert_after(layers: &mut Vec<Layer>, id: &str, copy: Layer) -> bool {
    if let Some(index) = layers.iter().position(|l| l.id == id) {
        layers.insert(index + 1, copy);
        return true;
    }
    for layer in layers {
        if find(&layer.children, id).is_some() {
            return insert_after(&mut layer.children, id, copy);
        }
    }
    false
}
fn regenerate_ids(layer: &mut Layer) {
    fn assign(layer: &mut Layer, map: &mut std::collections::HashMap<String, String>) {
        let old = layer.id.clone();
        layer.id = uuid::Uuid::new_v4().to_string().to_uppercase();
        map.insert(old, layer.id.clone());
        for child in &mut layer.children {
            assign(child, map);
        }
    }
    fn rewrite(layer: &mut Layer, map: &std::collections::HashMap<String, String>) {
        if let Some(source) = layer
            .metadata
            .get("maskSourceID")
            .and_then(serde_json::Value::as_str)
            && let Some(replacement) = map.get(source)
        {
            layer.metadata["maskSourceID"] = serde_json::json!(replacement);
        }
        for child in &mut layer.children {
            rewrite(child, map);
        }
    }
    let mut map = std::collections::HashMap::new();
    assign(layer, &mut map);
    rewrite(layer, &map);
}
/// Count each immutable image allocation once across the supplied layer trees.
fn retained_image_bytes(layers: &[Layer], seen: &mut std::collections::HashSet<usize>) -> usize {
    layers.iter().fold(0usize, |total, layer| {
        let own = [&layer.image, &layer.mask]
            .into_iter()
            .flatten()
            .fold(0usize, |n, image| {
                if seen.insert(image.allocation_id()) {
                    n.saturating_add(image.as_raw().capacity())
                } else {
                    n
                }
            });
        total
            .saturating_add(own)
            .saturating_add(layer.advanced.as_ref().map_or(0, |state| {
                let mut bytes = 0usize;
                if seen.insert(std::sync::Arc::as_ptr(state) as usize) {
                    bytes = std::mem::size_of::<crate::advanced::LayerState>()
                        + state
                            .recipe
                            .nodes
                            .iter()
                            .map(|n| n.name.len() + n.id.len() + 256 + n.owned_bytes())
                            .sum::<usize>();
                    for image in [&state.source, &state.result] {
                        let (w, h) = image.dimensions();
                        let tile = image.tile_size();
                        for y in (0..h).step_by(tile as usize) {
                            for x in (0..w).step_by(tile as usize) {
                                if seen.insert(image.tile_memory_identity(x, y)) {
                                    bytes = bytes.saturating_add(tile as usize * tile as usize * 8);
                                }
                            }
                        }
                    }
                    if let Some(raw) = &state.raw_bytes {
                        if seen.insert(std::sync::Arc::as_ptr(raw) as usize) {
                            bytes = bytes.saturating_add(raw.capacity());
                        }
                    }
                }
                bytes
            }))
            .saturating_add(retained_image_bytes(&layer.children, seen))
    })
}

// Per-snapshot nonshared state; shared raster allocations are charged separately.
fn document_bytes(document: &Document) -> usize {
    fn layer_bytes(layer: &Layer) -> usize {
        layer.children.iter().map(layer_bytes).sum::<usize>()
            + layer.metadata.to_string().len()
            + layer.name.len()
            + 256
    }
    document.layers.iter().map(layer_bytes).sum::<usize>()
        + document.metadata.to_string().len()
        + document.name.len()
        + 256
}
#[derive(Clone, Copy)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}
impl Transform {
    fn of(layer: &Layer, width: u32, height: u32) -> Self {
        let (width, height) = layer
            .image
            .as_ref()
            .map_or((width, height), |i| i.dimensions());
        let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
        let (sin, cos) = layer.rotation.to_radians().sin_cos();
        let (a, b, c, d) = (
            cos * layer.scale_x,
            sin * layer.scale_x,
            -sin * layer.scale_y,
            cos * layer.scale_y,
        );
        Self {
            a,
            b,
            c,
            d,
            tx: layer.offset_x + cx * layer.scale_x.abs() - a * cx - c * cy,
            ty: layer.offset_y + cy * layer.scale_y.abs() - b * cx - d * cy,
        }
    }
    fn for_layer(document: &Document, id: &str) -> Option<Self> {
        document
            .find_layer(id)
            .map(|layer| Self::of(layer, document.width, document.height))
    }
    fn for_mask(document: &Document, id: &str) -> Option<Self> {
        let layer = document.find_layer(id)?;
        let mask = layer.mask.as_ref()?;
        if let Some(p) = metadata_placement(layer, "maskPlacement") {
            let mut placed = Layer::group("");
            placed.offset_x = p.x;
            placed.offset_y = p.y;
            placed.scale_x = p.width / mask.width() as f32 * if p.flip_x { -1. } else { 1. };
            placed.scale_y = p.height / mask.height() as f32 * if p.flip_y { -1. } else { 1. };
            placed.rotation = p.rotation;
            return Some(Self::of(&placed, mask.width(), mask.height()));
        }
        let mut t = Self::for_layer(document, id)?;
        let (w, h) = layer
            .image
            .as_ref()
            .map(|i| i.dimensions())
            .unwrap_or((document.width, document.height));
        let sx = w as f32 / mask.width() as f32;
        let sy = h as f32 / mask.height() as f32;
        t.a *= sx;
        t.b *= sx;
        t.c *= sy;
        t.d *= sy;
        Some(t)
    }
    fn world(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }
    fn local(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 0.000000001 {
            return None;
        }
        let (x, y) = (x - self.tx, y - self.ty);
        Some((
            (self.d * x - self.c * y) / det,
            (-self.b * x + self.a * y) / det,
        ))
    }
    fn inverse_radius(self, radius: f32) -> f32 {
        let det = (self.a * self.d - self.b * self.c).abs();
        // Frobenius norm bounds every inverse-transformed circle, including shear.
        radius * (self.a * self.a + self.b * self.b + self.c * self.c + self.d * self.d).sqrt()
            / det
    }
}
fn flood_pixels(
    image: &RgbaImage,
    x: i32,
    y: i32,
    tolerance: u8,
    mut allowed: impl FnMut((u32, u32)) -> bool,
) -> Vec<(u32, u32)> {
    if x < 0 || y < 0 || x as u32 >= image.width() || y as u32 >= image.height() {
        return Vec::new();
    }
    let target = image.get_pixel(x as u32, y as u32).0;
    let mut visited = vec![false; image.width() as usize * image.height() as usize];
    let mut queue = VecDeque::from([(x as u32, y as u32)]);
    let mut output = Vec::new();
    visited[y as usize * image.width() as usize + x as usize] = true;
    while let Some((x, y)) = queue.pop_front() {
        let pixel = image.get_pixel(x, y).0;
        // Hidden RGB in fully transparent pixels must not split a transparent region.
        let matches = if target[3] == 0 && pixel[3] == 0 {
            true
        } else {
            (0..4).all(|c| pixel[c].abs_diff(target[c]) <= tolerance)
        };
        if !matches || !allowed((x, y)) {
            continue;
        }
        output.push((x, y));
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= image.width() || ny >= image.height() {
                continue;
            }
            let index = ny as usize * image.width() as usize + nx as usize;
            if !visited[index] {
                visited[index] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    output
}

fn metadata_bool(layer: &mut Layer, key: &str, value: bool) {
    if !layer.metadata.is_object() {
        layer.metadata = serde_json::json!({});
    }
    layer.metadata[key] = serde_json::Value::Bool(value);
}
fn restore_placement_bounds(layer: &mut Layer, value: LayerPlacement) {
    let Some(image) = &layer.image else {
        return;
    };
    layer.offset_x = value.x;
    layer.offset_y = value.y;
    layer.rotation = value.rotation;
    layer.scale_x = value.width / image.width() as f32 * if value.flip_x { -1.0 } else { 1.0 };
    layer.scale_y = value.height / image.height() as f32 * if value.flip_y { -1.0 } else { 1.0 };
}
fn resize_placement_preserving_upper_left(
    layer: &mut Layer,
    old: LayerPlacement,
) -> anyhow::Result<()> {
    let image = layer
        .image
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Live text has no cached pixels"))?;
    let scale_x = layer.scale_x.abs();
    let scale_y = layer.scale_y.abs();
    anyhow::ensure!(
        scale_x.is_finite() && scale_x > 0.0 && scale_y.is_finite() && scale_y > 0.0,
        "Invalid text layer scale"
    );
    let mut next = LayerPlacement {
        x: old.x,
        y: old.y,
        width: image.width() as f32 * scale_x,
        height: image.height() as f32 * scale_y,
        rotation: old.rotation,
        flip_x: old.flip_x,
        flip_y: old.flip_y,
    };
    let anchor = old.point(0.0, 0.0);
    let moved = next.point(0.0, 0.0);
    next.x += anchor.0 - moved.0;
    next.y += anchor.1 - moved.1;
    anyhow::ensure!(
        next.is_valid(),
        "Edited text placement exceeds supported bounds"
    );
    restore_placement_bounds(layer, next);
    Ok(())
}
fn parse_guides(metadata: &serde_json::Value) -> Vec<CanvasGuide> {
    metadata
        .get("guides")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?.to_owned();
            let axis = match value.get("axis")?.as_str()? {
                "horizontal" => GuideAxis::Horizontal,
                "vertical" => GuideAxis::Vertical,
                _ => return None,
            };
            let position = value.get("position")?.as_f64()? as f32;
            (position.is_finite() && position.abs() <= 1_000_000.0).then_some(CanvasGuide {
                id,
                axis,
                position,
            })
        })
        .collect()
}
fn set_guides(metadata: &mut serde_json::Value, guides: &[CanvasGuide]) {
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    metadata["guides"] = serde_json::Value::Array(guides.iter().map(|guide| serde_json::json!({
        "id": guide.id, "axis": match guide.axis { GuideAxis::Horizontal => "horizontal", GuideAxis::Vertical => "vertical" },
        "position": guide.position
    })).collect());
}
fn remove_metadata(layer: &mut Layer, key: &str) {
    if let Some(object) = layer.metadata.as_object_mut() {
        object.remove(key);
    }
}
fn is_live_object(layer: &Layer) -> bool {
    layer.advanced.is_some()
        || layer.metadata.get("text").is_some_and(|v| !v.is_null())
        || layer.metadata.get("shape").is_some_and(|v| !v.is_null())
}
fn source_size(layer: &Layer, document_width: u32, document_height: u32) -> (u32, u32) {
    layer
        .image
        .as_ref()
        .map(|i| i.dimensions())
        .unwrap_or((document_width, document_height))
}
fn placement_of(
    layer: &Layer,
    document_width: u32,
    document_height: u32,
) -> Option<LayerPlacement> {
    let (width, height) = source_size(layer, document_width, document_height);
    let value = LayerPlacement {
        x: layer.offset_x,
        y: layer.offset_y,
        width: width as f32 * layer.scale_x.abs(),
        height: height as f32 * layer.scale_y.abs(),
        rotation: layer.rotation,
        flip_x: layer.scale_x < 0.0,
        flip_y: layer.scale_y < 0.0,
    };
    value.is_valid().then_some(value)
}
fn mask_linked(layer: &Layer) -> bool {
    layer
        .metadata
        .get("maskLinked")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}
fn metadata_placement(layer: &Layer, key: &str) -> Option<LayerPlacement> {
    let value = layer.metadata.get(key)?;
    let pair = |key: &str| -> Option<(f32, f32)> {
        let values = value.get(key)?.as_array()?;
        Some((
            values.first()?.as_f64()? as f32,
            values.get(1)?.as_f64()? as f32,
        ))
    };
    let (x, y) = pair("origin")?;
    let (width, height) = pair("size")?;
    let result = LayerPlacement {
        x,
        y,
        width,
        height,
        rotation: value
            .get("rotation")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32,
        flip_x: value
            .get("flipX")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        flip_y: value
            .get("flipY")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    };
    result.is_valid().then_some(result)
}
fn set_metadata_placement(layer: &mut Layer, key: &str, value: LayerPlacement) {
    if !layer.metadata.is_object() {
        layer.metadata = serde_json::json!({});
    }
    let sampling = layer
        .metadata
        .get(key)
        .and_then(|v| v.get("sampling"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!("High quality"));
    layer.metadata[key] = serde_json::json!({ "origin": [value.x, value.y],
        "size": [value.width, value.height], "rotation": value.rotation,
        "flipX": value.flip_x, "flipY": value.flip_y, "sampling": sampling });
}
fn placement_following(
    value: LayerPlacement,
    from: LayerPlacement,
    to: LayerPlacement,
) -> LayerPlacement {
    let map = |point: (f32, f32)| {
        let (sin, cos) = from.rotation.to_radians().sin_cos();
        let center = from.center();
        let dx = point.0 - center.0;
        let dy = point.1 - center.1;
        let mut ux = (dx * cos + dy * sin) / from.width + 0.5;
        let mut uy = (-dx * sin + dy * cos) / from.height + 0.5;
        if from.flip_x {
            ux = 1.0 - ux;
        }
        if from.flip_y {
            uy = 1.0 - uy;
        }
        to.point(ux, uy)
    };
    let center = map(value.center());
    let x0 = map(value.point(0.0, 0.5));
    let x1 = map(value.point(1.0, 0.5));
    let y0 = map(value.point(0.5, 0.0));
    let y1 = map(value.point(0.5, 1.0));
    let vx = (x1.0 - x0.0, x1.1 - x0.1);
    let vy = (y1.0 - y0.0, y1.1 - y0.1);
    let width = vx.0.hypot(vx.1).max(1.0);
    let height = vy.0.hypot(vy.1).max(1.0);
    let flip_x = value.flip_x ^ from.flip_x ^ to.flip_x;
    let flip_y = value.flip_y ^ from.flip_y ^ to.flip_y;
    let mut rotation = vx.1.atan2(vx.0).to_degrees();
    if flip_x {
        rotation += 180.0;
    }
    LayerPlacement {
        x: center.0 - width * 0.5,
        y: center.1 - height * 0.5,
        width,
        height,
        rotation,
        flip_x,
        flip_y,
    }
}
fn carry_mask_placement(layer: &mut Layer, old: LayerPlacement, new: LayerPlacement) {
    let Some(mask) = &layer.mask else {
        return;
    };
    if mask.width() <= 1 && mask.height() <= 1 {
        remove_metadata(layer, "maskPlacement");
        return;
    }
    let explicit = metadata_placement(layer, "maskPlacement");
    let moved = if mask_linked(layer) {
        explicit.map(|p| placement_following(p, old, new))
    } else {
        Some(explicit.unwrap_or(old))
    };
    match moved {
        Some(value) if value != new => set_metadata_placement(layer, "maskPlacement", value),
        _ => remove_metadata(layer, "maskPlacement"),
    }
}
fn quad_is_usable(corners: [(f32, f32); 4]) -> bool {
    let cross = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
    };
    let values = [
        cross(corners[0], corners[1], corners[2]),
        cross(corners[1], corners[2], corners[3]),
        cross(corners[2], corners[3], corners[0]),
        cross(corners[3], corners[0], corners[1]),
    ];
    values.iter().all(|v| *v > 0.001) || values.iter().all(|v| *v < -0.001)
}
fn quad_uv(corners: [(f32, f32); 4], point: (f32, f32)) -> Option<(f32, f32)> {
    let p0 = corners[0];
    let e = (corners[1].0 - p0.0, corners[1].1 - p0.1);
    let f = (corners[3].0 - p0.0, corners[3].1 - p0.1);
    let g = (
        p0.0 - corners[1].0 + corners[2].0 - corners[3].0,
        p0.1 - corners[1].1 + corners[2].1 - corners[3].1,
    );
    let det = e.0 * f.1 - e.1 * f.0;
    let mut u = if det.abs() > 1e-8 {
        ((point.0 - p0.0) * f.1 - (point.1 - p0.1) * f.0) / det
    } else {
        0.5
    };
    let mut v = if det.abs() > 1e-8 {
        (e.0 * (point.1 - p0.1) - e.1 * (point.0 - p0.0)) / det
    } else {
        0.5
    };
    for _ in 0..8 {
        let x = p0.0 + e.0 * u + f.0 * v + g.0 * u * v - point.0;
        let y = p0.1 + e.1 * u + f.1 * v + g.1 * u * v - point.1;
        let du = (e.0 + g.0 * v, e.1 + g.1 * v);
        let dv = (f.0 + g.0 * u, f.1 + g.1 * u);
        let jacobian = du.0 * dv.1 - du.1 * dv.0;
        if jacobian.abs() < 1e-8 {
            return None;
        }
        u -= (x * dv.1 - y * dv.0) / jacobian;
        v -= (du.0 * y - du.1 * x) / jacobian;
    }
    (u >= -0.001 && u <= 1.001 && v >= -0.001 && v <= 1.001)
        .then_some((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)))
}
fn warp_quad(
    image: &RgbaImage,
    corners: [(f32, f32); 4],
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    flip_x: bool,
    flip_y: bool,
) -> RgbaImage {
    let mut output = RgbaImage::new(width, height);
    for (px, py, target) in output.enumerate_pixels_mut() {
        let Some((mut u, mut v)) = quad_uv(corners, (x + px as f32 + 0.5, y + py as f32 + 0.5))
        else {
            continue;
        };
        if flip_x {
            u = 1.0 - u;
        }
        if flip_y {
            v = 1.0 - v;
        }
        let sx = (u * image.width() as f32)
            .floor()
            .min(image.width().saturating_sub(1) as f32) as u32;
        let sy = (v * image.height() as f32)
            .floor()
            .min(image.height().saturating_sub(1) as f32) as u32;
        *target = *image.get_pixel(sx, sy);
    }
    output
}
fn normal_blend(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("normal") || mode.eq_ignore_ascii_case("source-over")
}

fn shallow_layer(layer: &Layer) -> Layer {
    Layer {
        id: layer.id.clone(),
        name: layer.name.clone(),
        visible: layer.visible,
        locked: layer.locked,
        opacity: layer.opacity,
        blend_mode: layer.blend_mode.clone(),
        offset_x: layer.offset_x,
        offset_y: layer.offset_y,
        rotation: layer.rotation,
        scale_x: layer.scale_x,
        scale_y: layer.scale_y,
        image: layer.image.clone(),
        mask: None,
        advanced: layer.advanced.clone(),
        children: Vec::new(),
        metadata: layer.metadata.clone(),
    }
}

fn selected_roots(layers: &[Layer], ids: &[String]) -> Option<Vec<String>> {
    let selected: std::collections::HashSet<_> = ids.iter().map(String::as_str).collect();
    if selected.iter().any(|id| find(layers, id).is_none()) {
        return None;
    }
    fn walk(layers: &[Layer], selected: &std::collections::HashSet<&str>, out: &mut Vec<String>) {
        for layer in layers {
            if selected.contains(layer.id.as_str()) {
                out.push(layer.id.clone());
            } else {
                walk(&layer.children, selected, out);
            }
        }
    }
    let mut roots = Vec::new();
    walk(layers, &selected, &mut roots);
    Some(roots)
}
fn tree_count(layers: &[Layer]) -> usize {
    layers.iter().map(|l| 1 + tree_count(&l.children)).sum()
}
fn tree_depth(layer: &Layer) -> usize {
    1 + layer.children.iter().map(tree_depth).max().unwrap_or(0)
}
fn roots_editable(layers: &[Layer], roots: &[String]) -> bool {
    fn unlocked(layer: &Layer) -> bool {
        !layer.locked && layer.children.iter().all(unlocked)
    }
    roots
        .iter()
        .all(|id| !locked_in_tree(layers, id, false) && find(layers, id).is_some_and(unlocked))
}
fn tree_path(layers: &[Layer], id: &str) -> Option<Vec<String>> {
    for layer in layers {
        if layer.id == id {
            return Some(vec![id.to_owned()]);
        }
        if let Some(mut path) = tree_path(&layer.children, id) {
            path.insert(0, layer.id.clone());
            return Some(path);
        }
    }
    None
}
fn valid_parent(layers: &[Layer], parent: Option<&str>) -> Option<usize> {
    let Some(id) = parent else {
        return Some(0);
    };
    let layer = find(layers, id)?;
    if !layer.is_group() || locked_in_tree(layers, id, false) {
        return None;
    }
    Some(tree_path(layers, id)?.len())
}
fn target_siblings_mut<'a>(
    layers: &'a mut Vec<Layer>,
    parent: Option<&str>,
) -> Option<&'a mut Vec<Layer>> {
    let Some(id) = parent else {
        return Some(layers);
    };
    for layer in layers {
        if layer.id == id {
            return Some(&mut layer.children);
        }
        if find(&layer.children, id).is_some() {
            return target_siblings_mut(&mut layer.children, Some(id));
        }
    }
    None
}
fn descendant_set(layers: &[Layer], roots: &[String]) -> std::collections::HashSet<String> {
    fn collect(layer: &Layer, out: &mut std::collections::HashSet<String>) {
        out.insert(layer.id.clone());
        for child in &layer.children {
            collect(child, out);
        }
    }
    let mut out = std::collections::HashSet::new();
    for id in roots {
        if let Some(layer) = find(layers, id) {
            collect(layer, &mut out);
        }
    }
    out
}
fn hierarchy_signature(layers: &[Layer]) -> Vec<(String, Option<String>)> {
    fn walk(layers: &[Layer], parent: Option<&str>, out: &mut Vec<(String, Option<String>)>) {
        for layer in layers {
            out.push((layer.id.clone(), parent.map(str::to_owned)));
            walk(&layer.children, Some(&layer.id), out);
        }
    }
    let mut out = Vec::new();
    walk(layers, None, &mut out);
    out
}
fn regenerate_forest_ids(layers: &mut [Layer]) -> std::collections::HashMap<String, String> {
    fn assign(layers: &mut [Layer], map: &mut std::collections::HashMap<String, String>) {
        for layer in layers {
            let old = layer.id.clone();
            layer.id = uuid::Uuid::new_v4().to_string().to_uppercase();
            map.insert(old, layer.id.clone());
            assign(&mut layer.children, map);
        }
    }
    fn rewrite(layers: &mut [Layer], map: &std::collections::HashMap<String, String>) {
        for layer in layers {
            if let Some(source) = layer
                .metadata
                .get("maskSourceID")
                .and_then(serde_json::Value::as_str)
                && let Some(replacement) = map.get(source)
            {
                layer.metadata["maskSourceID"] = serde_json::json!(replacement);
            }
            rewrite(&mut layer.children, map);
        }
    }
    let mut map = std::collections::HashMap::new();
    assign(layers, &mut map);
    rewrite(layers, &map);
    map
}

fn tree_pixels(layers: &[Layer]) -> u64 {
    layers.iter().map(layer_pixels).sum()
}
fn layer_pixels(layer: &Layer) -> u64 {
    layer
        .image
        .as_ref()
        .map_or(0, |i| u64::from(i.width()) * u64::from(i.height()))
        + layer
            .mask
            .as_ref()
            .map_or(0, |i| u64::from(i.width()) * u64::from(i.height()))
        + tree_pixels(&layer.children)
}

/// Admission checks for a new top-level layer tree. Package I/O repeats the
/// compatibility checks when writing, but imports must reject malformed or
/// over-budget content before it enters the live editor and history snapshots.
fn valid_insert_tree(layer: &Layer) -> bool {
    let bitmap_is_valid = |bitmap: &crate::shared_image::SharedImage| {
        crate::model::valid_dimensions(bitmap.width(), bitmap.height())
    };
    layer.name.trim().len() > 0
        && layer.name.len() <= 16_384
        && layer.opacity.is_finite()
        && (0.0..=1.0).contains(&layer.opacity)
        && [
            layer.offset_x,
            layer.offset_y,
            layer.rotation,
            layer.scale_x,
            layer.scale_y,
        ]
        .iter()
        .all(|value| value.is_finite())
        && layer.offset_x.abs() <= 1_000_000.0
        && layer.offset_y.abs() <= 1_000_000.0
        && layer.scale_x != 0.0
        && layer.scale_y != 0.0
        && (!layer.is_group() || layer.image.is_none())
        && (layer.image.is_none() || layer.children.is_empty())
        && layer.image.as_ref().is_none_or(bitmap_is_valid)
        && layer.mask.as_ref().is_none_or(|mask| {
            bitmap_is_valid(mask)
                && mask
                    .pixels()
                    .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2] && pixel[3] == 255)
        })
        && layer.children.iter().all(valid_insert_tree)
}

fn clip_source(layer: &Layer) -> Option<&str> {
    layer
        .metadata
        .get("maskSourceID")
        .and_then(serde_json::Value::as_str)
}
fn contiguous_clips(layers: &[Layer]) -> std::collections::HashSet<String> {
    fn walk(layers: &[Layer], out: &mut std::collections::HashSet<String>) {
        let mut base: Option<&str> = None;
        for layer in layers {
            if let Some(source) = clip_source(layer) {
                if Some(source) == base {
                    out.insert(layer.id.clone());
                } else {
                    base = Some(&layer.id);
                }
            } else {
                base = if layer.is_group() {
                    None
                } else {
                    Some(&layer.id)
                };
            }
            walk(&layer.children, out);
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(layers, &mut out);
    out
}
/// Drag/drop clipping follows the Mac contiguous-stack rules, with one deliberate
/// safety refinement: pre-existing arbitrary live-mask links outside a contiguous
/// stack survive unrelated moves. The source app clears all such links globally.
fn reconcile_drop_clipping(
    layers: &mut [Layer],
    inserted: &[String],
    mut candidates: std::collections::HashSet<String>,
) -> Result<bool, ()> {
    let inserted: std::collections::HashSet<&str> = inserted.iter().map(String::as_str).collect();
    fn adopt(
        layers: &mut [Layer],
        inserted: &std::collections::HashSet<&str>,
        candidates: &mut std::collections::HashSet<String>,
        parent_locked: bool,
    ) -> Result<bool, ()> {
        let mut changed = false;
        let mut index = 0;
        while index < layers.len() {
            if !inserted.contains(layers[index].id.as_str()) || layers[index].is_group() {
                index += 1;
                continue;
            }
            let first = index;
            while index < layers.len()
                && inserted.contains(layers[index].id.as_str())
                && !layers[index].is_group()
            {
                index += 1;
            }
            let end = index;
            if first == 0 || end >= layers.len() {
                continue;
            }
            let Some(source) = clip_source(&layers[end]).map(str::to_owned) else {
                continue;
            };
            if layers[first - 1].id != source
                && clip_source(&layers[first - 1]) != Some(source.as_str())
            {
                continue;
            }
            for layer in &mut layers[first..end] {
                if layer.id == source || clip_source(layer) == Some(source.as_str()) {
                    continue;
                }
                if parent_locked || layer.locked {
                    return Err(());
                }
                if !layer.metadata.is_object() {
                    layer.metadata = serde_json::json!({});
                }
                layer.metadata["maskSourceID"] = serde_json::json!(source);
                candidates.insert(layer.id.clone());
                changed = true;
            }
        }
        for layer in layers {
            changed |= adopt(
                &mut layer.children,
                inserted,
                candidates,
                parent_locked || layer.locked,
            )?;
        }
        Ok(changed)
    }
    fn release(
        layers: &mut [Layer],
        candidates: &std::collections::HashSet<String>,
        parent_locked: bool,
    ) -> Result<bool, ()> {
        let mut base: Option<String> = None;
        let mut changed = false;
        for layer in layers {
            if let Some(source) = clip_source(layer).map(str::to_owned) {
                if base.as_deref() != Some(source.as_str()) {
                    if candidates.contains(&layer.id) {
                        if parent_locked || layer.locked {
                            return Err(());
                        }
                        remove_metadata(layer, "maskSourceID");
                        changed = true;
                    }
                    base = Some(layer.id.clone());
                }
            } else {
                base = if layer.is_group() {
                    None
                } else {
                    Some(layer.id.clone())
                };
            }
            changed |= release(
                &mut layer.children,
                candidates,
                parent_locked || layer.locked,
            )?;
        }
        Ok(changed)
    }
    let changed = adopt(layers, &inserted, &mut candidates, false)?;
    Ok(release(layers, &candidates, false)? || changed)
}
