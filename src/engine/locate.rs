use crate::config::Settings;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Clamscan,
    Freshclam,
    Sigtool,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Clamscan, Tool::Freshclam, Tool::Sigtool];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Clamscan => "clamscan",
            Tool::Freshclam => "freshclam",
            Tool::Sigtool => "sigtool",
        }
    }

    pub fn binary_name(self) -> String {
        if cfg!(windows) {
            format!("{}.exe", self.name())
        } else {
            self.name().to_string()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClamBinaries {
    pub clamscan: Option<PathBuf>,
    pub freshclam: Option<PathBuf>,
    pub sigtool: Option<PathBuf>,
}

impl ClamBinaries {
    pub fn get(&self, tool: Tool) -> Option<&Path> {
        match tool {
            Tool::Clamscan => self.clamscan.as_deref(),
            Tool::Freshclam => self.freshclam.as_deref(),
            Tool::Sigtool => self.sigtool.as_deref(),
        }
    }

    pub fn missing(&self) -> Vec<Tool> {
        Tool::ALL
            .iter()
            .copied()
            .filter(|t| self.get(*t).is_none())
            .collect()
    }
}

/// Resolution order: explicit override (if it exists) → PATH → platform fallback dirs.
pub fn find_tool(
    tool: Tool,
    override_path: &str,
    path_env: Option<&OsStr>,
    fallback_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let trimmed = override_path.trim();
    if !trimmed.is_empty() {
        let p = PathBuf::from(trimmed);
        if p.is_file() {
            return Some(p);
        }
        log::warn!("configured {} path {} does not exist, falling back", tool.name(), p.display());
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Ok(p) = which::which_in(tool.binary_name(), path_env, &cwd) {
        return Some(p);
    }
    fallback_dirs
        .iter()
        .map(|d| d.join(tool.binary_name()))
        .find(|p| p.is_file())
}

pub fn default_fallback_dirs() -> Vec<PathBuf> {
    let dirs: &[&str] = if cfg!(target_os = "windows") {
        &[r"C:\Program Files\ClamAV", r"C:\Program Files (x86)\ClamAV"]
    } else if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
    } else {
        &["/usr/bin", "/usr/local/bin"]
    };
    dirs.iter().map(PathBuf::from).collect()
}

pub fn locate_all(settings: &Settings) -> ClamBinaries {
    let path_env = std::env::var_os("PATH");
    let fallback = default_fallback_dirs();
    ClamBinaries {
        clamscan: find_tool(Tool::Clamscan, &settings.clamscan_path, path_env.as_deref(), &fallback),
        freshclam: find_tool(Tool::Freshclam, &settings.freshclam_path, path_env.as_deref(), &fallback),
        sigtool: find_tool(Tool::Sigtool, &settings.sigtool_path, path_env.as_deref(), &fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_exe(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    #[test]
    fn binary_name_has_exe_suffix_only_on_windows() {
        let n = Tool::Clamscan.binary_name();
        if cfg!(windows) {
            assert_eq!(n, "clamscan.exe");
        } else {
            assert_eq!(n, "clamscan");
        }
    }

    #[test]
    fn override_path_wins_when_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let custom = fake_exe(dir.path(), "my-clamscan");
        let path_dir = tempfile::tempdir().unwrap();
        fake_exe(path_dir.path(), &Tool::Clamscan.binary_name());
        let found = find_tool(
            Tool::Clamscan,
            custom.to_str().unwrap(),
            Some(path_dir.path().as_os_str()),
            &[],
        );
        assert_eq!(found, Some(custom));
    }

    #[test]
    fn missing_override_falls_back_to_path() {
        let path_dir = tempfile::tempdir().unwrap();
        let in_path = fake_exe(path_dir.path(), &Tool::Freshclam.binary_name());
        let found = find_tool(
            Tool::Freshclam,
            "/definitely/not/here/freshclam",
            Some(path_dir.path().as_os_str()),
            &[],
        );
        assert_eq!(found, Some(in_path));
    }

    #[test]
    fn fallback_dir_used_when_path_empty() {
        let empty = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let in_fallback = fake_exe(fallback.path(), &Tool::Sigtool.binary_name());
        let found = find_tool(
            Tool::Sigtool,
            "",
            Some(empty.path().as_os_str()),
            &[fallback.path().to_path_buf()],
        );
        assert_eq!(found, Some(in_fallback));
    }

    #[test]
    fn none_when_nowhere() {
        let empty = tempfile::tempdir().unwrap();
        let found = find_tool(Tool::Sigtool, "", Some(empty.path().as_os_str()), &[]);
        assert_eq!(found, None);
    }

    #[test]
    fn missing_lists_unset_tools() {
        let b = ClamBinaries {
            clamscan: Some(PathBuf::from("/x/clamscan")),
            freshclam: None,
            sigtool: None,
        };
        assert_eq!(b.missing(), vec![Tool::Freshclam, Tool::Sigtool]);
        assert_eq!(b.get(Tool::Clamscan), Some(Path::new("/x/clamscan")));
    }
}
