//! Create-workspace controls for native page motion and bounded media inputs.
use super::create_ui::{ContentEvent, ContentJob};
use super::inspector_ui::panel_input as input;
use super::inspector_ui::{panel_button as button, panel_note, panel_section};
use super::*;
use gpui_kit::Div;
use omuse::motion::{
    Easing, LayerAnimation, LayerAnimationKind, LayerTrack, MotionAudioMedia, MotionClipMedia,
    MotionExportOptions, MotionFormat, MotionMedia, MotionTimeline, PageTimeline, PageTransition,
    SlideDirection, SubtitleCue, SubtitlePosition, SubtitlePresentation, SubtitleStyle,
    SubtitleTrack, VideoClipInfo,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

const NATIVE_PREVIEW_MAX_FRAMES: u64 = 300;
const NATIVE_PREVIEW_MAX_PLAYBACK_MS: u64 = 10_000;
const CLIP_PROBE_UI_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq)]
struct MotionSourceIdentity {
    create_epoch: u64,
    editor_revision: u64,
    collection: Option<(String, u64, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClipProbeSource {
    path: String,
    start: String,
    end: String,
    split: String,
}

struct ClipProbeJob {
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<VideoClipInfo, String>>,
    source: ClipProbeSource,
    identity: MotionSourceIdentity,
    started: Instant,
}

impl Drop for ClipProbeJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

enum MotionPreviewEvent {
    Started {
        pages: usize,
        frames: u64,
        frame_delay_ms: u64,
    },
    Frame {
        index: u64,
        total: u64,
        pixels: image::RgbaImage,
    },
}

enum ClipProbeCompletion {
    Cancelled,
    TimedOut,
    Finished(Result<VideoClipInfo, String>),
}

pub(super) struct MotionUiState {
    pub(super) duration: Entity<InputState>,
    pub(super) fps: Entity<InputState>,
    pub(super) audio_path: Entity<InputState>,
    pub(super) audio_offset_ms: Entity<InputState>,
    pub(super) audio_source_start_ms: Entity<InputState>,
    pub(super) audio_duration_ms: Entity<InputState>,
    pub(super) audio_volume: Entity<InputState>,
    pub(super) subtitle_path: Entity<InputState>,
    pub(super) subtitle_font_size: Entity<InputState>,
    pub(super) subtitle_outline: Entity<InputState>,
    pub(super) subtitle_presentation: SubtitlePresentation,
    pub(super) subtitle_position: SubtitlePosition,
    pub(super) cue_start_ms: Entity<InputState>,
    pub(super) cue_end_ms: Entity<InputState>,
    pub(super) cue_text: Entity<InputState>,
    pub(super) subtitles: Option<SubtitleTrack>,
    pub(super) selected_cue: Option<usize>,
    pub(super) clip_path: Entity<InputState>,
    pub(super) clip_start_ms: Entity<InputState>,
    pub(super) clip_end_ms: Entity<InputState>,
    pub(super) clip_split_ms: Entity<InputState>,
    pub(super) clip_include_audio: bool,
    pub(super) clip_info: Option<VideoClipInfo>,
    clip_probe: Option<ClipProbeJob>,
    hydrated: Option<(u64, u64)>,
}

impl MotionUiState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let mut input = |value| cx.new(|cx| InputState::new(window, cx).default_value(value));
        Self {
            duration: input("3"),
            fps: input("30"),
            audio_path: input(""),
            audio_offset_ms: input("0"),
            audio_source_start_ms: input("0"),
            audio_duration_ms: input("0"),
            audio_volume: input("1"),
            subtitle_path: input(""),
            subtitle_font_size: input("42"),
            subtitle_outline: input("2"),
            subtitle_presentation: SubtitlePresentation::Soft,
            subtitle_position: SubtitlePosition::Bottom,
            cue_start_ms: input("0"),
            cue_end_ms: input("1000"),
            cue_text: input(""),
            subtitles: None,
            selected_cue: None,
            clip_path: input(""),
            clip_start_ms: input("0"),
            clip_end_ms: input("15000"),
            // A split is an explicit operation.  Leaving this empty keeps a
            // normal trimmed-clip export independent of the optional split.
            clip_split_ms: input(""),
            clip_include_audio: true,
            clip_info: None,
            clip_probe: None,
            hydrated: None,
        }
    }
}

#[derive(Clone, Copy)]
enum MotionPreset {
    Duration,
    Fade,
    FadeOut,
    Rise,
    Pan,
    CrossFade,
    Slide(SlideDirection),
    Clear,
}

#[derive(Clone, Copy)]
enum MotionInput {
    Audio,
    Subtitles,
    Clip,
}

fn motion_section(title: &str, cx: &App) -> Div {
    panel_section(title.to_owned(), cx)
}

fn motion_note(value: impl Into<SharedString>, cx: &App) -> Div {
    panel_note(value, cx)
}

