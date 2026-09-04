//! og context command: ranked files and symbols for compact code context.

use std::path::Path;

use anyhow::Result;

use og_core::context;
use og_core::types::{EXIT_ERROR, EXIT_MATCH};

pub struct ContextParams<'a> {
    pub path: &'a Path,
    pub num_files: usize,
    pub symbols_per_file: usize,
    pub json: bool,
    pub skeleton: bool,
    pub max_tokens: usize,
    pub quiet: bool,
}

pub fn run(params: &ContextParams<'_>) -> Result<()> {
    let ranked = context::run_context(
        params.path,
        params.num_files,
        params.symbols_per_file,
        params.skeleton,
        params.max_tokens,
        params.quiet,
    )?;

    if ranked.is_empty() {
        if params.num_files == 0 || params.symbols_per_file == 0 {
            eprintln!("Nothing to show: -n/--symbols is 0 (no files or symbols requested)");
        } else {
            eprintln!("No indexed files under {}", params.path.display());
        }
        std::process::exit(EXIT_ERROR);
    }

    if params.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&context::context_json(&ranked))?
        );
    } else {
        context::print_context(&ranked);
    }

    std::process::exit(EXIT_MATCH);
}
