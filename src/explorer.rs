use crate::utils::{FileInfo, FileType};
use std::path::PathBuf;
use std::collections::HashMap;
use egui::{Ui, ScrollArea, Grid, Slider, vec2, Align2, Color32, FontId, RichText};

pub struct Explorer {
    pub current_dir: PathBuf,
    pub entries: Vec<FileInfo>,
    pub thumbnail_size: f32,
    pub texture_cache: HashMap<PathBuf, egui::TextureHandle>,
}

impl Explorer {
    pub fn new(initial_dir: PathBuf) -> Self {
        let entries = crate::utils::list_directory(&initial_dir);
        Self {
            current_dir: initial_dir,
            entries,
            thumbnail_size: 128.0,
            texture_cache: HashMap::new(),
        }
    }

    pub fn change_dir(&mut self, new_dir: PathBuf) {
        self.current_dir = new_dir;
        self.entries = crate::utils::list_directory(&self.current_dir);
        self.texture_cache.clear();
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.change_dir(parent.to_path_buf());
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<ExplorerAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            if ui.button("⬆ Up").clicked() {
                self.go_up();
                action = Some(ExplorerAction::None);
            }
            ui.label(format!("Directory: {}", self.current_dir.display()));
            ui.separator();
            ui.label("Thumbnails:");
            ui.add(Slider::new(&mut self.thumbnail_size, 64.0..=256.0).step_by(8.0));
        });

        ui.separator();

        ScrollArea::vertical().show(ui, |ui| {
            let spacing = 16.0;
            let available_width = ui.available_width();

            // Filter entries: only directories and images, and skip hidden files (starting with ".")
            let filtered_entries: Vec<&FileInfo> = self.entries.iter()
                .filter(|entry| {
                    let is_hidden = entry.name.starts_with(".");
                    let is_valid_type = entry.file_type == FileType::Directory || crate::utils::is_image(&entry.path);
                    !is_hidden && is_valid_type
                })
                .collect();

            let num_cols = (available_width / (self.thumbnail_size + spacing)).floor().max(1.0) as usize;

            Grid::new("explorer_grid")
                .spacing([spacing, spacing])
                .min_col_width(self.thumbnail_size)
                .show(ui, |ui| {
                    for chunk in filtered_entries.chunks(num_cols) {
                        // 1. Draw the thumbnails row
                        for entry in chunk {
                            let item_size = vec2(self.thumbnail_size, self.thumbnail_size);
                            let (rect, response) = ui.allocate_at_least(item_size, egui::Sense::click());

                            // Draw background
                            ui.painter().rect_filled(rect, 2.0, Color32::from_gray(30));

                            if entry.file_type == FileType::Directory {
                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "📁",
                                    FontId::proportional(self.thumbnail_size * 0.5),
                                    Color32::WHITE,
                                );
                            } else if crate::utils::is_image(&entry.path) {
                                let texture = if let Some(tex) = self.texture_cache.get(&entry.path) {
                                    Some(tex.clone())
                                } else {
                                    if let Ok(img) = image::open(&entry.path) {
                                        let size = [img.width() as usize, img.height() as usize];
                                        let image_buffer = img.to_rgba8();
                                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                            size,
                                            &image_buffer,
                                        );
                                        let tex = ui.ctx().load_texture(
                                            entry.path.to_string_lossy().to_string(),
                                            color_image,
                                            Default::default(),
                                        );
                                        self.texture_cache.insert(entry.path.clone(), tex.clone());
                                        Some(tex)
                                    } else {
                                        None
                                    }
                                };

                                if let Some(tex) = texture {
                                     ui.allocate_ui(rect.size(), |ui| {
                                         ui.add(egui::Image::from_texture(&tex).shrink_to_fit());
                                     });
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        "❌",
                                        FontId::proportional(self.thumbnail_size * 0.5),
                                        Color32::WHITE,
                                    );
                                }
                            }

                            if response.clicked() {
                                if entry.file_type == FileType::Directory {
                                    action = Some(ExplorerAction::EnterDirectory(entry.path.clone()));
                                } else {
                                    action = Some(ExplorerAction::OpenFile(entry.path.clone()));
                                }
                            }
                        }
                        ui.end_row();

                        // 2. Draw the names row
                        for entry in chunk {
                            let name_label = RichText::new(&entry.name).size(12.0);
                            let label_res = ui.label(name_label);
                            if label_res.clicked() {
                                if entry.file_type == FileType::Directory {
                                    action = Some(ExplorerAction::EnterDirectory(entry.path.clone()));
                                } else {
                                    action = Some(ExplorerAction::OpenFile(entry.path.clone()));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });

        action
    }
}

pub enum ExplorerAction {
    None,
    EnterDirectory(PathBuf),
    OpenFile(PathBuf),
}

