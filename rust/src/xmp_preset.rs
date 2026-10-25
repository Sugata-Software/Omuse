//! Bounded user-owned XMP preset intent adapter; never an Adobe renderer.
//! No external resource resolution, profile loading, source-sidecar writes or
//! unreviewed application. The UI must show every refused/approximate field.
use crate::camera_raw::Settings;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
const MAX_PROPERTIES: usize = 128;
const MAX_VALUE_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Rendered,
    DecodedRaw,
}

/// Populate from actual retained source/editor identity, never the XMP's claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TargetIdentity {
    pub kind: SourceKind,
    pub layer_id: String,
    pub source_id: String,
    pub editor_instance: u64,
    pub revision: u64,
    pub working_space: String,
    pub processing: String,
    pub tone_mapping: String,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub camera_profile: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Disposition {
    /// Identity/metadata only, never executed or used for resource lookup.
    RetainedIdentity,
    /// Native slider intent only; this is not calibrated Adobe appearance.
    Approximate,
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MappingRow {
    pub namespace: String,
    pub property: String,
    pub input: String,
    pub target: Option<String>,
    pub disposition: Disposition,
    pub explanation: String,
}

/// Portable inert provenance. Embed this bounded value beside the actual native
/// Camera Raw recipe; replay only that native recipe, never re-import this report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MappingReport {
    pub adapter_version: u32,
    pub source_sha256: String,
    pub target: TargetIdentity,
    pub rows: Vec<MappingRow>,
    pub notices: Vec<String>,
}

