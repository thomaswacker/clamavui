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

/// Default UI zoom; egui's 14 px base font is small on high-density displays.
pub const DEFAULT_ZOOM: f32 = 1.4;
pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 2.0;

/// Clamp a zoom factor into the supported range; NaN falls back to the default.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_nan() {
        DEFAULT_ZOOM
    } else {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

/// User settings persisted as JSON. Empty path strings mean "auto-detect".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub clamscan_path: String,
    pub freshclam_path: String,
    pub sigtool_path: String,
    pub check_signatures_on_start: bool,
    /// UI scale applied via `egui::Context::set_zoom_factor`.
    pub zoom_factor: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clamscan_path: String::new(),
            freshclam_path: String::new(),
            sigtool_path: String::new(),
            check_signatures_on_start: true,
            zoom_factor: DEFAULT_ZOOM,
        }
    }
}

impl Settings {
    /// Load settings; a missing or unreadable file yields defaults (and a warning in the log).
    pub fn load(path: &Path) -> Settings {
        let mut settings = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("settings file {} is invalid: {e}", path.display());
                Settings::default()
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("settings file {} unreadable: {e}", path.display());
                Settings::default()
            }
        };
        settings.zoom_factor = clamp_zoom(settings.zoom_factor);
        settings
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
    fn default_zoom_is_one_point_four() {
        assert_eq!(Settings::default().zoom_factor, DEFAULT_ZOOM);
        assert_eq!(DEFAULT_ZOOM, 1.4);
    }

    #[test]
    fn clamp_zoom_limits_to_valid_range() {
        assert_eq!(clamp_zoom(0.5), MIN_ZOOM);
        assert_eq!(clamp_zoom(3.0), MAX_ZOOM);
        assert_eq!(clamp_zoom(1.25), 1.25);
        assert_eq!(clamp_zoom(f32::NAN), DEFAULT_ZOOM);
    }

    #[test]
    fn zoom_roundtrips_and_is_clamped_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = Settings { zoom_factor: 1.6, ..Settings::default() };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file).zoom_factor, 1.6);

        fs::write(&file, r#"{"zoom_factor": 9.0}"#).unwrap();
        assert_eq!(Settings::load(&file).zoom_factor, MAX_ZOOM);
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
            zoom_factor: 1.25,
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
