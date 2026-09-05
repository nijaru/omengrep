//! og model: status/install for the default embedding model.

use anyhow::Result;

use og_core::model::Embedder as _;
use og_core::model::potion;

pub fn status() -> Result<()> {
    // Status is a read-only probe: it must never download. Presence is a
    // local cache check; the full load (for dims/vocab) only happens when
    // the files are already on disk.
    match potion::default_model_cached() {
        Some(_) => {
            let p = potion::PotionEmbedder::load_default()?;
            println!(
                "default: {} (dims {}, vocab {})",
                p.id(),
                p.dims(),
                p.vocab_size()
            );
            println!("status:  installed");
        }
        None => {
            println!("default: {} (dims 256)", potion::DEFAULT_REPO);
            println!("status:  not installed");
            println!("run 'og model install' to download (~33 MB)");
        }
    }
    Ok(())
}

pub fn install() -> Result<()> {
    let p = potion::PotionEmbedder::load_default()?;
    println!(
        "installed: {} (dims {}, vocab {})",
        p.id(),
        p.dims(),
        p.vocab_size()
    );
    Ok(())
}
