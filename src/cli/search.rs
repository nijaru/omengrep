use std::path::Path;
use std::time::Instant;

use anyhow::{bail, Result};

use crate::boost::boost_results;
use crate::cli::output::print_results;
use crate::index::{self, walker, SemanticIndex};
use crate::types::{FileRef, OutputFormat, EXIT_ERROR, EXIT_MATCH, EXIT_NO_MATCH};

pub fn run(
    query: Option<&str>,
    path: &Path,
    num_results: usize,
    threshold: f32,
    json: bool,
    files_only: bool,
    compact: bool,
    quiet: bool,
    file_types: Option<&str>,
    exclude: &[String],
    code_only: bool,
    no_index: bool,
) -> Result<()> {
    let query = match query {
        Some(q) => q,
        None => {
            bail!("No query provided. Run 'og --help' for usage.");
        }
    };

    // Check if query is a file reference
    if let Some(file_ref) = parse_file_reference(query) {
        return run_similar_search(file_ref, num_results, json, files_only, compact, quiet);
    }

    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !path.exists() {
        eprintln!("Path does not exist: {}", path.display());
        std::process::exit(EXIT_ERROR);
    }

    // Walk up to find existing index
    let (index_root, existing_index) = index::find_index_root(&path);
    let search_path = path.clone();

    if existing_index.is_none() {
        // Check for auto-build
        if std::env::var("OG_AUTO_BUILD")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
        {
            if !quiet {
                eprintln!("Building index (OG_AUTO_BUILD=1)...");
            }
            build_index(&path, quiet)?;
        } else {
            eprintln!("No index found. Run 'og build' first.");
            eprintln!("Tip: Set OG_AUTO_BUILD=1 for auto-indexing");
            std::process::exit(EXIT_ERROR);
        }
    }

    let index_root = if existing_index.is_some() {
        index_root
    } else {
        path.clone()
    };

    if !no_index {
        // Auto-update stale files
        if !quiet && index_root != search_path {
            eprintln!("Using index at {}", index_root.display());
        }

        let files = walker::scan(&index_root)?;
        let index = SemanticIndex::new(&index_root, None)?;
        let stale_count = index.needs_update(&files)?;

        if stale_count > 0 {
            if !quiet {
                eprint!("Updating {stale_count} changed files...");
            }
            let stats = index.update(&files, crate::embedder::BATCH_SIZE)?;
            if !quiet && stats.blocks > 0 {
                eprintln!(" updated {} blocks", stats.blocks);
            } else if !quiet {
                eprintln!(" done");
            }
        }
    }

    // Run search
    if !quiet {
        eprint!("Searching...");
    }
    let t0 = Instant::now();
    let index = SemanticIndex::new(&index_root, Some(&search_path))?;
    let mut results = index.search(query, num_results)?;
    let search_time = t0.elapsed();
    if !quiet {
        eprintln!("\r              \r");
    }

    if results.is_empty() {
        if !json {
            eprintln!("No results found");
        }
        std::process::exit(EXIT_NO_MATCH);
    }

    // Filter results
    results = filter_results(results, file_types, exclude, code_only);
    boost_results(&mut results, query);

    let format = if files_only {
        OutputFormat::FilesOnly
    } else if json {
        OutputFormat::Json
    } else if compact {
        OutputFormat::Compact
    } else {
        OutputFormat::Default
    };

    // Filter by threshold
    if threshold != 0.0 {
        results.retain(|r| r.score >= threshold);
    }

    print_results(&results, format, false, Some(&path));

    if !quiet && !json && !files_only {
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

    std::process::exit(if results.is_empty() {
        EXIT_NO_MATCH
    } else {
        EXIT_MATCH
    });
}

fn run_similar_search(
    file_ref: FileRef,
    num_results: usize,
    json: bool,
    files_only: bool,
    compact: bool,
    quiet: bool,
) -> Result<()> {
    let (file_path, line, name) = match &file_ref {
        FileRef::ByName { path, name } => (path.as_str(), None, Some(name.as_str())),
        FileRef::ByLine { path, line } => (path.as_str(), Some(*line), None),
        FileRef::ByFile { path } => (path.as_str(), None, None),
    };

    let file_dir = Path::new(file_path).parent().unwrap_or(Path::new("."));
    let (index_root, existing_index) = index::find_index_root(file_dir);

    if existing_index.is_none() {
        eprintln!("No index found. Run 'og build' first.");
        std::process::exit(EXIT_ERROR);
    }

    if !quiet {
        let ref_desc = match &file_ref {
            FileRef::ByName { path, name } => {
                format!(
                    "{}#{}",
                    Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    name
                )
            }
            FileRef::ByLine { path, line } => {
                format!(
                    "{}:{}",
                    Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    line
                )
            }
            FileRef::ByFile { path } => Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        };
        eprint!("Finding similar to {ref_desc}...");
    }

    let abs_path = Path::new(file_path)
        .canonicalize()
        .unwrap_or_else(|_| file_path.into());
    let abs_str = abs_path.to_string_lossy();

    let index = SemanticIndex::new(&index_root, None)?;
    let results = index.find_similar(&abs_str, line, name, num_results)?;

    if !quiet {
        eprintln!("\r                                \r");
    }

    if results.is_empty() {
        if !json {
            eprintln!("No similar code found");
        }
        std::process::exit(EXIT_NO_MATCH);
    }

    let format = if files_only {
        OutputFormat::FilesOnly
    } else if json {
        OutputFormat::Json
    } else if compact {
        OutputFormat::Compact
    } else {
        OutputFormat::Default
    };

    print_results(&results, format, true, Some(&index_root));

    if !quiet && !json {
        let result_word = if results.len() == 1 {
            "result"
        } else {
            "results"
        };
        eprintln!("{} similar {}", results.len(), result_word);
    }

    Ok(())
}

/// Build index for a path.
fn build_index(path: &Path, quiet: bool) -> Result<()> {
    let files = walker::scan(path)?;
    if files.is_empty() {
        if !quiet {
            eprintln!("No files found to index");
        }
        return Ok(());
    }

    let index = SemanticIndex::new(path, None)?;
    let t0 = Instant::now();

    let progress_fn = if quiet {
        None
    } else {
        Some(
            (|current: usize, total: usize, _msg: &str| {
                eprint!("\rIndexing {current}/{total}...");
            }) as fn(usize, usize, &str),
        )
    };

    let stats = index.index(
        &files,
        crate::embedder::BATCH_SIZE,
        progress_fn
            .as_ref()
            .map(|f| f as &dyn Fn(usize, usize, &str)),
    )?;
    let elapsed = t0.elapsed();

    if !quiet {
        eprintln!(
            "\rIndexed {} blocks from {} files ({:.1}s)",
            stats.blocks,
            stats.files,
            elapsed.as_secs_f64()
        );
    }

    Ok(())
}

/// Parse query as file reference: file#name, file:line, or existing file.
fn parse_file_reference(query: &str) -> Option<FileRef> {
    if query.is_empty() {
        return None;
    }

    // Check for #name syntax
    if let Some(hash_pos) = query.rfind('#') {
        let file_part = &query[..hash_pos];
        let name = &query[hash_pos + 1..];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && Path::new(file_part).exists()
        {
            return Some(FileRef::ByName {
                path: file_part.to_string(),
                name: name.to_string(),
            });
        }
    }

    // Check for :line syntax
    if let Some(colon_pos) = query.rfind(':') {
        let file_part = &query[..colon_pos];
        let line_part = &query[colon_pos + 1..];
        if let Ok(line) = line_part.parse::<usize>() {
            if Path::new(file_part).exists() {
                return Some(FileRef::ByLine {
                    path: file_part.to_string(),
                    line,
                });
            }
        }
    }

    // Check for plain file path
    let path = Path::new(query);
    if path.exists() && path.is_file() {
        return Some(FileRef::ByFile {
            path: query.to_string(),
        });
    }

    None
}

/// Filter results by file type and exclude patterns.
fn filter_results(
    mut results: Vec<crate::types::SearchResult>,
    file_types: Option<&str>,
    exclude: &[String],
    code_only: bool,
) -> Vec<crate::types::SearchResult> {
    // Build exclude list
    let mut exclude_patterns: Vec<String> = exclude.to_vec();
    if code_only {
        exclude_patterns.extend(
            ["*.md", "*.markdown", "*.txt", "*.rst", "*.adoc"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    if file_types.is_none() && exclude_patterns.is_empty() {
        return results;
    }

    // File type filtering
    if let Some(types) = file_types {
        let type_map: &[(&str, &[&str])] = &[
            ("py", &[".py", ".pyi"]),
            ("js", &[".js", ".jsx", ".mjs"]),
            ("ts", &[".ts", ".tsx"]),
            ("rust", &[".rs"]),
            ("rs", &[".rs"]),
            ("go", &[".go"]),
            ("java", &[".java"]),
            ("c", &[".c", ".h"]),
            ("cpp", &[".cpp", ".cc", ".cxx", ".hpp", ".hh"]),
            ("cs", &[".cs"]),
            ("rb", &[".rb"]),
            ("php", &[".php"]),
            ("sh", &[".sh", ".bash", ".zsh"]),
            ("md", &[".md", ".markdown"]),
            ("json", &[".json"]),
            ("yaml", &[".yaml", ".yml"]),
            ("toml", &[".toml"]),
        ];

        let mut allowed_exts: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ft in types.split(',') {
            let ft = ft.trim().to_lowercase();
            let found = type_map.iter().find(|(name, _)| *name == ft);
            if let Some((_, exts)) = found {
                for ext in *exts {
                    allowed_exts.insert(ext.to_string());
                }
            } else {
                allowed_exts.insert(format!(".{ft}"));
            }
        }

        results.retain(|r| allowed_exts.iter().any(|ext| r.file.ends_with(ext)));
    }

    // Exclude pattern filtering (simple glob matching)
    if !exclude_patterns.is_empty() {
        results.retain(|r| {
            !exclude_patterns.iter().any(|pattern| {
                // Simple glob: *.ext matching
                if let Some(ext) = pattern.strip_prefix('*') {
                    r.file.ends_with(ext)
                } else {
                    r.file.contains(pattern)
                }
            })
        });
    }

    results
}
