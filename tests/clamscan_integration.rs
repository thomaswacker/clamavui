//! Runs only with `cargo test -- --ignored` and a working clamscan + signature DB.
//! DB dir: env `CLAMAVUI_TEST_DB`, default `/var/lib/clamav`.
use clamavui::engine::scan::{start_scan, ScanEvent, ScanLine};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;

const EICAR: &str = "X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

fn db_dir() -> PathBuf {
    std::env::var_os("CLAMAVUI_TEST_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/clamav"))
}

fn run(targets: Vec<PathBuf>) -> Vec<ScanEvent> {
    let clamscan = which::which("clamscan").expect("clamscan in PATH");
    let (tx, rx) = channel();
    let _handle = start_scan(clamscan, db_dir(), targets, tx, || {});
    let mut events = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(ev) => {
                let done = matches!(ev, ScanEvent::Finished { .. } | ScanEvent::Aborted | ScanEvent::SpawnFailed(_));
                events.push(ev);
                if done {
                    break;
                }
            }
            Err(e) => panic!("no event within timeout: {e}"),
        }
    }
    events
}

#[test]
#[ignore]
fn eicar_yields_exactly_one_finding_and_exit_one() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("eicar.txt"), EICAR).unwrap();
    std::fs::write(dir.path().join("ok.txt"), "harmless").unwrap();
    let events = run(vec![dir.path().to_path_buf()]);
    let found = events
        .iter()
        .filter(|e| matches!(e, ScanEvent::Line(ScanLine::Found { .. })))
        .count();
    assert_eq!(found, 1);
    assert!(matches!(events.last(), Some(ScanEvent::Finished { exit_code: Some(1), .. })));
}

#[test]
#[ignore]
fn nonexistent_target_exits_two() {
    let events = run(vec![PathBuf::from("/definitely/not/here")]);
    assert!(matches!(events.last(), Some(ScanEvent::Finished { exit_code: Some(2), .. })));
}
