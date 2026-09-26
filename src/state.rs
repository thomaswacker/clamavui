use crate::engine::scan::{ScanEvent, ScanLine};
use crate::engine::signatures::SignatureStatus;
use crate::engine::update::UpdateEvent;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BUSY: &str = "Es läuft bereits ein Scan oder Update.";

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub started: Instant,
    pub files_scanned: u64,
    pub current_path: Option<PathBuf>,
    /// True until the first line arrives (clamscan is loading the DB).
    pub loading: bool,
}

#[derive(Debug, Clone)]
pub enum Phase {
    Idle,
    Scanning(ScanProgress),
    Updating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: u64,
    pub path: PathBuf,
    pub signature: String,
    /// Last failed action's message, shown in the row.
    pub error: Option<String>,
    /// Inline rename buffer while the user edits the name.
    pub rename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOutcome {
    Clean,
    Infected,
    Incomplete,
    Aborted,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub duration: Duration,
    pub files_scanned: u64,
    pub findings: usize,
    pub outcome: ScanOutcome,
}

#[derive(Debug)]
pub struct Model {
    pub phase: Phase,
    pub targets: Vec<PathBuf>,
    pub findings: Vec<Finding>,
    pub issues: Vec<String>,
    pub summary: Option<ScanSummary>,
    pub update_log: Vec<String>,
    pub update_error: Option<String>,
    /// `None` until the first status read completes.
    pub signature_status: Option<SignatureStatus>,
    pub signature_error: Option<String>,
    next_finding_id: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            targets: Vec::new(),
            findings: Vec::new(),
            issues: Vec::new(),
            summary: None,
            update_log: Vec::new(),
            update_error: None,
            signature_status: None,
            signature_error: None,
            next_finding_id: 1,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    pub fn signatures_available(&self) -> bool {
        self.signature_status.as_ref().is_some_and(|s| !s.is_missing())
    }

    pub fn add_target(&mut self, path: PathBuf) {
        if !self.targets.contains(&path) {
            self.targets.push(path);
        }
    }

    pub fn remove_target(&mut self, index: usize) {
        if index < self.targets.len() {
            self.targets.remove(index);
        }
    }

    pub fn can_start_scan(&self, has_clamscan: bool) -> bool {
        self.is_idle() && has_clamscan && !self.targets.is_empty() && self.signatures_available()
    }

    pub fn can_start_update(&self, has_freshclam: bool) -> bool {
        self.is_idle() && has_freshclam
    }

    pub fn begin_scan(&mut self) -> Result<(), &'static str> {
        if !self.is_idle() {
            return Err(BUSY);
        }
        self.findings.clear();
        self.issues.clear();
        self.summary = None;
        self.phase = Phase::Scanning(ScanProgress {
            started: Instant::now(),
            files_scanned: 0,
            current_path: None,
            loading: true,
        });
        Ok(())
    }

    pub fn apply_scan_event(&mut self, event: ScanEvent) {
        let Phase::Scanning(progress) = &mut self.phase else {
            log::warn!("scan event {event:?} received while not scanning");
            return;
        };
        match event {
            ScanEvent::Started => {}
            ScanEvent::Line(line) => {
                progress.loading = false;
                match line {
                    ScanLine::Clean(path) => {
                        progress.files_scanned += 1;
                        progress.current_path = Some(path);
                    }
                    ScanLine::Found { path, signature } => {
                        progress.files_scanned += 1;
                        progress.current_path = Some(path.clone());
                        let id = self.next_finding_id;
                        self.next_finding_id += 1;
                        self.findings.push(Finding { id, path, signature, error: None, rename: None });
                    }
                    ScanLine::Error { path, message } => {
                        self.issues.push(format!("{}: {message}", path.display()));
                    }
                    ScanLine::Warning(message) => self.issues.push(message),
                    ScanLine::Other(text) => log::info!("clamscan: {text}"),
                }
            }
            ScanEvent::Finished { exit_code, duration } => {
                let outcome = match exit_code {
                    Some(0) => ScanOutcome::Clean,
                    Some(1) => ScanOutcome::Infected,
                    Some(code) => {
                        log::warn!("clamscan exit code {code}");
                        ScanOutcome::Incomplete
                    }
                    None => ScanOutcome::Failed("clamscan wurde durch ein Signal beendet".into()),
                };
                self.finish_scan(duration, outcome);
            }
            ScanEvent::Aborted => {
                let duration = progress.started.elapsed();
                self.finish_scan(duration, ScanOutcome::Aborted);
            }
            ScanEvent::SpawnFailed(msg) => {
                let duration = progress.started.elapsed();
                self.finish_scan(duration, ScanOutcome::Failed(msg));
            }
        }
    }

