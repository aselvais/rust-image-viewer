//! Image metadata extraction: dimensions, format, colour info, on-disk size,
//! modification time, and EXIF tags (via `kamadak-exif`).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use image::ColorType;

/// Everything the details window can show for one image.
#[derive(Clone, Debug)]
pub struct ImageMetadata {
    /// Absolute path of the file.
    pub path: String,
    /// Human-friendly format name, e.g. `"PNG"`, `"JPEG"`.
    pub format: String,
    /// Pixel width and height.
    pub width: u32,
    pub height: u32,
    /// Human-friendly colour description, e.g. `"RGB (8-bit)"`.
    pub color: String,
    /// Number of components / channels.
    pub channels: u8,
    /// Bits stored per component.
    pub bit_depth: u16,
    /// Size on disk, in bytes.
    pub file_size: u64,
    /// Last-modified time as a UTC string, if available.
    pub modified: Option<String>,
    /// Parsed EXIF fields, as `(human_name, value)` pairs.
    pub exif: Vec<(String, String)>,
    /// Set if the image could not be decoded.
    pub decode_error: Option<String>,
}

impl ImageMetadata {
    /// Read a file and extract as much metadata as possible.
    ///
    /// Best-effort: any field that can't be computed is left blank / `None`,
    /// and the rest are still returned.
    pub fn load(path: &Path) -> Self {
        let mut meta = Self {
            path: path.to_string_lossy().into_owned(),
            format: "unknown".to_string(),
            width: 0,
            height: 0,
            color: String::new(),
            channels: 0,
            bit_depth: 0,
            file_size: 0,
            modified: None,
            exif: Vec::new(),
            decode_error: None,
        };

        // On-disk size, if any.
        if let Ok(m) = std::fs::metadata(path) {
            meta.file_size = m.len();
            meta.modified = m.modified().ok().map(|t| fmt_modified_utc(t));
        }

        // Read the whole file once: reused for format detection, decoding and EXIF.
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                meta.decode_error = Some(format!("could not read file: {e}"));
                return meta;
            }
        };

        // Format: guess from the bytes (falls back to the file extension).
        let format = match image::guess_format(&bytes) {
            Ok(f) => Some(f),
            Err(_) => image::ImageFormat::from_extension(
                path.extension().and_then(|e| e.to_str()).unwrap_or(""),
            )
            .or_else(|| image::ImageFormat::from_extension("")),
        };
        meta.format = human_format(format.as_ref());

        // Dimensions + colour via a full decode (on demand, so cheap enough).
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                meta.width = img.width();
                meta.height = img.height();
                let (color, channels, bits) = describe_color(img.color());
                meta.color = color;
                meta.channels = channels;
                meta.bit_depth = bits;
            }
            Err(e) => {
                meta.decode_error = Some(format!("could not decode image: {e}"));
            }
        }

        // EXIF only makes sense for a couple of container formats.
        meta.exif = extract_exif(&bytes, format);

        meta
    }
}

// ----------------------------------------------------------------------------
// Human-friendly helpers

fn human_format(fmt: Option<&image::ImageFormat>) -> String {
    let Some(fmt) = fmt else {
        return "unknown".into();
    };
    match fmt {
        image::ImageFormat::Png => "PNG",
        image::ImageFormat::Jpeg => "JPEG",
        image::ImageFormat::Gif => "GIF",
        image::ImageFormat::Bmp => "Bitmap",
        image::ImageFormat::WebP => "WebP",
        image::ImageFormat::Tiff => "TIFF",
        image::ImageFormat::Tga => "TGA",
        image::ImageFormat::Pnm => "PNM",
        image::ImageFormat::Dds => "DDS",
        image::ImageFormat::Ico => "ICO",
        image::ImageFormat::Hdr => "HDR (Radiance)",
        image::ImageFormat::OpenExr => "OpenEXR",
        image::ImageFormat::Farbfeld => "Farbfeld",
        image::ImageFormat::Avif => "AVIF",
        image::ImageFormat::Qoi => "QOI",
        _ => {
            return fmt
                .extensions_str()
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown".into())
        }
    }
    .to_string()
}

fn describe_color(ct: ColorType) -> (String, u8, u16) {
    match ct {
        ColorType::L8 => ("Grayscale (8-bit)".into(), 1, 8),
        ColorType::La8 => ("Grayscale + alpha (8-bit)".into(), 2, 8),
        ColorType::L16 => ("Grayscale (16-bit)".into(), 1, 16),
        ColorType::La16 => ("Grayscale + alpha (16-bit)".into(), 2, 16),
        ColorType::Rgb8 => ("RGB (8-bit)".into(), 3, 8),
        ColorType::Rgba8 => ("RGBA (8-bit)".into(), 4, 8),
        ColorType::Rgb16 => ("RGB (16-bit)".into(), 3, 16),
        ColorType::Rgba16 => ("RGBA (16-bit)".into(), 4, 16),
        ColorType::Rgb32F => ("RGB float (32-bit)".into(), 3, 32),
        ColorType::Rgba32F => ("RGBA float (32-bit)".into(), 4, 32),
        // `ColorType` is `#[non_exhaustive]`, so a catch-all is required.
        _ => ("Other pixel format".to_string(), 0, 0),
    }
}

/// Format a byte count as a human-readable string.
pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

// ----------------------------------------------------------------------------
// EXIF extraction (JPEG + TIFF containers)

