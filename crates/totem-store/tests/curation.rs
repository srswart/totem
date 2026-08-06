//! Curation at the store layer: supersede, never delete; restore on rollback
//! (ADV-CURATOR-001).
//!
//! The curator is the first writer that touches records it did not author, so
//! every guard here is about what it may reach: the scope predicate is the
//! store's, the retirement is pinned to the exact rows the event names, and a
//! merge that would apply to anything other than those rows applies to nothing
//! at all.

mod common;

use common::{ADA, GRACE, at, chain, memory, store, unit_vector};
use totem_core::{
    ActorId, Author, Content, CurationEventKind, Harness, MemoryCategory, MemoryRecord,
    MemoryStatus, Provenance, Scope, ScopeChain, SessionId,
};
use totem_store::{RecallQuery, Store, StoreError};

fn curator_provenance() -> Provenance {
    Provenance::new(
        Author::Curator(ActorId::new("totem-curator").expect("valid actor id")),
        Harness::Curator,
        SessionId::new("curate-1").expect("valid session id"),
        at("2026-08-06T08:00:00Z"),
    )
}

fn private(id: &str) -> Scope {
    Scope::Actor(ActorId::new(id).expect("valid actor id"))
}

/// A knowledge record, saved, with the body given.
async fn saved(
    store: &Store<surrealdb::engine::local::Db>,
    writer: &ScopeChain,
    scope: Scope,
    body: &str,
) -> MemoryRecord {
    let record = memory(MemoryCategory::Knowledge, scope, body);
    store
        .memories()
        .save(writer, &record)
        .await
        .expect("the record saves");
    record
}

/// A knowledge record written at a chosen time, saved.
async fn written(
    store: &Store<surrealdb::engine::local::Db>,
    writer: &ScopeChain,
    scope: Scope,
    body: &str,
    timestamp: &str,
) -> MemoryRecord {
    let record = common::written_at(MemoryCategory::Knowledge, scope, body, timestamp);
    store
        .memories()
        .save(writer, &record)
        .await
        .expect("the record saves");
    record
}

/// The superseding record a curator would write for `originals`.
fn superseding(originals: &[MemoryRecord], body: &str) -> MemoryRecord {
    let mut record = MemoryRecord::new(
        MemoryCategory::Knowledge,
        originals[0].scope.clone(),
        Content::new(body),
        curator_provenance(),
    );
    record.provenance.derived_from = originals.iter().map(|original| original.id).collect();
    record
}

async fn status_of(
    store: &Store<surrealdb::engine::local::Db>,
    reader: &ScopeChain,
    record: &MemoryRecord,
) -> MemoryStatus {
    store
        .memories()
        .get(reader, record.id)
        .await
        .expect("the read succeeds")
        .expect("the record is still there")
        .governance
        .status
}

#[tokio::test]
async fn a_merge_retires_the_originals_and_leaves_them_readable() {
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let first = saved(
        &store,
        &ada,
        repo_scope.clone(),
        "deploys happen on fridays",
    )
    .await;
    let second = saved(&store, &ada, repo_scope, "deploys happen on fridays.").await;
    let merged = superseding(
        &[first.clone(), second.clone()],
        "deploys happen on fridays",
    );

    let event = store
        .curation()
        .merge(
            &ada,
            &merged,
            &[first.clone(), second.clone()],
            curator_provenance(),
        )
        .await
        .expect("the merge applies");

    assert_eq!(event.kind, CurationEventKind::Merged);
    assert_eq!(event.superseded_ids(), vec![first.id, second.id]);
    // Superseded, not deleted: both originals are still readable, and the
    // lineage from the survivor back to them is on the record itself.
    assert_eq!(status_of(&store, &ada, &first).await, MemoryStatus::Retired);
    assert_eq!(
        status_of(&store, &ada, &second).await,
        MemoryStatus::Retired
    );
    assert_eq!(status_of(&store, &ada, &merged).await, MemoryStatus::Active);
    assert_eq!(
        store
            .memories()
            .get(&ada, merged.id)
            .await
            .expect("the read succeeds")
            .expect("the survivor is there")
            .provenance
            .derived_from,
        vec![first.id, second.id]
    );
}

