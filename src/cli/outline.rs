use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::index::{find_index_root, manifest::Manifest, VECTORS_DIR};
use crate::types::EXIT_ERROR;

/// A block entry for outline display.
struct OutlineEntry {
    name: String,
    block_type: String,
    start_line: usize,
    end_line: usize,
}

pub fn run(path: &Path, json: bool) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (index_root, index_dir) = find_index_root(&path);

    let Some(index_dir) = index_dir else {
        eprintln!("No index found. Run 'og build' to create.");
        std::process::exit(EXIT_ERROR);
    };

    let manifest = match Manifest::load(&index_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    let vectors_path = index_dir.join(VECTORS_DIR).to_string_lossy().into_owned();
    let store = match omendb::VectorStore::open(&vectors_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open index: {e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    // Compute scope prefix for filtering (relative to index root)
    let scope_prefix = path
        .strip_prefix(&index_root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty());

    // Collect matching files sorted by path
    let mut file_entries: Vec<(&str, &[String])> = manifest
        .files
        .iter()
        .filter(|(rel_path, _)| match &scope_prefix {
            Some(prefix) => {
                rel_path.as_str() == prefix.as_str() || rel_path.starts_with(&format!("{prefix}/"))
            }
            None => true,
        })
        .map(|(rel_path, entry)| (rel_path.as_str(), entry.blocks.as_slice()))
        .collect();

    file_entries.sort_by_key(|(path, _)| *path);

    if file_entries.is_empty() {
        eprintln!("No indexed files under {}", path.display());
        std::process::exit(EXIT_ERROR);
    }

    if json {
        print_json(&file_entries, &store)?;
    } else {
        print_default(&file_entries, &store);
    }

    Ok(())
}

fn get_blocks(block_ids: &[String], store: &omendb::VectorStore) -> Vec<OutlineEntry> {
    let mut entries: Vec<OutlineEntry> = block_ids
        .iter()
        .filter_map(|id| {
            let meta = store.get_metadata_by_id(id)?;
            Some(OutlineEntry {
                name: meta
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_type: meta
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                start_line: meta.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                end_line: meta.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            })
        })
        .collect();
    entries.sort_by_key(|e| e.start_line);
    entries
}

fn print_default(file_entries: &[(&str, &[String])], store: &omendb::VectorStore) {
    for (rel_path, block_ids) in file_entries {
        println!("{}", rel_path.bold());
        let blocks = get_blocks(block_ids, store);
        for entry in &blocks {
            println!(
                "  {:>5}  {:<12}  {}",
                entry.start_line + 1,
                entry.block_type.dimmed(),
                entry.name
            );
        }
        println!();
    }
}

fn print_json(file_entries: &[(&str, &[String])], store: &omendb::VectorStore) -> Result<()> {
    let output: Vec<serde_json::Value> = file_entries
        .iter()
        .map(|(rel_path, block_ids)| {
            let blocks: Vec<serde_json::Value> = get_blocks(block_ids, store)
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "type": e.block_type,
                        "line": e.start_line + 1,
                        "end_line": e.end_line + 1,
                    })
                })
                .collect();
            serde_json::json!({
                "file": rel_path,
                "blocks": blocks,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