static PHOTO_PROMOTION_PREPARING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
struct PhotoPromotionPermit;
impl Drop for PhotoPromotionPermit {
    fn drop(&mut self) {
        PHOTO_PROMOTION_PREPARING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// An identity wrapper prepared off the UI thread. Fields are private to the
/// crate so a caller cannot substitute a different source after certification.
pub struct PreparedPhotoPromotion {
    pub(crate) layer_id: String,
    pub(crate) editor_instance: u64,
    pub(crate) revision: u64,
    pub(crate) source: std::sync::Arc<image::RgbaImage>,
    pub(crate) state: crate::advanced::LayerState,
    // Keep admission through a completed result awaiting the UI, not only the
    // tile-construction loop. Cancelled ready results cannot pile up in memory.
    _permit: PhotoPromotionPermit,
}
impl PreparedPhotoPromotion {
    pub fn prepare(
        target: TargetIdentity,
        source: std::sync::Arc<image::RgbaImage>,
        name: String,
        blend_if: Option<crate::advanced_ops::BlendIf>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Self> {
        use std::sync::atomic::Ordering;
        ensure!(!cancel.load(Ordering::Relaxed), "Photo promotion cancelled");
        // Cancellation discards results immediately, but the existing bounded
        // constructor may finish its tile conversion. Retain one global permit
        // through that work so repeated cancel/reopen cannot queue huge copies.
        ensure!(
            PHOTO_PROMOTION_PREPARING
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok(),
            "A photo conversion is finishing; try again shortly"
        );
        let permit = PhotoPromotionPermit;
        let mut state = crate::advanced::LayerState::from_image(&source, &name)?;
        state.recipe.blend_if = blend_if;
        state.validate()?;
        ensure!(!cancel.load(Ordering::Relaxed), "Photo promotion cancelled");
        Ok(Self {
            layer_id: target.layer_id,
            editor_instance: target.editor_instance,
            revision: target.revision,
            source,
            state,
            _permit: permit,
        })
    }
}

pub const METADATA_KEY: &str = "xmpPresetImport";

impl MappingReport {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.adapter_version == 1
                && self.source_sha256.len() == 64
                && self
                    .source_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "Invalid XMP provenance identity"
        );
        for value in [
            &self.target.layer_id,
            &self.target.source_id,
            &self.target.working_space,
            &self.target.processing,
            &self.target.tone_mapping,
        ] {
            ensure!(
                !value.is_empty() && value.len() <= 256 && !value.contains('\0'),
                "Invalid XMP target provenance identity"
            );
        }
        for value in [
            &self.target.camera_make,
            &self.target.camera_model,
            &self.target.camera_profile,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                value.len() <= 256 && !value.contains('\0'),
                "XMP target camera identity exceeds limit"
            );
        }
        ensure!(
            self.rows.len() <= MAX_PROPERTIES && self.notices.len() <= 8,
            "XMP provenance exceeds property budget"
        );
        for row in &self.rows {
            ensure!(
                row.namespace.len() <= 256
                    && row.property.len() <= 128
                    && row.explanation.len() <= 1024
                    && row.target.as_ref().is_none_or(|target| target.len() <= 128),
                "XMP provenance row exceeds budget"
            );
            bounded(&row.input)?;
        }
        for notice in &self.notices {
            bounded(notice)?;
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 240 * 1024,
            "XMP mapping provenance exceeds save budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SavedProvenance {
    pub version: u32,
    pub mapping: MappingReport,
    pub native_settings_at_apply: Settings,
    pub modified_after_import: bool,
}
impl SavedProvenance {
    pub fn new(
        mapping: MappingReport,
        reviewed: &serde_json::Value,
        settings: &Settings,
    ) -> Result<Self> {
        let value = Self {
            version: 1,
            mapping,
            native_settings_at_apply: settings.clone(),
            modified_after_import: serde_json::to_value(settings)? != *reviewed,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported XMP provenance version");
        self.mapping.validate()?;
        crate::camera_raw::validate(&self.native_settings_at_apply)?;
        ensure!(
            self.native_settings_at_apply.processing == crate::camera_raw::Processing::Native16V1,
            "Imported preset application requires retained Native16 processing"
        );
        let settings_json = serde_json::to_value(&self.native_settings_at_apply)?;
        ensure!(
            self.mapping.target.processing == "Native16V1"
                && settings_json
                    .get("toneMapping")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Legacy")
                    == self.mapping.target.tone_mapping,
            "XMP target processing differs from saved native settings"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 256 * 1024,
            "XMP provenance exceeds save budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PresetPlan {
    report: MappingReport,
    changes: BTreeMap<&'static str, f32>,
    source_xml: std::sync::Arc<[u8]>,
}
impl PresetPlan {
    pub fn report(&self) -> &MappingReport {
        &self.report
    }
    pub fn mapped_count(&self) -> usize {
        self.changes.len()
    }
    /// Rebuild an inert review after an explicit target promotion. The original
    /// bounded packet is held only in memory and never used as a saved renderer.
    pub fn for_promoted_target(&self, target: TargetIdentity) -> Result<Self> {
        parse(&self.source_xml, target)
    }

    /// Call only after deliberate “Use supported settings” review. The same
    /// identity guard must be checked again before the existing native Apply.
    pub fn reviewed_settings(&self, base: &Settings, current: &TargetIdentity) -> Result<Settings> {
        ensure!(
            current == &self.report.target,
            "Preset source or editor changed; import again"
        );
        ensure!(
            !self.changes.is_empty(),
            "This preset has no supported settings to preview"
        );
        let base_json = serde_json::to_value(base)?;
        let processing = base_json
            .get("processing")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ByteCompatible");
        let tone_mapping = base_json
            .get("toneMapping")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Legacy");
        ensure!(
            processing == current.processing && tone_mapping == current.tone_mapping,
            "Target processing changed; review preset again"
        );
        let mut output = base.clone();
        for (&key, &value) in &self.changes {
            match key {
                "exposure" => output.exposure = value,
                "contrast" => output.contrast = value,
                "highlights" => output.highlights = value,
                "shadows" => output.shadows = value,
                "whites" => output.whites = value,
                "blacks" => output.blacks = value,
                "clarity" => output.clarity = value,
                "vibrance" => output.vibrance = value,
                "saturation" => output.saturation = value,
                "vignetteAmount" => output.vignette_amount = value,
                "vignetteMidpoint" => output.vignette_midpoint = value,
                _ => unreachable!("private parser owns setting keys"),
            }
        }
        crate::camera_raw::validate(&output)?;
        Ok(output)
    }
}

#[derive(Clone)]
struct Property {
    namespace: String,
    name: String,
    value: String,
    structured: bool,
}

fn mapping(name: &str) -> Option<(&'static str, f32, f32)> {
    Some(match name {
        "Exposure2012" => ("exposure", -5., 5.),
        "Exposure" => ("exposure", -4., 4.),
        "Contrast2012" => ("contrast", -100., 100.),
        "Contrast" => ("contrast", -50., 100.),
        "Highlights2012" => ("highlights", -100., 100.),
        "Shadows2012" => ("shadows", -100., 100.),
        "Whites2012" => ("whites", -100., 100.),
        "Blacks2012" => ("blacks", -100., 100.),
        "Clarity2012" => ("clarity", -100., 100.),
        "Vibrance" => ("vibrance", -100., 100.),
        "Saturation" => ("saturation", -100., 100.),
        "VignetteAmount" => ("vignetteAmount", -100., 100.),
        "VignetteMidpoint" => ("vignetteMidpoint", 0., 100.),
        _ => return None,
    })
}
fn identity(namespace: &str, name: &str) -> bool {
    (namespace == CRS
        && matches!(
            name,
            "ProcessVersion"
                | "Version"
                | "CameraProfile"
                | "CameraProfileDigest"
                | "CameraModelRestriction"
                | "RawFileName"
                | "Name"
                | "UUID"
                | "PresetType"
        ))
        || (namespace == TIFF && matches!(name, "Make" | "Model"))
}
fn bounded(value: &str) -> Result<()> {
    ensure!(value.len() <= MAX_VALUE_BYTES, "XMP property exceeds 1 KiB");
    ensure!(!value.contains('\0'), "Invalid XMP property text");
    Ok(())
}

/// Read only an explicitly selected local XMP file. Profile files, directories,
/// symlinks and devices are refused; no archive expansion or network lookup.
pub fn import(path: &std::path::Path, target: TargetIdentity) -> Result<PresetPlan> {
    use std::{fs, io::Read};
    ensure!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xmp")),
        "Choose an XMP preset; DCP/LCP profiles and archives are not supported"
    );
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "XMP input must be a regular file, not a symlink or device"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800); // O_NOFOLLOW | O_NONBLOCK.
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT.
    }
    let file = options.open(path).context("Cannot open XMP preset")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= MAX_INPUT_BYTES as u64,
        "XMP input must be a regular file no larger than 256 KiB"
    );
    let mut limited = file.take(MAX_INPUT_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    let after = limited.get_ref().metadata()?;
    ensure!(
        bytes.len() <= MAX_INPUT_BYTES
            && bytes.len() as u64 == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "XMP preset changed or exceeded its limit while being read"
    );
    parse(&bytes, target)
}

pub fn parse(input: &[u8], target: TargetIdentity) -> Result<PresetPlan> {
    ensure!(input.len() <= MAX_INPUT_BYTES, "XMP input exceeds 256 KiB");
    let xml = std::str::from_utf8(input).context("XMP preset must be UTF-8")?;
    for value in [
        &target.layer_id,
        &target.source_id,
        &target.working_space,
        &target.processing,
        &target.tone_mapping,
    ] {
        ensure!(
            !value.is_empty() && value.len() <= 256,
            "Invalid target identity"
        );
    }
    for value in [
        &target.camera_make,
        &target.camera_model,
        &target.camera_profile,
    ]
    .into_iter()
    .flatten()
    {
        ensure!(value.len() <= 256, "Target camera identity exceeds limit");
    }
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 2048,
            ..Default::default()
        },
    )
    .context("Invalid or unsupported XMP XML")?;
    ensure!(
        document
            .descendants()
            .all(|node| node.ancestors().count() <= 16),
        "XMP nesting exceeds 16 levels"
    );
    let rdf: Vec<_> = document
        .descendants()
        .filter(|node| node.has_tag_name((RDF, "RDF")))
        .collect();
    ensure!(rdf.len() == 1, "Expected exactly one RDF metadata packet");
    ensure!(
        rdf[0].ancestors().all(|node| !node.is_element()
            || node == rdf[0]
            || node.has_tag_name(("adobe:ns:meta/", "xmpmeta"))),
        "Unsupported XMP wrapper"
    );
    let mut properties = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |namespace: &str, name: &str, value: &str, structured: bool| -> Result<()> {
        ensure!(
            properties.len() < MAX_PROPERTIES,
            "XMP exceeds 128 scalar/structured properties"
        );
        ensure!(
            name.len() <= 128 && namespace.len() <= 256,
            "XMP property name exceeds limit"
        );
        bounded(value)?;
        ensure!(
            seen.insert((namespace.to_owned(), name.to_owned())),
            "Duplicate XMP property {name}; no last-wins import"
        );
        properties.push(Property {
            namespace: namespace.into(),
            name: name.into(),
            value: value.into(),
            structured,
        });
        Ok(())
    };
    let descriptions: Vec<_> = rdf[0].children().filter(|node| node.is_element()).collect();
    ensure!(
        !descriptions.is_empty() && descriptions.len() <= 8,
        "XMP must contain 1–8 RDF descriptions"
    );
    for description in descriptions {
        ensure!(
            description.has_tag_name((RDF, "Description")),
            "Unsupported RDF root element"
        );
        for attribute in description.attributes() {
            if attribute.namespace() == Some(RDF) && attribute.name() == "about" {
                continue;
            }
            push(
                attribute.namespace().unwrap_or(""),
                attribute.name(),
                attribute.value(),
                false,
            )?;
        }
        for child in description.children().filter(|node| node.is_element()) {
            let structured =
                child.children().any(|node| node.is_element()) || child.attributes().len() != 0;
            let value = if structured {
                "[structured RDF setting: not imported]"
            } else {
                child.text().unwrap_or("")
            };
            push(
                child.tag_name().namespace().unwrap_or(""),
                child.tag_name().name(),
                value,
                structured,
            )?;
        }
    }
    ensure!(!properties.is_empty(), "XMP contains no settings");
    // Precompute aliases before mapping, so property order and even identical
    // legacy/modern values cannot silently decide another source process.
    let mut slots: BTreeMap<&str, usize> = BTreeMap::new();
    for property in &properties {
        if property.namespace == CRS
            && let Some((slot, _, _)) = mapping(&property.name)
        {
            *slots.entry(slot).or_default() += 1;
        }
    }
    let mut changes = BTreeMap::new();
    let mut rows = Vec::new();
    for property in properties {
        let mut row = MappingRow {
            namespace: property.namespace.clone(),
            property: property.name.clone(),
            input: property.value.clone(),
            target: None,
            disposition: Disposition::Refused,
            explanation: String::new(),
        };
        if property.structured {
            row.explanation = "Structured curves, local corrections, profile/look tables and resource references are not imported".into();
        } else if property.namespace == CRS && property.name == "CameraProfile" {
            row.explanation = "Camera/profile transform refused; the source profile identity is retained only as an inert claim, with no DCP lookup or camera-matched fallback".into();
        } else if identity(&property.namespace, &property.name) {
            row.disposition = Disposition::RetainedIdentity;
            row.explanation = "Inert source claim only; does not select Adobe process, camera, profile or an external resource".into();
        } else if property.namespace != CRS {
            row.explanation =
                "Metadata outside the Camera Raw namespace is not an editing instruction".into();
        } else if let Some((slot, minimum, maximum)) = mapping(&property.name) {
            row.target = Some(slot.into());
            if slots[slot] != 1 {
                row.explanation = "Conflicting legacy/modern aliases target this slider; both refused regardless of XML order or value".into();
            } else {
                let number = property.value.trim().parse::<f32>();
                match number {
                    Ok(value) if value.is_finite() && (minimum..=maximum).contains(&value) => {
                        changes.insert(slot, value);
                        row.disposition = Disposition::Approximate;
                        row.explanation = format!(
                            "Native {slot} slider intent ({minimum}…{maximum}); Omuse rendering is not Adobe appearance or process equivalence"
                        );
                    }
                    _ => {
                        row.explanation = format!(
                            "Invalid/non-finite value or outside adapter bounds {minimum}…{maximum}; never clamped"
                        )
                    }
                }
            }
        } else if matches!(
            property.name.as_str(),
            "Temperature" | "Tint" | "WhiteBalance" | "IncrementalTemperature" | "IncrementalTint"
        ) {
            row.explanation = "RAW Kelvin/as-shot gains and rendered relative white balance are different; this adapter preserves the target's existing white balance".into();
        } else if property.name.contains("Profile")
            || property.name.contains("Look")
            || property.name.contains("DCP")
            || property.name.contains("LCP")
        {
            row.explanation = "DCP/LCP/profile/look transforms and redistribution are unsupported; no file lookup or table execution".into();
        } else {
            row.explanation = "Unsupported setting (including auto tone, crop, local masks, calibration and legacy non-equivalent sliders); target retained".into();
        }
        rows.push(row);
    }
    rows.sort_by(|a, b| (&a.namespace, &a.property).cmp(&(&b.namespace, &b.property)));
    Ok(PresetPlan {
        report: MappingReport {
            adapter_version: 1,
            source_sha256: crate::asset_library::sha256_hex(input),
            target,
            rows,
            notices: vec![
                "Review every field. Use supported settings changes only mapped native sliders; omitted/refused settings remain unchanged.".into(),
                "Adobe process/profile identity is retained as an inert claim. No Adobe colour/rendering parity or calibrated camera/profile fallback is established.".into(),
                "RAW Kelvin/as-shot and rendered relative white balance are never interchanged. The source XMP is not rewritten or redistributed.".into(),
            ],
        },
        changes,
        source_xml: input.into(),
    })
}
