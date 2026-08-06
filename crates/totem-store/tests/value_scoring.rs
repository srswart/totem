//! Metering and value/currency scoring in retrieval ranking (ADV-CORE-002).
//!
//! docs/tech-direction/value-attribution.md (ADV-CORE-004) is the
//! investigation these tests hold the implementation to: citation (via
//! `provenance.derived_from`) is the only signal with real discriminating
//! power available today (VAL-002), raw retrieval must not be weighted into
//! `value_score` directly (VAL-003), and explicit feedback has zero data
//! points yet (VAL-005, `totem_feedback` is `ADV-GATEWAY-004`, not built).

mod common;

use chrono::Utc;
use common::{ADA, GRACE, chain, memory, repo, store, written_at};
use totem_core::{Content, MemoryCategory, Provenance, Scope};
use totem_store::RecallQuery;

#[tokio::test]
async fn recall_increments_use_count_and_refreshes_currency_for_non_episodic_records() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "recall should meter this",
    );
    memories.save(&chain(ADA), &note).await.expect("write");
    assert_eq!(note.economics.use_count, 0);
    assert_eq!(note.economics.last_used_at, None);

    let before = Utc::now();
    memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds");

    let stored = memories
        .get(&chain(ADA), note.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.economics.use_count, 1, "use_count did not increment");
    assert_eq!(stored.economics.currency, 1.0, "currency was not refreshed");
    let last_used = stored
        .economics
        .last_used_at
        .expect("last_used_at was not set");
    assert!(
        last_used >= before,
        "last_used_at {last_used} predates the recall that set it",
    );

    // A second recall increments again — this is a running counter, not a flag.
    memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    let stored_again = memories
        .get(&chain(ADA), note.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored_again.economics.use_count, 2);
}

#[tokio::test]
async fn recall_never_touches_an_episodic_records_economics() {
    let store = store().await;
    let memories = store.memories();

    let episode = memory(
        MemoryCategory::Episodic,
        Scope::Project(repo()),
        "the turn exactly as it happened",
    );
    memories.save(&chain(ADA), &episode).await.expect("write");

    // Episodic rows are append-only at the database level (schema.rs): an
    // UPDATE that reached one would throw and fail the whole recall, not just
    // silently skip it. This is the regression guard for that hazard.
    let recalled = memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds even though an episodic row is in the result set");
    assert_eq!(recalled.len(), 1);

    let stored = memories
        .get(&chain(ADA), episode.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.economics.use_count, 0, "episodic use_count moved");
    assert_eq!(
        stored.economics.last_used_at, None,
        "episodic last_used_at moved"
    );
    assert_eq!(stored.economics.currency, 1.0, "episodic currency moved");
}

#[tokio::test]
async fn citing_a_memory_on_save_boosts_its_value_score() {
    let store = store().await;
    let memories = store.memories();

    let finding = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "TD-003: scope predicate is pushed into the index scan",
    );
    memories.save(&chain(ADA), &finding).await.expect("write");
    assert_eq!(finding.economics.value_score, 1.0);

    let citing = totem_core::MemoryRecord::new(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        Content::new("the store generates the scope predicate itself, per TD-003"),
        Provenance::new(
            totem_core::Author::Agent(common::actor(ADA)),
            totem_core::Harness::ClaudeCode,
            totem_core::SessionId::new("sess-2").expect("valid session id"),
            Utc::now(),
        )
        .derived_from([finding.id]),
    );
    memories
        .save(&chain(ADA), &citing)
        .await
        .expect("write with a citation");

    let stored = memories
        .get(&chain(ADA), finding.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert!(
        stored.economics.value_score > 1.0,
        "citation did not raise value_score: {}",
        stored.economics.value_score,
    );
}

#[tokio::test]
async fn citing_a_memory_outside_the_writers_chain_does_not_boost_it() {
    let store = store().await;
    let memories = store.memories();

    let graces_private = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's private finding",
    );
    memories
        .save(&chain(GRACE), &graces_private)
        .await
        .expect("grace writes her own scope");

    // Ada could never have legitimately recalled grace's private memory to
    // cite it — this is the adversarial case: a citation claim naming an id
    // outside the writer's own chain must not move that record's economics,
    // the same isolation invariant the recall path already enforces.
    let claims_a_citation = totem_core::MemoryRecord::new(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        Content::new("ada's own note, falsely claiming to derive from grace's private memory"),
        Provenance::new(
            totem_core::Author::Agent(common::actor(ADA)),
            totem_core::Harness::ClaudeCode,
            totem_core::SessionId::new("sess-3").expect("valid session id"),
            Utc::now(),
        )
        .derived_from([graces_private.id]),
    );
    memories
        .save(&chain(ADA), &claims_a_citation)
        .await
        .expect("ada's own write still succeeds");

    let stored = memories
        .get(&chain(GRACE), graces_private.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(
        stored.economics.value_score, 1.0,
        "a citation crossed the scope boundary and boosted grace's private memory",
    );
}

