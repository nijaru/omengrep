use std::sync::LazyLock;

use regex::Regex;

/// Regex matching identifier-like tokens (at least 2 chars, starts with letter).
static IDENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z][a-zA-Z0-9_]*[a-zA-Z0-9]").unwrap());

/// Regex matching camelCase boundaries (e.g., getUserProfile -> get|User|Profile).
static CAMEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").unwrap());

/// Regex matching ALLCAPS -> lowercase transitions (e.g., HTTPSClient -> HTTPS|Client).
static UPPER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([A-Z]+)([A-Z][a-z])").unwrap());

/// Split a single identifier into its component parts, lowercased.
///
/// Handles camelCase, PascalCase, ALLCAPS, and snake_case.
/// Returns empty vec if the word doesn't need splitting.
fn split_word(word: &str) -> Vec<String> {
    let has_camel = CAMEL_RE.is_match(word);
    let has_upper = UPPER_RE.is_match(word);
    let has_underscore = word.contains('_');

    if !has_camel && !has_upper && !has_underscore {
        return Vec::new();
    }

    // HTTPSClient -> HTTPS Client
    let expanded = UPPER_RE.replace_all(word, "$1 $2");
    // getUserProfile -> get User Profile
    let expanded = CAMEL_RE.replace_all(&expanded, "$1 $2");

    let parts: Vec<String> = expanded
        .split(['_', ' '])
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_lowercase())
        .collect();

    if parts.len() > 1 {
        parts
    } else {
        Vec::new()
    }
}

/// Language keywords that add noise to BM25 without discriminative value.
const KEYWORD_STOP_LIST: &[&str] = &[
    "pub",
    "fn",
    "let",
    "mut",
    "const",
    "use",
    "mod",
    "impl",
    "self",
    "crate",
    "super",
    "struct",
    "enum",
    "trait",
    "type",
    "where",
    "async",
    "await",
    "move",
    "ref",
    "return",
    "match",
    "loop",
    "while",
    "for",
    "break",
    "continue",
    "unsafe",
    "static",
    "extern",
    "dyn",
    "true",
    "false",
    "def",
    "class",
    "import",
    "from",
    "pass",
    "None",
    "True",
    "False",
    "elif",
    "else",
    "try",
    "except",
    "finally",
    "with",
    "yield",
    "lambda",
    "raise",
    "assert",
    "del",
    "global",
    "func",
    "var",
    "package",
    "defer",
    "chan",
    "select",
    "case",
    "default",
    "goto",
    "range",
    "void",
    "int",
    "char",
    "float",
    "double",
    "long",
    "short",
    "unsigned",
    "signed",
    "bool",
    "string",
    "null",
    "nil",
    "this",
    "new",
    "delete",
    "throw",
    "catch",
    "throws",
    "extends",
    "implements",
    "interface",
    "abstract",
    "final",
    "override",
    "virtual",
    "protected",
    "private",
    "public",
];

/// Split code identifiers for BM25 text search.
///
/// Finds camelCase and snake_case identifiers in the text and appends
/// their lowercase split forms. This allows BM25 to match queries like
/// "get user profile" against identifiers like `getUserProfile`.
///
/// The original text is preserved — split terms are appended at the end.
/// Language keywords are filtered from split terms to reduce noise.
pub fn split_identifiers(text: &str) -> String {
    let mut extra: Vec<String> = Vec::new();

    for mat in IDENT_RE.find_iter(text) {
        let word = mat.as_str();
        if word.len() < 4 {
            continue;
        }
        if KEYWORD_STOP_LIST.contains(&word) {
            continue;
        }
        let parts = split_word(word);
        for part in parts {
            if !KEYWORD_STOP_LIST.contains(&part.as_str()) {
                extra.push(part);
            }
        }
    }

    if extra.is_empty() {
        return text.to_string();
    }

    format!("{text} {}", extra.join(" "))
}

/// Extract lowercase terms from text, splitting camelCase and snake_case identifiers.
///
/// Used by boost.rs to compare query terms against block names.
pub fn extract_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();

    for mat in IDENT_RE.find_iter(text) {
        let word = mat.as_str();
        let parts = split_word(word);
        if parts.is_empty() {
            // No splitting needed — add as-is (lowercased)
            terms.push(word.to_lowercase());
        } else {
            terms.extend(parts);
        }
    }

    // Also pick up short words that the ident regex skips
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if !word.is_empty() && word.len() < 3 {
            terms.push(word.to_lowercase());
        }
    }

    terms.sort_unstable();
    terms.dedup();
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case() {
        let result = split_identifiers("getUserProfile");
        assert!(result.starts_with("getUserProfile"));
        assert!(result.contains("get"));
        assert!(result.contains("user"));
        assert!(result.contains("profile"));
    }

    #[test]
    fn snake_case() {
        let result = split_identifiers("get_user_profile");
        assert!(result.starts_with("get_user_profile"));
        assert!(result.contains("get"));
        assert!(result.contains("user"));
        assert!(result.contains("profile"));
    }

    #[test]
    fn upper_camel() {
        let result = split_identifiers("HTTPSConnection");
        assert!(result.contains("https"));
        assert!(result.contains("connection"));
    }

    #[test]
    fn no_split_needed() {
        let result = split_identifiers("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn short_words_skipped() {
        let result = split_identifiers("fn do");
        assert_eq!(result, "fn do");
    }

    #[test]
    fn mixed_content() {
        let result = split_identifiers("pub fn handleSearch(query: &str)");
        assert!(result.contains("handle"));
        assert!(result.contains("search"));
    }

    #[test]
    fn embedding_text_format() {
        let text = "function getUserProfile\npub fn get_user_profile(db: &Db) -> Result<Profile> {";
        let result = split_identifiers(text);
        assert!(result.contains("get"));
        assert!(result.contains("user"));
        assert!(result.contains("profile"));
    }

    #[test]
    fn preserves_term_frequency() {
        let result = split_identifiers("getUserProfile setUserProfile");
        let extra = result.split("setUserProfile ").nth(1).unwrap_or("");
        let terms: Vec<&str> = extra.split_whitespace().collect();
        // "user" and "profile" appear in both identifiers, so they should be repeated
        assert_eq!(terms.iter().filter(|&&t| t == "user").count(), 2);
        assert_eq!(terms.iter().filter(|&&t| t == "profile").count(), 2);
    }

    #[test]
    fn extract_terms_camel() {
        let terms = extract_terms("getUserProfile");
        assert!(terms.contains(&"get".to_string()));
        assert!(terms.contains(&"user".to_string()));
        assert!(terms.contains(&"profile".to_string()));
    }

    #[test]
    fn extract_terms_plain() {
        let terms = extract_terms("search");
        assert!(terms.contains(&"search".to_string()));
    }

    #[test]
    fn extract_terms_query() {
        let terms = extract_terms("error handling");
        assert!(terms.contains(&"error".to_string()));
        assert!(terms.contains(&"handling".to_string()));
    }

    #[test]
    fn extract_terms_short() {
        let terms = extract_terms("fn db io");
        assert!(terms.contains(&"fn".to_string()));
        assert!(terms.contains(&"db".to_string()));
        assert!(terms.contains(&"io".to_string()));
    }
}
