/// Code vocabulary synonym table for BM25 query expansion.
///
/// Applied at query time only — expands natural language query terms to
/// common code equivalents, raising BM25 recall without touching indexed text.
///
/// Format: (term, &[synonyms]). Lookup is exact, lowercased match.
static SYNONYMS: &[(&str, &[&str])] = &[
    // Authentication / identity
    (
        "auth",
        &[
            "authenticate",
            "login",
            "session",
            "token",
            "credential",
            "oauth",
            "jwt",
        ],
    ),
    (
        "login",
        &["auth", "authenticate", "signin", "session", "credential"],
    ),
    ("logout", &["signout", "session", "invalidate", "revoke"]),
    ("token", &["auth", "jwt", "session", "credential", "bearer"]),
    ("session", &["auth", "token", "cookie", "state"]),
    (
        "password",
        &["secret", "credential", "hash", "bcrypt", "passwd"],
    ),
    (
        "credential",
        &["auth", "token", "password", "secret", "oauth"],
    ),
    ("oauth", &["auth", "token", "jwt", "credential"]),
    // Error handling
    (
        "error",
        &["exception", "err", "fail", "failure", "panic", "bail"],
    ),
    ("exception", &["error", "err", "throw", "raise", "panic"]),
    ("err", &["error", "exception", "fail", "result"]),
    ("panic", &["error", "exception", "bail", "abort", "crash"]),
    ("fail", &["error", "exception", "err", "abort", "invalid"]),
    ("retry", &["backoff", "reconnect", "attempt", "repeat"]),
    // Parsing / serialization
    (
        "parse",
        &[
            "tokenize",
            "lex",
            "decode",
            "deserialize",
            "unmarshal",
            "read",
        ],
    ),
    (
        "serialize",
        &["encode", "marshal", "write", "dump", "format"],
    ),
    ("deserialize", &["parse", "decode", "unmarshal", "read"]),
    ("decode", &["parse", "deserialize", "unmarshal", "read"]),
    ("encode", &["serialize", "marshal", "write", "format"]),
    (
        "format",
        &["serialize", "encode", "render", "display", "print"],
    ),
    // HTTP / networking
    (
        "http",
        &[
            "request", "response", "client", "fetch", "endpoint", "rest", "api",
        ],
    ),
    ("request", &["http", "query", "call", "fetch", "endpoint"]),
    ("response", &["http", "reply", "result", "output"]),
    ("client", &["http", "api", "connect", "call"]),
    ("endpoint", &["route", "handler", "url", "path", "api"]),
    ("url", &["uri", "link", "path", "endpoint", "route"]),
    ("fetch", &["http", "request", "get", "download", "call"]),
    ("webhook", &["callback", "endpoint", "event", "handler"]),
    // Database / persistence
    (
        "db",
        &["database", "query", "store", "repository", "model", "sql"],
    ),
    (
        "database",
        &["db", "store", "repository", "model", "sql", "orm"],
    ),
    (
        "query",
        &["search", "select", "find", "fetch", "filter", "sql"],
    ),
    (
        "store",
        &["db", "database", "repository", "save", "persist", "cache"],
    ),
    ("repository", &["db", "store", "database", "model"]),
    ("model", &["schema", "entity", "record", "struct"]),
    ("schema", &["model", "struct", "definition", "type"]),
    ("migrate", &["schema", "database", "upgrade", "alter"]),
    ("transaction", &["commit", "rollback", "atomic", "db"]),
    // Caching
    ("cache", &["buffer", "memo", "store", "memoize", "ttl"]),
    ("buffer", &["cache", "queue", "channel", "stream"]),
    ("memo", &["cache", "memoize", "store"]),
    ("ttl", &["cache", "expire", "timeout"]),
    // Testing
    (
        "test",
        &["spec", "assert", "check", "verify", "expect", "mock"],
    ),
    ("assert", &["check", "verify", "expect", "test"]),
    ("mock", &["stub", "fake", "spy", "fixture", "test"]),
    ("fixture", &["mock", "stub", "seed", "test", "setup"]),
    // Configuration
    (
        "config",
        &["settings", "options", "params", "env", "configuration"],
    ),
    ("settings", &["config", "options", "params", "env"]),
    ("env", &["config", "environment", "settings", "vars"]),
    ("flag", &["option", "config", "feature", "toggle"]),
    // Logging / observability
    (
        "log",
        &["trace", "debug", "warn", "info", "event", "record", "audit"],
    ),
    ("logger", &["log", "trace", "event", "audit", "sink"]),
    ("trace", &["log", "span", "debug", "instrument"]),
    (
        "metric",
        &["counter", "gauge", "histogram", "stat", "telemetry"],
    ),
    ("telemetry", &["metric", "trace", "log", "monitor"]),
    // Cryptography / security
    ("hash", &["sha", "md5", "checksum", "digest", "hmac"]),
    ("encrypt", &["cipher", "sign", "secure", "crypto", "tls"]),
    ("decrypt", &["cipher", "decode", "unseal", "crypto"]),
    ("sign", &["signature", "verify", "hmac", "hash", "cert"]),
    ("cert", &["certificate", "tls", "ssl", "x509", "sign"]),
    ("tls", &["ssl", "cert", "encrypt", "secure", "https"]),
    // File / IO
    (
        "file",
        &["path", "disk", "read", "write", "stream", "fs", "io"],
    ),
    ("read", &["load", "fetch", "get", "parse", "file", "io"]),
    (
        "write",
        &["save", "store", "flush", "persist", "file", "io"],
    ),
    ("path", &["file", "dir", "directory", "url", "route"]),
    (
        "stream",
        &["buffer", "channel", "io", "pipe", "reader", "writer"],
    ),
    ("upload", &["file", "store", "write", "multipart", "blob"]),
    ("download", &["fetch", "file", "read", "get", "stream"]),
    // Async / concurrency
    (
        "async",
        &["future", "promise", "concurrent", "parallel", "spawn"],
    ),
    ("future", &["async", "promise", "task", "deferred"]),
    (
        "channel",
        &["queue", "buffer", "stream", "pipe", "send", "recv"],
    ),
    ("queue", &["channel", "buffer", "heap", "priority", "deque"]),
    ("mutex", &["lock", "sync", "concurrent", "guard", "rwlock"]),
    ("lock", &["mutex", "sync", "guard", "concurrent"]),
    (
        "thread",
        &["task", "worker", "spawn", "async", "concurrent"],
    ),
    ("spawn", &["thread", "task", "async", "goroutine"]),
    // Networking / connections
    (
        "connect",
        &["socket", "channel", "link", "bind", "handshake"],
    ),
    ("socket", &["tcp", "udp", "connect", "bind", "listen"]),
    ("server", &["listen", "host", "serve", "bind", "handler"]),
    (
        "middleware",
        &["handler", "filter", "interceptor", "plugin"],
    ),
    ("proxy", &["middleware", "forward", "gateway", "reverse"]),
    // Data structures
    ("list", &["array", "vec", "slice", "collection", "items"]),
    ("map", &["dict", "table", "hash", "index", "lookup"]),
    ("set", &["unique", "dedup", "collection", "hash"]),
    ("tree", &["graph", "node", "recursive", "traverse"]),
    ("graph", &["node", "edge", "tree", "traverse", "dag"]),
    // Search / retrieval
    (
        "search",
        &["find", "query", "lookup", "filter", "scan", "index"],
    ),
    ("find", &["search", "query", "lookup", "filter", "get"]),
    (
        "filter",
        &["search", "query", "select", "predicate", "where"],
    ),
    ("index", &["search", "lookup", "db", "store", "inverted"]),
    ("rank", &["score", "sort", "relevance", "weight", "boost"]),
    // Routing / dispatch
    ("route", &["path", "url", "endpoint", "handler", "router"]),
    ("router", &["route", "dispatch", "handler", "middleware"]),
    (
        "handler",
        &["callback", "listener", "hook", "middleware", "endpoint"],
    ),
    ("dispatch", &["route", "call", "invoke", "handle", "send"]),
    // Events / callbacks
    (
        "event",
        &["callback", "hook", "listener", "signal", "trigger"],
    ),
    ("callback", &["event", "handler", "closure", "hook"]),
    (
        "hook",
        &["callback", "event", "handler", "plugin", "middleware"],
    ),
    ("signal", &["event", "interrupt", "notify", "trigger"]),
    // Validation / sanitization
    (
        "validate",
        &["check", "verify", "assert", "sanitize", "parse"],
    ),
    (
        "sanitize",
        &["validate", "clean", "escape", "filter", "encode"],
    ),
    ("verify", &["validate", "assert", "check", "confirm"]),
    // Memory / allocation
    ("memory", &["heap", "stack", "alloc", "leak", "gc"]),
    ("alloc", &["memory", "heap", "new", "malloc"]),
    ("gc", &["memory", "cleanup", "free", "collect"]),
    // Pagination / limits
    ("paginate", &["page", "offset", "limit", "cursor", "scroll"]),
    ("limit", &["paginate", "max", "threshold", "cap", "quota"]),
    ("offset", &["paginate", "skip", "cursor", "page"]),
    // Notifications / messaging
    ("notify", &["alert", "push", "send", "event", "signal"]),
    ("email", &["smtp", "mail", "send", "notify", "message"]),
    ("message", &["event", "notify", "send", "queue", "payload"]),
    (
        "payload",
        &["body", "data", "message", "content", "request"],
    ),
    // Compression / encoding
    ("compress", &["zip", "gzip", "deflate", "encode", "pack"]),
    ("decompress", &["unzip", "inflate", "decode", "unpack"]),
    // Rate limiting
    (
        "ratelimit",
        &["throttle", "quota", "limit", "backoff", "burst"],
    ),
    ("throttle", &["ratelimit", "limit", "backoff", "slow"]),
];

