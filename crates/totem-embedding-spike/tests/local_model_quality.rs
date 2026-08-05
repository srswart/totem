//! ADV-STORE-007 — the measurement ADV-STORE-003 could not perform: the
//! recommended local pretrained model, run against the same corpus, the same
//! labeled queries, and the same harness as the lexical baseline.
//!
//! Opt-in (`local-model` feature): first use downloads BGE-small-en-v1.5's
//! weights, which the cloud sandbox egress policy blocks (EMB-002/EMB-003).
//! Run on a host with hub access:
//!
//! ```text
//! cargo test -p totem-embedding-spike --features local-model -- --nocapture
//! ```
#![cfg(feature = "local-model")]

use std::time::Instant;

use totem_embedding_spike::local_model::{FastembedProvider, MODEL_DIMENSIONS};
use totem_embedding_spike::{
    EmbeddingProvider, HashingEmbedder, corpus, evaluate_retrieval, labeled_queries,
};

/// The claim ADV-STORE-003's recommendation rested on without measurement:
/// a semantic local model retrieves correctly where the lexical baseline
/// does — and where it doesn't (the paraphrased query, EMB-001).
#[test]
fn bge_small_ranks_every_labeled_query_first_including_the_paraphrase() {
    let provider = FastembedProvider::try_new().expect(
        "model load failed — first use downloads weights, which the cloud \
         sandbox blocks (EMB-002); run this on a host with hub access",
    );
    println!(
        "model construction (incl. any cold-cache download): {:?}",
        provider.load_time
    );

    // ADV-STORE-001 pins its HNSW index to DIMENSION 384 on the strength of
    // this assertion.
    let probe = provider.embed("dimension probe");
    assert_eq!(
        probe.len(),
        MODEL_DIMENSIONS,
        "BGE-small-en-v1.5 dimensionality changed; ADV-STORE-001's index pin \
         and every stored vector must change with it"
    );

    let corpus = corpus();
    let queries = labeled_queries();

    // Per-call latency over the corpus — the number that matters for
    // synchronous gateway-write-time embedding.
    let started = Instant::now();
    for memory in &corpus {
        let _ = provider.embed(memory.body);
    }
    let per_call = started.elapsed() / u32::try_from(corpus.len()).expect("small corpus");
    println!(
        "mean per-call embed latency over {} texts: {per_call:?}",
        corpus.len()
    );

    let results = evaluate_retrieval(&provider, &corpus, &queries);
    for result in &results {
        println!(
            "bge-small: {:?} -> top1 {} (expected {}, rank {:?})",
            result.query, result.actual_top1, result.expected, result.rank_of_expected
        );
    }
    for result in &results {
        assert!(
            result.correct,
            "expected {} first for {:?}, got {} (rank {:?})",
            result.expected, result.query, result.actual_top1, result.rank_of_expected
        );
    }

    // Sensitivity control: the identical harness, corpus, and queries make
    // the lexical baseline fail the paraphrased query (EMB-001). If this
    // stops failing, the comparison above has lost its discriminating case.
    let paraphrase = queries
        .iter()
        .find(|q| !q.lexical_overlap)
        .expect("the query set contains one paraphrased case");
    let hashing = HashingEmbedder::new(256);
    let hashing_paraphrase = evaluate_retrieval(&hashing, &corpus, &queries)
        .into_iter()
        .find(|r| r.query == paraphrase.query)
        .expect("harness evaluated every query");
    assert!(
        !hashing_paraphrase.correct,
        "control broken: the lexical baseline now ranks the paraphrase first, \
         so this test no longer demonstrates a semantic-vs-lexical difference"
    );
}
