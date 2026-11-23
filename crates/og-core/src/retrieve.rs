//! Retrieval: BM25 (FTS5 terms) + trigram + vector exact-scan channels,
//! fused with RRF, then code-aware boosts (ported policy).
//!
//! Channel details:
//! - bm25: `block_fts` MATCH over identifier-split terms. OR semantics
//!   with bm25 ranking — the mem dogfood lesson (fix 08e9fe4): partial
//!   term matches must rank, not AND-filter to zero.
//! - trigram: `block_trigram` MATCH for substring identifier hits.
//! - vector: exact cosine scan joined by rank. Participates whenever the
//!   pinned model covers all indexed blocks (always true in the slice:
//!   full rebuild embeds every block).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use crate::index::Index;
use crate::search::{Filters, Scope};
use crate::types::SearchResult;

/// RRF constant (standard 60 from TREC).
const RRF_K: f32 = 60.0;

/// Channel rankings: (block_id, raw score) in rank order.
pub type Channel = Vec<(i64, f32)>;

/// Fuse channel rankings with Reciprocal Rank Fusion.
///
/// RRF is rank-based: raw scores only order within a channel; weights
/// scale channel influence. Returns top-k (block_id, fused_score).
pub fn rrf_fuse(channels: &[(f32, Channel)], k: usize) -> Vec<(i64, f32)> {
    let mut fused: HashMap<i64, f32> = HashMap::new();
    for (weight, ranking) in channels {
        for (rank_idx, (block_id, _)) in ranking.iter().enumerate() {
            let rank = rank_idx as f32 + 1.0;
            *fused.entry(*block_id).or_insert(0.0) += weight / (RRF_K + rank);
        }
    }
    let mut out: Vec<(i64, f32)> = fused.into_iter().collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(k);
    out
}

/// How many candidates each channel fetches before fusion (fusion needs
/// deeper pools than the final k to be meaningful).
fn fetch_k(k: usize) -> usize {
    k.saturating_mul(10).clamp(50, i64::MAX as usize)
}

/// Escape a term for an FTS5 MATCH string literal (double-quote wrapping).
fn fts_quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Hybrid search over the published generation.
pub fn search(
    index: &Index,
    query: &str,
    k: usize,
    with_semantic: bool,
) -> Result<Vec<SearchResult>> {
    search_with_signal(
        index,
        query,
        k,
        with_semantic,
        &Filters::default(),
        &Scope::All,
    )
    .map(|(r, _)| r)
}

/// Search result plus the channel-participation signal: when the lexical
/// channels (BM25 + trigram) come up empty, hits are semantic-only noise
/// candidates rather than keyword matches. The CLI surfaces this so a
/// missed identifier looks different from a ranked hit.
pub(crate) fn search_with_signal(
    index: &Index,
    query: &str,
    k: usize,
    with_semantic: bool,
    filters: &Filters,
    scope: &Scope,
) -> Result<(Vec<SearchResult>, bool)> {
    if k == 0 || query.trim().is_empty() {
        return Ok((Vec::new(), false));
    }
    let pool = fetch_k(k);
    let eligible = Candidates::new(&index.conn, filters, scope, None)?;
    if eligible.is_empty() {
        return Ok((Vec::new(), false));
    }
    // Query preparation (ported): identifier split + synonym expansion.
    let split = crate::tokenize::split_identifiers(query);
    let expanded = crate::synonyms::expand_query(&split);

    let bm25_hits = bm25_search(&index.conn, &expanded, pool, &eligible)?;
    let trigram_hits = trigram_search(&index.conn, query, pool, &eligible)?;
    let vector_hits = if with_semantic {
        vector_search(index, query, pool, &eligible)?
    } else {
        Vec::new()
    };

    let lexical_matched = !bm25_hits.is_empty() || !trigram_hits.is_empty();

    let channels: Vec<(f32, Channel)> =
        vec![(1.0, bm25_hits), (0.7, trigram_hits), (1.0, vector_hits)];
    let fused = rrf_fuse(&channels, k);

    let mut results = hydrate(&index.conn, fused)?;
    crate::boost::boost_results(&mut results, query);
    Ok((results, lexical_matched))
}

/// Similar-code search from a reference block: vector scan joined with
/// BM25 on the reference's own terms.
pub fn similar(
    index: &Index,
    file: &str,
    line: Option<usize>,
    name: Option<&str>,
    k: usize,
) -> Result<Vec<SearchResult>> {
    similar_filtered(
        index,
        (file, line, name),
        k,
        true,
        &Filters::default(),
        &Scope::All,
    )
}

