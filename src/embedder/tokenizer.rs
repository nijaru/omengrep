use anyhow::{Context, Result};
use tokenizers::Tokenizer;

use super::{DOC_MAX_LENGTH, MODEL_REPO, QUERY_MAX_LENGTH, TOKENIZER_FILE};

/// Wrapper around HuggingFace tokenizer.
pub struct TokenizerWrapper {
    tokenizer: Tokenizer,
}

impl TokenizerWrapper {
    pub fn new() -> Result<Self> {
        let tokenizer_path = download_tokenizer()?;
        let tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { tokenizer })
    }

    /// Encode texts for document embedding (longer max length).
    pub fn encode_documents(&self, texts: &[&str]) -> Result<Vec<tokenizers::Encoding>> {
        self.encode_batch(texts, DOC_MAX_LENGTH)
    }

    /// Encode a query (shorter max length).
    pub fn encode_query(&self, text: &str) -> Result<tokenizers::Encoding> {
        let mut tokenizer = self.tokenizer.clone();
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: QUERY_MAX_LENGTH,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            ..Default::default()
        }));
        tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn encode_batch(&self, texts: &[&str], max_length: usize) -> Result<Vec<tokenizers::Encoding>> {
        let mut tokenizer = self.tokenizer.clone();
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            ..Default::default()
        }));
        let inputs: Vec<tokenizers::EncodeInput> = texts
            .iter()
            .map(|t| tokenizers::EncodeInput::Single((*t).into()))
            .collect();
        tokenizer
            .encode_batch(inputs, true)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn download_tokenizer() -> Result<String> {
    let api = hf_hub::api::sync::Api::new().context("Failed to create HF Hub API")?;
    let repo = api.model(MODEL_REPO.to_string());
    let path = repo
        .get(TOKENIZER_FILE)
        .context("Failed to download tokenizer")?;
    Ok(path.to_string_lossy().into_owned())
}
