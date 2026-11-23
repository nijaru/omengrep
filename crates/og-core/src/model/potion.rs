//! Native static Model2Vec embedder (potion-code-16M-v2).
//!
//! Re-implements model2vec's StaticModel.encode in Rust: WordPiece tokenize
//! (no special tokens) → filter [UNK] → truncate to max_length → mean-pool
//! embedding rows → L2 normalize. No ONNX runtime, no
//! neural warm-up: inference is a row lookup plus pooling.
//!
//! Reference semantics (MinishLab/model2vec model2vec/model.py):
//! - `tokenize`: encode_batch_fast(add_special_tokens=False); ids with
//!   unk_token_id are dropped; pre-truncate input at
//!   max_length * median_token_length chars, then truncate ids to max_length.
//! - `encode`: mean-pool rows and normalize as required by the pinned
//!   snapshot (config.json: normalize=true); empty input → zero vector.
//! - Identity: the weights file hash + tokenizer hash + dims are pinned in
//!   the index manifest; a mismatch forces a rebuild (no vector-space mixing).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tokenizers::Tokenizer;

use crate::model::Embedder;

/// Default model on the Hub.
pub const DEFAULT_REPO: &str = "minishlab/potion-code-16M-v2";
const DEFAULT_REVISION: &str = "e9d2a44ca6a05ac6685f3b23709ea57eb7352d5b";
const MODEL_FILES: [&str; 3] = ["model.safetensors", "tokenizer.json", "config.json"];

/// Reference default (model2vec DEFAULT_MAX_LENGTH).
const MAX_LENGTH: usize = 512;

pub struct PotionEmbedder {
    id: String,
    dims: usize,
    tokenizer: Tokenizer,
    /// (vocab_size, dims) row-major.
    embeddings: Vec<f32>,
    vocab_size: usize,
    unk_token_id: Option<u32>,
    median_token_length: usize,
}

impl PotionEmbedder {
    /// Load from a local directory containing model.safetensors,
    /// tokenizer.json, and config.json.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let model_path = dir.join("model.safetensors");
        let tokenizer_path = dir.join("tokenizer.json");
        let config_path = dir.join("config.json");

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow!("loading tokenizer {}: {e}", tokenizer_path.display()))?;

        // Vectors: F16 [vocab, dims] from safetensors.
        let raw = std::fs::read(&model_path)
            .with_context(|| format!("reading {}", model_path.display()))?;
        let st = safetensors::SafeTensors::deserialize(&raw)
            .with_context(|| format!("parsing {}", model_path.display()))?;
        let tensor = st
            .tensor("embeddings")
            .map_err(|e| anyhow!("no 'embeddings' tensor: {e}"))?;
        anyhow::ensure!(
            tensor.dtype() == safetensors::Dtype::F16,
            "expected F16 embeddings"
        );
        let shape = tensor.shape();
        anyhow::ensure!(shape.len() == 2, "expected 2-D embeddings, got {shape:?}");
        let (vocab_size, dims) = (shape[0], shape[1]);
        let f16 = tensor
            .data()
            .chunks_exact(2)
            .map(|c| half::f16::from_le_bytes([c[0], c[1]]).to_f32())
            .collect::<Vec<f32>>();
        anyhow::ensure!(
            f16.len() == vocab_size * dims,
            "embedding count {} != {vocab_size}x{dims}",
            f16.len()
        );

        // Tokenizer facts.
        let vocab_len = tokenizer.get_vocab_size(true);
        if vocab_len != vocab_size {
            bail!(
                "tokenizer vocab ({vocab_len}) != embedding rows ({vocab_size}) — \
                 wrong tokenizer for these weights"
            );
        }
        let unk_token_id = tokenizer.get_vocab(true).get("[UNK]").copied();
        let tokens = tokenizer.get_vocab(true);
        let lengths: Vec<usize> = tokens.keys().map(|t: &String| t.len()).collect();
        let median_token_length = median(&mut lengths.clone()).clamp(1, 8);

        // The index compares dot products as cosines, so this backend
        // accepts only normalized models. A config change must not
        // silently change encoding under the same model identity.
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&config_path)?)
            .with_context(|| format!("parsing {}", config_path.display()))?;
        anyhow::ensure!(
            config.get("normalize").and_then(|v| v.as_bool()) == Some(true),
            "Potion config must enable normalization"
        );

        // Identity pins weights + tokenizer bytes.
        let id = format!(
            "{DEFAULT_REPO}@{model_hash}@{tokenizer_hash}",
            model_hash = blake3_12(&raw),
            tokenizer_hash = blake3_12(
                &std::fs::read(&tokenizer_path)
                    .with_context(|| format!("reading {}", tokenizer_path.display()))?
            ),
        );

        Ok(Self {
            id,
            dims,
            tokenizer,
            embeddings: f16,
            vocab_size,
            unk_token_id,
            median_token_length,
        })
    }

    /// Resolve the default model directory, downloading to the hf-hub cache
    /// on first use. Errors carry actionable messages for offline users.
    pub fn load_default() -> Result<Self> {
        let dir = download_default_model()?;
        Self::load_dir(&dir)
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    /// Tokenize one text to embedding-row ids, reference semantics:
    /// no special tokens, [UNK] dropped, truncated to MAX_LENGTH.
    fn token_ids(&self, text: &str) -> Vec<u32> {
        // Pre-truncate input: max_length * median_token_length chars.
        let char_limit = MAX_LENGTH * self.median_token_length;
        let input: String = if text.len() > char_limit {
            text.chars().take(char_limit).collect()
        } else {
            text.to_string()
        };

        let encoding = self
            .tokenizer
            .encode_char_offsets(input, false)
            .unwrap_or_default();

        let mut ids: Vec<u32> = encoding.get_ids().to_vec();
        if let Some(unk) = self.unk_token_id {
            ids.retain(|&id| id != unk);
        }
        ids.truncate(MAX_LENGTH);
        ids
    }

    fn mean_pool(&self, ids: &[u32]) -> Vec<f32> {
        let mut out = vec![0.0f32; self.dims];
        if ids.is_empty() {
            return out;
        }
        for &id in ids {
            let row = &self.embeddings[id as usize * self.dims..(id as usize + 1) * self.dims];
            for (o, &v) in out.iter_mut().zip(row) {
                *o += v;
            }
        }
        for o in &mut out {
            *o /= ids.len() as f32;
        }
        out
    }

    fn l2_normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for x in v {
                *x /= norm;
            }
        }
    }
}

