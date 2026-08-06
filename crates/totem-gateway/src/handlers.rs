//! `/recall` and `/save`: the first HTTP surface over `totem-store`
//! (docs/solution-intent.md §3.2; ADV-GATEWAY-001).
//!
//! Both handlers follow the same shape: resolve the caller's [`ScopeChain`],
//! do the store operation, then append one [`AccessLogEntry`] — so the access
//! log's "verifiable by an audit query" claim holds for every request that
//! reaches a handler, not just the ones a caller remembers to check.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use totem_core::{AccessLogEntry, AccessOperation, Content, MemoryRecord, Provenance, ScopeChain};
use totem_store::RecallQuery;

use crate::dto::{RecallRequest, RecallResponse, SaveRequest, SaveResponse};
use crate::error::GatewayError;
use crate::state::AppState;

pub(crate) async fn save(
    State(state): State<AppState>,
    Json(request): Json<SaveRequest>,
) -> Result<Json<SaveResponse>, GatewayError> {
    let writer = ScopeChain::resolve(
        request.author.actor(),
        request.project.as_ref(),
        &request.teams,
    );

    let content = Content::new(request.body).with_tags(request.tags);
    let content = totem_store::embed(state.embedder.as_ref(), content)?;

    let now = Utc::now();
    let mut provenance = Provenance::new(
        request.author.clone(),
        request.harness.clone(),
        request.session.clone(),
        now,
    );
    if let Some(turn) = request.turn {
        provenance = provenance.at_turn(turn);
    }

    let mut record = MemoryRecord::new(request.category, request.scope, content, provenance);
    record.subject = request.subject;

    state.store.memories().save(&writer, &record).await?;

    let mut entry = AccessLogEntry::new(
        request.author.actor().clone(),
        request.harness,
        request.session,
        AccessOperation::Save,
        "/save",
        now,
    )
    .for_memory(record.id);
    if let Some(turn) = request.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(Json(SaveResponse { id: record.id }))
}

pub(crate) async fn recall(
    State(state): State<AppState>,
    Json(request): Json<RecallRequest>,
) -> Result<Json<RecallResponse>, GatewayError> {
    let reader = ScopeChain::resolve(&request.actor, request.project.as_ref(), &request.teams);

    let mut query = RecallQuery::new();
    if let Some(text) = &request.query {
        let probe = state.embedder.embed(text)?;
        query = query.near(probe)?;
    }
    if !request.categories.is_empty() {
        query = query.in_categories(request.categories.clone());
    }
    if let Some(since) = request.since {
        query = query.since(since);
    }
    if let Some(limit) = request.limit {
        query = query.limit(limit);
    }

    let records = state.store.memories().recall(&reader, &query).await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        request.actor,
        request.harness,
        request.session,
        AccessOperation::Recall,
        "/recall",
        now,
    )
    .with_result_count(records.len() as u64);
    if let Some(turn) = request.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(Json(RecallResponse { records }))
}
