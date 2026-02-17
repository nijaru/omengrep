use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::embedder;

pub const MANIFEST_VERSION: u32 = 9;
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

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            model: embedder::MODEL.version.to_string(),
            files: HashMap::new(),
        }
    }
}

impl Manifest {
    pub fn load(index_dir: &Path) -> Result<Self> {
        let manifest_path = index_dir.join(MANIFEST_FILE);

        if !manifest_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        if content.trim().is_empty() {
            return Ok(Self::default());
        }

        let data: serde_json::Value = serde_json::from_str(&content)?;

        let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

        if version > MANIFEST_VERSION {
            bail!(
                "Index was created by a newer version of og. \
                 Please upgrade og or run 'og build --force' to rebuild."
            );
        }

        // Old manifests are incompatible — different model, dims, metric
        if version < MANIFEST_VERSION {
            let has_files = data
                .get("files")
                .map(|f| f.as_object().is_some_and(|o| !o.is_empty()))
                .unwrap_or(false);
            if has_files {
                bail!("Index was created by an older version. Run 'og build --force' to rebuild.");
            }
        }

        let manifest: Manifest = serde_json::from_value(data)?;
        Ok(manifest)
    }

    pub fn save(&self, index_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(index_dir)?;
        let manifest_path = index_dir.join(MANIFEST_FILE);
        let tmp_path = index_dir.join(".manifest.json.tmp");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &manifest_path)?;
        Ok(())
    }
}
