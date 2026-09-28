//! Release-artifact QA for Omuse's native Motion APIs.
//!
//! Usage:
//!
//! ```text
//! cargo run --release --example media_acceptance -- /tmp/omuse-media-acceptance
//! ```
//!
//! The directory must be new. Outputs are intentionally retained: inspect the
//! PNG frames and `results.json` alongside the MP4 stream metadata.

use anyhow::{Context, Result, ensure};
use image::Rgba;
use omuse::{
    document,
    model::{Document, Layer},
    motion::{
        self, AudioClip, ClipSplitOptions, ClipTranscodeOptions, LayerAnimation, LayerTrack,
        MotionAudioMedia, MotionExportOptions, MotionFormat, MotionMedia, MotionTimeline,
        PageTimeline, PageTransition, SubtitleCue, SubtitlePosition, SubtitlePresentation,
        SubtitleStyle, SubtitleTrack, VideoClipInfo,
    },
    objects::{self, LiveTextStyle, ObjectPoint, ObjectSize, TextAlignment},
    raster,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const FPS: u32 = 12;
const TOTAL_DURATION_MS: u64 = 3_200;

fn main() -> Result<()> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("Usage: media_acceptance <new-output-directory>")?;
    ensure!(
        !output.exists(),
        "Use a fresh output directory: {}",
        output.display()
    );
    fs::create_dir(&output).with_context(|| format!("Creating {}", output.display()))?;
    run(&output)
}

