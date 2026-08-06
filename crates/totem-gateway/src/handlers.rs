//! `/recall` and `/save`: the first HTTP surface over `totem-store`
//! (docs/solution-intent.md §3.2; ADV-GATEWAY-001). `/enroll` (ADV-CLI-001)
//! and `GET /landscape/:repo` (ADV-CONSOLE-001) join them: registering or
//! re-syncing a repo's ARRIVE landscape, and reading it back.
//!
//! `save`/`recall` build an [`ops`] input straight from the request's own
//! `totem-core` types, call the shared operation, and wrap the result — the
//! resolve-scope-chain/do-the-operation/append-one-access-log-entry sequence
//! itself lives in [`ops`], not here (ADV-GATEWAY-002's tidy step), so the
//! MCP surface gets the same behavior without duplicating it. `enroll` and
//! `landscape` have no scope chain to resolve (a landscape sync is not
//! scoped memory) and call `totem-store`'s [`totem_store::LandscapeRepository`]
//! directly — the same call `mcp.rs`'s `totem_landscape` tool makes, so the
//! REST and MCP surfaces cannot silently diverge on what a repo's landscape
//! contains.

use axum::Json;
use axum::extract::{Path, State};

use crate::dto::{
    EnrollRequest, EnrollResponse, LandscapeView, RecallRequest, RecallResponse, SaveRequest,
    SaveResponse,
};
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

pub(crate) async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>, GatewayError> {
    let summary = state
        .store
        .landscape()
        .sync(&request.snapshot, &request.source)
        .await
        .map_err(GatewayError::from)?;

    Ok(Json(EnrollResponse {
        systems: summary.systems,
        components: summary.components,
        advances: summary.advances,
    }))
}

pub(crate) async fn landscape(
    State(state): State<AppState>,
    Path(repo): Path<String>,
) -> Result<Json<LandscapeView>, GatewayError> {
    let view = state
        .store
        .landscape()
        .view(&repo)
        .await
        .map_err(GatewayError::from)?;

    Ok(Json(view))
}
