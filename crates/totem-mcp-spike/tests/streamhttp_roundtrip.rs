//! Drives a real rmcp client against a real in-process axum server exposing
//! `Echo` over streamable HTTP — a genuine HTTP round trip on loopback, no
//! mocking. The server binds an OS-assigned port so this never collides.

use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{
        StreamableHttpClientTransport,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use totem_mcp_spike::Echo;

#[tokio::test]
async fn echo_tool_round_trips_over_streamable_http() {
    let service = StreamableHttpService::new(
        || Ok(Echo),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum::serve");
    });

    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ().serve(transport).await.expect("initialize MCP session over streamable HTTP");

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools over streamable HTTP");
    assert!(
        tools.tools.iter().any(|t| t.name == "echo"),
        "expected an `echo` tool, got {tools:?}"
    );

    let result = client
        .call_tool(
            CallToolRequestParams::new("echo").with_arguments(
                serde_json::json!({ "text": "hello over streamable http" })
                    .as_object()
                    .cloned()
                    .expect("object"),
            ),
        )
        .await
        .expect("call_tool(echo) over streamable HTTP");

    let text = result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .map(|t| t.text.as_str());
    assert_eq!(text, Some("hello over streamable http"));

    client.cancel().await.expect("clean shutdown");

    server.abort();
    match server.await {
        Ok(()) => {}
        Err(e) if e.is_cancelled() => {}
        Err(e) => panic!("echo_streamhttp server task failed: {e}"),
    }
}