fn run(output: &Path) -> Result<()> {
    let cancel = AtomicBool::new(false);
    let encoder =
        motion::Ffmpeg::discover().context("Media acceptance needs FFmpeg and FFprobe")?;

    let tone = output.join("tone-440hz-pcm.wav");
    write_pcm_tone(&tone, 4_000)?;
    let captions = editable_captions();
    let srt = output.join("captions-editable.srt");
    let vtt = output.join("captions-editable.vtt");
    fs::write(&srt, captions.to_srt()?)?;
    fs::write(&vtt, captions_vtt())?;
    ensure!(
        SubtitleTrack::load(&srt)? == captions,
        "SRT round-trip changed captions"
    );
    ensure!(
        SubtitleTrack::load(&vtt)? == captions,
        "WebVTT import changed captions"
    );
    ensure!(
        SubtitleTrack::parse_srt(&captions.to_srt()?)? == captions,
        "Editable subtitle serialization is not stable"
    );

    let media = MotionMedia {
        audio: Some(MotionAudioMedia {
            path: tone.clone(),
            timeline_start_ms: 0,
            source_start_ms: 0,
            duration_ms: TOTAL_DURATION_MS,
            volume: 0.35,
        }),
        subtitles: Some(captions.clone()),
        subtitle_style: SubtitleStyle {
            presentation: SubtitlePresentation::Soft,
            font_size_px: 44,
            outline_px: 3,
            position: SubtitlePosition::Bottom,
        },
        subtitle_source_path: Some(vtt.clone()),
        ..Default::default()
    };

    let mut first = artwork("OMUSE", "Native motion\nacceptance", [24, 32, 55, 255])?;
    motion::set_motion_media(&mut first, &media)?;
    let second = artwork(
        "LOCAL",
        "Audio, captions\nand transitions",
        [50, 24, 48, 255],
    )?;
    let first_path = output.join("page-one-native-text.comp");
    let second_path = output.join("page-two-native-text.comp");
    document::save(&first, &first_path)?;
    document::save(&second, &second_path)?;
    let first = document::open(&first_path)?;
    let second = document::open(&second_path)?;
    ensure!(
        has_live_text(&first)?,
        "First page lost native text after reopen"
    );
    ensure!(
        has_live_text(&second)?,
        "Second page lost native text after reopen"
    );
    ensure!(
        motion::motion_media(&first)?.subtitles == Some(captions.clone()),
        "Embedded editable captions did not survive reopen"
    );
    raster::composite(&first).save(output.join("page-one-native-text.png"))?;
    raster::composite(&second).save(output.join("page-two-native-text.png"))?;

    let first_text = live_text_id(&first, "OMUSE title")?;
    let second_text = live_text_id(&second, "LOCAL title")?;
    let timeline = MotionTimeline {
        pages: vec![
            PageTimeline {
                page_id: "page-one".into(),
                duration_ms: 1_800,
                transition: PageTransition::CrossFade { duration_ms: 400 },
                tracks: vec![LayerTrack {
                    layer_id: first_text,
                    animations: vec![LayerAnimation::fade_in(0, 350)],
                }],
            },
            PageTimeline {
                page_id: "page-two".into(),
                duration_ms: 1_800,
                transition: PageTransition::None,
                tracks: vec![LayerTrack {
                    layer_id: second_text,
                    animations: LayerAnimation::rise_in(0, 450, 24.).into(),
                }],
            },
        ],
    };
    ensure!(
        timeline.duration_ms()? == TOTAL_DURATION_MS,
        "Acceptance timeline duration changed"
    );
    let documents = [first, second];
    let audio = AudioClip {
        path: tone.clone(),
        timeline_start_ms: 0,
        source_start_ms: 0,
        duration_ms: TOTAL_DURATION_MS,
        volume: 0.35,
    };

    let soft = output.join("motion-soft-subtitles.mp4");
    let burned = output.join("motion-burned-subtitles.mp4");
    motion::export_motion_with_ffmpeg(
        &encoder,
        &documents,
        &timeline,
        &soft,
        MotionFormat::Mp4,
        export_options(audio.clone(), captions.clone(), SubtitlePresentation::Soft),
        &cancel,
        |_| {},
    )?;
    motion::export_motion_with_ffmpeg(
        &encoder,
        &documents,
        &timeline,
        &burned,
        MotionFormat::Mp4,
        export_options(audio, captions, SubtitlePresentation::BurnIn),
        &cancel,
        |_| {},
    )?;

    let soft_info = checked_video(&encoder, &soft, "soft-subtitle MP4", 2_600, 3_600)?;
    let burned_info = checked_video(&encoder, &burned, "burned-subtitle MP4", 2_600, 3_600)?;
    let soft_streams = inspect_streams(&encoder, &soft)?;
    let burned_streams = inspect_streams(&encoder, &burned)?;
    ensure!(
        has_stream(&soft_streams, "audio"),
        "Soft MP4 has no audio stream"
    );
    ensure!(
        has_stream(&soft_streams, "subtitle"),
        "Soft MP4 has no selectable subtitle stream"
    );
    ensure!(
        has_stream(&burned_streams, "audio"),
        "Burned MP4 has no audio stream"
    );
    ensure!(
        !has_stream(&burned_streams, "subtitle"),
        "Burned MP4 unexpectedly retained a soft subtitle stream"
    );
    let soft_audio = decoded_tone(&encoder, &soft, "soft-subtitle MP4")?;
    let burned_audio = decoded_tone(&encoder, &burned, "burned-subtitle MP4")?;
    let soft_frame = output.join("soft-subtitle-frame-0750ms.png");
    let burned_frame = output.join("burned-subtitle-frame-0750ms.png");
    capture_frame(&encoder, &soft, &soft_frame, "0.750")?;
    capture_frame(&encoder, &burned, &burned_frame, "0.750")?;

    let trimmed = output.join("trimmed-with-audio.mp4");
    motion::transcode_video_clip(
        &encoder,
        &soft,
        &trimmed,
        ClipTranscodeOptions {
            source_start_ms: 400,
            duration_ms: 1_200,
            maximum_width: WIDTH,
            maximum_height: HEIGHT,
            frames_per_second: FPS,
            include_audio: true,
        },
        &cancel,
    )?;
    let trimmed_info = checked_video(&encoder, &trimmed, "trimmed clip", 900, 1_600)?;

    let source_end_ms = soft_info.duration_ms.min(TOTAL_DURATION_MS);
    let split_at_ms = source_end_ms / 2;
    ensure!(
        split_at_ms > 500 && split_at_ms + 500 < source_end_ms,
        "Invalid split range"
    );
    let split = motion::split_video_clip(
        &encoder,
        &soft,
        output.join("split-clips"),
        ClipSplitOptions {
            source_start_ms: 0,
            split_at_ms,
            source_end_ms,
            maximum_width: WIDTH,
            maximum_height: HEIGHT,
            frames_per_second: FPS,
            include_audio: true,
        },
        &cancel,
    )?;
    let before_info = checked_video(&encoder, &split.before, "split first clip", 500, 2_100)?;
    let after_info = checked_video(&encoder, &split.after, "split second clip", 500, 2_100)?;
    ensure!(
        before_info.duration_ms + after_info.duration_ms + 600 >= source_end_ms,
        "Split clips are materially shorter than their source interval"
    );

    let results = json!({
        "status": "passed",
        "command": "cargo run --release --example media_acceptance -- <new-output-directory>",
        "timeline": {"pages": 2, "durationMs": TOTAL_DURATION_MS, "fps": FPS, "dimensions": [WIDTH, HEIGHT]},
        "editableSources": {
            "pageOne": artifact(&first_path, output),
            "pageTwo": artifact(&second_path, output),
            "pageOneRaster": "page-one-native-text.png",
            "pageTwoRaster": "page-two-native-text.png",
            "nativeTextAndEmbeddedMotionMediaPreserved": true
        },
        "fixtures": {
            "pcmTone": artifact(&tone, output),
            "srt": artifact(&srt, output),
            "webvtt": artifact(&vtt, output),
            "editableSubtitleRoundTrip": true
        },
        "exports": [
            {"kind": "soft subtitles", "path": artifact(&soft, output), "frame": artifact(&soft_frame, output), "probe": video_json(&soft_info), "ffprobe": soft_streams, "decodedAudio": soft_audio, "expected": {"audio": true, "selectableSubtitleTrack": true, "toneHz": 440}},
            {"kind": "burned subtitles", "path": artifact(&burned, output), "frame": artifact(&burned_frame, output), "probe": video_json(&burned_info), "ffprobe": burned_streams, "decodedAudio": burned_audio, "expected": {"audio": true, "selectableSubtitleTrack": false, "styledCaptionVisibleInFrame": true, "toneHz": 440}},
            {"kind": "trimmed clip", "path": artifact(&trimmed, output), "probe": video_json(&trimmed_info), "expected": {"audio": true, "durationRangeMs": [900, 1600]}},
            {"kind": "split first", "path": artifact(&split.before, output), "probe": video_json(&before_info), "expected": {"audio": true}},
            {"kind": "split second", "path": artifact(&split.after, output), "probe": video_json(&after_info), "expected": {"audio": true}}
        ]
    });
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    println!("Media acceptance passed: {}", output.display());
    Ok(())
}

