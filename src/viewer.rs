use crate::thumbnail;
use crate::utils;
use egui;
use std::path::PathBuf;

/// A zoomable image viewer with a navigation toolbar and mouse-wheel zoom.
pub struct Viewer {
    pub current_image: PathBuf,

    /// Render scale where `1.0` == the image's native pixel size ("100%").
    zoom: f32,

    /// When true, the image is auto-scaled to fit the available area every frame.
    /// Any manual zoom (slider / mouse wheel / "100%") turns this off.
    fit_to_screen: bool,

    /// The image's natural pixel size, decoded lazily and cached per path.
    natural: Option<egui::Vec2>,
    natural_for: Option<PathBuf>,

    /// For images whose natural size exceeds [`crate::thumbnail::MAX_VIEW_SIDE`]
    /// we must never hand a full-resolution image to the GPU (that is what
    /// crashes). Instead we keep a *capped* texture, generated on a background
    /// thread and drained non-blockingly, plus its state and the path it was made
    /// for. Only used for oversized images; normal images skip this entirely.
    preview: Option<crate::thumbnail::Thumb>,
    preview_for: Option<PathBuf>,
    preview_seq: u64,
}

/// Zoom range enforced everywhere; slider bounds and wheel clamps use these.
const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 8.0;

impl Viewer {
    pub fn new(image_path: PathBuf) -> Self {
        Self {
            current_image: image_path,
            zoom: 1.0,
            fit_to_screen: true,
            natural: None,
            natural_for: None,
            preview: None,
            preview_for: None,
            preview_seq: 0,
        }
    }

    /// Decode the image's natural pixel size (header only — no full decode) and
    /// cache it, refreshing automatically when the current image changes.
    fn ensure_natural(&mut self) {
        if self.natural_for.as_ref() != Some(&self.current_image) {
            let natural = match image::image_dimensions(&self.current_image) {
                Ok((w, h)) => Some(egui::vec2(w as f32, h as f32)),
                Err(_) => None,
            };
            self.natural = natural;
            self.natural_for = Some(self.current_image.clone());
        }
    }

    fn file_name(&self) -> String {
        self.current_image
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(unknown)".into())
    }

    /// A zoom that makes the whole image fit inside `avail`, preserving aspect ratio.
    fn fit_zoom(&self, avail: egui::Vec2) -> Option<f32> {
        let nat = self.natural?;
        let ratio = (avail.x / nat.x).min(avail.y / nat.y);
        if ratio.is_finite() && ratio > 0.0 {
            Some(ratio.clamp(MIN_ZOOM, MAX_ZOOM))
        } else {
            None
        }
    }

    fn apply_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.fit_to_screen = false;
    }

    // ----------------------------------------------------------------
    // Toolbar — rendered in the top menu bar. Returns a nav action if a button was
    // clicked; zoom controls mutate state directly.

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) -> Option<ViewerAction> {
        let mut action = None;

        // Row 1: the viewing status — filename only.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Viewing: ").weak());
            ui.label(egui::RichText::new(self.file_name()).strong());
        });

        // Row 2: navigation + details, then the zoom controls, left to right.
        ui.horizontal(|ui| {
            if ui.button("⬅ Back").clicked() {
                action = Some(ViewerAction::Close);
            }
            if ui.button("Details (⌘I)").clicked() {
                action = Some(ViewerAction::ToggleMetadata);
            }
            ui.add_space(8.0);
            if ui.button("◀ Prev").clicked() {
                action = Some(ViewerAction::Previous);
            }
            if ui.button("Next ▶").clicked() {
                action = Some(ViewerAction::Next);
            }
            ui.separator();
            if ui.button("100%").clicked() {
                self.apply_zoom(1.0);
            }
            if ui.button("Fit screen").clicked() {
                // Auto-fit the whole image to the area on the next frame.
                self.fit_to_screen = true;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Zoom").weak());

            let old = self.zoom;
            let mut zoom = self.zoom;
            ui.add_sized(
                egui::vec2(180.0, 0.0),
                egui::Slider::new(&mut zoom, MIN_ZOOM..=MAX_ZOOM),
            );
            self.zoom = zoom;
            if zoom != old {
                // A manual zoom disables auto-fit.
                self.fit_to_screen = false;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!(
                "{:.0}%",
                (self.zoom * 100.0).round()
            )));
        });

        action
    }

    // ----------------------------------------------------------------
    // Image area — rendered in the central panel. Returns a nav action for
    // keyboard input; also zooms in response to the mouse wheel, trackpad pinch,
    // and Ctrl/Cmd + scroll.

    pub fn image_area(&mut self, ui: &mut egui::Ui) -> Option<ViewerAction> {
        let mut action = None;

        // Keyboard navigation — identical to the toolbar buttons.
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            action = Some(ViewerAction::Next);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            action = Some(ViewerAction::Previous);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(ViewerAction::Close);
        }

        self.ensure_natural();
        let avail = ui.available_size_before_wrap();

        // Auto-fit when in fit mode (on first open, or after "Fit screen").
        if self.fit_to_screen {
            if let Some(z) = self.fit_zoom(avail) {
                self.zoom = z;
            }
        }

        // On-screen size of the image at the current zoom.
        let displayed = self.natural.map(|nat| nat * self.zoom);
        let overflows = displayed.map_or(false, |d| d.x > avail.x || d.y > avail.y);

        // Trackpad pinch and Ctrl/Cmd + scroll both report this frame's
        // proportional zoom factor via `zoom_delta` (>1 = zoom in, <1 = zoom out,
        // 1 = no pinch). A pinch is an explicit zoom gesture, so it works even
        // while the image is zoomed in past the fit window — pinch further in, or
        // pinch out back towards fit. It is a separate field from the two-finger
        // scroll delta, so it never fights the panning below.
        let pinch = ui.input(|i| i.zoom_delta());
        if pinch.is_finite() && pinch.abs() > 1e-3 && (1.0 - pinch).abs() > 1e-3 {
            self.apply_zoom(self.zoom * pinch);
        }

        // Two-finger scroll zooms while the image still fits; once it is larger
        // than the viewport that same scroll instead pans it through the
        // ScrollArea below, so a single scroll gesture never both zooms and scrolls.
        let scroll = ui.input(|i| i.smooth_scroll_delta());
        if !overflows && scroll.y != 0.0 {
            let factor = 1.0 - scroll.y * 0.001;
            self.apply_zoom(self.zoom * factor);
        }

        // For images too large to upload to the GPU safely, never hand egui a
        // full-resolution texture (that is what crashes us). Instead we down-scale
        // to a capped texture on a background thread and show it — or a "preparing"
        // note while it's decoding. Normal-size images skip all of this and use
        // egui's file loader exactly as before.
        let big = self
            .natural
            .map(|n| n.x.max(n.y) > thumbnail::MAX_VIEW_SIDE as f32)
            .unwrap_or(false);

        if big {
            let ctx = ui.ctx();
            // (Re)start the capped decode whenever the open image changes.
            if self.preview_for.as_ref() != Some(&self.current_image) {
                self.preview = Some(thumbnail::Thumb::Queued);
                self.preview_for = Some(self.current_image.clone());
            }
            if let Some(state) = self.preview.as_mut() {
                // A single in-flight decode at a time is plenty for the viewer.
                let mut budget = 4;
                thumbnail::advance(
                    state,
                    &self.current_image,
                    thumbnail::MAX_VIEW_SIDE,
                    &mut budget,
                    ctx,
                    &mut self.preview_seq,
                );
            }
        }

        // `uri` is always built so its `&str` lives long enough to borrow for the
        // `ViewSource` below; for the oversized branch it simply goes unused.
        let uri = utils::image_uri(&self.current_image);
        let source = if big {
            match &self.preview {
                Some(thumbnail::Thumb::Ready(handle)) => ViewSource::Texture(handle),
                // Still decoding, or the capped texture couldn't be made at all.
                _ => ViewSource::Missing,
            }
        } else {
            ViewSource::Uri(uri.as_str())
        };

        draw_image(ui, self.zoom, displayed, avail, overflows, source);
        action
    }
}

