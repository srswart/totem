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
    AccessLogEntry, AccessOperation, ActorId, Author, Content, FeedbackSignal, Harness,
    MemoryCategory, MemoryId, MemoryRecord, Provenance, RepoId, Scope, ScopeChain, SessionId,
    SubjectKind, SubjectRef, TeamId,
};
use totem_store::{AdvanceView, RecallQuery};

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

/// Everything a feedback signal needs, independent of transport.
#[derive(Debug, Clone)]
pub struct FeedbackInput {
    /// The reader's own identity — the target memory must be visible to this
    /// actor's resolved chain.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The memory the signal is about.
    pub memory_id: MemoryId,
    /// The signal itself.
    pub signal: FeedbackSignal,
    /// Which harness the signal arrived through.
    pub harness: Harness,
    /// The harness session the signal belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Apply an explicit feedback signal and append one
/// [`AccessOperation::Feedback`] access-log entry — the input side of the
/// value loop ADV-CORE-002's automatic citation boost and usage
/// reinforcement feed alongside (ADV-GATEWAY-004 gap-fill).
pub async fn feedback(
    state: &AppState,
    input: FeedbackInput,
    endpoint: &str,
) -> Result<MemoryRecord, GatewayError> {
    let chain = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);
    let record = state
        .store
        .memories()
        .apply_feedback(&chain, input.memory_id, input.signal)
        .await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Feedback,
        endpoint,
        now,
    )
    .for_memory(record.id);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(record)
}

/// Everything a contest needs, independent of transport.
#[derive(Debug, Clone)]
pub struct ContestInput {
    /// The writer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The memory being contested.
    pub memory_id: MemoryId,
    /// Where the new Uncertainty record is written.
    pub scope: Scope,
    /// The conflicting claim.
    pub claim: String,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// Who is filing the contest.
    pub author: Author,
    /// Which harness the contest arrived through.
    pub harness: Harness,
    /// The harness session the contest belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// File an Uncertainty record against an existing memory, preserving both
/// claims instead of overwriting (ADV-GATEWAY-004 gap-fill): the contested
/// record is never revised, and the new claim lands as its own record,
/// linked back to it by `subject`.
///
/// Refused when the contested memory is not visible to the writer's own
/// chain — an Uncertainty record naming an id the writer cannot see would
/// leak that the id exists, the same information-leak concern
/// [`MemoryRepository::get`](totem_store::MemoryRepository::get) already
/// avoids for an ordinary read.
///
/// Delegates to [`save`] once the target is confirmed visible: an
/// Uncertainty record is, mechanically, just a save — the category and
/// subject are the only choices this function makes that a caller does not.
pub async fn contest(
    state: &AppState,
    input: ContestInput,
    endpoint: &str,
) -> Result<MemoryId, GatewayError> {
    let chain = ScopeChain::resolve(input.author.actor(), input.project.as_ref(), &input.teams);
    if state
        .store
        .memories()
        .get(&chain, input.memory_id)
        .await?
        .is_none()
    {
        return Err(GatewayError::from(totem_store::StoreError::NotFound(
            input.memory_id,
        )));
    }

    let subject = SubjectRef::new(SubjectKind::Memory, input.memory_id.to_string())
        .expect("a memory id's string form is always a valid subject id");

    let save_input = SaveInput {
        project: input.project,
        teams: input.teams,
        category: MemoryCategory::Uncertainty,
        scope: input.scope,
        subject: Some(subject),
        body: input.claim,
        tags: input.tags,
        author: input.author,
        harness: input.harness,
        session: input.session,
        turn: input.turn,
    };
    save(state, save_input, endpoint).await
}

/// Everything an advance log entry needs, independent of transport.
#[derive(Debug, Clone)]
pub struct AdvanceLogInput {
    /// The writer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The advance the entry concerns.
    pub advance_id: String,
    /// Where the log entry is written.
    pub scope: Scope,
    /// The entry itself.
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

/// Append a process-attuned log entry about an advance (ADV-GATEWAY-004
/// gap-fill). Writes to Totem's own mirror/memory only — `/arrive/` files in
/// the repo stay authoritative (`arrive-sync.yaml`'s invariant); this is a
/// session log, not a substitute for the advance's own `## Changes Made`.
///
/// Episodic, and not caller-chosen: an appended log entry is, by its own
/// nature, something nobody should later revise.
pub async fn advance_log(
    state: &AppState,
    input: AdvanceLogInput,
    endpoint: &str,
) -> Result<MemoryId, GatewayError> {
    let subject = SubjectRef::new(SubjectKind::Advance, input.advance_id)
        .map_err(|error| GatewayError::InvalidRequest(error.to_string()))?;

    let save_input = SaveInput {
        project: input.project,
        teams: input.teams,
        category: MemoryCategory::Episodic,
        scope: input.scope,
        subject: Some(subject),
        body: input.body,
        tags: input.tags,
        author: input.author,
        harness: input.harness,
        session: input.session,
        turn: input.turn,
    };
    save(state, save_input, endpoint).await
}

/// One advance's current status, read from the landscape mirror
/// (ADV-GATEWAY-004 gap-fill). Not scoped memory, so — like
/// [`totem_landscape`](crate::mcp::TotemMcp) — this reads the landscape
/// directly rather than resolving a [`ScopeChain`] or appending to the
/// access log, the same precedent `handlers::landscape` already sets.
pub async fn advance_status(
    state: &AppState,
    advance_id: &str,
) -> Result<Option<AdvanceView>, GatewayError> {
    Ok(state.store.landscape().advance(advance_id).await?)
}
