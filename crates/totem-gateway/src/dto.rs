//! Request and response shapes for the REST surface.
//!
//! Deliberately built on `totem-core`'s own types ([`Scope`], [`Author`],
//! [`MemoryCategory`], the validated id newtypes) rather than a parallel set
//! of gateway-only structs: every one of them already has the JSON shape this
//! API needs (`#[derive(Serialize, Deserialize)]`), so a caller-supplied
//! `"actor:ada "` or an unknown category is refused by the same validation
//! `totem-core` and `totem-store` already enforce, not a second copy of it
//! written here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use totem_core::{
    AccessLogEntry, ActorId, Author, CurationEvent, FeedbackSignal, Harness, MemoryCategory,
    MemoryId, MemoryRecord, PromotionEvent, RepoId, ReviewState, Scope, SessionId, SubjectRef,
    TeamId,
};
use totem_store::LandscapeSnapshot;
pub use totem_store::{AdvanceView, LandscapeView};

/// `POST /save` — a write with provenance auto-attached from the caller's own
/// identity (the objective's phrase; ADV-GATEWAY-003 will replace `author` /
/// `harness` / `session` with values derived from an authenticated token
/// instead of a caller-supplied claim).
#[derive(Debug, Clone, Deserialize)]
pub struct SaveRequest {
    /// The writer's own project membership, if any — part of the chain the
    /// target `scope` is checked against.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// The category the new record belongs to.
    pub category: MemoryCategory,
    /// Where the record is written. Refused if the writer's resolved chain
    /// (`author.actor()` + `project` + `teams`) does not contain it.
    pub scope: Scope,
    /// The entity or ARRIVE artifact the record concerns, if any.
    pub subject: Option<SubjectRef>,
    /// The memory's content.
    pub body: String,
    /// Free-form tags.
    #[serde(default)]
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

/// `POST /save` response: the identity of the record just written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResponse {
    /// The new record's id.
    pub id: MemoryId,
}

/// `POST /recall` — merged, scope-resolved context for a query.
#[derive(Debug, Clone, Deserialize)]
pub struct RecallRequest {
    /// The reader's own identity — the only `actor` scope the resolved chain
    /// can ever contain.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Free text to rank by vector proximity. `None` skips vector ranking
    /// and returns the chain's most recent records instead.
    pub query: Option<String>,
    /// Restrict to these categories. Empty means every category.
    #[serde(default)]
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

/// `POST /recall` response: the merged, scope-resolved view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResponse {
    /// The records the reader's chain permits, ranked or ordered per the
    /// request.
    pub records: Vec<MemoryRecord>,
}

/// `POST /feedback` — an explicit value signal about an existing memory
/// (ADV-GATEWAY-004 gap-fill): the input side of the value loop the
/// automatic citation boost and usage reinforcement (ADV-CORE-002) feed
/// alongside.
#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackRequest {
    /// The reader's own identity — the target memory must be visible to this
    /// actor's resolved chain.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// The memory the signal is about.
    pub memory_id: MemoryId,
    /// The signal itself: `used`, `wrong`, or `stale`.
    pub signal: FeedbackSignal,
    /// Which harness the signal arrived through.
    pub harness: Harness,
    /// The harness session the signal belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `POST /feedback` response: the record's economics after the signal
/// applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackResponse {
    /// The updated record.
    pub record: MemoryRecord,
}

/// `POST /contest` — file an Uncertainty record against an existing memory
/// (ADV-GATEWAY-004 gap-fill), preserving both claims instead of overwriting:
/// the contested memory is never revised, and the new claim lands as its own
/// record, linked back to it by `subject`.
#[derive(Debug, Clone, Deserialize)]
pub struct ContestRequest {
    /// The writer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// The memory being contested. Refused if the writer's chain cannot see
    /// it — an Uncertainty record naming an id the writer cannot see would
    /// leak that the id exists.
    pub memory_id: MemoryId,
    /// Where the new Uncertainty record is written. Refused if the writer's
    /// resolved chain does not contain it, same as `/save`.
    pub scope: Scope,
    /// The conflicting claim, preserved alongside the original rather than
    /// replacing it.
    pub claim: String,
    /// Free-form tags.
    #[serde(default)]
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

/// `POST /contest` response: the identity of the new Uncertainty record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContestResponse {
    /// The new record's id.
    pub id: MemoryId,
}

