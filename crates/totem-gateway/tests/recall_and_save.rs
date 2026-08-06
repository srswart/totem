//! `/save` and `/recall`: provenance auto-attached, scope isolation preserved
//! end-to-end over HTTP, and every request appended to the access log
//! (docs/solution-intent.md §3.2, §4; ADV-GATEWAY-001).

mod common;

use axum::http::StatusCode;
use common::{ADA, GRACE, assert_status, json_body, post, recall_request, save_request};
use totem_core::{ActorId, RepoId, ScopeChain};
use totem_gateway::{RecallResponse, SaveResponse};
use totem_store::EMBEDDING_DIMENSIONS;

fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("valid actor id")
}

fn reader_chain(id: &str) -> ScopeChain {
    ScopeChain::resolve(
        &actor(id),
        Some(&RepoId::new(common::REPO).expect("valid repo id")),
        &[],
    )
}

#[tokio::test]
async fn saving_and_recalling_round_trips_over_http() {
    let (router, _store) = common::app().await;

    let save_response = post(
        &router,
        "/save",
        save_request(ADA, "project:srswart/totem", "run `cargo fmt` before pushing"),
    )
    .await;
    assert_status(&save_response, StatusCode::OK);
    let saved: SaveResponse = json_body(save_response).await;

    let recall_response = post(&router, "/recall", recall_request(ADA)).await;
    assert_status(&recall_response, StatusCode::OK);
    let recalled: RecallResponse = json_body(recall_response).await;

    assert_eq!(recalled.records.len(), 1);
    assert_eq!(recalled.records[0].id, saved.id);
    assert_eq!(
        recalled.records[0].content.body,
        "run `cargo fmt` before pushing"
    );
}

#[tokio::test]
async fn recall_never_returns_another_actors_private_memory() {
    let (router, _store) = common::app().await;

    assert_status(
        &post(
            &router,
            "/save",
            save_request(ADA, "actor:ada", "ada's private note"),
        )
        .await,
        StatusCode::OK,
    );
    assert_status(
        &post(
            &router,
            "/save",
            save_request(GRACE, "actor:grace", "grace's private note"),
        )
        .await,
        StatusCode::OK,
    );

    let recalled: RecallResponse = json_body(post(&router, "/recall", recall_request(ADA)).await).await;
    let bodies: Vec<&str> = recalled
        .records
        .iter()
        .map(|record| record.content.body.as_str())
        .collect();
    assert_eq!(bodies, vec!["ada's private note"]);
}

#[tokio::test]
async fn save_auto_attaches_provenance_and_generates_an_embedding() {
    let (router, store) = common::app().await;

    let saved: SaveResponse = json_body(
        post(
            &router,
            "/save",
            save_request(ADA, "actor:ada", "a note worth embedding"),
        )
        .await,
    )
    .await;

    let record = store
        .memories()
        .get(&reader_chain(ADA), saved.id)
        .await
        .expect("read succeeds")
        .expect("the saved record is visible to its own author");

    assert_eq!(record.provenance.author.actor(), &actor(ADA));
    assert_eq!(record.provenance.session.to_string(), "sess-1");
    let embedding = record
        .content
        .embedding
        .expect("the gateway embeds content on write (embeddings.md §4: gateway, on write)");
    assert_eq!(embedding.len(), EMBEDDING_DIMENSIONS);
}

#[tokio::test]
async fn every_request_appends_one_access_log_entry() {
    let (router, store) = common::app().await;

    let saved: SaveResponse = json_body(
        post(
            &router,
            "/save",
            save_request(ADA, "actor:ada", "logged write"),
        )
        .await,
    )
    .await;
    let _: RecallResponse = json_body(post(&router, "/recall", recall_request(ADA)).await).await;

    let entries = store.access_log().list().await.expect("list succeeds");
    assert_eq!(entries.len(), 2, "expected one entry per request: {entries:?}");

    assert_eq!(entries[0].endpoint, "/save");
    assert_eq!(entries[0].actor, actor(ADA));
    assert_eq!(entries[0].memory_id, Some(saved.id));

    assert_eq!(entries[1].endpoint, "/recall");
    assert_eq!(entries[1].actor, actor(ADA));
    assert_eq!(entries[1].result_count, Some(1));
}

#[tokio::test]
async fn writing_into_another_actors_scope_is_refused() {
    let (router, store) = common::app().await;

    let response = post(
        &router,
        "/save",
        save_request(ADA, "actor:grace", "planted in grace's scope"),
    )
    .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    // Refused, and nothing was written or logged as a successful save.
    let recalled = store
        .memories()
        .recall(&reader_chain(GRACE), &totem_store::RecallQuery::new())
        .await
        .expect("recall succeeds");
    assert!(recalled.is_empty(), "a refused write still persisted");
}

#[tokio::test]
async fn a_malformed_scope_string_never_reaches_the_store() {
    let (router, _store) = common::app().await;

    let response = post(&router, "/save", save_request(ADA, "not-a-scope", "body")).await;
    assert!(
        response.status().is_client_error(),
        "expected a 4xx response for an unparsable scope, got {:?}",
        response.status(),
    );
}

#[tokio::test]
async fn a_text_query_still_returns_readable_matches() {
    let (router, _store) = common::app().await;

    assert_status(
        &post(
            &router,
            "/save",
            save_request(ADA, "actor:ada", "the store enforces scope isolation"),
        )
        .await,
        StatusCode::OK,
    );

    let mut request = recall_request(ADA);
    request["query"] = serde_json::json!("how is scope isolation enforced?");
    let recalled: RecallResponse = json_body(post(&router, "/recall", request).await).await;

    assert_eq!(recalled.records.len(), 1);
}

#[tokio::test]
async fn recalling_an_unknown_memory_never_happened_still_appends_an_access_log_entry() {
    // Exercises the "check error paths, not just the happy path" concern
    // (CLAUDE.md's unlogged-access hazard): an empty result is still a
    // completed recall, and the log must record it.
    let (router, store) = common::app().await;

    let recalled: RecallResponse = json_body(post(&router, "/recall", recall_request(ADA)).await).await;
    assert!(recalled.records.is_empty());

    let entries = store.access_log().list().await.expect("list succeeds");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].result_count, Some(0));
}