fn artwork(label: &str, headline: &str, background: [u8; 4]) -> Result<Document> {
    let mut document = Document::new(WIDTH, HEIGHT);
    document.name = label.into();
    // `.comp` has no document-background field. A regular raster layer makes
    // this fixture portable through the production document save path.
    document.background = [0; 4];
    let mut background_layer = Layer::paint(format!("{label} background"), WIDTH, HEIGHT);
    for pixel in background_layer
        .image
        .as_mut()
        .expect("paint layers have pixels")
        .pixels_mut()
    {
        *pixel = Rgba(background);
    }
    document.layers = vec![background_layer];
    document.layers.push(objects::live_text_layer(
        format!("{label} title"),
        ObjectPoint { x: 52., y: 62. },
        LiveTextStyle {
            content: headline.into(),
            font_name: "sans-serif".into(),
            font_size: 58.,
            red: 0.96,
            green: 0.91,
            blue: 0.78,
            alignment: TextAlignment::Left,
            tracking: 0.,
            leading: 66.,
            box_size: Some(ObjectSize {
                width: 520.,
                height: 220.,
            }),
            runs: vec![],
        },
    )?);
    document.layers.push(objects::live_text_layer(
        format!("{label} label"),
        ObjectPoint { x: 54., y: 332. },
        LiveTextStyle {
            content: format!("{label} · Linux-native creative tooling"),
            font_name: "sans-serif".into(),
            font_size: 20.,
            red: 0.79,
            green: 0.83,
            blue: 0.93,
            alignment: TextAlignment::Left,
            tracking: 1.2,
            leading: 26.,
            box_size: Some(ObjectSize {
                width: 520.,
                height: 50.,
            }),
            runs: vec![],
        },
    )?);
    Ok(document)
}

