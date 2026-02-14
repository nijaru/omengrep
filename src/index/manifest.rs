use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::embedder::MODEL_VERSION;

pub const MANIFEST_VERSION: u32 = 8;
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub model: String,
    pub files: HashMap<String, FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: String,
    pub blocks: Vec<String>,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            version: MANIFEST_VERSION,
            model: MODEL_VERSION.to_string(),
            files: HashMap::new(),
        }
    }

    pub fn load(index_dir: &Path) -> Result<Self> {
        let manifest_path = index_dir.join(MANIFEST_FILE);

        if !manifest_path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let data: serde_json::Value = serde_json::from_str(&content)?;

        let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

        // Old manifests (v1-v7) are incompatible — different model, dims, metric
        if version < MANIFEST_VERSION {
            let has_files = data
                .get("files")
                .map(|f| f.as_object().map_or(false, |o| !o.is_empty()))
                .unwrap_or(false);
            if has_files {
                bail!("Index was created by an older version. Run 'hhg build --force' to rebuild.");
            }
        }

        // Validate model version
        let stored_model = data.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if !stored_model.is_empty() && stored_model != MODEL_VERSION {
            let has_files = data
                .get("files")
                .map(|f| f.as_object().map_or(false, |o| !o.is_empty()))
                .unwrap_or(false);
            if has_files {
                bail!(
                    "Index was created with a different model. Run 'hhg build --force' to rebuild."
                );
            }
        }

        let manifest: Manifest = serde_json::from_value(data)?;
        Ok(manifest)
    }

    pub fn save(&self, index_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(index_dir)?;
        let manifest_path = index_dir.join(MANIFEST_FILE);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(manifest_path, content)?;
        Ok(())
    }
}
