use crate::state::{Model, Phase, ScanOutcome};
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingAction {
    Delete(u64),
    Trash(u64),
    RenameStart(u64),
    RenameCommit(u64),
    RenameCancel(u64),
    Ignore(u64),
}

/// Id of the inline rename field; the app requests focus for it when renaming starts.
pub fn rename_field_id(finding_id: u64) -> egui::Id {
    egui::Id::new(("rename_field", finding_id))
}

fn summary_text(model: &Model) -> Option<(Color32, String)> {
    let s = model.summary.as_ref()?;
    let (color, status) = match &s.outcome {
        ScanOutcome::Clean => (Color32::from_rgb(0x2e, 0xb8, 0x5c), "Sauber".to_string()),
        ScanOutcome::Infected => (Color32::LIGHT_RED, "Befunde".to_string()),
        ScanOutcome::Incomplete => (Color32::GOLD, "Scan unvollständig".to_string()),
        ScanOutcome::Aborted => (Color32::GRAY, "Abgebrochen".to_string()),
        ScanOutcome::Failed(msg) => (Color32::LIGHT_RED, format!("Fehlgeschlagen: {msg}")),
    };
    Some((
        color,
        format!(
            "{status} – {} Dateien in {:.1} s, {} Befunde",
            s.files_scanned,
            s.duration.as_secs_f64(),
            s.findings
        ),
    ))
}

/// Findings table with row actions, issues list and summary. Returns actions the app must execute.
pub fn show(ui: &mut egui::Ui, model: &mut Model, actions_enabled: bool) -> Vec<FindingAction> {
    let mut actions = Vec::new();
    ui.heading("Befunde");

    if let Some((color, text)) = summary_text(model) {
        ui.colored_label(color, text);
    }

    let scan_done = model.is_idle() && model.summary.is_some();
    if model.findings.is_empty() {
        if scan_done && matches!(model.summary.as_ref().map(|s| &s.outcome), Some(ScanOutcome::Clean)) {
            ui.colored_label(Color32::from_rgb(0x2e, 0xb8, 0x5c), "Keine Befunde");
        } else if matches!(model.phase, Phase::Scanning(_)) {
            ui.label(RichText::new("Noch keine Befunde.").italics());
        }
    } else {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let row_height = 24.0;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::auto().at_least(120.0))
            .column(Column::auto().at_least(320.0))
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Pfad");
                });
                header.col(|ui| {
                    ui.strong("Signatur");
                });
                header.col(|ui| {
                    ui.strong("Aktionen");
                });
            })
            .body(|mut body| {
                for finding in &mut model.findings {
                    let id = finding.id;
                    let height = if finding.error.is_some() { row_height * 2.0 } else { row_height };
                    body.row(height, |mut row| {
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                if let Some(buf) = &mut finding.rename {
                                    ui.add_enabled_ui(actions_enabled, |ui| {
                                        let resp = ui.add(
                                            egui::TextEdit::singleline(buf)
                                                .id(rename_field_id(id))
                                                .desired_width(f32::INFINITY),
                                        );
                                        // Single-line TextEdit surrenders focus on Enter/Escape.
                                        if actions_enabled && resp.lost_focus() && enter {
                                            actions.push(FindingAction::RenameCommit(id));
                                        } else if actions_enabled && resp.lost_focus() && escape {
                                            actions.push(FindingAction::RenameCancel(id));
                                        }
                                    });
                                } else {
                                    ui.monospace(finding.path.display().to_string())
                                        .on_hover_text(finding.path.display().to_string());
                                }
                                if let Some(err) = &finding.error {
                                    ui.add(egui::Label::new(RichText::new(err).color(Color32::LIGHT_RED)).truncate())
                                        .on_hover_text(err);
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.label(&finding.signature);
                        });
                        row.col(|ui| {
                            ui.add_enabled_ui(actions_enabled, |ui| {
                                if finding.rename.is_some() {
                                    if ui.button("Übernehmen").clicked() {
                                        actions.push(FindingAction::RenameCommit(id));
                                    }
                                    if ui.button("Abbrechen").clicked() {
                                        actions.push(FindingAction::RenameCancel(id));
                                    }
                                } else {
                                    if ui.button("Löschen").clicked() {
                                        actions.push(FindingAction::Delete(id));
                                    }
                                    if ui.button("Papierkorb").clicked() {
                                        actions.push(FindingAction::Trash(id));
                                    }
                                    if ui.button("Umbenennen").clicked() {
                                        actions.push(FindingAction::RenameStart(id));
                                    }
                                    if ui.button("Ignorieren").clicked() {
                                        actions.push(FindingAction::Ignore(id));
                                    }
                                }
                            });
                        });
                    });
                }
            });
    }

    if !model.issues.is_empty() {
        let open = matches!(model.summary.as_ref().map(|s| &s.outcome), Some(ScanOutcome::Incomplete));
        egui::CollapsingHeader::new(format!("Fehler/Warnungen ({})", model.issues.len()))
            .id_salt("issues_header")
            .default_open(open)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("issues")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for issue in &model.issues {
                            ui.monospace(issue);
                        }
                    });
            });
    }
    actions
}
