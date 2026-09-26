use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanLine {
    Clean(PathBuf),
    Found { path: PathBuf, signature: String },
    Error { path: PathBuf, message: String },
    Warning(String),
    Other(String),
}

/// Parse one clamscan stdout line. Returns `None` for lines that carry no information.
pub fn parse_scan_line(line: &str) -> Option<ScanLine> {
    let line = line.trim_end();
    if line.trim().is_empty() || line.starts_with("LibClamAV Warning:") {
        return None;
    }
    if let Some(msg) = line.strip_prefix("WARNING: ") {
        return Some(ScanLine::Warning(msg.to_string()));
    }
    if let Some(path) = line.strip_suffix(": OK") {
        return Some(ScanLine::Clean(PathBuf::from(path)));
    }
    if let Some(rest) = line.strip_suffix(" FOUND") {
        if let Some((path, signature)) = rest.rsplit_once(": ") {
            return Some(ScanLine::Found { path: PathBuf::from(path), signature: signature.to_string() });
        }
    }
    if let Some(rest) = line.strip_suffix(" ERROR") {
        if let Some((path, message)) = rest.rsplit_once(": ") {
            return Some(ScanLine::Error { path: PathBuf::from(path), message: message.to_string() });
        }
    }
    Some(ScanLine::Other(line.to_string()))
}

#[derive(Debug)]
pub enum ScanEvent {
    Started,
    Line(ScanLine),
    Finished { exit_code: Option<i32>, duration: Duration },
    Aborted,
    SpawnFailed(String),
}

/// Shared handle to the running clamscan process; `abort` kills it.
#[derive(Clone, Default)]
pub struct ScanHandle {
    child: Arc<Mutex<Option<Child>>>,
    aborted: Arc<AtomicBool>,
}

impl ScanHandle {
    /// Abort the scan. Safe to call at any time, including before `Started` is sent.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                if let Err(e) = child.kill() {
                    log::warn!("could not kill clamscan: {e}");
                }
            }
        }
    }
}

