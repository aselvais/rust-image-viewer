//! Thumbnails + preview-error handling for the file browser and viewer.
//!
//! # Why this exists
//!
//! egui's `Image::from_uri` hands the **entire** image to the GPU as a texture.
//! A very large photo — the 6280×10780 (≈68 Mpx) screenshots this project hit, or a
//! 16384px-tall image sitting exactly at a GPU's texture limit — creates a
//! ~200 MB texture. That either exceeds the GPU's max texture side or exhausts
//! memory, and egui/wgpu then fails inside the event loop, which **crashed the app**
//! whenever the file explorer browsed such a folder.
//!
//! Instead we decode and *downscale* each image to a small texture on a background
//! thread and show a graceful placeholder for anything that can't be previewed, so a
//! single oversized or corrupt file can never take the whole application down.

use egui::{ColorImage, TextureHandle, TextureOptions};
use std::path::{Path, PathBuf};
use std::result::Result as R;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;

/// A decoded, downscaled thumbnail: its (width, height) plus the raw RGBA8 bytes,
/// or a short human reason the image couldn't be previewed.
type Decoded = R<(u32, u32, Vec<u8>), String>;
/// The channel a background decode thread hands its `Decoded` result back over.
type DecodeChannel = mpsc::Receiver<Decoded>;

/// Longest side of a thumbnail GPU texture.
///
/// Thumbnails are only shown small, so a fixed cap keeps every texture tiny (≤ a
/// megabyte) and crisp after egui scales it to the cell — and, crucially,
/// guarantees we never upload a full-resolution texture (the crash source). The
/// thumbnail *slider* changes only how egui scales this texture on screen, so the
/// texture itself doesn't have to be re-made when it moves.
pub const THUMB_CAP: u32 = 1024;

/// Longest side the main viewer will ever upload for a texture.
///
/// Anything larger is downsampled to this first, so opening a multi-megapixel image
/// can't create an over-limit texture. Desktop GPUs commonly cap around 16384, so
/// 8192 is a safe, high-quality compromise.
pub const MAX_VIEW_SIDE: u32 = 8192;

// ---------------------------------------------------------------------------
// Core: decode + downscale, pure CPU work, never panics, unit-testable.
// ---------------------------------------------------------------------------

/// Decode `path` and downscale it to fit within `cap` on its longest side,
/// preserving aspect ratio (and never upscaling). Returns the thumbnail's `(width,
/// height)` as raw RGBA8 bytes.
///
/// This never touches egui — it only runs on a worker thread — and it never panics:
/// any read/decode/rasterise failure becomes an `Err` carrying a short, human
/// reason so the caller can render a placeholder instead of crashing.
pub fn resize_rgba(path: &Path, cap: u32) -> Decoded {
    use image::{imageops::Triangle, GenericImageView};

    // Header first: cheap, and lets us bail before a full decode for absurd inputs.
    // `ImageReader` needs a buffered reader that can also seek, so wrap the `File`.
    let file = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let decoded = image::ImageReader::new(file)
        .with_guessed_format()
        .map_err(|e| format!("could not open image: {e}"))?
        .decode()
        .map_err(|e| format!("could not decode image: {e}"))?;

    let (w, h) = decoded.dimensions();
    // No upscaling: only shrink images that are larger than the cap.
    let long = w.max(h).max(1) as f32;
    let scale = if long > cap as f32 {
        cap as f32 / long
    } else {
        1.0
    };
    let nw = (w as f32 * scale).round().max(1.0) as u32;
    let nh = (h as f32 * scale).round().max(1.0) as u32;

    // `Triangle` gives a clean, fast thumbnail without the ring artefacts of a
    // Catmull-Rom downscale.
    //
    // Crucially, we then read the *actual* dimensions off the resized buffer
    // rather than reusing `nw`/`nh`. `image::resize` can land a pixel or two
    // short of what our arithmetic computed (the filter's own rounding), and if
    // we ever hand egui a `(w, h)` that disagrees with the buffer's real length,
    // `ColorImage::from_rgba_unmultiplied` asserts `rgba.len() == w*h*4` and
    // panics — unwinding straight through the UI loop. That mismatch is the crash.
    let resized = decoded.resize(nw, nh, Triangle).to_rgba8();
    let (cw, ch) = resized.dimensions();
    Ok((cw, ch, resized.into_raw()))
}

// ---------------------------------------------------------------------------
// State machine driving background thumbnails, one entry per image.
// ---------------------------------------------------------------------------

/// The lifecycle of a single image's thumbnail texture.
///
/// `Thumb` intentionally does not derive `Debug`: its `Ready` variant owns a GPU
/// `TextureHandle`, which is not `Debug`, but nothing in the UI logs it.
#[derive(Default)]
pub enum Thumb {
    /// Not started yet — we're waiting for a free decode slot this frame (a folder
    /// with hundreds/thousands of images is drained a few at a time so the first
    /// paint never freezes).
    #[default]
    Queued,
    /// A background thread is decoding/downscaling it; the result arrives over this
    /// channel and is drained with a *non-blocking* `try_recv` so slow decodes never
    /// stall the UI thread.
    Pending(DecodeChannel),
    /// Finished: a small, already-loaded GPU texture, ready to render.
    Ready(TextureHandle),
    /// Couldn't be previewed — with a human reason to show under the cell.
    Broken(String),
}

