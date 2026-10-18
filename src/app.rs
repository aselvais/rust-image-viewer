use std::path::PathBuf;

use crate::explorer::{Explorer, ExplorerAction};
use crate::image_ops;
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

    // The "Selection" modal, opened by the "Selection" button in the thumbnail
    // view's top bar. It shows the count of selected images plus a picker of one
    // action to run on them, confirmed with "Cancel" / "OK".
    show_selection_modal: bool,

    // UI state for the action picker inside the "Selection" modal: the chosen
    // action plus the parameters for the resize / convert-to-JPEG choices.
    selection_action: SelectionAction,
    resize_width: String,
    jpeg_quality: u8,

    // The outcome of the last "OK" on the selection modal, shown in a transient
    // message bar so the user gets feedback that the bulk operation ran (and what
    // it did to the files on disk).
    selection_result: Option<SelectionOutcome>,
}

/// The bulk operation offered on a selection in the "Selection" modal. Only one
/// can be active at a time; "OK" is meant to trigger the chosen one.
#[derive(Clone, Copy, PartialEq)]
enum SelectionAction {
    /// Remove the selected items.
    Delete,
    /// Resize the selected images to `resize_width` pixels wide.
    Resize,
    /// Rotate the selected images 90° clockwise, in place.
    RotateClockwise,
    /// Rotate the selected images 90° counter-clockwise, in place.
    RotateCounterclockwise,
    /// Re-encode the selected images as JPEG at `jpeg_quality`% quality.
    ConvertJpeg,
}

/// The outcome of a bulk selection operation, surfaced to the user as a transient
/// message in the viewer. `message` is the headline; `detail` (when present) names
/// the first file that failed so the user knows where to look.
#[derive(Clone)]
struct SelectionOutcome {
    message: String,
    detail: Option<String>,
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
            show_selection_modal: false,
            // Defaults for the "Selection" modal's action picker.
            selection_action: SelectionAction::Delete,
            resize_width: "1920".to_string(),
            jpeg_quality: 90,
            selection_result: None,
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
        // Opening a new image clears any stale selection-operation result.
        self.selection_result = None;
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

        // Transient feedback for the last bulk selection operation (delete /
        // resize / convert). Rendered on the root UI above the active panel so it
        // shows whether the explorer or the viewer is open; dismiss it by clicking
        // the message or its button.
        if let Some(outcome) = self.selection_result.clone() {
            let mut dismiss = false;
            egui::Panel::top("selection_result_bar").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let headline = ui.label(
                        egui::RichText::new(outcome.message.as_str()).strong().small(),
                    );
                    if headline.clicked() {
                        dismiss = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Dismiss").clicked() {
                            dismiss = true;
                        }
                    });
                });
                if let Some(detail) = &outcome.detail {
                    ui.label(egui::RichText::new(detail.as_str()).small().weak());
                }
            });
            if dismiss {
                self.selection_result = None;
            }
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
                            // The "Selection" button just opens the modal; the count is
                            // read live from the explorer when the modal is rendered.
                            ExplorerAction::OpenSelectionModal => {
                                self.show_selection_modal = true;
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

        // Render the "Selection" modal on top of everything when the user clicked the
        // "Selection" button in the thumbnail view. The count is read live from the
        // explorer so it always matches; both buttons simply dismiss the window.
        if self.show_selection_modal {
            // A freshly opened modal starts with no stale result showing.
            self.selection_result = None;
            let count = self.explorer.selected_count();
            // `Context::screen_rect` isn't public in this egui version, so centre the
            // modal from the window's own rect instead.
            let center = ui.max_rect().center();
            egui::Area::new(egui::Id::new("selection-modal"))
                .order(egui::Order::Foreground)
                .default_pos(center)
                .show(ui.ctx(), |ui| {
                    let mut closed = false;
                    egui::Window::new("Selection")
                        .title_bar(false)
                        .resizable(false)
                        .collapsible(false)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{count} image(s) selected"))
                                    .strong(),
                            );

                            ui.separator();
                            ui.label(egui::RichText::new("Action (choose one):").strong());

                            // 1. Delete - the choice is captured here; the actual removal
                            //    is wired up in a later step.
                            ui.radio_value(
                                &mut self.selection_action,
                                SelectionAction::Delete,
                                "Delete the selected items.",
                            );

                            // 2. Resize, with a target-width text box (default 1920).
                            ui.radio_value(
                                &mut self.selection_action,
                                SelectionAction::Resize,
                                "Resize the selected images to a target width.",
                            );
                            if self.selection_action == SelectionAction::Resize {
                                ui.label("Target width (px):");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.resize_width)
                                        .desired_width(120.0),
                                );
                            }

                            // 3. Rotate 90° clockwise, in place.
                            ui.radio_value(
                                &mut self.selection_action,
                                SelectionAction::RotateClockwise,
                                "Rotate the selected images 90° clockwise.",
                            );

                            // 4. Rotate 90° counter-clockwise, in place.
                            ui.radio_value(
                                &mut self.selection_action,
                                SelectionAction::RotateCounterclockwise,
                                "Rotate the selected images 90° counter-clockwise.",
                            );

                            // 5. Convert to JPEG, with a compression-level picker (default 90).
                            ui.radio_value(
                                &mut self.selection_action,
                                SelectionAction::ConvertJpeg,
                                "Convert the selected images to JPEG.",
                            );
                            if self.selection_action == SelectionAction::ConvertJpeg {
                                let levels = [50u8, 60, 70, 80, 85, 90, 95, 100];
                                let mut chosen = self.jpeg_quality;
                                egui::ComboBox::from_label("Compression")
                                    .selected_text(format!("{chosen}%"))
                                    .show_ui(ui, |ui| {
                                        for &level in &levels {
                                            ui.selectable_value(&mut chosen, level, format!("{level}%"));
                                        }
                                    });
                                self.jpeg_quality = chosen;
                            }

                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Cancel").clicked() {
                                    closed = true;
                                }
                                // "OK" runs the chosen bulk operation on the selected
                                // images, then closes the modal.
                                if ui.button("OK").clicked() {
                                    self.run_selection_action();
                                    closed = true;
                                }
                            });
                        });
                    if closed {
                        self.show_selection_modal = false;
                    }
                });
        }
    }
}

