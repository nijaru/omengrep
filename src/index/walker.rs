use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;

/// Maximum file size to index (1MB).
const MAX_FILE_SIZE: u64 = 1_000_000;

/// Binary file extensions to skip.
const BINARY_EXTENSIONS: &[&str] = &[
    // Compiled/object files
    ".pyc",
    ".pyo",
    ".o",
    ".so",
    ".dylib",
    ".dll",
    ".bin",
    ".exe",
    ".a",
    ".lib",
    // Archives
    ".zip",
    ".tar",
    ".gz",
    ".bz2",
    ".xz",
    ".7z",
    ".rar",
    ".jar",
    ".war",
    ".whl",
    // Documents/media
    ".pdf",
    ".doc",
    ".docx",
    ".xls",
    ".xlsx",
    ".ppt",
    ".pptx",
    // Images
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".ico",
    ".svg",
    ".webp",
    ".bmp",
    ".tiff",
    // Audio/video
    ".mp3",
    ".mp4",
    ".wav",
    ".avi",
    ".mov",
    ".mkv",
    // Data files
    ".db",
    ".sqlite",
    ".sqlite3",
    ".pkl",
    ".npy",
    ".npz",
    ".onnx",
    ".pt",
    ".pth",
    ".safetensors",
    // Lock files
    ".lock",
];

/// Scan directory tree for text files, returning path -> content map.
pub fn scan(root: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut results = HashMap::new();

    let walker = WalkBuilder::new(root)
        .hidden(true) // Process hidden files check manually
        .git_ignore(true) // Respect .gitignore
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .max_filesize(Some(MAX_FILE_SIZE))
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Skip directories
        if entry.file_type().map_or(true, |ft| !ft.is_file()) {
            continue;
        }

        let path = entry.path();

        // Skip hidden files (starting with .)
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }

        // Skip binary extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = format!(".{}", ext.to_lowercase());
            if BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
                continue;
            }
        }

        // Skip lock json files
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with("-lock.json") {
                continue;
            }
        }

        // Read and check for binary content
        let raw = match std::fs::read(path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        // Binary detection: null byte in first 8192 bytes
        let check_len = raw.len().min(8192);
        if raw[..check_len].contains(&0) {
            continue;
        }

        // Decode as UTF-8
        let content = match String::from_utf8(raw) {
            Ok(s) => s,
            Err(_) => continue,
        };

        results.insert(path.to_path_buf(), content);
    }

    Ok(results)
}
