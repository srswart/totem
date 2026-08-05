//! Embedding generation for memory content (ADV-STORE-002).
//!
//! [docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md)
//! pins the production model: BGE-small-en-v1.5 via `fastembed`, 384
//! dimensions, embedded synchronously wherever memory is written. That
//! inference path needs model weights this crate's default test build cannot
//! download (EMB-002/EMB-003), so it lives behind the off-by-default
//! `fastembed` cargo feature (see [`fastembed_embedder`]) and is verified on a
//! workstation, not in this sandbox — nothing in this crate claims otherwise.
//! Every other build, including this crate's own default tests, uses
//! [`DeterministicEmbedder`]: a non-semantic stand-in with the pinned
//! dimensionality, so the attach → save → recall path can be proven
//! end-to-end without claiming a quality result this environment cannot
//! measure.
//!
//! Placement (gateway, on write, per the tech direction) is not wired up
//! here: `totem-gateway` does not exist yet (ADV-GATEWAY-001). [`embed`] is
//! deliberately a plain function over [`Content`], not a method on
//! [`crate::Store`], so a future gateway can call it before `save` without
//! this crate knowing anything about HTTP or MCP.

use totem_core::Content;

use crate::error::{StoreError, StoreResult};
use crate::schema::EMBEDDING_DIMENSIONS;

#[cfg(feature = "fastembed")]
pub mod fastembed_embedder;

/// Turns memory text into the vector the store's HNSW index ranks on.
///
/// Implementations are not required to return [`EMBEDDING_DIMENSIONS`]
/// components themselves — [`embed`] checks that once, in one place, so every
/// caller gets the same [`StoreError::EmbeddingDimensions`] rather than a
/// dimension mismatch surfacing later as a confusing database error.
pub trait Embedder: Send + Sync {
    /// A stable label for provenance and logs: which model produced this.
    fn model_name(&self) -> &'static str;

    /// Embed `text`.
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Generate an embedding for `content.body` and attach it, refusing a
/// dimension mismatch before it can reach the store's own check on `save`.
pub fn embed(embedder: &dyn Embedder, mut content: Content) -> StoreResult<Content> {
    let vector = embedder.embed(&content.body);
    if vector.len() != EMBEDDING_DIMENSIONS {
        return Err(StoreError::EmbeddingDimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: vector.len(),
        });
    }
    content.embedding = Some(vector);
    Ok(content)
}

/// A deterministic, offline, non-semantic embedder.
///
/// Character-trigram hashing, L2-normalized — the same shape ADV-STORE-003's
/// spike measured as EMB-001: real cosine geometry, so this crate's own tests
/// can exercise vector similarity end-to-end, but no semantic quality claim.
/// Real quality is [`fastembed_embedder::FastembedEmbedder`]'s job, measured
/// on a workstation (EMB-004), not this one.
#[derive(Debug, Clone, Copy)]
pub struct DeterministicEmbedder {
    dims: usize,
}

impl DeterministicEmbedder {
    /// An embedder producing vectors of the store's pinned dimension.
    pub fn new() -> Self {
        Self {
            dims: EMBEDDING_DIMENSIONS,
        }
    }
}

impl Default for DeterministicEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for DeterministicEmbedder {
    fn model_name(&self) -> &'static str {
        "deterministic-trigram-hash"
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0f32; self.dims];
        let lower = text.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();

        if chars.len() < 3 {
            bump(&mut vector, fnv1a(&lower));
        } else {
            for window in chars.windows(3) {
                let trigram: String = window.iter().collect();
                bump(&mut vector, fnv1a(&trigram));
            }
        }

        l2_normalize(&mut vector);
        vector
    }
}

fn bump(vector: &mut [f32], hash: u64) {
    let len = vector.len() as u64;
    let index = (hash % len) as usize;
    vector[index] += 1.0;
}

/// FNV-1a: no external dependency needed for a non-semantic hashing trick.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn l2_normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vector.iter_mut() {
            *x /= norm;
        }
    }
}
