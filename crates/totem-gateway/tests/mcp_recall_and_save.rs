//! `totem_recall`/`totem_save`/`totem_landscape` over a real stdio MCP
//! transport — the same behaviors `tests/recall_and_save.rs` proves for REST,
//! proven again here so "MCP tool calls route through the same core API as
//! REST" (`lib.rs`'s doc comment) is verified by execution, not just claimed
//! (ADV-GATEWAY-002). Each test spawns its own `mcp_stdio` child process, so
//! each gets its own fresh, isolated embedded store — the same isolation
//! `tests/recall_and_save.rs` gets from building its own router per test.

use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Map, Value, json};
use tokio::process::Command;

const ADA: &str = "ada";
const GRACE: &str = "grace";
const REPO: &str = "srswart/totem";

async fn client() -> RunningService<RoleClient, ()> {
    let transport =
        TokioChildProcess::new(Command::new(env!("CARGO_BIN_EXE_mcp_stdio")).configure(|_cmd| {}))
            .expect("spawn mcp_stdio child process");
    ().serve(transport)
        .await
        .expect("initialize MCP session over stdio")
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("value is a JSON object")
}

fn save_args(actor: &str, scope: &str, body: &str) -> Map<String, Value> {
    object(json!({
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
    }))
}

fn recall_args(actor: &str) -> Map<String, Value> {
    object(json!({
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
    }))
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_else(|| panic!("tool call returned no text content: {result:?}"))
}

fn json_of(result: &CallToolResult) -> Value {
    serde_json::from_str(&text_of(result)).expect("tool call's text content is valid JSON")
}

#[tokio::test]
async fn the_full_tool_surface_is_advertised() {
    let client = client().await;

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools over stdio");
    for name in ["totem_recall", "totem_save", "totem_landscape"] {
        assert!(
            tools.tools.iter().any(|tool| tool.name == name),
            "expected a `{name}` tool, got {tools:?}"
        );
    }

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_save_and_totem_recall_round_trip_over_stdio() {
    let client = client().await;

    let save_result = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                ADA,
                "project:srswart/totem",
                "run `cargo fmt` before pushing",
            )),
        )
        .await
        .expect("call_tool(totem_save)");
    assert_ne!(
        save_result.is_error,
        Some(true),
        "totem_save reported an error: {save_result:?}"
    );
    let saved = json_of(&save_result);
    let saved_id = saved["id"].as_str().expect("id is a string").to_string();

    let recall_result = client
        .call_tool(CallToolRequestParams::new("totem_recall").with_arguments(recall_args(ADA)))
        .await
        .expect("call_tool(totem_recall)");
    assert_ne!(
        recall_result.is_error,
        Some(true),
        "totem_recall reported an error: {recall_result:?}"
    );
    let records = json_of(&recall_result);
    let records = records
        .as_array()
        .expect("totem_recall returns a JSON array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], Value::String(saved_id));
    assert_eq!(
        records[0]["content"]["body"],
        Value::String("run `cargo fmt` before pushing".to_string())
    );

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_recall_never_returns_another_actors_private_memory() {
    let client = client().await;

    let ada_save = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                ADA,
                "actor:ada",
                "ada's private note",
            )),
        )
        .await
        .expect("call_tool(totem_save) for ada");
    assert_ne!(ada_save.is_error, Some(true), "{ada_save:?}");

    let grace_save = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                GRACE,
                "actor:grace",
                "grace's private note",
            )),
        )
        .await
        .expect("call_tool(totem_save) for grace");
    assert_ne!(grace_save.is_error, Some(true), "{grace_save:?}");

    let recall_result = client
        .call_tool(CallToolRequestParams::new("totem_recall").with_arguments(recall_args(ADA)))
        .await
        .expect("call_tool(totem_recall)");
    let records = json_of(&recall_result);
    let bodies: Vec<&str> = records
        .as_array()
        .expect("array")
        .iter()
        .map(|record| {
            record["content"]["body"]
                .as_str()
                .expect("body is a string")
        })
        .collect();
    assert_eq!(bodies, vec!["ada's private note"]);

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn writing_into_another_actors_scope_over_mcp_is_refused() {
    let client = client().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                ADA,
                "actor:grace",
                "planted in grace's scope",
            )),
        )
        .await;
    assert!(
        result.is_err(),
        "expected a protocol-level error for a denied scope, got {result:?}"
    );

    client.cancel().await.expect("clean shutdown");
}

/// Each test's `mcp_stdio` child process starts a fresh, private in-memory
/// store (docs/tech-direction/surrealdb.md §4) with nothing synced into it,
/// so this proves the honest "nothing here yet" case — real query, real
/// empty result, not a hardcoded stub. Proving the populated case (a real
/// sync feeding a real `totem_landscape` answer) needs a store the test can
/// seed before the tool call, which this stdio-child-process harness cannot
/// do; that path is covered instead by `totem-store`'s
/// `tests/landscape_sync.rs` and `totem-arrive-sync`'s
/// `tests/dogfood.rs::syncing_this_repos_landscape_populates_the_store_and_is_queryable_in_one_round_trip`.
#[tokio::test]
async fn totem_landscape_returns_an_empty_landscape_for_an_unsynced_repo() {
    let client = client().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("totem_landscape")
                .with_arguments(object(json!({ "repo": "058-totem" }))),
        )
        .await
        .expect("call_tool(totem_landscape)");
    assert_ne!(result.is_error, Some(true), "{result:?}");

    let landscape = json_of(&result);
    assert_eq!(landscape["repo"], Value::Null);
    assert_eq!(landscape["systems"], json!([]));
    assert_eq!(landscape["components"], json!([]));
    assert_eq!(landscape["advances"], json!([]));

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_landscape_without_a_repo_id_is_a_protocol_level_error() {
    let client = client().await;

    let result = client
        .call_tool(CallToolRequestParams::new("totem_landscape").with_arguments(object(json!({}))))
        .await;
    assert!(
        result.is_err(),
        "expected a protocol-level error for a missing `repo`, got {result:?}"
    );

    client.cancel().await.expect("clean shutdown");
}
