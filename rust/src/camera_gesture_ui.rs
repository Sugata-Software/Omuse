//! Draft-only Camera Raw samplers and target drags. The reference image is taken
//! from the exact pipeline stage preceding the control being adjusted.
use super::inspector_ui::panel_button as button;
use super::*;
use crate::camera_canvas::{CameraCanvasEvent as Event, CameraCanvasMode as Mode};
use gpui_kit::Div;
use omuse::{
    camera_gestures::{self, Target},
    camera_raw::{SampleStage, Settings},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Guard {
    editor: u64,
    revision: u64,
    selection: u64,
    page: u64,
    dialog: u64,
    layer: String,
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
    fn setup(
        cx: &mut TestAppContext,
    ) -> (
        Entity<EditorView>,
        &mut VisualTestContext,
        tempfile::TempDir,
    ) {
        cx.update(crate::init_test_theme);
        let recovery = tempfile::tempdir().unwrap();
        let path = recovery.path().to_owned();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(path);
            view.dialog = Dialog::None;
            let mut document = Document::new(48, 32);
            document.layers[0].image = Some(
                image::RgbaImage::from_fn(48, 32, |x, _| {
                    image::Rgba(if x < 24 {
                        [151, 139, 124, 255]
                    } else {
                        [170, 40, 190, 255]
                    })
                })
                .into(),
            );
            view.editor = Editor::new(document);
            view.focus.focus(window, cx);
            view.refresh(cx);
            view.open_camera_raw(window, cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(1000.)));
        (view, cx, recovery)
    }
    fn settings(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Settings {
        cx.update(|_, cx| serde_json::from_value(view.read(cx).camera_draft.clone()).unwrap())
    }
    fn prepare(view: &Entity<EditorView>, cx: &mut VisualTestContext, section: usize, mode: Mode) {
        view.update_in(cx, |view, window, cx| {
            view.camera_section = section;
            view.load_camera_form(window, cx);
            view.prepare_camera_tool(mode, window, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert!(!view.busy, "{}", view.status);
            assert!(
                view.camera_sample_ready(
                    &serde_json::from_value(view.camera_draft.clone()).unwrap()
                ),
                "{}",
                view.status
            );
            window.draw(cx).clear(cx);
        });
    }
    #[gpui_kit::test]
    fn camera_gesture_white_balance_and_defringe_use_stage_samples_without_editing_artwork(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        let original = cx.update(|_, cx| view.read(cx).pixels.clone());
        prepare(&view, cx, 0, Mode::WhiteBalance);
        let rect = cx.debug_bounds("camera-canvas").unwrap();
        let pick = rect.center() - point(px(70.), px(0.));
        cx.simulate_mouse_down(pick, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(pick, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        let white = settings(&view, cx);
        let expected = camera_gestures::white_balance(&Settings::default(), [151, 139, 124, 255])
            .unwrap()
            .0;
        assert!((white.temperature - expected.temperature).abs() < 0.01);
        assert!((white.tint - expected.tint).abs() < 0.01);
        prepare(&view, cx, 6, Mode::Defringe);
        let rect = cx.debug_bounds("camera-canvas").unwrap();
        let pick = rect.center() + point(px(70.), px(0.));
        cx.simulate_mouse_down(pick, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(pick, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        let defringe = settings(&view, cx);
        assert!(defringe.optics.purple_amount > 0.);
        assert!(defringe.optics.purple_hue_low < defringe.optics.purple_hue_high);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.pixels, original);
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }
    #[gpui_kit::test]
    fn camera_gesture_target_drag_escape_restores_then_apply_is_one_exact_undo(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        let original = cx.update(|_, cx| view.read(cx).pixels.clone());
        prepare(&view, cx, 3, Mode::TargetSaturation);
        let base = settings(&view, cx);
        let rect = cx.debug_bounds("camera-canvas").unwrap();
        let start = rect.center() + point(px(50.), px(0.));
        let end = start + point(px(0.), px(35.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        assert_ne!(settings(&view, cx).mixer.saturation, base.mixer.saturation);
        cx.simulate_keystrokes("escape");
        assert_eq!(settings(&view, cx), base);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.dialog, Dialog::CameraRaw);
            assert!(view.camera_gestures.drag.is_none());
        });
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        let expected = view.update_in(cx, |view, window, cx| {
            view.read_camera_form(cx).unwrap();
            let settings: Settings = serde_json::from_value(view.camera_draft.clone()).unwrap();
            assert_ne!(settings.mixer.saturation, base.mixer.saturation);
            let mut expected = Editor::new(view.editor.document.clone());
            expected.active_layer = view.editor.active_layer.clone();
            expected.selection = view.editor.selection.clone();
            expected
                .apply_image_operation(|source| omuse::camera_raw::apply(source, &settings))
                .unwrap();
            assert_eq!(view.editor.undo_depth(), 0);
            assert_eq!(view.pixels, original);
            view.confirm_dialog(window, cx);
            raster::composite(&expected.document)
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.dialog, Dialog::None);
            assert_eq!(view.pixels, expected);
            assert_eq!(view.editor.undo_depth(), 1);
            assert!(view.editor.undo());
            view.refresh(cx);
            assert_eq!(view.pixels, original);
        });
    }
    #[gpui_kit::test]
    fn camera_gesture_channel_switch_and_lost_release_cancel_the_original_draft(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        prepare(&view, cx, 2, Mode::TargetCurve);
        let original = settings(&view, cx);
        view.update_in(cx, |view, window, cx| {
            view.handle_camera_canvas(&Event::TargetStarted([151, 139, 124, 255]), window, cx);
            view.handle_camera_canvas(&Event::TargetMoved(0.2), window, cx);
            assert_ne!(view.camera_draft, serde_json::to_value(&original).unwrap());
            view.camera_curve_channel = 1;
            view.validate_camera_gesture_context(window, cx);
            assert_eq!(view.camera_draft, serde_json::to_value(&original).unwrap());
            view.handle_camera_canvas(&Event::TargetMoved(0.5), window, cx);
            assert_eq!(view.camera_draft, serde_json::to_value(&original).unwrap());
            assert!(view.camera_gestures.drag.is_none());
            view.handle_camera_canvas(&Event::TargetStarted([151, 139, 124, 255]), window, cx);
            view.handle_camera_canvas(&Event::TargetMoved(-0.1), window, cx);
            view.handle_camera_canvas(&Event::TargetCancelled, window, cx);
            assert_eq!(view.camera_draft, serde_json::to_value(&original).unwrap());
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }
    #[gpui_kit::test]
    fn camera_gesture_stale_reference_and_transparent_samples_are_rejected(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _recovery) = setup(cx);
        prepare(&view, cx, 3, Mode::TargetHue);
        view.update_in(cx, |view, window, cx| {
            view.handle_camera_canvas(&Event::TargetStarted([0, 255, 0, 0]), window, cx);
            assert!(view.camera_gestures.drag.is_none());
            view.editor.select_all();
            view.handle_camera_canvas(&Event::TargetStarted([0, 255, 0, 255]), window, cx);
            assert!(view.camera_gestures.drag.is_none());
            assert!(view.status.contains("refresh"));
            assert_eq!(view.editor.undo_depth(), 0);
        });
    }
}
struct Ready {
    guard: Guard,
    settings: Settings,
    stage: SampleStage,
}
struct Drag {
    guard: Guard,
    original: Settings,
    expected: Settings,
    pixel: [u8; 4],
    mode: Mode,
    channel: usize,
    section: usize,
    target: Target,
}
#[derive(Default)]
pub(super) struct CameraGestureState {
    mode: Option<Mode>,
    ready: Option<Ready>,
    drag: Option<Drag>,
    notice: Option<String>,
}

impl EditorView {
    fn camera_gesture_guard(&self) -> Guard {
        Guard {
            editor: self.editor.instance_id(),
            revision: self.editor.revision(),
            selection: self.editor.selection_revision(),
            page: self.create.epoch,
            dialog: self.dialog_generation,
            layer: self.editor.active_layer.clone(),
        }
    }
    pub(super) fn camera_canvas_mode(&self) -> Mode {
        match self.camera_section {
            0 => Mode::WhiteBalance,
            2 => Mode::TargetCurve,
            6 => Mode::Defringe,
            7 => Mode::Geometry,
            3 => self
                .camera_gestures
                .mode
                .filter(|m| {
                    matches!(
                        m,
                        Mode::PointColor
                            | Mode::TargetHue
                            | Mode::TargetSaturation
                            | Mode::TargetLuminance
                    )
                })
                .unwrap_or(Mode::PointColor),
            _ => Mode::PointColor,
        }
    }
    pub(super) fn camera_sample_stage(&self) -> Option<SampleStage> {
        if !matches!(self.camera_section, 0 | 2 | 3 | 6) {
            return None;
        }
        match self.camera_canvas_mode() {
            Mode::WhiteBalance => Some(SampleStage::WhiteBalance),
            Mode::TargetCurve => Some(SampleStage::Curve),
            Mode::Defringe => Some(SampleStage::Optics),
            Mode::Geometry => None,
            Mode::PointColor => Some(SampleStage::PointColor),
            _ => Some(SampleStage::Mixer),
        }
    }
    pub(super) fn init_camera_gestures(&mut self) {
        self.camera_gestures = CameraGestureState::default();
        // A newly opened dialog is neutral, so the initial point-
        // colour sampler remains immediately usable on the Mixer tab.
        self.camera_gestures.ready = Some(Ready {
            guard: self.camera_gesture_guard(),
            settings: Settings::for_new_edit(),
            stage: SampleStage::PointColor,
        });
    }
    pub(super) fn accept_camera_sample(
        &mut self,
        settings: Settings,
        stage: SampleStage,
        pixels: Arc<image::RgbaImage>,
        cx: &mut Context<Self>,
    ) {
        if self.camera_sample_stage() != Some(stage) {
            return;
        }
        self.camera_gestures.ready = Some(Ready {
            guard: self.camera_gesture_guard(),
            settings,
            stage,
        });
        let _ = self
            .camera_canvas
            .update(cx, |canvas, cx| canvas.set_source(pixels, cx));
    }
    pub(super) fn camera_gesture_notice(&mut self) -> Option<String> {
        self.camera_gestures.notice.take()
    }
    fn camera_sample_ready(&self, settings: &Settings) -> bool {
        self.camera_gestures.ready.as_ref().is_some_and(|ready| {
            ready.guard == self.camera_gesture_guard()
                && &ready.settings == settings
                && self.camera_sample_stage() == Some(ready.stage)
        })
    }
    pub(super) fn configure_camera_canvas(&mut self, cx: &mut Context<Self>) {
        let mode = self.camera_canvas_mode();
        let guides = serde_json::from_value(self.camera_draft["geometry"]["guides"].clone())
            .unwrap_or_default();
        let source = if mode == Mode::Geometry {
            self.camera_source().ok()
        } else {
            None
        };
        self.camera_canvas.update(cx, |canvas, cx| {
            canvas.set_mode(mode, cx);
            canvas.set_guides(
                if mode == Mode::Geometry {
                    guides
                } else {
                    vec![]
                },
                cx,
            );
            if let Some(source) = source {
                let _ = canvas.set_source(source, cx);
            }
        });
    }
    fn prepare_camera_tool(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.cancel_camera_gesture(window, cx);
        match self.read_camera_form(cx).and_then(|_| {
            serde_json::from_value::<Settings>(self.camera_draft.clone()).map_err(Into::into)
        }) {
            Ok(settings) => {
                self.camera_gestures.mode = Some(mode);
                self.load_camera_form(window, cx);
                self.start_camera_raw(settings, true, cx);
            }
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }
    fn camera_drag_current(&self, drag: &Drag) -> bool {
        self.dialog == Dialog::CameraRaw
            && !self.busy
            && drag.guard == self.camera_gesture_guard()
            && drag.mode == self.camera_canvas_mode()
            && drag.channel == self.camera_curve_channel
            && drag.section == self.camera_section
            && serde_json::from_value::<Settings>(self.camera_draft.clone())
                .ok()
                .as_ref()
                == Some(&drag.expected)
    }
    pub(super) fn cancel_camera_gesture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.camera_gestures.drag.take() else {
            return false;
        };
        self.camera_canvas
            .update(cx, |canvas, cx| canvas.cancel_target(cx));
        if self.dialog == Dialog::CameraRaw && drag.guard == self.camera_gesture_guard() {
            self.camera_draft = serde_json::to_value(drag.original).unwrap();
            self.load_camera_form(window, cx);
            self.status = "Targeted adjustment cancelled".into();
        }
        cx.notify();
        true
    }
    pub(super) fn validate_camera_gesture_context(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.camera_gestures.drag.as_ref().is_some_and(|drag| {
            !self.camera_drag_current(drag) || (!cfg!(test) && !window.is_window_active())
        }) {
            self.cancel_camera_gesture(window, cx);
        }
    }
    pub(super) fn handle_camera_canvas(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, Event::TargetCancelled) {
            self.cancel_camera_gesture(window, cx);
            return;
        }
        if self.dialog != Dialog::CameraRaw || self.busy {
            return;
        }
        if let Event::TargetMoved(delta) = event {
            let Some(drag) = self.camera_gestures.drag.as_ref() else {
                return;
            };
            if !self.camera_drag_current(drag) {
                self.cancel_camera_gesture(window, cx);
                return;
            }
            match camera_gestures::targeted(&drag.original, drag.pixel, drag.target, *delta) {
                Ok(settings) => {
                    self.camera_gestures.drag.as_mut().unwrap().expected = settings.clone();
                    self.camera_draft = serde_json::to_value(settings).unwrap();
                    self.load_camera_form(window, cx);
                    self.status =
                        "Targeted adjustment · release to preview · Escape to restore".into();
                }
                Err(error) => self.status = error.to_string(),
            }
            cx.notify();
            return;
        }
        if matches!(event, Event::TargetFinished) {
            let Some(drag) = self.camera_gestures.drag.take() else {
                return;
            };
            if self.camera_drag_current(&drag) && drag.expected != drag.original {
                self.start_camera_raw(drag.expected, true, cx);
            }
            return;
        }
        if let Err(error) = self.read_camera_form(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let settings = match serde_json::from_value::<Settings>(self.camera_draft.clone()) {
            Ok(settings) => settings,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        if !matches!(event, Event::Guide(_)) && !self.camera_sample_ready(&settings) {
            self.camera_canvas
                .update(cx, |canvas, cx| canvas.cancel_target(cx));
            self.status =
                "Choose Preview to refresh the colour reference, then sample again".into();
            cx.notify();
            return;
        }
        let mode = self.camera_canvas_mode();
        match event {
            Event::TargetStarted(pixel) => {
                let target = match mode {
                    Mode::TargetCurve => Target::Curve(self.camera_curve_channel),
                    Mode::TargetHue => Target::Hue,
                    Mode::TargetSaturation => Target::Saturation,
                    Mode::TargetLuminance => Target::Luminance,
                    _ => return,
                };
                // Validate the sampled tone before accepting a draft transaction.
                if let Err(error) = camera_gestures::targeted(&settings, *pixel, target, 0.00001) {
                    self.status = error.to_string();
                    self.camera_canvas
                        .update(cx, |canvas, cx| canvas.cancel_target(cx));
                } else {
                    self.camera_gestures.drag = Some(Drag {
                        guard: self.camera_gesture_guard(),
                        original: settings.clone(),
                        expected: settings,
                        pixel: *pixel,
                        mode,
                        channel: self.camera_curve_channel,
                        section: self.camera_section,
                        target,
                    });
                }
            }
            Event::Sampled(pixel) => {
                let result = match mode {
                    Mode::WhiteBalance => {
                        camera_gestures::white_balance(&settings, *pixel).map(|(next, limited)| {
                            self.camera_gestures.notice = Some(
                                if limited {
                                    "White balance reached the supported temperature/tint limit"
                                } else {
                                    "White balance sampled from a neutral reference"
                                }
                                .into(),
                            );
                            next
                        })
                    }
                    Mode::Defringe => camera_gestures::defringe(&settings, *pixel),
                    _ => return,
                };
                match result {
                    Ok(next) => {
                        self.camera_draft = serde_json::to_value(&next).unwrap();
                        self.load_camera_form(window, cx);
                        self.start_camera_raw(next, true, cx);
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            Event::Picked(color) if mode == Mode::PointColor && self.camera_section == 3 => {
                let points = self.camera_draft["mixer"]["points"].as_array_mut().unwrap();
                if points.len() >= 8 {
                    self.status = "Remove a point color before sampling another (maximum 8)".into();
                } else {
                    points.push(serde_json::to_value(color).unwrap());
                    self.status = "Point color sampled".into();
                    self.load_camera_form(window, cx);
                }
            }
            Event::Guide(guide) if mode == Mode::Geometry => {
                let guides = self.camera_draft["geometry"]["guides"]
                    .as_array_mut()
                    .unwrap();
                if guides.len() >= 16 {
                    self.status = "Remove a guide before drawing another (maximum 16)".into();
                } else {
                    guides.push(serde_json::to_value(guide).unwrap());
                    self.camera_draft["geometry"]["upright"] = "Guided".into();
                    self.status = "Geometry guide added".into();
                    self.load_camera_form(window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }
    pub(super) fn camera_gesture_controls(&self, cx: &mut Context<Self>) -> Div {
        let modes: &[(Mode, &str)] = match self.camera_section {
            0 => &[(Mode::WhiteBalance, "Pick neutral white balance")],
            2 => &[(Mode::TargetCurve, "Target curve on image")],
            3 => &[
                (Mode::PointColor, "Pick point colour"),
                (Mode::TargetHue, "Target hue"),
                (Mode::TargetSaturation, "Target saturation"),
                (Mode::TargetLuminance, "Target luminance"),
            ],
            6 => &[(Mode::Defringe, "Pick green or purple fringe")],
            _ => &[],
        };
        let mut body = div().flex().flex_col().gap_2();
        if modes.is_empty() {
            return body;
        }
        let mut buttons = div().flex().flex_wrap().gap_1();
        for (index, &(mode, label)) in modes.iter().enumerate() {
            buttons = buttons.child(
                button(
                    SharedString::from(format!("camera-gesture-{index}")),
                    label,
                    if self.camera_canvas_mode() == mode {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .debug_selector(move || format!("camera-gesture-{index}").into())
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.prepare_camera_tool(mode, window, cx)
                })),
            );
        }
        body = body.child(buttons);
        if matches!(self.camera_section, 0 | 2 | 6) && !self.busy {
            body = body.child(self.camera_canvas.clone());
        }
        body.child(div().text_sm().child(match self.camera_section {
            0=>"Colour reference before white balance. Select the tool to prepare it, then click a neutral midtone.",
            2=>"Reference before curves. Select the tool, then drag a tone up or down. Escape restores the draft.",
            6=>"Reference after lens corrections, before fringe removal. Select the tool, then click a coloured fringe.",
            _ if self.camera_canvas_mode()==Mode::PointColor=>"Reference after current mixer adjustments. Select the tool, then click a colour. Preview refreshes the reference after typed changes.",
            _=>"Reference before the mixer. Select a tool, then drag up/down. Preview refreshes the reference after typed changes.",
        }))
    }
}
