//! The SurrealDB schema and the constants downstream advances are pinned to.

/// The vector dimension every stored embedding must have.
///
/// Fixed now, before any embedding exists, so that ADV-STORE-002's pipeline
/// lands against an index it does not have to migrate: EMB-004 measured
/// BGE-small-en-v1.5 at 384 dimensions with cosine distance
/// (docs/tech-direction/embeddings.md).
pub const EMBEDDING_DIMENSIONS: usize = 384;

#[cfg(test)]
mod tests {
    //! Enforcement the repository API cannot be trusted to provide.
    //!
    //! These live inside the crate on purpose: they issue statements straight
    //! at the connection, which is exactly what an integration test *cannot*
    //! do. If the database refuses them, then no future code path in this
    //! crate — a curator job, a backfill, a "harmless" migration — can rewrite
    //! the audit substrate or store an unattributable row either.

    use surrealdb::types::Value;

    use crate::{EMBEDDING_DIMENSIONS, Store};

    async fn migrated() -> Store<surrealdb::engine::local::Db> {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        store
    }

    const EPISODE: &str = r#"
        CREATE memory:episode CONTENT {
            category: 'episodic', scope: 'project:srswart/totem', body: 'turn 1',
            tags: [],
            provenance: { author_kind: 'agent', author: 'ada', harness: 'claude_code',
                          session: 's1', created_at: d'2026-08-05T06:00:00Z', derived_from: [] },
            economics: { use_count: 0, value_score: 1.0, currency: 1.0 },
            governance: { status: 'active', review: 'not_required' }
        }
    "#;

    async fn body_of(store: &Store<surrealdb::engine::local::Db>, thing: &str) -> String {
        let mut response = store
            .connection()
            .query(format!("SELECT VALUE body FROM {thing}"))
            .await
            .expect("select sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        let rows = rows.into_array().expect("an array");
        rows.iter()
            .next()
            .and_then(|row| row.clone().into_string().ok())
            .expect("the record exists")
    }

    #[tokio::test]
    async fn the_database_refuses_to_update_an_episodic_row() {
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE)
            .await
            .expect("sent")
            .check()
            .expect("the episode is written");

        let refused = store
            .connection()
            .query("UPDATE memory:episode SET body = 'a tidier account'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote an episodic record: {refused:?}",
        );
        assert_eq!(body_of(&store, "memory:episode").await, "turn 1");
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_an_episodic_row() {
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE)
            .await
            .expect("sent")
            .check()
            .expect("the episode is written");

        let refused = store
            .connection()
            .query("DELETE memory:episode")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed an episodic record: {refused:?}",
        );
        assert_eq!(body_of(&store, "memory:episode").await, "turn 1");
    }

    #[tokio::test]
    async fn the_database_refuses_a_row_without_provenance() {
        let store = migrated().await;
        let refused = store
            .connection()
            .query(
                "CREATE memory:unattributed CONTENT {
                    category: 'knowledge', scope: 'project:srswart/totem',
                    body: 'who wrote this?', tags: [],
                    economics: { use_count: 0, value_score: 1.0, currency: 1.0 },
                    governance: { status: 'active', review: 'not_required' }
                }",
            )
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "an unattributable row was accepted: {refused:?}",
        );
    }

    #[tokio::test]
    async fn a_revisable_row_is_still_updatable() {
        // The negative control: if the append-only event refused *everything*,
        // the two tests above would pass against a store that cannot be
        // written to at all.
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE.replace("memory:episode", "memory:note").replace("'episodic'", "'knowledge'"))
            .await
            .expect("sent")
            .check()
            .expect("the note is written");

        store
            .connection()
            .query("UPDATE memory:note SET body = 'revised'")
            .await
            .expect("sent")
            .check()
            .expect("knowledge is revisable");
        assert_eq!(body_of(&store, "memory:note").await, "revised");
    }

    #[tokio::test]
    async fn the_vector_index_is_pinned_to_the_measured_dimension_and_distance() {
        let store = migrated().await;
        let mut response = store
            .connection()
            .query("INFO FOR TABLE memory")
            .await
            .expect("sent")
            .check()
            .expect("info succeeded");
        let info: Value = response.take(0).expect("info");
        let info = format!("{info:?}");

        assert!(
            info.contains(&format!(
                "HNSW DIMENSION {EMBEDDING_DIMENSIONS} DIST COSINE"
            )),
            "the vector index drifted from the EMB-004 pin: {info}",
        );
    }
}
