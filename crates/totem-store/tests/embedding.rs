//! ADV-STORE-002 — embedding generation attached to memory, and vector
//! similarity search over *generated* vectors rather than hand-crafted ones.
//!
//! Every recall test written before this advance builds its embeddings by
//! hand (`common::unit_vector`, an axis-aligned probe). These tests exercise
//! the actual pipeline instead: text in, an `EMBEDDING_DIMENSIONS`-wide vector
//! out, attached to `Content`, saved, and recalled by similarity — using
//! `DeterministicEmbedder`, the non-semantic stand-in this sandbox's tests run
//! against because the production model (`fastembed`/BGE-small-en-v1.5) needs
//! a model-weight download this environment's egress policy blocks
//! (EMB-002/EMB-003; see `src/fastembed_embedder.rs`, verified on a
//! workstation instead — not exercised here).

mod common;

use common::{ADA, chain, memory, repo, store};
use totem_core::{Content, MemoryCategory, Scope};
use totem_store::{
    DeterministicEmbedder, EMBEDDING_DIMENSIONS, Embedder, RecallQuery, StoreError, embed,
};

#[test]
fn the_deterministic_embedder_attaches_a_vector_of_the_pinned_dimension() {
    let embedder = DeterministicEmbedder::new();
    let content = embed(&embedder, Content::new("a memory body")).expect("embed succeeds");
    assert_eq!(
        content.embedding.as_ref().map(Vec::len),
        Some(EMBEDDING_DIMENSIONS),
    );
}

#[test]
fn embedding_leaves_body_and_tags_untouched() {
    let embedder = DeterministicEmbedder::new();
    let content = Content::new("a memory body").with_tags(["tag-a", "tag-b"]);
    let embedded = embed(&embedder, content).expect("embed succeeds");
    assert_eq!(embedded.body, "a memory body");
    assert_eq!(
        embedded.tags,
        vec!["tag-a".to_string(), "tag-b".to_string()]
    );
}

#[test]
fn the_deterministic_embedder_is_deterministic() {
    let embedder = DeterministicEmbedder::new();
    assert_eq!(
        embedder.embed("the same text twice"),
        embedder.embed("the same text twice"),
    );
}

#[test]
fn different_texts_produce_different_vectors() {
    let embedder = DeterministicEmbedder::new();
    assert_ne!(
        embedder.embed("alpha concerns pnpm and javascript tooling"),
        embedder.embed("a release train ships every other tuesday"),
    );
}

#[test]
fn embed_refuses_an_embedder_that_returns_the_wrong_dimension() {
    struct FourDimensionEmbedder;
    impl Embedder for FourDimensionEmbedder {
        fn model_name(&self) -> &'static str {
            "four-dimension-test-double"
        }
        fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.0, 1.0, 0.0, 0.0]
        }
    }

    let result = embed(&FourDimensionEmbedder, Content::new("body"));
    assert!(
        matches!(
            result,
            Err(StoreError::EmbeddingDimensions {
                expected: EMBEDDING_DIMENSIONS,
                actual: 4
            })
        ),
        "expected a dimension refusal, got {result:?}",
    );
}

/// The end-to-end proof the advance's Planned Implementation Tasks ask for:
/// embedding generation, the vector index, and recall integration, exercised
/// together rather than each in isolation.
#[tokio::test]
async fn recall_ranks_generated_embeddings_by_similarity() {
    let store = store().await;
    let memories = store.memories();
    let embedder = DeterministicEmbedder::new();

    let mut close = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "the team prefers pnpm over npm for javascript projects",
    );
    close.content = embed(&embedder, close.content).expect("embed succeeds");

    let mut far = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "the release train ships every other tuesday afternoon",
    );
    far.content = embed(&embedder, far.content).expect("embed succeeds");

    memories.save(&chain(ADA), &close).await.expect("write");
    memories.save(&chain(ADA), &far).await.expect("write");

    let probe = embedder.embed("should javascript projects use npm or pnpm");
    let query = RecallQuery::new()
        .near(probe)
        .expect("the probe matches the pinned dimension")
        .top_k(1);

    let recalled = memories
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");
    assert_eq!(
        recalled.first().map(|record| record.content.body.as_str()),
        Some(close.content.body.as_str()),
        "the lexically-closer record did not rank first",
    );
}
