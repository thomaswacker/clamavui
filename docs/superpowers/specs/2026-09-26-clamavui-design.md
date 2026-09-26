# ClamAV UI – Design-Spezifikation

Datum: 2026-09-26
Status: Entwurf, vom Auftraggeber im Gespräch abgenommen

## 1. Ziel und Zielgruppe

Eine grafische Desktop-Anwendung in Rust für Linux, Windows und macOS, die den Virenscanner ClamAV bedient. Zielgruppe sind der Autor und technische Nutzer, bei denen ClamAV (`clamscan`, `freshclam`, `sigtool`) bereits installiert ist. Die App bringt ClamAV nicht mit.

Die App kann:

1. den aktuellen Stand der Virensignaturen anzeigen,
2. ein Signatur-Update auslösen,
3. einzelne Dateien oder Ordner (rekursiv) scannen,
4. pro Befund eine Aktion anbieten: Löschen, In den Papierkorb, Umbenennen, Ignorieren.

UI-Sprache ist Deutsch. Es läuft höchstens ein Scan oder ein Update zur Zeit.

### Nicht enthalten (bewusst)

Zeitgesteuerte Scans, Echtzeitschutz, Quarantäne-Ordner, dauerhafte Allowlist, Scan-Historie, Mehrsprachigkeit, clamd-Unterstützung, Seeding der eigenen Signatur-DB aus der System-DB, Installer/Bundling, CI-Pipeline.

## 2. Getroffene Entscheidungen

| Frage | Entscheidung | Begründung |
|---|---|---|
| GUI-Toolkit | egui/eframe | Reines Rust, einfach, kleine Binary, plattformübergreifend |
| ClamAV-Anbindung | Subprozesse `clamscan`, `freshclam`, `sigtool` | Kein Build-Aufwand, keine C-Abhängigkeiten, ClamAV ist vorinstalliert |
| Signatur-DB | Eigenes Verzeichnis im Benutzerprofil | Keine Root-Rechte für Updates, gleicher Codepfad auf allen Plattformen |
| „Ignorieren" | Nur aus aktueller Liste entfernen | Kein gespeicherter Zustand nötig |

## 3. Architektur

Ein Cargo-Binary `clamavui`. Kernlogik (Modul `engine`, `actions`, `config`) hat keine egui-Abhängigkeit und ist per Unit-Test prüfbar. Die UI-Schicht (`app`, `ui/*`) setzt darauf auf.

```
src/
  main.rs            eframe-Start, Fenster, Theme, Logging-Init
  app.rs             App-Zustand, Zustandsautomat, Event-Verarbeitung, Views zusammensetzen
  ui/
    mod.rs
    status_panel.rs  Signaturstand + Update-Button + Update-Log
    scan_panel.rs    Zielauswahl, Start/Abbruch, Fortschritt
    results_panel.rs Befundliste mit Aktionen, Fehlerliste, Zusammenfassung
    settings.rs      Einstellungsfenster
  engine/
    mod.rs           ClamEngine { binaries: ClamBinaries, db_dir: PathBuf }
    locate.rs        Binaries finden (Einstellung → PATH → plattformtypische Orte)
    signatures.rs    DB-Status lesen (sigtool --info) und parsen
    scan.rs          clamscan starten, Zeilen streamen, parsen, abbrechen
    update.rs        freshclam.conf schreiben, freshclam starten, Ausgabe streamen
  actions.rs         Löschen, Papierkorb, Umbenennen
  config.rs          Plattformpfade, Einstellungen (JSON) laden/speichern
```

### Nebenläufigkeit

Lang laufende Aktionen (Scan, Update, Signaturstand lesen) laufen in einem `std::thread`. Der Worker sendet Ereignisse über `std::sync::mpsc::Sender<Event>` an die UI und ruft nach jedem Ereignis `egui::Context::request_repaint()` auf. Die UI leert den Kanal zu Beginn jedes Frames. Keine async-Runtime.

Abbruch eines Scans: Die UI hält ein `Arc<Mutex<Option<Child>>>` bzw. die Prozess-ID; „Abbrechen" ruft `Child::kill()`. Der Worker meldet danach `ScanEvent::Aborted`.

