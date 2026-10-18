use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub path: PathBuf,
    pub file_type: FileType,
}

pub fn list_directory(dir: &Path) -> Vec<FileInfo> {
    let mut entries = Vec::new();

    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            let file_type = if path.is_dir() {
                FileType::Directory
            } else {
                FileType::File
            };

            entries.push(FileInfo {
                name,
                path,
                file_type,
            });
        }
    }

    // Sort: Directories first, then files
    entries.sort_by(|a, b| match (&a.file_type, &b.file_type) {
        (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
        (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    entries
}

pub fn is_image(path: &Path) -> bool {
    let extensions = ["jpg", "jpeg", "png", "gif", "bmp", "webp"];
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return extensions.contains(&ext.to_lowercase().as_str());
    }
    false
}

/// Build the `file://` URI that egui's image loader (`egui_extras::FileLoader`)
/// expects for a local image, correctly on every platform.
///
/// The loader trims the `file://` prefix, then on Windows requires the remainder to
/// start with `/` — otherwise it prepends `\` and builds a bogus UNC path. A naive
/// `format!("file://{path}", path = p.to_string_lossy())` therefore works on Unix
/// (paths start with `/`, giving `file:///Users/...`) but on Windows yields
/// `file://C:\...`, which the loader turns into `\\C:\...` and the open fails — so
/// no texture loads and egui shows its broken-image placeholder (a red `!` triangle).
///
/// We normalise to forward slashes and, on Windows, prepend the extra slash so the
/// loader yields a valid `C:/...` path.
pub fn image_uri(path: &Path) -> String {
    normalize_file_uri(&path.to_string_lossy(), cfg!(target_os = "windows"))
}

/// Pure, platform-parameterised core of [`image_uri`] so the Windows branch is
/// testable on any host. `windows == true` takes the three-slash, forward-slash form
/// that `egui_extras`' loader accepts on Windows; otherwise the Unix `file://` +
/// absolute-path form.
fn normalize_file_uri(path: &str, windows: bool) -> String {
    // Backslashes are not legal inside a URI path; normalise to forward slashes so
    // the string is a well-formed URI on every platform.
    let forward = path.replace('\\', "/");
    if windows {
        // `file:///C:/Users/...` — the loader strips `file://` then the leading `/`,
        // leaving `C:/Users/...`, a path `std::fs` resolves correctly on Windows.
        format!("file:///{forward}")
    } else {
        // Unix/other: absolute paths already start with `/`, so the two-slash prefix
        // becomes a valid three-slash `file:///...` URI.
        format!("file://{forward}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_uri_is_valid_file_uri() {
        assert_eq!(
            normalize_file_uri("/Users/a/pics/cat.jpg", false),
            "file:///Users/a/pics/cat.jpg"
        );
    }

    #[test]
    fn windows_uri_is_three_slash_forward_slash() {
        // The exact form egui_extras' loader accepts on Windows. Before the fix the
        // app produced `file://C:\Users\...`, which the loader turned into a broken
        // `\\C:\...` path.
        let uri = normalize_file_uri("C:\\Users\\a\\pics\\cat.jpg", true);
        assert_eq!(uri, "file:///C:/Users/a/pics/cat.jpg");
        assert!(uri.starts_with("file:///C:/"));
    }

    #[test]
    fn windows_uri_never_has_double_backslash_prefix() {
        // Regression guard: the old `file://` + backslash path is the bug.
        let uri = normalize_file_uri("C:\\Users\\a", true);
        assert!(!uri.contains("file://\\"), "produced a broken URI: {uri}");
        assert!(
            uri.starts_with("file:///"),
            "expected three-slash form: {uri}"
        );
    }

    #[test]
    fn path_with_spaces_is_preserved() {
        // The loader does not percent-decode, so spaces stay literal; the URI is
        // still well-formed forward-slash.
        assert_eq!(
            normalize_file_uri("C:\\pics\\my cat.jpg", true),
            "file:///C:/pics/my cat.jpg",
        );
    }
}
