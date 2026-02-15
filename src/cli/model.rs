use anyhow::Result;
use hf_hub::api::sync::Api;

use crate::embedder::{MODEL_FILE, MODEL_REPO, TOKENIZER_FILE};

pub fn status() -> Result<()> {
    let api = Api::new()?;
    let repo = api.model(MODEL_REPO.to_string());

    let model_cached = repo.get(MODEL_FILE).is_ok();
    let tokenizer_cached = repo.get(TOKENIZER_FILE).is_ok();

    if model_cached && tokenizer_cached {
        println!("{MODEL_REPO} (installed)");
    } else {
        eprintln!("Model not installed -- run 'og model install'");
    }

    Ok(())
}

pub fn install() -> Result<()> {
    let api = Api::new()?;
    let repo = api.model(MODEL_REPO.to_string());

    println!("Downloading {MODEL_REPO}...");

    for filename in [MODEL_FILE, TOKENIZER_FILE] {
        match repo.get(filename) {
            Ok(path) => {
                println!("  {filename} -> {}", path.display());
            }
            Err(e) => {
                eprintln!("Failed to download {filename}: {e}");
                eprintln!("Check network connection and try again");
                std::process::exit(crate::types::EXIT_ERROR);
            }
        }
    }

    println!("Model installed: {MODEL_REPO}");
    Ok(())
}
