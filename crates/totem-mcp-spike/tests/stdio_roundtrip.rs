//! Drives a real rmcp client against the compiled `echo_stdio` binary over a
//! real stdio child-process transport — no mocking, no hand-rolled JSON-RPC.

use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use tokio::process::Command;

#[tokio::test]
async fn echo_tool_round_trips_over_stdio() {
    let transport =
        TokioChildProcess::new(Command::new(env!("CARGO_BIN_EXE_echo_stdio")).configure(|_cmd| {}))
            .expect("spawn echo_stdio child process");
    let client = ().serve(transport).await.expect("initialize MCP session over stdio");

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools over stdio");
    assert!(
        tools.tools.iter().any(|t| t.name == "echo"),
        "expected an `echo` tool, got {tools:?}"
    );

    let result = client
        .call_tool(
            CallToolRequestParams::new("echo").with_arguments(
                serde_json::json!({ "text": "hello over stdio" })
                    .as_object()
                    .cloned()
                    .expect("object"),
            ),
        )
        .await
        .expect("call_tool(echo) over stdio");

    let text = result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .map(|t| t.text.as_str());
    assert_eq!(text, Some("hello over stdio"));

    client.cancel().await.expect("clean shutdown");
}
