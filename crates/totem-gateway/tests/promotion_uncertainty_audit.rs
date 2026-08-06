//! `/promotions*`, `/uncertainty*`, and `/audit/:id` (ADV-CONSOLE-002): the
//! promotion-approval queue, the Uncertainty resolution queue, and the audit
//! trail viewer the console fronts.

mod common;

use axum::http::StatusCode;
use common::{ADA, GRACE, assert_status, json_body, post, save_request};
use serde_json::json;
use totem_core::AccessOperation;

fn reader(actor: &str) -> serde_json::Value {
    json!({
        "actor": actor,
        "project": common::REPO,
        "teams": [],
        "harness": "console",
        "session": "sess-1",
        "turn": null,
    })
}

fn propose_request(memory_id: &str, to: &str, author: &str) -> serde_json::Value {
    json!({
        "project": common::REPO,
        "teams": [],
        "memory_id": memory_id,
        "to": to,
        "author": { "kind": "agent", "actor": author },
        "harness": "claude_code",
        "session": "sess-1",
        "turn": null,
    })
}

fn decision_request(approver: &str, reason: Option<&str>) -> serde_json::Value {
    json!({
        "project": common::REPO,
        "teams": [],
        "author": { "kind": "human", "actor": approver },
        "harness": "console",
        "session": "sess-review",
        "turn": null,
        "reason": reason,
    })
}

fn resolve_request(actor: &str, decision: &str) -> serde_json::Value {
    json!({
        "actor": actor,
        "project": common::REPO,
        "teams": [],
        "decision": decision,
        "harness": "console",
        "session": "sess-review",
        "turn": null,
    })
}

async fn save_and_get_id(router: &axum::Router, actor: &str, scope: &str, body: &str) -> String {
    let saved: serde_json::Value =
        json_body(post(router, "/save", save_request(actor, scope, body)).await).await;
    saved["id"].as_str().expect("id is a string").to_string()
}

