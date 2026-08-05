//! Runs the `Echo` server over stdio. Attach with, e.g.:
//! `npx @modelcontextprotocol/inspector cargo run -p totem-mcp-spike --bin echo_stdio`

use rmcp::{ServiceExt, transport::stdio};
use totem_mcp_spike::Echo;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = Echo.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
