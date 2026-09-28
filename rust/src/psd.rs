//! Bounded Photoshop PSD (version 1, 8-bit RGB) layered import.
//!
//! This is an original implementation of Adobe's PSD file format records. It
//! retains supported Photoshop layers, while a PSD with no layer records opens
//! as one editable raster from its documented merged composite. Unsupported
//! live Photoshop descriptors still fail rather than silently flattening them.
use crate::model::{Document, Layer, MAX_LAYERS, MAX_PIXELS, valid_dimensions};
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use serde_json::{Value, json};
use std::{collections::HashMap, fs, io::Read, path::Path};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_ADDITIONAL_INFO_BLOCKS: usize = 4096;

#[derive(Default)]
struct RawLayer {
    name: String,
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    opacity: u8,
    fill: u8,
    clipping: bool,
    hidden: bool,
    blend: String,
    channels: Vec<(i16, usize)>,
    extra: HashMap<String, Vec<u8>>,
    mask_rect: [i32; 4],
    mask_disabled: bool,
    mask_linked: bool,
    mask_rendered: bool,
    has_mask: bool,
    section: Option<u32>,
    image: Option<RgbaImage>,
    mask: Option<RgbaImage>,
}

struct Record {
    parent: Option<String>,
    layer: Layer,
}

pub fn matches(path: &Path) -> bool {
    fs::File::open(path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut magic = [0; 4];
            f.read_exact(&mut magic)?;
            Ok(magic == *b"8BPS")
        })
        .unwrap_or(false)
}

pub fn open(path: &Path) -> Result<Document> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= MAX_FILE,
        "PSD exceeds 512 MiB"
    );
    let data = fs::read(path)?;
    parse(
        &data,
        path.file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("Photoshop"),
    )
}

fn parse(data: &[u8], name: &str) -> Result<Document> {
    let mut c = Cursor::new(data);
    ensure!(c.bytes(4)? == b"8BPS", "not a Photoshop document");
    ensure!(
        c.u16()? == 1,
        "Photoshop Large Document (PSB) is unsupported"
    );
    c.skip(6)?;
    let channels = c.u16()?;
    ensure!(
        (1..=56).contains(&channels),
        "PSD header channel count exceeds limit"
    );
    let height = c.u32()?;
    let width = c.u32()?;
    ensure!(
        valid_dimensions(width, height),
        "PSD dimensions exceed limits"
    );
    ensure!(
        c.u16()? == 8,
        "only 8-bit Photoshop documents are supported"
    );
    ensure!(c.u16()? == 3, "only RGB Photoshop documents are supported");
    let color_len = usize::try_from(c.u32()?)?;
    c.skip(color_len)?;
    let resources_end = section_end(&mut c)?;
    let mut resolution = 72.0;
    while c.pos < resources_end {
        ensure!(
            resources_end - c.pos >= 12,
            "truncated PSD image resource header"
        );
        ensure!(
            c.bytes(4)? == b"8BIM",
            "invalid PSD image resource signature"
        );
        let id = c.u16()?;
        let name_len = usize::from(c.u8()?);
        let name_end = c
            .pos
            .checked_add(name_len)
            .context("PSD resource name overflow")?;
        ensure!(
            name_end <= resources_end,
            "truncated PSD image resource name"
        );
        c.set(name_end)?;
        if (name_len + 1) % 2 == 1 {
            ensure!(c.pos < resources_end, "truncated PSD image resource name");
            c.skip(1)?;
        }
        let length_end = c
            .pos
            .checked_add(4)
            .context("PSD resource length overflow")?;
        ensure!(
            length_end <= resources_end,
            "truncated PSD image resource length"
        );
        let len = usize::try_from(c.u32()?)?;
        let start = c.pos;
        let payload_end = start.checked_add(len).context("PSD resource overflow")?;
        ensure!(
            payload_end <= resources_end,
            "truncated PSD image resource payload"
        );
        let padded_end = payload_end
            .checked_add(usize::from(len % 2 == 1))
            .context("PSD resource padding overflow")?;
        ensure!(
            padded_end <= resources_end,
            "truncated PSD image resource padding"
        );
        if id == 1039 && len != 0 {
            anyhow::bail!(
                "PSD has an embedded ICC profile. Omuse currently imports editable PSD content in sRGB; preserve the original and open a color-managed PNG/TIFF export or an sRGB PSD copy without an embedded profile."
            );
        }
        if id == 1005 && len >= 4 {
            resolution = (c.u32()? as f64 / 65536.).clamp(1., 9600.);
        }
        c.set(payload_end)?;
        if len % 2 == 1 {
            c.skip(1)?;
        }
    }
    c.set(resources_end)?;
    let layer_section_end = section_end(&mut c)?;
    if c.pos == layer_section_end {
        return flattened_document(
            width,
            height,
            channels,
            false,
            &c.data[layer_section_end..],
            name,
            resolution,
        );
    }
    ensure!(
        layer_section_end >= c.pos + 4,
        "truncated PSD layer section"
    );
    let info_len = usize::try_from(c.u32()?)?;
    let info_start = c.pos;
    let info_end = info_start
        .checked_add(info_len)
        .context("PSD layer info overflow")?;
    ensure!(
        info_end <= layer_section_end,
        "truncated PSD layer information"
    );
    if info_len == 0 {
        let merged_alpha = has_merged_transparency(&c.data[info_end..layer_section_end])?;
        return flattened_document(
            width,
            height,
            channels,
            merged_alpha,
            &c.data[layer_section_end..],
            name,
            resolution,
        );
    }
    ensure!(info_len >= 2, "truncated PSD layer information");
    let mut layers = Cursor::new(&c.data[info_start..info_end]);
    let count = usize::from(layers.i16()?.unsigned_abs());
    if count == 0 {
        let merged_alpha = has_merged_transparency(&c.data[info_end..layer_section_end])?;
        return flattened_document(
            width,
            height,
            channels,
            merged_alpha,
            &c.data[layer_section_end..],
            name,
            resolution,
        );
    }
    ensure!(count <= MAX_LAYERS, "PSD layer count exceeds limit");
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        raw.push(read_record(&mut layers)?);
    }
    let mut used = 0u64;
    for layer in &mut raw {
        decode_channels(&mut layers, layer, &mut used)?;
    }
    c.set(layer_section_end)?;
    let records = assemble(raw, width, height)?;
    let layers = nest(records, None)?;
    Ok(Document {
        width,
        height,
        name: name.into(),
        background: [0, 0, 0, 0],
        layers,
        metadata: json!({"documentID": uuid::Uuid::new_v4().to_string().to_uppercase(), "resolution": resolution, "sourceFormat": "PSD"}),
    })
}

