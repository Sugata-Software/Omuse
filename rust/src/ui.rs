#[cfg(all(test, feature = "ui-test"))]
#[path = "mask_inspection_ui_tests.rs"]
mod mask_inspection_ui_tests;
#[cfg(all(test, feature = "ui-test"))]
#[path = "photo_exchange_tests.rs"]
mod photo_exchange_tests;
#[cfg(all(test, feature = "ui-test"))]
#[path = "photo_reliability_ui_tests.rs"]
mod photo_reliability_ui_tests;

use crate::display_surface::{DisplaySurface, DisplayTile};
#[path = "advanced_ui.rs"]
mod advanced_ui;
#[path = "ai_ui.rs"]
mod ai_ui;
#[path = "asset_ui.rs"]
mod asset_ui;
#[path = "camera_gesture_ui.rs"]
mod camera_gesture_ui;
#[path = "camera_preview_ui.rs"]
mod camera_preview_ui;
#[path = "clipboard_ui.rs"]
mod clipboard_ui;
#[path = "command_search_ui.rs"]
mod command_search_ui;
#[path = "control_style.rs"]
mod control_style;
#[path = "create_design_ui.rs"]
mod create_design_ui;
#[path = "create_previews.rs"]
mod create_previews;
#[path = "create_ui.rs"]
mod create_ui;
#[path = "crop_ui.rs"]
mod crop_ui;
#[cfg(all(test, feature = "ui-test"))]
#[path = "editing_workflow_ui_tests.rs"]
mod editing_workflow_ui_tests;
#[path = "external_change_ui.rs"]
mod external_change_ui;
#[path = "finishing_ui.rs"]
mod finishing_ui;
#[path = "image_trace_ui.rs"]
mod image_trace_ui;
#[path = "inline_font_ui.rs"]
mod inline_font_ui;
#[path = "inline_text_ui.rs"]
mod inline_text_ui;
#[path = "inspector_ui.rs"]
mod inspector_ui;
#[path = "keyboard_ui.rs"]
mod keyboard_ui;
#[path = "layer_actions_ui.rs"]
mod layer_actions_ui;
#[path = "motion_ui.rs"]
mod motion_ui;
#[path = "numeric_ui.rs"]
mod numeric_ui;
#[path = "photo_io_ui.rs"]
mod photo_io_ui;
#[path = "pointer_path.rs"]
mod pointer_path;
#[path = "product_ui.rs"]
mod product_ui;
#[cfg(all(test, feature = "ui-test"))]
#[path = "project_extension_ui_tests.rs"]
mod project_extension_ui_tests;
#[path = "range_ui.rs"]
mod range_ui;
#[path = "recent_ui.rs"]
mod recent_ui;
#[path = "restore_ui.rs"]
mod restore_ui;
#[path = "retouch_ui.rs"]
mod retouch_ui;
#[path = "rich_text_ui.rs"]
mod rich_text_ui;
#[path = "selection_outline_ui.rs"]
mod selection_outline_ui;
#[path = "svg_import_ui.rs"]
mod svg_import_ui;
use selection_outline_ui::SelectionContourCache;
#[path = "jpeg_preview_ui.rs"]
mod jpeg_preview_ui;
#[cfg(test)]
#[path = "startup_preparation_tests.rs"]
mod startup_preparation_tests;
#[cfg(all(test, feature = "ui-test"))]
#[path = "studio_tests.rs"]
mod studio_tests;
#[path = "studio_ui.rs"]
mod studio_ui;
#[cfg(all(test, feature = "ui-test"))]
#[path = "subject_refine_ui_tests.rs"]
mod subject_refine_ui_tests;
#[path = "tablet_ui.rs"]
mod tablet_ui;
#[path = "vector_ui.rs"]
mod vector_ui;
#[path = "workflow_ui.rs"]
mod workflow_ui;
use crate::recovery::Recovery;
use crate::shortcuts::{self, Shortcuts};
use crate::transform_interaction::{
    CanvasPoint, DragMode, DragModifiers, HitTarget, LayerSelection, SelectionAction,
    TransformDrag, TransformGeometry, commit_drag, selection_bounds,
};
use control_style::{button, color_picker, control_radius, input, textarea};
use gpui_kit::{
    AnyElement, App, AppContext, BorderStyle, Bounds, ClipboardEntry, ClipboardItem, Context,
    Corners, Entity, ExternalPaths, FocusHandle, Image, ImageFormat, KeyBinding, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, Point,
    RenderImage, Rgba, ScrollWheelEvent, SharedString, Size, Window, actions,
    base::{
        ColorPickerState, FocusTrapElement,
        input::{InputEvent, InputState, TextareaState},
    },
    canvas, div, fill, outline, point,
    prelude::*,
    px, rgb, rgba, size,
};
use gpui_omarchy::{ActiveTheme, ButtonVariant, focus_scope};
use omuse::{
    document,
    editor::{Adjustment, Editor, PaintTool, Selection},
    filters::Filter,
    gradient_tools::{GradientKind, GradientSettings},
    model::{Document, Layer, PixelRect},
    objects::{self, Shape, TextStyle},
    raster,
    selection_tools::{SelectionMode, WandSettings},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

actions!(
    omuse,
    [
        New, Open, Save, SaveAs, Export, Import, Undo, Redo, Delete, Duplicate, SelectAll,
        Deselect, ZoomIn, ZoomOut, Fit, Quit, Copy, Cut, Paste
    ]
);

#[derive(Clone, PartialEq, gpui_kit::Action)]
#[action(namespace = omuse, no_json)]
pub struct Command {
    name: String,
}

pub fn bind_keys(cx: &mut App) {
    let settings = Shortcuts::load(&shortcuts::settings_path()).unwrap_or_default();
    install_shortcuts(&settings, &Shortcuts::default(), cx);
    ai_ui::bind_ai_keys(cx);
}
fn install_shortcuts(settings: &Shortcuts, previous: &Shortcuts, cx: &mut App) {
    // Shadow only editor bindings; preserve GPUI text-input and dialog bindings.
    for (id, _, default) in shortcuts::DEFINITIONS {
        let context = if matches!(*id, "command-search" | "quit") {
            "Omuse"
        } else {
            "Omuse && !Input"
        };
        for chord in [*default, previous.chord(id)] {
            if chord.is_empty() {
                continue;
            }
            cx.bind_keys([KeyBinding::new(chord, gpui_kit::NoAction, Some(context))]);
        }
    }
    for (id, _, _) in shortcuts::DEFINITIONS {
        if settings.chord(id).is_empty() {
            continue;
        }
        cx.bind_keys([KeyBinding::new(
            settings.chord(id),
            Command { name: (*id).into() },
            Some(if matches!(*id, "command-search" | "quit") {
                "Omuse"
            } else {
                "Omuse && !Input"
            }),
        )]);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tool {
    Brush,
    Pencil,
    Eraser,
    Fill,
    Gradient,
    Rectangle,
    Ellipse,
    Move,
    Picker,
    Clone,
    Heal,
    SpotHeal,
    Wand,
    Object,
    ShapeRect,
    ShapeEllipse,
    Line,
    Lasso,
    Hand,
    BlurBrush,
    Smudge,
    Liquify,
    Text,
}
impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Pencil => "Pencil",
            Self::Eraser => "Eraser",
            Self::Fill => "Fill",
            Self::Gradient => "Gradient",
            Self::Rectangle => "Rectangle select",
            Self::Ellipse => "Ellipse select",
            Self::Move => "Move layer",
            Self::Picker => "Sample color",
            Self::Clone => "Clone stamp",
            Self::Heal => "Healing clone",
            Self::SpotHeal => "Spot healing",
            Self::Wand => "Magic wand",
            Self::Object => "Connected subject",
            Self::ShapeRect => "Rectangle shape",
            Self::ShapeEllipse => "Ellipse shape",
            Self::Line => "Line",
            Self::Lasso => "Lasso",
            Self::Hand => "Hand",
            Self::BlurBrush => "Blur brush",
            Self::Smudge => "Smudge",
            Self::Liquify => "Liquify",
            Self::Text => "Type",
        }
    }
}
#[derive(Clone)]
enum Pending {
    New,
    Open,
    Quit,
    OpenPath(PathBuf),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Dialog {
    None,
    Unsaved,
    Save,
    Export,
    New,
    Rename,
    Resize,
    Open,
    Import,
    Recover,
    Filter,
    Finishing,
    Text,
    LayerMenu,
    Nest,
    Trim,
    Shortcuts,
    CommandSearch,
    Recent,
    ExternalChange,
    Transform,
    ResizeImage,
    Shape,
    Effects,
    MaskTransform,
    Guide,
    Distort,
    LiveMask,
    Adjustment,
    Selection,
    CameraRaw,
    RawImport,
    SvgImport,
    SubjectRefine,
    RangeMask,
    VectorPath,
    Workflow,
    Pro,
    ImportReport,
    ExportReport,
    ToolSettings,
    Gradient,
}

struct SaveConfirmation {
    path: PathBuf,
    stamp: u64,
    dialog_generation: u64,
}

struct InlineTextDraft {
    layer: Option<String>,
    style: objects::LiveTextStyle,
    origin: (f32, f32),
    input: Entity<TextareaState>,
    color: Entity<ColorPickerState>,
    color_edit: Option<inline_text_ui::TextColorEdit>,
    typing_color: Option<[f32; 4]>,
    typing_font: Option<String>,
    font_edit: Option<inline_font_ui::FontEdit>,
    font_search: Entity<InputState>,
    last_selection: std::ops::Range<usize>,
    history: std::collections::VecDeque<objects::LiveTextStyle>,
    restore_text_history: bool,
}

/// Own the images actually submitted to this canvas. Keeping one displayed
/// generation avoids retaining intermediate previews that were never painted.
/// Offscreen and superseded textures are evicted before the next submission.
#[derive(Default)]
struct CanvasTextures {
    images: Vec<Arc<RenderImage>>,
}

impl CanvasTextures {
    fn retain_visible(&mut self, images: Vec<Arc<RenderImage>>, window: &mut Window, cx: &mut App) {
        let retained: std::collections::HashSet<_> = images.iter().map(|image| image.id).collect();
        for previous in std::mem::replace(&mut self.images, images) {
            if !retained.contains(&previous.id) {
                cx.drop_image(previous, Some(window));
            }
        }
    }

    fn release(&mut self, cx: &mut App) {
        for image in self.images.drain(..) {
            cx.drop_image(image, None);
        }
    }
}

pub struct EditorView {
    editor: Editor,
    path: Option<PathBuf>,
    display: DisplaySurface,
    mask_preview: RefCell<
        Option<(
            u64,
            String,
            u64,
            usize,
            omuse::editor::LayerPlacement,
            DisplaySurface,
        )>,
    >,
    canvas_textures: Rc<RefCell<CanvasTextures>>,
    layer_thumbnails: RefCell<crate::studio_thumbnails::LayerThumbnails>,
    // Used only by the disposable native display comparison journey.
    display_reference: Option<Arc<RenderImage>>,
    display_probe_matte: Option<u32>,
    pixels: image::RgbaImage,
    pending_stroke_frame: Option<u64>,
    stroke_frame_generation: u64,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    tool: Tool,
    inspector_tab: studio_ui::InspectorTab,
    inspector_visible: bool,
    create: create_ui::CreateState,
    ai: ai_ui::AiState,
    product: product_ui::ProductUiState,
    asset_ui: asset_ui::AssetUiState,
    spot_healing_mode: omuse::spot_heal::SpotHealingMode,
    zoom: f32,
    pan: (f32, f32),
    crop: Option<omuse::crop::CropFrame>,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    drag_start: Option<(f32, f32)>,
    selection_box: Option<(f32, f32, f32, f32)>,
    status: String,
    dialog: Dialog,
    pending: Option<Pending>,
    path_input: Entity<InputState>,
    width_input: Entity<InputState>,
    height_input: Entity<InputState>,
    color: Entity<ColorPickerState>,
    dialog_color_picker: Entity<ColorPickerState>,
    dialog_color: [u8; 4],
    background: Entity<ColorPickerState>,
    background_color: [u8; 4],
    fill_tolerance: u8,
    text_origin: Option<(f32, f32)>,
    inline_text: Option<InlineTextDraft>,
    inline_preview: inline_text_ui::InlinePreview,
    text_hit_pending: Option<String>,
    font_names: Option<Vec<String>>,
    shape_corner_radius: f32,
    shape_line_width: f32,
    resize_resolution: f64,
    resize_sampling: usize,
    canvas_anchor: u8,
    canvas_fill: usize,
    canvas_units: usize,
    canvas_relative: bool,
    canvas_lock_aspect: bool,
    trim_options: omuse::editor::TrimOptions,
    collapsed_groups: std::collections::HashSet<String>,
    gradient_settings: GradientSettings,
    gradient_pending: Option<((f32, f32), (f32, f32))>,
    recovery: Recovery,
    photo_io: Option<photo_io_ui::PhotoIoJob>,
    busy: bool,
    live_stamp: Option<u64>,
    filter_kind: usize,
    filter_inputs: [Entity<InputState>; 3],
    clone_source: Option<(f32, f32)>,
    clone_offset: Option<(f32, f32)>,
    clone_aligned: bool,
    clone_all_layers: bool,
    clone_aligned_draft: bool,
    clone_all_layers_draft: bool,
    dialog_generation: u64,
    save_confirmation: Option<SaveConfirmation>,
    save_again: bool,
    jpeg_preview: Option<(Arc<RenderImage>, usize, u32, u32)>,
    jpeg_inspection: jpeg_preview_ui::JpegInspection,
    // Preview work has its own lifecycle.  It must never invalidate a submitted
    // file operation, whose completion is guarded by `dialog_generation`.
    jpeg_preview_generation: u64,
    jpeg_preview_task: Option<u64>,
    lasso: Vec<(f32, f32)>,
    pointer_path_error: Option<&'static str>,
    pan_pointer: Option<Point<Pixels>>,
    middle_pan_pointer: Option<Point<Pixels>>,
    space_down: bool,
    shortcuts: Shortcuts,
    shortcut_draft: Shortcuts,
    recording: Option<String>,
    command_search: command_search_ui::CommandSearchState,
    recent: recent_ui::RecentState,
    external: external_change_ui::ExternalState,
    numeric: RefCell<numeric_ui::NumericState>,
    show_grid: bool,
    effect_kind: usize,
    live_filter: bool,
    adjustment_kind: usize,
    selection_operation: i32,
    selection_mode: SelectionMode,
    wand_settings: WandSettings,
    wand_draft: WandSettings,
    selection_mode_draft: SelectionMode,
    selection_before_gesture: Option<Selection>,
    gesture_selection_mode: SelectionMode,
    raw_open: bool,
    camera_section: usize,
    camera_curve: Entity<crate::curve_editor::CurveEditor>,
    camera_curve_channel: usize,
    camera_canvas: Entity<crate::camera_canvas::CameraCanvas>,
    camera_scopes: Option<Arc<omuse::photo_scopes::PhotoScopes>>,
    camera_scopes_generation: u64,
    camera_scopes_preview: bool,
    camera_clip_shadows: bool,
    camera_clip_highlights: bool,
    camera_draft: serde_json::Value,
    camera_preview: camera_preview_ui::CameraPreviewState,
    camera_gestures: camera_gesture_ui::CameraGestureState,
    adjustment_draft: serde_json::Value,
    show_guides: bool,
    editing_object: Option<String>,
    layer_selection: LayerSelection,
    transform_drag: Option<TransformDrag>,
    transform_original_box: Option<omuse::editor::LayerPlacement>,
    transform_draft: Option<omuse::editor::LayerPlacement>,
    distort_draft: Option<[CanvasPoint; 4]>,
    subject_mask: Option<image::GrayImage>,
    range_draft: Option<range_ui::RangeDraft>,
    vector_draft: Option<vector_ui::VectorDraft>,
    image_trace: image_trace_ui::ImageTraceUi,
    workflow_draft: Option<workflow_ui::WorkflowDraft>,
    pro_draft: Option<advanced_ui::ProDraft>,
    finishing_draft: Option<finishing_ui::FinishingDraft>,
    svg_import_draft: Option<svg_import_ui::SvgImportDraft>,
    proof_settings: omuse::proofing::Settings,
    macro_recording: bool,
    tablet_painting: bool,
    tablet_tool_id: u64,
    macro_revision: u64,
    recorded_recipe: omuse::recipes::Recipe,
    last_batch_report: Option<omuse::recipes::BatchReport>,
    subject_guide: Option<image::RgbaImage>,
    subject_layer: Option<String>,
    subject_as_selection: bool,
    paint_mask: bool,
    mask_inspection: omuse::mask_inspection::MaskInspection,
    guide_drag: Option<(String, omuse::editor::GuideAxis, f32)>,
    preferences: crate::preferences::Preferences,
    import_notes: Vec<String>,
    export_notes: Vec<String>,
    selection_contour: RefCell<SelectionContourCache>,
    selection_ant_phase: u8,
    clipboard_origin: Option<(u64, (u32, u32), (f32, f32))>,
    detail_inputs: Vec<Entity<InputState>>,
}

/// CPU-only startup work. No GPUI entity or GPU atlas is touched here, so the
/// native launch surface can keep drawing while a document is decoded.
pub(crate) struct PreparedEditor {
    doc: Document,
    project: Option<omuse::create_project::Project>,
    path: Option<PathBuf>,
    status: String,
    live_stamp: Option<u64>,
    pixels: image::RgbaImage,
    display: DisplaySurface,
}

impl PreparedEditor {
    pub(crate) fn load(
        path: Option<PathBuf>,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Option<Self> {
        use std::sync::atomic::Ordering;
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let result = path
            .as_ref()
            .map(|p| EditorView::prepare_photo(p, cancelled));
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let (doc, project, pixels, live_stamp, status) = match result {
            Some(Ok((doc, project, pixels, stamp))) => {
                (doc, project, pixels, stamp, "Document opened".to_string())
            }
            fallback => {
                let status = match fallback {
                    Some(Err(err)) => format!("Could not open document: {err:#}"),
                    _ => "Ready — B brush · E eraser · scroll to zoom".to_string(),
                };
                let doc = Document::new(1024, 768);
                let pixels = raster::composite(&doc);
                (doc, None, pixels, None, status)
            }
        };
        // Shell completion commonly supplies a trailing slash for directory
        // packages. Keep a filename-shaped save target after a successful open.
        let path = path
            .filter(|_| live_stamp.is_some())
            .map(|path| path.components().collect());
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let display = DisplaySurface::new(&pixels);
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        Some(Self {
            doc,
            project,
            path,
            status,
            live_stamp,
            pixels,
            display,
        })
    }
}

impl EditorView {
    #[cfg(test)]
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prepared = PreparedEditor::load(path, &std::sync::atomic::AtomicBool::new(false))
            .expect("uncancelled preparation");
        Self::from_prepared(prepared, window, cx)
    }

    pub(crate) fn from_prepared(
        prepared: PreparedEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let PreparedEditor {
            doc,
            project,
            path: valid_path,
            status,
            live_stamp,
            pixels,
            display,
        } = prepared;
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let color = cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(0x7aa2f7)));
        cx.observe(&color, |this, state, cx| {
            if let Some(color) = state.read(cx).value() {
                let c: Rgba = color.into();
                this.editor.brush.color = [
                    (c.r * 255.).round() as u8,
                    (c.g * 255.).round() as u8,
                    (c.b * 255.).round() as u8,
                    (c.a * 255.).round() as u8,
                ];
            }
            cx.notify();
        })
        .detach();
        let dialog_color_picker =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(0x000000)));
        cx.observe(&dialog_color_picker, |this, state, cx| {
            if let Some(color) = state.read(cx).value() {
                let c: Rgba = color.into();
                this.dialog_color = [
                    (c.r * 255.).round() as u8,
                    (c.g * 255.).round() as u8,
                    (c.b * 255.).round() as u8,
                    (c.a * 255.).round() as u8,
                ];
                this.schedule_range_preview(cx);
            }
            cx.notify();
        })
        .detach();
        let background =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(0xffffff)));
        cx.observe(&background, |this, state, cx| {
            if let Some(color) = state.read(cx).value() {
                let c: Rgba = color.into();
                this.background_color = [
                    (c.r * 255.).round() as u8,
                    (c.g * 255.).round() as u8,
                    (c.b * 255.).round() as u8,
                    (c.a * 255.).round() as u8,
                ];
            }
            cx.notify();
        })
        .detach();
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |this, cx| {
                if this.vector_before_request(Pending::Quit, window, cx) {
                    return false;
                }
                if !this.finish_inline_text(true, window, cx) {
                    return false;
                }
                this.cancel_photo_io();
                if this.range_draft.is_some()
                    || this.vector_draft.is_some()
                    || this.workflow_draft.is_some()
                    || this.pro_draft.is_some()
                    || this.finishing_draft.is_some()
                    || this.dialog == Dialog::CameraRaw
                {
                    this.cancel_range(cx);
                    this.clear_vector(cx);
                    this.clear_workflow(cx);
                    this.clear_pro(cx);
                    this.cancel_finishing(cx);
                    this.cancel_camera_raw();
                    this.dialog_generation = this.dialog_generation.wrapping_add(1);
                    this.dialog = Dialog::None;
                }
                this.finish_interaction(cx);
                if this.create.saving {
                    this.pending = Some(Pending::Quit);
                    this.status = "Finishing the current save before closing…".into();
                    cx.notify();
                    return false;
                }
                if this.has_unsaved_work() {
                    this.pending = Some(Pending::Quit);
                    this.dialog = Dialog::Unsaved;
                    this.modal_focus.focus(window, cx);
                    cx.notify();
                    false
                } else {
                    this.recovery.clear();
                    true
                }
            })
            .unwrap_or(true)
        });
        let weak = cx.entity().downgrade();
        let editor_window = window.window_handle();
        cx.intercept_keystrokes(move |event, window, cx| {
            let _ = weak.update(cx, |this, cx| {
                // Bound input actions run before raw key capture. Intercept
                // Escape here so a focused numeric/text field cannot consume
                // cancellation of this editor window's pending image work.
                let modifiers = event.keystroke.modifiers;
                if this.busy
                    && this.photo_io.is_some()
                    && this.dialog == Dialog::None
                    && window.window_handle() == editor_window
                    && event.keystroke.key == "escape"
                    && !modifiers.control
                    && !modifiers.shift
                    && !modifiers.alt
                    && !modifiers.platform
                    && this.cancel_photo_io()
                {
                    this.dialog_generation = this.dialog_generation.wrapping_add(1);
                    this.focus.focus(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.inline_font_key(&event.keystroke, window, cx)
                    || this.recent_key(&event.keystroke, window, cx)
                    || this.command_search_key(&event.keystroke, window, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                if this.inline_text.is_some() && this.focus.contains_focused(window, cx) {
                    let key = event.keystroke.key.as_str();
                    let modifiers = event.keystroke.modifiers;
                    let unmodified = !modifiers.control
                        && !modifiers.shift
                        && !modifiers.alt
                        && !modifiers.platform;
                    let control_only = modifiers.control
                        && !modifiers.shift
                        && !modifiers.alt
                        && !modifiers.platform;
                    let chord = event.keystroke.unparse();
                    let save = this.shortcuts.chord("save") == chord;
                    let save_as = this.shortcuts.chord("save-as") == chord;
                    let quit = this.shortcuts.chord("quit") == chord;
                    if (key == "escape" && unmodified)
                        || (key == "enter" && control_only)
                        || save
                        || save_as
                        || quit
                    {
                        cx.stop_propagation();
                        let cancel = key == "escape" && unmodified;
                        if cancel && this.cancel_inline_color(window, cx) {
                            return;
                        }
                        let committed = this.finish_inline_text(!cancel, window, cx);
                        if committed && (save || save_as) {
                            if save_as {
                                this.save_dialog(false, window, cx);
                            } else {
                                this.save(window, cx);
                            }
                        }
                        if committed && quit {
                            this.request(Pending::Quit, window, cx);
                        }
                        return;
                    }
                }
                if this.dialog == Dialog::Shortcuts && this.modal_focus.contains_focused(window, cx)
                {
                    if let Some(id) = this.recording.clone() {
                        cx.stop_propagation();
                        if event.keystroke.key == "escape" {
                            this.recording = None;
                        } else {
                            match this.shortcut_draft.assign(&id, &event.keystroke.unparse()) {
                                Ok(()) => {
                                    this.recording = None;
                                    this.status = "Recorded. Apply to save shortcuts.".into();
                                }
                                Err(e) => this.status = e.to_string(),
                            }
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        let recovery = Recovery::new();
        let recover = recovery.available();
        let mut editor = Editor::new(doc);
        editor.brush.color = [122, 162, 247, 255];
        let camera_curve = cx.new(|_| {
            crate::curve_editor::CurveEditor::new(omuse::camera_raw::Settings::default().curve.rgb)
                .expect("valid default curve")
        });
        cx.subscribe(
            &camera_curve,
            |this, _, event: &crate::curve_editor::CurveChanged, cx| {
                if this.dialog == Dialog::CameraRaw
                    && !this.busy
                    && crate::camera_controls::SECTIONS
                        .get(this.camera_section)
                        .is_some_and(|section| section.0 == "curve")
                {
                    let channel = ["rgb", "red", "green", "blue"][this.camera_curve_channel];
                    this.camera_draft["curve"][channel] =
                        serde_json::to_value(&event.0).expect("finite curve points");
                    cx.notify();
                }
            },
        )
        .detach();
        let camera_canvas = cx.new(|_| {
            crate::camera_canvas::CameraCanvas::new(
                Arc::new(image::RgbaImage::new(1, 1)),
                crate::camera_canvas::CameraCanvasMode::PointColor,
            )
            .expect("nonempty preview")
        });
        cx.subscribe_in(
            &camera_canvas,
            window,
            |this, _, event: &crate::camera_canvas::CameraCanvasEvent, window, cx| {
                this.handle_camera_canvas(event, window, cx);
            },
        )
        .detach();
        let initial_layer = editor.active_layer.clone();
        let preferences = crate::preferences::Preferences::current();
        let path_input = cx.new(|cx| InputState::new(window, cx).placeholder("File path"));
        cx.subscribe(&path_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.invalidate_jpeg_preview();
                cx.notify();
            }
        })
        .detach();
        let width_input = cx.new(|cx| InputState::new(window, cx).default_value("1024"));
        let height_input = cx.new(|cx| InputState::new(window, cx).default_value("768"));
        cx.subscribe_in(
            &width_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.sync_canvas_aspect(true, window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &height_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.sync_canvas_aspect(false, window, cx);
                }
            },
        )
        .detach();
        let detail_inputs: Vec<_> = (0..128)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        for input in &detail_inputs[..5] {
            cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.invalidate_jpeg_preview();
                    this.schedule_range_preview(cx);
                    if this.vector_scene_active() && !this.busy {
                        // Intermediate text (for example an incomplete hex colour)
                        // keeps the last valid preview; Done reports invalid input.
                        let _ = this.update_vector_style(cx);
                    }
                    cx.notify();
                }
            })
            .detach();
        }
        cx.observe(&detail_inputs[9], |_, _, cx| cx.notify())
            .detach();
        let mut view = Self {
            editor,
            path: valid_path,
            display,
            mask_preview: RefCell::new(None),
            canvas_textures: Rc::new(RefCell::new(CanvasTextures::default())),
            layer_thumbnails: RefCell::new(crate::studio_thumbnails::LayerThumbnails::default()),
            display_reference: None,
            display_probe_matte: None,
            pixels,
            pending_stroke_frame: None,
            stroke_frame_generation: 0,
            focus,
            modal_focus: cx.focus_handle(),
            tool: Tool::Brush,
            inspector_tab: studio_ui::InspectorTab::Layers,
            inspector_visible: true,
            create: create_ui::CreateState::new(project, window, cx),
            ai: ai_ui::AiState::new(window, cx),
            product: product_ui::ProductUiState::new(window, cx),
            asset_ui: asset_ui::AssetUiState::new(window, cx),
            spot_healing_mode: omuse::spot_heal::SpotHealingMode::ContentAware,
            zoom: 0.65,
            pan: (0., 0.),
            crop: None,
            viewport: Rc::new(Cell::new(Bounds::default())),
            drag_start: None,
            selection_box: None,
            status,
            dialog: if recover.is_some() {
                Dialog::Recover
            } else {
                Dialog::None
            },
            pending: None,
            path_input,
            width_input,
            height_input,
            color,
            dialog_color_picker,
            dialog_color: [0, 0, 0, 255],
            background,
            background_color: [255, 255, 255, 255],
            fill_tolerance: 24,
            text_origin: None,
            inline_text: None,
            inline_preview: inline_text_ui::InlinePreview::default(),
            text_hit_pending: None,
            font_names: None,
            shape_corner_radius: 0.,
            shape_line_width: 4.,
            resize_resolution: 72.,
            resize_sampling: 2,
            canvas_anchor: 4,
            canvas_fill: 0,
            canvas_units: 0,
            canvas_relative: false,
            canvas_lock_aspect: false,
            trim_options: Default::default(),
            collapsed_groups: Default::default(),
            gradient_settings: GradientSettings::default(),
            gradient_pending: None,
            recovery,
            photo_io: None,
            busy: false,
            live_stamp,
            filter_kind: 0,
            filter_inputs: [
                cx.new(|cx| InputState::new(window, cx).default_value("0.5")),
                cx.new(|cx| InputState::new(window, cx).default_value("0")),
                cx.new(|cx| InputState::new(window, cx).default_value("0")),
            ],
            clone_source: None,
            clone_offset: None,
            clone_aligned: true,
            clone_all_layers: false,
            clone_aligned_draft: true,
            clone_all_layers_draft: false,
            dialog_generation: 0,
            save_confirmation: None,
            save_again: false,
            jpeg_preview: None,
            jpeg_inspection: jpeg_preview_ui::JpegInspection::default(),
            jpeg_preview_generation: 0,
            jpeg_preview_task: None,
            lasso: Vec::new(),
            pointer_path_error: None,
            pan_pointer: None,
            middle_pan_pointer: None,
            space_down: false,
            shortcuts: Shortcuts::load(&shortcuts::settings_path()).unwrap_or_default(),
            shortcut_draft: Shortcuts::default(),
            recording: None,
            command_search: command_search_ui::CommandSearchState::new(window, cx),
            recent: recent_ui::RecentState::new(window, cx),
            external: external_change_ui::ExternalState::default(),
            numeric: RefCell::new(numeric_ui::NumericState::default()),
            show_grid: preferences.grid,
            effect_kind: 0,
            live_filter: false,
            adjustment_kind: 0,
            selection_operation: 0,
            selection_mode: SelectionMode::Replace,
            wand_settings: WandSettings::default(),
            wand_draft: WandSettings::default(),
            selection_mode_draft: SelectionMode::Replace,
            selection_before_gesture: None,
            gesture_selection_mode: SelectionMode::Replace,
            raw_open: false,
            camera_section: 0,
            camera_curve,
            camera_curve_channel: 0,
            camera_canvas,
            camera_scopes: None,
            camera_scopes_generation: 0,
            camera_scopes_preview: false,
            camera_clip_shadows: false,
            camera_clip_highlights: false,
            camera_draft: serde_json::Value::Null,
            camera_preview: camera_preview_ui::CameraPreviewState::default(),
            camera_gestures: camera_gesture_ui::CameraGestureState::default(),
            adjustment_draft: serde_json::Value::Null,
            show_guides: preferences.guides,
            editing_object: None,
            layer_selection: LayerSelection {
                ids: vec![initial_layer.clone()],
                primary: Some(initial_layer),
            },
            transform_drag: None,
            transform_original_box: None,
            transform_draft: None,
            distort_draft: None,
            subject_mask: None,
            range_draft: None,
            vector_draft: None,
            image_trace: image_trace_ui::ImageTraceUi::default(),
            workflow_draft: None,
            pro_draft: None,
            finishing_draft: None,
            svg_import_draft: None,
            proof_settings: Default::default(),
            macro_recording: false,
            tablet_painting: false,
            tablet_tool_id: 0,
            macro_revision: 0,
            recorded_recipe: Default::default(),
            last_batch_report: None,
            subject_guide: None,
            subject_layer: None,
            subject_as_selection: false,
            paint_mask: false,
            mask_inspection: omuse::mask_inspection::MaskInspection::default(),
            guide_drag: None,
            preferences,
            import_notes: Vec::new(),
            export_notes: Vec::new(),
            selection_contour: RefCell::new(SelectionContourCache::default()),
            selection_ant_phase: 0,
            clipboard_origin: None,
            detail_inputs,
        };
        if let Some(settings) = advanced_ui::current_brush() {
            view.editor.brush.size = settings.size;
            view.editor.brush.hardness = settings.hardness;
            let _ = view.editor.set_brush_dynamics(Some(settings));
        }
        if let Some(path) = view.path.clone() {
            view.note_recent(path, cx);
        } else if !cfg!(test) {
            view.refresh_recent(cx);
        }
        view.start_external_watch(window, cx);
        cx.on_release(|view, cx| {
            view.clear_image_trace(cx);
            view.clear_range(cx);
            view.clear_vector(cx);
            view.clear_workflow(cx);
            view.clear_pro(cx);
            view.clear_finishing(cx);
            view.cancel_camera_raw();
            view.canvas_textures.borrow_mut().release(cx);
            view.layer_thumbnails.borrow_mut().release(cx);
            view.asset_ui.release(cx);
            view.create.release(cx);
            view.ai.release(cx);
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                let selected = entity
                    .update(cx, |view, _| view.editor.selection.is_some())
                    .unwrap_or(false);
                let delay = if selected { 120 } else { 500 };
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(delay))
                    .await;
                if entity
                    .update(cx, |view, cx| {
                        if view.editor.selection.is_some() {
                            // Scheduling reads only cache keys. Mask traversal
                            // stays in the cancellable outline worker.
                            let outline = view.contour_for_canvas(cx);
                            if !outline.points.is_empty() {
                                view.selection_ant_phase =
                                    view.selection_ant_phase.wrapping_add(1) % 8;
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view
    }
    fn finish_interaction(&mut self, cx: &mut Context<Self>) {
        self.finish_numeric_scrub(cx);
        self.tablet_painting = false;
        self.pan_pointer = None;
        self.middle_pan_pointer = None;
        if self.guide_drag.take().is_some() {
            self.drag_start = None;
            cx.notify();
            return;
        }
        if self.transform_drag.take().is_some() {
            self.drag_start = None;
            self.transform_original_box = None;
            self.transform_draft = None;
            self.distort_draft = None;
            cx.notify();
            return;
        }
        if self.drag_start.take().is_some() {
            self.editor.finish_stroke();
            self.editor.finish_clone_stroke();
            self.changed(cx);
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.mask_inspection.target().map(str::to_owned)
            && self
                .editor
                .document
                .find_layer(&id)
                .is_none_or(|layer| layer.mask.is_none())
        {
            self.mask_inspection.exit();
        }
        // A full refresh supersedes queued pointer work, including work from a
        // stroke that ended, was cancelled, or belonged to a replaced document.
        self.pending_stroke_frame = None;
        self.editor.take_stroke_damage();
        let errors = raster::validate(&self.editor.document);
        if !errors.is_empty() {
            self.status = format!(
                "Cannot render this edit: {}. Undo restores the previous state.",
                errors.join("; ")
            );
            cx.notify();
            return;
        }

        let numeric_preview = self.numeric_preview_document();
        self.pixels = raster::composite(numeric_preview.as_ref().unwrap_or(&self.editor.document));
        self.present_pixels(cx);
    }
    fn present_pixels(&mut self, cx: &mut Context<Self>) {
        if !self.proof_settings.enabled {
            self.display.replace(&self.pixels);
            cx.notify();
            return;
        }
        match omuse::proofing::render(&self.pixels, &self.proof_settings) {
            Ok(pixels) => {
                self.display.replace(&pixels);
            }
            Err(error) => {
                self.display.replace(&self.pixels);
                self.status = format!("Display profile unavailable: {error}");
            }
        }
        cx.notify();
    }
    fn queue_stroke_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_stroke_frame.is_some() {
            return;
        }
        self.stroke_frame_generation = self.stroke_frame_generation.wrapping_add(1);
        let generation = self.stroke_frame_generation;
        self.pending_stroke_frame = Some(generation);
        cx.on_next_frame(window, move |this, _, cx| {
            if this.pending_stroke_frame != Some(generation) {
                return;
            }
            this.pending_stroke_frame = None;
            this.refresh_stroke(cx);
        });
        cx.notify();
    }
    fn refresh_stroke(&mut self, cx: &mut Context<Self>) {
        let Some(damage) = self.editor.take_stroke_damage() else {
            return;
        };
        let doc = &self.editor.document;
        if !self.proof_settings.enabled
            && !damage.mask_target
            && let Some(layer) = doc.find_layer(&damage.layer_id)
            && layer.rotation == 0.
            && layer.scale_x == 1.
            && layer.scale_y == 1.
            && layer.offset_x.is_finite()
            && layer.offset_y.is_finite()
            && layer.offset_x.fract() == 0.
            && layer.offset_y.fract() == 0.
        {
            let region = damage.local_rect.translated_clipped(
                layer.offset_x as i64,
                layer.offset_y as i64,
                doc.width,
                doc.height,
            );
            // Eligibility is checked even for off-canvas edits: effects and
            // linked masks can make those pixels affect other canvas regions.
            if raster::composite_region(
                doc,
                &mut self.pixels,
                region.unwrap_or(PixelRect {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }),
            ) {
                if let Some(region) = region {
                    self.display.update_region(&self.pixels, region);
                    cx.notify();
                }
                return;
            }
        }
        self.refresh(cx);
    }
    fn record_recipe_step(&mut self, step: omuse::recipes::Step) {
        if !self.macro_recording {
            return;
        }
        let layer = self.editor.document.find_layer(&self.editor.active_layer);
        if self.editor.selection.is_some()
            || self.paint_mask
            || self.recorded_recipe.steps.len() >= 256
            || layer.is_none_or(|l| l.is_group() || l.image.is_none())
        {
            self.macro_recording = false;
            self.status = "Recipe recording paused: use whole-image edits without a selection; recipes hold at most 256 steps.".into();
            return;
        }
        self.recorded_recipe.steps.push(step);
        self.macro_revision = self.editor.revision();
        self.status = format!(
            "Recorded {} recipe steps · open Automation to stop or save",
            self.recorded_recipe.steps.len()
        );
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.crop = None;
        if self.macro_recording && self.macro_revision != self.editor.revision() {
            self.macro_recording = false;
            self.status = "Recipe recording paused: this edit is not a whole-image recipe step. Recorded steps remain available in Automation.".into();
        }
        if self.paint_mask
            && self
                .editor
                .document
                .find_layer(&self.editor.active_layer)
                .is_none_or(|layer| layer.mask.is_none())
        {
            self.paint_mask = false;
        }
        self.layer_selection
            .ids
            .retain(|id| self.editor.document.find_layer(id).is_some());
        if !self.layer_selection.ids.contains(&self.editor.active_layer) {
            self.layer_selection
                .click(self.editor.active_layer.clone(), SelectionAction::Replace);
        }
        self.refresh(cx);
        self.schedule_content_recovery();
    }
    /// Admit a background completion only while it still owns the current UI job.
    /// A cancelled job may continue running on the background executor, so it must
    /// never clear `busy` for a newer job or overwrite that job's status.
    fn finish_background_job(&mut self, generation: u64, dialog: Dialog) -> bool {
        if self.dialog_generation != generation || self.dialog != dialog {
            return false;
        }
        self.busy = false;
        true
    }
    fn preview_gradient(&mut self, cx: &mut Context<Self>) {
        let Some((start, end)) = self.gradient_pending else {
            return;
        };
        let mut preview = Editor::new(self.editor.document.clone());
        preview.active_layer = self.editor.active_layer.clone();
        preview.selection = self.editor.selection.clone();
        preview.brush = self.editor.brush.clone();
        let result = if self.paint_mask {
            preview.gradient_mask_with(
                &self.editor.active_layer,
                start,
                end,
                mask_value(self.editor.brush.color),
                mask_value(self.background_color),
                &self.gradient_settings,
            )
        } else {
            preview.gradient_with(
                start,
                end,
                self.editor.brush.color,
                self.background_color,
                &self.gradient_settings,
            )
        };
        match result {
            Ok(_) => {
                self.pixels = raster::composite(&preview.document);
                self.present_pixels(cx);
                self.status = "Gradient preview".into();
            }
            Err(error) => self.status = format!("Gradient preview: {error:#}"),
        }
        cx.notify();
    }
    fn persist_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences.grid = self.show_grid;
        self.preferences.guides = self.show_guides;
        if let Err(e) = self.preferences.persist() {
            self.status = format!("Preferences: {e:#}");
        }
        cx.notify();
    }
    fn ensure_font_names(&mut self, cx: &App) {
        if self.font_names.is_some() {
            return;
        }
        let mut names = cx.text_system().all_font_names();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        if names.is_empty() {
            names.push("sans-serif".into());
        }
        self.font_names = Some(names);
    }
    fn coordinates(&self, position: Point<Pixels>) -> (f32, f32) {
        let bounds = self.viewport.get();
        let w = self.editor.document.width as f32 * self.zoom;
        let h = self.editor.document.height as f32 * self.zoom;
        let left =
            f32::from(bounds.origin.x) + (f32::from(bounds.size.width) - w) / 2. + self.pan.0;
        let top =
            f32::from(bounds.origin.y) + (f32::from(bounds.size.height) - h) / 2. + self.pan.1;
        (
            (f32::from(position.x) - left) / self.zoom,
            (f32::from(position.y) - top) / self.zoom,
        )
    }
    fn down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog != Dialog::None || self.busy {
            return;
        }
        self.focus.focus(window, cx);
        if self.middle_pan_pointer.is_some() {
            if (self.tool == Tool::Hand && self.crop.is_none()) || self.space_down {
                self.pan_pointer = Some(event.position);
                self.middle_pan_pointer = Some(event.position);
            }
            return;
        }
        if (self.tool == Tool::Hand && self.crop.is_none()) || self.space_down {
            self.stop_vector_drag();
            if let Some(crop) = &mut self.crop {
                crop.end();
            }
            self.pan_pointer = Some(event.position);
            return;
        }
        if self.image_trace_active() {
            return;
        }
        if self.vector_scene_active() {
            self.vector_canvas_down(event, window, cx);
            return;
        }
        if self.mask_inspection.active() {
            self.status =
                "Mask inspection is read-only; Alt-click the mask badge to restore artwork".into();
            cx.notify();
            return;
        }
        let (x, y) = self.coordinates(event.position);
        if let Some(crop) = &mut self.crop {
            crop.begin((x, y), 8. / self.zoom);
            cx.notify();
            return;
        }
        if self.show_guides {
            let tolerance = 6. / self.zoom;
            if let Some(guide) = self.editor.guides().into_iter().find(|guide| {
                (match guide.axis {
                    omuse::editor::GuideAxis::Horizontal => (guide.position - y).abs(),
                    omuse::editor::GuideAxis::Vertical => (guide.position - x).abs(),
                }) <= tolerance
            }) {
                self.guide_drag = Some((guide.id, guide.axis, guide.position));
                self.drag_start = Some((x, y));
                self.status = "Dragging guide".into();
                cx.notify();
                return;
            }
        }
        if self.tool == Tool::Move {
            let pointer = CanvasPoint { x, y };
            if !self.layer_selection.ids.contains(&self.editor.active_layer) {
                self.layer_selection
                    .click(self.editor.active_layer.clone(), SelectionAction::Replace);
            }
            let current_box = selection_bounds(&self.editor, &self.layer_selection.ids);
            let candidate = current_box.and_then(|bounds| {
                if self.preferences.transform_box {
                    TransformGeometry::new(bounds, 24. / self.zoom)
                        .and_then(|geometry| geometry.hit_test(pointer, 8. / self.zoom))
                } else {
                    bounds
                        .contains(pointer.x, pointer.y)
                        .then_some(HitTarget::Move)
                }
            });
            let hit_layer = self
                .preferences
                .auto_select
                .then(|| hit_layer_at(&self.editor, x, y))
                .flatten();
            let frontmost_unselected = hit_layer
                .as_ref()
                .is_some_and(|id| !self.layer_selection.ids.contains(id));
            let single_raster_selected = match self.layer_selection.ids.as_slice() {
                [id] => self
                    .editor
                    .document
                    .find_layer(id)
                    .is_some_and(|layer| !layer.is_group()),
                _ => false,
            };
            let existing_hit = if candidate == Some(HitTarget::Move)
                && (event.modifiers.control
                    || event.modifiers.shift
                    || (frontmost_unselected && single_raster_selected))
            {
                None
            } else {
                candidate
            };
            if existing_hit.is_none() {
                if let Some(id) = hit_layer {
                    let action = if event.modifiers.control {
                        SelectionAction::Toggle
                    } else if event.modifiers.shift {
                        SelectionAction::Add
                    } else {
                        SelectionAction::Replace
                    };
                    self.layer_selection.click(id, action);
                    if let Some(primary) = self.layer_selection.primary.clone() {
                        self.editor.active_layer = primary;
                    }
                } else if !event.modifiers.control && !event.modifiers.shift {
                    self.layer_selection.clear();
                }
            }
            let Some(bounds) = selection_bounds(&self.editor, &self.layer_selection.ids) else {
                self.transform_drag = None;
                self.transform_original_box = None;
                self.transform_draft = None;
                cx.notify();
                return;
            };
            if (event.modifiers.control || event.modifiers.shift) && existing_hit.is_none() {
                cx.notify();
                return;
            }
            let target = existing_hit.or_else(|| {
                if self.preferences.transform_box {
                    TransformGeometry::new(bounds, 24. / self.zoom)?
                        .hit_test(pointer, 8. / self.zoom)
                } else {
                    bounds
                        .contains(pointer.x, pointer.y)
                        .then_some(HitTarget::Move)
                }
            });
            let mode = match target {
                Some(HitTarget::Resize(index))
                    if event.modifiers.control && self.layer_selection.ids.len() == 1 =>
                {
                    DragMode::Distort(index)
                }
                Some(HitTarget::Resize(index)) => DragMode::Resize(index),
                Some(HitTarget::Rotate) => DragMode::Rotate,
                Some(HitTarget::Move) => DragMode::Move,
                None => {
                    cx.notify();
                    return;
                }
            };
            self.drag_start = Some((x, y));
            let Some(geometry) = TransformGeometry::new(bounds, 0.) else {
                return;
            };
            let corners = geometry.corners();
            self.transform_drag = TransformDrag::new(bounds, pointer, mode)
                .and_then(|drag| drag.with_corners(corners));
            self.distort_draft = matches!(mode, DragMode::Distort(_)).then_some(corners);
            self.transform_original_box = Some(bounds);
            self.transform_draft = Some(bounds);
            self.selection_box = None;
            cx.notify();
            return;
        }
        if x < 0.
            || y < 0.
            || x >= self.editor.document.width as f32
            || y >= self.editor.document.height as f32
        {
            return;
        }
        if self.paint_mask
            && !matches!(
                self.tool,
                Tool::Brush
                    | Tool::Pencil
                    | Tool::Eraser
                    | Tool::Fill
                    | Tool::Gradient
                    | Tool::Clone
                    | Tool::Heal
                    | Tool::BlurBrush
                    | Tool::Smudge
                    | Tool::Liquify
            )
        {
            self.status = "This tool does not edit masks; switch the mask target off first".into();
            cx.notify();
            return;
        }
        if matches!(
            self.tool,
            Tool::Brush
                | Tool::Pencil
                | Tool::Eraser
                | Tool::Clone
                | Tool::Heal
                | Tool::Fill
                | Tool::Gradient
                | Tool::SpotHeal
        ) && !self.paint_mask
            && self
                .editor
                .document
                .find_layer(&self.editor.active_layer)
                .is_some_and(|l| {
                    l.metadata.get("text").is_some_and(|v| !v.is_null())
                        || l.metadata.get("shape").is_some_and(|v| !v.is_null())
                        || l.advanced.is_some()
                        || l.vector_scene.is_some()
                })
        {
            self.status="This layer has an editable source. Use its editing workspace, paint on a new layer, or Rasterize before painting.".into();
            cx.notify();
            return;
        }
        let drag_start = if matches!(
            self.tool,
            Tool::Rectangle
                | Tool::Ellipse
                | Tool::Gradient
                | Tool::ShapeRect
                | Tool::ShapeEllipse
                | Tool::Line
        ) {
            let snapped = snap_canvas_point(
                &self.editor,
                CanvasPoint { x, y },
                self.preferences.snapping && self.show_grid,
                self.preferences.snapping && self.show_guides,
                6. / self.zoom,
                omuse::canvas_grid::GridSettings {
                    spacing: self.preferences.grid_spacing,
                    subdivisions: self.preferences.grid_subdivisions,
                },
                event.modifiers.shift,
            );
            (snapped.x, snapped.y)
        } else {
            (x, y)
        };
        self.drag_start = Some(drag_start);
        self.pointer_path_error = None;
        if matches!(
            self.tool,
            Tool::Rectangle | Tool::Ellipse | Tool::Lasso | Tool::Wand | Tool::Object
        ) {
            self.selection_before_gesture = self.editor.selection.clone();
            self.gesture_selection_mode = if event.modifiers.alt {
                SelectionMode::Subtract
            } else if event.modifiers.shift {
                SelectionMode::Add
            } else {
                self.selection_mode
            };
        }
        match self.tool {
            Tool::Lasso | Tool::SpotHeal | Tool::BlurBrush | Tool::Smudge | Tool::Liquify => {
                self.lasso.clear();
                self.capture_pointer_point((x, y));
            }
            Tool::Brush | Tool::Pencil | Tool::Eraser => {
                let tool = match self.tool {
                    Tool::Eraser => PaintTool::Eraser,
                    Tool::Pencil => PaintTool::Pencil,
                    _ => PaintTool::Brush,
                };
                let started = if self.paint_mask {
                    self.editor.begin_mask_stroke(x, y, 1., tool)
                } else {
                    self.editor.begin_stroke(x, y, 1., tool)
                };
                if started {
                    self.status = if self.paint_mask {
                        "Painting layer mask"
                    } else {
                        "Painting layer"
                    }
                    .into();
                    self.queue_stroke_frame(window, cx);
                } else {
                    self.drag_start = None;
                    self.status = if self.paint_mask {
                        "Add an unlocked raster mask before painting it"
                    } else {
                        "Select an unlocked raster layer"
                    }
                    .into();
                    cx.notify();
                }
            }
            Tool::Fill => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    match self.editor.flood_fill_mask(
                        &id,
                        x as i32,
                        y as i32,
                        mask_value(self.editor.brush.color),
                        self.fill_tolerance,
                    ) {
                        Ok(true) => self.status = "Mask area filled".into(),
                        Ok(false) => self.status = "Mask fill made no change".into(),
                        Err(e) => self.status = format!("Mask fill: {e:#}"),
                    }
                } else {
                    self.editor.fill_at(
                        x as i32,
                        y as i32,
                        self.editor.brush.color,
                        self.fill_tolerance,
                    );
                }
                self.changed(cx);
            }
            Tool::Picker => {
                if x >= 0.
                    && y >= 0.
                    && x < (self.pixels.width() as f32)
                    && y < (self.pixels.height() as f32)
                {
                    let p = self.pixels.get_pixel(x as u32, y as u32).0;
                    self.editor.brush.color = p;
                    self.color.update(cx, |state, cx| {
                        state.set_value(rgba(u32::from_be_bytes(p)), window, cx)
                    });
                    cx.notify();
                }
            }
            Tool::Clone | Tool::Heal => {
                if event.modifiers.alt {
                    self.clone_source = Some((x, y));
                    self.clone_offset = None;
                    self.drag_start = None;
                    self.status = "Source set. Drag to retouch.".into();
                } else if let Some(chosen_source) = self.clone_source {
                    let source = if self.clone_aligned {
                        let offset = *self
                            .clone_offset
                            .get_or_insert((chosen_source.0 - x, chosen_source.1 - y));
                        (x + offset.0, y + offset.1)
                    } else {
                        chosen_source
                    };
                    if self.paint_mask {
                        self.lasso.clear();
                        self.capture_pointer_point((x, y));
                        self.status = "Mask clone stroke".into();
                    } else {
                        if self.clone_all_layers {
                            self.editor.begin_clone_stroke_from_canvas(
                                source,
                                (x, y),
                                self.tool == Tool::Heal,
                            );
                        } else {
                            self.editor
                                .begin_clone_stroke(source, (x, y), self.tool == Tool::Heal);
                        }
                        self.queue_stroke_frame(window, cx);
                    }
                } else {
                    self.status = "Alt-click to choose a source, then drag to retouch".into();
                }
                cx.notify();
            }
            Tool::Object => {
                self.drag_start = None;
                self.start_image_selection(x, y, true, cx);
            }
            Tool::Wand => {
                self.drag_start = None;
                self.start_image_selection(x, y, false, cx);
            }
            Tool::Text => {
                self.text_hit_pending = if event.modifiers.alt {
                    None
                } else {
                    text_at(&self.editor, x, y)
                };
                if self.text_hit_pending.is_none() {
                    self.selection_box = Some((x, y, 0., 0.));
                }
                cx.notify();
            }
            Tool::Move => {}
            Tool::Rectangle | Tool::Ellipse => {
                self.selection_box = Some((x, y, 0., 0.));
                cx.notify();
            }
            _ => {}
        }
    }
    fn middle_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog != Dialog::None || self.busy {
            return;
        }
        if self.inline_text.is_some()
            || self.tablet_painting
            || (self.drag_start.is_some() && self.pan_pointer.is_none())
        {
            return;
        }
        self.stop_vector_drag();
        self.focus.focus(window, cx);
        self.middle_pan_pointer = Some(event.position);
        if let Some(crop) = &mut self.crop {
            crop.end();
        }
        if self.pan_pointer.is_some() {
            self.pan_pointer = Some(event.position);
        }
        cx.notify();
    }
    fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.numeric_scrubbing() {
            return;
        }
        if self.dialog != Dialog::None || self.busy {
            return;
        }
        if event.pressed_button.is_none() {
            let primary = self.pan_pointer.take().is_some();
            let middle = self.middle_pan_pointer.take().is_some();
            if primary || middle {
                cx.notify();
                return;
            }
        }
        if let Some(last) = self.middle_pan_pointer.or(self.pan_pointer) {
            self.pan.0 += f32::from(event.position.x - last.x);
            self.pan.1 += f32::from(event.position.y - last.y);
            if self.pan_pointer.is_some() {
                self.pan_pointer = Some(event.position);
            }
            if self.middle_pan_pointer.is_some() {
                self.middle_pan_pointer = Some(event.position);
            }
            cx.notify();
            return;
        }
        if self.image_trace_active() {
            return;
        }
        if self.vector_scene_active() {
            self.vector_move(event, window, cx);
            return;
        }
        if self.crop.is_some() {
            let p = self.coordinates(event.position);
            let crop = self.crop.as_mut().unwrap();
            if event.pressed_button == Some(MouseButton::Left) {
                crop.update(p);
            } else {
                crop.end();
            }
            cx.notify();
            return;
        }
        let Some(start) = self.drag_start else {
            return;
        };
        if !event.dragging() {
            return;
        }
        let (mut x, mut y) = self.coordinates(event.position);
        if matches!(
            self.tool,
            Tool::Rectangle
                | Tool::Ellipse
                | Tool::Gradient
                | Tool::ShapeRect
                | Tool::ShapeEllipse
                | Tool::Line
        ) {
            let snapped = snap_canvas_point(
                &self.editor,
                CanvasPoint { x, y },
                self.preferences.snapping && self.show_grid,
                self.preferences.snapping && self.show_guides,
                6. / self.zoom,
                omuse::canvas_grid::GridSettings {
                    spacing: self.preferences.grid_spacing,
                    subdivisions: self.preferences.grid_subdivisions,
                },
                event.modifiers.shift,
            );
            x = snapped.x;
            y = snapped.y;
        }
        if let Some((_, axis, position)) = self.guide_drag.as_mut() {
            *position = match axis {
                omuse::editor::GuideAxis::Horizontal => y,
                omuse::editor::GuideAxis::Vertical => x,
            };
            cx.notify();
            return;
        }
        if self.tool == Tool::Move {
            if let Some(drag) = &self.transform_drag {
                let pointer = snap_canvas_point(
                    &self.editor,
                    CanvasPoint { x, y },
                    self.preferences.snapping && self.show_grid,
                    self.preferences.snapping && self.show_guides,
                    6. / self.zoom,
                    omuse::canvas_grid::GridSettings {
                        spacing: self.preferences.grid_spacing,
                        subdivisions: self.preferences.grid_subdivisions,
                    },
                    event.modifiers.shift,
                );
                if matches!(drag.mode, DragMode::Distort(_)) {
                    self.distort_draft = drag.distorted_corners(pointer, event.modifiers.shift);
                }
                self.transform_draft = Some(drag.updated(
                    pointer,
                    DragModifiers {
                        shift: event.modifiers.shift,
                        from_center: event.modifiers.alt,
                        lock_ratio: true,
                    },
                ));
                cx.notify();
            }
            return;
        }
        match self.tool {
            Tool::Lasso | Tool::SpotHeal | Tool::BlurBrush | Tool::Smudge | Tool::Liquify => {
                self.capture_pointer_point((x, y));
                self.selection_box = Some((
                    start.0.min(x),
                    start.1.min(y),
                    (x - start.0).abs(),
                    (y - start.1).abs(),
                ));
                cx.notify();
            }
            Tool::Brush | Tool::Pencil | Tool::Eraser => {
                self.editor.continue_stroke_at_zoom(x, y, 1., self.zoom);
                self.queue_stroke_frame(window, cx);
            }
            Tool::Clone | Tool::Heal => {
                if self.paint_mask {
                    self.capture_pointer_point((x, y));
                    self.selection_box = Some((
                        start.0.min(x),
                        start.1.min(y),
                        (x - start.0).abs(),
                        (y - start.1).abs(),
                    ));
                    cx.notify();
                } else {
                    self.editor.continue_clone_stroke((x, y));
                    self.queue_stroke_frame(window, cx);
                }
            }
            Tool::Rectangle
            | Tool::Ellipse
            | Tool::Gradient
            | Tool::Text
            | Tool::ShapeRect
            | Tool::ShapeEllipse
            | Tool::Line => {
                self.selection_box = Some((
                    start.0.min(x),
                    start.1.min(y),
                    (x - start.0).abs(),
                    (y - start.1).abs(),
                ));
                cx.notify();
            }
            _ => {}
        }
    }
    fn up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.numeric_scrubbing() {
            return;
        }
        if self.dialog != Dialog::None || self.busy {
            return;
        }
        if self.pan_pointer.take().is_some() {
            return;
        }
        if self.image_trace_active() {
            return;
        }
        if self.vector_scene_active() {
            self.vector_up(event, window, cx);
            return;
        }
        if self.mask_inspection.active() {
            return;
        }
        if self.crop.is_some() {
            let p = self.coordinates(event.position);
            let crop = self.crop.as_mut().unwrap();
            crop.update(p);
            crop.end();
            cx.notify();
            return;
        }
        if let Some((id, _, position)) = self.guide_drag.take() {
            self.drag_start = None;
            if self.editor.move_guide(&id, position) {
                self.status = "Guide moved".into();
                self.changed(cx);
            } else {
                cx.notify();
            }
            return;
        }
        let Some(start) = self.drag_start.take() else {
            return;
        };
        let (mut x, mut y) = self.coordinates(event.position);
        if matches!(
            self.tool,
            Tool::Lasso | Tool::SpotHeal | Tool::BlurBrush | Tool::Smudge | Tool::Liquify
        ) || (self.paint_mask && matches!(self.tool, Tool::Clone | Tool::Heal))
        {
            self.capture_pointer_point((x, y));
            if let Some(error) = self.pointer_path_error.take() {
                self.status = error.into();
                self.selection_box = self
                    .editor
                    .selection
                    .as_ref()
                    .and_then(|s| s.bounds())
                    .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
                self.lasso.clear();
                cx.notify();
                return;
            }
        }
        if matches!(
            self.tool,
            Tool::Rectangle
                | Tool::Ellipse
                | Tool::Gradient
                | Tool::ShapeRect
                | Tool::ShapeEllipse
                | Tool::Line
        ) {
            let snapped = snap_canvas_point(
                &self.editor,
                CanvasPoint { x, y },
                self.preferences.snapping && self.show_grid,
                self.preferences.snapping && self.show_guides,
                6. / self.zoom,
                omuse::canvas_grid::GridSettings {
                    spacing: self.preferences.grid_spacing,
                    subdivisions: self.preferences.grid_subdivisions,
                },
                event.modifiers.shift,
            );
            x = snapped.x;
            y = snapped.y;
        }
        match self.tool {
            Tool::Text => {
                self.selection_box = None;
                if let Some(id) = self.text_hit_pending.take() {
                    self.begin_inline_text(Some(id), start, None, window, cx);
                    return;
                }
                let dragged =
                    (x - start.0).abs() * self.zoom > 4. && (y - start.1).abs() * self.zoom > 4.;
                let origin = if dragged {
                    (start.0.min(x), start.1.min(y))
                } else {
                    start
                };
                let box_size = dragged.then_some(objects::ObjectSize {
                    width: (x - start.0).abs().max(16.),
                    height: (y - start.1).abs().max(16.),
                });
                self.begin_inline_text(None, origin, box_size, window, cx);
                return;
            }
            Tool::SpotHeal => {
                match self
                    .editor
                    .spot_heal_stroke(&self.lasso, self.spot_healing_mode, 0)
                {
                    Ok(true) => self.status = "Spot healing applied".into(),
                    Ok(false) => self.status = "Spot healing made no change".into(),
                    Err(error) => self.status = format!("Spot healing: {error:#}"),
                }
                self.selection_box = None;
            }
            Tool::BlurBrush | Tool::Smudge | Tool::Liquify => {
                let mode = match self.tool {
                    Tool::Smudge => omuse::retouch_brush::RetouchMode::Smudge,
                    Tool::Liquify => omuse::retouch_brush::RetouchMode::Liquify,
                    _ => omuse::retouch_brush::RetouchMode::Blur,
                };
                self.selection_box = None;
                let points = std::mem::take(&mut self.lasso);
                self.start_background_retouch(points, mode, window, cx);
                return;
            }

            Tool::Lasso => {
                self.editor.select_polygon(&self.lasso);
                if let Some(incoming) = self.editor.selection.clone() {
                    self.editor.selection = Some(omuse::selection_tools::combine(
                        self.selection_before_gesture.as_ref(),
                        &incoming,
                        self.gesture_selection_mode,
                    ));
                }
                self.selection_box = self
                    .editor
                    .selection
                    .as_ref()
                    .and_then(|s| s.bounds())
                    .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
            }
            Tool::Brush | Tool::Pencil | Tool::Eraser => {
                self.editor.finish_stroke();
            }
            Tool::Clone | Tool::Heal => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    if let Some(chosen_source) = self.clone_source {
                        let source = if self.clone_aligned {
                            let offset = *self.clone_offset.get_or_insert((
                                chosen_source.0 - start.0,
                                chosen_source.1 - start.1,
                            ));
                            (start.0 + offset.0, start.1 + offset.1)
                        } else {
                            chosen_source
                        };
                        match self.editor.clone_mask_stroke(
                            &id,
                            source,
                            &self.lasso,
                            self.tool == Tool::Heal,
                        ) {
                            Ok(true) => self.status = "Mask clone stroke applied".into(),
                            Ok(false) => self.status = "Mask clone stroke made no change".into(),
                            Err(e) => self.status = format!("Mask clone: {e:#}"),
                        }
                    }
                    self.selection_box = None;
                } else {
                    self.editor.finish_clone_stroke();
                }
            }
            Tool::ShapeRect | Tool::ShapeEllipse | Tool::Line => {
                let (left, top, width, height) = (
                    start.0.min(x),
                    start.1.min(y),
                    (x - start.0).abs().max(1.),
                    (y - start.1).abs().max(1.),
                );
                let shape = match self.tool {
                    Tool::ShapeRect => Shape::Rectangle {
                        x: left,
                        y: top,
                        width,
                        height,
                    },
                    Tool::ShapeEllipse => Shape::Ellipse {
                        x: left,
                        y: top,
                        width,
                        height,
                    },
                    _ => Shape::Line {
                        x1: start.0,
                        y1: start.1,
                        x2: x,
                        y2: y,
                    },
                };
                let _ = shape; // The editable source uses layer-local geometry.
                let c = self.editor.brush.color;
                let is_line = self.tool == Tool::Line;
                let pad = if is_line {
                    self.shape_line_width * 0.5 + 1.
                } else {
                    0.
                };
                let iw = (width + 2. * pad).ceil() as u32;
                let ih = (height + 2. * pad).ceil() as u32;
                let mut layer = Layer::paint("Shape", 1, 1);
                layer.offset_x = left - pad;
                layer.offset_y = top - pad;
                layer.opacity = c[3] as f32 / 255.;
                let style = objects::LiveShapeStyle {
                    kind: match self.tool {
                        Tool::ShapeRect => objects::LiveShapeKind::Rectangle,
                        Tool::ShapeEllipse => objects::LiveShapeKind::Ellipse,
                        _ => objects::LiveShapeKind::Line,
                    },
                    red: c[0] as f32 / 255.,
                    green: c[1] as f32 / 255.,
                    blue: c[2] as f32 / 255.,
                    corner_radius: if self.tool == Tool::ShapeRect {
                        self.shape_corner_radius
                    } else {
                        0.
                    },
                    line_width: if is_line {
                        Some(self.shape_line_width)
                    } else {
                        None
                    },
                    start: if is_line {
                        Some(objects::ObjectPoint {
                            x: (start.0 - left + pad) / iw as f32,
                            y: (start.1 - top + pad) / ih as f32,
                        })
                    } else {
                        None
                    },
                    end: if is_line {
                        Some(objects::ObjectPoint {
                            x: (x - left + pad) / iw as f32,
                            y: (y - top + pad) / ih as f32,
                        })
                    } else {
                        None
                    },
                };
                match objects::set_live_shape(&mut layer, style, iw, ih) {
                    Ok(()) => {
                        if self.editor.import_layer(layer).is_empty() {
                            self.status =
                                "The shape could not be added within the document limits.".into();
                        }
                    }
                    Err(e) => self.status = format!("Shape: {e:#}"),
                }
                self.selection_box = None;
            }
            Tool::Rectangle => {
                self.editor.select_rectangle(
                    start.0.min(x),
                    start.1.min(y),
                    (x - start.0).abs(),
                    (y - start.1).abs(),
                );
                if let Some(incoming) = self.editor.selection.clone() {
                    self.editor.selection = Some(omuse::selection_tools::combine(
                        self.selection_before_gesture.as_ref(),
                        &incoming,
                        self.gesture_selection_mode,
                    ));
                }
            }
            Tool::Ellipse => {
                self.editor.select_ellipse(
                    start.0.min(x),
                    start.1.min(y),
                    (x - start.0).abs(),
                    (y - start.1).abs(),
                );
                if let Some(incoming) = self.editor.selection.clone() {
                    self.editor.selection = Some(omuse::selection_tools::combine(
                        self.selection_before_gesture.as_ref(),
                        &incoming,
                        self.gesture_selection_mode,
                    ));
                }
            }
            Tool::Gradient => {
                self.gradient_pending = Some((start, (x, y)));
                self.dialog = Dialog::Gradient;
                self.detail_inputs[0].update(cx, |state, cx| state.set_value("100", window, cx));
                self.preview_gradient(cx);
                self.selection_box = None;
            }
            Tool::Move => {
                if let Some(corners) = self.distort_draft.take() {
                    let id = self.editor.active_layer.clone();
                    let unchanged = self
                        .transform_drag
                        .as_ref()
                        .and_then(|drag| drag.original_corners)
                        == Some(corners);
                    let result = if unchanged {
                        Ok(false)
                    } else {
                        self.editor.distort_layer(&id, corners.map(|p| (p.x, p.y)))
                    };
                    match result {
                        Ok(true) => self.status = "Corner distortion applied".into(),
                        Ok(false) if unchanged => self.status = "Transform unchanged".into(),
                        Ok(false) => {
                            self.status = "Rasterize and unlock this layer before distorting".into()
                        }
                        Err(error) => self.status = format!("Distortion: {error:#}"),
                    }
                    self.transform_original_box = None;
                    self.transform_draft = None;
                }
                if let (Some(original), Some(draft)) = (
                    self.transform_original_box.take(),
                    self.transform_draft.take(),
                ) {
                    if !commit_drag(&mut self.editor, &self.layer_selection.ids, original, draft) {
                        self.status = "Transform blocked by a locked or invalid layer".into();
                    }
                }
                self.transform_drag = None;
                self.selection_box = None;
            }
            _ => {}
        }
        if matches!(self.tool, Tool::Rectangle | Tool::Ellipse | Tool::Lasso) {
            self.editor
                .record_selection_change(self.selection_before_gesture.take());
            self.selection_box = self
                .editor
                .selection
                .as_ref()
                .and_then(|s| s.bounds())
                .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
        }
        self.changed(cx);
    }
    fn middle_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.middle_pan_pointer.take().is_some() {
            cx.notify();
        }
    }
    fn request(&mut self, what: Pending, window: &mut Window, cx: &mut Context<Self>) {
        if self.image_trace_active() && !matches!(what, Pending::Quit) {
            self.guard_image_trace(cx);
            return;
        }
        if self.vector_before_request(what.clone(), window, cx) {
            return;
        }
        if !self.finish_inline_text(true, window, cx) {
            return;
        }
        self.crop = None;
        if self.dialog != Dialog::None {
            return;
        }
        if self.editor.floating_selection_layer().is_some() {
            self.status =
                "Commit Selection or Cancel Selection before changing files or quitting".into();
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        if self.create.saving {
            self.pending = Some(what);
            self.status = "Finishing the current save…".into();
            cx.notify();
            return;
        }
        if self.has_unsaved_work() {
            self.pending = Some(what);
            self.dialog = Dialog::Unsaved;
            self.modal_focus.focus(window, cx);
            cx.notify();
        } else {
            self.perform(what, window, cx);
        }
    }
    fn perform(&mut self, what: Pending, window: &mut Window, cx: &mut Context<Self>) {
        self.pending = None;
        match what {
            Pending::OpenPath(path) => {
                self.dialog = Dialog::Open;
                self.dialog_generation += 1;
                self.open_photo_background(path, false, window, cx);
            }
            Pending::New => {
                self.dialog = Dialog::New;
                self.modal_focus.focus(window, cx);
            }
            Pending::Open => {
                self.open_dialog(false, window, cx);
            }
            Pending::Quit => {
                self.recovery.clear();
                window.remove_window();
            }
        }
        cx.notify();
    }
    fn open_dialog(&mut self, import: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.crop.is_some() {
            self.status = "Apply or cancel the crop before opening or importing artwork".into();
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        self.dialog_generation += 1;
        self.save_confirmation = None;
        self.dialog = if import { Dialog::Import } else { Dialog::Open };
        self.path_input.update(cx, |state, cx| {
            state.set_placeholder("File path", window, cx);
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }
    fn native_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.photo_io.is_some() {
            self.status =
                "An image operation is already running; its destination is locked.".into();
            cx.notify();
            return;
        }
        let mode = self.dialog;
        let generation = self.dialog_generation;
        if mode == Dialog::Open || mode == Dialog::Import {
            let task = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: mode == Dialog::Open,
                multiple: false,
                prompt: Some("Open in Omuse".into()),
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = task.await;
                let _ = view.update_in(cx, |this, window, cx| {
                    if this.dialog != mode || this.dialog_generation != generation {
                        return;
                    }
                    match result {
                        Ok(Ok(Some(paths))) if !paths.is_empty() => {
                            this.path_input.update(cx, |input, cx| {
                                input.set_value(paths[0].to_string_lossy().to_string(), window, cx)
                            });
                            cx.notify();
                        }
                        Ok(Err(e)) => {
                            this.status = format!("File chooser: {e}. Enter a path below.");
                            cx.notify();
                        }
                        _ => {}
                    }
                });
            })
            .detach();
        } else {
            let dir = omuse::identity::media_dir(
                omuse::identity::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")),
                omuse::identity::MediaFolder::Pictures,
            );
            let name = if mode == Dialog::Export {
                "Untitled.png"
            } else {
                "Untitled.omuse"
            };
            let task = cx.prompt_for_new_path(&dir, Some(name));
            cx.spawn_in(window, async move |view, cx| {
                let result = task.await;
                let _ = view.update_in(cx, |this, window, cx| {
                    if this.dialog != mode || this.dialog_generation != generation {
                        return;
                    }
                    match result {
                        Ok(Ok(Some(path))) => {
                            this.path_input.update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().to_string(), window, cx)
                            });
                            cx.notify();
                        }
                        Ok(Err(e)) => {
                            this.status = format!("File chooser: {e}. Enter a path below.");
                            cx.notify();
                        }
                        _ => {}
                    }
                });
            })
            .detach();
        }
    }
    fn save_dialog(&mut self, export: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.crop.is_some() {
            self.status = "Apply or cancel the crop before saving or exporting".into();
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        self.dialog_generation += 1;
        self.save_confirmation = None;
        self.dialog = if export { Dialog::Export } else { Dialog::Save };
        let default = self
            .path
            .as_ref()
            .map(|p| {
                if export {
                    p.with_extension("png")
                } else {
                    document::project_save_path(p).unwrap_or_else(|_| p.clone())
                }
            })
            .unwrap_or_else(|| {
                omuse::identity::media_dir(
                    omuse::identity::home_dir().unwrap_or_default(),
                    omuse::identity::MediaFolder::Pictures,
                )
                .join(if export {
                    "Untitled.png"
                } else {
                    "Untitled.omuse"
                })
            });
        self.path_input.update(cx, |state, cx| {
            state.set_placeholder("File path", window, cx);
            state.set_value(default.to_string_lossy().to_string(), window, cx);
            state.focus(window, cx);
        });
        if !export
            && self
                .path
                .as_ref()
                .is_some_and(|path| !document::is_omuse_path(path))
        {
            self.status = "Save an .omuse copy. Your original project will be kept.".into();
        }
        if export {
            self.jpeg_preview = None;
            self.jpeg_inspection = jpeg_preview_ui::JpegInspection::default();
            let dpi = self
                .editor
                .document
                .metadata
                .get("resolution")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && (1. ..=9600.).contains(v))
                .unwrap_or(72.);
            self.detail_inputs[4]
                .update(cx, |state, cx| state.set_value(dpi.to_string(), window, cx));
            for (i, value) in ["80", "255", "255", "255"].iter().enumerate() {
                self.detail_inputs[i].update(cx, |state, cx| state.set_value(*value, window, cx));
            }
        }
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.create.saving {
            self.save_again = true;
            self.status = "The latest edits will save after the current snapshot finishes.".into();
            cx.notify();
            return;
        }
        if self.crop.is_some() {
            self.status = "Apply or cancel the crop before saving".into();
            cx.notify();
            return;
        }
        self.finish_interaction(cx);
        if let Some(path) = self.path.clone().filter(|p| document::is_omuse_path(p)) {
            self.save_to(path, window, cx);
        } else {
            self.save_dialog(false, window, cx);
        }
    }
    /// Saving publishes a checked snapshot and cannot be cancelled after it has
    /// entered the worker.  Keep a pending navigation and its document intact
    /// until that worker reports its result.
    fn saving_blocks_navigation(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.create.saving {
            return false;
        }
        self.status = "Saving… Wait for the current save to finish.".into();
        cx.notify();
        true
    }
    fn save_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.create.saving {
            self.status = "A save is already running.".into();
            cx.notify();
            return;
        }
        let path = match document::project_save_path(&path) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let current_stamp = project_stamp(&path);
        if self.path.as_ref() == Some(&path)
            && self.live_stamp.is_some()
            && current_stamp != self.live_stamp
        {
            self.status = "The project changed on disk. Use Save as to keep both versions.".into();
            cx.notify();
            return;
        }
        // A legacy project can have a newer .omuse sibling already. Confirm the
        // exact destination and disk version before replacing another project.
        let expected_stamp = if self.path.as_ref() != Some(&path)
            && std::fs::symlink_metadata(&path).is_ok()
        {
            let Some(stamp) = current_stamp else {
                self.save_confirmation = None;
                self.status =
                    "Cannot safely replace this destination. Choose another project name.".into();
                cx.notify();
                return;
            };
            let approved = self.save_confirmation.as_ref().is_some_and(|confirmation| {
                self.dialog == Dialog::Save
                    && confirmation.dialog_generation == self.dialog_generation
                    && confirmation.path == path
                    && confirmation.stamp == stamp
            });
            if !approved {
                if self.dialog != Dialog::Save {
                    self.dialog_generation += 1;
                    self.dialog = Dialog::Save;
                }
                self.path_input.update(cx, |input, cx| {
                    input.set_value(path.to_string_lossy().to_string(), window, cx);
                    input.focus(window, cx);
                });
                self.save_confirmation = Some(SaveConfirmation {
                    path: path.clone(),
                    stamp,
                    dialog_generation: self.dialog_generation,
                });
                self.status = format!(
                    "{} already exists. Choose another name, or Replace project to overwrite it.",
                    path.display()
                );
                cx.notify();
                return;
            }
            Some(stamp)
        } else {
            current_stamp
        };
        self.save_confirmation = None;
        let result = self.save_content_background(path, expected_stamp, window, cx);
        self.create_error(result, cx);
        cx.notify();
    }
    fn confirming_project_replacement(&self, cx: &App) -> bool {
        self.dialog == Dialog::Save
            && self.save_confirmation.as_ref().is_some_and(|confirmation| {
                confirmation.dialog_generation == self.dialog_generation
                    && document::project_save_path(std::path::Path::new(
                        self.path_input.read(cx).value().trim(),
                    ))
                    .is_ok_and(|path| path == confirmation.path)
            })
    }
    fn confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.path_input.read(cx).value().to_string();
        let path = PathBuf::from(value.trim());
        if self.busy {
            // A range output change only regenerates its thumbnail when the
            // raw mask is cached. Apply can use that mask immediately, or own
            // the pending computation if its settings are still being read.
            if self.dialog == Dialog::RangeMask {
                self.run_range(true, cx);
            }
            return;
        }
        if matches!(self.dialog, Dialog::Open | Dialog::Import) && omuse::raw_import::matches(&path)
        {
            self.raw_open = self.dialog == Dialog::Open;
            self.dialog = Dialog::RawImport;
            for (i, value) in ["0", "5000", "0", "1"].iter().enumerate() {
                self.detail_inputs[i].update(cx, |s, cx| s.set_value(*value, window, cx));
            }
            self.status = "As-shot white balance is used at the default settings.".into();
            cx.notify();
            return;
        }
        match self.dialog {
            Dialog::SvgImport => self.apply_svg_import(window, cx),
            Dialog::RawImport => {
                let values: Option<Vec<f32>> = self.detail_inputs[..4]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                if let Some(v) = values {
                    self.start_raw_import(
                        path,
                        omuse::raw_import::DevelopSettings {
                            exposure: v[0],
                            temperature: v[1],
                            tint: v[2],
                            boost: v[3],
                            ..Default::default()
                        },
                        cx,
                    );
                } else {
                    self.status = "Enter numeric RAW development settings".into();
                }
            }
            Dialog::Guide => {
                let axis = match self.detail_inputs[0]
                    .read(cx)
                    .value()
                    .to_lowercase()
                    .as_str()
                {
                    "vertical" => Some(omuse::editor::GuideAxis::Vertical),
                    "horizontal" => Some(omuse::editor::GuideAxis::Horizontal),
                    _ => None,
                };
                if let (Some(axis), Ok(pos)) =
                    (axis, self.detail_inputs[1].read(cx).value().parse::<f32>())
                {
                    if self.editor.add_guide(axis, pos).is_some() {
                        self.show_guides = true;
                        self.persist_preferences(cx);
                        self.dialog = Dialog::None;
                        self.changed(cx);
                    } else {
                        self.status = "Invalid guide position".into();
                    }
                } else {
                    self.status = "Enter horizontal or vertical, and a numeric position".into();
                }
            }
            Dialog::MaskTransform => {
                let values: Option<Vec<f32>> = self.detail_inputs[..9]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                let id = self.editor.active_layer.clone();
                if let (Some(v), Some(mut p)) = (values, self.editor.mask_placement(&id)) {
                    p.x = v[0];
                    p.y = v[1];
                    p.width = v[2];
                    p.height = v[3];
                    p.rotation = v[4];
                    // Apply placement and link state together through one editor transaction.
                    if self.editor.set_mask_placement(&id, p) {
                        self.dialog = Dialog::None;
                        self.changed(cx);
                    } else {
                        self.status =
                            "Unlink the mask first, and enter valid changed dimensions".into();
                    }
                } else {
                    self.status = "Enter numeric mask coordinates".into();
                }
            }
            Dialog::Effects => {
                let values: Option<Vec<f32>> = self.detail_inputs[..4]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                if let Some(v) = values {
                    let id = self.editor.active_layer.clone();
                    let mut effects = self
                        .editor
                        .document
                        .find_layer(&id)
                        .and_then(|l| l.metadata.get("effects"))
                        .filter(|v| v.is_object())
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    let c = self.dialog_color;
                    let mut effect = serde_json::json!({"enabled":true,"opacity":v[0]/100.,"red":c[0] as f32/255.,"green":c[1] as f32/255.,"blue":c[2] as f32/255.});
                    match self.effect_kind {
                        0 => {
                            effect["size"] = v[1].into();
                            effect["inside"] =
                                (self.detail_inputs[4].read(cx).value().as_ref() == "1").into();
                        }
                        1 | 3 => {
                            effect["blur"] = v[1].into();
                            effect["angle"] = v[2].into();
                            effect["distance"] = v[3].into();
                        }
                        4 | 5 => {
                            effect["size"] = v[1].into();
                        }
                        _ => {}
                    }
                    effects[effect_key(self.effect_kind)] = effect;
                    match self.editor.set_layer_effects(&id, effects) {
                        Ok(_) => {
                            self.dialog = Dialog::None;
                            self.changed(cx);
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                } else {
                    self.status = "Enter numeric effect values".into();
                }
            }
            Dialog::CameraRaw => {
                match self.read_camera_form(cx).and_then(|_| {
                    serde_json::from_value(self.camera_draft.clone()).map_err(Into::into)
                }) {
                    Ok(settings) => self.start_camera_raw(settings, false, cx),
                    Err(e) => self.status = format!("Development settings: {e:#}"),
                }
            }
            Dialog::SubjectRefine => self.run_subject_refine(true, window, cx),
            Dialog::RangeMask => self.run_range(true, cx),
            Dialog::VectorPath => self.apply_vector(cx),
            Dialog::Workflow => self.run_workflow(true, window, cx),
            Dialog::Pro => self.run_pro(true, cx),
            Dialog::Finishing => self.run_finishing(true, cx),
            Dialog::Gradient => {
                let opacity = self.detail_inputs[0].read(cx).value().parse::<f32>();
                if let Ok(opacity) = opacity
                    && opacity.is_finite()
                    && (0. ..=100.).contains(&opacity)
                {
                    self.gradient_settings.opacity = opacity / 100.;
                    let Some((start, end)) = self.gradient_pending else {
                        self.dialog = Dialog::None;
                        self.refresh(cx);
                        return;
                    };
                    let result = if self.paint_mask {
                        self.editor.gradient_mask_with(
                            &self.editor.active_layer.clone(),
                            start,
                            end,
                            mask_value(self.editor.brush.color),
                            mask_value(self.background_color),
                            &self.gradient_settings,
                        )
                    } else {
                        self.editor.gradient_with(
                            start,
                            end,
                            self.editor.brush.color,
                            self.background_color,
                            &self.gradient_settings,
                        )
                    };
                    match result {
                        Ok(true) => {
                            self.gradient_pending = None;
                            self.dialog = Dialog::None;
                            self.status = "Gradient applied".into();
                            self.changed(cx);
                        }
                        Ok(false) => self.status = "Gradient made no change".into(),
                        Err(error) => self.status = format!("Gradient: {error:#}"),
                    }
                } else {
                    self.status = "Use gradient opacity from 0 to 100".into();
                }
            }
            Dialog::ToolSettings => {
                let values: Option<Vec<f32>> = self.detail_inputs[..9]
                    .iter()
                    .map(|input| input.read(cx).value().parse::<f32>().ok())
                    .collect();
                if let Some(values) = values
                    && values.iter().all(|value| value.is_finite())
                    && (1. ..=1024.).contains(&values[0])
                    && (0. ..=100.).contains(&values[1])
                    && (0. ..=100.).contains(&values[2])
                    && (0. ..=100.).contains(&values[3])
                    && (0. ..=255.).contains(&values[4])
                    && (0. ..=255.).contains(&values[5])
                    && (0. ..=2.).contains(&values[6])
                    && values[6].fract() == 0.
                    && (0. ..=5000.).contains(&values[7])
                    && (1. ..=5000.).contains(&values[8])
                {
                    self.editor.brush.size = values[0];
                    self.editor.brush.hardness = values[1] / 100.;
                    self.editor.brush.opacity = values[2] / 100.;
                    self.editor.brush.smoothing = values[3];
                    self.fill_tolerance = values[4].round() as u8;
                    self.wand_draft.tolerance = values[5].round() as u8;
                    self.wand_draft.sample_radius = values[6] as u8;
                    self.shape_corner_radius = values[7];
                    self.shape_line_width = values[8];
                    self.wand_settings = self.wand_draft;
                    self.selection_mode = self.selection_mode_draft;
                    self.clone_aligned = self.clone_aligned_draft;
                    self.clone_all_layers = self.clone_all_layers_draft;
                    if !self.clone_aligned {
                        self.clone_offset = None;
                    }
                    self.dialog = Dialog::None;
                    self.status = "Tool settings updated".into();
                    cx.notify();
                } else {
                    self.status = "Use size 1–1024, percentages 0–100, tolerance 0–255, wand radius 0–2, corner radius 0–5000, and line width 1–5000".into();
                }
            }
            Dialog::Selection => {
                if let Ok(v) = self.detail_inputs[0].read(cx).value().parse::<f32>() {
                    if v.is_finite() && (0. ..=128.).contains(&v) {
                        let previous = self.editor.selection.clone();
                        let changed = if self.selection_operation == 0 {
                            self.editor.feather_selection(v)
                        } else {
                            self.editor
                                .resize_selection(v.round() as i32 * self.selection_operation)
                        };
                        if changed {
                            self.editor.record_selection_change(previous);
                            self.selection_box = self
                                .editor
                                .selection
                                .as_ref()
                                .and_then(|s| s.bounds())
                                .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
                            self.dialog = Dialog::None;
                        } else {
                            self.status =
                                "Selection unchanged or radius too large for this image".into();
                        }
                    } else {
                        self.status = "Use a radius from 0 to 128 pixels".into();
                    }
                } else {
                    self.status = "Enter a numeric selection radius".into();
                }
            }
            Dialog::Adjustment => {
                let mut value = self.adjustment_draft.clone();
                let mut valid = true;
                for (i, f) in crate::adjustment_controls::fields(self.adjustment_kind)
                    .iter()
                    .enumerate()
                {
                    if f.boolean {
                        continue;
                    }
                    match self.detail_inputs[i].read(cx).value().parse::<f64>() {
                        Ok(v) if v.is_finite() => {
                            let n = if f.path.ends_with("Seed") || f.path.ends_with("/seed") {
                                if v < 0. || v > u32::MAX as f64 || v.fract() != 0. {
                                    valid = false;
                                    break;
                                }
                                serde_json::json!(v as u32)
                            } else {
                                serde_json::json!(v)
                            };
                            crate::adjustment_controls::set(&mut value, &f.path, n);
                        }
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }
                if self.adjustment_kind == 3 {
                    for i in 0..4 {
                        match crate::adjustment_controls::parse_curve(
                            &self.detail_inputs[i].read(cx).value(),
                        ) {
                            Ok(points) => value["curves"]["channels"][i] = points,
                            Err(e) => {
                                self.status = e.to_string();
                                valid = false;
                            }
                        }
                    }
                }
                if valid {
                    let result = if let Some(id) = self.editing_object.clone() {
                        self.editor.set_adjustment(&id, value).map(|_| ())
                    } else {
                        self.editor.add_adjustment(value).map(|_| ())
                    };
                    match result {
                        Ok(()) => {
                            self.dialog = Dialog::None;
                            self.editing_object = None;
                            self.changed(cx);
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                } else if self.adjustment_kind != 3 {
                    self.status =
                        "Enter valid finite numeric values (seeds are whole numbers)".into();
                }
            }
            Dialog::Distort => {
                let values: Option<Vec<f32>> = self.detail_inputs[..8]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                if let Some(v) = values {
                    let id = self.editor.active_layer.clone();
                    match self.editor.distort_layer(
                        &id,
                        [(v[0], v[1]), (v[2], v[3]), (v[4], v[5]), (v[6], v[7])],
                    ) {
                        Ok(true) => {
                            self.dialog = Dialog::None;
                            self.changed(cx);
                        }
                        Ok(false) => {
                            self.status =
                                "Rasterize the layer first, and unlock it before distorting".into()
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                } else {
                    self.status = "Enter numeric corner coordinates".into();
                }
            }
            Dialog::Transform => {
                let values: Option<Vec<f32>> = self.detail_inputs[..5]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                if let Some(v) = values.filter(|v| v.iter().all(|n| n.is_finite())) {
                    let id = self.editor.active_layer.clone();
                    if self
                        .editor
                        .transform_layer(&id, v[0], v[1], v[2], v[3] / 100., v[4] / 100.)
                    {
                        self.tool = Tool::Move;
                        self.layer_selection.click(id, SelectionAction::Replace);
                        self.dialog = Dialog::None;
                        self.changed(cx);
                    } else {
                        self.status = "Transform unchanged, invalid, or layer locked".into();
                    }
                } else {
                    self.status = "Enter finite numeric transform values".into();
                }
            }
            Dialog::Shortcuts => match self.shortcut_draft.save(&shortcuts::settings_path()) {
                Ok(()) => {
                    install_shortcuts(&self.shortcut_draft, &self.shortcuts, cx);
                    self.shortcuts = self.shortcut_draft.clone();
                    self.recording = None;
                    self.dialog = Dialog::None;
                    self.status = "Keyboard shortcuts saved".into();
                }
                Err(e) => self.status = format!("Could not save shortcuts: {e:#}"),
            },
            Dialog::Save => {
                if !value.trim().is_empty() {
                    self.save_to(path, window, cx);
                } else {
                    self.status = "Choose a project path".into();
                }
            }
            Dialog::Export => {
                let is_jpeg = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("jpg")
                            || extension.eq_ignore_ascii_case("jpeg")
                    });
                let values = if is_jpeg {
                    let values: Option<Vec<u8>> = self.detail_inputs[..4]
                        .iter()
                        .map(|input| input.read(cx).value().parse().ok())
                        .collect();
                    let Some(values) = values.filter(|values| (1..=100).contains(&values[0]))
                    else {
                        self.status =
                            "JPEG quality must be 1–100 and matte RGB must be 0–255".into();
                        cx.notify();
                        return;
                    };
                    values
                } else {
                    vec![80, 255, 255, 255]
                };
                let Ok(dpi) = self.detail_inputs[4].read(cx).value().parse::<f64>() else {
                    self.status = "Resolution must be 1–9600 DPI".into();
                    cx.notify();
                    return;
                };
                if !dpi.is_finite() || !(1. ..=9600.).contains(&dpi) {
                    self.status = "Resolution must be 1–9600 DPI".into();
                    cx.notify();
                    return;
                }
                let mut export_document = self.editor.document.clone();
                export_document.metadata["resolution"] = serde_json::json!(dpi);
                self.export_photo_background(
                    export_document,
                    path,
                    raster::ExportOptions {
                        jpeg_quality: values[0],
                        matte: [values[1], values[2], values[3]],
                    },
                    window,
                    cx,
                );
            }
            Dialog::Open => self.open_photo_background(path, false, window, cx),
            Dialog::Import => self.open_photo_background(path, true, window, cx),
            Dialog::Trim => {
                if self.editor.trim_canvas(self.trim_options) {
                    self.dialog = Dialog::None;
                    self.status = "Canvas trimmed; off-canvas artwork is retained".into();
                    self.changed(cx);
                } else {
                    self.status = "No trim boundary, no selected sides, or locked content".into();
                    cx.notify();
                }
            }
            Dialog::New | Dialog::Resize | Dialog::ResizeImage => {
                let dimensions = if self.dialog == Dialog::Resize {
                    self.canvas_dimensions(cx).and_then(|(w, h)| {
                        if w.is_finite() && h.is_finite() && w.round() >= 1. && h.round() >= 1. {
                            Some((w.round() as u32, h.round() as u32))
                        } else {
                            None
                        }
                    })
                } else {
                    self.width_input
                        .read(cx)
                        .value()
                        .parse::<u32>()
                        .ok()
                        .zip(self.height_input.read(cx).value().parse::<u32>().ok())
                };
                if let Some((w, h)) = dimensions.filter(|(w, h)| {
                    *w > 0
                        && *h > 0
                        && *w <= 8192
                        && *h <= 8192
                        && u64::from(*w) * u64::from(*h) <= 16_777_216
                }) {
                    if self.dialog == Dialog::New {
                        self.dialog_generation += 1;
                        self.install_opened_content(Document::new(w, h), None);
                        self.path = None;
                        self.pan = (0., 0.);
                        self.selection_box = None;
                        self.recovery.clear();
                    } else if self.dialog == Dialog::ResizeImage {
                        let resolution = self.detail_inputs[0].read(cx).value().parse::<f64>();
                        let Ok(resolution) = resolution else {
                            self.status = "Enter a resolution from 1 to 9600 DPI".into();
                            return;
                        };
                        if !resolution.is_finite() || !(1. ..=9600.).contains(&resolution) {
                            self.status = "Enter a resolution from 1 to 9600 DPI".into();
                            return;
                        }
                        let sampling =
                            ["Nearest", "Smooth", "High quality"][self.resize_sampling.min(2)];
                        let pixel_resize =
                            (w, h) != (self.editor.document.width, self.editor.document.height);
                        if !self
                            .editor
                            .resize_image_with_options(w, h, resolution, sampling)
                        {
                            self.status =
                                "Resize unchanged, locked or unsupported. Rotated editable sources require proportional scaling; rasterize a copy for a shearing resize.".into();
                            cx.notify();
                            return;
                        }
                        self.resize_resolution = resolution;
                        self.status = if pixel_resize {
                            "Image resized; editable text and shapes were rasterized"
                        } else {
                            "Image resolution updated"
                        }
                        .into();
                    } else {
                        let fill = match self.canvas_fill {
                            1 => Some(self.editor.brush.color),
                            2 => Some(self.background_color),
                            3 => Some([0, 0, 0, 255]),
                            4 => Some([255; 4]),
                            5 => Some(self.dialog_color),
                            _ => None,
                        };
                        if !self
                            .editor
                            .resize_canvas_anchored(w, h, self.canvas_anchor, fill)
                        {
                            self.status =
                                "Canvas unchanged, locked or exceeds processing limits".into();
                            cx.notify();
                            return;
                        }
                    }
                    self.dialog = Dialog::None;
                    self.changed(cx);
                } else {
                    self.status = "Use dimensions from 1 to 8192; at most 16 million pixels".into();
                }
            }
            Dialog::Filter => {
                let values: Option<Vec<f32>> = self
                    .filter_inputs
                    .iter()
                    .map(|f| f.read(cx).value().parse::<f32>().ok())
                    .collect();
                if let Some(v) = values {
                    let filter = make_filter(self.filter_kind, &v);
                    if let Err(e) = omuse::filters::validate(&filter) {
                        self.status = format!("Invalid adjustment: {e:#}");
                    } else if self.live_filter {
                        let result =
                            omuse::effects::adjustment_for_filter(&filter).and_then(|adjustment| {
                                if let Some(id) = self.editing_object.as_ref() {
                                    self.editor.set_adjustment(id, adjustment).map(|_| ())
                                } else {
                                    self.editor.add_adjustment(adjustment).map(|_| ())
                                }
                            });
                        match result {
                            Ok(()) => {
                                self.dialog = Dialog::None;
                                self.editing_object = None;
                                self.status = "Live adjustment saved".into();
                                self.changed(cx);
                            }
                            Err(e) => self.status = format!("Live adjustment: {e:#}"),
                        }
                    } else if self.editor.apply_filter(&filter) {
                        self.status = "Adjustment applied".into();
                        self.record_recipe_step(omuse::recipes::Step::Filter {
                            filter: filter.clone(),
                        });
                        self.dialog = Dialog::None;
                        self.changed(cx);
                    } else {
                        self.status = "Select an unlocked paint layer to adjust".into();
                    }
                } else {
                    self.status = "Enter numeric adjustment values".into();
                }
            }
            Dialog::Text => {
                let fields: Option<Vec<f32>> = [0usize, 2, 3, 5, 6]
                    .iter()
                    .map(|i| self.detail_inputs[*i].read(cx).value().parse().ok())
                    .collect();
                if let Some(v) = fields {
                    let c = self.dialog_color;
                    let mut style = self
                        .editing_object
                        .as_ref()
                        .and_then(|id| self.editor.document.find_layer(id))
                        .and_then(|l| objects::live_text(l).ok().flatten())
                        .unwrap_or_default();
                    objects::set_text_content(&mut style, value);
                    style.font_size = v[0];
                    style.font_name = self.detail_inputs[1].read(cx).value().to_string();
                    style.tracking = v[1];
                    style.leading = v[2];
                    style.red = c[0] as f32 / 255.;
                    style.green = c[1] as f32 / 255.;
                    style.blue = c[2] as f32 / 255.;
                    style.alignment = match self.detail_inputs[4]
                        .read(cx)
                        .value()
                        .to_lowercase()
                        .as_str()
                    {
                        "center" => objects::TextAlignment::Center,
                        "right" => objects::TextAlignment::Right,
                        _ => objects::TextAlignment::Left,
                    };
                    style.box_size = if v[3] > 0. && v[4] > 0. {
                        Some(objects::ObjectSize {
                            width: v[3],
                            height: v[4],
                        })
                    } else {
                        None
                    };
                    let result = if let Some(id) = self.editing_object.clone() {
                        if !editable_text(&self.editor.document.layers, &id, true, false) {
                            self.status =
                                "Unlock and show the text layer and its parent groups before editing"
                                    .into();
                            cx.notify();
                            return;
                        }
                        self.editor.set_live_text(&id, style).map(|_| ())
                    } else {
                        let mut l = Layer::paint("Text", 1, 1);
                        let origin = self.text_origin.unwrap_or((32., 32.));
                        l.offset_x = origin.0;
                        l.offset_y = origin.1;
                        objects::set_live_text(&mut l, style).and_then(|_| {
                            anyhow::ensure!(
                                !self.editor.import_layer(l).is_empty(),
                                "The text could not be added within the document limits"
                            );
                            Ok(())
                        })
                    };
                    match result {
                        Ok(()) => {
                            self.dialog = Dialog::None;
                            self.editing_object = None;
                            self.text_origin = None;
                            self.changed(cx);
                        }
                        Err(e) => self.status = format!("Text: {e:#}"),
                    }
                } else {
                    self.status = "Enter numeric size, spacing and box dimensions".into();
                }
            }
            Dialog::Shape => {
                let id = self.editor.active_layer.clone();
                let values: Option<Vec<f32>> = self.detail_inputs[..4]
                    .iter()
                    .map(|s| s.read(cx).value().parse().ok())
                    .collect();
                if let (Some(v), Some(mut style)) = (
                    values,
                    self.editor
                        .document
                        .find_layer(&id)
                        .and_then(|l| objects::live_shape(l).ok().flatten()),
                ) {
                    if v.iter().all(|v| v.is_finite())
                        && v[0] >= 1.
                        && v[1] >= 1.
                        && v[0] <= 8192.
                        && v[1] <= 8192.
                    {
                        style.corner_radius = v[2];
                        if style.kind == objects::LiveShapeKind::Line {
                            style.line_width = Some(v[3]);
                        }
                        let c = self.dialog_color;
                        style.red = c[0] as f32 / 255.;
                        style.green = c[1] as f32 / 255.;
                        style.blue = c[2] as f32 / 255.;
                        match self
                            .editor
                            .set_live_shape(&id, style, v[0] as u32, v[1] as u32)
                        {
                            Ok(_) => {
                                self.dialog = Dialog::None;
                                self.changed(cx);
                            }
                            Err(e) => self.status = e.to_string(),
                        }
                    } else {
                        self.status = "Invalid shape dimensions".into();
                    }
                } else {
                    self.status = "Enter numeric shape values".into();
                }
            }
            Dialog::Rename => {
                let id = self.editor.active_layer.clone();
                if !value.trim().is_empty() {
                    self.editor.rename_layer(&id, value.trim());
                    self.dialog = Dialog::None;
                    self.changed(cx);
                }
            }
            _ => {}
        }
        if self.dialog == Dialog::None {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
    fn command(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline_text.is_some() {
            self.status = "Finish text with Ctrl+Enter, or cancel with Escape".into();
            cx.notify();
            return;
        }
        if self.busy || self.dialog != Dialog::None {
            return;
        }
        if self.image_trace_active() && !Self::trace_command_allowed(name) {
            self.guard_image_trace(cx);
            return;
        }
        if self.vector_before_command(name, window, cx) {
            return;
        }
        if self.crop.is_some() {
            if name.starts_with("nudge-") {
                let step = if name.ends_with("-large") { 10. } else { 1. };
                let (dx, dy) = if name.starts_with("nudge-left") {
                    (-step, 0.)
                } else if name.starts_with("nudge-right") {
                    (step, 0.)
                } else if name.starts_with("nudge-up") {
                    (0., -step)
                } else {
                    (0., step)
                };
                self.crop.as_mut().unwrap().nudge(dx, dy);
                cx.notify();
                return;
            }
            if name == "crop" {
                return;
            }
            if !matches!(
                name,
                "zoom-in"
                    | "zoom-in-plus"
                    | "zoom-out"
                    | "fit"
                    | "actual"
                    | "toggle-panels"
                    | "command-search"
                    | "quit"
            ) {
                self.status = "Apply or cancel the crop before another editing command".into();
                cx.notify();
                return;
            }
        }
        self.finish_interaction(cx);
        if self.editor.floating_selection_layer().is_some()
            && (name.starts_with("tool-")
                || matches!(
                    name,
                    "new" | "open" | "save" | "save-as" | "export" | "import" | "quit"
                ))
        {
            self.status =
                "Commit Selection or Cancel Selection before changing tools or files".into();
            cx.notify();
            return;
        }
        if self.handle_keyboard_command(name, window, cx) {
            return;
        }
        if let Some(reason) = self.layer_action_unavailable(name) {
            self.status = reason.into();
            cx.notify();
            return;
        }
        match name {
            "command-search" => self.open_command_search(window, cx),
            "ask-omuse" => {
                self.inspector_tab = studio_ui::InspectorTab::Assistant;
                self.inspector_visible = true;
                self.discover_ai_connections(cx);
                self.focus_ai_prompt(window, cx);
            }
            "create-workspace" => {
                self.inspector_tab = studio_ui::InspectorTab::Create;
                self.inspector_visible = true;
            }
            "previous-page" | "next-page" => {
                if let Some(session) = &self.create.session {
                    let pages = session.project.page_ids();
                    let index = pages
                        .iter()
                        .position(|id| id == session.project.active_page_id())
                        .unwrap_or(0);
                    let target = if name == "next-page" {
                        index.saturating_add(1).min(pages.len() - 1)
                    } else {
                        index.saturating_sub(1)
                    };
                    let id = pages[target].clone();
                    let result = self.activate_page(&id, cx);
                    self.create_error(result, cx);
                }
            }
            "filter-stack" => self.open_pro(advanced_ui::Kind::Stack, window, cx),
            "blend-if" => self.open_pro(advanced_ui::Kind::Blend, window, cx),
            "advanced-retouch" => self.open_pro(advanced_ui::Kind::Retouch, window, cx),
            "controlled-removal" => self.open_pro(advanced_ui::Kind::Remove, window, cx),
            "editable-warp" => self.open_pro(advanced_ui::Kind::Warp, window, cx),
            "refine-workspace" => self.open_pro(advanced_ui::Kind::Refine, window, cx),
            "brush-studio" => self.open_pro(advanced_ui::Kind::Brush, window, cx),
            "smart-source" => self.open_workflow(workflow_ui::WorkflowKind::Source, window, cx),
            "editable-raw" => self.open_workflow(workflow_ui::WorkflowKind::Raw, window, cx),
            "colour-management" => {
                self.open_workflow(workflow_ui::WorkflowKind::Colour, window, cx)
            }
            "automation" => self.open_workflow(workflow_ui::WorkflowKind::Automation, window, cx),
            "multi-image" => self.open_workflow(workflow_ui::WorkflowKind::Merge, window, cx),
            "image-trace" => self.open_image_trace(window, cx),
            "vector-path" => {
                self.open_vector_scene(window, cx);
                self.vector_before_command("vector-path", window, cx);
            }
            "vector-scene" => self.open_vector_scene(window, cx),
            "vector-nodes" => {
                self.open_vector_scene(window, cx);
                self.vector_before_command("vector-nodes", window, cx);
            }
            "vector-mask" => self.open_vector(true, window, cx),
            id if shortcuts::definition(id)
                .is_some_and(|definition| definition.category == "Vector") =>
            {
                self.status =
                    "Open vector artwork with Shift+P, then select the objects to edit.".into();
                cx.notify();
            }
            "import-report" => {
                self.import_notes = omuse::import_report::conversion_notes(&self.editor.document);
                if self.import_notes.is_empty() {
                    self.status = "No import conversion notes for this document".into();
                } else {
                    self.dialog = Dialog::ImportReport;
                }
                cx.notify();
            }
            "export-report" => {
                if self.export_notes.is_empty() {
                    self.status = "No export conversion report in this session".into();
                } else {
                    self.dialog = Dialog::ExportReport;
                    self.modal_focus.focus(window, cx);
                }
                cx.notify();
            }
            "transform-selection" if self.paint_mask => {
                self.status =
                    "Transform Selection is unavailable while editing a mask; use Place Mask"
                        .into();
                cx.notify();
            }
            "transform-selection" => match self.editor.begin_floating_selection() {
                Ok(Some(id)) => {
                    self.editor.active_layer = id.clone();
                    self.layer_selection.click(id, SelectionAction::Replace);
                    self.tool = Tool::Move;
                    self.status =
                        "Selection is floating — transform it, then commit or cancel".into();
                    self.changed(cx);
                }
                Ok(None) => {
                    self.status = "Select pixels before transforming the selection".into();
                    cx.notify();
                }
                Err(e) => {
                    self.status = format!("Transform selection: {e:#}");
                    cx.notify();
                }
            },
            "commit-selection" => match self.editor.commit_floating_selection() {
                Ok(true) => {
                    self.status = "Floating selection committed".into();
                    self.changed(cx);
                }
                Ok(false) => {
                    self.status = "There is no floating selection".into();
                    cx.notify();
                }
                Err(e) => {
                    self.status = format!("Commit selection: {e:#}");
                    cx.notify();
                }
            },
            "cancel-selection" => {
                if self.editor.cancel_floating_selection() {
                    self.status = "Floating selection cancelled".into();
                    self.changed(cx);
                } else {
                    self.status = "There is no floating selection".into();
                    cx.notify();
                }
            }
            "add-guide" => {
                self.dialog = Dialog::Guide;
                self.detail_inputs[0].update(cx, |s, cx| s.set_value("vertical", window, cx));
                self.detail_inputs[1].update(cx, |s, cx| s.set_value("0", window, cx));
            }
            "mask-link" => {
                let id = self.editor.active_layer.clone();
                let linked = self
                    .editor
                    .document
                    .find_layer(&id)
                    .and_then(|l| l.metadata.get("maskLinked"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.editor.set_mask_linked(&id, !linked);
                self.changed(cx);
            }
            "mask-enable" => {
                let id = self.editor.active_layer.clone();
                let enabled = self
                    .editor
                    .document
                    .find_layer(&id)
                    .and_then(|l| l.metadata.get("maskEnabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.editor.set_mask_enabled(&id, !enabled);
                self.changed(cx);
            }
            "mask-transform" => {
                let id = self.editor.active_layer.clone();
                if let Some(p) = self.editor.mask_placement(&id) {
                    self.dialog = Dialog::MaskTransform;
                    for (i, v) in [p.x, p.y, p.width, p.height, p.rotation].iter().enumerate() {
                        self.detail_inputs[i]
                            .update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
                    }
                } else {
                    self.status = "Add a mask first".into();
                }
            }
            "effects" => {
                self.dialog = Dialog::Effects;
                self.open_effect(0, window, cx);
            }
            "camera-raw" => self.open_camera_raw(window, cx),
            "select-subject" | "remove-background" => {
                self.start_subject(name == "select-subject", window, cx)
            }
            "luminosity-range" | "color-range" => {
                self.open_range(name == "color-range", window, cx)
            }
            "live-adjustment" => {
                self.editing_object = None;
                self.dialog = Dialog::Adjustment;
                self.open_adjustment(0, None, window, cx);
            }
            "edit-adjustment" => {
                let v = self
                    .editor
                    .document
                    .find_layer(&self.editor.active_layer)
                    .and_then(|l| l.metadata.get("adjustment"))
                    .cloned();
                if let Some(v) = v {
                    let kind = crate::adjustment_controls::KINDS
                        .iter()
                        .position(|k| Some(*k) == v.get("kind").and_then(|v| v.as_str()))
                        .unwrap_or(0);
                    self.editing_object = Some(self.editor.active_layer.clone());
                    self.dialog = Dialog::Adjustment;
                    self.open_adjustment(kind, Some(v), window, cx);
                } else {
                    self.status = "Select an adjustment layer".into();
                }
            }
            "live-mask" => {
                self.dialog = Dialog::LiveMask;
            }
            "distort" => {
                if let Some(p) = self.editor.layer_placement(&self.editor.active_layer) {
                    self.dialog = Dialog::Distort;
                    let corners = [
                        p.point(0., 0.),
                        p.point(1., 0.),
                        p.point(1., 1.),
                        p.point(0., 1.),
                    ];
                    for (i, v) in corners.iter().flat_map(|&(x, y)| [x, y]).enumerate() {
                        self.detail_inputs[i]
                            .update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
                    }
                }
            }
            "transform" => {
                if let Some(l) = self.editor.document.find_layer(&self.editor.active_layer) {
                    self.tool = Tool::Move;
                    let v = [
                        l.offset_x,
                        l.offset_y,
                        l.rotation,
                        l.scale_x * 100.,
                        l.scale_y * 100.,
                    ];
                    for (i, value) in v.iter().enumerate() {
                        self.detail_inputs[i]
                            .update(cx, |s, cx| s.set_value(value.to_string(), window, cx));
                    }
                    self.dialog = Dialog::Transform;
                    self.detail_inputs[0].update(cx, |s, cx| s.focus(window, cx));
                }
            }
            "copy-merged" => {
                let mut merged = self.pixels.clone();
                let mut origin = (0., 0.);
                if let Some(selection) = &self.editor.selection {
                    for (x, y, p) in merged.enumerate_pixels_mut() {
                        let coverage =
                            selection.mask[y as usize * selection.width as usize + x as usize];
                        if coverage == 0 {
                            p.0 = [0; 4];
                        } else if coverage < 255 {
                            p[3] = ((u16::from(p[3]) * u16::from(coverage) + 127) / 255) as u8;
                        }
                    }
                    if let Some((x, y, w, h)) = selection.bounds() {
                        origin = (x as f32, y as f32);
                        merged = image::imageops::crop_imm(&merged, x, y, w, h).to_image();
                    }
                }
                let fingerprint = pixel_fingerprint(&merged);
                let dimensions = merged.dimensions();
                let mut bytes = std::io::Cursor::new(Vec::new());
                match image::DynamicImage::ImageRgba8(merged)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                {
                    Ok(()) => {
                        cx.set_global(clipboard_ui::LayerClipboardStore::default());
                        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                            ImageFormat::Png,
                            bytes.into_inner(),
                        )));
                        self.clipboard_origin = Some((fingerprint, dimensions, origin));
                        self.status = "Merged pixels copied".into();
                    }
                    Err(e) => self.status = format!("Copy failed: {e}"),
                }
            }
            "shortcuts" => {
                self.shortcut_draft = self.shortcuts.clone();
                self.recording = None;
                self.dialog = Dialog::Shortcuts;
                self.path_input.update(cx, |state, cx| {
                    state.set_value("", window, cx);
                    state.set_placeholder("Search commands, categories or shortcuts…", window, cx);
                    state.focus(window, cx);
                });
            }
            "copy" => self.copy(false, cx),
            "cut" => self.copy(true, cx),
            "paste" => self.paste(cx),
            "quit" => self.request(Pending::Quit, window, cx),
            "actual" => {
                self.set_zoom(1., None);
            }
            "grid" => {
                self.show_grid = !self.show_grid;
                self.persist_preferences(cx);
            }
            "grid-spacing" => {
                self.preferences.grid_spacing = match self.preferences.grid_spacing {
                    4 => 8,
                    8 => 16,
                    16 => 32,
                    _ => 4,
                };
                self.persist_preferences(cx);
                cx.notify();
            }
            "grid-subdivisions" => {
                self.preferences.grid_subdivisions = match self.preferences.grid_subdivisions {
                    1 => 2,
                    2 => 4,
                    4 => 8,
                    _ => 1,
                };
                self.persist_preferences(cx);
                cx.notify();
            }
            "guides" => {
                self.show_guides = !self.show_guides;
                self.persist_preferences(cx);
            }
            "rulers" => {
                self.preferences.rulers = !self.preferences.rulers;
                self.persist_preferences(cx);
            }
            "snapping" => {
                self.preferences.snapping = !self.preferences.snapping;
                self.persist_preferences(cx);
            }
            "auto-select" => {
                self.preferences.auto_select = !self.preferences.auto_select;
                self.persist_preferences(cx);
            }
            "transform-box" => {
                self.preferences.transform_box = !self.preferences.transform_box;
                self.persist_preferences(cx);
            }
            "fill-foreground" => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    if let Err(e) = self
                        .editor
                        .fill_mask_selection(&id, mask_value(self.editor.brush.color))
                    {
                        self.status = format!("Mask fill: {e:#}");
                    }
                } else {
                    self.editor.fill_selection(self.editor.brush.color);
                }
                self.changed(cx);
            }
            "fill-background" => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    if let Err(e) = self
                        .editor
                        .fill_mask_selection(&id, mask_value(self.background_color))
                    {
                        self.status = format!("Mask fill: {e:#}");
                    }
                } else {
                    self.editor.fill_selection(self.background_color);
                }
                self.changed(cx);
            }
            "swap-colors" => {
                let foreground = self.editor.brush.color;
                self.editor.brush.color = self.background_color;
                self.background_color = foreground;
                let foreground = self.editor.brush.color;
                let background = self.background_color;
                self.color.update(cx, |state, cx| {
                    state.set_value(rgba(u32::from_be_bytes(foreground)), window, cx)
                });
                self.background.update(cx, |state, cx| {
                    state.set_value(rgba(u32::from_be_bytes(background)), window, cx)
                });
                cx.notify();
            }
            "default-colors" => {
                self.editor.brush.color = [0, 0, 0, 255];
                self.background_color = [255, 255, 255, 255];
                self.color.update(cx, |state, cx| {
                    state.set_value(rgba(0x000000ff), window, cx)
                });
                self.background.update(cx, |state, cx| {
                    state.set_value(rgba(0xffffffff), window, cx)
                });
                cx.notify();
            }
            "delete-content" => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    if self.editor.selection.is_some() {
                        if let Err(e) = self
                            .editor
                            .fill_mask_selection(&id, mask_value(self.background_color))
                        {
                            self.status = format!("Mask delete: {e:#}");
                        }
                    } else if self.editor.remove_mask(&id, false) {
                        self.paint_mask = false;
                        self.status = "Mask removed".into();
                    } else {
                        self.status = "No removable mask on the active layer".into();
                    }
                } else if self.editor.selection.is_some() {
                    self.editor.clear_selected_pixels();
                } else {
                    let ids = self.selected_layer_ids();
                    if !self.editor.delete_layers(&ids) {
                        self.status =
                            "Cannot delete locked layers or layers used by live masks".into();
                    }
                }
                self.changed(cx);
            }
            "nudge-left" | "nudge-right" | "nudge-up" | "nudge-down" => {
                let (dx, dy) = match name {
                    "nudge-left" => (-1., 0.),
                    "nudge-right" => (1., 0.),
                    "nudge-up" => (0., -1.),
                    _ => (0., 1.),
                };
                self.editor.move_layers(&self.selected_layer_ids(), dx, dy);
                self.changed(cx);
            }
            name if name.starts_with("tool-") && name != "tool-settings" => {
                self.tool = match name {
                    "tool-pencil" => Tool::Pencil,
                    "tool-eraser" => Tool::Eraser,
                    "tool-fill" => Tool::Fill,
                    "tool-gradient" => Tool::Gradient,
                    "tool-rectangle" => Tool::Rectangle,
                    "tool-ellipse" => Tool::Ellipse,
                    "tool-move" => Tool::Move,
                    "tool-picker" => Tool::Picker,
                    "tool-clone" => Tool::Clone,
                    "tool-heal" => Tool::Heal,
                    "tool-spot-heal" => Tool::SpotHeal,
                    "tool-wand" => Tool::Wand,
                    "tool-object" => Tool::Object,
                    "tool-lasso" => Tool::Lasso,
                    "tool-blur-brush" => Tool::BlurBrush,
                    "tool-smudge" => Tool::Smudge,
                    "tool-liquify" => Tool::Liquify,
                    "tool-shape-rect" => Tool::ShapeRect,
                    "tool-shape-ellipse" => Tool::ShapeEllipse,
                    "tool-line" => Tool::Line,
                    _ => Tool::Brush,
                };
            }
            "new" => self.request(Pending::New, window, cx),
            "open" => self.request(Pending::Open, window, cx),
            "open-recent" => self.open_recent(window, cx),
            "save" => self.save(window, cx),
            "save-as" => self.save_dialog(false, window, cx),
            "export" => self.save_dialog(true, window, cx),
            "import" => self.open_dialog(true, window, cx),
            "undo" => {
                if let Err(error) = self.undo_or_collection(cx) {
                    self.status = format!("Undo: {error:#}");
                    cx.notify();
                }
            }
            "redo" => {
                if let Err(error) = self.redo_or_collection(cx) {
                    self.status = format!("Redo: {error:#}");
                    cx.notify();
                }
            }
            "add" => {
                self.editor.add_layer("Paint layer");
                self.changed(cx);
            }
            "group" => {
                let ids = self.selected_layer_ids();
                if let Some(id) = self.editor.group_layers(&ids, "Group") {
                    self.layer_selection.click(id, SelectionAction::Replace);
                    self.changed(cx);
                } else {
                    self.status = "Cannot group locked or dependent layers".into();
                }
            }
            "ungroup" => {
                let id = self.editor.active_layer.clone();
                match self.editor.ungroup_layer(&id) {
                    Ok(ids) => {
                        self.layer_selection = LayerSelection {
                            ids: ids.clone(),
                            primary: ids.first().cloned(),
                        };
                        self.changed(cx);
                    }
                    Err(error) => {
                        self.status = format!("Cannot ungroup: {error:#}");
                        cx.notify();
                    }
                }
            }
            "duplicate" => {
                let ids = self.selected_layer_ids();
                let copies = self.editor.duplicate_layers(&ids);
                if !copies.is_empty() {
                    self.select_layer_ids(copies);
                    self.changed(cx);
                }
            }
            "delete" => {
                let ids = self.selected_layer_ids();
                if self.editor.delete_layers(&ids) {
                    self.changed(cx);
                } else {
                    self.status = "Cannot delete locked layers or layers used by live masks".into();
                }
            }
            "text" => {
                self.tool = Tool::Text;
                self.status =
                    "Click text to edit; click or drag a new text box · Alt-click forces new text"
                        .into();
                cx.notify();
            }
            "new-text" | "edit-object" => {
                if name == "edit-object"
                    && self
                        .editor
                        .document
                        .find_layer(&self.editor.active_layer)
                        .is_some_and(|layer| layer.vector_scene.is_some())
                {
                    self.open_vector_scene(window, cx);
                    return;
                }
                if name == "edit-object"
                    && self
                        .editor
                        .document
                        .find_layer(&self.editor.active_layer)
                        .and_then(|layer| layer.advanced.as_ref())
                        .is_some_and(|state| state.recipe.vector.is_some())
                {
                    self.open_vector(false, window, cx);
                    return;
                }
                self.editing_object = None;
                let layer = self.editor.document.find_layer(&self.editor.active_layer);
                let text = if name == "edit-object" {
                    layer.and_then(|l| objects::live_text(l).ok().flatten())
                } else {
                    None
                };
                let shape = if name == "edit-object" {
                    layer.and_then(|l| objects::live_shape(l).ok().flatten())
                } else {
                    None
                };
                if let Some(style) = shape {
                    self.dialog = Dialog::Shape;
                    let (w, h) = layer
                        .and_then(|l| l.image.as_ref())
                        .map(|i| i.dimensions())
                        .unwrap_or((1, 1));
                    for (i, v) in [
                        w as f32,
                        h as f32,
                        style.corner_radius,
                        style.line_width.unwrap_or(1.),
                    ]
                    .iter()
                    .enumerate()
                    {
                        self.detail_inputs[i]
                            .update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
                    }
                    self.set_dialog_rgb(style.red, style.green, style.blue, window, cx);
                } else if name == "new-text" || text.is_some() {
                    if text.is_some() {
                        let id = self.editor.active_layer.clone();
                        if !editable_text(&self.editor.document.layers, &id, true, false) {
                            self.status =
                                "Unlock and show the text layer and its parent groups before editing"
                                    .into();
                            cx.notify();
                            return;
                        }
                        self.editing_object = Some(self.editor.active_layer.clone());
                    }
                    let style = text.unwrap_or_else(|| objects::LiveTextStyle {
                        font_name: "sans-serif".into(),
                        font_size: 48.,
                        red: self.editor.brush.color[0] as f32 / 255.,
                        green: self.editor.brush.color[1] as f32 / 255.,
                        blue: self.editor.brush.color[2] as f32 / 255.,
                        ..Default::default()
                    });
                    self.dialog = Dialog::Text;
                    self.path_input.update(cx, |s, cx| {
                        s.set_value(style.content.clone(), window, cx);
                        s.focus(window, cx);
                    });
                    let values = [
                        style.font_size.to_string(),
                        style.font_name.clone(),
                        style.tracking.to_string(),
                        style.leading.to_string(),
                        format!("{:?}", style.alignment),
                        style.box_size.map(|s| s.width).unwrap_or(0.).to_string(),
                        style.box_size.map(|s| s.height).unwrap_or(0.).to_string(),
                    ];
                    for (i, v) in values.iter().enumerate() {
                        self.detail_inputs[i]
                            .update(cx, |s, cx| s.set_value(v.clone(), window, cx));
                    }
                    self.detail_inputs[9].update(cx, |input, cx| input.set_value("", window, cx));
                    self.set_dialog_rgb(style.red, style.green, style.blue, window, cx);
                } else {
                    self.status = "Select editable text, a shape, or vector artwork".into();
                }
            }
            "rasterize" => {
                let id = self.editor.active_layer.clone();
                if self.editor.rasterize_layer(&id) {
                    self.status =
                        "Converted live object to pixels. Undo restores editability.".into();
                    self.changed(cx);
                }
            }
            "filter" => {
                self.editing_object = None;
                self.open_filter(0, window, cx);
            }
            "dither" => self.open_finishing(0, window, cx),
            "bloom-glow" => self.open_finishing(1, window, cx),
            "vignette-overlay" => self.open_finishing(2, window, cx),
            "local-contrast" => self.open_finishing(3, window, cx),
            "blend" => {
                let modes = [
                    "Normal",
                    "Multiply",
                    "Screen",
                    "Overlay",
                    "Darken",
                    "Lighten",
                    "Color Dodge",
                    "Color Burn",
                    "Soft Light",
                    "Hard Light",
                    "Difference",
                    "Exclusion",
                    "Hue",
                    "Saturation",
                    "Color",
                    "Luminosity",
                ];
                let id = self.editor.active_layer.clone();
                let current = self
                    .editor
                    .document
                    .find_layer(&id)
                    .map(|l| l.blend_mode.as_str())
                    .unwrap_or("Normal");
                let next =
                    (modes.iter().position(|m| *m == current).unwrap_or(0) + 1) % modes.len();
                self.editor.set_blend_mode(&id, modes[next]);
                self.changed(cx);
            }
            "add-mask" => {
                let id = self.editor.active_layer.clone();
                self.editor.add_mask(&id, true);
                self.changed(cx);
            }
            "sampling" => {
                let id = self.editor.active_layer.clone();
                let current = self
                    .editor
                    .document
                    .find_layer(&id)
                    .and_then(|layer| {
                        if self.paint_mask
                            && !layer.is_group()
                            && !layer
                                .metadata
                                .get("adjustment")
                                .is_some_and(|v| !v.is_null())
                        {
                            layer.metadata.pointer("/maskPlacement/sampling")
                        } else {
                            layer.metadata.pointer("/transform/sampling")
                        }
                    })
                    .and_then(|v| v.as_str())
                    .unwrap_or("High quality");
                let next = match current {
                    "Nearest" => "Smooth",
                    "Smooth" => "High quality",
                    _ => "Nearest",
                };
                match self.editor.set_sampling(&id, next, self.paint_mask) {
                    Ok(true) => {
                        self.status = format!("Sampling: {next}");
                        self.changed(cx);
                    }
                    Ok(false) => self.status = "Select an unlocked layer or existing mask".into(),
                    Err(error) => self.status = error.to_string(),
                }
            }
            "mask-paint" => {
                self.paint_mask = !self.paint_mask;
                self.status = if self.paint_mask {
                    "Brush strokes now edit the selected layer mask"
                } else {
                    "Brush strokes now edit layer pixels"
                }
                .into();
                cx.notify();
            }
            "mask-view" => {
                let id = self.editor.active_layer.clone();
                if self
                    .editor
                    .document
                    .find_layer(&id)
                    .is_some_and(|layer| layer.mask.is_some())
                {
                    self.mask_inspection.toggle(id);
                    self.status = if self.mask_inspection.active() {
                        "Mask inspection enabled · press Mask view again to restore artwork"
                    } else {
                        "Mask inspection disabled"
                    }
                    .into();
                    cx.notify();
                } else {
                    self.status = "Add a layer mask first".into();
                    cx.notify();
                }
            }
            "invert-mask" => {
                let id = self.editor.active_layer.clone();
                self.editor.invert_mask(&id);
                self.changed(cx);
            }
            "apply-mask" => {
                let id = self.editor.active_layer.clone();
                if !self.editor.remove_mask(&id, true) {
                    self.status = "Mask was not applied. Unlock the layer; rasterize an editable source before baking its mask.".into();
                }
                self.changed(cx);
            }
            "merge" => {
                if !self.editor.merge_down() {
                    self.status = "Merge needs two adjacent, unlocked Normal paint layers".into();
                }
                self.changed(cx);
            }
            "flatten" => {
                self.editor.flatten();
                self.changed(cx);
            }
            "rename" => {
                self.dialog = Dialog::Rename;
                let name = self
                    .editor
                    .document
                    .find_layer(&self.editor.active_layer)
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                self.path_input.update(cx, |state, cx| {
                    state.set_placeholder("Layer name", window, cx);
                    state.set_value(name, window, cx);
                    state.focus(window, cx);
                });
            }
            "up" | "down" => {
                let ids = self.editor.selected_layer_roots(&self.selected_layer_ids());
                let positions: Vec<_> = ids
                    .iter()
                    .filter_map(|id| layer_position(&self.editor.document.layers, id, None))
                    .collect();
                if let Some((parent, _, _)) = positions.first() {
                    if positions.iter().any(|(p, _, _)| p != parent) {
                        self.status = "Select sibling layers to move them one step".into();
                    } else {
                        let siblings = parent
                            .as_ref()
                            .and_then(|p| self.editor.document.find_layer(p))
                            .map(|l| &l.children)
                            .unwrap_or(&self.editor.document.layers);
                        let index = if name == "up" {
                            positions
                                .iter()
                                .map(|(_, i, _)| *i)
                                .max()
                                .unwrap()
                                .checked_add(1)
                                .filter(|i| *i < siblings.len())
                        } else {
                            positions
                                .iter()
                                .map(|(_, i, _)| *i)
                                .min()
                                .unwrap()
                                .checked_sub(1)
                        };
                        if let Some(index) = index {
                            let target = siblings[index].id.clone();
                            let moved = self.editor.drop_layers(
                                &ids,
                                Some(&target),
                                if name == "up" { 1 } else { -1 },
                                false,
                            );
                            if !moved.is_empty() {
                                self.select_layer_ids(moved);
                                self.changed(cx);
                            }
                        }
                    }
                }
            }
            "nest" => {
                self.dialog = Dialog::Nest;
                self.modal_focus.focus(window, cx);
            }
            "unnest" => {
                let ids = self.selected_layer_ids();
                let moved = self.editor.drop_layers(&ids, None, 0, false);
                if !moved.is_empty() {
                    self.select_layer_ids(moved);
                    self.changed(cx);
                }
            }
            "visibility" => {
                let id = self.editor.active_layer.clone();
                let visible = self
                    .editor
                    .document
                    .find_layer(&id)
                    .is_some_and(|l| l.visible);
                self.editor.set_visibility(&id, !visible);
                self.changed(cx);
            }
            "remove-mask" => {
                self.editor
                    .remove_mask(&self.editor.active_layer.clone(), false);
                self.paint_mask = false;
                self.changed(cx);
            }
            "clear-effects" => {
                if let Err(e) = self
                    .editor
                    .set_layer_effects(&self.editor.active_layer.clone(), serde_json::json!({}))
                {
                    self.status = e.to_string();
                }
                self.changed(cx);
            }
            "clipping" => {
                let id = self.editor.active_layer.clone();
                let existing = self
                    .editor
                    .document
                    .find_layer(&id)
                    .and_then(|l| l.metadata.get("maskSourceID"))
                    .and_then(|v| v.as_str())
                    .is_some();
                let source = layer_position(&self.editor.document.layers, &id, None).and_then(
                    |(parent, index, _)| {
                        let list = parent
                            .as_ref()
                            .and_then(|p| self.editor.document.find_layer(p))
                            .map(|p| &p.children)
                            .unwrap_or(&self.editor.document.layers);
                        index
                            .checked_sub(1)
                            .and_then(|i| list.get(i))
                            .filter(|l| l.image.is_some())
                            .map(|l| {
                                l.metadata
                                    .get("maskSourceID")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&l.id)
                                    .to_string()
                            })
                    },
                );
                if !existing && source.is_none() {
                    self.status = "Clipping needs a pixel layer immediately below".into();
                } else {
                    match self
                        .editor
                        .set_live_mask_source(&id, if existing { None } else { source.as_deref() })
                    {
                        Ok(_) => self.changed(cx),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
            "lock" => {
                let id = self.editor.active_layer.clone();
                let locked = self
                    .editor
                    .document
                    .find_layer(&id)
                    .is_some_and(|l| l.locked);
                self.editor.set_locked(&id, !locked);
                self.changed(cx);
            }
            "feather-selection" | "grow-selection" | "shrink-selection" => {
                self.selection_operation = match name {
                    "grow-selection" => 1,
                    "shrink-selection" => -1,
                    _ => 0,
                };
                self.dialog = Dialog::Selection;
                self.detail_inputs[0].update(cx, |s, cx| s.set_value("2", window, cx));
            }
            "content-fill" if self.paint_mask => {
                self.status = "Content-aware fill edits pixels; switch the mask target off".into();
                cx.notify();
            }
            "content-fill" => match self.editor.content_aware_fill() {
                Ok(true) => {
                    self.status = "Content-aware fill applied".into();
                    self.changed(cx);
                }
                Ok(false) => self.status = "Select pixels on an unlocked raster layer first".into(),
                Err(e) => self.status = e.to_string(),
            },
            "deselect" => {
                let previous = self.editor.selection.clone();
                self.editor.clear_selection();
                self.editor.record_selection_change(previous);
                self.selection_box = None;
                cx.notify();
            }
            "invert-selection" => {
                let previous = self.editor.selection.clone();
                self.editor.invert_selection();
                self.editor.record_selection_change(previous);
                cx.notify();
            }
            "select-all" => {
                let previous = self.editor.selection.clone();
                self.editor.select_rectangle(
                    0.,
                    0.,
                    self.editor.document.width as f32,
                    self.editor.document.height as f32,
                );
                self.selection_box = Some((
                    0.,
                    0.,
                    self.editor.document.width as f32,
                    self.editor.document.height as f32,
                ));
                self.editor.record_selection_change(previous);
                cx.notify();
            }
            "invert" => {
                if self.paint_mask {
                    let id = self.editor.active_layer.clone();
                    match self.editor.adjust_mask_selection(&id, Adjustment::Invert) {
                        Ok(true) => self.status = "Mask inverted".into(),
                        Ok(false) => self.status = "Mask invert made no change".into(),
                        Err(e) => self.status = format!("Mask invert: {e:#}"),
                    }
                } else {
                    if self.editor.adjust(Adjustment::Invert) {
                        self.record_recipe_step(omuse::recipes::Step::Adjustment {
                            adjustment: Adjustment::Invert,
                        });
                    }
                }
                self.changed(cx);
            }
            "gray" => {
                if self.editor.adjust(Adjustment::Grayscale) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Grayscale,
                    });
                }
                self.changed(cx);
            }
            "blur" => {
                if self.editor.adjust(Adjustment::Blur(2.)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Blur(2.),
                    });
                }
                self.changed(cx);
            }
            "sharpen" => {
                if self.editor.adjust(Adjustment::Sharpen(1.)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Sharpen(1.),
                    });
                }
                self.changed(cx);
            }
            "brighter" => {
                if self.editor.adjust(Adjustment::Brightness(0.1)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Brightness(0.1),
                    });
                }
                self.changed(cx);
            }
            "darker" => {
                if self.editor.adjust(Adjustment::Brightness(-0.1)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Brightness(-0.1),
                    });
                }
                self.changed(cx);
            }
            "contrast" => {
                if self.editor.adjust(Adjustment::Contrast(0.15)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Contrast(0.15),
                    });
                }
                self.changed(cx);
            }
            "saturation" => {
                if self.editor.adjust(Adjustment::Saturation(0.15)) {
                    self.record_recipe_step(omuse::recipes::Step::Adjustment {
                        adjustment: Adjustment::Saturation(0.15),
                    });
                }
                self.changed(cx);
            }
            "rotate" | "flip" => {
                let id = self.editor.active_layer.clone();
                if let Some(l) = self.editor.document.find_layer(&id) {
                    let (x, y, r, sx, sy) =
                        (l.offset_x, l.offset_y, l.rotation, l.scale_x, l.scale_y);
                    self.editor.transform_layer(
                        &id,
                        x,
                        y,
                        if name == "rotate" { r + 90. } else { r },
                        if name == "flip" { -sx } else { sx },
                        sy,
                    );
                    self.changed(cx);
                }
            }
            "trim" => {
                self.dialog = Dialog::Trim;
                self.trim_options = Default::default();
            }
            "resize" | "resize-image" => {
                self.canvas_units = 0;
                self.canvas_relative = false;
                self.canvas_lock_aspect = false;
                self.canvas_anchor = 4;
                self.canvas_fill = 0;
                self.dialog = if name == "resize" {
                    Dialog::Resize
                } else {
                    Dialog::ResizeImage
                };
                let (w, h) = (self.editor.document.width, self.editor.document.height);
                self.width_input
                    .update(cx, |s, cx| s.set_value(w.to_string(), window, cx));
                self.height_input
                    .update(cx, |s, cx| s.set_value(h.to_string(), window, cx));
                if name == "resize-image" {
                    self.detail_inputs[0].update(cx, |state, cx| {
                        state.set_value(self.resize_resolution.to_string(), window, cx)
                    });
                }
            }
            "tool-settings" => {
                self.dialog = Dialog::ToolSettings;
                self.wand_draft = self.wand_settings;
                self.selection_mode_draft = self.selection_mode;
                self.clone_aligned_draft = self.clone_aligned;
                self.clone_all_layers_draft = self.clone_all_layers;
                for (index, value) in [
                    self.editor.brush.size,
                    self.editor.brush.hardness * 100.,
                    self.editor.brush.opacity * 100.,
                    self.editor.brush.smoothing,
                    self.fill_tolerance as f32,
                    self.wand_settings.tolerance as f32,
                    self.wand_settings.sample_radius as f32,
                    self.shape_corner_radius,
                    self.shape_line_width,
                ]
                .into_iter()
                .enumerate()
                {
                    self.detail_inputs[index].update(cx, |state, cx| {
                        state.set_value(value.to_string(), window, cx)
                    });
                }
            }
            "crop" => {
                self.begin_crop(window, cx);
            }
            "zoom-in" => self.set_zoom(omuse::canvas_navigation::step_zoom(self.zoom, true), None),
            "zoom-out" => {
                self.set_zoom(omuse::canvas_navigation::step_zoom(self.zoom, false), None)
            }
            "fit" => {
                let b = self.viewport.get();
                self.zoom = ((f32::from(b.size.width) - 48.) / self.editor.document.width as f32)
                    .min((f32::from(b.size.height) - 48.) / self.editor.document.height as f32)
                    .clamp(0.02, 16.);
                self.pan = (0., 0.);
            }
            _ => {}
        }
        cx.notify();
    }
    #[cfg(test)]
    fn set_brush_rgb(
        &mut self,
        r: f32,
        g: f32,
        b: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let p = [
            (r * 255.).round() as u8,
            (g * 255.).round() as u8,
            (b * 255.).round() as u8,
            255,
        ];
        self.editor.brush.color = p;
        self.color.update(cx, |s, cx| {
            s.set_value(rgba(u32::from_be_bytes(p)), window, cx)
        });
    }
    fn set_dialog_rgb(
        &mut self,
        r: f32,
        g: f32,
        b: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let color = [
            (r * 255.).round() as u8,
            (g * 255.).round() as u8,
            (b * 255.).round() as u8,
            255,
        ];
        self.dialog_color = color;
        self.dialog_color_picker.update(cx, |state, cx| {
            state.set_value(rgba(u32::from_be_bytes(color)), window, cx)
        });
    }
    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if self.dialog != Dialog::None || self.crop.is_some() || self.busy {
            return;
        }
        self.finish_interaction(cx);
        if !self.paint_mask && self.editor.selection.is_none() {
            self.copy_layer_forest(cut, cx);
            return;
        }
        if cut
            && !self.paint_mask
            && let Err(error) = self.editor.validate_pixel_cut()
        {
            self.status = format!("Cut: {error:#}");
            cx.notify();
            return;
        }
        let id = self.editor.active_layer.clone();
        let copied = if self.paint_mask {
            self.editor.copy_mask_selection(&id)
        } else {
            Ok(self.editor.copy_selection())
        };
        if let Ok(Some(pixels)) = &copied {
            let pixels = pixels.clone();
            let fingerprint = pixel_fingerprint(&pixels);
            let dimensions = pixels.dimensions();
            let origin = self
                .editor
                .selection
                .as_ref()
                .and_then(|s| s.bounds())
                .map(|(x, y, _, _)| (x as f32, y as f32))
                .unwrap_or((0., 0.));
            let mut bytes = std::io::Cursor::new(Vec::new());
            match image::DynamicImage::ImageRgba8(pixels)
                .write_to(&mut bytes, image::ImageFormat::Png)
            {
                Ok(()) => {
                    // Build and encode the copy before editing. Refused/no-op cuts
                    // must not replace either the public or rich clipboard.
                    if cut {
                        let result = if self.paint_mask {
                            self.editor
                                .fill_mask_selection(&id, mask_value(self.background_color))
                        } else {
                            Ok(self.editor.cut_selection())
                        };
                        match result {
                            Ok(true) => self.changed(cx),
                            Ok(false) => {
                                self.status = "Cut made no change; clipboard preserved".into();
                                cx.notify();
                                return;
                            }
                            Err(e) => {
                                self.status = format!("Mask cut: {e:#}");
                                cx.notify();
                                return;
                            }
                        }
                    }
                    cx.set_global(clipboard_ui::LayerClipboardStore::default());
                    cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                        ImageFormat::Png,
                        bytes.into_inner(),
                    )));
                    self.clipboard_origin = Some((fingerprint, dimensions, origin));
                    self.status = if cut {
                        "Selected pixels cut to clipboard · Ctrl+Z to undo"
                    } else {
                        "Image copied to clipboard"
                    }
                    .into();
                }
                Err(e) => self.status = format!("Copy failed: {e}"),
            }
        } else if let Err(e) = copied {
            self.status = format!("Mask copy: {e:#}");
        } else {
            self.status = if self.paint_mask {
                "The selected mask region is empty"
            } else {
                "Select a paint layer with pixels to copy"
            }
            .into();
        }
        cx.notify();
    }
    fn paste(&mut self, cx: &mut Context<Self>) {
        if self.dialog != Dialog::None || self.crop.is_some() || self.busy {
            return;
        }
        self.finish_interaction(cx);
        let task = cx.read_from_clipboard_async();
        let generation = self.dialog_generation;
        let revision = self.editor.revision();
        let epoch = self.create.epoch;
        let document_id = self.editor.document.metadata.get("documentID").cloned();
        let clipboard_identity = clipboard_ui::clipboard_identity(cx);
        let selection_revision = self.editor.selection_revision();
        let active_layer = self.editor.active_layer.clone();
        let selected_layers = self.selected_layer_ids();
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.dialog != Dialog::None || this.dialog_generation != generation || this.crop.is_some() || this.busy
                    || this.editor.revision() != revision || this.create.epoch != epoch
                    || this.editor.document.metadata.get("documentID") != document_id.as_ref()
                    || this.editor.selection_revision() != selection_revision
                    || this.editor.active_layer != active_layer
                    || this.selected_layer_ids() != selected_layers
                    || this.drag_start.is_some() || this.transform_drag.is_some()
                    || this.guide_drag.is_some() || this.tablet_painting || this.inline_text.is_some()
                    || this.pan_pointer.is_some() || this.middle_pan_pointer.is_some()
                    || clipboard_ui::clipboard_identity(cx) != clipboard_identity {
                    return;
                }
                match result {
                    Ok(Some(item)) => {
                        if this.paste_layer_forest(&item,cx) { return; }
                        let entry = item.entries.into_iter().find_map(|entry| {
                            if let ClipboardEntry::Image(image) = entry {
                                Some(image)
                            } else {
                                None
                            }
                        });
                        if let Some(image) = entry {
                            let result = (|| -> anyhow::Result<Layer> {
                                anyhow::ensure!(
                                    image.bytes.len() <= 128 * 1024 * 1024,
                                    "Clipboard image too large"
                                );
                                let mut reader =
                                    image::ImageReader::new(std::io::Cursor::new(image.bytes))
                                        .with_guessed_format()?;
                                let mut limits = image::Limits::default();
                                limits.max_alloc = Some(256 * 1024 * 1024);
                                limits.max_image_width = Some(8192);
                                limits.max_image_height = Some(8192);
                                reader.limits(limits);
                                let pixels = reader.decode()?.to_rgba8();
                                anyhow::ensure!(
                                    omuse::model::valid_dimensions(pixels.width(), pixels.height()),
                                    "Clipboard dimensions exceed limits"
                                );
                                let mut layer = Layer::paint("Pasted image", 1, 1);
                                if let Some((hash, dimensions, origin)) = this.clipboard_origin {
                                    if dimensions == pixels.dimensions()
                                        && hash == pixel_fingerprint(&pixels)
                                    {
                                        layer.offset_x = origin.0;
                                        layer.offset_y = origin.1;
                                    }
                                }
                                layer.image = Some(pixels.into());
                                Ok(layer)
                            })();
                            match result {
                                Ok(layer) => {
                                    if this.editor.import_layer(layer).is_empty() {
                                        this.status = "The image could not be pasted within the document limits."
                                            .into();
                                    } else {
                                        this.status = "Pasted image as a new layer".into();
                                        this.changed(cx);
                                    }
                                }
                                Err(e) => this.status = format!("Paste failed: {e:#}"),
                            }
                        } else {
                            this.status = "Clipboard does not contain an image".into();
                        }
                    }
                    Ok(None) => this.status = "Clipboard is empty".into(),
                    Err(e) => this.status = format!("Paste failed: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn load_camera_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.configure_camera_canvas(cx);
        let channel = ["rgb", "red", "green", "blue"][self.camera_curve_channel];
        if let Ok(points) = serde_json::from_value(self.camera_draft["curve"][channel].clone()) {
            let _ = self
                .camera_curve
                .update(cx, |graph, cx| graph.set_points(points, cx));
        }
        for (i, f) in crate::camera_controls::fields(&self.camera_draft, self.camera_section)
            .iter()
            .enumerate()
        {
            self.detail_inputs[i].update(cx, |s, cx| {
                s.set_value(crate::camera_controls::display(&f.value), window, cx)
            });
        }
    }
    fn read_camera_form(&mut self, cx: &Context<Self>) -> anyhow::Result<()> {
        let mut next = self.camera_draft.clone();
        let curve_section = crate::camera_controls::SECTIONS
            .get(self.camera_section)
            .is_some_and(|section| section.0 == "curve");
        if curve_section {
            let channel = ["rgb", "red", "green", "blue"][self.camera_curve_channel];
            next["curve"][channel] = serde_json::to_value(self.camera_curve.read(cx).points())?;
        }
        for (i, f) in crate::camera_controls::fields(&self.camera_draft, self.camera_section)
            .iter()
            .enumerate()
        {
            if curve_section && f.value.is_array() {
                continue;
            }
            crate::camera_controls::store(&mut next, f, &self.detail_inputs[i].read(cx).value())?;
        }
        self.camera_draft = next;
        Ok(())
    }
    fn edit_camera_array(&mut self, add: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Err(e) = self.read_camera_form(cx) {
            self.status = e.to_string();
            cx.notify();
            return;
        }
        let key = crate::camera_controls::SECTIONS
            .get(self.camera_section)
            .map(|section| section.0)
            .unwrap_or("");
        let (pointer, value, limit, label) = match key {
            "mixer" => (
                "/mixer/points",
                serde_json::to_value(omuse::camera_raw::PointColor::default()).unwrap(),
                8,
                "point color",
            ),
            "geometry" => (
                "/geometry/guides",
                serde_json::to_value(omuse::camera_raw::GeometryGuide::default()).unwrap(),
                16,
                "geometry guide",
            ),
            _ => return,
        };
        let Some(values) = self
            .camera_draft
            .pointer_mut(pointer)
            .and_then(|v| v.as_array_mut())
        else {
            self.status = format!("Camera Raw is missing {pointer}");
            cx.notify();
            return;
        };
        if add {
            if values.len() >= limit {
                self.status = format!("Camera Raw supports at most {limit} {label}s");
                cx.notify();
                return;
            }
            values.push(value);
        } else {
            values.pop();
        }
        self.load_camera_form(window, cx);
        self.status = format!("{} {label}", if add { "Added" } else { "Removed last" });
        cx.notify();
    }
    fn start_raw_import(
        &mut self,
        path: PathBuf,
        settings: omuse::raw_import::DevelopSettings,
        cx: &mut Context<Self>,
    ) {
        let generation = self.dialog_generation;
        let open = self.raw_open;
        self.busy = true;
        self.status = "Developing RAW image…".into();
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let image = omuse::raw_import::develop(&path, &settings)?;
            let mut layer = Layer::paint(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("RAW image"),
                1,
                1,
            );
            layer.image = Some(image.into());
            Ok::<_, anyhow::Error>(layer)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if !this.finish_background_job(generation, Dialog::RawImport) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(layer) => {
                        if open {
                            let image = layer.image.as_ref().unwrap();
                            let mut doc = Document::new(image.width(), image.height());
                            doc.name = layer.name.clone();
                            doc.layers = vec![layer];
                            this.install_opened_content(doc, None);
                            this.editor.mark_unsaved();
                            this.path = None;
                            this.live_stamp = None;
                            this.selection_box = None;
                            this.pan = (0., 0.);
                            this.recovery.clear();
                            this.dialog_generation += 1;
                        } else {
                            if this.editor.import_layer(layer).is_empty() {
                                this.status = "The developed image could not be added within the document limits."
                                    .into();
                                cx.notify();
                                return;
                            }
                        }
                        this.dialog = Dialog::None;
                        this.status = "RAW image developed into sRGB".into();
                        this.changed(cx);
                    }
                    Err(e) => {
                        this.status = format!("RAW development failed: {e:#}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn start_image_selection(&mut self, x: f32, y: f32, object: bool, cx: &mut Context<Self>) {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.
            || y < 0.
            || x >= self.editor.document.width as f32
            || y >= self.editor.document.height as f32
        {
            return;
        }
        if object
            && u64::from(self.editor.document.width) * u64::from(self.editor.document.height)
                > 16_777_216
        {
            self.status =
                "Connected subject selection supports canvases up to 16 million pixels".into();
            cx.notify();
            return;
        }
        let source = match self.editor.selection_sample(self.wand_settings.all_layers) {
            Ok(source) => source,
            Err(error) => {
                self.status = format!("Selection source: {error:#}");
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.status = if object {
            "Finding the connected subject under the pointer… Escape cancels"
        } else {
            "Matching wand pixels… Escape cancels"
        }
        .into();
        let generation = self.dialog_generation;
        let mode = self.gesture_selection_mode;
        let previous = self.editor.selection.clone();
        let settings = self.wand_settings;
        let source_fingerprint = pixel_fingerprint(&source);
        let source_size = source.dimensions();
        let layer_id = self.editor.active_layer.clone();
        let task = cx.background_executor().spawn(async move {
            if object {
                let mask = omuse::segmentation::segment(&source)?;
                omuse::selection_tools::object_from_subject_mask(&mask, x as u32, y as u32, 0)
            } else {
                omuse::selection_tools::wand(&source, x as u32, y as u32, &settings)
            }
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ =
                view.update(cx, |this, cx| {
                    if this.dialog_generation != generation {
                        return;
                    }
                    this.busy = false;
                    if this.editor.active_layer != layer_id
                        || this.editor.selection_sample(settings.all_layers).map_or(
                            true,
                            |sample| {
                                sample.dimensions() != source_size
                                    || pixel_fingerprint(&sample) != source_fingerprint
                            },
                        )
                    {
                        this.status = "Selection source changed; result discarded".into();
                        cx.notify();
                        return;
                    }
                    match result {
                        Ok(incoming) => {
                            let found = incoming.mask.iter().any(|&v| v != 0);
                            if found || mode == SelectionMode::Replace {
                                this.editor.selection = Some(omuse::selection_tools::combine(
                                    previous.as_ref(),
                                    &incoming,
                                    mode,
                                ));
                                this.selection_box =
                                    this.editor.selection.as_ref().and_then(|s| s.bounds()).map(
                                        |(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32),
                                    );
                            }
                            this.editor.record_selection_change(previous);
                            this.status = if object && found {
                                "Connected subject selected; touching subjects may be joined"
                            } else if object {
                                "No foreground subject detected at that point"
                            } else if found {
                                "Wand selection updated"
                            } else {
                                "No matching pixels"
                            }
                            .into();
                        }
                        Err(e) => this.status = format!("Selection: {e:#}"),
                    }
                    cx.notify();
                });
        })
        .detach();
        cx.notify();
    }
    fn start_subject(&mut self, as_selection: bool, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.editor.active_layer.clone();
        let Some(layer_source) = self
            .editor
            .document
            .find_layer(&id)
            .and_then(|layer| layer.image.clone())
        else {
            self.status = "Select an image layer first".into();
            return;
        };
        let source = if as_selection {
            self.pixels.clone()
        } else {
            layer_source.into_image()
        };
        for (i, value) in ["0", "0", "0"].iter().enumerate() {
            self.detail_inputs[i].update(cx, |input, cx| input.set_value(*value, window, cx));
        }
        self.busy = true;
        self.status = "Finding the subject locally…".into();
        cx.notify();
        let guide = source.clone();
        let generation = self.dialog_generation;
        let task = cx
            .background_executor()
            .spawn(async move { omuse::segmentation::segment(&source) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if !this.finish_background_job(generation, Dialog::None) {
                    return;
                }
                let source_changed = if as_selection {
                    raster::composite(&this.editor.document) != guide
                } else {
                    this.editor
                        .document
                        .find_layer(&id)
                        .and_then(|layer| layer.image.as_deref())
                        != Some(&guide)
                };
                if source_changed {
                    this.status =
                        "Image changed while segmentation ran; result was not applied".into();
                    cx.notify();
                    return;
                }
                match result {
                    Ok(mask) => {
                        this.subject_mask = Some(mask);
                        this.subject_guide = Some(guide);
                        this.subject_layer = Some(id);
                        this.subject_as_selection = as_selection;
                        this.dialog = Dialog::SubjectRefine;
                        this.status = "Adjust the matte, preview it, then apply".into();
                        cx.notify();
                    }
                    Err(e) => {
                        this.status = format!("Subject selection failed: {e:#}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn subject_settings(&self, cx: &Context<Self>) -> anyhow::Result<omuse::matte::Settings> {
        let values: Vec<f32> = self.detail_inputs[..3]
            .iter()
            .map(|input| input.read(cx).value().parse::<f32>())
            .collect::<Result<_, _>>()?;
        Ok(omuse::matte::Settings {
            refine: values[0],
            contrast: values[1],
            shift: values[2],
        })
    }
    fn run_subject_refine(&mut self, apply: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.dialog != Dialog::SubjectRefine {
            return;
        }
        let (Some(mask), Some(guide), Some(id)) = (
            self.subject_mask.clone(),
            self.subject_guide.clone(),
            self.subject_layer.clone(),
        ) else {
            self.status = "The subject result is no longer available".into();
            return;
        };
        let settings = match self.subject_settings(cx) {
            Ok(settings) => settings,
            Err(e) => {
                self.status = format!("Matte settings: {e}");
                cx.notify();
                return;
            }
        };
        let generation = self.dialog_generation;
        let as_selection = self.subject_as_selection;
        self.busy = true;
        self.status = if apply {
            "Applying refined matte…"
        } else {
            "Refining matte preview…"
        }
        .into();
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { omuse::matte::refine(&mask, &guide, settings) });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_background_job(generation, Dialog::SubjectRefine) {
                    cx.notify();
                    return;
                }
                let refined = match result {
                    Ok(mask) => mask,
                    Err(e) => {
                        this.status = format!("Matte refinement failed: {e:#}");
                        cx.notify();
                        return;
                    }
                };
                if apply {
                    if as_selection {
                        let previous = this.editor.selection.clone();
                        this.editor.selection = Some(Selection {
                            width: refined.width(),
                            height: refined.height(),
                            mask: refined.into_raw(),
                        });
                        this.selection_box = this
                            .editor
                            .selection
                            .as_ref()
                            .and_then(|selection| selection.bounds())
                            .map(|(x, y, w, h)| (x as f32, y as f32, w as f32, h as f32));
                        this.editor.record_selection_change(previous);
                        this.dialog = Dialog::None;
                        this.status = "Subject selected".into();
                        this.subject_mask = None;
                        this.subject_guide = None;
                        this.subject_layer = None;
                        this.refresh(cx);
                    } else {
                        match this.editor.apply_subject_mask(&id, &refined, false) {
                            Ok(true) => {
                                this.dialog = Dialog::None;
                                this.status = "Background hidden with an editable mask".into();
                                this.subject_mask = None;
                                this.subject_guide = None;
                                this.subject_layer = None;
                                this.changed(cx);
                            }
                            Ok(false) => {
                                this.status = "Unlock the layer before changing its mask".into();
                                cx.notify();
                            }
                            Err(e) => {
                                this.status = format!("Subject mask: {e:#}");
                                cx.notify();
                            }
                        }
                    }
                    if this.dialog == Dialog::None {
                        this.dialog_generation = this.dialog_generation.wrapping_add(1);
                        this.focus.focus(window, cx);
                    }
                } else {
                    this.preview_subject_mask(&id, &refined, as_selection, cx);
                }
            });
        })
        .detach();
    }
    fn preview_subject_mask(
        &mut self,
        id: &str,
        mask: &image::GrayImage,
        as_selection: bool,
        cx: &mut Context<Self>,
    ) {
        let preview: anyhow::Result<image::RgbaImage> = (|| {
            if as_selection {
                let mut pixels = self.pixels.clone();
                for (pixel, matte) in pixels.pixels_mut().zip(mask.pixels()) {
                    let outside = 1. - f32::from(matte[0]) / 255.;
                    pixel[0] = (f32::from(pixel[0]) * (1. - outside * 0.55)).round() as u8;
                    pixel[1] = (f32::from(pixel[1]) * (1. - outside * 0.55)).round() as u8;
                    pixel[2] = (f32::from(pixel[2]) * (1. - outside * 0.2)).round() as u8;
                }
                Ok(pixels)
            } else {
                let mut editor = Editor::new(self.editor.document.clone());
                editor.active_layer = id.to_string();
                editor.apply_subject_mask(id, mask, false)?;
                Ok(raster::composite(&editor.document))
            }
        })();
        match preview {
            Ok(pixels) => {
                self.display.replace(&pixels);
                self.status = "Matte preview — the document is unchanged".into();
            }
            Err(e) => self.status = format!("Matte preview: {e:#}"),
        }
        cx.notify();
    }
    fn open_adjustment(
        &mut self,
        kind: usize,
        existing: Option<serde_json::Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjustment_kind = kind;
        self.adjustment_draft = existing.unwrap_or_else(|| crate::adjustment_controls::fresh(kind));
        for (i, f) in crate::adjustment_controls::fields(kind).iter().enumerate() {
            let v = self
                .adjustment_draft
                .pointer(&f.path)
                .and_then(|v| v.as_f64())
                .unwrap_or(f.default);
            self.detail_inputs[i].update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }
        if kind == 3 {
            for i in 0..4 {
                let value = self.adjustment_draft["curves"]["channels"][i]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|v| format!("{},{}", v["x"], v["y"]))
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_else(|| "0,0; 255,255".into());
                self.detail_inputs[i].update(cx, |s, cx| s.set_value(value, window, cx));
            }
        }
        cx.notify();
    }
    fn open_effect(&mut self, kind: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.effect_kind = kind;
        let effect = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|l| l.metadata.get("effects"))
            .and_then(|e| e.get(effect_key(kind)))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let n = |key: &str, default: f64| {
            effect.get(key).and_then(|v| v.as_f64()).unwrap_or(default) as f32
        };
        let values = [
            n("opacity", 0.7) * 100.,
            n(
                if kind == 1 || kind == 3 {
                    "blur"
                } else {
                    "size"
                },
                8.,
            ),
            n("angle", 90.),
            n("distance", 12.),
            if effect
                .get("inside")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                1.
            } else {
                0.
            },
        ];
        for (i, v) in values.iter().enumerate() {
            self.detail_inputs[i].update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }
        self.set_dialog_rgb(n("red", 0.), n("green", 0.), n("blue", 0.), window, cx);
        cx.notify();
    }
    fn open_filter(&mut self, kind: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_interaction(cx);
        self.dialog_generation += 1;
        self.filter_kind = kind;
        self.dialog = Dialog::Filter;
        self.modal_focus.focus(window, cx);
        let (_, _, defaults) = filter_spec(kind);
        for (i, value) in defaults.iter().enumerate() {
            self.filter_inputs[i].update(cx, |s, cx| s.set_value(value.to_string(), window, cx));
        }
        cx.notify();
    }
    fn control(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> gpui_omarchy::Button {
        inspector_ui::panel_button(id, label, ButtonVariant::Secondary, cx)
            .debug_selector(move || id.into())
            .on_click(cx.listener(move |this, _, window, cx| this.command(id, window, cx)))
    }
    fn layer_row(
        &self,
        layer: &Layer,
        depth: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let thumbnail = layer.image.as_ref().and_then(|image| {
            self.layer_thumbnails
                .borrow_mut()
                .image(&layer.id, image, window, cx)
        });
        let id = layer.id.clone();
        let vis_id = id.clone();
        let dragged = LayerDrag {
            ids: if self.layer_selection.ids.contains(&id) {
                self.selected_layer_ids()
            } else {
                vec![id.clone()]
            },
            name: layer.name.clone(),
        };
        let above_target = id.clone();
        let below_target = id.clone();
        let collapse_id = id.clone();
        let mask_target = id.clone();
        let effect_target = id.clone();
        let target = id.clone();
        let context_id = id.clone();
        let selected = self.layer_selection.ids.contains(&id)
            || (self.layer_selection.ids.is_empty() && self.editor.active_layer == id);
        let visible = layer.visible;
        let mut row = div()
            .id(SharedString::from(format!("layer-{id}")))
            .debug_selector({
                let name = format!("layer-{id}");
                move || name.clone()
            })
            .flex()
            .items_center()
            .gap_1()
            .pl(px((depth * 14 + 4) as f32))
            .pr_2()
            .py_1()
            .min_h(px(46.))
            .border_l_2()
            .border_color(if selected { t.accent } else { t.surface })
            .rounded(control_radius())
            .bg(if selected { t.selection } else { t.surface })
            .text_color(t.foreground)
            .hover(|s| s.bg(t.hover_fill()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.busy || this.dialog != Dialog::None {
                        return;
                    }
                    let action = if event.modifiers.control {
                        SelectionAction::Toggle
                    } else if event.modifiers.shift {
                        SelectionAction::Add
                    } else {
                        SelectionAction::Replace
                    };
                    if this.guard_image_trace(cx) {
                        return;
                    }
                    if this.vector_before_layer(
                        id.clone(),
                        action,
                        event.click_count == 2,
                        false,
                        window,
                        cx,
                    ) {
                        return;
                    }
                    this.finish_interaction(cx);
                    this.paint_mask = false;
                    if action != SelectionAction::Replace || !this.layer_selection.ids.contains(&id)
                    {
                        this.layer_selection.click(id.clone(), action);
                    } else {
                        this.layer_selection.primary = Some(id.clone());
                    }
                    if let Some(primary) = this.layer_selection.primary.clone() {
                        this.editor.active_layer = primary;
                    }
                    this.focus.focus(window, cx);
                    if event.click_count == 2 {
                        let adjustment = this.editor.document.find_layer(&id).is_some_and(|l| {
                            l.metadata.get("adjustment").is_some_and(|v| !v.is_null())
                        });
                        this.command(
                            if adjustment {
                                "edit-adjustment"
                            } else {
                                "edit-object"
                            },
                            window,
                            cx,
                        );
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, window, cx| {
                    if this.busy || this.dialog != Dialog::None {
                        return;
                    }
                    if this.guard_image_trace(cx) {
                        return;
                    }
                    if this.vector_before_layer(
                        context_id.clone(),
                        SelectionAction::Replace,
                        false,
                        true,
                        window,
                        cx,
                    ) {
                        return;
                    }
                    this.finish_interaction(cx);
                    this.editor.active_layer = context_id.clone();
                    this.paint_mask = false;
                    if !this.layer_selection.ids.contains(&context_id) {
                        this.layer_selection
                            .click(context_id.clone(), SelectionAction::Replace);
                    }
                    this.layer_selection.primary = Some(context_id.clone());
                    this.dialog = Dialog::LayerMenu;
                    this.modal_focus.focus(window, cx);
                    cx.notify();
                }),
            )
            .on_drag(dragged, |payload, _, _, cx| cx.new(|_| payload.clone()))
            .on_drop(cx.listener(move |this, drag: &MaskDrag, _, cx| {
                if this.dialog != Dialog::None
                    || this.busy
                    || this.vector_scene_active()
                    || this.image_trace_active()
                {
                    return;
                }
                match this.editor.copy_layer_mask(&drag.id, &mask_target, true) {
                    Ok(true) => this.changed(cx),
                    Ok(false) => {}
                    Err(e) => {
                        this.status = e.to_string();
                        cx.notify();
                    }
                }
            }))
            .on_drop(cx.listener(move |this, drag: &EffectDrag, _, cx| {
                if this.dialog != Dialog::None
                    || this.busy
                    || this.vector_scene_active()
                    || this.image_trace_active()
                {
                    return;
                }
                match this
                    .editor
                    .copy_layer_effect(&drag.id, &effect_target, &drag.kind, true)
                {
                    Ok(true) => this.changed(cx),
                    Ok(false) => {}
                    Err(e) => {
                        this.status = e.to_string();
                        cx.notify();
                    }
                }
            }))
            .on_drop(cx.listener(move |this, drag: &LayerDrag, window, cx| {
                let placement = if this
                    .editor
                    .document
                    .find_layer(&target)
                    .is_some_and(|l| l.is_group())
                {
                    0
                } else {
                    1
                };
                this.drop_layer_drag(drag, Some(&target), placement, window.modifiers().alt, cx);
            }))
            .child(
                button(
                    SharedString::from(format!("visible-{vis_id}")),
                    "",
                    ButtonVariant::Secondary,
                    cx,
                )
                .disabled(self.busy || self.vector_scene_active() || self.image_trace_active())
                .accessibility_label(if visible { "Hide layer" } else { "Show layer" })
                .size(px(24.))
                .p_0()
                .child(
                    crate::studio_icons::glyph(if visible { "eye" } else { "eye-off" })
                        .size(px(14.)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.editor.set_visibility(&vis_id, !visible);
                    this.changed(cx);
                })),
            )
            .child(studio_ui::layer_identity(layer, thumbnail, cx));
        if layer.mask.is_some() {
            let payload = MaskDrag {
                id: layer.id.clone(),
            };
            let mask_id = layer.id.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("mask-badge-{mask_id}")))
                    .debug_selector({
                        let name = format!("mask-badge-{mask_id}");
                        move || name.clone()
                    })
                    .px_1()
                    .border_1()
                    .rounded(control_radius())
                    .border_color(if self.mask_inspection.target().as_deref() == Some(layer.id.as_str()) {
                        t.warning
                    } else if self.paint_mask && self.editor.active_layer == layer.id {
                        t.accent
                    } else {
                        t.divider()
                    })
                    .text_size(px(9.))
                    .child("MASK")
                    .on_drag(payload, |payload, _, _, cx| cx.new(|_| payload.clone()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if this.busy || this.vector_scene_active() || this.image_trace_active()
                            {
                                return;
                            }
                            if event.modifiers.alt {
                                this.mask_inspection.toggle(mask_id.clone());
                                this.status = if this.mask_inspection.active() {
                                    "Mask inspection enabled · Alt-click the badge again to restore artwork"
                                } else {
                                    "Mask inspection disabled"
                                }
                                .into();
                                cx.notify();
                                return;
                            }
                            this.finish_interaction(cx);
                            this.editor.active_layer = mask_id.clone();
                            this.layer_selection
                                .click(mask_id.clone(), SelectionAction::Replace);
                            this.paint_mask = true;
                            this.status = "Mask selected; drag this badge to copy the mask".into();
                            cx.notify();
                        }),
                    ),
            );
        }
        if layer.is_group() {
            row = row.child(
                button(
                    SharedString::from(format!("collapse-{collapse_id}")),
                    "",
                    ButtonVariant::Secondary,
                    cx,
                )
                .accessibility_label(if self.collapsed_groups.contains(&collapse_id) {
                    "Expand group"
                } else {
                    "Collapse group"
                })
                .size(px(22.))
                .p_0()
                .child(
                    crate::studio_icons::glyph(if self.collapsed_groups.contains(&collapse_id) {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    })
                    .size(px(14.)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.collapsed_groups.remove(&collapse_id) {
                        this.collapsed_groups.insert(collapse_id.clone());
                    }
                    cx.notify();
                })),
            );
        }
        if layer.opacity < 1. {
            row = row.child(format!("{}%", (layer.opacity * 100.).round()));
        }
        let mut effect_rows = div().flex().flex_col().pl(px((depth * 14 + 20) as f32));
        if let Some(effects) = layer.metadata.get("effects").and_then(|v| v.as_object()) {
            for (kind, value) in effects {
                if !value.is_object() {
                    continue;
                }
                let payload = EffectDrag {
                    id: layer.id.clone(),
                    kind: kind.clone(),
                };
                effect_rows = effect_rows.child(
                    div()
                        .id(SharedString::from(format!(
                            "effect-row-{}-{kind}",
                            layer.id
                        )))
                        .debug_selector({
                            let name = format!("effect-row-{}-{kind}", layer.id);
                            move || name.clone()
                        })
                        .text_sm()
                        .text_color(t.secondary)
                        .child(format!("fx  {kind}"))
                        .on_drag(payload, |payload, _, _, cx| cx.new(|_| payload.clone())),
                );
            }
        }
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(SharedString::from(format!("above-{above_target}")))
                    .h(px(5.))
                    .w_full()
                    .on_drop(cx.listener(move |this, drag: &LayerDrag, window, cx| {
                        this.drop_layer_drag(
                            drag,
                            Some(&above_target),
                            1,
                            window.modifiers().alt,
                            cx,
                        );
                    })),
            )
            .child(row)
            .child(effect_rows)
            .child(
                div()
                    .id(SharedString::from(format!("below-{below_target}")))
                    .h(px(5.))
                    .w_full()
                    .on_drop(cx.listener(move |this, drag: &LayerDrag, window, cx| {
                        this.drop_layer_drag(
                            drag,
                            Some(&below_target),
                            -1,
                            window.modifiers().alt,
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }
    fn canvas_dimensions(&self, cx: &App) -> Option<(f64, f64)> {
        let dpi = self
            .editor
            .document
            .metadata
            .get("resolution")
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite() && *v > 0.)
            .unwrap_or(72.);
        let axis = |value: &str, original: u32| -> Option<f64> {
            let n = value.parse::<f64>().ok()?;
            let pixels = match self.canvas_units {
                1 => n * f64::from(original) / 100.,
                2 => n * dpi,
                3 => n * dpi / 2.54,
                _ => n,
            };
            Some(
                pixels
                    + if self.canvas_relative {
                        f64::from(original)
                    } else {
                        0.
                    },
            )
        };
        Some((
            axis(
                &self.width_input.read(cx).value(),
                self.editor.document.width,
            )?,
            axis(
                &self.height_input.read(cx).value(),
                self.editor.document.height,
            )?,
        ))
    }
    fn canvas_axis_from_pixels(&self, pixels: f64, original: u32) -> f64 {
        let dpi = self
            .editor
            .document
            .metadata
            .get("resolution")
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite() && *value > 0.)
            .unwrap_or(72.);
        let delta = pixels
            - if self.canvas_relative {
                f64::from(original)
            } else {
                0.
            };
        match self.canvas_units {
            1 => delta / f64::from(original) * 100.,
            2 => delta / dpi,
            3 => delta / dpi * 2.54,
            _ => delta,
        }
    }
    fn sync_canvas_aspect(
        &mut self,
        width_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog != Dialog::Resize || !self.canvas_lock_aspect {
            return;
        }
        let original_width = self.editor.document.width;
        let original_height = self.editor.document.height;
        let dpi = self
            .editor
            .document
            .metadata
            .get("resolution")
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite() && *value > 0.)
            .unwrap_or(72.);
        let parse_pixels = |value: &str, original: u32| -> Option<f64> {
            let value = value.parse::<f64>().ok()?;
            let delta = match self.canvas_units {
                1 => value * f64::from(original) / 100.,
                2 => value * dpi,
                3 => value * dpi / 2.54,
                _ => value,
            };
            let pixels = delta
                + if self.canvas_relative {
                    f64::from(original)
                } else {
                    0.
                };
            (pixels.is_finite() && pixels > 0.).then_some(pixels)
        };
        if width_changed {
            let Some(width) = parse_pixels(&self.width_input.read(cx).value(), original_width)
            else {
                return;
            };
            let height = width * f64::from(original_height) / f64::from(original_width);
            let displayed = self.canvas_axis_from_pixels(height, original_height);
            let displayed = displayed.to_string();
            if self.height_input.read(cx).value() != displayed {
                self.height_input
                    .update(cx, |input, cx| input.set_value(displayed, window, cx));
            }
        } else {
            let Some(height) = parse_pixels(&self.height_input.read(cx).value(), original_height)
            else {
                return;
            };
            let width = height * f64::from(original_width) / f64::from(original_height);
            let displayed = self.canvas_axis_from_pixels(width, original_width);
            let displayed = displayed.to_string();
            if self.width_input.read(cx).value() != displayed {
                self.width_input
                    .update(cx, |input, cx| input.set_value(displayed, window, cx));
            }
        }
        cx.notify();
    }
    fn invalidate_jpeg_preview(&mut self) {
        if self.jpeg_preview.is_some() || self.jpeg_preview_task.is_some() {
            self.jpeg_preview = None;
            self.jpeg_preview_generation = self.jpeg_preview_generation.wrapping_add(1);
        }
    }
    fn start_jpeg_preview(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.photo_io.is_some() {
            self.status =
                "An image operation is already running; JPEG preview is unavailable.".into();
            cx.notify();
            return;
        }
        if u64::from(self.editor.document.width) * u64::from(self.editor.document.height)
            > 16_000_000
        {
            self.status = "JPEG preview is limited to 16 megapixels".into();
            cx.notify();
            return;
        }
        if self.jpeg_preview_task.is_some() {
            return;
        }
        let values: Option<Vec<u8>> = self.detail_inputs[..4]
            .iter()
            .map(|input| input.read(cx).value().parse().ok())
            .collect();
        let Some(values) = values.filter(|values| (1..=100).contains(&values[0])) else {
            self.status = "JPEG quality must be 1–100 and matte RGB must be 0–255".into();
            cx.notify();
            return;
        };
        let Ok(dpi) = self.detail_inputs[4].read(cx).value().parse::<f64>() else {
            self.status = "Resolution must be 1–9600 DPI".into();
            cx.notify();
            return;
        };
        if !dpi.is_finite() || !(1. ..=9600.).contains(&dpi) {
            self.status = "Resolution must be 1–9600 DPI".into();
            cx.notify();
            return;
        }
        self.jpeg_preview_generation = self.jpeg_preview_generation.wrapping_add(1);
        let generation = self.jpeg_preview_generation;
        let dialog_generation = self.dialog_generation;
        self.jpeg_preview_task = Some(generation);
        self.jpeg_preview = None;
        let mut document = self.editor.document.clone();
        document.metadata["resolution"] = serde_json::json!(dpi);
        let options = raster::ExportOptions {
            jpeg_quality: values[0],
            matte: [values[1], values[2], values[3]],
        };
        let task = cx.background_executor().spawn(async move {
            let encoded = raster::encode_jpeg(&document, options)?;
            let decoded =
                image::load_from_memory_with_format(&encoded, image::ImageFormat::Jpeg)?.to_rgba8();
            anyhow::Ok((decoded, encoded.len()))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                if this.jpeg_preview_task == Some(generation) {
                    this.jpeg_preview_task = None;
                }
                if this.dialog != Dialog::Export
                    || this.dialog_generation != dialog_generation
                    || this.jpeg_preview_generation != generation
                    || this.photo_io.is_some()
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok((decoded, bytes)) => {
                        let dimensions = decoded.dimensions();
                        this.jpeg_preview =
                            Some((render_image(&decoded), bytes, dimensions.0, dimensions.1));
                        this.status = format!("JPEG preview encoded: {bytes} bytes");
                    }
                    Err(error) => this.status = format!("JPEG preview failed: {error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn change_canvas_units(
        &mut self,
        units: usize,
        relative: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((w, h)) = self.canvas_dimensions(cx) else {
            self.status = "Enter valid dimensions before changing units".into();
            cx.notify();
            return;
        };
        self.canvas_units = units;
        self.canvas_relative = relative;
        let dpi = self
            .editor
            .document
            .metadata
            .get("resolution")
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite() && *v > 0.)
            .unwrap_or(72.);
        let axis = |n: f64, original: u32| {
            let p = n - if relative { f64::from(original) } else { 0. };
            match units {
                1 => p / f64::from(original) * 100.,
                2 => p / dpi,
                3 => p / dpi * 2.54,
                _ => p,
            }
        };
        let width = axis(w, self.editor.document.width);
        let height = axis(h, self.editor.document.height);
        self.width_input.update(cx, |state, cx| {
            state.set_value(width.to_string(), window, cx)
        });
        self.height_input.update(cx, |state, cx| {
            state.set_value(height.to_string(), window, cx)
        });
        cx.notify();
    }
    fn selected_layer_ids(&self) -> Vec<String> {
        let ids: Vec<_> = self
            .layer_selection
            .ids
            .iter()
            .filter(|id| self.editor.document.find_layer(id).is_some())
            .cloned()
            .collect();
        if ids.is_empty() {
            vec![self.editor.active_layer.clone()]
        } else {
            ids
        }
    }
    fn select_layer_ids(&mut self, ids: Vec<String>) {
        if let Some(id) = ids.last() {
            self.editor.active_layer = id.clone();
        }
        self.layer_selection = LayerSelection {
            primary: ids.last().cloned(),
            ids,
        };
    }
    fn drop_layer_drag(
        &mut self,
        drag: &LayerDrag,
        target: Option<&str>,
        placement: i8,
        copy: bool,
        cx: &mut Context<Self>,
    ) {
        if self.dialog != Dialog::None
            || self.busy
            || self.vector_scene_active()
            || self.image_trace_active()
        {
            return;
        }
        let ids = self.editor.drop_layers(&drag.ids, target, placement, copy);
        if !ids.is_empty() {
            self.select_layer_ids(ids);
            self.changed(cx);
        } else {
            self.status =
                "Drop rejected: locked layer, dependency, cycle or unchanged position".into();
            cx.notify();
        }
    }
    fn capture_pointer_point(&mut self, point: (f32, f32)) {
        if self.pointer_path_error.is_none() {
            if let Err(error) = pointer_path::append(&mut self.lasso, point) {
                self.pointer_path_error = Some(error);
                self.status = error.into();
            }
        }
    }

    fn begin_inline_text(
        &mut self,
        layer: Option<String>,
        origin: (f32, f32),
        box_size: Option<objects::ObjectSize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inline_text.is_some() || self.dialog != Dialog::None || self.busy {
            return;
        }
        let (style, origin) = if let Some(id) = layer.as_ref() {
            if !editable_text(&self.editor.document.layers, id, true, false) {
                self.status =
                    "Unlock and show the text layer and its parent groups before editing".into();
                cx.notify();
                return;
            }
            let Some(style) = self
                .editor
                .document
                .find_layer(id)
                .and_then(|l| objects::live_text(l).ok().flatten())
            else {
                return;
            };
            let Some(placement) = self.editor.layer_placement(id) else {
                return;
            };
            self.select_layer_ids(vec![id.clone()]);
            (style, placement.point(0., 0.))
        } else {
            (
                objects::LiveTextStyle {
                    content: String::new(),
                    font_name: "sans-serif".into(),
                    font_size: 48.,
                    red: self.editor.brush.color[0] as f32 / 255.,
                    green: self.editor.brush.color[1] as f32 / 255.,
                    blue: self.editor.brush.color[2] as f32 / 255.,
                    box_size,
                    ..Default::default()
                },
                origin,
            )
        };
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(4));
        let color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(Rgba {
                r: style.red,
                g: style.green,
                b: style.blue,
                a: 1.,
            })
        });
        cx.observe_in(&input, window, |this, _, window, cx| {
            this.inline_input_changed(window, cx)
        })
        .detach();
        cx.observe_in(&color, window, |this, _, window, cx| {
            this.inline_color_changed(window, cx)
        })
        .detach();
        cx.subscribe_in(
            &color,
            window,
            |this, picker, _: &gpui_kit::base::ColorPickerEvent, window, cx| {
                if this
                    .inline_text
                    .as_ref()
                    .is_none_or(|draft| draft.color.entity_id() != picker.entity_id())
                {
                    return;
                }
                if !picker.read(cx).is_open() {
                    if let Some(edit) = this
                        .inline_text
                        .as_mut()
                        .and_then(|draft| draft.color_edit.as_mut())
                    {
                        edit.committed = true;
                    }
                    this.inline_color_changed(window, cx);
                }
            },
        )
        .detach();
        input.update(cx, |input, cx| {
            input.set_value(style.content.clone(), window, cx);
            input.set_text_align(
                match style.alignment {
                    objects::TextAlignment::Left => gpui_kit::TextAlign::Left,
                    objects::TextAlignment::Center => gpui_kit::TextAlign::Center,
                    objects::TextAlignment::Right => gpui_kit::TextAlign::Right,
                },
                cx,
            );
            input.focus(window, cx);
        });
        let font_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search fonts"));
        cx.observe(&font_search, |_, _, cx| cx.notify()).detach();
        self.inline_text = Some(InlineTextDraft {
            layer,
            style,
            origin,
            input,
            color,
            color_edit: None,
            typing_color: None,
            typing_font: None,
            font_edit: None,
            font_search,
            last_selection: 0..0,
            history: std::collections::VecDeque::new(),
            restore_text_history: false,
        });
        self.tool = Tool::Text;
        self.schedule_inline_preview(cx);
        self.status =
            "Editing text on canvas · Enter: new line · Ctrl+Enter: finish · Escape: cancel".into();
        cx.notify();
    }

    /// Draft keystrokes never mutate the document or consume document undo entries.
    /// A failed apply keeps the input and its native selection/IME state available.
    fn finish_inline_text(
        &mut self,
        apply: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.cancel_inline_font(window, cx);
        if apply && let Err(error) = self.sync_inline_text(cx) {
            self.status = format!("Text: {error:#}");
            cx.notify();
            return false;
        }
        let Some(draft) = self.inline_text.as_ref() else {
            return true;
        };
        if apply {
            let style = draft.style.clone();
            let result = if let Some(id) = draft.layer.as_ref() {
                if !editable_text(&self.editor.document.layers, id, true, false) {
                    Err(anyhow::anyhow!(
                        "Text layer or parent is locked, hidden or no longer available"
                    ))
                } else {
                    self.editor.set_live_text(id, style).map(|_| ())
                }
            } else if style.content.trim().is_empty() {
                Ok(())
            } else {
                let name: String = style
                    .content
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(40)
                    .collect();
                objects::live_text_layer(
                    &name,
                    objects::ObjectPoint {
                        x: draft.origin.0,
                        y: draft.origin.1,
                    },
                    style,
                )
                .and_then(|layer| {
                    anyhow::ensure!(
                        !self.editor.import_layer(layer).is_empty(),
                        "The text could not be added within the document limits"
                    );
                    Ok(())
                })
            };
            if let Err(error) = result {
                self.status = format!("Text: {error:#}");
                cx.notify();
                return false;
            }
        }
        self.inline_text = None;
        self.end_inline_preview(cx);
        self.drag_start = None;
        self.selection_box = None;
        self.status = if apply {
            "Text applied"
        } else {
            "Text edit cancelled"
        }
        .into();
        self.focus.focus(window, cx);
        if apply {
            self.changed(cx);
        } else {
            cx.notify();
        }
        true
    }

    fn inline_text_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let draft = self.inline_text.as_ref().expect("inline text draft");
        let t = cx.omarchy().clone();
        let bounds = self.viewport.get();
        let width = f32::from(bounds.size.width).max(240.);
        let height = f32::from(bounds.size.height).max(180.);
        let editor_width = draft
            .style
            .box_size
            .map(|b| b.width * self.zoom)
            .unwrap_or(420.)
            .clamp(220., width.max(220.) - 8.);
        let left = f32::from(bounds.origin.x)
            + ((width - self.editor.document.width as f32 * self.zoom) / 2.
                + self.pan.0
                + draft.origin.0 * self.zoom)
                .clamp(4., (width - editor_width - 4.).max(4.));
        // Keep the typing controls below the artwork instead of covering the
        // exact text/effects preview with a second, differently rendered font.
        let top = f32::from(bounds.origin.y) + (height - 250.).max(4.);
        let input = textarea("inline-text-input", &draft.input, window, cx)
            .debug_selector(|| "inline-text-input".into())
            .min_h(px(56.))
            .max_h(px(
                (height - (top - f32::from(bounds.origin.y)) - 84.).max(56.)
            ));
        // Occlude background controls while editing. Clicking outside applies once;
        // it never also activates the control beneath the click.
        div()
            .id("inline-text-surface")
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Undo, window, cx| {
                    this.arm_inline_history(window, cx);
                    cx.propagate();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Redo, window, cx| {
                    this.arm_inline_history(window, cx);
                    cx.propagate();
                }),
            )
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.cancel_inline_font(window, cx) {
                        this.finish_inline_text(true, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("inline-text-box")
                    .debug_selector(|| "inline-text-box".into())
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(editor_width))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.accent)
                    .p_1()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.secondary)
                            .child("LIVE TEXT · select letters to style them"),
                    )
                    .child(input)
                    .child(
                        button(
                            "inline-font-trigger",
                            format!("Font · {}", self.inline_font_label(cx)),
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "inline-font-trigger".into())
                        .w_full()
                        .h(px(28.))
                        .py_0()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.begin_inline_font(window, cx)),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .pt_1()
                            .child(color_picker("inline-text-colour", &draft.color, window, cx))
                            .child(div().text_sm().text_color(t.secondary).child(
                                if draft.input.read(cx).selected_range().is_empty() {
                                    "Typing colour"
                                } else {
                                    "Selection colour"
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .pt_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(t.secondary)
                                    .child("Ctrl+Enter to finish"),
                            )
                            .child(
                                button("inline-text-cancel", "Cancel", ButtonVariant::Outline, cx)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.finish_inline_text(false, window, cx);
                                    })),
                            )
                            .child(
                                button("inline-text-apply", "Apply", ButtonVariant::Primary, cx)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.finish_inline_text(true, window, cx);
                                    })),
                            ),
                    ),
            )
            .child(self.inline_font_popup(
                left,
                (top - 300.).max(f32::from(bounds.origin.y) + 4.),
                editor_width,
                window,
                cx,
            ))
            .into_any_element()
    }

    fn canvas_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let viewport = self.viewport.clone();
        let display = self
            .image_trace_display()
            .or_else(|| self.vector_canvas_display())
            .unwrap_or(&self.display);
        let document_width = self.editor.document.width;
        let document_height = self.editor.document.height;
        if !self.mask_inspection.active() {
            self.mask_preview.borrow_mut().take();
        }
        let mask_display = self.mask_inspection.target().and_then(|id| {
            let layer = self.editor.document.find_layer(id)?;
            let mask = layer.mask.as_ref()?;
            let revision = self.editor.revision();
            let placement =
                self.editor
                    .mask_placement(&layer.id)
                    .unwrap_or(omuse::editor::LayerPlacement {
                        x: layer.offset_x,
                        y: layer.offset_y,
                        width: mask.width() as f32,
                        height: mask.height() as f32,
                        rotation: layer.rotation,
                        flip_x: layer.scale_x < 0.,
                        flip_y: layer.scale_y < 0.,
                    });
            let mask_ptr = mask.as_raw().as_ptr() as usize;
            if let Some((
                cached_editor,
                cached_id,
                cached_revision,
                cached_ptr,
                cached_placement,
                cached,
            )) = self.mask_preview.borrow().as_ref()
                && *cached_editor == self.editor.instance_id()
                && cached_id == id
                && *cached_revision == revision
                && *cached_ptr == mask_ptr
                && *cached_placement == placement
            {
                return Some(cached.clone());
            }
            let outside = omuse::effects::mask_outside_coverage(&layer.metadata, mask);
            let mut preview = image::RgbaImage::new(document_width, document_height);
            let (sin, cos) = placement.rotation.to_radians().sin_cos();
            for (x, y, pixel) in preview.enumerate_pixels_mut() {
                let dx = x as f32 + 0.5 - placement.center().0;
                let dy = y as f32 + 0.5 - placement.center().1;
                let mut ux = (dx * cos + dy * sin) / placement.width + 0.5;
                let mut uy = (-dx * sin + dy * cos) / placement.height + 0.5;
                if placement.flip_x {
                    ux = 1. - ux;
                }
                if placement.flip_y {
                    uy = 1. - uy;
                }
                let value = if (0. ..=1.).contains(&ux) && (0. ..=1.).contains(&uy) {
                    let mx = (ux * mask.width() as f32)
                        .floor()
                        .min(mask.width() as f32 - 1.) as u32;
                    let my = (uy * mask.height() as f32)
                        .floor()
                        .min(mask.height() as f32 - 1.) as u32;
                    let p = mask.get_pixel(mx, my);
                    ((0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
                        * p[3] as f32
                        / 255.)
                        .round() as u8
                } else {
                    outside
                };
                *pixel = image::Rgba([value, value, value, 255]);
            }
            let surface = DisplaySurface::new(&preview);
            *self.mask_preview.borrow_mut() = Some((
                self.editor.instance_id(),
                id.to_owned(),
                revision,
                mask_ptr,
                placement,
                surface.clone(),
            ));
            Some(surface)
        });
        let display = mask_display.as_ref().unwrap_or(display);
        let tiles = display.snapshot();
        let display_dimensions = display.dimensions();
        let vector_overlay = self.vector_canvas_overlay();
        let vector_outline = self.vector_outline_active();
        let reference = self.display_reference.clone();
        let probe_matte = if mask_display.is_some() {
            Some(0x333333)
        } else {
            self.display_probe_matte
        };
        let textures = self.canvas_textures.clone();
        let zoom = self.zoom;
        let safe_areas = self.create.safe_areas;
        let safe_insets = self.create.safe_preset.insets();
        let pan = self.pan;
        let w = document_width as f32;
        let h = document_height as f32;
        let selection = self.selection_box;
        let crop_rect = self.crop.as_ref().map(|crop| crop.rect);
        let active_tool = self.tool;
        let shape_corner_radius = self.shape_corner_radius;
        let shape_line_width = self.shape_line_width;
        let selection_contour = self.contour_for_canvas(cx);
        let interaction_dragging = self.drag_start.is_some();
        let selection_ant_phase = self.selection_ant_phase as u32;
        let transform = if !self.image_trace_active()
            && !self.vector_scene_active()
            && self.crop.is_none()
            && self.tool == Tool::Move
            && self.preferences.transform_box
        {
            self.transform_draft
                .or_else(|| selection_bounds(&self.editor, &self.layer_selection.ids))
                .and_then(|placement| TransformGeometry::new(placement, 24. / zoom))
        } else {
            None
        };
        let accent = cx.omarchy().accent;
        let show_grid = self.show_grid;
        let grid_settings = omuse::canvas_grid::GridSettings {
            spacing: self.preferences.grid_spacing,
            subdivisions: self.preferences.grid_subdivisions,
        };
        let distort = self.distort_draft;
        let show_rulers = self.preferences.rulers;
        let guides: Vec<(bool, f32)> = if self.show_guides {
            self.editor
                .guides()
                .into_iter()
                .map(|guide| {
                    let position = self
                        .guide_drag
                        .as_ref()
                        .filter(|(id, _, _)| id == &guide.id)
                        .map(|(_, _, position)| *position)
                        .unwrap_or(guide.position);
                    (guide.axis == omuse::editor::GuideAxis::Vertical, position)
                })
                .collect()
        } else {
            vec![]
        };
        div()
            .id("artwork")
            .debug_selector(|| "artwork".into())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(cx.omarchy().inset)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if this.dialog != Dialog::None || this.busy {
                    return;
                }
                if this.guard_image_trace(cx) {
                    return;
                }
                if this.vector_before_import(paths.0.to_vec(), window, cx) {
                    return;
                }
                this.finish_interaction(cx);
                this.import_photos_background(paths.0.to_vec(), window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    if this.dialog == Dialog::None && !this.busy {
                        if this.vector_scene_active() || this.image_trace_active() {
                            this.inspector_visible = true;
                            this.inspector_tab = studio_ui::InspectorTab::Layers;
                            cx.notify();
                            return;
                        }
                        this.finish_interaction(cx);
                        this.dialog = Dialog::LayerMenu;
                        this.modal_focus.focus(window, cx);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::middle_down))
            .on_mouse_move(cx.listener(Self::moved))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::middle_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::middle_up))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if this.dialog != Dialog::None || this.busy {
                    return;
                }
                let d = event.delta.pixel_delta(px(24.));
                if event.modifiers.shift {
                    this.pan.0 += f32::from(d.y);
                    this.pan.1 += f32::from(d.x);
                } else {
                    this.set_zoom(
                        this.zoom * (1. + f32::from(d.y) * 0.005).clamp(0.5, 2.),
                        Some(event.position),
                    );
                }
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, _, _| {
                        viewport.set(bounds);
                    },
                    move |bounds, _, window, cx| {
                        let rect = Bounds::new(
                            point(
                                bounds.origin.x
                                    + (bounds.size.width - px(w * zoom)) / 2.
                                    + px(pan.0),
                                bounds.origin.y
                                    + (bounds.size.height - px(h * zoom)) / 2.
                                    + px(pan.1),
                            ),
                            size(px(w * zoom), px(h * zoom)),
                        );
                        let clipped = rect.intersect(&bounds);
                        // Neutral checkerboard is artwork transparency, independent of the UI theme.
                        window.paint_quad(fill(clipped, rgb(probe_matte.unwrap_or(0xd6d6d6))));
                        let tile = px(12.);
                        let cols = (f32::from(clipped.size.width) / 12.).ceil() as i32;
                        let rows = if probe_matte.is_some() {
                            0
                        } else {
                            (f32::from(clipped.size.height) / 12.).ceil() as i32
                        };
                        for y in 0..rows {
                            for x in 0..cols {
                                if (x + y) % 2 == 0 {
                                    let cell = Bounds::new(
                                        point(
                                            clipped.origin.x + tile * x as f32,
                                            clipped.origin.y + tile * y as f32,
                                        ),
                                        size(tile, tile),
                                    )
                                    .intersect(&clipped);
                                    window.paint_quad(fill(cell, rgb(0xf4f4f4)));
                                }
                            }
                        }
                        if !vector_outline {
                            paint_display_tiles(
                                &tiles,
                                reference.as_ref(),
                                &textures,
                                rect,
                                display_dimensions,
                                bounds,
                                window,
                                cx,
                            );
                        }
                        if safe_areas {
                            let [left, top, right, bottom] = safe_insets;
                            let inset_x = rect.size.width * left;
                            let inset_y = rect.size.height * top;
                            let safe = Bounds::new(
                                rect.origin + point(inset_x, inset_y),
                                size(
                                    rect.size.width * (1. - left - right),
                                    rect.size.height * (1. - top - bottom),
                                ),
                            );
                            for line in [
                                Bounds::new(safe.origin, size(safe.size.width, px(1.))),
                                Bounds::new(
                                    point(safe.origin.x, safe.bottom_right().y),
                                    size(safe.size.width, px(1.)),
                                ),
                                Bounds::new(safe.origin, size(px(1.), safe.size.height)),
                                Bounds::new(
                                    point(safe.bottom_right().x, safe.origin.y),
                                    size(px(1.), safe.size.height),
                                ),
                            ] {
                                window.paint_quad(fill(line.intersect(&clipped), accent));
                            }
                        }
                        if show_rulers {
                            let ruler = px(18.);
                            window.paint_quad(fill(
                                Bounds::new(rect.origin, size(rect.size.width, ruler)),
                                rgba(0x20242acc),
                            ));
                            window.paint_quad(fill(
                                Bounds::new(rect.origin, size(ruler, rect.size.height)),
                                rgba(0x20242acc),
                            ));
                            let ruler_step = if zoom >= 1. { 50. } else { 200. };
                            for n in 0..=((w / ruler_step).ceil() as usize).min(2048) {
                                let x = rect.origin.x + px(n as f32 * ruler_step * zoom);
                                window.paint_quad(fill(
                                    Bounds::new(point(x, rect.origin.y), size(px(1.), px(7.))),
                                    rgba(0xffffffbb),
                                ));
                            }
                            for n in 0..=((h / ruler_step).ceil() as usize).min(2048) {
                                let y = rect.origin.y + px(n as f32 * ruler_step * zoom);
                                window.paint_quad(fill(
                                    Bounds::new(point(rect.origin.x, y), size(px(7.), px(1.))),
                                    rgba(0xffffffbb),
                                ));
                            }
                        }
                        if show_grid {
                            let min_grid_x = ((f32::from(clipped.origin.x - rect.origin.x) / zoom)
                                - grid_settings.minor_spacing())
                            .max(0.);
                            let max_grid_x = (f32::from(clipped.bottom_right().x - rect.origin.x)
                                / zoom
                                + grid_settings.minor_spacing())
                            .min(w);
                            let min_grid_y = ((f32::from(clipped.origin.y - rect.origin.y) / zoom)
                                - grid_settings.minor_spacing())
                            .max(0.);
                            let max_grid_y = (f32::from(clipped.bottom_right().y - rect.origin.y)
                                / zoom
                                + grid_settings.minor_spacing())
                            .min(h);
                            for (x, major) in
                                grid_settings.visible_lines(min_grid_x, max_grid_x, zoom)
                            {
                                let line = Bounds::new(
                                    point(rect.origin.x + px(x * zoom), rect.origin.y),
                                    size(px(1.), rect.size.height),
                                )
                                .intersect(&clipped);
                                window.paint_quad(fill(
                                    line,
                                    if major {
                                        rgba(0x555555aa)
                                    } else {
                                        rgba(0x55555555)
                                    },
                                ));
                            }
                            for (y, major) in
                                grid_settings.visible_lines(min_grid_y, max_grid_y, zoom)
                            {
                                let line = Bounds::new(
                                    point(rect.origin.x, rect.origin.y + px(y * zoom)),
                                    size(rect.size.width, px(1.)),
                                )
                                .intersect(&clipped);
                                window.paint_quad(fill(
                                    line,
                                    if major {
                                        rgba(0x555555aa)
                                    } else {
                                        rgba(0x55555555)
                                    },
                                ));
                            }
                        }
                        for (vertical, pos) in &guides {
                            let line = if *vertical {
                                Bounds::new(
                                    point(rect.origin.x + px(pos * zoom), rect.origin.y),
                                    size(px(1.), rect.size.height),
                                )
                            } else {
                                Bounds::new(
                                    point(rect.origin.x, rect.origin.y + px(pos * zoom)),
                                    size(rect.size.width, px(1.)),
                                )
                            }
                            .intersect(&clipped);
                            window.paint_quad(fill(line, rgb(0x00cfff)));
                        }
                        if let Some(crop) = crop_rect {
                            crop_ui::paint_crop(crop, rect, clipped, zoom, window);
                        }
                        if let Some(mut geometry) = transform {
                            if let Some(corners) = distort {
                                for i in 0..4 {
                                    geometry.handles[i * 2] = corners[i];
                                    geometry.handles[i * 2 + 1] = CanvasPoint {
                                        x: (corners[i].x + corners[(i + 1) % 4].x) * 0.5,
                                        y: (corners[i].y + corners[(i + 1) % 4].y) * 0.5,
                                    };
                                }
                            }
                            let screen_point = |p: CanvasPoint| {
                                point(
                                    rect.origin.x + px(p.x * zoom),
                                    rect.origin.y + px(p.y * zoom),
                                )
                            };
                            for (a, b) in [(0, 2), (2, 4), (4, 6), (6, 0)] {
                                let start = geometry.handles[a];
                                let end = geometry.handles[b];
                                let length = (end.x - start.x).hypot(end.y - start.y) * zoom;
                                let steps = (length / 4.).ceil().clamp(1., 4096.) as usize;
                                for step in 0..=steps {
                                    let t = step as f32 / steps as f32;
                                    let p = CanvasPoint {
                                        x: start.x + (end.x - start.x) * t,
                                        y: start.y + (end.y - start.y) * t,
                                    };
                                    let screen = screen_point(p);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(screen.x - px(1.), screen.y - px(1.)),
                                            size(px(2.), px(2.)),
                                        ),
                                        accent,
                                    ));
                                }
                            }
                            for handle in geometry.handles {
                                let screen = screen_point(handle);
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(screen.x - px(4.), screen.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    accent,
                                ));
                            }
                            let rotation = screen_point(geometry.rotation_handle);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(rotation.x - px(5.), rotation.y - px(5.)),
                                    size(px(10.), px(10.)),
                                ),
                                accent,
                            ));
                        }
                        for &(x, y) in &selection_contour.points {
                            let screen_phase = ((x + y) as f32 * zoom).round() as u32;
                            let color = if ((screen_phase + selection_ant_phase) / 4) % 2 == 0 {
                                rgb(0xffffff)
                            } else {
                                rgb(0x000000)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        rect.origin.x + px(x as f32 * zoom),
                                        rect.origin.y + px(y as f32 * zoom),
                                    ),
                                    size(
                                        px((selection_contour.cell_size as f32 * zoom)
                                            .min(zoom.max(2.))
                                            .max(1.)),
                                        px((selection_contour.cell_size as f32 * zoom)
                                            .min(zoom.max(2.))
                                            .max(1.)),
                                    ),
                                )
                                .intersect(&clipped),
                                color,
                            ));
                        }
                        if let Some(overlay) = vector_overlay {
                            overlay.paint(rect, bounds, window);
                        }
                        if interaction_dragging && let Some((x, y, sw, sh)) = selection {
                            let sel = Bounds::new(
                                point(rect.origin.x + px(x * zoom), rect.origin.y + px(y * zoom)),
                                size(px(sw * zoom), px(sh * zoom)),
                            )
                            .intersect(&bounds);
                            let preview = outline(sel, accent, BorderStyle::Solid);
                            let preview = match active_tool {
                                Tool::ShapeRect => preview
                                    .corner_radii(px(shape_corner_radius * zoom))
                                    .border_widths(px(shape_line_width.max(1.) * zoom)),
                                Tool::ShapeEllipse => preview
                                    .corner_radii(px(f32::from(
                                        sel.size.width.min(sel.size.height),
                                    ) * 0.5))
                                    .border_widths(px(shape_line_width.max(1.) * zoom)),
                                Tool::Line => {
                                    preview.border_widths(px(shape_line_width.max(1.) * zoom))
                                }
                                _ => preview,
                            };
                            window.paint_quad(preview);
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
    fn dialog_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.dialog == Dialog::Recent {
            return self.recent_view(window, cx);
        }
        if self.dialog == Dialog::ExternalChange {
            return self.external_view(window, cx);
        }
        if self.dialog == Dialog::CommandSearch {
            return self.command_search_view(window, cx);
        }
        if self.dialog == Dialog::Text {
            self.ensure_font_names(cx);
        }
        let t = cx.omarchy().clone();
        let title = match self.dialog {
            Dialog::Unsaved => "Save your changes?",
            Dialog::Save => "Save project",
            Dialog::Export => "Export image",
            Dialog::New => "New document",
            Dialog::Resize => "Resize canvas",
            Dialog::Rename => "Rename layer",
            Dialog::Open => "Open document",
            Dialog::Import => "Import image",
            Dialog::Recover => "Recover previous work?",
            Dialog::Filter => "Adjustments",
            Dialog::Text => "Add text",
            Dialog::LayerMenu => "Layer actions",
            Dialog::Nest => "Move selected layers into a group",
            Dialog::Trim => "Trim canvas",
            Dialog::Shortcuts => "Keyboard shortcuts",
            Dialog::CommandSearch => "Commands & shortcuts",
            Dialog::Recent => "Recent projects",
            Dialog::ExternalChange => "Project changed on disk",
            Dialog::Transform => "Transform layer",
            Dialog::ResizeImage => "Resize image",
            Dialog::Shape => "Edit shape",
            Dialog::Effects => "Live effects",
            Dialog::MaskTransform => "Place mask independently",
            Dialog::Guide => "Canvas guides",
            Dialog::Distort => "Distort layer corners",
            Dialog::LiveMask => "Choose a live mask source",
            Dialog::Adjustment => "Live adjustment layer",
            Dialog::Selection => "Modify selection",
            Dialog::RawImport => "Develop camera RAW",
            Dialog::SvgImport => "Import SVG artwork",
            Dialog::CameraRaw => "Camera Raw",
            Dialog::SubjectRefine => "Refine subject matte",
            Dialog::RangeMask => {
                if self.range_draft.as_ref().is_some_and(|draft| draft.color) {
                    "Colour range"
                } else {
                    "Luminosity range"
                }
            }
            Dialog::VectorPath => {
                if self
                    .vector_draft
                    .as_ref()
                    .is_some_and(|draft| draft.is_scene())
                {
                    "Vector artwork"
                } else {
                    "Vector paths & masks"
                }
            }
            Dialog::Workflow => self.workflow_title(),
            Dialog::Pro => self.pro_title(),
            Dialog::Finishing => "Finishing effects",
            Dialog::ImportReport => "Import conversion report",
            Dialog::ExportReport => "Last export conversion report",
            Dialog::ToolSettings => "Tool settings",
            Dialog::Gradient => "Gradient preview",
            Dialog::None => "",
        };
        let mut body = div()
            .id("dialog-body")
            .debug_selector(|| "dialog-body".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .pb_2()
                    .border_b_1()
                    .border_color(t.divider())
                    .text_size(px(20.))
                    .text_color(t.bright)
                    .child(title),
            );
        let mut footer = None;
        if matches!(self.dialog, Dialog::ImportReport | Dialog::ExportReport) {
            let notes = if self.dialog == Dialog::ExportReport {
                &self.export_notes
            } else {
                &self.import_notes
            };
            if notes.is_empty() {
                body = body.child("No conversion notes were recorded.");
            } else {
                for note in notes {
                    body = body.child(div().text_sm().child(format!("• {note}")));
                }
            }
            body = body.child(
                button("close-import-report", "Close", ButtonVariant::Primary, cx).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.dialog = Dialog::None;
                        this.focus.focus(window, cx);
                        cx.notify();
                    }),
                ),
            );
        } else if self.dialog == Dialog::Nest {
            fn groups(layers: &[Layer], depth: usize, out: &mut Vec<(String, String)>) {
                for l in layers.iter().rev() {
                    if l.is_group() {
                        out.push((l.id.clone(), format!("{}{}", "  ".repeat(depth), l.name)));
                        groups(&l.children, depth + 1, out);
                    }
                }
            }
            let mut choices = Vec::new();
            groups(&self.editor.document.layers, 0, &mut choices);
            if choices.is_empty() {
                body = body.child("Create a group first.");
            }
            for (id, name) in choices {
                let reason = self.layer_nest_target_unavailable(&id);
                let label = reason.map_or(name.clone(), |reason| format!("{name} — {reason}"));
                body = body.child(button(SharedString::from(format!("nest-target-{id}")),label,ButtonVariant::Outline,cx).disabled(reason.is_some()).on_click(cx.listener(move |this,_,window,cx| {
                    if let Some(reason) = this.layer_nest_target_unavailable(&id) { this.status=reason.into(); cx.notify(); return; }
                    let ids = this.selected_layer_ids();
                    let moved = this.editor.drop_layers(&ids,Some(&id),0,false);
                    if !moved.is_empty() { this.select_layer_ids(moved); this.dialog=Dialog::None; this.focus.focus(window,cx); this.changed(cx); }
                    else { this.status="Cannot move into this group: locked, unchanged or would create a cycle".into(); cx.notify(); }
                })));
            }
            body = body.child(div().child(self.status.clone())).child(
                button("cancel-nest", "Cancel", ButtonVariant::Outline, cx).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.dialog = Dialog::None;
                        this.focus.focus(window, cx);
                        cx.notify();
                    },
                )),
            );
        } else if self.dialog == Dialog::LayerMenu {
            body = body.child(self.layer_actions_body(cx));
        } else if self.dialog == Dialog::Unsaved {
            let saving = self.create.saving;
            body = body.child("This document has changes that have not been saved.");
            if saving {
                body = body.child(div().text_sm().text_color(t.warning).child(
                    "Saving… Keep editing to cancel this navigation; the save will finish.",
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        button(
                            "cancel-unsaved",
                            if saving {
                                "Keep editing while saving"
                            } else {
                                "Cancel"
                            },
                            ButtonVariant::Outline,
                            cx,
                        )
                        .debug_selector(|| "cancel-unsaved".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pending = None;
                            this.dialog = Dialog::None;
                            this.dialog_generation += 1;
                            this.focus.focus(window, cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        button("discard", "Discard changes", ButtonVariant::Outline, cx)
                            .debug_selector(|| "discard".into())
                            .disabled(saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.saving_blocks_navigation(cx) {
                                    return;
                                }
                                this.dialog = Dialog::None;
                                if let Some(p) = this.pending.take() {
                                    this.perform(p, window, cx);
                                }
                            })),
                    )
                    .child(
                        button(
                            "save-pending",
                            if saving { "Saving…" } else { "Save" },
                            ButtonVariant::Primary,
                            cx,
                        )
                        .debug_selector(|| "save-pending".into())
                        .disabled(saving)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.saving_blocks_navigation(cx) {
                                this.save(window, cx);
                            }
                        })),
                    ),
            );
        } else if self.dialog == Dialog::Recover {
            body=body.child("A recovery copy from an earlier Rust editor session is available. Your saved project will stay unchanged.")
                .child(div().flex().gap_2()
                    .child(button("skip-recovery","Keep for later",ButtonVariant::Outline,cx).on_click(cx.listener(|this,_,_,cx|{this.dialog=Dialog::None;cx.notify();})))
                    .child(button("recover","Recover",ButtonVariant::Primary,cx).on_click(cx.listener(|this,_,_,cx|{if let Some(path)=this.recovery.available(){match Self::open_content(&path){Ok((doc,project))=>{this.install_opened_content(doc,project);this.editor.mark_unsaved();this.path=None;this.dialog=Dialog::None;this.status="Recovered a copy. Use Save to choose a project location.".into();this.refresh(cx);},Err(e)=>this.status=format!("Recovery failed: {e:#}")}}cx.notify();}))));
        } else {
            if self.dialog == Dialog::SvgImport {
                body = body.child(self.svg_import_body(window, cx));
            } else if self.dialog == Dialog::RawImport {
                body = body.child(self.path_input.read(cx).value().to_string());
                for (i, label) in [
                    "Exposure (−3 to 3 stops)",
                    "Temperature (2000–12000 K; 5000 retains as-shot)",
                    "Tint (−150 to 150; 0 retains as-shot)",
                    "Tone boost (0–1)",
                ]
                .iter()
                .enumerate()
                {
                    body = body.child(div().child(*label).child(input(
                        SharedString::from(format!("raw-import-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )));
                }
                body = body
                    .child("Develops locally with LibRaw. The original camera file is unchanged.");
            } else if self.dialog == Dialog::CameraRaw {
                body = body.child(crate::camera_scopes::panel(
                    self.camera_scopes.clone(),
                    if self.busy {
                        "Updating preview…"
                    } else if self.camera_scopes_preview {
                        "Last preview · before clipping warnings"
                    } else {
                        "Original layer"
                    },
                    cx,
                ));
                let mut tabs = div().flex().flex_wrap().gap_1();
                for (i, (_, name)) in crate::camera_controls::SECTIONS.iter().enumerate() {
                    tabs = tabs.child(
                        button(
                            SharedString::from(format!("camera-tab-{i}")),
                            *name,
                            if i == self.camera_section {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            if this.busy {
                                return;
                            }
                            match this.read_camera_form(cx) {
                                Ok(()) => {
                                    this.camera_section = i;
                                    this.load_camera_form(w, cx);
                                }
                                Err(e) => this.status = e.to_string(),
                            }
                            cx.notify();
                        })),
                    );
                }
                body = body.child(tabs);
                let curve_section = crate::camera_controls::SECTIONS
                    .get(self.camera_section)
                    .is_some_and(|section| section.0 == "curve");
                if curve_section {
                    let mut channels = div().flex().gap_1();
                    for (index, label) in ["RGB", "Red", "Green", "Blue"].iter().enumerate() {
                        channels = channels.child(
                            button(
                                SharedString::from(format!("camera-curve-channel-{index}")),
                                *label,
                                if self.camera_curve_channel == index {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Outline
                                },
                                cx,
                            )
                            .disabled(self.busy)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if this.busy {
                                        return;
                                    }
                                    if let Err(error) = this.read_camera_form(cx) {
                                        this.status = error.to_string();
                                        return;
                                    }
                                    this.camera_curve_channel = index;
                                    this.load_camera_form(window, cx);
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    body = body.child(channels);
                    if self.busy {
                        body =
                            body.child("Curve editing is paused while the preview is processing.");
                    } else {
                        body = body
                            .child(self.camera_curve.clone())
                            .child("Click to add · Drag to adjust · Right-click to remove");
                    }
                }
                let section_key = crate::camera_controls::SECTIONS
                    .get(self.camera_section)
                    .map(|section| section.0)
                    .unwrap_or("");
                body = body.child(self.camera_gesture_controls(cx));
                if matches!(section_key, "mixer" | "geometry") {
                    if !self.busy {
                        body = body.child(self.camera_canvas.clone()).child(
                            if section_key == "mixer" {
                                "Click the preview to sample a point color"
                            } else {
                                "Drag along a line to add a geometry guide"
                            },
                        );
                    }
                    let noun = if section_key == "mixer" {
                        "point color"
                    } else {
                        "guide"
                    };
                    body = body.child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                button(
                                    "camera-array-add",
                                    format!("Add {noun}"),
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .disabled(self.busy)
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.edit_camera_array(true, window, cx),
                                )),
                            )
                            .child(
                                button(
                                    "camera-array-remove",
                                    format!("Remove last {noun}"),
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .disabled(self.busy)
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.edit_camera_array(false, window, cx),
                                )),
                            ),
                    );
                }
                for (i, f) in
                    crate::camera_controls::fields(&self.camera_draft, self.camera_section)
                        .iter()
                        .enumerate()
                {
                    if curve_section && f.value.is_array() {
                        continue;
                    }
                    if let Some(spec) = Self::camera_numeric_spec(f) {
                        body = body.child(self.numeric_row(
                            format!("camera-field-{i}"),
                            f.label.clone(),
                            numeric_ui::Target::Detail(i),
                            spec,
                            window,
                            cx,
                        ));
                    } else {
                        body = body.child(div().child(f.label.clone()).child(input(
                            SharedString::from(format!("camera-field-{i}")),
                            &self.detail_inputs[i],
                            window,
                            cx,
                        )));
                    }
                }
                body = body.child(inspector_ui::numeric_hint(cx));
                for (id, label, enabled, shadow) in [
                    (
                        "camera-clip-shadows",
                        "Shadow clipping",
                        self.camera_clip_shadows,
                        true,
                    ),
                    (
                        "camera-clip-highlights",
                        "Highlight clipping",
                        self.camera_clip_highlights,
                        false,
                    ),
                ] {
                    body = body.child(
                        button(
                            id,
                            format!("{label}: {}", if enabled { "on" } else { "off" }),
                            ButtonVariant::Outline,
                            cx,
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.busy {
                                return;
                            }
                            if shadow {
                                this.camera_clip_shadows = !this.camera_clip_shadows;
                            } else {
                                this.camera_clip_highlights = !this.camera_clip_highlights;
                            }
                            match this.read_camera_form(cx).and_then(|_| {
                                serde_json::from_value(this.camera_draft.clone())
                                    .map_err(Into::into)
                            }) {
                                Ok(settings) => this.start_camera_raw(settings, true, cx),
                                Err(error) => {
                                    this.status = error.to_string();
                                    cx.notify();
                                }
                            }
                        })),
                    );
                }
                body = body.child(
                    button("camera-preview", "Preview", ButtonVariant::Outline, cx)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.busy {
                                return;
                            }
                            match this.read_camera_form(cx).and_then(|_| {
                                serde_json::from_value(this.camera_draft.clone())
                                    .map_err(Into::into)
                            }) {
                                Ok(settings) => this.start_camera_raw(settings, true, cx),
                                Err(e) => {
                                    this.status = e.to_string();
                                    cx.notify();
                                }
                            }
                        })),
                );
                body=body.child("Preview keeps the original intact. Apply commits one undo step; Cancel restores the original view.");
            } else if self.dialog == Dialog::Pro {
                body = body.child(self.pro_controls(window, cx));
            } else if self.dialog == Dialog::Finishing {
                body = body.child(self.finishing_controls(window, cx));
            } else if self.dialog == Dialog::Workflow {
                body = body.child(self.workflow_controls(window, cx));
            } else if self.dialog == Dialog::VectorPath {
                body = body.child(self.render_vector(window, cx));
            } else if self.dialog == Dialog::RangeMask {
                body = body.child(self.range_controls(window, cx));
            } else if self.dialog == Dialog::SubjectRefine {
                for (i, label) in [
                    "Edge refinement (0–100 px)",
                    "Contrast (0–100)",
                    "Edge shift (−100–100)",
                ]
                .iter()
                .enumerate()
                {
                    body = body.child(div().child(*label).child(input(
                        SharedString::from(format!("subject-refine-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )));
                }
                body = body
                    .child(
                        button("subject-preview", "Preview", ButtonVariant::Outline, cx).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.run_subject_refine(false, window, cx)
                            }),
                        ),
                    )
                    .child("Inference used the visible canvas. Preview and Cancel leave the document and undo history unchanged.");
            } else if self.dialog == Dialog::Gradient {
                body = body.child("Opacity (0–100%)").child(input(
                    "gradient-opacity",
                    &self.detail_inputs[0],
                    window,
                    cx,
                ));
                let settings = self.gradient_settings;
                for (id, label, enabled) in [
                    (
                        "gradient-radial",
                        "Radial",
                        settings.kind == GradientKind::Radial,
                    ),
                    ("gradient-transparent", "Transparent", settings.transparent),
                    ("gradient-reverse", "Reverse", settings.reverse),
                ] {
                    body = body.child(
                        button(
                            id,
                            format!("{label}: {}", if enabled { "on" } else { "off" }),
                            ButtonVariant::Outline,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match id {
                                "gradient-radial" => {
                                    this.gradient_settings.kind =
                                        if this.gradient_settings.kind == GradientKind::Linear {
                                            GradientKind::Radial
                                        } else {
                                            GradientKind::Linear
                                        }
                                }
                                "gradient-transparent" => {
                                    this.gradient_settings.transparent =
                                        !this.gradient_settings.transparent
                                }
                                _ => {
                                    this.gradient_settings.reverse = !this.gradient_settings.reverse
                                }
                            }
                            this.preview_gradient(cx);
                        })),
                    );
                }
                body = body.child(
                    button("gradient-preview", "Preview", ButtonVariant::Outline, cx).on_click(
                        cx.listener(|this, _, _, cx| {
                            if let Ok(opacity) =
                                this.detail_inputs[0].read(cx).value().parse::<f32>()
                                && opacity.is_finite()
                                && (0. ..=100.).contains(&opacity)
                            {
                                this.gradient_settings.opacity = opacity / 100.;
                                this.preview_gradient(cx);
                            } else {
                                this.status = "Use gradient opacity from 0 to 100".into();
                                cx.notify();
                            }
                        }),
                    ),
                );
            } else if self.dialog == Dialog::ToolSettings {
                for (index, label) in [
                    "Brush size (1–1024 px)",
                    "Hardness (0–100%)",
                    "Opacity (0–100%)",
                    "Smoothing (0–100%)",
                    "Fill tolerance (0–255)",
                    "Wand tolerance (0–255)",
                    "Wand sample radius (0, 1 or 2)",
                    "Rectangle corner radius (0–5000 px)",
                    "Line shape width (1–5000 px)",
                ]
                .into_iter()
                .enumerate()
                {
                    let spec = match index {
                        0 => numeric_ui::SIZE,
                        1 => numeric_ui::HARDNESS,
                        2 => numeric_ui::OPACITY,
                        3 => numeric_ui::SMOOTHING,
                        4 | 5 => numeric_ui::Spec::new(0., 255., 1., 32., 0),
                        6 => numeric_ui::Spec::new(0., 2., 1., 0., 0),
                        7 => numeric_ui::Spec::new(0., 5000., 1., 0., 2),
                        _ => numeric_ui::Spec::new(1., 5000., 1., 4., 2),
                    };
                    body = body.child(self.numeric_row(
                        format!("tool-setting-{index}"),
                        label,
                        numeric_ui::Target::Detail(index),
                        spec,
                        window,
                        cx,
                    ));
                }
                body = body.child(inspector_ui::numeric_hint(cx));
                for (id, label, field) in [
                    (
                        "wand-contiguous",
                        "Wand contiguous",
                        self.wand_draft.contiguous,
                    ),
                    (
                        "wand-all-layers",
                        "Wand sample all layers",
                        self.wand_draft.all_layers,
                    ),
                    ("wand-aa", "Wand anti-alias", self.wand_draft.anti_alias),
                    ("clone-aligned", "Clone aligned", self.clone_aligned_draft),
                    (
                        "clone-all-layers",
                        "Clone sample all layers",
                        self.clone_all_layers_draft,
                    ),
                ] {
                    body = body.child(
                        button(
                            id,
                            format!("{label}: {}", if field { "on" } else { "off" }),
                            ButtonVariant::Outline,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match id {
                                "wand-contiguous" => {
                                    this.wand_draft.contiguous = !this.wand_draft.contiguous
                                }
                                "wand-all-layers" => {
                                    this.wand_draft.all_layers = !this.wand_draft.all_layers
                                }
                                "wand-aa" => {
                                    this.wand_draft.anti_alias = !this.wand_draft.anti_alias
                                }
                                "clone-aligned" => {
                                    this.clone_aligned_draft = !this.clone_aligned_draft
                                }
                                _ => this.clone_all_layers_draft = !this.clone_all_layers_draft,
                            }
                            cx.notify();
                        })),
                    );
                }
                let mut modes = div().flex().gap_1();
                for (label, mode) in [
                    ("Replace", SelectionMode::Replace),
                    ("Add", SelectionMode::Add),
                    ("Subtract", SelectionMode::Subtract),
                ] {
                    modes = modes.child(
                        button(
                            SharedString::from(format!("selection-mode-{label}")),
                            label,
                            if self.selection_mode_draft == mode {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selection_mode_draft = mode;
                            cx.notify();
                        })),
                    );
                }
                body = body.child("Selection mode").child(modes);
            } else if self.dialog == Dialog::Selection {
                body = body
                    .child(match self.selection_operation {
                        1 => "Expand radius (pixels)",
                        -1 => "Contract radius (pixels)",
                        _ => "Feather radius (pixels)",
                    })
                    .child(input(
                        "selection-radius",
                        &self.detail_inputs[0],
                        window,
                        cx,
                    ));
            } else if self.dialog == Dialog::Adjustment {
                let mut choices = div().flex().flex_wrap().gap_1();
                for (i, label) in crate::adjustment_controls::KINDS.iter().enumerate() {
                    choices = choices.child(
                        button(
                            SharedString::from(format!("adjustment-kind-{i}")),
                            *label,
                            if i == self.adjustment_kind {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(
                            cx.listener(move |this, _, w, cx| this.open_adjustment(i, None, w, cx)),
                        ),
                    );
                }
                body = body.child(choices);
                for (i, f) in crate::adjustment_controls::fields(self.adjustment_kind)
                    .iter()
                    .enumerate()
                {
                    if f.boolean {
                        let path = f.path.clone();
                        let enabled = self
                            .adjustment_draft
                            .pointer(&path)
                            .and_then(|v| v.as_bool())
                            .unwrap_or(f.default != 0.);
                        body = body.child(
                            button(
                                SharedString::from(format!("adjustment-bool-{i}")),
                                format!("{}: {}", f.label, if enabled { "on" } else { "off" }),
                                ButtonVariant::Outline,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    crate::adjustment_controls::set(
                                        &mut this.adjustment_draft,
                                        &path,
                                        (!enabled).into(),
                                    );
                                    cx.notify();
                                },
                            )),
                        );
                    } else {
                        body = body.child(self.adjustment_numeric_row(f, i, window, cx));
                    }
                }
                body = body.child(inspector_ui::numeric_hint(cx));
                if self.adjustment_kind == 3 {
                    body = body.child("Curve control points: x,y; x,y (0–255)");
                    for (i, label) in ["RGB", "Red", "Green", "Blue"].iter().enumerate() {
                        body = body.child(div().child(*label).child(input(
                            SharedString::from(format!("curve-channel-{i}")),
                            &self.detail_inputs[i],
                            window,
                            cx,
                        )));
                    }
                }
            } else if self.dialog == Dialog::Distort {
                for (i, label) in [
                    "Top left X",
                    "Top left Y",
                    "Top right X",
                    "Top right Y",
                    "Bottom right X",
                    "Bottom right Y",
                    "Bottom left X",
                    "Bottom left Y",
                ]
                .iter()
                .enumerate()
                {
                    body = body.child(div().child(*label).child(input(
                        SharedString::from(format!("distort-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )));
                }
                body=body.child("Resamples pixel layers into a convex quadrilateral. Undo restores the original pixels.");
            } else if self.dialog == Dialog::LiveMask {
                let mut choices = vec![(String::new(), "Remove live mask".to_string())];
                fn collect(layers: &[Layer], out: &mut Vec<(String, String)>) {
                    for l in layers {
                        out.push((l.id.clone(), l.name.clone()));
                        collect(&l.children, out);
                    }
                }
                collect(&self.editor.document.layers, &mut choices);
                for (source, label) in choices {
                    if source != self.editor.active_layer {
                        body = body.child(
                            button(
                                SharedString::from(format!("mask-source-{source}")),
                                label,
                                ButtonVariant::Outline,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let id = this.editor.active_layer.clone();
                                    match this.editor.set_live_mask_source(
                                        &id,
                                        if source.is_empty() {
                                            None
                                        } else {
                                            Some(&source)
                                        },
                                    ) {
                                        Ok(_) => {
                                            this.dialog = Dialog::None;
                                            this.changed(cx);
                                        }
                                        Err(e) => this.status = e.to_string(),
                                    }
                                },
                            )),
                        );
                    }
                }
            } else if self.dialog == Dialog::Guide {
                body = body
                    .child("Axis: horizontal or vertical")
                    .child(input("guide-axis", &self.detail_inputs[0], window, cx))
                    .child("Position in canvas pixels")
                    .child(input("guide-position", &self.detail_inputs[1], window, cx));
                for guide in self.editor.guides() {
                    let id = guide.id.clone();
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .child(format!("{:?}: {}", guide.axis, guide.position))
                            .child(
                                button(
                                    SharedString::from(format!("remove-guide-{id}")),
                                    "Remove",
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.editor.remove_guide(&id);
                                        this.changed(cx);
                                    },
                                )),
                            ),
                    )
                }
            } else if self.dialog == Dialog::MaskTransform {
                for (i, label) in ["X", "Y", "Width", "Height", "Rotation"].iter().enumerate() {
                    let spec = match i {
                        0 | 1 => numeric_ui::Spec::new(-1_000_000., 1_000_000., 1., 0., 3),
                        2 | 3 => numeric_ui::Spec::new(
                            1.,
                            300_000.,
                            1.,
                            f64::from(if i == 2 {
                                self.editor.document.width
                            } else {
                                self.editor.document.height
                            }),
                            3,
                        ),
                        _ => numeric_ui::Spec::new(-360., 360., 1., 0., 3),
                    };
                    body = body.child(self.numeric_row(
                        format!("mask-place-{i}"),
                        *label,
                        numeric_ui::Target::Detail(i),
                        spec,
                        window,
                        cx,
                    ));
                }
                body = body.child(inspector_ui::numeric_hint(cx));
            } else if self.dialog == Dialog::Effects {
                let mut choices = div().flex().flex_wrap().gap_1();
                for (i, label) in [
                    "Stroke",
                    "Shadow",
                    "Color overlay",
                    "Inner shadow",
                    "Outer glow",
                    "Inner glow",
                ]
                .iter()
                .enumerate()
                {
                    choices = choices.child(
                        button(
                            SharedString::from(format!("effect-{i}")),
                            *label,
                            if self.effect_kind == i {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.open_effect(i, window, cx)),
                        ),
                    );
                }
                body = body.child(choices);
                for (i, label) in [
                    "Opacity (%)",
                    "Size / blur (pixels)",
                    "Angle (degrees)",
                    "Distance (pixels)",
                ]
                .iter()
                .enumerate()
                {
                    if i == 0
                        || self.effect_kind != 2
                            && (i == 1 || self.effect_kind == 1 || self.effect_kind == 3)
                    {
                        body = body.child(div().child(*label).child(input(
                            SharedString::from(format!("effect-param-{i}")),
                            &self.detail_inputs[i],
                            window,
                            cx,
                        )));
                    }
                }
                if self.effect_kind == 0 {
                    body = body.child(
                        div()
                            .child("Stroke position: 0 outside, 1 inside")
                            .child(input("stroke-inside", &self.detail_inputs[4], window, cx)),
                    );
                }
                body = body.child(color_picker(
                    "effect-color",
                    &self.dialog_color_picker,
                    window,
                    cx,
                ));
                body = body.child(
                    button(
                        "remove-effect",
                        "Remove this effect",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let id = this.editor.active_layer.clone();
                        let mut value = this
                            .editor
                            .document
                            .find_layer(&id)
                            .and_then(|l| l.metadata.get("effects"))
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}));
                        if let Some(o) = value.as_object_mut() {
                            o.remove(effect_key(this.effect_kind));
                        }
                        match this.editor.set_layer_effects(&id, value) {
                            Ok(_) => {
                                this.dialog = Dialog::None;
                                this.changed(cx);
                            }
                            Err(e) => this.status = e.to_string(),
                        }
                    })),
                );
            } else if self.dialog == Dialog::Transform {
                for (i, label) in [
                    "X (pixels)",
                    "Y (pixels)",
                    "Rotation (degrees)",
                    "Horizontal scale (%)",
                    "Vertical scale (%)",
                ]
                .iter()
                .enumerate()
                {
                    let spec = match i {
                        0 | 1 => numeric_ui::Spec::new(-1_000_000., 1_000_000., 1., 0., 3),
                        2 => numeric_ui::Spec::new(-360., 360., 1., 0., 3),
                        _ => numeric_ui::Spec::new(-100_000., 100_000., 1., 100., 3),
                    };
                    body = body.child(self.numeric_row(
                        format!("transform-{i}"),
                        *label,
                        numeric_ui::Target::Detail(i),
                        spec,
                        window,
                        cx,
                    ));
                }
                body = body
                    .child(inspector_ui::numeric_hint(cx))
                    .child("Negative scale flips an axis. Applying creates one undo step.");
            } else if self.dialog == Dialog::Shortcuts {
                body = body.child("Record a key combination, or Clear to leave a command unbound. Escape cancels recording. Apply saves changes; Cancel discards them.");
                body = body.child("Super belongs to Omarchy. Search also matches categories and your current shortcuts.");
                body = body.child(input("shortcut-search", &self.path_input, window, cx));
                let query = self.path_input.read(cx).value();
                let entries = self.shortcut_draft.search(&query);
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(t.secondary)
                        .child(format!("{} commands", entries.len())),
                );
                for entry in entries {
                    let id = entry.definition.id;
                    let recording = self.recording.as_deref() == Some(id);
                    let chord = if entry.chord.is_empty() {
                        "Unbound".into()
                    } else {
                        shortcuts::display_chord(&entry.chord)
                    };
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(entry.definition.label)
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(t.secondary)
                                            .child(shortcuts::category(id)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_family(t.mono_font.clone())
                                    .child(chord),
                            )
                            .child(
                                button(
                                    SharedString::from(format!("record-{id}")),
                                    if recording { "Press keys…" } else { "Record" },
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .debug_selector(move || format!("record-{id}"))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.recording = Some(id.into());
                                        this.modal_focus.focus(window, cx);
                                        this.status =
                                            "Press the new shortcut; Escape cancels recording"
                                                .into();
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                button(
                                    SharedString::from(format!("clear-shortcut-{id}")),
                                    "Clear",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .debug_selector(move || format!("clear-shortcut-{id}"))
                                .disabled(entry.chord.is_empty())
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        match this.shortcut_draft.clear(id) {
                                            Ok(()) => {
                                                this.status =
                                                    "Shortcut cleared. Apply to save.".into()
                                            }
                                            Err(error) => this.status = error.to_string(),
                                        }
                                        this.recording = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                button(
                                    SharedString::from(format!("default-shortcut-{id}")),
                                    "Default",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .debug_selector(move || format!("default-shortcut-{id}"))
                                .disabled(!self.shortcut_draft.overrides.contains_key(id))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        match this.shortcut_draft.reset(id) {
                                            Ok(()) => {
                                                this.status =
                                                    "Default restored. Apply to save.".into()
                                            }
                                            Err(error) => this.status = error.to_string(),
                                        }
                                        this.recording = None;
                                        cx.notify();
                                    },
                                )),
                            ),
                    );
                }
                body = body.child(
                    button(
                        "reset-shortcuts",
                        "Restore defaults",
                        ButtonVariant::Outline,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.shortcut_draft = Shortcuts::default();
                        this.recording = None;
                        this.status = "Defaults restored in this draft. Apply to save.".into();
                        cx.notify();
                    })),
                );
            } else if self.dialog == Dialog::Filter {
                let names = [
                    "Exposure",
                    "Levels",
                    "Curves",
                    "HSL",
                    "Color balance",
                    "Blur",
                    "Sharpen",
                    "Noise",
                    "Vignette",
                    "Bloom",
                    "Tonal contrast",
                ];
                let mut choices = div().flex().flex_wrap().gap_1();
                for (i, name) in names.iter().enumerate() {
                    choices = choices.child(
                        button(
                            SharedString::from(format!("filter-{i}")),
                            *name,
                            if i == self.filter_kind {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.open_filter(i, window, cx)),
                        ),
                    );
                }
                body = body.child(choices);
                let (_, labels, _) = filter_spec(self.filter_kind);
                for (i, label) in labels.iter().enumerate() {
                    if !label.is_empty() {
                        body = body.child(div().child(*label).child(input(
                            SharedString::from(format!("filter-value-{i}")),
                            &self.filter_inputs[i],
                            window,
                            cx,
                        )));
                    }
                }
                body = body.child(
                    button(
                        "filter-mode",
                        if self.live_filter {
                            "Mode: live adjustment layer"
                        } else {
                            "Mode: edit layer pixels"
                        },
                        ButtonVariant::Outline,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.live_filter = !this.live_filter;
                        this.editing_object = None;
                        cx.notify();
                    })),
                );
                body = body.child(if self.live_filter {
                    "A live layer changes the layers below it; source pixels remain editable."
                } else {
                    "Applies to the active pixel layer and respects the selection."
                });
            } else if self.dialog == Dialog::Text {
                body = body.child(input("text-content", &self.path_input, window, cx));
                for (i, label) in [
                    "Font size",
                    "Font family",
                    "Tracking",
                    "Leading (0 = automatic)",
                    "Alignment: Left, Center or Right",
                    "Text box width (0 = automatic)",
                    "Text box height (0 = automatic)",
                ]
                .iter()
                .enumerate()
                {
                    body = body.child(div().child(*label).child(input(
                        SharedString::from(format!("text-detail-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )));
                }
                body = body.child("Search installed fonts").child(input(
                    "font-search",
                    &self.detail_inputs[9],
                    window,
                    cx,
                ));
                let query = self.detail_inputs[9].read(cx).value().to_lowercase();
                let mut fonts = div()
                    .id("installed-fonts")
                    .debug_selector(|| "installed-fonts".into())
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1();
                for font in self
                    .font_names
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter(|font| query.is_empty() || font.to_lowercase().contains(&query))
                    .take(64)
                    .cloned()
                {
                    let label = font.clone();
                    fonts = fonts.child(
                        button(
                            SharedString::from(format!("font-{font}")),
                            label,
                            ButtonVariant::Outline,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.detail_inputs[1].update(cx, |input, cx| {
                                    input.set_value(font.clone(), window, cx)
                                });
                            },
                        )),
                    );
                }
                body = body.child(fonts);
                body = body.child(color_picker(
                    "text-color",
                    &self.dialog_color_picker,
                    window,
                    cx,
                ));
            } else if self.dialog == Dialog::Shape {
                for (i, label) in ["Width", "Height", "Corner radius", "Line width"]
                    .iter()
                    .enumerate()
                {
                    body = body.child(div().child(*label).child(input(
                        SharedString::from(format!("shape-detail-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    )));
                }
                body = body.child(color_picker(
                    "shape-color",
                    &self.dialog_color_picker,
                    window,
                    cx,
                ));
            } else if self.dialog == Dialog::Trim {
                use omuse::editor::TrimBasedOn;
                for (index, label, based) in [
                    (0, "Transparent pixels", TrimBasedOn::Transparent),
                    (1, "Top-left color", TrimBasedOn::TopLeft),
                    (2, "Bottom-right color", TrimBasedOn::BottomRight),
                ] {
                    body = body.child(
                        button(
                            SharedString::from(format!("trim-based-{index}")),
                            label,
                            if self.trim_options.based_on == based {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.trim_options.based_on = based;
                            cx.notify();
                        })),
                    );
                }
                let mut sides = div().flex().gap_1();
                for (index, label, selected) in [
                    (0, "Top", self.trim_options.top),
                    (1, "Bottom", self.trim_options.bottom),
                    (2, "Left", self.trim_options.left),
                    (3, "Right", self.trim_options.right),
                ] {
                    sides = sides.child(
                        button(
                            SharedString::from(format!("trim-side-{index}")),
                            label,
                            if selected {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let v = match index {
                                0 => &mut this.trim_options.top,
                                1 => &mut this.trim_options.bottom,
                                2 => &mut this.trim_options.left,
                                _ => &mut this.trim_options.right,
                            };
                            *v = !*v;
                            cx.notify();
                        })),
                    );
                }
                body = body.child("Trim away").child(sides);
            } else if matches!(
                self.dialog,
                Dialog::New | Dialog::Resize | Dialog::ResizeImage
            ) {
                body = body.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(div().flex_1().child("Width").child(input(
                            "width",
                            &self.width_input,
                            window,
                            cx,
                        )))
                        .child(div().flex_1().child("Height").child(input(
                            "height",
                            &self.height_input,
                            window,
                            cx,
                        ))),
                );
                if self.dialog == Dialog::Resize {
                    body = body.child(
                        button(
                            "canvas-lock-aspect",
                            if self.canvas_lock_aspect {
                                "Original aspect: locked"
                            } else {
                                "Original aspect: unlocked"
                            },
                            if self.canvas_lock_aspect {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.canvas_lock_aspect = !this.canvas_lock_aspect;
                            if this.canvas_lock_aspect {
                                this.sync_canvas_aspect(true, window, cx);
                            } else {
                                cx.notify();
                            }
                        })),
                    );
                    let mut units = div().flex().gap_1();
                    for (index, label) in ["Pixels", "Percent", "Inches", "Centimeters"]
                        .into_iter()
                        .enumerate()
                    {
                        units = units.child(
                            button(
                                SharedString::from(format!("canvas-unit-{index}")),
                                label,
                                if self.canvas_units == index {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Outline
                                },
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.change_canvas_units(
                                        index,
                                        this.canvas_relative,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        );
                    }
                    body = body.child("Units").child(units).child(
                        button(
                            "canvas-relative",
                            if self.canvas_relative {
                                "Relative: on"
                            } else {
                                "Relative: off"
                            },
                            ButtonVariant::Outline,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.change_canvas_units(
                                this.canvas_units,
                                !this.canvas_relative,
                                window,
                                cx,
                            );
                        })),
                    );
                    body = body.child("Anchor — artwork is moved, never scaled");
                    for row in 0..3u8 {
                        let mut anchors = div().flex().gap_1();
                        for column in 0..3u8 {
                            let index = row * 3 + column;
                            anchors = anchors.child(
                                button(
                                    SharedString::from(format!("canvas-anchor-{index}")),
                                    [
                                        "Top left",
                                        "Top center",
                                        "Top right",
                                        "Middle left",
                                        "Center",
                                        "Middle right",
                                        "Bottom left",
                                        "Bottom center",
                                        "Bottom right",
                                    ][index as usize],
                                    if self.canvas_anchor == index {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Outline
                                    },
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.canvas_anchor = index;
                                        cx.notify();
                                    },
                                )),
                            );
                        }
                        body = body.child(anchors);
                    }
                    body = body.child("Canvas extension");
                    let mut fills = div().flex().flex_wrap().gap_1();
                    for (index, label) in [
                        "Transparent",
                        "Foreground",
                        "Background",
                        "Black",
                        "White",
                        "Custom",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        fills = fills.child(
                            button(
                                SharedString::from(format!("canvas-fill-{index}")),
                                label,
                                if self.canvas_fill == index {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Outline
                                },
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.canvas_fill = index;
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    body = body.child(fills);
                    if self.canvas_fill == 5 {
                        body = body.child(color_picker(
                            "canvas-custom-color",
                            &self.dialog_color_picker,
                            window,
                            cx,
                        ));
                    }
                }
                if self.dialog == Dialog::ResizeImage {
                    body = body.child("Resolution (1–9600 DPI)").child(input(
                        "resize-resolution",
                        &self.detail_inputs[0],
                        window,
                        cx,
                    ));
                    let mut choices = div().flex().gap_1();
                    for (index, label) in ["Nearest", "Smooth", "High quality"]
                        .into_iter()
                        .enumerate()
                    {
                        choices = choices.child(
                            button(
                                SharedString::from(format!("resize-sampling-{index}")),
                                label,
                                if self.resize_sampling == index {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Outline
                                },
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.resize_sampling = index;
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    body = body.child("Sampling").child(choices);
                }
            } else {
                let export_path = PathBuf::from(self.path_input.read(cx).value().as_ref());
                let is_jpeg = self.dialog == Dialog::Export
                    && export_path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            extension.eq_ignore_ascii_case("jpg")
                                || extension.eq_ignore_ascii_case("jpeg")
                        });
                if is_jpeg {
                    body = body.gap_2().child(self.jpeg_export_form(window, cx));
                } else {
                    body = body.child(input("path", &self.path_input, window, cx));
                    if self.dialog == Dialog::Export {
                        for (i, label) in [(4usize, "Resolution (DPI, 1–9600)")] {
                            body = body.child(div().child(label).child(input(
                                SharedString::from(format!("export-option-{i}")),
                                &self.detail_inputs[i],
                                window,
                                cx,
                            )));
                        }
                    }
                    if self.dialog != Dialog::Rename {
                        body = body.child(
                            button("browse", "Browse…", ButtonVariant::Outline, cx)
                                .disabled(self.photo_io.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.native_browse(window, cx)
                                })),
                        );
                        body=body.child(div().text_sm().text_color(t.secondary).child(match self.dialog{Dialog::Export=>"PNG · JPEG · WebP · TIFF · PSD. Choose with the extension. PSD exports converted 8-bit pixel layers; keep .omuse for editable text, vectors and full precision.",Dialog::Save=>"Omuse project folder (.omuse). Layers and collection pages remain editable. The extension is added automatically.",Dialog::Open=>"Choose an .omuse project, an image, or an older .comp project.",_=>"The image will be added as a new layer."}));
                    }
                }
            }
            footer = Some(
                div()
                    .debug_selector(|| "dialog-footer".into())
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.warning)
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                button(
                                    "cancel-dialog",
                                    if self.dialog == Dialog::Save && self.create.saving {
                                        "Saving…"
                                    } else {
                                        "Cancel"
                                    },
                                    ButtonVariant::Outline,
                                    cx,
                                )
                                .debug_selector(|| "cancel-dialog".into())
                                .disabled(self.dialog == Dialog::Save && self.create.saving)
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        if this.dialog == Dialog::Save
                                            && this.saving_blocks_navigation(cx)
                                        {
                                            return;
                                        }
                                        this.cancel_range(cx);
                                        if !matches!(
                                            this.dialog,
                                            Dialog::CommandSearch | Dialog::Shortcuts
                                        ) {
                                            this.clear_vector(cx);
                                        }
                                        this.clear_workflow(cx);
                                        this.clear_pro(cx);
                                        this.cancel_finishing(cx);
                                        this.cancel_camera_raw();
                                        this.svg_import_draft = None;
                                        if !this.cancel_photo_io() {
                                            this.refresh(cx);
                                        }
                                        this.dialog = Dialog::None;
                                        this.pending = None;
                                        this.dialog_generation += 1;
                                        this.busy = false;
                                        this.subject_mask = None;
                                        this.subject_guide = None;
                                        this.subject_layer = None;
                                        this.gradient_pending = None;
                                        this.text_origin = None;
                                        this.focus.focus(window, cx);
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                button(
                                    "confirm-dialog",
                                    match self.dialog {
                                        Dialog::Open => "Open",
                                        Dialog::Import => "Import",
                                        Dialog::Save if self.confirming_project_replacement(cx) => {
                                            "Replace project"
                                        }
                                        Dialog::Save => "Save",
                                        Dialog::Export => "Export",
                                        Dialog::New => "Create",
                                        _ => "Apply",
                                    },
                                    ButtonVariant::Primary,
                                    cx,
                                )
                                .debug_selector(|| "confirm-dialog".into())
                                .disabled(self.dialog == Dialog::CameraRaw && self.busy)
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.confirm_dialog(window, cx),
                                )),
                            ),
                    ),
            );
        }
        let advanced_workspace = matches!(
            self.dialog,
            Dialog::Pro
                | Dialog::Finishing
                | Dialog::Workflow
                | Dialog::VectorPath
                | Dialog::Shortcuts
        );
        let dialog_width = if advanced_workspace {
            (f32::from(window.viewport_size().width) - 32.).clamp(560., 960.)
        } else {
            560.
        };
        let dialog_height =
            if advanced_workspace || matches!(self.dialog, Dialog::CameraRaw | Dialog::Export) {
                (f32::from(window.viewport_size().height) - 48.).clamp(480., 820.)
            } else {
                540.
            };
        let mut dialog = div()
            .id("dialog")
            .debug_selector(|| "dialog".into())
            .w(px(dialog_width))
            .max_h(px(dialog_height))
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .bg(t.surface)
            .border_1()
            .rounded(px(6.))
            .border_color(t.control_border())
            .child(body);
        if let Some(footer) = footer {
            dialog = dialog.child(footer);
        }
        div()
            .id("modal-shield")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .child(dialog.focus_trap("modal-focus", &self.modal_focus))
            .into_any_element()
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.validate_image_trace(cx);
        self.validate_vector_canvas(cx);
        self.validate_numeric_context(window, cx);
        self.validate_camera_gesture_context(window, cx);
        if let Some(error) = self.editor.take_mask_paint_error() {
            self.refresh(cx);
            self.status = format!("Mask edit left your work unchanged: {error}");
        }
        self.sync_create_fields(window, cx);
        self.sync_motion_fields(window, cx);
        if let Some(error) = self.recovery.error() {
            self.status = format!("Recovery unavailable: {error}");
        }
        let t = cx.omarchy().clone();
        let title = format!(
            "{}{} — Omuse",
            if self.has_unsaved_work() { "● " } else { "" },
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into())
        );
        window.set_window_title(&title);
        let header = self.studio_header(window, cx);
        let context_bar = self.studio_context(window, cx);
        let tools = self.studio_tools(cx);
        let inspector = self.studio_inspector(window, cx);
        let footer = self.studio_footer(cx);
        let input_modal =
            self.dialog != Dialog::None || self.inline_text.is_some() || self.create.phone_preview;
        let modal = input_modal || self.busy;
        let mut root = focus_scope("editor-root")
            .track_focus(&self.focus)
            .on_tablet(cx.listener(Self::tablet))
            .on_tablet_out(cx.listener(Self::tablet))
            .on_mouse_move(cx.listener(Self::numeric_moved))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::numeric_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::numeric_up))
            .key_context(if input_modal { "OmuseInput" } else { "Omuse" })
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(t.background)
            .text_color(t.foreground)
            .font_family(t.font)
            .text_size(px(13.))
            .on_action(cx.listener(move |this, _: &New, w, cx| {
                if !modal {
                    this.command("new", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Open, w, cx| {
                if !modal {
                    this.command("open", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Save, w, cx| {
                if !modal {
                    this.command("save", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &SaveAs, w, cx| {
                if !modal {
                    this.command("save-as", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Export, w, cx| {
                if !modal {
                    this.command("export", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Import, w, cx| {
                if !modal {
                    this.command("import", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Undo, w, cx| {
                if !modal {
                    this.command("undo", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Redo, w, cx| {
                if !modal {
                    this.command("redo", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Deselect, w, cx| {
                if !modal {
                    this.command("deselect", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &SelectAll, w, cx| {
                if !modal {
                    this.command("select-all", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &ZoomIn, w, cx| {
                if !modal {
                    this.command("zoom-in", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &ZoomOut, w, cx| {
                if !modal {
                    this.command("zoom-out", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Fit, w, cx| {
                if !modal {
                    this.command("fit", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Copy, w, cx| {
                if !modal {
                    this.command("copy", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Cut, w, cx| {
                if !modal {
                    this.command("cut", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Paste, w, cx| {
                if !modal {
                    this.command("paste", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, _: &Quit, w, cx| {
                if !modal {
                    this.command("quit", w, cx);
                }
            }))
            .on_action(cx.listener(move |this, action: &Command, w, cx| {
                if !modal
                    && (this.focus.is_focused(w)
                        || matches!(action.name.as_str(), "command-search" | "quit"))
                {
                    this.command(&action.name, w, cx);
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && this.cancel_camera_gesture(window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "escape" && this.cancel_numeric_scrub(window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if this.create.phone_preview {
                    if event.keystroke.key == "escape" {
                        this.create.phone_preview = false;
                        this.focus.focus(window, cx);
                        cx.notify();
                    }
                    return;
                }
                if this.dialog != Dialog::None {
                    if event.keystroke.key == "escape" {
                        if this.dialog == Dialog::Save && this.saving_blocks_navigation(cx) {
                            return;
                        }
                        this.cancel_photo_io();
                        if this.dialog == Dialog::ExternalChange {
                            this.dismiss_external_notice();
                        }
                        let preview_dialog = matches!(
                            this.dialog,
                            Dialog::CameraRaw | Dialog::SubjectRefine | Dialog::Gradient
                        );
                        let camera_preview = this.dialog == Dialog::CameraRaw;
                        let gradient_preview = this.dialog == Dialog::Gradient;
                        let keep_vector =
                            matches!(this.dialog, Dialog::CommandSearch | Dialog::Shortcuts);
                        this.dialog = Dialog::None;
                        this.pending = None;
                        this.dialog_generation += 1;
                        this.cancel_range(cx);
                        if !keep_vector {
                            this.clear_vector(cx);
                        }
                        this.clear_workflow(cx);
                        this.clear_pro(cx);
                        this.cancel_finishing(cx);
                        this.cancel_camera_raw();
                        this.svg_import_draft = None;
                        if preview_dialog {
                            this.busy = false;
                            this.status = if camera_preview {
                                "Camera Raw preview cancelled"
                            } else if gradient_preview {
                                "Gradient preview cancelled"
                            } else {
                                "Subject matte cancelled"
                            }
                            .into();
                            this.refresh(cx);
                            this.subject_mask = None;
                            this.subject_guide = None;
                            this.subject_layer = None;
                            this.gradient_pending = None;
                            this.text_origin = None;
                        }
                        this.focus.focus(window, cx);
                        cx.notify();
                    }
                    return;
                }
                if !this.focus.is_focused(window)
                    || event.keystroke.modifiers.control
                    || event.keystroke.modifiers.alt
                    || event.keystroke.modifiers.platform
                {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "space" => {
                        this.space_down = true;
                    }
                    "escape" => {
                        if this.mask_inspection.active() {
                            this.mask_inspection.exit();
                            this.status = "Mask inspection disabled".into();
                            cx.notify();
                            return;
                        }
                        if this.image_trace_active() {
                            this.cancel_image_trace(window, cx);
                            return;
                        }
                        if this.vector_scene_active() {
                            this.cancel_vector_canvas(window, cx);
                            return;
                        }
                        if this.crop.is_some() {
                            this.cancel_crop(window, cx);
                            return;
                        }
                        if this.cancel_photo_io() {
                            this.dialog_generation += 1;
                            cx.notify();
                            return;
                        }
                        if this.busy {
                            this.dialog_generation += 1;
                            this.busy = false;
                            this.status = "Operation cancelled".into();
                        }
                        this.editor.cancel_stroke();
                        this.editor.cancel_clone_stroke();
                        let cancelled_drag = this.transform_drag.is_some();
                        this.drag_start = None;
                        this.transform_drag = None;
                        this.transform_original_box = None;
                        this.transform_draft = None;
                        this.distort_draft = None;
                        this.guide_drag = None;
                        if !cancelled_drag && this.editor.cancel_floating_selection() {
                            this.status = "Floating selection cancelled".into();
                            this.changed(cx);
                        } else {
                            this.refresh(cx);
                        }
                    }
                    "enter" => {
                        if this.image_trace_active() {
                            this.keep_image_trace(window, cx);
                            return;
                        }
                        if this.vector_scene_active() {
                            this.apply_vector(cx);
                            return;
                        }
                        if this.crop.is_some() {
                            this.apply_crop(window, cx);
                            return;
                        }
                        if this.editor.floating_selection_layer().is_some() {
                            match this.editor.commit_floating_selection() {
                                Ok(true) => {
                                    this.status = "Floating selection committed".into();
                                    this.changed(cx);
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    this.status = format!("Commit selection: {e:#}");
                                    cx.notify();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui_kit::KeyUpEvent, _, _| {
                if event.keystroke.key == "space" {
                    this.space_down = false;
                }
            }))
            .child(header)
            .child(context_bar)
            .child(self.external_banner(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(tools)
                    .child(self.canvas_view(cx))
                    .when(self.inspector_visible, |body| body.child(inspector)),
            )
            .child(self.create_strip(cx))
            .child(footer);
        if self.create.phone_preview {
            root = root.child(self.phone_preview_view(cx));
        }
        if self.inline_text.is_some() {
            root = root.child(self.inline_text_view(window, cx));
        }
        if self.dialog != Dialog::None {
            root = root.child(self.dialog_view(window, cx));
        }
        root
    }
}

fn editable_text(layers: &[Layer], id: &str, visible: bool, locked: bool) -> bool {
    layers.iter().any(|layer| {
        let visible = visible && layer.visible;
        let locked = locked || layer.locked;
        if layer.id == id {
            visible && !locked && objects::live_text(layer).ok().flatten().is_some()
        } else {
            editable_text(&layer.children, id, visible, locked)
        }
    })
}

fn text_at(editor: &Editor, x: f32, y: f32) -> Option<String> {
    fn find(editor: &Editor, layers: &[Layer], x: f32, y: f32) -> Option<String> {
        for layer in layers.iter().rev().filter(|l| l.visible) {
            if let Some(id) = find(editor, &layer.children, x, y) {
                return Some(id);
            }
            if objects::live_text(layer).ok().flatten().is_some()
                && editor
                    .layer_placement(&layer.id)
                    .is_some_and(|p| p.contains(x, y))
            {
                return Some(layer.id.clone());
            }
        }
        None
    }
    find(editor, &editor.document.layers, x, y)
}

#[cfg(test)]
fn selection_contour_points(selection: &Selection, limit: usize) -> Vec<(u32, u32)> {
    omuse::selection_outline::generate(
        selection,
        1,
        omuse::model::PixelRect {
            x: 0,
            y: 0,
            width: selection.width,
            height: selection.height,
        },
        limit,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map(|outline| outline.points)
    .unwrap_or_default()
}

fn snap_canvas_point(
    editor: &Editor,
    point: CanvasPoint,
    grid: bool,
    guides: bool,
    tolerance: f32,
    settings: omuse::canvas_grid::GridSettings,
    bypass: bool,
) -> CanvasPoint {
    fn nearest(value: f32, candidates: impl Iterator<Item = f32>, tolerance: f32) -> f32 {
        candidates
            .filter_map(|candidate| {
                let distance = (candidate - value).abs();
                (distance <= tolerance).then_some((distance, candidate))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, candidate)| candidate)
            .unwrap_or(value)
    }
    let grid_step = if grid && !bypass {
        Some(settings.minor_spacing())
    } else {
        None
    };
    let vertical = editor
        .guides()
        .into_iter()
        .filter(|guide| guides && guide.axis == omuse::editor::GuideAxis::Vertical)
        .map(|guide| guide.position)
        .chain(grid_step.map(|step| (point.x / step).round() * step));
    let horizontal = editor
        .guides()
        .into_iter()
        .filter(|guide| guides && guide.axis == omuse::editor::GuideAxis::Horizontal)
        .map(|guide| guide.position)
        .chain(grid_step.map(|step| (point.y / step).round() * step));
    CanvasPoint {
        x: if guides || grid {
            nearest(point.x, vertical, tolerance)
        } else {
            point.x
        },
        y: if guides || grid {
            nearest(point.y, horizontal, tolerance)
        } else {
            point.y
        },
    }
}

fn hit_layer_at(editor: &Editor, x: f32, y: f32) -> Option<String> {
    fn visit(editor: &Editor, layers: &[Layer], x: f32, y: f32) -> Option<String> {
        for layer in layers.iter().rev() {
            if !layer.visible || layer.opacity <= 0. {
                continue;
            }
            if layer.is_group() {
                if let Some(id) = visit(editor, &layer.children, x, y) {
                    return Some(id);
                }
            } else if layer.image.is_some()
                && editor
                    .layer_placement(&layer.id)
                    .is_some_and(|placement| placement.contains(x, y))
            {
                return Some(layer.id.clone());
            }
        }
        None
    }
    visit(editor, &editor.document.layers, x, y)
}

fn display_tile_bounds(
    tile: &DisplayTile,
    origin: Point<Pixels>,
    pixel_size: Size<Pixels>,
) -> (Bounds<Pixels>, Bounds<Pixels>) {
    let edge = |x: f32, y: f32| {
        point(
            origin.x + pixel_size.width * x,
            origin.y + pixel_size.height * y,
        )
    };
    let x = tile.rect.x as f32;
    let y = tile.rect.y as f32;
    let right = (tile.rect.x + tile.rect.width) as f32;
    let bottom = (tile.rect.y + tile.rect.height) as f32;
    // Shared edges derive from the same canvas origin and source coordinate;
    // accumulating rounded tile widths creates cracks at fractional zoom.
    (
        Bounds::from_corners(edge(x, y), edge(right, bottom)),
        Bounds::from_corners(edge(x - 1., y - 1.), edge(right + 1., bottom + 1.)),
    )
}

fn paint_display_tiles(
    tiles: &[DisplayTile],
    reference: Option<&Arc<RenderImage>>,
    textures: &RefCell<CanvasTextures>,
    canvas_bounds: Bounds<Pixels>,
    source_dimensions: (u32, u32),
    viewport: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if source_dimensions.0 == 0 || source_dimensions.1 == 0 {
        textures.borrow_mut().retain_visible(Vec::new(), window, cx);
        return;
    }
    // Match the complete image's device-pixel endpoints before subdividing.
    // Otherwise full-image snapping changes its effective scale while each
    // tile keeps the nominal zoom, causing a drifting minification phase.
    let scale = window.scale_factor();
    let snap = |value: Pixels| px((f32::from(value) * scale).round() / scale);
    let canvas_bounds = Bounds::from_corners(
        point(snap(canvas_bounds.left()), snap(canvas_bounds.top())),
        point(snap(canvas_bounds.right()), snap(canvas_bounds.bottom())),
    );
    let pixel_size = size(
        canvas_bounds.size.width / source_dimensions.0 as f32,
        canvas_bounds.size.height / source_dimensions.1 as f32,
    );
    let visible: Vec<_> = tiles
        .iter()
        .filter_map(|tile| {
            let (core, halo) = display_tile_bounds(tile, canvas_bounds.origin, pixel_size);
            let clipped = core.intersect(&viewport);
            (clipped.size.width > px(0.) && clipped.size.height > px(0.))
                .then_some((tile, core, halo))
        })
        .collect();
    let images = if let Some(reference) = reference {
        vec![reference.clone()]
    } else {
        visible
            .iter()
            .map(|(tile, _, _)| tile.image.clone())
            .collect()
    };
    textures.borrow_mut().retain_visible(images, window, cx);
    window.with_content_mask(Some(gpui_kit::ContentMask { bounds: viewport }), |window| {
        if let Some(reference) = reference {
            // The independent monolithic oracle has the same clamped outer
            // border as the tiled surface. Bare atlas images have no padding
            // and can sample an unrelated allocation at their outer edges.
            let halo = Bounds::from_corners(
                canvas_bounds.origin - point(pixel_size.width, pixel_size.height),
                canvas_bounds.bottom_right() + point(pixel_size.width, pixel_size.height),
            );
            let _ = window.paint_image(
                canvas_bounds,
                halo,
                Corners::default(),
                reference.clone(),
                0,
                false,
            );
        } else {
            for (tile, core, halo) in visible {
                // Clip with the content mask, preserving the complete core UVs
                // instead of rounding a partial viewport crop to source pixels.
                let _ = window.paint_image(
                    core,
                    halo,
                    Corners::default(),
                    tile.image.clone(),
                    0,
                    false,
                );
            }
        }
    });
}

fn render_image(pixels: &image::RgbaImage) -> Arc<RenderImage> {
    let mut bgra = pixels.clone();
    for p in bgra.pixels_mut() {
        p.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]))
}
fn mask_value(color: [u8; 4]) -> u8 {
    ((u32::from(color[0]) * 54 + u32::from(color[1]) * 183 + u32::from(color[2]) * 19 + 128) / 256)
        as u8
}
fn layer_position(
    layers: &[Layer],
    id: &str,
    parent: Option<&str>,
) -> Option<(Option<String>, usize, usize)> {
    for (index, layer) in layers.iter().enumerate() {
        if layer.id == id {
            return Some((parent.map(str::to_owned), index, layers.len()));
        }
        if let Some(found) = layer_position(&layer.children, id, Some(&layer.id)) {
            return Some(found);
        }
    }
    None
}

pub fn self_test(output: Option<PathBuf>) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    let dir = output
        .unwrap_or_else(|| std::env::temp_dir().join(format!("omuse-test-{}", std::process::id())));
    std::fs::create_dir_all(&dir)?;
    let mut editor = Editor::new(Document::new(128, 96));
    editor.brush.color = [230, 42, 80, 255];
    editor.brush.size = 12.;
    ensure!(
        editor.begin_stroke(15., 20., 1., PaintTool::Brush),
        "begin stroke"
    );
    editor.continue_stroke(100., 70., 1.);
    editor.finish_stroke();
    ensure!(editor.is_dirty(), "stroke must dirty document");
    let painted = raster::composite(&editor.document);
    ensure!(painted.pixels().any(|p| p[3] > 0), "stroke visible");
    ensure!(editor.undo(), "undo stroke");
    ensure!(
        raster::composite(&editor.document)
            .pixels()
            .all(|p| p[3] == 0),
        "undo clears stroke"
    );
    ensure!(editor.redo(), "redo stroke");
    ensure!(
        raster::composite(&editor.document) == painted,
        "redo pixel match"
    );
    let project = dir.join("Journey.omuse");
    document::save(&editor.document, &project).context("save project")?;
    editor.mark_saved();
    ensure!(!editor.is_dirty(), "saved document clean");
    let reopened = document::open(&project).context("open project")?;
    ensure!(
        raster::composite(&reopened) == painted,
        "project roundtrip pixels"
    );
    for ext in ["png", "jpg", "webp", "tiff"] {
        let path = dir.join(format!("Journey.{ext}"));
        raster::export(&reopened, &path)?;
        let image = image::open(&path)?;
        ensure!(
            image.width() == 128 && image.height() == 96,
            "export dimensions"
        );
    }
    editor.add_layer("Gradient");
    editor.gradient(
        (0., 0.),
        (128., 96.),
        [0, 0, 255, 100],
        [255, 255, 255, 100],
    );
    editor.adjust(Adjustment::Grayscale);
    ensure!(editor.undo(), "adjustment undo");
    std::fs::write(
        dir.join("results.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","journeys":["paint","undo","redo","save","reopen","PNG","JPEG","WebP","TIFF","gradient","adjustment"],"width":128,"height":96}),
        )?,
    )?;
    println!("Rust editing journey passed: {}", dir.display());
    Ok(())
}

#[cfg(all(test, feature = "ui-test"))]
#[path = "region_history_ui_tests.rs"]
mod region_history_ui_tests;

#[cfg(all(test, feature = "ui-test"))]
#[path = "desktop_history_tests.rs"]
mod desktop_history_tests;

#[cfg(all(test, feature = "ui-test"))]
mod interaction_tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext};

    #[gpui_kit::test]
    fn painting_replaces_only_damaged_display_tiles_and_retires_gpu_images(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(768, 512));
            view.editor.brush.size = 4.;
            view.dialog = Dialog::None;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(850.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let before = cx.update(|window, cx| {
            let tiles = view.read(cx).display.snapshot();
            assert_eq!(tiles.len(), 6);
            assert!(
                tiles
                    .iter()
                    .all(|tile| window.has_image_atlas_entry(&tile.image))
            );
            tiles
        });
        view.update_in(cx, |view, window, cx| {
            view.editor.begin_stroke(256., 100., 1., PaintTool::Pencil);
            view.queue_stroke_frame(window, cx);
        });
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let after = cx.update(|window, cx| {
            let tiles = view.read(cx).display.snapshot();
            for (index, (old, new)) in before.iter().zip(tiles.iter()).enumerate() {
                assert_eq!(old.image.id == new.image.id, index >= 2);
                assert!(window.has_image_atlas_entry(&new.image));
                if index < 2 {
                    assert!(!window.has_image_atlas_entry(&old.image));
                }
            }
            tiles
        });
        view.update(cx, |view, cx| {
            assert!(view.editor.finish_stroke());
            view.changed(cx);
            assert!(Arc::ptr_eq(&view.display.snapshot(), &after));
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        view.update(cx, |view, cx| {
            view.pan = (10000., 10000.);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(
                after
                    .iter()
                    .all(|tile| !window.has_image_atlas_entry(&tile.image))
            );
            assert!(view.read(cx).canvas_textures.borrow().images.is_empty());
        });
    }

    #[gpui_kit::test]
    fn document_resize_and_unpainted_previews_do_not_retain_obsolete_textures(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| EditorView::new(None, window, cx));
        cx.simulate_resize(size(px(1200.), px(850.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let old = cx.update(|_, cx| view.read(cx).display.snapshot());
        view.update(cx, |view, cx| {
            let preview = image::RgbaImage::from_pixel(1024, 768, image::Rgba([1, 2, 3, 255]));
            view.display.replace(&preview);
            let intermediate = Arc::downgrade(&view.display.snapshot()[0].image);
            view.editor = Editor::new(Document::new(257, 129));
            view.refresh(cx);
            assert!(intermediate.upgrade().is_none());
            assert_eq!(view.display.dimensions(), (257, 129));
            assert_eq!(view.display.snapshot().len(), 1);
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(
                old.iter()
                    .all(|tile| !window.has_image_atlas_entry(&tile.image))
            );
            assert!(
                view.read(cx)
                    .display
                    .snapshot()
                    .iter()
                    .all(|tile| window.has_image_atlas_entry(&tile.image))
            );
        });
    }

    #[gpui_kit::test]
    fn stroke_frames_coalesce_and_match_complete_composite(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(48, 32));
            view.editor.document.layers[0].offset_x = -4.;
            view.editor.document.layers[0].offset_y = 3.;
            view.editor.brush.size = 2.;
            view.refresh(cx);
            view
        });
        let before = cx.update(|_, cx| view.read(cx).display.snapshot());
        view.update_in(cx, |view, window, cx| {
            assert!(view.editor.begin_stroke(4., 8., 1., PaintTool::Pencil));
            view.queue_stroke_frame(window, cx);
            let ticket = view.pending_stroke_frame;
            for x in 5..36 {
                view.editor.continue_stroke(x as f32, 8., 1.);
                view.queue_stroke_frame(window, cx);
                assert_eq!(view.pending_stroke_frame, ticket);
                assert!(Arc::ptr_eq(&view.display.snapshot(), &before));
            }
        });
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        view.update(cx, |view, _| {
            assert!(view.pending_stroke_frame.is_none());
            assert!(!Arc::ptr_eq(&view.display.snapshot(), &before));
            assert_eq!(view.pixels, raster::composite(&view.editor.document));
            assert_eq!(view.editor.take_stroke_damage(), None);
            assert!(view.pixels.pixels().any(|p| p[3] > 0));
        });
    }

    #[gpui_kit::test]
    fn cancelled_and_replaced_strokes_cannot_publish_stale_frames(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| EditorView::new(None, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.editor = Editor::new(Document::new(48, 32));
            view.refresh(cx);
            view.editor.begin_stroke(7., 8., 1., PaintTool::Brush);
            view.queue_stroke_frame(window, cx);
            let old_ticket = view.pending_stroke_frame;
            view.editor.cancel_stroke();
            view.refresh(cx);
            assert!(view.pixels.pixels().all(|p| p[3] == 0));
            // A new document and stroke can arrive before the old callback runs.
            view.editor = Editor::new(Document::new(24, 18));
            view.refresh(cx);
            view.editor.brush.color = [212, 31, 58, 255];
            view.editor.begin_stroke(14., 9., 1., PaintTool::Brush);
            view.queue_stroke_frame(window, cx);
            assert_ne!(view.pending_stroke_frame, old_ticket);
        });
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        view.update(cx, |view, _| {
            assert_eq!(view.pixels.dimensions(), (24, 18));
            assert_eq!(view.pixels, raster::composite(&view.editor.document));
            assert_eq!(view.pixels.get_pixel(14, 9)[0], 212);
            assert!(view.pending_stroke_frame.is_none());
        });
        let presented = cx.update(|_, cx| view.read(cx).display.snapshot());
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.update(|_, cx| assert!(Arc::ptr_eq(&view.read(cx).display.snapshot(), &presented)));
    }

    #[gpui_kit::test]
    fn stroke_frames_fall_back_for_masks_and_transforms(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| EditorView::new(None, window, cx));
        for mask in [false, true] {
            view.update_in(cx, |view, window, cx| {
                view.editor = Editor::new(Document::new(48, 32));
                view.editor.document.layers[0].image = Some(
                    image::RgbaImage::from_pixel(48, 32, image::Rgba([30, 80, 160, 255])).into(),
                );
                view.editor.brush.size = 8.;
                if mask {
                    let id = view.editor.active_layer.clone();
                    view.editor.add_mask(&id, true);
                    view.editor.brush.color = [0, 0, 0, 255];
                } else {
                    view.editor.document.layers[0].rotation = 17.;
                    view.editor.document.layers[0].offset_x = 0.5;
                }
                view.refresh(cx);
                if mask {
                    assert!(
                        view.editor
                            .begin_mask_stroke(20., 16., 1., PaintTool::Brush)
                    );
                } else {
                    assert!(view.editor.begin_stroke(20., 16., 1., PaintTool::Brush));
                }
                view.queue_stroke_frame(window, cx);
            });
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            view.update(cx, |view, _| {
                assert_eq!(view.pixels, raster::composite(&view.editor.document));
                assert!(view.pending_stroke_frame.is_none());
            });
        }
    }

    #[gpui_kit::test]
    fn unchanged_or_off_canvas_stamps_do_not_replace_display_image(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| EditorView::new(None, window, cx));
        for off_canvas in [false, true] {
            let before = view.update_in(cx, |view, window, cx| {
                view.editor = Editor::new(Document::new(48, 32));
                view.editor.brush.size = 1.;
                if off_canvas {
                    view.editor.document.layers[0].offset_x = -40.;
                } else {
                    view.editor.brush.opacity = 0.;
                }
                view.refresh(cx);
                let before = view.display.snapshot();
                let x = if off_canvas { -30.5 } else { 10.5 };
                view.editor.begin_stroke(x, 12.5, 1., PaintTool::Pencil);
                view.queue_stroke_frame(window, cx);
                before
            });
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            view.update(cx, |view, _| {
                assert!(Arc::ptr_eq(&view.display.snapshot(), &before));
                assert_eq!(view.pixels, raster::composite(&view.editor.document));
                assert!(view.pending_stroke_frame.is_none());
                assert_eq!(view.editor.finish_stroke(), off_canvas);
            });
        }
    }

    #[gpui_kit::test]
    fn stale_background_completions_cannot_release_a_newer_job(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery_dir = temp.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery_dir);
            view
        });

        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                for dialog in [
                    Dialog::CameraRaw,
                    Dialog::RawImport,
                    Dialog::None,
                    Dialog::SubjectRefine,
                ] {
                    view.dialog_generation = 42;
                    view.dialog = dialog;
                    view.busy = true;
                    view.status = "newer job is running".into();

                    assert!(!view.finish_background_job(41, dialog));
                    assert!(view.busy, "stale generation released {dialog:?}");
                    assert_eq!(view.status, "newer job is running");

                    assert!(!view.finish_background_job(42, Dialog::Export));
                    assert!(view.busy, "wrong dialog released {dialog:?}");

                    assert!(view.finish_background_job(42, dialog));
                    assert!(!view.busy, "current completion did not release {dialog:?}");
                }
            })
        });
    }

    #[gpui_kit::test]
    fn save_in_progress_keeps_modal_navigation_and_pending_action_intact(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::Save;
            view.create.saving = true;
            view.pending = Some(Pending::New);
            view
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("escape");
        view.update(cx, |view, _| {
            assert_eq!(view.dialog, Dialog::Save);
            assert!(matches!(view.pending, Some(Pending::New)));
            assert_eq!(view.status, "Saving… Wait for the current save to finish.");
        });

        view.update(cx, |view, cx| {
            view.dialog = Dialog::Unsaved;
            view.pending = Some(Pending::New);
            // This is the guard used by Discard and a programmatic Save click
            // if a disabled control is bypassed.
            assert!(view.saving_blocks_navigation(cx));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let discard = cx.debug_bounds("discard").expect("Discard button");
        let save = cx
            .debug_bounds("save-pending")
            .expect("Save pending button");
        cx.simulate_click(discard.center(), Modifiers::default());
        cx.simulate_click(save.center(), Modifiers::default());
        view.update(cx, |view, _| {
            assert_eq!(view.dialog, Dialog::Unsaved);
            assert!(matches!(view.pending, Some(Pending::New)));
            assert!(view.create.saving);
        });

        let cancel = cx
            .debug_bounds("cancel-unsaved")
            .expect("Keep editing button");
        cx.simulate_click(cancel.center(), Modifiers::default());
        view.update(cx, |view, _| {
            assert_eq!(view.dialog, Dialog::None);
            assert!(view.pending.is_none());
            assert!(view.create.saving);
        });
    }

    #[gpui_kit::test]
    fn actual_mask_and_effect_drag_do_not_turn_into_layer_moves(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery_dir = temp.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery_dir);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 10));
            let source = view.editor.active_layer.clone();
            view.editor.add_mask(&source, true);
            view.editor
                .set_layer_effects(&source, serde_json::json!({"stroke":{"size":2.}}))
                .unwrap();
            let target = view.editor.add_layer("Target");
            let spare = view.editor.add_layer("Spare");
            view.select_layer_ids(vec![source, spare]);
            view.editor.active_layer = target;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1600.), px(1400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (source, target, original, depth) = cx.update(|_, cx| {
            let app = view.read(cx);
            (
                app.editor.document.layers[0].id.clone(),
                app.editor.document.layers[1].id.clone(),
                app.editor
                    .document
                    .layers
                    .iter()
                    .map(|l| l.id.clone())
                    .collect::<Vec<_>>(),
                app.editor.undo_depth(),
            )
        });
        let mask_selector: &'static str =
            Box::leak(format!("mask-badge-{source}").into_boxed_str());
        let target_selector: &'static str = Box::leak(format!("layer-{target}").into_boxed_str());
        let effect_selector: &'static str =
            Box::leak(format!("effect-row-{source}-stroke").into_boxed_str());
        for (selector, is_mask) in [(mask_selector, true), (effect_selector, false)] {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let from = cx
                .debug_bounds(selector)
                .expect("drag source visible")
                .center();
            let to = cx
                .debug_bounds(target_selector)
                .expect("drop target visible")
                .center();
            cx.simulate_mouse_move(from, None, Modifiers::default());
            cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.simulate_mouse_move(
                from - point(px(10.), px(0.)),
                Some(MouseButton::Left),
                Modifiers::default(),
            );
            cx.update(|_, cx| assert!(cx.has_active_drag(), "real drag must start"));
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
            cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
            cx.update(|_, cx| {
                let app = view.read(cx);
                assert!(!cx.has_active_drag());
                assert_eq!(
                    app.editor
                        .document
                        .layers
                        .iter()
                        .map(|l| l.id.clone())
                        .collect::<Vec<_>>(),
                    original,
                    "copying a badge must not reorder layers"
                );
                let target = app.editor.document.find_layer(&target).unwrap();
                if is_mask {
                    assert!(target.mask.is_some());
                } else {
                    assert!(target.metadata["effects"]["stroke"].is_object());
                }
                assert!(
                    app.editor
                        .document
                        .find_layer(&source)
                        .unwrap()
                        .mask
                        .is_some()
                );
            });
        }
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.undo_depth(), depth + 2));
    }

    #[gpui_kit::test]
    fn actual_selected_layer_drag_keeps_the_batch_after_mouse_down(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery_dir = temp.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery_dir);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 10));
            let a = view.editor.active_layer.clone();
            let b = view.editor.add_layer("B");
            view.editor.add_group("Destination");
            view.select_layer_ids(vec![a, b]);
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1600.), px(1400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (ids, target, depth) = cx.update(|_, cx| {
            let app = view.read(cx);
            (
                app.selected_layer_ids(),
                app.editor.document.layers.last().unwrap().id.clone(),
                app.editor.undo_depth(),
            )
        });
        let source_selector: &'static str = Box::leak(format!("layer-{}", ids[0]).into_boxed_str());
        let target_selector: &'static str = Box::leak(format!("layer-{target}").into_boxed_str());
        let bounds = cx.debug_bounds(source_selector).expect("source visible");
        let from = point(bounds.origin.x + px(80.), bounds.center().y);
        let to = cx
            .debug_bounds(target_selector)
            .expect("destination visible")
            .center();
        cx.simulate_mouse_move(from, None, Modifiers::default());
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            assert_eq!(view.read(cx).selected_layer_ids(), ids);
            window.draw(cx).clear(cx);
        });
        cx.simulate_mouse_move(
            from + point(px(10.), px(0.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.update(|_, cx| assert!(cx.has_active_drag()));
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.editor
                    .document
                    .find_layer(&target)
                    .unwrap()
                    .children
                    .iter()
                    .map(|l| l.id.clone())
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(app.editor.document.layers.len(), 1);
            assert_eq!(app.editor.undo_depth(), depth + 1);
        });
    }

    #[gpui_kit::test]
    fn layer_batch_commands_preserve_selection_and_single_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 10));
            let a = view.editor.active_layer.clone();
            let b = view.editor.add_layer("B");
            view.editor.add_layer("Keep");
            view.select_layer_ids(vec![a, b]);
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            let before = view.editor.undo_depth();
            view.command("group", window, cx);
            let group = view.editor.active_layer.clone();
            assert_eq!(
                view.editor
                    .document
                    .find_layer(&group)
                    .unwrap()
                    .children
                    .len(),
                2
            );
            assert_eq!(view.editor.document.layers.len(), 2);
            assert_eq!(view.editor.undo_depth(), before + 1);
            view.command("undo", window, cx);
            assert_eq!(view.editor.document.layers.len(), 3);
            let ids = view.editor.document.layers[..2]
                .iter()
                .map(|l| l.id.clone())
                .collect();
            view.select_layer_ids(ids);
            view.command("duplicate", window, cx);
            assert_eq!(view.layer_selection.ids.len(), 2);
            assert_eq!(view.editor.document.layers.len(), 5);
            view.command("delete", window, cx);
            assert_eq!(view.editor.document.layers.len(), 3);
            assert!(
                view.layer_selection.ids.iter().all(|id| view
                    .editor
                    .document
                    .find_layer(id)
                    .is_some())
            );
        });
    }

    #[gpui_kit::test]
    fn multi_layer_drop_copy_and_explicit_nest_are_transactions(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 10));
            let a = view.editor.active_layer.clone();
            let b = view.editor.add_layer("B");
            view.editor.add_group("Target");
            view.select_layer_ids(vec![a, b]);
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            let target = view.editor.document.layers.last().unwrap().id.clone();
            let drag = LayerDrag {
                ids: view.selected_layer_ids(),
                name: "Two layers".into(),
            };
            let before = view.editor.undo_depth();
            view.drop_layer_drag(&drag, Some(&target), 0, true, cx);
            assert_eq!(
                view.editor
                    .document
                    .find_layer(&target)
                    .unwrap()
                    .children
                    .len(),
                2
            );
            assert_eq!(view.editor.document.layers.len(), 3);
            assert_eq!(view.editor.undo_depth(), before + 1);
            let copied = view.selected_layer_ids();
            assert!(copied.iter().all(|id| !drag.ids.contains(id)));
            view.command("nest", window, cx);
            assert_eq!(view.dialog, Dialog::Nest);
            assert_eq!(view.editor.undo_depth(), before + 1);
            view.dialog = Dialog::None;
            view.command("unnest", window, cx);
            assert!(
                view.editor
                    .document
                    .find_layer(&target)
                    .unwrap()
                    .children
                    .is_empty()
            );
            assert_eq!(view.editor.document.layers.len(), 5);
        });
    }

    #[gpui_kit::test]
    fn canvas_units_anchor_fill_and_trim_use_one_commit_each(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(12, 10));
            view.editor.document.metadata["resolution"] = serde_json::json!(100.);
            view.editor.document.layers[0]
                .image
                .as_mut()
                .unwrap()
                .put_pixel(3, 2, image::Rgba([200, 40, 20, 255]));
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.command("resize", window, cx);
            view.change_canvas_units(1, false, window, cx);
            assert_eq!(view.width_input.read(cx).value().as_ref(), "100");
            view.width_input
                .update(cx, |s, cx| s.set_value("200", window, cx));
            view.height_input
                .update(cx, |s, cx| s.set_value("200", window, cx));
            view.change_canvas_units(2, false, window, cx);
            let (w, h) = view.canvas_dimensions(cx).unwrap();
            assert!((w - 24.).abs() < 1e-9 && (h - 20.).abs() < 1e-9);
            view.canvas_anchor = 8;
            let before = view.editor.undo_depth();
            view.confirm_dialog(window, cx);
            assert_eq!(
                (view.editor.document.width, view.editor.document.height),
                (24, 20)
            );
            assert_eq!(view.editor.undo_depth(), before + 1);
            assert_eq!(view.pixels.get_pixel(15, 12).0, [200, 40, 20, 255]);
            view.command("trim", window, cx);
            view.confirm_dialog(window, cx);
            assert_eq!(
                (view.editor.document.width, view.editor.document.height),
                (1, 1)
            );
            assert_eq!(view.pixels.get_pixel(0, 0).0, [200, 40, 20, 255]);
            assert_eq!(view.editor.undo_depth(), before + 2);
        });
    }

    #[gpui_kit::test]
    fn pointer_edit_shortcuts_unsaved_guard_and_theme_change(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let temp = tempfile::tempdir().unwrap();
        let recovery_dir = temp.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery_dir);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(128, 96));
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1440.), px(920.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("artwork").expect("canvas must be visible");
        let a = bounds.center() - point(px(50.), px(20.));
        let b = bounds.center() + point(px(50.), px(20.));
        cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::default());
        let painted = cx.update(|_, cx| {
            let app = view.read(cx);
            assert!(app.editor.is_dirty());
            assert!(app.pixels.pixels().any(|p| p[3] > 0));
            app.pixels.clone()
        });
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| assert!(view.read(cx).pixels.pixels().all(|p| p[3] == 0)));
        cx.simulate_keystrokes("ctrl-shift-z");
        cx.update(|_, cx| assert_eq!(view.read(cx).pixels, painted));
        cx.simulate_keystrokes("ctrl-n");
        cx.update(|_, cx| assert!(view.read(cx).dialog == Dialog::Unsaved));
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert!(view.read(cx).dialog == Dialog::None);
            assert_eq!(view.read(cx).pixels, painted);
        });
        cx.update(|window, cx| {
            gpui_omarchy::Theme::flexoki_light().apply(cx);
            window.draw(cx).clear(cx);
            assert_eq!(view.read(cx).pixels, painted);
        });
        cx.update(|window, cx| {
            gpui_omarchy::Theme::tokyo_night().apply(cx);
            window.draw(cx).clear(cx);
            assert_eq!(view.read(cx).pixels, painted);
        });
        // Saving through the actual dialog leaves reopened pixels identical.
        let project = temp.path().join("UI.omuse");
        let project_path = project.clone();
        view.update(cx, |app, cx| {
            app.path = Some(project_path);
            cx.notify();
        });
        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();
        cx.update(|_, cx| assert!(!view.read(cx).editor.is_dirty()));
        assert_eq!(
            raster::composite(&document::open(&project).unwrap()),
            painted
        );
    }

    #[gpui_kit::test]
    fn transform_drag_commits_once_and_escape_cancels_draft(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(128, 96));
            let id = view.editor.active_layer.clone();
            view.layer_selection.click(id, SelectionAction::Replace);
            view.tool = Tool::Move;
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let depth = cx.update(|_, cx| view.read(cx).editor.undo_depth());
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        let moved = point(center.x + px(20.), center.y);
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            let placement = view
                .editor
                .layer_placement(&view.editor.active_layer)
                .unwrap();
            assert_eq!(placement.x, 10.);
            assert_eq!(view.editor.undo_depth(), depth + 1);
        });
        let moved_center = point(center.x + px(20.), center.y);
        cx.simulate_mouse_down(moved_center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            point(moved_center.x + px(30.), moved_center.y),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                view.editor
                    .layer_placement(&view.editor.active_layer)
                    .unwrap()
                    .x,
                10.
            );
            assert_eq!(view.editor.undo_depth(), depth + 1);
            assert!(view.transform_drag.is_none());
        });
    }

    #[gpui_kit::test]
    fn dragging_a_rotated_flipped_photo_keeps_its_shape_and_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(128, 96));
            let id = view.editor.active_layer.clone();
            let original = view.editor.layer_placement(&id).unwrap();
            assert!(view.editor.set_layer_placement(
                &id,
                omuse::editor::LayerPlacement {
                    rotation: 37.,
                    flip_x: true,
                    flip_y: true,
                    ..original
                }
            ));
            view.layer_selection.click(id, SelectionAction::Replace);
            view.tool = Tool::Move;
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let (original, depth) = cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            (
                editor.layer_placement(&editor.active_layer).unwrap(),
                editor.undo_depth(),
            )
        });
        let destination = center + point(px(20.), px(-12.));
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        let expected = omuse::editor::LayerPlacement {
            x: original.x + 10.,
            y: original.y - 6.,
            ..original
        };
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            assert_eq!(editor.layer_placement(&editor.active_layer), Some(expected));
            assert_eq!(editor.undo_depth(), depth + 1);
        });
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            assert_eq!(editor.layer_placement(&editor.active_layer), Some(original));
        });
        cx.simulate_keystrokes("ctrl-shift-z");
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            assert_eq!(editor.layer_placement(&editor.active_layer), Some(expected));
        });
    }

    #[gpui_kit::test]
    fn ctrl_handle_distortion_is_one_transaction_and_escape_cancels(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            let mut doc = Document::new(64, 48);
            doc.layers[0].image = Some(
                image::RgbaImage::from_fn(64, 48, |x, y| {
                    image::Rgba([(x * 4) as u8, (y * 5) as u8, 83, 255])
                })
                .into(),
            );
            doc.layers[0].scale_x = -1.;
            doc.layers[0].scale_y = -1.;
            view.editor = Editor::new(doc);
            let id = view.editor.active_layer.clone();
            view.layer_selection.click(id, SelectionAction::Replace);
            view.tool = Tool::Move;
            view.zoom = 3.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let corner = center - point(px(96.), px(72.));
        let moved = corner + point(px(18.), px(15.));
        let modifiers = Modifiers {
            control: true,
            ..Default::default()
        };
        let before = cx.update(|_, cx| raster::composite(&view.read(cx).editor.document));
        let depth = cx.update(|_, cx| view.read(cx).editor.undo_depth());
        // Clicking a handle without dragging must not bake the transform or
        // create an undo step.
        cx.simulate_mouse_down(corner, MouseButton::Left, modifiers);
        cx.simulate_mouse_up(corner, MouseButton::Left, modifiers);
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            let placement = editor.layer_placement(&editor.active_layer).unwrap();
            assert!(placement.flip_x && placement.flip_y);
            assert_eq!(editor.undo_depth(), depth);
            assert_eq!(raster::composite(&editor.document), before);
        });
        cx.simulate_mouse_down(corner, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), modifiers);
        cx.update(|_, cx| assert!(view.read(cx).distort_draft.is_some()));
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.undo_depth(), depth));
        cx.simulate_mouse_up(moved, MouseButton::Left, modifiers);
        cx.simulate_mouse_down(corner, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), modifiers);
        cx.simulate_mouse_up(moved, MouseButton::Left, modifiers);
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            assert_eq!(editor.undo_depth(), depth + 1);
            let pixels = raster::composite(&editor.document);
            // The asymmetric artwork must still run right-to-left and
            // bottom-to-top after a real corner move.
            assert!(pixels.get_pixel(16, 24)[0] > pixels.get_pixel(48, 24)[0]);
            assert!(pixels.get_pixel(32, 12)[1] > pixels.get_pixel(32, 36)[1]);
        });
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| {
            let editor = &view.read(cx).editor;
            assert_eq!(raster::composite(&editor.document), before);
            let placement = editor.layer_placement(&editor.active_layer).unwrap();
            assert!(placement.flip_x && placement.flip_y);
        });
    }

    #[gpui_kit::test]
    fn spot_healing_pointer_needs_no_clone_source(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            let mut doc = Document::new(64, 48);
            let mut pixels =
                image::RgbaImage::from_pixel(64, 48, image::Rgba([120, 120, 120, 255]));
            pixels.put_pixel(32, 24, image::Rgba([255, 0, 0, 255]));
            doc.layers[0].image = Some(pixels.into());
            view.editor = Editor::new(doc);
            view.editor.brush.size = 8.;
            view.tool = Tool::SpotHeal;
            view.zoom = 3.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let depth = cx.update(|_, cx| view.read(cx).editor.undo_depth());
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert!(app.clone_source.is_none());
            assert_eq!(app.editor.undo_depth(), depth + 1);
            assert_ne!(app.pixels.get_pixel(32, 24).0, [255, 0, 0, 255]);
        });
    }

    #[gpui_kit::test]
    fn camera_scopes_follow_selected_preview_without_clipping_or_history(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let source = image::RgbaImage::from_raw(
            4,
            1,
            vec![
                0, 0, 0, 255, 80, 40, 20, 255, 60, 60, 60, 255, 90, 90, 90, 0,
            ],
        )
        .unwrap();
        let original = source.clone();
        let settings = omuse::camera_raw::Settings {
            exposure: 1.,
            ..Default::default()
        };
        let graded = omuse::camera_raw::apply(&source, &settings).unwrap();
        let mut expected = source.clone();
        expected.put_pixel(0, 0, *graded.get_pixel(0, 0));
        expected.put_pixel(1, 0, *graded.get_pixel(1, 0));
        let expected_scopes = omuse::photo_scopes::PhotoScopes::analyze(&expected);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = EditorView::new(None, window, cx);
            app.dialog = Dialog::None;
            let mut doc = Document::new(4, 1);
            doc.layers[0].image = Some(source.into());
            app.editor = Editor::new(doc);
            app.editor.select_rectangle(0., 0., 2., 1.);
            app.refresh(cx);
            app
        });
        cx.simulate_resize(size(px(900.), px(700.)));
        let depth = cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command("camera-raw", window, cx);
                // Start before initial-source analysis completes: its late result
                // must not replace the scopes for this developed preview.
                app.camera_clip_shadows = true;
                app.camera_clip_highlights = true;
                app.start_camera_raw(settings, true, cx);
                app.editor.undo_depth()
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let app = view.read(cx);
            assert!(!app.busy, "{}", app.status);
            assert!(app.camera_scopes_preview, "{}", app.status);
            assert_eq!(app.camera_scopes.as_deref(), Some(&expected_scopes));
            assert_eq!(
                app.editor.document.layers[0].image.as_deref(),
                Some(&original)
            );
            assert_eq!(app.editor.undo_depth(), depth);
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("camera-scopes").is_some());
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert_eq!(app.dialog, Dialog::None);
            assert_eq!(
                app.editor.document.layers[0].image.as_deref(),
                Some(&original)
            );
            assert_eq!(app.editor.undo_depth(), depth);
        });
    }

    #[gpui_kit::test]
    fn late_camera_scope_and_preview_jobs_cannot_replace_a_new_dialog(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = EditorView::new(None, window, cx);
            app.dialog = Dialog::None;
            app.editor = Editor::new(Document::new(8, 8));
            app.refresh(cx);
            app
        });
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command("camera-raw", window, cx);
                app.start_camera_raw(omuse::camera_raw::Settings::default(), true, cx);
                // The same transition made by Cancel, followed by a fresh Camera
                // Raw dialog. Leave both older worker completions queued.
                app.dialog_generation += 1;
                app.busy = false;
                app.dialog = Dialog::None;
                app.command("camera-raw", window, cx);
                let fresh = Arc::new(image::RgbaImage::from_pixel(
                    8,
                    8,
                    image::Rgba([1, 2, 3, 255]),
                ));
                app.start_camera_scopes(fresh, cx);
                app.status = "New dialog".into();
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = view.read(cx);
            let scopes = app.camera_scopes.as_ref().unwrap();
            assert!(!app.camera_scopes_preview);
            assert_eq!(scopes.rgb[0][1], 64 * 255);
            assert_eq!(scopes.rgb[1][2], 64 * 255);
            assert_eq!(scopes.rgb[2][3], 64 * 255);
            assert_eq!(app.status, "New dialog");
            assert!(!app.busy);
            assert_eq!(app.editor.undo_depth(), 0);
        });
    }

    #[gpui_kit::test]
    fn camera_preview_gestures_update_draft_without_touching_artwork(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            let mut doc = Document::new(64, 48);
            doc.layers[0].image =
                Some(image::RgbaImage::from_pixel(64, 48, image::Rgba([0, 255, 0, 255])).into());
            view.editor = Editor::new(doc);
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(900.)));
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.command("camera-raw", window, cx);
                app.camera_section = 3;
                app.load_camera_form(window, cx);
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let preview = cx.debug_bounds("camera-canvas").unwrap();
        cx.simulate_mouse_down(preview.center(), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(preview.center(), MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                assert_eq!(
                    app.camera_draft["mixer"]["points"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(app.camera_draft["mixer"]["points"][0]["hue"], 120.);
                assert_eq!(app.editor.undo_depth(), 0);
                app.camera_section = 7;
                app.load_camera_form(window, cx);
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let preview = cx.debug_bounds("camera-canvas").unwrap();
        let start = preview.center() - point(px(30.), px(30.));
        let end = preview.center() + point(px(30.), px(30.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.camera_draft["geometry"]["guides"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(app.camera_draft["geometry"]["upright"], "Guided");
            assert_eq!(app.editor.undo_depth(), 0);
            assert_eq!(app.pixels.get_pixel(32, 24).0, [0, 255, 0, 255]);
        });
    }

    #[gpui_kit::test]
    fn subject_refine_cancel_restores_preview_without_history(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(64, 48));
            view.refresh(cx);
            view
        });
        let (before, depth) = cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let mask = image::GrayImage::from_pixel(64, 48, image::Luma([128]));
                view.subject_mask = Some(mask.clone());
                view.subject_guide = Some(view.pixels.clone());
                view.subject_layer = Some(view.editor.active_layer.clone());
                view.subject_as_selection = true;
                view.dialog = Dialog::SubjectRefine;
                view.preview_subject_mask("ignored", &mask, true, cx);
                (
                    raster::composite(&view.editor.document),
                    view.editor.undo_depth(),
                )
            })
        });
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(raster::composite(&view.editor.document), before);
            assert_eq!(view.editor.undo_depth(), depth);
            assert!(view.subject_mask.is_none());
            assert_eq!(view.dialog, Dialog::None);
        });
    }

    #[gpui_kit::test]
    fn middle_button_pan_handles_combined_buttons_and_lost_release(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(128, 96));
            view.tool = Tool::Brush;
            view.zoom = 1.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let first = center - point(px(80.), px(50.));
        let second = first + point(px(30.), px(20.));

        for tool in [Tool::Brush, Tool::Fill] {
            view.update(cx, |view, _| view.tool = tool);
            let before = cx.update(|_, cx| raster::composite(&view.read(cx).editor.document));
            cx.simulate_mouse_down(center, MouseButton::Middle, Modifiers::default());
            cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(center, MouseButton::Middle, Modifiers::default());
            cx.update(|_, cx| {
                let view = view.read(cx);
                assert_eq!(raster::composite(&view.editor.document), before);
                assert!(view.drag_start.is_none());
                assert_eq!(view.editor.undo_depth(), 0);
            });
        }

        view.update(cx, |view, _| view.tool = Tool::Move);
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| assert!(view.read(cx).transform_drag.is_some()));
        cx.simulate_mouse_down(second, MouseButton::Middle, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.middle_pan_pointer.is_none());
            assert!(view.transform_drag.is_some());
            assert_eq!(view.pan, (0., 0.));
        });
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());

        cx.simulate_mouse_down(first, MouseButton::Middle, Modifiers::default());
        cx.simulate_mouse_move(second, Some(MouseButton::Middle), Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pan, (30., 20.));
            assert!(view.drag_start.is_none());
            assert_eq!(view.editor.undo_depth(), 0);
        });
        cx.simulate_mouse_up(second, MouseButton::Middle, Modifiers::default());
        cx.update(|_, cx| assert!(view.read(cx).middle_pan_pointer.is_none()));

        view.update(cx, |view, cx| {
            view.tool = Tool::Hand;
            cx.notify();
        });
        let left_start = center - point(px(60.), px(30.));
        let middle_start = center + point(px(90.), px(70.));
        let together = center - point(px(40.), px(30.));
        cx.simulate_mouse_down(left_start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_down(middle_start, MouseButton::Middle, Modifiers::default());
        cx.simulate_mouse_move(together, Some(MouseButton::Left), Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pan, (-100., -80.));
            assert!(view.pan_pointer.is_some() && view.middle_pan_pointer.is_some());
        });
        cx.simulate_mouse_up(together, MouseButton::Middle, Modifiers::default());
        let left_only = together + point(px(15.), px(5.));
        cx.simulate_mouse_move(left_only, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(left_only, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pan, (-85., -75.));
            assert!(view.pan_pointer.is_none() && view.middle_pan_pointer.is_none());
        });

        let held = cx.update(|_, cx| view.read(cx).pan);
        cx.simulate_mouse_down(center, MouseButton::Middle, Modifiers::default());
        cx.simulate_mouse_move(center + point(px(20.), px(20.)), None, Modifiers::default());
        cx.simulate_mouse_move(
            center + point(px(40.), px(40.)),
            Some(MouseButton::Middle),
            Modifiers::default(),
        );
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pan, held);
            assert!(view.middle_pan_pointer.is_none());
        });
    }

    #[gpui_kit::test]
    fn auto_select_prefers_frontmost_visible_nested_layer_and_can_be_disabled(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(80, 60));
            let bottom = view.editor.document.layers[0].id.clone();

            let candidate = Layer::paint("Nested frontmost visible", 80, 60);
            let candidate_id = candidate.id.clone();
            let mut hidden_child = Layer::paint("Hidden child", 80, 60);
            hidden_child.visible = false;
            let mut transparent_child = Layer::paint("Zero opacity child", 80, 60);
            transparent_child.opacity = 0.;
            let mut group = Layer::group("Top group");
            group.children = vec![candidate, hidden_child, transparent_child];
            view.editor.document.layers.push(group);
            let mut hidden_root = Layer::paint("Hidden root", 80, 60);
            hidden_root.visible = false;
            view.editor.document.layers.push(hidden_root);
            let mut transparent_root = Layer::paint("Zero opacity root", 80, 60);
            transparent_root.opacity = 0.;
            view.editor.document.layers.push(transparent_root);

            assert_eq!(
                hit_layer_at(&view.editor, 40., 30.),
                Some(candidate_id.clone())
            );
            view.editor.active_layer = bottom.clone();
            view.select_layer_ids(vec![bottom]);
            view.tool = Tool::Move;
            view.preferences.auto_select = true;
            view.preferences.transform_box = true;
            view.zoom = 1.;
            view.refresh(cx);
            view
        });
        let (bottom, candidate, group) = cx.update(|_, cx| {
            let view = view.read(cx);
            (
                view.editor.document.layers[0].id.clone(),
                view.editor.document.layers[1].children[0].id.clone(),
                view.editor.document.layers[1].id.clone(),
            )
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.editor.active_layer, candidate);
            assert_eq!(view.layer_selection.ids, vec![candidate.clone()]);
        });

        view.update(cx, |view, cx| {
            view.editor.active_layer = bottom.clone();
            view.select_layer_ids(vec![bottom.clone()]);
            view.preferences.auto_select = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.editor.active_layer, bottom);
            assert_eq!(view.layer_selection.ids, vec![bottom.clone()]);
        });

        let before = cx.update(|_, cx| view.read(cx).editor.layer_placement(&candidate).unwrap());
        view.update(cx, |view, cx| {
            view.editor.active_layer = group.clone();
            view.select_layer_ids(vec![group.clone()]);
            view.preferences.auto_select = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let destination = center + point(px(12.), px(7.));
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            let after = view.editor.layer_placement(&candidate).unwrap();
            assert_eq!(view.editor.active_layer, group);
            assert_eq!(view.layer_selection.ids, vec![group.clone()]);
            assert_eq!((after.x - before.x, after.y - before.y), (12., 7.));
        });
    }

    #[gpui_kit::test]
    fn auto_select_preserves_a_multiselection_when_its_union_gap_is_dragged(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(100, 60));
            let background = view.editor.active_layer.clone();
            let mut left = Layer::paint("Left", 20, 20);
            left.offset_x = 10.;
            left.offset_y = 20.;
            let left_id = left.id.clone();
            let mut right = Layer::paint("Right", 20, 20);
            right.offset_x = 70.;
            right.offset_y = 20.;
            let right_id = right.id.clone();
            view.editor.document.layers.extend([left, right]);
            view.editor.active_layer = right_id.clone();
            view.select_layer_ids(vec![left_id, right_id]);
            view.tool = Tool::Move;
            view.preferences.auto_select = true;
            view.preferences.transform_box = true;
            view.zoom = 1.;
            assert_eq!(hit_layer_at(&view.editor, 50., 30.), Some(background));
            view.refresh(cx);
            view
        });
        let (selected, before) = cx.update(|_, cx| {
            let view = view.read(cx);
            let selected = view.layer_selection.ids.clone();
            let placements = selected
                .iter()
                .map(|id| view.editor.layer_placement(id).unwrap())
                .collect::<Vec<_>>();
            (selected, placements)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let destination = center + point(px(9.), px(4.));
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.layer_selection.ids, selected);
            for (id, before) in selected.iter().zip(before) {
                let after = view.editor.layer_placement(id).unwrap();
                assert_eq!((after.x - before.x, after.y - before.y), (9., 4.));
            }
        });
    }

    #[gpui_kit::test]
    fn visible_guide_drags_once_and_move_pointer_snaps(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(128, 96));
            view.editor
                .add_guide(omuse::editor::GuideAxis::Vertical, 64.);
            view.show_guides = true;
            view.show_grid = true;
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let center = cx.debug_bounds("artwork").unwrap().center();
        let depth = cx.update(|_, cx| view.read(cx).editor.undo_depth());
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        let destination = point(center.x + px(20.), center.y);
        cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.editor.guides()[0].position, 74.);
            assert_eq!(view.editor.undo_depth(), depth + 1);
            assert_eq!(
                snap_canvas_point(
                    &view.editor,
                    CanvasPoint { x: 81., y: 15. },
                    true,
                    true,
                    3.,
                    omuse::canvas_grid::GridSettings::default(),
                    false,
                ),
                CanvasPoint { x: 80., y: 16. }
            );
        });
    }

    #[gpui_kit::test]
    fn adding_a_guide_persists_its_auto_shown_state(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::Guide;
            view.editor = Editor::new(Document::new(128, 96));
            view.show_guides = false;
            view.preferences.guides = false;
            view.detail_inputs[0].update(cx, |input, cx| input.set_value("vertical", window, cx));
            view.detail_inputs[1].update(cx, |input, cx| input.set_value("42", window, cx));
            view
        });
        view.update_in(cx, |view, window, cx| view.confirm_dialog(window, cx));
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::None);
            assert!(view.show_guides && view.preferences.guides);
            assert_eq!(view.editor.guides().len(), 1);
            assert_eq!(view.editor.guides()[0].position, 42.);
        });
    }

    #[gpui_kit::test]
    fn mask_target_fill_changes_only_mask_and_one_history_step(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(64, 48));
            let id = view.editor.active_layer.clone();
            assert!(view.editor.add_mask(&id, true));
            view.paint_mask = true;
            view.tool = Tool::Fill;
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.set_brush_rgb(0., 0., 0., window, cx))
        });
        let center = cx.debug_bounds("artwork").unwrap().center();
        let (source, depth) = cx.update(|_, cx| {
            let view = view.read(cx);
            let layer = view
                .editor
                .document
                .find_layer(&view.editor.active_layer)
                .unwrap();
            (layer.image.clone(), view.editor.undo_depth())
        });
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            let layer = view
                .editor
                .document
                .find_layer(&view.editor.active_layer)
                .unwrap();
            assert_eq!(layer.image, source);
            assert!(
                layer
                    .mask
                    .as_ref()
                    .unwrap()
                    .pixels()
                    .all(|pixel| pixel[0] == 0)
            );
            assert_eq!(view.editor.undo_depth(), depth + 1);
            assert_eq!(view.status, "Mask area filled");
        });
    }

    #[test]
    fn contour_sampling_keeps_large_off_grid_rectangle_edges() {
        let (width, height) = (4096u32, 4096u32);
        let mut mask = vec![0; width as usize * height as usize];
        for y in 5..4090usize {
            for x in 3..4088usize {
                mask[y * width as usize + x] = 255;
            }
        }
        let contour = selection_contour_points(
            &Selection {
                width,
                height,
                mask,
            },
            200_000,
        );
        assert!(contour.iter().any(|&(x, y)| x == 3 && y > 5));
        assert!(contour.iter().any(|&(x, y)| y == 5 && x > 3));
        assert!(contour.iter().any(|&(x, y)| x == 4087 && y < 4089));
        assert!(contour.iter().any(|&(x, y)| y == 4089 && x < 4087));
    }

    #[gpui_kit::test]
    fn camera_curve_edit_is_draft_only_and_cancel_preserves_history(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(64, 48));
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(850.)));
        let (before, depth) = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.command("camera-raw", window, cx);
                view.camera_section = crate::camera_controls::SECTIONS
                    .iter()
                    .position(|section| section.0 == "curve")
                    .unwrap();
                view.camera_curve_channel = 0;
                view.load_camera_form(window, cx);
                (
                    raster::composite(&view.editor.document),
                    view.editor.undo_depth(),
                )
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let graph = cx.debug_bounds("camera-curve-editor").unwrap();
        let body = cx.debug_bounds("dialog-body").unwrap();
        assert!(graph.origin.y >= body.origin.y);
        assert!(graph.origin.y + graph.size.height <= body.origin.y + body.size.height);
        cx.simulate_mouse_down(graph.center(), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(graph.center(), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                view.camera_draft["curve"]["rgb"].as_array().unwrap().len(),
                3
            );
            assert_eq!(raster::composite(&view.editor.document), before);
            assert_eq!(view.editor.undo_depth(), depth);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::None);
            assert_eq!(raster::composite(&view.editor.document), before);
            assert_eq!(view.editor.undo_depth(), depth);
        });
    }

    #[gpui_kit::test]
    fn mask_transform_selection_never_lifts_source_pixels(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(32, 24));
            let id = view.editor.active_layer.clone();
            assert!(view.editor.add_mask(&id, true));
            view.editor.select_rectangle(3., 4., 12., 8.);
            view.paint_mask = true;
            view.refresh(cx);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let id = view.editor.active_layer.clone();
                let source = view.editor.document.find_layer(&id).unwrap().image.clone();
                let depth = view.editor.undo_depth();
                view.command("transform-selection", window, cx);
                assert_eq!(view.editor.document.find_layer(&id).unwrap().image, source);
                assert!(view.editor.floating_selection_layer().is_none());
                assert_eq!(view.editor.undo_depth(), depth);
                assert!(view.status.contains("unavailable while editing a mask"));
            });
        });
    }

    #[gpui_kit::test]
    fn mask_delete_without_selection_removes_mask_and_preserves_source(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(32, 24));
            let id = view.editor.active_layer.clone();
            assert!(view.editor.add_mask(&id, true));
            view.paint_mask = true;
            view.refresh(cx);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let id = view.editor.active_layer.clone();
                let source = view.editor.document.find_layer(&id).unwrap().image.clone();
                let depth = view.editor.undo_depth();
                view.command("delete-content", window, cx);
                let layer = view.editor.document.find_layer(&id).unwrap();
                assert_eq!(layer.image, source);
                assert!(layer.mask.is_none());
                assert!(!view.paint_mask);
                assert_eq!(view.editor.undo_depth(), depth + 1);
            });
        });
    }

    #[gpui_kit::test]
    fn gradient_preview_cancel_preserves_pixels_and_history(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(48, 32));
            view.tool = Tool::Gradient;
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let artwork = cx.debug_bounds("artwork").unwrap();
        let start = artwork.center() - point(px(20.), px(0.));
        let end = artwork.center() + point(px(20.), px(0.));
        let (before, depth) = cx.update(|_, cx| {
            let view = view.read(cx);
            (
                raster::composite(&view.editor.document),
                view.editor.undo_depth(),
            )
        });
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Gradient));
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(raster::composite(&view.editor.document), before);
            assert_eq!(view.editor.undo_depth(), depth);
            assert_eq!(view.dialog, Dialog::None);
        });
    }

    #[gpui_kit::test]
    fn asynchronous_wand_selects_without_changing_document(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(8, 8));
            view.editor.document.layers[0]
                .image
                .as_mut()
                .unwrap()
                .put_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
            view.wand_settings.tolerance = 0;
            view.refresh(cx);
            view
        });
        view.update(cx, |view, cx| view.start_image_selection(2., 2., false, cx));
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(!view.busy);
            assert_eq!(
                view.editor
                    .selection
                    .as_ref()
                    .unwrap()
                    .mask
                    .iter()
                    .filter(|&&v| v != 0)
                    .count(),
                1
            );
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(!view.editor.is_dirty());
        });
    }

    #[gpui_kit::test]
    fn compact_camera_raw_dialog_keeps_action_footer_visible(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(64, 48));
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.command("camera-raw", window, cx));
            window.draw(cx).clear(cx);
        });
        let dialog = cx.debug_bounds("dialog").expect("dialog must be visible");
        let footer = cx
            .debug_bounds("dialog-footer")
            .expect("action footer must be visible");
        assert!(footer.origin.y >= dialog.origin.y);
        assert!(footer.origin.y + footer.size.height <= dialog.origin.y + dialog.size.height);
        assert!(dialog.origin.y >= px(0.));
        assert!(dialog.origin.y + dialog.size.height <= px(600.));
    }

    #[gpui_kit::test]
    fn aligned_clone_keeps_offset_across_disjoint_strokes(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(80, 40));
            view.tool = Tool::Clone;
            view.clone_aligned = true;
            view.zoom = 2.;
            view.editor.brush.size = 2.;
            let id = view.editor.active_layer.clone();
            let image = view
                .editor
                .document
                .find_layer_mut(&id)
                .unwrap()
                .image
                .as_mut()
                .unwrap();
            for y in 6..15 {
                for x in 6..15 {
                    image.put_pixel(x, y, image::Rgba([240, 40, 20, 255]));
                }
            }
            // The second disjoint target (60,25) must sample (40,25) when the
            // original source-to-target offset (-20,0) is retained.
            for y in 21..30 {
                for x in 36..45 {
                    image.put_pixel(x, y, image::Rgba([20, 60, 230, 255]));
                }
            }
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let artwork = cx.debug_bounds("artwork").unwrap();
        let canvas = |x: f32, y: f32| {
            point(
                artwork.center().x - px(80.) + px(x * 2.),
                artwork.center().y - px(40.) + px(y * 2.),
            )
        };
        let source = canvas(10., 10.);
        cx.simulate_mouse_down(
            source,
            MouseButton::Left,
            Modifiers {
                alt: true,
                ..Default::default()
            },
        );
        cx.simulate_mouse_up(source, MouseButton::Left, Modifiers::default());
        for target in [canvas(30., 10.), canvas(60., 25.)] {
            cx.simulate_mouse_down(target, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(
                point(target.x + px(2.), target.y),
                Some(MouseButton::Left),
                Modifiers::default(),
            );
            cx.simulate_mouse_up(
                point(target.x + px(2.), target.y),
                MouseButton::Left,
                Modifiers::default(),
            );
        }
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.clone_source, Some((10., 10.)));
            assert_eq!(view.clone_offset, Some((-20., 0.)));
            assert_eq!(view.editor.undo_depth(), 2);
            let image = view
                .editor
                .document
                .find_layer(&view.editor.active_layer)
                .unwrap()
                .image
                .as_ref()
                .unwrap();
            let first = image.get_pixel(30, 10).0;
            let second = image.get_pixel(60, 25).0;
            assert!(
                first[0] > first[2] && first[3] > 0,
                "first clone: {first:?}"
            );
            assert!(
                second[2] > second[0] && second[3] > 0,
                "second clone: {second:?}"
            );
        });
    }

    #[gpui_kit::test]
    fn installed_fonts_load_only_when_text_dialog_opens_and_stay_cached(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| assert!(view.read(cx).font_names.is_none()));

        view.update(cx, |view, cx| {
            view.dialog = Dialog::Text;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            let fonts = view
                .font_names
                .as_mut()
                .expect("fonts loaded on first open");
            assert!(!fonts.is_empty());
            fonts.push("Omuse cached-font sentinel".into());
            view.dialog = Dialog::None;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.dialog = Dialog::Text;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| {
            assert!(
                view.read(cx)
                    .font_names
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|name| name == "Omuse cached-font sentinel")
            );
        });
    }

    #[gpui_kit::test]
    fn text_dialog_color_is_draft_and_apply_does_not_change_foreground(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(80, 40));
            view.color.update(cx, |picker, cx| {
                picker.set_value(rgba(0x0ac81eff), window, cx)
            });
            let style = objects::LiveTextStyle {
                content: "Draft color".into(),
                red: 1.,
                green: 0.,
                blue: 0.,
                ..Default::default()
            };
            let layer =
                objects::live_text_layer("Text", objects::ObjectPoint { x: 4., y: 5. }, style)
                    .unwrap();
            view.editor.import_layer(layer);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let foreground = view.editor.brush.color;
                view.command("edit-object", window, cx);
                assert_eq!(view.dialog, Dialog::Text);
                assert_eq!(view.editor.brush.color, foreground);
                view.dialog_color_picker.update(cx, |picker, cx| {
                    picker.set_value(rgba(0x145adcff), window, cx)
                });
            });
            window.draw(cx).clear(cx);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).dialog, Dialog::None);
            assert_eq!(view.read(cx).editor.brush.color, [10, 200, 30, 255]);
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.command("edit-object", window, cx);
                view.dialog_color_picker.update(cx, |picker, cx| {
                    picker.set_value(rgba(0x145adcff), window, cx)
                });
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let foreground = [10, 200, 30, 255];
                assert_eq!(view.editor.brush.color, foreground);
                view.confirm_dialog(window, cx);
                assert_eq!(view.editor.brush.color, foreground);
                let text = objects::live_text(
                    view.editor
                        .document
                        .find_layer(&view.editor.active_layer)
                        .unwrap(),
                )
                .unwrap()
                .unwrap();
                assert!(text.blue > text.red && text.blue > text.green);
            });
        });
    }

    #[gpui_kit::test]
    fn png_export_ignores_invalid_jpeg_only_fields(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("export.png");
        let output_text = output.to_string_lossy().to_string();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::Export;
            view.editor = Editor::new(Document::new(8, 8));
            view.path_input
                .update(cx, |input, cx| input.set_value(output_text, window, cx));
            for index in 0..4 {
                view.detail_inputs[index]
                    .update(cx, |input, cx| input.set_value("invalid", window, cx));
            }
            view.detail_inputs[4].update(cx, |input, cx| input.set_value("72", window, cx));
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.confirm_dialog(window, cx));
        });
        cx.run_until_parked();
        assert!(output.is_file());
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::None));
    }

    #[gpui_kit::test]
    fn canvas_aspect_lock_tracks_width_in_relative_physical_units(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(400, 200));
            view.editor.document.metadata["resolution"] = serde_json::json!(100.0);
            view.dialog = Dialog::Resize;
            view.canvas_units = 2;
            view.canvas_relative = true;
            view.canvas_lock_aspect = true;
            view.width_input
                .update(cx, |input, cx| input.set_value("2", window, cx));
            view.sync_canvas_aspect(true, window, cx);
            view
        });
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.height_input.read(cx).value(), "1");
            assert_eq!(view.canvas_dimensions(cx), Some((600., 300.)));
        });
    }

    #[test]
    fn jpeg_preview_is_real_encoded_matted_output() {
        let mut document = Document::new(2, 1);
        let layer = document.layers.first_mut().unwrap();
        layer.image = Some(
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 128, 0, 0, 0, 0])
                .unwrap()
                .into(),
        );
        let encoded = raster::encode_jpeg(
            &document,
            raster::ExportOptions {
                jpeg_quality: 80,
                matte: [255, 255, 255],
            },
        )
        .unwrap();
        let bytes = encoded.len();
        let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::Jpeg)
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert!(bytes > 0);
        assert_eq!(decoded.get_pixel(0, 0)[3], 255);
        assert_eq!(decoded.get_pixel(1, 0)[3], 255);
    }

    #[gpui_kit::test]
    fn type_tool_click_places_live_text_at_canvas_point(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(100, 60));
            view.zoom = 2.;
            view.refresh(cx);
            view.command("text", window, cx);
            view
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let artwork = cx.debug_bounds("artwork").unwrap();
        let target = point(
            artwork.center().x - px(100.) + px(34. * 2.),
            artwork.center().y - px(60.) + px(21. * 2.),
        );
        cx.simulate_mouse_down(target, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.dialog, Dialog::None);
                view.inline_text
                    .as_ref()
                    .unwrap()
                    .input
                    .update(cx, |input, cx| input.set_value("Placed", window, cx));
                assert!(view.finish_inline_text(true, window, cx));
                let layer = view
                    .editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .unwrap();
                assert!((layer.offset_x - 34.).abs() < 0.01);
                assert!((layer.offset_y - 21.).abs() < 0.01);
                assert_eq!(
                    objects::live_text(layer).unwrap().unwrap().content,
                    "Placed"
                );
            });
        });
    }

    #[gpui_kit::test]
    fn inline_text_native_keys_multiline_cancel_commit_and_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(640, 480));
            view.zoom = 1.;
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(850.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            view.update(cx, |v, cx| {
                v.begin_inline_text(None, (30., 40.), None, window, cx)
            });
        });
        cx.simulate_input("Draft é 東京");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Second line");
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert_eq!(
                v.inline_text
                    .as_ref()
                    .unwrap()
                    .input
                    .read(cx)
                    .value()
                    .as_ref(),
                "Draft é 東京\nSecond line"
            );
            assert_eq!(v.editor.undo_depth(), 0);
            assert!(!v.editor.is_dirty());
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                assert!(v.inline_text.is_none());
                assert_eq!(v.editor.undo_depth(), 0);
                v.begin_inline_text(None, (30., 40.), None, window, cx);
            });
        });
        cx.simulate_input("Committed");
        cx.simulate_keystrokes("ctrl-enter");
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.inline_text.is_none());
            assert_eq!(v.editor.undo_depth(), 1);
            assert_eq!(
                objects::live_text(
                    v.editor
                        .document
                        .find_layer(&v.editor.active_layer)
                        .unwrap()
                )
                .unwrap()
                .unwrap()
                .content,
                "Committed"
            );
        });
        cx.simulate_keystrokes("ctrl-z");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.document.layers.len(), 1));
        cx.simulate_keystrokes("ctrl-shift-z");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.document.layers.len(), 2));
    }

    #[gpui_kit::test]
    fn inline_text_existing_draft_guards_locks_errors_and_empty_new_layers(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.dialog = Dialog::None;
            v.editor = Editor::new(Document::new(200, 150));
            v.editor.import_layer(
                objects::live_text_layer(
                    "Original",
                    objects::ObjectPoint { x: 10., y: 20. },
                    objects::LiveTextStyle {
                        content: "Original".into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
            );
            v.refresh(cx);
            v
        });
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                let id = v.editor.active_layer.clone();
                let before = raster::composite(&v.editor.document);
                let depth = v.editor.undo_depth();
                v.begin_inline_text(Some(id.clone()), (0., 0.), None, window, cx);
                v.inline_text
                    .as_ref()
                    .unwrap()
                    .input
                    .update(cx, |input, cx| input.set_value("Different", window, cx));
                assert!(v.finish_inline_text(false, window, cx));
                assert_eq!(raster::composite(&v.editor.document), before);
                assert_eq!(v.editor.undo_depth(), depth);
                v.begin_inline_text(None, (30., 40.), None, window, cx);
                assert!(v.finish_inline_text(true, window, cx));
                assert_eq!(v.editor.undo_depth(), depth);
                v.begin_inline_text(Some(id.clone()), (0., 0.), None, window, cx);
                v.inline_text.as_mut().unwrap().style.font_size = f32::NAN;
                assert!(!v.finish_inline_text(true, window, cx));
                assert!(v.inline_text.is_some());
                assert_eq!(raster::composite(&v.editor.document), before);
                assert_eq!(v.editor.undo_depth(), depth);
                v.finish_inline_text(false, window, cx);
                v.editor.set_locked(&id, true);
                v.begin_inline_text(Some(id.clone()), (0., 0.), None, window, cx);
                assert!(v.inline_text.is_none());
            });
        });
    }

    #[gpui_kit::test]
    fn inline_text_drag_box_and_click_existing_use_canvas_gestures(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.dialog = Dialog::None;
            v.editor = Editor::new(Document::new(300, 200));
            v.tool = Tool::Text;
            v.zoom = 1.;
            v.refresh(cx);
            v
        });
        cx.simulate_resize(size(px(1200.), px(850.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let artwork = cx.debug_bounds("artwork").unwrap();
        let start = artwork.center() - point(px(100.), px(60.));
        let end = start + point(px(160.), px(100.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx).inline_text.as_ref().unwrap().style.box_size,
                Some(objects::ObjectSize {
                    width: 160.,
                    height: 100.
                })
            )
        });
        cx.simulate_input("Box");
        cx.simulate_keystrokes("ctrl-enter");
        cx.simulate_click(start + point(px(20.), px(20.)), Modifiers::default());
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert_eq!(
                v.inline_text.as_ref().unwrap().layer.as_ref(),
                Some(&v.editor.active_layer)
            );
        });
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("Edited");
        cx.simulate_keystrokes("ctrl-enter");
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert_eq!(v.editor.document.layers.len(), 2);
            assert_eq!(v.editor.undo_depth(), 2);
            assert_eq!(
                objects::live_text(
                    v.editor
                        .document
                        .find_layer(&v.editor.active_layer)
                        .unwrap()
                )
                .unwrap()
                .unwrap()
                .content,
                "Edited"
            );
        });
    }

    #[gpui_kit::test]
    fn shape_tool_uses_predraw_corner_radius_and_line_width(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(100, 60));
            view.zoom = 2.;
            view.refresh(cx);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.command("tool-settings", window, cx);
                assert_eq!(view.dialog, Dialog::ToolSettings);
                view.detail_inputs[7].update(cx, |input, cx| input.set_value("13", window, cx));
                view.detail_inputs[8].update(cx, |input, cx| input.set_value("9", window, cx));
                view.confirm_dialog(window, cx);
                assert_eq!(view.dialog, Dialog::None, "{}", view.status);
                assert_eq!(view.shape_corner_radius, 13.);
                assert_eq!(view.shape_line_width, 9.);
                view.tool = Tool::ShapeRect;
                cx.notify();
            });
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let artwork = cx.debug_bounds("artwork").unwrap();
        let canvas = |x: f32, y: f32| {
            point(
                artwork.center().x - px(100.) + px(x * 2.),
                artwork.center().y - px(60.) + px(y * 2.),
            )
        };
        cx.simulate_mouse_down(canvas(10., 10.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            canvas(40., 35.),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(canvas(40., 35.), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            let shape = objects::live_shape(
                view.editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(shape.kind, objects::LiveShapeKind::Rectangle);
            assert_eq!(shape.corner_radius, 13.);
        });
        view.update(cx, |view, cx| {
            view.tool = Tool::Line;
            cx.notify();
        });
        cx.simulate_mouse_down(canvas(15., 45.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            canvas(55., 45.),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(canvas(55., 45.), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            let shape = objects::live_shape(
                view.editor
                    .document
                    .find_layer(&view.editor.active_layer)
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(shape.kind, objects::LiveShapeKind::Line);
            assert_eq!(shape.line_width, Some(9.));
        });
    }

    #[gpui_kit::test]
    fn grid_controls_share_settings_and_shift_bypasses_snapping(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(100, 60));
            view.refresh(cx);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.preferences.grid_spacing, 8);
                view.command("grid-spacing", window, cx);
                view.command("grid-subdivisions", window, cx);
                assert_eq!(view.preferences.grid_spacing, 16);
                assert_eq!(view.preferences.grid_subdivisions, 2);
                let settings = omuse::canvas_grid::GridSettings {
                    spacing: view.preferences.grid_spacing,
                    subdivisions: view.preferences.grid_subdivisions,
                };
                assert_eq!(
                    snap_canvas_point(
                        &view.editor,
                        CanvasPoint { x: 7., y: 7. },
                        true,
                        false,
                        20.,
                        settings,
                        false,
                    ),
                    CanvasPoint { x: 8., y: 8. }
                );
                assert_eq!(
                    snap_canvas_point(
                        &view.editor,
                        CanvasPoint { x: 7., y: 7. },
                        true,
                        false,
                        20.,
                        settings,
                        true,
                    ),
                    CanvasPoint { x: 7., y: 7. }
                );
            });
        });
    }

    #[gpui_kit::test]
    fn mask_view_command_is_reversible_without_history(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(32, 24));
            let id = view.editor.active_layer.clone();
            assert!(view.editor.add_mask(&id, true));
            view.refresh(cx);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let depth = view.editor.undo_depth();
                view.command("mask-view", window, cx);
                assert!(view.mask_inspection.active());
                assert_eq!(view.editor.undo_depth(), depth);
                view.command("mask-view", window, cx);
                assert!(!view.mask_inspection.active());
                assert_eq!(view.editor.undo_depth(), depth);
            });
        });
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod parity_interaction_tests {
    use super::*;
    use gpui_kit::TestAppContext;
    #[gpui_kit::test]
    fn live_object_dialog_edit_undo_and_project_roundtrip(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.recovery = Recovery::at(temp.path().join("recovery"));
            v.dialog = Dialog::None;
            v.editor = Editor::new(Document::new(320, 240));
            v
        });
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.command("new-text", window, cx);
                v.path_input
                    .update(cx, |s, cx| s.set_value("Editable Linux", window, cx));
                v.confirm_dialog(window, cx);
                assert!(v.dialog == Dialog::None, "{}", v.status);
                assert!(
                    objects::live_text(
                        v.editor
                            .document
                            .find_layer(&v.editor.active_layer)
                            .unwrap()
                    )
                    .unwrap()
                    .is_some()
                );
                v.command("edit-object", window, cx);
                v.path_input
                    .update(cx, |s, cx| s.set_value("Edited again", window, cx));
                v.confirm_dialog(window, cx);
                assert!(v.dialog == Dialog::None, "{}", v.status);
                let layer = v
                    .editor
                    .document
                    .find_layer(&v.editor.active_layer)
                    .unwrap();
                assert_eq!(
                    objects::live_text(layer).unwrap().unwrap().content,
                    "Edited again"
                );
                v.command("undo", window, cx);
                assert_eq!(
                    objects::live_text(
                        v.editor
                            .document
                            .find_layer(&v.editor.active_layer)
                            .unwrap()
                    )
                    .unwrap()
                    .unwrap()
                    .content,
                    "Editable Linux"
                );
                v.command("redo", window, cx);
                let path = temp.path().join("Editable.omuse");
                document::save(&v.editor.document, &path).unwrap();
                let reopened = document::open(&path).unwrap();
                assert_eq!(raster::composite(&reopened), v.pixels);
                assert_eq!(
                    objects::live_text(reopened.find_layer(&v.editor.active_layer).unwrap())
                        .unwrap()
                        .unwrap()
                        .content,
                    "Edited again"
                );
            })
        });
    }
    #[gpui_kit::test]
    fn shortcut_recorder_intercepts_commands_and_remapped_save_works(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.recovery = Recovery::at(temp.path().join("recovery"));
            v.dialog = Dialog::None;
            v.editor = Editor::new(Document::new(32, 32));
            v.path = Some(temp.path().join("Keys.omuse"));
            v
        });
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.command("shortcuts", window, cx);
                v.recording = Some("save".into());
                v.modal_focus.focus(window, cx);
            })
        });
        cx.simulate_keystrokes("ctrl-alt-s");
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                assert_eq!(v.shortcut_draft.chord("save"), "ctrl-alt-s");
                assert!(v.recording.is_none());
                install_shortcuts(&v.shortcut_draft, &v.shortcuts, cx);
                v.shortcuts = v.shortcut_draft.clone();
                v.dialog = Dialog::None;
                v.focus.focus(window, cx);
                v.editor.fill_selection([50, 80, 120, 255]);
                v.changed(cx);
            })
        });
        cx.simulate_keystrokes("ctrl-s");
        assert!(!temp.path().join("Keys.omuse").exists());
        cx.simulate_keystrokes("ctrl-alt-s");
        cx.run_until_parked();
        assert!(temp.path().join("Keys.omuse/manifest.json").exists());
        cx.update(|_, cx| assert!(!view.read(cx).editor.is_dirty()));
    }

    #[gpui_kit::test]
    fn inline_text_save_chords_distinguish_save_as_from_save(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        cx.update(bind_keys);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Existing.omuse");
        let mut doc = Document::new(160, 120);
        doc.layers.push(
            objects::live_text_layer(
                "Editable",
                objects::ObjectPoint { x: 12., y: 18. },
                objects::LiveTextStyle {
                    content: "On disk".into(),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let id = doc.layers.last().unwrap().id.clone();
        document::save(&doc, &path).unwrap();
        let manifest = std::fs::read(path.join("manifest.json")).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(Some(path.clone()), window, cx);
            v.dialog = Dialog::None;
            v.editor = Editor::new(doc);
            v.editor.mark_saved();
            v.begin_inline_text(Some(id.clone()), (0., 0.), None, window, cx);
            v.inline_text
                .as_ref()
                .unwrap()
                .input
                .update(cx, |input, cx| {
                    input.set_value("Committed for Save As", window, cx)
                });
            v
        });

        cx.simulate_keystrokes("ctrl-shift-s");
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.inline_text.is_none());
            assert_eq!(v.dialog, Dialog::Save);
            assert!(v.editor.is_dirty());
            assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), manifest);
        });

        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.dialog = Dialog::None;
                v.focus.focus(window, cx);
                v.begin_inline_text(Some(id.clone()), (0., 0.), None, window, cx);
                v.inline_text
                    .as_ref()
                    .unwrap()
                    .input
                    .update(cx, |input, cx| input.set_value("Saved once", window, cx));
            })
        });
        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.inline_text.is_none());
            assert_eq!(v.dialog, Dialog::None);
            assert!(!v.editor.is_dirty());
            assert_eq!(
                objects::live_text(document::open(&path).unwrap().find_layer(&id).unwrap())
                    .unwrap()
                    .unwrap()
                    .content,
                "Saved once"
            );
        });
    }

    #[gpui_kit::test]
    fn detailed_text_dialog_rejects_locks_at_open_and_confirm_without_losing_input(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut v = EditorView::new(None, window, cx);
            v.dialog = Dialog::None;
            v.editor = Editor::new(Document::new(160, 120));
            v.editor.import_layer(
                objects::live_text_layer(
                    "Editable",
                    objects::ObjectPoint { x: 12., y: 18. },
                    objects::LiveTextStyle {
                        content: "Original".into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
            );
            let id = v.editor.active_layer.clone();
            assert!(v.editor.set_locked(&id, true));
            v.command("edit-object", window, cx);
            assert_eq!(v.dialog, Dialog::None);
            assert!(v.status.contains("Unlock"));
            assert!(v.editor.set_locked(&id, false));
            v.command("edit-object", window, cx);
            assert_eq!(v.dialog, Dialog::Text);
            v.path_input.update(cx, |input, cx| {
                input.set_value("Retain this draft", window, cx)
            });
            assert!(v.editor.set_locked(&id, true));
            v.confirm_dialog(window, cx);
            assert_eq!(v.dialog, Dialog::Text);
            assert_eq!(v.path_input.read(cx).value().as_ref(), "Retain this draft");
            assert!(v.status.contains("Unlock"));
            assert_eq!(
                objects::live_text(v.editor.document.find_layer(&id).unwrap())
                    .unwrap()
                    .unwrap()
                    .content,
                "Original"
            );
            v
        });
        cx.update(|_, cx| assert_eq!(view.read(cx).dialog, Dialog::Text));
    }
}

fn filter_spec(kind: usize) -> (&'static str, [&'static str; 3], [f32; 3]) {
    match kind {
        0 => (
            "Exposure",
            ["Exposure (stops −10 to 10)", "", ""],
            [0.5, 0., 0.],
        ),
        1 => (
            "Levels",
            ["Black point (0–1)", "White point (0–1)", "Gamma (0.1–10)"],
            [0., 1., 1.],
        ),
        2 => ("Curves", ["Midpoint output (0–1)", "", ""], [0.6, 0., 0.]),
        3 => (
            "HSL",
            [
                "Hue rotation (degrees)",
                "Saturation shift (−1 to 1)",
                "Lightness shift (−1 to 1)",
            ],
            [0., 0.1, 0.],
        ),
        4 => (
            "Color balance",
            [
                "Red shift (−1 to 1)",
                "Green shift (−1 to 1)",
                "Blue shift (−1 to 1)",
            ],
            [0.05, 0., 0.],
        ),
        5 => ("Blur", ["Radius (pixels)", "", ""], [2., 0., 0.]),
        6 => (
            "Sharpen",
            ["Radius (pixels)", "Amount", "Threshold (0–1)"],
            [1., 1., 0.05],
        ),
        7 => ("Noise", ["Amount (0–1)", "", ""], [0.1, 0., 0.]),
        8 => (
            "Vignette",
            ["Amount (0–1)", "Midpoint (0–1)", "Feather (0–1)"],
            [0.5, 0.5, 0.5],
        ),
        9 => (
            "Bloom",
            ["Radius (pixels)", "Amount", "Threshold (0–1)"],
            [8., 0.5, 0.7],
        ),
        _ => (
            "Tonal contrast",
            [
                "Shadows (−1 to 1)",
                "Midtones (−1 to 1)",
                "Highlights (−1 to 1)",
            ],
            [0.1, 0.1, 0.1],
        ),
    }
}
fn make_filter(kind: usize, v: &[f32]) -> Filter {
    match kind {
        0 => Filter::Exposure { stops: v[0] },
        1 => Filter::Levels {
            black: v[0],
            white: v[1],
            gamma: v[2],
        },
        2 => Filter::Curves {
            points: vec![(0., 0.), (0.5, v[0]), (1., 1.)],
        },
        3 => Filter::Hsl {
            hue_degrees: v[0],
            saturation: v[1],
            lightness: v[2],
        },
        4 => Filter::ColorBalance {
            red: v[0],
            green: v[1],
            blue: v[2],
        },
        5 => Filter::GaussianBlur { sigma: v[0] },
        6 => Filter::UnsharpMask {
            sigma: v[0],
            amount: v[1],
            threshold: v[2],
        },
        7 => Filter::Noise {
            amount: v[0],
            seed: 42,
            monochrome: true,
        },
        8 => Filter::Vignette {
            amount: v[0],
            midpoint: v[1],
            feather: v[2],
        },
        9 => Filter::Bloom {
            sigma: v[0],
            amount: v[1],
            threshold: v[2],
        },
        _ => Filter::TonalContrast {
            shadows: v[0],
            midtones: v[1],
            highlights: v[2],
        },
    }
}

#[derive(Clone)]
struct MaskDrag {
    id: String,
}
impl Render for MaskDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(cx.omarchy().selection)
            .child("Copy mask")
    }
}
#[derive(Clone)]
struct EffectDrag {
    id: String,
    kind: String,
}
impl Render for EffectDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(cx.omarchy().selection)
            .child(format!("Copy {}", self.kind))
    }
}
#[derive(Clone)]
struct LayerDrag {
    ids: Vec<String>,
    name: String,
}
impl Render for LayerDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(cx.omarchy().selection)
            .text_color(cx.omarchy().foreground)
            .border_1()
            .border_color(cx.omarchy().accent)
            .child(self.name.clone())
    }
}

fn project_stamp(path: &std::path::Path) -> Option<u64> {
    omuse::save_guard::package_stamp(path)
}

impl EditorView {
    /// A synthetic, externally captured display comparison; never opens a user
    /// document and is reachable only from the explicit --ui-smoke entry point.
    fn start_native_display_probe(
        &mut self,
        dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recovery = Recovery::at(dir.join("recovery"));
        self.dialog = Dialog::None;
        self.path = None;
        self.show_grid = false;
        self.show_guides = false;
        self.preferences.rulers = false;
        self.tool = Tool::Brush;
        let source = image::RgbaImage::from_fn(1025, 769, |x, y| {
            if (96..144).contains(&(y % 256)) {
                image::Rgba([208, 93, 173, 175])
            } else {
                image::Rgba([
                    [55, 127, 209, 245][((x / 64) % 4) as usize],
                    [25, 191, 87, 225][((y / 64) % 4) as usize],
                    if (x / 8 + y / 8) % 2 == 0 { 40 } else { 220 },
                    if (x / 128 + y / 128) % 2 == 0 {
                        255
                    } else {
                        160
                    },
                ])
            }
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = async {
                use anyhow::{Context, ensure};
                std::fs::create_dir_all(&dir)?;
                source.save(dir.join("source.png"))?;
                std::fs::write(dir.join("window-ready"), b"ready")?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                while !dir.join("start").exists() {
                    ensure!(std::time::Instant::now() < deadline, "display probe did not receive start");
                    cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                }
                for index in 0..30 {
                    let zoom = [0.37, 1., 1.25, 2., 1.25][(index % 10) / 2];
                    let reference = index % 2 == 1;
                    let edited = index % 10 >= 8;
                    let matte = ["checkerboard", "black", "white"][index / 10];
                    view.update_in(cx, |this, window, cx| {
                        this.display_reference = None;
                        this.display_probe_matte = match index / 10 {
                            1 => Some(0x000000),
                            2 => Some(0xffffff),
                            _ => None,
                        };
                        if !reference {
                            let mut doc = Document::new(source.width(), source.height());
                            doc.layers[0].image = Some(source.clone().into());
                            this.editor = Editor::new(doc);
                            this.refresh(cx);
                        }
                        this.zoom = zoom;
                        this.pan = (0.25, -0.375);
                        this.status = "Synthetic display comparison".into();
                        if edited && !reference {
                            this.editor.brush.size = 7.;
                            this.editor.brush.color = [5, 240, 17, 255];
                            this.editor.begin_stroke(256., 256., 1., PaintTool::Pencil);
                            this.editor.continue_stroke(267., 264., 1.);
                            this.queue_stroke_frame(window, cx);
                        }
                        if reference {
                            // Keep this oracle independent of display tile
                            // construction, including channel conversion.
                            let (width, height) = this.pixels.dimensions();
                            let padded = image::RgbaImage::from_fn(width + 2, height + 2, |x, y| {
                                *this.pixels.get_pixel(x.saturating_sub(1).min(width - 1), y.saturating_sub(1).min(height - 1))
                            });
                            this.display_reference = Some(render_image(&padded));
                        }
                        cx.notify();
                    })?;
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                    while view.update(cx, |this, _| this.pending_stroke_frame.is_some())? {
                        ensure!(std::time::Instant::now() < deadline, "display probe stroke frame timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                    }
                    view.update(cx, |this, _| { this.editor.finish_stroke(); })?;
                    cx.update(|window, cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                    cx.background_executor().timer(std::time::Duration::from_millis(180)).await;
                    let metadata = view.update_in(cx, |this, window, _| {
                        let b = this.viewport.get();
                        serde_json::json!({
                            "index": index, "mode": if reference { "reference" } else { "tiled" },
                            "zoom": zoom, "edited": edited, "matte": matte, "scale_factor": window.scale_factor(),
                            "viewport": { "x": f32::from(b.origin.x), "y": f32::from(b.origin.y), "width": f32::from(b.size.width), "height": f32::from(b.size.height) },
                            "canvas": { "x": f32::from(b.origin.x)+(f32::from(b.size.width)-source.width() as f32*zoom)/2.+this.pan.0, "y": f32::from(b.origin.y)+(f32::from(b.size.height)-source.height() as f32*zoom)/2.+this.pan.1, "width": source.width() as f32*zoom, "height": source.height() as f32*zoom },
                            "resident_images": this.canvas_textures.borrow().images.len(),
                            "source_dimensions": [source.width(), source.height()],
                            "comparison_reference": "monolithic_with_clamped_outer_halo"
                        })
                    })?;
                    let ready = dir.join(format!("case-{index}-ready.json"));
                    let pending = dir.join(format!("case-{index}-ready.json.tmp"));
                    std::fs::write(&pending, serde_json::to_vec_pretty(&metadata)?)?;
                    std::fs::rename(pending, ready)?;
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !dir.join(format!("case-{index}-captured")).exists() {
                        ensure!(std::time::Instant::now() < deadline, "display probe case {index} capture timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(25)).await;
                    }
                }
                std::fs::write(dir.join("display-results.json"), serde_json::to_vec_pretty(&serde_json::json!({"status":"passed","cases":30}))?).context("write display probe results")?;
                Ok::<(), anyhow::Error>(())
            }.await;
            if let Err(error) = result {
                let _ = std::fs::write(dir.join("native-error.txt"), format!("{error:#}"));
                eprintln!("Native display probe failed: {error:#}");
            }
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            let _ = cx.update(|_, cx| cx.quit());
        }).detach();
    }

    pub fn start_native_journey(
        &mut self,
        dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if omuse::identity::env_var("OMUSE_NATIVE_AI_TEST").as_deref() == Ok("1") {
            self.start_native_ai_qualification(dir, window, cx);
            return;
        }
        if omuse::identity::env_var("OMUSE_NATIVE_DISPLAY_PROBE").as_deref() == Ok("1") {
            self.start_native_display_probe(dir, window, cx);
            return;
        }
        self.recovery = Recovery::at(dir.join("recovery"));
        self.dialog = Dialog::None;
        self.path = None;
        self.editor = Editor::new(Document::new(640, 480));
        self.editor.brush.color = [238, 88, 107, 255];
        self.editor.brush.size = 28.;
        self.zoom = 1.;
        self.refresh(cx);
        cx.spawn_in(window,async move|view,cx|{
            let result=async{
                use anyhow::{ensure,Context};
                std::fs::create_dir_all(&dir)?;
                cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
                if omuse::identity::env_var_os("OMUSE_NATIVE_WAIT").is_some(){
                    std::fs::write(dir.join("window-ready"),b"ready")?;
                    let mut waited=0;
                    while !dir.join("start").exists(){ensure!(waited<100,"native harness did not focus window");cx.background_executor().timer(std::time::Duration::from_millis(100)).await;waited+=1;}
                    cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
                }
                cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx)})?;
                ensure!(view.update(cx,|this,_|{let b=this.viewport.get();b.size.width>px(20.)&&b.size.height>px(20.)})?,"window has no usable canvas area");
                let (a,b)=view.update(cx,|this,_|{let b=this.viewport.get();let dx=px((f32::from(b.size.width)*0.2).min(120.));let dy=px((f32::from(b.size.height)*0.2).min(60.));(b.center()-point(dx,dy),b.center()+point(dx,dy))})?;
                let trace=view.update(cx,|this,_|serde_json::json!({"viewport":format!("{:?}",this.viewport.get()),"active_layer":this.editor.active_layer,"dialog_open":this.dialog!=Dialog::None,"from":format!("{:?}",a),"to":format!("{:?}",b)}))?;
                std::fs::write(dir.join("geometry.json"),serde_json::to_vec_pretty(&trace)?)?;
                let native_bounds=cx.update(|window,_|format!("window={:?}, viewport={:?}",window.bounds(),window.viewport_size()))?;
                std::fs::write(dir.join("window-bounds.txt"),native_bounds)?;
                cx.update(|window,cx|{window.dispatch_event(gpui_kit::PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position:a,modifiers:Default::default(),click_count:1,first_mouse:false}),cx);})?;
                for step in 1..=20{let p=a+(b-a)*(step as f32/20.);cx.update(|window,cx|{window.dispatch_event(gpui_kit::PlatformInput::MouseMove(MouseMoveEvent{position:p,pressed_button:Some(MouseButton::Left),modifiers:Default::default()}),cx);})?;}
                // Exercise the real display-frame callback before mouse-up's
                // final full refresh can hide a missing or stale preview.
                let preview_deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
                while view.update(cx,|this,_|this.pending_stroke_frame.is_some())? {
                    ensure!(std::time::Instant::now()<preview_deadline,"native stroke preview did not reach a display frame");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|this.pixels.pixels().any(|p|p[3]>0)&&this.pixels==raster::composite(&this.editor.document))?,"native in-progress stroke preview differs from complete render");
                cx.update(|window,cx|{window.dispatch_event(gpui_kit::PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:b,modifiers:Default::default(),click_count:1}),cx);})?;
                let painted=view.update(cx,|this,_|this.pixels.clone())?;ensure!(painted.pixels().any(|p|p[3]>0),"native pointer painting produced no pixels");
                cx.update(|window,cx|window.dispatch_action(Box::new(Undo),cx))?;
                ensure!(view.update(cx,|this,_|this.pixels.pixels().all(|p|p[3]==0))?,"native undo did not clear stroke");
                cx.update(|window,cx|window.dispatch_action(Box::new(Redo),cx))?;
                ensure!(view.update(cx,|this,_|this.pixels==painted)?,"native redo pixel mismatch");
                cx.update(|window,cx|window.dispatch_action(Box::new(New),cx))?;
                ensure!(view.update(cx,|this,_|this.dialog==Dialog::Unsaved)?,"native new must prompt before replacing document");
                view.update_in(cx,|this,window,cx|{this.dialog=Dialog::None;this.pending=None;this.focus.focus(window,cx);})?;
                let project=dir.join("Native.omuse");
                view.update_in(cx,|this,window,cx|this.save_to(project.clone(),window,cx))?;
                let save_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.create.saving)? {
                    ensure!(std::time::Instant::now()<save_deadline,"native save timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|!this.has_unsaved_work())?,"native save left unsaved work: {}",view.update(cx,|this,_|this.status.clone())?);
                ensure!(project.join("manifest.json").is_file(),"native save did not produce project");
                ensure!(raster::composite(&document::open(&project)?)==painted,"native reopen differs");
                // Only this synthetic window's explicit theme is changed; desktop theme is untouched.
                cx.update(|_,cx|gpui_omarchy::Theme::flexoki_light().apply(cx))?;
                ensure!(view.update(cx,|this,_|this.pixels==painted)?,"light theme modified artwork");
                cx.update(|_,cx|gpui_omarchy::Theme::follow_system(cx))?;
                view.update_in(cx,|this,window,cx|->anyhow::Result<()>{
                    this.begin_inline_text(None,(32.,32.),None,window,cx);
                    this.inline_text.as_ref().unwrap().input.update(cx,|s,cx|s.set_value("Editable native text",window,cx));
                    ensure!(this.finish_inline_text(true,window,cx),"native inline insert: {}",this.status);
                    ensure!(this.dialog==Dialog::None,"native text insertion: {}",this.status);
                    this.begin_inline_text(Some(this.editor.active_layer.clone()),(0.,0.),None,window,cx);
                    this.inline_text.as_ref().unwrap().input.update(cx,|s,cx|s.set_value("Edited native text",window,cx));
                    ensure!(this.finish_inline_text(true,window,cx),"native inline edit: {}",this.status);
                    ensure!(this.dialog==Dialog::None,"native text editing: {}",this.status);
                    let text_id=this.editor.active_layer.clone();
                    ensure!(objects::live_text(this.editor.document.find_layer(&text_id).unwrap())?.unwrap().content=="Edited native text","text remains editable");
                    this.command("live-adjustment",window,cx);
                    this.open_adjustment(10,None,window,cx);
                    this.confirm_dialog(window,cx);
                    ensure!(this.dialog==Dialog::None,"native adjustment insertion: {}",this.status);
                    this.command("edit-adjustment",window,cx);
                    ensure!(this.dialog==Dialog::Adjustment,"native adjustment reopening");
                    this.confirm_dialog(window,cx);
                    ensure!(this.dialog==Dialog::None,"native adjustment apply: {}",this.status);
                    this.editor.select_layer(&text_id);
                    this.command("effects",window,cx);
                    this.confirm_dialog(window,cx);
                    ensure!(this.dialog==Dialog::None,"native effects: {}",this.status);
                    this.save_to(dir.join("NativeParity.omuse"),window,cx);
                    Ok(())
                })??;
                let save_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.create.saving)? {
                    ensure!(std::time::Instant::now()<save_deadline,"native save timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|!this.has_unsaved_work())?,"native save left unsaved work: {}",view.update(cx,|this,_|this.status.clone())?);
                ensure!(raster::composite(&document::open(&dir.join("NativeParity.omuse"))?)==view.update(cx,|this,_|this.pixels.clone())?,"native live-document reopen differs");
                // Exercise both asynchronous range tools against known tones.
                view.update_in(cx, |this, window, cx| {
                    let mut doc = Document::new(128,64);
                    doc.layers[0].image = Some(image::RgbaImage::from_fn(128,64,|x,_| {
                        let v = [0,128,200,255][x as usize / 32]; image::Rgba([v,v,v,255])
                    }).into());
                    this.editor = Editor::new(doc); this.refresh(cx);
                    this.command("luminosity-range",window,cx);
                })?;
                let deadline = std::time::Instant::now()+std::time::Duration::from_secs(5);
                while !view.update(cx,|this,_|this.range_preview_ready())? {
                    ensure!(std::time::Instant::now()<deadline,"native luminosity preview timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                view.update_in(cx, |this, window, cx| -> anyhow::Result<()> {
                    this.confirm_dialog(window,cx);
                    ensure!(this.dialog==Dialog::None,"native range apply: {}",this.status);
                    let mask=&this.editor.selection.as_ref().context("range selection missing")?.mask;
                    ensure!(mask[0]==0 && mask[40]>0 && mask[40]<255 && mask[127]==255,"native luminosity coverage mismatch");
                    ensure!(this.editor.undo_depth()==1 && !this.editor.is_dirty(),"range selection changed document history");
                    ensure!(this.editor.undo() && this.editor.selection.is_none(),"range undo failed");
                    ensure!(this.editor.redo() && this.editor.selection.is_some(),"range redo failed");
                    this.editor.brush.color=[255,255,255,255];
                    this.command("color-range",window,cx);
                    Ok(())
                })??;
                let deadline = std::time::Instant::now()+std::time::Duration::from_secs(5);
                while !view.update(cx,|this,_|this.range_preview_ready())? {
                    ensure!(std::time::Instant::now()<deadline,"native colour preview timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                view.update_in(cx, |this, window, cx| -> anyhow::Result<()> {
                    let source=this.editor.document.layers[0].image.clone();
                    this.set_range_output(range_ui::RangeOutput::LayerMask,cx);
                    this.confirm_dialog(window,cx);
                    ensure!(this.dialog==Dialog::None,"native colour mask apply: {}",this.status);
                    let layer=&this.editor.document.layers[0];
                    ensure!(layer.image==source,"range mask changed source pixels");
                    let mask=layer.mask.as_ref().context("range layer mask missing")?;
                    ensure!(mask.get_pixel(0,0)[0]==0 && mask.get_pixel(127,0)[0]==255,"native colour range mismatch");
                    this.save_to(dir.join("NativeRange.omuse"),window,cx);
                    Ok(())
                })??;
                let save_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.create.saving)? {
                    ensure!(std::time::Instant::now()<save_deadline,"native save timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|!this.has_unsaved_work())?,"native save left unsaved work: {}",view.update(cx,|this,_|this.status.clone())?);
                ensure!(raster::composite(&document::open(&dir.join("NativeRange.omuse"))?)==view.update(cx,|this,_|this.pixels.clone())?,"range mask save/reopen mismatch");
                view.update(cx,|this,_|{
                    this.install_opened_content(Document::new(640,480),None);
                    this.path=None;this.live_stamp=None;
                })?;
                // Exercise an editable source through actual window-bound
                // workspaces, their workers, history, persistence and export.
                let master_path = dir.join("NativeMaster16.png");
                let master = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(64, 32, |x,y| image::Rgba([1001 + x as u16 * 701, 12345 + y as u16 * 911, 43210, 65535]));
                image::DynamicImage::ImageRgba16(master.clone()).save(&master_path)?;
                view.update_in(cx, |this, window, cx| {
                    this.open_workflow(workflow_ui::WorkflowKind::Source, window, cx);
                    this.path_input.update(cx, |s,cx| s.set_value(master_path.to_string_lossy(), window, cx));
                    this.confirm_dialog(window,cx);
                })?;
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
                while view.update(cx,|this,_|this.busy)? {
                    ensure!(std::time::Instant::now()<deadline,"editable import timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                view.update_in(cx,|this,window,cx| -> anyhow::Result<()> {
                    ensure!(this.dialog==Dialog::None,"editable import: {}",this.status);
                    let state = this.editor.document.find_layer(&this.editor.active_layer).and_then(|l|l.advanced.as_ref()).context("editable import missing original")?;
                    ensure!(state.source.to_rgba16()==master,"native import lost 16-bit samples");
                    this.open_pro(advanced_ui::Kind::Stack,window,cx);
                    this.detail_inputs[0].update(cx,|s,cx|s.set_value("0.5",window,cx));
                    this.pro_add_node(false,cx);
                    Ok(())
                })??;
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
                while view.update(cx,|this,_|this.busy)? {
                    ensure!(std::time::Instant::now()<deadline,"editable preview timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                view.update_in(cx,|this,window,cx|this.confirm_dialog(window,cx))?;
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
                while view.update(cx,|this,_|this.busy)? {
                    ensure!(std::time::Instant::now()<deadline,"editable Apply timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                view.update_in(cx,|this,window,cx| -> anyhow::Result<()> {
                    ensure!(this.dialog==Dialog::None,"editable stack Apply: {}",this.status);
                    let state = this.editor.document.find_layer(&this.editor.active_layer).and_then(|l|l.advanced.as_ref()).context("editable stack missing")?;
                    ensure!(state.recipe.nodes.len()==1 && state.source.to_rgba16()==master,"filter stack lost original or operation");
                    this.save_to(dir.join("NativeEditable.omuse"),window,cx);
                    Ok(())
                })??;
                let save_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.create.saving)? {
                    ensure!(std::time::Instant::now()<save_deadline,"native save timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|!this.has_unsaved_work())?,"native save left unsaved work: {}",view.update(cx,|this,_|this.status.clone())?);
                view.update_in(cx,|this,_window,_cx| -> anyhow::Result<()> {
                    let before = raster::composite16(&this.editor.document)?;
                    let reopened=document::open(&dir.join("NativeEditable.omuse"))?;
                    ensure!(raster::composite16(&reopened)?==before,"16-bit reopen differed");
                    let exported=dir.join("NativeEditable16.png");
                    raster::export16(&reopened,&exported)?;
                    ensure!(image::open(exported)?.to_rgba16()==before,"16-bit export differed");
                    ensure!(this.editor.undo(),"editable stack undo missing");
                    ensure!(this.editor.document.find_layer(&this.editor.active_layer).and_then(|l|l.advanced.as_ref()).is_some_and(|s|s.recipe.nodes.is_empty()),"editable undo did not restore recipe");
                    this.editor=Editor::new(Document::new(640,480));this.path=None;this.live_stamp=None;
                    Ok(())
                })??;
                // Exercise the actual asynchronous file-dialog route on a
                // deterministic photo raster before leaving the showcase open.
                let photo_path=dir.join("NativePhoto.png");
                let photo=image::RgbaImage::from_fn(96,64,|x,y|image::Rgba([(x*2)as u8,(y*3)as u8,91,255]));
                photo.save(&photo_path)?;
                view.update_in(cx,|this,window,cx|{
                    this.dialog=Dialog::Open;
                    this.path_input.update(cx,|input,cx|input.set_value(photo_path.to_string_lossy().to_string(),window,cx));
                    this.confirm_dialog(window,cx);
                })?;
                let photo_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.photo_io.is_some())? {
                    ensure!(std::time::Instant::now()<photo_deadline,"native photo open timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(view.update(cx,|this,_|this.dialog==Dialog::None&&this.pixels==photo)?,"native photo import differs");
                let photo_export=dir.join("NativePhotoEdited.png");
                let edited=view.update_in(cx,|this,window,cx|->anyhow::Result<_>{
                    ensure!(this.editor.adjust(Adjustment::Brightness(0.05)),"native photo adjustment");
                    ensure!(this.editor.crop_canvas(4,4,80,52),"native photo crop");
                    ensure!(this.editor.resize_image(64,40),"native photo resize");
                    let expected=raster::composite(&this.editor.document);
                    ensure!(this.editor.undo()&&this.editor.redo(),"native photo undo/redo");
                    ensure!(raster::composite(&this.editor.document)==expected,"native photo redo differs");
                    this.changed(cx);
                    this.save_dialog(true,window,cx);
                    this.path_input.update(cx,|input,cx|input.set_value(photo_export.to_string_lossy().to_string(),window,cx));
                    this.confirm_dialog(window,cx);
                    Ok(expected)
                })??;
                let export_deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                while view.update(cx,|this,_|this.photo_io.is_some())? {
                    ensure!(std::time::Instant::now()<export_deadline,"native photo export timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                }
                ensure!(image::open(&photo_export)?.to_rgba8()==edited,"native photo export differs");
                // Exercise the actual keymap and modal routing in the native window.
                // These are in-process GPUI events, not compositor-injected input.
                view.update_in(cx, |this, window, cx| {
                    this.install_opened_content(Document::new(32,24), None);
                    this.path = None;
                    this.dialog = Dialog::None;
                    this.focus.focus(window, cx);
                    this.refresh(cx);
                })?;
                cx.update(|window,cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                cx.update(|window,cx| window.dispatch_event(gpui_kit::PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse("ctrl-k").unwrap(), is_held: false, prefer_character_input: false,
                }),cx))?;
                cx.update(|window,cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                view.update_in(cx, |this, window, cx| -> anyhow::Result<()> {
                    ensure!(this.dialog == Dialog::CommandSearch, "command search did not open from Ctrl+K");
                    this.native_command_query("new layer", window, cx);
                    Ok(())
                })??;
                cx.update(|window,cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                cx.update(|window,cx| window.dispatch_event(gpui_kit::PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse("enter").unwrap(), is_held: false, prefer_character_input: false,
                }),cx))?;
                view.update_in(cx, |this, window, _| -> anyhow::Result<()> {
                    ensure!(this.dialog == Dialog::None && this.editor.document.layers.len() == 2 && this.editor.undo_depth() == 1, "command search must run New layer exactly once");
                    ensure!(this.focus.is_focused(window), "command search did not restore canvas focus");
                    Ok(())
                })??;
                cx.update(|window,cx| { window.refresh(); window.draw(cx).clear(cx); })?;
                cx.update(|window,cx| window.dispatch_event(gpui_kit::PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse("ctrl-z").unwrap(), is_held: false, prefer_character_input: false,
                }),cx))?;
                ensure!(view.update(cx, |this,_| this.editor.document.layers.len() == 1)?, "keyboard undo did not reverse palette command");
                view.update_in(cx,|this,_window,_cx|{
                    this.install_opened_content(Document::new(640,480),None);
                    this.path=None;this.live_stamp=None;this.dialog=Dialog::None;
                })?;
                view.update_in(cx,|this,window,cx| -> anyhow::Result<()> {
                    let w=this.editor.document.width;let h=this.editor.document.height;
                    for(shape,color)in[(Shape::Ellipse{x:70.,y:75.,width:180.,height:180.},[122,162,247,255]),(Shape::Rectangle{x:310.,y:140.,width:230.,height:190.},[158,206,106,255])]{if let Ok(layer)=objects::shape_layer("Shape",w,h,shape,color,None){this.editor.import_layer(layer);}}
                    let text=TextStyle{size:42.,color:[40,48,72,255],x:62.,y:365.,..Default::default()};if let Ok(layer)=objects::text_layer("Title",w,h,"Omuse",&text){this.editor.import_layer(layer);}
                    this.status="Native desktop acceptance passed — synthetic artwork".into();this.refresh(cx);this.command("fit",window,cx);
                    if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("text") {
                        this.begin_inline_text(None,(80.,110.),Some(objects::ObjectSize { width: 320., height: 160. }),window,cx);
                        this.inline_text.as_ref().unwrap().input.update(cx,|s,cx|s.set_value("Edit directly on canvas\nOmuse on Linux",window,cx));
                    } else if let Ok(panel) = omuse::identity::env_var("OMUSE_NATIVE_PANEL") {
                        if panel == "image-trace" {
                            let pixels=image::load_from_memory(include_bytes!("../assets/omuse.png"))?.to_rgba8();
                            let mut doc=Document::new(pixels.width(),pixels.height());
                            doc.layers[0].name="Muse artwork".into();
                            doc.layers[0].image=Some(pixels.into());
                            this.editor=Editor::new(doc);
                            this.select_layer_ids(vec![this.editor.active_layer.clone()]);
                            this.refresh(cx);
                            this.command("fit",window,cx);
                            this.command("image-trace",window,cx);
                        } else if panel == "jpeg-preview" {
                            let pixels = image::load_from_memory(include_bytes!("../assets/omuse.png"))?.to_rgba8();
                            let mut doc=Document::new(pixels.width(),pixels.height());doc.layers[0].image=Some(pixels.into());
                            this.editor=Editor::new(doc);this.refresh(cx);this.command("fit",window,cx);
                            this.save_dialog(true,window,cx);
                            this.path_input.update(cx,|input,cx|input.set_value("Omuse preview.jpg",window,cx));
                        } else if panel == "crop" {
                            this.command("crop",window,cx);
                            if let Some(crop)=&mut this.crop { crop.set_preset(3); }
                        } else if matches!(panel.as_str(), "commands" | "shortcuts") {
                            this.command(if panel == "commands" { "command-search" } else { "shortcuts" }, window, cx);
                            if panel == "commands" { this.native_command_query("mask", window, cx); }
                        } else if matches!(panel.as_str(), "create" | "templates" | "assistant" | "content-export" | "motion") {
                            this.prepare_create_inspection(&panel,cx)?;
                            this.command("fit",window,cx);
                        } else if matches!(panel.as_str(), "dither" | "bloom-glow" | "vignette-overlay" | "local-contrast") {
                            let pixels=this.pixels.clone();
                            let mut doc=Document::new(pixels.width(),pixels.height());
                            doc.layers[0].image=Some(pixels.into());
                            this.editor=Editor::new(doc);
                            this.command(&panel,window,cx);
                            this.run_finishing(false,cx);
                        } else if matches!(panel.as_str(), "filter-stack" | "target-colour" | "reference-match" | "blend-if" | "advanced-retouch" | "controlled-removal" | "editable-warp" | "refine-workspace" | "brush-studio" | "smart-source" | "colour-management" | "automation" | "multi-image" | "vector-path" | "vector-scene" | "vector-styles" | "vector-text" | "vector-mask") {
                            let pixels=this.pixels.clone();
                            let mut doc=Document::new(pixels.width(),pixels.height());doc.layers[0].image=Some(pixels.into());
                            this.editor=Editor::new(doc);
                            if matches!(panel.as_str(),"controlled-removal"|"refine-workspace") {this.editor.select_rectangle(120.,100.,160.,180.);}
                            this.command(if matches!(panel.as_str(),"target-colour"|"reference-match") {"filter-stack"} else if matches!(panel.as_str(),"vector-styles"|"vector-text") {"vector-scene"} else {&panel},window,cx);
                            if panel=="target-colour" {this.pro_choose_effect(14,window,cx);}
                            if panel=="reference-match" {
                                this.pro_choose_effect(15,window,cx);
                                let reference=dir.join("Reference-palette.png");
                                let mut image=image::load_from_memory(include_bytes!("../assets/omuse.png"))?.to_rgba8();
                                for p in image.pixels_mut() { p[0]=p[0].saturating_add(30);p[2]=p[2].saturating_add(65); }
                                image.save(&reference)?;this.pro_load_reference(reference,cx);
                            }
                            if panel=="vector-path" {this.prepare_vector_inspection(window,cx)?;}
                            if panel=="vector-scene" {this.prepare_vector_scene_inspection(window,cx)?;}
                            if panel=="vector-styles" {this.prepare_vector_styles_inspection(window,cx)?;}
                            if panel=="vector-text" {this.prepare_vector_text_inspection(window,cx)?;}
                            if matches!(panel.as_str(),"filter-stack"|"target-colour") {this.pro_add_node(false,cx);} else if this.dialog==Dialog::Pro && panel!="reference-match" {this.run_pro(false,cx);}
                        } else if matches!(panel.as_str(), "luminosity-range" | "color-range" | "hue-range") {
                            this.inspector_tab=studio_ui::InspectorTab::Selection;
                            if matches!(panel.as_str(),"color-range"|"hue-range") { this.editor.brush.color=[122,162,247,255]; }
                            this.command(if panel=="hue-range" {"color-range"}else{&panel},window,cx);
                            if panel=="hue-range" {this.set_range_hue_mode(true,window,cx);}
                        } else if panel == "colour-picker" {
                            this.inspector_tab = studio_ui::InspectorTab::Layers;
                            this.tool = Tool::Brush;
                            this.color.update(cx, |state, cx| state.set_open(true, cx));
                        } else if matches!(panel.as_str(), "layers" | "develop" | "selection" | "canvas") {
                            this.inspector_tab = match panel.as_str() {
                                "develop" => studio_ui::InspectorTab::Develop,
                                "selection" => studio_ui::InspectorTab::Selection,
                                "canvas" => studio_ui::InspectorTab::Canvas,
                                _ => studio_ui::InspectorTab::Layers,
                            };
                            this.tool = Tool::Brush;
                        } else {
                        let pixels = this.pixels.clone();
                        let mut doc = Document::new(pixels.width(), pixels.height());
                        doc.layers[0].image = Some(pixels.into());
                        this.editor = Editor::new(doc);
                        this.command("camera-raw",window,cx);
                        this.camera_section = match panel.as_str() { "curves" => 2, "mixer" => 3, "geometry" => 7, _ => 0 };
                        this.load_camera_form(window,cx);
                        cx.notify();
                        }
                    }
                    match omuse::identity::env_var("OMUSE_NATIVE_THEME").ok().as_deref() {
                        Some("light") => gpui_omarchy::Theme::flexoki_light().apply(cx),
                        Some("dark") => gpui_omarchy::Theme::tokyo_night().apply(cx),
                        _ => {},
                    }
                    Ok(())
                })??;
                cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx)})?;
                if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("jpeg-preview") {
                    cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                    view.update(cx,|this,cx|this.start_jpeg_preview(cx))?;
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                    while view.update(cx,|this,_|this.jpeg_preview_task.is_some())? {
                        ensure!(std::time::Instant::now()<deadline,"JPEG inspection preview timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    ensure!(view.update(cx,|this,_|this.jpeg_preview.is_some())?,"JPEG inspection did not produce encoded preview");
                    cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx)})?;
                }
                if view.update(cx,|this,_|this.dialog==Dialog::RangeMask)? {
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
                    while !view.update(cx,|this,_|this.range_preview_ready())? {
                        ensure!(std::time::Instant::now()<deadline,"range inspection preview timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx)})?;
                }
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                while view.update(cx,|this,_|this.busy)? {
                    ensure!(std::time::Instant::now()<deadline,"inspection workspace preview timed out");
                    cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                }
                if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("reference-match") {
                    view.update(cx,|this,cx|this.pro_add_node(false,cx))?;
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                    while view.update(cx,|this,_|this.busy)? {
                        ensure!(std::time::Instant::now()<deadline,"Reference colour preview timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    ensure!(view.update(cx,|this,_|this.status.starts_with("Preview"))?,"Reference colour preview failed: {}",view.update(cx,|this,_|this.status.clone())?);
                }
                if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("image-trace") {
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                    while !view.update(cx,|this,_|this.image_trace_ready())? {
                        ensure!(std::time::Instant::now()<deadline,"image trace preview timed out");
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    view.update(cx,|this,_| {
                        ensure!(this.dialog==Dialog::None,"image trace opened a modal");
                        ensure!(this.image_trace_display().is_some(),"trace composite missing");
                        ensure!(this.editor.undo_depth()==0 && this.editor.document.layers.len()==1,"preview changed the document");
                        Ok::<(),anyhow::Error>(())
                    })??;
                }
                if matches!(omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref(), Some("vector-scene" | "vector-path" | "vector-styles" | "vector-text")) {
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                    while !view.update(cx,|this,_|this.vector_canvas_ready())? {
                        ensure!(std::time::Instant::now()<deadline,"main canvas vector preview timed out: {}", view.update(cx,|this,_|this.status.clone())?);
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    view.update(cx,|this,_| {
                        ensure!(this.dialog==Dialog::None,"vector editing opened a modal");
                        ensure!(this.inspector_tab==studio_ui::InspectorTab::Layers,"vector editing detached from Layers");
                        ensure!(this.vector_canvas_overlay().is_some(),"main canvas vector overlay missing");
                        Ok::<(),anyhow::Error>(())
                    })??;
                }
                if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("templates") {
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(30);
                    while view.update(cx,|this,_|this.create.template_previews.images.len())? < omuse::create::templates().len() {
                        ensure!(std::time::Instant::now()<deadline,"template catalogue previews did not finish");
                        cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                    }
                }
                cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx)})?;
                let mut report=serde_json::json!({"status":"passed","renderer":"GPUI native window","checks":["coalesced in-progress stroke preview","pointer painting","undo","redo","unsaved guard","save","reopen pixel equality","light theme retains artwork","system theme following","inline text insert/edit","live adjustment insert/reopen","live effects","live-document save/reopen","luminosity selection and undo","colour range mask save/reopen","16-bit source import retains exact samples","editable filter preview and Apply","editable source save/reopen and undo","16-bit export pixel equality","background photo open","photo adjustment crop resize and undo redo","background photo export pixel equality","command palette keyboard open search execute","palette command keyboard undo and focus restoration"]});
                if matches!(omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref(), Some("vector-scene" | "vector-path" | "vector-styles" | "vector-text")) {
                    for check in ["vector artwork shares main canvas without modal", "vector inspector shares Layers", "settled vector composite and canvas overlay"] {
                        report["checks"].as_array_mut().unwrap().push(serde_json::json!(check));
                    }
                }
                if omuse::identity::env_var("OMUSE_NATIVE_PANEL").ok().as_deref() == Some("image-trace") {
                    for check in ["image trace shares main canvas without modal", "settled trace composite", "trace preview leaves document and history unchanged"] {
                        report["checks"].as_array_mut().unwrap().push(serde_json::json!(check));
                    }
                }
                std::fs::write(dir.join("native-results.json"),serde_json::to_vec_pretty(&report)?).context("write native report")?;
                println!("Native GUI journey passed: {}",dir.display());
                Ok::<(),anyhow::Error>(())
            }.await;
            if let Err(error)=result{let _=std::fs::write(dir.join("native-error.txt"),format!("{error:#}"));eprintln!("Native GUI journey failed: {error:#}");}
            cx.background_executor().timer(std::time::Duration::from_secs(45)).await;
            let _=cx.update(|_,cx|cx.quit());
        }).detach();
    }
}

fn effect_key(kind: usize) -> &'static str {
    [
        "stroke",
        "shadow",
        "colorOverlay",
        "innerShadow",
        "outerGlow",
        "innerGlow",
    ][kind.min(5)]
}

fn pixel_fingerprint(image: &image::RgbaImage) -> u64 {
    image
        .as_raw()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}
