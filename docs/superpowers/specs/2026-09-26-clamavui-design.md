# ClamAV UI – Design Specification

Date: 2026-09-26
Status: Draft, approved by the client in conversation

## 1. Goal and Target Audience

A graphical desktop application in Rust for Linux, Windows and macOS that operates the virus scanner ClamAV. The target audience is the author and technical users who already have ClamAV (`clamscan`, `freshclam`, `sigtool`) installed. The app does not bundle ClamAV.

The app can:

1. show the current status of the virus signatures,
2. trigger a signature update,
3. scan individual files or folders (recursively),
4. offer an action per finding: delete, move to trash, rename, ignore.

The UI language is German. At most one scan or one update runs at a time.

### Not included (deliberately)

Scheduled scans, real-time protection, quarantine folder, persistent allowlist, scan history, multi-language support, clamd support, seeding the app's own signature DB from the system DB, installer/bundling, CI pipeline.

## 2. Decisions Made

| Question | Decision | Rationale |
|---|---|---|
| GUI toolkit | egui/eframe | Pure Rust, simple, small binary, cross-platform |
| ClamAV integration | Subprocesses `clamscan`, `freshclam`, `sigtool` | No build overhead, no C dependencies, ClamAV is pre-installed |
| Signature DB | Own directory in the user profile | No root privileges needed for updates, same code path on all platforms |
| „Ignorieren" ("Ignore") | Only remove from the current list | No persisted state needed |

## 3. Architecture

A single Cargo binary `clamavui`. Core logic (modules `engine`, `actions`, `config`) has no egui dependency and is unit-testable. The UI layer (`app`, `ui/*`) builds on top of it.

```
src/
  main.rs            eframe start, window, theme, logging init
  app.rs             app state, state machine, event processing, assembling views
  ui/
    mod.rs
    status_panel.rs  signature status + update button + update log
    scan_panel.rs    target selection, start/cancel, progress
    results_panel.rs findings list with actions, error list, summary
    settings.rs      settings window
  engine/
    mod.rs           ClamEngine { binaries: ClamBinaries, db_dir: PathBuf }
    locate.rs        find binaries (setting → PATH → platform-typical locations)
    signatures.rs    read DB status (sigtool --info) and parse it
    scan.rs          start clamscan, stream lines, parse, abort
    update.rs        write freshclam.conf, start freshclam, stream output
  actions.rs         delete, trash, rename
  config.rs          platform paths, load/save settings (JSON)
```

### Concurrency

Long-running actions (scan, update, reading signature status) run in a `std::thread`. The worker sends events over `std::sync::mpsc::Sender<Event>` to the UI and calls `egui::Context::request_repaint()` after each event. The UI drains the channel at the start of every frame. No async runtime.

Aborting a scan: worker and UI share an `Arc<Mutex<Option<Child>>>`; „Abbrechen" ("Cancel") locks the mutex and calls `Child::kill()`. The worker then reports `ScanEvent::Aborted`.

### State Machine (in `app.rs`, modeled without an egui dependency)

```
Idle ──Scan starten──▶ Scanning ──Fertig/Abbruch/Fehler──▶ Idle
Idle ──Update starten─▶ Updating ──Fertig/Fehler────────▶ Idle
```

(„Scan starten" = start scan, „Fertig/Abbruch/Fehler" = done/abort/error, „Update starten" = start update, „Fertig/Fehler" = done/error)

While `Scanning` or `Updating`, start buttons and finding actions are disabled. A signature-status refresh is a short-lived worker that does not block a state of its own, but is triggered automatically after an update.

### Crates

`eframe`, `egui`, `egui_extras` (table), `rfd` (file dialogs), `trash`, `directories`, `serde`, `serde_json`, `chrono`, `which`, `thiserror`, `log`, `env_logger`. Dev: `tempfile`.

## 4. ClamAV Integration

### 4.1 Locating Binaries (`engine/locate.rs`)

For each of the three tools, in this order:

1. path set in the settings (if not empty and the file exists),
2. `which::which(name)` in PATH,
3. platform-typical locations:
   - Linux: `/usr/bin`, `/usr/local/bin`
   - macOS: `/opt/homebrew/bin`, `/usr/local/bin`, `/opt/local/bin`
   - Windows: `C:\Program Files\ClamAV`, `C:\Program Files (x86)\ClamAV` (filename with `.exe`)

