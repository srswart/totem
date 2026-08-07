//! Totem as an OAuth 2.1 resource server (ADV-GATEWAY-013).
//!
//! Per the MCP authorization spec (2025-06-18) an MCP server is a *resource
//! server*: it publishes protected-resource metadata, points at a third-party
//! authorization server, and validates the tokens that server issues. It does
//! not issue tokens and runs no login UI.
//!
//! These tests use HS256 with a fixed key rather than RS256 with JWKS: the
//! logic under test is issuer/audience/expiry checking and the claims→grant
//! mapping, none of which depends on the signature algorithm. The real
//! RS256-over-JWKS path is verified live against WorkOS AuthKit and recorded
//! in the advance — stated here so nobody mistakes these for proof of it.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use totem_gateway::{AppState, OAuthVerifier};
use tower::ServiceExt;

const TEST_KEY: &[u8] = b"a-test-signing-key-not-used-anywhere-real";
const ISSUER: &str = "https://decent-genius-72-staging.authkit.app";
const RESOURCE: &str = "https://totem-dev.fly.dev/mcp";

async fn app() -> axum::Router {
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let mut state = AppState::over(store);
    state.oauth = Some(std::sync::Arc::new(OAuthVerifier::with_fixed_key(
        ISSUER.to_string(),
        vec![
            RESOURCE.to_string(),
            "https://totem-dev.fly.dev".to_string(),
        ],
        common::REPO.to_string(),
        format!("project:{}", common::REPO),
        jsonwebtoken::DecodingKey::from_secret(TEST_KEY),
        Algorithm::HS256,
    )));
    totem_gateway::authenticated_app(state)
}

fn token(claims: Value) -> String {
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(TEST_KEY),
    )
    .expect("encodes")
}

fn valid_claims() -> Value {
    json!({
        "iss": ISSUER,
        "aud": RESOURCE,
        "sub": "user_01ABCDEF",
        "exp": (Utc::now() + Duration::hours(1)).timestamp(),
        "iat": Utc::now().timestamp(),
    })
}

async fn recall_with(router: &axum::Router, bearer: Option<&str>) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri("/recall")
        .header("content-type", "application/json");
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    let body = json!({
        "actor": "user_01ABCDEF", "project": common::REPO, "teams": [],
        "query": null, "categories": [], "since": null, "limit": null,
        "harness": "claude_code", "session": "oauth-test", "turn": null,
    });
    router
        .clone()
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("response")
        .status()
}

#[tokio::test]
async fn protected_resource_metadata_is_served_without_a_credential() {
    let router = app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a client cannot learn how to authenticate if discovery needs a credential (MCP-014)"
    );
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let document: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(document["resource"], RESOURCE);
    assert_eq!(document["authorization_servers"][0], ISSUER);
    assert_eq!(document["bearer_methods_supported"][0], "header");
}

#[tokio::test]
async fn an_unauthenticated_refusal_points_at_the_metadata() {
    let router = app().await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recall")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("request builds"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let header = response
        .headers()
        .get("www-authenticate")
        .expect("WWW-Authenticate present")
        .to_str()
        .expect("ascii");
    assert!(
        header.contains("resource_metadata="),
        "RFC 9728 §5.1: the refusal must say where the metadata lives, got {header}"
    );
    assert!(
        header.contains("/.well-known/oauth-protected-resource"),
        "{header}"
    );
}

#[tokio::test]
async fn a_valid_token_authenticates_as_its_subject() {
    let router = app().await;
    assert_eq!(
        recall_with(&router, Some(&token(valid_claims()))).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_token_for_another_audience_is_refused() {
    let router = app().await;
    let mut claims = valid_claims();
    claims["aud"] = json!("https://someone-elses-service.example.com");

    assert_eq!(
        recall_with(&router, Some(&token(claims))).await,
        StatusCode::UNAUTHORIZED,
        "accepting a token minted for another service is the confused-deputy \
         failure the MCP spec calls out by name"
    );
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() {
    let router = app().await;
    let mut claims = valid_claims();
    claims["iss"] = json!("https://attacker.example.com");

    assert_eq!(
        recall_with(&router, Some(&token(claims))).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let router = app().await;
    let mut claims = valid_claims();
    claims["exp"] = json!((Utc::now() - Duration::hours(1)).timestamp());

    assert_eq!(
        recall_with(&router, Some(&token(claims))).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_tampered_token_is_refused() {
    let router = app().await;
    let signed = token(valid_claims());
    let tampered = format!("{}x", &signed[..signed.len() - 1]);

    assert_eq!(
        recall_with(&router, Some(&tampered)).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn static_bearer_credentials_still_work_alongside_oauth() {
    // ADV-GATEWAY-003's path must survive: curl, the CLI, Claude Code's own
    // registration and the Claude API connector all use it.
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let state = AppState::over(store);
    let issued = state
        .tokens
        .issue(
            common::REPO,
            &format!("project:{}", common::REPO),
            "ada",
            None,
        )
        .expect("issue");
    let router = totem_gateway::authenticated_app(state);

    let body = json!({
        "actor": "ada", "project": common::REPO, "teams": [],
        "query": null, "categories": [], "since": null, "limit": null,
        "harness": "claude_code", "session": "oauth-test", "turn": null,
    });
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recall")
                .header("authorization", format!("Bearer {issued}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}
