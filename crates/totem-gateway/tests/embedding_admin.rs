//! The embedder must be visible from outside the process (ADV-STORE-008).
//!
//! A deployment serving non-semantic recall looks exactly like one doing the
//! real thing — same routes, same shapes, same confident ordering — right up
//! until someone trusts a ranking. These tests hold the line that you can ask.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use totem_gateway::AppState;
use tower::ServiceExt as _;

async fn gateway() -> axum::Router {
    let state = AppState::in_memory()
        .await
        .expect("embedded engine connects and migrations apply");
    totem_gateway::router(state)
}

async fn json_of(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    serde_json::from_slice(&bytes).expect("the response is JSON")
}

#[tokio::test]
async fn the_running_embedder_is_reportable() {
    let response = gateway()
        .await
        .oneshot(
            Request::builder()
                .uri("/admin/embedding")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_of(response).await;
    assert!(
        body["running"].is_string(),
        "the running model must be reportable without reading build flags: {body}"
    );
    assert_eq!(
        body["uniform"], true,
        "an empty store is trivially in one space: {body}"
    );
}

#[tokio::test]
async fn a_mixed_index_reports_itself_as_not_uniform() {
    // The condition the whole advance exists to eliminate. If this can be
    // created without the gateway saying so, an operator has no way to know
    // recall is ranking across two geometries.
    let state = AppState::in_memory().await.expect("state");
    let store = state.store.clone();
    let app = totem_gateway::router(state);

    let chain = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new("ada").expect("valid actor"),
        None,
        &[],
    );
    for (body, model) in [("first", "model-a"), ("second", "model-b")] {
        let mut record = totem_core::MemoryRecord::new(
            totem_core::MemoryCategory::Knowledge,
            totem_core::Scope::Actor(totem_core::ActorId::new("ada").expect("valid actor")),
            totem_core::Content::new(body),
            totem_core::Provenance::new(
                totem_core::Author::Agent(totem_core::ActorId::new("agent:t").expect("actor")),
                totem_core::Harness::ClaudeCode,
                totem_core::SessionId::new("s").expect("session"),
                chrono::Utc::now(),
            ),
        );
        record.content.embedding = Some(vec![0.0; totem_store::EMBEDDING_DIMENSIONS]);
        record.content.embedding_model = Some(model.to_string());
        store.memories().save(&chain, &record).await.expect("save");
    }

    let body = json_of(
        app.oneshot(
            Request::builder()
                .uri("/admin/embedding")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the router answers"),
    )
    .await;

    assert_eq!(
        body["uniform"], false,
        "two models in one index must be reported as mixed: {body}"
    );
    assert_eq!(
        body["rows_by_model"].as_array().map(Vec::len),
        Some(2),
        "both spaces must be named, so an operator knows what to re-embed: {body}"
    );
}

#[tokio::test]
async fn re_embedding_makes_a_mixed_index_uniform() {
    let state = AppState::in_memory().await.expect("state");
    let store = state.store.clone();
    let running = state.embedder.model_name().to_string();
    let app = totem_gateway::router(state);

    let chain = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new("ada").expect("valid actor"),
        None,
        &[],
    );
    for (body, model) in [("first", "model-a"), ("second", "model-b")] {
        let mut record = totem_core::MemoryRecord::new(
            totem_core::MemoryCategory::Knowledge,
            totem_core::Scope::Actor(totem_core::ActorId::new("ada").expect("valid actor")),
            totem_core::Content::new(body),
            totem_core::Provenance::new(
                totem_core::Author::Agent(totem_core::ActorId::new("agent:t").expect("actor")),
                totem_core::Harness::ClaudeCode,
                totem_core::SessionId::new("s").expect("session"),
                chrono::Utc::now(),
            ),
        );
        record.content.embedding = Some(vec![0.0; totem_store::EMBEDDING_DIMENSIONS]);
        record.content.embedding_model = Some(model.to_string());
        store.memories().save(&chain, &record).await.expect("save");
    }

    let summary = json_of(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/reembed")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("the router answers"),
    )
    .await;
    assert_eq!(summary["reembedded"], 2, "{summary}");
    assert_eq!(summary["model"], running, "{summary}");

    let after = json_of(
        app.oneshot(
            Request::builder()
                .uri("/admin/embedding")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the router answers"),
    )
    .await;
    assert_eq!(after["uniform"], true, "{after}");
}

/// The access log distinguishes a reinforcing read from an observing one
/// (ADV-GATEWAY-017).
///
/// No separate boolean: `endpoint` already carries it, because `/recall` and
/// `/recall/explain` *are* the metered and unmetered reads. This test is what
/// makes that claim evidence rather than an assertion — without it the advance
/// was citing an integration test that did not exist.
#[tokio::test]
async fn the_access_log_says_which_reads_could_have_reinforced() {
    let state = AppState::in_memory().await.expect("state");
    let store = state.store.clone();
    let app = totem_gateway::router(state);

    let body = |endpoint: &str| {
        serde_json::json!({
            "actor": "ada",
            "query": "anything",
            "harness": "claude_code",
            "session": format!("session-for-{endpoint}"),
        })
        .to_string()
    };

    for endpoint in ["/recall", "/recall/explain"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(endpoint)
                    .header("content-type", "application/json")
                    .body(Body::from(body(endpoint)))
                    .expect("request builds"),
            )
            .await
            .expect("the router answers");
        assert_eq!(response.status(), StatusCode::OK, "{endpoint} answers");
    }

    let endpoints: Vec<String> = store
        .access_log()
        .list()
        .await
        .expect("list succeeds")
        .into_iter()
        .map(|entry| entry.endpoint)
        .collect();

    assert!(
        endpoints.iter().any(|e| e == "/recall"),
        "the reinforcing read must be identifiable in the log: {endpoints:?}"
    );
    assert!(
        endpoints.iter().any(|e| e == "/recall/explain"),
        "so must the observing one — an operator reconstructing why an \
         economics figure moved has only this to go on: {endpoints:?}"
    );
}
