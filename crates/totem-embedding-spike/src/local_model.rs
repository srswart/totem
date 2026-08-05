//! ADV-STORE-007 — the recommended provider (EMB-003's shape), actually run.
//!
//! `fastembed` downloads its model weights to a local cache on first use and
//! then runs fully offline. That first-use download is exactly what the cloud
//! sandbox egress policy blocks (EMB-002), so everything here sits behind the
//! off-by-default `local-model` feature and is executed on a host that can
//! reach the model hub — the same split ADV-STORE-006 used for the SurrealDB
//! server. With the feature off, this module compiles to nothing.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::{EmbeddingProvider, l2_normalize};

/// BGE-small-en-v1.5's output dimensionality. ADV-STORE-001's HNSW index pin
/// (`DIMENSION 384`) is derived from this value; if the model changes, both
/// change together and every stored vector must be re-embedded (curator's
/// job, per docs/tech-direction/embeddings.md §4).
pub const MODEL_DIMENSIONS: usize = 384;

/// The recommended candidate: BGE-small-en-v1.5 running locally over ONNX.
///
/// `fastembed`'s `embed` takes `&mut self`, while [`EmbeddingProvider`] is a
/// shared-reference trait — hence the `Mutex`. Contention is irrelevant here:
/// this is a measurement harness, not production code.
pub struct FastembedProvider {
    model: Mutex<TextEmbedding>,
    /// How long construction took — model load, plus the one-time weight
    /// download when the cache is cold. Reported separately from per-call
    /// latency because only the latter matters at gateway-write time.
    pub load_time: Duration,
}

impl std::fmt::Debug for FastembedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `TextEmbedding` holds an ONNX session with no Debug of its own.
        f.debug_struct("FastembedProvider")
            .field("load_time", &self.load_time)
            .finish_non_exhaustive()
    }
}

impl FastembedProvider {
    /// Loads (downloading on a cold cache) BGE-small-en-v1.5.
    pub fn try_new() -> Result<Self, fastembed::Error> {
        let started = Instant::now();
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))?;
        Ok(Self {
            model: Mutex::new(model),
            load_time: started.elapsed(),
        })
    }
}

impl EmbeddingProvider for FastembedProvider {
    fn name(&self) -> &'static str {
        "local-fastembed-bge-small-en-v1.5"
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = self
            .model
            .lock()
            .expect("no panic can poison this lock mid-embed")
            .embed([text], None)
            .expect("local inference on an in-memory model does not fail")
            .pop()
            .expect("one input text yields one embedding");
        // The comparison harness's `cosine` assumes L2-normalized inputs, as
        // documented there. Normalizing is idempotent if the model already
        // normalized its output.
        l2_normalize(&mut vector);
        vector
    }
}
