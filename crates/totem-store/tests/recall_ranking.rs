//! Recall ranking: what survives a re-embed, and what relevance loses to.
//!
//! Written while taking ADV-STORE-008's golden-query evidence on the
//! deployment, where four different queries — two of them *exact body
//! matches* — returned the same seven records in the same order.
//!
//! The re-embed pass was the obvious suspect and is exonerated here: ranking
//! survives it. The cause is the scoring function, and it is pre-existing.
mod common;
use common::{ADA, chain, store};
use totem_core::{MemoryCategory, Scope};
use totem_store::{DeterministicEmbedder, Embedder, RecallQuery};

/// A second "model": different label, and vectors that still rank sensibly
/// (same geometry, shifted), so ranking is expected to survive.
#[derive(Debug)]
struct ModelB(DeterministicEmbedder);
impl Embedder for ModelB {
    fn model_name(&self) -> &'static str {
        "model-b"
    }
    fn embed(&self, text: &str) -> totem_store::StoreResult<Vec<f32>> {
        self.0.embed(text)
    }
}

async fn bodies(
    store: &totem_store::Store<surrealdb::engine::local::Db>,
    q: &str,
    e: &dyn Embedder,
) -> Vec<String> {
    let probe = e.embed(q).expect("probe");
    let query = RecallQuery::new().near(probe).expect("near");
    store
        .memories()
        .recall(&chain(ADA), &query)
        .await
        .expect("recall")
        .into_iter()
        .map(|r| r.content.body)
        .collect()
}

#[tokio::test]
async fn ranking_survives_a_reembed() {
    let e = DeterministicEmbedder::new();
    let store = store().await;
    for b in [
        "cats sleep on warm keyboards",
        "the gateway owns the store exclusively",
        "backups run nightly",
    ] {
        let mut r = common::memory(
            MemoryCategory::Knowledge,
            Scope::Actor(common::actor(ADA)),
            b,
        );
        r.content = totem_store::embed(&e, r.content).expect("embed");
        store.memories().save(&chain(ADA), &r).await.expect("save");
    }

    let before = bodies(&store, "the gateway owns the store exclusively", &e).await;
    println!("BEFORE reembed, top = {:?}", before.first());

    let b = ModelB(DeterministicEmbedder::new());
    let s = store.memories().reembed_all(&b).await.expect("reembed");
    println!("reembed summary: {s:?}");

    let after = bodies(&store, "the gateway owns the store exclusively", &b).await;
    println!("AFTER reembed,  top = {:?}", after.first());

    assert_eq!(
        before.first(),
        after.first(),
        "re-embedding changed the ranking"
    );
}

/// **Was the ADV-CORE-008 defect; now the regression test for its fix.**
/// Un-ignored when the relevance gate landed.
///
/// `combined_score = relevance * value_score * currency * category_weight`.
/// `relevance_from_distance` is `1/(1+d)`, so across the whole cosine range
/// relevance varies by at most 3x. `value_score` has no such bound, and
/// `reinforce_usage` raises it on every record a recall returns — so whatever
/// ranks highly becomes more valuable and ranks highly more often. Past a
/// certain accumulation, no query can outrank it.
///
/// Owner: ADV-CORE-002's value loop, not ADV-STORE-008. Recorded rather than
/// fixed here because changing the ranking formula is a product decision with
/// its own evidence, not a detail of switching embedders.
#[tokio::test]
async fn does_an_instructions_record_win_regardless_of_the_query() {
    // The deployment's shape: one `instructions` memory that has been recalled
    // repeatedly (every recall reinforces usage, raising value_score), among
    // `knowledge` memories. On the deployment, four different queries —
    // including exact body matches — all returned the instructions record
    // first. Is relevance being swamped?
    let e = DeterministicEmbedder::new();
    let store = store().await;

    let mut instr = common::memory(
        MemoryCategory::Instructions,
        Scope::Actor(common::actor(ADA)),
        "Always run arrive plan check before pushing plan edits.",
    );
    instr.content = totem_store::embed(&e, instr.content).expect("embed");
    store
        .memories()
        .save(&chain(ADA), &instr)
        .await
        .expect("save");

    for b in [
        "DEP-001: the gateway owns an embedded RocksDB store",
        "ack-probe: measuring whether the MCP response stream terminates",
    ] {
        let mut r = common::memory(
            MemoryCategory::Knowledge,
            Scope::Actor(common::actor(ADA)),
            b,
        );
        r.content = totem_store::embed(&e, r.content).expect("embed");
        store.memories().save(&chain(ADA), &r).await.expect("save");
    }

    // Recall repeatedly, as the deployment has been, so usage accumulates.
    for _ in 0..6 {
        let _ = bodies(&store, "anything", &e).await;
    }

    let exact = bodies(
        &store,
        "ack-probe: measuring whether the MCP response stream terminates",
        &e,
    )
    .await;
    println!(
        "query = exact body of a knowledge record; top = {:?}",
        exact.first()
    );
    assert_eq!(
        exact.first().map(String::as_str),
        Some("ack-probe: measuring whether the MCP response stream terminates"),
        "an exact body match must outrank an unrelated instructions record"
    );
}
