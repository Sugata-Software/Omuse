//! Native page/layer motion rendering and bounded FFmpeg export.
//!
//! Omuse evaluates the timeline and composites every frame itself. FFmpeg is a
//! validated encoder process only; it never receives project files or layer
//! metadata. Output is written to a unique sibling and atomically published.

use crate::{model::Document, raster};
use anyhow::{Context, Result, bail, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub const MAX_MOTION_PAGES: usize = 256;
pub const MAX_PAGE_DURATION_MS: u32 = 120_000;
pub const MAX_TIMELINE_DURATION_MS: u64 = 600_000;
pub const MAX_MOTION_FRAMES: u64 = 18_000;
pub const MAX_MOTION_PIXELS: u64 = 16_777_216;
pub const MAX_PIXEL_FRAMES: u64 = 40_000_000_000;
pub const MAX_SUBTITLE_CUES: usize = 10_000;
pub const MAX_AUDIO_CLIPS: usize = 16;
pub const MAX_CLIP_DURATION_MS: u64 = 120_000;
const MAX_SUBTITLE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SUBTITLE_CUE_BYTES: usize = 16 * 1024;
const MAX_SUBTITLE_CUE_LINES: usize = 16;
const MAX_MEDIA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CLIP_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_FFPROBE_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseInOut,
}

