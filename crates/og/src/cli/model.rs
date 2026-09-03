//! og model: status/install for the default embedding model.

use anyhow::Result;

use og_core::model::Embedder as _;
use og_core::model::potion;

pub fn status() -> Result<()> {
    match potion::PotionEmbedder::load_default() {
        Ok(p) => {
            println!(
                "default: {} (dims {}, vocab {})",
                p.id(),
                p.dims(),
                p.vocab_size()
            );
            println!("status:  installed");
        }
        Err(e) => {
            println!("default: {} (dims 256)", potion::DEFAULT_REPO);
            println!("status:  not installed ({e:#})");
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
