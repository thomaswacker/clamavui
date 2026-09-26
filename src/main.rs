#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clamavui::app::ClamApp;
use clamavui::config::AppPaths;

fn main() -> eframe::Result {
    env_logger::init();
    let paths = AppPaths::detect().unwrap_or_else(|| {
        log::error!("no home directory found, using current directory");
        AppPaths {
            config_dir: std::path::PathBuf::from("clamavui-config"),
            db_dir: std::path::PathBuf::from("clamavui-db"),
        }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ClamAV UI")
        .with_app_id("clamavui")
        .with_inner_size([1000.0, 780.0])
        .with_min_inner_size([720.0, 520.0])
        .with_drag_and_drop(true);
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("window icon could not be decoded: {e}"),
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ClamAV UI",
        options,
        Box::new(move |cc| Ok(Box::new(ClamApp::new(cc, paths)))),
    )
}