fn has_live_text(document: &Document) -> Result<bool> {
    for layer in &document.layers {
        if objects::live_text(layer)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn live_text_id(document: &Document, name: &str) -> Result<String> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.name == name)
        .with_context(|| format!("Missing live text layer {name:?}"))?;
    ensure!(
        objects::live_text(layer)?.is_some(),
        "{name:?} is not live text"
    );
    Ok(layer.id.clone())
}

fn editable_captions() -> SubtitleTrack {
    SubtitleTrack {
        cues: vec![
            SubtitleCue {
                start_ms: 200,
                end_ms: 1_100,
                text: "Soft track: editable after export".into(),
            },
            SubtitleCue {
                start_ms: 1_500,
                end_ms: 2_600,
                text: "Burned style: readable on any player".into(),
            },
        ],
    }
}

fn captions_vtt() -> &'static str {
    "WEBVTT\n\n00:00:00.200 --> 00:00:01.100\nSoft track: editable after export\n\n00:00:01.500 --> 00:00:02.600\nBurned style: readable on any player\n"
}

fn export_options(
    audio: AudioClip,
    subtitles: SubtitleTrack,
    presentation: SubtitlePresentation,
) -> MotionExportOptions {
    MotionExportOptions {
        frames_per_second: FPS,
        max_frames: 64,
        audio: vec![audio],
        subtitles: Some(subtitles),
        subtitle_style: SubtitleStyle {
            presentation,
            font_size_px: 44,
            outline_px: 3,
            position: SubtitlePosition::Bottom,
        },
    }
}

fn checked_video(
    encoder: &motion::Ffmpeg,
    path: &Path,
    label: &str,
    min_duration_ms: u64,
    max_duration_ms: u64,
) -> Result<VideoClipInfo> {
    let info = motion::probe_video_clip(encoder, path)
        .with_context(|| format!("Probing {label} at {}", path.display()))?;
    ensure!(
        (info.width, info.height) == (WIDTH, HEIGHT),
        "{label} dimensions changed to {}x{}",
        info.width,
        info.height
    );
    ensure!(
        (min_duration_ms..=max_duration_ms).contains(&info.duration_ms),
        "{label} duration {} ms is outside {min_duration_ms}–{max_duration_ms} ms",
        info.duration_ms
    );
    ensure!(info.has_audio, "{label} lost its audio stream");
    Ok(info)
}

fn inspect_streams(encoder: &motion::Ffmpeg, path: &Path) -> Result<Value> {
    let ffprobe = encoder.executable().with_file_name("ffprobe");
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,width,height,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Inspecting streams in {}", path.display()))?;
    ensure!(
        output.status.success(),
        "FFprobe could not inspect {}",
        path.display()
    );
    ensure!(
        output.stdout.len() <= 1024 * 1024,
        "FFprobe response is too large"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn has_stream(streams: &Value, kind: &str) -> bool {
    streams["streams"].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|stream| stream["codec_type"].as_str() == Some(kind))
    })
}

