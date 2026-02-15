pub mod manifest;
pub mod walker;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rayon::prelude::*;

use crate::embedder::{self, Embedder, TOKEN_DIM};
use crate::extractor::Extractor;
use crate::tokenize::split_identifiers;
use crate::types::{Block, IndexStats, SearchResult};

use manifest::{FileEntry, Manifest};

pub const INDEX_DIR: &str = ".og";
pub const VECTORS_DIR: &str = "vectors";

/// Block types that are documentation, not code.
const DOC_BLOCK_TYPES: &[&str] = &["text", "section"];

/// Manages semantic search index using omendb.
pub struct SemanticIndex {
    root: PathBuf,
    index_dir: PathBuf,
    vectors_path: String,
    search_scope: Option<String>,
    embedder: Box<dyn Embedder>,
}

impl SemanticIndex {
    pub fn new(root: &Path, search_scope: Option<&Path>) -> Result<Self> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let index_dir = root.join(INDEX_DIR);
        let vectors_path = index_dir.join(VECTORS_DIR).to_string_lossy().into_owned();

        let scope = search_scope.and_then(|s| {
            let s = s.canonicalize().unwrap_or_else(|_| s.to_path_buf());
            if s != root {
                s.strip_prefix(&root)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            } else {
                None
            }
        });

        let embedder = embedder::create_embedder()?;