Result: `ClamBinaries { clamscan: Option<PathBuf>, freshclam: Option<PathBuf>, sigtool: Option<PathBuf> }`. If one is missing, the UI shows a banner with a link to the settings; dependent buttons are disabled (scan needs clamscan, update needs freshclam, status needs sigtool).

### 4.2 Signature DB (`config.rs`, `engine/signatures.rs`)

DB directory: `directories::ProjectDirs::from("", "", "clamavui").data_dir()/db`, i.e. for example

- Linux: `~/.local/share/clamavui/db`
- macOS: `~/Library/Application Support/clamavui/db`
- Windows: `%APPDATA%\clamavui\data\db` (the `directories` crate appends `data` and `config` segments on Windows, so the config dir is `%APPDATA%\clamavui\config`)

The directory is created at startup if it is missing.

Reading status: for each of the files `main`, `daily`, `bytecode`, `<name>.cld` is preferred, otherwise `<name>.cvd` is used. Then `sigtool --info <datei>` (`<file>`) is run and parsed:

```
File: /…/daily.cld
Build time: 03 May 2026 06:24 +0000
Version: 27990
Signatures: 355446
```

Result per file: `DbFileInfo { name, version: u32, build_time: DateTime<Utc>, signatures: u64 }`. Aggregated: `SignatureStatus { files: Vec<DbFileInfo>, total_signatures: u64, newest_build: Option<DateTime<Utc>> }`. `newest_build` is the build time of `daily` if present, otherwise the newest across all files. If both `main` and `daily` are missing, the status counts as „keine Signaturen" ("no signatures"). 

Traffic light: green < 2 days, yellow < 7 days, red ≥ 7 days or no signatures. Warning lines from sigtool (`LibClamAV Warning: …`) are ignored during parsing.

### 4.3 Update (`engine/update.rs`)

On the first update (or if the file is missing), the app writes `freshclam.conf` into `ProjectDirs.config_dir()`:

```
DatabaseMirror database.clamav.net
DatabaseDirectory <db_dir>
```

Invocation: `freshclam --config-file=<conf> --datadir=<db_dir> --stdout`. stdout is read line by line (carriage returns in progress lines are treated as line separators) and sent to the UI as `UpdateEvent::Line(String)`. At the end, `UpdateEvent::Finished { exit_code }`. Exit code 0 = success (including "already up-to-date"), otherwise error. After `Finished`, the UI triggers a signature-status refresh.

On the first download, around 300 MB need to be fetched; before the first update the UI shows the note „Erstes Update lädt ca. 300 MB" ("First update downloads approx. 300 MB").

### 4.4 Scan (`engine/scan.rs`)

Invocation: `clamscan --database=<db_dir> --recursive --stdout --no-summary <ziel1> [<ziel2> …]`.

stdout is parsed line by line (`parse_scan_line(&str) -> Option<ScanLine>`):

| Line ends with | Result |
|---|---|
| `: OK` | `ScanLine::Clean(path)` |
| `: <Signatur> FOUND` | `ScanLine::Found { path, signature }` |
| `: <Text> ERROR` | `ScanLine::Error { path, message }` |
| starts with `WARNING: ` | `ScanLine::Warning(message)` |
| starts with `LibClamAV Warning:` | ignored |
| otherwise | `ScanLine::Other(line)` (into the log) |

The path is split at the **last** `: ` before the keyword, so that colons within the path (Windows drives, filenames) don't cause problems.

Events to the UI: `ScanEvent::Started`, `ScanEvent::Line(ScanLine)`, `ScanEvent::Finished { exit_code, duration }`, `ScanEvent::Aborted`, `ScanEvent::SpawnFailed(String)`.

