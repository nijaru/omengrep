//! Golden tests for the tk-4z53 outline/context port: exact JSON shapes
//! against a fixed fixture (deterministic embedder — no model download).
//!
//! The ranking math and JSON shapes are the v0.0.3 baseline; the goldens
//! pin them against the new SQLite catalog.

use og_core::context;
use og_core::index::{self, Incremental};
use og_core::model::DeterministicEmbedder;

const ALPHA_RS: &str = r#"pub struct Config {
    pub name: String,
    pub retries: u32,
}

impl Config {
    pub fn load(path: &str) -> Config {
        Config { name: path.to_string(), retries: 3 }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if key == "name" {
            Some(self.name.clone())
        } else {
            None
        }
    }
}

fn helper(value: u32) -> u32 {
    value + 1
}
"#;

const BETA_RS: &str = r#"use crate::alpha::Config;

pub fn run(path: &str) -> String {
    let cfg = Config::load(path);
    cfg.get("name").unwrap_or_default()
}
"#;

const NOTES_MD: &str = "# Notes\n\nSome prose about Config and load.\n";

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("src").join("empty")).unwrap();
    std::fs::write(root.join("src/alpha.rs"), ALPHA_RS).unwrap();
    std::fs::write(root.join("src/beta.rs"), BETA_RS).unwrap();
    std::fs::write(root.join("notes.md"), NOTES_MD).unwrap();
    let embedder = DeterministicEmbedder::default();
    index::build_with(root, &embedder, true, Incremental::Auto).unwrap();
    dir
}

#[test]
fn outline_skeleton_golden() {
    let dir = fixture();
    let files = context::run_outline(
        &dir.path().join("src/alpha.rs"),
        true,  // skeleton
        8000,  // max tokens (default; not binding on this fixture)
        false, // quiet
    )
    .unwrap();
    let json = serde_json::to_string_pretty(&context::outline_json(&files)).unwrap();
    let expected = r#"[
  {
    "blocks": [
      {
        "end_line": 4,
        "line": 1,
        "name": "Config",
        "skeleton": "pub struct Config { ... }",
        "type": "class"
      },
      {
        "end_line": 9,
        "line": 7,
        "name": "load",
        "skeleton": "pub fn load(path: &str) -> Config { ... }",
        "type": "function"
      },
      {
        "end_line": 17,
        "line": 11,
        "name": "get",
        "skeleton": "pub fn get(&self, key: &str) -> Option<String> { ... }",
        "type": "function"
      },
      {
        "end_line": 22,
        "line": 20,
        "name": "helper",
        "skeleton": "fn helper(value: u32) -> u32 { ... }",
        "type": "function"
      }
    ],
    "file": "src/alpha.rs"
  }
]"#;
    assert_eq!(json, expected);
}

#[test]
fn outline_scopes_to_subdir() {
    let dir = fixture();
    let files = context::run_outline(&dir.path().join("src"), false, 8000, false).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(names, vec!["src/alpha.rs", "src/beta.rs"]);
    // No skeletons requested: entries carry None.
    assert!(
        files
            .iter()
            .all(|f| f.blocks.iter().all(|b| b.skeleton.is_none()))
    );
}

#[test]
fn outline_includes_prose_but_context_excludes_it() {
    let dir = fixture();
    let root = dir.path();
    // Outline lists every indexed file, prose included.
    let files = context::run_outline(root, false, 8000, false).unwrap();
    assert!(files.iter().any(|f| f.file == "notes.md"));
    // Context ranking only sees symbol blocks.
    let ranked = context::run_context(root, 12, 5, false, 8000, false).unwrap();
    assert!(!ranked.iter().any(|f| f.file == "notes.md"));
}

#[test]
fn context_golden() {
    let dir = fixture();
    let ranked = context::run_context(dir.path(), 5, 3, false, 8000, false).unwrap();
    let json = serde_json::to_string_pretty(&context::context_json(&ranked)).unwrap();
    let expected = r#"[
  {
    "definition_score": 9.0,
    "file": "src/alpha.rs",
    "inbound_files": 1,
    "inbound_refs": 2,
    "score": 16.0,
    "symbols": [
      {
        "end_line": 4,
        "inbound_files": 1,
        "inbound_refs": 1,
        "line": 1,
        "name": "Config",
        "score": 8.5,
        "type": "class"
      },
      {
        "end_line": 9,
        "inbound_files": 1,
        "inbound_refs": 1,
        "line": 7,
        "name": "load",
        "score": 7.0,
        "type": "function"
      },
      {
        "end_line": 17,
        "inbound_files": 0,
        "inbound_refs": 0,
        "line": 11,
        "name": "get",
        "score": 2.0,
        "type": "function"
      }
    ]
  },
  {
    "definition_score": 2.0,
    "file": "src/beta.rs",
    "inbound_files": 0,
    "inbound_refs": 0,
    "score": 2.0,
    "symbols": [
      {
        "end_line": 6,
        "inbound_files": 0,
        "inbound_refs": 0,
        "line": 3,
        "name": "run",
        "score": 2.0,
        "type": "function"
      }
    ]
  }
]"#;
    assert_eq!(json, expected);
}

#[test]
fn context_token_budget_keeps_top_ranked_first() {
    let dir = fixture();
    // Starvation budget: only the top file's top symbol must survive.
    let ranked = context::run_context(dir.path(), 12, 5, false, 4, false).unwrap();
    assert!(!ranked.is_empty());
    assert_eq!(ranked[0].file, "src/alpha.rs");
    assert!(!ranked[0].symbols.is_empty());
    assert_eq!(ranked[0].symbols[0].name, "Config");
}

#[test]
fn outline_empty_scope_returns_no_files() {
    let dir = fixture();
    let files = context::run_outline(&dir.path().join("src/empty"), false, 8000, false).unwrap();
    assert!(files.is_empty());
    let ranked =
        context::run_context(&dir.path().join("src/empty"), 12, 5, false, 8000, false).unwrap();
    assert!(ranked.is_empty());
}

#[test]
fn scope_matches_legacy_rule() {
    assert!(context::scope_matches("src/a.rs", None));
    assert!(context::scope_matches("src/a.rs", Some("src/a.rs")));
    assert!(context::scope_matches("src/nested/a.rs", Some("src")));
    assert!(!context::scope_matches("src/a.rs", Some("src/a")));
    assert!(!context::scope_matches("other/b.rs", Some("src")));
    // LIKE metacharacters must not act as wildcards.
    assert!(!context::scope_matches("src100/a.rs", Some("src_")));
    assert!(context::scope_matches("src_/a.rs", Some("src_")));
}

#[test]
fn scope_like_metacharacters_do_not_wildcard_in_sql() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("my_mod")).unwrap();
    std::fs::create_dir_all(root.join("myXmod")).unwrap();
    std::fs::write(root.join("my_mod/a.rs"), "fn under() {}\n").unwrap();
    std::fs::write(root.join("myXmod/b.rs"), "fn beside() {}\n").unwrap();
    let embedder = DeterministicEmbedder::default();
    index::build_with(root, &embedder, true, Incremental::Auto).unwrap();

    // `_` in the scope must match a literal underscore, not any char.
    let files = context::run_outline(&root.join("my_mod"), false, 8000, false).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(names, vec!["my_mod/a.rs"]);
}
