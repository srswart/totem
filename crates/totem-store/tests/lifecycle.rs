//! Append-only episodic history, and provenance that survives a round trip.

mod common;

use common::{ADA, GRACE, chain, memory, store};
use totem_core::{
    Author, Content, Harness, LifecycleError, MemoryCategory, MemoryId, MemoryRecord, Provenance,
    Scope, SessionId, SubjectKind, SubjectRef,
};
use totem_store::StoreError;

#[tokio::test]
async fn episodic_records_refuse_revision() {
    let store = store().await;
    let memories = store.memories();

    let episode = memory(
        MemoryCategory::Episodic,
        Scope::Project(common::repo()),
        "the turn exactly as it happened",
    );
    memories.save(&chain(ADA), &episode).await.expect("write");

    let refused = memories
        .revise(&chain(ADA), episode.id, Content::new("a tidier account"))
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

    let stored = memories
        .get(&chain(ADA), episode.id)
        .await
        .expect("get succeeds")
        .expect("the episode is still there");
    assert_eq!(stored.content.body, "the turn exactly as it happened");
}

#[tokio::test]
async fn revisable_categories_are_rewritten_in_place() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "the store filters above the query",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let revised = memories
        .revise(
            &chain(ADA),
            note.id,
            Content::new("the store filters inside the query"),
        )
        .await
        .expect("knowledge is revisable");
    assert_eq!(revised.content.body, "the store filters inside the query");
    assert_eq!(revised.id, note.id);

    let stored = memories
        .get(&chain(ADA), note.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.content.body, "the store filters inside the query");
}

#[tokio::test]
async fn a_record_outside_the_writers_chain_cannot_be_revised() {
    let store = store().await;
    let memories = store.memories();

    let graces = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's working note",
    );
    memories.save(&chain(GRACE), &graces).await.expect("write");

    let refused = memories
        .revise(&chain(ADA), graces.id, Content::new("rewritten by ada"))
        .await;
    assert!(
        matches!(refused, Err(StoreError::NotFound(_))),
        "expected the record to read as absent, got {refused:?}",
    );

    let stored = memories
        .get(&chain(GRACE), graces.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.content.body, "grace's working note");
}

#[tokio::test]
async fn a_revision_cannot_move_a_record_into_another_scope() {
    let store = store().await;
    let memories = store.memories();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "private for now",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    // Sharing is by promotion, which is a recorded event (ADV-CORE-003), not a
    // side effect of editing content.
    let revised = memories
        .revise(&chain(ADA), note.id, Content::new("still private"))
        .await
        .expect("revision succeeds");
    assert_eq!(revised.scope, Scope::Actor(common::actor(ADA)));
}

#[tokio::test]
async fn provenance_survives_the_round_trip_intact() {
    let store = store().await;
    let memories = store.memories();

    let source = MemoryId::new();
    let record = MemoryRecord::new(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        Content::new("derived from a turn").with_tags(["store", "isolation"]),
        Provenance::new(
            Author::Curator(common::actor("dedupe")),
            Harness::Other("some-future-harness".to_string()),
            SessionId::new("job-7").expect("valid session id"),
            common::at("2026-08-04T11:22:33Z"),
        )
        .at_turn(4)
        .derived_from([source]),
    );

    memories.save(&chain(ADA), &record).await.expect("write");
    let stored = memories
        .get(&chain(ADA), record.id)
        .await
        .expect("get succeeds")
        .expect("still there");

    assert_eq!(stored.provenance, record.provenance);
    assert_eq!(stored.content.tags, vec!["store", "isolation"]);
    assert_eq!(stored.economics, record.economics);
    assert_eq!(stored.governance, record.governance);
}

#[tokio::test]
async fn every_category_round_trips_with_its_own_governance() {
    let store = store().await;
    let memories = store.memories();

    for category in MemoryCategory::ALL {
        let record = memory(
            category,
            Scope::Project(common::repo()),
            &format!("a {category:?} record"),
        );
        memories
            .save(&chain(ADA), &record)
            .await
            .unwrap_or_else(|error| panic!("{category:?} write failed: {error}"));

        let stored = memories
            .get(&chain(ADA), record.id)
            .await
            .expect("get succeeds")
            .unwrap_or_else(|| panic!("{category:?} record vanished"));
        assert_eq!(stored, record, "{category:?} did not round trip");
    }
}

#[tokio::test]
async fn a_subject_reference_round_trips_as_a_landscape_link() {
    let store = store().await;
    let memories = store.memories();

    let mut record = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "about the store component",
    );
    record.subject = Some(SubjectRef::new(SubjectKind::Component, "store").expect("valid subject"));
    memories.save(&chain(ADA), &record).await.expect("write");

    let stored = memories
        .get(&chain(ADA), record.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.subject, record.subject);

    // Repo ids carry a slash, which is not a bare record-id key.
    let mut about_repo = memory(
        MemoryCategory::Identity,
        Scope::Project(common::repo()),
        "about the repo itself",
    );
    about_repo.subject =
        Some(SubjectRef::new(SubjectKind::Repo, common::REPO).expect("valid subject"));
    memories
        .save(&chain(ADA), &about_repo)
        .await
        .expect("write");
    let stored = memories
        .get(&chain(ADA), about_repo.id)
        .await
        .expect("get succeeds")
        .expect("still there");
    assert_eq!(stored.subject, about_repo.subject);
}
