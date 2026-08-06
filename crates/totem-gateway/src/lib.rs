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
use axum::routing::{get, post};

pub use dto::{
    AdvanceLogRequest, AdvanceLogResponse, AdvanceStatusResponse, AdvanceView, ContestRequest,
    ContestResponse, EnrollRequest, EnrollResponse, FeedbackRequest, FeedbackResponse,
    LandscapeView, RecallRequest, RecallResponse, SaveRequest, SaveResponse,
};
pub use error::GatewayError;
pub use mcp::TotemMcp;
pub use state::AppState;

/// Build the REST router. `POST /recall`, `POST /save`, `POST /enroll`,
/// `GET /landscape/:repo`, `POST /feedback`, `POST /contest`,
/// `POST /advance/log`, and `GET /advance/:id/status` are the only routes
/// this crate adds; the MCP surface ([`TotemMcp`]: `totem_recall`,
/// `totem_save`, `totem_landscape`, `totem_feedback`, `totem_contest`,
/// `totem_advance_status`, `totem_advance_log`) calls the same [`ops`]
/// functions (or, for the landscape and advance-status reads, the same
/// `totem-store` calls) rather than duplicating them.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/recall", post(handlers::recall))
        .route("/save", post(handlers::save))
        .route("/feedback", post(handlers::feedback))
        .route("/contest", post(handlers::contest))
        .route("/advance/log", post(handlers::advance_log))
        .route("/advance/{id}/status", get(handlers::advance_status))
        .route("/enroll", post(handlers::enroll))
        .route("/landscape/{repo}", get(handlers::landscape))
        .with_state(state)
}