impl Default for Easing {
    fn default() -> Self {
        Self::EaseInOut
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerAnimationKind {
    Fade {
        from: f32,
        to: f32,
    },
    Pan {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    },
    Scale {
        from: f32,
        to: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerAnimation {
    pub start_ms: u32,
    pub end_ms: u32,
    pub easing: Easing,
    pub animation: LayerAnimationKind,
}

impl LayerAnimation {
    pub fn fade_in(start_ms: u32, duration_ms: u32) -> Self {
        Self {
            start_ms,
            end_ms: start_ms.saturating_add(duration_ms),
            easing: Easing::EaseInOut,
            animation: LayerAnimationKind::Fade { from: 0.0, to: 1.0 },
        }
    }

    pub fn fade_out(start_ms: u32, duration_ms: u32) -> Self {
        Self {
            start_ms,
            end_ms: start_ms.saturating_add(duration_ms),
            easing: Easing::EaseInOut,
            animation: LayerAnimationKind::Fade { from: 1.0, to: 0.0 },
        }
    }

    pub fn rise_in(start_ms: u32, duration_ms: u32, distance: f32) -> [Self; 2] {
        [
            Self::fade_in(start_ms, duration_ms),
            Self {
                start_ms,
                end_ms: start_ms.saturating_add(duration_ms),
                easing: Easing::EaseInOut,
                animation: LayerAnimationKind::Pan {
                    from_x: 0.0,
                    from_y: distance,
                    to_x: 0.0,
                    to_y: 0.0,
                },
            },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerTrack {
    pub layer_id: String,
    pub animations: Vec<LayerAnimation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageTransition {
    None,
    CrossFade {
        duration_ms: u32,
    },
    Slide {
        duration_ms: u32,
        direction: SlideDirection,
    },
}

impl PageTransition {
    pub fn duration_ms(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::CrossFade { duration_ms } | Self::Slide { duration_ms, .. } => *duration_ms,
        }
    }
}

impl Default for PageTransition {
    fn default() -> Self {
        Self::None
    }
}

impl MotionTimeline {
    pub fn duration_ms(&self) -> Result<u64> {
        ensure!(!self.pages.is_empty(), "motion timeline has no pages");
        let mut duration = 0u64;
        for (index, page) in self.pages.iter().enumerate() {
            ensure!(page.duration_ms > 0, "page duration must be positive");
            let transition = page.transition.duration_ms();
            if index + 1 == self.pages.len() {
                ensure!(transition == 0, "the final page cannot have a transition");
            } else {
                ensure!(
                    transition < page.duration_ms,
                    "transition must be shorter than its page"
                );
            }
            duration = duration
                .checked_add(u64::from(page.duration_ms - transition))
                .context("timeline duration overflow")?;
        }
        // The final page has no transition, so the loop includes its full tail.
        Ok(duration)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageTimeline {
    pub page_id: String,
    pub duration_ms: u32,
    pub transition: PageTransition,
    pub tracks: Vec<LayerTrack>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MotionTimeline {
    pub pages: Vec<PageTimeline>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub cues: Vec<SubtitleCue>,
}

/// MP4 subtitle presentation. Soft tracks remain selectable by the viewer;
/// burn-in is a fixed, readable treatment for platforms that ignore tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitlePresentation {
    #[default]
    Soft,
    BurnIn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitlePosition {
    Top,
    Centre,
    #[default]
    Bottom,
}

/// Styling is deliberately a bounded fixed treatment rather than an arbitrary
/// FFmpeg filter fragment. Cue text stays editable in the document either way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubtitleStyle {
    #[serde(default)]
    pub presentation: SubtitlePresentation,
    #[serde(default = "default_subtitle_font_size")]
    pub font_size_px: u32,
    #[serde(default = "default_subtitle_outline")]
    pub outline_px: u32,
    #[serde(default)]
    pub position: SubtitlePosition,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            presentation: SubtitlePresentation::Soft,
            font_size_px: default_subtitle_font_size(),
            outline_px: default_subtitle_outline(),
            position: SubtitlePosition::Bottom,
        }
    }
}

fn default_subtitle_font_size() -> u32 {
    42
}

fn default_subtitle_outline() -> u32 {
    2
}

impl SubtitleStyle {
    fn validate(&self) -> Result<()> {
        ensure!(
            (12..=192).contains(&self.font_size_px),
            "subtitle font size must be 12–192 px"
        );
        ensure!(
            (1..=12).contains(&self.outline_px),
            "subtitle outline must be 1–12 px"
        );
        Ok(())
    }

    /// FFmpeg turns SRT into ASS using a 384×288 PlayRes by default. Declare
    /// the actual output grid so the public pixel-valued controls stay pixel
    /// valued at every canvas resolution.
    fn ass_force_style(&self, canvas_width: u32, canvas_height: u32) -> String {
        let alignment = match self.position {
            SubtitlePosition::Top => 8,
            SubtitlePosition::Centre => 5,
            SubtitlePosition::Bottom => 2,
        };
        let margin = (self.font_size_px / 2).clamp(16, 128);
        format!(
            "PlayResX={canvas_width},PlayResY={canvas_height},FontSize={},PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,BorderStyle=1,Outline={},Shadow=1,Alignment={alignment},MarginV={margin}",
            self.font_size_px, self.outline_px,
        )
    }
}

impl SubtitleTrack {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= MAX_SUBTITLE_BYTES as u64,
            "subtitle file is not a bounded regular file"
        );
        let input = fs::read_to_string(path)?;
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("srt") => Self::parse_srt(&input),
            Some("vtt") | Some("webvtt") => Self::parse_webvtt(&input),
            _ => bail!("subtitles must be SRT or WebVTT"),
        }
    }

    pub fn parse_srt(input: &str) -> Result<Self> {
        ensure!(
            input.len() <= MAX_SUBTITLE_BYTES,
            "subtitle file is too large"
        );
        let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
        let mut cues = Vec::new();
        for block in normalized
            .split("\n\n")
            .filter(|block| !block.trim().is_empty())
        {
            let lines = block.lines().collect::<Vec<_>>();
            let timing_index = lines
                .iter()
                .position(|line| line.contains("-->"))
                .context("subtitle cue is missing timing")?;
            let (start_ms, end_ms) = parse_timing_line(lines[timing_index], ',')?;
            let text = lines[timing_index + 1..].join("\n");
            cues.push(SubtitleCue {
                start_ms,
                end_ms,
                text,
            });
        }
        let track = Self { cues };
        track.validate(None)?;
        Ok(track)
    }

    pub fn parse_webvtt(input: &str) -> Result<Self> {
        ensure!(
            input.len() <= MAX_SUBTITLE_BYTES,
            "subtitle file is too large"
        );
        let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
        let body = normalized
            .strip_prefix("WEBVTT")
            .context("WebVTT file is missing its WEBVTT header")?;
        let mut cues = Vec::new();
        for block in body.split("\n\n").filter(|block| !block.trim().is_empty()) {
            let lines = block.lines().collect::<Vec<_>>();
            let Some(timing_index) = lines.iter().position(|line| line.contains("-->")) else {
                continue;
            };
            let (start_ms, end_ms) = parse_timing_line(lines[timing_index], '.')?;
            cues.push(SubtitleCue {
                start_ms,
                end_ms,
                text: lines[timing_index + 1..].join("\n"),
            });
        }
        let track = Self { cues };
        track.validate(None)?;
        Ok(track)
    }

    pub fn validate(&self, duration_ms: Option<u64>) -> Result<()> {
        ensure!(
            self.cues.len() <= MAX_SUBTITLE_CUES,
            "too many subtitle cues"
        );
        let mut text_bytes = 0usize;
        let mut previous_start = 0;
        for (index, cue) in self.cues.iter().enumerate() {
            ensure!(cue.start_ms < cue.end_ms, "subtitle cue has invalid timing");
            ensure!(
                index == 0 || cue.start_ms >= previous_start,
                "subtitle cues are out of order"
            );
            if let Some(duration_ms) = duration_ms {
                ensure!(
                    cue.end_ms <= duration_ms,
                    "subtitle cue exceeds the timeline"
                );
            }
            ensure!(!cue.text.contains('\0'), "subtitle cue contains NUL");
            ensure!(
                cue.text.len() <= MAX_SUBTITLE_CUE_BYTES,
                "subtitle cue text is too large"
            );
            ensure!(
                cue.text.lines().count() <= MAX_SUBTITLE_CUE_LINES,
                "subtitle cue has too many lines"
            );
            text_bytes = text_bytes
                .checked_add(cue.text.len())
                .context("subtitle text size overflow")?;
            ensure!(
                text_bytes <= MAX_SUBTITLE_BYTES,
                "subtitle text is too large"
            );
            previous_start = cue.start_ms;
        }
        Ok(())
    }

    pub fn to_srt(&self) -> Result<String> {
        self.validate(None)?;
        let mut output = String::new();
        for (index, cue) in self.cues.iter().enumerate() {
            output.push_str(&(index + 1).to_string());
            output.push('\n');
            output.push_str(&format_time(cue.start_ms, ','));
            output.push_str(" --> ");
            output.push_str(&format_time(cue.end_ms, ','));
            output.push('\n');
            output.push_str(&cue.text);
            output.push_str("\n\n");
        }
        Ok(output)
    }
}

/// Versioned, editable media settings retained with a canvas page. The media
/// files themselves stay in their original user-selected locations; exports
/// validate every path again before invoking FFmpeg.
pub const MOTION_MEDIA_METADATA_KEY: &str = "omuseMotionMedia";
pub const MOTION_MEDIA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionMedia {
    #[serde(default = "motion_media_version")]
    pub version: u32,
    #[serde(default)]
    pub audio: Option<MotionAudioMedia>,
    #[serde(default)]
    pub subtitles: Option<SubtitleTrack>,
    #[serde(default)]
    pub subtitle_style: SubtitleStyle,
    /// Optional provenance only. Exports use the embedded editable cues, so a
    /// moved subtitle file never makes a saved project unexportable.
    #[serde(default)]
    pub subtitle_source_path: Option<PathBuf>,
    #[serde(default)]
    pub clip: Option<MotionClipMedia>,
}

impl Default for MotionMedia {
    fn default() -> Self {
        Self {
            version: MOTION_MEDIA_VERSION,
            audio: None,
            subtitles: None,
            subtitle_style: SubtitleStyle::default(),
            subtitle_source_path: None,
            clip: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionAudioMedia {
    pub path: PathBuf,
    #[serde(default)]
    pub timeline_start_ms: u64,
    #[serde(default)]
    pub source_start_ms: u64,
    /// Zero means extend to the remaining timeline duration at export time.
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default = "default_audio_volume")]
    pub volume: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionClipMedia {
    pub path: PathBuf,
    #[serde(default)]
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    /// Absolute source timestamp where this bounded trim should be split.
    #[serde(default)]
    pub split_at_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub include_audio: bool,
}

fn motion_media_version() -> u32 {
    MOTION_MEDIA_VERSION
}

fn default_audio_volume() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

impl MotionMedia {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == MOTION_MEDIA_VERSION,
            "unsupported motion media version"
        );
        if let Some(audio) = &self.audio {
            ensure!(
                !audio.path.as_os_str().is_empty(),
                "motion audio path is empty"
            );
            ensure!(
                audio.volume.is_finite() && (0.0..=4.0).contains(&audio.volume),
                "motion audio volume is outside 0–4"
            );
            ensure!(
                audio.timeline_start_ms <= MAX_TIMELINE_DURATION_MS
                    && audio.source_start_ms <= MAX_TIMELINE_DURATION_MS
                    && audio.duration_ms <= MAX_TIMELINE_DURATION_MS,
                "motion audio timing exceeds the supported timeline"
            );
        }
        if let Some(subtitles) = &self.subtitles {
            subtitles.validate(None)?;
        }
        self.subtitle_style.validate()?;
        if let Some(path) = &self.subtitle_source_path {
            ensure!(
                !path.as_os_str().is_empty(),
                "motion subtitle source path is empty"
            );
        }
        if let Some(clip) = &self.clip {
            ensure!(
                !clip.path.as_os_str().is_empty(),
                "motion clip path is empty"
            );
            ensure!(
                clip.source_end_ms > clip.source_start_ms,
                "motion clip end must be after its start"
            );
            ensure!(
                clip.source_end_ms - clip.source_start_ms <= MAX_CLIP_DURATION_MS,
                "motion clip trim exceeds two minutes"
            );
            if let Some(split_at_ms) = clip.split_at_ms {
                ensure!(
                    split_at_ms > clip.source_start_ms && split_at_ms < clip.source_end_ms,
                    "motion clip split must be inside its trim range"
                );
            }
        }
        Ok(())
    }

    pub fn audio_clip(&self, total_duration_ms: u64) -> Result<Option<AudioClip>> {
        let Some(audio) = &self.audio else {
            return Ok(None);
        };
        let duration_ms = if audio.duration_ms == 0 {
            total_duration_ms
                .checked_sub(audio.timeline_start_ms)
                .context("motion audio starts after the timeline")?
        } else {
            audio.duration_ms
        };
        Ok(Some(AudioClip {
            path: audio.path.clone(),
            timeline_start_ms: audio.timeline_start_ms,
            source_start_ms: audio.source_start_ms,
            duration_ms,
            volume: audio.volume,
        }))
    }

    pub fn clip_options(&self) -> Option<ClipTranscodeOptions> {
        self.clip.as_ref().map(|clip| ClipTranscodeOptions {
            source_start_ms: clip.source_start_ms,
            duration_ms: clip.source_end_ms - clip.source_start_ms,
            include_audio: clip.include_audio,
            ..Default::default()
        })
    }

    pub fn split_options(&self) -> Option<ClipSplitOptions> {
        self.clip.as_ref().and_then(|clip| {
            clip.split_at_ms.map(|split_at_ms| ClipSplitOptions {
                source_start_ms: clip.source_start_ms,
                split_at_ms,
                source_end_ms: clip.source_end_ms,
                include_audio: clip.include_audio,
                ..Default::default()
            })
        })
    }
}

/// Decode persisted motion media. Absent metadata is a clean, editable empty
/// state so documents authored before motion media remain compatible.
pub fn motion_media(document: &Document) -> Result<MotionMedia> {
    let Some(value) = document.metadata.get(MOTION_MEDIA_METADATA_KEY) else {
        return Ok(MotionMedia::default());
    };
    if value.is_null() {
        return Ok(MotionMedia::default());
    }
    let media: MotionMedia =
        serde_json::from_value(value.clone()).context("invalid motion media metadata")?;
    media.validate()?;
    Ok(media)
}

/// Persist validated editable media information without touching the media
/// files or replacing any rendered pixels.
pub fn set_motion_media(document: &mut Document, media: &MotionMedia) -> Result<()> {
    media.validate()?;
    document.metadata[MOTION_MEDIA_METADATA_KEY] = serde_json::to_value(media)?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AudioClip {
    pub path: PathBuf,
    pub timeline_start_ms: u64,
    pub source_start_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionFormat {
    Mp4,
    Gif,
}

#[derive(Clone, Debug)]
pub struct MotionExportOptions {
    pub frames_per_second: u32,
    pub max_frames: u64,
    pub audio: Vec<AudioClip>,
    pub subtitles: Option<SubtitleTrack>,
    pub subtitle_style: SubtitleStyle,
}

impl Default for MotionExportOptions {
    fn default() -> Self {
        Self {
            frames_per_second: 30,
            max_frames: MAX_MOTION_FRAMES,
            audio: Vec::new(),
            subtitles: None,
            subtitle_style: SubtitleStyle::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionProgress {
    pub rendered_frames: u64,
    pub total_frames: u64,
    pub page_index: usize,
}

#[derive(Clone, Debug)]
pub struct Ffmpeg {
    executable: PathBuf,
    pub version: String,
}

impl Ffmpeg {
    pub fn discover() -> Result<Self> {
        let path =
            env::var_os("PATH").context("PATH is unavailable; FFmpeg cannot be discovered")?;
        for directory in env::split_paths(&path) {
            let candidate = directory.join("ffmpeg");
            if candidate.exists()
                && let Ok(ffmpeg) = Self::from_path(&candidate)
            {
                return Ok(ffmpeg);
            }
        }
        bail!("FFmpeg was not found in PATH")
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve FFmpeg at {}", path.display()))?;
        let metadata = fs::metadata(&canonical)?;
        ensure!(metadata.is_file(), "FFmpeg path is not a regular file");
        let output = Command::new(&canonical)
            .arg("-version")
            .stdin(Stdio::null())
            .output()
            .context("Starting FFmpeg version probe")?;
        ensure!(output.status.success(), "FFmpeg version probe failed");
        ensure!(
            output.stdout.len() <= 1024 * 1024,
            "FFmpeg version response is too large"
        );
        let first_line = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        ensure!(
            first_line.starts_with("ffmpeg version "),
            "Unexpected FFmpeg executable"
        );
        for (kind, name) in [("encoder", "libx264"), ("encoder", "gif")] {
            let status = Command::new(&canonical)
                .args(["-hide_banner", "-loglevel", "error", "-h"])
                .arg(format!("{kind}={name}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            ensure!(status.success(), "FFmpeg lacks the required {name} {kind}");
        }
        let filters = Command::new(&canonical)
            .args(["-hide_banner", "-filters"])
            .stdin(Stdio::null())
            .output()?;
        ensure!(filters.status.success(), "FFmpeg filter probe failed");
        ensure!(
            filters.stdout.len() <= 4 * 1024 * 1024,
            "FFmpeg filter response is too large"
        );
        let filters = String::from_utf8_lossy(&filters.stdout);
        ensure!(
            filters.contains("palettegen") && filters.contains("paletteuse"),
            "FFmpeg lacks GIF palette filters"
        );
        Ok(Self {
            executable: canonical,
            version: first_line,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoClipInfo {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub has_audio: bool,
}

#[derive(Clone, Debug)]
pub struct ClipTranscodeOptions {
    pub source_start_ms: u64,
    pub duration_ms: u64,
    pub maximum_width: u32,
    pub maximum_height: u32,
    pub frames_per_second: u32,
    pub include_audio: bool,
}

/// Two output segments from one selected bounded source interval. Paths are
/// chosen by Omuse beneath a newly published output folder, never beside or
/// over the original clip.
#[derive(Clone, Debug)]
pub struct ClipSplitOptions {
    pub source_start_ms: u64,
    pub split_at_ms: u64,
    pub source_end_ms: u64,
    pub maximum_width: u32,
    pub maximum_height: u32,
    pub frames_per_second: u32,
    pub include_audio: bool,
}

impl Default for ClipSplitOptions {
    fn default() -> Self {
        Self {
            source_start_ms: 0,
            split_at_ms: 7_500,
            source_end_ms: 15_000,
            maximum_width: 1920,
            maximum_height: 1920,
            frames_per_second: 30,
            include_audio: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitClipResult {
    pub directory: PathBuf,
    pub before: PathBuf,
    pub after: PathBuf,
}

impl Default for ClipTranscodeOptions {
    fn default() -> Self {
        Self {
            source_start_ms: 0,
            duration_ms: 15_000,
            maximum_width: 1920,
            maximum_height: 1920,
            frames_per_second: 30,
            include_audio: true,
        }
    }
}

/// Probe a local clip with FFprobe shipped beside the validated FFmpeg binary.
/// Only bounded regular files and one video stream are accepted.
pub fn probe_video_clip(ffmpeg: &Ffmpeg, path: impl AsRef<Path>) -> Result<VideoClipInfo> {
    probe_video_clip_cancellable(ffmpeg, path, &AtomicBool::new(false))
}

/// Cancellable, time-bounded variant used by the native Motion panel. The
/// process is terminated before returning a cancellation or timeout, so a
/// stalled/malformed local source cannot hold the UI job indefinitely.
pub fn probe_video_clip_cancellable(
    ffmpeg: &Ffmpeg,
    path: impl AsRef<Path>,
    cancel: &AtomicBool,
) -> Result<VideoClipInfo> {
    probe_video_clip_with_timeout(ffmpeg, path.as_ref(), cancel, CLIP_PROBE_TIMEOUT)
}

fn probe_video_clip_with_timeout(
    ffmpeg: &Ffmpeg,
    path: &Path,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<VideoClipInfo> {
    check_cancel(cancel)?;
    ensure!(
        timeout > Duration::ZERO,
        "clip probe timeout must be positive"
    );
    let path = path.as_ref();
    validate_media_file(path, "video clip")?;
    let ffprobe = ffmpeg.executable().with_file_name("ffprobe");
    let probe = Ffmpeg::from_companion(&ffprobe, "ffprobe")?;
    check_cancel(cancel)?;
    let output = run_ffprobe_probe(&probe, path, cancel, timeout)?;
    parse_video_clip_info(&output)
}

fn run_ffprobe_probe(
    probe: &Path,
    path: &Path,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut child = Command::new(probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting FFprobe")?;
    let stdout = child.stdout.take().context("Capturing FFprobe output")?;
    let stderr = child.stderr.take().context("Capturing FFprobe errors")?;
    let stdout_reader =
        thread::spawn(move || read_bounded_pipe(stdout, MAX_FFPROBE_RESPONSE_BYTES));
    let stderr_reader = thread::spawn(move || read_bounded_pipe(stderr, 64 * 1024));
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            bail!("clip probe cancelled");
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            bail!("clip probe timed out after {} seconds", timeout.as_secs());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(error).context("Waiting for FFprobe");
            }
        }
    };
    let (stdout, stdout_truncated) = stdout_reader.join().unwrap_or_else(|_| (Vec::new(), true));
    let (stderr, _) = stderr_reader
        .join()
        .unwrap_or_else(|_| (b"FFprobe error reader failed".to_vec(), false));
    ensure!(!stdout_truncated, "FFprobe response is too large");
    ensure!(
        status.success(),
        "FFprobe could not read the video clip: {}",
        String::from_utf8_lossy(&stderr).trim()
    );
    Ok(stdout)
}

fn parse_video_clip_info(output: &[u8]) -> Result<VideoClipInfo> {
    let value: serde_json::Value = serde_json::from_slice(output)?;
    let streams = value["streams"]
        .as_array()
        .context("FFprobe returned no streams")?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"].as_str() == Some("video"))
        .context("clip has no video stream")?;
    let width = u32::try_from(
        video["width"]
            .as_u64()
            .context("clip width is unavailable")?,
    )
    .context("clip width is too large")?;
    let height = u32::try_from(
        video["height"]
            .as_u64()
            .context("clip height is unavailable")?,
    )
    .context("clip height is too large")?;
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_MOTION_PIXELS,
        "clip dimensions exceed motion bounds"
    );
    let duration_seconds = video["duration"]
        .as_str()
        .or_else(|| value["format"]["duration"].as_str())
        .context("clip duration is unavailable")?
        .parse::<f64>()?;
    ensure!(
        duration_seconds.is_finite() && duration_seconds > 0.0 && duration_seconds <= 86_400.0,
        "clip duration exceeds supported bounds"
    );
    Ok(VideoClipInfo {
        width,
        height,
        duration_ms: (duration_seconds * 1000.0).round() as u64,
        has_audio: streams
            .iter()
            .any(|stream| stream["codec_type"].as_str() == Some("audio")),
    })
}

impl Ffmpeg {
    fn from_companion(path: &Path, name: &str) -> Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("{name} was not found beside FFmpeg"))?;
        ensure!(
            fs::metadata(&canonical)?.is_file(),
            "{name} is not a regular file"
        );
        let output = Command::new(&canonical)
            .arg("-version")
            .stdin(Stdio::null())
            .output()?;
        ensure!(output.status.success(), "{name} version probe failed");
        let prefix = format!("{name} version ");
        ensure!(
            String::from_utf8_lossy(&output.stdout).starts_with(&prefix),
            "unexpected {name} executable"
        );
        Ok(canonical)
    }
}

/// Create a normalized, bounded MP4 clip for later placement on a page. The
/// destination is never replaced, including publication races.
pub fn transcode_video_clip(
    ffmpeg: &Ffmpeg,
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: ClipTranscodeOptions,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    check_cancel(cancel)?;
    let source = source.as_ref();
    // The caller may cancel while ffprobe is inspecting a damaged or stalled
    // local source. Keep that cancellation token all the way through the
    // probe instead of starting a fresh uncancellable probe.
    let info = probe_video_clip_cancellable(ffmpeg, source, cancel)?;
    ensure!(
        options.duration_ms > 0 && options.duration_ms <= MAX_CLIP_DURATION_MS,
        "prepared clips must be between one frame and two minutes"
    );
    ensure!(
        options.source_start_ms.saturating_add(options.duration_ms) <= info.duration_ms + 100,
        "requested clip range exceeds the source"
    );
    ensure!(
        (1..=60).contains(&options.frames_per_second),
        "clip frame rate must be 1–60 fps"
    );
    ensure!(
        options.maximum_width > 0
            && options.maximum_height > 0
            && u64::from(options.maximum_width) * u64::from(options.maximum_height)
                <= MAX_MOTION_PIXELS,
        "prepared clip dimensions exceed motion bounds"
    );
    let destination = destination.as_ref();
    ensure!(!destination.exists(), "clip destination already exists");
    ensure!(
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")),
        "prepared clips must use an .mp4 destination"
    );
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".omuse-clip-{}.mp4", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut command = Command::new(ffmpeg.executable());
        command
            .args(["-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{:.6}", options.source_start_ms as f64 / 1000.0))
            .arg("-t")
            .arg(format!("{:.6}", options.duration_ms as f64 / 1000.0))
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0"]);
        if options.include_audio && info.has_audio {
            command.args(["-map", "0:a:0?", "-c:a", "aac"]);
        } else {
            command.arg("-an");
        }
        command
            .arg("-vf")
            .arg(format!(
                "scale='min({},iw)':'min({},ih)':force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2",
                options.maximum_width, options.maximum_height
            ))
            .arg("-r")
            .arg(options.frames_per_second.to_string())
            .args([
                "-c:v",
                "libx264",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&temporary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("Starting FFmpeg clip transcode")?;
        let stderr = child.stderr.take().context("Capturing FFmpeg errors")?;
        let stderr_reader = thread::spawn(move || read_bounded_stderr(stderr));
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let status = child.wait()?;
                let _ = stderr_reader.join();
                bail!("clip transcode cancelled after {status}")
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let stderr = stderr_reader
            .join()
            .unwrap_or_else(|_| b"FFmpeg error reader failed".to_vec());
        ensure!(
            status.success(),
            "FFmpeg clip transcode failed: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
        check_cancel(cancel)?;
        ensure!(
            fs::metadata(&temporary)?.len() > 0,
            "FFmpeg produced an empty clip"
        );
        crate::durable_fs::sync_path(&temporary)?;
        fs::hard_link(&temporary, destination)
            .context("Publishing clip without replacing an existing file")?;
        let _ = fs::remove_file(&temporary);
        let _ = crate::durable_fs::sync_path(parent);
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result?;
    Ok(destination.to_owned())
}

/// Prepare the two sides of a selected source split. Both clips are encoded in
/// a private sibling directory and that directory is published once, with a
/// no-replace rename on Linux. The original source is read only.
pub fn split_video_clip(
    ffmpeg: &Ffmpeg,
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: ClipSplitOptions,
    cancel: &AtomicBool,
) -> Result<SplitClipResult> {
    check_cancel(cancel)?;
    ensure!(
        options.source_start_ms < options.split_at_ms
            && options.split_at_ms < options.source_end_ms,
        "clip split must be inside its source range"
    );
    ensure!(
        options.source_end_ms - options.source_start_ms <= MAX_CLIP_DURATION_MS,
        "clip split range exceeds two minutes"
    );
    let destination = destination.as_ref();
    ensure!(
        !destination.exists(),
        "split clip destination already exists"
    );
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(".omuse-clip-split-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let before = stage.join("before.mp4");
    let after = stage.join("after.mp4");
    let result = (|| -> Result<()> {
        let common = ClipTranscodeOptions {
            maximum_width: options.maximum_width,
            maximum_height: options.maximum_height,
            frames_per_second: options.frames_per_second,
            include_audio: options.include_audio,
            ..Default::default()
        };
        transcode_video_clip(
            ffmpeg,
            source.as_ref(),
            &before,
            ClipTranscodeOptions {
                source_start_ms: options.source_start_ms,
                duration_ms: options.split_at_ms - options.source_start_ms,
                ..common.clone()
            },
            cancel,
        )?;
        check_cancel(cancel)?;
        transcode_video_clip(
            ffmpeg,
            source.as_ref(),
            &after,
            ClipTranscodeOptions {
                source_start_ms: options.split_at_ms,
                duration_ms: options.source_end_ms - options.split_at_ms,
                ..common
            },
            cancel,
        )?;
        check_cancel(cancel)?;
        crate::durable_fs::sync_path(&before)?;
        crate::durable_fs::sync_path(&after)?;
        crate::durable_fs::sync_path(&stage)?;
        publish_new_directory(&stage, destination)?;
        let _ = crate::durable_fs::sync_path(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result?;
    Ok(SplitClipResult {
        directory: destination.to_owned(),
        before: destination.join("before.mp4"),
        after: destination.join("after.mp4"),
    })
}

#[cfg(target_os = "linux")]
fn publish_new_directory(from: &Path, to: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renameat2(
            olddirfd: i32,
            oldpath: *const std::ffi::c_char,
            newdirfd: i32,
            newpath: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: owned NUL-terminated paths outlive the syscall. Linux's
    // RENAME_NOREPLACE prevents a concurrent output folder from being lost.
    if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("Publishing split clips without replacing an existing folder");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn publish_new_directory(from: &Path, to: &Path) -> Result<()> {
    ensure!(!to.exists(), "split clip destination already exists");
    fs::rename(from, to).context("Publishing split clip folder")?;
    Ok(())
}

/// Discover FFmpeg and export a native motion timeline.
pub fn export_motion(
    documents: &[Document],
    timeline: &MotionTimeline,
    destination: impl AsRef<Path>,
    format: MotionFormat,
    options: MotionExportOptions,
    cancel: &AtomicBool,
    progress: impl FnMut(MotionProgress),
) -> Result<PathBuf> {
    check_cancel(cancel)?;
    let ffmpeg = Ffmpeg::discover()?;
    export_motion_with_ffmpeg(
        &ffmpeg,
        documents,
        timeline,
        destination,
        format,
        options,
        cancel,
        progress,
    )
}

pub fn export_motion_with_ffmpeg(
    ffmpeg: &Ffmpeg,
    documents: &[Document],
    timeline: &MotionTimeline,
    destination: impl AsRef<Path>,
    format: MotionFormat,
    options: MotionExportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(MotionProgress),
) -> Result<PathBuf> {
    check_cancel(cancel)?;
    let plan = validate_timeline(documents, timeline, &options)?;
    ensure!(
        !(format == MotionFormat::Gif
            && (!options.audio.is_empty() || options.subtitles.is_some())),
        "GIF does not support embedded audio or subtitles"
    );
    let destination = destination.as_ref();
    ensure!(
        !destination.exists(),
        "motion export destination already exists"
    );
    let expected_extension = match format {
        MotionFormat::Mp4 => "mp4",
        MotionFormat::Gif => "gif",
    };
    ensure!(
        destination
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(expected_extension)),
        "motion export extension must be .{expected_extension}"
    );
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".omuse-motion-{}.{}",
        uuid::Uuid::new_v4(),
        expected_extension
    ));
    let subtitle_path = if let Some(subtitles) = &options.subtitles {
        let path = parent.join(format!(".omuse-subtitles-{}.srt", uuid::Uuid::new_v4()));
        fs::write(&path, subtitles.to_srt()?)?;
        Some(path)
    } else {
        None
    };
    let burn_subtitles = subtitle_path.is_some()
        && options.subtitle_style.presentation == SubtitlePresentation::BurnIn;
    let result = (|| -> Result<()> {
        let mut command = Command::new(ffmpeg.executable());
        command
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pixel_format")
            .arg("rgba")
            .arg("-video_size")
            .arg(format!("{}x{}", plan.width, plan.height))
            .arg("-framerate")
            .arg(options.frames_per_second.to_string())
            .arg("-i")
            .arg("pipe:0");
        for clip in &options.audio {
            command.arg("-i").arg(&clip.path);
        }
        if let Some(path) = &subtitle_path
            && !burn_subtitles
        {
            command.arg("-f").arg("srt").arg("-i").arg(path);
        }
        if format == MotionFormat::Mp4 {
            command.arg("-map").arg("0:v:0");
        }
        if !options.audio.is_empty() {
            let (filter, label) = audio_filter(&options.audio);
            command
                .arg("-filter_complex")
                .arg(filter)
                .arg("-map")
                .arg(label);
        } else if format == MotionFormat::Mp4 {
            command.arg("-an");
        }
        if subtitle_path.is_some() && !burn_subtitles {
            let subtitle_input = options.audio.len() + 1;
            command
                .arg("-map")
                .arg(format!("{subtitle_input}:s:0"))
                .arg("-c:s")
                .arg("mov_text")
                .arg("-metadata:s:s:0")
                .arg("language=und");
        }
        match format {
            MotionFormat::Mp4 => {
                let video_filter = if burn_subtitles {
                    let path = subtitle_path.as_ref().unwrap();
                    format!(
                        "subtitles=filename='{}':charenc=UTF-8:force_style='{}',pad=ceil(iw/2)*2:ceil(ih/2)*2",
                        escape_subtitle_filter_path(path)?,
                        options
                            .subtitle_style
                            .ass_force_style(plan.width, plan.height)
                    )
                } else {
                    "pad=ceil(iw/2)*2:ceil(ih/2)*2".into()
                };
                command
                    .arg("-c:v")
                    .arg("libx264")
                    .arg("-preset")
                    .arg("medium")
                    .arg("-crf")
                    .arg("18")
                    .arg("-vf")
                    .arg(video_filter)
                    .arg("-pix_fmt")
                    .arg("yuv420p")
                    .arg("-movflags")
                    .arg("+faststart");
                if !options.audio.is_empty() {
                    command.arg("-c:a").arg("aac");
                }
            }
            MotionFormat::Gif => {
                command
                    .arg("-filter_complex")
                    .arg(
                        "[0:v]split[a][b];[a]palettegen=max_colors=256[p];[b][p]paletteuse=dither=sierra2_4a[out]",
                    )
                    .arg("-map")
                    .arg("[out]");
            }
        }
        command
            .arg("-t")
            .arg(format!("{:.6}", plan.total_duration_ms as f64 / 1000.0))
            .arg(&temporary)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("Starting FFmpeg encoder")?;
        let stderr = child.stderr.take().context("Capturing FFmpeg errors")?;
        let stderr_reader = thread::spawn(move || read_bounded_stderr(stderr));
        let mut stdin = child.stdin.take().context("Opening FFmpeg frame input")?;
        let render_result = (|| -> Result<()> {
            for frame_index in 0..plan.frames {
                check_cancel(cancel)?;
                let time_ms =
                    frame_index.saturating_mul(1000) / u64::from(options.frames_per_second);
                let frame = render_frame_with_plan(documents, timeline, &plan, time_ms)?;
                stdin
                    .write_all(frame.as_raw())
                    .context("FFmpeg stopped accepting rendered frames")?;
                progress(MotionProgress {
                    rendered_frames: frame_index + 1,
                    total_frames: plan.frames,
                    page_index: page_at_time(&plan, time_ms),
                });
            }
            Ok(())
        })();
        drop(stdin);
        if render_result.is_err() {
            let _ = child.kill();
        }
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let status = child.wait()?;
                let _ = stderr_reader.join();
                bail!("motion export cancelled after {status}")
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let stderr = stderr_reader
            .join()
            .unwrap_or_else(|_| b"FFmpeg error reader failed".to_vec());
        render_result?;
        ensure!(
            status.success(),
            "FFmpeg encoding failed: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
        check_cancel(cancel)?;
        ensure!(
            fs::metadata(&temporary)?.len() > 0,
            "FFmpeg produced an empty file"
        );
        crate::durable_fs::sync_path(&temporary)?;
        fs::hard_link(&temporary, destination)
            .context("Publishing motion export without replacing an existing file")?;
        let _ = fs::remove_file(&temporary);
        let _ = crate::durable_fs::sync_path(parent);
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    if let Some(path) = subtitle_path {
        let _ = fs::remove_file(path);
    }
    result?;
    Ok(destination.to_owned())
}

/// Render one exact timeline instant. Useful for preview and deterministic
/// tests; export calls the same function for every encoded frame.
pub fn render_frame(
    documents: &[Document],
    timeline: &MotionTimeline,
    time_ms: u64,
) -> Result<RgbaImage> {
    let plan = validate_timeline(documents, timeline, &MotionExportOptions::default())?;
    ensure!(
        time_ms < plan.total_duration_ms,
        "frame time is outside the timeline"
    );
    render_frame_with_plan(documents, timeline, &plan, time_ms)
}

#[derive(Clone, Debug)]
struct TimelinePlan {
    starts: Vec<u64>,
    ends: Vec<u64>,
    total_duration_ms: u64,
    frames: u64,
    width: u32,
    height: u32,
}

fn validate_timeline(
    documents: &[Document],
    timeline: &MotionTimeline,
    options: &MotionExportOptions,
) -> Result<TimelinePlan> {
    ensure!(
        !documents.is_empty() && documents.len() <= MAX_MOTION_PAGES,
        "motion exports require 1–{MAX_MOTION_PAGES} pages"
    );
    ensure!(
        documents.len() == timeline.pages.len(),
        "timeline page count does not match documents"
    );
    ensure!(
        (1..=60).contains(&options.frames_per_second),
        "frame rate must be 1–60 fps"
    );
    ensure!(
        options.max_frames > 0 && options.max_frames <= MAX_MOTION_FRAMES,
        "invalid motion frame limit"
    );
    options.subtitle_style.validate()?;
    let (width, height) = (documents[0].width, documents[0].height);
    let pixels = u64::from(width) * u64::from(height);
    ensure!(
        pixels > 0 && pixels <= MAX_MOTION_PIXELS,
        "motion canvas exceeds 16 megapixels"
    );
    let mut starts = Vec::with_capacity(documents.len());
    let mut ends = Vec::with_capacity(documents.len());
    let mut start = 0u64;
    for (page_index, (document, page)) in documents.iter().zip(&timeline.pages).enumerate() {
        let errors = raster::validate(document);
        ensure!(
            errors.is_empty(),
            "invalid motion page: {}",
            errors.join("; ")
        );
        ensure!(
            (document.width, document.height) == (width, height),
            "motion pages must have equal dimensions"
        );
        ensure!(
            (1..=MAX_PAGE_DURATION_MS).contains(&page.duration_ms),
            "page duration is outside supported bounds"
        );
        let transition = page.transition.duration_ms();
        if page_index + 1 == documents.len() {
            ensure!(
                transition == 0,
                "the final page cannot transition to a missing page"
            );
        } else {
            ensure!(
                transition < page.duration_ms,
                "transition must be shorter than its page"
            );
            ensure!(transition <= 10_000, "transition exceeds 10 seconds");
        }
        let mut layer_ids = std::collections::HashSet::new();
        for track in &page.tracks {
            ensure!(
                !track.layer_id.is_empty(),
                "motion track is missing a layer ID"
            );
            ensure!(
                layer_ids.insert(&track.layer_id),
                "duplicate motion track for a layer"
            );
            ensure!(
                document.find_layer(&track.layer_id).is_some(),
                "motion track references a missing layer"
            );
            ensure!(
                track.animations.len() <= 128,
                "too many animations on a layer"
            );
            for animation in &track.animations {
                validate_animation(animation, page.duration_ms)?;
            }
        }
        starts.push(start);
        ends.push(start + u64::from(page.duration_ms));
        start = start
            .checked_add(u64::from(page.duration_ms - transition))
            .context("timeline duration overflow")?;
    }
    let last = timeline.pages.last().unwrap();
    let total_duration_ms = starts.last().copied().unwrap() + u64::from(last.duration_ms);
    ensure!(
        total_duration_ms <= MAX_TIMELINE_DURATION_MS,
        "motion timeline exceeds 10 minutes"
    );
    let frames = total_duration_ms
        .checked_mul(u64::from(options.frames_per_second))
        .context("motion frame count overflow")?
        .div_ceil(1000);
    ensure!(
        frames <= options.max_frames,
        "motion export exceeds its frame limit"
    );
    ensure!(
        frames
            .checked_mul(pixels)
            .is_some_and(|work| work <= MAX_PIXEL_FRAMES),
        "motion export exceeds its render work limit"
    );
    ensure!(
        options.audio.len() <= MAX_AUDIO_CLIPS,
        "too many audio clips"
    );
    for clip in &options.audio {
        validate_audio_clip(clip, total_duration_ms)?;
    }
    if let Some(subtitles) = &options.subtitles {
        subtitles.validate(Some(total_duration_ms))?;
    }
    Ok(TimelinePlan {
        starts,
        ends,
        total_duration_ms,
        frames,
        width,
        height,
    })
}

fn validate_animation(animation: &LayerAnimation, page_duration: u32) -> Result<()> {
    ensure!(
        animation.start_ms < animation.end_ms && animation.end_ms <= page_duration,
        "layer animation has invalid timing"
    );
    match animation.animation {
        LayerAnimationKind::Fade { from, to } => ensure!(
            from.is_finite()
                && to.is_finite()
                && (0.0..=1.0).contains(&from)
                && (0.0..=1.0).contains(&to),
            "fade opacity must be between zero and one"
        ),
        LayerAnimationKind::Pan {
            from_x,
            from_y,
            to_x,
            to_y,
        } => ensure!(
            [from_x, from_y, to_x, to_y]
                .into_iter()
                .all(|value| value.is_finite() && value.abs() <= 100_000.0),
            "pan coordinates exceed supported bounds"
        ),
        LayerAnimationKind::Scale { from, to } => ensure!(
            from.is_finite()
                && to.is_finite()
                && (0.01..=100.0).contains(&from.abs())
                && (0.01..=100.0).contains(&to.abs()),
            "animation scale exceeds supported bounds"
        ),
    }
    Ok(())
}

fn validate_audio_clip(clip: &AudioClip, total_duration_ms: u64) -> Result<()> {
    let metadata = fs::symlink_metadata(&clip.path)
        .with_context(|| format!("Cannot inspect audio clip {}", clip.path.display()))?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() > 0 && metadata.len() <= MAX_MEDIA_BYTES,
        "audio clip is not a bounded regular file"
    );
    ensure!(clip.duration_ms > 0, "audio clip duration must be positive");
    ensure!(
        clip.timeline_start_ms.saturating_add(clip.duration_ms) <= total_duration_ms,
        "audio clip exceeds the motion timeline"
    );
    ensure!(
        clip.volume.is_finite() && (0.0..=4.0).contains(&clip.volume),
        "audio volume is outside 0–4"
    );
    Ok(())
}

fn render_frame_with_plan(
    documents: &[Document],
    timeline: &MotionTimeline,
    plan: &TimelinePlan,
    time_ms: u64,
) -> Result<RgbaImage> {
    let page_index = page_at_time(plan, time_ms);
    let local_time = time_ms.saturating_sub(plan.starts[page_index]);
    let page = &timeline.pages[page_index];
    let transition_duration = u64::from(page.transition.duration_ms());
    let transition_start = u64::from(page.duration_ms).saturating_sub(transition_duration);
    if transition_duration > 0 && local_time >= transition_start && page_index + 1 < documents.len()
    {
        let progress = (local_time - transition_start) as f32 / transition_duration as f32;
        let outgoing = render_page(&documents[page_index], page, local_time as u32)?;
        let incoming_local = time_ms.saturating_sub(plan.starts[page_index + 1]) as u32;
        let incoming = render_page(
            &documents[page_index + 1],
            &timeline.pages[page_index + 1],
            incoming_local,
        )?;
        Ok(apply_transition(
            &outgoing,
            &incoming,
            &page.transition,
            progress,
        ))
    } else {
        render_page(&documents[page_index], page, local_time as u32)
    }
}

fn page_at_time(plan: &TimelinePlan, time_ms: u64) -> usize {
    plan.ends
        .iter()
        .position(|end| time_ms < *end)
        .unwrap_or(plan.ends.len() - 1)
}

fn validate_media_file(path: &Path, name: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Cannot inspect {name} {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() > 0 && metadata.len() <= MAX_MEDIA_BYTES,
        "{name} is not a bounded regular file"
    );
    Ok(())
}

/// Quote the generated temporary subtitle path for FFmpeg's filter grammar.
/// The style fragment itself is wholly generated from validated numeric enums;
/// no user-controlled filter syntax is accepted.
fn escape_subtitle_filter_path(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .context("subtitle burn-in needs a UTF-8 output path")?;
    ensure!(
        !path.chars().any(char::is_control),
        "subtitle burn-in path contains a control character"
    );
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            ':' => escaped.push_str("\\:"),
            ',' => escaped.push_str("\\,"),
            '[' => escaped.push_str("\\["),
            ']' => escaped.push_str("\\]"),
            ';' => escaped.push_str("\\;"),
            character => escaped.push(character),
        }
    }
    Ok(escaped)
}

fn render_page(document: &Document, timeline: &PageTimeline, time_ms: u32) -> Result<RgbaImage> {
    let mut frame = document.clone();
    for track in &timeline.tracks {
        let layer = frame
            .find_layer_mut(&track.layer_id)
            .context("motion layer disappeared")?;
        for animation in &track.animations {
            let value = animation_progress(animation, time_ms);
            match animation.animation {
                LayerAnimationKind::Fade { from, to } => {
                    layer.opacity *= interpolate(from, to, value);
                    layer.opacity = layer.opacity.clamp(0.0, 1.0);
                }
                LayerAnimationKind::Pan {
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                } => {
                    layer.offset_x += interpolate(from_x, to_x, value);
                    layer.offset_y += interpolate(from_y, to_y, value);
                }
                LayerAnimationKind::Scale { from, to } => {
                    let scale = interpolate(from, to, value);
                    layer.scale_x *= scale;
                    layer.scale_y *= scale;
                }
            }
        }
    }
    let result = raster::composite(&frame);
    ensure!(
        result.dimensions() == (document.width, document.height),
        "motion compositor returned invalid dimensions"
    );
    Ok(result)
}

fn animation_progress(animation: &LayerAnimation, time_ms: u32) -> f32 {
    let linear = if time_ms <= animation.start_ms {
        0.0
    } else if time_ms >= animation.end_ms {
        1.0
    } else {
        (time_ms - animation.start_ms) as f32 / (animation.end_ms - animation.start_ms) as f32
    };
    match animation.easing {
        Easing::Linear => linear,
        Easing::EaseInOut => linear * linear * (3.0 - 2.0 * linear),
    }
}

fn interpolate(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

fn apply_transition(
    outgoing: &RgbaImage,
    incoming: &RgbaImage,
    transition: &PageTransition,
    progress: f32,
) -> RgbaImage {
    let progress = progress.clamp(0.0, 1.0);
    match transition {
        PageTransition::None => outgoing.clone(),
        PageTransition::CrossFade { .. } => {
            RgbaImage::from_fn(outgoing.width(), outgoing.height(), |x, y| {
                let a = outgoing.get_pixel(x, y).0;
                let b = incoming.get_pixel(x, y).0;
                Rgba(std::array::from_fn(|channel| {
                    interpolate(f32::from(a[channel]), f32::from(b[channel]), progress).round()
                        as u8
                }))
            })
        }
        PageTransition::Slide { direction, .. } => {
            let width = outgoing.width();
            let height = outgoing.height();
            let x = (progress * width as f32).round() as i64;
            let y = (progress * height as f32).round() as i64;
            let (out_x, out_y, in_x, in_y) = match direction {
                SlideDirection::Left => (-x, 0, i64::from(width) - x, 0),
                SlideDirection::Right => (x, 0, -(i64::from(width) - x), 0),
                SlideDirection::Up => (0, -y, 0, i64::from(height) - y),
                SlideDirection::Down => (0, y, 0, -(i64::from(height) - y)),
            };
            let mut output = RgbaImage::new(width, height);
            place(&mut output, outgoing, out_x, out_y);
            place(&mut output, incoming, in_x, in_y);
            output
        }
    }
}

fn place(destination: &mut RgbaImage, source: &RgbaImage, offset_x: i64, offset_y: i64) {
    for y in 0..source.height() {
        let target_y = i64::from(y) + offset_y;
        if !(0..i64::from(destination.height())).contains(&target_y) {
            continue;
        }
        for x in 0..source.width() {
            let target_x = i64::from(x) + offset_x;
            if (0..i64::from(destination.width())).contains(&target_x) {
                destination.put_pixel(target_x as u32, target_y as u32, *source.get_pixel(x, y));
            }
        }
    }
}

fn audio_filter(clips: &[AudioClip]) -> (String, String) {
    let mut filters = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        filters.push(format!(
            "[{}:a:0]atrim=start={:.6}:duration={:.6},asetpts=PTS-STARTPTS,adelay={}:all=1,volume={:.4}[a{}]",
            index + 1,
            clip.source_start_ms as f64 / 1000.0,
            clip.duration_ms as f64 / 1000.0,
            clip.timeline_start_ms,
            clip.volume,
            index
        ));
    }
    let inputs = (0..clips.len())
        .map(|index| format!("[a{index}]"))
        .collect::<String>();
    filters.push(format!(
        "{inputs}amix=inputs={}:normalize=0:dropout_transition=0[aout]",
        clips.len()
    ));
    (filters.join(";"), "[aout]".into())
}

fn read_bounded_pipe(mut pipe: impl Read, keep: usize) -> (Vec<u8>, bool) {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 4096];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if retained.len() < keep {
                    let remaining = keep - retained.len();
                    retained.extend_from_slice(&buffer[..read.min(remaining)]);
                    truncated |= read > remaining;
                } else {
                    truncated = true;
                }
            }
        }
    }
    (retained, truncated)
}

fn read_bounded_stderr(stderr: impl Read) -> Vec<u8> {
    read_bounded_pipe(stderr, 64 * 1024).0
}

fn parse_timing_line(line: &str, fraction_separator: char) -> Result<(u64, u64)> {
    let (start, remainder) = line.split_once("-->").context("invalid subtitle timing")?;
    let end = remainder
        .split_whitespace()
        .next()
        .context("missing subtitle end time")?;
    Ok((
        parse_time(start.trim(), fraction_separator)?,
        parse_time(end.trim(), fraction_separator)?,
    ))
}

fn parse_time(value: &str, fraction_separator: char) -> Result<u64> {
    let (whole, fraction) = value
        .rsplit_once(fraction_separator)
        .context("subtitle time is missing milliseconds")?;
    ensure!(
        fraction.len() == 3,
        "subtitle milliseconds must have three digits"
    );
    let fields = whole.split(':').collect::<Vec<_>>();
    ensure!((2..=3).contains(&fields.len()), "invalid subtitle time");
    let (hours, minutes, seconds) = if fields.len() == 3 {
        (
            fields[0].parse::<u64>()?,
            fields[1].parse::<u64>()?,
            fields[2].parse::<u64>()?,
        )
    } else {
        (0, fields[0].parse::<u64>()?, fields[1].parse::<u64>()?)
    };
    let milliseconds = fraction.parse::<u64>()?;
    ensure!(minutes < 60 && seconds < 60, "invalid subtitle time fields");
    hours
        .checked_mul(3_600_000)
        .and_then(|value| value.checked_add(minutes * 60_000))
        .and_then(|value| value.checked_add(seconds * 1000))
        .and_then(|value| value.checked_add(milliseconds))
        .context("subtitle time overflow")
}

fn format_time(milliseconds: u64, separator: char) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = (milliseconds / 60_000) % 60;
    let seconds = (milliseconds / 1000) % 60;
    let fraction = milliseconds % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}{separator}{fraction:03}")
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "motion export cancelled");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn solid(width: u32, height: u32, color: [u8; 4]) -> Document {
        let mut document = Document::new(width, height);
        document.layers[0].image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
        document
    }

    #[test]
    fn page_overlap_and_crossfade_have_exact_timing() {
        let red = solid(2, 1, [255, 0, 0, 255]);
        let blue = solid(2, 1, [0, 0, 255, 255]);
        let timeline = MotionTimeline {
            pages: vec![
                PageTimeline {
                    page_id: "red".into(),
                    duration_ms: 1000,
                    transition: PageTransition::CrossFade { duration_ms: 200 },
                    tracks: vec![],
                },
                PageTimeline {
                    page_id: "blue".into(),
                    duration_ms: 1000,
                    transition: PageTransition::None,
                    tracks: vec![],
                },
            ],
        };
        let documents = [red, blue];
        assert_eq!(
            render_frame(&documents, &timeline, 799)
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [255, 0, 0, 255]
        );
        let middle = render_frame(&documents, &timeline, 900).unwrap();
        assert_eq!(middle.get_pixel(0, 0).0, [128, 0, 128, 255]);
        assert_eq!(
            render_frame(&documents, &timeline, 1000)
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [0, 0, 255, 255]
        );
        let plan =
            validate_timeline(&documents, &timeline, &MotionExportOptions::default()).unwrap();
        assert_eq!(plan.starts, vec![0, 800]);
        assert_eq!(plan.total_duration_ms, 1800);
        assert_eq!(plan.frames, 54);
    }

    #[test]
    fn ffprobe_parser_keeps_only_bounded_video_metadata() {
        let info = parse_video_clip_info(
            br#"{
                "streams": [
                    {"codec_type":"audio"},
                    {"codec_type":"video","width":1920,"height":1080,"duration":"2.5"}
                ],
                "format": {"duration":"3.0"}
            }"#,
        )
        .unwrap();
        assert_eq!(
            info,
            VideoClipInfo {
                width: 1920,
                height: 1080,
                duration_ms: 2_500,
                has_audio: true,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancellable_ffprobe_returns_a_bounded_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let ffprobe = directory.path().join("ffprobe");
        std::fs::write(
            &ffprobe,
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then\n  echo 'ffprobe version test'\n  exit 0\nfi\nexec sleep 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&ffprobe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let source = directory.path().join("source.mp4");
        std::fs::write(&source, b"not a real movie").unwrap();
        let ffmpeg = Ffmpeg {
            executable: directory.path().join("ffmpeg"),
            version: "ffmpeg version test".into(),
        };
        let error = probe_video_clip_with_timeout(
            &ffmpeg,
            &source,
            &AtomicBool::new(false),
            Duration::from_millis(5),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }

    #[cfg(unix)]
    #[test]
    fn clip_transcode_cancels_while_ffprobe_is_stalled() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let ffprobe = directory.path().join("ffprobe");
        std::fs::write(
            &ffprobe,
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then\n  echo 'ffprobe version test'\n  exit 0\nfi\nexec sleep 2\n",
        )
        .unwrap();
        std::fs::set_permissions(&ffprobe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let source = directory.path().join("source.mp4");
        let destination = directory.path().join("prepared.mp4");
        std::fs::write(&source, b"not a real movie").unwrap();
        let ffmpeg = Ffmpeg {
            executable: directory.path().join("ffmpeg"),
            version: "ffmpeg version test".into(),
        };
        let cancel = AtomicBool::new(false);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(25));
                cancel.store(true, Ordering::Relaxed);
            });
            let error = transcode_video_clip(
                &ffmpeg,
                &source,
                &destination,
                ClipTranscodeOptions::default(),
                &cancel,
            )
            .unwrap_err();
            assert!(error.to_string().contains("cancelled"));
        });
        assert!(!destination.exists());
    }

    #[test]
    fn layer_fade_uses_native_compositor() {
        let mut page = solid(1, 1, [100, 50, 20, 255]);
        page.background = [0, 0, 0, 255];
        let id = page.layers[0].id.clone();
        let timeline = MotionTimeline {
            pages: vec![PageTimeline {
                page_id: "page".into(),
                duration_ms: 1000,
                transition: PageTransition::None,
                tracks: vec![LayerTrack {
                    layer_id: id,
                    animations: vec![LayerAnimation {
                        start_ms: 0,
                        end_ms: 1000,
                        easing: Easing::Linear,
                        animation: LayerAnimationKind::Fade { from: 0.0, to: 1.0 },
                    }],
                }],
            }],
        };
        assert_eq!(
            render_frame(&[page], &timeline, 0)
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn subtitle_import_round_trips_timing() {
        let source = "1\r\n00:00:01,250 --> 00:00:02,500\r\nFirst line\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,125\r\nSecond\r\n";
        let track = SubtitleTrack::parse_srt(source).unwrap();
        assert_eq!(track.cues[0].start_ms, 1250);
        assert_eq!(track.cues[1].end_ms, 4125);
        assert_eq!(
            SubtitleTrack::parse_srt(&track.to_srt().unwrap()).unwrap(),
            track
        );
    }

    #[test]
    fn motion_media_metadata_preserves_editable_subtitles_audio_and_trim() {
        let mut document = solid(2, 1, [20, 40, 60, 255]);
        let media = MotionMedia {
            version: MOTION_MEDIA_VERSION,
            audio: Some(MotionAudioMedia {
                path: PathBuf::from("soundtrack.ogg"),
                timeline_start_ms: 250,
                source_start_ms: 500,
                duration_ms: 1_000,
                volume: 0.75,
            }),
            subtitles: Some(SubtitleTrack {
                cues: vec![SubtitleCue {
                    start_ms: 250,
                    end_ms: 1_250,
                    text: "Saved cue".into(),
                }],
            }),
            subtitle_style: SubtitleStyle {
                presentation: SubtitlePresentation::BurnIn,
                font_size_px: 48,
                outline_px: 3,
                position: SubtitlePosition::Top,
            },
            subtitle_source_path: Some(PathBuf::from("captions.vtt")),
            clip: Some(MotionClipMedia {
                path: PathBuf::from("source.mov"),
                source_start_ms: 2_000,
                source_end_ms: 12_000,
                split_at_ms: Some(7_000),
                include_audio: false,
            }),
        };
        set_motion_media(&mut document, &media).unwrap();

        let reopened = motion_media(&document).unwrap();
        assert_eq!(reopened, media);
        assert_eq!(
            reopened
                .audio_clip(2_000)
                .unwrap()
                .unwrap()
                .timeline_start_ms,
            250
        );
        let clip = reopened.clip_options().unwrap();
        assert_eq!(
            (clip.source_start_ms, clip.duration_ms, clip.include_audio),
            (2_000, 10_000, false)
        );
        let split = reopened.split_options().unwrap();
        assert_eq!(
            (
                split.source_start_ms,
                split.split_at_ms,
                split.source_end_ms
            ),
            (2_000, 7_000, 12_000)
        );
    }

    #[test]
    fn motion_media_rejects_unknown_versions_overlong_trim_and_invalid_split() {
        let unknown = MotionMedia {
            version: MOTION_MEDIA_VERSION + 1,
            ..Default::default()
        };
        assert!(unknown.validate().is_err());
        let overlong = MotionMedia {
            clip: Some(MotionClipMedia {
                path: PathBuf::from("source.mov"),
                source_start_ms: 0,
                source_end_ms: MAX_CLIP_DURATION_MS + 1,
                split_at_ms: None,
                include_audio: true,
            }),
            ..Default::default()
        };
        assert!(overlong.validate().is_err());
        let invalid_split = MotionMedia {
            clip: Some(MotionClipMedia {
                path: PathBuf::from("source.mov"),
                source_start_ms: 1_000,
                source_end_ms: 2_000,
                split_at_ms: Some(2_000),
                include_audio: true,
            }),
            ..Default::default()
        };
        assert!(invalid_split.validate().is_err());
    }

    #[test]
    fn fade_out_and_directional_transition_are_authored_natively() {
        let fade = LayerAnimation::fade_out(750, 250);
        assert_eq!(fade.start_ms, 750);
        assert_eq!(fade.end_ms, 1_000);
        assert_eq!(
            fade.animation,
            LayerAnimationKind::Fade { from: 1.0, to: 0.0 }
        );
        let transition = PageTransition::Slide {
            duration_ms: 400,
            direction: SlideDirection::Left,
        };
        assert_eq!(transition.duration_ms(), 400);
    }

    #[test]
    fn directional_slides_cover_odd_dimensions_without_transparent_seams() {
        let red = RgbaImage::from_pixel(3, 1, Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(3, 1, Rgba([0, 0, 255, 255]));
        let left = apply_transition(
            &red,
            &blue,
            &PageTransition::Slide {
                duration_ms: 100,
                direction: SlideDirection::Left,
            },
            0.5,
        );
        assert_eq!(left.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(left.get_pixel(1, 0).0, [0, 0, 255, 255]);
        assert!(left.pixels().all(|pixel| pixel.0[3] == 255));

        let red = RgbaImage::from_pixel(1, 3, Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(1, 3, Rgba([0, 0, 255, 255]));
        let down = apply_transition(
            &red,
            &blue,
            &PageTransition::Slide {
                duration_ms: 100,
                direction: SlideDirection::Down,
            },
            0.5,
        );
        assert_eq!(down.get_pixel(0, 2).0, [255, 0, 0, 255]);
        assert!(down.pixels().all(|pixel| pixel.0[3] == 255));
    }

    #[test]
    fn subtitle_style_and_cue_bounds_are_validated() {
        assert!(
            SubtitleStyle {
                font_size_px: 11,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        let track = SubtitleTrack {
            cues: vec![SubtitleCue {
                start_ms: 0,
                end_ms: 1_000,
                text: format!("{}line", "line\n".repeat(MAX_SUBTITLE_CUE_LINES)),
            }],
        };
        assert!(track.validate(None).is_err());
        assert_eq!(
            escape_subtitle_filter_path(Path::new("/tmp/quote'comma,colon:bracket[;].srt"))
                .unwrap(),
            "/tmp/quote\\'comma\\,colon\\:bracket\\[\\;\\].srt"
        );

        let style = SubtitleStyle {
            presentation: SubtitlePresentation::BurnIn,
            font_size_px: 44,
            outline_px: 3,
            position: SubtitlePosition::Bottom,
        };
        assert_eq!(
            style.ass_force_style(640, 480),
            "PlayResX=640,PlayResY=480,FontSize=44,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,BorderStyle=1,Outline=3,Shadow=1,Alignment=2,MarginV=22"
        );
    }

    #[test]
    fn cancelled_export_does_not_probe_or_publish() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("cancelled.mp4");
        let timeline = MotionTimeline {
            pages: vec![PageTimeline {
                page_id: "page".into(),
                duration_ms: 1000,
                transition: PageTransition::None,
                tracks: vec![],
            }],
        };
        let error = export_motion(
            &[solid(1, 1, [0, 0, 0, 255])],
            &timeline,
            &destination,
            MotionFormat::Mp4,
            MotionExportOptions::default(),
            &AtomicBool::new(true),
            |_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(!destination.exists());
    }

    #[test]
    fn installed_ffmpeg_round_trips_mp4_gif_and_honours_mid_export_cancel() {
        let Ok(ffmpeg) = Ffmpeg::discover() else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let document = solid(16, 12, [20, 80, 160, 255]);
        let timeline = MotionTimeline {
            pages: vec![PageTimeline {
                page_id: "page".into(),
                duration_ms: 400,
                transition: PageTransition::None,
                tracks: vec![],
            }],
        };
        let options = MotionExportOptions {
            frames_per_second: 5,
            ..Default::default()
        };
        let mp4 = directory.path().join("roundtrip.mp4");
        export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &mp4,
            MotionFormat::Mp4,
            options.clone(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let mp4_info = probe_video_clip(&ffmpeg, &mp4).unwrap();
        assert_eq!((mp4_info.width, mp4_info.height), (16, 12));
        assert!((300..=600).contains(&mp4_info.duration_ms));

        let gif = directory.path().join("roundtrip.gif");
        export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &gif,
            MotionFormat::Gif,
            options.clone(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let gif_info = probe_video_clip(&ffmpeg, &gif).unwrap();
        assert_eq!((gif_info.width, gif_info.height), (16, 12));

        let soft_subtitles = directory.path().join("soft-subtitles.mp4");
        let subtitle_options = MotionExportOptions {
            subtitles: Some(SubtitleTrack {
                cues: vec![SubtitleCue {
                    start_ms: 0,
                    end_ms: 300,
                    text: "Saved cue".into(),
                }],
            }),
            ..options.clone()
        };
        export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &soft_subtitles,
            MotionFormat::Mp4,
            subtitle_options,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(soft_subtitles.exists());

        let burned_subtitles = directory.path().join("burned-subtitles.mp4");
        let burn_options = MotionExportOptions {
            subtitles: Some(SubtitleTrack {
                cues: vec![SubtitleCue {
                    start_ms: 0,
                    end_ms: 300,
                    text: "Readable cue".into(),
                }],
            }),
            subtitle_style: SubtitleStyle {
                presentation: SubtitlePresentation::BurnIn,
                font_size_px: 24,
                outline_px: 2,
                position: SubtitlePosition::Bottom,
            },
            ..options.clone()
        };
        export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &burned_subtitles,
            MotionFormat::Mp4,
            burn_options,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(burned_subtitles.exists());

        let split_directory = directory.path().join("split");
        let split = split_video_clip(
            &ffmpeg,
            &mp4,
            &split_directory,
            ClipSplitOptions {
                source_start_ms: 0,
                split_at_ms: mp4_info.duration_ms / 2,
                source_end_ms: mp4_info.duration_ms,
                maximum_width: 16,
                maximum_height: 12,
                frames_per_second: 5,
                include_audio: false,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(split.before.is_file() && split.after.is_file());
        assert!(
            split_video_clip(
                &ffmpeg,
                &mp4,
                &split_directory,
                ClipSplitOptions {
                    source_start_ms: 0,
                    split_at_ms: mp4_info.duration_ms / 2,
                    source_end_ms: mp4_info.duration_ms,
                    maximum_width: 16,
                    maximum_height: 12,
                    frames_per_second: 5,
                    include_audio: false,
                },
                &AtomicBool::new(false),
            )
            .is_err()
        );

        let cancelled = directory.path().join("cancelled-during-encode.mp4");
        let cancel = AtomicBool::new(false);
        let result = export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &cancelled,
            MotionFormat::Mp4,
            options.clone(),
            &cancel,
            |progress| {
                if progress.rendered_frames == 1 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(!cancelled.exists());
        let cancelled_after_frames = directory.path().join("cancelled-after-frames.mp4");
        let cancel = AtomicBool::new(false);
        let result = export_motion_with_ffmpeg(
            &ffmpeg,
            std::slice::from_ref(&document),
            &timeline,
            &cancelled_after_frames,
            MotionFormat::Mp4,
            options,
            &cancel,
            |progress| {
                if progress.rendered_frames == progress.total_frames {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(!cancelled_after_frames.exists());
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omuse-motion-")
        }));
    }
}
