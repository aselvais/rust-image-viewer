use crate::explorer::{Explorer, ExplorerAction};
use crate::viewer::{Viewer, ViewerAction};
use crate::utils::is_image;
use std::path::PathBuf;
use eframe::egui;

pub enum AppMode {
    Idle,
    Viewing(Viewer, Vec<PathBuf>),
}

pub struct ImageViewerApp {
    explorer: Explorer,
    mode: AppMode,
}

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_dir: PathBuf) -> Self {
        // Initialize the image loader for egui_extras
        egui_extras::install_image_loaders(&cc.egui_ctx);

        Self {
            explorer: Explorer::new(initial_dir),
            mode: AppMode::Idle,
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            match &mut self.mode {
                AppMode::Idle => {
                    if let Some(action) = self.explorer.ui(ui) {
                        match action {
                            ExplorerAction::EnterDirectory(path) => {
                                self.explorer.change_dir(path.clone());
                            }
                            ExplorerAction::OpenFile(path) => {
                                // Get all images in the current directory to allow navigation
                                let images: Vec<PathBuf> = crate::utils::list_directory(&self.explorer.current_dir)
                                    .into_iter()
                                    .filter(|info| info.file_type == crate::utils::FileType::File && is_image(&info.path))
                                    .map(|info| info.path)
                                    .collect();
                                
                                if !images.is_empty() {
                                    self.mode = AppMode::Viewing(Viewer::new(path.clone()), images);
                                }
                            }
                            ExplorerAction::None => {}
                        }
                    }
                }
                AppMode::Viewing(viewer, images) => {
                    if let Some(action) = viewer.ui(ui) {
                        match action {
                            ViewerAction::Next => {
                                let current_path = &viewer.current_image;
                                if let Some(idx) = images.iter().position(|p| p == current_path) {
                                    if idx + 1 < images.len() {
                                        viewer.current_image = images[idx + 1].clone();
                                    }
                                }
                            }
                            ViewerAction::Previous => {
                                let current_path = &viewer.current_image;
                                if let Some(idx) = images.iter().position(|p| p == current_path) {
                                    if idx > 0 {
                                        viewer.current_image = images[idx - 1].clone();
                                    }
                                }
                            }
                            ViewerAction::Close => {
                                self.mode = AppMode::Idle;
                            }
                        }
                    }
                }
            }
        });
    }
}
