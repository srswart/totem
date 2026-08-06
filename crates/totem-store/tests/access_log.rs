//! The access log: an audit record for every read and write (docs/project-brief.md
//! G3; ADV-GATEWAY-001). Written against the public API only — there is no
//! accessor for the SurrealDB connection.

mod common;

use common::{ADA, GRACE, chain, memory, store};
use totem_core::{AccessLogEntry, AccessOperation, Harness, MemoryCategory, Scope, SessionId};

fn entry(operation: AccessOperation, endpoint: &str) -> AccessLogEntry {
    AccessLogEntry::new(
        common::actor(ADA),
        Harness::ClaudeCode,
        SessionId::new("sess-1").expect("valid session id"),
        operation,
        endpoint,
        common::at("2026-08-05T06:00:00Z"),
    )
}

#[tokio::test]
async fn a_recorded_entry_is_returned_by_list() {
    let store = store().await;
    let log = store.access_log();

    let recorded = entry(AccessOperation::Recall, "/recall").with_result_count(3);
    log.record(&recorded)
        .await
        .expect("access log accepts the entry");

    let entries = log.list().await.expect("list succeeds");
    assert_eq!(entries, vec![recorded]);
}

#[tokio::test]
async fn a_save_entry_carries_the_written_memory_id() {
    let store = store().await;
    let memories = store.memories();
    let log = store.access_log();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "a note",
    );
    memories.save(&chain(ADA), &note).await.expect("write");

    let recorded = entry(AccessOperation::Save, "/save").for_memory(note.id);
    log.record(&recorded)
        .await
        .expect("access log accepts the entry");

    let entries = log.list().await.expect("list succeeds");
    assert_eq!(entries, vec![recorded]);
    assert_eq!(entries[0].memory_id, Some(note.id));
}

#[tokio::test]
async fn for_memory_returns_only_that_records_entries_oldest_first() {
    let store = store().await;
    let memories = store.memories();
    let log = store.access_log();

    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "a note",
    );
    let other = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "a different note",
    );
    memories.save(&chain(ADA), &note).await.expect("write");
    memories.save(&chain(ADA), &other).await.expect("write");

    let mut first = entry(AccessOperation::Save, "/save").for_memory(note.id);
    first.at = common::at("2026-08-05T06:00:00Z");
    let mut second = entry(AccessOperation::Recall, "/recall").for_memory(note.id);
    second.at = common::at("2026-08-05T06:05:00Z");
    let unrelated = entry(AccessOperation::Save, "/save").for_memory(other.id);

    // Recorded out of order, so the ordering assertion below cannot pass by
    // accident of insertion order.
    log.record(&unrelated).await.expect("write");
    log.record(&second).await.expect("write");
    log.record(&first).await.expect("write");

    let entries = log
        .for_memory(&chain(ADA), note.id)
        .await
        .expect("for_memory succeeds");
    assert_eq!(entries, vec![first, second]);
}

#[tokio::test]
async fn for_memory_on_a_record_outside_the_readers_chain_reads_as_not_found() {
    let store = store().await;
    let memories = store.memories();
    let log = store.access_log();

    let graces = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's working note",
    );
    memories.save(&chain(GRACE), &graces).await.expect("write");
    log.record(&entry(AccessOperation::Save, "/save").for_memory(graces.id))
        .await
        .expect("write");

    let refused = log.for_memory(&chain(ADA), graces.id).await;
    assert!(
        matches!(refused, Err(totem_store::StoreError::NotFound(_))),
        "expected the record to read as absent, got {refused:?}",
    );
}

#[tokio::test]
async fn entries_are_listed_oldest_first() {
    let store = store().await;
    let log = store.access_log();

    let mut first = entry(AccessOperation::Recall, "/recall");
    first.at = common::at("2026-08-05T06:00:00Z");
    let mut second = entry(AccessOperation::Save, "/save");
    second.at = common::at("2026-08-05T06:05:00Z");

    // Recorded out of order, so the assertion below cannot pass by accident of
    // insertion order.
    log.record(&second).await.expect("write");
    log.record(&first).await.expect("write");

    let entries = log.list().await.expect("list succeeds");
    assert_eq!(entries, vec![first, second]);
}
