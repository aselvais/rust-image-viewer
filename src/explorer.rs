use crate::thumbnail::{self, Thumb};
use crate::utils::{FileInfo, FileType};
use egui::{vec2, Color32, RichText, ScrollArea, Slider, TextureHandle, Ui};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Explorer {
    pub current_dir: PathBuf,
    pub entries: Vec<FileInfo>,
    pub thumbnail_size: f32,

    /// Cached thumbnail state per *absolute* image path. Keyed by the canonical
    /// path so the same file browsed twice reuses one texture. Cleared on
    /// directory change so we don't leak textures for folders we've left.
    thumbs: HashMap<PathBuf, Thumb>,
    /// Monotonic counter giving each freshly-created thumbnail texture a unique
    /// name (`thumb-{n}`), so the egui texture registry never collides.
    tex_seq: u64,
}

/// Render-ready data for a single grid cell, copied out of the live thumbnail
/// cache just before painting. Decoupling render from the cache means the paint
/// loop never borrows the cache while it's being advanced the same frame.
enum CellView {
    Directory {
        name: String,
        path: PathBuf,
    },
    Image {
        name: String,
        path: PathBuf,
        image: CellImage,
    },
}

/// What the cell should paint for its image area.
enum CellImage {
    /// A ready texture (a cheap `retain`-only clone of the cached handle).
    Texture(TextureHandle),
    /// Still decoding / waiting its turn — show a "loading" placeholder.
    Loading,
    /// Couldn't be previewed, with the reason to show on hover.
    Broken(String),
}

impl Explorer {
    pub fn new(initial_dir: PathBuf) -> Self {
        let entries = crate::utils::list_directory(&initial_dir);
        Self {
            current_dir: initial_dir,
            entries,
            thumbnail_size: 128.0,
            thumbs: HashMap::new(),
            tex_seq: 0,
        }
    }

