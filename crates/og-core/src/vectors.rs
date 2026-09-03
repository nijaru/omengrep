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

/// Per-row bytes for fp16 vectors (tk-1i9o scale gate: halves the sidecar
/// and search RSS vs f32; cosine deviation < 1e-3 on 256-d unit vectors).
const FP16_BYTES: usize = 2;

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
    /// Open `vectors-000.bin` with `dims` dimensions, fp16 rows.
    pub fn open(path: &Path, dims: usize) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("opening vector store {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("mmapping vector store {}", path.display()))?;
        let row_bytes = dims * FP16_BYTES;
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
                    // fp16 rows: little-endian lanes, decoded to f32
                    let lane = [bytes[j * FP16_BYTES], bytes[j * FP16_BYTES + 1]];
                    dot += q * half::f16::from_le_bytes(lane).to_f32();
                }
                dot
            })
            .collect();

        let mut ranked: Vec<(usize, f32)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, s)| *s >= SIMILARITY_FLOOR)
            .collect();
        if ranked.is_empty() {
            return Vec::new();
        }
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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
        let rows = file.metadata()?.len() as i64 / (dims as i64 * FP16_BYTES as i64);
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
        self.dims * FP16_BYTES
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
        let mut bytes = Vec::with_capacity(vec.len() * FP16_BYTES);
        for &v in vec {
            bytes.extend_from_slice(&half::f16::from_f32(v).to_le_bytes());
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
        // a matches itself with cosine ~1.0 (fp16 lane error ~2^-11)
        assert_eq!(top[0].0, 1);
        assert!((top[0].1 - 1.0).abs() < 5e-3, "cosine: {}", top[0].1);
    }

    #[test]
    fn fp16_cosine_deviation_negligible() {
        // 256-d unit vectors: fp16 storage must not move cosine past the
        // ranking noise floor (floor is 0.30; deviation budget 1e-3).
        let (_dir, path) = tempdir();
        let dims = 256;
        let a = normalize(
            &(0..dims)
                .map(|i| (i as f32 * 0.7).sin() + 2.0)
                .collect::<Vec<_>>(),
        );
        // b close to a: high cosine, exercises lane precision, not filtering
        let b = normalize(
            &a.iter()
                .enumerate()
                .map(|(i, x)| x + 0.01 * ((i as f32 * 1.3).sin()))
                .collect::<Vec<_>>(),
        );
        let mut w = VectorWriter::create(&path, dims).unwrap();
        w.write_at(1, &a).unwrap();
        w.write_at(2, &b).unwrap();
        w.finish().unwrap();

        let f32_dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(f32_dot > 0.99, "test setup: f32 dot {f32_dot}");
        let store = VectorStore::open(&path, dims).unwrap();
        let top = store.top_k(&a, 2);
        assert_eq!(top[0].0, 1);
        let cross = top.iter().find(|(id, _)| *id == 2).map(|(_, s)| *s);
        match cross {
            Some(c) => assert!((c - f32_dot).abs() < 1e-3, "f32 {f32_dot} vs fp16 {c}"),
            None => panic!("near-duplicate row filtered: f32 dot {f32_dot}"),
        }
    }

    fn normalize(v: &[f32]) -> Vec<f32> {
        let norm = v
            .iter()
            .map(|x| (*x as f64) * (*x as f64))
            .sum::<f64>()
            .sqrt();
        v.iter().map(|x| (*x as f64 / norm) as f32).collect()
    }
}
