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
    entries.sort_by(|a, b| {
        match (&a.file_type, &b.file_type) {
            (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
            (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
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
