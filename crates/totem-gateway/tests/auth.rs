//! Token auth for cloud agents (ADV-GATEWAY-003): the gateway's
//! least-privilege invariant, `gateway.yaml`'s "Cloud credentials are
//! least-privilege: tokens bound to repo + scope", proven by execution over
//! both surfaces a cloud agent can reach — REST and MCP over streamable HTTP.
//!
//! The load-bearing claim is the negative one. A token bound to repo A and one
//! actor must not be able to name repo B, act as another actor, or widen its
//! own chain to a scope it was not issued for — so most of these tests assert
//! a refusal, not a success. Each drives the real composed application
//! ([`totem_gateway::authenticated_app`]), not the authorization functions in
//! isolation, because the failure this advance guards against is a *surface*
//! that forgot to ask, not a predicate that answered wrong.
//!
//! The streamable-HTTP tests bind a real loopback listener and drive a real
//! `rmcp` client over it — the transport shape
//! [docs/tech-direction/mcp.md](../../../docs/tech-direction/mcp.md) MCP-003
//! and MCP-004 name as what cloud harnesses actually require — rather than
//! calling the tool functions directly, so "a cloud agent holding a scoped
//! token can call the same tool surface as desktop harnesses" is verified end
//! to end.

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Map, Value, json};
use tokio::task::JoinHandle;
use totem_core::{AccessOperation, RefusalReason};
use totem_gateway::{AppState, TokenRegistry};
use tower::ServiceExt as _;

const ADA: &str = "ada";
const GRACE: &str = "grace";
const REPO: &str = "srswart/totem";
const OTHER_REPO: &str = "srswart/other";

/// A composed, authenticated application over a fresh embedded store, plus the
/// registry its tokens were issued from.
async fn app() -> (Router, TokenRegistry) {
    let state = AppState::in_memory()
        .await
        .expect("embedded engine connects and migrations apply");
    let tokens = state.tokens.clone();
    (totem_gateway::authenticated_app(state), tokens)
}

/// Like [`app`], but also hands back the [`AppState`] so a test can read the
/// access log a refusal is expected to have appended to (ADV-CORE-006).
async fn app_with_state() -> (Router, TokenRegistry, AppState) {
    let state = AppState::in_memory()
        .await
        .expect("embedded engine connects and migrations apply");
    let tokens = state.tokens.clone();
    let router = totem_gateway::authenticated_app(state.clone());
    (router, tokens, state)
}

/// A token bound to `REPO` + `project:REPO` + `ADA` — the ordinary cloud-agent
/// credential shape.
fn project_token(tokens: &TokenRegistry) -> String {
    tokens
        .issue(REPO, &format!("project:{REPO}"), ADA, None)
        .expect("a coherent repo/scope/actor binding issues")
}

async fn send(router: &Router, request: Request<Body>) -> Response<Body> {
    router
        .clone()
        .oneshot(request)
        .await
        .expect("the router does not fail to produce a response")
}

fn post(path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&body).expect("body serialises"),
        ))
        .expect("request builds")
}

fn get(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).expect("request builds")
}

async fn body_text(response: Response<Body>) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("body is utf-8")
}

