pub mod onnx;
pub mod tokenizer;

use anyhow::{Context, Result};
use ndarray::Array2;

/// Configuration for an embedding model.
pub struct ModelConfig {
    pub repo: &'static str,
    pub model_file: &'static str,
    pub tokenizer_file: &'static str,
    pub token_dim: usize,
    pub doc_max_length: usize,
    pub query_max_length: usize,
    pub version: &'static str,
    pub batch_size: usize,
}

/// LateOn-Code-edge: 17M params, 48d/token, INT8 ONNX.
pub const MODEL: &ModelConfig = &ModelConfig {
    repo: "lightonai/LateOn-Code-edge",
    model_file: "model.onnx",
    tokenizer_file: "tokenizer.json",
    token_dim: 48,
    doc_max_length: 512,
    query_max_length: 256,
    version: "lateon-code-edge-v1",
    batch_size: 64,
};

/// Embedding output: variable-length token embeddings per document.
/// Each document produces (num_tokens, token_dim) embeddings.
pub struct TokenEmbeddings {
    /// One entry per document: each is (num_tokens, token_dim).
    pub embeddings: Vec<Array2<f32>>,
}

/// Trait for multi-vector embedding backends.
pub trait Embedder: Send + Sync {
    /// Embed documents, returning per-token embeddings for each.
    fn embed_documents(&self, texts: &[&str]) -> Result<TokenEmbeddings>;

    /// Embed a query, returning token embeddings.
    fn embed_query(&self, text: &str) -> Result<Array2<f32>>;
}

/// Create the embedder, downloading model files if needed.
pub fn create_embedder() -> Result<Box<dyn Embedder>> {
    let (model_path, tokenizer_path) = download_model_files(MODEL)?;
    Ok(Box::new(onnx::OnnxEmbedder::new(
        &model_path,
        &tokenizer_path,
        MODEL,
    )?))
}

/// Download both model and tokenizer files, returning their local paths.
fn download_model_files(config: &ModelConfig) -> Result<(String, String)> {
    let api = hf_hub::api::sync::Api::new().context("Failed to create HF Hub API")?;
    let repo = api.model(config.repo.to_string());

    let model_path = repo.get(config.model_file).with_context(|| {
        format!(
            "Failed to download model from {}. Run 'og model install' while online.",
            config.repo
        )
    })?;

    let tokenizer_path = repo.get(config.tokenizer_file).with_context(|| {
        format!(
            "Failed to download tokenizer from {}. Run 'og model install' while online.",
            config.repo
        )
    })?;

    Ok((
        model_path.to_string_lossy().into_owned(),
        tokenizer_path.to_string_lossy().into_owned(),
    ))
}
