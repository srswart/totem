//! `GET /landscape/:repo`: the REST twin of `totem_landscape`
//! (ADV-GATEWAY-002's MCP tool), added so `totem-console` (ADV-CONSOLE-001)
//! has a way to read the landscape without speaking MCP over stdio.
//!
//! Deliberately reuses the same `totem-store` call the MCP tool already uses
//! (`crates/totem-gateway/src/mcp.rs::totem_landscape`) rather than a second
//! implementation, so the two surfaces cannot silently diverge.

mod common;

use axum::http::StatusCode;
use common::{assert_status, get, json_body};
use serde_json::json;

fn enroll_request(repo_id: &str, source: &str) -> serde_json::Value {
    json!({
        "repo": { "id": repo_id, "name": "Totem", "git_repo": "srswart/totem" },
        "systems": [
            { "id": "058-totem-core", "name": "Totem Core" }
        ],
        "components": [
            {
                "id": "console",
                "system": "058-totem-core",
                "name": "Totem Console",
                "stage": "incubating",
                "owners": [{ "id": "team:058-totem", "name": "058-totem" }]
            }
        ],
        "advances": [
            {
                "id": "ADV-CONSOLE-001",
                "system": "058-totem-core",
                "title": "Landscape dashboard + memory browser",
                "status": "in_progress",
                "components": ["console"]
            }
        ],
        "source": source,
    })
}

#[tokio::test]
async fn a_repo_that_has_never_been_synced_returns_an_empty_landscape_not_an_error() {
    let (router, _store) = common::app().await;

    let response = get(&router, "/landscape/never-synced").await;
    assert_status(&response, StatusCode::OK);

    let body: serde_json::Value = json_body(response).await;
    assert!(body["repo"].is_null());
    assert_eq!(body["systems"], json!([]));
    assert_eq!(body["components"], json!([]));
    assert_eq!(body["advances"], json!([]));
}

#[tokio::test]
async fn a_synced_repo_s_landscape_is_readable_over_rest() {
    let (router, _store) = common::app().await;

    assert_status(
        &common::post(
            &router,
            "/enroll",
            enroll_request("058-totem", "cli:enroll"),
        )
        .await,
        StatusCode::OK,
    );

    let response = get(&router, "/landscape/058-totem").await;
    assert_status(&response, StatusCode::OK);

    let body: serde_json::Value = json_body(response).await;
    assert_eq!(body["repo"]["id"], "058-totem");
    assert_eq!(body["systems"].as_array().expect("array").len(), 1);
    assert_eq!(body["components"].as_array().expect("array").len(), 1);
    assert_eq!(body["advances"].as_array().expect("array").len(), 1);
    assert_eq!(body["components"][0]["id"], "console");
    assert_eq!(body["advances"][0]["id"], "ADV-CONSOLE-001");
}
