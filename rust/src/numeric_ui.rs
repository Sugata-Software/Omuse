//! Shared typed numbers and label scrubbing. Artwork drags remain a preview
//! until release, so a long drag adds exactly one editor history entry.
use super::*;
use gpui_kit::{CursorStyle, Div, Focusable, Stateful};
use std::collections::HashMap;
#[path = "camera_numeric.rs"]
mod camera_numeric;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Target {
    BrushSize,
    BrushOpacity,
    BrushHardness,
    BrushSmoothing,
    LayerOpacity,
    Detail(usize),
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, PlatformInput, TestAppContext};

    fn fixture(
        window: &mut Window,
        cx: &mut Context<EditorView>,
        recovery: Recovery,
    ) -> EditorView {
        let mut view = EditorView::new(None, window, cx);
        view.recovery = recovery;
        view.dialog = Dialog::None;
        view.tool = Tool::Brush;
        view.editor = Editor::new(Document::new(12, 8));
        view.editor.document.layers[0].image =
            Some(image::RgbaImage::from_pixel(12, 8, image::Rgba([60, 120, 200, 255])).into());
        view.refresh(cx);
        view
    }

    fn motion(x: f32, held: bool, fine: bool) -> MouseMoveEvent {
        MouseMoveEvent {
            position: point(px(x), px(40.)),
            pressed_button: held.then_some(MouseButton::Left),
            modifiers: Modifiers {
                shift: fine,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[gpui_kit::test]
    fn opacity_scrub_is_a_preview_and_release_creates_one_undo(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        view.update_in(cx, |view, window, cx| {
            let original = view.pixels.clone();
            let source = view.editor.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .as_arc();
            view.begin_numeric_scrub(Target::LayerOpacity, OPACITY, 100., window, cx);
            view.numeric_moved(&motion(98., true, false), window, cx);
            assert_eq!(
                view.pixels, original,
                "two-pixel slack must not edit artwork"
            );
            for x in [80., 65., 50.] {
                view.numeric_moved(&motion(x, true, false), window, cx);
                assert_eq!(view.editor.document.layers[0].opacity, 1.);
                assert_eq!(view.editor.undo_depth(), 0);
            }
            assert_eq!(view.pixels.get_pixel(3, 3)[3], 128);
            assert!(Arc::ptr_eq(
                &source,
                &view.editor.document.layers[0]
                    .image
                    .as_ref()
                    .unwrap()
                    .as_arc()
            ));
            assert!(view.finish_numeric_scrub(cx));
            assert_eq!(view.editor.document.layers[0].opacity, 0.5);
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(
                !view.finish_numeric_scrub(cx),
                "duplicate release must not create history"
            );
            assert!(view.editor.undo());
            view.refresh(cx);
            assert_eq!(view.pixels, original);
            assert!(view.editor.redo());
            assert_eq!(view.editor.document.layers[0].opacity, 0.5);
        });
    }

    #[gpui_kit::test]
    fn cancel_lost_button_and_replaced_targets_never_commit_artwork(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        view.update_in(cx, |view, window, cx| {
            let original = view.pixels.clone();
            for lost_button in [false, true] {
                view.begin_numeric_scrub(Target::LayerOpacity, OPACITY, 100., window, cx);
                view.numeric_moved(&motion(40., true, false), window, cx);
                if lost_button {
                    view.numeric_moved(&motion(41., false, false), window, cx);
                } else {
                    assert!(view.cancel_numeric_scrub(window, cx));
                }
                assert_eq!(view.pixels, original);
                assert_eq!(view.editor.document.layers[0].opacity, 1.);
                assert_eq!(view.editor.undo_depth(), 0);
                assert!(!view.numeric_scrubbing());
            }
            view.begin_numeric_scrub(Target::LayerOpacity, OPACITY, 100., window, cx);
            view.numeric_moved(&motion(25., true, false), window, cx);
            let mut replacement = view.editor.document.clone();
            replacement.layers[0].opacity = 0.8;
            view.editor = Editor::new(replacement); // Same IDs/revision, different editor lifetime.
            view.validate_numeric_context(window, cx);
            assert!(!view.numeric_scrubbing());
            assert_eq!(view.editor.document.layers[0].opacity, 0.8);
            assert_eq!(view.editor.undo_depth(), 0);

            let mut group = Layer::group("Locked parent");
            group.locked = true;
            group.children.push(view.editor.document.layers.remove(0));
            view.editor.document.layers.push(group);
            assert!(!view.numeric_allowed(Target::LayerOpacity));
            view.begin_numeric_scrub(Target::LayerOpacity, OPACITY, 100., window, cx);
            assert!(!view.numeric_scrubbing());
        });
    }

    #[gpui_kit::test]
    fn tool_precision_and_dialog_cancel_keep_their_original_values(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        view.update_in(cx, |view, window, cx| {
            view.begin_numeric_scrub(Target::BrushSize, SIZE, 100., window, cx);
            view.numeric_moved(&motion(110., true, false), window, cx);
            assert_eq!(view.editor.brush.size, 26.);
            view.numeric_moved(&motion(120., true, true), window, cx);
            assert_eq!(
                view.editor.brush.size, 27.,
                "Shift must change subsequent deltas without jumping"
            );
            view.numeric_moved(&motion(130., true, false), window, cx);
            assert_eq!(view.editor.brush.size, 37.);
            assert!(view.cancel_numeric_scrub(window, cx));
            assert_eq!(view.editor.brush.size, 16.);
            assert_eq!(view.editor.undo_depth(), 0);

            view.dialog = Dialog::Transform;
            view.detail_inputs[0].update(cx, |input, cx| input.set_value("1.250", window, cx));
            let spec = Spec::new(-1000., 1000., 1., 0., 3);
            view.begin_numeric_scrub(Target::Detail(0), spec, 100., window, cx);
            view.numeric_moved(&motion(120., true, false), window, cx);
            assert_eq!(view.detail_inputs[0].read(cx).value().as_ref(), "21.25");
            assert_eq!(view.editor.document.layers[0].offset_x, 0.);
            view.cancel_numeric_scrub(window, cx);
            assert_eq!(view.detail_inputs[0].read(cx).value().as_ref(), "1.250");

            view.begin_numeric_scrub(Target::Detail(0), spec, 100., window, cx);
            view.numeric_moved(&motion(110., true, false), window, cx);
            view.dialog_generation += 1;
            view.detail_inputs[0].update(cx, |input, cx| input.set_value("88", window, cx));
            view.validate_numeric_context(window, cx);
            assert_eq!(
                view.detail_inputs[0].read(cx).value().as_ref(),
                "88",
                "stale scrub cannot restore an old dialog's field"
            );
            assert_eq!(view.editor.undo_depth(), 0);
            view.dialog = Dialog::Adjustment;
            view.open_adjustment(0, None, window, cx);
            view.begin_numeric_scrub(Target::Detail(0), spec, 100., window, cx);
            view.numeric_moved(&motion(110., true, false), window, cx);
            view.open_adjustment(6, None, window, cx);
            view.validate_numeric_context(window, cx);
            assert!(!view.numeric_scrubbing());
            assert_eq!(
                view.detail_inputs[0].read(cx).value().as_ref(),
                "10",
                "changing adjustment kind cannot restore the previous field"
            );
        });
    }

    #[gpui_kit::test]
    fn visible_layer_label_drag_cannot_paint_canvas_and_escape_discards_preview(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        cx.simulate_resize(size(px(1400.), px(1000.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let label = cx
            .debug_bounds("numeric-label-layer-opacity")
            .unwrap()
            .center();
        cx.simulate_mouse_down(label, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            point(label.x - px(30.), label.y),
            MouseButton::Left,
            Modifiers::default(),
        );
        view.update(cx, |view, _| {
            assert!(view.numeric_scrubbing());
            assert!(view.pixels.get_pixel(3, 3)[3] < 255);
            assert_eq!(view.editor.undo_depth(), 0);
            assert_eq!(
                view.editor.document.layers[0]
                    .image
                    .as_ref()
                    .unwrap()
                    .get_pixel(3, 3)
                    .0,
                [60, 120, 200, 255]
            );
        });
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(
            point(label.x - px(30.), label.y),
            MouseButton::Left,
            Modifiers::default(),
        );
        view.update(cx, |view, _| {
            assert!(!view.numeric_scrubbing());
            assert_eq!(view.editor.undo_depth(), 0);
            assert_eq!(view.editor.document.layers[0].opacity, 1.);
            assert_eq!(view.pixels.get_pixel(3, 3)[3], 255);
        });
    }

    #[gpui_kit::test]
    fn visible_brush_numbers_support_native_typing_arrows_escape_and_reset(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        // Native input Focus/Blur events require an active platform window.
        // An inactive test window accepts simulated text but never emits Focus.
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.simulate_resize(size(px(1400.), px(1000.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let field = cx
            .debug_bounds("numeric-value-brush-size")
            .unwrap()
            .center();
        cx.simulate_click(field, Modifiers::default());
        view.update_in(cx, |view, window, cx| {
            let state = view.numeric.borrow();
            let entry = state.inputs.get(&Target::BrushSize).unwrap();
            assert!(window.is_window_active());
            assert!(entry.input.read(cx).focus_handle(cx).is_focused(window));
            assert_eq!(entry.editing.as_ref(), Some(&view.numeric_guard()));
        });
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("27.125");
        view.update(cx, |view, cx| {
            let state = view.numeric.borrow();
            let entry = state.inputs.get(&Target::BrushSize).unwrap();
            assert_eq!(entry.input.read(cx).value().as_ref(), "27.125");
            assert_eq!(view.editor.brush.size, 16., "typing remains a draft");
        });
        cx.simulate_keystrokes("enter");
        view.update(cx, |view, _| assert_eq!(view.editor.brush.size, 27.125));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // Merely focusing and leaving a field must never round the stored value.
        cx.simulate_click(field, Modifiers::default());
        view.update_in(cx, |view, window, cx| view.focus.focus(window, cx));
        cx.run_until_parked();
        view.update(cx, |view, _| assert_eq!(view.editor.brush.size, 27.125));
        cx.simulate_click(field, Modifiers::default());
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("shift-down");
        view.update(cx, |view, _| {
            assert!((view.editor.brush.size - 28.025).abs() < 0.0001)
        });
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("900");
        cx.simulate_keystrokes("escape");
        view.update(cx, |view, _| {
            assert!((view.editor.brush.size - 28.025).abs() < 0.0001)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let label = cx
            .debug_bounds("numeric-label-brush-size")
            .unwrap()
            .center();
        cx.update(|window, cx| {
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: label,
                    click_count: 2,
                    ..Default::default()
                }),
                cx,
            );
        });
        view.update(cx, |view, _| {
            assert_eq!(view.editor.brush.size, 16.);
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }

    #[test]
    fn typed_numbers_reject_non_finite_out_of_range_and_fractional_seeds() {
        for text in ["NaN", "inf", "-inf", "", "1e999", "1025", "0"] {
            assert!(SIZE.parse(text).is_err(), "{text}");
        }
        assert_eq!(SIZE.parse("27.125").unwrap(), 27.125);
        let seed = Spec::new(0., u32::MAX as f64, 1., 0., 0);
        assert!(seed.parse("3.2").is_err());
        assert_eq!(seed.parse("4294967295").unwrap(), u32::MAX as f64);
    }

    #[gpui_kit::test]
    fn coloured_adjustment_tracks_share_numeric_drafts_clamp_and_cancel(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.simulate_resize(size(px(1400.), px(1000.)));
        for kind in [1, 10, 11] {
            let field = crate::adjustment_controls::fields(kind).remove(0);
            let (min, max, _, _) = field.numeric_bounds();
            view.update_in(cx, |view, window, cx| {
                view.dialog = Dialog::Adjustment;
                view.open_adjustment(kind, None, window, cx);
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let track = cx.debug_bounds("numeric-track-adjustment-value-0").unwrap();
            let start = point(track.left() + track.size.width * 0.25, track.center().y);
            cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
            view.update(cx, |view, cx| {
                let value: f64 = view.detail_inputs[0].read(cx).value().parse().unwrap();
                assert!(
                    (value - (min + (max - min) * 0.25)).abs() < 0.02,
                    "{}: {value}",
                    field.path
                );
                assert!(view.numeric_scrubbing());
                assert_eq!(view.editor.undo_depth(), 0);
            });
            cx.simulate_mouse_move(
                point(track.right() + px(80.), track.center().y),
                MouseButton::Left,
                Modifiers::default(),
            );
            view.update(cx, |view, cx| {
                assert_eq!(
                    view.detail_inputs[0]
                        .read(cx)
                        .value()
                        .parse::<f64>()
                        .unwrap(),
                    max
                )
            });
            cx.simulate_keystrokes("escape");
            cx.simulate_mouse_up(track.center(), MouseButton::Left, Modifiers::default());
            view.update(cx, |view, cx| {
                assert_eq!(
                    view.detail_inputs[0]
                        .read(cx)
                        .value()
                        .parse::<f64>()
                        .unwrap(),
                    field.default
                );
                assert!(!view.numeric_scrubbing());
                assert_eq!(view.editor.undo_depth(), 0);
            });

            // Releasing beyond the modal still ends the drag. Subsequent
            // pointer movement must not alter a completed dialog draft.
            cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
            let outside = point(track.right() + px(80.), track.center().y);
            cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(start, None, Modifiers::default());
            view.update(cx, |view, cx| {
                assert!(!view.numeric_scrubbing());
                assert_eq!(
                    view.detail_inputs[0]
                        .read(cx)
                        .value()
                        .parse::<f64>()
                        .unwrap(),
                    max
                );
                assert_eq!(view.editor.undo_depth(), 0);
            });

            // A completed slider change feeds the same native keyboard field.
            cx.simulate_click(start, Modifiers::default());
            cx.simulate_keystrokes("up");
            view.update(cx, |view, cx| {
                let value: f64 = view.detail_inputs[0].read(cx).value().parse().unwrap();
                assert!((value - (min + (max - min) * 0.25 + 1.)).abs() < 0.02);
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let reset = cx
                .debug_bounds("numeric-reset-adjustment-value-0")
                .unwrap()
                .center();
            cx.simulate_click(reset, Modifiers::default());
            view.update(cx, |view, cx| {
                assert_eq!(
                    view.detail_inputs[0]
                        .read(cx)
                        .value()
                        .parse::<f64>()
                        .unwrap(),
                    field.default
                );
                assert_eq!(view.editor.undo_depth(), 0);
                assert_eq!(view.editor.document.layers.len(), 1);
                assert_eq!(view.pixels.get_pixel(3, 3).0, [60, 120, 200, 255]);
            });
        }
    }

    #[gpui_kit::test]
    fn camera_numeric_cancel_and_section_change_preserve_the_correct_fields(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            fixture(window, cx, Recovery::at(temp.path().join("recovery")))
        });
        view.update_in(cx, |view, window, cx| {
            view.dialog = Dialog::CameraRaw;
            view.camera_section = 0;
            view.camera_draft =
                serde_json::to_value(omuse::camera_raw::Settings::default()).unwrap();
            view.load_camera_form(window, cx);
            let fields = crate::camera_controls::fields(&view.camera_draft, 0);
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.path == "/exposure")
                .unwrap();
            let spec = EditorView::camera_numeric_spec(field).unwrap();
            view.begin_numeric_scrub(Target::Detail(index), spec, 100., window, cx);
            view.numeric_moved(&motion(500., true, false), window, cx);
            assert_eq!(view.detail_inputs[index].read(cx).value().as_ref(), "5");
            assert_eq!(view.editor.undo_depth(), 0);
            view.cancel_numeric_scrub(window, cx);
            assert_eq!(view.detail_inputs[index].read(cx).value().as_ref(), "0.0");
            view.begin_numeric_scrub(Target::Detail(index), spec, 100., window, cx);
            view.numeric_moved(&motion(120., true, false), window, cx);
            view.camera_section = 1;
            view.load_camera_form(window, cx);
            let new_fields: Vec<_> = view
                .detail_inputs
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect();
            view.validate_numeric_context(window, cx);
            assert!(!view.numeric_scrubbing());
            assert_eq!(
                view.detail_inputs
                    .iter()
                    .map(|input| input.read(cx).value().to_string())
                    .collect::<Vec<_>>(),
                new_fields
            );
            assert_eq!(view.editor.undo_depth(), 0);
            assert_eq!(view.pixels.get_pixel(3, 3).0, [60, 120, 200, 255]);
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Spec {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    pub decimals: usize,
}

impl Spec {
    pub const fn new(min: f64, max: f64, step: f64, default: f64, decimals: usize) -> Self {
        Self {
            min,
            max,
            step,
            default,
            decimals,
        }
    }

    pub fn parse(self, text: &str) -> Result<f64, String> {
        let number = text
            .trim()
            .parse::<f64>()
            .map_err(|_| "Enter a number".to_string())?;
        if !number.is_finite() || number < self.min || number > self.max {
            return Err(format!("Use a value from {} to {}", self.min, self.max));
        }
        if self.decimals == 0 && number.fract() != 0. {
            return Err("Use a whole number".into());
        }
        Ok(number)
    }

    fn bound(self, value: f64) -> f64 {
        let scale = 10f64.powi(self.decimals.min(6) as i32);
        ((value.clamp(self.min, self.max) * scale).round() / scale).clamp(self.min, self.max)
    }

    fn text(self, value: f64) -> String {
        // Hide f32 representation noise without hiding a deliberately typed
        // fraction (27.125 must not appear as 27.12 after accepting it).
        let rounded = self.bound(value);
        let decimals = if (value - rounded).abs() <= value.abs() * f64::from(f32::EPSILON) {
            self.decimals
        } else {
            self.decimals.max(6)
        };
        let mut text = format!("{:.*}", decimals, value);
        if text.contains('.') {
            while text.ends_with('0') {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
        }
        if text == "-0" {
            text = "0".into();
        }
        text
    }
}

pub(super) const SIZE: Spec = Spec::new(1., 1024., 1., 16., 2);
pub(super) const OPACITY: Spec = Spec::new(0., 100., 1., 100., 2);
pub(super) const HARDNESS: Spec = Spec::new(0., 100., 1., 85., 2);
pub(super) const SMOOTHING: Spec = Spec::new(0., 100., 1., 0., 2);

#[derive(Clone, Debug, PartialEq)]
struct Guard {
    instance: u64,
    revision: u64,
    active: String,
    dialog: Dialog,
    generation: u64,
    tool: Tool,
    adjustment_kind: usize,
    editing_object: Option<String>,
    camera_section: usize,
}

struct InputEntry {
    input: Entity<InputState>,
    editing: Option<Guard>,
    original_text: String,
}

struct Scrub {
    target: Target,
    spec: Spec,
    guard: Guard,
    original: f64,
    original_text: Option<String>,
    value: f64,
    raw: f64,
    origin_x: f32,
    last_x: f32,
    moved: bool,
    track_width: Option<f32>,
}

impl Scrub {
    fn move_to(&mut self, x: f32, fine: bool) -> bool {
        if !x.is_finite() {
            return false;
        }
        if !self.moved && (x - self.origin_x).abs() < 3. {
            return false;
        }
        self.moved = true;
        let factor = if fine { 0.1 } else { 1. };
        let units_per_pixel = self.track_width.map_or(self.spec.step, |width| {
            (self.spec.max - self.spec.min) / f64::from(width)
        });
        self.raw = (self.raw + f64::from(x - self.last_x) * units_per_pixel * factor)
            .clamp(self.spec.min, self.spec.max);
        self.last_x = x;
        let next = self.spec.bound(self.raw);
        let changed = self.value != next;
        self.value = next;
        changed
    }
}

#[derive(Default)]
pub(super) struct NumericState {
    inputs: HashMap<Target, InputEntry>,
    scrub: Option<Scrub>,
}

impl EditorView {
    pub(super) fn camera_numeric_spec(field: &crate::camera_controls::Field) -> Option<Spec> {
        camera_numeric::field_spec(field)
    }

    pub(super) fn adjustment_numeric_row(
        &self,
        field: &crate::adjustment_controls::Field,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let (min, max, step, decimals) = field.numeric_bounds();
        let spec = Spec::new(min, max, step, field.default, decimals);
        let target = Target::Detail(index);
        let id = format!("adjustment-value-{index}");
        let row = self.numeric_row(id.clone(), field.label.clone(), target, spec, window, cx);
        let Some(colors) = field.track_colors() else {
            return div().child(row);
        };
        let theme = cx.omarchy().clone();
        let enabled = self.numeric_allowed(target);
        let input = self.detail_inputs[index].clone();
        let value = self
            .numeric_value(target, cx)
            .filter(|value| value.is_finite())
            .unwrap_or(spec.default)
            .clamp(min, max);
        let fraction = ((value - min) / (max - min)) as f32;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measured = bounds.clone();
        let mut paint = div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(7.))
            .h(px(6.))
            .rounded(px(3.))
            .overflow_hidden()
            .flex();
        for colors in colors.windows(2) {
            paint = paint.child(div().flex_1().h_full().bg(gpui_kit::linear_gradient(
                90.,
                gpui_kit::linear_color_stop(rgb(colors[0]), 0.),
                gpui_kit::linear_color_stop(rgb(colors[1]), 1.),
            )));
        }
        let track = div()
            .id(SharedString::from(format!("numeric-track-{id}")))
            .debug_selector(move || format!("numeric-track-adjustment-value-{index}"))
            .relative()
            .w_full()
            .h(px(20.))
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .child(
                canvas(move |area, _, _| measured.set(area), |_, _, _, _| {})
                    .absolute()
                    .inset_0()
                    .size_full(),
            )
            .child(paint)
            .child(
                div()
                    .absolute()
                    .left(gpui_kit::relative(fraction))
                    .ml(px(-5.))
                    .top(px(5.))
                    .size(px(10.))
                    .rounded_full()
                    .bg(theme.foreground)
                    .border_2()
                    .border_color(theme.background),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.numeric_allowed(target) {
                        if event.click_count >= 2 {
                            this.cancel_numeric_scrub(window, cx);
                            this.commit_numeric_value(target, spec.default, window, cx);
                        } else {
                            let area = bounds.get();
                            this.begin_numeric_track(
                                target,
                                spec,
                                f32::from(event.position.x),
                                f32::from(area.origin.x),
                                f32::from(area.size.width),
                                window,
                                cx,
                            );
                        }
                        input.update(cx, |input, cx| input.focus(window, cx));
                    }
                    cx.stop_propagation();
                }),
            );
        let reset_input = self.detail_inputs[index].clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .pb_2()
            .child(
                row.child(
                    button(
                        SharedString::from(format!("numeric-reset-{id}")),
                        "Reset",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .debug_selector(move || format!("numeric-reset-adjustment-value-{index}"))
                    .accessibility_label(format!(
                        "Reset {} to {}",
                        field.label,
                        spec.text(spec.default)
                    ))
                    .h(px(24.))
                    .px_2()
                    .py_0()
                    .text_size(px(10.))
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.cancel_numeric_scrub(window, cx);
                        this.commit_numeric_value(target, spec.default, window, cx);
                        reset_input.update(cx, |input, cx| input.focus(window, cx));
                    })),
                ),
            )
            .child(track)
    }

    fn begin_numeric_track(
        &mut self,
        target: Target,
        spec: Spec,
        x: f32,
        left: f32,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !x.is_finite() || !left.is_finite() || !width.is_finite() || width < 1. {
            return;
        }
        self.begin_numeric_scrub(target, spec, x, window, cx);
        let value = spec.bound(
            spec.min + f64::from(((x - left) / width).clamp(0., 1.)) * (spec.max - spec.min),
        );
        {
            let mut state = self.numeric.borrow_mut();
            let Some(scrub) = state.scrub.as_mut() else {
                return;
            };
            scrub.value = value;
            scrub.raw = value;
            scrub.track_width = Some(width);
            scrub.moved = true;
        }
        // Colour tracks are dialog fields. Updating their common InputState
        // changes the draft only; Apply owns the single artwork transaction.
        self.commit_numeric_value(target, value, window, cx);
    }

    pub(super) fn numeric_scrubbing(&self) -> bool {
        self.numeric.borrow().scrub.is_some()
    }
    fn numeric_guard(&self) -> Guard {
        Guard {
            instance: self.editor.instance_id(),
            revision: self.editor.revision(),
            active: self.editor.active_layer.clone(),
            dialog: self.dialog,
            generation: self.dialog_generation,
            tool: self.tool,
            adjustment_kind: self.adjustment_kind,
            editing_object: self.editing_object.clone(),
            camera_section: self.camera_section,
        }
    }

    fn numeric_allowed(&self, target: Target) -> bool {
        if self.busy
            || self.crop.is_some()
            || self.inline_text.is_some()
            || self.editor.floating_selection_layer().is_some()
        {
            return false;
        }
        if matches!(target, Target::Detail(_)) {
            return self.dialog != Dialog::None;
        }
        if self.dialog != Dialog::None {
            return false;
        }
        if target == Target::LayerOpacity {
            fn locked(layers: &[Layer], id: &str, parent: bool) -> Option<bool> {
                for layer in layers {
                    let value = parent || layer.locked;
                    if layer.id == id {
                        return Some(value);
                    }
                    if let Some(value) = locked(&layer.children, id, value) {
                        return Some(value);
                    }
                }
                None
            }
            return locked(
                &self.editor.document.layers,
                &self.editor.active_layer,
                false,
            ) == Some(false);
        }
        true
    }

    fn numeric_value(&self, target: Target, cx: &App) -> Option<f64> {
        Some(match target {
            Target::BrushSize => f64::from(self.editor.brush.size),
            Target::BrushOpacity => f64::from(self.editor.brush.opacity) * 100.,
            Target::BrushHardness => f64::from(self.editor.brush.hardness) * 100.,
            Target::BrushSmoothing => f64::from(self.editor.brush.smoothing),
            Target::LayerOpacity => {
                if let Some(scrub) = self.numeric.borrow().scrub.as_ref()
                    && scrub.target == target
                    && scrub.guard == self.numeric_guard()
                {
                    return Some(scrub.value);
                }
                f64::from(
                    self.editor
                        .document
                        .find_layer(&self.editor.active_layer)?
                        .opacity,
                ) * 100.
            }
            Target::Detail(index) => self
                .detail_inputs
                .get(index)?
                .read(cx)
                .value()
                .parse()
                .ok()?,
        })
    }

    fn numeric_input(
        &self,
        target: Target,
        spec: Spec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Target::Detail(index) = target {
            return self.detail_inputs[index].clone();
        }
        let existing = self
            .numeric
            .borrow()
            .inputs
            .get(&target)
            .map(|entry| entry.input.clone());
        let state = if let Some(state) = existing {
            state
        } else {
            let value = self.numeric_value(target, cx).unwrap_or(spec.default);
            let state = cx.new(|cx| InputState::new(window, cx).default_value(spec.text(value)));
            cx.subscribe_in(
                &state,
                window,
                move |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Focus => {
                        let guard = this.numeric_guard();
                        if let Some(entry) = this.numeric.borrow_mut().inputs.get_mut(&target) {
                            entry.editing = Some(guard);
                            entry.original_text = input.read(cx).value().to_string();
                        }
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        let draft =
                            this.numeric
                                .borrow_mut()
                                .inputs
                                .get_mut(&target)
                                .and_then(|entry| {
                                    entry
                                        .editing
                                        .take()
                                        .map(|guard| (guard, entry.original_text.clone()))
                                });
                        let text = input.read(cx).value().to_string();
                        if draft.as_ref().is_some_and(|(guard, original)| {
                            guard == &this.numeric_guard() && *original != text
                        }) && this.numeric_allowed(target)
                        {
                            match spec.parse(&text) {
                                Ok(value) => this.commit_numeric_value(target, value, window, cx),
                                Err(error) => {
                                    this.status = error;
                                    cx.notify();
                                }
                            }
                        }
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            this.focus.focus(window, cx);
                        }
                    }
                    _ => {}
                },
            )
            .detach();
            self.numeric.borrow_mut().inputs.insert(
                target,
                InputEntry {
                    input: state.clone(),
                    editing: None,
                    original_text: String::new(),
                },
            );
            state
        };
        if !state.read(cx).focus_handle(cx).is_focused(window) {
            let value = spec.text(self.numeric_value(target, cx).unwrap_or(spec.default));
            if state.read(cx).value().as_ref() != value {
                state.update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        state
    }

    /// A themed, reusable numeric row. Labels scrub; the field retains native
    /// text editing. Arrow keys nudge and Shift uses a tenth of the normal step.
    pub(super) fn numeric_row(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        target: Target,
        spec: Spec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = id.into();
        let label = label.into();
        let field = self.numeric_input(target, spec, window, cx);
        let enabled = self.numeric_allowed(target);
        let t = cx.omarchy().clone();
        let keyboard_field = field.clone();
        let capture_view = cx.entity().downgrade();
        div()
            .id(SharedString::from(format!("numeric-row-{id}")))
            .relative()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .flex_shrink_0()
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        // Modal shields occlude the root's bubbling pointer
                        // handlers. Capture only this row's active drag before
                        // hit testing so it also follows/release outside the
                        // row or dialog; ordinary pointer events pass through.
                        let moved_view = capture_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            let active = phase == gpui_kit::DispatchPhase::Capture
                                && moved_view
                                    .read_with(cx, |view, _| {
                                        view.numeric
                                            .borrow()
                                            .scrub
                                            .as_ref()
                                            .is_some_and(|scrub| scrub.target == target)
                                    })
                                    .unwrap_or(false);
                            if active {
                                let _ = moved_view.update(cx, |view, cx| {
                                    view.numeric_moved(event, window, cx);
                                    cx.stop_propagation();
                                });
                            }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            let active = phase == gpui_kit::DispatchPhase::Capture
                                && event.button == MouseButton::Left
                                && capture_view
                                    .read_with(cx, |view, _| {
                                        view.numeric
                                            .borrow()
                                            .scrub
                                            .as_ref()
                                            .is_some_and(|scrub| scrub.target == target)
                                    })
                                    .unwrap_or(false);
                            if active {
                                let _ = capture_view.update(cx, |view, cx| {
                                    view.numeric_up(event, window, cx);
                                });
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .id(SharedString::from(format!("numeric-label-{id}")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("numeric-label-{id}")
                    })
                    .text_size(px(11.))
                    .text_color(if enabled {
                        t.secondary
                    } else {
                        t.secondary.opacity(0.45)
                    })
                    .cursor(if enabled {
                        CursorStyle::ResizeLeftRight
                    } else {
                        CursorStyle::Arrow
                    })
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            if this.numeric_allowed(target) {
                                if event.click_count >= 2 {
                                    this.cancel_numeric_scrub(window, cx);
                                    this.commit_numeric_value(target, spec.default, window, cx);
                                } else {
                                    this.begin_numeric_scrub(
                                        target,
                                        spec,
                                        event.position.x.into(),
                                        window,
                                        cx,
                                    );
                                }
                            }
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(div().flex_1().min_w(px(4.)))
            .child(
                div()
                    .w(px(76.))
                    .flex_shrink_0()
                    .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        let key = event.keystroke.key.as_str();
                        if key == "escape" {
                            if let Some(entry) = this.numeric.borrow_mut().inputs.get_mut(&target) {
                                entry.editing = None;
                            }
                            if !this.cancel_numeric_scrub(window, cx) {
                                if let Some(value) = this.numeric_value(target, cx) {
                                    keyboard_field.update(cx, |state, cx| {
                                        state.set_value(spec.text(value), window, cx)
                                    });
                                }
                            }
                            this.focus.focus(window, cx);
                            cx.stop_propagation();
                            return;
                        }
                        let modifiers = event.keystroke.modifiers;
                        if matches!(key, "up" | "down")
                            && !modifiers.control
                            && !modifiers.alt
                            && !modifiers.platform
                            && this.numeric_allowed(target)
                        {
                            let text = keyboard_field.read(cx).value().to_string();
                            if let Ok(value) = spec.parse(&text) {
                                let step = spec.step * if modifiers.shift { 0.1 } else { 1. };
                                let precision = Spec {
                                    decimals: if spec.decimals == 0 {
                                        0
                                    } else {
                                        spec.decimals.max(6)
                                    },
                                    ..spec
                                };
                                let value =
                                    precision.bound(value + if key == "up" { step } else { -step });
                                this.commit_numeric_value(target, value, window, cx);
                                keyboard_field.update(cx, |state, cx| {
                                    state.set_value(spec.text(value), window, cx)
                                });
                                let guard = this.numeric_guard();
                                if let Some(entry) =
                                    this.numeric.borrow_mut().inputs.get_mut(&target)
                                {
                                    entry.editing = Some(guard);
                                    entry.original_text = spec.text(value);
                                }
                            }
                            cx.stop_propagation();
                        }
                    }))
                    .child(if enabled {
                        gpui_omarchy::input(
                            SharedString::from(format!("numeric-value-{id}")),
                            &field,
                            window,
                            cx,
                        )
                        .debug_selector(move || format!("numeric-value-{id}"))
                        .h(px(28.))
                        .py_0()
                        .rounded(px(4.))
                        .into_any_element()
                    } else {
                        div()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .text_color(t.secondary.opacity(0.45))
                            .child(field.read(cx).value())
                            .into_any_element()
                    }),
            )
    }

    fn begin_numeric_scrub(
        &mut self,
        target: Target,
        spec: Spec,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_interaction(cx);
        if !self.numeric_allowed(target) {
            return;
        }
        let Some(value) = self.numeric_value(target, cx).filter(|v| v.is_finite()) else {
            self.status = "Enter a valid number before dragging its label".into();
            cx.notify();
            return;
        };
        let original_text = if let Target::Detail(index) = target {
            Some(self.detail_inputs[index].read(cx).value().to_string())
        } else {
            None
        };
        self.numeric.borrow_mut().scrub = Some(Scrub {
            target,
            spec,
            guard: self.numeric_guard(),
            original: value,
            original_text,
            value,
            raw: value,
            origin_x: x,
            last_x: x,
            moved: false,
            track_width: None,
        });
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn set_numeric_tool(&mut self, target: Target, value: f64) {
        match target {
            Target::BrushSize => self.editor.brush.size = value as f32,
            Target::BrushOpacity => self.editor.brush.opacity = (value / 100.) as f32,
            Target::BrushHardness => self.editor.brush.hardness = (value / 100.) as f32,
            Target::BrushSmoothing => self.editor.brush.smoothing = value as f32,
            _ => {}
        }
    }

    fn commit_numeric_value(
        &mut self,
        target: Target,
        value: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.numeric_allowed(target) || !value.is_finite() {
            return;
        }
        match target {
            Target::Detail(index) => {
                self.detail_inputs[index].update(cx, |input, cx| {
                    input.set_value(value.to_string(), window, cx)
                });
                self.invalidate_jpeg_preview();
                self.schedule_range_preview(cx);
            }
            Target::LayerOpacity => {
                self.finish_interaction(cx);
                let id = self.editor.active_layer.clone();
                if self.editor.set_opacity(&id, (value / 100.) as f32) {
                    self.changed(cx);
                }
            }
            _ => self.set_numeric_tool(target, value),
        }
        cx.notify();
    }

    pub(super) fn numeric_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.numeric.borrow().scrub.is_none() {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            self.cancel_numeric_scrub(window, cx);
            return;
        }
        let Some(mut scrub) = self.numeric.borrow_mut().scrub.take() else {
            return;
        };
        if scrub.guard != self.numeric_guard() || !self.numeric_allowed(scrub.target) {
            self.refresh(cx);
            return;
        }
        let changed = scrub.move_to(event.position.x.into(), event.modifiers.shift);
        let target = scrub.target;
        if changed {
            match target {
                Target::Detail(index) => self.detail_inputs[index].update(cx, |input, cx| {
                    input.set_value(scrub.spec.text(scrub.value), window, cx)
                }),
                Target::LayerOpacity => {}
                _ => self.set_numeric_tool(target, scrub.value),
            }
        }
        self.numeric.borrow_mut().scrub = Some(scrub);
        if changed && target == Target::LayerOpacity {
            self.refresh(cx);
        } else if changed {
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn numeric_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.numeric.borrow().scrub.is_some() {
            self.finish_numeric_scrub(cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn finish_numeric_scrub(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(scrub) = self.numeric.borrow_mut().scrub.take() else {
            return false;
        };
        if scrub.guard == self.numeric_guard()
            && self.numeric_allowed(scrub.target)
            && scrub.target == Target::LayerOpacity
            && scrub.value != scrub.original
        {
            let id = self.editor.active_layer.clone();
            if self.editor.set_opacity(&id, (scrub.value / 100.) as f32) {
                self.changed(cx);
            }
        } else if scrub.target == Target::LayerOpacity {
            self.refresh(cx);
        }
        cx.notify();
        true
    }

    pub(super) fn cancel_numeric_scrub(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(scrub) = self.numeric.borrow_mut().scrub.take() else {
            return false;
        };
        if scrub.guard == self.numeric_guard() {
            if let (Target::Detail(index), Some(original)) = (scrub.target, scrub.original_text) {
                self.detail_inputs[index]
                    .update(cx, |input, cx| input.set_value(original, window, cx));
            } else {
                self.set_numeric_tool(scrub.target, scrub.original);
            }
        }
        if scrub.target == Target::LayerOpacity {
            self.refresh(cx);
        } else {
            cx.notify();
        }
        true
    }

    pub(super) fn validate_numeric_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stale = self.numeric.borrow().scrub.as_ref().is_some_and(|scrub| {
            scrub.guard != self.numeric_guard()
                || !self.numeric_allowed(scrub.target)
                || (!cfg!(test) && !window.is_window_active())
        });
        if stale {
            self.cancel_numeric_scrub(window, cx);
        }
    }

    pub(super) fn numeric_preview_document(&self) -> Option<Document> {
        let state = self.numeric.borrow();
        let scrub = state.scrub.as_ref()?;
        if scrub.target != Target::LayerOpacity || scrub.guard != self.numeric_guard() {
            return None;
        }
        let mut document = self.editor.document.clone();
        document.find_layer_mut(&scrub.guard.active)?.opacity = (scrub.value / 100.) as f32;
        Some(document)
    }
}