pub(crate) fn similar_filtered(
    index: &Index,
    (file, line, name): (&str, Option<usize>, Option<&str>),
    k: usize,
    with_semantic: bool,
    filters: &Filters,
    scope: &Scope,
) -> Result<Vec<SearchResult>> {
    if k == 0 {
        return Ok(Vec::new());
    }
    let Some((block_id, ref_name, ref_content)) = resolve_reference(&index.conn, file, line, name)?
    else {
        return Ok(Vec::new());
    };
    let pool = fetch_k(k);
    let eligible = Candidates::new(&index.conn, filters, scope, Some(block_id))?;
    if eligible.is_empty() {
        return Ok(Vec::new());
    }
    let vector_hits = if with_semantic {
        vector_search(index, &ref_content, pool, &eligible)?
    } else {
        Vec::new()
    };
    let ref_terms = crate::tokenize::split_identifiers(&ref_content);
    let bm25_hits = bm25_search(&index.conn, &ref_terms, pool, &eligible)?;

    let fused = rrf_fuse(&[(1.0, vector_hits), (0.5, bm25_hits)], k);
    let mut results = hydrate(&index.conn, fused)?;
    crate::boost::boost_results(&mut results, name.unwrap_or(&ref_name));
    Ok(results)
}

/// One eligibility set for all channels. The common all-files query
/// doesn't need a catalog pass; a similar-code query excludes its seed
/// by rowid (several different blocks can share a start line).
struct Candidates {
    only: Option<HashSet<i64>>,
    except: Option<i64>,
}

impl Candidates {
    fn new(
        conn: &Connection,
        filters: &Filters,
        scope: &Scope,
        except: Option<i64>,
    ) -> Result<Self> {
        let only = if matches!(scope, Scope::All) && filters.is_empty() {
            None
        } else {
            let mut ids = HashSet::new();
            let mut stmt =
                conn.prepare("SELECT id, file, name, CASE WHEN ?1 THEN content END FROM blocks")?;
            let mut rows = stmt.query([filters.regex.is_some()])?;
            while let Some(row) = rows.next()? {
                let file = Path::new(row.get_ref(1)?.as_str()?);
                if !scope.contains(file) || !filters.matches_file(file) {
                    continue;
                }
                if let Some(regex) = &filters.regex
                    && !regex.is_match(row.get_ref(2)?.as_str()?)
                    && !regex.is_match(row.get_ref(3)?.as_str()?)
                {
                    continue;
                }
                ids.insert(row.get(0)?);
            }
            Some(ids)
        };
        Ok(Self { only, except })
    }

    fn unrestricted(&self) -> bool {
        self.only.is_none() && self.except.is_none()
    }

    fn is_empty(&self) -> bool {
        self.only.as_ref().is_some_and(HashSet::is_empty)
    }

    fn contains(&self, id: i64) -> bool {
        self.except != Some(id) && self.only.as_ref().is_none_or(|ids| ids.contains(&id))
    }
}

// --- channels ---

/// BM25 over identifier-split terms, OR-matched (mem lesson 08e9fe4:
/// partial matches rank; full matches rank above partial).
fn bm25_search(conn: &Connection, terms: &str, k: usize, eligible: &Candidates) -> Result<Channel> {
    let split: Vec<&str> = terms.split_whitespace().collect();
    if split.is_empty() {
        return Ok(Vec::new());
    }
    let match_expr = split
        .iter()
        .map(|t| fts_quote(t))
        .collect::<Vec<_>>()
        .join(" OR ");

    fts_search(conn, "block_fts", &match_expr, k, eligible)
}

/// Trigram substring channel over block names.
fn trigram_search(
    conn: &Connection,
    query: &str,
    k: usize,
    eligible: &Candidates,
) -> Result<Channel> {
    let trimmed = query.trim();
    if trimmed.len() < 3 {
        return Ok(Vec::new());
    }
    fts_search(conn, "block_trigram", &fts_quote(trimmed), k, eligible)
}

fn fts_search(
    conn: &Connection,
    table: &str,
    query: &str,
    k: usize,
    eligible: &Candidates,
) -> Result<Channel> {
    // LIMIT must count eligible hits, not rank positions in the whole
    // index. Retain the SQLite limit for unfiltered searches.
    let sql = format!(
        "SELECT rowid, bm25({table}) AS score FROM {table}
                       WHERE {table} MATCH ?1 ORDER BY score, rowid LIMIT ?2"
    );
    let limit = if eligible.unrestricted() {
        k as i64
    } else {
        -1
    };
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params![query, limit])?;
    let mut hits = Vec::new();
    while hits.len() < k
        && let Some(row) = rows.next()?
    {
        let id = row.get(0)?;
        if eligible.contains(id) {
            hits.push((id, row.get(1)?));
        }
    }
    Ok(hits)
}

