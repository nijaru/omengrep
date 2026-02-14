use regex::Regex;

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

    let query_lower = query.to_lowercase();

    // Split camelCase and snake_case
    let camel_re = Regex::new(r"([a-z])([A-Z])").unwrap();
    let expanded = camel_re.replace_all(&query_lower, "$1 $2");
    let split_re = Regex::new(r"[\s_\-./]+").unwrap();
    let query_terms: std::collections::HashSet<&str> = split_re
        .split(&expanded)
        .filter(|t| !t.is_empty())
        .filter(|t| t.len() >= 3 || SHORT_WHITELIST.contains(t))
        .collect();

    let query_wants_class = query_terms
        .iter()
        .any(|t| matches!(*t, "class" | "struct" | "type"));
    let query_wants_func = query_terms
        .iter()
        .any(|t| matches!(*t, "function" | "func" | "fn" | "method" | "def"));

    for r in results.iter_mut() {
        let mut boost: f64 = 1.0;
        let name = r.name.to_lowercase();
        let block_type = r.block_type.to_lowercase();
        let file_path = r.file.to_lowercase();

        // Expand name terms
        let name_expanded = camel_re.replace_all(&name, "$1 $2");
        let name_terms: std::collections::HashSet<&str> = split_re
            .split(&name_expanded)
            .filter(|t| !t.is_empty())
            .collect();

        // 1. Name matching
        if !name.is_empty() && query_terms.contains(name.as_str()) {
            boost *= 2.5;
        } else {
            let overlap = query_terms.intersection(&name_terms).count();
            if overlap > 0 {
                boost *= 1.0 + (0.3 * overlap as f64);
            }
        }

        // 2. Type boost
        if query_wants_class && matches!(block_type.as_str(), "class" | "struct") {
            boost *= 1.5;
        } else if query_wants_func && matches!(block_type.as_str(), "function" | "method") {
            boost *= 1.5;
        } else if !query_wants_class && !query_wants_func {
            boost *= match block_type.as_str() {
                "class" | "struct" => 1.3,
                "function" | "method" => 1.2,
                "interface" | "type" | "trait" | "enum" => 1.1,
                _ => 1.0,
            };
        }

        // 3. File path relevance
        if query_terms
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

const SHORT_WHITELIST: &[&str] = &["db", "fs", "io", "ui", "id", "ok", "fn", "rx", "tx", "api"];