/// `POST /advance/log` — append a process-attuned log entry about an
/// advance (ADV-GATEWAY-004 gap-fill). Writes to Totem's own mirror/memory
/// only; `/arrive/` files in the repo stay authoritative
/// (`arrive-sync.yaml`'s invariant) — this is a session log, not a substitute
/// for the advance's own `## Changes Made`.
#[derive(Debug, Clone, Deserialize)]
pub struct AdvanceLogRequest {
    /// The writer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// The advance the entry concerns (`ADV-<COMPONENT>-<SEQ>`).
    pub advance_id: String,
    /// Where the log entry is written. Refused if the writer's resolved
    /// chain does not contain it, same as `/save`.
    pub scope: Scope,
    /// The entry itself.
    pub body: String,
    /// Free-form tags.
    #[serde(default)]
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

/// `POST /advance/log` response: the identity of the record just written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceLogResponse {
    /// The new record's id.
    pub id: MemoryId,
}

/// `GET /advance/:id/status` response: one advance's current status, read
/// from the landscape mirror (ADV-GATEWAY-004 gap-fill: `totem_advance_status`).
#[derive(Debug, Clone, Serialize)]
pub struct AdvanceStatusResponse {
    /// The advance, or `None` if this id has never been synced.
    pub advance: Option<AdvanceView>,
}

/// `POST /enroll` — register or re-sync one repo's ARRIVE landscape (`totem
/// enroll`, ADV-CLI-001). `totem-cli` parses `/arrive/` locally
/// (`totem_arrive_sync::read_repo_artifacts`) and posts the resulting
/// snapshot here rather than opening its own store connection, since the
/// gateway — not the CLI — owns the store. Built directly on
/// [`LandscapeSnapshot`] (flattened), the same "reuse the core type's own
/// `Deserialize` rather than a parallel copy" pattern this module's other
/// requests use.
#[derive(Debug, Clone, Deserialize)]
pub struct EnrollRequest {
    /// The parsed `/arrive/` tree to sync.
    #[serde(flatten)]
    pub snapshot: LandscapeSnapshot,
    /// Where the ingested artifacts came from, e.g. `"cli:enroll"` or
    /// `"hook:post-commit"` — recorded as this sync run's provenance
    /// (`arrive-sync.yaml`'s "every ingestion records sync provenance").
    pub source: String,
}

/// `POST /enroll` response: what this sync run wrote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollResponse {
    /// Systems written.
    pub systems: usize,
    /// Components written.
    pub components: usize,
    /// Advances written.
    pub advances: usize,
}

/// `POST /promotions` — ask for a record to move to a wider scope
/// (ADV-CONSOLE-002: the write path the approval queue answers).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposePromotionRequest {
    /// The proposer's own project membership, if any.
    pub project: Option<RepoId>,
    /// The proposer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// The record being proposed for a wider scope.
    pub memory_id: MemoryId,
    /// Where the proposal asks the record to move. Refused if the proposer's
    /// resolved chain does not contain it, same as `/save`.
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

/// `POST /promotions` response: whether the move happened at once or is
/// queued for a human.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ProposePromotionResponse {
    /// Policy allowed the move outright, and it already happened.
    Promoted {
        /// The recorded ask.
        proposal: PromotionEvent,
        /// The recorded decision that moved the record.
        decision: Box<PromotionEvent>,
    },
    /// A human must decide; the record has not moved.
    Pending {
        /// The recorded ask, now in the queue.
        proposal: PromotionEvent,
    },
}