/// Vector exact scan. The embedder is constructed from the manifest's
/// pinned identity — never a default — so query and index vectors live in
/// the same space.
fn vector_search(index: &Index, query: &str, k: usize, eligible: &Candidates) -> Result<Channel> {
    if index.vectors.is_empty() {
        return Ok(Vec::new());
    }
    let embedder = crate::model::embedder_for(&index.manifest.model_id)?;
    anyhow::ensure!(
        embedder.dims() == index.vectors.dims(),
        "model/vector dimension mismatch"
    );
    let embedded = embedder.embed(&[query])?;
    Ok(index
        .vectors
        .top_k_where(&embedded[0], k, |id| eligible.contains(id)))
}

// --- hydration ---

/// Materialize ranked block ids as SearchResults with fused scores.
fn hydrate(conn: &rusqlite::Connection, ranked: Vec<(i64, f32)>) -> Result<Vec<SearchResult>> {
    if ranked.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::with_capacity(ranked.len());
    let mut stmt = conn.prepare(
        "SELECT file, block_type, name, start_line, end_line, content
                                 FROM blocks WHERE id = ?1",
    )?;
    for (block_id, fused) in ranked {
        let row = stmt.query_row([block_id], |row| {
            Ok(SearchResult {
                file: row.get(0)?,
                block_type: row.get(1)?,
                name: row.get(2)?,
                line: row.get::<_, i64>(3)? as usize + 1,
                end_line: row.get::<_, i64>(4)? as usize + 1,
                content: Some(row.get(5)?),
                score: fused,
            })
        });
        results.push(row?);
    }
    Ok(results)
}

// --- similar-search helpers ---

/// Resolve a reference (file, optional line/name) to (block_id, name, content).
fn resolve_reference(
    conn: &rusqlite::Connection,
    file: &str,
    line: Option<usize>,
    name: Option<&str>,
) -> Result<Option<(i64, String, String)>> {
    let read = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?, r.get(2)?));
    let row = if let Some(name) = name {
        conn.query_row(
            "SELECT id, name, content FROM blocks WHERE file = ?1 AND name = ?2
             ORDER BY start_line, id LIMIT 1",
            rusqlite::params![file, name],
            read,
        )
    } else if let Some(line) = line {
        let Some(zero_based) = line.checked_sub(1).and_then(|n| i64::try_from(n).ok()) else {
            return Ok(None);
        };
        conn.query_row(
            "SELECT id, name, content FROM blocks
             WHERE file = ?1 AND start_line <= ?2 AND end_line >= ?2
             ORDER BY (end_line - start_line), id LIMIT 1",
            rusqlite::params![file, zero_based],
            read,
        )
    } else {
        conn.query_row(
            "SELECT id, name, content FROM blocks WHERE file = ?1
             ORDER BY start_line, id LIMIT 1",
            [file],
            read,
        )
    };
    Ok(row.optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_basic_ordering() {
        let bm25: Channel = vec![(1, 1.0), (2, 0.9), (3, 0.8)];
        let vec_: Channel = vec![(2, 0.99), (4, 0.5), (1, 0.4)];
        let fused = rrf_fuse(&[(1.0, bm25), (1.0, vec_)], 10);
        // Block 2 appears high in both channels -> tops RRF.
        assert_eq!(fused[0].0, 2);
        // All four blocks represented.
        assert_eq!(fused.len(), 4);
    }

    #[test]
    fn rrf_respects_weights() {
        let a: Channel = vec![(1, 1.0)];
        let b: Channel = vec![(2, 1.0)];
        // Weight 2 channel dominates for rank-1 blocks.
        let fused = rrf_fuse(&[(1.0, a), (2.0, b)], 10);
        assert_eq!(fused[0].0, 2);
    }

    #[test]
    fn rrf_truncates_to_k() {
        let a: Channel = (1..=100).map(|i| (i, 1.0)).collect();
        let fused = rrf_fuse(&[(1.0, a)], 5);
        assert_eq!(fused.len(), 5);
    }

    #[test]
    fn rrf_empty_channels() {
        let fused = rrf_fuse(&[], 10);
        assert!(fused.is_empty());
    }
}
