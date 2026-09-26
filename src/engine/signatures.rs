use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("sigtool-Ausgabe unvollständig: Feld \u{201E}{0}\u{201D} fehlt")]
    MissingField(&'static str),
    #[error("sigtool-Ausgabe ungültig: {0}")]
    Invalid(String),
    #[error("sigtool konnte nicht gestartet werden ({path}): {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("sigtool meldete einen Fehler für {0}")]
    Failed(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbFileInfo {
    pub name: String,
    pub version: u32,
    pub build_time: DateTime<Utc>,
    pub signatures: u64,
}

/// Parse `sigtool --info <file>` output. Unknown lines (including LibClamAV warnings) are ignored.
pub fn parse_sigtool_info(output: &str) -> Result<DbFileInfo, SignatureError> {
    let mut name = None;
    let mut version = None;
    let mut build_time = None;
    let mut signatures = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "File" => {
                // Handle both Unix and Windows paths by splitting on both separators
                let filename = value
                    .split(['/', '\\'])
                    .next_back()
                    .unwrap_or("");
                name = Path::new(filename)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned());
            }
            "Build time" => {
                let parsed = DateTime::parse_from_str(value, "%d %b %Y %H:%M %z")
                    .map_err(|e| SignatureError::Invalid(format!("Build time \u{201E}{}\u{201D}: {}", value, e)))?;
                build_time = Some(parsed.with_timezone(&Utc));
            }
            "Version" => {
                version = Some(value.parse::<u32>().map_err(|e| {
                    SignatureError::Invalid(format!("Version \u{201E}{}\u{201D}: {}", value, e))
                })?);
            }
            "Signatures" => {
                signatures = Some(value.parse::<u64>().map_err(|e| {
                    SignatureError::Invalid(format!("Signatures \u{201E}{}\u{201D}: {}", value, e))
                })?);
            }
            _ => {}
        }
    }
    Ok(DbFileInfo {
        name: name.ok_or(SignatureError::MissingField("File"))?,
        version: version.ok_or(SignatureError::MissingField("Version"))?,
        build_time: build_time.ok_or(SignatureError::MissingField("Build time"))?,
        signatures: signatures.ok_or(SignatureError::MissingField("Signatures"))?,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignatureStatus {
    pub files: Vec<DbFileInfo>,
    pub total_signatures: u64,
    pub newest_build: Option<DateTime<Utc>>,
}

impl SignatureStatus {
    pub fn from_files(files: Vec<DbFileInfo>) -> Self {
        let total_signatures = files.iter().map(|f| f.signatures).sum();
        let daily = files.iter().find(|f| f.name == "daily").map(|f| f.build_time);
        let newest_build = daily.or_else(|| files.iter().map(|f| f.build_time).max());
        Self { files, total_signatures, newest_build }
    }

    /// True when neither `main` nor `daily` is present: clamscan cannot run.
    pub fn is_missing(&self) -> bool {
        !self.files.iter().any(|f| f.name == "main" || f.name == "daily")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Aging,
    Stale,
    Missing,
}

pub fn freshness(status: &SignatureStatus, now: DateTime<Utc>) -> Freshness {
    if status.is_missing() {
        return Freshness::Missing;
    }
    match status.newest_build {
        None => Freshness::Missing,
        Some(build) => {
            let age = now.signed_duration_since(build);
            if age < Duration::days(2) {
                Freshness::Fresh
            } else if age < Duration::days(7) {
                Freshness::Aging
            } else {
                Freshness::Stale
            }
        }
    }
}

pub const DB_NAMES: [&str; 3] = ["main", "daily", "bytecode"];

/// Prefer the incremental `.cld` over the packed `.cvd`.
pub fn db_file(db_dir: &Path, name: &str) -> Option<PathBuf> {
    ["cld", "cvd"]
        .iter()
        .map(|ext| db_dir.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
}

/// Run `sigtool --info` for every present DB file. An empty dir yields a "missing" status.
pub fn read_status(sigtool: &Path, db_dir: &Path) -> Result<SignatureStatus, SignatureError> {
    let mut files = Vec::new();
    for name in DB_NAMES {
        let Some(file) = db_file(db_dir, name) else { continue };
        let output = crate::engine::quiet_command(sigtool)
            .arg("--info")
            .arg(&file)
            .output()
            .map_err(|source| SignatureError::Spawn { path: sigtool.to_path_buf(), source })?;
        if !output.status.success() {
            log::warn!("sigtool --info {} failed: {}", file.display(), String::from_utf8_lossy(&output.stderr));
            return Err(SignatureError::Failed(file));
        }
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        let info = parse_sigtool_info(&text)
            .inspect_err(|_| log::warn!("unparsable sigtool output for {}:\n{text}", file.display()))?;
        files.push(info);
    }
    Ok(SignatureStatus::from_files(files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const DAILY: &str = "File: /var/lib/clamav/daily.cld
Build time: 03 May 2026 06:24 +0000
Version: 27990
Signatures: 355446
Functionality level: 90
Builder: svc.clamav-publisher
Verification OK.
";

    #[test]
    fn parses_daily_info() {
        let info = parse_sigtool_info(DAILY).unwrap();
        assert_eq!(info.name, "daily");
        assert_eq!(info.version, 27990);
        assert_eq!(info.signatures, 355_446);
        assert_eq!(info.build_time, Utc.with_ymd_and_hms(2026, 5, 3, 6, 24, 0).unwrap());
    }

    #[test]
    fn parses_info_with_warning_lines() {
        let text = format!(
            "{DAILY}LibClamAV Warning: **************************************************
LibClamAV Warning: ***  The virus database is older than 7 days!  ***
LibClamAV Warning: **************************************************
"
        );
        let info = parse_sigtool_info(&text).unwrap();
        assert_eq!(info.version, 27990);
    }

    #[test]
    fn parses_windows_file_path() {
        let text = "File: C:\\Users\\me\\AppData\\Roaming\\clamavui\\db\\main.cvd
Build time: 16 Dec 2025 23:18 +0000
Version: 63
Signatures: 3287027
";
        let info = parse_sigtool_info(text).unwrap();
        assert_eq!(info.name, "main");
        assert_eq!(info.version, 63);
    }

    #[test]
    fn missing_field_is_error() {
        let err = parse_sigtool_info("File: /x/daily.cld\nVersion: 1\n").unwrap_err();
        assert!(matches!(err, SignatureError::MissingField(_)));
    }

    #[test]
    fn garbage_build_time_is_error() {
        let text = "File: /x/daily.cld\nBuild time: yesterday\nVersion: 1\nSignatures: 2\n";
        assert!(matches!(parse_sigtool_info(text).unwrap_err(), SignatureError::Invalid(_)));
    }

    fn info(name: &str, days_ago: i64, sigs: u64) -> DbFileInfo {
        DbFileInfo {
            name: name.into(),
            version: 1,
            build_time: Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap() - Duration::days(days_ago),
            signatures: sigs,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap()
    }

    #[test]
    fn status_sums_and_prefers_daily_build_time() {
        let s = SignatureStatus::from_files(vec![info("main", 200, 100), info("daily", 3, 50)]);
        assert_eq!(s.total_signatures, 150);
        assert_eq!(s.newest_build, Some(info("daily", 3, 0).build_time));
        assert!(!s.is_missing());
    }

    #[test]
    fn status_without_main_and_daily_is_missing() {
        let s = SignatureStatus::from_files(vec![info("bytecode", 1, 10)]);
        assert!(s.is_missing());
        assert_eq!(freshness(&s, now()), Freshness::Missing);
        assert_eq!(freshness(&SignatureStatus::default(), now()), Freshness::Missing);
    }

    #[test]
    fn freshness_thresholds() {
        let fresh = SignatureStatus::from_files(vec![info("daily", 1, 1)]);
        let aging = SignatureStatus::from_files(vec![info("daily", 5, 1)]);
        let stale = SignatureStatus::from_files(vec![info("daily", 7, 1)]);
        assert_eq!(freshness(&fresh, now()), Freshness::Fresh);
        assert_eq!(freshness(&aging, now()), Freshness::Aging);
        assert_eq!(freshness(&stale, now()), Freshness::Stale);
    }

    #[test]
    fn db_file_prefers_cld_over_cvd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("daily.cvd"), "").unwrap();
        assert_eq!(db_file(dir.path(), "daily"), Some(dir.path().join("daily.cvd")));
        std::fs::write(dir.path().join("daily.cld"), "").unwrap();
        assert_eq!(db_file(dir.path(), "daily"), Some(dir.path().join("daily.cld")));
        assert_eq!(db_file(dir.path(), "main"), None);
    }

    #[test]
    fn read_status_on_empty_dir_is_missing_without_running_sigtool() {
        let dir = tempfile::tempdir().unwrap();
        let status = read_status(Path::new("/nonexistent/sigtool"), dir.path()).unwrap();
        assert!(status.is_missing());
    }
}
