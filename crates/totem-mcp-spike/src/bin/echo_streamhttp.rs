//! Runs the `Echo` server over streamable HTTP, mounted at `/mcp` on an axum
//! router. Attach with, e.g.:
//! `npx @modelcontextprotocol/inspector` -> Streamable HTTP -> `http://127.0.0.1:8765/mcp`

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use totem_mcp_spike::Echo;

const BIND_ADDRESS: &str = "127.0.0.1:8765";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = StreamableHttpService::new(
        || Ok(Echo),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(BIND_ADDRESS).await?;
    println!("totem-mcp-spike echo server listening on http://{BIND_ADDRESS}/mcp");
    axum::serve(listener, router).await?;
    Ok(())
}
