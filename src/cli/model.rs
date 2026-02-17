use anyhow::Result;
use hf_hub::api::sync::Api;

use crate::embedder;

pub fn status() -> Result<()> {
    let config = embedder::MODEL;
    let api = Api::new()?;
    let repo = api.model(config.repo.to_string());
    let installed = repo.get(config.model_file).is_ok() && repo.get(config.tokenizer_file).is_ok();
    let marker = if installed {
        "installed"
    } else {
        "not installed"
    };
    println!("  {} ({}d/token, {marker})", config.repo, config.token_dim);

    Ok(())
}

pub fn install() -> Result<()> {
    let config = embedder::MODEL;
    let api = Api::new()?;
    let repo = api.model(config.repo.to_string());

    println!("Downloading {}...", config.repo);

    for filename in [config.model_file, config.tokenizer_file] {
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

    println!("Model installed: {}", config.repo);
    Ok(())
}
