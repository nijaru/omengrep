use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use ignore::{WalkBuilder, WalkState};

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
    ".out",
    ".app",
    ".ipa",
    ".apk",
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
    ".dmg",
    ".iso",
    // Documents/media
    ".pdf",
    ".doc",
    ".docx",
    ".xls",
    ".xlsx",
    ".ppt",
    ".pptx",
    ".epub",
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
    ".raw",
    ".psd",
    ".ai",
    // Audio/video
    ".mp3",
    ".mp4",
    ".wav",
    ".avi",
    ".mov",
    ".mkv",
    ".flv",
    ".wmv",
    ".m4a",
    // Data/Database files
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
    ".model",
    ".bin",
    ".weights",
    ".h5",
    ".tflite",
    ".parquet",
    ".avro",
    ".orc",
    // Lock files
    ".lock",
];

/// Patterns and extensions for sensitive files (secrets, credentials, etc.)
const SENSITIVE_PATTERNS: &[&str] = &[
    // Credentials and Secrets
    ".env",
    ".env.local",
    ".env.development",
    ".env.test",
    ".env.production",
    ".password",
    ".secret",
    ".token",
    ".key",
    // Cloud/Platform Credentials
    "credentials",
    "config", // Often in .aws/ or .google/
    "service-account.json",
    "client_secret.json",
    // SSH Keys
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "authorized_keys",
    "known_hosts",
    // Certificates
    ".pem",
    ".crt",
    ".cer",
    ".pfx",
    ".p12",
    ".jks",
    ".keystore",
    // Infrastructure/CI State
    ".tfstate",
    ".tfstate.backup",
    "terraform.tfstate",
    // Database Journals/Dumps
    ".db-journal",
    ".sql.gz",
    ".sql.bz2",
];

/// Metadata for a scanned file: (file_size, mtime_secs).
pub type FileMetadata = (u64, u64);

/// Check if a file path should be skipped during scanning.
fn should_skip(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return true,
    };

    // Hidden files and specific exclusion patterns
    if name.starts_with('.') || name.ends_with("-lock.json") {
        // Double check for common env patterns that might not be hidden on all OSes
        if name.starts_with(".env") {
            return true;
        }
        return true;
    }

    // Sensitive patterns (case-insensitive)
    let name_lower = name.to_lowercase();
    for pattern in SENSITIVE_PATTERNS {
        if name_lower == pattern.to_lowercase() || name_lower.ends_with(&pattern.to_lowercase()) {
            return true;
        }
    }

    // Binary extensions (case-insensitive)
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = format!(".{}", ext.to_lowercase());
        if BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
            return true;
        }
    }

    false
}

/// Build a directory walker with standard filtering options.
fn build_walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .max_filesize(Some(MAX_FILE_SIZE))
        .build()
}

/// Scan directory tree for file metadata only (no content reads).
/// Returns path -> (file_size, mtime_secs) for each eligible file.
pub fn scan_metadata(root: &Path) -> Result<HashMap<PathBuf, FileMetadata>> {
    let mut results = HashMap::new();

    for entry in build_walker(root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_none_or(|ft| !ft.is_file()) {
            continue;
        }

        let path = entry.path();
        if should_skip(path) {
            continue;
        }

        if let Ok(meta) = std::fs::metadata(path) {
            let size = meta.len();
            let mtime = meta
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            results.insert(path.to_path_buf(), (size, mtime));
        }
    }

    Ok(results)
}

/// Get mtime for a single file path.
pub fn file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read a text file using an already-captured mtime.
pub fn read_text(path: &Path, mtime: u64) -> Option<(String, u64)> {
    let raw = std::fs::read(path).ok()?;

    // Binary detection: null byte in first 8192 bytes
    let check_len = raw.len().min(8192);
    if raw[..check_len].contains(&0) {
        return None;
    }

    String::from_utf8(raw).ok().map(|content| (content, mtime))
}

/// Scan directory tree for text files, returning path -> (content, mtime).
/// mtime is captured before reading content so it's never newer than what was read.
pub fn scan(root: &Path) -> Result<HashMap<PathBuf, (String, u64)>> {
    let (tx, rx) = std::sync::mpsc::channel();

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .max_filesize(Some(MAX_FILE_SIZE))
        .build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |result| {
            let entry = match result {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                return WalkState::Continue;
            }

            let path = entry.path();
            if should_skip(path) {
                return WalkState::Continue;
            }

            // Stat before read so mtime is never newer than the content we index
            let mtime = file_mtime(path);

            let Some(data) = read_text(path, mtime) else {
                return WalkState::Continue;
            };

            let _ = tx.send((path.to_path_buf(), data));
            WalkState::Continue
        })
    });

    drop(tx);
    let mut results = HashMap::new();
    for (path, data) in rx {
        results.insert(path, data);
    }

    Ok(results)
}