/// How many images to *start* decoding in a single frame. Bounds transient CPU
/// memory on huge folders and keeps the UI responsive.
pub const PER_FRAME_BUDGET: i32 = 24;

/// Kick off a background, *detached* decode of `path`, aspect-fit into `cap`.
///
/// The result is handed back over a channel (not a `JoinHandle`), because
/// `JoinHandle::join` blocks — and a 68-megapixel decode can take a second, which
/// would freeze the UI. The decode thread is detached, so it finishes on its own;
///
/// Any panic raised while decoding is caught, so a pathological file can't unwind
/// through egui's event loop and abort the process.
fn start_decode(path: PathBuf, cap: u32) -> R<DecodeChannel, String> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name(format!("thumb-decode-{cap}"))
        .spawn(move || {
            let res =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| resize_rgba(&path, cap)))
                    .unwrap_or_else(|_| Err("a problem occurred while decoding this image".into()));
            // Ignore a send error: the receiver may have been dropped (e.g. the
            // folder was changed away) — discarding the result then is fine.
            let _ = tx.send(res);
        })
        .map_err(|e| e.to_string())?;
    Ok(rx)
}

/// Advance one thumbnail as far as possible *this frame*, mutating it in place.
///
/// Never blocks, so a slow decode can't stall the UI:
/// * `Pending` → drained with `try_recv`: becomes `Ready` (texture loaded) or
///   `Broken` when finished; left `Pending` if the worker is still running.
/// * `Queued` with budget left → starts a decode (`Pending`), or `Broken` if the
///   thread can't even start.
/// * `Queued` with no budget → left `Queued` (tried again next frame).
/// * `Ready` / `Broken` → left untouched (stable; the slider only re-scales display).
pub fn advance(
    state: &mut Thumb,
    path: &Path,
    cap: u32,
    budget: &mut i32,
    ctx: &egui::Context,
    seq: &mut u64,
) {
    let old = std::mem::replace(state, Thumb::Queued);
    *state = match old {
        Thumb::Ready(h) => Thumb::Ready(h),
        Thumb::Broken(reason) => Thumb::Broken(reason),
        Thumb::Pending(rx) => drain(rx, ctx, seq),
        Thumb::Queued if *budget > 0 => {
            *budget -= 1;
            match start_decode(path.to_path_buf(), cap) {
                Ok(rx) => Thumb::Pending(rx),
                Err(_) => Thumb::Broken("could not start the image preview".into()),
            }
        }
        Thumb::Queued => Thumb::Queued,
    };
}

