//! Bounded Photoshop Type Tool descriptors and EngineData. This intentionally
//! accepts only text Omuse can edit without discarding a layout feature. The
//! caller keeps Photoshop's cached pixels; unsupported records get a visible
//! conversion note instead of invented font/style defaults.
use crate::objects::{LiveTextStyle, RichTextRun, TextAlignment};
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;

const MAX_DATA: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_VALUES: usize = 30_000;
const MAX_TEXT: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Number(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Enum(String),
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Null,
}
impl Value {
    fn at(&self, path: &[&str]) -> Option<&Value> {
        let mut value = self;
        for key in path {
            value = value.object()?.get(*key)?;
        }
        Some(value)
    }
    fn object(&self) -> Option<&BTreeMap<String, Value>> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn array(&self) -> Option<&[Value]> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn text(&self) -> Option<&str> {
        if let Self::String(v) | Self::Enum(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn number(&self) -> Option<f64> {
        if let Self::Number(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    fn boolean(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

pub(super) fn parse(data: &[u8]) -> Result<LiveTextStyle> {
    ensure!(data.len() <= MAX_DATA, "text descriptor exceeds 8 MiB");
    let mut reader = Descriptor {
        bytes: data,
        offset: 0,
        remaining: MAX_VALUES,
    };
    ensure!(
        reader.u16()? == 1,
        "unsupported Type Tool descriptor version"
    );
    let mut matrix = [0.; 6];
    for item in &mut matrix {
        *item = reader.f64()?;
    }
    let scale = matrix[0];
    ensure!(
        scale > 0.
            && scale.is_finite()
            && matrix[1].abs() < 1e-6
            && matrix[2].abs() < 1e-6
            && (matrix[3] - scale).abs() < 1e-6 * scale.max(1.),
        "rotated, flipped, sheared or nonuniformly scaled text remains rasterized"
    );
    ensure!(
        reader.u16()? == 50 && reader.u32()? == 16,
        "unsupported text descriptor version"
    );
    let text = reader.descriptor(0)?;
    ensure!(
        text.at(&["Ornt"])
            .and_then(Value::text)
            .is_none_or(|s| s == "Hrzn"),
        "vertical Photoshop text remains rasterized"
    );
    // A Type Tool block includes its warp descriptor and a final bounds rect.
    // Refuse a missing/malformed warp section rather than assuming no warp.
    ensure!(
        reader.u16()? == 1 && reader.u32()? == 16,
        "unsupported Photoshop warp descriptor"
    );
    let warp = reader.descriptor(0)?;
    ensure!(
        warp.at(&["warpStyle"])
            .and_then(Value::text)
            .is_some_and(|s| matches!(s, "warpNone" | "none")),
        "warped Photoshop text remains rasterized"
    );
    if reader.bytes.len() > reader.offset {
        // Real Photoshop PSD records may include up to three zero bytes of
        // four-byte tagged-block alignment inside the declared TySh length.
        // Keep bounds compulsory whenever a tail is present, and do not treat
        // arbitrary trailing data as ignorable padding.
        let tail = reader.bytes.len() - reader.offset;
        ensure!((16..=19).contains(&tail), "invalid Type Tool bounds");
        reader.take(16)?;
        ensure!(
            reader.take(tail - 16)?.iter().all(|byte| *byte == 0),
            "invalid Type Tool padding"
        );
    }
    let raw = match text.at(&["EngineData"]) {
        Some(Value::Bytes(v)) => v,
        _ => bail!("text has no supported EngineData styling"),
    };
    let engine = Engine::parse(raw)?;
    let content = text
        .at(&["Txt "])
        .or_else(|| text.at(&["Txt"]))
        .and_then(Value::text)
        .or_else(|| {
            engine
                .at(&["EngineDict", "Editor", "Text"])
                .and_then(Value::text)
        })
        .context("missing Photoshop text content")?;
    let content = content
        .trim_start_matches(['\u{feff}', '\0'])
        .trim_end_matches('\0')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    ensure!(
        !content.is_empty() && content.encode_utf16().count() <= MAX_TEXT,
        "Photoshop text is empty or exceeds 100000 UTF-16 units"
    );
    let runs = engine
        .at(&["EngineDict", "StyleRun", "RunArray"])
        .and_then(Value::array)
        .context("missing Photoshop text style runs")?;
    let first = runs.first().context("missing Photoshop text style")?;
    let data = first.at(&["StyleSheet", "StyleSheetData"]).unwrap_or(first);
    ensure!(data.object().is_some(), "invalid Photoshop text style");
    for run in runs.iter().skip(1) {
        let other = run.at(&["StyleSheet", "StyleSheetData"]).unwrap_or(run);
        ensure!(
            other == data,
            "mixed Photoshop text styles remain rasterized"
        );
    }
    let font_index = required_number(data, "Font")?;
    ensure!(
        font_index >= 0. && font_index.fract() == 0.,
        "invalid Photoshop font index"
    );
    let fonts = engine
        .at(&["ResourceDict", "FontSet"])
        .and_then(Value::array)
        .context("missing Photoshop font set")?;
    let font = fonts
        .get(font_index as usize)
        .and_then(|v| v.at(&["Name"]))
        .and_then(Value::text)
        .context("Photoshop font index is out of bounds")?;
    for key in ["HorizontalScale", "VerticalScale"] {
        ensure!(
            (number(data, key, 1.)? - 1.).abs() < 1e-6,
            "scaled Photoshop glyphs remain rasterized"
        );
    }
    for key in ["BaselineShift", "FontCaps", "FontBaseline"] {
        ensure!(
            number(data, key, 0.)? == 0.,
            "Photoshop {key} text remains rasterized"
        );
    }
    for key in ["Underline", "Strikethrough"] {
        ensure!(
            data.at(&[key])
                .is_none_or(|v| matches!(v, Value::Bool(false) | Value::Number(0.))),
            "Photoshop {key} text remains rasterized"
        );
    }
    ensure!(
        !boolean(data, "StrokeFlag", false)?,
        "outlined Photoshop text remains rasterized"
    );
    ensure!(
        boolean(data, "FillFlag", true)?,
        "unfilled Photoshop text remains rasterized"
    );
    let size = required_number(data, "FontSize")? * scale;
    let rgba = fill_color(data)?;
    let paragraphs = engine
        .at(&["EngineDict", "ParagraphRun", "RunArray"])
        .and_then(Value::array)
        .context("missing Photoshop paragraph style")?;
    let mut alignment = None;
    for paragraph in paragraphs {
        let properties = paragraph
            .at(&["ParagraphSheet", "Properties"])
            .context("missing Photoshop paragraph properties")?;
        let current = match number(properties, "Justification", 0.)? {
            0. => TextAlignment::Left,
            1. => TextAlignment::Right,
            2. => TextAlignment::Center,
            _ => bail!("justified Photoshop text remains rasterized"),
        };
        ensure!(
            alignment.is_none_or(|a| a == current),
            "mixed paragraph alignment remains rasterized"
        );
        for key in [
            "FirstLineIndent",
            "StartIndent",
            "EndIndent",
            "SpaceBefore",
            "SpaceAfter",
        ] {
            ensure!(
                number(properties, key, 0.)? == 0.,
                "Photoshop paragraph spacing remains rasterized"
            );
        }
        alignment = Some(current);
    }
    // Engine ShapeType 1 is paragraph text. Keep its wrapping as cached pixels
    // until the box model can be faithfully converted; point text is type 0.
    if let Some(shapes) = engine
        .at(&["EngineDict", "Rendered", "Shapes", "Children"])
        .and_then(Value::array)
    {
        for shape in shapes {
            ensure!(
                number(shape, "ShapeType", 0.)? == 0.,
                "Photoshop paragraph-box text remains rasterized"
            );
        }
    }
    let bold = boolean(data, "FauxBold", false)?;
    let italic = boolean(data, "FauxItalic", false)?;
    let mut style = LiveTextStyle {
        content,
        font_name: font.to_owned(),
        font_size: size as f32,
        red: rgba[0],
        green: rgba[1],
        blue: rgba[2],
        alignment: alignment.unwrap_or_default(),
        tracking: (number(data, "Tracking", 0.)? * size / 1000.) as f32,
        leading: if boolean(data, "AutoLeading", true)? {
            0.
        } else {
            (required_number(data, "Leading")? * scale) as f32
        },
        box_size: None,
        runs: vec![],
    };
    if bold || italic || rgba[3] < 1. {
        style.runs.push(RichTextRun {
            start: 0,
            end: style.content.len(),
            font_name: None,
            font_size: None,
            weight: bold.then_some(700),
            italic: italic.then_some(true),
            color: (rgba[3] < 1.).then_some(rgba),
        });
    }
    crate::objects::validate_text_style(&style)?;
    Ok(style)
}

fn number(data: &Value, key: &str, default: f64) -> Result<f64> {
    match data.at(&[key]) {
        None => Ok(default),
        Some(v) => v
            .number()
            .filter(|n| n.is_finite())
            .context("invalid numeric Photoshop text style"),
    }
}
fn required_number(data: &Value, key: &str) -> Result<f64> {
    data.at(&[key])
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .with_context(|| format!("missing Photoshop {key}"))
}
fn boolean(data: &Value, key: &str, default: bool) -> Result<bool> {
    match data.at(&[key]) {
        None => Ok(default),
        Some(v) => v.boolean().context("invalid boolean Photoshop text style"),
    }
}
fn fill_color(data: &Value) -> Result<[f32; 4]> {
    let fill = data
        .at(&["FillColor"])
        .context("missing Photoshop text colour")?;
    let values = fill
        .at(&["Values"])
        .and_then(Value::array)
        .context("missing Photoshop text colour components")?;
    let values = values
        .iter()
        .map(|v| {
            v.number()
                .filter(|v| v.is_finite() && (0. ..=1.).contains(v))
                .context("unsupported Photoshop text colour components")
        })
        .collect::<Result<Vec<_>>>()?;
    match (number(fill, "Type", 1.)?, values.as_slice()) {
        (1., [alpha, r, g, b]) => Ok([*r as f32, *g as f32, *b as f32, *alpha as f32]),
        (0., [alpha, gray]) => Ok([*gray as f32, *gray as f32, *gray as f32, *alpha as f32]),
        _ => bail!("non-RGB Photoshop text colours remain rasterized"),
    }
}

struct Descriptor<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: usize,
}
impl<'a> Descriptor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        ensure!(
            count <= self.bytes.len().saturating_sub(self.offset),
            "truncated Photoshop text descriptor"
        );
        let result = &self.bytes[self.offset..self.offset + count];
        self.offset += count;
        Ok(result)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn f64(&mut self) -> Result<f64> {
        let number = f64::from_be_bytes(self.take(8)?.try_into()?);
        ensure!(number.is_finite(), "non-finite Photoshop text value");
        Ok(number)
    }
    fn unicode(&mut self) -> Result<String> {
        let count = self.u32()? as usize;
        ensure!(count <= MAX_TEXT, "Photoshop descriptor text exceeds limit");
        let bytes = self.take(count.checked_mul(2).context("text size overflow")?)?;
        utf16(bytes)
    }
    fn identifier(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        let len = if len == 0 { 4 } else { len };
        ensure!(len <= 512, "Photoshop descriptor identifier exceeds limit");
        let data = self.take(len)?;
        ensure!(data.is_ascii(), "invalid Photoshop descriptor identifier");
        Ok(std::str::from_utf8(data)?.to_owned())
    }
    fn descriptor(&mut self, depth: usize) -> Result<Value> {
        ensure!(
            depth < MAX_DEPTH,
            "Photoshop text descriptor nesting exceeds limit"
        );
        self.unicode()?;
        self.identifier()?;
        let count = self.u32()? as usize;
        ensure!(
            count <= self.remaining,
            "Photoshop text descriptor item budget exceeded"
        );
        let mut object = BTreeMap::new();
        for _ in 0..count {
            let key = self.identifier()?;
            let kind = self.take(4)?;
            let value = self.value(kind, depth + 1)?;
            ensure!(
                object.insert(key, value).is_none(),
                "duplicate Photoshop text descriptor item"
            );
        }
        Ok(Value::Object(object))
    }
    fn value(&mut self, kind: &[u8], depth: usize) -> Result<Value> {
        ensure!(
            depth < MAX_DEPTH && self.remaining > 0,
            "Photoshop text descriptor budget exceeded"
        );
        self.remaining -= 1;
        Ok(match kind {
            b"TEXT" => Value::String(self.unicode()?),
            b"doub" => Value::Number(self.f64()?),
            b"UntF" => {
                self.take(4)?;
                Value::Number(self.f64()?)
            }
            b"long" => Value::Number(i32::from_be_bytes(self.take(4)?.try_into()?) as f64),
            b"comp" => Value::Number(i64::from_be_bytes(self.take(8)?.try_into()?) as f64),
            b"bool" => {
                let value = self.take(1)?[0];
                ensure!(value <= 1, "invalid descriptor boolean");
                Value::Bool(value != 0)
            }
            b"enum" => {
                self.identifier()?;
                Value::Enum(self.identifier()?)
            }
            b"tdta" | b"alis" => {
                let len = self.u32()? as usize;
                ensure!(len <= MAX_DATA, "Photoshop engine exceeds 8 MiB");
                Value::Bytes(self.take(len)?.to_vec())
            }
            b"Objc" | b"GlbO" => self.descriptor(depth + 1)?,
            b"VlLs" => {
                let count = self.u32()? as usize;
                ensure!(
                    count <= self.remaining,
                    "Photoshop text list budget exceeded"
                );
                let mut values = Vec::new();
                for _ in 0..count {
                    let kind = self.take(4)?;
                    values.push(self.value(kind, depth + 1)?);
                }
                Value::Array(values)
            }
            b"type" | b"GlbC" => {
                self.unicode()?;
                self.identifier()?;
                Value::Null
            }
            _ => bail!("unsupported Photoshop text descriptor item"),
        })
    }
}

fn utf16(bytes: &[u8]) -> Result<String> {
    ensure!(bytes.len() % 2 == 0, "invalid Photoshop UTF-16 text");
    String::from_utf16(
        &bytes
            .chunks_exact(2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .collect::<Vec<_>>(),
    )
    .context("invalid Photoshop UTF-16 text")
}

struct Engine<'a> {
    data: &'a [u8],
    offset: usize,
    remaining: usize,
}
impl<'a> Engine<'a> {
    fn parse(data: &'a [u8]) -> Result<Value> {
        ensure!(data.len() <= MAX_DATA, "Photoshop engine exceeds 8 MiB");
        let mut parser = Self {
            data,
            offset: 0,
            remaining: MAX_VALUES,
        };
        let result = parser.value(0)?;
        parser.whitespace();
        ensure!(
            parser.offset == data.len() && result.object().is_some(),
            "invalid Photoshop engine dictionary"
        );
        Ok(result)
    }
    fn whitespace(&mut self) {
        loop {
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_whitespace() || c == 0)
            {
                self.offset += 1;
            }
            if self.peek() != Some(b'%') {
                break;
            }
            while self.peek().is_some_and(|c| c != b'\n' && c != b'\r') {
                self.offset += 1;
            }
        }
    }
    fn peek(&self) -> Option<u8> {
        self.data.get(self.offset).copied()
    }
    fn byte(&mut self) -> Result<u8> {
        let byte = self.peek().context("truncated Photoshop engine")?;
        self.offset += 1;
        Ok(byte)
    }
    fn consume(&mut self, bytes: &[u8]) -> bool {
        if self.data[self.offset..].starts_with(bytes) {
            self.offset += bytes.len();
            true
        } else {
            false
        }
    }
    fn word(&mut self) -> Result<String> {
        let start = self.offset;
        while self
            .peek()
            .is_some_and(|c| !c.is_ascii_whitespace() && !b"<>[]()/{}%\0".contains(&c))
        {
            self.offset += 1;
        }
        ensure!(
            self.offset > start && self.offset - start <= 512,
            "invalid Photoshop engine token"
        );
        Ok(std::str::from_utf8(&self.data[start..self.offset])
            .context("invalid Photoshop engine token")?
            .to_owned())
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        ensure!(
            depth < MAX_DEPTH && self.remaining > 0,
            "Photoshop engine nesting/item budget exceeded"
        );
        self.remaining -= 1;
        self.whitespace();
        if self.consume(b"<<") {
            let mut object = BTreeMap::new();
            loop {
                self.whitespace();
                if self.consume(b">>") {
                    break;
                }
                ensure!(self.consume(b"/"), "expected Photoshop engine property");
                let key = self.word()?;
                let value = self.value(depth + 1)?;
                ensure!(
                    object.insert(key, value).is_none(),
                    "duplicate Photoshop engine property"
                );
            }
            return Ok(Value::Object(object));
        }
        if self.consume(b"[") {
            let mut values = Vec::new();
            loop {
                self.whitespace();
                if self.consume(b"]") {
                    break;
                }
                values.push(self.value(depth + 1)?);
            }
            return Ok(Value::Array(values));
        }
        if self.consume(b"(") {
            return Ok(Value::String(self.string()?));
        }
        if self.consume(b"<") {
            let mut digits = Vec::new();
            loop {
                let c = self.byte()?;
                if c == b'>' {
                    break;
                }
                if c.is_ascii_whitespace() {
                    continue;
                }
                let digit = (c as char)
                    .to_digit(16)
                    .context("invalid Photoshop engine hex string")?
                    as u8;
                ensure!(
                    digits.len() <= MAX_TEXT * 4,
                    "Photoshop engine string exceeds limit"
                );
                digits.push(digit);
            }
            if digits.len() % 2 != 0 {
                digits.push(0);
            }
            return Ok(Value::String(Self::decode_string(
                &digits
                    .chunks_exact(2)
                    .map(|b| b[0] * 16 + b[1])
                    .collect::<Vec<_>>(),
            )?));
        }
        if self.consume(b"/") {
            return Ok(Value::String(self.word()?));
        }
        let word = self.word()?;
        Ok(match word.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            _ => {
                let number = word
                    .parse::<f64>()
                    .context("invalid Photoshop engine number")?;
                ensure!(number.is_finite(), "non-finite Photoshop engine number");
                Value::Number(number)
            }
        })
    }
    fn string(&mut self) -> Result<String> {
        let mut bytes = Vec::new();
        let mut nesting = 1usize;
        while nesting > 0 {
            ensure!(
                bytes.len() <= MAX_TEXT * 4,
                "Photoshop engine string exceeds limit"
            );
            let mut c = self.byte()?;
            if c == b'\\' {
                c = self.byte()?;
                match c {
                    b'n' => c = b'\n',
                    b'r' => c = b'\r',
                    b't' => c = b'\t',
                    b'b' => c = 8,
                    b'f' => c = 12,
                    b'\r' => {
                        if self.peek() == Some(b'\n') {
                            self.offset += 1;
                        }
                        continue;
                    }
                    b'\n' => continue,
                    b'0'..=b'7' => {
                        let mut value = u16::from(c - b'0');
                        for _ in 0..2 {
                            if self.peek().is_some_and(|b| (b'0'..=b'7').contains(&b)) {
                                value = value * 8 + u16::from(self.byte()? - b'0');
                            } else {
                                break;
                            }
                        }
                        c = value as u8;
                    }
                    _ => {}
                }
                bytes.push(c);
            } else if c == b'(' {
                nesting += 1;
                ensure!(
                    nesting <= MAX_DEPTH,
                    "Photoshop string nesting exceeds limit"
                );
                bytes.push(c);
            } else if c == b')' {
                nesting -= 1;
                if nesting > 0 {
                    bytes.push(c);
                }
            } else {
                bytes.push(c);
            }
        }
        Self::decode_string(&bytes)
    }
    fn decode_string(bytes: &[u8]) -> Result<String> {
        if bytes.starts_with(&[0xfe, 0xff]) {
            utf16(&bytes[2..])
        } else {
            Ok(std::str::from_utf8(bytes)
                .context("invalid Photoshop engine text encoding")?
                .to_owned())
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn id(data: &mut Vec<u8>, key: &str) {
        data.extend_from_slice(&(if key.len() == 4 { 0 } else { key.len() as u32 }).to_be_bytes());
        data.extend_from_slice(key.as_bytes());
    }
    fn descriptor(data: &mut Vec<u8>, class: &str, values: &[(&str, Vec<u8>)]) {
        data.extend_from_slice(&16u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        id(data, class);
        data.extend_from_slice(&(values.len() as u32).to_be_bytes());
        for (key, value) in values {
            id(data, key);
            data.extend_from_slice(value);
        }
    }
    fn string(value: &str) -> Vec<u8> {
        let mut result = b"TEXT".to_vec();
        let utf = value.encode_utf16().collect::<Vec<_>>();
        result.extend_from_slice(&(utf.len() as u32).to_be_bytes());
        for unit in utf {
            result.extend_from_slice(&unit.to_be_bytes());
        }
        result
    }
    fn enumeration(kind: &str, value: &str) -> Vec<u8> {
        let mut result = b"enum".to_vec();
        id(&mut result, kind);
        id(&mut result, value);
        result
    }
    pub(crate) fn fixture(content: &str, style_extra: &str, warp: bool) -> Vec<u8> {
        let engine = format!(
            "<< /EngineDict << /StyleRun << /RunArray [ << /StyleSheet << /StyleSheetData << /Font 0 /FontSize 24 /Tracking 20 /AutoLeading false /Leading 30 /FillColor << /Type 1 /Values [1 .2 .4 .6] >> {style_extra} >> >> >> ] >> /ParagraphRun << /RunArray [ << /ParagraphSheet << /Properties << /Justification 0 >> >> >> ] >> >> /ResourceDict << /FontSet [ << /Name (DejaVu Sans) >> ] >> >>"
        );
        let mut data = 1u16.to_be_bytes().to_vec();
        for value in [1f64, 0., 0., 1., 12., 34.] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        data.extend_from_slice(&50u16.to_be_bytes());
        let mut raw = b"tdta".to_vec();
        raw.extend_from_slice(&(engine.len() as u32).to_be_bytes());
        raw.extend_from_slice(engine.as_bytes());
        descriptor(
            &mut data,
            "TxLr",
            &[
                ("Txt ", string(content)),
                ("Ornt", enumeration("Ornt", "Hrzn")),
                ("EngineData", raw),
            ],
        );
        data.extend_from_slice(&1u16.to_be_bytes());
        descriptor(
            &mut data,
            "warp",
            &[(
                "warpStyle",
                enumeration("warpStyle", if warp { "warpArc" } else { "warpNone" }),
            )],
        );
        data
    }
    #[test]
    fn simple_unicode_text_preserves_editable_style() {
        let style = parse(&fixture(
            "Hello 🧡\r世界",
            "/FauxBold true /FauxItalic true",
            false,
        ))
        .unwrap();
        assert_eq!(style.content, "Hello 🧡\n世界");
        assert_eq!(style.font_name, "DejaVu Sans");
        assert_eq!(style.font_size, 24.);
        assert_eq!(style.leading, 30.);
        assert_eq!(style.tracking, 0.48);
        assert_eq!([style.red, style.green, style.blue], [0.2, 0.4, 0.6]);
        assert_eq!(style.runs[0].end, style.content.len());
        assert_eq!(style.runs[0].weight, Some(700));
        assert_eq!(style.runs[0].italic, Some(true));
    }
    #[test]
    fn independent_photoshop_type_tool_record_is_editable() {
        let data = include_bytes!("../tests/fixtures/photoshop/psd-tools-text.TySh");
        let style = parse(data).expect("independent Photoshop Type Tool record");
        assert_eq!(style.font_name, "ArialMT");
        assert_eq!(style.font_size, 13.);
        assert!(style.content.starts_with("Line 1\nLine 2\nLine 3"));
        assert_eq!(style.alignment, TextAlignment::Left);
        assert_eq!([style.red, style.green, style.blue], [0., 0., 0.]);
    }
    #[test]
    fn independent_psd_type_tool_alignment_padding_is_supported() {
        let data = include_bytes!("../tests/fixtures/photoshop/psd-tools-text-psd.TySh");
        let style = parse(data).expect("independent Photoshop PSD Type Tool record");
        assert!(style.content.starts_with("Line 1\nLine 2\nLine 3"));
        assert_eq!(style.font_name, "ArialMT");
        assert_eq!(style.font_size, 13.);
        let mut invalid = data.to_vec();
        *invalid.last_mut().unwrap() = 1;
        assert!(parse(&invalid).unwrap_err().to_string().contains("padding"));
        invalid.extend_from_slice(&[0; 4]);
        assert!(parse(&invalid).is_err());
    }
    #[test]
    fn malformed_and_unsupported_text_falls_back_without_panics() {
        let data = fixture("Title", "", false);
        for end in 0..data.len() {
            assert!(parse(&data[..end]).is_err(), "truncation {end}");
        }
        assert!(
            parse(&fixture("Title", "", true))
                .unwrap_err()
                .to_string()
                .contains("warped")
        );
        assert!(parse(&fixture("Title", "/HorizontalScale 1.5", false)).is_err());
        let mut rotated = data.clone();
        rotated[10..18].copy_from_slice(&0.5f64.to_be_bytes());
        assert!(parse(&rotated).unwrap_err().to_string().contains("rotated"));
        let mut nan = data;
        nan[2..10].copy_from_slice(&f64::NAN.to_be_bytes());
        assert!(parse(&nan).is_err());
    }
    #[test]
    fn engine_strings_decode_utf16_escapes_and_bound_nesting() {
        let value = Engine::parse(
            b"<< /A <FEFF0048D83EDD E10069> /B (a\\101\\n\\(b\\)) /C [1 -2.5 true] >>",
        )
        .unwrap();
        assert_eq!(value.at(&["A"]).and_then(Value::text), Some("H🧡i"));
        assert_eq!(value.at(&["B"]).and_then(Value::text), Some("aA\n(b)"));
        let deep = format!("<< /A {}1{} >>", "[".repeat(10000), "]".repeat(10000));
        assert!(Engine::parse(deep.as_bytes()).is_err());
        for broken in [
            b"<< /A 1e999 >>".as_slice(),
            b"<< /A (oops",
            b"<< /A <FEFFd800> >>",
            b"<< /A true /A false >>",
        ] {
            assert!(Engine::parse(broken).is_err());
        }
    }
}
