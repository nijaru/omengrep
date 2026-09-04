//! Index lifecycle: build a generation, publish atomically via CURRENT,
//! open the published generation, locate index roots by walking up.
//!
//! Layout (ai/design/rust-rewrite-2026-09.md):
//! ```text
//! .og/
//! ├── CURRENT                  # pointer file: generation dir name
//! └── generations/
//!     └── g-<hash>/
//!         ├── catalog.sqlite  # files, blocks, block_fts, block_trigram
//!         ├── vectors-000.bin  # f32 rows in blocks.id order
//!         └── manifest.json
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::catalog;
use crate::manifest::{MANIFEST_VERSION, Manifest};
use crate::model::Embedder;
use crate::scan;
use crate::types::{Block, IndexStats};
use crate::vectors::{VectorStore, VectorWriter};

pub const INDEX_DIR: &str = ".og";

/// A published, opened index generation ready for search.
pub struct Index {
    pub manifest: Manifest,
    pub conn: rusqlite::Connection,
    pub vectors: VectorStore,
}

impl Index {
    pub fn open(index_root: &Path) -> Result<Self> {
        let og_dir = index_root.join(INDEX_DIR);
        let current = std::fs::read_to_string(og_dir.join("CURRENT"))
            .context("no index found — run 'og build' first")?;
        let gen_name = current.trim();
        let gen_dir = og_dir.join("generations").join(gen_name);

        let manifest = Manifest::read(&gen_dir.join("manifest.json"))?;
        let conn = catalog::open_readonly(&gen_dir.join("catalog.sqlite"))
            .with_context(|| format!("opening generation {gen_name}"))?;
        let vectors = VectorStore::open(&gen_dir.join("vectors-000.bin"), manifest.dims)
            .with_context(|| format!("opening vectors for generation {gen_name}"))?;

        Ok(Self {
            manifest,
            conn,
            vectors,
        })
    }
}

/// Find the nearest enclosing index root: walk up from `start` looking for
/// `.og/CURRENT`. Returns (index_root, found).
pub fn find_index_root(start: &Path) -> (PathBuf, bool) {
    let mut dir = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if dir.join(INDEX_DIR).join("CURRENT").exists() {
            return (dir, true);
        }
        if !dir.pop() {
            return (start.to_path_buf(), false);
        }
    }
}

/// Build or incrementally update the index for `root`, publishing atomically.
///
/// Policy (tk-0ql2):
/// - No previous generation, model identity change, or schema change:
///   full build into a fresh staging dir.
/// - Otherwise: copy the previous generation's catalog + sidecar as the
///   staging base, diff per-file fingerprints (size/mtime), re-embed and
///   re-extract only changed files, delete vanished files' blocks (zeroed
///   vector rows), republish under a content+identity derived name.
pub fn build(root: &Path, embedder: &dyn Embedder, quiet: bool) -> Result<IndexStats> {
    build_with(root, embedder, quiet, Incremental::Auto)
}

/// Whether the incremental path may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incremental {
    /// Use incremental when the previous generation is compatible.
    Auto,
    /// Force a full rebuild (corruption recovery, --force).
    Force,
}

