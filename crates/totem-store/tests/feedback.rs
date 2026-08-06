//! Explicit feedback signals applied to a record's economics
//! (ADV-GATEWAY-004 gap-fill): the input side of the value loop
//! `tests/value_scoring.rs`'s automatic citation-boost and
//! usage-reinforcement signals feed alongside.

mod common;

use common::{ADA, GRACE, chain, memory, store};
use totem_core::{FeedbackSignal, LifecycleError, MemoryCategory, MemoryId, Scope};
use totem_store::StoreError;

#[tokio::test]
async fn a_used_signal_raises_value_score_and_leaves_currency_untouched() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "the store filters above the query",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let updated = memories
        .apply_feedback(&chain(ADA), note.id, FeedbackSignal::Used)
        .await
        .expect("feedback applies");
    assert!(updated.economics.value_score > note.economics.value_score);
    assert_eq!(updated.economics.currency, note.economics.currency);

    let stored = memories
        .get(&chain(ADA), note.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.economics.value_score, updated.economics.value_score);
}

#[tokio::test]
async fn a_wrong_signal_lowers_value_score() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "a claim that turned out to be incorrect",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let updated = memories
        .apply_feedback(&chain(ADA), note.id, FeedbackSignal::Wrong)
        .await
        .expect("feedback applies");
    assert!(updated.economics.value_score < note.economics.value_score);
}

#[tokio::test]
async fn a_stale_signal_zeroes_currency() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Context,
        Scope::Project(common::repo()),
        "what is going on right now",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let updated = memories
        .apply_feedback(&chain(ADA), note.id, FeedbackSignal::Stale)
        .await
        .expect("feedback applies");
    assert_eq!(updated.economics.currency, 0.0);
    assert_eq!(updated.economics.value_score, note.economics.value_score);
}

#[tokio::test]
async fn feedback_on_an_episodic_record_is_refused() {
    let store = store().await;
    let memories = store.memories();

    let episode = memory(
        MemoryCategory::Episodic,
        Scope::Project(common::repo()),
        "the turn exactly as it happened",
    );
    memories.save(&chain(ADA), &episode).await.expect("write");

    let refused = memories
        .apply_feedback(&chain(ADA), episode.id, FeedbackSignal::Used)
        .await;
    assert!(
        matches!(
            refused,
            Err(StoreError::Lifecycle(LifecycleError::AppendOnly(
                MemoryCategory::Episodic
            )))
        ),
        "expected an append-only refusal, got {refused:?}",
    );
}

#[tokio::test]
async fn feedback_on_a_record_outside_the_callers_chain_reads_as_not_found() {
    let store = store().await;
    let memories = store.memories();

    let graces = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's working note",
    );
    memories.save(&chain(GRACE), &graces).await.expect("write");

    let refused = memories
        .apply_feedback(&chain(ADA), graces.id, FeedbackSignal::Used)
        .await;
    assert!(
        matches!(refused, Err(StoreError::NotFound(_))),
        "expected the record to read as absent, got {refused:?}",
    );
}

#[tokio::test]
async fn feedback_on_an_unknown_id_is_not_found() {
    let store = store().await;
    let memories = store.memories();

    let refused = memories
        .apply_feedback(&chain(ADA), MemoryId::new(), FeedbackSignal::Used)
        .await;
    assert!(matches!(refused, Err(StoreError::NotFound(_))));
}
