use std::collections::HashSet;

use crate::tokenize;
use crate::types::SearchResult;

/// Apply code-aware ranking boosts to search results.
///
/// Boosts:
/// - Exact name match: 2.5x
/// - Term overlap: +30% per matching term (camelCase/snake_case aware)
/// - Type match: 1.5x if query mentions the type (e.g., "class", "function")
/// - Type hierarchy: class 1.3x, function 1.2x (fallback if no type in query)
/// - File path relevance: 1.15x
/// - Max total boost capped at 4x
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

    let query_wants_class = query_set
        .iter()
        .any(|t| matches!(*t, "class" | "struct" | "type"));
    let query_wants_func = query_set
        .iter()
        .any(|t| matches!(*t, "function" | "func" | "fn" | "method" | "def"));

    for r in results.iter_mut() {
        let mut boost: f64 = 1.0;
        let name_lower = r.name.to_lowercase();
        let block_type = r.block_type.to_lowercase();
        let file_path = r.file.to_lowercase();

        // Extract terms from the block name (splits camelCase/snake_case)
        let name_terms = tokenize::extract_terms(&r.name);
        let name_set: HashSet<&str> = name_terms.iter().map(|s| s.as_str()).collect();

        // 1. Name matching
        if !name_lower.is_empty() && query_set.contains(name_lower.as_str()) {
            boost *= 2.5;
        } else {
            let overlap = query_set.intersection(&name_set).count();
            if overlap > 0 {
                boost *= 1.0 + (0.3 * overlap as f64);
            }
        }

        // 2. Type boost
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

        // 3. File path relevance
        if query_set
            .iter()
            .any(|t| t.len() >= 3 && file_path.contains(*t))
        {
            boost *= 1.15;
        }

        // Cap at 4x
        boost = boost.min(4.0);
        r.score *= boost as f32;
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

const SHORT_WHITELIST: &[&str] = &[
    "db", "fs", "io", "ui", "id", "ok", "fn", "rx", "tx", "api", "vm", "os", "gc", "ip", "sql",
    "cli", "tls", "rpc",
];
