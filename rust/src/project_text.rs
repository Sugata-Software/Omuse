//! Convert external project text without re-rendering its cached artwork.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;

use crate::objects::{LiveTextStyle, RichTextRun, validate_text_style};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ColorRun {
    location: usize,
    length: usize,
    red: f32,
    green: f32,
    blue: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontRun {
    location: usize,
    length: usize,
    font_name: String,
}

/// External formats 10/11 use independent UTF-16 colour/font ranges. Native
/// runs use non-overlapping UTF-8 boundaries, so split only at actual changes
/// and combine both overrides. Cached artwork is deliberately left untouched.
pub(crate) fn normalize_record(record: &mut Value, version: u64) -> Result<()> {
    let Some(text) = record.get_mut("text").filter(|text| !text.is_null()) else {
        return Ok(());
    };
    let colors = external_runs(text, "colorRuns", version, 10)?;
    let fonts = external_runs(text, "fontRuns", version, 11)?;
    if colors.is_none() && fonts.is_none() {
        return Ok(());
    }
    let colors = colors.unwrap_or_default();
    let fonts = fonts.unwrap_or_default();
    let mut style: LiveTextStyle =
        serde_json::from_value(text.clone()).context("Invalid external text metadata")?;
    validate_text_style(&style)?;
    ensure!(
        style.runs.is_empty() || (colors.is_empty() && fonts.is_empty()),
        "Text contains conflicting native and legacy run encodings"
    );
    let mut boundaries = vec![None; style.content.encode_utf16().count() + 1];
    let mut units = 0;
    for (byte, ch) in style.content.char_indices() {
        boundaries[units] = Some(byte);
        units += ch.len_utf16();
    }
    boundaries[units] = Some(style.content.len());
    let mut color_ranges = Vec::with_capacity(colors.len());
    let mut font_ranges = Vec::with_capacity(fonts.len());
    let mut cuts = Vec::with_capacity((colors.len() + fonts.len()) * 2);
    let mut previous = 0;
    for raw in colors {
        let run: ColorRun =
            serde_json::from_value(raw.clone()).context("Invalid text colour run")?;
        let (start, end) = convert_range(&boundaries, run.location, run.length, &mut previous)?;
        ensure!(
            [run.red, run.green, run.blue]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "Invalid text colour"
        );
        color_ranges.push((start, end, [run.red, run.green, run.blue, 1.]));
        cuts.extend([start, end]);
    }
    previous = 0;
    for raw in fonts {
        let run: FontRun = serde_json::from_value(raw.clone()).context("Invalid text font run")?;
        let (start, end) = convert_range(&boundaries, run.location, run.length, &mut previous)?;
        ensure!(
            !run.font_name.trim().is_empty() && run.font_name.len() <= 512,
            "Invalid text font name"
        );
        font_ranges.push((start, end, run.font_name));
        cuts.extend([start, end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let (mut color_index, mut font_index) = (0, 0);
    for bounds in cuts.windows(2) {
        let (start, end) = (bounds[0], bounds[1]);
        while color_ranges
            .get(color_index)
            .is_some_and(|run| run.1 <= start)
        {
            color_index += 1;
        }
        while font_ranges
            .get(font_index)
            .is_some_and(|run| run.1 <= start)
        {
            font_index += 1;
        }
        let color = color_ranges
            .get(color_index)
            .filter(|run| run.0 <= start)
            .map(|run| run.2);
        let font_name = font_ranges
            .get(font_index)
            .filter(|run| run.0 <= start)
            .map(|run| run.2.clone());
        if color.is_none() && font_name.is_none() {
            continue;
        }
        ensure!(
            style.runs.len() < 4096,
            "Combined text styles exceed 4096 runs"
        );
        style.runs.push(RichTextRun {
            start,
            end,
            font_name,
            font_size: None,
            weight: None,
            italic: None,
            color,
        });
    }
    validate_text_style(&style)?;
    let text = text.as_object_mut().context("Invalid text metadata")?;
    text.remove("colorRuns");
    text.remove("fontRuns");
    if !style.runs.is_empty() {
        text.insert("runs".into(), serde_json::to_value(style.runs)?);
    }
    Ok(())
}

fn external_runs<'a>(
    text: &'a Value,
    key: &str,
    version: u64,
    minimum: u64,
) -> Result<Option<&'a [Value]>> {
    let Some(raw) = text.get(key).filter(|runs| !runs.is_null()) else {
        return Ok(None);
    };
    ensure!(
        version >= minimum,
        "Text {key} require project format {minimum}"
    );
    let raw = raw
        .as_array()
        .with_context(|| format!("Invalid text {key}"))?;
    ensure!(raw.len() <= 4096, "Text has too many {key}");
    Ok(Some(raw))
}

fn convert_range(
    boundaries: &[Option<usize>],
    location: usize,
    length: usize,
    previous: &mut usize,
) -> Result<(usize, usize)> {
    let end = location
        .checked_add(length)
        .context("Text range overflows")?;
    ensure!(
        length > 0 && location >= *previous && end < boundaries.len(),
        "Text ranges must be sorted, non-overlapping and within the text"
    );
    let start = boundaries[location].context("Text range splits a Unicode character")?;
    let end_byte = boundaries[end].context("Text range splits a Unicode character")?;
    *previous = end;
    Ok((start, end_byte))
}
