//! Save and recall, independent of transport.
//!
//! Both `handlers.rs` (REST) and `mcp.rs` (ADV-GATEWAY-002) call exactly these
//! two functions rather than each re-implementing "resolve the scope chain,
//! do the store operation, append one access-log entry" — so the access log's
//! "every read and write is logged" claim, and the embedding-on-write
//! placement decision, hold identically no matter which surface a caller
//! used ([`crate::router`]'s doc comment).
//!
//! Every operation takes the [`Caller`] making it, and authorizes the identity
//! that call asserts before touching the store (ADV-GATEWAY-003). That is why
//! it is a parameter rather than something a surface looks up for itself: a
//! new handler or a new MCP tool cannot reach these functions without saying
//! who is calling, so "this surface forgot to check the credential" is not a
//! mistake that compiles.

use chrono::{DateTime, Utc};
use totem_core::{
    AccessLogEntry, AccessOperation, ActorId, Author, Content, CurationEvent, FeedbackSignal,
    Harness, MemoryCategory, MemoryId, MemoryRecord, PromotionEvent, PromotionId, Provenance,
    RepoId, ReviewState, Scope, ScopeChain, SessionId, SubjectKind, SubjectRef, TeamId,
};
use totem_store::{AdvanceView, PromotionOutcome, RecallQuery};

use crate::auth::{Caller, log_refusal};
use crate::error::GatewayError;
use crate::state::AppState;

/// Refuse an identity this caller may not assert, appending one refusal entry
/// to the access log when it does (ADV-CORE-006) — the single choke point
/// every `ops` function's own `caller.authorize_identity` check now goes
/// through, so a refusal is logged identically no matter which operation
/// produced it.
async fn authorize_identity(
    state: &AppState,
    caller: &Caller,
    actor: &ActorId,
    project: Option<&RepoId>,
    teams: &[TeamId],
    endpoint: &str,
) -> Result<(), GatewayError> {
    if let Err(error) = caller.authorize_identity(actor, project, teams) {
        return Err(log_refusal(state, caller, error, endpoint).await);
    }
    Ok(())
}

/// Refuse a write into a scope this caller was not granted, logging the
/// refusal the same way [`authorize_identity`] does.
async fn authorize_scope(
    state: &AppState,
    caller: &Caller,
    scope: &Scope,
    endpoint: &str,
) -> Result<(), GatewayError> {
    if let Err(error) = caller.authorize_scope(scope) {
        return Err(log_refusal(state, caller, error, endpoint).await);
    }
    Ok(())
}

