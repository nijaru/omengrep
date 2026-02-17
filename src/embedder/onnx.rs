use std::sync::Mutex;

use anyhow::{Context, Result};
use ndarray::Array2;
use ort::value::TensorRef;

use super::tokenizer::TokenizerWrapper;
use super::{Embedder, ModelConfig, TokenEmbeddings};

/// ONNX-based embedder for LateOn-Code models.
pub struct OnnxEmbedder {
    session: Mutex<ort::session::Session>,
    tokenizer: TokenizerWrapper,
    batch_size: usize,
}

impl OnnxEmbedder {
    pub fn new(model_path: &str, tokenizer_path: &str, config: &ModelConfig) -> Result<Self> {
        let session = ort::session::Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus())?
            .commit_from_file(model_path)
            .context("Failed to load ONNX model")?;
        let tokenizer = TokenizerWrapper::new(tokenizer_path, config)?;
        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            batch_size: config.batch_size,
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<TokenEmbeddings> {
        let encodings = self.tokenizer.encode_documents(texts)?;

        let batch_size = encodings.len();
        let seq_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0);

        // Build input tensors
        let mut input_ids = vec![0i64; batch_size * seq_len];
        let mut attention_mask = vec![0i64; batch_size * seq_len];

        for (i, enc) in encodings.iter().enumerate() {
            for (j, &id) in enc.get_ids().iter().enumerate() {
                input_ids[i * seq_len + j] = id as i64;
            }
            for (j, &mask) in enc.get_attention_mask().iter().enumerate() {
                attention_mask[i * seq_len + j] = mask as i64;
            }
        }

        let input_ids = ndarray::Array2::from_shape_vec((batch_size, seq_len), input_ids)?;
        let attention_mask =
            ndarray::Array2::from_shape_vec((batch_size, seq_len), attention_mask)?;

        // Run inference
        let input_ids_tensor = TensorRef::from_array_view(&input_ids)?;
        let attention_mask_tensor = TensorRef::from_array_view(&attention_mask)?;
        let mut session = self.session.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
        ])?;

        // Extract token embeddings: (batch, seq_len, TOKEN_DIM)
        let output = outputs.get("last_hidden_state").unwrap_or(&outputs[0]);
        let view = output.try_extract_array::<f32>()?;

        // Extract per-document token embeddings, filtering by attention mask
        let mut result = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let num_tokens = encodings[i]
                .get_attention_mask()
                .iter()
                .filter(|&&m| m == 1)
                .count();

            // Slice the output view directly — avoids element-by-element copy
            let mut tokens = view.slice(ndarray::s![i, 0..num_tokens, ..]).to_owned();

            // L2 normalize each token vector
            for mut row in tokens.rows_mut() {
                let norm: f32 = row.dot(&row).sqrt();
                if norm > 1e-9 {
                    row /= norm;
                }
            }
            result.push(tokens);
        }

        Ok(TokenEmbeddings { embeddings: result })
    }
}

impl Embedder for OnnxEmbedder {
    fn embed_documents(&self, texts: &[&str]) -> Result<TokenEmbeddings> {
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.batch_size) {
            let batch_result = self.embed_batch(chunk)?;
            all_embeddings.extend(batch_result.embeddings);
        }

        Ok(TokenEmbeddings {
            embeddings: all_embeddings,
        })
    }

    fn embed_query(&self, text: &str) -> Result<Array2<f32>> {
        let result = self.embed_batch(&[text])?;
        result
            .embeddings
            .into_iter()
            .next()
            .context("No embedding produced for query")
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