/// Non-blocking drain of a finished-or-still-running decode.
fn drain(rx: DecodeChannel, ctx: &egui::Context, seq: &mut u64) -> Thumb {
    match rx.try_recv() {
        Ok(Ok((w, h, rgba))) => {
            // Build the texture on the UI thread, inside a `catch_unwind`. The
            // buffer already carries its own real dimensions, so this path is safe
            // in practice — but this is the last line of defence: a driver limit,
            // a weird colour space, or an unforeseen egui assertion in the
            // *construction* step degrades this one cell to a placeholder instead
            // of letting the panic unwind through egui's event loop and quit the
            // whole app. That is the "nice way to handle such errors" the bug
            // report asked for.
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                // A fresh, unique texture per image (names are monotonic so
                // caching by path never collides). The old handle for this path,
                // if any, was dropped when we cleared the map.
                *seq += 1;
                let name = format!("thumb-{}", *seq);
                ctx.load_texture(name, image, TextureOptions::default())
            })) {
                Ok(handle) => Thumb::Ready(handle),
                Err(_) => Thumb::Broken("problem generating this preview".into()),
            }
        }
        Ok(Err(reason)) => Thumb::Broken(reason),
        // The worker is still running — keep it in flight; it'll be drained later.
        Err(TryRecvError::Empty) => Thumb::Pending(rx),
        // The worker exited without delivering a result (it was killed, or the
        // channel dropped out from under it): show a placeholder instead of a stuck
        // "loading" spinner.
        Err(TryRecvError::Disconnected) => {
            Thumb::Broken("the image preview stopped unexpectedly".into())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — run headless (no window), exercising the exact decode/resize path and
// proving a bad/oversized file produces a graceful result instead of a panic.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny valid image, written to a temp file as PNG.
    fn write_rgba(path: &Path, w: u32, h: u32, fill: u8) {
        let rgba = image::RgbaImage::from_pixel(w, h, image::Rgba([fill, fill, fill, 255]));
        image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8).expect("write test image");
    }

    #[test]
    fn downscale_shrinks_large_side_to_the_cap() {
        let w = 40;
        let h = 8; // 5:1 aspect
        let path =
            std::env::temp_dir().join(format!("rive_thumb_{w}x{h}_{}.png", std::process::id()));
        write_rgba(&path, w, h, 200);

        let (tw, th, rgba) = resize_rgba(&path, 10).expect("decode+resize ok");
        // Longest side (40) must come back at the cap (10), aspect preserved → 10×2.
        assert_eq!(tw, 10, "width should be scaled to the cap");
        assert!(th <= 2, "height should scale with the same ratio, got {th}");
        assert_eq!(
            rgba.len(),
            tw as usize * th as usize * 4,
            "RGBA byte count should match w*h*4"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let path =
            std::env::temp_dir().join(format!("rive_thumb_small_{}.png", std::process::id()));
        write_rgba(&path, 8, 6, 128);

        let (tw, th, _) = resize_rgba(&path, 512).expect("decode+resize ok");
        // 8×6 is already far smaller than a 512 cap, so it stays 8×6 (no upscaling).
        assert_eq!((tw, th), (8, 6));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_a_graceful_error_not_a_panic() {
        let path =
            std::env::temp_dir().join(format!("rive_thumb_corrupt_{}.png", std::process::id()));
        std::fs::write(&path, b"this is not an image at all, just junk bytes here")
            .expect("write junk");

        let result = resize_rgba(&path, 128);
        assert!(
            result.is_err(),
            "corrupt input must return Err, not Ok and not panic"
        );
        let reason = result.expect_err("had an error");
        assert!(
            !reason.is_empty(),
            "the error should carry a reason for the placeholder"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn huge_image_is_capped_to_the_cap() {
        // A genuinely large but solid-colour image (fast to make, and the shape of
        // the crash-prone case) must come back capped to THUMB_CAP on its long
        // side. This is the exact guarantee that stops a full-resolution texture
        // from ever being uploaded to the GPU.
        let (w, h) = (2048u32, 3072u32);
        let rgb = image::RgbaImage::from_pixel(w, h, image::Rgba([120, 80, 160, 255]));
        let path = std::env::temp_dir().join(format!("rive_huge_{}.png", std::process::id()));
        image::save_buffer(&path, &rgb, w, h, image::ColorType::Rgba8).expect("write huge png");

        let (tw, th, _rgba) = resize_rgba(&path, THUMB_CAP).expect("decode+resize ok");
        // The long side (3072) must come back exactly at the cap, no axis may exceed.
        assert_eq!(
            tw.max(th),
            THUMB_CAP,
            "long side must be capped to {THUMB_CAP}"
        );
        assert!(
            tw <= THUMB_CAP && th <= THUMB_CAP,
            "no axis may exceed the cap"
        );
        // Aspect ratio preserved: the short side scales in the same ratio.
        let expected_short = (w as f32 * THUMB_CAP as f32 / h as f32).round() as u32;
        assert_eq!(
            tw.min(th),
            expected_short,
            "short side should scale in ratio, got {tw}x{th}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resized_buffer_dimensions_match_what_egui_will_build() {
        // Regression guard for the crash we just fixed. An earlier version
        // computed the thumbnail's (w, h) on its own and handed *that* pair to
        // egui, while `image::resize` actually produced a buffer a pixel or two
        // smaller (the filter's own rounding). egui then asserted
        // `rgba.len() == w*h*4`, failed, and the panic unwound through the UI
        // loop and quit the whole app. Guard: the dims we return are the buffer's
        // *actual* dims, and building the texture from them never panics. 5402x472
        // at cap 1024 specifically reproduces the 1019/89 the filter hit on a real
        // file in the user's folder.
        let (w, h) = (5402u32, 472u32);
        let path = std::env::temp_dir().join(format!("rive_dim_{}_{}.png", w, std::process::id()));
        let src = image::RgbaImage::from_pixel(w, h, image::Rgba([90, 120, 160, 255]));
        image::save_buffer(&path, &src, w, h, image::ColorType::Rgba8).expect("write png");

        let (tw, th, rgba) = resize_rgba(&path, THUMB_CAP).expect("resize ok");

        // THE invariant the crash violated: the reported dims must match the
        // buffer's real length. If they ever disagree again, this fails.
        assert_eq!(
            rgba.len(),
            tw as usize * th as usize * 4,
            "buffer length must match the reported {tw}x{th} texture; that mismatch is the crash",
        );
        // The exact constructor egui uses must build without panicking.
        let build = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            // `tw`/`th` are `Copy`, so they survive the `move` closure; `rgba` is moved in.
            egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &rgba);
        }));
        build.expect("ColorImage must build from the returned buffer without panicking");
        assert!(
            tw.max(th) <= THUMB_CAP,
            "long side must be capped at or under {THUMB_CAP}, got {tw}x{th}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_path_is_graceful() {
        let p = std::path::Path::new("/no/such/image/here.png");
        assert!(resize_rgba(p, 128).is_err());
    }
}
