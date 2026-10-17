mod app;
mod explorer;
mod metadata;
mod utils;
mod viewer;

use app::ImageViewerApp;
use eframe::egui;
use eframe::NativeOptions;
use std::env;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // Get the current directory or a path from command line arguments
    let mut initial_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if let Some(arg) = env::args().nth(1) {
        initial_dir = PathBuf::from(arg);
    }

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Image Viewer",
        options,
        Box::new(|cc| Ok(Box::new(ImageViewerApp::new(cc, initial_dir)))),
    )
}
