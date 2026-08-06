//! Resolving a pending human-gated review, and the queue it answers
//! (ADV-CONSOLE-002: the Uncertainty queue's resolution step).

mod common;

use common::{ADA, GRACE, chain, memory, store};
use totem_core::{GovernanceError, MemoryCategory, MemoryId, ReviewState, Scope};
use totem_store::StoreError;

#[tokio::test]
async fn pending_review_lists_only_that_categorys_open_reviews_oldest_first() {
    let store = store().await;
    let memories = store.memories();

    let mut first = memory(
        MemoryCategory::Uncertainty,
        Scope::Project(common::repo()),
        "claim one contradicts claim two",
    );
    first.provenance.created_at = common::at("2026-08-06T06:00:00Z");
    let mut second = memory(
        MemoryCategory::Uncertainty,
        Scope::Project(common::repo()),
        "claim three contradicts claim four",
    );
    second.provenance.created_at = common::at("2026-08-06T06:05:00Z");
    let knowledge = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "not a contested claim",
    );

    // Written out of order, so the ordering assertion cannot pass by accident
    // of insertion order.
    memories.save(&chain(ADA), &second).await.expect("write");
    memories.save(&chain(ADA), &first).await.expect("write");
    memories.save(&chain(ADA), &knowledge).await.expect("write");

    let pending = memories
        .pending_review(&chain(ADA), MemoryCategory::Uncertainty)
        .await
        .expect("pending_review succeeds");

    assert_eq!(pending.iter().map(|r| r.id).collect::<Vec<_>>(), vec![
        first.id, second.id
    ]);
}

#[tokio::test]
async fn pending_review_is_scoped_to_the_readers_chain() {
    let store = store().await;
    let memories = store.memories();

    let graces = memory(
        MemoryCategory::Uncertainty,
        Scope::Actor(common::actor(GRACE)),
        "grace's own contradiction",
    );
    memories.save(&chain(GRACE), &graces).await.expect("write");

    let pending = memories
        .pending_review(&chain(ADA), MemoryCategory::Uncertainty)
        .await
        .expect("pending_review succeeds");
    assert!(
        pending.is_empty(),
        "ada's chain should not see grace's private Uncertainty record: {pending:?}"
    );
}

#[tokio::test]
async fn resolving_a_pending_review_removes_it_from_the_queue() {
    let store = store().await;
    let memories = store.memories();

    let contested = memory(
        MemoryCategory::Uncertainty,
        Scope::Project(common::repo()),
        "claim one contradicts claim two",
    );
    memories.save(&chain(ADA), &contested).await.expect("write");

    let resolved = memories
        .resolve_review(&chain(ADA), contested.id, ReviewState::Approved)
        .await
        .expect("resolution applies");
    assert_eq!(resolved.governance.review, ReviewState::Approved);

    let pending = memories
        .pending_review(&chain(ADA), MemoryCategory::Uncertainty)
        .await
        .expect("pending_review succeeds");
    assert!(pending.is_empty(), "resolved record still queued: {pending:?}");

    let stored = memories
        .get(&chain(ADA), contested.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.governance.review, ReviewState::Approved);
}

#[tokio::test]
async fn a_review_cannot_be_resolved_twice() {
    let store = store().await;
    let memories = store.memories();

    let contested = memory(
        MemoryCategory::Uncertainty,
        Scope::Project(common::repo()),
        "claim one contradicts claim two",
    );
    memories.save(&chain(ADA), &contested).await.expect("write");

    memories
        .resolve_review(&chain(ADA), contested.id, ReviewState::Rejected)
        .await
        .expect("first resolution applies");

    let second = memories
        .resolve_review(&chain(ADA), contested.id, ReviewState::Approved)
        .await;
    assert!(
        matches!(
            second,
            Err(StoreError::Governance(GovernanceError::NotPending(
                ReviewState::Rejected
            )))
        ),
        "expected a second decision to be refused, got {second:?}",
    );
}

#[tokio::test]
async fn resolving_a_review_that_needs_no_human_gate_is_refused() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "knowledge is not human-gated",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let refused = memories
        .resolve_review(&chain(ADA), note.id, ReviewState::Approved)
        .await;
    assert!(
        matches!(
            refused,
            Err(StoreError::Governance(GovernanceError::NotPending(
                ReviewState::NotRequired
            )))
        ),
        "expected a not-pending refusal, got {refused:?}",
    );
}

#[tokio::test]
async fn resolving_to_a_non_decision_state_is_refused_before_any_write() {
    let store = store().await;
    let memories = store.memories();

    let contested = memory(
        MemoryCategory::Uncertainty,
        Scope::Project(common::repo()),
        "claim one contradicts claim two",
    );
    memories.save(&chain(ADA), &contested).await.expect("write");

    let refused = memories
        .resolve_review(&chain(ADA), contested.id, ReviewState::Pending)
        .await;
    assert!(
        matches!(
            refused,
            Err(StoreError::Governance(GovernanceError::NotADecision(
                ReviewState::Pending
            )))
        ),
        "expected a not-a-decision refusal, got {refused:?}",
    );

    // Still open: the refused attempt above did not consume the review.
    let resolved = memories
        .resolve_review(&chain(ADA), contested.id, ReviewState::Approved)
        .await
        .expect("the review is still pending");
    assert_eq!(resolved.governance.review, ReviewState::Approved);
}

#[tokio::test]
async fn resolving_a_record_outside_the_resolvers_chain_reads_as_not_found() {
    let store = store().await;
    let memories = store.memories();

    let graces = memory(
        MemoryCategory::Uncertainty,
        Scope::Actor(common::actor(GRACE)),
        "grace's own contradiction",
    );
    memories.save(&chain(GRACE), &graces).await.expect("write");

    let refused = memories
        .resolve_review(&chain(ADA), graces.id, ReviewState::Approved)
        .await;
    assert!(
        matches!(refused, Err(StoreError::NotFound(_))),
        "expected the record to read as absent, got {refused:?}",
    );
}

#[tokio::test]
async fn resolving_an_unknown_id_is_not_found() {
    let store = store().await;
    let memories = store.memories();

    let refused = memories
        .resolve_review(&chain(ADA), MemoryId::new(), ReviewState::Approved)
        .await;
    assert!(matches!(refused, Err(StoreError::NotFound(_))));
}
