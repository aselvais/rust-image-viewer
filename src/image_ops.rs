//! Bulk image operations behind the "Selection" modal: delete, rotate, resize, and
//! convert to JPEG, all applied to the set of images the user has picked.
//!
//! Every function here is pure disk work — it never touches egui — and it never
//! panics the batch: a single corrupt or unwritable image is reported in
//! [`BulkResult::errors`] instead of aborting, so one bad file can't take the rest
//! with it. Keeping the logic UI-free also lets it be unit-tested on its own, the
//! same way [`crate::thumbnail::resize_rgba`] is.

use std::path::{Path, PathBuf};

/// Which bulk action produced a [`BulkResult`], so its summary reads naturally.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// The files were permanently removed from disk.
    Delete,
    /// The files were scaled to a target width, in place.
    Resize,
    /// The files were rotated 90° clockwise, in place.
    RotateClockwise,
    /// The files were rotated 90° counter-clockwise, in place.
    RotateCounterclockwise,
    /// A JPEG copy of each file was written next to it.
    ConvertJpeg,
}

/// The outcome of a bulk operation over a list of image files: how many succeeded
/// and failed, plus one short reason per failure.
#[derive(Debug, Default, Clone)]
pub struct BulkResult {
    /// How many files were processed successfully.
    pub succeeded: usize,
    /// How many files failed.
    pub failed: usize,
    /// One entry per failed file: `(path, short human reason)`.
    pub errors: Vec<(PathBuf, String)>,
}

impl BulkResult {
    /// Total files that were attempted.
    pub fn total(&self) -> usize {
        self.succeeded + self.failed
    }

    /// A human-readable `(message, detail)` pair for the modal.
    ///
    /// `message` is the headline — `"Deleted 3 image(s)."`, `"Resized 2 of 3
    /// image(s); 1 failed."`. `detail` is `Some` when at least one file failed,
    /// naming the first one and its reason so the user knows where to look.
    pub fn outcome(&self, op: Operation) -> (String, Option<String>) {
        let total = self.total();
        if total == 0 {
            return ("Nothing to do.".to_string(), None);
        }

        let lead = match op {
            Operation::Delete => "Deleted",
            Operation::Resize => "Resized",
            Operation::RotateClockwise => "Rotated 90° clockwise",
            Operation::RotateCounterclockwise => "Rotated 90° counter-clockwise",
            Operation::ConvertJpeg => "Converted to JPEG",
        };

        let message = match (self.succeeded, self.failed) {
            (_, 0) => format!("{lead} {total} image(s)."),
            (0, _) => format!("{lead} none of {total} image(s) — all failed."),
            (ok, fail) => format!("{lead} {ok} of {total} image(s); {fail} failed."),
        };

        let detail = self.errors.first().map(|(path, reason)| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            // Keep the appended detail short so the modal stays compact.
            let short: String = reason.chars().take(120).collect();
            format!("First problem — {name}: {short}")
        });

