//! Bounded, truthful exchange reports. A successful export is not a claim of
//! application-specific round-trip fidelity.
use crate::vector_scene::VectorScene;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const METADATA_KEY: &str = "omuseImportCompatibility";
const MAX_NOTES: usize = 128;
const MAX_TEXT: usize = 600;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub operation: String,
    pub file: String,
    pub preserved: Vec<String>,
    pub converted: Vec<String>,
    pub not_included: Vec<String>,
    pub blocked: Vec<String>,
}

fn bounded(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect()
}

impl CompatibilityReport {
    fn new(operation: &str, path: &Path) -> Self {
        Self {
            operation: operation.into(),
            file: bounded(&path.to_string_lossy()),
            preserved: Vec::new(),
            converted: Vec::new(),
            not_included: Vec::new(),
            blocked: Vec::new(),
        }
    }

    pub fn svg_import(path: &Path, scene: Option<&VectorScene>, warnings: &[String]) -> Self {
        let mut report = Self::new("Editable SVG import", path);
        report.preserved.push(match scene {
            Some(scene) => format!("{} editable vector objects; Bézier paths, supported fills, strokes, opacity, visibility and group ancestry.", scene.objects.len()),
            None => "Editable path geometry, solid fill and supported stroke settings.".into(),
        });
        report.converted.push("SVG viewport fitted proportionally into the current vector layer; existing canvas size retained.".into());
        report
            .converted
            .extend(warnings.iter().take(MAX_NOTES - 1).map(|w| bounded(w)));
        report.not_included.push("Original SVG source markup, CSS and application-specific editing data. Keep the source file for those records.".into());
        report
    }

    pub fn vector_export(path: &Path, scene: Option<&VectorScene>, pdf: bool) -> Self {
        let mut report = Self::new(
            if pdf {
                "Vector PDF export"
            } else {
                "Editable SVG export"
            },
            path,
        );
        report.preserved.push("Vector path geometry, supported fill/stroke appearance and transparency from this artwork layer.".into());
        if let Some(scene) = scene {
            let dithered = scene
                .objects
                .iter()
                .filter(|o| !o.is_clip && o.fill_gradient.as_ref().is_some_and(|g| g.dither))
                .count();
            if dithered > 0 {
                report.preserved[0]="Geometry of objects that remain vector, plus supported artwork appearance and transparency; raster conversions are listed below.".into();
                report.converted.push(format!("{dithered} dithered gradient objects (including their strokes) converted to embedded pixels at the artwork's source-pixel resolution. Clipping and opacity remain in the exported appearance; edit the original geometry and paint in .omuse. Editable SVG import does not reconstruct these image objects."));
            }
            if scene.objects.iter().any(|o| {
                o.fill_gradient.as_ref().is_some_and(|g| {
                    g.method == crate::vector_scene::GradientMethod::Perceptual && !g.dither
                })
            }) {
                report.converted.push("Perceptual gradient colour curves approximated by bounded classic vector stops. Original method and editable stop count remain in .omuse; SVG reimport can exceed the editable 16-stop limit.".into());
            }
            if scene.objects.iter().any(|object| object.is_clip) {
                report.converted.push("Clip source silhouettes exported as vector clipping geometry; original clip-source paint, names and editing organization remain in .omuse only.".into());
            }
            if !pdf {
                report.preserved.push(format!(
                    "{} painted objects with names, order, group ancestry{} and visibility.",
                    scene
                        .objects
                        .iter()
                        .filter(|object| !object.is_clip)
                        .count(),
                    if dithered > 0 { "" } else { ", transforms" }
                ));
            } else if scene.objects.iter().any(|object| !object.visible) {
                report
                    .not_included
                    .push("Hidden vector objects do not appear on the PDF page.".into());
            }
            if scene
                .objects
                .iter()
                .any(|object| object.text_path.is_some())
            {
                report.converted.push("Text on a path exported as vector outlines; characters, font settings and the editable text recipe remain in .omuse only.".into());
            }
        }
        if pdf {
            report.converted.push("Artwork mapped to a PDF page using document resolution. This PDF contains vector appearance, not the Omuse editing model.".into());
        }
        report.not_included.push("Other document layers, raster content, layer masks/effects and Omuse edit history. This command exports the active vector artwork only.".into());
        report.not_included.push("An Illustrator or other editor round trip is not guaranteed. Save .omuse to retain the complete editable document.".into());
        report
    }

