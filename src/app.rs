use std::path::PathBuf;

use crate::explorer::{Explorer, ExplorerAction};
use crate::metadata::ImageMetadata;
use crate::viewer::{Viewer, ViewerAction};
use crate::utils::is_image;
use eframe::egui;

pub enum AppMode {
    Idle,
    Viewing(Viewer, Vec<PathBuf>),
}

pub struct ImageViewerApp {
    explorer: Explorer,
    mode: AppMode,

    // The "Image Details" panel (rendered inside the main window).
    show_metadata: bool,
    meta: Option<ImageMetadata>,
    meta_for: Option<PathBuf>,
}

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_dir: PathBuf) -> Self {
        // Initialize the image loader for egui_extras.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        Self {
            explorer: Explorer::new(initial_dir),
            mode: AppMode::Idle,
            show_metadata: false,
            meta: None,
            meta_for: None,
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Global shortcut: Cmd/Ctrl + I toggles the details panel (only when an image is open).
        let toggle = ui.input(|i| i.key_pressed(egui::Key::I) && i.modifiers.command);

        egui::CentralPanel::default().show(ui, |ui| {
            match &mut self.mode {
                AppMode::Idle => {
                    if let Some(action) = self.explorer.ui(ui) {
                        match action {
                            ExplorerAction::EnterDirectory(path) => {
                                self.explorer.change_dir(path.clone());
                            }
                            ExplorerAction::OpenFile(path) => {
                                // Collect images in the current directory to allow next/prev navigation.
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
                                let current_path = viewer.current_image.clone();
                                if let Some(idx) = images.iter().position(|p| p == &current_path) {
                                    if idx + 1 < images.len() {
                                        viewer.current_image = images[idx + 1].clone();
                                    }
                                }
                            }
                            ViewerAction::Previous => {
                                let current_path = viewer.current_image.clone();
                                if let Some(idx) = images.iter().position(|p| p == &current_path) {
                                    if idx > 0 {
                                        viewer.current_image = images[idx - 1].clone();
                                    }
                                }
                            }
                            ViewerAction::Close => {
                                self.mode = AppMode::Idle;
                                self.show_metadata = false;
                            }
                            ViewerAction::ToggleMetadata => {
                                self.show_metadata = !self.show_metadata;
                            }
                        }
                    }
                }
            }

            // Global Cmd/Ctrl + I toggle (only meaningful while an image is open).
            if toggle {
                if matches!(self.mode, AppMode::Viewing(..)) {
                    self.show_metadata = !self.show_metadata;
                }
            }
        });

        // Render the "Image Details" panel *inside* the main window as a floating Area,
        // only when requested and an image is currently open.
        if self.show_metadata {
            // Find the current image's path, or give up (and drop the flag) if nothing is open.
            let current = match &self.mode {
                AppMode::Viewing(viewer, _) => Some(viewer.current_image.clone()),
                AppMode::Idle => {
                    self.show_metadata = false;
                    None
                }
            };

            if let Some(current) = current {
                // Re-load metadata only when the current image changed.
                if self.meta_for.as_ref() != Some(&current) {
                    self.meta = Some(ImageMetadata::load(&current));
                    self.meta_for = Some(current.clone());
                }

                // Floating panel rendered on top of the viewer, within the same OS window.
                if let Some(meta) = self.meta.clone() {
                    egui::Area::new(egui::Id::new("image-details-panel"))
                        .order(egui::Order::Foreground)
                        .default_pos([16.0, 16.0])
                        .show(ui.ctx(), |ui| {
                            let mut closed = false;
                            egui::Window::new("Image Details")
                                .resizable(true)
                                .show(ui, |ui| {
                                    // Top row: a small close button on the far right.
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Min),
                                        |ui| {
                                            if ui.button(egui::RichText::new("✕").size(15.0)).clicked() {
                                                closed = true;
                                            }
                                        },
                                    );
                                    ui.separator();
                                    render_metadata_panel(ui, &meta);
                                });
                            if closed {
                                self.show_metadata = false;
                            }
                        });
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Metadata window content

fn render_metadata_panel(ui: &mut egui::Ui, meta: &ImageMetadata) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // File path as a heading.
            ui.label(egui::RichText::new(&meta.path).strong());

            if let Some(err) = &meta.decode_error {
                ui.colored_label(egui::Color32::YELLOW, &format!("⚠ {err}"));
                ui.separator();
            }

            ui.separator();
            ui.label(egui::RichText::new("Overview").strong());
            egui::Grid::new("meta_overview")
                .num_columns(2)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    meta_row(ui, "Format", &meta.format);
                    meta_row(
                        ui,
                        "Dimensions",
                        &format!("{} × {} pixels", meta.width, meta.height),
                    );
                    let mp = meta.width as f64 * meta.height as f64 / 1_000_000.0;
                    if mp > 0.0 {
                        meta_row(ui, "Megapixels", &format!("{mp:.2} MP"));
                    }
                    meta_row(ui, "Colour", &meta.color);
                    meta_row(ui, "Channels", &meta.channels.to_string());
                    meta_row(
                        ui,
                        "Bit depth",
                        &format!("{}/component (bits)", meta.bit_depth),
                    );
                    meta_row(ui, "File size", &crate::metadata::format_bytes(meta.file_size));
                    if let Some(m) = &meta.modified {
                        meta_row(ui, "Last modified", m);
                    }
                });

            ui.separator();
            if meta.exif.is_empty() {
                ui.label(
                    egui::RichText::new("EXIF data: none found").weak(),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("EXIF data ({} fields)", meta.exif.len())).strong(),
                );
                egui::Grid::new("meta_exif")
                    .num_columns(2)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        for (name, value) in &meta.exif {
                            ui.label(egui::RichText::new(name).strong());
                            ui.label(value);
                            ui.end_row();
                        }
                    });
            }

            ui.separator();
            ui.label(
                egui::RichText::new("Close: ⌘/Ctrl + I, the details button, or the panel's ✕.")
                    .weak()
                    .small(),
            );
        });
}

fn meta_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}
