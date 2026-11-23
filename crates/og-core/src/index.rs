//! Index lifecycle: build a generation, publish atomically via CURRENT,
//! open the published generation, locate index roots by walking up.
//!
//! Layout:
//! ```text
//! .og/
//! ├── LOCK                     # stable writer lock, retained by clean
//! ├── CURRENT                  # pointer file: generation dir name
//! └── generations/
//!     └── g-<hash>/
//!         ├── catalog.sqlite  # files, blocks, block_fts, block_trigram
//!         ├── vectors-000.bin  # fp16 rows in blocks.id order
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
        let pointer = og_dir.join("CURRENT");
        let open_current = |current: &str| {
            let name = current.trim();
            anyhow::ensure!(
                valid_generation_name(name),
                "invalid CURRENT generation name"
            );
            Self::open_generation(&og_dir.join("generations").join(name))
                .with_context(|| format!("opening generation {name}"))
        };
        let current =
            std::fs::read_to_string(&pointer).context("no index found — run 'og build' first")?;
        // GC can remove a generation between reading CURRENT and opening
        // both components. Retry if a writer moved the pointer meanwhile.
        let result = open_current(&current);
        if result.is_err()
            && let Ok(next) = std::fs::read_to_string(&pointer)
            && next != current
        {
            return open_current(&next);
        }
        result
    }

    fn open_generation(dir: &Path) -> Result<Self> {
        let manifest = Manifest::read(&dir.join("manifest.json"))?;
        anyhow::ensure!(
            manifest.schema_version == catalog::SCHEMA_VERSION,
            "index schema {} unsupported — run 'og build' to rebuild",
            manifest.schema_version
        );
        let conn = catalog::open_readonly(&dir.join("catalog.sqlite"))?;
        let vectors = VectorStore::open(&dir.join("vectors-000.bin"), manifest.dims)?;
        let last_id: i64 =
            conn.query_row("SELECT coalesce(max(id), 0) FROM blocks", [], |r| r.get(0))?;
        anyhow::ensure!(
            vectors.len() as i64 >= last_id,
            "vector store is missing rows"
        );
        Ok(Self {
            manifest,
            conn,
            vectors,
        })
    }
}

