//! `totem-gateway`: the first surfaces over `totem-store` — recall and save,
//! with provenance auto-attached and every request appended to the access
//! log (docs/solution-intent.md §3.2, §4), over both REST (ADV-GATEWAY-001)
//! and MCP stdio (ADV-GATEWAY-002).
//!
//! Thin over the core API, deliberately: every rule enforced here already
//! lives in `totem-core` (validated identifiers, [`totem_core::Scope`]
//! parsing) or `totem-store` (scope isolation, append-only categories,
//! embedding dimensions) — this crate wires HTTP/MCP onto that, and
//! duplicates none of it. That is the mitigation the project brief names for
//! "standard drift": a router with almost no logic of its own has almost
//! nothing to drift.
//!
//! [`router`] (REST) and [`TotemMcp`] (MCP) both call [`ops::recall`] /
//! [`ops::save`] — the same operations, not two implementations of them — so
//! provenance and access logging behave identically no matter which surface a
//! caller used.

#![warn(missing_docs)]

mod dto;
mod error;
mod handlers;
mod mcp;
mod ops;
mod state;

use axum::Router;
use axum::routing::post;

pub use dto::{RecallRequest, RecallResponse, SaveRequest, SaveResponse};
pub use error::GatewayError;
pub use mcp::TotemMcp;
pub use state::AppState;

/// Build the REST router. `POST /recall` and `POST /save` are the only
/// routes this crate adds; the MCP surface ([`TotemMcp`]: `totem_recall`,
/// `totem_save`, `totem_landscape`) calls the same [`ops`] functions rather
/// than duplicating them.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/recall", post(handlers::recall))
        .route("/save", post(handlers::save))
        .with_state(state)
}
