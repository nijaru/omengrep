//! Query surface: hybrid search + file-reference similar-code search.
//! Ported from og v0.0.3 cli/search.rs policy: file ref parsing, file-type
//! and exclude filtering, exit codes.

use std::path::Path;

use anyhow::Result;

use crate::index::{self, Index};
use crate::retrieve;
use crate::types::{FileRef, SearchResult};

/// Search a query against the nearest index. Errors exit via ExitError.
pub fn run_search(
    query: &str,
    path: &Path,
    num_results: usize,
    with_semantic: bool,
    quiet: bool,
) -> Result<Vec<SearchResult>> {
    let start = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (index_root, found) = index::find_index_root(&start);
    if !found {
        // Auto-build (v0.0.3 OG_AUTO_BUILD behavior).
        if auto_build_enabled() {
            let embedder = crate::model::default_embedder()?;
            index::build_with(&start, embedder.as_ref(), false, index::Incremental::Auto)?;
        } else {
            anyhow::bail!("No index found. Run 'og build' first.\nTip: OG_AUTO_BUILD=1 enables auto-indexing.");
        }
    }

    // Auto-update stale files before searching (v0.0.3 behavior). Errors
    // are non-fatal: a stale index still answers via BM25.
    if found
        && let Ok(true) = index::update_if_stale(&index_root, quiet)
    {
        eprintln!("Index updated");
    }

    let idx = Index::open(&index_root)?;
    let mut results = retrieve::search(&idx, query, num_results, with_semantic)?;

    // Scope results to the search path (index may cover an ancestor).
    let prefix = start
        .strip_prefix(&index_root)
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .into_owned();
    if !prefix.is_empty() {
        let scope = format!("{}/", prefix.trim_end_matches('/'));
        results.retain(|r| r.file.starts_with(&scope));
    }

    Ok(results)
}

pub(crate) fn auto_build_enabled() -> bool {
    std::env::var("OG_AUTO_BUILD")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Similar-code search from a file reference (file#name, file:line, file).
pub fn run_similar(ref_path: &str, line: Option<usize>, name: Option<&str>, k: usize) -> Result<Vec<SearchResult>> {
    let file_dir = Path::new(ref_path)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let (index_root, found) = index::find_index_root(&file_dir);
    if !found {
        anyhow::bail!("No index found. Run 'og build' first.");
    }

    let idx = Index::open(&index_root)?;
    // Resolve the reference path relative to the index root.
    let abs = Path::new(ref_path).canonicalize().unwrap_or_else(|_| ref_path.into());
    let rel = abs
        .strip_prefix(&index_root)
        .unwrap_or(&abs)
        .to_string_lossy()
        .into_owned();

    retrieve::similar(&idx, &rel, line, name, k)
}

/// Parse query as file reference: file#name, file:line, or existing file.
/// Ported verbatim from v0.0.3 policy.
pub fn parse_file_reference(query: &str) -> Option<FileRef> {
    if query.is_empty() {
        return None;
    }

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

    if let Some(colon_pos) = query.rfind(':') {
        let file_part = &query[..colon_pos];
        let line_part = &query[colon_pos + 1..];
        if let Ok(line) = line_part.parse::<usize>()
            && Path::new(file_part).exists()
        {
            return Some(FileRef::ByLine {
                path: file_part.to_string(),
                line,
            });
        }
    }

    let path = Path::new(query);
    if path.exists() && path.is_file() {
        return Some(FileRef::ByFile {
            path: query.to_string(),
        });
    }

    None
}

/// Filter results by file type and exclude patterns. Ported verbatim.
pub fn filter_results(
    mut results: Vec<SearchResult>,
    file_types: Option<&str>,
    exclude: &[String],
    code_only: bool,
) -> Vec<SearchResult> {
    let mut exclude_patterns: Vec<String> = exclude.to_vec();
    if code_only {
        exclude_patterns.extend(
            ["*.md", "*.markdown", "*.txt", "*.rst", "*.adoc"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

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
            if let Some((_, exts)) = type_map.iter().find(|(name, _)| *name == ft) {
                for ext in *exts {
                    allowed_exts.insert(ext.to_string());
                }
            } else {
                allowed_exts.insert(format!(".{ft}"));
            }
        }

        results.retain(|r| allowed_exts.iter().any(|ext| r.file.ends_with(ext)));
    }

    if !exclude_patterns.is_empty() {
        results.retain(|r| {
            !exclude_patterns.iter().any(|pattern| {
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
