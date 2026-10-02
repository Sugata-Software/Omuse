//! Stable per-user paths and bounded migration from the previous application id.
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

pub const APP_ID: &str = "omuse";
pub const LEGACY_APP_ID: &str = "compositor-rust";

const CONFIG_FILES: &[(&str, u64)] = &[
    ("preferences.json", 16 * 1024),
    ("shortcuts.json", 256 * 1024),
    ("brush.json", 4 * 1024 * 1024),
];

pub fn env_var_os(name: &str) -> Option<OsString> {
    env_var_os_with(name, |key| std::env::var_os(key))
}

pub fn env_var(name: &str) -> Result<String, std::env::VarError> {
    env_var_os(name)
        .ok_or(std::env::VarError::NotPresent)?
        .into_string()
        .map_err(std::env::VarError::NotUnicode)
}

fn env_var_os_with(name: &str, mut get: impl FnMut(&str) -> Option<OsString>) -> Option<OsString> {
    get(name).or_else(|| {
        name.strip_prefix("OMUSE_")
            .and_then(|suffix| get(&format!("COMPOSITOR_{suffix}")))
    })
}

/// The user's home directory: `HOME`, or `USERPROFILE` on Windows.
pub fn home_dir() -> Option<PathBuf> {
    non_empty_var("HOME")
        .or_else(|| non_empty_var("USERPROFILE").filter(|_| cfg!(windows)))
        .map(PathBuf::from)
}

fn non_empty_var(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

/// An XDG override wins on every platform. Windows has no XDG defaults, so
/// it uses `windows_folder`: `APPDATA` for settings that roam with the
/// profile, `LOCALAPPDATA` for data, recovery and state kept on this machine.
fn home_child(variable: &str, fallback: &str, windows_folder: &str) -> PathBuf {
    non_empty_var(variable)
        .or_else(|| non_empty_var(windows_folder).filter(|_| cfg!(windows)))
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(fallback)))
        .unwrap_or_else(std::env::temp_dir)
}

pub fn config_dir() -> PathBuf {
    let config_home = home_child("XDG_CONFIG_HOME", ".config", "APPDATA");
    if !cfg!(test) {
        let state_home = home_child("XDG_STATE_HOME", ".local/state", "LOCALAPPDATA");
        let _ = migrate_config_files(&config_home, &state_home);
    }
    config_home.join(APP_ID)
}

pub fn config_file_path(name: &str) -> PathBuf {
    assert!(is_file_name(name), "Config path must be one file name");
    config_dir().join(name)
}

pub fn data_dir() -> PathBuf {
    home_child("XDG_DATA_HOME", ".local/share", "LOCALAPPDATA").join(APP_ID)
}

pub fn legacy_data_dir() -> PathBuf {
    home_child("XDG_DATA_HOME", ".local/share", "LOCALAPPDATA").join(LEGACY_APP_ID)
}

fn is_file_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn migrate_config_files(config_home: &Path, state_home: &Path) -> io::Result<()> {
    let marker = state_home
        .join(APP_ID)
        .join("migrations/config-from-compositor-rust-v1");
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.file_type().is_file() => return Ok(()),
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Config migration marker is not a regular file",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let canonical = config_home.join(APP_ID);
    let legacy = config_home.join(LEGACY_APP_ID);
    for (name, limit) in CONFIG_FILES {
        copy_file_once(&legacy.join(name), &canonical.join(name), *limit)?;
    }
    publish_marker(&marker)
}

fn copy_file_once(source: &Path, destination: &Path, limit: u64) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Legacy config is not a bounded regular file",
        ));
    }

    let source_file = open_read_nofollow(source)?;
    let opened = source_file.metadata()?;
    if !opened.is_file() || opened.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Legacy config changed during migration",
        ));
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    source_file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Legacy config is too large",
        ));
    }

    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing config directory"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".migration-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut output = create_private(&temp)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        match fs::hard_link(&temp, destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        crate::durable_fs::sync_path(parent)
    })();
    let _ = fs::remove_file(&temp);
    result
}

fn publish_marker(marker: &Path) -> io::Result<()> {
    let parent = marker
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing marker directory"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".migration-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = create_private(&temp)?;
        file.write_all(b"complete\n")?;
        file.sync_all()?;
        match fs::hard_link(&temp, marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        crate::durable_fs::sync_path(parent)
    })();
    let _ = fs::remove_file(temp);
    result
}

#[cfg(target_os = "linux")]
fn open_read_nofollow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(0o400000)
        .open(path)
}

#[cfg(not(target_os = "linux"))]
fn open_read_nofollow(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_environment_wins_and_legacy_is_only_a_fallback() {
        let get = |key: &str| match key {
            "OMUSE_MODE" => Some(OsString::from("new")),
            "COMPOSITOR_MODE" => Some(OsString::from("old")),
            _ => None,
        };
        assert_eq!(env_var_os_with("OMUSE_MODE", get), Some("new".into()));
        assert_eq!(
            env_var_os_with("OMUSE_OTHER", |key| (key == "COMPOSITOR_OTHER")
                .then(|| "old".into())),
            Some("old".into())
        );
    }

    #[test]
    fn migration_preserves_canonical_conflicts_and_does_not_resurrect_legacy() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        let state = temp.path().join("state");
        fs::create_dir_all(config.join(LEGACY_APP_ID)).unwrap();
        fs::create_dir_all(config.join(APP_ID)).unwrap();
        fs::write(
            config.join(LEGACY_APP_ID).join("preferences.json"),
            b"legacy prefs",
        )
        .unwrap();
        fs::write(
            config.join(LEGACY_APP_ID).join("shortcuts.json"),
            b"legacy keys",
        )
        .unwrap();
        fs::write(
            config.join(LEGACY_APP_ID).join("brush.json"),
            b"legacy brush",
        )
        .unwrap();
        fs::write(
            config.join(APP_ID).join("preferences.json"),
            b"canonical prefs",
        )
        .unwrap();

        migrate_config_files(&config, &state).unwrap();
        assert_eq!(
            fs::read(config.join(APP_ID).join("preferences.json")).unwrap(),
            b"canonical prefs"
        );
        assert_eq!(
            fs::read(config.join(APP_ID).join("shortcuts.json")).unwrap(),
            b"legacy keys"
        );
        assert_eq!(
            fs::read(config.join(APP_ID).join("brush.json")).unwrap(),
            b"legacy brush"
        );

        fs::remove_file(config.join(APP_ID).join("shortcuts.json")).unwrap();
        migrate_config_files(&config, &state).unwrap();
        assert!(
            !config.join(APP_ID).join("shortcuts.json").exists(),
            "a reset canonical file must not make the old override return"
        );
        assert!(config.join(LEGACY_APP_ID).join("shortcuts.json").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn migration_does_not_follow_legacy_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        let state = temp.path().join("state");
        fs::create_dir_all(config.join(LEGACY_APP_ID)).unwrap();
        let target = temp.path().join("unrelated");
        fs::write(&target, b"do not copy").unwrap();
        std::os::unix::fs::symlink(&target, config.join(LEGACY_APP_ID).join("preferences.json"))
            .unwrap();
        assert!(migrate_config_files(&config, &state).is_err());
        assert!(!config.join(APP_ID).join("preferences.json").exists());
        assert_eq!(fs::read(target).unwrap(), b"do not copy");
    }
}