impl EditorView {
    pub(super) fn create_motion_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.create.job.is_some();
        let mut cue_list = div().flex().flex_col().gap_1();
        let cue_count = self
            .create
            .motion
            .subtitles
            .as_ref()
            .map_or(0, |track| track.cues.len());
        if let Some(track) = &self.create.motion.subtitles {
            for (index, cue) in track.cues.iter().enumerate() {
                let label = format!(
                    "{}–{} ms  {}",
                    cue.start_ms,
                    cue.end_ms,
                    cue.text.lines().next().unwrap_or_default()
                );
                cue_list = cue_list.child(
                    button(
                        SharedString::from(format!("motion-cue-{index}")),
                        label,
                        if self.create.motion.selected_cue == Some(index) {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        cx,
                    )
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_motion_cue(index, window, cx);
                    })),
                );
            }
        }
        let mut panel = motion_section("MOTION", cx)
            .child(motion_note(
                "Animate native layers and pages, then encode locally with FFmpeg.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Page seconds", cx))
                            .child(input(
                                "motion-seconds",
                                &self.create.motion.duration,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Frames / second", cx))
                            .child(input("motion-fps", &self.create.motion.fps, window, cx)),
                    ),
            )
            .child(motion_note("Selected layer animation", cx))
            .child(
                button(
                    "motion-apply-duration",
                    "Apply duration to this page",
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.apply_motion_preset(MotionPreset::Duration, cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        button("motion-fade", "Fade", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(MotionPreset::Fade, cx);
                                this.create_error(result, cx);
                            }),
                        ).flex_1().min_w(px(116.)),
                    )
                    .child(
                        button("motion-fade-out", "Fade out", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(MotionPreset::FadeOut, cx);
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button("motion-rise", "Rise", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(MotionPreset::Rise, cx);
                                this.create_error(result, cx);
                            }),
                        ).flex_1().min_w(px(116.)),
                    )
                    .child(
                        button("motion-pan", "Slow pan", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(MotionPreset::Pan, cx);
                                this.create_error(result, cx);
                            }),
                        ).flex_1().min_w(px(116.)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        button(
                            "motion-scrub-start",
                            "Page start",
                            ButtonVariant::Secondary,
                            cx,
                        )
                            .flex_1()
                            .min_w(px(116.))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.preview_motion_at(0.0, cx);
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button(
                            "motion-scrub-middle",
                            "Page middle",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .min_w(px(116.))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.preview_motion_at(0.5, cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button(
                            "motion-scrub-end",
                            "Page end",
                            ButtonVariant::Secondary,
                            cx,
                        )
                            .flex_1()
                            .min_w(px(116.))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.preview_motion_at(1.0, cx);
                                this.create_error(result, cx);
                            })),
                    ),
            )
            .child(
                button(
                    "motion-play",
                    "Play project",
                    ButtonVariant::Primary,
                    cx,
                )
                    .w_full()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.play_motion_preview(cx);
                        this.create_error(result, cx);
                    })),
            )
            .child(
                button(
                    "motion-return-canvas",
                    "Return to editable canvas",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(job) = &this.create.job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    this.status = "Editable canvas".into();
                    this.refresh(cx);
                })),
            )
            .child(motion_note(
                "Page controls preview the active page. Play project previews every page and its transitions visually; audio and subtitles are verified in an exported MP4.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "motion-crossfade",
                            "Crossfade to next page",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .min_w_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.apply_motion_preset(MotionPreset::CrossFade, cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button("motion-clear", "Clear", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(MotionPreset::Clear, cx);
                                this.create_error(result, cx);
                            }),
                        ),
                    ),
            )
            .child(motion_note("Directional page transition", cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        button("motion-slide-left", "Slide ←", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(
                                    MotionPreset::Slide(SlideDirection::Left),
                                    cx,
                                );
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button(
                            "motion-slide-right",
                            "Slide →",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .min_w(px(116.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.apply_motion_preset(
                                MotionPreset::Slide(SlideDirection::Right),
                                cx,
                            );
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button("motion-slide-up", "Slide ↑", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(
                                    MotionPreset::Slide(SlideDirection::Up),
                                    cx,
                                );
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button("motion-slide-down", "Slide ↓", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.apply_motion_preset(
                                    MotionPreset::Slide(SlideDirection::Down),
                                    cx,
                                );
                                this.create_error(result, cx);
                            })),
                    ),
            )
            .child(motion_note("Audio (MP4)", cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(input(
                        "motion-audio-path",
                        &self.create.motion.audio_path,
                        window,
                        cx,
                    )))
                    .child(
                        button("motion-audio", "Choose…", ButtonVariant::Secondary, cx).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.choose_motion_input(MotionInput::Audio, window, cx)
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_w(px(132.))
                            .child(motion_note("Offset ms", cx))
                            .child(input(
                                "motion-audio-offset",
                                &self.create.motion.audio_offset_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_w(px(132.))
                            .child(motion_note("Source ms", cx))
                            .child(input(
                                "motion-audio-source-start",
                                &self.create.motion.audio_source_start_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_w(px(132.))
                            .child(motion_note("Length ms (0 = rest)", cx))
                            .child(input(
                                "motion-audio-length",
                                &self.create.motion.audio_duration_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_w(px(132.))
                            .child(motion_note("Volume (0–4)", cx))
                            .child(input(
                                "motion-audio-volume",
                                &self.create.motion.audio_volume,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(motion_note("Subtitles — SRT or WebVTT (MP4)", cx))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(input(
                        "motion-subtitles-path",
                        &self.create.motion.subtitle_path,
                        window,
                        cx,
                    )))
                    .child(
                        button("motion-subtitles", "Choose…", ButtonVariant::Secondary, cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_motion_input(MotionInput::Subtitles, window, cx)
                            })),
                    ),
            )
            .child(motion_note(
                format!("Editable subtitle cues ({cue_count})"),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        button(
                            "motion-subtitles-soft",
                            "Soft subtitle track",
                            if self.create.motion.subtitle_presentation
                                == SubtitlePresentation::Soft
                            {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.create.motion.subtitle_presentation = SubtitlePresentation::Soft;
                            this.refresh(cx);
                        })),
                    )
                    .child(
                        button(
                            "motion-subtitles-burn",
                            "Burn in readable subtitles",
                            if self.create.motion.subtitle_presentation
                                == SubtitlePresentation::BurnIn
                            {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.create.motion.subtitle_presentation = SubtitlePresentation::BurnIn;
                            this.refresh(cx);
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Font px (12–192)", cx))
                            .child(input(
                                "motion-subtitle-font-size",
                                &self.create.motion.subtitle_font_size,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Outline px (1–12)", cx))
                            .child(input(
                                "motion-subtitle-outline",
                                &self.create.motion.subtitle_outline,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "motion-subtitle-top",
                            "Top",
                            if self.create.motion.subtitle_position == SubtitlePosition::Top {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.create.motion.subtitle_position = SubtitlePosition::Top;
                            this.refresh(cx);
                        })),
                    )
                    .child(
                        button(
                            "motion-subtitle-centre",
                            "Centre",
                            if self.create.motion.subtitle_position == SubtitlePosition::Centre {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.create.motion.subtitle_position = SubtitlePosition::Centre;
                            this.refresh(cx);
                        })),
                    )
                    .child(
                        button(
                            "motion-subtitle-bottom",
                            "Bottom",
                            if self.create.motion.subtitle_position == SubtitlePosition::Bottom {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.create.motion.subtitle_position = SubtitlePosition::Bottom;
                            this.refresh(cx);
                        })),
                    ),
            )
            .child(cue_list)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Start ms", cx))
                            .child(input(
                                "motion-cue-start",
                                &self.create.motion.cue_start_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("End ms", cx))
                            .child(input(
                                "motion-cue-end",
                                &self.create.motion.cue_end_ms,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(div().flex_1().min_w(px(132.)).child(input(
                        "motion-cue-text",
                        &self.create.motion.cue_text,
                        window,
                        cx,
                    )))
                    .child(
                        button(
                            "motion-cue-upsert",
                            if self.create.motion.selected_cue.is_some() {
                                "Update cue"
                            } else {
                                "Add cue"
                            },
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .min_w(px(116.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.upsert_motion_cue(cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button("motion-cue-remove", "Remove", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .disabled(self.create.motion.selected_cue.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.remove_motion_cue(cx);
                                this.create_error(result, cx);
                            })),
                    ),
            )
            .child(motion_note("Short source clip", cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(div().flex_1().min_w(px(132.)).child(input(
                        "motion-clip-path",
                        &self.create.motion.clip_path,
                        window,
                        cx,
                    )))
                    .child(
                        button("motion-clip", "Choose…", ButtonVariant::Secondary, cx)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_motion_input(MotionInput::Clip, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(132.))
                            .child(motion_note("Trim start ms", cx))
                            .child(input(
                                "motion-clip-start",
                                &self.create.motion.clip_start_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(132.))
                            .child(motion_note("Trim end ms", cx))
                            .child(input(
                                "motion-clip-end",
                                &self.create.motion.clip_end_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        button("motion-clip-probe", "Probe", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .min_w(px(116.))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let result = this.probe_motion_clip(window, cx);
                                this.create_error(result, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(motion_note("Split at ms", cx))
                            .child(input(
                                "motion-clip-split",
                                &self.create.motion.clip_split_ms,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        button(
                            "motion-prepare-split",
                            "Prepare two clips…",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_clip_split_destination(window, cx)
                        })),
                    ),
            )
            .child(
                button(
                    "motion-clip-audio",
                    if self.create.motion.clip_include_audio {
                        "Prepared clip audio: included"
                    } else {
                        "Prepared clip audio: muted"
                    },
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.create.motion.clip_include_audio = !this.create.motion.clip_include_audio;
                    this.refresh(cx);
                })),
            )
            .child(
                button(
                    "motion-prepare-clip",
                    "Prepare trimmed clip…",
                    ButtonVariant::Secondary,
                    cx,
                )
                .disabled(busy)
                .on_click(
                    cx.listener(|this, _, window, cx| this.choose_clip_destination(window, cx)),
                ),
            )
            .child(
                button(
                    "motion-save-media",
                    "Save media settings",
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.persist_motion_media(cx);
                    this.create_error(result, cx);
                })),
            );
        if let Some(info) = &self.create.motion.clip_info {
            panel = panel.child(motion_note(
                format!(
                    "Source: {} × {} · {} ms · {}",
                    info.width,
                    info.height,
                    info.duration_ms,
                    if info.has_audio { "audio" } else { "silent" }
                ),
                cx,
            ));
        }
        panel
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "motion-preview",
                            "Export visual preview GIF…",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .disabled(busy)
                        .flex_1()
                        .min_w_0()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_motion_export(MotionFormat::Gif, window, cx)
                            })),
                    )
                    .child(
                        button("motion-export-mp4", "MP4…", ButtonVariant::Primary, cx)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_motion_export(MotionFormat::Mp4, window, cx)
                            })),
                    ),
            )
            .child(motion_note(
                "GIF is visual only. MP4 export carries the configured audio and soft or burned-in subtitle track.",
                cx,
            ))
            .into_any_element()
    }

    fn preview_motion_at(&mut self, position: f32, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none(),
            "A content job is already running"
        );
        let duration_ms = self.motion_duration_ms(cx)?;
        let mut page = read_page_timeline(&self.editor.document, duration_ms)?;
        page.duration_ms = duration_ms;
        page.transition = PageTransition::None;
        let timeline = MotionTimeline { pages: vec![page] };
        let time_ms =
            ((duration_ms.saturating_sub(1)) as f32 * position.clamp(0.0, 1.0)).round() as u64;
        let pixels = omuse::motion::render_frame(
            std::slice::from_ref(&self.editor.document),
            &timeline,
            time_ms,
        )?;
        self.display.replace(&pixels);
        self.status = format!("Motion preview at {} ms — artwork is unchanged", time_ms);
        cx.notify();
        Ok(())
    }

    fn play_motion_preview(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none(),
            "A content job is already running"
        );
        let duration_ms = self.motion_duration_ms(cx)?;
        let requested_fps = self.motion_fps(cx)?;
        // Snapshot every Create page before starting the worker. `motion_snapshot`
        // materializes lazy pages in that worker, so pressing Play cannot stall the
        // GPUI event loop on a large collection.
        let mut project = self.content_snapshot()?;
        let source = self.motion_source_identity();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let completion_cancel = cancel.clone();
        let (frame_sender, frame_receiver) = mpsc::sync_channel(2);
        let (done_sender, done_receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("omuse-motion-preview".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    anyhow::ensure!(
                        !worker_cancel.load(Ordering::Relaxed),
                        "Motion preview cancelled"
                    );
                    let (documents, timeline) = motion_snapshot(&mut project, duration_ms)?;
                    let total_duration_ms = timeline.duration_ms()?;
                    let (preview_frames, frame_delay_ms) =
                        native_preview_timing(total_duration_ms, requested_fps)?;
                    frame_sender
                        .send(MotionPreviewEvent::Started {
                            pages: documents.len(),
                            frames: preview_frames,
                            frame_delay_ms,
                        })
                        .map_err(|_| anyhow::anyhow!("Motion preview closed"))?;
                    for index in 0..preview_frames {
                        anyhow::ensure!(
                            !worker_cancel.load(Ordering::Relaxed),
                            "Motion preview cancelled"
                        );
                        let time_ms =
                            native_preview_sample_time(index, preview_frames, total_duration_ms);
                        let frame = omuse::motion::render_frame(&documents, &timeline, time_ms)?;
                        frame_sender
                            .send(MotionPreviewEvent::Frame {
                                index: index + 1,
                                total: preview_frames,
                                pixels: frame,
                            })
                            .map_err(|_| anyhow::anyhow!("Motion preview closed"))?;
                    }
                    Ok(())
                })();
                let _ = done_sender.send(result.map_err(|error| format!("{error:#}")));
            })?;
        let (_event_sender, event_receiver) = std::sync::mpsc::sync_channel::<ContentEvent>(1);
        self.create.job = Some(ContentJob {
            cancel,
            receiver: event_receiver,
            detail: "Playing native motion preview…".into(),
        });
        cx.spawn(async move |view, cx| {
            let mut frame_delay_ms = 16;
            let result = loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(frame_delay_ms))
                    .await;
                let mut latest = None;
                let mut started = None;
                while let Ok(event) = frame_receiver.try_recv() {
                    match event {
                        MotionPreviewEvent::Started {
                            pages,
                            frames,
                            frame_delay_ms: delay,
                        } => {
                            frame_delay_ms = delay;
                            started = Some((pages, frames));
                        }
                        MotionPreviewEvent::Frame {
                            index,
                            total,
                            pixels,
                        } => latest = Some((index, total, pixels)),
                    }
                }
                if started.is_some() || latest.is_some() {
                    let current = view
                        .update(cx, |this, cx| {
                            if !this.motion_source_is_current(&source) {
                                completion_cancel.store(true, Ordering::Relaxed);
                                return false;
                            }
                            if let Some(job) = &mut this.create.job {
                                if let Some((pages, frames)) = started.as_ref() {
                                    job.detail = format!(
                                        "Playing {pages}-page timeline with transitions ({frames} frames)…"
                                    );
                                }
                                if let Some((index, total, pixels)) = latest.as_ref() {
                                    this.display.replace(pixels);
                                    job.detail = format!(
                                        "Project preview frame {index} of {total}"
                                    );
                                }
                            }
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !current {
                        break Err("Motion preview stopped because the page changed".into());
                    }
                }
                if completion_cancel.load(Ordering::Relaxed) {
                    break Err("Motion preview cancelled".to_owned());
                }
                match done_receiver.try_recv() {
                    Ok(result) => break result,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break Err("Motion preview stopped unexpectedly".into());
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            };
            let _ = view.update(cx, |this, cx| {
                this.create.job = None;
                this.status = match result {
                    Ok(()) => {
                        "Project motion preview finished — artwork is unchanged; audio and subtitles are exported with MP4."
                            .into()
                    }
                    Err(error) => error,
                };
                this.refresh(cx);
            });
        })
        .detach();
        Ok(())
    }

    fn motion_source_identity(&self) -> MotionSourceIdentity {
        MotionSourceIdentity {
            create_epoch: self.create.epoch,
            editor_revision: self.editor.revision(),
            collection: self.create.session.as_ref().map(|session| {
                (
                    session.project.id.clone(),
                    session.generation,
                    session.project.active_page_id().to_owned(),
                )
            }),
        }
    }

    fn motion_source_is_current(&self, source: &MotionSourceIdentity) -> bool {
        self.motion_source_identity() == *source
    }

    /// Keep the media controls in sync with the editable document. The page
    /// revision is part of the identity because collection page switches can
    /// retain the same Create epoch.
    pub(super) fn sync_motion_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let identity = (self.create.epoch, self.editor.revision());
        if self.create.motion.hydrated == Some(identity) {
            return;
        }
        let media = match omuse::motion::motion_media(&self.editor.document) {
            Ok(media) => media,
            Err(error) => {
                self.status = format!("Motion media could not be read: {error:#}");
                self.create.motion.hydrated = Some(identity);
                return;
            }
        };
        let duration_ms = read_page_timeline(&self.editor.document, 3_000)
            .map(|page| page.duration_ms)
            .unwrap_or(3_000);
        set_motion_input(
            &self.create.motion.duration,
            format_duration_seconds(duration_ms),
            window,
            cx,
        );
        if let Some(audio) = &media.audio {
            set_motion_input(
                &self.create.motion.audio_path,
                audio.path.to_string_lossy().to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.audio_offset_ms,
                audio.timeline_start_ms.to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.audio_source_start_ms,
                audio.source_start_ms.to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.audio_duration_ms,
                audio.duration_ms.to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.audio_volume,
                audio.volume.to_string(),
                window,
                cx,
            );
        } else {
            for (input, value) in [
                (&self.create.motion.audio_path, ""),
                (&self.create.motion.audio_offset_ms, "0"),
                (&self.create.motion.audio_source_start_ms, "0"),
                (&self.create.motion.audio_duration_ms, "0"),
                (&self.create.motion.audio_volume, "1"),
            ] {
                set_motion_input(input, value.to_owned(), window, cx);
            }
        }
        if let Some(clip) = &media.clip {
            set_motion_input(
                &self.create.motion.clip_path,
                clip.path.to_string_lossy().to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.clip_start_ms,
                clip.source_start_ms.to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.clip_end_ms,
                clip.source_end_ms.to_string(),
                window,
                cx,
            );
            set_motion_input(
                &self.create.motion.clip_split_ms,
                clip.split_at_ms
                    .map_or_else(String::new, |value| value.to_string()),
                window,
                cx,
            );
            self.create.motion.clip_include_audio = clip.include_audio;
        } else {
            set_motion_input(&self.create.motion.clip_path, String::new(), window, cx);
            set_motion_input(&self.create.motion.clip_start_ms, "0".into(), window, cx);
            set_motion_input(&self.create.motion.clip_end_ms, "15000".into(), window, cx);
            set_motion_input(&self.create.motion.clip_split_ms, String::new(), window, cx);
            self.create.motion.clip_include_audio = true;
        }
        set_motion_input(
            &self.create.motion.subtitle_path,
            media
                .subtitle_source_path
                .as_ref()
                .map_or_else(String::new, |path| path.to_string_lossy().to_string()),
            window,
            cx,
        );
        set_motion_input(
            &self.create.motion.subtitle_font_size,
            media.subtitle_style.font_size_px.to_string(),
            window,
            cx,
        );
        set_motion_input(
            &self.create.motion.subtitle_outline,
            media.subtitle_style.outline_px.to_string(),
            window,
            cx,
        );
        self.create.motion.subtitle_presentation = media.subtitle_style.presentation;
        self.create.motion.subtitle_position = media.subtitle_style.position;
        self.create.motion.subtitles = media.subtitles;
        self.create.motion.selected_cue = None;
        self.create.motion.clip_info = None;
        self.create.motion.hydrated = Some(identity);
    }

    fn motion_media_from_controls(&self, cx: &App) -> anyhow::Result<MotionMedia> {
        let audio_path = motion_input_value(&self.create.motion.audio_path, cx);
        let clip_path = motion_input_value(&self.create.motion.clip_path, cx);
        let audio = if audio_path.is_empty() {
            None
        } else {
            Some(MotionAudioMedia {
                path: PathBuf::from(audio_path),
                timeline_start_ms: motion_u64(
                    &self.create.motion.audio_offset_ms,
                    "Audio offset",
                    cx,
                )?,
                source_start_ms: motion_u64(
                    &self.create.motion.audio_source_start_ms,
                    "Audio source start",
                    cx,
                )?,
                duration_ms: motion_u64(&self.create.motion.audio_duration_ms, "Audio length", cx)?,
                volume: motion_f32(&self.create.motion.audio_volume, "Audio volume", cx)?,
            })
        };
        let clip = if clip_path.is_empty() {
            None
        } else {
            let split = motion_optional_u64(&self.create.motion.clip_split_ms, "Clip split", cx)?;
            Some(MotionClipMedia {
                path: PathBuf::from(clip_path),
                source_start_ms: motion_u64(
                    &self.create.motion.clip_start_ms,
                    "Clip trim start",
                    cx,
                )?,
                source_end_ms: motion_u64(&self.create.motion.clip_end_ms, "Clip trim end", cx)?,
                split_at_ms: split,
                include_audio: self.create.motion.clip_include_audio,
            })
        };
        let media = MotionMedia {
            version: omuse::motion::MOTION_MEDIA_VERSION,
            audio,
            subtitles: self.create.motion.subtitles.clone(),
            subtitle_style: SubtitleStyle {
                presentation: self.create.motion.subtitle_presentation,
                font_size_px: motion_u32(
                    &self.create.motion.subtitle_font_size,
                    "Subtitle font size",
                    cx,
                )?,
                outline_px: motion_u32(
                    &self.create.motion.subtitle_outline,
                    "Subtitle outline",
                    cx,
                )?,
                position: self.create.motion.subtitle_position,
            },
            subtitle_source_path: match motion_input_value(&self.create.motion.subtitle_path, cx) {
                value if value.is_empty() => None,
                value => Some(PathBuf::from(value)),
            },
            clip,
        };
        media.validate()?;
        Ok(media)
    }

    fn persist_motion_media(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let media = self.motion_media_from_controls(cx)?;
        let mut document = self.editor.document.clone();
        omuse::motion::set_motion_media(&mut document, &media)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.status = "Motion media settings saved with this canvas".into();
        Ok(())
    }

    fn select_motion_cue(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cue) = self
            .create
            .motion
            .subtitles
            .as_ref()
            .and_then(|track| track.cues.get(index))
            .cloned()
        else {
            return;
        };
        set_motion_input(
            &self.create.motion.cue_start_ms,
            cue.start_ms.to_string(),
            window,
            cx,
        );
        set_motion_input(
            &self.create.motion.cue_end_ms,
            cue.end_ms.to_string(),
            window,
            cx,
        );
        set_motion_input(&self.create.motion.cue_text, cue.text, window, cx);
        self.create.motion.selected_cue = Some(index);
        self.refresh(cx);
    }

    fn upsert_motion_cue(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let cue = SubtitleCue {
            start_ms: motion_u64(&self.create.motion.cue_start_ms, "Cue start", cx)?,
            end_ms: motion_u64(&self.create.motion.cue_end_ms, "Cue end", cx)?,
            text: motion_input_value(&self.create.motion.cue_text, cx),
        };
        anyhow::ensure!(!cue.text.is_empty(), "Subtitle cue text is empty");
        anyhow::ensure!(
            cue.end_ms > cue.start_ms,
            "Subtitle cue end must follow its start"
        );
        let track = self
            .create
            .motion
            .subtitles
            .get_or_insert_with(SubtitleTrack::default);
        if let Some(index) = self.create.motion.selected_cue {
            anyhow::ensure!(
                index < track.cues.len(),
                "Selected subtitle cue no longer exists"
            );
            track.cues[index] = cue.clone();
        } else {
            track.cues.push(cue.clone());
        }
        track.cues.sort_by(|left, right| {
            (left.start_ms, left.end_ms, &left.text).cmp(&(
                right.start_ms,
                right.end_ms,
                &right.text,
            ))
        });
        track.validate(None)?;
        self.create.motion.selected_cue = track.cues.iter().position(|current| current == &cue);
        self.persist_motion_media(cx)
    }

    fn remove_motion_cue(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let index = self
            .create
            .motion
            .selected_cue
            .ok_or_else(|| anyhow::anyhow!("Choose a subtitle cue to remove"))?;
        let track = self
            .create
            .motion
            .subtitles
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("There are no subtitle cues"))?;
        anyhow::ensure!(
            index < track.cues.len(),
            "Selected subtitle cue no longer exists"
        );
        track.cues.remove(index);
        self.create.motion.selected_cue = None;
        self.persist_motion_media(cx)
    }

    fn probe_motion_clip(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none() && self.create.motion.clip_probe.is_none(),
            "A content job is already running"
        );
        let source = ClipProbeSource {
            path: motion_input_value(&self.create.motion.clip_path, cx),
            start: motion_input_value(&self.create.motion.clip_start_ms, cx),
            end: motion_input_value(&self.create.motion.clip_end_ms, cx),
            split: motion_input_value(&self.create.motion.clip_split_ms, cx),
        };
        anyhow::ensure!(!source.path.is_empty(), "Choose a source clip first");
        // Parse on the UI thread so validation errors are immediate. FFmpeg
        // discovery and every filesystem/process interaction stay in the worker.
        clip_probe_range_from_source(&source)?;
        let identity = self.motion_source_identity();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_path = PathBuf::from(&source.path);
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("omuse-clip-probe".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<VideoClipInfo> {
                    anyhow::ensure!(
                        !worker_cancel.load(Ordering::Relaxed),
                        "Clip probe cancelled"
                    );
                    let ffmpeg = omuse::motion::Ffmpeg::discover()?;
                    anyhow::ensure!(
                        !worker_cancel.load(Ordering::Relaxed),
                        "Clip probe cancelled"
                    );
                    omuse::motion::probe_video_clip_cancellable(
                        &ffmpeg,
                        &worker_path,
                        &worker_cancel,
                    )
                })();
                let _ = sender.send(result.map_err(|error| format!("{error:#}")));
            })?;
        let (_event_sender, event_receiver) = mpsc::sync_channel::<ContentEvent>(1);
        self.create.job = Some(ContentJob {
            cancel: cancel.clone(),
            receiver: event_receiver,
            detail: "Probing source clip with FFprobe…".into(),
        });
        self.create.motion.clip_probe = Some(ClipProbeJob {
            cancel,
            receiver,
            source,
            identity,
            started: Instant::now(),
        });
        self.status = "Probing source clip…".into();
        self.poll_motion_clip_probe(window, cx);
        cx.notify();
        Ok(())
    }

    fn poll_motion_clip_probe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let keep = view
                    .update_in(cx, |this, window, cx| {
                        let outcome = {
                            let Some(probe) = this.create.motion.clip_probe.as_mut() else {
                                return false;
                            };
                            if probe.cancel.load(Ordering::Relaxed) {
                                Some(ClipProbeCompletion::Cancelled)
                            } else if probe.started.elapsed() >= CLIP_PROBE_UI_TIMEOUT {
                                probe.cancel.store(true, Ordering::Relaxed);
                                Some(ClipProbeCompletion::TimedOut)
                            } else {
                                match probe.receiver.try_recv() {
                                    Ok(result) => Some(ClipProbeCompletion::Finished(result)),
                                    Err(mpsc::TryRecvError::Empty) => None,
                                    Err(mpsc::TryRecvError::Disconnected) => {
                                        Some(ClipProbeCompletion::Finished(Err(
                                            "The clip probe worker stopped before it could finish"
                                                .into(),
                                        )))
                                    }
                                }
                            }
                        };
                        let Some(outcome) = outcome else {
                            return true;
                        };
                        let probe = this
                            .create
                            .motion
                            .clip_probe
                            .take()
                            .expect("clip probe completion has a job");
                        this.create.job = None;
                        if !this.motion_source_is_current(&probe.identity)
                            || !clip_probe_source_matches(&probe.source, this, cx)
                        {
                            this.status =
                                "Clip probe result discarded because its source or trim controls changed"
                                    .into();
                        } else {
                            match outcome {
                                ClipProbeCompletion::Cancelled => {
                                    this.status = "Clip probe cancelled".into();
                                }
                                ClipProbeCompletion::TimedOut => {
                                    this.status = format!(
                                        "Clip probe timed out after {} seconds; verify the source and try again",
                                        CLIP_PROBE_UI_TIMEOUT.as_secs()
                                    );
                                }
                                ClipProbeCompletion::Finished(Err(error)) => {
                                    this.status = format!("Clip probe: {error}");
                                }
                                ClipProbeCompletion::Finished(Ok(info)) => {
                                    let result = this.apply_clip_probe_result(
                                        &probe.source,
                                        info,
                                        window,
                                        cx,
                                    );
                                    this.create_error(result, cx);
                                }
                            }
                        }
                        cx.notify();
                        false
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_clip_probe_result(
        &mut self,
        source: &ClipProbeSource,
        info: VideoClipInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let (start, end, split) = clip_probe_range_from_source(source)?;
        let (end, split) = normalize_clip_probe_range(&info, start, end, split)?;
        set_motion_input(&self.create.motion.clip_end_ms, end.to_string(), window, cx);
        set_motion_input(
            &self.create.motion.clip_split_ms,
            split.to_string(),
            window,
            cx,
        );
        self.create.motion.clip_info = Some(info.clone());
        self.status = format!(
            "Clip verified: {} × {} · {} ms · {} · trim {}–{} ms",
            info.width,
            info.height,
            info.duration_ms,
            if info.has_audio {
                "audio available"
            } else {
                "no audio"
            },
            start,
            end,
        );
        Ok(())
    }

    fn apply_motion_preset(
        &mut self,
        preset: MotionPreset,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if matches!(preset, MotionPreset::CrossFade | MotionPreset::Slide(_)) {
            anyhow::ensure!(
                self.motion_has_next_page(),
                "A page transition needs a following Create page"
            );
        }
        let duration_ms = self.motion_duration_ms(cx)?;
        let mut document = self.editor.document.clone();
        let mut page = read_page_timeline(&document, duration_ms)?;
        page.duration_ms = duration_ms;
        match preset {
            MotionPreset::Duration => {}
            MotionPreset::Clear => {
                page.tracks.clear();
                page.transition = PageTransition::None;
            }
            MotionPreset::CrossFade => {
                page.transition = PageTransition::CrossFade {
                    duration_ms: 400.min(duration_ms.saturating_sub(1)),
                };
            }
            MotionPreset::Slide(direction) => {
                page.transition = PageTransition::Slide {
                    duration_ms: 400.min(duration_ms.saturating_sub(1)),
                    direction,
                };
            }
            preset => {
                let layer_id = self.editor.active_layer.clone();
                anyhow::ensure!(
                    document.find_layer(&layer_id).is_some(),
                    "Choose a layer to animate"
                );
                let entrance = 500.min(duration_ms);
                let animations = match preset {
                    MotionPreset::Fade => vec![LayerAnimation::fade_in(0, entrance)],
                    MotionPreset::FadeOut => vec![LayerAnimation::fade_out(
                        duration_ms.saturating_sub(entrance),
                        entrance,
                    )],
                    MotionPreset::Rise => LayerAnimation::rise_in(0, entrance, 48.0).to_vec(),
                    MotionPreset::Pan => vec![LayerAnimation {
                        start_ms: 0,
                        end_ms: duration_ms,
                        easing: Easing::EaseInOut,
                        animation: LayerAnimationKind::Pan {
                            from_x: -24.0,
                            from_y: 0.0,
                            to_x: 24.0,
                            to_y: 0.0,
                        },
                    }],
                    _ => unreachable!(),
                };
                page.tracks.retain(|track| track.layer_id != layer_id);
                page.tracks.push(LayerTrack {
                    layer_id,
                    animations,
                });
            }
        }
        document.metadata["omuseMotion"] = serde_json::to_value(page)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        Ok(())
    }

    fn motion_has_next_page(&self) -> bool {
        self.create.session.as_ref().is_some_and(|session| {
            let pages = session.project.page_ids();
            pages
                .iter()
                .position(|id| id == session.project.active_page_id())
                .is_some_and(|index| index + 1 < pages.len())
        })
    }

    fn motion_duration_ms(&self, cx: &App) -> anyhow::Result<u32> {
        let seconds = motion_input_value(&self.create.motion.duration, cx).parse::<f64>()?;
        anyhow::ensure!(
            seconds.is_finite() && (0.25..=120.0).contains(&seconds),
            "Page duration must be 0.25–120 seconds"
        );
        Ok((seconds * 1000.0).round() as u32)
    }

    fn motion_fps(&self, cx: &App) -> anyhow::Result<u32> {
        let fps = motion_input_value(&self.create.motion.fps, cx).parse::<u32>()?;
        anyhow::ensure!((1..=60).contains(&fps), "Frame rate must be 1–60 fps");
        Ok(fps)
    }

    fn choose_motion_input(
        &mut self,
        kind: MotionInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = match kind {
            MotionInput::Audio => "Choose audio for MP4",
            MotionInput::Subtitles => "Choose SRT or WebVTT subtitles",
            MotionInput::Clip => "Choose a short video clip",
        };
        let task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt.into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let paths = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if let Ok(Ok(Some(paths))) = paths
                    && let Some(path) = paths.into_iter().next()
                {
                    let result = match kind {
                        MotionInput::Audio => {
                            set_motion_input(
                                &this.create.motion.audio_path,
                                path.to_string_lossy().to_string(),
                                window,
                                cx,
                            );
                            this.persist_motion_media(cx)
                        }
                        MotionInput::Subtitles => (|| -> anyhow::Result<()> {
                            let track = SubtitleTrack::load(&path)?;
                            set_motion_input(
                                &this.create.motion.subtitle_path,
                                path.to_string_lossy().to_string(),
                                window,
                                cx,
                            );
                            this.create.motion.subtitles = Some(track);
                            this.create.motion.selected_cue = None;
                            this.persist_motion_media(cx)
                        })(),
                        MotionInput::Clip => {
                            set_motion_input(
                                &this.create.motion.clip_path,
                                path.to_string_lossy().to_string(),
                                window,
                                cx,
                            );
                            this.create.motion.clip_info = None;
                            this.persist_motion_media(cx)
                        }
                    };
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }

    fn choose_motion_export(
        &mut self,
        format: MotionFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.create.job.is_some() {
            return;
        }
        let extension = match format {
            MotionFormat::Mp4 => "mp4",
            MotionFormat::Gif => "gif",
        };
        let directory = omuse::identity::media_dir(
            omuse::identity::home_dir().unwrap_or_default(),
            omuse::identity::MediaFolder::Videos,
        );
        let task = cx.prompt_for_new_path(&directory, Some(&format!("Omuse motion.{extension}")));
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if let Ok(Ok(Some(mut path))) = result {
                    path.set_extension(extension);
                    let result = this.start_motion_export(path, format, cx);
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }

    fn start_motion_export(
        &mut self,
        path: PathBuf,
        format: MotionFormat,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none(),
            "A content job is already running"
        );
        self.persist_motion_media(cx)?;
        let media = omuse::motion::motion_media(&self.editor.document)?;
        let mut project = self.content_snapshot()?;
        let duration_ms = self.motion_duration_ms(cx)?;
        let fps = self.motion_fps(cx)?;
        let (documents, timeline) = motion_snapshot(&mut project, duration_ms)?;
        let total_duration = timeline.duration_ms()?;
        let mut options = MotionExportOptions {
            frames_per_second: fps,
            ..Default::default()
        };
        if format == MotionFormat::Mp4 {
            if let Some(audio) = media.audio_clip(total_duration)? {
                options.audio.push(audio);
            }
            options.subtitles = media.subtitles;
            options.subtitle_style = media.subtitle_style;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(32);
        std::thread::Builder::new()
            .name("omuse-motion-export".into())
            .spawn(move || {
                let result = omuse::motion::export_motion(
                    &documents,
                    &timeline,
                    &path,
                    format,
                    options,
                    &worker_cancel,
                    |progress| {
                        let _ = sender.try_send(ContentEvent::Progress(format!(
                            "Rendering frame {} of {}",
                            progress.rendered_frames, progress.total_frames
                        )));
                    },
                );
                let _ = sender.send(ContentEvent::Finished(
                    result.map_err(|error| format!("{error:#}")),
                ));
            })?;
        self.create.job = Some(ContentJob {
            cancel,
            receiver,
            detail: "Starting motion export…".into(),
        });
        self.poll_content_job(cx);
        Ok(())
    }

    fn choose_clip_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.create.job.is_some() {
            return;
        }
        let directory = omuse::identity::media_dir(
            omuse::identity::home_dir().unwrap_or_default(),
            omuse::identity::MediaFolder::Videos,
        );
        let task = cx.prompt_for_new_path(&directory, Some("Omuse prepared clip.mp4"));
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if let Ok(Ok(Some(mut path))) = result {
                    path.set_extension("mp4");
                    let result = this.start_clip_transcode(path, cx);
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }

    fn choose_clip_split_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.create.job.is_some() {
            return;
        }
        let directory = omuse::identity::media_dir(
            omuse::identity::home_dir().unwrap_or_default(),
            omuse::identity::MediaFolder::Videos,
        );
        let task = cx.prompt_for_new_path(&directory, Some("Omuse split clips"));
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if let Ok(Ok(Some(path))) = result {
                    let result = this.start_clip_split(path, cx);
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }

    fn start_clip_transcode(
        &mut self,
        destination: PathBuf,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none(),
            "A content job is already running"
        );
        let media = self.motion_media_from_controls(cx)?;
        let source = media
            .clip
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Choose a source clip"))?
            .path
            .clone();
        let mut options = media
            .clip_options()
            .ok_or_else(|| anyhow::anyhow!("Choose a source clip"))?;
        self.persist_motion_media(cx)?;
        let fps = self.motion_fps(cx)?;
        let (width, height) = (self.editor.document.width, self.editor.document.height);
        options.maximum_width = width;
        options.maximum_height = height;
        options.frames_per_second = fps;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("omuse-clip-transcode".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<PathBuf> {
                    let ffmpeg = omuse::motion::Ffmpeg::discover()?;
                    omuse::motion::transcode_video_clip(
                        &ffmpeg,
                        source,
                        destination,
                        options,
                        &worker_cancel,
                    )
                })();
                let _ = sender.send(ContentEvent::Finished(
                    result.map_err(|error| format!("{error:#}")),
                ));
            })?;
        self.create.job = Some(ContentJob {
            cancel,
            receiver,
            detail: "Preparing short clip with FFmpeg…".into(),
        });
        self.poll_content_job(cx);
        Ok(())
    }

    fn start_clip_split(
        &mut self,
        destination: PathBuf,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.job.is_none(),
            "A content job is already running"
        );
        let media = self.motion_media_from_controls(cx)?;
        let source = media
            .clip
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Choose a source clip"))?
            .path
            .clone();
        let mut options = media
            .split_options()
            .ok_or_else(|| anyhow::anyhow!("Set a split point inside the trim range"))?;
        self.persist_motion_media(cx)?;
        options.maximum_width = self.editor.document.width;
        options.maximum_height = self.editor.document.height;
        options.frames_per_second = self.motion_fps(cx)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("omuse-clip-split".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<PathBuf> {
                    let ffmpeg = omuse::motion::Ffmpeg::discover()?;
                    Ok(omuse::motion::split_video_clip(
                        &ffmpeg,
                        source,
                        destination,
                        options,
                        &worker_cancel,
                    )?
                    .directory)
                })();
                let _ = sender.send(ContentEvent::Finished(
                    result.map_err(|error| format!("{error:#}")),
                ));
            })?;
        self.create.job = Some(ContentJob {
            cancel,
            receiver,
            detail: "Preparing two trimmed clips with FFmpeg…".into(),
        });
        self.poll_content_job(cx);
        Ok(())
    }
}