        Ok(Self {
            root,
            index_dir,
            vectors_path,
            search_scope: scope,
            embedder,
        })
    }

    /// Build index from scanned files.
    pub fn index(
        &self,
        files: &HashMap<PathBuf, String>,
        batch_size: usize,
        on_progress: Option<&dyn Fn(usize, usize, &str)>,
    ) -> Result<IndexStats> {
        std::fs::create_dir_all(&self.index_dir)?;
        let mut manifest = Manifest::load(&self.index_dir)?;
        let mut stats = IndexStats::default();

        // Open omendb multi-vector store
        let mut store = self.open_or_create_store()?;
        store.enable_text_search()?;

        // Identify files needing processing (borrow content, don't clone)
        let mut to_process: Vec<(&Path, &str, String, String)> = Vec::new();
        for (path, content) in files {
            let rel_path = self.to_relative(path);
            let file_hash = hash_content(content);

            if let Some(entry) = manifest.files.get(&rel_path) {
                if entry.hash == file_hash {
                    stats.skipped += 1;
                    continue;
                }
                // Delete old blocks
                for block_id in &entry.blocks {
                    let _ = store.delete(block_id);
                }
                stats.deleted += entry.blocks.len();
            }

            to_process.push((path.as_path(), content.as_str(), rel_path, file_hash));
        }

        if to_process.is_empty() {
            if stats.deleted > 0 {
                store.flush()?;
            }
            return Ok(stats);
        }

        store.flush()?;

        // Extract blocks in parallel
        let all_blocks: Vec<(Vec<Block>, String, String)> = to_process
            .par_iter()
            .map(|(_path, content, rel_path, file_hash)| {
                let mut extractor = Extractor::new();
                let blocks = extractor.extract(rel_path, content).unwrap_or_default();
                (blocks, rel_path.clone(), file_hash.clone())
            })
            .collect();

        // Flatten blocks, compute embedding text once, track file stats
        struct PreparedBlock {
            block: Block,
            text: String,
        }

        let mut prepared: Vec<PreparedBlock> = Vec::new();
        for (blocks, _rel_path, _file_hash) in &all_blocks {
            if blocks.is_empty() {
                stats.errors += 1;
            } else {
                stats.files += 1;
            }
            for block in blocks {
                let text = block.embedding_text();
                prepared.push(PreparedBlock {
                    block: block.clone(),
                    text,
                });
            }
        }

        if prepared.is_empty() {
            manifest.save(&self.index_dir)?;
            return Ok(stats);
        }

        // Sort by text length for better batching (avoids recomputing embedding_text)
        prepared.sort_by_key(|p| p.text.len());

        let total = prepared.len();

        // Embed in batches
        for start in (0..total).step_by(batch_size) {
            let end = (start + batch_size).min(total);
            if let Some(progress) = on_progress {
                progress(
                    start,
                    total,
                    &format!("Embedding {}-{} of {total}", start, end),
                );
            }

            let batch_refs: Vec<&str> = prepared[start..end]
                .iter()
                .map(|p| p.text.as_str())
                .collect();
            let token_embeddings = self.embedder.embed_documents(&batch_refs)?;

            for (idx, token_emb) in token_embeddings.embeddings.iter().enumerate() {
                let block = &prepared[start + idx].block;

                // Convert ndarray to Vec<Vec<f32>> for omendb
                let tokens: Vec<Vec<f32>> =
                    token_emb.rows().into_iter().map(|r| r.to_vec()).collect();

                let metadata = serde_json::json!({
                    "file": block.file,
                    "type": block.block_type,
                    "name": block.name,
                    "start_line": block.start_line,
                    "end_line": block.end_line,
                    "content": block.content,
                });

                store.store(&block.id, tokens, metadata)?;

                // Index text for BM25 hybrid search (with split identifiers)
                let bm25_text = split_identifiers(&prepared[start + idx].text);
                store.index_text(&block.id, &bm25_text)?;

                stats.blocks += 1;
            }
        }

        store.flush()?;

        // Update manifest
        for (blocks, rel_path, file_hash) in &all_blocks {
            if !blocks.is_empty() {
                manifest.files.insert(
                    rel_path.clone(),
                    FileEntry {
                        hash: file_hash.clone(),
                        blocks: blocks.iter().map(|b| b.id.clone()).collect(),
                    },
                );
            }
        }

        manifest.save(&self.index_dir)?;

        if let Some(progress) = on_progress {
            progress(total, total, "Done");
        }

        Ok(stats)
    }

    /// Hybrid search: semantic + BM25.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>> {
        let store = self.open_store()?;

        let query_tokens = self.embedder.embed_query(query)?;
        let tokens: Vec<Vec<f32>> = (0..query_tokens.nrows())
            .map(|r| query_tokens.row(r).to_vec())
            .collect();
        let token_refs: Vec<&[f32]> = tokens.iter().map(|v| v.as_slice()).collect();

        let search_k = k * 3; // Over-fetch for scope filtering

        // Split identifiers in query for BM25 matching
        let bm25_query = split_identifiers(query);
        let results = store.search_multi_with_text(&bm25_query, &token_refs, search_k, None)?;

        let mut output = Vec::new();
        for r in results {
            let file = r
                .metadata
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(scope) = &self.search_scope {
                if !file.starts_with(scope.as_str()) {
                    continue;
                }
            }

            let abs_file = self.to_absolute(file);

            output.push(SearchResult {
                file: abs_file,
                block_type: r
                    .metadata
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: r
                    .metadata
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                line: r
                    .metadata
                    .get("start_line")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                end_line: r
                    .metadata
                    .get("end_line")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                content: r
                    .metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                score: r.distance,
            });
        }

        output.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        output.truncate(k);
        Ok(output)
    }

    /// Find blocks similar to a given file/block.
    pub fn find_similar(
        &self,
        file_path: &str,
        line: Option<usize>,
        name: Option<&str>,
        k: usize,
    ) -> Result<Vec<SearchResult>> {
        let manifest = Manifest::load(&self.index_dir)?;
        let store = self.open_store()?;

        let rel_path = self.to_relative(&PathBuf::from(file_path));
        let entry = manifest
            .files
            .get(&rel_path)
            .with_context(|| format!("File not in index: {rel_path}"))?;

        if entry.blocks.is_empty() {
            bail!("No blocks found in {rel_path}");
        }

        // Find target block
        let block_id = if let Some(name) = name {
            find_block_by_name(&store, &entry.blocks, name)?
        } else if let Some(line) = line {
            find_block_by_line(&store, &entry.blocks, line)
                .unwrap_or_else(|| entry.blocks[0].clone())
        } else {
            entry.blocks[0].clone()
        };

        // Get the block's FDE vector and search for similar
        let (query_vec, _meta) = store
            .get(&block_id)
            .with_context(|| "Could not retrieve block embedding")?;

        let results = store.search(&query_vec, k * 3 + entry.blocks.len(), None)?;

        let block_set: std::collections::HashSet<&str> =
            entry.blocks.iter().map(|s| s.as_str()).collect();

        let mut output = Vec::new();
        for r in results {
            if block_set.contains(r.id.as_str()) {
                continue;
            }

            let block_type = r
                .metadata
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if DOC_BLOCK_TYPES.contains(&block_type) {
                continue;
            }

            let file = r
                .metadata
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(scope) = &self.search_scope {
                if !file.starts_with(scope.as_str()) {
                    continue;
                }
            }

            let score = (2.0 - r.distance) / 2.0;

            output.push(SearchResult {
                file: self.to_absolute(file),
                block_type: block_type.to_string(),
                name: r
                    .metadata
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                line: r
                    .metadata
                    .get("start_line")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                end_line: r
                    .metadata
                    .get("end_line")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                content: r
                    .metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                score,
            });

            if output.len() >= k {
                break;
            }
        }

        Ok(output)
    }

    /// Check if index exists.
    pub fn is_indexed(&self) -> bool {
        self.index_dir.join("manifest.json").exists()
    }

    /// Count indexed blocks.
    pub fn count(&self) -> Result<usize> {
        let manifest = Manifest::load(&self.index_dir)?;
        Ok(manifest.files.values().map(|e| e.blocks.len()).sum())
    }

    /// Get stale files (changed + deleted).
    pub fn get_stale_files(
        &self,
        files: &HashMap<PathBuf, String>,
    ) -> Result<(Vec<PathBuf>, Vec<String>)> {
        let manifest = Manifest::load(&self.index_dir)?;

        let mut changed = Vec::new();
        let mut current_rel_files = std::collections::HashSet::new();

        for (path, content) in files {
            let rel_path = self.to_relative(path);
            current_rel_files.insert(rel_path.clone());
            let file_hash = hash_content(content);

            match manifest.files.get(&rel_path) {
                Some(entry) if entry.hash == file_hash => {}
                _ => changed.push(path.clone()),
            }
        }

        let deleted: Vec<String> = manifest
            .files
            .keys()
            .filter(|k| !current_rel_files.contains(*k))
            .cloned()
            .collect();

        Ok((changed, deleted))
    }

    /// Quick check: how many files need updating?
    pub fn needs_update(&self, files: &HashMap<PathBuf, String>) -> Result<usize> {
        let (changed, deleted) = self.get_stale_files(files)?;
        Ok(changed.len() + deleted.len())
    }

    /// Incremental update.
    pub fn update(
        &self,
        files: &HashMap<PathBuf, String>,
        batch_size: usize,
    ) -> Result<IndexStats> {
        let (changed, deleted) = self.get_stale_files(files)?;

        if changed.is_empty() && deleted.is_empty() {
            return Ok(IndexStats {
                skipped: files.len(),
                ..Default::default()
            });
        }

        // Delete vectors for deleted files in a scoped block
        // so the store lock is released before self.index() re-acquires it
        let mut deleted_count = 0;
        {
            let mut store = self.open_store()?;
            let mut manifest = Manifest::load(&self.index_dir)?;

            for rel_path in &deleted {
                if let Some(entry) = manifest.files.remove(rel_path) {
                    for block_id in &entry.blocks {
                        let _ = store.delete(block_id);
                    }
                    deleted_count += entry.blocks.len();
                }
            }

            if deleted_count > 0 {
                store.flush()?;
                manifest.save(&self.index_dir)?;
            }
        }

        // Re-index changed files (opens store internally)
        let changed_files: HashMap<PathBuf, String> = changed
            .into_iter()
            .filter_map(|p| files.get(&p).map(|c| (p, c.clone())))
            .collect();

        let mut stats = self.index(&changed_files, batch_size, None)?;
        stats.deleted += deleted_count;
        Ok(stats)
    }

    /// Delete the entire index.
    pub fn clear(&self) -> Result<()> {
        if self.index_dir.exists() {
            std::fs::remove_dir_all(&self.index_dir)?;
        }
        Ok(())
    }

    /// Remove all blocks matching a path prefix.
    pub fn remove_prefix(&self, prefix: &str) -> Result<IndexStats> {
        let prefix = prefix.trim_end_matches('/');
        if prefix.is_empty() || prefix == "." {
            return Ok(IndexStats::default());
        }

        let mut store = self.open_store()?;

        let mut manifest = Manifest::load(&self.index_dir)?;
        let mut stats = IndexStats::default();

        let to_remove: Vec<String> = manifest
            .files
            .keys()
            .filter(|k| *k == prefix || k.starts_with(&format!("{prefix}/")))
            .cloned()
            .collect();

        for rel_path in &to_remove {
            if let Some(entry) = manifest.files.remove(rel_path) {
                for block_id in &entry.blocks {
                    let _ = store.delete(block_id);
                }
                stats.blocks += entry.blocks.len();
                stats.files += 1;
            }
        }

        store.flush()?;
        manifest.save(&self.index_dir)?;

        Ok(stats)
    }

    fn to_relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn to_absolute(&self, rel_path: &str) -> String {
        if Path::new(rel_path).is_absolute() {
            rel_path.to_string()
        } else {
            self.root.join(rel_path).to_string_lossy().into_owned()
        }
    }

    /// Open existing multi-vector store (for search/read operations).
    fn open_store(&self) -> Result<omendb::VectorStore> {
        omendb::VectorStore::open(&self.vectors_path).context("Failed to open vector store")
    }

    /// Open existing store or create a new multi-vector store (for indexing).
    fn open_or_create_store(&self) -> Result<omendb::VectorStore> {
        let vectors_path = Path::new(&self.vectors_path);
        // omendb appends ".omen" to the path for the storage file
        let mut omen_path = vectors_path.as_os_str().to_os_string();
        omen_path.push(".omen");

        if vectors_path.exists() || Path::new(&omen_path).exists() {
            omendb::VectorStore::open(&self.vectors_path).context("Failed to open vector store")
        } else {
            omendb::VectorStore::multi_vector(TOKEN_DIM)
                .persist(&self.vectors_path)
                .context("Failed to create vector store")
        }
    }
}

