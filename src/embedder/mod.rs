pub mod onnx;
pub mod tokenizer;

use anyhow::Result;
use ndarray::Array2;

/// Model configuration for LateOn-Code-edge.
pub const MODEL_REPO: &str = "lightonai/LateOn-Code-edge";
pub const MODEL_FILE: &str = "model.onnx";
pub const TOKENIZER_FILE: &str = "tokenizer.json";
pub const TOKEN_DIM: usize = 48;
pub const DOC_MAX_LENGTH: usize = 2048;
pub const QUERY_MAX_LENGTH: usize = 256;
pub const MODEL_VERSION: &str = "lateon-code-edge-v1";
pub const BATCH_SIZE: usize = 64;

/// Embedding output: variable-length token embeddings per document.
/// Each document produces (num_tokens, TOKEN_DIM) embeddings.
pub struct TokenEmbeddings {
    /// One entry per document: each is (num_tokens, TOKEN_DIM).
    pub embeddings: Vec<Array2<f32>>,
}

/// Trait for multi-vector embedding backends.
pub trait Embedder: Send + Sync {
    /// Embed documents, returning per-token embeddings for each.
    fn embed_documents(&self, texts: &[&str]) -> Result<TokenEmbeddings>;

    /// Embed a query, returning token embeddings.
    fn embed_query(&self, text: &str) -> Result<Array2<f32>>;
}

/// Create the default embedder.
pub fn create_embedder() -> Result<Box<dyn Embedder>> {
    Ok(Box::new(onnx::OnnxEmbedder::new()?))
}
