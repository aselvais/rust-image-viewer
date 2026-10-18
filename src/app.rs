use std::path::PathBuf;

use crate::explorer::{Explorer, ExplorerAction};
use crate::metadata::ImageMetadata;
use crate::utils::is_image;
use crate::viewer::{Viewer, ViewerAction};
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

    /// Open a path — dragged from the OS, passed on the command line, or clicked in
    /// the explorer. A directory is *browsed*: the explorer switches to it. An image
    /// is *opened* for viewing, with its sibling images (same directory) collected
    /// so the ◀ Prev / Next ▶ navigation keeps working.
    fn open_path(&mut self, path: &PathBuf) {
        // A dropped/clicked folder: browse it in the explorer.
        if path.is_dir() {
            self.explorer.change_dir(path.clone());
            self.mode = AppMode::Idle;
            return;
        }

        if !is_image(path) {
            // Not a folder and not an image we can open — ignore it.
            return;
        }

        // An image: collect its siblings (the directory it lives in) for navigation.
        let dir = match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
            _ => return,
        };

        let images: Vec<PathBuf> = crate::utils::list_directory(&dir)
            .into_iter()
            .filter(|info| info.file_type == crate::utils::FileType::File && is_image(&info.path))
            .map(|info| info.path)
            .collect();

        self.mode = AppMode::Viewing(Viewer::new(path.clone()), images);
    }
}

impl eframe::App for ImageViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Global shortcut: Cmd/Ctrl + I toggles the details panel (only when an image is open).
        let toggle = ui.input(|i| i.key_pressed(egui::Key::I) && i.modifiers.command);

        // Files/directories dragged from the OS (e.g. Finder onto the window)
        // arrive as per-frame input. A dropped folder is browsed; a dropped image
        // is opened. Routed through the same helper the explorer uses, so it works
        // in both the browsing and viewing states.
        let dropped: Vec<PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if let Some(path) = dropped.first() {
            self.open_path(&path);
        }

        match &mut self.mode {
            AppMode::Idle => {
                egui::CentralPanel::default().show(ui, |ui| {
                    if let Some(action) = self.explorer.ui(ui) {
                        match action {
                            // Both cases resolve to the same helper, which also powers
                            // drag-and-drop from the OS.
                            ExplorerAction::EnterDirectory(path)
                            | ExplorerAction::OpenFile(path) => {
                                self.open_path(&path);
                            }
                            ExplorerAction::None => {}
                        }
                    }
                });
            }
            AppMode::Viewing(viewer, images) => {
                // A top menu-bar toolbar, then the image in the central panel.
                let mut action: Option<ViewerAction> = None;

                egui::Panel::top("viewer_menu").show(ui, |ui| {
                    if let Some(a) = viewer.menu_bar(ui) {
                        action = Some(a);
                    }
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    // Always call image_area so the image is drawn; only take its
                    // (keyboard) action when the menu bar didn't already act.
                    let img_action = viewer.image_area(ui);
                    if action.is_none() {
                        if let Some(a) = img_action {
                            action = Some(a);
                        }
                    }
                });

                match action {
                    Some(ViewerAction::Next) => {
                        let current_path = viewer.current_image.clone();
                        if let Some(idx) = images.iter().position(|p| p == &current_path) {
                            if idx + 1 < images.len() {
                                viewer.current_image = images[idx + 1].clone();
                            }
                        }
                    }
                    Some(ViewerAction::Previous) => {
                        let current_path = viewer.current_image.clone();
                        if let Some(idx) = images.iter().position(|p| p == &current_path) {
                            if idx > 0 {
                                viewer.current_image = images[idx - 1].clone();
                            }
                        }
                    }
                    Some(ViewerAction::Close) => {
                        self.mode = AppMode::Idle;
                        self.show_metadata = false;
                    }
                    Some(ViewerAction::ToggleMetadata) => {
                        self.show_metadata = !self.show_metadata;
                    }
                    None => {}
                }
            }
        }

        // Global Cmd/Ctrl + I toggle (only meaningful while an image is open).
        if toggle {
            if matches!(self.mode, AppMode::Viewing(..)) {
                self.show_metadata = !self.show_metadata;
            }
        }

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
                                            if ui
                                                .button(egui::RichText::new("✕").size(15.0))
                                                .clicked()
                                            {
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
                    meta_row(
                        ui,
                        "File size",
                        &crate::metadata::format_bytes(meta.file_size),
                    );
                    if let Some(m) = &meta.modified {
                        meta_row(ui, "Last modified", m);
                    }
                });

            ui.separator();
            if meta.exif.is_empty() {
                ui.label(egui::RichText::new("EXIF data: none found").weak());
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
