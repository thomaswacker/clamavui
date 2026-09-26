use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Platform directories used by the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub db_dir: PathBuf,
}

impl AppPaths {
    /// Resolve directories via `directories::ProjectDirs`. `None` only if no home dir is known.
    pub fn detect() -> Option<AppPaths> {
        let dirs = directories::ProjectDirs::from("", "", "clamavui")?;
        Some(AppPaths {
            config_dir: dirs.config_dir().to_path_buf(),
            db_dir: dirs.data_dir().join("db"),
        })
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn freshclam_conf(&self) -> PathBuf {
        self.config_dir.join("freshclam.conf")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.db_dir)
    }
}

/// User settings persisted as JSON. Empty path strings mean "auto-detect".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub clamscan_path: String,
    pub freshclam_path: String,
    pub sigtool_path: String,
    pub check_signatures_on_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clamscan_path: String::new(),
            freshclam_path: String::new(),
            sigtool_path: String::new(),
            check_signatures_on_start: true,
        }
    }
}

impl Settings {
    /// Load settings; a missing or unreadable file yields defaults (and a warning in the log).
    pub fn load(path: &Path) -> Settings {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("settings file {} is invalid: {e}", path.display());
                Settings::default()
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("settings file {} unreadable: {e}", path.display());
                Settings::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).expect("Settings is always serializable");
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_check_signatures_on_start() {
        let s = Settings::default();
        assert!(s.check_signatures_on_start);
        assert!(s.clamscan_path.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("settings.json");
        let s = Settings {
            clamscan_path: "/opt/bin/clamscan".into(),
            freshclam_path: String::new(),
            sigtool_path: "C:\\ClamAV\\sigtool.exe".into(),
            check_signatures_on_start: false,
        };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
    }

    #[test]
    fn load_missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(&dir.path().join("nope.json")), Settings::default());
    }

    #[test]
    fn load_invalid_json_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, "{ not json").unwrap();
        assert_eq!(Settings::load(&file), Settings::default());
    }

    #[test]
    fn load_partial_json_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, r#"{"clamscan_path": "/x/clamscan"}"#).unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.clamscan_path, "/x/clamscan");
        assert!(s.check_signatures_on_start);
    }

    #[test]
    fn app_paths_derive_files_from_config_dir() {
        let p = AppPaths {
            config_dir: PathBuf::from("/cfg"),
            db_dir: PathBuf::from("/data/db"),
        };
        assert_eq!(p.settings_file(), PathBuf::from("/cfg/settings.json"));
        assert_eq!(p.freshclam_conf(), PathBuf::from("/cfg/freshclam.conf"));
    }

    #[test]
    fn ensure_dirs_creates_both() {
        let dir = tempfile::tempdir().unwrap();
        let p = AppPaths {
            config_dir: dir.path().join("cfg"),
            db_dir: dir.path().join("data").join("db"),
        };
        p.ensure_dirs().unwrap();
        assert!(p.config_dir.is_dir());
        assert!(p.db_dir.is_dir());
    }
}
