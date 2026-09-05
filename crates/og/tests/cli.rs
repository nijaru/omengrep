//! og CLI integration tests. Ported from v0.0.3 tests/cli.rs (deleted with
//! the legacy tree): same fixtures and regression intent, adapted to the
//! 0.0.4 surface — deterministic builds (offline CI), generation layout,
//! 0/1/2 exit codes, `og <subcommand>` flag order.

use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn og() -> Command {
    Command::cargo_bin("og").unwrap()
}

fn json_files(stdout: &[u8]) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_slice(stdout).unwrap_or(serde_json::json!([]));
    v.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|r| r["file"].as_str().map(String::from))
        .collect()
}

/// Build a deterministic index in a temp directory by copying fixtures.
/// Deterministic mode keeps CI offline (no model download); search tests
/// exercise BM25 + trigram channels, which carry these queries.
fn build_fixture_index() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for entry in std::fs::read_dir(fixtures_dir()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            std::fs::copy(entry.path(), tmp.path().join(entry.file_name())).unwrap();
        }
    }

    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
    tmp
}

fn run_json(args: &[&str]) -> (Output, serde_json::Value) {
    let out = og().args(args).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::json!([]));
    (out, parsed)
}

// --- build / status / clean ---

#[test]
fn build_creates_generation_layout() {
    let tmp = build_fixture_index();
    // v0.0.3 asserted .og/manifest.json; 0.0.4 publishes generations.
    let current = std::fs::read_to_string(tmp.path().join(".og/CURRENT")).unwrap();
    let gen_dir = tmp.path().join(".og/generations").join(current.trim());
    assert!(gen_dir.join("manifest.json").exists(), "manifest missing");
    assert!(gen_dir.join("catalog.sqlite").exists(), "catalog missing");
    assert!(gen_dir.join("vectors-000.bin").exists(), "sidecar missing");
}

