//! ADV-STORE-004 experiments against the embedded `kv-mem` engine — the engine
//! every Totem test uses. Each test builds its own instance and seeds its own
//! fixtures; nothing here assumes a running `surreal` server.

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem};
use surrealdb::types::Action;
use totem_store_spike::{
    explain_scoped_knn, install_toy_dataset, recall_bodies_with_string_cutoff, verify_live_query,
    verify_one_round_trip, verify_transaction_atomicity,
};

async fn seeded() -> surrealdb::Result<Surreal<Db>> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("totem").use_db("spike").await?;
    install_toy_dataset(&db).await?;
    Ok(db)
}

#[tokio::test]
async fn one_round_trip_assembles_vector_graph_and_temporal_context() -> surrealdb::Result<()> {
    let db = seeded().await?;

    let bodies = verify_one_round_trip(&db).await?;

    // Nearest readable memory first, then the episode it derives from. The
    // stale Context record is inside K and inside the scope but outside the
    // temporal cutoff, so the same statement drops it.
    assert_eq!(
        bodies,
        vec![
            "scope isolation is enforced in the store".to_string(),
            "the turn that produced the rule".to_string(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn recall_never_returns_another_actors_scope() -> surrealdb::Result<()> {
    let db = seeded().await?;

    let bodies = verify_one_round_trip(&db).await?;

    // The foreign records are *closer* to the probe than anything readable, so
    // this also proves the scope predicate is not a post-filter applied after
    // the vector operator truncated to K.
    assert!(
        !bodies.iter().any(|b| b.contains("private")),
        "another actor's memory came back from recall: {bodies:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_string_temporal_cutoff_silently_filters_nothing() -> surrealdb::Result<()> {
    let db = seeded().await?;

    let bodies = recall_bodies_with_string_cutoff(&db).await?;

    // No error is raised — the stale Context record simply comes back. This is
    // the failure mode ADV-STORE-001 has to avoid when it builds bindings from
    // caller-supplied filters.
    assert!(
        bodies.iter().any(|b| b == "working set from months ago"),
        "expected the string-bound cutoff to filter nothing, got {bodies:?}"
    );
    Ok(())
}

#[tokio::test]
async fn scope_predicate_is_pushed_into_the_index_scan() -> surrealdb::Result<()> {
    let db = seeded().await?;

    let plan = explain_scoped_knn(&db).await?;

    // The plan must show the HNSW index doing the work *and* carrying the scope
    // predicate. If a future SurrealDB release stops pushing the predicate down,
    // this fails and ADV-STORE-001's over-fetch budget has to be revisited.
    assert!(
        plan.contains("KnnScan") && plan.contains("mem_embedding"),
        "HNSW index was not used by the recall query: {plan}"
    );
    assert!(
        plan.contains("\"operator\": String(\"KnnScan\")") && plan.contains("scope INSIDE"),
        "scope predicate was not pushed into the index scan: {plan}"
    );
    Ok(())
}

#[tokio::test]
async fn a_turns_writes_commit_or_roll_back_together() -> surrealdb::Result<()> {
    let db = seeded().await?;

    verify_transaction_atomicity(&db).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn live_queries_fire_on_the_embedded_engine_for_committed_writes_only()
-> surrealdb::Result<()> {
    let db = seeded().await?;

    let seen = verify_live_query(&db).await?;

    // One CREATE (the decision) and one UPDATE (the entity counter) from the
    // committed turn, then the sentinel. The rolled-back turn wrote a memory in
    // the same scope between them and must contribute nothing.
    let actions: Vec<Action> = seen.iter().map(|(action, _)| *action).collect();
    assert_eq!(
        actions,
        vec![Action::Create, Action::Update, Action::Create],
        "live feed did not match the committed writes: {seen:?}"
    );
    assert!(
        !seen.iter().any(|(_, id)| id.contains("orphan")),
        "a rolled-back write was published to the live feed: {seen:?}"
    );
    Ok(())
}
