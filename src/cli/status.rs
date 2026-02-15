use std::path::Path;

use anyhow::Result;

use crate::index::{walker, SemanticIndex, INDEX_DIR};
use crate::types::EXIT_ERROR;

pub fn run(path: &Path) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    if !path.join(INDEX_DIR).join("manifest.json").exists() {
        eprintln!("No index. Run 'og build' to create.");
        return Ok(());
    }

    let index = match SemanticIndex::new(&path, None) {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("older version") || msg.contains("different model") {
                eprintln!("Index needs rebuild. Run: og build --force");
                return Ok(());
            }
            eprintln!("{e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    let block_count = index.count()?;
    let files = walker::scan(&path)?;
    let file_count = files.len();

    let stale_result = index.get_stale_files(&files);
    match stale_result {
        Ok((changed, deleted)) => {
            let stale_count = changed.len() + deleted.len();
            if stale_count == 0 {
                println!("{file_count} files, {block_count} blocks (up to date)");
            } else {
                let mut parts = Vec::new();
                if !changed.is_empty() {
                    parts.push(format!("{} changed", changed.len()));
                }
                if !deleted.is_empty() {
                    parts.push(format!("{} deleted", deleted.len()));
                }
                let stale_str = parts.join(", ");
                println!(
                    "{file_count} files, {block_count} blocks ({stale_str}) -- run 'og build'"
                );
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("older version") || msg.contains("different model") {
                eprintln!("Index needs rebuild. Run: og build --force");
            } else {
                eprintln!("{e}");
                std::process::exit(EXIT_ERROR);
            }
        }
    }

    Ok(())
}
