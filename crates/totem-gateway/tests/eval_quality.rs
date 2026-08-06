//! Sensitivity proof for the recall-quality scorer (ADV-GATEWAY-008): the
//! golden query set's own reader (the positive control) scores near-perfect,
//! and a reader with no visibility into the corpus's project scope (the
//! negative control) scores near zero — proving the scorer is sensitive to
//! who is actually asking, not a fixed number regardless of input.

use totem_core::{ActorId, ScopeChain};
use totem_gateway::eval::quality::score_recall_quality;
use totem_store::Store;
use totem_store::corpus;

async fn seeded_store() -> Store<surrealdb::engine::local::Db> {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    corpus::seed(&store).await.expect("corpus seeds");
    store
}

#[tokio::test]
async fn golden_readers_score_near_perfect() {
    let store = seeded_store().await;

    let report = score_recall_quality(&store, None)
        .await
        .expect("scoring succeeds");

    assert_eq!(report.precision_at_1, Some(1.0));
    assert_eq!(report.recall_at_k, Some(1.0));
    assert_eq!(report.queries.len(), corpus::golden_queries().len());
}

#[tokio::test]
async fn a_reader_outside_the_corpus_scores_near_zero() {
    let store = seeded_store().await;
    let outsider = ScopeChain::resolve(
        &ActorId::new(corpus::JUNIPER).expect("valid actor id"),
        None,
        &[],
    );

    let report = score_recall_quality(&store, Some(&outsider))
        .await
        .expect("scoring succeeds");

    assert_eq!(report.precision_at_1, Some(0.0));
    assert_eq!(report.recall_at_k, Some(0.0));
}
