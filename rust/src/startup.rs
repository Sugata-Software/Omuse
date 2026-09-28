//! A first-frame brand surface while document preparation runs off the UI thread.
//! Decorative motion ends when the editor is ready; normal launches have no hold.
use crate::ui::{EditorView, PreparedEditor};
use gpui_kit::{
    Animation, AnimationExt, App, Context, Div, Entity, FontWeight, Image, ImageFormat,
    PathBuilder, Render, RenderImage, Stateful, Task, Window, canvas, div, img, point, prelude::*,
    px, rgb,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const PAPER: u32 = 0xf7ebd6;
const INK: u32 = 0x2b1620;
const SUNSET: [u32; 4] = [0xf2b33d, 0xf0885a, 0xe2622a, 0xff2e88];
const HANDOFF: Duration = Duration::from_millis(220);

#[cfg(all(test, feature = "ui-test"))]
#[path = "startup_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    FirstFrame,
    Loading,
    Handoff,
    Ready,
}

struct Artwork {
    muse: Option<Arc<RenderImage>>,
    wordmark: Option<Arc<RenderImage>>,
}

impl Artwork {
    fn new(cx: &App) -> Self {
        let render = |bytes: &[u8]| {
            Image::from_bytes(ImageFormat::Svg, bytes.to_vec())
                .to_image_data(cx.svg_renderer())
                .map_err(|error| eprintln!("Splash artwork unavailable: {error:#}"))
                .ok()
        };
        Self {
            muse: render(include_bytes!("../assets/splash-muse.svg")),
            wordmark: render(include_bytes!("../assets/splash-wordmark.svg")),
        }
    }

    fn release(self, window: &mut Window) {
        for image in [self.muse, self.wordmark].into_iter().flatten() {
            let _ = window.drop_image(image);
        }
    }
}

pub(crate) struct StartupView {
    phase: Phase,
    path: Option<PathBuf>,
    caption: String,
    editor: Option<Entity<EditorView>>,
    artwork: Option<Artwork>,
    cancelled: Arc<AtomicBool>,
    load_task: Option<Task<()>>,
    transition_task: Option<Task<()>>,
    native_journey: Option<PathBuf>,
    capture_gate: bool,
    force_reduced_motion: bool,
    started: Instant,
}

impl StartupView {
    pub(crate) fn new(
        path: Option<PathBuf>,
        native_journey: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let capture_gate = native_journey.is_some()
            && omuse::identity::env_var("OMUSE_NATIVE_STARTUP_WAIT").as_deref() == Ok("1");
        let force_reduced_motion = matches!(
            omuse::identity::env_var("OMUSE_REDUCED_MOTION").as_deref(),
            Ok("1" | "true")
        );
        let caption = path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| format!("Opening {}", name.to_string_lossy()))
            .unwrap_or_else(|| "Preparing your workspace".into());
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_on_release = cancelled.clone();
        cx.on_release(move |_, _| cancel_on_release.store(true, Ordering::Relaxed))
            .detach();

