//! Ranking must be answerable (ADV-GATEWAY-016).
//!
//! ADV-CORE-008 shipped a real fix and could not finish its own diagnosis:
//! five of six golden queries kept returning the same unrelated record first,
//! and there was no way to ask the deployed system why. Recall returned
//! records and nothing else — no distance, no per-factor score, and no sign
//! that the gate had excluded anything.
//!
//! These tests hold the three properties that make the answer trustworthy:
//! the explanation matches the ordering it claims to explain, records the
//! gate excluded are visible, and asking does not change the answer.

mod common;

use common::{ADA, chain, store};
use totem_core::{MemoryCategory, Scope};
use totem_store::{DeterministicEmbedder, RecallQuery, embed};

/// A probe vector for `text`, in the same space the corpus is embedded in.
fn probe(text: &str) -> Vec<f32> {
    embed(
        &DeterministicEmbedder::new(),
        totem_core::Content::new(text),
    )
    .expect("the deterministic embedder produces a correctly-sized vector")
    .embedding
    .expect("embed always attaches a vector")
}

/// A store holding `bodies` as Knowledge at Ada's actor scope.
async fn store_with(bodies: &[&str]) -> totem_store::Store<surrealdb::engine::local::Db> {
    let store = store().await;
    for body in bodies {
        let mut record = common::memory(
            MemoryCategory::Knowledge,
            Scope::Actor(common::actor(ADA)),
            body,
        );
        record.content =
            embed(&DeterministicEmbedder::new(), record.content).expect("embedding succeeds");
        store
            .memories()
            .save(&chain(ADA), &record)
            .await
            .expect("save succeeds");
    }
    store
}

const MATCH: &str = "the gateway owns the embedded store exclusively";
const OTHER: &str = "phase-eleven shipped the console sign-in flow";

#[tokio::test]
async fn the_explanation_matches_the_ordering_it_explains() {
    // The property that makes the whole feature trustworthy. An explanation
    // derived from a second code path would agree with the ranking right up
    // until the moment somebody needed it to.
    let store = store_with(&[MATCH, OTHER]).await;
    let query = RecallQuery::new()
        .near(probe(MATCH))
        .expect("probe is the right width")
        .top_k(10);

    let explained = store
        .memories()
        .explain_ranking(&chain(ADA), &query)
        .await
        .expect("explaining succeeds");
    let recalled = store
        .memories()
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");

    let explained_order: Vec<&str> = explained
        .iter()
        .filter(|entry| entry.included)
        .map(|entry| entry.record.content.body.as_str())
        .collect();
    let recalled_order: Vec<&str> = recalled
        .iter()
        .map(|record| record.content.body.as_str())
        .collect();

    assert_eq!(
        explained_order, recalled_order,
        "the explanation must describe the ordering recall actually produced"
    );

    for entry in &explained {
        let recomputed = entry.score.relevance
            * entry.score.value
            * entry.score.currency
            * entry.score.category_weight;
        assert!(
            (recomputed - entry.score.combined).abs() < 1e-6,
            "the itemised factors must multiply to the score the ordering used: {:?}",
            entry.score
        );
    }
}