/// What to render in the image area, chosen by [`Viewer::image_area`].
enum ViewSource<'a> {
    /// A normal-size image, shown via egui's file loader. This is unchanged from
    /// the original behaviour and has no crash risk — the image is small.
    Uri(&'a str),
    /// An oversized image, rendered from its pre-down-scaled *capped* texture.
    /// Because we only ever hand the GPU this texture (never the original), it
    /// can't exceed the GPU's texture limits.
    Texture(&'a egui::TextureHandle),
    /// An oversized image whose capped texture isn't ready yet (still decoding) or
    /// couldn't be made at all — show a friendly note instead of crashing.
    Missing,
}

/// Draw the image at its current zoom.
///
/// When the image fits inside the area we centre it. When it is larger than the
/// area we wrap it in a two-axis `ScrollArea`, so its scroll bars let you pan
/// left / right / up / down instead of the image being clipped.
///
/// The `source` decides *what* is painted: an ordinary file-loaded image, a
/// down-scaled texture for an oversized one, or a "can't preview" note.
fn draw_image(
    ui: &mut egui::Ui,
    zoom: f32,
    displayed: Option<egui::Vec2>,
    avail: egui::Vec2,
    overflows: bool,
    source: ViewSource,
) {
    match source {
        // egui_extras' file loader needs a well-formed `file://` URI; the caller
        // already built it via the shared helper so Windows paths (`C:\...`) turn
        // into the `file:///C:/...` form the loader expects.
        ViewSource::Uri(uri) => {
            if overflows {
                // Let the image take its true displayed size so the ScrollArea can
                // size its content and show a scroll bar on every axis that overflows.
                let size = displayed.unwrap_or(egui::vec2(avail.x, avail.y));
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_size(size);
                        ui.add(egui::Image::from_uri(uri).fit_to_original_size(zoom));
                    });
            } else {
                // Fits: centre it, capped to the area so it is never clipped.
                ui.centered_and_justified(|ui| {
                    ui.add(
                        egui::Image::from_uri(uri)
                            .fit_to_original_size(zoom)
                            .max_size(egui::vec2(avail.x.max(1.0), avail.y.max(1.0))),
                    );
                });
            }
        }

        // The texture is already capped, so displaying it is always safe. Centre
        // and cap it to the area. (At extreme zoom the capped image may overflow,
        // but a capped texture can't crash anything.)
        ViewSource::Texture(handle) => {
            ui.centered_and_justified(|ui| {
                ui.add(
                    egui::Image::from_texture(handle)
                        .fit_to_original_size(zoom)
                        .max_size(egui::vec2(avail.x.max(1.0), avail.y.max(1.0))),
                );
            });
        }

        ViewSource::Missing => {
            ui.centered_and_justified(|ui| {
                ui.heading("Can't preview this image");
                ui.label(
                    egui::RichText::new(
                        "It's very large or couldn't be decoded, so it's being \
                         prepared for viewing."
                            .to_string(),
                    )
                    .weak(),
                );
            });
        }
    }
}

pub enum ViewerAction {
    Next,
    Previous,
    Close,
    ToggleMetadata,
}