    pub fn rejected(operation: &str, path: &Path, reason: &str) -> Self {
        let mut report = Self::new(operation, path);
        report.blocked.push(bounded(reason));
        report.not_included.push(
            "No artwork was imported and no output file was published by this operation.".into(),
        );
        report
    }

    /// Validate untrusted saved metadata before it reaches UI or a new project.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            [&self.operation, &self.file]
                .into_iter()
                .all(|s| s.chars().count() <= MAX_TEXT && !s.chars().any(char::is_control)),
            "Invalid compatibility report header"
        );
        let groups = [
            &self.preserved,
            &self.converted,
            &self.not_included,
            &self.blocked,
        ];
        anyhow::ensure!(
            groups.iter().all(|notes| notes.len() <= MAX_NOTES
                && notes
                    .iter()
                    .all(|s| s.chars().count() <= MAX_TEXT && !s.chars().any(char::is_control))),
            "Compatibility report exceeds display limits"
        );
        Ok(())
    }

    pub fn lines(&self) -> Vec<String> {
        if self.validate().is_err() {
            return vec![
                "Compatibility report unavailable: saved report exceeds display limits.".into(),
            ];
        }
        let mut lines = vec![self.operation.clone(), format!("File: {}", self.file)];
        for (heading, notes) in [
            ("Preserved", &self.preserved),
            ("Converted", &self.converted),
            ("Not included", &self.not_included),
            ("Blocked", &self.blocked),
        ] {
            for note in notes {
                lines.push(format!("{heading}: {note}"));
            }
        }
        lines
    }
}

/// Borrowed validation for the compositor's hot admission path; no report clone.
pub fn metadata_is_valid(metadata: &serde_json::Value) -> bool {
    fn text(value: Option<&serde_json::Value>) -> bool {
        value.and_then(|value| value.as_str()).is_some_and(|value| {
            value.len() <= MAX_TEXT * 4
                && value.chars().count() <= MAX_TEXT
                && !value.chars().any(char::is_control)
        })
    }
    let Some(object) = metadata
        .get(METADATA_KEY)
        .and_then(|value| value.as_object())
    else {
        return false;
    };
    object.len() == 6
        && ["operation", "file"]
            .iter()
            .all(|key| text(object.get(*key)))
        && ["preserved", "converted", "not_included", "blocked"]
            .iter()
            .all(|key| {
                object
                    .get(*key)
                    .and_then(|value| value.as_array())
                    .is_some_and(|notes| {
                        notes.len() <= MAX_NOTES && notes.iter().all(|note| text(Some(note)))
                    })
            })
}

pub fn from_metadata(metadata: &serde_json::Value) -> Option<CompatibilityReport> {
    if !metadata_is_valid(metadata) {
        return None;
    }
    serde_json::from_value(metadata.get(METADATA_KEY)?.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_conversion_warnings_remain_visible_and_sources_are_not_claimed_lossless() {
        let report = CompatibilityReport::svg_import(
            Path::new("source.svg"),
            None,
            &["Text converted to outlines".into()],
        );
        let lines = report.lines().join("\n");
        assert!(lines.contains("Converted: Text converted to outlines"));
        assert!(lines.contains("Preserved: Editable path geometry"));
        assert!(lines.contains("Not included: Original SVG source markup"));
        assert!(report.blocked.is_empty());
    }

    #[test]
    fn failed_exchange_never_claims_preserved_artwork() {
        let report = CompatibilityReport::rejected(
            "SVG import",
            Path::new("linked.svg"),
            "External resource rejected",
        );
        assert!(report.preserved.is_empty());
        assert!(report.converted.is_empty());
        assert!(report.lines().iter().any(|s| s.starts_with("Blocked:")));
    }

    #[test]
    fn report_metadata_is_bounded_and_control_characters_are_removed() {
        let report =
            CompatibilityReport::rejected("Import", Path::new("a\nb.svg"), &"x".repeat(1000));
        report.validate().unwrap();
        assert_eq!(report.file, "ab.svg");
        assert_eq!(report.blocked[0].len(), MAX_TEXT);
        let mut metadata = serde_json::json!({METADATA_KEY: report});
        assert!(from_metadata(&metadata).is_some());
        metadata[METADATA_KEY]["blocked"] = serde_json::json!(vec!["too many"; MAX_NOTES + 1]);
        assert!(from_metadata(&metadata).is_none());
    }
}