impl Embedder for PotionEmbedder {
    fn id(&self) -> &str {
        &self.id
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

impl PotionEmbedder {
    fn embed_one(&self, text: &str) -> Vec<f32> {
        let ids = self.token_ids(text);
        let mut v = self.mean_pool(&ids);
        Self::l2_normalize(&mut v);
        v
    }
}

fn blake3_12(data: &[u8]) -> String {
    blake3::hash(data).to_hex()[..12].to_string()
}

fn median(sorted: &mut [usize]) -> usize {
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn default_repo() -> hf_hub::Repo {
    hf_hub::Repo::with_revision(
        DEFAULT_REPO.into(),
        hf_hub::RepoType::Model,
        DEFAULT_REVISION.into(),
    )
}

/// Local-only presence check. Look directly in the pinned snapshot so an
/// existing download via the main branch is reusable without networking.
pub fn default_model_cached() -> Option<PathBuf> {
    let cache = hf_hub::Cache::from_env();
    let dir = cache.repo(default_repo()).pointer_path(DEFAULT_REVISION);
    MODEL_FILES
        .iter()
        .all(|name| dir.join(name).is_file())
        .then_some(dir)
}

fn download_default_model() -> Result<PathBuf> {
    if let Some(dir) = default_model_cached() {
        return Ok(dir);
    }

    let cache = hf_hub::Cache::from_env();
    eprintln!(
        "Downloading embedding model {DEFAULT_REPO} (~33 MB, first use) to {}",
        cache.path().display()
    );
    let api = hf_hub::api::sync::ApiBuilder::from_cache(cache)
        .build()
        .context("creating HF API client (offline?)")?;
    let repo = api.repo(default_repo());
    for name in MODEL_FILES {
        repo.get(name).with_context(|| {
            format!("downloading {name} from {DEFAULT_REPO}; run 'og model install' with network to prefetch")
        })?;
    }
    default_model_cached().context("model download did not populate the pinned snapshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_sane() {
        assert_eq!(median(&mut [3, 1, 2]), 2);
        assert_eq!(median(&mut [5, 1, 2, 4]), 4); // upper median on even splits
    }

    #[test]
    fn l2() {
        let mut v = vec![3.0, 4.0];
        PotionEmbedder::l2_normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
