//! Re-embedding every stored memory into one vector space (ADV-STORE-008).
//!
//! The deployed gateway has been writing `DeterministicEmbedder` vectors.
//! Switching it to BGE-small-en-v1.5 without re-embedding would leave two
//! incompatible geometries in one HNSW index, and cosine distance between
//! them is not merely worse — it is meaningless. Ranking would still return
//! results, confidently, in an order that means nothing. That is the failure
//! this pass exists to prevent, and the reason it cannot be optional.
//!
//! These tests use the deterministic embedder for both sides: the *model
//! download* is what the sandbox blocks (EMB-002/EMB-003), not the re-embed
//! logic, so the logic is verified here and the real model is verified on the
//! deployed instance by golden query.

use std::sync::Arc;

mod common;

use common::{ADA, chain, store};
use surrealdb::engine::local::Db;
use totem_core::{MemoryCategory, Scope};
use totem_store::{DeterministicEmbedder, Embedder, Store, StoreResult};

/// A stand-in for "a different model" that is still offline.
///
/// Re-embedding is driven by the *label* a row carries, not by the weights,
/// so a second embedder with a different `model_name` exercises the whole
/// path without downloading anything.
#[derive(Debug)]
struct RelabelledEmbedder(DeterministicEmbedder);

impl Embedder for RelabelledEmbedder {
    fn model_name(&self) -> &'static str {
        "test-other-model-v1"
    }

    fn embed(&self, text: &str) -> StoreResult<Vec<f32>> {
        // Deliberately different vectors, not just a different label: a pass
        // that only rewrote labels would look identical from the outside.
        self.0.embed(&format!("other:{text}"))
    }
}

/// A store holding `bodies`, each embedded by `embedder`.
async fn store_with(bodies: &[&str], embedder: &dyn Embedder) -> Store<Db> {
    let store = store().await;
    for body in bodies {
        let mut record = common::memory(
            MemoryCategory::Knowledge,
            Scope::Actor(common::actor(ADA)),
            body,
        );
        record.content = totem_store::embed(embedder, record.content)
            .expect("the deterministic embedder produces a correctly-sized vector");
        store
            .memories()
            .save(&chain(ADA), &record)
            .await
            .expect("save succeeds");
    }
    store
}

#[tokio::test]
async fn reembedding_rewrites_every_row_written_by_a_different_model() {
    let old = DeterministicEmbedder::new();
    let store = store_with(
        &["the gateway owns the store", "phase-011 is in flight"],
        &old,
    )
    .await;

    let before = store
        .memories()
        .embedding_models()
        .await
        .expect("the store reports which models its rows were embedded by");
    assert_eq!(
        before,
        vec![(old.model_name().to_string(), 2)],
        "both rows should be labelled with the embedder that wrote them"
    );

    let new: Arc<dyn Embedder> = Arc::new(RelabelledEmbedder(DeterministicEmbedder::new()));
    let summary = store
        .memories()
        .reembed_all(new.as_ref())
        .await
        .expect("re-embedding succeeds");

    assert_eq!(summary.examined, 2);
    assert_eq!(summary.reembedded, 2);
    assert_eq!(summary.skipped, 0);

    let after = store
        .memories()
        .embedding_models()
        .await
        .expect("the store reports models after the pass");
    assert_eq!(
        after,
        vec![("test-other-model-v1".to_string(), 2)],
        "no row may be left in the old space — a mixed index ranks meaninglessly"
    );
}

#[tokio::test]
async fn reembedding_twice_is_a_no_op_the_second_time() {
    let old = DeterministicEmbedder::new();
    let store = store_with(&["a memory", "another"], &old).await;
    let new: Arc<dyn Embedder> = Arc::new(RelabelledEmbedder(DeterministicEmbedder::new()));

    store
        .memories()
        .reembed_all(new.as_ref())
        .await
        .expect("first pass succeeds");
    let second = store
        .memories()
        .reembed_all(new.as_ref())
        .await
        .expect("second pass succeeds");

    assert_eq!(
        (second.examined, second.reembedded, second.skipped),
        (2, 0, 2),
        "a re-run must skip rows already in the target space: this pass runs \
         on a deployment, and re-embedding is not free"
    );
}

#[tokio::test]
async fn a_row_the_new_model_cannot_embed_leaves_the_pass_honest() {
    // A model that fails on one input must not silently leave that row in the
    // old space while reporting success: the operator would believe the index
    // is uniform when it is not.
    struct FailsOnOne(DeterministicEmbedder);
    impl Embedder for FailsOnOne {
        fn model_name(&self) -> &'static str {
            "test-fails-on-one"
        }
        fn embed(&self, text: &str) -> StoreResult<Vec<f32>> {
            if text.contains("poison") {
                return Err(totem_store::StoreError::Embedding(
                    "refusing this input".to_string(),
                ));
            }
            self.0.embed(text)
        }
    }

    let store = store_with(&["fine", "poison pill"], &DeterministicEmbedder::new()).await;
    let error = store
        .memories()
        .reembed_all(&FailsOnOne(DeterministicEmbedder::new()))
        .await
        .expect_err("a row that cannot be re-embedded must fail the pass");

    assert!(
        matches!(error, totem_store::StoreError::Embedding(_)),
        "the embedding failure must surface as itself, not as a row error: {error:?}"
    );
}
