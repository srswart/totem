//! Recall responses carry no embedding vectors (ADV-GATEWAY-014).
//!
//! Found in the first real agent use of the deployed gateway: every recalled
//! record returned its 384-float vector (EMB-004's pin) to a caller that pays
//! per token and cannot use it — ranking already happened server-side. Recall
//! is the most frequent call in the dogfood trial, so the cost lands on the
//! path the value loop most needs to be cheap.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app_with_one_memory() -> (axum::Router, usize) {
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let state = totem_gateway::AppState::over(store);
    let router = totem_gateway::router(state);

    let save = json!({
        "project": common::REPO, "teams": [], "category": "knowledge",
        "scope": format!("project:{}", common::REPO), "subject": null,
        "body": "A memory whose vector the caller neither needs nor wants to pay for.",
        "tags": [], "author": {"kind": "human", "actor": "ada"},
        "harness": "claude_code", "session": "payload-test", "turn": null,
    });
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/save")
                .header("content-type", "application/json")
                .body(Body::from(save.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    (router, 0)
}

async fn recall_body(router: &axum::Router) -> (Value, usize) {
    let request = json!({
        "actor": "ada", "project": common::REPO, "teams": [], "query": null,
        "categories": [], "since": null, "limit": null,
        "harness": "claude_code", "session": "payload-test", "turn": null,
    });
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recall")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let size = bytes.len();
    (serde_json::from_slice(&bytes).expect("json"), size)
}

#[tokio::test]
async fn recall_responses_carry_no_embedding() {
    let (router, _) = app_with_one_memory().await;
    let (body, size) = recall_body(&router).await;

    let rendered = body.to_string();
    assert!(
        !rendered.contains("\"embedding\""),
        "recall returned an embedding field to a client that cannot use it \
         ({size} bytes): {}",
        &rendered[..rendered.len().min(200)]
    );
    assert_eq!(
        body["records"].as_array().expect("records").len(),
        1,
        "the record itself must still be returned"
    );
    assert!(
        rendered.contains("neither needs nor wants"),
        "the body must survive the trim"
    );
}

#[tokio::test]
async fn the_store_still_holds_the_vector_that_recall_omits() {
    // The trim is a response-shape decision, not data loss: vector search
    // depends on the embedding, so a change that dropped it from the store
    // would silently break ranking rather than save tokens.
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let state = totem_gateway::AppState::over(store.clone());
    let router = totem_gateway::router(state);

    let save = json!({
        "project": common::REPO, "teams": [], "category": "knowledge",
        "scope": format!("project:{}", common::REPO), "subject": null,
        "body": "Vector retained server-side.", "tags": [],
        "author": {"kind": "human", "actor": "ada"},
        "harness": "claude_code", "session": "payload-test", "turn": null,
    });
    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/save")
                .header("content-type", "application/json")
                .body(Body::from(save.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("response");

    let chain = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new("ada").expect("actor"),
        Some(&totem_core::RepoId::new(common::REPO).expect("repo")),
        &[],
    );
    let stored = store
        .memories()
        .recall(&chain, &totem_store::RecallQuery::default())
        .await
        .expect("recall");
    assert!(
        stored.iter().any(|r| r.content.embedding.is_some()),
        "the store must keep vectors — search needs them"
    );
}
