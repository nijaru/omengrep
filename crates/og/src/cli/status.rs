//! og status command: report the published generation.

use std::path::Path;

use anyhow::Result;

use og_core::index::{self, Index};

pub fn run(path: &Path) -> Result<()> {
    let start = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (root, found) = index::find_index_root(&start);
    if !found {
        println!("No index found (looked up from {})", start.display());
        return Ok(());
    }

    let idx = Index::open(&root)?;
    let m = &idx.manifest;

    // Staleness: the same check searches auto-applies. Status is where a
    // human inspects, so say whether the index is fresh.
    let freshness = if index::needs_update(&root).unwrap_or(false) {
        "stale (any search refreshes it, or run: og build)"
    } else {
        "up to date"
    };

    // Generation name is hash(content+model+schema); read CURRENT for
    // the actual dir, then measure it. (Reconstructing from the manifest
    // content_hash alone is wrong — status printed "?" when I tried.)
    let og_dir = root.join(index::INDEX_DIR);
    let gen_name = std::fs::read_to_string(og_dir.join("CURRENT"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let gen_dir = og_dir.join("generations").join(&gen_name);
    let size = dir_size(&gen_dir)
        .map(human_size)
        .unwrap_or_else(|| "?".into());

    println!("Index root:    {}", root.display());
    println!("Generation:   {} ({})", gen_name, size);
    println!("Status:       {}", freshness);
    println!("Files:         {}", m.files);
    println!("Blocks:        {}", m.blocks);
    println!("Model:         {}", m.model_id);
    println!("Dimensions:    {}", m.dims);
    Ok(())
}

fn dir_size(path: &Path) -> Option<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        total += entry.metadata().ok()?.len();
    }
    Some(total)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