/// The layer-and-mask section can carry tagged blocks after the global mask.
/// `Mtrn` is Photoshop's explicit declaration that the extra merged-image
/// plane is transparency, rather than an arbitrary saved/spot channel.
fn has_merged_transparency(data: &[u8]) -> Result<bool> {
    if data.is_empty() {
        return Ok(false);
    }
    let mut c = Cursor::new(data);
    let global_mask_len = usize::try_from(c.u32()?)?;
    c.skip(global_mask_len)?;
    let mut merged_transparency = false;
    let mut blocks = 0usize;
    while c.pos < c.data.len() {
        blocks += 1;
        ensure!(
            blocks <= MAX_ADDITIONAL_INFO_BLOCKS,
            "PSD has too many additional layer information blocks"
        );
        let signature = c.ascii(4)?;
        ensure!(
            signature == "8BIM" || signature == "8B64",
            "invalid PSD additional layer information signature"
        );
        let key = c.ascii(4)?;
        let len = if signature == "8B64" {
            usize::try_from(c.u64()?).context("PSD additional info exceeds address space")?
        } else {
            usize::try_from(c.u32()?)?
        };
        c.skip(len)?;
        if len % 2 == 1 {
            c.skip(1)?;
        }
        merged_transparency |= matches!(key.as_str(), "Mtrn" | "Mt16" | "Mt32");
    }
    Ok(merged_transparency)
}

fn flattened_document(
    width: u32,
    height: u32,
    channels: u16,
    merged_transparency: bool,
    data: &[u8],
    name: &str,
    resolution: f64,
) -> Result<Document> {
    let channel_count = match channels {
        3 => 3usize,
        4 if merged_transparency => 4usize,
        4 => anyhow::bail!(
            "PSD merged composite has an extra channel without an explicit merged-transparency declaration; refusing to treat a spot channel as alpha"
        ),
        count if count < 3 => anyhow::bail!(
            "RGB PSD merged composite has fewer than the three required colour channels"
        ),
        _ => anyhow::bail!(
            "PSD merged composite has unsupported extra channels; only RGB plus explicitly declared merged transparency is supported"
        ),
    };
    let pixels = usize::try_from(u64::from(width) * u64::from(height))
        .context("PSD composite dimensions exceed address space")?;
    let planes = decode_merged_data(data, width, height, channel_count)?;
    let rgba_len = pixels
        .checked_mul(4)
        .context("PSD composite RGBA size overflow")?;
    let mut rgba = Vec::with_capacity(rgba_len);
    for index in 0..pixels {
        rgba.extend_from_slice(&[
            planes[index],
            planes[pixels + index],
            planes[pixels * 2 + index],
            if channel_count == 4 {
                planes[pixels * 3 + index]
            } else {
                255
            },
        ]);
    }
    let image = RgbaImage::from_raw(width, height, rgba)
        .context("PSD composite dimensions did not match decoded pixels")?;
    let mut layer = Layer::paint("Merged composite", 1, 1);
    layer.image = Some(image.into());
    layer.metadata = json!({"psdMergedComposite": true});
    Ok(Document {
        width,
        height,
        name: name.into(),
        background: [0, 0, 0, 0],
        layers: vec![layer],
        metadata: json!({"documentID": uuid::Uuid::new_v4().to_string().to_uppercase(), "resolution": resolution, "sourceFormat": "PSD"}),
    })
}

fn decode_merged_data(data: &[u8], width: u32, height: u32, channels: usize) -> Result<Vec<u8>> {
    let pixels = usize::try_from(u64::from(width) * u64::from(height))
        .context("PSD composite dimensions exceed address space")?;
    let expected = pixels
        .checked_mul(channels)
        .context("PSD composite channel size overflow")?;
    let mut c = Cursor::new(data);
    let compression = c.u16()?;
    let payload = c.bytes(c.data.len().saturating_sub(c.pos))?;
    let decoded = match compression {
        0 => {
            ensure!(payload.len() == expected, "invalid raw PSD composite size");
            payload.to_vec()
        }
        1 => return decode_merged_rle(width, height, channels, payload),
        2 | 3 => {
            let mut decoded =
                decode_zip_exact(payload, expected, "invalid PSD ZIP composite size")?;
            if compression == 3 {
                for plane in decoded.chunks_exact_mut(pixels) {
                    unapply_prediction(plane, width as usize);
                }
            }
            decoded
        }
        _ => anyhow::bail!("unsupported PSD composite compression {compression}"),
    };
    Ok(decoded)
}

fn decode_merged_rle(width: u32, height: u32, channels: usize, data: &[u8]) -> Result<Vec<u8>> {
    let width = width as usize;
    let height = height as usize;
    let rows = height
        .checked_mul(channels)
        .context("PSD RLE row count overflow")?;
    let table_len = rows.checked_mul(2).context("PSD RLE table overflow")?;
    ensure!(
        data.len() >= table_len,
        "truncated PSD composite RLE row table"
    );
    let counts: Vec<usize> = (0..rows)
        .map(|row| usize::from(u16::from_be_bytes([data[row * 2], data[row * 2 + 1]])))
        .collect();
    let pixels = width
        .checked_mul(height)
        .context("PSD composite dimensions overflow")?;
    let mut offset = table_len;
    let total = pixels
        .checked_mul(channels)
        .context("PSD composite channel size overflow")?;
    let mut decoded = Vec::with_capacity(total);
    for channel in 0..channels {
        for row in 0..height {
            let count = counts[channel * height + row];
            let end = offset
                .checked_add(count)
                .context("PSD composite RLE overflow")?;
            ensure!(end <= data.len(), "truncated PSD composite RLE row");
            decoded.extend_from_slice(&unpack_packbits_row(&data[offset..end], width)?);
            offset = end;
        }
    }
    ensure!(
        offset == data.len(),
        "unexpected trailing PSD composite RLE data"
    );
    Ok(decoded)
}

fn unapply_prediction(data: &mut [u8], width: usize) {
    for row in data.chunks_exact_mut(width) {
        for column in 1..row.len() {
            row[column] = row[column].wrapping_add(row[column - 1]);
        }
    }
}

fn section_end(c: &mut Cursor<'_>) -> Result<usize> {
    let len = usize::try_from(c.u32()?)?;
    let end = c.pos.checked_add(len).context("PSD section overflow")?;
    ensure!(end <= c.data.len(), "truncated PSD section");
    Ok(end)
}

fn read_record(c: &mut Cursor<'_>) -> Result<RawLayer> {
    let mut l = RawLayer {
        opacity: 255,
        fill: 255,
        blend: "norm".into(),
        mask_linked: true,
        ..Default::default()
    };
    l.top = c.i32()?;
    l.left = c.i32()?;
    l.bottom = c.i32()?;
    l.right = c.i32()?;
    let channels = usize::from(c.u16()?);
    ensure!(channels <= 56, "PSD channel count exceeds limit");
    for _ in 0..channels {
        l.channels.push((c.i16()?, usize::try_from(c.u32()?)?));
    }
    ensure!(c.bytes(4)? == b"8BIM", "invalid PSD layer signature");
    l.blend = c.ascii(4)?;
    l.opacity = c.u8()?;
    l.clipping = c.u8()? != 0;
    l.hidden = c.u8()? & 2 != 0;
    c.skip(1)?;
    let extra_end = section_end(c)?;
    let mask_len = usize::try_from(c.u32()?)?;
    let mask_end = c.pos.checked_add(mask_len).context("PSD mask overflow")?;
    ensure!(mask_end <= extra_end, "truncated PSD mask record");
    if mask_len >= 20 {
        l.has_mask = true;
        l.mask_rect = [c.i32()?, c.i32()?, c.i32()?, c.i32()?];
        let _default = c.u8()?;
        let flags = c.u8()?;
        l.mask_disabled = flags & 2 != 0;
        l.mask_linked = flags & 1 == 0;
        l.mask_rendered = flags & 8 != 0;
    }
    c.set(mask_end)?;
    let ranges = usize::try_from(c.u32()?)?;
    c.skip(ranges)?;
    let n = usize::from(c.u8()?);
    l.name = latin1(c.bytes(n)?);
    c.skip((4 - ((n + 1) % 4)) % 4)?;
    while c.pos + 12 <= extra_end {
        let sig = c.ascii(4)?;
        if sig != "8BIM" && sig != "8B64" {
            break;
        }
        let key = c.ascii(4)?;
        let len = if sig == "8B64" {
            usize::try_from(c.u64()?).context("PSD descriptor exceeds address space")?
        } else {
            usize::try_from(c.u32()?)?
        };
        let payload = c.bytes(len)?.to_vec();
        if len % 2 == 1 {
            c.skip(1)?;
        }
        if key == "luni" {
            if let Some(n) = unicode_name(&payload) {
                l.name = n;
            }
        }
        if key == "iOpa" {
            if let Some(v) = payload.first() {
                l.fill = *v;
            }
        }
        if matches!(key.as_str(), "lsct" | "lsdk") && payload.len() >= 4 {
            l.section = Some(be_u32(&payload, 0));
        }
        l.extra.insert(key, payload);
    }
    c.set(extra_end)?;
    Ok(l)
}

