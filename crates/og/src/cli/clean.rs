//! og clean command: remove the .og index directory.

use std::path::Path;

use anyhow::Result;

use og_core::index::{self, INDEX_DIR};

pub fn run(path: &Path) -> Result<()> {
    let start = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (root, found) = index::find_index_root(&start);
    if !found {
        println!("No index found under {}", start.display());
        return Ok(());
    }
    let og_dir = root.join(INDEX_DIR);
    std::fs::remove_dir_all(&og_dir)?;
    println!("Removed {}", og_dir.display());
    Ok(())
}