/// `POST /promotions/pending` — the queue a human decides from
/// (ADV-CONSOLE-002).
#[derive(Debug, Clone, Deserialize)]
pub struct PromotionQueueRequest {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `POST /promotions/pending` response: open proposals aimed at a scope the
/// reader can reach, oldest first.
#[derive(Debug, Clone, Serialize)]
pub struct PromotionQueueResponse {
    /// The open proposals.
    pub pending: Vec<PromotionEvent>,
}

/// `POST /promotions/:id/record` — the record a queued proposal names, for
/// the reviewer who must decide on it (ADV-CONSOLE-002).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposedRecordRequest {
    /// The reviewer's own identity.
    pub actor: ActorId,
    /// The reviewer's project membership, if any.
    pub project: Option<RepoId>,
    /// The reviewer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `POST /promotions/:id/record` response: `None` once the proposal is
/// decided or if it never named a record this reviewer may see.
#[derive(Debug, Clone, Serialize)]
pub struct ProposedRecordResponse {
    /// The proposed record, while the proposal is still open.
    pub record: Option<MemoryRecord>,
}

/// `POST /promotions/:id/approve` and `POST /promotions/:id/reject`
/// (ADV-CONSOLE-002): a human's decision on a queued proposal.
#[derive(Debug, Clone, Deserialize)]
pub struct PromotionDecisionRequest {
    /// The approver's own project membership, if any.
    pub project: Option<RepoId>,
    /// The approver's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Who is deciding.
    pub author: Author,
    /// Which harness the decision arrived through.
    pub harness: Harness,
    /// The harness session the decision belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
    /// Why, in the words of whoever decided — only meaningful on a rejection.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /promotions/:id/approve` / `POST /promotions/:id/reject` response.
#[derive(Debug, Clone, Serialize)]
pub struct PromotionDecisionResponse {
    /// The recorded decision.
    pub decision: PromotionEvent,
}

/// `POST /uncertainty/pending` — Uncertainty records awaiting a human
/// decision (ADV-CONSOLE-002).
#[derive(Debug, Clone, Deserialize)]
pub struct UncertaintyQueueRequest {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `POST /uncertainty/pending` response: open Uncertainty reviews the
/// reader's chain can see, oldest first.
#[derive(Debug, Clone, Serialize)]
pub struct UncertaintyQueueResponse {
    /// The open reviews.
    pub pending: Vec<MemoryRecord>,
}

/// `POST /uncertainty/:id/resolve` — a human's decision on a contested
/// memory (ADV-CONSOLE-002).
#[derive(Debug, Clone, Deserialize)]
pub struct ResolveUncertaintyRequest {
    /// The resolver's own identity.
    pub actor: ActorId,
    /// The resolver's project membership, if any.
    pub project: Option<RepoId>,
    /// The resolver's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Approve or reject the contested record. Refused unless the record's
    /// review is currently pending.
    pub decision: ReviewState,
    /// Which harness the resolution arrived through.
    pub harness: Harness,
    /// The harness session the resolution belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `POST /uncertainty/:id/resolve` response: the record after resolution.
#[derive(Debug, Clone, Serialize)]
pub struct ResolveUncertaintyResponse {
    /// The updated record.
    pub record: MemoryRecord,
}

/// `POST /audit/:id` — one memory's full audit trail (ADV-CONSOLE-002): its
/// provenance, access history, curator lineage, and promotion history.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditRequest {
    /// The reader's own identity.
    pub actor: ActorId,
    /// The reader's project membership, if any.
    pub project: Option<RepoId>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<TeamId>,
    /// Which harness the read arrived through.
    pub harness: Harness,
    /// The harness session the read belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// `GET /landscape/:repo/events` (ADV-CONSOLE-003) — a query string, not a
/// JSON body: the browser's native `EventSource` can only issue a bare `GET`,
/// with no custom body or header. `harness` is not a caller-supplied field
/// here the way it is on every POST endpoint above: this route only ever
/// serves the console's own live subscription, so the handler fixes it to
/// [`totem_core::Harness::Console`] rather than trusting a query parameter
/// for it — a `Harness` value does not round-trip through a flat query string
/// anyway (its `Other(String)` variant is not representable as one key).
#[derive(Debug, Clone, Deserialize)]
pub struct LandscapeEventsQuery {
    /// The subscriber's own identity — the actor every relayed read is
    /// logged under.
    pub actor: ActorId,
    /// The browser session subscribing, so its relayed reads share one
    /// access-log session the same way any other console call's `session`
    /// field does.
    pub session: SessionId,
}

/// `POST /audit/:id` response.
#[derive(Debug, Clone, Serialize)]
pub struct AuditTrailResponse {
    /// The record itself.
    pub record: MemoryRecord,
    /// Every logged read or write naming this record.
    pub access_log: Vec<AccessLogEntry>,
    /// The merges (and rollbacks) this record took part in.
    pub curation_history: Vec<CurationEvent>,
    /// This record's whole scope history.
    pub promotion_history: Vec<PromotionEvent>,
}