/// Refuse a landscape enroll/read naming a different repo than this caller's
/// credential, logging the refusal the same way [`authorize_identity`] does
/// — [`crate::handlers::enroll`] and [`crate::handlers::landscape`]'s own
/// `caller.authorize_repo` checks, which have no [`ScopeChain`] to resolve
/// and so live outside the rest of this module's operations.
pub(crate) async fn authorize_repo(
    state: &AppState,
    caller: &Caller,
    requested: &RepoId,
    endpoint: &str,
) -> Result<(), GatewayError> {
    if let Err(error) = caller.authorize_repo(requested) {
        return Err(log_refusal(state, caller, error, endpoint).await);
    }
    Ok(())
}

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
    caller: &Caller,
    endpoint: &str,
) -> Result<MemoryId, GatewayError> {
    authorize_identity(
        state,
        caller,
        input.author.actor(),
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    authorize_scope(state, caller, &input.scope, endpoint).await?;

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
    caller: &Caller,
    endpoint: &str,
) -> Result<Vec<MemoryRecord>, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;

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
    caller: &Caller,
    endpoint: &str,
) -> Result<MemoryRecord, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;

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
    caller: &Caller,
    endpoint: &str,
) -> Result<MemoryId, GatewayError> {
    // Authorized before the visibility probe below, not only inside the `save`
    // it delegates to: that probe is itself a read, and answering it for an
    // identity the caller may not assert would leak whether an id exists.
    authorize_identity(
        state,
        caller,
        input.author.actor(),
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    authorize_scope(state, caller, &input.scope, endpoint).await?;

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
    save(state, save_input, caller, endpoint).await
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
    caller: &Caller,
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
    save(state, save_input, caller, endpoint).await
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

/// Everything a promotion proposal needs, independent of transport.
#[derive(Debug, Clone)]
pub struct ProposePromotionInput {
    /// The proposer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The proposer's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The record being proposed for a wider scope.
    pub memory_id: MemoryId,
    /// Where the proposal asks the record to move.
    pub to: Scope,
    /// Who is proposing.
    pub author: Author,
    /// Which harness the proposal arrived through.
    pub harness: Harness,
    /// The harness session the proposal belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Ask for a record to move to a wider scope and append one
/// [`AccessOperation::Propose`] access-log entry (ADV-CONSOLE-002).
pub async fn propose_promotion(
    state: &AppState,
    input: ProposePromotionInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<PromotionOutcome, GatewayError> {
    authorize_identity(
        state,
        caller,
        input.author.actor(),
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    authorize_scope(state, caller, &input.to, endpoint).await?;

    let proposer = ScopeChain::resolve(input.author.actor(), input.project.as_ref(), &input.teams);

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

    let outcome = state
        .store
        .promotions()
        .propose(&proposer, input.memory_id, input.to, provenance)
        .await?;

    let mut entry = AccessLogEntry::new(
        input.author.actor().clone(),
        input.harness,
        input.session,
        AccessOperation::Propose,
        endpoint,
        now,
    )
    .for_memory(input.memory_id);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(outcome)
}

/// Everything a promotion or Uncertainty queue read needs, independent of
/// transport — the reader's identity, and what a harness reports about the
/// read itself.
#[derive(Debug, Clone)]
pub struct QueueReadInput {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// The open promotion proposals this reader may act on, oldest first, and
/// append one [`AccessOperation::Recall`] access-log entry — a queue read is
/// a read of scoped memory-adjacent state, logged the same way `/recall` is.
pub async fn promotion_pending(
    state: &AppState,
    input: QueueReadInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<Vec<PromotionEvent>, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let reader = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let pending = state.store.promotions().pending(&reader).await?;
    log_queue_read(state, input, endpoint, pending.len() as u64).await?;
    Ok(pending)
}

/// Uncertainty records awaiting a human decision this reader may see, oldest
/// first, logged the same way [`promotion_pending`] is.
pub async fn pending_uncertainty(
    state: &AppState,
    input: QueueReadInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<Vec<MemoryRecord>, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let reader = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let pending = state
        .store
        .memories()
        .pending_review(&reader, MemoryCategory::Uncertainty)
        .await?;
    log_queue_read(state, input, endpoint, pending.len() as u64).await?;
    Ok(pending)
}

async fn log_queue_read(
    state: &AppState,
    input: QueueReadInput,
    endpoint: &str,
    result_count: u64,
) -> Result<(), GatewayError> {
    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Recall,
        endpoint,
        now,
    )
    .with_result_count(result_count);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;
    Ok(())
}

/// Everything reading one proposed record needs, independent of transport.
#[derive(Debug, Clone)]
pub struct ProposedRecordInput {
    /// The reviewer's own identity.
    pub actor: ActorId,
    /// The reviewer's project membership, if any.
    pub project: Option<RepoId>,
    /// The reviewer's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The proposal to read the named record of.
    pub proposal: PromotionId,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// The record a queued proposal names, for the reviewer deciding on it
/// (ADV-CONSOLE-002) — `None` once the proposal is decided, or if this
/// reviewer's chain cannot reach it.
pub async fn proposed_record(
    state: &AppState,
    input: ProposedRecordInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<Option<MemoryRecord>, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let reviewer = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let record = state
        .store
        .promotions()
        .proposed_record(&reviewer, input.proposal)
        .await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Recall,
        endpoint,
        now,
    )
    .with_result_count(u64::from(record.is_some()));
    if let Some(id) = record.as_ref().map(|record| record.id) {
        entry = entry.for_memory(id);
    }
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(record)
}

/// Everything a promotion decision needs, independent of transport.
#[derive(Debug, Clone)]
pub struct PromotionDecisionInput {
    /// The approver's own project membership, if any.
    pub project: Option<RepoId>,
    /// The approver's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The proposal being decided.
    pub proposal: PromotionId,
    /// Who is deciding.
    pub author: Author,
    /// Which harness the decision arrived through.
    pub harness: Harness,
    /// The harness session the decision belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
    /// Why, in the words of whoever decided — only meaningful on a rejection.
    pub reason: Option<String>,
}

/// Approve a queued proposal, moving the record, and append one
/// [`AccessOperation::PromotionDecision`] access-log entry (ADV-CONSOLE-002).
pub async fn approve_promotion(
    state: &AppState,
    input: PromotionDecisionInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<PromotionEvent, GatewayError> {
    decide_promotion(state, input, caller, endpoint, true).await
}

/// Refuse a queued proposal — the record does not move — and append one
/// [`AccessOperation::PromotionDecision`] access-log entry (ADV-CONSOLE-002).
pub async fn reject_promotion(
    state: &AppState,
    input: PromotionDecisionInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<PromotionEvent, GatewayError> {
    decide_promotion(state, input, caller, endpoint, false).await
}

async fn decide_promotion(
    state: &AppState,
    input: PromotionDecisionInput,
    caller: &Caller,
    endpoint: &str,
    approve: bool,
) -> Result<PromotionEvent, GatewayError> {
    authorize_identity(
        state,
        caller,
        input.author.actor(),
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let approver = ScopeChain::resolve(input.author.actor(), input.project.as_ref(), &input.teams);

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

    let decision = if approve {
        state
            .store
            .promotions()
            .approve(&approver, input.proposal, provenance)
            .await?
    } else {
        state
            .store
            .promotions()
            .reject(&approver, input.proposal, provenance, input.reason)
            .await?
    };

    let mut entry = AccessLogEntry::new(
        input.author.actor().clone(),
        input.harness,
        input.session,
        AccessOperation::PromotionDecision,
        endpoint,
        now,
    )
    .for_memory(decision.memory);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(decision)
}

/// Everything an Uncertainty resolution needs, independent of transport.
#[derive(Debug, Clone)]
pub struct ResolveUncertaintyInput {
    /// The resolver's own identity.
    pub actor: ActorId,
    /// The resolver's project membership, if any.
    pub project: Option<RepoId>,
    /// The resolver's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The contested record being resolved.
    pub memory_id: MemoryId,
    /// Approve or reject it.
    pub decision: ReviewState,
    /// Which harness the resolution arrived through.
    pub harness: Harness,
    /// The harness session the resolution belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Record a human's decision on a contested memory and append one
/// [`AccessOperation::Resolve`] access-log entry (ADV-CONSOLE-002: the
/// Uncertainty queue's resolution step).
pub async fn resolve_uncertainty(
    state: &AppState,
    input: ResolveUncertaintyInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<MemoryRecord, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let resolver = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let record = state
        .store
        .memories()
        .resolve_review(&resolver, input.memory_id, input.decision)
        .await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Resolve,
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

/// One memory's full audit trail (ADV-CONSOLE-002): its provenance, access
/// history, curator lineage, and promotion history.
#[derive(Debug, Clone)]
pub struct AuditTrail {
    /// The record itself.
    pub record: MemoryRecord,
    /// Every logged read or write naming this record.
    pub access_log: Vec<AccessLogEntry>,
    /// The merges (and rollbacks) this record took part in.
    pub curation_history: Vec<CurationEvent>,
    /// This record's whole scope history.
    pub promotion_history: Vec<PromotionEvent>,
}

/// Everything an audit trail read needs, independent of transport.
#[derive(Debug, Clone)]
pub struct AuditInput {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    pub teams: Vec<TeamId>,
    /// The record to reconstruct the audit trail of.
    pub memory_id: MemoryId,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Reconstruct one memory's audit trail and append one
/// [`AccessOperation::Recall`] access-log entry.
///
/// Refused when the reader cannot see the record itself
/// ([`totem_store::StoreError::NotFound`]), the same visibility-before-disclosure
/// rule every other read here applies. Every sub-query
/// ([`totem_store::AccessLogRepository::for_memory`],
/// [`totem_store::CurationRepository::history`],
/// [`totem_store::PromotionRepository::history`]) re-checks that visibility
/// itself rather than trusting this function's own check.
pub async fn audit_trail(
    state: &AppState,
    input: AuditInput,
    caller: &Caller,
    endpoint: &str,
) -> Result<AuditTrail, GatewayError> {
    authorize_identity(
        state,
        caller,
        &input.actor,
        input.project.as_ref(),
        &input.teams,
        endpoint,
    )
    .await?;
    let reader = ScopeChain::resolve(&input.actor, input.project.as_ref(), &input.teams);

    let Some(record) = state.store.memories().get(&reader, input.memory_id).await? else {
        return Err(GatewayError::from(totem_store::StoreError::NotFound(
            input.memory_id,
        )));
    };
    let access_log = state
        .store
        .access_log()
        .for_memory(&reader, input.memory_id)
        .await?;
    let curation_history = state
        .store
        .curation()
        .history(&reader, input.memory_id)
        .await?;
    let promotion_history = state
        .store
        .promotions()
        .history(&reader, input.memory_id)
        .await?;

    let now = Utc::now();
    let mut entry = AccessLogEntry::new(
        input.actor,
        input.harness,
        input.session,
        AccessOperation::Recall,
        endpoint,
        now,
    )
    .for_memory(input.memory_id);
    if let Some(turn) = input.turn {
        entry = entry.at_turn(turn);
    }
    state.store.access_log().record(&entry).await?;

    Ok(AuditTrail {
        record,
        access_log,
        curation_history,
        promotion_history,
    })
}
