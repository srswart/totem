//! The MCP binary: `totem_recall`/`totem_save`/`totem_landscape` over stdio,
//! for desktop harnesses (Claude Code, Cursor). Attach with, e.g.:
//! `npx @modelcontextprotocol/inspector cargo run -p totem-gateway --bin mcp_stdio`
//!
//! Same embedded, non-persistent store as the `totem-gateway` REST binary,
//! and the same open deployment-topology question (`main.rs`'s doc comment,
//! docs/solution-intent.md §9) — this advance's scope is the tool surface,
//! not that decision.

use rmcp::{ServiceExt, transport::stdio};
use totem_gateway::{AppState, TotemMcp};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let state = AppState::in_memory().await?;

    let service = TotemMcp::new(state).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