impl ImageViewerApp {
    /// Parse the modal's "Target width (px)" text box into a positive pixel width.
    /// Returns `None` when empty or non-numeric; the caller reports that as a
    /// cancelled resize rather than crashing.
    fn valid_resize_width(&self) -> Option<u32> {
        let parsed = self.resize_width.trim().parse::<u32>().ok()?;
        if parsed == 0 {
            None
        } else {
            Some(parsed)
        }
    }

    /// Run the bulk operation chosen in the "Selection" modal on every currently
    /// selected image, then refresh the explorer and thumbnail cache so the UI
    /// reflects the change. The human-readable summary is stored on
    /// `selection_result` and shown as a transient bar; the caller closes the
    /// modal.
    fn run_selection_action(&mut self) {
        // Snapshot the selection before any mutation.
        let paths = self.explorer.selected_paths();
        let op = match self.selection_action {
            SelectionAction::Delete => image_ops::Operation::Delete,
            SelectionAction::Resize => image_ops::Operation::Resize,
            SelectionAction::RotateClockwise => image_ops::Operation::RotateClockwise,
            SelectionAction::RotateCounterclockwise => {
                image_ops::Operation::RotateCounterclockwise
            }
            SelectionAction::ConvertJpeg => image_ops::Operation::ConvertJpeg,
        };

        // Run the chosen operation. A resize with an empty / non-numeric target
        // width is reported and the files are left untouched.
        let result: image_ops::BulkResult = match op {
            image_ops::Operation::Delete => image_ops::delete_files(&paths),
            image_ops::Operation::Resize => match self.valid_resize_width() {
                Some(width) => image_ops::resize_to_width(&paths, width),
                None => {
                    self.selection_result = Some(SelectionOutcome {
                        message: "Resize skipped: enter a valid target width."
                            .to_string(),
                        detail: None,
                    });
                    return;
                }
            },
            image_ops::Operation::RotateClockwise => {
                image_ops::rotate_90(&paths, image_ops::Rotation::Clockwise)
            }
            image_ops::Operation::RotateCounterclockwise => {
                image_ops::rotate_90(&paths, image_ops::Rotation::Counterclockwise)
            }
            image_ops::Operation::ConvertJpeg => {
                image_ops::convert_to_jpeg(&paths, self.jpeg_quality)
            }
        };

        // The op ran on the current selection: drop it so the next modal start
        // begins clean, and refresh the view when anything actually changed on
        // disk (a delete removed files, a resize/convert rewrote or added them).
        self.explorer.deselect_all();
        if result.succeeded > 0 {
            self.explorer.refresh();
            self.explorer.invalidate_thumbnails(&paths);
        }

        let (message, detail) = result.outcome(op);
        self.selection_result = Some(SelectionOutcome { message, detail });
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