### Zustandsautomat (in `app.rs`, ohne egui-Abhängigkeit modelliert)

```
Idle ──Scan starten──▶ Scanning ──Fertig/Abbruch/Fehler──▶ Idle
Idle ──Update starten─▶ Updating ──Fertig/Fehler────────▶ Idle
```

Während `Scanning` oder `Updating` sind Start-Buttons und Befund-Aktionen deaktiviert. Ein Signaturstand-Refresh ist ein kurzer Worker, der keinen eigenen Zustand blockiert, aber nach einem Update automatisch ausgelöst wird.

### Crates

`eframe`, `egui`, `egui_extras` (Tabelle), `rfd` (Dateidialoge), `trash`, `directories`, `serde`, `serde_json`, `chrono`, `which`, `thiserror`, `log`, `env_logger`. Dev: `tempfile`.

## 4. ClamAV-Anbindung

### 4.1 Binaries finden (`engine/locate.rs`)

Für jedes der drei Werkzeuge in dieser Reihenfolge:

1. In den Einstellungen gesetzter Pfad (wenn nicht leer und Datei existiert),
2. `which::which(name)` im PATH,
3. plattformtypische Orte:
   - Linux: `/usr/bin`, `/usr/local/bin`
   - macOS: `/opt/homebrew/bin`, `/usr/local/bin`, `/opt/local/bin`
   - Windows: `C:\Program Files\ClamAV`, `C:\Program Files (x86)\ClamAV` (Dateiname mit `.exe`)

Ergebnis: `ClamBinaries { clamscan: Option<PathBuf>, freshclam: Option<PathBuf>, sigtool: Option<PathBuf> }`. Fehlt eines, zeigt die UI ein Banner mit Link zu den Einstellungen; abhängige Buttons sind deaktiviert (Scan braucht clamscan, Update braucht freshclam, Status braucht sigtool).

### 4.2 Signatur-DB (`config.rs`, `engine/signatures.rs`)

DB-Verzeichnis: `directories::ProjectDirs::from("", "", "clamavui").data_dir()/db`, also z. B.

- Linux: `~/.local/share/clamavui/db`
- macOS: `~/Library/Application Support/clamavui/db`
- Windows: `%APPDATA%\clamavui\db`

Das Verzeichnis wird beim Start angelegt, falls es fehlt.

Status lesen: Für jede der Dateien `main`, `daily`, `bytecode` wird `<name>.cld` bevorzugt, sonst `<name>.cvd` genommen. Darauf `sigtool --info <datei>` ausführen und parsen:

```
File: /…/daily.cld
Build time: 03 May 2026 06:24 +0000
Version: 27990
Signatures: 355446
```

Ergebnis pro Datei: `DbFileInfo { name, version: u32, build_time: DateTime<Utc>, signatures: u64 }`. Zusammengefasst: `SignatureStatus { files: Vec<DbFileInfo>, total_signatures: u64, newest_build: Option<DateTime<Utc>> }`. `newest_build` ist die Build-Zeit von `daily`, falls vorhanden, sonst die neueste über alle Dateien. Fehlt `main` und `daily`, gilt der Status als „keine Signaturen".

Ampel: grün < 2 Tage, gelb < 7 Tage, rot ≥ 7 Tage oder keine Signaturen. Warnzeilen von sigtool (`LibClamAV Warning: …`) werden beim Parsen ignoriert.

### 4.3 Update (`engine/update.rs`)

Beim ersten Update (oder wenn die Datei fehlt) schreibt die App `freshclam.conf` in `ProjectDirs.config_dir()`:

```
DatabaseMirror database.clamav.net
DatabaseDirectory <db_dir>
```