    fn finish_scan(&mut self, duration: Duration, outcome: ScanOutcome) {
        let files_scanned = match &self.phase {
            Phase::Scanning(p) => p.files_scanned,
            _ => 0,
        };
        self.summary = Some(ScanSummary { duration, files_scanned, findings: self.findings.len(), outcome });
        self.phase = Phase::Idle;
    }

    pub fn begin_update(&mut self) -> Result<(), &'static str> {
        if !self.is_idle() {
            return Err(BUSY);
        }
        self.update_log.clear();
        self.update_error = None;
        self.phase = Phase::Updating;
        Ok(())
    }

    /// Returns true when the update run is over (success or failure).
    pub fn apply_update_event(&mut self, event: UpdateEvent) -> bool {
        match event {
            UpdateEvent::Started => false,
            UpdateEvent::Line(line) => {
                self.update_log.push(line);
                false
            }
            UpdateEvent::Finished { exit_code } => {
                if exit_code != Some(0) {
                    self.update_error = Some(match exit_code {
                        Some(code) => format!("freshclam beendet mit Exit-Code {code}"),
                        None => "freshclam wurde durch ein Signal beendet".into(),
                    });
                }
                self.phase = Phase::Idle;
                true
            }
            UpdateEvent::SpawnFailed(msg) => {
                self.update_error = Some(format!("freshclam konnte nicht gestartet werden: {msg}"));
                self.phase = Phase::Idle;
                true
            }
        }
    }

    pub fn finding_mut(&mut self, id: u64) -> Option<&mut Finding> {
        self.findings.iter_mut().find(|f| f.id == id)
    }

    pub fn remove_finding(&mut self, id: u64) {
        self.findings.retain(|f| f.id != id);
    }

    pub fn set_finding_error(&mut self, id: u64, message: String) {
        if let Some(f) = self.finding_mut(id) {
            f.error = Some(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::signatures::DbFileInfo;
    use chrono::Utc;

    fn with_signatures() -> Model {
        let mut m = Model::new();
        m.signature_status = Some(SignatureStatus::from_files(vec![DbFileInfo {
            name: "daily".into(),
            version: 1,
            build_time: Utc::now(),
            signatures: 10,
        }]));
        m
    }

    fn found(path: &str) -> ScanEvent {
        ScanEvent::Line(ScanLine::Found { path: PathBuf::from(path), signature: "Eicar-Test-Signature".into() })
    }

    #[test]
    fn new_model_is_idle_and_cannot_scan() {
        let m = Model::new();
        assert!(m.is_idle());
        assert!(!m.can_start_scan(true));
        assert!(!m.signatures_available());
    }

    #[test]
    fn add_target_deduplicates() {
        let mut m = Model::new();
        m.add_target(PathBuf::from("/a"));
        m.add_target(PathBuf::from("/a"));
        m.add_target(PathBuf::from("/b"));
        assert_eq!(m.targets, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        m.remove_target(0);
        assert_eq!(m.targets, vec![PathBuf::from("/b")]);
    }

    #[test]
    fn can_start_scan_requires_everything() {
        let mut m = with_signatures();
        assert!(!m.can_start_scan(true), "no targets");
        m.add_target(PathBuf::from("/a"));
        assert!(!m.can_start_scan(false), "no clamscan");
        assert!(m.can_start_scan(true));
        m.signature_status = Some(SignatureStatus::default());
        assert!(!m.can_start_scan(true), "signatures missing");
    }

    #[test]
    fn begin_scan_clears_old_results_and_enters_scanning() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.findings.push(Finding { id: 99, path: "/old".into(), signature: "x".into(), error: None, rename: None });
        m.summary = Some(ScanSummary { duration: Duration::ZERO, files_scanned: 0, findings: 0, outcome: ScanOutcome::Clean });
        m.begin_scan().unwrap();
        assert!(matches!(m.phase, Phase::Scanning(ref p) if p.loading && p.files_scanned == 0));
        assert!(m.findings.is_empty());
        assert!(m.summary.is_none());
        assert_eq!(m.begin_scan(), Err("Es läuft bereits ein Scan oder Update."));
        assert_eq!(m.begin_update(), Err("Es läuft bereits ein Scan oder Update."));
    }

    #[test]
    fn scan_events_update_progress_and_findings() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(ScanEvent::Started);
        m.apply_scan_event(ScanEvent::Line(ScanLine::Clean(PathBuf::from("/a/ok.txt"))));
        m.apply_scan_event(found("/a/eicar.txt"));
        m.apply_scan_event(ScanEvent::Line(ScanLine::Error { path: "/a/locked".into(), message: "Can't open".into() }));
        m.apply_scan_event(ScanEvent::Line(ScanLine::Warning("/a/x: Can't access file".into())));
        match &m.phase {
            Phase::Scanning(p) => {
                assert!(!p.loading);
                assert_eq!(p.files_scanned, 2);
                assert_eq!(p.current_path, Some(PathBuf::from("/a/eicar.txt")));
            }
            other => panic!("unexpected phase {other:?}"),
        }
        assert_eq!(m.findings.len(), 1);
        assert_eq!(m.findings[0].id, 1);
        assert_eq!(m.issues, vec!["/a/locked: Can't open", "/a/x: Can't access file"]);

        m.apply_scan_event(ScanEvent::Finished { exit_code: Some(1), duration: Duration::from_secs(3) });
        assert!(m.is_idle());
        let s = m.summary.as_ref().unwrap();
        assert_eq!(s.outcome, ScanOutcome::Infected);
        assert_eq!(s.files_scanned, 2);
        assert_eq!(s.findings, 1);
    }

    #[test]
    fn exit_zero_is_clean() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(ScanEvent::Finished { exit_code: Some(0), duration: Duration::ZERO });
        assert_eq!(m.summary.as_ref().unwrap().outcome, ScanOutcome::Clean);
    }

    #[test]
    fn exit_two_marks_incomplete() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(ScanEvent::Finished { exit_code: Some(2), duration: Duration::ZERO });
        assert_eq!(m.summary.as_ref().unwrap().outcome, ScanOutcome::Incomplete);
        assert!(m.is_idle());
    }

    #[test]
    fn aborted_scan_ends_idle_with_aborted_summary() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(ScanEvent::Aborted);
        assert!(m.is_idle());
        assert_eq!(m.summary.as_ref().unwrap().outcome, ScanOutcome::Aborted);
    }

    #[test]
    fn spawn_failure_ends_idle_with_failed_summary() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(ScanEvent::SpawnFailed("no such file".into()));
        assert!(m.is_idle());
        assert_eq!(m.summary.as_ref().unwrap().outcome, ScanOutcome::Failed("no such file".into()));
    }

    #[test]
    fn update_events_log_and_finish() {
        let mut m = Model::new();
        assert!(m.can_start_update(true));
        assert!(!m.can_start_update(false));
        m.update_error = Some("old".into());
        m.begin_update().unwrap();
        assert!(matches!(m.phase, Phase::Updating));
        assert!(m.update_error.is_none());
        assert!(!m.apply_update_event(UpdateEvent::Started));
        assert!(!m.apply_update_event(UpdateEvent::Line("Downloading daily.cvd".into())));
        assert_eq!(m.update_log, vec!["Downloading daily.cvd"]);
        assert!(m.apply_update_event(UpdateEvent::Finished { exit_code: Some(0) }));
        assert!(m.is_idle());
        assert!(m.update_error.is_none());
    }

    #[test]
    fn update_failure_sets_error() {
        let mut m = Model::new();
        m.begin_update().unwrap();
        assert!(m.apply_update_event(UpdateEvent::Finished { exit_code: Some(1) }));
        assert_eq!(m.update_error.as_deref(), Some("freshclam beendet mit Exit-Code 1"));
        m.begin_update().unwrap();
        assert!(m.apply_update_event(UpdateEvent::SpawnFailed("boom".into())));
        assert_eq!(m.update_error.as_deref(), Some("freshclam konnte nicht gestartet werden: boom"));
        assert!(m.is_idle());
    }

    #[test]
    fn finding_helpers() {
        let mut m = with_signatures();
        m.add_target(PathBuf::from("/a"));
        m.begin_scan().unwrap();
        m.apply_scan_event(found("/a/1"));
        m.apply_scan_event(found("/a/2"));
        let id = m.findings[0].id;
        m.set_finding_error(id, "Keine Berechtigung".into());
        assert_eq!(m.finding_mut(id).unwrap().error.as_deref(), Some("Keine Berechtigung"));
        m.remove_finding(id);
        assert_eq!(m.findings.len(), 1);
        assert!(m.finding_mut(id).is_none());
    }
}