#[tokio::test]
async fn a_gated_record_is_absent_from_results_and_present_in_the_explanation() {
    // The question this feature exists to answer is "why is the record I
    // expected missing?", and a gated record is missing by definition. An
    // explanation that omitted them could not answer it.
    let store = store_with(&[MATCH]).await;

    // A record whose embedding is the *negation* of the probe: cosine
    // distance 2.0, the far end of the range and unambiguously past the gate.
    //
    // Written by hand rather than by choosing unrelated-sounding words,
    // because that does not work. Measured here: two texts sharing no
    // subject — "the gateway owns the embedded store exclusively" against
    // "phase-eleven shipped the console sign-in flow" — sit at distance
    // **0.767**, comfortably inside a gate set at orthogonality. The real
    // embedder behaves the same way (ADV-CORE-008 measured an unrelated
    // record at 0.824 on the deployment). Prose does not reach orthogonality,
    // so a test that expects it would be asserting something about English
    // rather than about the gate.
    let mut antipode = common::memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "a record pointing the opposite way in the vector space",
    );
    antipode.content.embedding = Some(probe(MATCH).iter().map(|component| -component).collect());
    store
        .memories()
        .save(&chain(ADA), &antipode)
        .await
        .expect("save succeeds");

    let query = RecallQuery::new()
        .near(probe(MATCH))
        .expect("probe is the right width")
        .top_k(10);

    let recalled = store
        .memories()
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");
    let explained = store
        .memories()
        .explain_ranking(&chain(ADA), &query)
        .await
        .expect("explaining succeeds");

    let gated: Vec<&totem_store::RankExplanation> = explained
        .iter()
        .filter(|entry| entry.score.gated_out())
        .collect();
    assert!(
        !gated.is_empty(),
        "an unrelated record should fail the gate against an exact-match probe; \
         distances were {:?}",
        explained
            .iter()
            .map(|e| (e.record.content.body.as_str(), e.score.distance))
            .collect::<Vec<_>>()
    );

    for entry in gated {
        assert!(
            !entry.included,
            "a gated record cannot be reported as returned: {:?}",
            entry.score
        );
        assert!(
            !recalled
                .iter()
                .any(|record| record.content.body == entry.record.content.body),
            "a gated record must be absent from recall's results"
        );
        assert_eq!(
            entry.score.combined, 0.0,
            "the gate zeroes the whole score, and the explanation must say so"
        );
    }
}

#[tokio::test]
async fn explaining_a_ranking_does_not_reinforce_it() {
    // Asking why must not change the answer. `recall` reinforces what it
    // returns, so explaining *through* it would move the very economics being
    // explained — which is exactly what contaminated ADV-CORE-008's
    // before/after measurement on the deployment.
    let store = store_with(&[MATCH, OTHER]).await;
    let query = RecallQuery::new()
        .near(probe(MATCH))
        .expect("probe is the right width")
        .top_k(10);

    for _ in 0..3 {
        store
            .memories()
            .explain_ranking(&chain(ADA), &query)
            .await
            .expect("explaining succeeds");
    }

    // Asserted on the stored rows, not on the response: a response could
    // report pristine economics while the write had already happened.
    let stored = store
        .memories()
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("a plain listing succeeds");
    for record in &stored {
        assert_eq!(
            record.economics.use_count, 0,
            "explaining must not meter a use: {}",
            record.content.body
        );
        assert!(
            record.economics.last_used_at.is_none(),
            "explaining must not stamp last_used_at: {}",
            record.content.body
        );
    }
}

#[tokio::test]
async fn an_observing_recall_returns_the_same_records_and_meters_none_of_them() {
    // The value loop must survive this: an observing read is the *same read*,
    // not a weaker one. Only the metering differs (ADV-GATEWAY-017).
    let store = store_with(&[MATCH, OTHER]).await;
    let query = RecallQuery::new();

    let observed = store
        .memories()
        .recall_observing(&chain(ADA), &query)
        .await
        .expect("observing succeeds");
    assert_eq!(
        observed.len(),
        2,
        "an observing read returns everything a metered one would"
    );

    // Asserted on the stored rows: a response could report pristine economics
    // after the write had already happened.
    let after = store
        .memories()
        .recall_observing(&chain(ADA), &query)
        .await
        .expect("observing succeeds");
    for record in &after {
        assert_eq!(
            record.economics.use_count, 0,
            "observing must not meter a use: {}",
            record.content.body
        );
        assert!(
            record.economics.last_used_at.is_none(),
            "observing must not stamp last_used_at: {}",
            record.content.body
        );
    }

    // And the ordinary path still meters, so this advance cannot have turned
    // the value loop off by accident.
    store
        .memories()
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");
    let metered = store
        .memories()
        .recall_observing(&chain(ADA), &query)
        .await
        .expect("observing succeeds");
    assert!(
        metered.iter().all(|record| record.economics.use_count == 1),
        "an ordinary recall must still reinforce"
    );
}