#[tokio::test]
async fn citing_an_episodic_source_does_not_error_and_leaves_it_unmodified() {
    let store = store().await;
    let memories = store.memories();

    let episode = memory(
        MemoryCategory::Episodic,
        Scope::Project(repo()),
        "the turn a later note was derived from",
    );
    memories.save(&chain(ADA), &episode).await.expect("write");

    let derived = totem_core::MemoryRecord::new(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        Content::new("distilled from the episode above"),
        Provenance::new(
            totem_core::Author::Agent(common::actor(ADA)),
            totem_core::Harness::ClaudeCode,
            totem_core::SessionId::new("sess-4").expect("valid session id"),
            Utc::now(),
        )
        .derived_from([episode.id]),
    );
    memories
        .save(&chain(ADA), &derived)
        .await
        .expect("citing an episodic source does not throw the append-only guard");

    let stored = memories
        .get(&chain(ADA), episode.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(
        stored.economics.value_score, 1.0,
        "episodic value_score moved"
    );
}

#[tokio::test]
async fn recall_ranks_a_cited_record_above_an_uncited_one_of_equal_relevance() {
    let store = store().await;
    let memories = store.memories();

    // Both written at the same instant, same category, no vector probe (so
    // relevance is neutral for both) — the only thing that should separate
    // them is the citation boost.
    let uncited = written_at(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "never cited",
        "2026-08-05T06:00:00Z",
    );
    let cited = written_at(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "will be cited",
        "2026-08-05T06:00:00Z",
    );
    memories.save(&chain(ADA), &uncited).await.expect("write");
    memories.save(&chain(ADA), &cited).await.expect("write");

    let citing = totem_core::MemoryRecord::new(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        Content::new("citing the second record"),
        Provenance::new(
            totem_core::Author::Agent(common::actor(ADA)),
            totem_core::Harness::ClaudeCode,
            totem_core::SessionId::new("sess-5").expect("valid session id"),
            Utc::now(),
        )
        .derived_from([cited.id]),
    );
    memories.save(&chain(ADA), &citing).await.expect("write");

    let recalled = memories
        .recall(
            &chain(ADA),
            &RecallQuery::new().in_categories([MemoryCategory::Knowledge]),
        )
        .await
        .expect("recall succeeds");

    let cited_position = recalled
        .iter()
        .position(|record| record.id == cited.id)
        .expect("the cited record is in the result set");
    let uncited_position = recalled
        .iter()
        .position(|record| record.id == uncited.id)
        .expect("the uncited record is in the result set");
    assert!(
        cited_position < uncited_position,
        "the cited record did not outrank the uncited one: {:?}",
        recalled.iter().map(|r| &r.content.body).collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn recall_ranks_a_recently_reinforced_decaying_record_above_a_stale_one() {
    let store = store().await;
    let memories = store.memories();

    // Knowledge decays (category.rs); a record written long ago and never
    // reread should rank behind one written moments ago, once currency
    // enters the ranking.
    let stale = written_at(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "written a long time ago, never reread",
        "2026-01-01T00:00:00Z",
    );
    let fresh = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "written moments ago",
    );
    memories.save(&chain(ADA), &stale).await.expect("write");
    memories.save(&chain(ADA), &fresh).await.expect("write");

    let recalled = memories
        .recall(
            &chain(ADA),
            &RecallQuery::new().in_categories([MemoryCategory::Knowledge]),
        )
        .await
        .expect("recall succeeds");

    let fresh_position = recalled
        .iter()
        .position(|record| record.id == fresh.id)
        .expect("the fresh record is in the result set");
    let stale_position = recalled
        .iter()
        .position(|record| record.id == stale.id)
        .expect("the stale record is in the result set");
    assert!(
        fresh_position < stale_position,
        "the stale record was not out-ranked by currency decay: {:?}",
        recalled.iter().map(|r| &r.content.body).collect::<Vec<_>>(),
    );
}
