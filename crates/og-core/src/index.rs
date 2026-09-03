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
use crate::manifest::{Manifest, MANIFEST_VERSION};
use crate::model::{DeterministicEmbedder, Embedder};
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

    /// True when the model identity matches the default build embedder.
    pub fn matches_default_model(&self) -> bool {
        self.manifest.model_id == DeterministicEmbedder::ID
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

/// Build a fresh generation for `root` and publish it atomically.
///
/// Always full-build in the slice (incremental lands in tk-0ql2): scan →
/// extract (rayon) → embed → write catalog + vectors into a staging dir →
/// fsync → write manifest → rename staging to final → swap CURRENT.
pub fn build(root: &Path, embedder: &dyn Embedder, quiet: bool) -> Result<IndexStats> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let og_dir = root.join(INDEX_DIR);
    std::fs::create_dir_all(og_dir.join("generations"))?;

    // Stage into a temp dir inside generations/ so the final rename is atomic.
    let staging = og_dir.join("generations").join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    if !quiet {
        eprint!("Scanning files...");
    }
    let files = scan::scan(&root)?;
    if !quiet {
        eprintln!("\rScanned {} files", files.len());
    }

    // Extract blocks in parallel.
    type ExtractedFile = anyhow::Result<(String, String, i64, i64, Vec<Block>)>;
    let file_list: Vec<(PathBuf, String, u64)> = files
        .into_iter()
        .map(|(path, (content, mtime))| (path, content, mtime))
        .collect();

    if !quiet {
        eprint!("Extracting blocks...");
    }
    let t0 = std::time::Instant::now();

    let results: Vec<ExtractedFile> = file_list
        .into_par_iter()
        .map(|(path, content, mtime)| {
            let rel = path
                .strip_prefix(&root)
                .with_context(|| format!("stripping prefix from {}", path.display()))?
                .to_string_lossy()
                .into_owned();
            let size = content.len() as i64;
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let mut extractor = crate::extract::Extractor::new();
            let blocks = extractor.extract(&rel, &content)?;
            Ok((rel, content_hash_string(&hash), size, mtime as i64, blocks))
        })
        .collect();

    let mut stats = IndexStats::default();
    let conn = catalog::open(&staging.join("catalog.sqlite"))?;
    catalog::init_schema(&conn)?;

    let dims = embedder.dims();
    let mut vec_writer = VectorWriter::create(&staging.join("vectors-000.bin"), dims)?;

    for result in results {
        match result {
            Ok((rel, hash, size, mtime, blocks)) => {
                stats.files += 1;
                stats.blocks += blocks.len();
                catalog::insert_file(&conn, &rel, size, mtime, &hash, &blocks)?;
                for block in &blocks {
                    let text = block.embedding_text();
                    let embedded = embedder.embed(&[&text])?;
                    vec_writer.write_vec(&embedded[0])?;
                }
            }
            Err(e) => {
                stats.errors += 1;
                if !quiet {
                    eprintln!("  error: {e:#}");
                }
            }
        }
    }

    vec_writer.finish()?;

    if !quiet {
        eprintln!(
            "\rExtracted {} blocks from {} files ({:.1}s)",
            stats.blocks,
            stats.files,
            t0.elapsed().as_secs_f64()
        );
    }

    // Pin identity + stats, write manifest, publish.
    let content_hash = compute_content_hash(&conn)?;
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

    // Final generation dir name derives from content + model identity.
    let gen_name = format!("g-{}", &manifest.content_hash[..12]);
    let final_dir = og_dir.join("generations").join(&gen_name);
    if final_dir.exists() {
        // Same content republished: drop the old staging copy and reuse.
        std::fs::remove_dir_all(&staging)?;
    } else {
        std::fs::rename(&staging, &final_dir)?;
    }
    manifest.write(&final_dir.join("manifest.json"))?;

    // Publish: temp-write CURRENT + atomic rename.
    let current_tmp = og_dir.join("CURRENT.tmp");
    std::fs::write(&current_tmp, &gen_name)?;
    if let Some(dir) = og_dir.parent() {
        // fsync the generations dir for durability of the rename.
        if let Ok(d) = std::fs::File::open(og_dir.join("generations")) {
            let _ = d.sync_all();
        }
        let _ = dir;
    }
    std::fs::rename(&current_tmp, og_dir.join("CURRENT"))?;

    // GC old generations (keep current + one previous for in-flight readers).
    gc_generations(&og_dir, 2)?;

    Ok(stats)
}

fn content_hash_string(hex: &str) -> String {
    hex.to_string()
}

fn compute_content_hash(conn: &rusqlite::Connection) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut stmt = conn.prepare(
        "SELECT path, size, mtime, hash FROM files ORDER BY path",
    )?;
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
