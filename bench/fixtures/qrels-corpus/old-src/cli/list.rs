use std::path::Path;

use anyhow::Result;

use crate::index::{self, SemanticIndex};

pub fn run(path: &Path) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let indexes = index::find_subdir_indexes(&path, true);

    if indexes.is_empty() {
        eprintln!("No indexes found");
        return Ok(());
    }

    for idx_path in &indexes {
        let idx_root = match idx_path.parent() {
            Some(p) => p,
            None => continue,
        };

        let display_path = match idx_root.strip_prefix(&path) {
            Ok(rel) if rel.to_string_lossy() == "" => ".".to_string(),
            Ok(rel) => format!("./{}", rel.display()),
            Err(_) => idx_root.display().to_string(),
        };

        match SemanticIndex::new(idx_root, None) {
            Ok(index) => match index.count() {
                Ok(count) => println!("  {display_path}/.og/ ({count} blocks)"),
                Err(_) => println!("  {display_path}/.og/ (needs rebuild)"),
            },
            Err(_) => println!("  {display_path}/.og/ (needs rebuild)"),
        }
    }

    Ok(())
}
