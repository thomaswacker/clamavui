use crate::config::{AppPaths, Settings};
use crate::engine::locate::{ClamBinaries, Tool};
use egui::{Color32, RichText};

fn path_row(ui: &mut egui::Ui, tool: Tool, value: &mut String, resolved: Option<&std::path::Path>) -> bool {
    ui.label(tool.name());
    let resp = ui.add(
        egui::TextEdit::singleline(value)
            .hint_text("leer = automatisch suchen")
            .desired_width(280.0),
    );
    // Re-locate and save when editing ends, not on every keystroke.
    let committed = resp.lost_focus();
    match resolved {
        Some(p) => {
            ui.label(RichText::new(p.display().to_string()).small().weak());
        }
        None => {
            ui.colored_label(Color32::LIGHT_RED, "nicht gefunden");
        }
    }
    ui.end_row();
    committed
}

/// Settings window. Returns true when editing of a path field ended or the checkbox toggled
/// (caller re-locates binaries and saves).
pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    settings: &mut Settings,
    binaries: &ClamBinaries,
    paths: &AppPaths,
    save_error: Option<&str>,
) -> bool {
    let mut changed = false;
    egui::Window::new("Einstellungen")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Pfade zu den ClamAV-Programmen:");
            egui::Grid::new("binary_paths").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                changed |= path_row(ui, Tool::Clamscan, &mut settings.clamscan_path, binaries.get(Tool::Clamscan));
                changed |= path_row(ui, Tool::Freshclam, &mut settings.freshclam_path, binaries.get(Tool::Freshclam));
                changed |= path_row(ui, Tool::Sigtool, &mut settings.sigtool_path, binaries.get(Tool::Sigtool));
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Signatur-Verzeichnis:");
                ui.monospace(paths.db_dir.display().to_string());
                if ui.button("Ordner öffnen").clicked() {
                    if let Err(e) = opener::open(&paths.db_dir) {
                        log::warn!("could not open {}: {e}", paths.db_dir.display());
                    }
                }
            });
            ui.separator();
            changed |= ui
                .checkbox(&mut settings.check_signatures_on_start, "Beim Start Signaturstand prüfen")
                .changed();
            if let Some(err) = save_error {
                ui.separator();
                ui.colored_label(Color32::LIGHT_RED, err);
            }
        });
    changed
}
