//! The store's highest-severity invariant: a caller reads only its own chain.
//!
//! Leaking private context across scopes is the project's worst failure
//! (docs/project-brief.md, "Key risks"). These tests are written against the
//! public API only — there is no accessor for the SurrealDB connection — so
//! they exercise the same surface a gateway or curator would.

mod common;

use common::{ADA, GRACE, REPO, TEAM, chain, chain_with_team, memory, repo, sorted_bodies, store};
use totem_core::{MemoryCategory, Scope, ScopeChain};
use totem_store::{RecallQuery, StoreError};

#[tokio::test]
async fn a_reader_never_sees_another_actors_private_memory() {
    let store = store().await;
    let memories = store.memories();

    let ada_private = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(ADA)),
        "ada's private note",
    );
    let grace_private = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's private note",
    );
    let shared = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "shared project note",
    );

    memories
        .save(&chain(ADA), &ada_private)
        .await
        .expect("ada writes her own scope");
    memories
        .save(&chain(GRACE), &grace_private)
        .await
        .expect("grace writes her own scope");
    memories
        .save(&chain(ADA), &shared)
        .await
        .expect("ada writes the project scope");

    let ada_view = memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&ada_view),
        vec![
            "ada's private note".to_string(),
            "shared project note".to_string()
        ],
    );

    let grace_view = memories
        .recall(&chain(GRACE), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&grace_view),
        vec![
            "grace's private note".to_string(),
            "shared project note".to_string()
        ],
    );
}

#[tokio::test]
async fn a_foreign_record_reads_as_absent_rather_than_forbidden() {
    let store = store().await;
    let memories = store.memories();

    let grace_private = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "grace's private note",
    );
    memories
        .save(&chain(GRACE), &grace_private)
        .await
        .expect("grace writes her own scope");

    // Not an error: an error that distinguished "exists but denied" from "does
    // not exist" would let a caller enumerate another actor's memory ids.
    assert_eq!(
        memories
            .get(&chain(ADA), grace_private.id)
            .await
            .expect("get succeeds"),
        None,
    );
    assert!(
        memories
            .get(&chain(GRACE), grace_private.id)
            .await
            .expect("get succeeds")
            .is_some(),
    );
}

#[tokio::test]
async fn a_write_outside_the_writers_chain_is_refused_and_persists_nothing() {
    let store = store().await;
    let memories = store.memories();

    let into_graces_scope = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "planted in grace's scope",
    );

    let refused = memories.save(&chain(ADA), &into_graces_scope).await;
    assert!(
        matches!(refused, Err(StoreError::ScopeDenied { .. })),
        "expected ScopeDenied, got {refused:?}",
    );

    // The refusal must be a refusal to *write*, not a filter on reads.
    assert_eq!(
        memories
            .get(&chain(GRACE), into_graces_scope.id)
            .await
            .expect("get succeeds"),
        None,
    );
}

#[tokio::test]
async fn a_team_scope_is_readable_only_by_members() {
    let store = store().await;
    let memories = store.memories();

    let team_rule = memory(
        MemoryCategory::Instructions,
        Scope::Team(common::team()),
        "the team squashes nothing",
    );
    memories
        .save(&chain_with_team(ADA), &team_rule)
        .await
        .expect("a member writes the team scope");

    let member_view = memories
        .recall(&chain_with_team(GRACE), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&member_view),
        vec!["the team squashes nothing".to_string()],
    );

    let outsider_view = memories
        .recall(&chain(GRACE), &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert!(
        outsider_view.is_empty(),
        "a non-member read the team scope: {:?}",
        sorted_bodies(&outsider_view),
    );
}

#[tokio::test]
async fn the_merged_view_keeps_the_narrowest_scope_and_drops_the_duplicate() {
    let store = store().await;
    let memories = store.memories();

    // The same fact, held at two scopes. Chain position is precedence, so the
    // actor's own version must win and the project's copy must not appear
    // alongside it (docs/solution-intent.md §2.2).
    let project_version = memory(
        MemoryCategory::Instructions,
        Scope::Project(repo()),
        "Run `cargo fmt` before pushing",
    );
    let personal_version = memory(
        MemoryCategory::Instructions,
        Scope::Actor(common::actor(ADA)),
        "run `cargo fmt`   BEFORE pushing",
    );

    memories
        .save(&chain(ADA), &project_version)
        .await
        .expect("project write");
    memories
        .save(&chain(ADA), &personal_version)
        .await
        .expect("actor write");

    let merged = memories
        .recall(&chain(ADA), &RecallQuery::new())
        .await
        .expect("recall succeeds");

    assert_eq!(merged.len(), 1, "duplicate survived the merge: {merged:?}");
    assert_eq!(merged[0].id, personal_version.id);
    assert_eq!(merged[0].scope, Scope::Actor(common::actor(ADA)));
}

#[tokio::test]
async fn the_scope_predicate_is_generated_by_the_store_and_reaches_the_index_scan() {
    let store = store().await;
    let memories = store.memories();

    let mut near = memory(
        MemoryCategory::Knowledge,
        Scope::Project(repo()),
        "readable and near the probe",
    );
    near.content.embedding = Some(common::unit_vector(1));

    // Deliberately *nearer* the probe than anything readable: an engine that
    // truncated to top-K by distance and filtered afterwards would return the
    // foreign row or nothing at all (TD-003).
    let mut foreign = memory(
        MemoryCategory::Knowledge,
        Scope::Actor(common::actor(GRACE)),
        "private to grace and nearer the probe",
    );
    foreign.content.embedding = Some(common::unit_vector(0));

    memories.save(&chain(ADA), &near).await.expect("write");
    memories.save(&chain(GRACE), &foreign).await.expect("write");

    let query = RecallQuery::new()
        .near(common::unit_vector(0))
        .expect("a 384-dimension probe")
        .top_k(3);

    let recalled = memories
        .recall(&chain(ADA), &query)
        .await
        .expect("recall succeeds");
    assert_eq!(
        sorted_bodies(&recalled),
        vec!["readable and near the probe".to_string()],
    );

    // Results alone cannot prove where the filter ran (TD-002/TD-003): assert
    // on the plan, so a regression to "fetch everything, filter in Rust" is
    // visible rather than silent.
    let plan = memories
        .explain_recall(&chain(ADA), &query)
        .await
        .expect("explain succeeds");
    assert!(plan.contains("KnnScan"), "plan did not use the index: {plan}");
    assert!(
        plan.contains(&format!("actor:{ADA}")) && plan.contains(&format!("project:{REPO}")),
        "the reader's chain is missing from the plan predicate: {plan}",
    );
    assert!(
        !plan.contains(&format!("actor:{GRACE}")),
        "another actor's scope reached the plan: {plan}",
    );
    assert!(
        !plan.contains(&format!("team:{TEAM}")),
        "a scope outside the reader's chain reached the plan: {plan}",
    );
}

#[tokio::test]
async fn an_empty_project_chain_still_carries_the_actors_own_scope() {
    let store = store().await;
    let memories = store.memories();

    let solo = ScopeChain::resolve(&common::actor(ADA), None, &[]);
    let note = memory(
        MemoryCategory::Context,
        Scope::Actor(common::actor(ADA)),
        "working alone",
    );
    memories.save(&solo, &note).await.expect("write");

    let recalled = memories
        .recall(&solo, &RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert_eq!(sorted_bodies(&recalled), vec!["working alone".to_string()]);
}
