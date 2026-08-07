//! Every MCP tool call over streamable HTTP terminates (ADV-GATEWAY-015).
//!
//! A `totem_save` through the claude.ai connector completed server-side while
//! the client waited indefinitely. That is worse than a clean failure: an
//! agent that sees no acknowledgement retries, and a retried save writes a
//! second record — silent duplicate memories in a store whose value rests on
//! being trustworthy.
//!
//! **These tests drive a real rmcp client over real HTTP**, because the
//! existing MCP tests call the handler in-process with typed arguments and
//! therefore cannot observe transport behaviour at all. That blind spot has
//! now produced three defects in two days (the published schema, the
//! unauthenticated clients, the CLI's missing TLS).

mod common;

use std::time::Duration;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::json;

/// Anything longer than this is a hang, not slowness: these calls are a
/// loopback round trip against an in-memory store.
const PATIENCE: Duration = Duration::from_secs(10);

/// The **authenticated** application, because `/mcp` is only mounted there —
/// which is also the composition an external client meets. Returns the URI
/// and the credential to present.
async fn spawn_gateway() -> (String, String) {
    let store = totem_store::Store::in_memory().await.expect("store");
    store.migrate().await.expect("migrate");
    let state = totem_gateway::AppState::over(store);
    let token = state
        .tokens
        .issue(
            common::REPO,
            &format!("project:{}", common::REPO),
            "ada",
            None,
        )
        .expect("issue credential");
    let router = totem_gateway::authenticated_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (format!("http://{addr}/mcp"), token)
}

/// A transport carrying the bearer credential, the way a real client does.
fn transport_for(uri: String, token: &str) -> StreamableHttpClientTransport<reqwest::Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(uri);
    // rmcp applies `bearer_auth` itself, so this is the bare token: a
    // "Bearer " prefix here would be doubled and refused.
    config.auth_header = Some(token.to_string());
    StreamableHttpClientTransport::with_client(reqwest::Client::new(), config)
}

fn save_args() -> serde_json::Map<String, serde_json::Value> {
    json!({
        "project": common::REPO,
        "teams": [],
        "category": "knowledge",
        "scope": format!("project:{}", common::REPO),
        "body": "A save whose acknowledgement must reach the client.",
        "tags": [],
        "author": {"kind": "human", "actor": "ada"},
        "harness": "claude_code",
        "session": "ack-test",
    })
    .as_object()
    .expect("object")
    .clone()
}

#[tokio::test]
async fn a_save_over_http_returns_its_acknowledgement() {
    let (uri, token) = spawn_gateway().await;
    let transport = transport_for(uri, &token);
    let client = tokio::time::timeout(PATIENCE, ().serve(transport))
        .await
        .expect("initialize must not hang")
        .expect("initialize");

    let result = tokio::time::timeout(
        PATIENCE,
        client.call_tool(CallToolRequestParams::new("totem_save").with_arguments(save_args())),
    )
    .await
    .expect("the save must acknowledge — a write that never answers makes an agent retry")
    .expect("call_tool");

    assert_eq!(result.is_error, Some(false), "{result:?}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn a_recall_over_http_returns_its_acknowledgement() {
    let (uri, token) = spawn_gateway().await;
    let transport = transport_for(uri, &token);
    let client = tokio::time::timeout(PATIENCE, ().serve(transport))
        .await
        .expect("initialize must not hang")
        .expect("initialize");

    let args = json!({
        "project": common::REPO, "actor": "ada", "teams": [],
        "harness": "claude_code", "session": "ack-test",
    })
    .as_object()
    .expect("object")
    .clone();

    let result = tokio::time::timeout(
        PATIENCE,
        client.call_tool(CallToolRequestParams::new("totem_recall").with_arguments(args)),
    )
    .await
    .expect("the recall must acknowledge")
    .expect("call_tool");

    assert_eq!(result.is_error, Some(false), "{result:?}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn consecutive_tool_calls_on_one_session_all_acknowledge() {
    // The reported hang followed earlier successful calls on the same
    // connector session, so a single call proving nothing is not enough.
    let (uri, token) = spawn_gateway().await;
    let transport = transport_for(uri, &token);
    let client = tokio::time::timeout(PATIENCE, ().serve(transport))
        .await
        .expect("initialize must not hang")
        .expect("initialize");

    for attempt in 0..3 {
        let result = tokio::time::timeout(
            PATIENCE,
            client.call_tool(CallToolRequestParams::new("totem_save").with_arguments(save_args())),
        )
        .await
        .unwrap_or_else(|_| panic!("call {attempt} never acknowledged"))
        .expect("call_tool");
        assert_eq!(result.is_error, Some(false), "call {attempt}: {result:?}");
    }
    let _ = client.cancel().await;
}
