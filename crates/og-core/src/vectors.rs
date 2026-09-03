//! Vector sidecar: contiguous fp16 rows, one per block, mmapped for search.
//!
//! Rows are written in catalog block order during build; row i corresponds
//! to blocks.rowid i (1-based). Exact rayon scan computes cosine similarity
//! against a normalized query vector.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use memmap2::Mmap;
use rayon::prelude::*;

/// Per-row bytes for f32 vectors (future neural tier keeps fp32 precision).
const FP32_BYTES: usize = 4;

/// Minimum cosine similarity for a hit. Filters channel noise: unrelated
/// texts hover near 0 (random 256-d pairs std-dev ~0.06), related code sits
/// well above. Revisit when tk-7wp8 swaps in a real model.
const SIMILARITY_FLOOR: f32 = 0.30;

/// An open vector store for a published generation.
pub struct VectorStore {
    mmap: Mmap,
    dims: usize,
    row_bytes: usize,
}

impl VectorStore {
    /// Open `vectors-000.bin` with `dims` dimensions, f32 rows.
    pub fn open(path: &Path, dims: usize) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("opening vector store {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("mmapping vector store {}", path.display()))?;
        let row_bytes = dims * FP32_BYTES;
        if mmap.len() % row_bytes != 0 {
            anyhow::bail!(
                "vector store size {} not a multiple of row size {}",
                mmap.len(),
                row_bytes
            );
        }
        Ok(Self {
            mmap,
            dims,
            row_bytes,
        })
    }

    pub fn len(&self) -> usize {
        self.mmap.len() / self.row_bytes
    }

    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    fn row(&self, i: usize) -> &[u8] {
        &self.mmap[i * self.row_bytes..(i + 1) * self.row_bytes]
    }

    /// Exact cosine scan: top-k (block rowid, similarity) pairs.
    /// `query` must be unit-normalized. Rowids are 1-based, matching blocks.id.
    pub fn top_k(&self, query: &[f32], k: usize) -> Vec<(i64, f32)> {
        debug_assert_eq!(query.len(), self.dims, "query/vector dim mismatch");
        let n = self.len();
        let scores: Vec<f32> = (0..n)
            .into_par_iter()
            .map(|i| {
                let bytes = self.row(i);
                let mut dot = 0.0f32;
                for (j, &q) in query.iter().enumerate() {
                    // f32 rows: little-endian lanes
                    let bytes_j =
                        &bytes[j * FP32_BYTES..(j + 1) * FP32_BYTES];
                    let bits = u32::from_le_bytes([
                        bytes_j[0],
                        bytes_j[1],
                        bytes_j[2],
                        bytes_j[3],
                    ]);
                    let v = f32::from_bits(bits);
                    dot += q * v;
                }
                dot
            })
            .collect();

        let mut ranked: Vec<(usize, f32)> =
            scores.into_iter().enumerate().filter(|(_, s)| *s >= SIMILARITY_FLOOR).collect();
        if ranked.is_empty() {
            return Vec::new();
        }
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked
            .into_iter()
            .take(k)
            .map(|(i, s)| (i as i64 + 1, s))
            .collect()
    }
}

/// Position-addressed writer for the vector sidecar.
///
/// Rows live at explicit 1-based rowid positions matching `blocks.id`.
/// SQLite may reuse deleted max rowids, so rows are NEVER appended by
/// position arithmetic — callers pass the rowid that SQLite actually
/// assigned. Gaps (never-reused holes) are zero-padded; the similarity
/// floor makes them unreachable. No BufWriter: seek+write per row keeps
/// positioning unambiguous.
pub struct VectorWriter {
    file: std::fs::File,
    dims: usize,
    /// Highest rowid ever written (== sidecar row count).
    max_row: i64,
}

impl VectorWriter {
    pub fn create(path: &Path, dims: usize) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)
            .with_context(|| format!("creating vector store {}", path.display()))?;
        Ok(Self {
            file,
            dims,
            max_row: 0,
        })
    }

    /// Open an existing sidecar for in-place row updates (incremental path).
    pub fn open_existing(path: &Path, dims: usize) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .with_context(|| format!("opening vector store for update {}", path.display()))?;
        let rows = file.metadata()?.len() as i64 / (dims as i64 * FP32_BYTES as i64);
        Ok(Self {
            file,
            dims,
            max_row: rows,
        })
    }

    /// Current row count (== max rowid written).
    pub fn rows(&self) -> i64 {
        self.max_row
    }

    fn row_bytes(&self) -> usize {
        self.dims * FP32_BYTES
    }

    /// Write a vector at its 1-based rowid. Zero-pads gaps below `rowid`.
    pub fn write_at(&mut self, rowid: i64, vec: &[f32]) -> Result<()> {
        anyhow::ensure!(
            vec.len() == self.dims,
            "vector dim {} != expected {}",
            vec.len(),
            self.dims
        );
        anyhow::ensure!(rowid >= 1, "rowid must be >= 1, got {rowid}");
        self.seek_row(rowid)?;
        self.write_row_bytes(vec)?;
        if rowid > self.max_row {
            self.max_row = rowid;
        }
        Ok(())
    }

    /// Zero the rows of deleted blocks (holes stay unreachable via floor).
    pub fn zero_rows(&mut self, rowids: &[i64]) -> Result<()> {
        let zeros = vec![0.0f32; self.dims];
        for id in rowids {
            if *id >= 1 && *id <= self.max_row {
                self.write_at(*id, &zeros)?;
            }
        }
        Ok(())
    }

    fn seek_row(&mut self, rowid: i64) -> Result<()> {
        use std::io::Seek;
        let offset = (rowid - 1) as u64 * self.row_bytes() as u64;
        self.file
            .seek(std::io::SeekFrom::Start(offset))
            .with_context(|| format!("seeking vector row {rowid}"))?;
        Ok(())
    }

    fn write_row_bytes(&mut self, vec: &[f32]) -> Result<()> {
        let mut bytes = Vec::with_capacity(vec.len() * FP32_BYTES);
        for &v in vec {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        use std::io::Write;
        self.file.write_all(&bytes)?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vectors-000.bin");
        (dir, p)
    }

    #[test]
    fn write_read_roundtrip() {
        let (_dir, path) = tempdir();
        let dims = 8;
        let a: Vec<f32> = (0..dims).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..dims).map(|i| (dims - i) as f32).collect();

        let a = normalize(&a);
        let b = normalize(&b);

        let mut w = VectorWriter::create(&path, dims).unwrap();
        w.write_at(1, &a).unwrap();
        w.write_at(2, &b).unwrap();
        w.finish().unwrap();

        let store = VectorStore::open(&path, dims).unwrap();
        assert_eq!(store.len(), 2);

        let top = store.top_k(&a, 2);
        // a matches itself with cosine 1.0 (rows are unit-normalized)
        assert_eq!(top[0].0, 1);
        assert!((top[0].1 - 1.0).abs() < 1e-5, "cosine: {}", top[0].1);
    }

    fn normalize(v: &[f32]) -> Vec<f32> {
        let norm = v.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
        v.iter().map(|x| (*x as f64 / norm) as f32).collect()
    }
}