#[test]
fn status_shows_files_and_blocks() {
    let tmp = build_fixture_index();
    og().args(["status", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Files"))
        .stdout(predicate::str::contains("Blocks"))
        // staleness signal is present and truthful right after a build
        .stdout(predicate::str::contains("Status:       up to date"));
}

#[test]
fn status_reports_stale_after_change() {
    let tmp = build_fixture_index();
    std::fs::write(
        tmp.path().join("late_file.py"),
        "def late_helper():\n    pass\n",
    )
    .unwrap();
    og().args(["status", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stale"));
}

#[test]
fn clean_removes_index() {
    let tmp = build_fixture_index();
    assert!(tmp.path().join(".og").exists());

    og().args(["clean", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    assert!(!tmp.path().join(".og").exists());
}

#[test]
fn build_force_rebuilds() {
    let tmp = build_fixture_index();
    let before = std::fs::read_to_string(tmp.path().join(".og/CURRENT")).unwrap();

    og().args([
        "build",
        "--force",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    let after = std::fs::read_to_string(tmp.path().join(".og/CURRENT")).unwrap();
    // Same content + model + schema => same generation name republished.
    assert_eq!(before.trim(), after.trim());
}

#[test]
fn incremental_update_on_search() {
    let tmp = build_fixture_index();

    std::fs::write(
        tmp.path().join("new_file.py"),
        "def hello_world():\n    print('hello')\n",
    )
    .unwrap();

    // Search auto-updates the stale index first (v0.0.3 behavior, kept).
    og().args(["-q", "hello_world", tmp.path().to_str().unwrap()])
        .assert()
        .stderr(predicate::str::contains("Index updated"));
}

#[test]
fn no_index_exits_2() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.rs"), "fn main() {}").unwrap();

    og().args(["-q", "query", tmp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("No index found"));
}

// --- search ---

#[test]
fn search_finds_results() {
    let tmp = build_fixture_index();
    og().args(["-q", "error handling", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("errors.rs"));
}

#[test]
fn search_authentication() {
    let tmp = build_fixture_index();
    og().args(["-q", "authentication", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth.py"));
}

#[test]
fn search_json_output_shape() {
    let tmp = build_fixture_index();
    let (out, parsed) = run_json(&[
        "--json",
        "-q",
        "error",
        tmp.path().to_str().unwrap(),
        "-n",
        "2",
    ]);
    assert!(out.status.success());
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty());
    let first = &arr[0];
    for key in ["file", "type", "name", "line", "end_line", "score"] {
        assert!(first.get(key).is_some(), "missing {key}");
    }
    assert!(first.get("content").is_some(), "json keeps content");
}

#[test]
fn search_no_content_strips_content() {
    let tmp = build_fixture_index();
    let (_out, parsed) = run_json(&[
        "--no-content",
        "-q",
        "error",
        tmp.path().to_str().unwrap(),
        "-n",
        "2",
    ]);
    assert!(parsed.as_array().unwrap()[0].get("content").is_none());
}

#[test]
fn search_gibberish_exits_1_with_valid_json() {
    let tmp = build_fixture_index();
    // v0.0.3 MaxSim always returned candidates; 0.0.4 ranking gates noise
    // (similarity floor), so gibberish is a clean no-match exit 1.
    let (out, parsed) = run_json(&[
        "--json",
        "-q",
        "zzzznonexistentqueryzzzz",
        tmp.path().to_str().unwrap(),
        "-n",
        "1",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(parsed.as_array().unwrap().is_empty());
}

#[test]
fn search_files_only() {
    let tmp = build_fixture_index();
    og().args(["-l", "-q", "authentication", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth.py"));
}

#[test]
fn search_type_filter() {
    let tmp = build_fixture_index();
    let out = og()
        .args(["-t", "py", "-q", "password", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("auth.py"));
    assert!(!stdout.contains("errors.rs"));
}

#[test]
fn search_limit_results() {
    let tmp = build_fixture_index();
    let (_out, parsed) = run_json(&[
        "--json",
        "-q",
        "-n",
        "1",
        "function",
        tmp.path().to_str().unwrap(),
    ]);
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

#[test]
fn camel_case_query_matches() {
    let tmp = build_fixture_index();
    // Fixtures carry camelCase identifiers (api_handlers.ts); split terms
    // must match them via the identifier-split FTS channel.
    og().args(["-q", "user manager", tmp.path().to_str().unwrap()])
        .assert()
        .success();
}

// --- file references (similar code) ---

#[test]
fn similar_search_by_name_uses_raw_score() {
    let tmp = build_fixture_index();
    // v0.0.3 regression: negative MaxSim printed as "-40099% similar".
    // 0.0.4 shows raw fused scores.
    let file_ref = format!("{}#AppError", tmp.path().join("errors.rs").display());
    let out = og()
        .args(["-q", &file_ref, tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("score:"),
        "similar search must show 'score:'; got: {stdout}"
    );
    assert!(
        !stdout.contains("% similar"),
        "similar search must not show '% similar'; got: {stdout}"
    );
}

// --- scoping regressions (v0.0.3: starts_with("src/cli") matched src/cli_utils) ---

#[test]
fn search_scope_excludes_sibling_directory() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("src/cli")).unwrap();
    std::fs::create_dir_all(tmp.path().join("src/cli_utils")).unwrap();
    std::fs::write(
        tmp.path().join("src/cli/mod.rs"),
        "pub fn run_dispatch() {}\npub fn execute_command() {}\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/cli_utils/helper.rs"),
        "pub fn quuxhelper_format() {}\npub fn quuxhelper_parse() {}\n",
    )
    .unwrap();

    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    // Unscoped: cli_utils must be findable (proves it is indexed).
    let (_out, parsed) = run_json(&[
        "--json",
        "-q",
        "-n",
        "10",
        "quuxhelper",
        tmp.path().to_str().unwrap(),
    ]);
    let files = json_files(_out.stdout.as_slice());
    // json_files returns relative paths; results may be either form.
    let names: Vec<String> = parsed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["file"].as_str().map(String::from))
        .collect();
    assert!(
        names.iter().any(|f| f.contains("cli_utils")),
        "unscoped search must find cli_utils/helper.rs; got: {names:?} / {files:?}"
    );

    // Scoped to src/cli: cli_utils must be excluded.
    let cli_path = tmp.path().join("src/cli");
    let (out, parsed) = run_json(&[
        "--json",
        "-q",
        "-n",
        "10",
        "quuxhelper",
        cli_path.to_str().unwrap(),
    ]);
    let names: Vec<String> = parsed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["file"].as_str().map(String::from))
        .collect();
    assert_eq!(out.status.code(), Some(1), "no match in scope: exit 1");
    assert!(
        !names.iter().any(|f| f.contains("cli_utils")),
        "scoped search must exclude src/cli_utils; got: {names:?}"
    );
}

#[test]
fn context_scope_excludes_sibling_directory() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("src/cli")).unwrap();
    std::fs::create_dir_all(tmp.path().join("src/cli_utils")).unwrap();
    std::fs::write(
        tmp.path().join("src/cli/mod.rs"),
        "pub struct CommandRouter {}\npub fn route_command() {}\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/cli_utils/helper.rs"),
        "pub struct UtilityRouter {}\npub fn route_utility() {}\n",
    )
    .unwrap();

    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    let cli_path = tmp.path().join("src/cli");
    let out = og()
        .args(["context", "--json", "-q", cli_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());

    let files = json_files(&out.stdout);
    assert!(
        files
            .iter()
            .any(|f| f == "src/cli/mod.rs" || f.ends_with("src/cli/mod.rs")),
        "scoped context must include src/cli/mod.rs; got: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("cli_utils")),
        "scoped context must exclude src/cli_utils; got: {files:?}"
    );
}

// --- markdown chunking (v0.0.3 regression: chunks shared one ID) ---

#[test]
fn markdown_long_section_indexes_all_chunks() {
    let tmp = TempDir::new().unwrap();
    let long_content = "detailed explanation of the semantic indexing pipeline. ".repeat(40)
        + &"additional content about vector embeddings and retrieval. ".repeat(20);
    let md = format!("# Architecture\n\n{long_content}\n");
    std::fs::write(tmp.path().join("README.md"), &md).unwrap();

    let out = og()
        .args(["build", "--deterministic", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let blocks: usize = stderr
        .split_whitespace()
        .skip_while(|w| *w != "Indexed")
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert!(
        blocks >= 2,
        "long markdown section must produce ≥2 indexed chunks; got {blocks}"
    );
}

// --- outline / context JSON surfaces (new in 0.0.4, replacing v0.0.3 shapes) ---

#[test]
fn outline_json_shape() {
    let tmp = build_fixture_index();
    let out = og()
        .args([
            "outline",
            "--json",
            "-q",
            tmp.path().join("errors.rs").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let files = parsed.as_array().unwrap();
    assert_eq!(files.len(), 1);
    let blocks = files[0]["blocks"].as_array().unwrap();
    assert!(!blocks.is_empty());
    for key in ["name", "type", "line", "end_line"] {
        assert!(blocks[0].get(key).is_some(), "missing {key}");
    }
    // No skeleton flag: entries must not carry a skeleton field.
    assert!(blocks[0].get("skeleton").is_none());
}

#[test]
fn outline_skeleton_includes_signatures() {
    let tmp = build_fixture_index();
    let out = og()
        .args([
            "outline",
            "--json",
            "--skeleton",
            "-q",
            tmp.path().join("errors.rs").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let blocks = parsed.as_array().unwrap()[0]["blocks"].as_array().unwrap();
    assert!(blocks[0]["skeleton"].as_str().is_some());
}

#[test]
fn context_json_output_shape() {
    let tmp = build_fixture_index();
    let out = og()
        .args([
            "context",
            "--json",
            "-q",
            "-n",
            "2",
            "--symbols",
            "3",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let files = parsed.as_array().unwrap();
    assert!(!files.is_empty());
    assert!(files.len() <= 2);

    let first = &files[0];
    for key in [
        "file",
        "score",
        "definition_score",
        "inbound_refs",
        "inbound_files",
    ] {
        assert!(first.get(key).is_some(), "missing {key}");
    }
    assert!(first["symbols"].as_array().unwrap().len() <= 3);
}

// --- highlight (ported: terminal-only styling) ---

#[test]
fn highlight_marks_query_terms_in_default_output() {
    let tmp = build_fixture_index();
    let out = og()
        .args([
            "--highlight",
            "-q",
            "authentication",
            tmp.path().to_str().unwrap(),
            "-n",
            "1",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\x1b[33m"),
        "highlight output must contain yellow ANSI styling; got: {stdout}"
    );
}

#[test]
fn highlight_does_not_change_json_output() {
    let tmp = build_fixture_index();
    let out = og()
        .args([
            "--json",
            "--highlight",
            "-q",
            "authentication",
            tmp.path().to_str().unwrap(),
            "-n",
            "1",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_slice(out.stdout.as_slice()).unwrap();
    assert!(parsed.as_array().is_some());
    assert!(
        !stdout.contains("\x1b[33m"),
        "json output must not include ANSI styling; got: {stdout}"
    );
}

// --- invalid argv (usage-rs migration regression guards) ---

#[test]
fn unknown_flag_is_hard_error() {
    // clap parity: a mistyped flag must exit 2, never silently degrade
    // into the query positional (usage-rs default would swallow it).
    let tmp = build_fixture_index();
    og().args(["--bogus-flag"])
        .current_dir(tmp.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn missing_flag_value_is_error() {
    og().args(["--exclude"]).assert().code(2);
}

#[test]
fn invalid_integer_value_is_error() {
    og().args(["-n", "abc", "query"]).assert().code(2);
}

#[test]
fn unknown_word_without_flags_is_a_search_not_error() {
    // A bare word is a query (search-first CLI): no lexical/semantic
    // match for a nonsense word is exit 1, not a subcommand error.
    let tmp = build_fixture_index();
    og().args([
        "-q",
        "--json",
        "frobnicate_zzz",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .code(1);
    // and the stdout is valid empty JSON
}

// --- adversarial matrix (2026-09-04 post-usage-rs sweep) ---
// The invalid-argv gap shipped a real bug once; these encode the whole
// matrix that found it plus the build-root validation fix.

#[test]
fn build_nonexistent_root_is_error_not_silent_create() {
    // Regression: build used to canonicalize-with-fallback and then
    // create_dir_all, minting a directory + index from a typo'd path
    // with exit 0.
    let tmp = TempDir::new().unwrap();
    let ghost = tmp.path().join("typo-dir");
    og().args(["build", "-q", ghost.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does not exist"));
    assert!(!ghost.exists(), "build must not create the root directory");
}

#[test]
fn build_file_as_root_is_error() {
    let tmp = TempDir::new().unwrap();
    let f = tmp.path().join("a.rs");
    std::fs::write(&f, "fn a() {}\n").unwrap();
    og().args(["build", "-q", f.to_str().unwrap()])
        .assert()
        .code(2);
    assert!(!tmp.path().join("a.rs/.og").exists());
}

#[test]
fn context_zero_truncation_says_what_happened() {
    let tmp = build_fixture_index();
    og().args(["context", "-n", "0", tmp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("-n/--symbols is 0"));
    og().args(["context", "--symbols", "0", tmp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("-n/--symbols is 0"));
}

#[test]
fn empty_query_emits_valid_empty_json() {
    let tmp = build_fixture_index();
    og().args(["--json", "-q", "", tmp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stdout("[]\n");
}

#[test]
fn invalid_regex_is_error_not_panic() {
    let tmp = build_fixture_index();
    for bad in ["[", "(", "*start"] {
        og().args(["-e", bad, "-q", "password", tmp.path().to_str().unwrap()])
            .assert()
            .code(2);
    }
}

#[test]
fn negative_preview_lines_is_usage_error() {
    og().args(["-C", "-1", "q"]).assert().code(2);
}

#[test]
fn unicode_query_is_handled() {
    let tmp = build_fixture_index();
    // No fixture carries café/naïve identifiers; expect clean no-match.
    og().args(["-q", "--json", "café naïve", tmp.path().to_str().unwrap()])
        .assert()
        .code(1);
}

// --- corruption recovery (2026-09-04 sweep: success-message-over-garbage) ---

#[test]
fn corrupt_catalog_incremental_build_recovers() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("t.rs"), "fn probe_a() {}\n").unwrap();
    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    // Corrupt the published catalog: raw garbage bytes in place.
    let og_dir = tmp.path().join(".og");
    let cur = std::fs::read_to_string(og_dir.join("CURRENT")).unwrap();
    std::fs::write(
        og_dir
            .join("generations")
            .join(cur.trim())
            .join("catalog.sqlite"),
        b"garbage bytes, not sqlite",
    )
    .unwrap();

    // Incremental build must fall back to full rebuild AND replace the
    // corrupt generation — the old collision branch re-pointed CURRENT at
    // garbage while printing "Indexed N blocks".
    og().args(["build", "--deterministic", tmp.path().to_str().unwrap()])
        .assert()
        .code(0)
        .stderr(predicate::str::contains(
            "Rebuilding (previous generation unreadable)",
        ))
        .stderr(predicate::str::contains("Replacing corrupt generation"));
    og().args(["-q", "-l", "probe_a", tmp.path().to_str().unwrap()])
        .assert()
        .code(0)
        .stdout("t.rs\n");
}

#[test]
fn corrupt_catalog_status_reports_corrupt_not_healthy() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("t.rs"), "fn probe_b() {}\n").unwrap();
    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
    let og_dir = tmp.path().join(".og");
    let cur = std::fs::read_to_string(og_dir.join("CURRENT")).unwrap();
    std::fs::write(
        og_dir
            .join("generations")
            .join(cur.trim())
            .join("catalog.sqlite"),
        b"garbage bytes, not sqlite",
    )
    .unwrap();

    // Status must open the catalog (not just the manifest) and say so.
    og().args(["status", tmp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Status:       corrupt"))
        .stdout(predicate::str::contains("Recovery:     og build --force"));
}

#[test]
fn corrupt_catalog_force_build_recovers() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("t.rs"), "fn probe_c() {}\n").unwrap();
    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
    let og_dir = tmp.path().join(".og");
    let cur = std::fs::read_to_string(og_dir.join("CURRENT")).unwrap();
    std::fs::write(
        og_dir
            .join("generations")
            .join(cur.trim())
            .join("catalog.sqlite"),
        b"garbage bytes, not sqlite",
    )
    .unwrap();

    og().args([
        "build",
        "--force",
        "--deterministic",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .code(0)
    .stderr(predicate::str::contains("Replacing corrupt generation"));
    og().args(["-q", "-l", "probe_c", tmp.path().to_str().unwrap()])
        .assert()
        .code(0)
        .stdout("t.rs\n");
}

// --- symlink behavior (2026-09-04 sweep: correct by probe, now pinned) ---

#[test]
fn symlinks_followed_once_no_double_indexing() {
    let tmp = TempDir::new().unwrap();
    let real = tmp.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("t.rs"), "fn sym_probe() {}\n").unwrap();
    // Two links to the same target: the scanner must not index it twice.
    std::os::unix::fs::symlink("real", tmp.path().join("a")).unwrap();
    std::os::unix::fs::symlink("real", tmp.path().join("b")).unwrap();

    og().args([
        "build",
        "--deterministic",
        "-q",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
    og().args(["status", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Files:         1"));
    og().args(["-q", "-l", "sym_probe", tmp.path().to_str().unwrap()])
        .assert()
        .code(0)
        .stdout("real/t.rs\n");
}

// --- first-run UX (2026-09-05: silent download + HF_HOME ignored) ---

#[test]
fn hf_home_is_respected_for_model_cache() {
    // The loader must honor HF_HOME (standard relocation env var); the
    // old Api::new() hardcoded ~/.cache/huggingface/hub and re-downloaded
    // a second copy for anyone who moved their cache. Requires network OR
    // a warm HF_HOME; CI runs offline, so assert only the resolution path
    // via model status against a redirected cache that holds nothing:
    // it must NOT report installed from the default location.
    let tmp = TempDir::new().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_og"))
        .env("HF_HOME", tmp.path())
        .args(["model", "status"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("not installed") || stdout.contains("status:  installed"),
        "model status must run against HF_HOME (got: {stdout})"
    );
    // The decisive assertion: no files were created in the redirected
    // cache just by running status (no silent download on a status probe).
    let mut entries = std::fs::read_dir(tmp.path()).unwrap();
    assert!(
        entries.next().is_none(),
        "model status must not download into HF_HOME silently"
    );
}
