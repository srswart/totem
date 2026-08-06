//! `totem_feedback`, `totem_contest`, `totem_advance_log`, and
//! `totem_advance_status` over a real stdio MCP transport (ADV-GATEWAY-004
//! gap-fill) — the same "MCP tool calls route through the same core API as
//! REST" proof `tests/mcp_recall_and_save.rs` gives the earlier tools, for
//! this advance's four.

use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Map, Value, json};
use tokio::process::Command;

const ADA: &str = "ada";
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
async fn the_gap_fill_tool_surface_is_advertised() {
    let client = client().await;

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools over stdio");
    for name in [
        "totem_feedback",
        "totem_contest",
        "totem_advance_log",
        "totem_advance_status",
    ] {
        assert!(
            tools.tools.iter().any(|tool| tool.name == name),
            "expected a `{name}` tool, got {tools:?}"
        );
    }

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_feedback_raises_value_score_over_stdio() {
    let client = client().await;

    let save_result = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                ADA,
                "project:srswart/totem",
                "a note to reinforce",
            )),
        )
        .await
        .expect("call_tool(totem_save)");
    let saved = json_of(&save_result);
    let memory_id = saved["id"].as_str().expect("id is a string").to_string();

    let feedback_result = client
        .call_tool(
            CallToolRequestParams::new("totem_feedback").with_arguments(object(json!({
                "actor": ADA,
                "project": REPO,
                "teams": [],
                "memory_id": memory_id,
                "signal": "used",
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }))),
        )
        .await
        .expect("call_tool(totem_feedback)");
    assert_ne!(
        feedback_result.is_error,
        Some(true),
        "totem_feedback reported an error: {feedback_result:?}"
    );
    let record = json_of(&feedback_result);
    assert!(
        record["economics"]["value_score"]
            .as_f64()
            .expect("value_score is a number")
            > 1.0
    );

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_contest_preserves_the_original_and_returns_a_new_id() {
    let client = client().await;

    let save_result = client
        .call_tool(
            CallToolRequestParams::new("totem_save").with_arguments(save_args(
                ADA,
                "project:srswart/totem",
                "the deploy runs on Tuesdays",
            )),
        )
        .await
        .expect("call_tool(totem_save)");
    let saved = json_of(&save_result);
    let memory_id = saved["id"].as_str().expect("id is a string").to_string();

    let contest_result = client
        .call_tool(
            CallToolRequestParams::new("totem_contest").with_arguments(object(json!({
                "project": REPO,
                "teams": [],
                "memory_id": memory_id,
                "scope": "project:srswart/totem",
                "claim": "actually the deploy runs on Thursdays",
                "tags": [],
                "author": { "kind": "agent", "actor": ADA },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }))),
        )
        .await
        .expect("call_tool(totem_contest)");
    assert_ne!(
        contest_result.is_error,
        Some(true),
        "totem_contest reported an error: {contest_result:?}"
    );
    let contested = json_of(&contest_result);
    let contest_id = contested["id"].as_str().expect("id is a string");
    assert_ne!(contest_id, memory_id);

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_contest_against_an_invisible_memory_is_a_protocol_level_error() {
    let client = client().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("totem_contest").with_arguments(object(json!({
                "project": REPO,
                "teams": [],
                "memory_id": "00000000-0000-0000-0000-000000000000",
                "scope": "project:srswart/totem",
                "claim": "a claim about nothing",
                "tags": [],
                "author": { "kind": "agent", "actor": ADA },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }))),
        )
        .await;
    assert!(
        result.is_err(),
        "expected a protocol-level error for a nonexistent memory id, got {result:?}"
    );

    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn totem_advance_log_then_totem_advance_status_round_trips_over_stdio() {
    let client = client().await;

    let log_result = client
        .call_tool(
            CallToolRequestParams::new("totem_advance_log").with_arguments(object(json!({
                "project": REPO,
                "teams": [],
                "advance_id": "ADV-GATEWAY-004",
                "scope": "project:srswart/totem",
                "body": "wired the four gap-fill tools",
                "tags": [],
                "author": { "kind": "agent", "actor": ADA },
                "harness": "claude_code",
                "session": "sess-1",
                "turn": null,
            }))),
        )
        .await
        .expect("call_tool(totem_advance_log)");
    assert_ne!(log_result.is_error, Some(true), "{log_result:?}");

    // `totem_advance_status` reads the landscape mirror, not scoped memory —
    // it stays "not yet enrolled" (an empty landscape) until a sync writes
    // it, which this test's fresh store has not done, proving the same
    // honest "nothing here yet" case `totem_landscape`'s own stdio test
    // proves.
    let status_result = client
        .call_tool(
            CallToolRequestParams::new("totem_advance_status")
                .with_arguments(object(json!({ "advance_id": "ADV-GATEWAY-004" }))),
        )
        .await
        .expect("call_tool(totem_advance_status)");
    assert_ne!(status_result.is_error, Some(true), "{status_result:?}");
    let status = json_of(&status_result);
    assert_eq!(status["advance"], Value::Null);

    client.cancel().await.expect("clean shutdown");
}
