use crate::actions::{delete_file, rename_file, suggested_rename, trash_file, ActionError};
use crate::config::{AppPaths, Settings};
use crate::engine::locate::{locate_all, ClamBinaries};
use crate::engine::scan::{start_scan, ScanEvent, ScanHandle};
use crate::engine::signatures::{read_status, SignatureStatus};
use crate::engine::update::{ensure_freshclam_conf, start_update, UpdateEvent};
use crate::state::Model;
use crate::ui::results_panel::{self, rename_field_id, FindingAction};
use crate::ui::scan_panel::{self, ScanAction};
use crate::ui::status_panel::{self, StatusAction};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

pub struct ClamApp {
    paths: AppPaths,
    settings: Settings,
    binaries: ClamBinaries,
    model: Model,
    scan_rx: Option<Receiver<ScanEvent>>,
    scan_handle: Option<ScanHandle>,
    update_rx: Option<Receiver<UpdateEvent>>,
    status_rx: Option<Receiver<Result<SignatureStatus, String>>>,
    show_settings: bool,
    confirm_delete: Option<u64>,
}

impl ClamApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: AppPaths) -> Self {
        if let Err(e) = paths.ensure_dirs() {
            log::error!("could not create app directories: {e}");
        }
        let settings = Settings::load(&paths.settings_file());
        let binaries = locate_all(&settings);
        log::info!("binaries: {binaries:?}");
        let mut app = Self {
            paths,
            settings,
            binaries,
            model: Model::new(),
            scan_rx: None,
            scan_handle: None,
            update_rx: None,
            status_rx: None,
            show_settings: false,
            confirm_delete: None,
        };
        if app.settings.check_signatures_on_start {
            app.refresh_status(&cc.egui_ctx);
        } else {
            app.model.signature_error = Some("Prüfung beim Start deaktiviert".into());
        }
        app
    }

    fn refresh_status(&mut self, ctx: &egui::Context) {
        let Some(sigtool) = self.binaries.sigtool.clone() else {
            self.model.signature_error = Some("sigtool nicht gefunden".into());
            return;
        };
        let db_dir = self.paths.db_dir.clone();
        let (tx, rx) = channel();
        self.status_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = read_status(&sigtool, &db_dir).map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn start_update(&mut self, ctx: &egui::Context) {
        let Some(freshclam) = self.binaries.freshclam.clone() else { return };
        let conf = self.paths.freshclam_conf();
        if let Err(e) = ensure_freshclam_conf(&conf, &self.paths.db_dir) {
            self.model.update_error = Some(format!("freshclam.conf konnte nicht geschrieben werden: {e}"));
            return;
        }
        if let Err(msg) = self.model.begin_update() {
            log::warn!("{msg}");
            return;
        }
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        let ctx = ctx.clone();
        start_update(freshclam, conf, self.paths.db_dir.clone(), tx, move || ctx.request_repaint());
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.scan_rx {
            for ev in rx.try_iter() {
                self.model.apply_scan_event(ev);
            }
            if self.model.is_idle() {
                self.scan_rx = None;
                self.scan_handle = None;
            }
        }
        let mut update_finished = false;
        if let Some(rx) = &self.update_rx {
            for ev in rx.try_iter() {
                update_finished |= self.model.apply_update_event(ev);
            }
        }
        if update_finished {
            self.update_rx = None;
            self.refresh_status(ctx);
        }
        if let Some(rx) = &self.status_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(status) => {
                        self.model.signature_status = Some(status);
                        self.model.signature_error = None;
                    }
                    Err(e) => self.model.signature_error = Some(e),
                }
                self.status_rx = None;
            }
        }
    }

    fn first_update_hint(&self) -> bool {
        !self.model.signatures_available()
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        let Some(clamscan) = self.binaries.clamscan.clone() else { return };
        if let Err(msg) = self.model.begin_scan() {
            log::warn!("{msg}");
            return;
        }
        let (tx, rx) = channel();
        self.scan_rx = Some(rx);
        let ctx = ctx.clone();
        self.scan_handle = Some(start_scan(
            clamscan,
            self.paths.db_dir.clone(),
            self.model.targets.clone(),
            tx,
            move || ctx.request_repaint(),
        ));
    }

    fn abort_scan(&self) {
        if let Some(handle) = &self.scan_handle {
            handle.abort();
        }
    }

    fn scan_missing_reason(&self) -> Option<&'static str> {
        if !self.model.is_idle() {
            Some("Scan oder Update läuft")
        } else if self.binaries.clamscan.is_none() {
            Some("clamscan nicht gefunden (siehe Einstellungen)")
        } else if !self.model.signatures_available() {
            Some("Erst Signaturen laden")
        } else if self.model.targets.is_empty() {
            Some("Ziel auswählen")
        } else {
            None
        }
    }

    fn collect_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect()
        });
        if !self.model.is_idle() {
            return;
        }
        for path in dropped {
            self.model.add_target(path);
        }
    }

    fn handle_scan_action(&mut self, action: ScanAction, ctx: &egui::Context) {
        match action {
            ScanAction::None => {}
            ScanAction::PickFiles => {
                if let Some(files) = rfd::FileDialog::new().set_title("Dateien wählen").pick_files() {
                    for f in files {
                        self.model.add_target(f);
                    }
                }
            }
            ScanAction::PickFolder => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Ordner wählen").pick_folder() {
                    self.model.add_target(dir);
                }
            }
            ScanAction::Start => self.start_scan(ctx),
            ScanAction::Abort => self.abort_scan(),
        }
    }

    fn handle_finding_action(&mut self, action: FindingAction, ctx: &egui::Context) {
        match action {
            FindingAction::Delete(id) => self.confirm_delete = Some(id),
            FindingAction::Trash(id) => {
                let Some(path) = self.model.finding_mut(id).map(|f| f.path.clone()) else { return };
                match trash_file(&path) {
                    Ok(()) => self.model.remove_finding(id),
                    Err(ActionError::Trash(msg)) => self.model.set_finding_error(
                        id,
                        format!("Papierkorb nicht verfügbar ({msg}). Stattdessen löschen?"),
                    ),
                    Err(e) => self.model.set_finding_error(id, e.to_string()),
                }
            }
            FindingAction::RenameStart(id) => {
                if let Some(f) = self.model.finding_mut(id) {
                    f.rename = Some(suggested_rename(&f.path));
                    f.error = None;
                    ctx.memory_mut(|m| m.request_focus(rename_field_id(id)));
                }
            }
            FindingAction::RenameCancel(id) => {
                if let Some(f) = self.model.finding_mut(id) {
                    f.rename = None;
                }
            }
            FindingAction::RenameCommit(id) => {
                let Some((path, new_name)) = self
                    .model
                    .finding_mut(id)
                    .and_then(|f| f.rename.clone().map(|n| (f.path.clone(), n)))
                else {
                    return;
                };
                match rename_file(&path, &new_name) {
                    Ok(_) => self.model.remove_finding(id),
                    Err(e) => self.model.set_finding_error(id, e.to_string()),
                }
            }
            FindingAction::Ignore(id) => self.model.remove_finding(id),
        }
    }

    fn execute_delete(&mut self, id: u64) {
        let Some(path) = self.model.finding_mut(id).map(|f| f.path.clone()) else { return };
        match delete_file(&path) {
            Ok(()) => self.model.remove_finding(id),
            Err(e) => self.model.set_finding_error(id, e.to_string()),
        }
    }

    fn show_delete_confirmation(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else { return };
        let Some(path) = self.model.finding_mut(id).map(|f| f.path.clone()) else {
            self.confirm_delete = None;
            return;
        };
        let modal = egui::Modal::new(egui::Id::new("confirm_delete")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Datei endgültig löschen?");
            ui.monospace(path.display().to_string());
            ui.label("Diese Aktion kann nicht rückgängig gemacht werden.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Löschen").clicked() {
                    self.execute_delete(id);
                    self.confirm_delete = None;
                }
                if ui.button("Abbrechen").clicked() {
                    self.confirm_delete = None;
                }
            });
        });
        if modal.should_close() {
            self.confirm_delete = None;
        }
    }
}

impl eframe::App for ClamApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Utc::now();
        egui::Panel::top(egui::Id::new("status_panel")).show(ui, |ui| {
            ui.add_space(4.0);
            let can_update = self.model.can_start_update(self.binaries.freshclam.is_some());
            match status_panel::show(ui, &self.model, now, can_update, self.first_update_hint()) {
                StatusAction::StartUpdate => self.start_update(&ctx),
                StatusAction::None => {}
            }
            ui.add_space(4.0);
        });
        self.collect_dropped_files(&ctx);
        egui::CentralPanel::default().show(ui, |ui| {
            let can_scan = self.model.can_start_scan(self.binaries.clamscan.is_some());
            let reason = self.scan_missing_reason();
            let action = scan_panel::show(ui, &mut self.model, can_scan, reason);
            self.handle_scan_action(action, &ctx);
            ui.separator();
            let actions_enabled = self.model.is_idle();
            let finding_actions = results_panel::show(ui, &mut self.model, actions_enabled);
            for action in finding_actions {
                self.handle_finding_action(action, &ctx);
            }
        });
        self.show_delete_confirmation(&ctx);
        // `show_settings` is wired in a later task.
        let _ = &self.show_settings;
    }
}
