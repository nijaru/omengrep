use std::collections::HashSet;

use crate::tokenize;
use crate::types::SearchResult;

/// Apply code-aware ranking boosts to search results.
///
/// Boosts:
/// - Exact name match: 2.5x (code queries only)
/// - Term overlap: +30% per matching term (code queries only, camelCase/snake_case aware)
/// - Content match: up to 2x for NL queries (query terms in block content)
/// - Type match: 1.5x if query mentions the type (e.g., "class", "function")
/// - Type hierarchy: function 1.3x, class 1.2x (fallback if no type in query)
/// - File path relevance: 1.15x (code queries only)
/// - Max total boost capped at 4x
///
/// IMPORTANT: omendb MaxSim scores are negative (less negative = more similar, like cosine
/// distance). Applying boost via multiplication makes negative scores worse. Instead we divide:
/// score /= boost for negative scores, score *= boost for positive. This correctly moves scores
/// toward zero (more similar) when boosting.
pub fn boost_results(results: &mut [SearchResult], query: &str) {
    if results.is_empty() || query.is_empty() {
        return;
    }

    let query_terms = tokenize::extract_terms(query);
    let query_set: HashSet<&str> = query_terms
        .iter()
        .filter(|t| t.len() >= 3 || SHORT_WHITELIST.contains(&t.as_str()))
        .map(|s| s.as_str())
        .collect();

    // Name/term boosts only apply to code-style queries (camelCase or snake_case).
    // NL queries like "parse HTTP headers" have no identifier patterns — applying
    // name boosts promotes wrong results that happen to share English words with the query.
    let is_code_query = looks_like_code_query(query);

    let query_wants_class = query_set
        .iter()
        .any(|t| matches!(*t, "class" | "struct" | "type"));
    let query_wants_func = query_set
        .iter()
        .any(|t| matches!(*t, "function" | "func" | "fn" | "method" | "def"));

    for r in results.iter_mut() {
        let mut boost: f64 = 1.0;
        let block_type = r.block_type.to_lowercase();

        // 1. Name and term matching (code queries only)
        if is_code_query {
            let name_lower = r.name.to_lowercase();
            let name_terms = tokenize::extract_terms(&r.name);
            let name_set: HashSet<&str> = name_terms.iter().map(|s| s.as_str()).collect();

            if !name_lower.is_empty() && query_set.contains(name_lower.as_str()) {
                boost *= 2.5;
            } else {
                let overlap = query_set.intersection(&name_set).count();
                if overlap > 0 {
                    boost *= 1.0 + (0.3 * overlap as f64);
                }
            }
        }

        // 2. Content match (NL queries only)
        // Count how many query terms appear in the block content. Functions whose body/docstring
        // contains most query terms are likely the semantically correct result.
        if !is_code_query && !query_set.is_empty() {
            if let Some(content) = &r.content {
                let content_lower = content.to_lowercase();
                let matching = query_set
                    .iter()
                    .filter(|&&t| content_lower.contains(t))
                    .count();
                if matching > 0 {
                    let ratio = matching as f64 / query_set.len() as f64;
                    boost *= 1.0 + ratio; // up to 2.0x at full match
                }
            }
        }

        // 3. Type boost
        let type_matches_query = (query_wants_class
            && matches!(block_type.as_str(), "class" | "struct"))
            || (query_wants_func && matches!(block_type.as_str(), "function" | "method"));

        if type_matches_query {
            boost *= 1.5;
        } else if !query_wants_class && !query_wants_func {
            boost *= match block_type.as_str() {
                "function" | "method" => 1.3,
                "class" | "struct" => 1.2,
                "interface" | "type" | "trait" | "enum" => 1.1,
                _ => 1.0,
            };
        }

        // 4. File path relevance (code queries only)
        if is_code_query {
            let file_path = r.file.to_lowercase();
            if query_set
                .iter()
                .any(|t| t.len() >= 3 && file_path.contains(*t))
            {
                boost *= 1.15;
            }
        }

        // Cap at 4x
        boost = boost.min(4.0);

        // Apply boost: divide negative scores (moves toward zero = more similar),
        // multiply positive scores. Multiplying negative scores by >1 makes them worse.
        if r.score < 0.0 {
            r.score /= boost as f32;
        } else {
            r.score *= boost as f32;
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Returns true if the query looks like a code identifier (camelCase or snake_case).
/// NL queries ("parse HTTP headers") return false — they contain no identifier patterns.
fn looks_like_code_query(query: &str) -> bool {
    if query.contains('_') {
        return true;
    }
    let bytes = query.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i - 1].is_ascii_lowercase() && bytes[i].is_ascii_uppercase() {
            return true;
        }
    }
    false
}

const SHORT_WHITELIST: &[&str] = &[
    "db", "fs", "io", "ui", "id", "ok", "fn", "rx", "tx", "api", "vm", "os", "gc", "ip", "sql",
    "cli", "tls", "rpc",
];