        (message, detail)
    }
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Permanently delete the given files.
///
/// This is a *hard* delete (the file is removed, not moved to the trash). A path
/// that no longer exists is silently skipped — not counted as a failure — and a
/// path that names a directory is rejected rather than deleted, so a stray
/// directory in the selection can never be wiped.
pub fn delete_files(paths: &[PathBuf]) -> BulkResult {
    let mut result = BulkResult::default();

    for path in paths {
        // A missing file is "already handled" — don't report it as a failure.
        if !path.exists() {
            continue;
        }
        // Never delete a directory, even if one slipped into the selection.
        if !path.is_file() {
            result.failed += 1;
            result.errors.push((path.clone(), "not a regular file".to_string()));
            continue;
        }

        if let Err(e) = std::fs::remove_file(path) {
            result.failed += 1;
            result.errors.push((path.clone(), e.to_string()));
        } else {
            result.succeeded += 1;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Resize
// ---------------------------------------------------------------------------

/// Resize every file to `target_width` pixels *wide*, preserving the aspect ratio,
/// and write the result back over the original (its on-disk format is kept).
///
/// The width is read literally, matching the modal's "Target width (px)" field: a
/// value larger than the original upscales; a smaller one shrinks. Each image is
/// written to a sibling temp file and then atomically renamed over the original, so
/// a failure mid-write never leaves a half-written original behind.
///
/// Note: animated formats (e.g. a multi-frame GIF) keep only their first frame,
/// since the `image` crate decodes a single-frame `DynamicImage`.
pub fn resize_to_width(paths: &[PathBuf], target_width: u32) -> BulkResult {
    let mut result = BulkResult::default();

    for path in paths {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resize_one(path, target_width)
        })) {
            Ok(Ok(())) => result.succeeded += 1,
            Ok(Err(reason)) => {
                result.failed += 1;
                result.errors.push((path.clone(), reason));
            }
            Err(_) => {
                result.failed += 1;
                result.errors.push((
                    path.clone(),
                    "a problem occurred while resizing this image".to_string(),
                ));
            }
        }
    }

    result
}

fn resize_one(path: &Path, target_width: u32) -> std::result::Result<(), String> {
    use image::{imageops::Triangle, GenericImageView};

    let decoded = decode(path)?;
    let (w, h) = decoded.dimensions();
    if w == 0 || h == 0 {
        return Err("image has zero size".to_string());
    }

    // Scale the height to match the new width, keeping the aspect ratio.
    let scale = target_width as f64 / w.max(1) as f64;
    let new_w = target_width.max(1);
    let new_h = (h as f64 * scale).round().max(1.0) as u32;
    let resized = decoded.resize(new_w, new_h, Triangle);

    // Preserve the original file's format. Infer it from the extension (the same
    // approach `metadata` uses); an unknown extension can't be safely guessed.
    let format = image_format_for(path)?;

    // Write to a sibling temp file, then rename it over the original.
    let tmp = sibling_temp(path);
    resized
        .save_with_format(&tmp, format)
        .map_err(|e| format!("could not write resized image: {e}"))?;

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(format!("could not replace the original: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Rotate 90°
// ---------------------------------------------------------------------------

/// The direction of a quarter-turn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// 90° clockwise.
    Clockwise,
    /// 90° counter-clockwise (equivalently 270° clockwise).
    Counterclockwise,
}

/// Rotate every file a quarter-turn — 90° clockwise or counter-clockwise —
/// preserving the on-disk format, and write the result back over the original.
///
/// Like [`resize_to_width`], each image is written to a sibling temp file and then
/// atomically renamed over the original, so a failure mid-write never leaves a
/// half-written original behind. A quarter-turn swaps the image's width and height,
/// so the new file is the same picture turned on its side.
///
/// Note: animated formats (e.g. a multi-frame GIF) keep only their first frame,
/// since the `image` crate decodes a single-frame `DynamicImage`.
pub fn rotate_90(paths: &[PathBuf], direction: Rotation) -> BulkResult {
    let mut result = BulkResult::default();

    for path in paths {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rotate_one(path, direction)
        })) {
            Ok(Ok(())) => result.succeeded += 1,
            Ok(Err(reason)) => {
                result.failed += 1;
                result.errors.push((path.clone(), reason));
            }
            Err(_) => {
                result.failed += 1;
                result.errors.push((
                    path.clone(),
                    "a problem occurred while rotating this image".to_string(),
                ));
            }
        }
    }

    result
}

