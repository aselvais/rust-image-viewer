use std::path::PathBuf;
use egui::{Ui};

pub struct Viewer {
    pub current_image: PathBuf,
}

impl Viewer {
    pub fn new(image_path: PathBuf) -> Self {
        Self {
            current_image: image_path,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<ViewerAction> {
        let mut action = None;

        // Handle keyboard input
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            action = Some(ViewerAction::Previous);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            action = Some(ViewerAction::Next);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(ViewerAction::Close);
        }

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button("⬅ Back").clicked() {
                    action = Some(ViewerAction::Close);
                }
                ui.label(format!("Viewing: {}", self.current_image.file_name().unwrap_or_default().to_string_lossy()));
                ui.separator();
                if ui.button("Details (⌘I)").clicked() {
                    action = Some(ViewerAction::ToggleMetadata);
                }
            });

            ui.centered_and_justified(|ui| {
                // Display the image.
                // We use from_uri which can take a file path as a URI.
                let uri = format!("file://{}", self.current_image.to_string_lossy());
                let img = egui::Image::from_uri(uri);
                ui.add(img.shrink_to_fit());
            });
        });

        action
    }
}

pub enum ViewerAction {
    Next,
    Previous,
    Close,
    ToggleMetadata,
}
