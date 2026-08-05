//! Contracts the schema owes its callers: replayable migrations, a temporal
//! filter that actually filters, a vector index that actually gets used, and a
//! pinned embedding dimension.

mod common;

use common::{ADA, chain, memory, sorted_bodies, store, written_at};
use totem_core::{MemoryCategory, Scope};
use totem_store::{EMBEDDING_DIMENSIONS, MIGRATIONS, RecallQuery, Store, StoreError};

#[tokio::test]
async fn migrations_apply_once_and_replay_as_a_no_op() {
    let store = Store::in_memory().await.expect("engine connects");

    let first = store.migrate().await.expect("first migration run");
    assert_eq!(
        first,
        MIGRATIONS.iter().map(|m| m.version).collect::<Vec<_>>(),
        "the first run should apply every migration",
    );
    assert!(!first.is_empty(), "there is at least one migration");

    let second = store.migrate().await.expect("second migration run");
    assert!(
        second.is_empty(),
        "re-running migrations applied {second:?} a second time",
    );

    let applied = store
        .applied_migrations()
        .await
        .expect("the ledger is readable");
    assert_eq!(
        applied.iter().map(|m| m.version).collect::<Vec<_>>(),
        first,
        "the ledger should record exactly what was applied, once each",
    );

    // A migrated database is still usable after the replay.
    let memories = store.memories();
    let note = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "written after a replayed migration",
    );
    memories.save(&chain(ADA), &note).await.expect("write");
}

#[tokio::test]
async fn migration_versions_are_ordered_and_unique() {
    let versions: Vec<u32> = MIGRATIONS.iter().map(|m| m.version).collect();
    let mut sorted = versions.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        versions, sorted,
        "migrations must be strictly increasing and unique",
    );
}

#[tokio::test]
async fn a_temporal_cutoff_filters_and_is_bound_as_a_datetime() {
    let store = store().await;
    let memories = store.memories();

    for (body, timestamp) in [
        ("written in january", "2026-01-01T00:00:00Z"),
        ("written in july", "2026-07-01T00:00:00Z"),
        ("written in august", "2026-08-01T00:00:00Z"),
    ] {
        let record = written_at(
            MemoryCategory::Knowledge,
            Scope::Project(common::repo()),
            body,
            timestamp,
        );
        memories.save(&chain(ADA), &record).await.expect("write");
    }

    let query = RecallQuery::new().since(common::at("2026-06-01T00:00:00Z"));
    let recalled = memories
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");

    // TD-004: a cutoff bound as a string raises no error and filters *nothing*,
    // so the january row is the tell. The API takes a `DateTime<Utc>`, which is
    // why a string cannot be expressed here at all — this asserts the binding
    // behind it.
    assert_eq!(
        sorted_bodies(&recalled),
        vec![
            "written in august".to_string(),
            "written in july".to_string()
        ],
    );

    let plan = memories
        .explain_recall(&chain(ADA), &query)
        .await
        .expect("explain succeeds");
    assert!(
        plan.contains("d'2026-06-01T00:00:00Z'"),
        "the cutoff did not reach the plan as a datetime literal: {plan}",
    );
}

#[tokio::test]
async fn vector_recall_uses_the_hnsw_index_rather_than_a_full_scan() {
    let store = store().await;
    let memories = store.memories();

    for axis in 0..4 {
        let mut record = memory(
            MemoryCategory::Knowledge,
            Scope::Project(common::repo()),
            &format!("record on axis {axis}"),
        );
        record.content.embedding = Some(common::unit_vector(axis));
        memories.save(&chain(ADA), &record).await.expect("write");
    }

    let query = RecallQuery::new()
        .near(common::unit_vector(2))
        .expect("a 384-dimension probe")
        .top_k(2)
        .search_effort(40);

    let recalled = memories
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");
    assert_eq!(
        recalled.first().map(|record| record.content.body.as_str()),
        Some("record on axis 2"),
        "the nearest record did not rank first: {:?}",
        sorted_bodies(&recalled),
    );

    // TD-002: `<|K,EF|>` uses the index and `<|K,COSINE|>` silently does not,
    // and both return correct rows on a small table. Only the plan tells them
    // apart.
    let plan = memories
        .explain_recall(&chain(ADA), &query)
        .await
        .expect("explain succeeds");
    assert!(
        plan.contains("KnnScan"),
        "vector recall fell back to a scan: {plan}",
    );
    assert!(
        plan.contains("memory_embedding"),
        "the plan did not name the vector index: {plan}",
    );
    assert!(
        !plan.contains("KnnTopK"),
        "the plan used brute-force top-K: {plan}",
    );
}

#[tokio::test]
async fn recall_without_a_probe_still_returns_records_that_have_no_embedding() {
    let store = store().await;
    let memories = store.memories();

    let unembedded = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "no embedding yet",
    );
    memories
        .save(&chain(ADA), &unembedded)
        .await
        .expect("write");

    // Until ADV-STORE-002 lands the pipeline, every record is in this state.
    let recalled = memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&recalled),
        vec!["no embedding yet".to_string()]
    );
}

#[tokio::test]
async fn an_embedding_of_the_wrong_dimension_is_refused_on_both_paths() {
    let store = store().await;
    let memories = store.memories();

    let mut record = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "a four-dimension embedding",
    );
    record.content.embedding = Some(vec![0.0, 1.0, 0.0, 0.0]);

    let refused = memories.save(&chain(ADA), &record).await;
    assert!(
        matches!(
            refused,
            Err(StoreError::EmbeddingDimensions {
                expected: EMBEDDING_DIMENSIONS,
                actual: 4
            })
        ),
        "expected a dimension refusal on write, got {refused:?}",
    );

    let refused = RecallQuery::new().near(vec![0.0, 1.0, 0.0, 0.0]);
    assert!(
        matches!(
            refused,
            Err(StoreError::EmbeddingDimensions {
                expected: EMBEDDING_DIMENSIONS,
                actual: 4
            })
        ),
        "expected a dimension refusal on the probe, got {refused:?}",
    );
}

#[tokio::test]
async fn recall_respects_category_and_limit_filters() {
    let store = store().await;
    let memories = store.memories();

    for category in [
        MemoryCategory::Knowledge,
        MemoryCategory::Instructions,
        MemoryCategory::Context,
    ] {
        let record = memory(
            category,
            Scope::Project(common::repo()),
            &format!("a {category:?} record"),
        );
        memories.save(&chain(ADA), &record).await.expect("write");
    }

    let filtered = memories
        .recall(
            &chain(ADA),
            &RecallQuery::new().in_categories([MemoryCategory::Instructions]),
        )
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&filtered),
        vec!["a Instructions record".to_string()],
    );

    let capped = memories
        .recall(&chain(ADA), &RecallQuery::new().limit(2))
        .await
        .expect("recall succeeds");
    assert_eq!(capped.len(), 2);
}