fn rotate_one(path: &Path, direction: Rotation) -> std::result::Result<(), String> {
    let decoded = decode(path)?;

    // A quarter-turn swaps width and height. `rotate90` is 90° clockwise;
    // `rotate270` is the counter-clockwise direction (270° clockwise).
    let rotated = match direction {
        Rotation::Clockwise => decoded.rotate90(),
        Rotation::Counterclockwise => decoded.rotate270(),
    };

    // Preserve the original file's format, exactly as `resize_one` does.
    let format = image_format_for(path)?;

    let tmp = sibling_temp(path);
    rotated
        .save_with_format(&tmp, format)
        .map_err(|e| format!("could not write rotated image: {e}"))?;

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(format!("could not replace the original: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Convert to JPEG
// ---------------------------------------------------------------------------

/// Write a JPEG copy of every file next to it, at `quality` (0–100).
///
/// The original is left untouched — a non-destructive convert: each `<name>.png`
/// becomes `<name>.jpg` in the same folder. A source already named `.jpg` is
/// re-encoded in place at the chosen quality. JPEG carries no alpha channel, so any
/// transparency is flattened to black.
pub fn convert_to_jpeg(paths: &[PathBuf], quality: u8) -> BulkResult {
    let mut result = BulkResult::default();

    for path in paths {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            convert_one(path, quality)
        })) {
            Ok(Ok(())) => result.succeeded += 1,
            Ok(Err(reason)) => {
                result.failed += 1;
                result.errors.push((path.clone(), reason));
            }
            Err(_) => {
                result.failed += 1;
                result.errors.push((
                    path.clone(),
                    "a problem occurred while converting this image".to_string(),
                ));
            }
        }
    }

    result
}

fn convert_one(path: &Path, quality: u8) -> std::result::Result<(), String> {
    use image::codecs::jpeg::JpegEncoder;

    let decoded = decode(path)?;
    // JPEG is always 3-channel; flatten any alpha (and luma/16-bit) to RGB8 so the
    // encoder's pixel-type requirement is always met.
    let rgb = decoded.to_rgb8();

    let target = jpeg_sibling(path);
    let tmp = sibling_temp(&target);

    let file = std::fs::File::create(&tmp).map_err(|e| format!("could not open output: {e}"))?;
    let mut encoder = JpegEncoder::new_with_quality(file, quality);
    encoder
        .encode_image(&rgb)
        .map_err(|e| format!("could not encode JPEG: {e}"))?;
    // Close the file handle before renaming, so Windows won't reject the rename of
    // an open handle.
    drop(encoder);

    match std::fs::rename(&tmp, &target) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(format!("could not write {}: {e}", target.display()))
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Open and decode an image, mirroring the thumbnail pipeline: a `BufReader` so
/// `ImageReader` can seek, guessed format, then `decode`.
fn decode(path: &Path) -> std::result::Result<image::DynamicImage, String> {
    let file = std::io::BufReader::new(
        std::fs::File::open(path).map_err(|e| format!("could not open image: {e}"))?,
    );
    image::ImageReader::new(file)
        .with_guessed_format()
        .map_err(|e| format!("could not open image: {e}"))?
        .decode()
        .map_err(|e| format!("could not decode image: {e}"))
}

/// The [`image::ImageFormat`] implied by a file's extension, used to write a
/// resized result back in the same format. An unknown or missing extension is a
/// hard error (we won't guess wrong).
fn image_format_for(path: &Path) -> std::result::Result<image::ImageFormat, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    image::ImageFormat::from_extension(&ext).ok_or_else(|| format!("unknown file format .{ext}"))
}

/// A sibling path with a unique, non-colliding name for an in-progress write.
///
/// A process-id suffix avoids clobbering a real file with the same base name; the
/// temp file is always removed or renamed away, so it never lingers.
fn sibling_temp(path: &Path) -> PathBuf {
    // `file_stem` yields an `OsStr`; lossy-encode it so we can build a string.
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    parent.join(format!("{stem}.{}.tmp", std::process::id()))
}

