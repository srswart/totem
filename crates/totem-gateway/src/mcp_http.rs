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
    // Plain JSON replies — but **only for session-less requests**.
    //
    // ADV-GATEWAY-015 established what this setting actually does in rmcp
    // 3.1.0: it is consulted on the stateless paths only. Once a client holds
    // a session (every client, immediately after `initialize`, because this
    // service is mounted with a `LocalSessionManager`), a POSTed request
    // returns `sse_stream_response(..)` unconditionally. The comment that
    // used to sit here claimed Totem answers tool calls with plain JSON; it
    // does not, and that inaccuracy survived because no test drove the
    // transport.
    //
    // Kept rather than deleted because it is correct for the session-less
    // callers it does reach, and because deleting it would erase the record
    // of what was investigated. If SSE framing ever proves to be the cause of
    // a client hang, the lever is upstream (or a stateless mount), not here.
    config.json_response = true;
    // `allowed_hosts` defaults to loopback-only (rmcp's DNS-rebinding
    // protection). A deployment with a public hostname names it via
    // TOTEM_MCP_ALLOWED_HOSTS (comma-separated host or host:port entries) —
    // executed first by the ADV-GATEWAY-011 connector probe, whose tunnel
    // Host the default rightly refused with `403 Host header is not allowed`.
    if let Ok(hosts) = std::env::var("TOTEM_MCP_ALLOWED_HOSTS") {
        config.allowed_hosts.extend(
            hosts
                .split(',')
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty()),
        );
    }

    let service = StreamableHttpService::new(
        move || Ok(TotemMcp::token_bound(state.clone())),
        LocalSessionManager::default().into(),
        config,
    );

    Router::new().nest_service("/mcp", service)
}
