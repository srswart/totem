//! `GET /landscape/:repo/events` (ADV-CONSOLE-003): the live relay the
//! console's dashboard subscribes to instead of polling `GET
//! /landscape/:repo` with a manual Refresh button.
//!
//! Driven the same way every other REST test in this crate drives a request
//! — `tower::ServiceExt::oneshot` over the real, composed `Router` — rather
//! than a bound TCP loopback listener. Unlike the MCP streamable-HTTP tests
//! (`tests/auth.rs`), there is no separate wire protocol to validate here:
//! SSE is plain HTTP with a streaming body, and `oneshot` already drives the
//! real `Service`/middleware stack (auth included) end to end; only the body
//! is read incrementally, frame by frame, instead of collected all at once.

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use totem_gateway::AppState;
use tower::ServiceExt;

const REPO: &str = "srswart/totem";
const OTHER_REPO: &str = "srswart/other";
const ARRIVE_ID: &str = "058-totem";
const ADA: &str = "ada";

async fn get_stream(router: &axum::Router, path: &str, token: Option<&str>) -> Response<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    router
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request builds"))
        .await
        .expect("the router does not fail to produce a response")
}

async fn authed_post(router: &axum::Router, path: &str, token: &str, body: Value) -> Response<Body> {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&body).expect("body serialises"),
                ))
                .expect("request builds"),
        )
        .await
        .expect("the router does not fail to produce a response")
}

fn enroll_request(repo_id: &str, git_repo: &str, advance_id: &str) -> Value {
    json!({
        "repo": { "id": repo_id, "name": "Totem", "git_repo": git_repo },
        "systems": [{ "id": "058-totem-core", "name": "Totem Core" }],
        "components": [],
        "advances": [
            {
                "id": advance_id,
                "system": "058-totem-core",
                "title": "A landscape advance",
                "status": "planned",
                "components": []
            }
        ],
        "source": "test:landscape-events",
    })
}

/// One assembled `text/event-stream` frame (everything up to the blank line
/// that terminates it), read from the response body as more chunks arrive —
/// deliberately not assuming one `http_body` frame equals one SSE event, so
/// this stays correct even if the transport ever coalesces or splits writes
/// differently than it does today.
async fn next_sse_event(body: &mut Body, timeout: Duration) -> String {
    let mut buf = Vec::new();
    tokio::time::timeout(timeout, async {
        loop {
            if let Some(pos) = find_blank_line(&buf) {
                return String::from_utf8(buf[..pos].to_vec()).expect("frame is utf-8");
            }
            let frame = body
                .frame()
                .await
                .expect("the stream must not end before an event arrives")
                .expect("reading a body frame must not fail");
            if let Some(data) = frame.data_ref() {
                buf.extend_from_slice(data);
            }
        }
    })
    .await
    .expect("an event must arrive before the timeout")
}

fn find_blank_line(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\n\n")
}

fn event_data(frame: &str) -> Value {
    let data_line = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or_else(|| panic!("frame has no `data:` line: {frame:?}"));
    serde_json::from_str(data_line).expect("data line is valid JSON")
}

#[tokio::test]
async fn connecting_delivers_the_current_landscape_immediately() {
    let (router, _store) = common::app().await;
    assert_status(
        &common::post(&router, "/enroll", enroll_request(ARRIVE_ID, REPO, "ADV-A-001")).await,
        StatusCode::OK,
    );

    let response = get_stream(
        &router,
        &format!("/landscape/{ARRIVE_ID}/events?actor={ADA}&session=sess-1"),
        None,
    )
    .await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .expect("content-type is set"),
        "text/event-stream",
    );

    let mut body = response.into_body();
    let frame = next_sse_event(&mut body, Duration::from_secs(5)).await;
    assert!(
        frame.starts_with("event: landscape\n"),
        "got: {frame:?}"
    );
    let data = event_data(&frame);
    assert_eq!(data["repo"]["id"], ARRIVE_ID);
    assert_eq!(data["advances"][0]["id"], "ADV-A-001");
}

fn advance_ids(data: &Value) -> Vec<String> {
    data["advances"]
        .as_array()
        .expect("advances is an array")
        .iter()
        .map(|advance| advance["id"].as_str().expect("id is a string").to_string())
        .collect()
}