#[tokio::test]
async fn recall_stops_returning_a_retired_record() {
    // Without this, "supersede" would be a label on a row that still competes
    // for the agent's context window — the duplicate would never go away.
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let first = saved(
        &store,
        &ada,
        repo_scope.clone(),
        "deploys happen on fridays",
    )
    .await;
    let second = saved(&store, &ada, repo_scope, "deploys happen on fridays.").await;
    let merged = superseding(
        &[first.clone(), second.clone()],
        "deploys happen on fridays",
    );

    store
        .curation()
        .merge(
            &ada,
            &merged,
            &[first.clone(), second],
            curator_provenance(),
        )
        .await
        .expect("the merge applies");

    let recalled = store
        .memories()
        .recall(&ada, &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(
        recalled.iter().map(|record| record.id).collect::<Vec<_>>(),
        vec![merged.id],
        "a retired record was still recalled",
    );
}

#[tokio::test]
async fn a_curator_cannot_supersede_a_record_it_cannot_see() {
    // Grace's chain does not reach ada's private scope, so ada's records read
    // as absent — the curator gets the same refusal any other caller would.
    // The survivor is written at the shared project scope, which grace *can*
    // reach: without that, the refusal would come from the write side and this
    // test would never exercise the read side at all.
    let store = store().await;
    let ada = chain(ADA);
    let grace = chain(GRACE);
    let first = saved(&store, &ada, private(ADA), "a private note").await;
    let second = saved(&store, &ada, private(ADA), "a private note!").await;
    let mut merged = superseding(&[first.clone(), second.clone()], "a private note");
    merged.scope = grace.scopes()[1].clone();

    let refused = store
        .curation()
        .merge(
            &grace,
            &merged,
            &[first.clone(), second],
            curator_provenance(),
        )
        .await;

    assert!(
        matches!(refused, Err(StoreError::NotFound(id)) if id == first.id),
        "a curator reached into another actor's scope: {refused:?}",
    );
    assert_eq!(status_of(&store, &ada, &first).await, MemoryStatus::Active);
}

#[tokio::test]
async fn a_curator_cannot_write_the_survivor_into_a_scope_it_cannot_reach() {
    let store = store().await;
    let ada = chain(ADA);
    let first = saved(&store, &ada, private(ADA), "a private note").await;
    let second = saved(&store, &ada, private(ADA), "a private note!").await;
    let mut merged = superseding(&[first.clone(), second.clone()], "a private note");
    merged.scope = private(GRACE);

    let refused = store
        .curation()
        .merge(&ada, &merged, &[first, second], curator_provenance())
        .await;

    assert!(
        matches!(
            refused,
            Err(StoreError::ScopeDenied { .. }) | Err(StoreError::Curation(_))
        ),
        "a survivor was written outside the curator's chain: {refused:?}",
    );
}

#[tokio::test]
async fn the_same_merge_cannot_be_applied_twice() {
    // The concurrency guard, reachable without a second connection: once the
    // originals are retired, a merge naming them applies to nothing — and an
    // event claiming it did must not survive.
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let first = saved(&store, &ada, repo_scope.clone(), "a fact").await;
    let second = saved(&store, &ada, repo_scope, "a fact.").await;
    let merged = superseding(&[first.clone(), second.clone()], "a fact");

    store
        .curation()
        .merge(
            &ada,
            &merged,
            &[first.clone(), second.clone()],
            curator_provenance(),
        )
        .await
        .expect("the first merge applies");

    let second_survivor = superseding(&[first.clone(), second.clone()], "a fact");
    let refused = store
        .curation()
        .merge(
            &ada,
            &second_survivor,
            &[first, second],
            curator_provenance(),
        )
        .await;

    assert!(
        refused.is_err(),
        "the same merge applied twice: {refused:?}"
    );
    assert_eq!(
        store
            .curation()
            .events(&ada)
            .await
            .expect("the trail reads")
            .len(),
        1,
        "a merge that applied to nothing left an event behind",
    );
    assert!(
        store
            .memories()
            .get(&ada, second_survivor.id)
            .await
            .expect("the read succeeds")
            .is_none(),
        "the refused merge still inserted its survivor",
    );
}

#[tokio::test]
async fn a_rollback_restores_the_originals_and_retires_the_survivor() {
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let first = saved(&store, &ada, repo_scope.clone(), "a fact").await;
    let second = saved(&store, &ada, repo_scope, "a fact.").await;
    let merged = superseding(&[first.clone(), second.clone()], "a fact");
    let event = store
        .curation()
        .merge(
            &ada,
            &merged,
            &[first.clone(), second.clone()],
            curator_provenance(),
        )
        .await
        .expect("the merge applies");

    let rollback = store
        .curation()
        .rollback(
            &ada,
            event.id,
            curator_provenance(),
            Some("not the same fact".to_string()),
        )
        .await
        .expect("the rollback applies");

    assert_eq!(rollback.kind, CurationEventKind::RolledBack);
    assert_eq!(rollback.rolls_back, Some(event.id));
    assert_eq!(status_of(&store, &ada, &first).await, MemoryStatus::Active);
    assert_eq!(status_of(&store, &ada, &second).await, MemoryStatus::Active);
    assert_eq!(
        status_of(&store, &ada, &merged).await,
        MemoryStatus::Retired
    );

    let recalled = store
        .memories()
        .recall(&ada, &RecallQuery::new())
        .await
        .expect("recall succeeds");
    let mut ids: Vec<_> = recalled.iter().map(|record| record.id).collect();
    ids.sort();
    let mut expected = vec![first.id, second.id];
    expected.sort();
    assert_eq!(ids, expected, "rollback did not restore the readable set");
}

#[tokio::test]
async fn a_merge_cannot_be_rolled_back_twice() {
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let first = saved(&store, &ada, repo_scope.clone(), "a fact").await;
    let second = saved(&store, &ada, repo_scope, "a fact.").await;
    let merged = superseding(&[first.clone(), second.clone()], "a fact");
    let event = store
        .curation()
        .merge(&ada, &merged, &[first, second], curator_provenance())
        .await
        .expect("the merge applies");
    store
        .curation()
        .rollback(&ada, event.id, curator_provenance(), None)
        .await
        .expect("the first rollback applies");

    let refused = store
        .curation()
        .rollback(&ada, event.id, curator_provenance(), None)
        .await;

    assert!(
        matches!(refused, Err(StoreError::CurationRolledBack(id)) if id == event.id),
        "a merge was rolled back twice: {refused:?}",
    );
    assert_eq!(
        store
            .curation()
            .events(&ada)
            .await
            .expect("the trail reads")
            .len(),
        2,
        "the refused rollback left an event behind",
    );
}

#[tokio::test]
async fn the_curation_trail_is_scope_filtered() {
    let store = store().await;
    let ada = chain(ADA);
    let grace = chain(GRACE);
    let first = saved(&store, &ada, private(ADA), "a private note").await;
    let second = saved(&store, &ada, private(ADA), "a private note!").await;
    let merged = superseding(&[first.clone(), second.clone()], "a private note");
    store
        .curation()
        .merge(&ada, &merged, &[first, second], curator_provenance())
        .await
        .expect("the merge applies");

    assert_eq!(
        store
            .curation()
            .events(&ada)
            .await
            .expect("ada reads her own trail")
            .len(),
        1
    );
    assert!(
        store
            .curation()
            .events(&grace)
            .await
            .expect("grace reads hers")
            .is_empty(),
        "one actor's curation trail was visible to another",
    );
}

#[tokio::test]
async fn scanning_for_candidates_reads_the_active_set_without_metering_it() {
    // A curator scan is not a use. Counting it as one would let a background
    // job manufacture the retrieval signal the value loop ranks on (G4).
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    // Distinct write times: the scan orders by them, and a tie would leave the
    // expected order to the database.
    let live = written(
        &store,
        &ada,
        repo_scope.clone(),
        "a fact",
        "2026-08-05T06:00:00Z",
    )
    .await;
    let other = written(
        &store,
        &ada,
        repo_scope.clone(),
        "another fact",
        "2026-08-05T07:00:00Z",
    )
    .await;
    let merged = superseding(&[live.clone(), other.clone()], "a fact");
    let unrelated = memory(MemoryCategory::Context, repo_scope, "the working set");
    store
        .memories()
        .save(&ada, &unrelated)
        .await
        .expect("saves");

    let candidates = store
        .curation()
        .candidates(&ada, MemoryCategory::Knowledge)
        .await
        .expect("the scan succeeds");
    assert_eq!(
        candidates
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![live.id, other.id],
        "the scan returned something other than the active knowledge set",
    );

    store
        .curation()
        .merge(&ada, &merged, &[live.clone(), other], curator_provenance())
        .await
        .expect("the merge applies");
    assert_eq!(
        store
            .curation()
            .candidates(&ada, MemoryCategory::Knowledge)
            .await
            .expect("the scan succeeds")
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![merged.id],
        "a retired record was offered as a candidate again",
    );

    assert_eq!(
        store
            .memories()
            .get(&ada, live.id)
            .await
            .expect("the read succeeds")
            .expect("still there")
            .economics
            .use_count,
        0,
        "the curator's scan metered a record as used",
    );
}

#[tokio::test]
async fn a_scan_refuses_a_category_no_curator_may_act_on_alone() {
    let store = store().await;
    let ada = chain(ADA);
    for category in [
        MemoryCategory::Episodic,
        MemoryCategory::Instructions,
        MemoryCategory::Uncertainty,
    ] {
        let refused = store.curation().candidates(&ada, category).await;
        assert!(
            matches!(refused, Err(StoreError::Curation(_))),
            "{category:?} was offered for curation: {refused:?}",
        );
    }
}

#[tokio::test]
async fn a_merge_carries_the_value_its_originals_earned() {
    // The survivor inherits the accumulated economics of what it replaces:
    // dropping them would make every dedupe a quiet reset of the value loop.
    let store = store().await;
    let ada = chain(ADA);
    let repo_scope = ada.scopes()[1].clone();
    let mut first = memory(MemoryCategory::Knowledge, repo_scope.clone(), "a fact");
    first.content.embedding = Some(unit_vector(1));
    first.economics.use_count = 4;
    first.economics.value_score = 1.6;
    store.memories().save(&ada, &first).await.expect("saves");
    let second = saved(&store, &ada, repo_scope, "a fact.").await;

    let mut merged = superseding(&[first.clone(), second.clone()], "a fact");
    merged.economics.use_count = first.economics.use_count + second.economics.use_count;
    merged.economics.value_score = first.economics.value_score;
    store
        .curation()
        .merge(&ada, &merged, &[first, second], curator_provenance())
        .await
        .expect("the merge applies");

    let stored = store
        .memories()
        .get(&ada, merged.id)
        .await
        .expect("the read succeeds")
        .expect("the survivor is there");
    assert_eq!(stored.economics.use_count, 4);
    assert!((stored.economics.value_score - 1.6).abs() < f32::EPSILON);
}