Meaning of the exit code: 0 clean, 1 at least one finding, 2 an error occurred (results are shown anyway, with the warning „Scan unvollständig" ("Scan incomplete")). Before the first parsed line, the UI shows the phase „Signaturen laden…" ("Loading signatures…") (clamscan takes ~5 s to load the DB).

## 5. UI

One window, three areas stacked vertically, plus a settings window.

### 5.1 Header Area „Signaturen" ("Signatures")

One line: traffic-light dot, text „3.627.854 Signaturen, Stand 03.05.2026 (146 Tage alt)" ("3,627,854 signatures, as of 03.05.2026 (146 days old)") or „Keine Signaturen vorhanden" ("No signatures available"), on the right the button „Aktualisieren" ("Update"). During `Updating`: the button shows „Läuft…" ("Running…") with a spinner, below it an expandable freshclam log (last line always visible). After completion, the line updates itself automatically. On error: red line with exit code, the old status remains shown.

### 5.2 Middle Area „Scan"

Buttons „Datei wählen…" ("Choose file(s)…") (multi-selection) and „Ordner wählen…" ("Choose folder…") open native dialogs (rfd). Below that, a list of the chosen targets with a remove cross per entry. Drag-and-drop of files/folders onto the window adds targets. Button „Scan starten" ("Start scan") (disabled without targets, without clamscan, or without signatures); during `Scanning` it shows „Abbrechen" ("Cancel") instead.

Progress line: spinner, phase („Signaturen laden…" ("Loading signatures…") or „Prüfe: <zuletzt gemeldeter Pfad>" ("Checking: <last reported path>")), counter „N Dateien geprüft, M Befunde" ("N files checked, M findings"). No progress bar, since the total count is not known in advance.

### 5.3 Lower Area „Befunde" ("Findings")

Table (egui_extras) with columns path, signature, actions. Four buttons per row: „Löschen" ("Delete"), „Papierkorb" ("Trash"), „Umbenennen" ("Rename"), „Ignorieren" ("Ignore"). If the scan is finished and the list is empty: „Keine Befunde" ("No findings") in green. Below that, an expandable „Fehler/Warnungen (N)" ("Errors/warnings (N)"). Summary line after completion: duration, files checked, findings, status (Sauber / Befunde / Unvollständig / Abgebrochen) ("Clean / Findings / Incomplete / Aborted").

Finding actions are disabled during `Scanning`.

### 5.4 Settings

A gear icon in the top right opens an egui window with:

- three text fields for binary paths (empty = automatic; next to each, the currently resolved path or „nicht gefunden" ("not found")),
- DB directory (display only) with a button „Ordner öffnen" ("Open folder"),
- checkbox „Beim Start Signaturstand prüfen" ("Check signature status on startup") (default: on).

Saved as JSON in `ProjectDirs.config_dir()/settings.json`. Changes to binary paths immediately trigger a new search.

## 6. Finding Actions (`actions.rs`)

| Action | Behavior | Error case |
|---|---|---|
| Löschen (Delete) | Modal confirmation dialog, then `fs::remove_file` | Error text shown in red in the row (e.g. „Keine Berechtigung" ("No permission")), row stays |
| Papierkorb (Trash) | `trash::delete`, no confirmation prompt | Error text in the row, hint „Stattdessen löschen?" ("Delete instead?") |
| Umbenennen (Rename) | Inline text field, pre-filled with `<name>.infected`; Enter confirms, Esc cancels; `fs::rename` in the same directory | Target already exists → error, no overwrite; otherwise OS error |
| Ignorieren (Ignore) | Remove row from the list | none |

Successful actions remove the row. All file operations run synchronously on the UI thread. Every error is additionally logged via `log::warn!`.

## 7. Error Handling

| Situation | Behavior |
|---|---|
| Binary missing | Banner at the top with a link to settings, dependent buttons disabled |
| No signatures in the DB directory | Scan button disabled, hint „Erst Signaturen laden" ("Load signatures first") |
| freshclam exit ≠ 0 | Error line with exit code, log stays expandable, old status remains |
| clamscan exit 2 | Show results, warning „Scan unvollständig" ("Scan incomplete"), error list expanded |
| Process cannot start | Error text with binary path and OS error |
| sigtool output not parseable | Status „unbekannt" ("unknown") (yellow), raw output in the log |

Logging via `log` + `env_logger` to stderr, level via `RUST_LOG`.

## 8. Tests

- **Parser (unit, without ClamAV):** `parse_scan_line` with `OK`, `FOUND`, `ERROR`, `WARNING`, paths with colon/space, Windows paths; `parse_sigtool_info` with real sample output including warning lines; traffic-light computation from build time; freshclam.conf generation.
- **Binary search (unit):** order setting → PATH → default locations with a faked PATH and `tempfile`.
- **Actions (unit):** delete and rename against `tempfile` directories, including error cases (target exists, file missing). Trash is not tested in an automated way.
- **State machine (unit):** allowed and forbidden transitions.
- **Integration (`#[ignore]`, only with clamscan in PATH):** EICAR file in a temp directory → exactly one finding, exit 1; non-existent path → exit 2.
- No automated UI tests.

## 9. Build and Packaging

- Rust 2021, `cargo build --release` with no extra steps. Linux system dependencies (libxkbcommon, libgtk-3 for rfd) are noted in the README.
- Windows: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.
- License MIT. ClamAV is only invoked as an external process.
- No installer, no CI in this version.
