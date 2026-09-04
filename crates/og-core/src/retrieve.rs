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

use std::collections::HashMap;

use anyhow::Result;

use crate::index::Index;
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
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(k);
    out
}

/// How many candidates each channel fetches before fusion (fusion needs
/// deeper pools than the final k to be meaningful).
fn fetch_k(k: usize) -> usize {
    (k * 10).max(50)
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
    search_with_signal(index, query, k, with_semantic).map(|(r, _)| r)
}

/// Search result plus the channel-participation signal: when the lexical
/// channels (BM25 + trigram) come up empty, hits are semantic-only noise
/// candidates rather than keyword matches. The CLI surfaces this so a
/// missed identifier looks different from a ranked hit.
pub fn search_with_signal(
    index: &Index,
    query: &str,
    k: usize,
    with_semantic: bool,
) -> Result<(Vec<SearchResult>, bool)> {
    // Query preparation (ported): identifier split + synonym expansion.
    let split = crate::tokenize::split_identifiers(query);
    let expanded = crate::synonyms::expand_query(&split);

    let bm25_hits = bm25_search(&index.conn, &expanded, fetch_k(k))?;
    let trigram_hits = trigram_search(&index.conn, query, fetch_k(k))?;
    let vector_hits = if with_semantic {
        vector_search(index, query, fetch_k(k))?
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
    let Some((block_id, ref_name, ref_content)) = resolve_reference(&index.conn, file, line, name)?
    else {
        return Ok(Vec::new());
    };

    // Vector channel: embed the reference content, exact scan (manifest-pinned model).
    let embedder = crate::model::embedder_for(&index.manifest.model_id)?;
    let embedded = embedder.embed(&[&ref_content])?;
    let vector_hits = index.vectors.top_k(&embedded[0], k + 1);

    // BM25 channel: the reference's own identifier terms bring name siblings.
    let ref_terms = crate::tokenize::split_identifiers(&ref_content);
    let bm25_hits = bm25_search(&index.conn, &ref_terms, fetch_k(k))?;

    let mut channels: Vec<(f32, Channel)> = vec![(1.0, vector_hits)];
    if !bm25_hits.is_empty() {
        channels.push((0.5, bm25_hits));
    }
    let fused = rrf_fuse(&channels, k + 1);

    let mut results = hydrate(&index.conn, fused)?;
    // Exclude the reference block itself.
    results.retain(|r| r_row_id(&index.conn, &r.file, r.line) != Some(block_id));
    crate::boost::boost_results(&mut results, name.unwrap_or(&ref_name));
    results.truncate(k);
    Ok(results)
}

// --- channels ---

/// BM25 over identifier-split terms, OR-matched (mem lesson 08e9fe4:
/// partial matches rank; full matches rank above partial).
fn bm25_search(conn: &rusqlite::Connection, terms: &str, k: usize) -> Result<Channel> {
    let split: Vec<&str> = terms.split_whitespace().collect();
    if split.is_empty() {
        return Ok(Vec::new());
    }
    let match_expr = split
        .iter()
        .map(|t| fts_quote(t))
        .collect::<Vec<_>>()
        .join(" OR ");

    let sql = format!(
        "SELECT block_id, bm25(block_fts) AS score
         FROM block_fts
         WHERE block_fts MATCH ?
         ORDER BY score
         LIMIT {k}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([&match_expr], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Trigram substring channel over block names.
fn trigram_search(conn: &rusqlite::Connection, query: &str, k: usize) -> Result<Channel> {
    let trimmed = query.trim();
    if trimmed.len() < 3 {
        return Ok(Vec::new());
    }
    let match_expr = fts_quote(trimmed);

    let sql = format!(
        "SELECT block_id, bm25(block_trigram) AS score
         FROM block_trigram
         WHERE block_trigram MATCH ?
         ORDER BY score
         LIMIT {k}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([&match_expr], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Vector exact scan. The embedder is constructed from the manifest's
/// pinned identity — never a default — so query and index vectors live in
/// the same space. Cache-miss at query time degrades to no vector channel.
fn vector_search(index: &Index, query: &str, k: usize) -> Result<Channel> {
    let embedder = crate::model::embedder_for(&index.manifest.model_id)?;
    let embedded = embedder.embed(&[query])?;
    Ok(index.vectors.top_k(&embedded[0], k))
}

// --- hydration ---

/// Materialize ranked block ids as SearchResults with fused scores.
fn hydrate(conn: &rusqlite::Connection, ranked: Vec<(i64, f32)>) -> Result<Vec<SearchResult>> {
    if ranked.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::with_capacity(ranked.len());
    for (block_id, fused) in ranked {
        let row = conn.query_row(
            "SELECT file, block_type, name, start_line, end_line, content
             FROM blocks WHERE id = ?1",
            rusqlite::params![block_id],
            |row| {
                Ok(SearchResult {
                    file: row.get(0)?,
                    block_type: row.get(1)?,
                    name: row.get(2)?,
                    line: row.get::<_, i64>(3)? as usize,
                    end_line: row.get::<_, i64>(4)? as usize,
                    content: Some(row.get(5)?),
                    score: fused,
                })
            },
        );
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
    // By name: exact match within the file.
    if let Some(name) = name {
        let row = conn.query_row(
            "SELECT id, name, content FROM blocks WHERE file = ?1 AND name = ?2
             ORDER BY start_line LIMIT 1",
            rusqlite::params![file, name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        );
        if let Ok(hit) = row {
            return Ok(Some(hit));
        }
    }

    // By line: smallest containing block, else first block in the file.
    if let Some(line) = line {
        let row = conn.query_row(
            "SELECT id, name, content FROM blocks
             WHERE file = ?1 AND start_line <= ?2 AND end_line >= ?2
             ORDER BY (end_line - start_line) LIMIT 1",
            rusqlite::params![file, line as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        );
        if let Ok(hit) = row {
            return Ok(Some(hit));
        }
    }

    // By file: first block.
    let row = conn.query_row(
        "SELECT id, name, content FROM blocks WHERE file = ?1
         ORDER BY start_line LIMIT 1",
        rusqlite::params![file],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    );
    match row {
        Ok(hit) => Ok(Some(hit)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Integer row id for a (file, start_line) — used to exclude the reference.
fn r_row_id(conn: &rusqlite::Connection, file: &str, start_line: usize) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM blocks WHERE file = ?1 AND start_line = ?2 LIMIT 1",
        rusqlite::params![file, start_line as i64],
        |r| r.get::<_, i64>(0),
    )
    .ok()
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
