use crate::config::{AppPaths, Settings};
use crate::engine::locate::{locate_all, ClamBinaries};
use crate::engine::scan::{ScanEvent, ScanHandle};
use crate::engine::signatures::{read_status, SignatureStatus};
use crate::engine::update::{ensure_freshclam_conf, start_update, UpdateEvent};
use crate::state::Model;
use crate::ui::status_panel::{self, StatusAction};
use chrono::Utc;
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
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Scan-Bereich folgt.");
        });
        // `show_settings`, `confirm_delete` and `scan_handle` are wired in later tasks.
        let _ = (&self.show_settings, &self.confirm_delete, &self.scan_handle);
    }
}
