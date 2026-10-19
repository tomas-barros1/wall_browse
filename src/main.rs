mod app;
mod model;
mod provider;
mod storage;

use eframe::egui;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("WallBrowse")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([860.0, 560.0]);
    if let Ok(icon) = image::load_from_memory(include_bytes!("../assets/wallbrowse.png")) {
        let rgba = icon.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "WallBrowse",
        options,
        Box::new(|cc| Ok(Box::new(app::WallBrowseApp::new(cc)))),
    )
}
