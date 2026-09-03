//! Outline + ranked context over the SQLite catalog. Ported from og
//! v0.0.3 cli/outline.rs + cli/context.rs: same scope filtering, ordering,
//! ranking math, and JSON shapes. Block rows come from catalog SQL instead
//! of omendb metadata lookups.
//!
//! Packing (new in the Rust rewrite, per the design doc's "token-budgeted
//! packing"): after the legacy count truncation (num_files /
//! symbols_per_file), a token budget (`--max-tokens`, ~4 chars/token)
//! drops lowest-priority items first. The first file's first symbol is
//! always kept, so packed output is never empty when blocks exist.

pub mod outline;
pub mod rank;

use std::path::{Path, PathBuf};

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::index::{self, Index};

pub use outline::{OutlineFile, build_outline, outline_json, print_default as print_outline};
pub use rank::{IndexedBlock, RankedFile, rank_context};

/// One catalog block row (0-indexed lines, as stored).
pub struct BlockRow {
    pub file: String,
    pub name: String,
    pub block_type: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub skeleton: String,
}

/// Approximate tokens for packing: ~4 chars/token (char-based, CJK-safe).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Remaining token budget. `take` always succeeds until anything has been
/// emitted, guaranteeing non-empty output when input is non-empty.
pub struct TokenBudget {
    remaining: i64,
}

impl TokenBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            remaining: max_tokens as i64,
        }
    }

    /// Charge `cost` tokens. Returns false when the item must be skipped
    /// (budget exhausted and output already committed).
    pub fn take(&mut self, cost: usize, committed: bool) -> bool {
        if !committed {
            self.remaining -= cost as i64;
            return true;
        }
        if self.remaining >= cost as i64 {
            self.remaining -= cost as i64;
            true
        } else {
            false
        }
    }
}

/// Open the nearest index for a read path, with v0.0.3 behaviors:
/// OG_AUTO_BUILD auto-build when missing, stale auto-update when found.
pub struct ScopedIndex {
    pub index_root: PathBuf,
    pub scope: Option<String>,
    pub index: Index,
}

pub fn open_scoped(path: &Path, quiet: bool) -> Result<ScopedIndex> {
    let start = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (index_root, found) = index::find_index_root(&start);
    if !found {
        if crate::search::auto_build_enabled() {
            let embedder = crate::model::default_embedder()?;
            index::build_with(&start, embedder.as_ref(), false, index::Incremental::Auto)?;
        } else {
            anyhow::bail!(
                "No index found. Run 'og build' first.\nTip: OG_AUTO_BUILD=1 enables auto-indexing."
            );
        }
    }

    if found && let Ok(true) = index::update_if_stale(&index_root, quiet) {
        eprintln!("Index updated");
    }

    let scope = start
        .strip_prefix(&index_root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty());
    let index = Index::open(&index_root)?;
    Ok(ScopedIndex {
        index_root,
        scope,
        index,
    })
}