fn native_preview_timing(
    total_duration_ms: u64,
    frames_per_second: u32,
) -> anyhow::Result<(u64, u64)> {
    anyhow::ensure!(
        total_duration_ms > 0,
        "Motion preview has no timeline duration"
    );
    anyhow::ensure!(
        (1..=60).contains(&frames_per_second),
        "Frame rate must be 1–60 fps"
    );
    let full_frames = total_duration_ms
        .saturating_mul(u64::from(frames_per_second))
        .div_ceil(1000);
    let preview_frames = full_frames.clamp(1, NATIVE_PREVIEW_MAX_FRAMES);
    let frame_delay_ms = (NATIVE_PREVIEW_MAX_PLAYBACK_MS / preview_frames)
        .min(1000 / u64::from(frames_per_second))
        .max(1);
    Ok((preview_frames, frame_delay_ms))
}

fn native_preview_sample_time(index: u64, frames: u64, total_duration_ms: u64) -> u64 {
    if frames <= 1 || total_duration_ms <= 1 {
        return 0;
    }
    index
        .min(frames - 1)
        .saturating_mul(total_duration_ms - 1)
        .checked_div(frames - 1)
        .unwrap_or(0)
}

fn clip_probe_source_matches(source: &ClipProbeSource, view: &EditorView, cx: &App) -> bool {
    source.path == motion_input_value(&view.create.motion.clip_path, cx)
        && source.start == motion_input_value(&view.create.motion.clip_start_ms, cx)
        && source.end == motion_input_value(&view.create.motion.clip_end_ms, cx)
        && source.split == motion_input_value(&view.create.motion.clip_split_ms, cx)
}

