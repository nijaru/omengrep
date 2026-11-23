//! Query surface: hybrid search + file-reference similar-code search.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;

use crate::index::{self, Index};
use crate::retrieve;
use crate::types::{FileRef, SearchResult};

/// Search a query against the nearest index, applying scope and filters
/// before ranking and limiting results.
pub fn run_search(
    query: &str,
    path: &Path,
    num_results: usize,
    with_semantic: bool,
    quiet: bool,
    filters: &Filters,
) -> Result<(Vec<SearchResult>, bool)> {
    let start = path
        .canonicalize()
        .with_context(|| format!("opening search path {}", path.display()))?;
    let (mut index_root, found) = index::find_index_root(&start);
    if found {
        if index::update_if_stale(&index_root, quiet)? {
            eprintln!("Index updated");
        }
    } else if auto_build_enabled() {
        if start.is_file() {
            index_root = start
                .parent()
                .context("missing parent directory")?
                .to_path_buf();
        }
        let embedder = crate::model::default_embedder()?;
        index::build_with(
            &index_root,
            embedder.as_ref(),
            quiet,
            index::Incremental::Auto,
        )?;
    } else {
        anyhow::bail!(
            "No index found. Run 'og build' first.\nTip: OG_AUTO_BUILD=1 enables auto-indexing."
        );
    }

    let idx = Index::open(&index_root)?;
    let scope = Scope::within(&index_root, &start)?;
    retrieve::search_with_signal(&idx, query, num_results, with_semantic, filters, &scope)
}

pub(crate) fn auto_build_enabled() -> bool {
    std::env::var("OG_AUTO_BUILD")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Similar-code search from a file reference (file#name, file:line, file).
pub fn run_similar(
    reference: &FileRef,
    path: Option<&Path>,
    k: usize,
    with_semantic: bool,
    quiet: bool,
    filters: &Filters,
) -> Result<Vec<SearchResult>> {
    let (ref_path, line, name) = reference.parts();
    let abs = Path::new(ref_path)
        .canonicalize()
        .with_context(|| format!("opening reference file {ref_path}"))?;
    let (index_root, found) = index::find_index_root(&abs);
    if !found {
        anyhow::bail!("No index found. Run 'og build' first.");
    }

    if index::update_if_stale(&index_root, quiet)? {
        eprintln!("Index updated");
    }

    let idx = Index::open(&index_root)?;
    let scope = match path {
        Some(path) => Scope::within(&index_root, &path.canonicalize()?)?,
        None => Scope::All,
    };
    let rel = abs.strip_prefix(&index_root)?.to_string_lossy();
    retrieve::similar_filtered(&idx, (&rel, line, name), k, with_semantic, filters, &scope)
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
            && Path::new(file_part).is_file()
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
            && Path::new(file_part).is_file()
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

/// Query restrictions shared by text and similar-code search.
#[derive(Default)]
pub struct Filters {
    exts: HashSet<String>,
    excludes: GlobSet,
    code_only: bool,
    pub(crate) regex: Option<Regex>,
}

impl Filters {
    pub fn new(
        types: Option<&str>,
        exclude: &[String],
        code_only: bool,
        regex: Option<&str>,
    ) -> Result<Self> {
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

        let mut exts = HashSet::new();
        for ft in types.into_iter().flat_map(|types| types.split(',')) {
            let ft = ft.trim().to_lowercase();
            if let Some((_, aliases)) = type_map.iter().find(|(name, _)| *name == ft) {
                exts.extend(aliases.iter().map(|ext| ext[1..].to_string()));
            } else {
                exts.insert(ft);
            }
        }

        let mut excludes = GlobSetBuilder::new();
        for pattern in exclude {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .with_context(|| format!("invalid exclude glob {pattern:?}"))?;
            excludes.add(glob);
        }
        Ok(Self {
            exts,
            excludes: excludes.build()?,
            code_only,
            regex: regex.map(Regex::new).transpose().context("invalid regex")?,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.exts.is_empty() && self.excludes.is_empty() && !self.code_only && self.regex.is_none()
    }

    pub(crate) fn matches_file(&self, file: &Path) -> bool {
        let ext = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        !(self.code_only
            && matches!(
                ext.as_str(),
                "md" | "mdx" | "markdown" | "txt" | "rst" | "adoc"
            ))
            && (self.exts.is_empty() || self.exts.contains(&ext))
            && !self.excludes.is_match(file)
            && !file
                .file_name()
                .is_some_and(|name| self.excludes.is_match(Path::new(name)))
    }
}

pub(crate) enum Scope {
    All,
    Directory(PathBuf),
    File(PathBuf),
}

impl Scope {
    fn within(root: &Path, path: &Path) -> Result<Self> {
        let relative = path.strip_prefix(root).with_context(|| {
            format!(
                "search path {} is outside index root {}",
                path.display(),
                root.display()
            )
        })?;
        Ok(if relative.as_os_str().is_empty() {
            Self::All
        } else if path.is_dir() {
            Self::Directory(relative.to_path_buf())
        } else {
            Self::File(relative.to_path_buf())
        })
    }

    pub(crate) fn contains(&self, file: &Path) -> bool {
        match self {
            Self::All => true,
            Self::Directory(dir) => file.starts_with(dir),
            Self::File(path) => file == path,
        }
    }
}
