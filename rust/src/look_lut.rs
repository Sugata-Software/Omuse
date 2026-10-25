//! Portable, bounded `.cube` looks. Version 1 interprets RGB as encoded sRGB,
//! clamps lookup coordinates to their declared domain, uses linear 1D and
//! trilinear 3D interpolation, then clips the Amount blend to 0..1. This is a
//! creative look, not an ICC camera/display profile or a linear/HDR transform.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CUBE_SIZE: usize = 65;
pub const MAX_SHAPER_SIZE: usize = 4096;
const MAX_LINE_BYTES: usize = 16_384;
const MAX_TITLE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    #[default]
    EncodedSrgb,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GamutPolicy {
    /// Clip the final source + Amount * (look - source) in encoded sRGB.
    #[default]
    ClipOutput,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CubeLut {
    pub version: u32,
    pub title: String,
    pub encoding: Encoding,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// Domain of the 3D lookup after an optional 1D shaper.
    pub cube_domain_min: [f32; 3],
    pub cube_domain_max: [f32; 3],
    pub shaper: Vec<[f32; 3]>,
    /// Zero denotes a 1D-only look. Cube samples use red-fastest ordering.
    pub cube_size: u8,
    pub values: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub table: CubeLut,
    /// 1 is 100%; version 1 supports 0..=200%.
    pub amount: f32,
    pub gamut: GamutPolicy,
}

impl Settings {
    pub fn new(table: CubeLut) -> Self {
        Self {
            table,
            amount: 1.,
            gamut: GamutPolicy::ClipOutput,
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.table.validate()?;
        ensure!(
            self.amount.is_finite() && (0. ..=2.).contains(&self.amount),
            "Look Amount must be between 0 and 200%"
        );
        Ok(())
    }
    /// Call after validation. Alpha and colour-space conversion belong to the
    /// image kernel; this function only evaluates encoded-sRGB colour.
    pub fn sample(&self, source: [f32; 3]) -> [f32; 3] {
        if self.amount == 0. {
            return source;
        }
        let looked = self.table.sample(source);
        std::array::from_fn(|c| (source[c] + self.amount * (looked[c] - source[c])).clamp(0., 1.))
    }
    pub fn owned_bytes(&self) -> usize {
        self.table.owned_bytes()
    }
}

impl CubeLut {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported portable look version");
        ensure!(
            !self.title.is_empty()
                && self.title.len() <= MAX_TITLE_BYTES
                && !self.title.chars().any(char::is_control),
            "Look title is invalid or too long"
        );
        for (min, max) in [
            (self.domain_min, self.domain_max),
            (self.cube_domain_min, self.cube_domain_max),
        ] {
            ensure!(
                (0..3).all(|c| min[c].is_finite()
                    && max[c].is_finite()
                    && (-16. ..=16.).contains(&min[c])
                    && (-16. ..=16.).contains(&max[c])
                    && max[c] > min[c]),
                "Look domains must be finite, increasing, and within -16..16"
            );
        }
        ensure!(
            self.shaper.is_empty() || (2..=MAX_SHAPER_SIZE).contains(&self.shaper.len()),
            "A 1D look needs 2–4096 entries"
        );
        let size = usize::from(self.cube_size);
        ensure!(
            size == 0 || (2..=MAX_CUBE_SIZE).contains(&size),
            "A 3D look needs 2–65 entries per axis"
        );
        ensure!(
            self.values.len() == size.pow(3),
            "3D look sample count does not match its dimensions"
        );
        ensure!(
            !self.shaper.is_empty() || size > 0,
            "Look has no lookup table"
        );
        ensure!(
            self.shaper
                .iter()
                .chain(&self.values)
                .flatten()
                .all(|v| v.is_finite() && (-16. ..=16.).contains(v)),
            "Look samples must be finite and within -16..16"
        );
        Ok(())
    }
    pub fn owned_bytes(&self) -> usize {
        self.title
            .capacity()
            .saturating_add(self.shaper.capacity().saturating_mul(12))
            .saturating_add(self.values.capacity().saturating_mul(12))
    }
    pub fn description(&self) -> String {
        match (self.shaper.len(), self.cube_size) {
            (n, 0) => format!("{n}-entry 1D · encoded sRGB"),
            (0, n) => format!("{n}³ · trilinear · encoded sRGB"),
            (n, size) => format!("{n}-entry shaper + {size}³ · encoded sRGB"),
        }
    }
    /// A stable diagnostic identity of the complete embedded table and its
    /// interpretation. This is FNV-1a, not a signature or security boundary;
    /// table bytes, never this hash alone, remain authoritative.
    pub fn fingerprint(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        let mut add = |bytes: &[u8]| {
            for &b in bytes {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        };
        add(&self.version.to_le_bytes());
        add(&[self.cube_size]);
        add(&(self.shaper.len() as u64).to_le_bytes());
        for v in self
            .domain_min
            .into_iter()
            .chain(self.domain_max)
            .chain(self.cube_domain_min)
            .chain(self.cube_domain_max)
            .chain(self.shaper.iter().chain(&self.values).flatten().copied())
        {
            add(&v.to_bits().to_le_bytes());
        }
        format!("cube-v1-fnv1a64-{hash:016x}")
    }
    fn sample(&self, source: [f32; 3]) -> [f32; 3] {
        let normal = normalize(source, self.domain_min, self.domain_max);
        let shaped = if self.shaper.is_empty() {
            normal
        } else {
            std::array::from_fn(|c| {
                let (i, j, t) = interval(normal[c], self.shaper.len());
                lerp(self.shaper[i][c], self.shaper[j][c], t)
            })
        };
        let size = usize::from(self.cube_size);
        if size == 0 {
            return shaped;
        }
        let coords = if self.shaper.is_empty() {
            normal
        } else {
            normalize(shaped, self.cube_domain_min, self.cube_domain_max)
        };
        let axes = coords.map(|v| interval(v, size));
        let at = |r: usize, g: usize, b: usize, c: usize| self.values[(b * size + g) * size + r][c];
        std::array::from_fn(|c| {
            let (r0, r1, rt) = axes[0];
            let (g0, g1, gt) = axes[1];
            let (b0, b1, bt) = axes[2];
            let plane = |b| {
                lerp(
                    lerp(at(r0, g0, b, c), at(r1, g0, b, c), rt),
                    lerp(at(r0, g1, b, c), at(r1, g1, b, c), rt),
                    gt,
                )
            };
            lerp(plane(b0), plane(b1), bt)
        })
    }
}

fn normalize(rgb: [f32; 3], min: [f32; 3], max: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| ((rgb[c] - min[c]) / (max[c] - min[c])).clamp(0., 1.))
}
fn interval(v: f32, len: usize) -> (usize, usize, f32) {
    let pos = v * (len - 1) as f32;
    let i = (pos.floor() as usize).min(len - 1);
    (i, (i + 1).min(len - 1), pos - i as f32)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Read a bounded regular file, rejecting symlinks/devices/FIFOs. The source
/// file is never rewritten; projects embed the validated table itself.
pub fn import(path: &Path) -> Result<CubeLut> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "Look input must be a regular file, not a symlink or device"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path).context("Cannot open cube look")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_INPUT_BYTES as u64,
        "Look input must be a regular file no larger than 16 MiB"
    );
    let mut bytes = Vec::new();
    let mut bounded = file.take(MAX_INPUT_BYTES as u64 + 1);
    bounded.read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_INPUT_BYTES, "Cube look exceeds 16 MiB");
    ensure!(
        bytes.len() as u64 == metadata.len()
            && bounded.get_ref().metadata()?.len() == metadata.len(),
        "Cube look changed while it was read"
    );
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported look");
    parse(
        std::str::from_utf8(&bytes).context("Cube look must be UTF-8 text")?,
        fallback,
    )
}

