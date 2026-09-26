use crate::state::{Model, Phase};
use egui::RichText;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAction {
    None,
    PickFiles,
    PickFolder,
    Start,
    Abort,
}

/// Target selection, start/abort and progress. `missing_reason` explains a disabled start button.
pub fn show(
    ui: &mut egui::Ui,
    model: &mut Model,
    can_scan: bool,
    missing_reason: Option<&str>,
) -> ScanAction {
    let mut action = ScanAction::None;
    let scanning = matches!(model.phase, Phase::Scanning(_));
    let busy = !model.is_idle();

    ui.heading("Scan");
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!busy, |ui| {
            if ui.button("Datei wählen…").clicked() {
                action = ScanAction::PickFiles;
            }
            if ui.button("Ordner wählen…").clicked() {
                action = ScanAction::PickFolder;
            }
        });
        ui.label(RichText::new("Dateien oder Ordner können auch ins Fenster gezogen werden.").small());
    });

    if model.targets.is_empty() {
        ui.label(RichText::new("Keine Ziele ausgewählt.").italics());
    } else {
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("targets")
            .max_height(120.0)
            .show(ui, |ui| {
                for (i, target) in model.targets.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!busy, egui::Button::new("✕").small()).clicked() {
                            remove = Some(i);
                        }
                        ui.monospace(target.display().to_string());
                    });
                }
            });
        if let Some(i) = remove {
            model.remove_target(i);
        }
    }

    ui.horizontal(|ui| {
        if scanning {
            if ui.button("Abbrechen").clicked() {
                action = ScanAction::Abort;
            }
        } else if ui.add_enabled(can_scan, egui::Button::new("Scan starten")).clicked() {
            action = ScanAction::Start;
        }
        if !scanning {
            if let Some(reason) = missing_reason {
                ui.label(RichText::new(reason).small().weak());
            }
        }
    });

    if let Phase::Scanning(progress) = &model.phase {
        ui.horizontal(|ui| {
            ui.spinner();
            if progress.loading {
                ui.label("Signaturen laden…");
            } else {
                let current = progress
                    .current_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                ui.label(format!("Prüfe: {current}"));
            }
        });
        ui.label(format!(
            "{} Dateien geprüft, {} Befunde",
            progress.files_scanned,
            model.findings.len()
        ));
    }
    action
}