#[tokio::test]
async fn a_later_write_pushes_a_second_event_with_the_updated_view() {
    let (router, _store) = common::app().await;
    assert_status(
        &common::post(&router, "/enroll", enroll_request(ARRIVE_ID, REPO, "ADV-A-001")).await,
        StatusCode::OK,
    );

    let response = get_stream(
        &router,
        &format!("/landscape/{ARRIVE_ID}/events?actor={ADA}&session=sess-1"),
        None,
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let mut body = response.into_body();

    let first = next_sse_event(&mut body, Duration::from_secs(5)).await;
    assert_eq!(advance_ids(&event_data(&first)), vec!["ADV-A-001"]);

    // `sync` never deletes an advance absent from a later snapshot (only the
    // `impacts` edges of advances present *in* that snapshot are replaced —
    // `LandscapeRepository::sync`'s own doc), so a second enroll with a
    // different advance id grows the set rather than replacing it. What this
    // proves is the thing that matters: the second event is a *fresh*
    // store-enforced read (ADV-A-002 present), not a stale repeat of the
    // first — not a specific array position, which `view`'s own query makes
    // no ordering promise about.
    assert_status(
        &common::post(&router, "/enroll", enroll_request(ARRIVE_ID, REPO, "ADV-A-002")).await,
        StatusCode::OK,
    );

    let second = next_sse_event(&mut body, Duration::from_secs(5)).await;
    let ids = advance_ids(&event_data(&second));
    assert!(
        ids.contains(&"ADV-A-002".to_string()),
        "a re-sync must push a fresh view, not repeat the stale one: {ids:?}"
    );
}

#[tokio::test]
async fn every_delivered_event_appends_one_access_log_entry() {
    let (router, store) = common::app().await;
    assert_status(
        &common::post(&router, "/enroll", enroll_request(ARRIVE_ID, REPO, "ADV-A-001")).await,
        StatusCode::OK,
    );

    let response = get_stream(
        &router,
        &format!("/landscape/{ARRIVE_ID}/events?actor={ADA}&session=sess-1"),
        None,
    )
    .await;
    let mut body = response.into_body();
    next_sse_event(&mut body, Duration::from_secs(5)).await;

    assert_status(
        &common::post(&router, "/enroll", enroll_request(ARRIVE_ID, REPO, "ADV-A-002")).await,
        StatusCode::OK,
    );
    next_sse_event(&mut body, Duration::from_secs(5)).await;

    let entries = store.access_log().list().await.expect("list succeeds");
    let relay_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.endpoint == "/landscape/{repo}/events")
        .collect();
    assert_eq!(
        relay_entries.len(),
        2,
        "every relayed view is a read, and every read is logged: {entries:?}"
    );
    for entry in relay_entries {
        assert_eq!(entry.actor.as_ref().map(ToString::to_string).as_deref(), Some(ADA));
    }
}

#[tokio::test]
async fn a_token_bound_to_another_repo_cannot_subscribe() {
    let state = AppState::in_memory()
        .await
        .expect("embedded engine connects and migrations apply");
    let tokens = state.tokens.clone();
    let router = totem_gateway::authenticated_app(state);

    let owner_token = tokens
        .issue(REPO, &format!("project:{REPO}"), ADA, None)
        .expect("a coherent binding issues");
    assert_status(
        &authed_post(
            &router,
            "/enroll",
            &owner_token,
            enroll_request(ARRIVE_ID, REPO, "ADV-A-001"),
        )
        .await,
        StatusCode::OK,
    );

    let other_token = tokens
        .issue(OTHER_REPO, &format!("project:{OTHER_REPO}"), ADA, None)
        .expect("a coherent binding for the other repo issues");
    let response = get_stream(
        &router,
        &format!("/landscape/{ARRIVE_ID}/events?actor={ADA}&session=sess-1"),
        Some(&other_token),
    )
    .await;
    assert_status(&response, StatusCode::FORBIDDEN);
}

fn assert_status(response: &Response<Body>, expected: StatusCode) {
    assert_eq!(
        response.status(),
        expected,
        "unexpected status: {:?}",
        response.status()
    );
}
