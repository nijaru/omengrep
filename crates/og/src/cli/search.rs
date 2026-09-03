//! og search command: query or file-ref search with filters + output.

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};

use og_core::search;
use og_core::types::{EXIT_ERROR, EXIT_MATCH, EXIT_NO_MATCH, FileRef, OutputFormat};
use og_core::output;

pub struct SearchParams<'a> {
    pub query: Option<&'a str>,
    pub path: &'a Path,
    pub num_results: usize,
    pub format: OutputFormat,
    pub quiet: bool,
    pub file_types: Option<&'a str>,
    pub exclude: &'a [String],
    pub code_only: bool,
    pub no_semantic: bool,
    pub context_lines: usize,
    pub regex: Option<&'a str>,
    pub highlight: bool,
}

pub fn run(params: &SearchParams<'_>) -> Result<()> {
    let query = match params.query {
        Some(q) => q,
        None => bail!("No query provided. Run 'og --help' for usage."),
    };

    // File reference? -> similar-code search.
    if let Some(file_ref) = search::parse_file_reference(query) {
        return run_similar(file_ref, params);
    }

    let t0 = Instant::now();
    let mut results =
        search::run_search(query, params.path, params.num_results, !params.no_semantic)?;
    let search_time = t0.elapsed();

    if results.is_empty() {
        if !matches!(params.format, OutputFormat::Json) {
            eprintln!("No results found");
        }
        std::process::exit(EXIT_NO_MATCH);
    }

    results = search::filter_results(results, params.file_types, params.exclude, params.code_only);

    if let Some(pattern) = params.regex {
        match regex::Regex::new(pattern) {
            Ok(re) => {
                results.retain(|r| {
                    r.content.as_deref().is_some_and(|c| re.is_match(c)) || re.is_match(&r.name)
                });
            }
            Err(e) => {
                eprintln!("Invalid regex: {e}");
                std::process::exit(EXIT_ERROR);
            }
        }
    }

    let root = params
        .path
        .canonicalize()
        .unwrap_or_else(|_| params.path.to_path_buf());
    output::print_results(
        &results,
        params.format,
        false,
        Some(&root),
        params.context_lines,
        params.highlight.then_some(query),
    );

    if !params.quiet && !matches!(params.format, OutputFormat::Json | OutputFormat::FilesOnly) {
        let result_word = if results.len() == 1 { "result" } else { "results" };
        eprintln!(
            "{} {} ({:.2}s)",
            results.len(),
            result_word,
            search_time.as_secs_f64()
        );
    }

    std::process::exit(if results.is_empty() {
        EXIT_NO_MATCH
    } else {
        EXIT_MATCH
    });
}

fn run_similar(file_ref: FileRef, params: &SearchParams<'_>) -> Result<()> {
    let (path, line, name) = match &file_ref {
        FileRef::ByName { path, name } => (path.as_str(), None, Some(name.as_str())),
        FileRef::ByLine { path, line } => (path.as_str(), Some(*line), None),
        FileRef::ByFile { path } => (path.as_str(), None, None),
    };

    let t0 = Instant::now();
    let results = search::run_similar(path, line, name, params.num_results)?;
    let search_time = t0.elapsed();

    if results.is_empty() {
        if !matches!(params.format, OutputFormat::Json) {
            eprintln!("No similar code found");
        }
        std::process::exit(EXIT_NO_MATCH);
    }

    let root = params
        .path
        .canonicalize()
        .unwrap_or_else(|_| params.path.to_path_buf());
    output::print_results(
        &results,
        params.format,
        true,
        Some(&root),
        params.context_lines,
        params.highlight.then_some(name.unwrap_or("")),
    );

    if !params.quiet && !matches!(params.format, OutputFormat::Json | OutputFormat::FilesOnly) {
        let result_word = if results.len() == 1 { "result" } else { "results" };
        eprintln!(
            "{} similar {} ({:.2}s)",
            results.len(),
            result_word,
            search_time.as_secs_f64()
        );
    }

    Ok(())
}