fn decode_channels(c: &mut Cursor<'_>, l: &mut RawLayer, used: &mut u64) -> Result<()> {
    let w = positive_extent(l.left, l.right)?;
    let h = positive_extent(l.top, l.bottom)?;
    let mw = positive_extent(l.mask_rect[1], l.mask_rect[3])?;
    let mh = positive_extent(l.mask_rect[0], l.mask_rect[2])?;
    bound_surface(w, h, used)?;
    if l.has_mask {
        bound_surface(mw, mh, used)?;
    }
    let mut planes = HashMap::<i16, Vec<u8>>::new();
    for &(id, len) in &l.channels {
        let start = c.pos;
        let end = start.checked_add(len).context("PSD channel overflow")?;
        ensure!(end <= c.data.len(), "truncated PSD channel");
        if matches!(id, -2 | -1 | 0 | 1 | 2) && len >= 2 {
            let compression = c.u16()?;
            let payload = c.bytes(len - 2)?;
            let (cw, ch) = if id == -2 { (mw, mh) } else { (w, h) };
            if cw > 0 && ch > 0 {
                planes.insert(id, decode_plane(compression, cw, ch, payload)?);
            }
        }
        c.set(end)?;
    }
    if l.has_mask && !l.mask_rendered && mw > 0 && mh > 0 {
        if let Some(gray) = planes.remove(&-2) {
            l.mask = Some(RgbaImage::from_fn(mw, mh, |x, y| {
                let v = gray[y as usize * mw as usize + x as usize];
                Rgba([v, v, v, 255])
            }));
        }
    }
    if w > 0 && h > 0 {
        let n = w as usize * h as usize;
        let black = vec![0; n];
        let opaque = vec![255; n];
        let r = planes.get(&0).unwrap_or(&black);
        let g = planes.get(&1).unwrap_or(&black);
        let b = planes.get(&2).unwrap_or(&black);
        let a = planes.get(&-1).unwrap_or(&opaque);
        l.image = Some(RgbaImage::from_fn(w, h, |x, y| {
            let i = y as usize * w as usize + x as usize;
            Rgba([r[i], g[i], b[i], a[i]])
        }));
    }
    Ok(())
}

fn bound_surface(w: u32, h: u32, used: &mut u64) -> Result<()> {
    if w == 0 || h == 0 {
        return Ok(());
    }
    ensure!(valid_dimensions(w, h), "PSD layer dimensions exceed limits");
    let n = u64::from(w) * u64::from(h);
    ensure!(
        n <= MAX_PIXELS.saturating_sub(*used),
        "PSD decoded pixel budget exceeded"
    );
    *used += n;
    Ok(())
}

fn decode_plane(compression: u16, w: u32, h: u32, data: &[u8]) -> Result<Vec<u8>> {
    let expected = w as usize * h as usize;
    match compression {
        0 => {
            ensure!(data.len() >= expected, "truncated raw PSD channel");
            Ok(data[..expected].to_vec())
        }
        1 => unpack_packbits(w as usize, h as usize, data),
        2 | 3 => {
            let mut decoded = decode_zip_exact(data, expected, "invalid PSD ZIP channel size")?;
            if compression == 3 {
                unapply_prediction(&mut decoded, w as usize);
            }
            Ok(decoded)
        }
        _ => anyhow::bail!("unsupported PSD channel compression {compression}"),
    }
}

fn decode_zip_exact(data: &[u8], expected: usize, error: &'static str) -> Result<Vec<u8>> {
    let mut decoded = Vec::with_capacity(expected);
    // The declared geometry bounds the useful image data. Reading a ZIP stream
    // to EOF before comparing its length lets a tiny PSD expand into unbounded
    // memory before it is rejected.
    let limit = u64::try_from(expected)
        .context("PSD decoded channel size exceeds address space")?
        .checked_add(1)
        .context("PSD decoded channel size overflow")?;
    flate2::read::ZlibDecoder::new(data)
        .take(limit)
        .read_to_end(&mut decoded)?;
    ensure!(decoded.len() == expected, "{error}");
    Ok(decoded)
}

fn unpack_packbits(w: usize, h: usize, data: &[u8]) -> Result<Vec<u8>> {
    ensure!(data.len() >= h * 2, "truncated PSD RLE row table");
    let mut offset = h * 2;
    let counts: Vec<usize> = (0..h)
        .map(|y| usize::from(u16::from_be_bytes([data[y * 2], data[y * 2 + 1]])))
        .collect();
    let mut out = vec![0; w * h];
    for (row, bytes) in counts.into_iter().enumerate() {
        let end = offset.checked_add(bytes).context("PSD RLE overflow")?;
        ensure!(end <= data.len(), "truncated PSD RLE row");
        out[row * w..(row + 1) * w].copy_from_slice(&unpack_packbits_row(&data[offset..end], w)?);
        offset = end;
    }
    Ok(out)
}

fn unpack_packbits_row(data: &[u8], width: usize) -> Result<Vec<u8>> {
    let mut offset = 0;
    let mut out = Vec::with_capacity(width);
    while out.len() < width {
        ensure!(offset < data.len(), "short PSD RLE row");
        let n = data[offset] as i8;
        offset += 1;
        if n >= 0 {
            let count = n as usize + 1;
            ensure!(
                out.len() + count <= width && offset + count <= data.len(),
                "invalid PSD RLE literal"
            );
            out.extend_from_slice(&data[offset..offset + count]);
            offset += count;
        } else if n != -128 {
            let count = (1i16 - i16::from(n)) as usize;
            ensure!(
                out.len() + count <= width && offset < data.len(),
                "invalid PSD RLE repeat"
            );
            out.resize(out.len() + count, data[offset]);
            offset += 1;
        }
    }
    Ok(out)
}

