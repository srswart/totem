//! The gateway serves the console, without shadowing the API
//! (ADV-GATEWAY-010).
//!
//! Serving a single-page app means a catch-all fallback, and a greedy
//! fallback is a classic way to break an API silently: a mistyped or
//! not-yet-implemented endpoint starts returning `index.html` with status
//! 200, so a client sees HTML where it expected JSON and reports something
//! misleading. Both directions are asserted here.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn app() -> axum::Router {
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let state = totem_gateway::AppState::over(store);
    totem_gateway::authenticated_app(state)
}

async fn get(router: &axum::Router, uri: &str) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn the_console_is_served_at_the_root() {
    let (status, body) = get(&app().await, "/").await;

    assert_eq!(status, StatusCode::OK, "the console must be reachable");
    assert!(
        body.contains("<html") || body.contains("<!DOCTYPE"),
        "expected the console document, got: {}",
        &body[..body.len().min(120)]
    );
}

#[tokio::test]
async fn a_client_side_route_falls_back_to_the_console() {
    // A page the browser owns, not the server: it must render the app
    // rather than 404, or a refresh on any console URL breaks.
    let (status, body) = get(&app().await, "/governance").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<html") || body.contains("<!DOCTYPE"), "{body:.120}");
}

#[tokio::test]
async fn the_fallback_never_shadows_an_api_route() {
    let router = app().await;

    // Unauthenticated API calls must still be refused with 401 — never
    // answered with the console document, which would turn an auth failure
    // into a baffling parse error on the client.
    for uri in [
        "/landscape/058-totem",
        "/advance/ADV-GATEWAY-010/status",
    ] {
        let (status, body) = get(&router, uri).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{uri} was answered by the SPA fallback instead of the API: {body:.120}"
        );
    }
}

#[tokio::test]
async fn health_and_discovery_still_answer_as_themselves() {
    let router = app().await;

    let (status, body) = get(&router, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok", "the fallback swallowed /health");

    // The OAuth metadata route answers 404 when no verifier is configured —
    // a *route-specific* 404, not the SPA fallback.
    let (status, body) = get(&router, "/.well-known/oauth-protected-resource").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        !body.contains("<html"),
        "discovery was answered by the SPA fallback: {body:.120}"
    );
}
