//! og build command: full-rebuild generation build + atomic publish.

use std::path::Path;

use anyhow::Result;

use og_core::index;
use og_core::model::DeterministicEmbedder;

pub fn run(path: &Path, quiet: bool) -> Result<()> {
    let embedder = DeterministicEmbedder::default();
    let stats = index::build(path, &embedder, quiet)?;
    if !quiet {
        eprintln!(
            "Indexed {} blocks from {} files",
            stats.blocks, stats.files
        );
        if stats.errors > 0 {
            eprintln!("  {} files failed", stats.errors);
        }
    }
    Ok(())
}
