//! ADV-STORE-002 — the production embedder: BGE-small-en-v1.5 via
//! `fastembed`.
//!
//! Off-by-default (`fastembed` cargo feature): first use downloads model
//! weights, which this environment's egress policy blocks
//! (EMB-002/EMB-003). Verified on a workstation instead, the same split
//! ADV-STORE-006/007 used for SurrealDB server-mode parity and the model
//! itself — this module is **not** sandbox-verified, and no test in this
//! crate's default build exercises it.

use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::error::{StoreError, StoreResult};

use super::Embedder;

/// BGE-small-en-v1.5 over ONNX, loaded once at construction.
///
/// Per docs/tech-direction/embeddings.md (EMB-004), cold construction is
/// ~276s (one-time weight download) versus ~124ms warm, so callers must
/// build this once at service startup and hold it — never per request.
pub struct FastembedEmbedder {
    model: Mutex<TextEmbedding>,
}

impl std::fmt::Debug for FastembedEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `TextEmbedding` holds an ONNX session with no `Debug` of its own.
        f.debug_struct("FastembedEmbedder").finish_non_exhaustive()
    }
}

impl FastembedEmbedder {
    /// Load the model, downloading its weights on a cold cache.
    pub fn try_new() -> Result<Self, fastembed::Error> {
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl Embedder for FastembedEmbedder {
    fn model_name(&self) -> &'static str {
        "fastembed-bge-small-en-v1.5"
    }

    fn embed(&self, text: &str) -> StoreResult<Vec<f32>> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| StoreError::Embedding("embedding model lock is poisoned".to_string()))?;
        let mut vectors = model
            .embed([text], None)
            .map_err(|error| StoreError::Embedding(error.to_string()))?;
        vectors.pop().ok_or_else(|| {
            StoreError::Embedding("model returned no embedding for the input".to_string())
        })
    }
}