fn clip_probe_range_from_source(
    source: &ClipProbeSource,
) -> anyhow::Result<(u64, u64, Option<u64>)> {
    let start = motion_u64_value(&source.start, "Clip trim start")?;
    let end = motion_u64_value(&source.end, "Clip trim end")?;
    let split = if source.split.is_empty() {
        None
    } else {
        Some(motion_u64_value(&source.split, "Clip split")?)
    };
    Ok((start, end, split))
}

fn normalize_clip_probe_range(
    info: &VideoClipInfo,
    start: u64,
    requested_end: u64,
    requested_split: Option<u64>,
) -> anyhow::Result<(u64, u64)> {
    let end = requested_end
        .min(info.duration_ms)
        .min(start.saturating_add(omuse::motion::MAX_CLIP_DURATION_MS));
    anyhow::ensure!(end > start, "Clip trim range is outside the source");
    anyhow::ensure!(
        end.saturating_sub(start) >= 2,
        "Clip trim range must contain at least 2 ms to set a split point"
    );
    let split = requested_split.unwrap_or_else(|| start + (end - start) / 2);
    Ok((end, split.clamp(start + 1, end - 1)))
}

fn set_motion_input(
    input: &Entity<InputState>,
    value: String,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) {
    input.update(cx, |input, cx| input.set_value(value, window, cx));
}