Aufruf: `freshclam --config-file=<conf> --datadir=<db_dir> --stdout`. stdout wird zeilenweise gelesen (Carriage Returns in Fortschrittszeilen werden als Zeilentrenner behandelt) und als `UpdateEvent::Line(String)` an die UI gesendet. Am Ende `UpdateEvent::Finished { exit_code }`. Exit-Code 0 = Erfolg (auch „already up-to-date"), sonst Fehler. Nach `Finished` löst die UI einen Signaturstand-Refresh aus.

Beim ersten Download sind rund 300 MB zu laden; die UI zeigt vor dem ersten Update den Hinweis „Erstes Update lädt ca. 300 MB".

### 4.4 Scan (`engine/scan.rs`)

Aufruf: `clamscan --database=<db_dir> --recursive --stdout --no-summary <ziel1> [<ziel2> …]`.

stdout wird zeilenweise geparst (`parse_scan_line(&str) -> Option<ScanLine>`):

| Zeile endet auf | Ergebnis |
|---|---|
| `: OK` | `ScanLine::Clean(path)` |
| `: <Signatur> FOUND` | `ScanLine::Found { path, signature }` |
| `: <Text> ERROR` | `ScanLine::Error { path, message }` |
| beginnt mit `WARNING: ` | `ScanLine::Warning(message)` |
| beginnt mit `LibClamAV Warning:` | ignoriert |
| sonst | `ScanLine::Other(line)` (ins Log) |

Der Pfad wird am **letzten** `: ` vor dem Schlüsselwort getrennt, damit Doppelpunkte im Pfad (Windows-Laufwerke, Dateinamen) nicht stören.

Ereignisse an die UI: `ScanEvent::Started`, `ScanEvent::Line(ScanLine)`, `ScanEvent::Finished { exit_code, duration }`, `ScanEvent::Aborted`, `ScanEvent::SpawnFailed(String)`.

Bedeutung des Exit-Codes: 0 sauber, 1 mindestens ein Befund, 2 Fehler aufgetreten (Ergebnisse werden trotzdem angezeigt, mit Warnung „Scan unvollständig"). Vor der ersten geparsten Zeile zeigt die UI die Phase „Signaturen laden…" an (clamscan braucht ~5 s zum Laden der DB).

## 5. UI

Ein Fenster, drei Bereiche untereinander, plus ein Einstellungsfenster.

### 5.1 Kopfbereich „Signaturen"

Eine Zeile: Ampelpunkt, Text „3.627.854 Signaturen, Stand 03.05.2026 (146 Tage alt)" bzw. „Keine Signaturen vorhanden", rechts Button „Aktualisieren". Während `Updating`: Button zeigt „Läuft…" mit Spinner, darunter aufklappbar das freshclam-Log (letzte Zeile immer sichtbar). Nach Abschluss aktualisiert sich die Zeile automatisch. Bei Fehler: rote Zeile mit Exit-Code, alter Stand bleibt stehen.

### 5.2 Mittelbereich „Scan"

Buttons „Datei wählen…" (Mehrfachauswahl) und „Ordner wählen…" öffnen native Dialoge (rfd). Darunter Liste der gewählten Ziele mit Entfernen-Kreuz pro Eintrag. Drag-and-drop von Dateien/Ordnern auf das Fenster fügt Ziele hinzu. Button „Scan starten" (deaktiviert ohne Ziele, ohne clamscan oder ohne Signaturen), während `Scanning` stattdessen „Abbrechen".

Fortschrittszeile: Spinner, Phase („Signaturen laden…" oder „Prüfe: <zuletzt gemeldeter Pfad>"), Zähler „N Dateien geprüft, M Befunde". Kein Prozentbalken, da die Gesamtzahl nicht vorab bekannt ist.

### 5.3 Unterer Bereich „Befunde"

Tabelle (egui_extras) mit Spalten Pfad, Signatur, Aktionen. Pro Zeile vier Buttons: „Löschen", „Papierkorb", „Umbenennen", „Ignorieren". Ist der Scan fertig und die Liste leer: „Keine Befunde" in grün. Darunter aufklappbar „Fehler/Warnungen (N)". Zusammenfassungszeile nach Abschluss: Dauer, geprüfte Dateien, Befunde, Status (Sauber / Befunde / Unvollständig / Abgebrochen).

Befund-Aktionen sind während `Scanning` deaktiviert.

### 5.4 Einstellungen

Zahnrad-Icon oben rechts öffnet ein egui-Fenster mit:

- drei Textfeldern für Binary-Pfade (leer = automatisch; daneben der aktuell aufgelöste Pfad oder „nicht gefunden"),
- DB-Verzeichnis (nur Anzeige) mit Button „Ordner öffnen",
- Checkbox „Beim Start Signaturstand prüfen" (Standard: an).

Gespeichert als JSON in `ProjectDirs.config_dir()/settings.json`. Änderungen an Binary-Pfaden lösen sofort eine neue Suche aus.

## 6. Befund-Aktionen (`actions.rs`)

| Aktion | Verhalten | Fehlerfall |
|---|---|---|
| Löschen | Modaler Bestätigungsdialog, dann `fs::remove_file` | Fehlertext rot in der Zeile (z. B. „Keine Berechtigung"), Zeile bleibt |
| Papierkorb | `trash::delete`, ohne Rückfrage | Fehlertext in der Zeile, Hinweis „Stattdessen löschen?" |
| Umbenennen | Inline-Textfeld, vorbelegt mit `<name>.infected`; Enter bestätigt, Esc bricht ab; `fs::rename` im selben Verzeichnis | Ziel existiert bereits → Fehler, kein Überschreiben; sonst OS-Fehler |
| Ignorieren | Zeile aus der Liste entfernen | keiner |

Erfolgreiche Aktionen entfernen die Zeile. Alle Dateioperationen laufen synchron im UI-Thread. Jeder Fehler wird zusätzlich per `log::warn!` ausgegeben.

## 7. Fehlerbehandlung

| Situation | Verhalten |
|---|---|
| Binary fehlt | Banner oben mit Link zu Einstellungen, abhängige Buttons deaktiviert |
| Keine Signaturen im DB-Verzeichnis | Scan-Button deaktiviert, Hinweis „Erst Signaturen laden" |
| freshclam Exit ≠ 0 | Fehlerzeile mit Exit-Code, Log bleibt aufklappbar, alter Stand bleibt |
| clamscan Exit 2 | Ergebnisse anzeigen, Warnung „Scan unvollständig", Fehlerliste aufklappen |
| Prozess nicht startbar | Fehlertext mit Binary-Pfad und OS-Fehler |
| sigtool-Ausgabe nicht parsebar | Status „unbekannt" (gelb), Rohausgabe im Log |

Logging über `log` + `env_logger` auf stderr, Level per `RUST_LOG`.

## 8. Tests

- **Parser (Unit, ohne ClamAV):** `parse_scan_line` mit `OK`, `FOUND`, `ERROR`, `WARNING`, Pfaden mit Doppelpunkt/Leerzeichen, Windows-Pfaden; `parse_sigtool_info` mit echter Beispielausgabe inkl. Warnzeilen; Ampel-Berechnung aus Build-Zeit; freshclam.conf-Erzeugung.
- **Binary-Suche (Unit):** Reihenfolge Einstellung → PATH → Standardorte mit gefaktem PATH und `tempfile`.
- **Aktionen (Unit):** Löschen und Umbenennen gegen `tempfile`-Verzeichnisse, inkl. Fehlerfälle (Ziel existiert, Datei fehlt). Papierkorb wird nicht automatisiert getestet.
- **Zustandsautomat (Unit):** Erlaubte und verbotene Übergänge.
- **Integration (`#[ignore]`, nur mit clamscan im PATH):** EICAR-Datei in Tempverzeichnis → genau ein Befund, Exit 1; nicht existierender Pfad → Exit 2.
- Keine automatisierten UI-Tests.

## 9. Build und Packaging

- Rust 2021, `cargo build --release` ohne Zusatzschritte. Linux-Systemabhängigkeiten (libxkbcommon, libgtk-3 für rfd, libssl nicht nötig) werden in der README genannt.
- Windows: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.
- Lizenz MIT. ClamAV wird nur als externer Prozess aufgerufen.
- Kein Installer, keine CI in dieser Version.
