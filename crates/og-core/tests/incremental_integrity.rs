//! Incremental-build integrity (tk-0ql2 invariants).
//!
//! Regression: SQLite reuses a freed max rowid on re-insert, so vector rows
//! MUST be written at the rowids SQLite actually assigned — never at
//! append-arithmetic positions. The original bug: modify the file holding
//! the highest rowids → its blocks re-insert at the freed rowid while the
//! vector lands at max+1 → misalignment → vector-channel hydration errors
//! and silently wrong rankings.

use std::path::Path;

use og_core::index::{self, Incremental, Index};
use og_core::model::DeterministicEmbedder;
use og_core::model::Embedder;
use og_core::retrieve;

fn write(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
}

fn build(root: &Path) {
    let e = DeterministicEmbedder::default();
    index::build_with(root, &e, true, Incremental::Auto).unwrap();
}

#[test]
fn incremental_updates_keep_vector_rows_aligned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();

    write(&src.join("a.rs"), "fn alpha() {}\n");
    write(&src.join("b.rs"), "fn beta() {}\n"); // b gets the highest rowids
    write(&src.join("c.rs"), "fn gamma() {}\n");
    build(root);

    // Modify the file holding the max rowid: delete frees its rowid, and
    // SQLite re-assigns the same freed rowid (not max+1) on re-insert.
    write(
        &src.join("b.rs"),
        "fn beta_v2() { println!(\"changed\"); }\n",
    );
    build(root);

    let idx = Index::open(root).unwrap();

    // BM25 + vector channels hydrate every hit (misalignment would fail
    // hydration with "Query returned no rows" or return wrong blocks).
    let results = retrieve::search(&idx, "beta_v2", 5, true).unwrap();
    assert!(
        results.iter().any(|r| r.name == "beta_v2"),
        "updated block not found: {:?}",
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
    );

    // Vector channel specifically: every ranked block must exist.
    let e = DeterministicEmbedder::default();
    let q = e.embed(&["beta_v2"]).unwrap();
    let hits = idx.vectors.top_k(&q[0], 10);
    for (rowid, score) in &hits {
        let exists: bool = idx
            .conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE id = ?1",
                rusqlite::params![rowid],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        assert!(exists, "vector row {rowid} (score {score}) has no block");
    }

    // Similar-code (file#name path) works on the updated generation.
    let similar = retrieve::similar(&idx, "src/b.rs", None, Some("beta_v2"), 3).unwrap();
    assert!(
        similar.iter().all(|r| r.name != "beta_v2"),
        "reference block must be excluded"
    );
}

#[test]
fn incremental_deletion_removes_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();

    write(&src.join("a.rs"), "fn alpha() {}\n");
    write(&src.join("b.rs"), "fn beta() {}\n");
    build(root);

    std::fs::remove_file(src.join("a.rs")).unwrap();
    build(root);

    let idx = Index::open(root).unwrap();
    let count: i64 = idx
        .conn
        .query_row(
            "SELECT COUNT(*) FROM blocks WHERE file = 'src/a.rs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "deleted file still has blocks");
    let files: i64 = idx
        .conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(files, 1);
}

#[test]
fn generation_names_mix_model_identity() {
    // Same content, different models => different generation names; the
    // manifest in staging is published with the rename (never mutated in
    // place under the old generation name).
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    write(&root.join("src/x.rs"), "fn xray() {}\n");

    let det = DeterministicEmbedder::default();
    index::build_with(root, &det, true, Incremental::Auto).unwrap();
    let gen1 = index_dir_generation(root);

    // Rebuild same content + same model: republishes the same name.
    index::build_with(root, &det, true, Incremental::Auto).unwrap();
    let gen2 = index_dir_generation(root);
    assert_eq!(
        gen1, gen2,
        "same content+model must republish same generation"
    );

    // A deterministic-vs-potion identity difference can't be exercised
    // without the model download; the guard is exercised via the manifest
    // identity check in build_with (covered by unit tests + manual gates).
}

#[test]
fn staging_leftovers_are_cleaned_on_next_build() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    write(&root.join("src/y.rs"), "fn yankee() {}\n");
    build(root);

    // Simulate an interrupted build: leftover .staging directory.
    let staging = root.join(".og").join("generations").join(".staging");
    std::fs::create_dir_all(&staging).unwrap();
    write(&staging.join("garbage.txt"), "interrupted");

    build(root);
    assert!(
        !staging.exists(),
        "staging must be cleared by the next build"
    );
    assert!(Index::open(root).is_ok(), "index remains readable");
}

fn index_dir_generation(root: &Path) -> String {
    std::fs::read_to_string(root.join(".og").join("CURRENT"))
        .unwrap()
        .trim()
        .to_string()
}
