pub mod manifest;
pub mod walker;

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rayon::prelude::*;

use crate::embedder::{self, Embedder};
use crate::extractor::Extractor;
use crate::tokenize::split_identifiers;
use crate::types::{Block, IndexStats, SearchResult};
use omendb::SearchOptions;

use manifest::{FileEntry, Manifest};

pub const INDEX_DIR: &str = ".og";
pub const VECTORS_DIR: &str = "vectors";

/// Block types that are documentation, not code.
const DOC_BLOCK_TYPES: &[&str] = &["text", "section"];

/// When search scope filters results, over-fetch by this factor to compensate.
const SCOPE_OVERFETCH: usize = 5;

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
        let scope = Self::compute_scope(&root, search_scope);
        let embedder = embedder::create_embedder()?;

        Ok(Self {
            root,
            index_dir,
            vectors_path,
            search_scope: scope,
            embedder,
        })
    }

    /// Set search scope after construction (for reusing a single instance).
    pub fn set_search_scope(&mut self, search_scope: Option<&Path>) {
        self.search_scope = Self::compute_scope(&self.root, search_scope);
    }

    fn compute_scope(root: &Path, search_scope: Option<&Path>) -> Option<String> {
        search_scope.and_then(|s| {
            let s = s.canonicalize().unwrap_or_else(|_| s.to_path_buf());
            if s != *root {
                s.strip_prefix(root)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            } else {
                None
            }
        })
    }

    /// Build index from scanned files.
    pub fn index(
        &self,
        files: &HashMap<PathBuf, String>,
        on_progress: Option<&dyn Fn(usize, usize, &str)>,
    ) -> Result<IndexStats> {
        std::fs::create_dir_all(&self.index_dir)?;
        let mut manifest = Manifest::load(&self.index_dir)?;
        manifest.model = embedder::MODEL.version.to_string();
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

        // Extract blocks in parallel, reusing Extractor per thread
        let all_blocks: Vec<(Vec<Block>, String, String)> = to_process
            .par_iter()
            .map_init(
                Extractor::new,
                |extractor, (_path, content, rel_path, file_hash)| {
                    let blocks = extractor.extract(rel_path, content).unwrap_or_default();
                    (blocks, rel_path.clone(), file_hash.clone())
                },
            )
            .collect();

        // Flatten blocks, compute embedding text once, track file stats.
        // Store (file_idx, block_idx) to reference blocks without cloning.
        struct PreparedBlock {
            file_idx: usize,
            block_idx: usize,
            text: String,
        }

        let mut prepared: Vec<PreparedBlock> = Vec::new();
        for (file_idx, (blocks, _rel_path, _file_hash)) in all_blocks.iter().enumerate() {
            if blocks.is_empty() {
                stats.errors += 1;
            } else {
                stats.files += 1;
            }
            for (block_idx, block) in blocks.iter().enumerate() {
                let text = block.embedding_text();
                prepared.push(PreparedBlock {
                    file_idx,
                    block_idx,
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
        let batch_size = embedder::MODEL.batch_size;

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
                let p = &prepared[start + idx];
                let block = &all_blocks[p.file_idx].0[p.block_idx];

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

                let bm25_text = split_identifiers(&p.text);
                store.store_with_text(&block.id, tokens, &bm25_text, metadata)?;

                stats.blocks += 1;
            }
        }

        store.flush()?;

        // Update manifest
        for (i, (blocks, rel_path, file_hash)) in all_blocks.iter().enumerate() {
            if !blocks.is_empty() {
                let mtime = to_process
                    .get(i)
                    .map(|(path, _, _, _)| walker::file_mtime(path))
                    .unwrap_or(0);
                manifest.files.insert(
                    rel_path.clone(),
                    FileEntry {
                        hash: file_hash.clone(),
                        blocks: blocks.iter().map(|b| b.id.clone()).collect(),
                        mtime,
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

    /// Hybrid search: semantic + BM25 with merged candidates.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>> {
        let store = self.open_store()?;

        let query_tokens = self.embedder.embed_query(query)?;
        let tokens: Vec<Vec<f32>> = (0..query_tokens.nrows())
            .map(|r| query_tokens.row(r).to_vec())
            .collect();
        let token_refs: Vec<&[f32]> = tokens.iter().map(|v| v.as_slice()).collect();

        // Over-fetch more when scope filtering will discard results
        let overfetch = if self.search_scope.is_some() {
            SCOPE_OVERFETCH
        } else {
            1
        };
        let search_k = k.saturating_mul(overfetch);

        // Run both BM25+MaxSim and pure semantic search, merge by ID
        let bm25_query = split_identifiers(query);
        let bm25_results =
            store.search_multi_with_text(&bm25_query, &token_refs, search_k, None)?;
        let semantic_results =
            store.query_with_options(&token_refs, search_k, &SearchOptions::default())?;

        // Merge: keep higher score per ID
        let mut best: HashMap<String, omendb::SearchResult> =
            HashMap::with_capacity(bm25_results.len() + semantic_results.len());

        let mut merge = |results: Vec<omendb::SearchResult>| {
            for r in results {
                match best.entry(r.id.clone()) {
                    Entry::Occupied(mut e) => {
                        if r.distance > e.get().distance {
                            *e.get_mut() = r;
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert(r);
                    }
                }
            }
        };
        merge(bm25_results);
        merge(semantic_results);

        let mut output = Vec::new();
        for r in best.into_values() {
            if let Some(scope) = &self.search_scope {
                let file = r
                    .metadata
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !file.starts_with(scope.as_str()) {
                    continue;
                }
            }

            output.push(self.result_from_omendb(&r));
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

        // Get the block's token embeddings and search with MaxSim reranking
        let (query_tokens, _meta) = store
            .get_tokens(&block_id)
            .with_context(|| "Could not retrieve block token embeddings")?;

        let token_refs: Vec<&[f32]> = query_tokens.iter().map(|v| v.as_slice()).collect();
        let search_k = k.saturating_mul(3).saturating_add(entry.blocks.len());
        let results = store.query_with_options(&token_refs, search_k, &SearchOptions::default())?;

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

            if let Some(scope) = &self.search_scope {
                let file = r
                    .metadata
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !file.starts_with(scope.as_str()) {
                    continue;
                }
            }

            output.push(self.result_from_omendb(&r));

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

    /// Get stale files by comparing content hashes against manifest.
    fn get_stale_files_with_manifest(
        &self,
        files: &HashMap<PathBuf, String>,
        manifest: &Manifest,
    ) -> (Vec<PathBuf>, Vec<String>) {
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

        (changed, deleted)
    }

    /// Fast staleness check using mtime only (no content reads).
    /// Returns paths that may have changed (mtime differs or missing from manifest)
    /// and deleted paths (in manifest but not on disk).
    pub fn get_stale_files_fast(
        &self,
        metadata: &HashMap<PathBuf, walker::FileMetadata>,
    ) -> Result<(Vec<PathBuf>, Vec<String>)> {
        let manifest = Manifest::load(&self.index_dir)?;

        let mut maybe_changed = Vec::new();
        let mut current_rel_files = std::collections::HashSet::new();

        for (path, &(_size, mtime)) in metadata {
            let rel_path = self.to_relative(path);
            current_rel_files.insert(rel_path.clone());

            match manifest.files.get(&rel_path) {
                Some(entry) if entry.mtime == mtime && mtime > 0 => {}
                _ => maybe_changed.push(path.clone()),
            }
        }

        let deleted: Vec<String> = manifest
            .files
            .keys()
            .filter(|k| !current_rel_files.contains(*k))
            .cloned()
            .collect();

        Ok((maybe_changed, deleted))
    }

    /// Check for stale files and update if needed. Single manifest load.
    /// Uses metadata for fast pre-check, only reads content for changed files.
    pub fn check_and_update(
        &self,
        metadata: &HashMap<PathBuf, walker::FileMetadata>,
    ) -> Result<(usize, Option<IndexStats>)> {
        let manifest = Manifest::load(&self.index_dir)?;

        // Fast mtime pre-check
        let mut maybe_changed = Vec::new();
        let mut current_rel_files = std::collections::HashSet::new();

        for (path, &(_size, mtime)) in metadata {
            let rel_path = self.to_relative(path);
            current_rel_files.insert(rel_path.clone());

            match manifest.files.get(&rel_path) {
                Some(entry) if entry.mtime == mtime && mtime > 0 => {}
                _ => maybe_changed.push(path.clone()),
            }
        }

        let deleted: Vec<String> = manifest
            .files
            .keys()
            .filter(|k| !current_rel_files.contains(*k))
            .cloned()
            .collect();

        let stale_count = maybe_changed.len() + deleted.len();
        if stale_count == 0 {
            return Ok((0, None));
        }

        // Read content only for potentially changed files, then hash-check
        let mut changed_files: HashMap<PathBuf, String> = HashMap::new();
        for path in &maybe_changed {
            let raw = match std::fs::read(path) {
                Ok(data) => data,
                Err(_) => continue,
            };
            let check_len = raw.len().min(8192);
            if raw[..check_len].contains(&0) {
                continue;
            }
            let content = match String::from_utf8(raw) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let rel_path = self.to_relative(path);
            let file_hash = hash_content(&content);
            match manifest.files.get(&rel_path) {
                Some(entry) if entry.hash == file_hash => {}
                _ => {
                    changed_files.insert(path.clone(), content);
                }
            }
        }

        if changed_files.is_empty() && deleted.is_empty() {
            return Ok((0, None));
        }

        let actual_stale = changed_files.len() + deleted.len();

        // Delete vectors for deleted files
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

        let mut stats = self.index(&changed_files, None)?;
        stats.deleted += deleted_count;
        Ok((actual_stale, Some(stats)))
    }

    /// Get stale files (changed + deleted). Loads manifest internally.
    pub fn get_stale_files(
        &self,
        files: &HashMap<PathBuf, String>,
    ) -> Result<(Vec<PathBuf>, Vec<String>)> {
        let manifest = Manifest::load(&self.index_dir)?;
        Ok(self.get_stale_files_with_manifest(files, &manifest))
    }

    /// Quick check: how many files need updating?
    pub fn needs_update(&self, files: &HashMap<PathBuf, String>) -> Result<usize> {
        let manifest = Manifest::load(&self.index_dir)?;
        let (changed, deleted) = self.get_stale_files_with_manifest(files, &manifest);
        Ok(changed.len() + deleted.len())
    }

    /// Incremental update.
    pub fn update(&self, files: &HashMap<PathBuf, String>) -> Result<IndexStats> {
        let manifest = Manifest::load(&self.index_dir)?;
        let (changed, deleted) = self.get_stale_files_with_manifest(files, &manifest);

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

        let mut stats = self.index(&changed_files, None)?;
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

    fn result_from_omendb(&self, r: &omendb::SearchResult) -> SearchResult {
        let file = r
            .metadata
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        SearchResult {
            file: self.to_absolute(file),
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
        }
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
            omendb::VectorStore::multi_vector_with(
                embedder::MODEL.token_dim,
                omendb::MultiVectorConfig::compact(),
            )?
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
        if let Some(meta) = store.get_metadata_by_id(block_id) {
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
        if let Some(meta) = store.get_metadata_by_id(block_id) {
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
