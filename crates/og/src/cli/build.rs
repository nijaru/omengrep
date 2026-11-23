//! og build command: full-rebuild generation build + atomic publish.

use std::path::Path;

use anyhow::Result;

use og_core::index;
use og_core::model::Embedder;

pub fn run(path: &Path, deterministic: bool, force: bool, quiet: bool) -> Result<()> {
    let embedder: Box<dyn Embedder> = if deterministic {
        Box::new(og_core::model::DeterministicEmbedder::default())
    } else {
        og_core::model::default_embedder()?
    };
    let mode = if force {
        index::Incremental::Force
    } else {
        index::Incremental::Auto
    };
    index::build_with(path, embedder.as_ref(), quiet, mode)?;
    Ok(())
}