fn assemble(raw: Vec<RawLayer>, width: u32, height: u32) -> Result<Vec<Record>> {
    let mut records = vec![];
    let mut groups: Vec<String> = vec![];
    for l in raw {
        if l.section == Some(3) {
            groups.push(new_id());
            continue;
        }
        let is_group = matches!(l.section, Some(1 | 2));
        let id = if is_group {
            groups.pop().unwrap_or_else(new_id)
        } else {
            new_id()
        };
        let mut conversions = conversion_notes(&l, is_group)?;
        let mut layer = if is_group {
            Layer::group(nonempty_name(&l.name))
        } else {
            let mut x = Layer::group(nonempty_name(&l.name));
            x.metadata = json!({});
            x.image = l.image.map(Into::into);
            x
        };
        layer.id = id;
        layer.visible = !l.hidden;
        let has_effects = ["lfx2", "lrFX", "lmfx"]
            .iter()
            .any(|key| l.extra.contains_key(*key));
        layer.opacity = if has_effects && l.fill != 255 {
            f32::from(l.opacity) / 255.
        } else {
            f32::from(l.opacity) / 255. * f32::from(l.fill) / 255.
        };
        layer.blend_mode = blend(&l.blend)?.into();
        layer.offset_x = if is_group { 0. } else { l.left as f32 };
        layer.offset_y = if is_group { 0. } else { l.top as f32 };
        if is_group {
            layer.metadata = json!({"isGroup":true});
        }
        if !is_group {
            if let Some((image, left, top, shape)) = live_vector(&l.extra, width, height)? {
                layer.image = Some(image.into());
                layer.offset_x = left;
                layer.offset_y = top;
                layer.metadata["shape"] = serde_json::to_value(shape)?;
                conversions.retain(|note| !note.contains("vector layer"));
            }
        }
        if let Some(adjustment) = adjustment(&l.extra)? {
            layer.image = None;
            layer.offset_x = 0.;
            layer.offset_y = 0.;
            layer.metadata["adjustment"] = adjustment;
            layer.metadata["psdCanvasSize"] = json!([width, height]);
        }
        if let Some(mask) = l.mask {
            layer.mask = Some(mask.into());
            layer.metadata["maskEnabled"] = json!(!l.mask_disabled);
            layer.metadata["maskLinked"] = json!(l.mask_linked);
            let [top, left, bottom, right] = l.mask_rect;
            layer.metadata["maskPlacement"] = json!({"origin":[left,top], "size":[right-left,bottom-top], "rotation":0, "flipX":false, "flipY":false, "sampling":"Nearest"});
        }
        layer.metadata["psdClipping"] = json!(l.clipping);
        if !conversions.is_empty() {
            layer.metadata["psdConversions"] = json!(conversions);
        }
        records.push(Record {
            parent: groups.last().cloned(),
            layer,
        });
    }
    ensure!(groups.is_empty(), "unbalanced PSD group section records");
    let mut base_for_parent: HashMap<Option<String>, Option<String>> = HashMap::new();
    for record in &mut records {
        let parent = record.parent.clone();
        if record.layer.metadata["psdClipping"].as_bool() == Some(true) {
            let source = base_for_parent
                .get(&parent)
                .and_then(Clone::clone)
                .with_context(|| {
                    format!(
                        "unsupported clipping-mask base for PSD layer {}",
                        record.layer.name
                    )
                })?;
            record.layer.metadata["maskSourceID"] = json!(source);
        } else if !record.layer.is_group()
            && record.layer.image.is_some()
            && record.layer.metadata.get("adjustment").is_none()
        {
            base_for_parent.insert(parent, Some(record.layer.id.clone()));
        } else {
            base_for_parent.insert(parent, None);
        }
    }
    Ok(records)
}

fn conversion_notes(l: &RawLayer, is_group: bool) -> Result<Vec<&'static str>> {
    if is_group {
        return Ok(vec![]);
    }
    let mut notes = vec![];
    for (keys, label) in [
        (
            &["TySh", "tySh", "txt2"][..],
            "Editable Photoshop text was imported as cached pixels.",
        ),
        (
            &["SoLd", "SoLE"][..],
            "The Photoshop smart object was imported as cached pixels.",
        ),
        (
            &["lfx2", "lrFX", "lmfx"][..],
            "Photoshop layer effects were discarded; cached pixels were preserved.",
        ),
        (
            &["vmsk", "vsms", "vogk"][..],
            "The Photoshop vector layer was imported as cached pixels.",
        ),
    ] {
        if keys.iter().any(|k| l.extra.contains_key(*k)) {
            ensure!(
                l.image.is_some(),
                "unsupported Photoshop descriptor without cached pixels on layer {}",
                nonempty_name(&l.name)
            );
            notes.push(label);
        }
    }
    let known = ["levl", "curv", "hue2", "hue "];
    let adjustment_keys = [
        "expA", "grdm", "brit", "blnc", "nvrt", "thrs", "post", "mixr", "selc", "blwh", "phfl",
        "vibA",
    ];
    if let Some(key) = adjustment_keys.iter().find(|k| l.extra.contains_key(**k)) {
        anyhow::bail!(
            "unsupported PSD adjustment descriptor {key} on layer {}",
            nonempty_name(&l.name)
        );
    }
    let _ = known;
    Ok(notes)
}

fn live_vector(
    extra: &HashMap<String, Vec<u8>>,
    _canvas_width: u32,
    _canvas_height: u32,
) -> Result<Option<(RgbaImage, f32, f32, crate::objects::LiveShapeStyle)>> {
    let Some(origin) = extra.get("vogk") else {
        return Ok(None);
    };
    if let Some(stroke) = extra.get("vstk") {
        if descriptor_bool(stroke, b"fillEnabled") != Some(true)
            || descriptor_bool(stroke, b"strokeEnabled") != Some(false)
        {
            return Ok(None);
        }
    }
    let Some(fill) = extra.get("SoCo").and_then(|data| descriptor_rgb(data)) else {
        return Ok(None);
    };
    let Some(kind) = descriptor_i32(origin, "keyOriginType") else {
        return Ok(None);
    };
    let kind = match kind {
        1 => crate::objects::LiveShapeKind::Rectangle,
        5 => crate::objects::LiveShapeKind::Ellipse,
        _ => return Ok(None),
    };
    let Some(from) = find_bytes(origin, b"keyOriginShapeBBox", 0) else {
        return Ok(None);
    };
    let Some(left) = descriptor_unit(origin, b"Left", from) else {
        return Ok(None);
    };
    let Some(top) = descriptor_unit(origin, b"Top ", from) else {
        return Ok(None);
    };
    let Some(right) = descriptor_unit(origin, b"Rght", from) else {
        return Ok(None);
    };
    let Some(bottom) = descriptor_unit(origin, b"Btom", from) else {
        return Ok(None);
    };
    let (w, h) = (right - left, bottom - top);
    ensure!(
        left.is_finite() && top.is_finite() && w >= 1. && h >= 1. && w <= 30_000. && h <= 30_000.,
        "invalid PSD vector bounds"
    );
    let (width, height) = (w.ceil() as u32, h.ceil() as u32);
    ensure!(
        valid_dimensions(width, height),
        "PSD vector exceeds pixel bounds"
    );
    let geometry = match kind {
        crate::objects::LiveShapeKind::Rectangle => crate::objects::Shape::Rectangle {
            x: 0.,
            y: 0.,
            width: width as f32,
            height: height as f32,
        },
        crate::objects::LiveShapeKind::Ellipse => crate::objects::Shape::Ellipse {
            x: 0.,
            y: 0.,
            width: width as f32,
            height: height as f32,
        },
        crate::objects::LiveShapeKind::Line => unreachable!(),
    };
    let rgba = [
        (fill[0] * 255.).round() as u8,
        (fill[1] * 255.).round() as u8,
        (fill[2] * 255.).round() as u8,
        255,
    ];
    let made = crate::objects::shape_layer("PSD vector", width, height, geometry, rgba, None)?;
    let image = made.image.context("PSD vector rasterization failed")?;
    let shape = crate::objects::LiveShapeStyle {
        kind,
        red: fill[0],
        green: fill[1],
        blue: fill[2],
        corner_radius: 0.,
        line_width: None,
        start: None,
        end: None,
    };
    Ok(Some((image.into_image(), left as f32, top as f32, shape)))
}

