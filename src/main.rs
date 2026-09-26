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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ClamAV UI")
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([640.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "ClamAV UI",
        options,
        Box::new(move |cc| Ok(Box::new(ClamApp::new(cc, paths)))),
    )
}