fn extract_exif(bytes: &[u8], format: Option<image::ImageFormat>) -> Vec<(String, String)> {
    let fields: Vec<exif::Field> = match format {
        Some(image::ImageFormat::Jpeg) | None => {
            // JPEG: EXIF lives in the APP1 segment.
            let mut reader = std::io::BufReader::new(bytes);
            match exif::get_exif_attr_from_jpeg(&mut reader) {
                Ok(seg) if !seg.is_empty() => {
                    exif::parse_exif(&seg).map(|(f, _)| f).unwrap_or_default()
                }
                _ => Vec::new(),
            }
        }
        Some(image::ImageFormat::Tiff) => {
            exif::parse_exif(bytes).map(|(f, _)| f).unwrap_or_default()
        }
        _ => Vec::new(),
    };

    fields
        .iter()
        .map(|f| {
            let name = f.tag.description().unwrap_or("Unknown tag").to_string();
            let mut value = f.value.display_as(f.tag).to_string();
            // Tidy obvious binary dumps that aren't useful as text.
            clean_value(&mut value, f);
            (name, value)
        })
        .filter(|(_, v)| !v.trim().is_empty())
        .collect()
}

fn clean_value(value: &mut String, field: &exif::Field) {
    // Some tags hold long binary blobs (e.g. thumbnails); collapse them.
    if value.len() > 512 {
        *value = format!("«binary data, {} bytes»", value.len());
        return;
    }
    // Trim a single trailing NUL / whitespace that EXIF ASCII strings carry.
    let trimmed = value.trim_end();
    *value = trimmed.to_string();
    let _ = field; // tag kept for future use; currently unused.
}

/// Convert a UNIX timestamp to a readable UTC date string.
fn fmt_modified_utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let hh = (secs % 3_600) / 3_600;
    let mm = (secs % 3_600) / 60;
    let ss = secs % 60;
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

/// Days-since-civil-epoch to (year, month, day).
/// Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u64, u64) {
    let z = z + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 2) as u64;
    let m = (mp + 3 + 12 * (mp / 10)) as u64;
    let y = y + mp / 10;
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::ImageMetadata;

    #[test]
    fn extracts_dimensions_format_and_colour_for_png() {
        // A 320x240 RGB PNG written to a temp file.
        let img = image::DynamicImage::new_rgb8(320, 240);
        let path = std::env::temp_dir().join("rive_meta_test.png");
        img.save(&path).expect("write test png");

        let meta = ImageMetadata::load(path.as_ref());
        assert!(
            meta.decode_error.is_none(),
            "decode_error: {:?}",
            meta.decode_error
        );
        assert_eq!(meta.width, 320);
        assert_eq!(meta.height, 240);
        assert_eq!(meta.format, "PNG");
        assert_eq!(meta.channels, 3);
        assert_eq!(meta.bit_depth, 8);
        assert!(meta.file_size > 0, "file_size should be > 0");
        assert!(meta.modified.is_some(), "modified time should be present");
        // A PNG carries no EXIF, so this branch must be empty.
        assert!(meta.exif.is_empty(), "unexpected EXIF: {:?}", meta.exif);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn handles_nonexistent_gracefully() {
        let meta = ImageMetadata::load(std::path::Path::new("/does/not/exist/xyz.png"));
        assert!(meta.decode_error.is_some());
        assert!(meta.width == 0 && meta.exif.is_empty());
    }

    #[test]
    fn exif_extraction_reads_jpeg_tags_when_present() {
        use std::process::Command;

        // Skip when exiftool isn't installed (keeps CI green).
        let have = Command::new("sh")
            .arg("-c")
            .arg("command -v exiftool")
            .output()
            .expect("detect exiftool");
        if !have.status.success() {
            eprintln!("(skipping EXIF test: exiftool not found)");
            return;
        }

        let dir = std::env::temp_dir();
        // Unique name avoids collisions across parallel/stale runs.
        let pid = std::process::id();
        let src = dir.join(format!("rive_exif_src_{}.jpg", pid));
        // Start from a clean file.
        let _ = std::fs::remove_file(&src);
        image::DynamicImage::new_rgb8(64, 48)
            .save(&src)
            .expect("write src jpeg");

        // Add a few known EXIF tags in place (no `-o`, avoids overwrite-collision).
        let status = Command::new("exiftool")
            .args([
                "-overwrite_original",
                "-Artist=Test Author",
                "-ImageDescription=Hello EXIF",
                "-DateTimeOriginal=2024:01:15 10:30:00",
            ])
            .arg(&src)
            .status()
            .expect("run exiftool");
        if !status.success() {
            eprintln!("(skipping EXIF test: exiftool failed)");
            let _ = std::fs::remove_file(&src);
            return;
        }

        let meta = ImageMetadata::load(src.as_ref());
        assert!(
            meta.decode_error.is_none(),
            "decode_error: {:?}",
            meta.decode_error
        );
        assert!(
            !meta.exif.is_empty(),
            "expected some EXIF fields to be extracted"
        );

        let joined = meta
            .exif
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        // At least one of the tags we wrote should come back as a human name.
        let ok = joined.contains("Artist")
            || joined.contains("ImageDescription")
            || joined.contains("Image Description")
            || joined.contains("DateTime")
            || joined.contains("Date");
        assert!(ok, "none of the expected EXIF tags found in: {joined}");

        let _ = std::fs::remove_file(&src);
    }
}
