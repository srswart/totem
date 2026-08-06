//! The access log: an audit record for every read and write (docs/project-brief.md
//! G3; ADV-GATEWAY-001). Written against the public API only — there is no
//! accessor for the SurrealDB connection.

mod common;

use common::{ADA, chain, memory, store};
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
