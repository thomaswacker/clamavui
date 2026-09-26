use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("Datei nicht gefunden")]
    NotFound,
    #[error("Keine Berechtigung")]
    PermissionDenied,
    #[error("Ziel existiert bereits: {0}")]
    TargetExists(PathBuf),
    #[error("Ungültiger Dateiname")]
    InvalidName,
    #[error("Papierkorb nicht verfügbar: {0}")]
    Trash(String),
    #[error("{0}")]
    Io(String),
}

impl From<io::Error> for ActionError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => ActionError::NotFound,
            io::ErrorKind::PermissionDenied => ActionError::PermissionDenied,
            _ => ActionError::Io(e.to_string()),
        }
    }
}

pub fn delete_file(path: &Path) -> Result<(), ActionError> {
    fs::remove_file(path).map_err(Into::into)
}

pub fn trash_file(path: &Path) -> Result<(), ActionError> {
    if !path.exists() {
        return Err(ActionError::NotFound);
    }
    trash::delete(path).map_err(|e| ActionError::Trash(e.to_string()))
}

pub fn suggested_rename(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    format!("{name}.infected")
}

/// Rename within the same directory. Never overwrites.
pub fn rename_file(path: &Path, new_name: &str) -> Result<PathBuf, ActionError> {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name == "." || new_name == ".." || new_name.contains(['/', '\\']) {
        return Err(ActionError::InvalidName);
    }
    if !path.exists() {
        return Err(ActionError::NotFound);
    }
    let target = path.with_file_name(new_name);
    if target.exists() {
        return Err(ActionError::TargetExists(target));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, "x").unwrap();
        p
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = file(dir.path(), "a.txt");
        delete_file(&p).unwrap();
        assert!(!p.exists());
    }

    #[test]
    fn delete_missing_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(delete_file(&dir.path().join("nope")), Err(ActionError::NotFound)));
    }

    #[test]
    fn trash_missing_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(trash_file(&dir.path().join("nope")), Err(ActionError::NotFound)));
    }

    #[test]
    fn suggested_rename_appends_infected() {
        assert_eq!(suggested_rename(Path::new("/x/y/virus.exe")), "virus.exe.infected");
    }

    #[test]
    fn rename_moves_within_directory() {
        let dir = tempfile::tempdir().unwrap();
        let p = file(dir.path(), "a.txt");
        let target = rename_file(&p, "a.txt.infected").unwrap();
        assert_eq!(target, dir.path().join("a.txt.infected"));
        assert!(!p.exists());
        assert!(target.exists());
    }

    #[test]
    fn rename_refuses_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let p = file(dir.path(), "a.txt");
        let existing = file(dir.path(), "b.txt");
        assert!(matches!(rename_file(&p, "b.txt"), Err(ActionError::TargetExists(_))));
        assert!(p.exists());
        assert_eq!(fs::read_to_string(existing).unwrap(), "x");
    }

    #[test]
    fn rename_rejects_invalid_names() {
        let dir = tempfile::tempdir().unwrap();
        let p = file(dir.path(), "a.txt");
        for bad in ["", "   ", "..", ".", "sub/dir.txt", "sub\\dir.txt"] {
            assert!(matches!(rename_file(&p, bad), Err(ActionError::InvalidName)), "name {bad:?}");
        }
        assert!(p.exists());
    }

    #[test]
    fn rename_missing_source_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(rename_file(&dir.path().join("nope"), "x"), Err(ActionError::NotFound)));
    }

    #[test]
    fn errors_have_german_messages() {
        assert_eq!(ActionError::NotFound.to_string(), "Datei nicht gefunden");
        assert_eq!(ActionError::PermissionDenied.to_string(), "Keine Berechtigung");
    }
}