/// Decode the deliberately simple acceptance tone rather than accepting a
/// container audio stream as proof it carries audible content. The fixture is
/// bounded to 3.2 seconds, and its decoded size is verified before analysis.
fn decoded_tone(encoder: &motion::Ffmpeg, path: &Path, label: &str) -> Result<Value> {
    const SAMPLE_RATE: f64 = 48_000.0;
    const MAX_DECODED_BYTES: usize = 10 * 48_000 * 4;
    let output = Command::new(encoder.executable())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Decoding audio from {label}"))?
        .wait_with_output()
        .with_context(|| format!("Waiting for decoded audio from {label}"))?;
    ensure!(
        output.status.success(),
        "FFmpeg could not decode {label} audio: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure!(
        output.stdout.len() <= MAX_DECODED_BYTES && output.stdout.len() % 4 == 0,
        "Decoded {label} audio is invalid or exceeds its bounded fixture size"
    );

    let samples = output
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte sample")))
        .collect::<Vec<_>>();
    ensure!(
        samples.len() >= 48_000,
        "Decoded {label} audio is too short"
    );
    ensure!(
        samples.iter().all(|sample| sample.is_finite()),
        "Decoded {label} audio contains non-finite samples"
    );
    let peak = samples
        .iter()
        .fold(0.0f64, |peak, sample| peak.max(f64::from(sample.abs())));
    let rms = (samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    ensure!(
        peak >= 0.02 && rms >= 0.01,
        "Decoded {label} audio is effectively silent (peak {peak:.5}, RMS {rms:.5})"
    );

    let mut upward_crossings = 0u64;
    let mut first_crossing = None;
    let mut last_crossing = 0.0;
    for (index, pair) in samples.windows(2).enumerate() {
        let (previous, current) = (f64::from(pair[0]), f64::from(pair[1]));
        if previous <= 0.0 && current > 0.0 {
            let crossing = index as f64 + previous.abs() / (current - previous);
            first_crossing.get_or_insert(crossing);
            last_crossing = crossing;
            upward_crossings += 1;
        }
    }
    let first_crossing = first_crossing.context("Decoded tone has no zero crossings")?;
    ensure!(
        upward_crossings >= 100 && last_crossing > first_crossing,
        "Decoded {label} has too few tone cycles"
    );
    let frequency_hz =
        (upward_crossings - 1) as f64 * SAMPLE_RATE / (last_crossing - first_crossing);
    ensure!(
        (430.0..=450.0).contains(&frequency_hz),
        "Decoded {label} tone is {frequency_hz:.2} Hz rather than approximately 440 Hz"
    );
    Ok(json!({
        "sampleRateHz": SAMPLE_RATE as u32,
        "samples": samples.len(),
        "peak": peak,
        "rms": rms,
        "upwardZeroCrossings": upward_crossings,
        "estimatedFrequencyHz": frequency_hz
    }))
}

fn capture_frame(
    encoder: &motion::Ffmpeg,
    source: &Path,
    destination: &Path,
    timestamp: &str,
) -> Result<()> {
    let output = Command::new(encoder.executable())
        .args(["-hide_banner", "-loglevel", "error", "-ss", timestamp, "-i"])
        .arg(source)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an"])
        .arg(destination)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Capturing {}", destination.display()))?;
    ensure!(
        output.status.success() && destination.is_file(),
        "FFmpeg could not capture {}: {}",
        destination.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn write_pcm_tone(path: &Path, duration_ms: u64) -> Result<()> {
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    let sample_count = duration_ms
        .checked_mul(u64::from(SAMPLE_RATE))
        .context("tone duration overflow")?
        / 1_000;
    let data_bytes = sample_count.checked_mul(2).context("tone size overflow")?;
    ensure!(
        data_bytes <= u64::from(u32::MAX),
        "tone fixture is too large"
    );
    let mut file = fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36u32 + data_bytes as u32).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&SAMPLE_RATE.to_le_bytes())?;
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(CHANNELS * BITS_PER_SAMPLE / 8).to_le_bytes())?;
    file.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&(data_bytes as u32).to_le_bytes())?;
    for sample in 0..sample_count {
        let phase = sample as f32 * std::f32::consts::TAU * 440.0 / SAMPLE_RATE as f32;
        let value = (phase.sin() * i16::MAX as f32 * 0.2) as i16;
        file.write_all(&value.to_le_bytes())?;
    }
    file.sync_all()?;
    Ok(())
}

fn video_json(info: &VideoClipInfo) -> Value {
    json!({
        "width": info.width,
        "height": info.height,
        "durationMs": info.duration_ms,
        "hasAudio": info.has_audio
    })
}

fn artifact(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
