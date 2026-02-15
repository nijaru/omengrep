use std::path::Path;

use anyhow::Result;

use crate::index::{self, SemanticIndex, INDEX_DIR};
use crate::types::EXIT_ERROR;

pub fn run(path: &Path, recursive: bool) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut deleted_count = 0;

    // Delete root index if exists
    if path.join(INDEX_DIR).join("manifest.json").exists() {
        let index = SemanticIndex::new(&path, None)?;
        index.clear()?;
        println!("Deleted ./.og/");
        deleted_count += 1;
    } else {
        // Check if this path is part of a parent index
        if let Some(parent) = index::find_parent_index(&path) {
            if let Ok(rel_prefix) = path.strip_prefix(&parent) {
                let rel_str = rel_prefix.to_string_lossy();
                if !rel_str.is_empty() && rel_str != "." {
                    let index = SemanticIndex::new(&parent, None)?;
                    match index.remove_prefix(&rel_str) {
                        Ok(stats) => {
                            if stats.blocks > 0 {
                                println!(
                                    "Removed {} blocks ({} files) from parent index",
                                    stats.blocks, stats.files
                                );
                                deleted_count += 1;
                            } else {
                                eprintln!("No blocks found for {} in parent index", rel_str);
                            }
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.contains("older version") || msg.contains("different model") {
                                eprintln!(
                                    "Parent index needs rebuild. Run: og build --force {}",
                                    parent.display()
                                );
                                std::process::exit(EXIT_ERROR);
                            }
                            return Err(e);
                        }
                    }
                } else {
                    eprintln!(
                        "Hint: Use 'og clean {}' to delete the parent index",
                        parent.display()
                    );
                }
            }
        }
    }

    // Delete subdir indexes if recursive
    if recursive {
        let subdir_indexes = index::find_subdir_indexes(&path, false);
        for idx_path in &subdir_indexes {
            if let Ok(rel_path) = idx_path.parent().unwrap_or(&path).strip_prefix(&path) {
                match std::fs::remove_dir_all(idx_path) {
                    Ok(()) => {
                        println!("Deleted ./{}/{}/ ", rel_path.display(), INDEX_DIR);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        eprintln!("Failed to delete {}: {e}", idx_path.display());
                    }
                }
            }
        }
    }

    if deleted_count == 0 {
        eprintln!("No indexes to delete");
    } else if deleted_count > 1 {
        println!("Deleted {deleted_count} indexes");
    }

    Ok(())
}
