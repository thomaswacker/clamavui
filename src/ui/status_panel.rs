use crate::engine::signatures::{freshness, Freshness};
use crate::state::{Model, Phase};
use crate::ui::format_thousands;
use chrono::{DateTime, Local, Utc};
use egui::{Color32, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAction {
    None,
    StartUpdate,
    Refresh,
}

fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, color);
}

fn status_line(model: &Model, now: DateTime<Utc>) -> (Color32, String) {
    if let Some(err) = &model.signature_error {
        return (Color32::GOLD, format!("Signaturstand unbekannt: {err}"));
    }
    let Some(status) = &model.signature_status else {
        return (Color32::GRAY, "Signaturstand wird gelesen…".into());
    };
    let level = freshness(status, now);
    let color = match level {
        Freshness::Fresh => Color32::from_rgb(0x2e, 0xb8, 0x5c),
        Freshness::Aging => Color32::GOLD,
        Freshness::Stale | Freshness::Missing => Color32::LIGHT_RED,
    };
    let text = match (level, status.newest_build) {
        (Freshness::Missing, _) | (_, None) => "Keine Signaturen vorhanden. Bitte „Aktualisieren“ ausführen.".to_string(),
        (_, Some(build)) => {
            let age_days = now.signed_duration_since(build).num_days();
            format!(
                "{} Signaturen, Stand {} ({} Tage alt)",
                format_thousands(status.total_signatures),
                build.with_timezone(&Local).format("%d.%m.%Y"),
                age_days
            )
        }
    };
    (color, text)
}

/// Signature status row with update button, a "check now" link and collapsible update log.
pub fn show(
    ui: &mut egui::Ui,
    model: &Model,
    now: DateTime<Utc>,
    can_update: bool,
    first_update_hint: bool,
) -> StatusAction {
    let mut action = StatusAction::None;
    ui.horizontal(|ui| {
        ui.heading("Signaturen");
        ui.add_space(8.0);
        let (color, text) = status_line(model, now);
        dot(ui, color);
        ui.label(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if matches!(model.phase, Phase::Updating) {
                ui.spinner();
                ui.label("Läuft…");
            } else if ui.add_enabled(can_update, egui::Button::new("Aktualisieren")).clicked() {
                action = StatusAction::StartUpdate;
            }
            if model.is_idle() && ui.link("Jetzt prüfen").clicked() {
                action = StatusAction::Refresh;
            }
        });
    });
    if first_update_hint && model.is_idle() {
        ui.label(RichText::new("Das erste Update lädt ca. 300 MB.").small());
    }
    if let Some(err) = &model.update_error {
        ui.colored_label(Color32::LIGHT_RED, format!("Update fehlgeschlagen: {err}"));
    }
    if !model.update_log.is_empty() {
        let open = matches!(model.phase, Phase::Updating) || model.update_error.is_some();
        egui::CollapsingHeader::new("Update-Protokoll")
            .default_open(open)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &model.update_log {
                            ui.monospace(line);
                        }
                    });
            });
    }
    action
}
