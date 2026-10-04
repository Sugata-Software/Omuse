//! Per-user view choices, independent of artwork and isolated in tests.
use anyhow::{Result, ensure};
use omuse::canvas_grid::GridSettings;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub grid: bool,
    pub guides: bool,
    pub rulers: bool,
    pub snapping: bool,
    pub auto_select: bool,
    pub transform_box: bool,
    pub grid_spacing: u32,
    pub grid_subdivisions: u8,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            grid: false,
            guides: false,
            rulers: false,
            snapping: true,
            auto_select: true,
            transform_box: true,
            grid_spacing: GridSettings::default().spacing,
            grid_subdivisions: GridSettings::default().subdivisions,
        }
    }
}
impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        ensure!(
            file.metadata()?.len() <= 16_384,
            "Preferences file is too large"
        );
        let preferences: Self = serde_json::from_reader(file)?;
        GridSettings {
            spacing: preferences.grid_spacing,
            subdivisions: preferences.grid_subdivisions,
        }
        .validate()?;
        Ok(preferences)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing preferences directory"))?;
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".preferences-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)?;
            omuse::durable_fs::sync_path(parent)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
    pub fn current() -> Self {
        if cfg!(test) {
            Self::default()
        } else {
            Self::load(&settings_path()).unwrap_or_default()
        }
    }
    pub fn persist(&self) -> Result<()> {
        GridSettings {
            spacing: self.grid_spacing,
            subdivisions: self.grid_subdivisions,
        }
        .validate()?;
        if cfg!(test) {
            Ok(())
        } else {
            self.save(&settings_path())
        }
    }
}
pub fn settings_path() -> PathBuf {
    omuse::identity::config_file_path("preferences.json")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_preferences_keep_new_defaults_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"grid":true}"#).unwrap();
        let mut prefs = Preferences::load(&path).unwrap();
        assert!(prefs.grid && prefs.snapping && prefs.auto_select);
        prefs.snapping = false;
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), prefs);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn malformed_and_oversized_preferences_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"grid":"yes"}"#).unwrap();
        assert!(Preferences::load(&path).is_err());
        std::fs::write(&path, vec![b' '; 16_385]).unwrap();
        assert!(Preferences::load(&path).is_err());
        assert_eq!(Preferences::current(), Preferences::default());
    }
}