pub fn build_with(
    root: &Path,
    embedder: &dyn Embedder,
    quiet: bool,
    mode: Incremental,
) -> Result<IndexStats> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let og_dir = root.join(INDEX_DIR);
    std::fs::create_dir_all(og_dir.join("generations"))?;

    let staging = og_dir.join("generations").join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }

    // Resolve previous generation; decide full vs incremental.
    let prev = current_generation(&og_dir);
    let prev_manifest = prev.as_ref().map(|g| {
        let p = og_dir.join("generations").join(g).join("manifest.json");
        Manifest::read(&p)
    });

    let use_incremental = mode == Incremental::Auto
        && matches!(
            prev_manifest.as_ref(),
            Some(Ok(m)) if m.model_id == embedder.id()
                && m.schema_version == catalog::SCHEMA_VERSION
                && m.version == MANIFEST_VERSION
        );

    let (staging, stats, content_hash, dims) = if use_incremental {
        let prev_gen = prev.clone().expect("checked above");
        let prev_dir = og_dir.join("generations").join(&prev_gen);
        incremental_build(&root, &og_dir, &prev_dir, embedder, &staging, quiet)?
    } else {
        if !quiet && prev.is_some() {
            eprintln!("Rebuilding (model or schema changed)");
        }
        full_build(&root, &og_dir, embedder, &staging, quiet)?
    };

    let manifest = Manifest {
        version: MANIFEST_VERSION,
        root: root.to_string_lossy().into_owned(),
        model_id: embedder.id().to_string(),
        dims,
        schema_version: catalog::SCHEMA_VERSION,
        blocks: stats.blocks,
        files: stats.files,
        built_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        content_hash,
    };

    // Manifest is written inside staging BEFORE the rename: the published
    // generation is complete and immutable the moment CURRENT flips.
    manifest.write(&staging.join("manifest.json"))?;

    // Generation name derives from content + model identity + storage
    // schema: any of the three changing must publish a new directory, or a
    // schema bump would discard the fresh build (same name as the existing
    // generation) and leave the old row format behind for a new reader.
    let name_input = format!(
        "{}\x1f{}\x1f{}",
        manifest.content_hash, manifest.model_id, manifest.schema_version
    );
    let gen_hex = blake3::hash(name_input.as_bytes()).to_hex().to_string();
    let gen_name = format!("g-{}", &gen_hex[..12]);
    let final_dir = og_dir.join("generations").join(&gen_name);
    if final_dir.exists() && final_dir != staging {
        std::fs::remove_dir_all(&staging)?;
    } else if final_dir != staging {
        std::fs::rename(&staging, &final_dir)?;
    }

    // Publish: temp-write CURRENT + atomic rename.
    let current_tmp = og_dir.join("CURRENT.tmp");
    std::fs::write(&current_tmp, &gen_name)?;
    if let Ok(d) = std::fs::File::open(og_dir.join("generations")) {
        let _ = d.sync_all();
    }
    std::fs::rename(&current_tmp, og_dir.join("CURRENT"))?;

    gc_generations(&og_dir, 2)?;

    Ok(stats)
}

/// True when on-disk files differ from the catalog (metadata-only check:
/// no content reads, no model load).
pub fn needs_update(index_root: &Path) -> Result<bool> {
    let idx = Index::open(index_root)?;
    let catalog_fp = catalog::file_fingerprints(&idx.conn)?;

    let on_disk = crate::scan::scan_metadata(index_root)?;
    let root = index_root;
    let mut disk_rel: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (abs, (size, mtime)) in &on_disk {
        let Ok(rel) = abs.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().into_owned();
        match catalog_fp.get(&rel) {
            Some(&(s, m)) if s == *size as i64 && m == *mtime as i64 => {}
            _ => return Ok(true), // new or changed
        }
        disk_rel.insert(rel);
    }
    for rel in catalog_fp.keys() {
        if !disk_rel.contains(rel) {
            return Ok(true); // deleted
        }
    }
    Ok(false)
}

/// Auto-update before search (v0.0.3 behavior): if the catalog is stale,
/// run an incremental build in the index's pinned model space. Returns
/// whether a rebuild happened. Callers treat errors as non-fatal: a stale
/// index still answers queries via BM25.
pub fn update_if_stale(index_root: &Path, quiet: bool) -> Result<bool> {
    if !needs_update(index_root)? {
        return Ok(false);
    }
    let idx = Index::open(index_root)?;
    let embedder = crate::model::embedder_for(&idx.manifest.model_id)?;
    drop(idx);
    build_with(index_root, embedder.as_ref(), quiet, Incremental::Auto)?;
    Ok(true)
}