/// Walk up directory tree to find existing index.
pub fn find_index_root(search_path: &Path) -> (PathBuf, Option<PathBuf>) {
    let search_path = search_path
        .canonicalize()
        .unwrap_or_else(|_| search_path.to_path_buf());

    let mut current = search_path.clone();
    loop {
        let index_dir = current.join(INDEX_DIR);
        if index_dir.join("manifest.json").exists() {
            return (current, Some(index_dir));
        }
        if !current.pop() {
            break;
        }
    }

    (search_path, None)
}

/// Find parent directory with existing index (not at path itself).
pub fn find_parent_index(path: &Path) -> Option<PathBuf> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut current = path.clone();

    if !current.pop() {
        return None;
    }

    loop {
        let index_dir = current.join(INDEX_DIR);
        if index_dir.join("manifest.json").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }

    None
}

/// Find all .og/ directories under path.
pub fn find_subdir_indexes(path: &Path, include_root: bool) -> Vec<PathBuf> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut indexes = Vec::new();

    for entry in walkdir::WalkDir::new(&path).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !name.starts_with('.') || name == INDEX_DIR
    }) {
        let Ok(entry) = entry else { continue };
        if entry.file_name() == INDEX_DIR && entry.file_type().is_dir() {
            let idx_path = entry.path().to_path_buf();
            if idx_path.join("manifest.json").exists()
                && (include_root || idx_path.parent() != Some(&path))
            {
                indexes.push(idx_path);
            }
        }
    }

    indexes
}

