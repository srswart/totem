//! `totem-gateway`: the first HTTP surface over `totem-store` — recall and
//! save, with provenance auto-attached and every request appended to the
//! access log (docs/solution-intent.md §3.2, §4).
//!
//! Thin over the core API, deliberately: every rule enforced here already
//! lives in `totem-core` (validated identifiers, [`totem_core::Scope`]
//! parsing) or `totem-store` (scope isolation, append-only categories,
//! embedding dimensions) — this crate wires HTTP onto that, and duplicates
//! none of it. That is the mitigation the project brief names for "standard
//! drift": a router with almost no logic of its own has almost nothing to
//! drift.

#![warn(missing_docs)]

mod dto;
mod error;
mod handlers;
mod state;

use axum::Router;
use axum::routing::post;

pub use dto::{RecallRequest, RecallResponse, SaveRequest, SaveResponse};
pub use error::GatewayError;
pub use state::AppState;

/// Build the router. `POST /recall` and `POST /save` are the only routes
/// this advance adds; the MCP surface (`totem_recall`, `totem_save`,
/// `totem_landscape`) calls the same handlers' underlying operations from
/// ADV-GATEWAY-002 rather than duplicating them.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/recall", post(handlers::recall))
        .route("/save", post(handlers::save))
        .with_state(state)
}