/// Current generation dir name, if any.
fn current_generation(og_dir: &Path) -> Option<String> {
    std::fs::read_to_string(og_dir.join("CURRENT"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn full_build(
    root: &Path,
    _og_dir: &Path,
    embedder: &dyn Embedder,
    staging: &Path,
    quiet: bool,
) -> Result<(PathBuf, IndexStats, String, usize)> {
    std::fs::create_dir_all(staging)?;
    if !quiet {
        eprint!("Scanning files...");
    }
    let scan::ScanResult { files, ignored } = scan::scan(root)?;
    if !quiet {
        if ignored > 0 {
            eprintln!(
                "\rScanned {} files ({} ignore file{} in effect)",
                files.len(),
                ignored,
                if ignored == 1 { "" } else { "s" }
            );
        } else {
            eprintln!("\rScanned {} files", files.len());
        }
    }

    let t0 = std::time::Instant::now();
    let file_vec: Vec<(PathBuf, String, u64)> = files
        .into_iter()
        .map(|(path, (content, mtime))| (path, content, mtime))
        .collect();
    let results = extract_all(root, file_vec);

    let conn = catalog::open(&staging.join("catalog.sqlite"))?;
    catalog::init_schema(&conn)?;

    let dims = embedder.dims();
    let mut vec_writer = VectorWriter::create(&staging.join("vectors-000.bin"), dims)?;
    embed_and_store(&conn, &mut vec_writer, results, embedder)?;
    vec_writer.finish()?;

    let stats = stats_from_conn(&conn)?;
    let content_hash = compute_content_hash(&conn)?;
    if !quiet {
        eprintln!(
            "Indexed {} blocks from {} files ({:.1}s)",
            stats.blocks,
            stats.files,
            t0.elapsed().as_secs_f64()
        );
        if stats.skipped > 0 {
            eprintln!(
                "  {} files yielded no blocks (below chunk minimum, or no constructs)",
                stats.skipped
            );
        }
    }
    Ok((staging.to_path_buf(), stats, content_hash, dims))
}

fn incremental_build(
    root: &Path,
    _og_dir: &Path,
    prev_dir: &Path,
    embedder: &dyn Embedder,
    staging: &Path,
    quiet: bool,
) -> Result<(PathBuf, IndexStats, String, usize)> {
    // Copy the previous generation as the staging base (immutable source).
    copy_dir(prev_dir, staging)?;

    let conn = catalog::open(&staging.join("catalog.sqlite"))?;
    // Reuse one connection for all file updates; WAL on the copy only.
    let prev_fingerprints = catalog::file_fingerprints(&conn)?;

    if !quiet {
        eprint!("Scanning files...");
    }
    let scan::ScanResult { files, ignored } = scan::scan(root)?;
    if !quiet {
        if ignored > 0 {
            eprintln!(
                "\rScanned {} files ({} ignore file{} in effect)",
                files.len(),
                ignored,
                if ignored == 1 { "" } else { "s" }
            );
        } else {
            eprintln!("\rScanned {} files", files.len());
        }
    }

    // Diff: changed/new files need re-extraction + re-embed; deleted files
    // are removed from the catalog.
    let mut changed: Vec<(PathBuf, String, u64)> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();

    let mut scan_fp: std::collections::HashMap<String, (i64, i64)> =
        std::collections::HashMap::new();
    for (path, (content, mtime)) in &files {
        let rel = rel_path(root, path)?;
        scan_fp.insert(rel.clone(), (content.len() as i64, *mtime as i64));
        match prev_fingerprints.get(&rel) {
            Some(&(size, m)) if size == content.len() as i64 && m == *mtime as i64 => {}
            _ => changed.push((path.clone(), content.clone(), *mtime)),
        }
    }
    for rel in prev_fingerprints.keys() {
        if !scan_fp.contains_key(rel) {
            deleted.push(rel.clone());
        }
    }

    if changed.is_empty() && deleted.is_empty() {
        if !quiet {
            eprintln!("Index up to date");
        }
    } else if !quiet {
        eprintln!(
            "Updating {} changed, {} removed",
            changed.len(),
            deleted.len()
        );
    }

    // Deletions: remove catalog rows + zero vector rows.
    let mut vec_writer =
        VectorWriter::open_existing(&staging.join("vectors-000.bin"), embedder.dims())?;
    let mut deleted_rowids: Vec<i64> = Vec::new();
    for rel in &deleted {
        deleted_rowids.extend(catalog::delete_file(&conn, rel)?);
    }
    if !deleted_rowids.is_empty() {
        vec_writer.zero_rows(&deleted_rowids)?;
    }

    // Changed files: embed_and_store handles per-file delete + re-insert
    // (zeroing old vector rows, appending new ones at fresh rowids).
    if !changed.is_empty() {
        let results = extract_all(root, changed);
        embed_and_store(&conn, &mut vec_writer, results, embedder)?;
    }

    vec_writer.finish()?;
    let stats = stats_from_conn(&conn)?;
    let content_hash = compute_content_hash(&conn)?;
    if !quiet && stats.skipped > 0 {
        eprintln!(
            "  {} files yielded no blocks (below chunk minimum, or no constructs)",
            stats.skipped
        );
    }
    Ok((staging.to_path_buf(), stats, content_hash, embedder.dims()))
}

/// Per-file extraction result: (rel_path, content_hash, size, mtime, blocks).
type Extracted = anyhow::Result<(String, String, i64, i64, Vec<Block>)>;

fn extract_all(root: &Path, files: Vec<(PathBuf, String, u64)>) -> Vec<Extracted> {
    files
        .into_par_iter()
        .map(|(path, content, mtime)| {
            let rel = rel_path(root, &path)?;
            let size = content.len() as i64;
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let mut extractor = crate::extract::Extractor::new();
            let blocks = extractor.extract(&rel, &content)?;
            Ok((rel, hash, size, mtime as i64, blocks))
        })
        .collect()
}

/// Embed extraction results and store file + blocks + vector rows.
/// Row order: rows are appended as blocks are inserted, so vector row i
/// tracks blocks.rowid i (SQLite guarantees monotonic rowids on insert).
fn embed_and_store(
    conn: &rusqlite::Connection,
    vec_writer: &mut VectorWriter,
    results: Vec<Extracted>,
    embedder: &dyn Embedder,
) -> Result<IndexStats> {
    let mut stats = IndexStats::default();
    for result in results {
        match result {
            Ok((rel, hash, size, mtime, blocks)) => {
                // Embed the whole file's blocks in one batch.
                let texts: Vec<String> = blocks.iter().map(|b| b.embedding_text()).collect();
                let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                let embeddings = embedder.embed(&refs)?;

                // Replace the file's rows atomically (delete + insert).
                let old_ids = catalog::delete_file(conn, &rel)?;
                vec_writer.zero_rows(&old_ids)?;

                // insert_file returns the rowids SQLite actually assigned
                // (it may reuse a freed max rowid); vectors are written at
                // exactly those positions.
                let rowids = catalog::insert_file(conn, &rel, size, mtime, &hash, &blocks)?;
                for (rowid, v) in rowids.iter().zip(&embeddings) {
                    vec_writer.write_at(*rowid, v)?;
                }
                if blocks.is_empty() {
                    // Parsed/visible but produced nothing (e.g. prose below
                    // the minimum chunk size, or no extractable constructs).
                    stats.skipped += 1;
                }
                stats.files += 1;
                stats.blocks += blocks.len();
            }
            Err(_e) => {
                stats.errors += 1;
            }
        }
    }
    Ok(stats)
}

fn stats_from_conn(conn: &rusqlite::Connection) -> Result<IndexStats> {
    Ok(IndexStats {
        files: catalog::count_files(conn)?,
        blocks: catalog::count_blocks(conn)?,
        skipped: catalog::count_empty_files(conn)?,
        ..Default::default()
    })
}

fn rel_path(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .with_context(|| format!("stripping prefix from {}", path.display()))?
        .to_string_lossy()
        .into_owned())
}

/// Copy a directory tree (previous generation -> staging).
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    copy_dir_inner(src, dst)
}

fn copy_dir_inner(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir_inner(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn compute_content_hash(conn: &rusqlite::Connection) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut stmt = conn.prepare("SELECT path, size, mtime, hash FROM files ORDER BY path")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let f: String = row.get(0)?;
        let s: i64 = row.get(1)?;
        let m: i64 = row.get(2)?;
        let h: String = row.get(3)?;
        hasher.update(format!("{f}\x1f{s}\x1f{m}\x1f{h}\n").as_bytes());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn gc_generations(og_dir: &Path, keep: usize) -> Result<()> {
    let current = std::fs::read_to_string(og_dir.join("CURRENT"))?;
    let current = current.trim();
    let mut gens: Vec<PathBuf> = std::fs::read_dir(og_dir.join("generations"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    // Keep CURRENT first, then most recent by modified time.
    gens.retain(|p| p.file_name().map(|n| n != current).unwrap_or(true));
    gens.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    while gens.len() > keep.saturating_sub(1) {
        let victim = gens.pop();
        match victim {
            Some(v) => {
                let _ = std::fs::remove_dir_all(v);
            }
            None => break,
        }
    }
    Ok(())
}

/// Hash helper used by the CLI status command.
pub fn generation_name(index: &Index) -> &str {
    // Manifest doesn't carry the dir name; derive from CURRENT for display.
    Box::leak(Box::new(index.manifest.content_hash[..12].to_string()))
}
