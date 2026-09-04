//! og build command: full-rebuild generation build + atomic publish.

use std::path::Path;

use anyhow::Result;

use og_core::index;
use og_core::model::Embedder;

pub fn run(path: &Path, deterministic: bool, force: bool, quiet: bool) -> Result<()> {
    let embedder: Box<dyn Embedder> = if deterministic {
        Box::new(og_core::model::DeterministicEmbedder::default())
    } else {
        // Default: potion. A model download failure is a hard error —
        // silent fallback would rebuild the whole index in the wrong space.
        // (The deterministic embedder remains a --deterministic escape hatch.)
        Box::new(og_core::model::potion::PotionEmbedder::load_default()?)
    };
    let mode = if force {
        index::Incremental::Force
    } else {
        index::Incremental::Auto
    };
    let stats = index::build_with(path, embedder.as_ref(), quiet, mode)?;
    // index::build_with already printed the "Indexed N blocks ... (Xs)"
    // summary; only surface per-run extras here (never duplicate the line).
    if !quiet && stats.errors > 0 {
        eprintln!("  {} files failed", stats.errors);
    }
    Ok(())
}
