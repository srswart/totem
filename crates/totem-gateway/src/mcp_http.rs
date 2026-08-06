//! MCP over streamable HTTP, for cloud agents (ADV-GATEWAY-003).
//!
//! [docs/tech-direction/mcp.md](../../../docs/tech-direction/mcp.md) MCP-003
//! and MCP-004 established this as the transport cloud harnesses actually
//! reach: Claude Code names streamable HTTP "the recommended option for
//! connecting to remote MCP servers", and the Claude API's MCP connector
//! accepts *only* streamable HTTP or SSE at a public HTTPS URL — a stdio
//! deployment is unreachable from that path entirely. MCP-001 already executed
//! this exact mount shape (`StreamableHttpService` on an `axum::Router`)
//! end-to-end, so this is the spike's finding put into the real gateway.
//!
//! The service is only ever built through [`routes`], and [`routes`] only ever
//! builds a token-bound [`TotemMcp`]. The two cannot be separated by a later
//! edit that mounts the tool surface and forgets the credential layer:
//! `TotemMcp::token_bound` refuses any call that arrives without a verified
//! [`Caller`](crate::auth::Caller), whether or not the layer is present.

use axum::Router;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::mcp::TotemMcp;
use crate::state::AppState;

/// `POST /mcp`: the MCP tool surface over streamable HTTP.
///
/// Merged into [`crate::authenticated_app`], never mounted on its own.
pub(crate) fn routes(state: AppState) -> Router {
    // `StreamableHttpServerConfig` is `#[non_exhaustive]`, so it is built by
    // mutation rather than struct-update syntax.
    let mut config = StreamableHttpServerConfig::default();
    // Plain JSON replies for simple request/response tools. MCP-001 observed
    // that the default frames even a trivial response as `text/event-stream`;
    // Totem's tools are all request/response, and rmcp still falls back to SSE
    // by itself if a handler ever emits something mid-call.
    config.json_response = true;
    // `allowed_hosts` is left at its loopback-only default on purpose: a
    // public deployment must widen it to its own hostnames, and that belongs
    // with the deployment that has one (ADV-INFRA-001), not hard-coded here.

    let service = StreamableHttpService::new(
        move || Ok(TotemMcp::token_bound(state.clone())),
        LocalSessionManager::default().into(),
        config,
    );

    Router::new().nest_service("/mcp", service)
}