/// Legacy scope rule: exact file match or `<prefix>/` directory prefix.
pub fn scope_matches(file: &str, scope: Option<&str>) -> bool {
    match scope {
        Some(prefix) => file == prefix || file.starts_with(&format!("{prefix}/")),
        None => true,
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// SQL predicate for `col` implementing [`scope_matches`].
fn scope_predicate(col: &str, scope: Option<&str>) -> (String, Vec<String>) {
    match scope {
        None => ("1 = 1".to_string(), Vec::new()),
        Some(prefix) => (
            format!("({col} = ?1 OR {col} LIKE ?2 ESCAPE '\\')"),
            vec![prefix.to_string(), format!("{}/%", escape_like(prefix))],
        ),
    }
}

/// Indexed file paths in scope, sorted (includes files with zero blocks).
pub fn files_in_scope(conn: &rusqlite::Connection, scope: Option<&str>) -> Result<Vec<String>> {
    let (pred, args) = scope_predicate("path", scope);
    let sql = format!("SELECT path FROM files WHERE {pred} ORDER BY path");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<String> = match args.as_slice() {
        [a, b] => stmt
            .query_map(rusqlite::params![a, b], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?,
        _ => stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?,
    };
    Ok(refs)
}

/// Catalog blocks in scope, ordered by file then start line.
/// `include_docs` keeps prose blocks (outline); context ranking skips them.
pub fn load_block_rows(
    conn: &rusqlite::Connection,
    scope: Option<&str>,
    include_docs: bool,
) -> Result<Vec<BlockRow>> {
    let (pred, args) = scope_predicate("file", scope);
    let doc_filter = if include_docs {
        ""
    } else {
        " AND block_type NOT IN ('text', 'section')"
    };
    let sql = format!(
        "SELECT file, name, block_type, start_line, end_line, content, skeleton \
         FROM blocks WHERE {pred}{doc_filter} ORDER BY file, start_line"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(BlockRow {
            file: row.get(0)?,
            name: row.get(1)?,
            block_type: row.get(2)?,
            start_line: row.get::<_, i64>(3)? as usize,
            end_line: row.get::<_, i64>(4)? as usize,
            content: row.get(5)?,
            skeleton: row.get(6)?,
        })
    };
    let rows: Vec<BlockRow> = match args.as_slice() {
        [a, b] => stmt
            .query_map(rusqlite::params![a, b], map_row)?
            .collect::<std::result::Result<_, _>>()?,
        _ => stmt
            .query_map([], map_row)?
            .collect::<std::result::Result<_, _>>()?,
    };
    Ok(rows)
}

/// Block structure of indexed files in scope. Returns empty when no files
/// are in scope (CLI maps this to EXIT_NO_MATCH / legacy EXIT_ERROR).
pub fn run_outline(
    path: &Path,
    include_skeleton: bool,
    max_tokens: usize,
    quiet: bool,
) -> Result<Vec<OutlineFile>> {
    let scoped = open_scoped(path, quiet)?;
    let files = files_in_scope(&scoped.index.conn, scoped.scope.as_deref())?;
    let rows = load_block_rows(&scoped.index.conn, scoped.scope.as_deref(), true)?;
    Ok(build_outline(&files, &rows, include_skeleton, max_tokens))
}

/// Ranked files + symbols for compact code context.
pub fn run_context(
    path: &Path,
    num_files: usize,
    symbols_per_file: usize,
    include_skeleton: bool,
    max_tokens: usize,
    quiet: bool,
) -> Result<Vec<RankedFile>> {
    let scoped = open_scoped(path, quiet)?;
    let rows = load_block_rows(&scoped.index.conn, scoped.scope.as_deref(), false)?;
    let blocks: Vec<IndexedBlock> = rows
        .into_iter()
        .map(|r| IndexedBlock {
            file: r.file,
            name: r.name,
            block_type: r.block_type,
            start_line: r.start_line,
            end_line: r.end_line,
            content: r.content,
            skeleton: r.skeleton,
        })
        .collect();
    let ranked = rank_context(&blocks, num_files, symbols_per_file, include_skeleton);
    Ok(pack_context(ranked, include_skeleton, max_tokens))
}

/// Token-budget packing over ranked files: rank order, include-if-fits,
/// skip-if-not, first file's first symbol always kept. Files left with no
/// symbols are dropped.
fn pack_context(
    ranked: Vec<RankedFile>,
    include_skeleton: bool,
    max_tokens: usize,
) -> Vec<RankedFile> {
    let mut budget = TokenBudget::new(max_tokens);
    let mut out: Vec<RankedFile> = Vec::with_capacity(ranked.len());
    for mut file in ranked {
        // Header first: a file that doesn't fit is skipped whole (except
        // the first file, which is guaranteed). Symbol costs follow.
        let header_cost = estimate_tokens(&file.file) + 2;
        if !budget.take(header_cost, !out.is_empty()) {
            continue;
        }
        let mut kept: Vec<rank::RankedSymbol> = Vec::with_capacity(file.symbols.len());
        for symbol in file.symbols.drain(..) {
            let mut cost = estimate_tokens(&symbol.name) + estimate_tokens(&symbol.block_type) + 2;
            if include_skeleton {
                cost += symbol.skeleton.as_deref().map(estimate_tokens).unwrap_or(0);
            }
            if !budget.take(cost, !out.is_empty() || !kept.is_empty()) {
                continue;
            }
            kept.push(symbol);
        }
        if kept.is_empty() {
            continue;
        }
        file.symbols = kept;
        out.push(file);
    }
    out
}

/// Legacy JSON shape: bare array of ranked files.
pub fn context_json(ranked: &[RankedFile]) -> serde_json::Value {
    serde_json::to_value(ranked).unwrap_or(serde_json::Value::Null)
}

/// Legacy default (human) rendering, verbatim from v0.0.3.
pub fn print_context(files: &[RankedFile]) {
    for file in files {
        println!(
            "{} {} {} {}",
            file.file.bold(),
            format!("score:{:.1}", file.score).dimmed(),
            format!("refs:{}", file.inbound_refs).dimmed(),
            format!("files:{}", file.inbound_files).dimmed(),
        );

        for symbol in &file.symbols {
            println!(
                "  {:>5}  {:<12}  {} {}",
                symbol.line,
                symbol.block_type.dimmed(),
                symbol.name,
                format!("score:{:.1}", symbol.score).dimmed(),
            );
            if let Some(skeleton) = &symbol.skeleton {
                for line in skeleton.lines() {
                    if !line.trim().is_empty() {
                        println!("         {}", line.dimmed());
                    }
                }
            }
        }
        println!();
    }
}