/// Expand BM25 query text with code vocabulary synonyms.
///
/// Takes the output of `split_identifiers` (original text + camelCase splits),
/// extracts lowercase terms, looks up each in the synonym table, and appends
/// any synonyms not already present in the text.
pub fn expand_query(text: &str) -> String {
    let terms = crate::tokenize::extract_terms(text);
    let text_lower = text.to_lowercase();

    let mut additions: Vec<&'static str> = Vec::new();
    for term in &terms {
        for &(key, synonyms) in SYNONYMS {
            if term == key {
                for &syn in synonyms {
                    if !text_lower.contains(syn) {
                        additions.push(syn);
                    }
                }
                break;
            }
        }
    }

    if additions.is_empty() {
        return text.to_string();
    }

    // Deduplicate preserving first occurrence
    let mut seen = std::collections::HashSet::new();
    additions.retain(|s| seen.insert(*s));

    format!("{text} {}", additions.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_auth() {
        let result = expand_query("auth");
        assert!(result.contains("authenticate"), "got: {result}");
        assert!(result.contains("login"), "got: {result}");
        assert!(result.contains("token"), "got: {result}");
    }

    #[test]
    fn expands_error() {
        let result = expand_query("error handling");
        assert!(result.contains("exception"), "got: {result}");
        assert!(result.contains("fail"), "got: {result}");
    }

    #[test]
    fn no_duplicates_in_expansion() {
        // "auth" and "login" both expand to overlapping sets — no dups
        let result = expand_query("auth login");
        let words: Vec<&str> = result.split_whitespace().collect();
        let unique: std::collections::HashSet<&str> = words.iter().copied().collect();
        assert_eq!(words.len(), unique.len(), "duplicates found: {result}");
    }

    #[test]
    fn no_expansion_for_unknown_terms() {
        let result = expand_query("foobar baz");
        assert_eq!(result, "foobar baz");
    }

    #[test]
    fn skips_terms_already_in_text() {
        // "login" is already in text — should not be added again via auth expansion
        let result = expand_query("auth login");
        let login_count = result.split_whitespace().filter(|&w| w == "login").count();
        assert_eq!(login_count, 1, "login appears more than once: {result}");
    }
}
