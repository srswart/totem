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
    ActorId, Author, Harness, MemoryCategory, MemoryId, MemoryRecord, RepoId, Scope, SessionId,
    SubjectRef, TeamId,
};

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
