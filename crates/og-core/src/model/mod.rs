//! Embedder boundary. All model identity flows through the manifest:
//! a catalog built with a different model id is rejected (force rebuild).

pub mod potion;

use anyhow::Result;

/// A text embedding model.
///
/// Implementations must be deterministic for a given model id: the same
/// input text must always produce the same vector, or generation-qualified
/// rebuilds will silently mix vector spaces.
pub trait Embedder: Send + Sync {
    /// Stable identity string pinned in the manifest (e.g. "deterministic-v1",
    /// "minishlab/potion-code-16M-v2@<weights-hash>").
    fn id(&self) -> &str;

    /// Vector dimensionality.
    fn dims(&self) -> usize;

    /// Embed a batch of texts. Output len must equal input len.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

/// Construct the embedder named by a manifest model_id. Identity pins the
/// weights: a cached model whose hash differs from the manifest errors
/// (rebuild required), never silently re-embeds into the old space.
pub fn embedder_for(model_id: &str) -> Result<Box<dyn Embedder>> {
    if model_id == DeterministicEmbedder::ID {
        return Ok(Box::new(DeterministicEmbedder::default()));
    }
    if model_id.starts_with(potion::DEFAULT_REPO) {
        let p = potion::PotionEmbedder::load_default()?;
        anyhow::ensure!(
            p.id() == model_id,
            "cached model identity {} != manifest {} — weights changed, rebuild required",
            p.id(),
            model_id
        );
        return Ok(Box::new(p));
    }
    anyhow::bail!("unknown model id: {model_id}")
}

/// Default build embedder: potion (the quality default). No silent
/// fallback — callers decide policy via `--deterministic` or `og model install`.
pub fn default_embedder() -> Result<Box<dyn Embedder>> {
    Ok(Box::new(potion::PotionEmbedder::load_default()?))
}

/// Deterministic test embedder for the vertical slice (tk-4a8q).
///
/// Blake3-based counter-mode expansion: each text hashes to a 256-d
/// unit vector. Same text in, same vector out — enough to exercise the
/// vector channel, exact scan, and RRF fusion end-to-end without a model.
/// It has no semantic signal; relevance comes from BM25 + boosts in the
/// slice. Replaced by the static Model2Vec embedder in tk-7wp8.
pub struct DeterministicEmbedder {
    dims: usize,
}

impl DeterministicEmbedder {
    pub const ID: &'static str = "deterministic-v1";

    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
}

impl Default for DeterministicEmbedder {
    fn default() -> Self {
        Self::new(256)
    }
}

impl Embedder for DeterministicEmbedder {
    fn id(&self) -> &str {
        Self::ID
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| embed_deterministic(text, self.dims))
            .collect())
    }
}

/// Hash a text to a unit-normalized f32 vector of `dims` dimensions.
fn embed_deterministic(text: &str, dims: usize) -> Vec<f32> {
    // Counter mode: blake3(text || counter) blocks expand into f32 lanes.
    let mut hasher = blake3::Hasher::new();
    hasher.update(text.as_bytes());

    let mut out = vec![0.0f32; dims];
    let mut filled = 0;

    // Each blake3 block yields 8 u32 values (as 8 f32 lanes via xorshift mix).
    let mut counter: u32 = 0;
    while filled < dims {
        let mut h = hasher.clone();
        h.update(&counter.to_le_bytes());
        let digest = h.finalize();
        for word in digest.as_bytes().chunks_exact(4) {
            if filled >= dims {
                break;
            }
            let bits = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            // Map u32 to (-1, 1), avoiding zero-division in normalize.
            out[filled] = (bits as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32;
            filled += 1;
        }
        counter += 1;
    }

    // L2 normalize so dot product == cosine similarity.
    let norm = out
        .iter()
        .map(|v| (*v as f64) * (*v as f64))
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for v in &mut out {
            *v = (*v as f64 / norm) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_normalized() {
        let a = embed_deterministic("fn main() {}", 256);
        let b = embed_deterministic("fn main() {}", 256);
        assert_eq!(a, b);

        let norm: f64 = a.iter().map(|v| (*v as f64) * (*v as f64)).sum();
        assert!((norm - 1.0).abs() < 1e-4, "not unit length: {norm}");
    }

    #[test]
    fn different_texts_differ() {
        let a = embed_deterministic("alpha", 256);
        let b = embed_deterministic("beta", 256);
        assert_ne!(a, b);

        // Distinct texts should not be collinear.
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(dot.abs() < 0.9, "unexpectedly similar: {dot}");
    }

    #[test]
    fn trait_plumbing() {
        let e = DeterministicEmbedder::default();
        assert_eq!(e.id(), "deterministic-v1");
        assert_eq!(e.dims(), 256);
        let out = e.embed(&["x", "y"]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 256);
    }
}
