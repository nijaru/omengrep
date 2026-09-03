//! og build command: full-rebuild generation build + atomic publish.

use std::path::Path;

use anyhow::Result;

use og_core::index;
use og_core::model::Embedder;

pub fn run(path: &Path, deterministic: bool, quiet: bool) -> Result<()> {
    let embedder: Box<dyn Embedder> = if deterministic {
        Box::new(og_core::model::DeterministicEmbedder::default())
    } else {
        // Default: potion. A model download failure is a hard error —
        // silent fallback would rebuild the whole index in the wrong space.
        // (The deterministic embedder remains a --deterministic escape hatch.)
        Box::new(og_core::model::potion::PotionEmbedder::load_default()?)
    };
    let stats = index::build(path, embedder.as_ref(), quiet)?;
    if !quiet {
        eprintln!("Indexed {} blocks from {} files", stats.blocks, stats.files);
        if stats.errors > 0 {
            eprintln!("  {} files failed", stats.errors);
        }
    }
    Ok(())
}
