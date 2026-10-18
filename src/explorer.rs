use crate::thumbnail::{self, Thumb};
use crate::utils::{FileInfo, FileType};
use egui::{vec2, Color32, RichText, ScrollArea, Slider, TextureHandle, Ui};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct Explorer {
    pub current_dir: PathBuf,
    pub entries: Vec<FileInfo>,
    pub thumbnail_size: f32,

    /// When true, clicking a thumbnail toggles its selection instead of opening it —
    /// the way to pick several images at once. Toggled by the "Select" button in the
    /// top bar. Selection is per-directory (cleared on `change_dir`).
    selecting: bool,
    /// Absolute paths of the currently selected images.
    selected: HashSet<PathBuf>,

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
            selecting: false,
            selected: HashSet::new(),
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
        // Selection is per-directory, so clear it whenever the folder changes.
        self.selected.clear();
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.change_dir(parent.to_path_buf());
        }
    }

    /// Number of images currently selected, shown by the "Selection" button/modal.
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    /// Select every image shown in the grid (image files, excluding hidden
    /// dotfiles) — the "Select all" action.
    pub fn select_all_images(&mut self) {
        for entry in &self.entries {
            let hidden = entry.name.starts_with('.');
            let is_image = entry.file_type == FileType::File && crate::utils::is_image(&entry.path);
            if !hidden && is_image {
                self.selected.insert(entry.path.clone());
            }
        }
    }

    /// Clear the selection entirely — the "Deselect all" action.
    pub fn deselect_all(&mut self) {
        self.selected.clear();
    }

    /// An owned snapshot of the currently selected image paths, for the bulk
    /// operations the "Selection" modal runs on them.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected.iter().cloned().collect()
    }

    /// Re-list the current directory's contents after a bulk action changed them on
    /// disk (a delete removed files, a convert added `.jpg` siblings), without
    /// touching the selection or the thumbnail cache.
    pub fn refresh(&mut self) {
        self.entries = crate::utils::list_directory(&self.current_dir);
    }

    /// Drop the cached thumbnail texture(s) for the given paths so the next frame
    /// regenerates them from the freshly-written files instead of showing the old
    /// image.
    pub fn invalidate_thumbnails(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.thumbs.remove(path);
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
            ui.separator();
            // Toggle between "click opens" and "click selects". While on, clicking a
            // thumbnail toggles it in the selection (outlined with a border) instead of
            // opening it — this is the way to pick several images at once.
            let select_btn = egui::Button::new("☑ Select").fill(if self.selecting {
                egui::Color32::from_rgb(70, 110, 150)
            } else {
                egui::Color32::TRANSPARENT
            });
            if ui.add(select_btn).clicked() {
                self.selecting = !self.selecting;
            }
            // "Select all" / "Deselect all" bulk-manage the current selection.
            if ui.button("Select all").clicked() {
                self.select_all_images();
            }
            if ui.button("Deselect all").clicked() {
                self.deselect_all();
            }
            // Show the live selection count; clicking it opens the confirmation modal.
            if ui
                .button(format!("📋 Selection ({})", self.selected.len()))
                .clicked()
            {
                action = Some(ExplorerAction::OpenSelectionModal);
            }
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

            // Snapshot the selection state so the per-cell closure below only ever
            // touches these locals (never `self`), which keeps the borrow checker
            // happy while the grid lays out. The actual toggle is applied after the
            // loop (see below).
            let selecting = self.selecting;
            let selected = &self.selected;

            // A clicked image in selection mode is recorded here and toggled *after*
            // the loop.
            let mut clicked_cell: Option<PathBuf> = None;

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

                            // Outline a selected image so multi-selection is visible.
                            let is_selected = match view {
                                CellView::Image { path, .. } => selected.contains(path),
                                CellView::Directory { .. } => false,
                            };
                            if is_selected {
                                cell.painter().rect_stroke(
                                    cell.max_rect().shrink(1.0),
                                    2.0,
                                    egui::Stroke::new(3.0, Color32::from_rgb(80, 180, 255)),
                                    egui::StrokeKind::Inside,
                                );
                            }

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
                                match view {
                                    // Directories always navigate, even in select mode, so
                                    // you can move between folders while picking images.
                                    CellView::Directory { path, .. } => {
                                        action =
                                            Some(ExplorerAction::EnterDirectory(path.clone()));
                                    }
                                    CellView::Image { path, .. } => {
                                        if selecting {
                                            // Record the click and toggle it after the
                                            // loop, so this closure only ever reads the
                                            // selection snapshot, never mutates it.
                                            clicked_cell = Some(path.clone());
                                        } else {
                                            action = Some(ExplorerAction::OpenFile(path.clone()));
                                        }
                                    }
                                }
                            }
                        },
                    );
                }
            });

            // Apply the recorded selection toggle now that the layout pass is done, so
            // the per-cell closure above never mutated the selection cache mid-grid.
            if let Some(path) = clicked_cell {
                if self.selected.contains(&path) {
                    self.selected.remove(&path);
                } else {
                    self.selected.insert(path);
                }
            }
        });

        action
    }
}

pub enum ExplorerAction {
    None,
    EnterDirectory(PathBuf),
    OpenFile(PathBuf),
    /// The "Selection" button was clicked: open the modal showing how many images
    /// are currently selected. The count itself is read live from the explorer.
    OpenSelectionModal,
}

#[cfg(test)]
mod tests {
    use super::Explorer;
    use crate::utils::{FileInfo, FileType};
    use std::path::PathBuf;

    fn entry(name: &str, path: &str, dir: bool) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            path: PathBuf::from(path),
            file_type: if dir {
                FileType::Directory
            } else {
                FileType::File
            },
        }
    }

    /// "Select all" mirrors the grid: it picks visible image files only — skipping
    /// directories (even one named like an image), hidden dotfiles, and non-images.
    #[test]
    fn select_all_picks_visible_images_only() {
        let mut explorer = Explorer::new(PathBuf::from("/nonexistent-xyz"));
        explorer.entries = vec![
            entry("photos", "/photos", true),
            entry("cat.png", "/cat.png", false),
            entry("dog.jpg", "/dog.jpg", false),
            entry("tricky.png", "/tricky.png", true), // an image-named directory
            entry(".secret.png", "/.secret.png", false), // a hidden image
            entry("notes.txt", "/notes.txt", false), // not an image
        ];

        explorer.select_all_images();

        assert_eq!(explorer.selected_count(), 2);
        assert!(explorer.selected.contains(&PathBuf::from("/cat.png")));
        assert!(explorer.selected.contains(&PathBuf::from("/dog.jpg")));
        assert!(!explorer.selected.contains(&PathBuf::from("/tricky.png")));
        assert!(!explorer.selected.contains(&PathBuf::from("/.secret.png")));
        assert!(!explorer.selected.contains(&PathBuf::from("/notes.txt")));
    }

    /// "Deselect all" empties whatever is currently selected.
    #[test]
    fn deselect_all_clears_selection() {
        let mut explorer = Explorer::new(PathBuf::from("/nonexistent-xyz"));
        explorer.selected.insert(PathBuf::from("/cat.png"));
        explorer.selected.insert(PathBuf::from("/dog.jpg"));
        assert_eq!(explorer.selected_count(), 2);

        explorer.deselect_all();

        assert_eq!(explorer.selected_count(), 0);
    }
}
