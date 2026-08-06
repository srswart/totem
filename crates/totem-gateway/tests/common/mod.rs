//! Fixtures shared by the gateway's integration tests.
//!
//! Each test builds its own embedded `kv-mem` store (docs/tech-direction/surrealdb.md
//! §4) and its own router, and drives it with `tower::ServiceExt::oneshot`
//! rather than a bound TCP listener — the same surface a real HTTP client
//! gets, without a socket.

#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::Value;
use totem_gateway::AppState;
use totem_store::{DeterministicEmbedder, Store};
use tower::ServiceExt;

pub const ADA: &str = "ada";
pub const GRACE: &str = "grace";
pub const REPO: &str = "srswart/totem";

/// A router over a fresh, migrated, embedded store.
pub async fn app() -> (Router, Store<surrealdb::engine::local::Db>) {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    let state = AppState {
        store: store.clone(),
        embedder: Arc::new(DeterministicEmbedder::new()),
    };
    (totem_gateway::router(state), store)
}

pub async fn post(router: &Router, path: &str, body: Value) -> Response<Body> {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&body).expect("body serialises"),
                ))
                .expect("request builds"),
        )
        .await
        .expect("the router does not fail to produce a response")
}

pub async fn get(router: &Router, path: &str) -> Response<Body> {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the router does not fail to produce a response")
}

pub async fn json_body<T: DeserializeOwned>(response: Response<Body>) -> T {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("body deserialises")
}

pub fn assert_status(response: &Response<Body>, expected: StatusCode) {
    assert_eq!(
        response.status(),
        expected,
        "unexpected status: {:?}",
        response.status()
    );
}

pub fn save_request(actor: &str, scope: &str, body: &str) -> Value {
    serde_json::json!({
        "project": REPO,
        "teams": [],
        "category": "knowledge",
        "scope": scope,
        "subject": null,
        "body": body,
        "tags": [],
        "author": { "kind": "agent", "actor": actor },
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

pub fn recall_request(actor: &str) -> Value {
    serde_json::json!({
        "actor": actor,
        "project": REPO,
        "teams": [],
        "query": null,
        "categories": [],
        "since": null,
        "limit": null,
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}
