//! Save and recall, independent of transport.
//!
//! Both `handlers.rs` (REST) and `mcp.rs` (ADV-GATEWAY-002) call exactly these
//! two functions rather than each re-implementing "resolve the scope chain,
//! do the store operation, append one access-log entry" — so the access log's
//! "every read and write is logged" claim, and the embedding-on-write
//! placement decision, hold identically no matter which surface a caller
//! used ([`crate::router`]'s doc comment).

use chrono::{DateTime, Utc};
use totem_core::{
    AccessLogEntry, AccessOperation, ActorId, Author, Content, Harness, MemoryCategory, MemoryId,
    MemoryRecord, Provenance, RepoId, Scope, ScopeChain, SessionId, SubjectRef, TeamId,
};
use totem_store::RecallQuery;

use crate::error::GatewayError;
use crate::state::AppState;

/// Everything a save needs, independent of transport.
#[derive(Debug, Clone)]
pub struct SaveInput {
    /// The writer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The category the new record belongs to.
    pub category: MemoryCategory,
    /// Where the record is written.
    pub scope: Scope,
    /// The entity or ARRIVE artifact the record concerns, if any.
    pub subject: Option<SubjectRef>,
    /// The memory's content.
    pub body: String,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// Who is writing.
    pub author: Author,
    /// Which harness the write arrived through.
    pub harness: Harness,
    /// The harness session the write belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Everything a recall needs, independent of transport.
#[derive(Debug, Clone)]
pub struct RecallInput {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// Free text to rank by vector proximity. `None` skips vector ranking.
    pub query: Option<String>,
    /// Restrict to these categories. Empty means every category.
    pub categories: Vec<MemoryCategory>,
    /// Only records written strictly after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Cap the merged result set.
    pub limit: Option<usize>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Write a memory and append one [`AccessOperation::Save`] access-log entry.
///
/// `endpoint` names the surface that handled the call (`/save` for REST,
/// `mcp:totem_save` for MCP) so an audit query can tell them apart.
pub async fn save(
    state: &AppState,
    input: SaveInput,
    endpoint: &str,
) -> Result<MemoryId, GatewayError> {
    let writer = ScopeChain::resolve(input.author.actor(), input.project.as_ref(), &input.teams);

    let content = Content::new(input.body).with_tags(input.tags);
    let content = totem_store::embed(state.embedder.as_ref(), content)?;

    let now = Utc::now();
    let mut provenance = Provenance::new(
        input.author.clone(),
        input.harness.clone(),
        input.session.clone(),
        now,
    );
    if let Some(turn) = input.turn {
        provenance = provenance.at_turn(turn);
    }

    let mut record = MemoryRecord::new(input.category, input.scope, content, provenance);
    record.subject = input.subject;

    state.store.memories().save(&writer, &record).await?;

    let mut entry = AccessLogEntry::new(
        input.author.actor().clone(),
        input.harness,
        input.session,
        AccessOperation::Save,
        endpoint,
        now,
    )
    .for_memory(record.id);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(record.id)
}

/// Read the merged, scope-resolved view and append one
/// [`AccessOperation::Recall`] access-log entry.
///
/// `endpoint` names the surface that handled the call (`/recall` for REST,
/// `mcp:totem_recall` for MCP) so an audit query can tell them apart.
pub async fn recall(
    state: &AppState,
    input: RecallInput,
    endpoint: &str,
) -> Result<Vec<MemoryRecord>, GatewayError> {
    let reader = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let mut query = RecallQuery::new();
    if let Some(text) = &input.query {
        let probe = state.embedder.embed(text)?;
        query = query.near(probe)?;
    }
    if !input.categories.is_empty() {
        query = query.in_categories(input.categories.clone());
    }
    if let Some(since) = input.since {
        query = query.since(since);
    }
    if let Some(limit) = input.limit {
        query = query.limit(limit);
    }

    let records = state.store.memories().recall(&reader, &query).await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Recall,
        endpoint,
        now,
    )
    .with_result_count(records.len() as u64);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(records)
}