fn motion_input_value(input: &Entity<InputState>, cx: &App) -> String {
    input.read(cx).value().trim().to_owned()
}

fn motion_u64(input: &Entity<InputState>, name: &str, cx: &App) -> anyhow::Result<u64> {
    motion_u64_value(&motion_input_value(input, cx), name)
}

fn motion_u64_value(value: &str, name: &str) -> anyhow::Result<u64> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{name} must be a whole number of milliseconds"))
}

fn motion_optional_u64(
    input: &Entity<InputState>,
    name: &str,
    cx: &App,
) -> anyhow::Result<Option<u64>> {
    let value = motion_input_value(input, cx);
    if value.is_empty() {
        Ok(None)
    } else {
        value
            .parse()
            .map(Some)
            .map_err(|_| anyhow::anyhow!("{name} must be a whole number of milliseconds"))
    }
}

fn motion_u32(input: &Entity<InputState>, name: &str, cx: &App) -> anyhow::Result<u32> {
    let value = motion_u64(input, name, cx)?;
    u32::try_from(value).map_err(|_| anyhow::anyhow!("{name} is too large"))
}

fn motion_f32(input: &Entity<InputState>, name: &str, cx: &App) -> anyhow::Result<f32> {
    motion_input_value(input, cx)
        .parse()
        .map_err(|_| anyhow::anyhow!("{name} must be a number"))
}

