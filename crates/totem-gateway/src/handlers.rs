//! `/recall` and `/save`: the first HTTP surface over `totem-store`
//! (docs/solution-intent.md §3.2; ADV-GATEWAY-001).
//!
//! Both handlers build an [`ops`] input straight from the request's own
//! `totem-core` types, call the shared operation, and wrap the result — the
//! resolve-scope-chain/do-the-operation/append-one-access-log-entry sequence
//! itself lives in [`ops`], not here (ADV-GATEWAY-002's tidy step), so the
//! MCP surface gets the same behavior without duplicating it.

use axum::Json;
use axum::extract::State;

use crate::dto::{RecallRequest, RecallResponse, SaveRequest, SaveResponse};
use crate::error::GatewayError;
use crate::ops::{self, RecallInput, SaveInput};
use crate::state::AppState;

pub(crate) async fn save(
    State(state): State<AppState>,
    Json(request): Json<SaveRequest>,
) -> Result<Json<SaveResponse>, GatewayError> {
    let input = SaveInput {
        project: request.project,
        teams: request.teams,
        category: request.category,
        scope: request.scope,
        subject: request.subject,
        body: request.body,
        tags: request.tags,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let id = ops::save(&state, input, "/save").await?;

    Ok(Json(SaveResponse { id }))
}

pub(crate) async fn recall(
    State(state): State<AppState>,
    Json(request): Json<RecallRequest>,
) -> Result<Json<RecallResponse>, GatewayError> {
    let input = RecallInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        query: request.query,
        categories: request.categories,
        since: request.since,
        limit: request.limit,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let records = ops::recall(&state, input, "/recall").await?;

    Ok(Json(RecallResponse { records }))
}
