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
    // Legacy (pre-0.1.0 omendb) .og dirs carry no CURRENT pointer: the new
    // core builds a fresh generation alongside them without touching their
    // files. Say so once, so the orphaned disk use is not a mystery.
    let root = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !quiet && root.join(".og").exists() && !root.join(".og").join("CURRENT").exists() {
        eprintln!("No current-format index found; building fresh (legacy .og files are left untouched — delete .og to reclaim space).");
    }
    let stats = index::build_with(path, embedder.as_ref(), quiet, mode)?;
    if !quiet {
        eprintln!("Indexed {} blocks from {} files", stats.blocks, stats.files);
        if stats.errors > 0 {
            eprintln!("  {} files failed", stats.errors);
        }
    }
    Ok(())
}