fn find_block_by_name(
    store: &omendb::VectorStore,
    block_ids: &[String],
    name: &str,
) -> Result<String> {
    let mut matches = Vec::new();

    for block_id in block_ids {
        if let Some((_vec, meta)) = store.get(block_id) {
            let block_name = meta.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if block_name == name || block_name.ends_with(&format!(".{name}")) {
                matches.push((
                    block_id.clone(),
                    block_name.to_string(),
                    meta.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                    meta.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ));
            }
        }
    }

    match matches.len() {
        0 => bail!("No block named '{name}' found"),
        1 => Ok(matches[0].0.clone()),
        _ => {
            let details: Vec<String> = matches
                .iter()
                .map(|(_, n, line, t)| format!("  - line {line}: {t} {n}"))
                .collect();
            bail!(
                "Multiple blocks named '{name}' found:\n{}\nUse file:<line> to specify.",
                details.join("\n")
            )
        }
    }
}

fn find_block_by_line(
    store: &omendb::VectorStore,
    block_ids: &[String],
    line: usize,
) -> Option<String> {
    for block_id in block_ids {
        if let Some((_vec, meta)) = store.get(block_id) {
            let start = meta.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let end = meta.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if start <= line && line <= end {
                return Some(block_id.clone());
            }
        }
    }
    None
}

fn hash_content(content: &str) -> String {
    let hash = blake3::hash(content.as_bytes());
    hash.to_hex()[..16].to_string()
}
