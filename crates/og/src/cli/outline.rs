//! og outline command: block structure of indexed files.

use std::path::Path;

use anyhow::Result;

use og_core::context;
use og_core::types::{EXIT_ERROR, EXIT_MATCH};

pub struct OutlineParams<'a> {
    pub path: &'a Path,
    pub json: bool,
    pub skeleton: bool,
    pub max_tokens: usize,
    pub quiet: bool,
}

pub fn run(params: &OutlineParams<'_>) -> Result<()> {
    let files = context::run_outline(params.path, params.skeleton, params.max_tokens, params.quiet)?;

    if files.is_empty() {
        eprintln!(
            "No indexed files under {}",
            params.path.display()
        );
        std::process::exit(EXIT_ERROR);
    }

    if params.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&context::outline_json(&files))?
        );
    } else {
        context::print_outline(&files);
    }

    std::process::exit(EXIT_MATCH);
}
