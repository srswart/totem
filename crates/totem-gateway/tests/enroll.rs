//! `POST /enroll`: register or re-sync one repo's ARRIVE landscape
//! (docs/solution-intent.md §3.3, §2.3; ADV-CLI-001).
//!
//! `totem-cli`'s `totem enroll` is the intended caller: it parses `/arrive/`
//! locally (`totem_arrive_sync::read_repo_artifacts`) and posts the resulting
//! snapshot here rather than opening its own store connection, since the
//! gateway — not the CLI — owns the store. This test drives the same JSON
//! shape directly, the way any other enrolling client would.

mod common;

use axum::http::StatusCode;
use common::{assert_status, json_body, post};
use serde_json::json;

fn enroll_request(repo_id: &str, source: &str) -> serde_json::Value {
    json!({
        "repo": { "id": repo_id, "name": "Totem", "git_repo": "srswart/totem" },
        "systems": [
            { "id": "058-totem-core", "name": "Totem Core" }
        ],
        "components": [
            {
                "id": "cli",
                "system": "058-totem-core",
                "name": "Totem CLI",
                "stage": "incubating",
                "owners": [{ "id": "team:058-totem", "name": "058-totem" }]
            }
        ],
        "advances": [
            {
                "id": "ADV-CLI-001",
                "system": "058-totem-core",
                "title": "totem CLI",
                "status": "in_progress",
                "components": ["cli"]
            }
        ],
        "source": source,
    })
}

#[tokio::test]
async fn enrolling_syncs_the_landscape_and_reports_a_summary() {
    let (router, store) = common::app().await;

    let response = post(
        &router,
        "/enroll",
        enroll_request("058-totem", "cli:enroll"),
    )
    .await;
    assert_status(&response, StatusCode::OK);

    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["systems"], 1);
    assert_eq!(body["components"], 1);
    assert_eq!(body["advances"], 1);

    let view = store
        .landscape()
        .view("058-totem")
        .await
        .expect("landscape view succeeds");
    assert_eq!(view.repo.expect("repo was synced").id, "058-totem");
    assert_eq!(view.components.len(), 1);
    assert_eq!(view.advances.len(), 1);
}

#[tokio::test]
async fn re_enrolling_the_same_repo_is_idempotent() {
    let (router, store) = common::app().await;

    assert_status(
        &post(
            &router,
            "/enroll",
            enroll_request("058-totem", "cli:enroll"),
        )
        .await,
        StatusCode::OK,
    );
    assert_status(
        &post(
            &router,
            "/enroll",
            enroll_request("058-totem", "cli:enroll"),
        )
        .await,
        StatusCode::OK,
    );

    let view = store
        .landscape()
        .view("058-totem")
        .await
        .expect("landscape view succeeds");
    assert_eq!(view.components.len(), 1, "re-sync must not duplicate rows");
    assert_eq!(view.advances.len(), 1, "re-sync must not duplicate rows");

    let runs = store
        .landscape()
        .sync_runs()
        .await
        .expect("sync_runs succeeds");
    assert_eq!(runs.len(), 2, "each enroll call records its own sync_run");
}

#[tokio::test]
async fn a_malformed_enroll_body_never_reaches_the_store() {
    let (router, _store) = common::app().await;

    let response = post(
        &router,
        "/enroll",
        json!({ "repo": { "id": "058-totem" }, "source": "cli:enroll" }),
    )
    .await;
    assert!(
        response.status().is_client_error(),
        "expected a 4xx response for a missing required field, got {:?}",
        response.status(),
    );
}
