//! Validated, per-user editor shortcuts. Control is the Linux menu modifier.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const DEFINITIONS: &[(&str, &str, &str)] = &[
    ("new", "New canvas", "ctrl-n"),
    ("ask-omuse", "Ask Omuse", "ctrl-shift-j"),
    ("create-workspace", "Create workspace", "ctrl-shift-e"),
    ("previous-page", "Previous page", "alt-pageup"),
    ("next-page", "Next page", "alt-pagedown"),
    ("open", "Open", "ctrl-o"),
    ("save", "Save", "ctrl-s"),
    ("save-as", "Save as", "ctrl-shift-s"),
    ("export", "Export", "ctrl-e"),
    ("import", "Import", "ctrl-shift-o"),
    ("undo", "Undo", "ctrl-z"),
    ("redo", "Redo", "ctrl-shift-z"),
    ("copy", "Copy", "ctrl-c"),
    ("cut", "Cut", "ctrl-x"),
    ("paste", "Paste", "ctrl-v"),
    ("copy-merged", "Copy merged", "ctrl-shift-c"),
    ("select-all", "Select all", "ctrl-a"),
    ("deselect", "Deselect", "ctrl-d"),
    ("invert-selection", "Invert selection", "ctrl-shift-i"),
    ("zoom-in", "Zoom in", "ctrl-="),
    ("zoom-out", "Zoom out", "ctrl--"),
    ("fit", "Fit canvas", "ctrl-0"),
    ("actual", "Actual pixels", "ctrl-1"),
    ("quit", "Close", "ctrl-q"),
    ("duplicate", "Duplicate selected layers", "ctrl-j"),
    ("delete-content", "Delete selection or layer", "backspace"),
    ("add", "New layer", "ctrl-shift-n"),
    ("group", "Group selected layers", "ctrl-g"),
    ("transform", "Transform", "ctrl-t"),
    ("invert", "Invert pixels", "ctrl-i"),
    ("fill-foreground", "Fill foreground", "alt-backspace"),
    ("fill-background", "Fill background", "ctrl-backspace"),
    ("default-colors", "Default colors", "d"),
    ("swap-colors", "Swap colors", "x"),
    ("tool-settings", "Tool settings", ""),
    ("filter-stack", "Editable filter stack", ""),
    ("blend-if", "Blend If", ""),
    ("smart-source", "Smart source", ""),
    ("editable-raw", "Develop embedded RAW", ""),
    ("colour-management", "Precision and colour", ""),
    ("advanced-retouch", "Frequency and tonal retouch", ""),
    ("controlled-removal", "Controlled content-aware removal", ""),
    ("editable-warp", "Editable mesh and pin warp", ""),
    ("refine-workspace", "Selection refinement workspace", ""),
    ("brush-studio", "Brush studio", ""),
    ("vector-path", "Vector paths", ""),
    ("vector-mask", "Vector mask", ""),
    ("automation", "Recipes and batch processing", ""),
    ("multi-image", "Focus HDR and panorama merge", ""),
    ("grid", "Toggle grid", "ctrl-'"),
    ("guides", "Toggle guides", "ctrl-;"),
    ("shortcuts", "Keyboard shortcuts", "ctrl-alt-k"),
    ("tool-brush", "Brush", "b"),
    ("tool-pencil", "Pencil", "p"),
    ("tool-eraser", "Eraser", "e"),
    ("tool-fill", "Fill", "f"),
    ("tool-gradient", "Gradient", "g"),
    ("tool-rectangle", "Rectangle selection", "m"),
    ("tool-ellipse", "Ellipse selection", "o"),
    ("tool-move", "Move", "v"),
    ("tool-picker", "Eyedropper", "i"),
    ("tool-clone", "Clone stamp", "s"),
    ("tool-heal", "Healing clone", "h"),
    ("tool-spot-heal", "Spot healing", ""),
    ("rulers", "Toggle rulers", ""),
    ("snapping", "Toggle snapping", ""),
    ("auto-select", "Toggle auto-select", ""),
    ("transform-box", "Toggle transform box", ""),
    ("trim", "Trim canvas", ""),
    ("visibility", "Toggle layer visibility", ""),
    ("clipping", "Toggle clipping mask", ""),
    ("remove-mask", "Delete layer mask", ""),
    ("clear-effects", "Delete layer effects", ""),
    ("import-report", "Import conversion report", ""),
    ("tool-wand", "Wand", "w"),
    ("tool-lasso", "Lasso", "l"),
    ("text", "Text", "t"),
    ("camera-raw", "Camera Raw", ""),
    ("live-adjustment", "Add adjustment layer", ""),
    ("edit-adjustment", "Edit adjustment", ""),
    ("effects", "Layer effects", ""),
    ("edit-object", "Edit text or shape", ""),
    ("rasterize", "Rasterize object", ""),
    ("distort", "Distort corners", ""),
    ("sampling", "Cycle sampling quality", ""),
    ("transform-selection", "Transform selection", ""),
    ("commit-selection", "Commit selection", ""),
    ("cancel-selection", "Cancel selection", ""),
    ("select-subject", "Select subject", ""),
    ("luminosity-range", "Luminosity range", ""),
    ("color-range", "Colour range", ""),
    ("tool-object", "Connected subject tool", ""),
    ("remove-background", "Remove background", ""),
    ("content-fill", "Content-aware fill", ""),
    ("feather-selection", "Feather selection", ""),
    ("grow-selection", "Expand selection", ""),
    ("shrink-selection", "Contract selection", ""),
    ("mask-paint", "Toggle mask painting", ""),
    ("mask-link", "Link mask", ""),
    ("mask-transform", "Place mask", ""),
    ("live-mask", "Live mask source", ""),
    ("add-guide", "Manage guides", ""),
    ("resize-image", "Resize image", ""),
    ("tool-blur-brush", "Blur brush", ""),
    ("tool-smudge", "Smudge", ""),
    ("tool-liquify", "Liquify", ""),
    ("tool-shape-rect", "Rectangle shape", ""),
    ("tool-shape-ellipse", "Ellipse shape", ""),
    ("tool-line", "Line", ""),
    ("nudge-left", "Nudge left", "left"),
    ("nudge-right", "Nudge right", "right"),
    ("nudge-up", "Nudge up", "up"),
    ("nudge-down", "Nudge down", "down"),
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Shortcuts {
    pub overrides: BTreeMap<String, String>,
}
impl Shortcuts {
    pub fn chord(&self, id: &str) -> &str {
        self.overrides
            .get(id)
            .map(String::as_str)
            .unwrap_or_else(|| {
                DEFINITIONS
                    .iter()
                    .find(|d| d.0 == id)
                    .map(|d| d.2)
                    .unwrap_or("")
            })
    }
    pub fn assign(&mut self, id: &str, chord: &str) -> Result<()> {
        ensure!(DEFINITIONS.iter().any(|d| d.0 == id), "Unknown command");
        let parsed = gpui_kit::Keystroke::parse(chord).map_err(|e| anyhow::anyhow!("{e}"))?;
        let chord = parsed.unparse();
        ensure!(
            !["escape", "enter", "tab", "space"].contains(&chord.as_str()),
            "This key is reserved for dialog navigation or temporary panning"
        );
        ensure!(
            !parsed.modifiers.platform,
            "Super is reserved for the Linux desktop"
        );
        for (other, label, _) in DEFINITIONS {
            ensure!(
                *other == id || self.chord(other) != chord,
                "Already assigned to {label}"
            );
        }
        self.overrides.insert(id.into(), chord);
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            value.overrides.len() <= DEFINITIONS.len(),
            "Too many shortcut overrides"
        );
        // Validate the effective map as a whole, allowing exchanges of two defaults.
        let mut seen = std::collections::BTreeSet::new();
        for (id, _, _) in DEFINITIONS {
            let chord = value.chord(id);
            if chord.is_empty() {
                continue;
            }
            let parsed = gpui_kit::Keystroke::parse(chord).map_err(|e| anyhow::anyhow!("{e}"))?;
            ensure!(
                parsed.unparse() == chord
                    && !parsed.modifiers.platform
                    && !["escape", "enter", "tab", "space"].contains(&chord),
                "Invalid shortcut"
            );
            ensure!(seen.insert(chord), "Conflicting shortcuts");
        }
        ensure!(
            value
                .overrides
                .keys()
                .all(|k| DEFINITIONS.iter().any(|d| d.0 == k)),
            "Unknown shortcut command"
        );
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing settings directory"))?;
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".shortcuts-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)?;
            f.write_all(&serde_json::to_vec_pretty(self)?)?;
            f.sync_all()?;
            std::fs::rename(&temp, path)?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
}
pub fn settings_path() -> std::path::PathBuf {
    omuse::identity::config_file_path("shortcuts.json")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assignment_conflict_does_not_change_previous_binding() {
        let mut s = Shortcuts::default();
        assert!(s.assign("save", "ctrl-o").is_err());
        assert_eq!(s.chord("save"), "ctrl-s");
        assert!(s.assign("save", "super-s").is_err());
        assert!(s.assign("save", "escape").is_err());
    }
    #[test]
    fn saved_recording_reloads_and_corrupt_file_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("keys.json");
        let mut s = Shortcuts::default();
        s.assign("save", "ctrl-alt-s").unwrap();
        s.save(&p).unwrap();
        assert_eq!(Shortcuts::load(&p).unwrap().chord("save"), "ctrl-alt-s");
        std::fs::write(&p, r#"{"overrides":{"save":"ctrl-o"}}"#).unwrap();
        assert!(Shortcuts::load(&p).is_err());
    }
}
