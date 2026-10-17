// On Windows, build as a subsystem=WINDOWS GUI app so launching it doesn't pop a
// black console window. Gated to Windows only, so the macOS/Linux build is
// unaffected.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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
    // Decide where to start: a path dropped from the OS or passed on the
    // command line, falling back to the current working directory.
    let initial_dir = initial_path().unwrap_or_else(|| {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_icon(load_window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Image Viewer",
        options,
        Box::new(|cc| Ok(Box::new(ImageViewerApp::new(cc, initial_dir)))),
    )
}

/// Resolve the path this launch should start browsing/viewing.
///
/// macOS Launch Services injects a process-serial-number argument (`-psn_0_xxxx`)
/// into the first launch of a `.app`, so any argument starting with `-` is a flag
/// and is skipped. The first argument that names an existing file or directory
/// wins; when none exists we fall back to the current directory.
fn initial_path() -> Option<PathBuf> {
    env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-') && PathBuf::from(arg).exists())
        .map(PathBuf::from)
}

/// A small embedded image used as the OS window/taskbar icon.
///
/// `assets/AppIcon.png` is the app's artwork. Decoding it (the `image` crate is
/// already a dependency) keeps the repo free of generated binary resource files;
/// if it ever goes missing we fall back to no icon rather than failing to build.
fn load_window_icon() -> egui::IconData {
    match image::load_from_memory(include_bytes!("../assets/AppIcon.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                width,
                height,
                rgba: rgba.into_raw(),
            }
        }
        Err(_) => egui::IconData::default(),
    }
}