/// [`save_and_get_id`] always writes Knowledge (`save_request`'s fixed
/// category), which is Automatic and cannot exercise the human-gated queue —
/// this writes Instructions instead.
async fn save_instructions_and_get_id(
    router: &axum::Router,
    actor: &str,
    scope: &str,
    body: &str,
) -> String {
    let saved: serde_json::Value = json_body(
        post(
            router,
            "/save",
            json!({
                "project": common::REPO,
                "teams": [],
                "category": "instructions",
                "scope": scope,
                "subject": null,
                "body": body,
                "tags": [],
                "author": { "kind": "agent", "actor": actor },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    saved["id"].as_str().expect("id is a string").to_string()
}

#[tokio::test]
async fn a_knowledge_promotion_from_a_private_scope_happens_at_once() {
    let (router, _store) = common::app().await;
    let memory_id = save_and_get_id(&router, ADA, "actor:ada", "a note worth sharing").await;

    let response = post(
        &router,
        "/promotions",
        propose_request(&memory_id, "project:srswart/totem", ADA),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["outcome"], "promoted");
    assert_eq!(body["proposal"]["memory"], memory_id);
}

#[tokio::test]
async fn an_instructions_promotion_queues_for_a_human_and_the_queue_lists_it() {
    let (router, _store) = common::app().await;
    let memory_id = save_instructions_and_get_id(
        &router,
        ADA,
        "actor:ada",
        "run cargo fmt before every commit",
    )
    .await;

    let response = post(
        &router,
        "/promotions",
        json!({
            "project": common::REPO,
            "teams": [],
            "memory_id": memory_id,
            "to": "project:srswart/totem",
            "author": { "kind": "human", "actor": ADA },
            "harness": "console",
            "session": "sess-1",
            "turn": null,
        }),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["outcome"], "pending");

    let queue: serde_json::Value =
        json_body(post(&router, "/promotions/pending", reader(ADA)).await).await;
    let pending = queue["pending"].as_array().expect("pending is an array");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["memory"], memory_id);
}

#[tokio::test]
async fn a_reviewer_can_read_the_record_a_pending_proposal_names() {
    let (router, _store) = common::app().await;
    // Ada writes an Instructions note nobody else can see yet...
    let memory_id =
        save_instructions_and_get_id(&router, ADA, "actor:ada", "review PRs within a day").await;
    let proposed: serde_json::Value = json_body(
        post(
            &router,
            "/promotions",
            json!({
                "project": common::REPO,
                "teams": [],
                "memory_id": memory_id,
                "to": "project:srswart/totem",
                "author": { "kind": "human", "actor": ADA },
                "harness": "console",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    let proposal_id = proposed["proposal"]["id"].as_str().expect("id is a string");

    // ...but grace, a project member who can reach the target scope, can read
    // the proposed record while it is open.
    let response = post(
        &router,
        &format!("/promotions/{proposal_id}/record"),
        reader(GRACE),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["record"]["content"]["body"], "review PRs within a day");
}

#[tokio::test]
async fn approving_a_pending_promotion_moves_the_record_and_is_recorded() {
    let (router, store) = common::app().await;
    let memory_id =
        save_instructions_and_get_id(&router, ADA, "actor:ada", "review PRs within a day").await;
    let proposed: serde_json::Value = json_body(
        post(
            &router,
            "/promotions",
            json!({
                "project": common::REPO,
                "teams": [],
                "memory_id": memory_id,
                "to": "project:srswart/totem",
                "author": { "kind": "human", "actor": ADA },
                "harness": "console",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    let proposal_id = proposed["proposal"]["id"].as_str().expect("id is a string");

    let response = post(
        &router,
        &format!("/promotions/{proposal_id}/approve"),
        decision_request(GRACE, None),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["decision"]["kind"], "approved");

    let reader = totem_core::ScopeChain::resolve(
        &totem_core::ActorId::new(ADA).expect("valid actor id"),
        Some(&totem_core::RepoId::new(common::REPO).expect("valid repo id")),
        &[],
    );
    let moved = store
        .memories()
        .get(&reader, memory_id.parse().expect("valid memory id"))
        .await
        .expect("get succeeds")
        .expect("the record still exists");
    assert_eq!(
        moved.scope,
        totem_core::Scope::Project(totem_core::RepoId::new(common::REPO).expect("valid repo id"))
    );
}

#[tokio::test]
async fn rejecting_a_pending_promotion_leaves_the_record_where_it_was() {
    let (router, _store) = common::app().await;
    let memory_id =
        save_instructions_and_get_id(&router, ADA, "actor:ada", "a private draft rule").await;
    let proposed: serde_json::Value = json_body(
        post(
            &router,
            "/promotions",
            json!({
                "project": common::REPO,
                "teams": [],
                "memory_id": memory_id,
                "to": "project:srswart/totem",
                "author": { "kind": "human", "actor": ADA },
                "harness": "console",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    let proposal_id = proposed["proposal"]["id"].as_str().expect("id is a string");

    let response = post(
        &router,
        &format!("/promotions/{proposal_id}/reject"),
        decision_request(GRACE, Some("not ready for the whole project yet")),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["decision"]["kind"], "rejected");

    let queue: serde_json::Value =
        json_body(post(&router, "/promotions/pending", reader(ADA)).await).await;
    assert!(
        queue["pending"]
            .as_array()
            .expect("pending is an array")
            .is_empty(),
        "a decided proposal must leave the queue: {queue:?}"
    );
}

#[tokio::test]
async fn an_uncontested_project_scope_cannot_be_proposed_into_by_a_non_member() {
    let (router, _store) = common::app().await;
    let memory_id = save_and_get_id(&router, GRACE, "actor:grace", "grace's private note").await;

    let response = post(
        &router,
        "/promotions",
        propose_request(&memory_id, "actor:ada", GRACE),
    )
    .await;
    // Widening into a scope ada does not share with grace: not a widening
    // move at all (two disjoint actor scopes), so the policy itself refuses.
    assert!(
        response.status().is_client_error(),
        "expected a client error, got {:?}",
        response.status()
    );
}

#[tokio::test]
async fn an_uncertainty_record_starts_in_the_pending_queue_and_resolving_it_clears_the_queue() {
    let (router, store) = common::app().await;
    let original_id = save_and_get_id(
        &router,
        ADA,
        "project:srswart/totem",
        "the deploy runs on Tuesdays",
    )
    .await;

    let contested: serde_json::Value = json_body(
        post(
            &router,
            "/contest",
            json!({
                "project": common::REPO,
                "teams": [],
                "memory_id": original_id,
                "scope": "project:srswart/totem",
                "claim": "actually the deploy runs on Thursdays",
                "tags": [],
                "author": { "kind": "agent", "actor": ADA },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    let uncertainty_id = contested["id"].as_str().expect("id is a string");

    let queue: serde_json::Value =
        json_body(post(&router, "/uncertainty/pending", reader(ADA)).await).await;
    let pending = queue["pending"].as_array().expect("pending is an array");
    assert!(
        pending.iter().any(|record| record["id"] == uncertainty_id),
        "expected the new Uncertainty record in the queue: {queue:?}"
    );

    let response = post(
        &router,
        &format!("/uncertainty/{uncertainty_id}/resolve"),
        resolve_request(ADA, "approved"),
    )
    .await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["record"]["governance"]["review"], "approved");

    let queue_after: serde_json::Value =
        json_body(post(&router, "/uncertainty/pending", reader(ADA)).await).await;
    assert!(
        queue_after["pending"]
            .as_array()
            .expect("pending is an array")
            .iter()
            .all(|record| record["id"] != uncertainty_id),
        "a resolved record must leave the queue: {queue_after:?}"
    );

    let entries = store.access_log().list().await.expect("list succeeds");
    assert!(
        entries
            .iter()
            .any(|entry| entry.operation == AccessOperation::Resolve),
        "expected a Resolve access-log entry: {entries:?}"
    );
}

#[tokio::test]
async fn resolving_an_uncertainty_record_a_second_time_is_a_conflict() {
    let (router, _store) = common::app().await;
    let original_id = save_and_get_id(
        &router,
        ADA,
        "project:srswart/totem",
        "the deploy runs on Tuesdays",
    )
    .await;
    let contested: serde_json::Value = json_body(
        post(
            &router,
            "/contest",
            json!({
                "project": common::REPO,
                "teams": [],
                "memory_id": original_id,
                "scope": "project:srswart/totem",
                "claim": "actually the deploy runs on Thursdays",
                "tags": [],
                "author": { "kind": "agent", "actor": ADA },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }),
        )
        .await,
    )
    .await;
    let uncertainty_id = contested["id"].as_str().expect("id is a string");

    assert_status(
        &post(
            &router,
            &format!("/uncertainty/{uncertainty_id}/resolve"),
            resolve_request(ADA, "rejected"),
        )
        .await,
        StatusCode::OK,
    );

    let second = post(
        &router,
        &format!("/uncertainty/{uncertainty_id}/resolve"),
        resolve_request(ADA, "approved"),
    )
    .await;
    assert_status(&second, StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_audit_trail_reports_the_record_and_its_access_history() {
    let (router, _store) = common::app().await;
    let memory_id = save_and_get_id(
        &router,
        ADA,
        "project:srswart/totem",
        "a note worth auditing",
    )
    .await;

    // A recall touches the record too, so the audit trail has more than one
    // entry to show.
    assert_status(
        &post(&router, "/recall", common::recall_request(ADA)).await,
        StatusCode::OK,
    );

    let response = post(&router, &format!("/audit/{memory_id}"), reader(ADA)).await;
    assert_status(&response, StatusCode::OK);
    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["record"]["id"], memory_id);
    let access_log = body["access_log"]
        .as_array()
        .expect("access_log is an array");
    assert!(
        access_log
            .iter()
            .any(|entry| entry["operation"] == "save" && entry["memory_id"] == memory_id),
        "expected the /save entry in the audit trail: {access_log:?}"
    );
    assert!(body["curation_history"].as_array().is_some());
    assert!(body["promotion_history"].as_array().is_some());
}

#[tokio::test]
async fn the_audit_trail_for_a_record_outside_the_readers_chain_is_not_found() {
    let (router, _store) = common::app().await;
    let memory_id = save_and_get_id(&router, GRACE, "actor:grace", "grace's private note").await;

    let response = post(&router, &format!("/audit/{memory_id}"), reader(ADA)).await;
    assert_status(&response, StatusCode::NOT_FOUND);
}