    pub fn change_dir(&mut self, new_dir: PathBuf) {
        self.current_dir = new_dir;
        self.entries = crate::utils::list_directory(&self.current_dir);
        // Drop the old folder's textures so we don't keep them around; they'll be
        // re-generated on demand if we browse back.
        self.thumbs.clear();
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
            let ctx = ui.ctx();

            // Filter entries: only directories and images, and skip hidden files
            // (dotfiles). Clone them into an owned vec so the view-building pass that
            // follows can read the thumbnail cache without fighting its mutation.
            let filtered: Vec<FileInfo> = self
                .entries
                .iter()
                .filter(|entry| {
                    let is_hidden = entry.name.starts_with('.');
                    let is_valid_type = entry.file_type == FileType::Directory
                        || crate::utils::is_image(&entry.path);
                    !is_hidden && is_valid_type
                })
                .cloned()
                .collect();

            // Drive thumbnail generation within a per-frame budget, so a folder with
            // thousands of (possibly huge) images drains gently instead of freezing
            // the first paint. Each image decodes + downscales on a *detached*
            // background thread; the main thread only uploads the tiny, capped
            // texture — it's never handed a full-resolution image, which is what used
            // to exceed the GPU texture limits and crash the app.
            let mut budget = thumbnail::PER_FRAME_BUDGET;
            for entry in &filtered {
                if entry.file_type == FileType::File {
                    let slot = self.thumbs.entry(entry.path.clone()).or_default();
                    thumbnail::advance(
                        slot,
                        &entry.path,
                        thumbnail::THUMB_CAP,
                        &mut budget,
                        ctx,
                        &mut self.tex_seq,
                    );
                }
            }

            // Snapshot each cell's current thumbnail state into owned views. A ready
            // `Texture` is a cheap refcount clone of the cached GPU handle; anything
            // still decoding shows a loading placeholder and fills in next frame.
            let views: Vec<CellView> = filtered
                .iter()
                .map(|entry| {
                    if entry.file_type == FileType::Directory {
                        CellView::Directory {
                            name: entry.name.clone(),
                            path: entry.path.clone(),
                        }
                    } else {
                        let image = match self.thumbs.get(&entry.path) {
                            Some(Thumb::Ready(handle)) => CellImage::Texture(handle.clone()),
                            Some(Thumb::Broken(reason)) => CellImage::Broken(reason.clone()),
                            // Queued / Pending / never-started → a loading placeholder.
                            _ => CellImage::Loading,
                        };
                        CellView::Image {
                            name: entry.name.clone(),
                            path: entry.path.clone(),
                            image,
                        }
                    }
                })
                .collect();

            // Each item is a fixed-size cell (thumbnail on top, name below).
            //
            // We deliberately avoid egui's `Grid`: Grid caches the previous
            // frame's column widths/row heights and uses them to *predict* cell
            // placement. With async image loads the cell size swings between the
            // loading fallback and the real thumbnail size, so the cached positions
            // lag a frame and alternating cells get allocated on top of each other
            // -- that is the "every other thumbnails missing" bug.
            // `horizontal_wrapped` recomputes positions every frame with no cache,
            // and a fixed cell size keeps the layout stable.
            let thumb = self.thumbnail_size;
            let item = vec2(thumb, thumb + 20.0);

            ui.horizontal_wrapped(|ui| {
                for view in &views {
                    let _ = ui.allocate_ui_with_layout(
                        item,
                        egui::Layout::top_down(egui::Align::Center),
                        |cell| {
                            // Fill the whole cell with a dark background.
                            cell.painter().rect_filled(
                                cell.max_rect(),
                                2.0,
                                Color32::from_gray(30),
                            );

                            let mut clicked = false;
                            match view {
                                CellView::Directory { .. } => {
                                    let resp = cell.add(
                                        egui::Label::new(RichText::new("📁").size(thumb * 0.5))
                                            .sense(egui::Sense::click()),
                                    );
                                    clicked |= resp.clicked();
                                }
                                CellView::Image { name, image, .. } => match image {
                                    CellImage::Texture(handle) => {
                                        // The texture was already down-scaled to THUMB_CAP
                                        // on a background thread; here we only ask egui to
                                        // display *it* at the cell size (aspect-preserved,
                                        // no upscale) — never the full-resolution image.
                                        let resp = cell.add(
                                            egui::Image::from_texture(handle)
                                                .max_size(vec2(thumb, thumb))
                                                .sense(egui::Sense::click())
                                                .alt_text(name.clone()),
                                        );
                                        clicked |= resp.clicked();
                                    }
                                    CellImage::Broken(reason) => {
                                        // Could not be previewed (corrupt file, unsupported
                                        // format, a decode problem, …). Show a placeholder
                                        // with the reason on hover instead of crashing.
                                        let resp = cell
                                            .add(
                                                egui::Label::new(
                                                    RichText::new("\u{26a0}")
                                                        .size(thumb * 0.4)
                                                        .color(Color32::from_gray(160)),
                                                )
                                                .sense(egui::Sense::click()),
                                            )
                                            .on_hover_text(reason.clone());
                                        clicked |= resp.clicked();
                                    }
                                    CellImage::Loading => {
                                        let resp = cell
                                            .add(
                                                egui::Label::new(
                                                    RichText::new("\u{2022}")
                                                        .size(thumb * 0.5)
                                                        .color(Color32::from_gray(120)),
                                                )
                                                .sense(egui::Sense::click()),
                                            )
                                            .on_hover_text("Generating thumbnail\u{2026}");
                                        clicked |= resp.clicked();
                                    }
                                },
                            }

                            // Filename under the thumbnail.
                            let name_text = match view {
                                CellView::Directory { name, .. } | CellView::Image { name, .. } => {
                                    name
                                }
                            };
                            let name_resp = cell.add(
                                egui::Label::new(RichText::new(name_text).size(12.0))
                                    .sense(egui::Sense::click()),
                            );
                            clicked |= name_resp.clicked();

                            if clicked {
                                action = match view {
                                    CellView::Directory { path, .. } => {
                                        Some(ExplorerAction::EnterDirectory(path.clone()))
                                    }
                                    CellView::Image { path, .. } => {
                                        Some(ExplorerAction::OpenFile(path.clone()))
                                    }
                                };
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
