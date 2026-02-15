#[allow(deprecated)]
use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

#[allow(deprecated)]
fn og() -> Command {
    Command::cargo_bin("og").unwrap()
}

/// Build an index in a temp directory by copying fixtures, return the temp dir.
fn build_fixture_index() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let fixtures = fixtures_dir();

    // Copy fixture files to temp dir
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let dest = tmp.path().join(entry.file_name());
        std::fs::copy(entry.path(), &dest).unwrap();
    }

    // Build index
    og().args(["build", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Indexed"));

    tmp
}

#[test]
fn build_creates_index() {
    let tmp = build_fixture_index();
    assert!(tmp.path().join(".og/manifest.json").exists());
}

#[test]
fn status_shows_files() {
    let tmp = build_fixture_index();

    og().args(["status", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("blocks"));
}

#[test]
fn search_finds_results() {
    let tmp = build_fixture_index();

    og().args(["error handling", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("errors.rs"));
}

#[test]
fn search_authentication() {
    let tmp = build_fixture_index();

    og().args(["authentication", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth.py"));
}

#[test]
fn search_no_match_exits_1() {
    let tmp = build_fixture_index();

    og().args(["zzzznonexistentqueryzzzz", tmp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("No results"));
}

#[test]
fn search_json_output() {
    let tmp = build_fixture_index();

    let output = og()
        .args(["--json", "error", tmp.path().to_str().unwrap(), "-n", "2"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.is_array());
    assert!(!parsed.as_array().unwrap().is_empty());

    let first = &parsed[0];
    assert!(first.get("file").is_some());
    assert!(first.get("type").is_some());
    assert!(first.get("name").is_some());
    assert!(first.get("line").is_some());
    assert!(first.get("score").is_some());
}

#[test]
fn search_files_only() {
    let tmp = build_fixture_index();

    og().args(["-l", "authentication", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth.py"));
}

#[test]
fn search_type_filter() {
    let tmp = build_fixture_index();

    // Filter to .py files only — use "password" which is unique to auth.py
    let output = og()
        .args(["-t", "py", "password", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("auth.py"));
    assert!(!stdout.contains("errors.rs"));
}

#[test]
fn search_limit_results() {
    let tmp = build_fixture_index();

    let output = og()
        .args([
            "--json",
            "-n",
            "1",
            "function",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

#[test]
fn clean_removes_index() {
    let tmp = build_fixture_index();
    assert!(tmp.path().join(".og").exists());

    og().args(["clean", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted"));

    assert!(!tmp.path().join(".og").exists());
}

#[test]
fn no_index_exits_2() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.rs"), "fn main() {}").unwrap();

    og().args(["query", tmp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("No index found"));
}

#[test]
fn build_force_rebuilds() {
    let tmp = build_fixture_index();

    og().args(["build", "--force", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Indexed"));
}

#[test]
fn incremental_update() {
    let tmp = build_fixture_index();

    // Add a new file
    std::fs::write(
        tmp.path().join("new_file.py"),
        "def hello_world():\n    print('hello')\n",
    )
    .unwrap();

    // Search should auto-update
    og().args(["hello", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Updating"));
}

#[test]
fn camel_case_query_matches() {
    let tmp = build_fixture_index();

    // The fixtures have camelCase identifiers (api_handlers.ts)
    // Query with split terms should match
    og().args(["user manager", tmp.path().to_str().unwrap()])
        .assert()
        .success();
}