fn save_body(actor: &str, project: &str, scope: &str, body: &str) -> Value {
    json!({
        "project": project,
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

fn recall_body(actor: &str, project: Option<&str>, teams: Vec<&str>) -> Value {
    json!({
        "actor": actor,
        "project": project,
        "teams": teams,
        "query": null,
        "categories": [],
        "since": null,
        "limit": null,
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

/// An `/enroll` body naming `arrive_id` as the ARRIVE registry id and
/// `git_repo` as the `owner/name` GitHub identity — the two id spaces
/// ADV-GATEWAY-009 unifies. Empty systems/components/advances: these tests
/// are about the repo binding, not the sync content `tests/enroll.rs` already
/// covers.
fn enroll_body(arrive_id: &str, git_repo: &str) -> Value {
    json!({
        "repo": { "id": arrive_id, "name": "Totem", "git_repo": git_repo },
        "systems": [],
        "components": [],
        "advances": [],
        "source": "test:enroll",
    })
}

// ---------------------------------------------------------------------------
// Authentication: is there a live credential at all?
// ---------------------------------------------------------------------------

/// The unauthenticated surface is an enumerated exception, not a hole.
///
/// Fly's health checks cannot present a credential (ADV-INFRA-002), the same
/// way OAuth discovery clients cannot (MCP-014). `/health` is therefore
/// deliberately outside the auth layer — and these tests pin *both*
/// directions: it answers without a credential, and nothing else does.
#[tokio::test]
async fn health_answers_without_a_credential() {
    let (router, _tokens) = app().await;

    let response = send(
        &router,
        Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .expect("request builds"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_text(response).await, "ok");
}

#[tokio::test]
async fn health_reveals_nothing_but_liveness() {
    let (router, _tokens) = app().await;

    let response = send(
        &router,
        Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .expect("request builds"),
    )
    .await;
    let body = body_text(response).await;

    // A health endpoint that leaks build metadata, store paths, or counts
    // hands an unauthenticated caller reconnaissance for free.
    assert_eq!(
        body, "ok",
        "health must answer liveness only, got: {body}"
    );
}

#[tokio::test]
async fn no_route_other_than_health_answers_without_a_credential() {
    let (router, _tokens) = app().await;

    // One representative of every unauthenticated-reachable shape: a GET, a
    // POST, the MCP surface, and a path that does not exist. None may serve
    // data without a credential.
    for (method, uri) in [
        ("GET", "/landscape/058-totem"),
        ("POST", "/recall"),
        ("POST", "/mcp"),
        ("GET", "/advance/ADV-INFRA-002/status"),
    ] {
        let response = send(
            &router,
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("request builds"),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must stay behind the auth layer"
        );
    }
}

#[tokio::test]
async fn a_request_with_no_credential_is_refused() {
    let (router, _tokens) = app().await;

    let response = send(
        &router,
        post("/recall", None, recall_body(ADA, Some(REPO), vec![])),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("www-authenticate")
            .map(|value| value.to_str().expect("header is ascii")),
        Some("Bearer"),
        "a 401 must tell the client how to authenticate (docs/tech-direction/mcp.md MCP-003)"
    );
}

#[tokio::test]
async fn an_unknown_credential_is_refused() {
    let (router, _tokens) = app().await;

    let response = send(
        &router,
        post(
            "/recall",
            Some("totem_cred_not_a_real_token"),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_revoked_credential_is_refused() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let before = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(before.status(), StatusCode::OK, "sanity: the token works");

    assert!(tokens.revoke(&token), "revoking a live token reports true");

    let after = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_expired_credential_is_refused() {
    let (router, tokens) = app().await;
    let token = tokens
        .issue(
            REPO,
            &format!("project:{REPO}"),
            ADA,
            Some(Utc::now() - Duration::seconds(1)),
        )
        .expect("an already-expired token still issues");

    let response = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_registry_never_retains_the_plaintext_token() {
    let tokens = TokenRegistry::new();
    let token = project_token(&tokens);

    assert!(
        !format!("{tokens:?}").contains(&token),
        "a token registry that keeps plaintext turns a state dump into a credential dump"
    );
    assert!(
        tokens.verify(&token, Utc::now()).is_ok(),
        "sanity: the token still verifies against its stored hash"
    );
}

// ---------------------------------------------------------------------------
// Refusals join the access log (ADV-CORE-006): the gateway's "no unlogged
// access" invariant covers every *successful* read and write; these prove it
// now also covers what it refused, and that the refusal itself is the only
// store touch a refused request makes.
// ---------------------------------------------------------------------------

fn is_hex_sha256(fingerprint: &str) -> bool {
    fingerprint.len() == 64 && fingerprint.chars().all(|c| c.is_ascii_hexdigit())
}

#[tokio::test]
async fn a_request_with_no_credential_leaves_exactly_one_refusal_entry() {
    let (router, _tokens, state) = app_with_state().await;

    let response = send(
        &router,
        post(
            "/save",
            None,
            save_body(ADA, REPO, &format!("project:{REPO}"), "never written"),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let entries = state
        .store
        .access_log()
        .list()
        .await
        .expect("list succeeds");
    assert_eq!(
        entries.len(),
        1,
        "a refused write must not touch the store beyond the refusal itself: {entries:?}"
    );
    assert_eq!(entries[0].operation, AccessOperation::Refused);
    assert_eq!(entries[0].endpoint, "/save");
    assert_eq!(
        entries[0].refusal_reason,
        Some(RefusalReason::MissingCredential)
    );
    assert_eq!(
        entries[0].credential_fingerprint, None,
        "there was no credential to fingerprint"
    );
    assert_eq!(entries[0].actor, None);
    assert_eq!(entries[0].harness, None);
    assert_eq!(entries[0].session, None);
}

#[tokio::test]
async fn an_unknown_credential_refusal_is_logged_with_its_fingerprint() {
    let (router, _tokens, state) = app_with_state().await;

    let response = send(
        &router,
        post(
            "/recall",
            Some("totem_cred_not_a_real_token"),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let entries = state
        .store
        .access_log()
        .list()
        .await
        .expect("list succeeds");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].refusal_reason,
        Some(RefusalReason::UnknownCredential)
    );
    let fingerprint = entries[0]
        .credential_fingerprint
        .as_deref()
        .expect("a presented, if forged, credential is still fingerprinted");
    assert!(
        is_hex_sha256(fingerprint),
        "expected a hex-encoded SHA-256 digest, got {fingerprint}"
    );
    assert!(
        !fingerprint.contains("totem_cred_not_a_real_token"),
        "the fingerprint must never be (or contain) the token text"
    );
}

#[tokio::test]
async fn an_expired_credential_refusal_is_logged() {
    let (router, tokens, state) = app_with_state().await;
    let token = tokens
        .issue(
            REPO,
            &format!("project:{REPO}"),
            ADA,
            Some(Utc::now() - Duration::seconds(1)),
        )
        .expect("an already-expired token still issues");

    let response = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let entries = state
        .store
        .access_log()
        .list()
        .await
        .expect("list succeeds");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].refusal_reason, Some(RefusalReason::Expired));
}

#[tokio::test]
async fn an_authorization_refusal_is_logged_with_the_bound_callers_fingerprint() {
    let (router, tokens, state) = app_with_state().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(GRACE, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let entries = state
        .store
        .access_log()
        .list()
        .await
        .expect("list succeeds");
    assert_eq!(
        entries.len(),
        1,
        "a refused read must not touch the store beyond the refusal itself: {entries:?}"
    );
    assert_eq!(entries[0].operation, AccessOperation::Refused);
    assert_eq!(
        entries[0].refusal_reason,
        Some(RefusalReason::ActorNotBound)
    );
    let fingerprint = entries[0].credential_fingerprint.as_deref().expect(
        "a live, bound credential is fingerprinted even when the request it made is refused",
    );
    assert!(is_hex_sha256(fingerprint));
}

#[tokio::test]
async fn a_credential_refusal_over_the_mcp_surface_is_logged_too() {
    let (router, _tokens, state) = app_with_state().await;

    let response = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .expect("request builds"),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "the shared auth layer refuses before rmcp's own framing ever runs"
    );

    let entries = state
        .store
        .access_log()
        .list()
        .await
        .expect("list succeeds");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].endpoint, "/mcp");
    assert_eq!(
        entries[0].refusal_reason,
        Some(RefusalReason::MissingCredential)
    );
}

// ---------------------------------------------------------------------------
// Authorization: the credential is live, but is the request inside its bounds?
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_token_cannot_read_as_another_actor() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(GRACE, Some(REPO), vec![]),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_text(response).await;
    assert!(
        body.contains("actor"),
        "the refusal should name the rule it enforced, got: {body}"
    );
}

#[tokio::test]
async fn a_token_cannot_write_as_another_author() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post(
            "/save",
            Some(&token),
            save_body(
                GRACE,
                REPO,
                &format!("project:{REPO}"),
                "not grace's to write",
            ),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_token_bound_to_one_repo_cannot_name_another() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let read = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(OTHER_REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(
        read.status(),
        StatusCode::FORBIDDEN,
        "a repo-bound token that can read another repo's project scope is the leak this advance exists to prevent"
    );

    let write = send(
        &router,
        post(
            "/save",
            Some(&token),
            save_body(ADA, OTHER_REPO, &format!("project:{OTHER_REPO}"), "leak"),
        ),
    )
    .await;
    assert_eq!(write.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_actor_bound_token_cannot_widen_its_chain_to_project_scope() {
    let (router, tokens) = app().await;
    let token = tokens
        .issue(REPO, &format!("actor:{ADA}"), ADA, None)
        .expect("an actor-scoped token issues");

    let widened = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(
        widened.status(),
        StatusCode::FORBIDDEN,
        "an actor-bound token that resolves a project scope reads memories it was never granted"
    );

    let own = send(
        &router,
        post("/recall", Some(&token), recall_body(ADA, None, vec![])),
    )
    .await;
    assert_eq!(
        own.status(),
        StatusCode::OK,
        "its own actor scope is exactly what it was issued for"
    );
}

#[tokio::test]
async fn a_token_cannot_claim_a_team_it_was_not_issued_for() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec!["058-totem"]),
        ),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "team membership asserted by the caller, not by the credential, is a self-granted scope"
    );
}

#[tokio::test]
async fn only_a_platform_bound_token_may_write_platform_scope() {
    let (router, tokens) = app().await;

    // Platform is in every resolved chain by `ScopeChain::resolve`'s own
    // construction, so the store alone would accept this write from anyone.
    // The credential is what must refuse it.
    let project_bound = project_token(&tokens);
    let refused = send(
        &router,
        post(
            "/save",
            Some(&project_bound),
            save_body(ADA, REPO, "platform", "everyone would see this"),
        ),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    let platform_bound = tokens
        .issue(REPO, "platform", ADA, None)
        .expect("a platform-scoped token issues");
    let allowed = send(
        &router,
        post(
            "/save",
            Some(&platform_bound),
            save_body(ADA, REPO, "platform", "deliberately shared"),
        ),
    )
    .await;
    assert_eq!(allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_token_within_its_bounds_can_save_and_recall() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let saved = send(
        &router,
        post(
            "/save",
            Some(&token),
            save_body(
                ADA,
                REPO,
                &format!("project:{REPO}"),
                "the gateway pins rmcp 3.1.0",
            ),
        ),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);

    let recalled = send(
        &router,
        post(
            "/recall",
            Some(&token),
            recall_body(ADA, Some(REPO), vec![]),
        ),
    )
    .await;
    assert_eq!(recalled.status(), StatusCode::OK);
    let body = body_text(recalled).await;
    assert!(
        body.contains("the gateway pins rmcp 3.1.0"),
        "a token acting inside its own bounds must still get its memories, got: {body}"
    );
}

#[tokio::test]
async fn a_credential_cannot_be_issued_for_a_scope_it_does_not_own() {
    let tokens = TokenRegistry::new();

    assert!(
        tokens
            .issue(REPO, &format!("project:{OTHER_REPO}"), ADA, None)
            .is_err(),
        "a credential naming a repo other than its own binding is over-scoped at issue time"
    );
    assert!(
        tokens
            .issue(REPO, &format!("actor:{GRACE}"), ADA, None)
            .is_err(),
        "a credential naming an actor other than its own binding is over-scoped at issue time"
    );
}

// ---------------------------------------------------------------------------
// Repo binding: `/enroll` and `GET /landscape/:repo` (ADV-GATEWAY-009).
//
// Before this advance, both routes were authenticated but not repo-bound —
// any valid credential could enroll or read any repo's landscape, an
// enumeration vector ADV-GATEWAY-003 disclosed and left open. These tests
// prove the refusal, plus a control proving the bound repo still round-trips.
// ---------------------------------------------------------------------------

const ARRIVE_ID: &str = "058-totem";

#[tokio::test]
async fn enrolling_a_snapshot_naming_another_repo_is_refused() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post("/enroll", Some(&token), enroll_body(ARRIVE_ID, OTHER_REPO)),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a credential bound to one repo enrolling a snapshot for another is the enumeration \
         vector this advance closes"
    );
}

#[tokio::test]
async fn enrolling_a_snapshot_cannot_rebind_an_arrive_id_another_repo_already_owns() {
    let (router, tokens) = app().await;
    let owner_token = project_token(&tokens);
    assert_eq!(
        send(
            &router,
            post("/enroll", Some(&owner_token), enroll_body(ARRIVE_ID, REPO)),
        )
        .await
        .status(),
        StatusCode::OK,
        "seeding the landscape an attacker then tries to take over"
    );

    // The check that a snapshot names the *caller's own* repo is not enough
    // on its own: `sync` upserts by the snapshot's ARRIVE id and overwrites
    // whatever `git_repo` the request asserts, so a credential bound to a
    // *different* repo could otherwise hijack an already-enrolled ARRIVE id
    // just by asserting its own binding in the snapshot (PR #43 review).
    let attacker_token = tokens
        .issue(OTHER_REPO, &format!("project:{OTHER_REPO}"), ADA, None)
        .expect("a coherent binding for the attacker's own repo issues");
    let hijack = send(
        &router,
        post(
            "/enroll",
            Some(&attacker_token),
            enroll_body(ARRIVE_ID, OTHER_REPO),
        ),
    )
    .await;
    assert_eq!(
        hijack.status(),
        StatusCode::FORBIDDEN,
        "a bound credential must not be able to take over an ARRIVE id another repo already owns"
    );

    // The original owner's binding must be untouched by the refused attempt.
    let view = send(
        &router,
        get(&format!("/landscape/{ARRIVE_ID}"), Some(&owner_token)),
    )
    .await;
    assert_eq!(
        view.status(),
        StatusCode::OK,
        "the rightful owner must still be able to read its own landscape"
    );

    // The rightful owner re-syncing the same ARRIVE id must still succeed —
    // the fix must not turn every re-enroll into a rebind refusal.
    let resync = send(
        &router,
        post("/enroll", Some(&owner_token), enroll_body(ARRIVE_ID, REPO)),
    )
    .await;
    assert_eq!(
        resync.status(),
        StatusCode::OK,
        "the rightful owner re-syncing its own ARRIVE id must not be refused"
    );
}

#[tokio::test]
async fn enrolling_a_snapshot_with_a_blank_arrive_id_is_a_client_error() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    // An empty or untrimmed ARRIVE id must be refused outright — not synced
    // as an ambiguous store key, and not silently swapped for the caller's
    // own repo if it later feeds an auth error (Copilot review, PR #44).
    for arrive_id in ["", " 058-totem"] {
        let response = send(
            &router,
            post("/enroll", Some(&token), enroll_body(arrive_id, REPO)),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "arrive_id {arrive_id:?} must be refused as malformed input"
        );
    }
}

#[tokio::test]
async fn enrolling_a_snapshot_for_the_bound_repo_succeeds() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(
        &router,
        post("/enroll", Some(&token), enroll_body(ARRIVE_ID, REPO)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn reading_another_repo_s_landscape_is_refused() {
    let (router, tokens) = app().await;
    let owner_token = project_token(&tokens);
    assert_eq!(
        send(
            &router,
            post("/enroll", Some(&owner_token), enroll_body(ARRIVE_ID, REPO)),
        )
        .await
        .status(),
        StatusCode::OK,
        "seeding the landscape this test then tries to read as someone else"
    );

    let other_token = tokens
        .issue(OTHER_REPO, &format!("project:{OTHER_REPO}"), ADA, None)
        .expect("a coherent binding for the other repo issues");
    let response = send(
        &router,
        get(&format!("/landscape/{ARRIVE_ID}"), Some(&other_token)),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "no route can enumerate other repos' landscapes, regardless of the ARRIVE id vs. \
         owner/name id space a caller tries to name it in"
    );
}

#[tokio::test]
async fn reading_the_bound_repo_s_landscape_succeeds() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);
    assert_eq!(
        send(
            &router,
            post("/enroll", Some(&token), enroll_body(ARRIVE_ID, REPO))
        )
        .await
        .status(),
        StatusCode::OK,
    );

    let response = send(
        &router,
        get(&format!("/landscape/{ARRIVE_ID}"), Some(&token)),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a credential reading the landscape it enrolled must still round-trip"
    );
    let body = body_text(response).await;
    assert!(body.contains(ARRIVE_ID), "got: {body}");
}

#[tokio::test]
async fn a_bound_token_cannot_confirm_a_never_synced_repo_s_binding() {
    let (router, tokens) = app().await;
    let token = project_token(&tokens);

    let response = send(&router, get("/landscape/never-synced", Some(&token))).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "an unconfirmed binding refuses just as certainly as a real mismatch — otherwise a \
         bound token could probe which ARRIVE ids exist by watching for a 200 vs. 403"
    );
}

// ---------------------------------------------------------------------------
// MCP over streamable HTTP: the transport cloud harnesses actually require.
// ---------------------------------------------------------------------------

/// A bound loopback server over the composed, authenticated application.
struct Server {
    address: SocketAddr,
    handle: JoinHandle<()>,
    tokens: TokenRegistry,
}

impl Server {
    async fn start() -> Self {
        let (router, tokens) = app().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind an OS-assigned loopback port");
        let address = listener.local_addr().expect("listener has a local address");
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("axum::serve");
        });
        Self {
            address,
            handle,
            tokens,
        }
    }

    fn uri(&self) -> String {
        format!("http://{}/mcp", self.address)
    }

    async fn client(&self, token: Option<&str>) -> Result<RunningService<RoleClient, ()>, String> {
        let mut config = StreamableHttpClientTransportConfig::with_uri(self.uri());
        if let Some(token) = token {
            config = config.auth_header(token);
        }
        ().serve(StreamableHttpClientTransport::from_config(config))
            .await
            .map_err(|error| error.to_string())
    }

    async fn stop(self) {
        self.handle.abort();
        match self.handle.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => panic!("gateway server task failed: {error}"),
        }
    }
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("value is a JSON object")
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_else(|| panic!("tool call returned no text content: {result:?}"))
}

#[tokio::test]
async fn streamable_http_refuses_a_session_with_no_credential() {
    let server = Server::start().await;

    let outcome = server.client(None).await;

    assert!(
        outcome.is_err(),
        "an unauthenticated cloud agent must not get an MCP session at all"
    );
    server.stop().await;
}

#[tokio::test]
async fn a_scoped_token_reaches_the_same_tool_surface_over_streamable_http() {
    let server = Server::start().await;
    let token = project_token(&server.tokens);
    let client = server.client(Some(&token)).await.expect("initialize");

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools over streamable HTTP");
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    for expected in [
        "totem_recall",
        "totem_save",
        "totem_landscape",
        "totem_feedback",
        "totem_contest",
        "totem_advance_log",
        "totem_advance_status",
    ] {
        assert!(
            names.contains(&expected),
            "cloud agents must see the same tool surface as desktop harnesses; missing {expected} in {names:?}"
        );
    }

    let saved = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(object(save_body(
                ADA,
                REPO,
                &format!("project:{REPO}"),
                "streamable HTTP carries the same tools",
            ))),
        )
        .await
        .expect("totem_save over streamable HTTP");
    assert!(text_of(&saved).contains("id"), "save returns the new id");

    let recalled = client
        .call_tool(
            CallToolRequestParams::new("totem_recall").with_arguments(object(recall_body(
                ADA,
                Some(REPO),
                vec![],
            ))),
        )
        .await
        .expect("totem_recall over streamable HTTP");
    assert!(
        text_of(&recalled).contains("streamable HTTP carries the same tools"),
        "the record saved over this transport must come back through it"
    );

    client.cancel().await.expect("clean shutdown");
    server.stop().await;
}

#[tokio::test]
async fn a_token_cannot_act_as_another_actor_over_streamable_http() {
    let server = Server::start().await;
    let token = project_token(&server.tokens);
    let client = server.client(Some(&token)).await.expect("initialize");

    let outcome = client
        .call_tool(
            CallToolRequestParams::new("totem_recall").with_arguments(object(recall_body(
                GRACE,
                Some(REPO),
                vec![],
            ))),
        )
        .await;

    assert!(
        outcome.is_err(),
        "the MCP surface must enforce the same bounds as REST, not just the REST surface: {outcome:?}"
    );

    client.cancel().await.expect("clean shutdown");
    server.stop().await;
}

#[tokio::test]
async fn a_token_bound_to_one_repo_cannot_name_another_over_streamable_http() {
    let server = Server::start().await;
    let token = project_token(&server.tokens);
    let client = server.client(Some(&token)).await.expect("initialize");

    let outcome = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(object(save_body(
                ADA,
                OTHER_REPO,
                &format!("project:{OTHER_REPO}"),
                "leak",
            ))),
        )
        .await;

    assert!(
        outcome.is_err(),
        "a repo-bound token writing another repo's project scope over MCP: {outcome:?}"
    );

    client.cancel().await.expect("clean shutdown");
    server.stop().await;
}

#[tokio::test]
async fn totem_landscape_refuses_a_repo_this_token_cannot_confirm_over_streamable_http() {
    let server = Server::start().await;
    let token = project_token(&server.tokens);
    let client = server.client(Some(&token)).await.expect("initialize");

    // The REST surface (`tests/auth.rs`'s
    // `a_bound_token_cannot_confirm_a_never_synced_repo_s_binding`) and this
    // MCP tool share the same `handlers::landscape`/`totem_landscape`
    // refusal rule, not two implementations of it — proven here the same way
    // `a_token_bound_to_one_repo_cannot_name_another_over_streamable_http`
    // proves the write side over MCP rather than assuming it from REST alone.
    let outcome = client
        .call_tool(
            CallToolRequestParams::new("totem_landscape")
                .with_arguments(object(json!({ "repo": "never-synced" }))),
        )
        .await;

    assert!(
        outcome.is_err(),
        "a repo-bound token reading a landscape it cannot confirm over MCP: {outcome:?}"
    );

    client.cancel().await.expect("clean shutdown");
    server.stop().await;
}