fn format_duration_seconds(duration_ms: u32) -> String {
    let seconds = f64::from(duration_ms) / 1000.0;
    if duration_ms % 1000 == 0 {
        (duration_ms / 1000).to_string()
    } else {
        format!("{seconds:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

fn read_page_timeline(
    document: &Document,
    default_duration_ms: u32,
) -> anyhow::Result<PageTimeline> {
    match document.metadata.get("omuseMotion") {
        Some(value) if !value.is_null() => {
            let page: PageTimeline = serde_json::from_value(value.clone())?;
            Ok(page)
        }
        _ => Ok(PageTimeline {
            page_id: String::new(),
            duration_ms: default_duration_ms,
            transition: PageTransition::None,
            tracks: Vec::new(),
        }),
    }
}

fn motion_snapshot(
    project: &mut omuse::create_project::Project,
    default_duration_ms: u32,
) -> anyhow::Result<(Vec<Document>, MotionTimeline)> {
    let mut documents = Vec::new();
    let mut pages = Vec::new();
    project.for_each_page_document(|summary, document| {
        let mut page = read_page_timeline(document, default_duration_ms)?;
        page.page_id = summary.id.clone();
        documents.push(document.clone());
        pages.push(page);
        Ok(())
    })?;
    ensure_final_page_has_no_transition(&pages)?;
    Ok((documents, MotionTimeline { pages }))
}

/// A transition belongs to the page that leads into its successor.  Rejecting
/// stale metadata here is safer than silently changing the user's export.
fn ensure_final_page_has_no_transition(pages: &[PageTimeline]) -> anyhow::Result<()> {
    if let Some(last) = pages.last() {
        anyhow::ensure!(
            last.transition == PageTransition::None,
            "The final Create page has a transition; clear it before exporting motion"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_preview_sampling_covers_transition_and_timeline_tail() {
        // Two 1 s pages with a 200 ms outgoing transition produce an 1.8 s
        // project timeline. The native preview must not collapse this into the
        // active page's 1 s duration.
        let total_duration_ms = 1_800;
        let (frames, frame_delay_ms) = native_preview_timing(total_duration_ms, 30).unwrap();
        assert_eq!(frames, 54);
        assert_eq!(frame_delay_ms, 33);
        let times = (0..frames)
            .map(|index| native_preview_sample_time(index, frames, total_duration_ms))
            .collect::<Vec<_>>();
        assert_eq!(times.first(), Some(&0));
        assert_eq!(times.last(), Some(&(total_duration_ms - 1)));
        assert!(times.iter().any(|time| (800..1_000).contains(time)));
        assert!(times.iter().any(|time| *time > 1_000));
    }

    #[test]
    fn clip_probe_normalizes_only_a_valid_split_range() {
        let info = VideoClipInfo {
            width: 1920,
            height: 1080,
            duration_ms: 10_000,
            has_audio: true,
        };
        assert_eq!(
            normalize_clip_probe_range(&info, 9_000, 20_000, None).unwrap(),
            (10_000, 9_500)
        );
        assert_eq!(
            normalize_clip_probe_range(&info, 100, 500, Some(0)).unwrap(),
            (500, 101)
        );
        assert!(normalize_clip_probe_range(&info, 9_999, 10_000, None).is_err());
        assert!(normalize_clip_probe_range(&info, 10_000, 10_000, None).is_err());
    }

    #[test]
    fn final_transition_is_rejected_instead_of_silently_removed() {
        let pages = vec![PageTimeline {
            page_id: "final".into(),
            duration_ms: 1_000,
            transition: PageTransition::CrossFade { duration_ms: 400 },
            tracks: Vec::new(),
        }];
        assert!(ensure_final_page_has_no_transition(&pages).is_err());
        assert!(
            ensure_final_page_has_no_transition(&[PageTimeline {
                page_id: "final".into(),
                duration_ms: 1_000,
                transition: PageTransition::None,
                tracks: Vec::new(),
            }])
            .is_ok()
        );
    }
}
