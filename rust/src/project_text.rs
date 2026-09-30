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

/// Format 10 uses UTF-16 ranges, whereas native Omuse rich text uses UTF-8
/// boundaries. Reject a range inside a surrogate pair instead of rounding it.
pub(crate) fn normalize_record(record: &mut Value, version: u64) -> Result<()> {
    let Some(text) = record.get_mut("text").filter(|text| !text.is_null()) else {
        return Ok(());
    };
    let Some(raw) = text.get("colorRuns").filter(|runs| !runs.is_null()) else {
        return Ok(());
    };
    ensure!(version >= 10, "Text colour runs require project format 10");
    let raw = raw.as_array().context("Invalid text colour runs")?;
    ensure!(raw.len() <= 4096, "Text has too many colour runs");
    let mut style: LiveTextStyle =
        serde_json::from_value(text.clone()).context("Invalid format-10 text metadata")?;
    validate_text_style(&style)?;
    ensure!(
        style.runs.is_empty() || raw.is_empty(),
        "Text contains conflicting native and legacy run encodings"
    );
    let mut boundaries = vec![None; style.content.encode_utf16().count() + 1];
    let mut units = 0;
    for (byte, ch) in style.content.char_indices() {
        boundaries[units] = Some(byte);
        units += ch.len_utf16();
    }
    boundaries[units] = Some(style.content.len());
    let mut previous_end = 0;
    for value in raw {
        let run: ColorRun =
            serde_json::from_value(value.clone()).context("Invalid text colour run")?;
        let end = run
            .location
            .checked_add(run.length)
            .context("Text colour range overflows")?;
        ensure!(
            run.length > 0 && run.location >= previous_end && end <= units,
            "Text colour ranges must be sorted, non-overlapping and within the text"
        );
        let start =
            boundaries[run.location].context("Text colour range splits a Unicode character")?;
        let end_byte = boundaries[end].context("Text colour range splits a Unicode character")?;
        style.runs.push(RichTextRun {
            start,
            end: end_byte,
            font_name: None,
            font_size: None,
            weight: None,
            italic: None,
            color: Some([run.red, run.green, run.blue, 1.]),
        });
        previous_end = end;
    }
    validate_text_style(&style)?;
    let text = text.as_object_mut().context("Invalid text metadata")?;
    text.remove("colorRuns");
    if !style.runs.is_empty() {
        text.insert("runs".into(), serde_json::to_value(style.runs)?);
    }
    Ok(())
}