fn descriptor_rgb(data: &[u8]) -> Option<[f32; 3]> {
    let channel = |key| {
        descriptor_double(data, key).map(|v| if v > 1. { (v / 255.).clamp(0., 1.) } else { v.clamp(0., 1.) } as f32)
    };
    Some([channel(b"Rd  ")?, channel(b"Grn ")?, channel(b"Bl  ")?])
}
fn descriptor_i32(data: &[u8], key: &str) -> Option<i32> {
    let at = find_bytes(data, key.as_bytes(), 0)? + key.len();
    (at + 8 <= data.len() && &data[at..at + 4] == b"long")
        .then(|| i32::from_be_bytes(data[at + 4..at + 8].try_into().unwrap()))
}
fn descriptor_unit(data: &[u8], key: &[u8], from: usize) -> Option<f64> {
    let key_at = find_bytes(data, key, from)?;
    let unit = find_bytes(data, b"UntF", key_at)?;
    if unit > key_at + key.len() + 32 {
        return None;
    }
    descriptor_f64(data, unit + 8)
}
fn descriptor_bool(data: &[u8], key: &[u8]) -> Option<bool> {
    let at = find_bytes(data, key, 0)? + key.len();
    (at + 5 <= data.len() && &data[at..at + 4] == b"bool").then(|| data[at + 4] != 0)
}
fn descriptor_double(data: &[u8], key: &[u8]) -> Option<f64> {
    let at = find_bytes(data, key, 0)? + key.len();
    (at + 12 <= data.len() && &data[at..at + 4] == b"doub")
        .then(|| descriptor_f64(data, at + 4))
        .flatten()
}
fn descriptor_f64(data: &[u8], at: usize) -> Option<f64> {
    (at + 8 <= data.len())
        .then(|| f64::from_bits(u64::from_be_bytes(data[at..at + 8].try_into().unwrap())))
}
fn find_bytes(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?
        .windows(needle.len())
        .position(|x| x == needle)
        .map(|x| x + from)
}

fn adjustment(extra: &HashMap<String, Vec<u8>>) -> Result<Option<Value>> {
    if let Some(d) = extra.get("levl") {
        return Ok(Some(levels(d)?));
    }
    if let Some(d) = extra.get("curv") {
        return Ok(Some(curves(d)?));
    }
    if let Some(d) = extra.get("hue2").or_else(|| extra.get("hue ")) {
        return Ok(Some(hue(d)?));
    }
    Ok(None)
}

fn base_adjustment() -> Value {
    crate::effects::adjustment_for_filter(&crate::filters::Filter::Invert)
        .expect("static adjustment defaults")
}
fn levels(d: &[u8]) -> Result<Value> {
    ensure!(d.len() >= 42, "truncated PSD levels descriptor");
    let mut v = base_adjustment();
    v["kind"] = json!("Levels");
    let mut ranges = vec![];
    for channel in 0..4 {
        let b = 2 + channel * 10;
        ranges.push(json!({"black":be_u16(d,b),"white":be_u16(d,b+2),"outputBlack":be_u16(d,b+4),"outputWhite":be_u16(d,b+6),"gamma":f64::from(be_u16(d,b+8))/256.}));
    }
    v["levels"] = json!({"channel":"RGB","ranges":ranges});
    Ok(v)
}
fn curves(d: &[u8]) -> Result<Value> {
    let mut o = 0;
    if d.first() == Some(&0) {
        o += 1;
    }
    ensure!(o + 4 <= d.len(), "truncated PSD curves descriptor");
    let version = be_u16(d, o);
    o += 2;
    ensure!(matches!(version, 1 | 4), "unsupported PSD curves version");
    let count = usize::from(be_u16(d, o));
    o += 2;
    ensure!(count <= 4, "unsupported PSD curve channel count");
    let line = json!([{"x":0,"y":0},{"x":255,"y":255}]);
    let mut channels = vec![line.clone(), line.clone(), line.clone(), line];
    for channel in 0..count {
        ensure!(o + 2 <= d.len(), "truncated PSD curve");
        let n = usize::from(be_u16(d, o));
        o += 2;
        ensure!((2..=256).contains(&n), "invalid PSD curve points");
        let mut points = vec![];
        for _ in 0..n {
            ensure!(o + 4 <= d.len(), "truncated PSD curve point");
            points.push(json!({"x":be_u16(d,o+2).min(255),"y":be_u16(d,o).min(255)}));
            o += 4;
        }
        channels[channel] = Value::Array(points);
    }
    let mut v = base_adjustment();
    v["kind"] = json!("Curves");
    v["curves"] = json!({"channel":"RGB","channels":channels});
    Ok(v)
}
fn hue(d: &[u8]) -> Result<Value> {
    ensure!(d.len() >= 10, "truncated PSD hue/saturation descriptor");
    let mut v = base_adjustment();
    v["kind"] = json!("Hue/Saturation");
    v["colorize"] = json!(d[2] != 0);
    v["hue"] = json!(i16::from_be_bytes([d[4], d[5]]));
    v["saturation"] = json!(i16::from_be_bytes([d[6], d[7]]));
    v["lightness"] = json!(i16::from_be_bytes([d[8], d[9]]));
    Ok(v)
}