/// Spawn clamscan in a worker thread. Events go to `tx`; `notify` is called after each send
/// (the UI passes `ctx.request_repaint`).
pub fn start_scan(
    clamscan: PathBuf,
    db_dir: PathBuf,
    targets: Vec<PathBuf>,
    tx: Sender<ScanEvent>,
    notify: impl Fn() + Send + Sync + 'static,
) -> ScanHandle {
    let handle = ScanHandle::default();
    let worker = handle.clone();
    std::thread::spawn(move || {
        let send = |ev: ScanEvent| {
            let _ = tx.send(ev);
            notify();
        };
        let start = Instant::now();
        let mut cmd = crate::engine::quiet_command(&clamscan);
        cmd.arg(format!("--database={}", db_dir.display()))
            .arg("--recursive")
            .arg("--stdout")
            .arg("--no-summary")
            .args(&targets)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                send(ScanEvent::SpawnFailed(format!("{}: {e}", clamscan.display())));
                return;
            }
        };
        let stdout = child.stdout.take().expect("stdout is piped");
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::debug!("clamscan stderr: {line}");
                }
            });
        }
        {
            let mut guard = worker.child.lock().expect("scan child mutex");
            *guard = Some(child);
            if worker.aborted.load(Ordering::SeqCst) {
                if let Some(child) = guard.as_mut() {
                    if let Err(e) = child.kill() {
                        log::warn!("could not kill clamscan after early abort: {e}");
                    }
                }
            }
        }
        send(ScanEvent::Started);

        // If abort() ran before the child was stored, the process was already killed above;
        // skip reading so we do not wait on output.
        if !worker.aborted.load(Ordering::SeqCst) {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(parsed) = parse_scan_line(&line) {
                    send(ScanEvent::Line(parsed));
                }
            }
        } else {
            drop(stdout);
        }

        let child = worker.child.lock().expect("scan child mutex").take();
        let status = child.map(|mut c| c.wait());
        if worker.aborted.load(Ordering::SeqCst) {
            send(ScanEvent::Aborted);
            return;
        }
        match status {
            Some(Ok(st)) => send(ScanEvent::Finished { exit_code: st.code(), duration: start.elapsed() }),
            Some(Err(e)) => send(ScanEvent::SpawnFailed(format!("Warten auf clamscan fehlgeschlagen: {e}"))),
            None => send(ScanEvent::Aborted),
        }
    });
    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_line() {
        assert_eq!(
            parse_scan_line("/tmp/t/ok.txt: OK"),
            Some(ScanLine::Clean(PathBuf::from("/tmp/t/ok.txt")))
        );
    }

    #[test]
    fn parses_found_line() {
        assert_eq!(
            parse_scan_line("/tmp/t/eicar.txt: Eicar-Test-Signature FOUND"),
            Some(ScanLine::Found {
                path: PathBuf::from("/tmp/t/eicar.txt"),
                signature: "Eicar-Test-Signature".into()
            })
        );
    }

    #[test]
    fn parses_found_with_colon_in_path() {
        let line = "C:\\Users\\me\\a: b.txt: Win.Test.EICAR_HDB-1 FOUND";
        assert_eq!(
            parse_scan_line(line),
            Some(ScanLine::Found {
                path: PathBuf::from("C:\\Users\\me\\a: b.txt"),
                signature: "Win.Test.EICAR_HDB-1".into()
            })
        );
    }

    #[test]
    fn parses_windows_path_ok() {
        assert_eq!(
            parse_scan_line("C:\\Users\\me\\file.txt: OK\r"),
            Some(ScanLine::Clean(PathBuf::from("C:\\Users\\me\\file.txt")))
        );
    }

    #[test]
    fn parses_error_line() {
        assert_eq!(
            parse_scan_line("/root/secret: Can't open file or directory ERROR"),
            Some(ScanLine::Error {
                path: PathBuf::from("/root/secret"),
                message: "Can't open file or directory".into()
            })
        );
    }

    #[test]
    fn parses_warning_line() {
        assert_eq!(
            parse_scan_line("WARNING: /nonexistent: Can't access file"),
            Some(ScanLine::Warning("/nonexistent: Can't access file".into()))
        );
    }

    #[test]
    fn ignores_libclamav_warnings_and_blank_lines() {
        assert_eq!(parse_scan_line("LibClamAV Warning: *** The virus database is older than 7 days! ***"), None);
        assert_eq!(parse_scan_line(""), None);
        assert_eq!(parse_scan_line("   "), None);
    }

    #[test]
    fn unknown_line_is_other() {
        assert_eq!(
            parse_scan_line("/nonexistent/path: No such file or directory"),
            Some(ScanLine::Other("/nonexistent/path: No such file or directory".into()))
        );
    }

    #[test]
    fn abort_without_child_does_not_panic() {
        ScanHandle::default().abort();
    }

    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn collect(rx: &std::sync::mpsc::Receiver<ScanEvent>) -> Vec<ScanEvent> {
        let mut events = Vec::new();
        loop {
            let ev = rx.recv_timeout(Duration::from_secs(10)).expect("event within timeout");
            let done = matches!(ev, ScanEvent::Finished { .. } | ScanEvent::Aborted | ScanEvent::SpawnFailed(_));
            events.push(ev);
            if done {
                return events;
            }
        }
    }

    #[test]
    fn spawn_failure_reports_spawn_failed_and_notifies() {
        let (tx, rx) = channel();
        let notified = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&notified);
        let _h = start_scan(PathBuf::from("/definitely/not/a/binary"), PathBuf::from("/db"), vec![PathBuf::from("/x")], tx, move || flag.store(true, Ordering::SeqCst));
        let events = collect(&rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ScanEvent::SpawnFailed(_)));
        assert!(notified.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    fn fake_clamscan(dir: &std::path::Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-clamscan");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn worker_streams_lines_and_finishes_with_exit_code() {
        use std::path::Path;
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_clamscan(dir.path(), "echo '/a/ok.txt: OK'; echo '/a/bad.txt: Eicar-Test-Signature FOUND'; exit 1");
        let (tx, rx) = channel();
        let _h = start_scan(bin, PathBuf::from("/db"), vec![PathBuf::from("/a")], tx, || {});
        let events = collect(&rx);
        assert!(matches!(events[0], ScanEvent::Started));
        assert!(matches!(&events[1], ScanEvent::Line(ScanLine::Clean(p)) if p == Path::new("/a/ok.txt")));
        assert!(matches!(&events[2], ScanEvent::Line(ScanLine::Found { signature, .. }) if signature == "Eicar-Test-Signature"));
        assert!(matches!(events[3], ScanEvent::Finished { exit_code: Some(1), .. }));
        assert_eq!(events.len(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn abort_kills_running_scan_and_reports_aborted() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_clamscan(dir.path(), "exec sleep 30");
        let (tx, rx) = channel();
        let handle = start_scan(bin, PathBuf::from("/db"), vec![PathBuf::from("/a")], tx, || {});
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), ScanEvent::Started));
        let t = std::time::Instant::now();
        handle.abort();
        let events = collect(&rx);
        assert!(matches!(events.last(), Some(ScanEvent::Aborted)));
        assert!(t.elapsed() < Duration::from_secs(5), "abort must not wait for the process to finish naturally");
    }

    #[cfg(unix)]
    #[test]
    fn abort_before_started_still_kills_process() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_clamscan(dir.path(), "exec sleep 30");
        let (tx, rx) = channel();
        let handle = start_scan(bin, PathBuf::from("/db"), vec![PathBuf::from("/a")], tx, || {});
        let t = std::time::Instant::now();
        handle.abort(); // may run before the worker stored the child
        let events = collect(&rx);
        assert!(matches!(events.last(), Some(ScanEvent::Aborted)));
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
