//! Generation manifest: pinned identity for an immutable published generation.
//!
//! Model identity (embedder id + dims) is pinned here. A catalog opened
//! under a different identity is rejected — no silent vector-space mixing.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: i64,
    /// Index root this generation indexes (absolute, canonical).
    pub root: String,
    /// Embedder identity (model id). Rebuild if it differs.
    pub model_id: String,
    /// Embedding dimensions.
    pub dims: usize,
    /// Catalog schema version.
    pub schema_version: i64,
    /// Blocks in this generation (== vector rows).
    pub blocks: usize,
    /// Files indexed.
    pub files: usize,
    /// Build timestamp (unix secs).
    pub built_at: u64,
    /// Deterministic content stamp of what was indexed.
    pub content_hash: String,
}

impl Manifest {
    pub fn write(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        let m: Manifest = serde_json::from_str(&raw)
            .with_context(|| format!("parsing manifest {}", path.display()))?;
        if m.version != MANIFEST_VERSION {
            anyhow::bail!(
                "manifest version {} unsupported (expected {MANIFEST_VERSION}) — rebuild required",
                m.version
            );
        }
        Ok(m)
    }
}
