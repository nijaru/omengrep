//! Vector sidecar: contiguous fp16 rows, one per block, mmapped for search.
//!
//! Rows are written in catalog block order during build; row i corresponds
//! to blocks.rowid i (1-based). Exact rayon scan computes cosine similarity
//! against a normalized query vector.

use std::fs::File;
use std::path::{Path, PathBuf};

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

/// Append-only writer for building a vector sidecar alongside the catalog.
/// Rows must be written in the same order as catalog block inserts.
pub struct VectorWriter {
    file: std::io::BufWriter<File>,
    dims: usize,
    rows: usize,
    path: PathBuf,
}

impl VectorWriter {
    pub fn create(path: &Path, dims: usize) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)
            .with_context(|| format!("creating vector store {}", path.display()))?;
        Ok(Self {
            file: std::io::BufWriter::new(file),
            dims,
            rows: 0,
            path: path.to_path_buf(),
        })
    }

    pub fn write_vec(&mut self, vec: &[f32]) -> Result<usize> {
        anyhow::ensure!(
            vec.len() == self.dims,
            "vector dim {} != expected {}",
            vec.len(),
            self.dims
        );
        let mut bytes = Vec::with_capacity(vec.len() * FP32_BYTES);
        for &v in vec {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        use std::io::Write;
        self.file.write_all(&bytes)?;
        self.rows += 1;
        Ok(self.rows)
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn finish(mut self) -> Result<PathBuf> {
        use std::io::Write;
        self.file.flush()?;
        let f = self.file.into_inner()?;
        f.sync_all()?;
        Ok(self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        w.write_vec(&a).unwrap();
        w.write_vec(&b).unwrap();
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
