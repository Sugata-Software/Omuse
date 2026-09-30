//! A bounded project history. Linux paths are stored losslessly, including names
//! that are not UTF-8. No document contents or provider information are recorded.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

pub const LIMIT: usize = 10;
const MAX_FILE: u64 = 64 * 1024;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct RecentProjects {
    pub paths: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    paths: Vec<Vec<u8>>,
}

impl RecentProjects {
    pub fn load(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        match fs::File::open(path) {
            Ok(file) => {
                file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        }
        ensure!(
            bytes.len() as u64 <= MAX_FILE,
            "Recent-project history is too large"
        );
        let stored: Stored =
            serde_json::from_slice(&bytes).context("Invalid recent-project history")?;
        ensure!(
            stored.version == 1 && stored.paths.len() <= LIMIT,
            "Unsupported recent-project history"
        );
        let mut result = Self::default();
        for bytes in stored.paths {
            ensure!(
                bytes.len() <= 4096 && !bytes.contains(&0),
                "Invalid recent-project path"
            );
            let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
            ensure!(path.is_absolute(), "Recent-project paths must be absolute");
            if !result.paths.contains(&path) {
                result.paths.push(path);
            }
        }
        Ok(result)
    }

    pub fn note(&mut self, path: &Path) -> Result<()> {
        let path = path
            .canonicalize()
            .context("The recent project is no longer available")?;
        ensure!(
            path.is_dir()
                && (path.join("manifest.json").is_file() || path.join("project.json").is_file()),
            "Recent entry is not a project"
        );
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(LIMIT);
        Ok(())
    }

    pub fn refresh(&mut self) {
        self.paths.retain(|p| {
            p.is_dir() && (p.join("manifest.json").is_file() || p.join("project.json").is_file())
        });
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        ensure!(self.paths.len() <= LIMIT, "Too many recent projects");
        let parent = path.parent().context("History needs a parent directory")?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(&Stored {
            version: 1,
            paths: self
                .paths
                .iter()
                .map(|p| p.as_os_str().as_bytes().to_vec())
                .collect(),
        })?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE,
            "Recent-project history is too large"
        );
        let temporary = parent.join(format!(".recent-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(temporary);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_deduplicates_prunes_and_roundtrips_non_utf8_paths() {
        let temp = tempfile::tempdir().unwrap();
        let mut history = RecentProjects::default();
        for n in 0..12 {
            let path = temp.path().join(format!("{n}.omuse"));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("manifest.json"), "{}").unwrap();
            history.note(&path).unwrap();
        }
        assert_eq!(history.paths.len(), 10);
        assert_eq!(history.paths[0].file_name().unwrap(), "11.omuse");
        let legacy = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"Older-\xff.comp".to_vec()));
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join("manifest.json"), "{}").unwrap();
        history.note(&legacy).unwrap();
        history.note(&legacy.join(".")).unwrap();
        assert_eq!(history.paths.iter().filter(|p| **p == legacy).count(), 1);
        let file = temp.path().join("recent.json");
        history.save(&file).unwrap();
        assert_eq!(RecentProjects::load(&file).unwrap(), history);
        fs::remove_dir_all(&legacy).unwrap();
        history.refresh();
        assert!(!history.paths.contains(&legacy));
        let before = history.clone();
        assert!(history.note(&legacy).is_err());
        assert_eq!(history, before);
        RecentProjects::default().save(&file).unwrap();
        assert!(RecentProjects::load(&file).unwrap().paths.is_empty());
        assert!(
            !fs::read_dir(temp.path()).unwrap().any(|p| p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
    #[test]
    fn history_rejects_truncation_unknown_schema_and_oversized_data() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("recent.json");
        for bytes in [
            b"{".to_vec(),
            br#"{"version":2,"paths":[]}"#.to_vec(),
            br#"{"version":1,"paths":[[0]]}"#.to_vec(),
            vec![b' '; MAX_FILE as usize + 1],
        ] {
            fs::write(&file, &bytes).unwrap();
            assert!(RecentProjects::load(&file).is_err());
            assert_eq!(fs::read(&file).unwrap(), bytes);
        }
    }
}