fn valid_generation_name(name: &str) -> bool {
    name.starts_with("g-") && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn lock_writer(og_dir: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(og_dir)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(og_dir.join("LOCK"))?;
    file.lock().context("locking index for writing")?;
    Ok(file)
}

/// Remove index data while serializing with builds. Keep the lock inode so
/// concurrent writers can never lock different files for the same index.
pub fn clean(root: &Path) -> Result<()> {
    let og_dir = root.join(INDEX_DIR);
    let _lock = lock_writer(&og_dir)?;
    // Unpublish first so new readers don't try to open the files we remove.
    match std::fs::remove_file(og_dir.join("CURRENT")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    for entry in std::fs::read_dir(&og_dir)? {
        let entry = entry?;
        if entry.file_name() == "LOCK" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
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
    // Refuse to build against a nonexistent root: canonicalize() would
    // silently fall back to the raw path and create_dir_all would mint
    // the directory (plus an index inside it) from a typo.
    let root = root
        .canonicalize()
        .with_context(|| format!("index root {} does not exist", root.display()))?;
    anyhow::ensure!(root.is_dir(), "index root must be a directory");
    let _lock = lock_writer(&root.join(INDEX_DIR))?;
    build_locked(&root, embedder, quiet, mode)
}

fn build_locked(
    root: &Path,
    embedder: &dyn Embedder,
    quiet: bool,
    mode: Incremental,
) -> Result<IndexStats> {
    let og_dir = root.join(INDEX_DIR);
    let generations = og_dir.join("generations");
    std::fs::create_dir_all(&generations)?;

    let staging = generations.join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }

    let previous = current_generation(&og_dir);
    let base = previous.as_ref().and_then(|name| {
        let dir = generations.join(name);
        let idx = Index::open_generation(&dir).ok()?;
        (mode == Incremental::Auto
            && idx.manifest.model_id == embedder.id()
            && idx.manifest.dims == embedder.dims())
        .then_some((dir, idx))
    });
    if !quiet && previous.is_some() && base.is_none() {
        if mode == Incremental::Force {
            eprintln!("Rebuilding (forced)");
        } else {
            eprintln!("Rebuilding (previous generation unreadable or incompatible)");
        }
    }

    let scan::ScanResult { files, ignored } = scan::scan(root)?;
    if !quiet {
        eprintln!(
            "Scanned {} files ({ignored} ignore files in effect)",
            files.len()
        );
    }

    let mut deleted = match &base {
        Some((_, idx)) => catalog::file_fingerprints(&idx.conn)?,
        None => Default::default(),
    };
    let mut changed = Vec::new();
    for (path, (source, mtime)) in files {
        let path = rel_path(root, &path)?;
        if deleted.remove(&path) != Some((source.len() as i64, mtime as i64)) {
            changed.push(SourceFile {
                path,
                source,
                mtime: mtime as i64,
            });
        }
    }
    if let Some((_, idx)) = &base
        && changed.is_empty()
        && deleted.is_empty()
    {
        if !quiet {
            eprintln!("Index up to date");
        }
        return stats_from_conn(&idx.conn);
    }

    let dims = embedder.dims();
    let t0 = std::time::Instant::now();
    std::fs::create_dir(&staging)?;
    let catalog_path = staging.join("catalog.sqlite");
    let vectors_path = staging.join("vectors-000.bin");
    let (conn, mut vectors) = if let Some((dir, _)) = base {
        if !quiet {
            eprintln!(
                "Updating {} changed, {} removed",
                changed.len(),
                deleted.len()
            );
        }
        std::fs::copy(dir.join("catalog.sqlite"), &catalog_path)?;
        std::fs::copy(dir.join("vectors-000.bin"), &vectors_path)?;
        (
            catalog::open(&catalog_path)?,
            VectorWriter::open_existing(&vectors_path, dims)?,
        )
    } else {
        let conn = catalog::open(&catalog_path)?;
        catalog::init_schema(&conn)?;
        (conn, VectorWriter::create(&vectors_path, dims)?)
    };

    let mut deleted: Vec<_> = deleted.into_keys().collect();
    deleted.sort_unstable();
    for path in deleted {
        vectors.zero_rows(&catalog::delete_file(&conn, &path)?)?;
    }
    store_files(&conn, &mut vectors, changed, embedder)?;
    vectors.finish()?;

    let stats = stats_from_conn(&conn)?;
    let content_hash = compute_content_hash(&conn)?;
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    drop(conn);
    if !quiet {
        eprintln!(
            "Indexed {} blocks from {} files ({:.1}s)",
            stats.blocks,
            stats.files,
            t0.elapsed().as_secs_f64()
        );
        if stats.skipped > 0 {
            eprintln!("  {} files yielded no blocks", stats.skipped);
        }
    }

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

    // Reuse identical readable snapshots. Repair under a different
    // directory name rather than removing a directory CURRENT may name.
    let name_input = format!(
        "{}\x1f{}\x1f{}\x1f{}",
        manifest.content_hash, manifest.model_id, manifest.schema_version, manifest.dims
    );
    let gen_hex = blake3::hash(name_input.as_bytes()).to_hex().to_string();
    let base_name = format!("g-{}", &gen_hex[..12]);
    let mut gen_name = base_name.clone();
    for suffix in 1.. {
        let final_dir = generations.join(&gen_name);
        if !final_dir.exists() {
            std::fs::rename(&staging, &final_dir)?;
            break;
        }
        if let Ok(idx) = Index::open_generation(&final_dir)
            && idx.manifest.content_hash == manifest.content_hash
            && idx.manifest.model_id == manifest.model_id
            && idx.manifest.dims == manifest.dims
        {
            std::fs::remove_dir_all(&staging)?;
            break;
        }
        if !quiet {
            eprintln!("Replacing corrupt generation or hash collision: {gen_name}");
        }
        gen_name = format!("{base_name}-{suffix}");
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

/// Check file metadata for changes without loading the model. Only
/// uncatalogued paths need a content read to distinguish text from binaries.
pub fn needs_update(index_root: &Path) -> Result<bool> {
    let idx = Index::open(index_root)?;
    needs_update_at(index_root, &idx)
}

fn needs_update_at(index_root: &Path, idx: &Index) -> Result<bool> {
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
            // The metadata walk cannot identify extensionless binaries
            // and invalid UTF-8. Confirm uncatalogued files are indexable.
            None if scan::read_text(abs, *mtime).is_none() => continue,
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
/// whether a rebuild happened. Propagate errors rather than silently
/// answering from an index we could not refresh.
pub fn update_if_stale(index_root: &Path, quiet: bool) -> Result<bool> {
    if !needs_update(index_root)? {
        return Ok(false);
    }
    let _lock = lock_writer(&index_root.join(INDEX_DIR))?;
    // Recheck under the writer lock, including the model identity: a build
    // may have made the index current or selected another model.
    let idx = Index::open(index_root)?;
    if !needs_update_at(index_root, &idx)? {
        return Ok(false);
    }
    let embedder = crate::model::embedder_for(&idx.manifest.model_id)?;
    drop(idx);
    build_locked(index_root, embedder.as_ref(), quiet, Incremental::Auto)?;
    Ok(true)
}

/// Current generation dir name, if any.
fn current_generation(og_dir: &Path) -> Option<String> {
    std::fs::read_to_string(og_dir.join("CURRENT"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| valid_generation_name(s))
}

struct SourceFile {
    path: String,
    source: String,
    mtime: i64,
}

fn store_files(
    conn: &rusqlite::Connection,
    vectors: &mut VectorWriter,
    mut files: Vec<SourceFile>,
    embedder: &dyn Embedder,
) -> Result<()> {
    // Stable insertion order makes score ties reproducible across fresh builds.
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    let parsed: Vec<_> = files
        .into_par_iter()
        .map_init(crate::extract::Extractor::new, |extractor, file| {
            let blocks = extractor
                .extract(&file.path, &file.source)
                .with_context(|| format!("extracting {}", file.path))?;
            let size = file.source.len() as i64;
            let hash = blake3::hash(file.source.as_bytes()).to_hex().to_string();
            Ok::<_, anyhow::Error>((file.path, size, file.mtime, hash, blocks))
        })
        .collect();
    for result in parsed {
        let (path, size, mtime, hash, blocks) = result?;
        let texts: Vec<_> = blocks.iter().map(Block::embedding_text).collect();
        let refs: Vec<_> = texts.iter().map(String::as_str).collect();
        let embeddings = embedder.embed(&refs)?;
        anyhow::ensure!(
            blocks.len() == embeddings.len(),
            "wrong embedding count for {path}"
        );
        vectors.zero_rows(&catalog::delete_file(conn, &path)?)?;
        let rowids = catalog::insert_file(conn, &path, size, mtime, &hash, &blocks)?;
        for (id, vector) in rowids.into_iter().zip(embeddings) {
            vectors.write_at(id, &vector)?;
        }
    }
    Ok(())
}

fn stats_from_conn(conn: &rusqlite::Connection) -> Result<IndexStats> {
    Ok(IndexStats {
        files: catalog::count_files(conn)?,
        blocks: catalog::count_blocks(conn)?,
        skipped: catalog::count_empty_files(conn)?,
    })
}

fn rel_path(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .with_context(|| format!("stripping prefix from {}", path.display()))?
        .to_string_lossy()
        .into_owned())
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
    gens.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
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
