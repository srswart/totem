//! Real, offline, deterministic experiments against the local hashing-trick
//! candidate. Part of the default `cargo test --workspace` run — unlike
//! the API-reachability probe, this makes no network call.

use totem_embedding_spike::{
    EmbeddingProvider, HashingEmbedder, corpus, evaluate_retrieval, labeled_queries,
};

#[test]
fn hashing_embedder_wins_on_lexical_overlap_queries() {
    let provider = HashingEmbedder::new(256);
    let corpus = corpus();
    let queries: Vec<_> = labeled_queries()
        .into_iter()
        .filter(|q| q.lexical_overlap)
        .collect();

    let results = evaluate_retrieval(&provider, &corpus, &queries);

    for result in &results {
        assert!(
            result.correct,
            "expected top-1 match for {:?} to be {:?}, got {:?} (rank of expected: {:?})",
            result.query, result.expected, result.actual_top1, result.rank_of_expected
        );
    }
}

/// Negative control: a query paraphrased away from its expected match's
/// vocabulary should NOT rank first under a purely lexical embedder. If this
/// assertion starts failing, the corpus lost the vocabulary gap it exists to
/// exercise, and the finding below needs re-checking against a harder query.
#[test]
fn hashing_embedder_loses_on_paraphrased_query_with_no_shared_vocabulary() {
    let provider = HashingEmbedder::new(256);
    let corpus = corpus();
    let queries: Vec<_> = labeled_queries()
        .into_iter()
        .filter(|q| !q.lexical_overlap)
        .collect();
    assert_eq!(
        queries.len(),
        1,
        "expected exactly one paraphrased query in the fixture"
    );

    let results = evaluate_retrieval(&provider, &corpus, &queries);
    let paraphrased = &results[0];

    assert!(
        !paraphrased.correct,
        "expected the purely lexical embedder to MISS the paraphrased query \
         {:?} (it shares no trigram vocabulary with {:?}); it got the top-1 \
         match right instead, which means either the fixture accidentally \
         shares vocabulary or the embedder is doing more than lexical \
         matching — re-examine before trusting the quality finding",
        paraphrased.query, paraphrased.expected
    );
}

#[test]
fn embeddings_are_l2_normalized_so_cosine_is_a_plain_dot_product() {
    let provider = HashingEmbedder::new(64);
    for memory in corpus() {
        let vector = provider.embed(memory.body);
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "{}: expected unit-length embedding, got norm {norm}",
            memory.id
        );
    }
}