fn nest(mut records: Vec<Record>, parent: Option<&str>) -> Result<Vec<Layer>> {
    ensure!(parent.is_none(), "PSD hierarchy root must be empty");
    let mut buckets: HashMap<Option<String>, Vec<Layer>> = HashMap::new();
    for record in records.drain(..) {
        buckets.entry(record.parent).or_default().push(record.layer);
    }
    fn take(
        parent: Option<String>,
        buckets: &mut HashMap<Option<String>, Vec<Layer>>,
    ) -> Vec<Layer> {
        let mut layers = buckets.remove(&parent).unwrap_or_default();
        for layer in &mut layers {
            layer.children = take(Some(layer.id.clone()), buckets);
        }
        layers
    }
    let result = take(None, &mut buckets);
    ensure!(buckets.is_empty(), "PSD hierarchy contains orphaned layers");
    Ok(result)
}
fn blend(k: &str) -> Result<&'static str> {
    Ok(match k {
        "norm" | "pass" => "Normal",
        "mul " => "Multiply",
        "scrn" => "Screen",
        "over" => "Overlay",
        "sLit" => "Soft Light",
        "dark" => "Darken",
        "lite" => "Lighten",
        "diff" => "Difference",
        "div " => "Color Dodge",
        "idiv" => "Color Burn",
        "hue " => "Hue",
        "sat " => "Saturation",
        "colr" => "Color",
        "lum " => "Luminosity",
        "lbrn" => "Linear Burn",
        "lddg" => "Linear Dodge (Add)",
        "hLit" => "Hard Light",
        "vLit" => "Vivid Light",
        "lLit" => "Linear Light",
        "pLit" => "Pin Light",
        "hMix" => "Hard Mix",
        "smud" => "Exclusion",
        "fsub" => "Subtract",
        "fdiv" => "Divide",
        _ => anyhow::bail!("unsupported PSD blend mode {k:?}"),
    })
}
fn positive_extent(a: i32, b: i32) -> Result<u32> {
    if b <= a {
        Ok(0)
    } else {
        u32::try_from(b - a).context("PSD extent overflow")
    }
}
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}
fn nonempty_name(s: &str) -> &str {
    if s.is_empty() { "Layer" } else { s }
}
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&x| char::from(x)).collect()
}
fn be_u16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn be_u32(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(d[o..o + 4].try_into().unwrap())
}
fn unicode_name(d: &[u8]) -> Option<String> {
    if d.len() < 4 {
        return None;
    }
    let n = be_u32(d, 0) as usize;
    if n == 0 || 4 + n * 2 > d.len() {
        return None;
    }
    let u: Vec<u16> = (0..n).map(|i| be_u16(d, 4 + i * 2)).collect();
    Some(String::from_utf16_lossy(&u).trim_matches('\0').into())
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn need(&self, n: usize) -> Result<()> {
        ensure!(
            n <= self.data.len().saturating_sub(self.pos),
            "truncated PSD"
        );
        Ok(())
    }
    fn set(&mut self, p: usize) -> Result<()> {
        ensure!(p <= self.data.len(), "truncated PSD");
        self.pos = p;
        Ok(())
    }
    fn skip(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    fn ascii(&mut self, n: usize) -> Result<String> {
        Ok(std::str::from_utf8(self.bytes(n)?)
            .context("invalid PSD ASCII")?
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packbits_and_bounds() {
        assert_eq!(unpack_packbits(4, 1, &[0, 2, 253, 7]).unwrap(), vec![7; 4]);
        assert!(decode_plane(2, 1, 1, &[]).is_err());
    }
    #[test]
    fn truncated_and_psb_are_rejected() {
        assert!(parse(b"8BPS", "x").is_err());
        let mut h = Vec::from(&b"8BPS"[..]);
        h.extend_from_slice(&2u16.to_be_bytes());
        h.extend_from_slice(&[0; 6]);
        assert!(parse(&h, "x").unwrap_err().to_string().contains("PSB"));
    }

    fn flattened_psd(
        width: u32,
        height: u32,
        channels: u16,
        layer_section: &[u8],
        compression: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        flattened_psd_with_resources(
            width,
            height,
            channels,
            &[],
            layer_section,
            compression,
            payload,
        )
    }

    fn flattened_psd_with_resources(
        width: u32,
        height: u32,
        channels: u16,
        resources: &[u8],
        layer_section: &[u8],
        compression: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut psd = Vec::from(&b"8BPS"[..]);
        psd.extend_from_slice(&1u16.to_be_bytes());
        psd.extend_from_slice(&[0; 6]);
        psd.extend_from_slice(&channels.to_be_bytes());
        psd.extend_from_slice(&height.to_be_bytes());
        psd.extend_from_slice(&width.to_be_bytes());
        psd.extend_from_slice(&8u16.to_be_bytes());
        psd.extend_from_slice(&3u16.to_be_bytes());
        psd.extend_from_slice(&0u32.to_be_bytes()); // colour mode data
        psd.extend_from_slice(&(resources.len() as u32).to_be_bytes());
        psd.extend_from_slice(resources);
        psd.extend_from_slice(&(layer_section.len() as u32).to_be_bytes());
        psd.extend_from_slice(layer_section);
        psd.extend_from_slice(&compression.to_be_bytes());
        psd.extend_from_slice(payload);
        psd
    }

    fn image_resource(id: u16, name: &[u8], payload: &[u8]) -> Vec<u8> {
        assert!(name.len() <= u8::MAX as usize);
        let mut resource = Vec::from(&b"8BIM"[..]);
        resource.extend_from_slice(&id.to_be_bytes());
        resource.push(name.len() as u8);
        resource.extend_from_slice(name);
        if (name.len() + 1) % 2 == 1 {
            resource.push(0);
        }
        resource.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        resource.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            resource.push(0);
        }
        resource
    }

    fn planar(pixels: &[[u8; 4]], channels: usize) -> Vec<u8> {
        (0..channels)
            .flat_map(|channel| pixels.iter().map(move |pixel| pixel[channel]))
            .collect()
    }

    fn rle_literal_planar(width: usize, height: usize, channels: usize, data: &[u8]) -> Vec<u8> {
        let mut rows = Vec::with_capacity(channels * height);
        for plane in data.chunks_exact(width * height) {
            for row in plane.chunks_exact(width) {
                assert!(!row.is_empty() && row.len() <= 128);
                let mut encoded = vec![(row.len() - 1) as u8];
                encoded.extend_from_slice(row);
                rows.push(encoded);
            }
        }
        assert_eq!(rows.len(), channels * height);
        let mut encoded = Vec::new();
        for row in &rows {
            encoded.extend_from_slice(&(row.len() as u16).to_be_bytes());
        }
        for row in rows {
            encoded.extend_from_slice(&row);
        }
        encoded
    }

    fn predicted_planar(width: usize, height: usize, data: &[u8]) -> Vec<u8> {
        let mut predicted = data.to_vec();
        for plane in predicted.chunks_exact_mut(width * height) {
            for row in plane.chunks_exact_mut(width) {
                for column in (1..row.len()).rev() {
                    row[column] = row[column].wrapping_sub(row[column - 1]);
                }
            }
        }
        predicted
    }

    fn zip(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn assert_flattened_pixels(psd: &[u8], expected: &[[u8; 4]]) {
        let document = parse(psd, "Flattened").unwrap();
        assert_eq!(document.layers.len(), 1);
        assert_eq!(document.layers[0].name, "Merged composite");
        assert_eq!(
            document.layers[0].metadata["psdMergedComposite"],
            serde_json::json!(true)
        );
        let image = document.layers[0].image.as_ref().unwrap();
        for (actual, expected) in image.pixels().zip(expected) {
            assert_eq!(actual.0, *expected);
        }
    }

    struct SyntheticLayer<'a> {
        name: &'a str,
        blend: &'a str,
        opacity: u8,
        clipping: bool,
        section: Option<u32>,
        pixels: Option<Vec<[u8; 4]>>,
        mask: Option<Vec<u8>>,
    }

    fn synthetic_layered_psd(width: u32, height: u32, layers: &[SyntheticLayer<'_>]) -> Vec<u8> {
        let pixel_count = width as usize * height as usize;
        let mut records = Vec::new();
        let mut channel_data = Vec::new();
        for layer in layers {
            assert_eq!(layer.blend.len(), 4);
            let raster = layer.pixels.as_ref();
            if let Some(pixels) = raster {
                assert_eq!(pixels.len(), pixel_count);
            }
            if let Some(mask) = &layer.mask {
                assert_eq!(mask.len(), pixel_count);
                assert!(raster.is_some(), "only raster layers may carry a mask");
            }
            let (bottom, right) = if raster.is_some() {
                (height as i32, width as i32)
            } else {
                (0, 0)
            };
            for value in [0i32, 0, bottom, right] {
                records.extend_from_slice(&value.to_be_bytes());
            }
            let channel_count = if raster.is_some() {
                4 + usize::from(layer.mask.is_some())
            } else {
                0
            };
            records.extend_from_slice(&(channel_count as u16).to_be_bytes());
            if let Some(pixels) = raster {
                for channel in [0i16, 1, 2, -1] {
                    records.extend_from_slice(&channel.to_be_bytes());
                    records.extend_from_slice(&((pixel_count + 2) as u32).to_be_bytes());
                    channel_data.extend_from_slice(&0u16.to_be_bytes());
                    channel_data.extend(pixels.iter().map(|pixel| {
                        if channel == -1 {
                            pixel[3]
                        } else {
                            pixel[channel as usize]
                        }
                    }));
                }
                if let Some(mask) = &layer.mask {
                    records.extend_from_slice(&(-2i16).to_be_bytes());
                    records.extend_from_slice(&((pixel_count + 2) as u32).to_be_bytes());
                    channel_data.extend_from_slice(&0u16.to_be_bytes());
                    channel_data.extend_from_slice(mask);
                }
            }
            records.extend_from_slice(b"8BIM");
            records.extend_from_slice(layer.blend.as_bytes());
            records.push(layer.opacity);
            records.push(u8::from(layer.clipping));
            records.extend_from_slice(&[0, 0]); // visible flags and filler

            let mut extra = Vec::new();
            if layer.mask.is_some() {
                extra.extend_from_slice(&20u32.to_be_bytes());
                for value in [0i32, 0, height as i32, width as i32] {
                    extra.extend_from_slice(&value.to_be_bytes());
                }
                extra.extend_from_slice(&[0, 0, 0, 0]); // default, flags and padding
            } else {
                extra.extend_from_slice(&0u32.to_be_bytes());
            }
            extra.extend_from_slice(&0u32.to_be_bytes()); // blending ranges
            assert!(layer.name.len() <= u8::MAX as usize);
            extra.push(layer.name.len() as u8);
            extra.extend_from_slice(layer.name.as_bytes());
            while extra.len() % 4 != 0 {
                extra.push(0);
            }
            if let Some(section) = layer.section {
                extra.extend_from_slice(b"8BIM");
                extra.extend_from_slice(b"lsct");
                extra.extend_from_slice(&4u32.to_be_bytes());
                extra.extend_from_slice(&section.to_be_bytes());
            }
            records.extend_from_slice(&(extra.len() as u32).to_be_bytes());
            records.extend_from_slice(&extra);
        }

        let mut layer_info = Vec::new();
        layer_info.extend_from_slice(&(layers.len() as i16).to_be_bytes());
        layer_info.extend_from_slice(&records);
        layer_info.extend_from_slice(&channel_data);
        let mut layer_section = Vec::new();
        layer_section.extend_from_slice(&(layer_info.len() as u32).to_be_bytes());
        layer_section.extend_from_slice(&layer_info);
        layer_section.extend_from_slice(&0u32.to_be_bytes()); // global layer mask
        flattened_psd(width, height, 4, &layer_section, 0, &[])
    }

    #[test]
    fn embedded_icc_psd_is_rejected_before_pixel_decode() {
        let profile = image_resource(1039, b"", &[0x01, 0x23, 0x45]);
        let error = parse(
            &flattened_psd_with_resources(1, 1, 3, &profile, &[], 0, &[10, 20, 30]),
            "Tagged",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("embedded ICC profile"), "{error}");
        assert!(error.contains("PNG/TIFF"), "{error}");
    }

    #[test]
    fn image_resource_payload_and_padding_stay_inside_the_declared_section() {
        let mut truncated_payload = Vec::from(&b"8BIM"[..]);
        truncated_payload.extend_from_slice(&1039u16.to_be_bytes());
        truncated_payload.push(0); // empty Pascal name
        truncated_payload.push(0); // Pascal name padding
        truncated_payload.extend_from_slice(&4u32.to_be_bytes());
        truncated_payload.push(0); // only one of four declared payload bytes
        let error = parse(
            &flattened_psd_with_resources(1, 1, 3, &truncated_payload, &[], 0, &[10, 20, 30]),
            "Truncated profile",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("resource payload"), "{error}");

        let mut missing_padding = image_resource(1005, b"", &[0]);
        missing_padding.pop();
        let error = parse(
            &flattened_psd_with_resources(1, 1, 3, &missing_padding, &[], 0, &[10, 20, 30]),
            "Missing padding",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("resource padding"), "{error}");
    }

    #[test]
    fn flattened_rgb_psd_preserves_exact_pixels_for_raw_rle_zip_and_prediction() {
        let pixels = [
            [10, 20, 30, 255],
            [40, 50, 60, 255],
            [70, 80, 90, 255],
            [100, 110, 120, 255],
        ];
        let planes = planar(&pixels, 3);
        assert_flattened_pixels(&flattened_psd(2, 2, 3, &[], 0, &planes), &pixels);
        let mut explicit_empty_layers = Vec::new();
        explicit_empty_layers.extend_from_slice(&2u32.to_be_bytes());
        explicit_empty_layers.extend_from_slice(&0u16.to_be_bytes());
        explicit_empty_layers.extend_from_slice(&0u32.to_be_bytes()); // no global mask
        assert_flattened_pixels(
            &flattened_psd(2, 2, 3, &explicit_empty_layers, 0, &planes),
            &pixels,
        );
        assert_flattened_pixels(
            &flattened_psd(2, 2, 3, &[], 1, &rle_literal_planar(2, 2, 3, &planes)),
            &pixels,
        );
        assert_flattened_pixels(&flattened_psd(2, 2, 3, &[], 2, &zip(&planes)), &pixels);
        assert_flattened_pixels(
            &flattened_psd(2, 2, 3, &[], 3, &zip(&predicted_planar(2, 2, &planes))),
            &pixels,
        );
    }

    #[test]
    fn flattened_alpha_requires_an_explicit_merged_transparency_marker() {
        let pixels = [
            [10, 20, 30, 0],
            [40, 50, 60, 64],
            [70, 80, 90, 128],
            [100, 110, 120, 255],
        ];
        let planes = planar(&pixels, 4);
        let mut layer_section = Vec::new();
        layer_section.extend_from_slice(&0u32.to_be_bytes()); // no layer-info records
        layer_section.extend_from_slice(&0u32.to_be_bytes()); // no global mask
        layer_section.extend_from_slice(b"8BIM");
        layer_section.extend_from_slice(b"Mtrn");
        layer_section.extend_from_slice(&0u32.to_be_bytes());
        assert_flattened_pixels(&flattened_psd(2, 2, 4, &layer_section, 0, &planes), &pixels);
        let error = parse(&flattened_psd(2, 2, 4, &[], 0, &planes), "Spot")
            .unwrap_err()
            .to_string();
        assert!(error.contains("refusing to treat a spot channel as alpha"));
    }

    #[test]
    fn flattened_psd_composite_bounds_and_layer_info_are_checked() {
        let error = parse(
            &flattened_psd(1, 1, 3, &[], 2, &zip(&vec![0; 4096])),
            "Oversized ZIP",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid PSD ZIP composite size"));

        // Layer records and their channel data are confined to the declared
        // layer-info section. They must not consume the following global-mask
        // or merged-image bytes when a corrupt length claims more data.
        let mut crossing_info = Vec::new();
        crossing_info.extend_from_slice(&8u32.to_be_bytes());
        crossing_info.extend_from_slice(&0u16.to_be_bytes());
        let error = parse(
            &flattened_psd(1, 1, 3, &crossing_info, 0, &[1, 2, 3]),
            "Crossing info",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("truncated PSD layer information"));
    }

    #[test]
    fn generated_layered_psd_preserves_group_opacity_mask_and_clipping_stack() {
        let transparent = [0, 0, 0, 0];
        let psd = synthetic_layered_psd(
            4,
            1,
            &[
                SyntheticLayer {
                    name: "Group divider",
                    blend: "pass",
                    opacity: 255,
                    clipping: false,
                    section: Some(3),
                    pixels: None,
                    mask: None,
                },
                SyntheticLayer {
                    name: "Inside",
                    blend: "norm",
                    opacity: 255,
                    clipping: false,
                    section: None,
                    pixels: Some(vec![
                        [0, 255, 0, 255],
                        transparent,
                        transparent,
                        transparent,
                    ]),
                    mask: None,
                },
                SyntheticLayer {
                    name: "Half opacity group",
                    blend: "pass",
                    opacity: 128,
                    clipping: false,
                    section: Some(1),
                    pixels: None,
                    mask: None,
                },
                SyntheticLayer {
                    name: "Masked base",
                    blend: "norm",
                    opacity: 255,
                    clipping: false,
                    section: None,
                    pixels: Some(vec![
                        transparent,
                        transparent,
                        [255, 0, 0, 255],
                        [255, 0, 0, 255],
                    ]),
                    mask: Some(vec![0, 0, 255, 0]),
                },
                SyntheticLayer {
                    name: "Clipped blue",
                    blend: "norm",
                    opacity: 255,
                    clipping: true,
                    section: None,
                    pixels: Some(vec![
                        transparent,
                        transparent,
                        [0, 0, 255, 255],
                        [0, 0, 255, 255],
                    ]),
                    mask: None,
                },
            ],
        );
        let document = parse(&psd, "Generated layers").unwrap();
        assert!(crate::raster::validate(&document).is_empty());
        assert_eq!(document.layers.len(), 3);
        let group = &document.layers[0];
        assert!(group.is_group());
        assert_eq!(group.name, "Half opacity group");
        assert_eq!(group.opacity, 128.0 / 255.0);
        assert_eq!(group.children.len(), 1);
        assert_eq!(group.children[0].name, "Inside");
        let base = &document.layers[1];
        assert!(base.mask.is_some());
        assert_eq!(base.mask.as_ref().unwrap().get_pixel(2, 0)[0], 255);
        assert_eq!(base.mask.as_ref().unwrap().get_pixel(3, 0)[0], 0);
        let clipped = &document.layers[2];
        assert_eq!(
            clipped.metadata["maskSourceID"].as_str(),
            Some(base.id.as_str())
        );

        let composite = crate::raster::composite(&document);
        assert_eq!(composite.get_pixel(0, 0).0, [0, 255, 0, 128]);
        assert_eq!(composite.get_pixel(1, 0).0, [0, 0, 0, 0]);
        assert_eq!(composite.get_pixel(2, 0).0, [0, 0, 255, 255]);
        assert_eq!(composite.get_pixel(3, 0).0, [0, 0, 0, 0]);
    }

    #[test]
    fn generated_layered_psd_blend_keys_have_independent_expected_pixels() {
        let base = [128, 128, 64, 255];
        let source = [64, 128, 255, 255];
        for (key, name, expected) in [
            ("vLit", "Vivid Light", [2, 129, 255, 255]),
            ("lLit", "Linear Light", [1, 129, 255, 255]),
            ("pLit", "Pin Light", [128, 128, 255, 255]),
            ("hMix", "Hard Mix", [0, 255, 255, 255]),
        ] {
            let psd = synthetic_layered_psd(
                1,
                1,
                &[
                    SyntheticLayer {
                        name: "Backdrop",
                        blend: "norm",
                        opacity: 255,
                        clipping: false,
                        section: None,
                        pixels: Some(vec![base]),
                        mask: None,
                    },
                    SyntheticLayer {
                        name: "Blend",
                        blend: key,
                        opacity: 255,
                        clipping: false,
                        section: None,
                        pixels: Some(vec![source]),
                        mask: None,
                    },
                ],
            );
            let document = parse(&psd, name).unwrap();
            assert_eq!(document.layers[1].blend_mode, name);
            assert_eq!(
                crate::raster::composite(&document).get_pixel(0, 0).0,
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn external_layered_fixture_when_configured() {
        let Some(path) = crate::identity::env_var_os("OMUSE_PSD_FIXTURE") else {
            return;
        };
        let document = open(Path::new(&path)).unwrap();
        assert!(document.width > 0 && document.height > 0);
        assert!(
            document.layers.len() >= 2,
            "fixture did not preserve layers"
        );
        assert!(document.layers.iter().all(|layer| layer.image.is_some()));
        if let Some(reference) = crate::identity::env_var_os("OMUSE_PSD_REFERENCE") {
            let expected = image::ImageReader::open(reference)
                .unwrap()
                .decode()
                .unwrap()
                .to_rgba8();
            let actual = crate::raster::composite(&document);
            assert_eq!(actual.dimensions(), expected.dimensions());
            let mut mismatches = 0usize;
            let mut max_delta = 0u8;
            for (a, b) in actual.pixels().zip(expected.pixels()) {
                if a != b {
                    mismatches += 1;
                }
                for channel in 0..4 {
                    max_delta = max_delta.max(a[channel].abs_diff(b[channel]));
                }
            }
            assert!(
                max_delta <= 1,
                "PSD render differs at {mismatches} pixels; maximum channel delta {max_delta}"
            );
        }
    }
    #[test]
    fn external_unsupported_fixture_when_configured() {
        let Some(path) = crate::identity::env_var_os("OMUSE_PSD_REJECT_FIXTURE") else {
            return;
        };
        let error = open(Path::new(&path)).unwrap_err().to_string();
        assert!(
            error.contains("unsupported") || error.contains("only 8-bit"),
            "{error}"
        );
    }
}

#[cfg(test)]
mod zip_tests {
    use super::*;
    #[test]
    fn zip_and_prediction_preserve_each_row() {
        use std::io::Write;
        let pixels = vec![10u8, 20, 30, 40, 50, 60];
        let zip = |bytes: &[u8]| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        };
        assert_eq!(decode_plane(2, 3, 2, &zip(&pixels)).unwrap(), pixels);
        assert_eq!(
            decode_plane(3, 3, 2, &zip(&[10, 10, 10, 40, 10, 10])).unwrap(),
            pixels
        );
        assert!(decode_plane(2, 2, 2, &zip(&pixels)).is_err());
    }

    #[test]
    fn zip_plane_larger_than_declared_geometry_is_rejected() {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&vec![0; 4_096]).unwrap();
        let compressed = encoder.finish().unwrap();
        let error = decode_plane(2, 1, 1, &compressed).unwrap_err().to_string();
        assert!(error.contains("invalid PSD ZIP channel size"));
    }
}
