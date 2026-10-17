use crate::utils::{FileInfo, FileType};
use std::path::PathBuf;
use egui::{vec2, Color32, RichText, ScrollArea, Slider, Ui};

pub struct Explorer {
    pub current_dir: PathBuf,
    pub entries: Vec<FileInfo>,
    pub thumbnail_size: f32,
}

impl Explorer {
    pub fn new(initial_dir: PathBuf) -> Self {
        let entries = crate::utils::list_directory(&initial_dir);
        Self {
            current_dir: initial_dir,
            entries,
            thumbnail_size: 128.0,
        }
    }

    pub fn change_dir(&mut self, new_dir: PathBuf) {
        self.current_dir = new_dir;
        self.entries = crate::utils::list_directory(&self.current_dir);
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
            ui.spacing_mut().item_spacing = vec2(spacing, spacing);

            // Filter entries: only directories and images, and skip hidden files (starting with ".").
            let filtered_entries: Vec<&FileInfo> = self
                .entries
                .iter()
                .filter(|entry| {
                    let is_hidden = entry.name.starts_with('.');
                    let is_valid_type =
                        entry.file_type == FileType::Directory || crate::utils::is_image(&entry.path);
                    !is_hidden && is_valid_type
                })
                .collect();

            // Each item is a fixed-size cell (thumbnail on top, name below).
            //
            // We deliberately avoid egui's `Grid`: Grid caches the previous
            // frame's column widths/row heights and uses them to *predict* cell
            // placement. With async image loads the cell size swings between the
            // 24px loading fallback and the real thumbnail size, so the cached
            // positions lag a frame and alternating cells get allocated on top of
            // each other -- that is the "every other thumbnails missing" bug.
            // `horizontal_wrapped` recomputes positions every frame with no cache,
            // and a fixed cell size keeps the layout stable.
            let thumb = self.thumbnail_size;
            let item = vec2(thumb, thumb + 20.0);

            ui.horizontal_wrapped(|ui| {
                for entry in &filtered_entries {
                    let _ = ui.allocate_ui_with_layout(
                        item,
                        egui::Layout::top_down(egui::Align::Center),
                        |cell| {
                            // Fill the whole cell with a dark background.
                            cell.painter()
                                .rect_filled(cell.max_rect(), 2.0, Color32::from_gray(30));

                            let mut clicked = false;
                            if entry.file_type == FileType::Directory {
                                let resp = cell.add(
                                    egui::Label::new(egui::RichText::new("📁").size(thumb * 0.5))
                                        .sense(egui::Sense::click()),
                                );
                                clicked |= resp.clicked();
                            } else {
                                // FileLoader resolves the raw path after trimming "file://";
                                // it does NOT percent-decode, so pass the absolute path as-is.
                                let path =
                                    std::fs::canonicalize(&entry.path).unwrap_or_else(|_| entry.path.clone());
                                let uri = format!("file://{}", path.to_string_lossy());
                                // Cap to the cell size (preserves aspect ratio, no upscale) so a
                                // large photo shrinks to fit instead of overflowing and vanishing.
                                let resp = cell.add(
                                    egui::Image::from_uri(uri.as_str())
                                        .max_size(vec2(thumb, thumb))
                                        .sense(egui::Sense::click())
                                        .alt_text(entry.name.clone()),
                                );
                                clicked |= resp.clicked();
                            }

                            // Filename under the thumbnail.
                            let name_resp = cell.add(
                                egui::Label::new(RichText::new(&entry.name).size(12.0))
                                    .sense(egui::Sense::click()),
                            );
                            clicked |= name_resp.clicked();

                            if clicked {
                                if entry.file_type == FileType::Directory {
                                    action = Some(ExplorerAction::EnterDirectory(entry.path.clone()));
                                } else {
                                    action = Some(ExplorerAction::OpenFile(entry.path.clone()));
                                }
                            }
                        },
                    );
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