/// Parse 1D, 3D, or Resolve-style 1D-shaper + 3D cube tables. No table is
/// resampled. Ambiguous mixed DOMAIN/INPUT_RANGE directives are refused.
pub fn parse(text: &str, fallback_title: &str) -> Result<CubeLut> {
    ensure!(text.len() <= MAX_INPUT_BYTES, "Cube look exceeds 16 MiB");
    let mut title = None;
    let mut one = None;
    let mut three = None;
    let mut min = None;
    let mut max = None;
    let mut one_range = None;
    let mut three_range = None;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for (number, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        ensure!(
            line.len() <= MAX_LINE_BYTES,
            "Cube line {} exceeds 16 KiB",
            number + 1
        );
        let mut quoted = false;
        let end = line
            .char_indices()
            .find_map(|(i, c)| {
                if c == '"' {
                    quoted = !quoted;
                }
                (c == '#' && !quoted).then_some(i)
            })
            .unwrap_or(line.len());
        let line = line[..end].trim();
        if line.is_empty() {
            continue;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        let directive = words[0];
        if directive.as_bytes()[0].is_ascii_alphabetic() {
            ensure!(
                rows.is_empty(),
                "Cube headers must precede all sample rows (line {})",
                number + 1
            );
            match directive {
                "TITLE" => {
                    ensure!(title.is_none(), "Duplicate cube TITLE");
                    let value = line[5..].trim();
                    ensure!(
                        value.starts_with('"') && value.ends_with('"') && value.len() >= 2,
                        "Cube TITLE must be quoted"
                    );
                    title = Some(value[1..value.len() - 1].to_owned());
                }
                "LUT_1D_SIZE" | "LUT_3D_SIZE" => {
                    ensure!(words.len() == 2, "Invalid cube size directive");
                    let n: usize = words[1].parse().context("Invalid cube table size")?;
                    let slot = if directive == "LUT_1D_SIZE" {
                        &mut one
                    } else {
                        &mut three
                    };
                    ensure!(slot.is_none(), "Duplicate {directive}");
                    let limit = if directive == "LUT_1D_SIZE" {
                        MAX_SHAPER_SIZE
                    } else {
                        MAX_CUBE_SIZE
                    };
                    ensure!(
                        (2..=limit).contains(&n),
                        "{directive} must be between 2 and {limit}; no automatic resampling is performed"
                    );
                    *slot = Some(n);
                }
                "DOMAIN_MIN" | "DOMAIN_MAX" => {
                    ensure!(words.len() == 4, "Cube domain needs three numbers");
                    let v = triple(&words[1..])?;
                    let slot = if directive == "DOMAIN_MIN" {
                        &mut min
                    } else {
                        &mut max
                    };
                    ensure!(slot.is_none(), "Duplicate {directive}");
                    *slot = Some(v);
                }
                "LUT_1D_INPUT_RANGE" | "LUT_3D_INPUT_RANGE" => {
                    ensure!(words.len() == 3, "Cube input range needs two numbers");
                    let a = number_value(words[1])?;
                    let b = number_value(words[2])?;
                    ensure!(b > a, "Cube input range must increase");
                    let slot = if directive == "LUT_1D_INPUT_RANGE" {
                        &mut one_range
                    } else {
                        &mut three_range
                    };
                    ensure!(slot.is_none(), "Duplicate {directive}");
                    *slot = Some((a, b));
                }
                _ => bail!(
                    "Unsupported cube directive {directive} on line {}",
                    number + 1
                ),
            }
        } else {
            let expected = one.unwrap_or(0) + three.unwrap_or(0usize).pow(3);
            ensure!(expected > 0, "Cube table size must precede sample rows");
            ensure!(
                rows.len() < expected,
                "Cube has more sample rows than declared"
            );
            ensure!(
                words.len() == 3,
                "Cube sample needs three numbers on line {}",
                number + 1
            );
            rows.push(triple(&words)?);
        }
    }
    ensure!(
        one.is_some() || three.is_some(),
        "Cube declares no supported table"
    );
    ensure!(
        one_range.is_none() || one.is_some(),
        "1D input range has no 1D table"
    );
    ensure!(
        three_range.is_none() || three.is_some(),
        "3D input range has no 3D table"
    );
    ensure!(
        !(min.is_some() || max.is_some())
            || (one_range.is_none()
                && three_range.is_none()
                && !(one.is_some() && three.is_some())),
        "Combined shaper/cube tables must use explicit INPUT_RANGE directives; mixed DOMAIN directives are ambiguous"
    );
    let expected = one.unwrap_or(0) + three.unwrap_or(0usize).pow(3);
    ensure!(
        rows.len() == expected,
        "Cube declares {expected} sample rows but contains {}",
        rows.len()
    );
    let domain = if one.is_some() {
        one_range
    } else {
        three_range
    };
    let domain_min = min.unwrap_or([domain.map_or(0., |r| r.0); 3]);
    let domain_max = max.unwrap_or([domain.map_or(1., |r| r.1); 3]);
    let values = rows.split_off(one.unwrap_or(0));
    let look = CubeLut {
        version: 1,
        title: title.unwrap_or_else(|| {
            fallback_title
                .chars()
                .filter(|c| !c.is_control())
                .take(64)
                .collect()
        }),
        encoding: Encoding::EncodedSrgb,
        domain_min,
        domain_max,
        cube_domain_min: [three_range.map_or(0., |r| r.0); 3],
        cube_domain_max: [three_range.map_or(1., |r| r.1); 3],
        shaper: rows,
        cube_size: three.unwrap_or(0) as u8,
        values,
    };
    look.validate()?;
    Ok(look)
}
fn number_value(s: &str) -> Result<f32> {
    let v: f32 = s.parse().context("Invalid cube number")?;
    ensure!(
        v.is_finite() && (-16. ..=16.).contains(&v),
        "Cube numbers must be finite and within -16..16"
    );
    Ok(v)
}
fn triple(words: &[&str]) -> Result<[f32; 3]> {
    Ok([
        number_value(words[0])?,
        number_value(words[1])?,
        number_value(words[2])?,
    ])
}
