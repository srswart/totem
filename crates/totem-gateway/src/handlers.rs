//! `/recall` and `/save`: the first HTTP surface over `totem-store`
//! (docs/solution-intent.md §3.2; ADV-GATEWAY-001).
//!
//! Both handlers follow the same shape: resolve the caller's [`ScopeChain`],
//! do the store operation, then append one [`AccessLogEntry`] — so the access
//! log's "verifiable by an audit query" claim holds for every request that
//! reaches a handler, not just the ones a caller remembers to check.

use axum::Json;
use axum::extract::State;

use crate::dto::{RecallRequest, RecallResponse, SaveRequest, SaveResponse};
use crate::error::GatewayError;
use crate::state::AppState;

pub(crate) async fn save(
    State(state): State<AppState>,
    Json(request): Json<SaveRequest>,
) -> Result<Json<SaveResponse>, GatewayError> {
    let _ = (state, request);
    unimplemented!("ADV-GATEWAY-001")
}

pub(crate) async fn recall(
    State(state): State<AppState>,
    Json(request): Json<RecallRequest>,
) -> Result<Json<RecallResponse>, GatewayError> {
    let _ = (state, request);
    unimplemented!("ADV-GATEWAY-001")
}
