use anyhow::Result;
use tokenizers::Tokenizer;

use super::ModelConfig;

const QUERY_PREFIX: &str = "[Q] ";
const DOCUMENT_PREFIX: &str = "[D] ";

/// Wrapper around HuggingFace tokenizer.
/// Pre-configured with truncation/padding to avoid cloning per batch.
pub struct TokenizerWrapper {
    doc_tokenizer: Tokenizer,
    query_tokenizer: Tokenizer,
}

impl TokenizerWrapper {
    pub fn new(tokenizer_path: &str, config: &ModelConfig) -> Result<Self> {
        // `og build` already uses Rayon for extraction. Tokenizers also uses the
        // global Rayon pool for padding, which can deadlock if extractor workers
        // fill the bounded channel while the consumer waits on tokenizer jobs.
        tokenizers::parallelism::set_parallelism(false);

        let base = Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut doc_tokenizer = base.clone();
        doc_tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: config.doc_max_length,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        doc_tokenizer.with_padding(Some(tokenizers::PaddingParams::default()));

        let mut query_tokenizer = base;
        query_tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: config.query_max_length,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        query_tokenizer.with_padding(Some(tokenizers::PaddingParams::default()));

        Ok(Self {
            doc_tokenizer,
            query_tokenizer,
        })
    }

    /// Encode texts for document embedding.
    pub fn encode_documents(&self, texts: &[&str]) -> Result<Vec<tokenizers::Encoding>> {
        texts
            .iter()
            .map(|text| {
                let prepared = format!("{DOCUMENT_PREFIX}{text}");
                self.doc_tokenizer
                    .encode(tokenizers::EncodeInput::Single(prepared.into()), true)
                    .map_err(|e| anyhow::anyhow!("{e}"))
            })
            .collect()
    }

    /// Encode a query (shorter max length).
    pub fn encode_query(&self, text: &str) -> Result<tokenizers::Encoding> {
        let prepared = format!("{QUERY_PREFIX}{text}");
        self.query_tokenizer
            .encode(prepared, true)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{DOCUMENT_PREFIX, QUERY_PREFIX};

    #[test]
    fn model_prefixes_are_distinct_and_include_spacing() {
        assert_eq!(QUERY_PREFIX, "[Q] ");
        assert_eq!(DOCUMENT_PREFIX, "[D] ");
        assert_ne!(QUERY_PREFIX, DOCUMENT_PREFIX);
    }
}
