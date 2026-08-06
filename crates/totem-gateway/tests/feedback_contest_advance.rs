//! `/feedback`, `/contest`, `/advance/log`, and `/advance/:id/status`
//! (ADV-GATEWAY-004 gap-fill): the four MCP-tool-backing REST endpoints this
//! advance adds, over HTTP — the same coverage `tests/recall_and_save.rs`
//! gives `/recall`/`/save`.

mod common;

use axum::http::StatusCode;
use common::{ADA, GRACE, assert_status, get, json_body, post, save_request};
use serde_json::json;

fn feedback_request(actor: &str, memory_id: &str, signal: &str) -> serde_json::Value {
    json!({
        "actor": actor,
        "project": common::REPO,
        "teams": [],
        "memory_id": memory_id,
        "signal": signal,
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

fn contest_request(memory_id: &str, scope: &str, claim: &str) -> serde_json::Value {
    json!({
        "project": common::REPO,
        "teams": [],
        "memory_id": memory_id,
        "scope": scope,
        "claim": claim,
        "tags": [],
        "author": { "kind": "agent", "actor": ADA },
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

fn advance_log_request(advance_id: &str, scope: &str, body: &str) -> serde_json::Value {
    json!({
        "project": common::REPO,
        "teams": [],
        "advance_id": advance_id,
        "scope": scope,
        "body": body,
        "tags": [],
        "author": { "kind": "agent", "actor": ADA },
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

#[tokio::test]
async fn a_used_signal_raises_value_score_over_http() {
    let (router, _store) = common::app().await;

    let saved: serde_json::Value = json_body(
        post(
            &router,
            "/save",
            save_request(ADA, "project:srswart/totem", "a note worth reinforcing"),
        )
        .await,
    )
    .await;
    let memory_id = saved["id"].as_str().expect("id is a string");

    let response = post(
        &router,
        "/feedback",
        feedback_request(ADA, memory_id, "used"),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert!(
        body["record"]["economics"]["value_score"]
            .as_f64()
            .expect("value_score is a number")
            > 1.0
    );
}

#[tokio::test]
async fn feedback_on_a_memory_outside_the_callers_chain_is_not_found() {
    let (router, _store) = common::app().await;

    let saved: serde_json::Value = json_body(
        post(
            &router,
            "/save",
            save_request(GRACE, "actor:grace", "grace's private note"),
        )
        .await,
    )
    .await;
    let memory_id = saved["id"].as_str().expect("id is a string");

    let response = post(
        &router,
        "/feedback",
        feedback_request(ADA, memory_id, "used"),
    )
    .await;
    assert_status(&response, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn contesting_a_memory_preserves_the_original_and_files_an_uncertainty_record() {
    let (router, store) = common::app().await;

    let saved: serde_json::Value = json_body(
        post(
            &router,
            "/save",
            save_request(ADA, "project:srswart/totem", "the deploy runs on Tuesdays"),
        )
        .await,
    )
    .await;
    let memory_id = saved["id"].as_str().expect("id is a string").to_string();

    let response = post(
        &router,
        "/contest",
        contest_request(
            &memory_id,
            "project:srswart/totem",
            "actually the deploy runs on Thursdays",
        ),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    let contest_id = body["id"].as_str().expect("id is a string");
    assert_ne!(
        contest_id, memory_id,
        "contest must not reuse the original id"
    );

    let reader = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new(ADA).expect("valid actor id"),
        Some(&totem_core::RepoId::new(common::REPO).expect("valid repo id")),
        &[],
    );
    let original = store
        .memories()
        .get(&reader, memory_id.parse().expect("valid memory id"))
        .await
        .expect("get succeeds")
        .expect("the original record is untouched");
    assert_eq!(original.content.body, "the deploy runs on Tuesdays");

    let uncertainty = store
        .memories()
        .get(&reader, contest_id.parse().expect("valid memory id"))
        .await
        .expect("get succeeds")
        .expect("the uncertainty record exists");
    assert_eq!(
        uncertainty.category,
        totem_core::MemoryCategory::Uncertainty
    );
    assert_eq!(
        uncertainty.content.body,
        "actually the deploy runs on Thursdays"
    );
    assert_eq!(
        uncertainty.subject,
        Some(
            totem_core::SubjectRef::new(totem_core::SubjectKind::Memory, memory_id)
                .expect("valid subject")
        )
    );
}

#[tokio::test]
async fn contesting_an_id_outside_the_writers_chain_is_not_found() {
    let (router, _store) = common::app().await;

    let saved: serde_json::Value = json_body(
        post(
            &router,
            "/save",
            save_request(GRACE, "actor:grace", "grace's private note"),
        )
        .await,
    )
    .await;
    let memory_id = saved["id"].as_str().expect("id is a string");

    let response = post(
        &router,
        "/contest",
        contest_request(
            memory_id,
            "actor:ada",
            "a claim about a note ada cannot see",
        ),
    )
    .await;
    assert_status(&response, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn advance_log_writes_an_episodic_record_linked_to_the_advance() {
    let (router, store) = common::app().await;

    let response = post(
        &router,
        "/advance/log",
        advance_log_request(
            "ADV-GATEWAY-004",
            "project:srswart/totem",
            "implemented the four gap-fill MCP tools",
        ),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    let id = body["id"].as_str().expect("id is a string");

    let reader = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new(ADA).expect("valid actor id"),
        Some(&totem_core::RepoId::new(common::REPO).expect("valid repo id")),
        &[],
    );
    let record = store
        .memories()
        .get(&reader, id.parse().expect("valid memory id"))
        .await
        .expect("get succeeds")
        .expect("the log entry exists");
    assert_eq!(record.category, totem_core::MemoryCategory::Episodic);
    assert_eq!(
        record.subject,
        Some(
            totem_core::SubjectRef::new(totem_core::SubjectKind::Advance, "ADV-GATEWAY-004")
                .expect("valid subject")
        )
    );
}

#[tokio::test]
async fn advance_log_with_an_empty_advance_id_is_a_client_error() {
    let (router, _store) = common::app().await;

    let response = post(
        &router,
        "/advance/log",
        advance_log_request("", "project:srswart/totem", "a log entry"),
    )
    .await;
    assert!(
        response.status().is_client_error(),
        "expected a 4xx response for an empty advance_id, got {:?}",
        response.status(),
    );
}

#[tokio::test]
async fn advance_status_reads_a_synced_advance_by_id() {
    let (router, _store) = common::app().await;

    assert_status(
        &post(
            &router,
            "/enroll",
            json!({
                "repo": { "id": "058-totem", "name": "Totem" },
                "systems": [{ "id": "058-totem-core", "name": "Totem Core" }],
                "components": [{
                    "id": "gateway",
                    "system": "058-totem-core",
                    "name": "Totem Gateway",
                    "stage": "incubating",
                    "owners": [],
                }],
                "advances": [{
                    "id": "ADV-GATEWAY-004",
                    "system": "058-totem-core",
                    "title": "MCP tools: feedback, contest, advance_status/advance_log",
                    "status": "in_progress",
                    "components": ["gateway"],
                }],
                "source": "test",
            }),
        )
        .await,
        StatusCode::OK,
    );

    let response = get(&router, "/advance/ADV-GATEWAY-004/status").await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["advance"]["id"], "ADV-GATEWAY-004");
    assert_eq!(body["advance"]["status"], "in_progress");
}

#[tokio::test]
async fn advance_status_for_an_unsynced_id_is_null_not_an_error() {
    let (router, _store) = common::app().await;

    let response = get(&router, "/advance/ADV-NEVER-SYNCED-999/status").await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert!(body["advance"].is_null());
}

#[tokio::test]
async fn feedback_and_contest_each_append_one_access_log_entry() {
    let (router, store) = common::app().await;

    let saved: serde_json::Value = json_body(
        post(
            &router,
            "/save",
            save_request(
                ADA,
                "project:srswart/totem",
                "a note worth signalling about",
            ),
        )
        .await,
    )
    .await;
    let memory_id = saved["id"].as_str().expect("id is a string").to_string();

    assert_status(
        &post(
            &router,
            "/feedback",
            feedback_request(ADA, &memory_id, "used"),
        )
        .await,
        StatusCode::OK,
    );
    assert_status(
        &post(
            &router,
            "/contest",
            contest_request(&memory_id, "project:srswart/totem", "a conflicting claim"),
        )
        .await,
        StatusCode::OK,
    );

    let entries = store.access_log().list().await.expect("list succeeds");
    // One /save, one /feedback, one /contest (contest is itself a save).
    assert_eq!(
        entries.len(),
        3,
        "expected one entry per request: {entries:?}"
    );
    assert_eq!(entries[1].endpoint, "/feedback");
    assert_eq!(entries[2].endpoint, "/contest");
}
