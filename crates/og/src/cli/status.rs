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
    println!("Index root:    {}", root.display());
    println!("Generation:    g-{}", &m.content_hash[..12]);
    println!("Model:         {}", m.model_id);
    println!("Dimensions:    {}", m.dims);
    println!("Blocks:        {}", m.blocks);
    println!("Files:         {}", m.files);
    println!("Schema:        v{}", m.schema_version);
    println!("Vector rows:   {}", idx.vectors.len());
    Ok(())
}