/// The `.jpg` sibling of `path` (its extension replaced with `jpg`), the output of
/// a non-destructive JPEG convert.
fn jpeg_sibling(path: &Path) -> PathBuf {
    // `file_stem` yields an `OsStr`; lossy-encode it for the new name.
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let mut out = path.to_path_buf();
    out.set_file_name(format!("{stem}.jpg"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A scratch directory unique to this test, with a single PNG of the given
    /// size. Returns the PNG's path.
    fn scratch_image(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        image::DynamicImage::new_rgba8(w, h).save(&path).expect("write scratch png");
        path
    }

    /// A fresh temp dir tagged for this test (unique per process, cleaned up first).
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rive_ops_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn delete_removes_the_files() {
        let dir = temp_dir("del");
        let a = scratch_image(&dir, "a.png", 8, 8);
        let b = scratch_image(&dir, "b.png", 8, 8);
        let missing = dir.join("missing.png");
        assert!(a.exists() && b.exists());

        let result = delete_files(&[a.clone(), b.clone(), missing]);
        assert!(!a.exists(), "a.png should be deleted");
        assert!(!b.exists(), "b.png should be deleted");
        // The missing file is skipped, not counted as a failure.
        assert_eq!(result.succeeded, 2, "the two present files should succeed");
        assert_eq!(result.failed, 0, "a missing file is skipped, not a failure");
        assert!(result.errors.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_refuses_directories() {
        let dir = temp_dir("deldir");
        let sub = dir.join("subdir");
        std::fs::create_dir(&sub).ok();

        let result = delete_files(&[sub.clone()]);
        assert!(sub.is_dir(), "a directory must never be deleted");
        assert_eq!(result.failed, 1, "a directory is reported as a failure");
        assert_eq!(result.succeeded, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_sets_width_and_keeps_aspect_ratio() {
        let dir = temp_dir("resize");
        let path = scratch_image(&dir, "shot.png", 200, 100);

        let result = resize_to_width(&[path.clone()], 50);
        assert_eq!(result.failed, 0, "resize should succeed: {:?}", result.errors);
        assert!(path.exists(), "the original file must still exist after resize");

        // Re-read it and confirm the new width, with the aspect ratio preserved.
        let decoded = image::open(&path).expect("reopen resized image");
        assert_eq!(decoded.width(), 50, "width should become the target");
        assert_eq!(decoded.height(), 25, "height should scale in ratio (100 -> 25)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_reports_undecodable_files() {
        let dir = temp_dir("resizebad");
        let bad = dir.join("broken.png");
        std::fs::write(&bad, b"not really a png").ok();

        let result = resize_to_width(&[bad.clone()], 50);
        assert_eq!(result.failed, 1, "an undecodable file should fail");
        assert_eq!(result.succeeded, 0);
        assert!(result.errors.iter().any(|(p, _)| p == &bad));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_creates_jpeg_sibling() {
        let dir = temp_dir("jpeg");
        let src = scratch_image(&dir, "photo.png", 64, 48);
        let out = dir.join("photo.jpg");

        let result = convert_to_jpeg(&[src.clone()], 85);
        assert_eq!(result.failed, 0, "convert should succeed: {:?}", result.errors);
        assert!(out.exists(), "a .jpg sibling should be created");
        assert!(src.exists(), "the original should remain (non-destructive)");
        // And the output really is a JPEG. `guess_format` reads bytes, so read the
        // freshly-written file first.
        let bytes = std::fs::read(&out).expect("read the written jpeg");
        assert_eq!(image::guess_format(&bytes).unwrap(), image::ImageFormat::Jpeg);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Rotate 90° -------------------------------------------------------

    /// A scratch PNG of `w`×`h` with one bright pixel in the top-left corner, so
    /// the two rotation directions are distinguishable. The image is deliberately
    /// non-square: a square looks identical after a 90° turn and couldn't tell the
    /// directions apart.
    fn corner_image(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        let mut img = image::RgbaImage::new(w, h);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(img).save(&path).expect("write corner png");
        path
    }

    #[test]
    fn rotate_clockwise_swaps_dimensions() {
        let dir = temp_dir("rotcw");
        let path = scratch_image(&dir, "shot.png", 200, 100);

        let result = rotate_90(&[path.clone()], Rotation::Clockwise);
        assert_eq!(result.failed, 0, "rotate should succeed: {:?}", result.errors);
        assert!(path.exists(), "the original file must still exist after rotate");

        // A quarter-turn swaps width and height.
        let decoded = image::open(&path).expect("reopen rotated image");
        assert_eq!(decoded.width(), 100, "width and height should swap");
        assert_eq!(decoded.height(), 200);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotate_counterclockwise_swaps_dimensions() {
        let dir = temp_dir("rotccw");
        let path = scratch_image(&dir, "shot.png", 200, 100);

        let result = rotate_90(&[path.clone()], Rotation::Counterclockwise);
        assert_eq!(result.failed, 0, "rotate should succeed: {:?}", result.errors);
        assert!(path.exists());

        let decoded = image::open(&path).expect("reopen rotated image");
        assert_eq!(decoded.width(), 100);
        assert_eq!(decoded.height(), 200);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotate_clockwise_and_counterclockwise_differ() {
        let dir = temp_dir("rotdir");
        // Two identical non-square sources so the two turns are comparable.
        let cw = corner_image(&dir, "cw.png", 16, 8);
        let ccw = corner_image(&dir, "ccw.png", 16, 8);

        let cw_res = rotate_90(&[cw.clone()], Rotation::Clockwise);
        let ccw_res = rotate_90(&[ccw.clone()], Rotation::Counterclockwise);
        assert_eq!(cw_res.failed, 0, "clockwise: {:?}", cw_res.errors);
        assert_eq!(ccw_res.failed, 0, "counter-clockwise: {:?}", ccw_res.errors);

        // A non-square image turned the two ways must land in different positions.
        let cw_bytes = std::fs::read(&cw).expect("read clockwise output");
        let ccw_bytes = std::fs::read(&ccw).expect("read counter-clockwise output");
        assert_ne!(
            cw_bytes, ccw_bytes,
            "clockwise and counter-clockwise must produce different images"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotate_reports_undecodable_files() {
        let dir = temp_dir("rotbad");
        let bad = dir.join("broken.png");
        std::fs::write(&bad, b"not really a png").ok();

        let result = rotate_90(&[bad.clone()], Rotation::Clockwise);
        assert_eq!(result.failed, 1, "an undecodable file should fail");
        assert_eq!(result.succeeded, 0);
        assert!(result.errors.iter().any(|(p, _)| p == &bad));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotate_outcome_reads_naturally() {
        let mut cw = BulkResult::default();
        cw.succeeded = 4;
        let (msg, detail) = cw.outcome(Operation::RotateClockwise);
        assert_eq!(msg, "Rotated 90° clockwise 4 image(s).");
        assert!(detail.is_none());

        let mut ccw = BulkResult::default();
        ccw.succeeded = 1;
        let (msg, _) = ccw.outcome(Operation::RotateCounterclockwise);
        assert_eq!(msg, "Rotated 90° counter-clockwise 1 image(s).");
    }

    #[test]
    fn summary_reads_naturally() {
        let mut ok = BulkResult::default();
        ok.succeeded = 3;
        let (msg, detail) = ok.outcome(Operation::Delete);
        assert_eq!(msg, "Deleted 3 image(s).");
        assert!(detail.is_none());

        let mut conv = BulkResult::default();
        conv.succeeded = 1;
        let (msg, _) = conv.outcome(Operation::ConvertJpeg);
        assert_eq!(msg, "Converted to JPEG 1 image(s).");

        let mut mixed = BulkResult::default();
        mixed.succeeded = 2;
        mixed.failed = 1;
        let (msg, detail) = mixed.outcome(Operation::Resize);
        assert!(msg.contains("Resized 2 of 3 image(s); 1 failed."), "got: {msg}");
        // No error recorded, so no detail line.
        assert!(detail.is_none());

        let empty = BulkResult::default();
        assert_eq!(empty.outcome(Operation::Delete).0, "Nothing to do.");
    }
}



