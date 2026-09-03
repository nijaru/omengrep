//! Native static Model2Vec embedder (potion-code-16M-v2).
//!
//! Re-implements model2vec's StaticModel.encode in Rust: WordPiece tokenize
//! (no special tokens) → filter [UNK] → truncate to max_length → mean-pool
//! embedding rows → L2 normalize (config-driven). No ONNX runtime, no
//! neural warm-up: inference is a row lookup plus pooling.
//!
//! Reference semantics (MinishLab/model2vec model2vec/model.py):
//! - `tokenize`: encode_batch_fast(add_special_tokens=False); ids with
//!   unk_token_id are dropped; pre-truncate input at
//!   max_length * median_token_length chars, then truncate ids to max_length.
//! - `encode`: mean-pool rows; empty token list → zero vector; then
//!   normalize if config says so (potion config.json: normalize=true).
//! - Identity: the weights file hash + tokenizer hash + dims are pinned in
//!   the index manifest; a mismatch forces a rebuild (no vector-space mixing).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tokenizers::Tokenizer;

use crate::model::Embedder;

/// Default model on the Hub.
pub const DEFAULT_REPO: &str = "minishlab/potion-code-16M-v2";

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
    normalize: bool,
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
        anyhow::ensure!(tensor.dtype() == safetensors::Dtype::F16, "expected F16 embeddings");
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
        let unk_token_id = tokenizer
            .get_vocab(true)
            .get("[UNK]")
            .copied();
        let tokens = tokenizer.get_vocab(true);
        let lengths: Vec<usize> = tokens.keys().map(|t: &String| t.len()).collect();
        let median_token_length = median(&mut lengths.clone()).clamp(1, 8);

        // Config: normalize (potion ships true).
        let normalize = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| v.get("normalize").and_then(|n| n.as_bool()))
            .unwrap_or(true);

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
            normalize,
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
        if self.normalize {
            Self::l2_normalize(&mut v);
        }
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

/// Download (or resolve from cache) the default potion model.
fn download_default_model() -> Result<PathBuf> {
    use hf_hub::api::sync::Api;

    let api = Api::new().context("creating HF API client (offline?)")?;
    let repo = api.model(DEFAULT_REPO.to_string());
    let mut files = Vec::new();
    for name in ["model.safetensors", "tokenizer.json", "config.json"] {
        let path = repo
            .download(name)
            .with_context(|| format!("downloading {name} from {DEFAULT_REPO} (run 'og model install' with network)"))?;
        files.push(path);
    }
    // All files live in the same snapshot dir.
    files[0]
        .parent()
        .map(Path::to_path_buf)
        .context("snapshot dir")
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