        // A deferred effect is not enough: the first splash frame must have
        // actually painted before document decoding/compositing is scheduled.
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.start_load(window, cx));
        });
        Self {
            phase: Phase::FirstFrame,
            path,
            caption,
            editor: None,
            artwork: Some(Artwork::new(cx)),
            cancelled,
            load_task: None,
            transition_task: None,
            native_journey,
            capture_gate,
            force_reduced_motion,
            started: Instant::now(),
        }
    }

    fn reduced_motion(&self, cx: &App) -> bool {
        self.force_reduced_motion || cx.reduce_motion()
    }

    fn record(&self, stage: &str, cx: &App) {
        if let Some(dir) = &self.native_journey {
            let record = serde_json::json!({
                "stage": stage,
                "elapsed_ms": self.started.elapsed().as_millis(),
                "pid": std::process::id(),
                "reduced_motion": self.reduced_motion(cx),
                "capture_gate": self.capture_gate,
            });
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(
                dir.join(format!("startup-{stage}.json")),
                serde_json::to_vec_pretty(&record).unwrap(),
            );
        }
    }

    fn start_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase != Phase::FirstFrame || self.cancelled.load(Ordering::Relaxed) {
            return;
        }
        self.phase = Phase::Loading;
        self.record("painted", cx);
        let path = self.path.take();
        let cancelled = self.cancelled.clone();
        let capture_dir = self.native_journey.clone().filter(|_| self.capture_gate);
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            // Only the explicitly requested native capture harness can hold
            // this gate. Ordinary launches never sleep before preparation.
            if let Some(dir) = capture_dir {
                let deadline = Instant::now() + Duration::from_secs(20);
                while !dir.join("continue-startup").is_file() {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    if Instant::now() >= deadline {
                        let _ = std::fs::write(
                            dir.join("native-error.txt"),
                            "The startup capture harness did not release its gate.",
                        );
                        // A forgotten test harness may fail its check, but it
                        // must not strand an otherwise usable editor.
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(25))
                        .await;
                }
            }
            let work_cancelled = cancelled.clone();
            let prepared = cx
                .background_executor()
                .spawn(async move { PreparedEditor::load(path, &work_cancelled) })
                .await;
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            if let Some(prepared) = prepared {
                let _ = view.update_in(cx, |this, window, cx| {
                    this.finish_load(prepared, window, cx);
                });
            }
        }));
        cx.notify();
    }

    fn finish_load(
        &mut self,
        prepared: PreparedEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.phase != Phase::Loading || self.cancelled.load(Ordering::Relaxed) {
            return;
        }
        self.record("prepared", cx);
        self.editor = Some(cx.new(|cx| {
            let mut editor = EditorView::from_prepared(prepared, window, cx);
            if let Some(dir) = self.native_journey.clone() {
                editor.start_native_journey(dir, window, cx);
            }
            editor
        }));
        self.phase = Phase::Handoff;
        self.record("editor-ready", cx);
        if self.reduced_motion(cx) {
            self.finish_handoff(window, cx);
        } else {
            // Start retirement after the first handoff frame actually paints.
            // A slow first editor layout must not consume the whole fade.
            let weak = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.phase != Phase::Handoff {
                        return;
                    }
                    this.transition_task = Some(cx.spawn_in(window, async move |view, cx| {
                        cx.background_executor().timer(HANDOFF).await;
                        let _ = view.update_in(cx, |this, window, cx| {
                            this.finish_handoff(window, cx);
                        });
                    }));
                });
            });
        }
        cx.notify();
    }

    fn finish_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase != Phase::Handoff {
            return;
        }
        self.phase = Phase::Ready;
        if let Some(artwork) = self.artwork.take() {
            artwork.release(window);
        }
        self.record("dismissed", cx);
        cx.notify();
    }

    fn surface(&self, window: &Window, cx: &App) -> Stateful<Div> {
        let compact = window.viewport_size().height < px(720.);
        let mark_size = if compact { 174. } else { 210. };
        let mut lockup = div().flex().flex_col().items_center().gap(px(22.));
        if let Some(muse) = self.artwork.as_ref().and_then(|art| art.muse.clone()) {
            lockup = lockup.child(img(muse).w(px(mark_size)).h(px(mark_size * 0.953)));
        }
        if let Some(wordmark) = self.artwork.as_ref().and_then(|art| art.wordmark.clone()) {
            lockup = lockup.child(img(wordmark).w(px(278.)).h(px(62.)));
        } else {
            lockup = lockup.child(
                div()
                    .text_size(px(70.))
                    .font_weight(FontWeight::BOLD)
                    .child("omuse"),
            );
        }
        let accent = div().flex().gap(px(5.)).h(px(4.));
        let accent = if self.reduced_motion(cx) || self.phase == Phase::Handoff {
            accent
                .children(
                    SUNSET
                        .into_iter()
                        .map(|color| div().w(px(27.)).h(px(3.)).rounded_full().bg(rgb(color))),
                )
                .into_any_element()
        } else {
            accent
                .with_animation(
                    "startup-sunset-wave",
                    Animation::new(Duration::from_millis(2400))
                        .repeat()
                        .with_max_fps(30.),
                    |row, delta| {
                        row.children(SUNSET.into_iter().enumerate().map(|(index, color)| {
                            let wave = (0.5
                                + 0.5
                                    * (std::f32::consts::TAU * (delta - index as f32 * 0.16))
                                        .cos())
                            .powi(2);
                            div()
                                .w(px(27.))
                                .h(px(3.))
                                .rounded_full()
                                .bg(rgb(color))
                                .opacity(0.3 + 0.7 * wave)
                        }))
                    },
                )
                .into_any_element()
        };
        div()
            .id("startup-surface")
            .debug_selector(|| "startup-surface".into())
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .occlude()
            .bg(rgb(PAPER))
            .text_color(rgb(INK))
            .child(
                canvas(
                    |_, _, _| (),
                    |bounds, _, window, _| {
                        let x = f32::from(bounds.origin.x);
                        let y = f32::from(bounds.origin.y);
                        let w = f32::from(bounds.size.width);
                        let h = f32::from(bounds.size.height);
                        for (index, color) in SUNSET.into_iter().enumerate() {
                            let offset = index as f32 * 22.;
                            let mut path = PathBuilder::stroke(px(12.));
                            path.move_to(point(px(x - 90.), px(y + h * 0.58 + offset)));
                            path.cubic_bezier_to(
                                point(px(x + w * 0.27), px(y + h + 85. + offset)),
                                point(px(x + w * 0.22), px(y + h * 0.56 + offset)),
                                point(px(x - 45.), px(y + h * 0.93 + offset)),
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(color).opacity(0.12));
                            }
                            let radius = 180. + index as f32 * 22.;
                            let mut arc = PathBuilder::stroke(px(1.));
                            arc.move_to(point(px(x + w - radius), px(y - 24.)));
                            arc.arc_to(
                                point(px(radius), px(radius)),
                                px(0.),
                                false,
                                false,
                                point(px(x + w + 24.), px(y + radius)),
                            );
                            if let Ok(path) = arc.build() {
                                window.paint_path(path, rgb(INK).opacity(0.10));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(28.))
                    .left(px(32.))
                    .text_size(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(INK).opacity(0.5))
                    .child("I M A G E   S T U D I O"),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(lockup)
                    .child(div().h(px(if compact { 36. } else { 44. })))
                    .child(accent)
                    .child(
                        div()
                            .id("startup-status")
                            .debug_selector(|| "startup-status".into())
                            .mt(px(16.))
                            .max_w(px(440.))
                            .px_6()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(rgb(INK).opacity(0.65))
                            .child(if self.phase == Phase::Handoff {
                                "Ready".into()
                            } else {
                                self.caption.clone()
                            }),
                    ),
            )
    }
}

impl Render for StartupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // If reduced motion is enabled during the transition, unmount now;
        // an invisible overlay must never linger over a ready editor.
        if self.phase == Phase::Handoff && self.reduced_motion(cx) {
            self.finish_handoff(window, cx);
        }
        let mut root = div().id("omuse-launch").relative().size_full();
        if let Some(editor) = &self.editor {
            root = root.child(editor.clone());
        }
        match self.phase {
            Phase::Ready => root,
            Phase::Handoff => root.child(self.surface(window, cx).with_animation(
                "startup-handoff",
                Animation::new(HANDOFF).with_max_fps(30.),
                |surface, progress| surface.opacity((1. - progress).powi(2)),
            )),
            Phase::FirstFrame | Phase::Loading => root.child(self.surface(window, cx)),
        }
    }
}
