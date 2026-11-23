//! og search command: query or file-ref search with filters + output.

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};

use og_core::output;
use og_core::search;
use og_core::types::{EXIT_MATCH, EXIT_NO_MATCH, FileRef, OutputFormat};

pub struct SearchParams<'a> {
    pub query: Option<&'a str>,
    pub path: Option<&'a Path>,
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

    let filters = search::Filters::new(
        params.file_types,
        params.exclude,
        params.code_only,
        params.regex,
    )?;

    // File reference? -> similar-code search. Warn when the input is
    // ambiguous: a plausible search query that also names a real file
    // (e.g. `og auth.py` intending search) silently switches modes here.
    // Path-with-marker refs (file#name, file:line) are unambiguous.
    if let Some(file_ref) = search::parse_file_reference(query) {
        let ambiguous = matches!(&file_ref, FileRef::ByFile { .. })
            && !query.contains('#')
            && !query.contains(':')
            && !params.quiet;
        if ambiguous {
            eprintln!(
                "Note: '{}' is an existing file — running similar-code search. \
                 For text search, quote it differently or pass a subdirectory path.",
                query
            );
        }
        return run_similar(file_ref, params, &filters);
    }

    let path = params.path.unwrap_or(Path::new("."));
    let t0 = Instant::now();
    let (results, lexical_matched) = search::run_search(
        query,
        path,
        params.num_results,
        !params.no_semantic,
        params.quiet,
        &filters,
    )?;
    let search_time = t0.elapsed();

    if results.is_empty() {
        no_match(params.format, params.quiet, "No results found");
    }

    // Distinguish 'ranked matches' from 'semantic-only candidates': when
    // the lexical channels found nothing, these hits are vector-noise-range
    // guesses — say so (stderr; JSON stays machine-clean).
    if !lexical_matched && !params.quiet {
        eprintln!("Note: no keyword matches; showing nearest semantic candidates only");
    }

    let root = path.canonicalize()?;
    output::print_results(
        &results,
        params.format,
        false,
        Some(&root),
        params.context_lines,
        params.highlight.then_some(query),
    );

    if !params.quiet && !matches!(params.format, OutputFormat::Json | OutputFormat::FilesOnly) {
        let result_word = if results.len() == 1 {
            "result"
        } else {
            "results"
        };
        eprintln!(
            "{} {} ({:.2}s)",
            results.len(),
            result_word,
            search_time.as_secs_f64()
        );
    }

    std::process::exit(EXIT_MATCH);
}

fn no_match(format: OutputFormat, quiet: bool, message: &str) -> ! {
    if matches!(format, OutputFormat::Json | OutputFormat::NoContent) {
        println!("[]");
    } else if !quiet {
        eprintln!("{message}");
    }
    std::process::exit(EXIT_NO_MATCH);
}

fn run_similar(
    file_ref: FileRef,
    params: &SearchParams<'_>,
    filters: &search::Filters,
) -> Result<()> {
    let t0 = Instant::now();
    let results = search::run_similar(
        &file_ref,
        params.path,
        params.num_results,
        !params.no_semantic,
        params.quiet,
        filters,
    )?;
    let search_time = t0.elapsed();

    if results.is_empty() {
        no_match(params.format, params.quiet, "No similar code found");
    }

    let root = params.path.map(Path::canonicalize).transpose()?;
    let (_, _, name) = file_ref.parts();
    output::print_results(
        &results,
        params.format,
        true,
        root.as_deref(),
        params.context_lines,
        params.highlight.then_some(name.unwrap_or("")),
    );

    if !params.quiet && !matches!(params.format, OutputFormat::Json | OutputFormat::FilesOnly) {
        let result_word = if results.len() == 1 {
            "result"
        } else {
            "results"
        };
        eprintln!(
            "{} similar {} ({:.2}s)",
            results.len(),
            result_word,
            search_time.as_secs_f64()
        );
    }

    Ok(())
}
