//! `totem-gateway`: the first surfaces over `totem-store` — recall and save,
//! with provenance auto-attached and every request appended to the access
//! log (docs/solution-intent.md §3.2, §4), over REST (ADV-GATEWAY-001), MCP
//! stdio (ADV-GATEWAY-002), and MCP over streamable HTTP with bearer
//! credentials for cloud agents (ADV-GATEWAY-003).
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
//!
//! # Two compositions, and which one is remotely reachable
//!
//! [`router`] is the **local** composition: every caller is
//! [`Caller::Trusted`], identity is taken at its word, and it is what the
//! in-process tests and a single-user desktop deployment use — the same trust
//! model the stdio MCP binary has always had, where the process boundary is
//! the credential.
//!
//! [`authenticated_app`] is the **remote** composition: the same REST routes
//! plus the streamable-HTTP MCP surface, with every route behind
//! [`auth::authenticate`]. It is the only composition that should ever be
//! bound to a non-loopback listener. There is no configuration that turns
//! [`router`] into it, and no route in it that skips the credential layer.

#![warn(missing_docs)]

mod auth;
mod dto;
mod error;
pub mod eval;
mod handlers;
mod mcp;
mod mcp_http;
mod ops;
mod state;

use axum::Router;
use axum::routing::{get, post};

pub use auth::{AuthError, Caller, TokenGrant, TokenRegistry};
pub use dto::{
    AdvanceLogRequest, AdvanceLogResponse, AdvanceStatusResponse, AdvanceView, AuditRequest,
    AuditTrailResponse, ContestRequest, ContestResponse, EnrollRequest, EnrollResponse,
    FeedbackRequest, FeedbackResponse, LandscapeView, PromotionDecisionRequest,
    PromotionDecisionResponse, PromotionQueueRequest, PromotionQueueResponse,
    ProposePromotionRequest, ProposePromotionResponse, ProposedRecordRequest,
    ProposedRecordResponse, RecallRequest, RecallResponse, ResolveUncertaintyRequest,
    ResolveUncertaintyResponse, SaveRequest, SaveResponse, UncertaintyQueueRequest,
    UncertaintyQueueResponse,
};
pub use error::GatewayError;
pub use mcp::TotemMcp;
pub use state::AppState;

/// The REST routes themselves, with no caller attached.
///
/// Private on purpose: a router in this state has no [`Caller`] extension, so
/// its handlers cannot run. Each public composition below supplies exactly one
/// — which is what makes "forgot to add the credential layer" a startup-time
/// failure rather than an open door.
fn routes(state: AppState) -> Router {
    Router::new()
        .route("/recall", post(handlers::recall))
        .route("/save", post(handlers::save))
        .route("/feedback", post(handlers::feedback))
        .route("/contest", post(handlers::contest))
        .route("/advance/log", post(handlers::advance_log))
        .route("/advance/{id}/status", get(handlers::advance_status))
        .route("/enroll", post(handlers::enroll))
        .route("/landscape/{repo}", get(handlers::landscape))
        .route("/promotions", post(handlers::propose_promotion))
        .route("/promotions/pending", post(handlers::promotion_pending))
        .route("/promotions/{id}/record", post(handlers::proposed_record))
        .route(
            "/promotions/{id}/approve",
            post(handlers::approve_promotion),
        )
        .route("/promotions/{id}/reject", post(handlers::reject_promotion))
        .route("/uncertainty/pending", post(handlers::pending_uncertainty))
        .route(
            "/uncertainty/{id}/resolve",
            post(handlers::resolve_uncertainty),
        )
        .route("/audit/{id}", post(handlers::audit_trail))
        .with_state(state)
}

/// Build the **local** REST router: `POST /recall`, `POST /save`,
/// `POST /enroll`, `GET /landscape/:repo`, `POST /feedback`, `POST /contest`,
/// `POST /advance/log`, `GET /advance/:id/status`, the promotion-approval
/// surface (`POST /promotions`, `POST /promotions/pending`,
/// `POST /promotions/:id/record`, `POST /promotions/:id/approve`,
/// `POST /promotions/:id/reject`), the Uncertainty queue
/// (`POST /uncertainty/pending`, `POST /uncertainty/:id/resolve`), and the
/// audit trail (`POST /audit/:id`) — ADV-CONSOLE-002.
///
/// Every caller is [`Caller::Trusted`] — this composition authenticates
/// nobody. Use it in-process, or on a loopback listener for a single-user
/// desktop deployment; use [`authenticated_app`] for anything a cloud agent
/// can reach.
pub fn router(state: AppState) -> Router {
    routes(state).layer(axum::Extension(Caller::Trusted))
}

/// Build the **remote** application: the REST routes plus the streamable-HTTP
/// MCP surface at `/mcp`, every one of them behind bearer-credential
/// verification against [`AppState::tokens`].
///
/// A request with no credential, an unknown one, or an expired one is refused
/// with `401` and a `WWW-Authenticate: Bearer` header before it reaches a
/// handler. A request with a valid credential carries a [`Caller::Bound`]
/// grant, and every operation checks the identity it asserts against that
/// grant (`gateway.yaml`'s least-privilege invariant).
///
/// An [`AppState`] with an empty [`TokenRegistry`] serves nothing: that is the
/// fail-closed default, not a misconfiguration to work around.
pub fn authenticated_app(state: AppState) -> Router {
    routes(state.clone())
        .merge(mcp_http::routes(state.clone()))
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::authenticate,
        ))
}
