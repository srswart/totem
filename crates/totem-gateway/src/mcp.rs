//! The MCP tool surface: `totem_recall`, `totem_save`, `totem_landscape`
//! (docs/solution-intent.md §3.1; ADV-GATEWAY-002). Served over stdio for
//! desktop harnesses (Claude Code, Cursor) — streamable HTTP + auth for cloud
//! agents is ADV-GATEWAY-003's job, per
//! [docs/tech-direction/mcp.md](../../../docs/tech-direction/mcp.md)'s
//! recommendation to design against streamable HTTP separately once that
//! advance's token design exists.
//!
//! Every tool call resolves into [`ops::recall`]/[`ops::save`] — the same
//! functions the REST surface (`handlers.rs`) uses — so provenance and access
//! logging behave identically no matter which transport a caller used.
//!
//! Tool parameters are plain JSON primitives rather than `totem-core`'s own
//! types directly: an MCP tool's input schema comes from
//! [`schemars::JsonSchema`], which `totem-core`'s hand-validated newtypes
//! (`ActorId`, `Scope`, `Harness`, ...) do not implement — adding it there
//! would be a `core`-component change, out of this advance's declared scope
//! (`gateway` only). Every field is still parsed through `totem-core`'s own
//! fallible constructors (`ActorId::new`, `Scope::from_str`, `Harness`'s and
//! `Author`'s own `Deserialize`) before it reaches [`ops`] — the validation
//! itself is not duplicated, only the envelope shape.

use chrono::{DateTime, Utc};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ErrorData, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use totem_core::{
    ActorId, Author, FeedbackSignal, Harness, MemoryCategory, MemoryId, RepoId, Scope, SessionId,
    SubjectRef, TeamId,
};

use crate::error::GatewayError;
use crate::ops::{self, AdvanceLogInput, ContestInput, FeedbackInput, RecallInput, SaveInput};
use crate::state::AppState;

/// Parameters for `totem_recall`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecallParams {
    /// The caller's own actor id — the only `actor` scope the resolved chain
    /// can ever contain.
    pub actor: String,
    /// The caller's project membership, if any, as `owner/name`.
    pub project: Option<String>,
    /// The caller's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<String>,
    /// Free text to rank by vector proximity. Omit to skip vector ranking and
    /// return the chain's most recent records instead.
    pub query: Option<String>,
    /// Restrict to these categories (`episodic`, `identity`, `knowledge`,
    /// `context`, `instructions`, `uncertainty`). Empty means every category.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Only records written strictly after this RFC 3339 instant.
    pub since: Option<String>,
    /// Cap the merged result set.
    pub limit: Option<usize>,
    /// Which harness this call arrived through, e.g. `"claude_code"`, or
    /// `{"other": "some-name"}` for a harness Totem does not know by name.
    pub harness: JsonValue,
    /// The harness session this call belongs to.
    pub session: String,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Parameters for `totem_save`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveParams {
    /// The writer's own project membership, if any.
    pub project: Option<String>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<String>,
    /// The category the new record belongs to.
    pub category: String,
    /// Where the record is written, e.g. `"actor:ada"`, `"project:owner/name"`,
    /// `"team:id"`, or `"platform"`. Refused if the writer's resolved chain
    /// does not contain it.
    pub scope: String,
    /// The entity or ARRIVE artifact this record concerns, if any:
    /// `{"kind": "component", "id": "gateway"}`.
    pub subject: Option<JsonValue>,
    /// The memory's content.
    pub body: String,
    /// Free-form tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who is writing, e.g. `{"kind": "agent", "actor": "ada"}`.
    pub author: JsonValue,
    /// Which harness this call arrived through.
    pub harness: JsonValue,
    /// The harness session this call belongs to.
    pub session: String,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Parameters for `totem_landscape`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LandscapeParams {
    /// The ARRIVE-enrolled repo id to describe — `registry.yaml`'s
    /// `repo_id` (e.g. `"058-totem"`), **not** the `owner/name` form
    /// `totem_recall`/`totem_save`'s `project` field uses. The two are
    /// different id spaces: `project` names a repo's scope for memory
    /// isolation, while this names the repo as ARRIVE's own governance
    /// registry (`arrive/registry.yaml`) identifies it, which is how the
    /// landscape graph keys every `repo` record (ADV-ARRIVE-SYNC-001).
    pub repo: Option<String>,
}

/// Parameters for `totem_feedback`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FeedbackParams {
    /// The reader's own identity — the target memory must be visible to this
    /// actor's resolved chain.
    pub actor: String,
    /// The reader's project membership, if any, as `owner/name`.
    pub project: Option<String>,
    /// The reader's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<String>,
    /// The memory the signal is about.
    pub memory_id: String,
    /// The signal: `used`, `wrong`, or `stale`.
    pub signal: String,
    /// Which harness this call arrived through.
    pub harness: JsonValue,
    /// The harness session this call belongs to.
    pub session: String,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Parameters for `totem_contest`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContestParams {
    /// The writer's own project membership, if any.
    pub project: Option<String>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<String>,
    /// The memory being contested. Refused if the writer's chain cannot see
    /// it.
    pub memory_id: String,
    /// Where the new Uncertainty record is written, e.g. `"project:owner/name"`.
    pub scope: String,
    /// The conflicting claim, preserved alongside the original rather than
    /// replacing it.
    pub claim: String,
    /// Free-form tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who is filing the contest, e.g. `{"kind": "agent", "actor": "ada"}`.
    pub author: JsonValue,
    /// Which harness this call arrived through.
    pub harness: JsonValue,
    /// The harness session this call belongs to.
    pub session: String,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Parameters for `totem_advance_log`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AdvanceLogParams {
    /// The writer's own project membership, if any.
    pub project: Option<String>,
    /// The writer's team memberships, if any.
    #[serde(default)]
    pub teams: Vec<String>,
    /// The advance the entry concerns (`ADV-<COMPONENT>-<SEQ>`).
    pub advance_id: String,
    /// Where the log entry is written.
    pub scope: String,
    /// The entry itself.
    pub body: String,
    /// Free-form tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who is writing.
    pub author: JsonValue,
    /// Which harness this call arrived through.
    pub harness: JsonValue,
    /// The harness session this call belongs to.
    pub session: String,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
}

/// Parameters for `totem_advance_status`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AdvanceStatusParams {
    /// The advance id to look up (`ADV-<COMPONENT>-<SEQ>`) — globally unique
    /// by ARRIVE convention, so no repo qualifier is needed.
    pub advance_id: String,
}

fn invalid_params(message: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(message.to_string(), None)
}

fn internal_error(message: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(message.to_string(), None)
}

/// Maps a [`GatewayError`] the same way `error.rs` maps it to an HTTP status:
/// a rule the caller can act on (a denied scope, a missing record, a
/// lifecycle refusal) comes back as a client-facing error; an internal detail
/// (a decode failure, a database error string) never does.
fn gateway_error(error: GatewayError) -> ErrorData {
    match &error {
        GatewayError::Store(
            totem_store::StoreError::ScopeDenied { .. }
            | totem_store::StoreError::NotFound(_)
            | totem_store::StoreError::Lifecycle(_),
        ) => invalid_params(error),
        GatewayError::Store(_) => ErrorData::internal_error("internal error", None),
        GatewayError::InvalidRequest(_) => invalid_params(error),
    }
}

fn parse_recall_input(params: RecallParams) -> Result<RecallInput, ErrorData> {
    let actor = ActorId::new(params.actor).map_err(invalid_params)?;
    let project = params
        .project
        .map(RepoId::new)
        .transpose()
        .map_err(invalid_params)?;
    let teams = params
        .teams
        .into_iter()
        .map(TeamId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_params)?;
    let categories = params
        .categories
        .into_iter()
        .map(|category| serde_json::from_value(JsonValue::String(category)))
        .collect::<Result<Vec<MemoryCategory>, _>>()
        .map_err(invalid_params)?;
    let since = params
        .since
        .map(|instant| instant.parse::<DateTime<Utc>>())
        .transpose()
        .map_err(invalid_params)?;
    let harness: Harness = serde_json::from_value(params.harness).map_err(invalid_params)?;
    let session = SessionId::new(params.session).map_err(invalid_params)?;

    Ok(RecallInput {
        actor,
        project,
        teams,
        query: params.query,
        categories,
        since,
        limit: params.limit,
        harness,
        session,
        turn: params.turn,
    })
}

fn parse_save_input(params: SaveParams) -> Result<SaveInput, ErrorData> {
    let project = params
        .project
        .map(RepoId::new)
        .transpose()
        .map_err(invalid_params)?;
    let teams = params
        .teams
        .into_iter()
        .map(TeamId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_params)?;
    let category: MemoryCategory =
        serde_json::from_value(JsonValue::String(params.category)).map_err(invalid_params)?;
    let scope: Scope = params.scope.parse().map_err(invalid_params)?;
    let subject: Option<SubjectRef> = params
        .subject
        .map(serde_json::from_value)
        .transpose()
        .map_err(invalid_params)?;
    let author: Author = serde_json::from_value(params.author).map_err(invalid_params)?;
    let harness: Harness = serde_json::from_value(params.harness).map_err(invalid_params)?;
    let session = SessionId::new(params.session).map_err(invalid_params)?;

    Ok(SaveInput {
        project,
        teams,
        category,
        scope,
        subject,
        body: params.body,
        tags: params.tags,
        author,
        harness,
        session,
        turn: params.turn,
    })
}

fn parse_feedback_input(params: FeedbackParams) -> Result<FeedbackInput, ErrorData> {
    let actor = ActorId::new(params.actor).map_err(invalid_params)?;
    let project = params
        .project
        .map(RepoId::new)
        .transpose()
        .map_err(invalid_params)?;
    let teams = params
        .teams
        .into_iter()
        .map(TeamId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_params)?;
    let memory_id: MemoryId = params.memory_id.parse().map_err(invalid_params)?;
    let signal: FeedbackSignal =
        serde_json::from_value(JsonValue::String(params.signal)).map_err(invalid_params)?;
    let harness: Harness = serde_json::from_value(params.harness).map_err(invalid_params)?;
    let session = SessionId::new(params.session).map_err(invalid_params)?;

    Ok(FeedbackInput {
        actor,
        project,
        teams,
        memory_id,
        signal,
        harness,
        session,
        turn: params.turn,
    })
}

fn parse_contest_input(params: ContestParams) -> Result<ContestInput, ErrorData> {
    let project = params
        .project
        .map(RepoId::new)
        .transpose()
        .map_err(invalid_params)?;
    let teams = params
        .teams
        .into_iter()
        .map(TeamId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_params)?;
    let memory_id: MemoryId = params.memory_id.parse().map_err(invalid_params)?;
    let scope: Scope = params.scope.parse().map_err(invalid_params)?;
    let author: Author = serde_json::from_value(params.author).map_err(invalid_params)?;
    let harness: Harness = serde_json::from_value(params.harness).map_err(invalid_params)?;
    let session = SessionId::new(params.session).map_err(invalid_params)?;

    Ok(ContestInput {
        project,
        teams,
        memory_id,
        scope,
        claim: params.claim,
        tags: params.tags,
        author,
        harness,
        session,
        turn: params.turn,
    })
}

fn parse_advance_log_input(params: AdvanceLogParams) -> Result<AdvanceLogInput, ErrorData> {
    let project = params
        .project
        .map(RepoId::new)
        .transpose()
        .map_err(invalid_params)?;
    let teams = params
        .teams
        .into_iter()
        .map(TeamId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_params)?;
    let scope: Scope = params.scope.parse().map_err(invalid_params)?;
    let author: Author = serde_json::from_value(params.author).map_err(invalid_params)?;
    let harness: Harness = serde_json::from_value(params.harness).map_err(invalid_params)?;
    let session = SessionId::new(params.session).map_err(invalid_params)?;

    Ok(AdvanceLogInput {
        project,
        teams,
        advance_id: params.advance_id,
        scope,
        body: params.body,
        tags: params.tags,
        author,
        harness,
        session,
        turn: params.turn,
    })
}

/// The MCP handler. Wraps the same [`AppState`] the REST router uses, so
/// running both surfaces over one store is a matter of constructing this and
/// [`crate::router`] from the same state, not two separate stacks.
#[derive(Debug, Clone)]
pub struct TotemMcp {
    state: AppState,
}

impl TotemMcp {
    /// Build the MCP handler over the given gateway state.
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tool_router(server_handler)]
impl TotemMcp {
    /// `totem_recall` — merged, scope-resolved context for a query, returned
    /// as a JSON-encoded array of memory records (not `RecallResponse`'s
    /// `{"records": [...]}` object shape — REST wraps the array, this tool
    /// returns it bare).
    #[tool(
        description = "Recall merged, scope-resolved memory for a query: relevant Knowledge, Instructions, Context, and other typed memories the caller's scope chain can see, ranked by vector proximity when `query` is given. Returns a JSON array of memory records."
    )]
    async fn totem_recall(
        &self,
        Parameters(params): Parameters<RecallParams>,
    ) -> Result<String, ErrorData> {
        let input = parse_recall_input(params)?;
        let records = ops::recall(&self.state, input, "mcp:totem_recall")
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&records).map_err(internal_error)
    }

    /// `totem_save` — write a memory with provenance auto-attached, returned
    /// as a JSON-encoded `{"id": "..."}`.
    #[tool(
        description = "Save a new memory with provenance auto-attached from the caller's claimed identity. Refused if the target scope is outside the caller's own scope chain. Returns the new record's id as JSON."
    )]
    async fn totem_save(
        &self,
        Parameters(params): Parameters<SaveParams>,
    ) -> Result<String, ErrorData> {
        let input = parse_save_input(params)?;
        let id = ops::save(&self.state, input, "mcp:totem_save")
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&serde_json::json!({ "id": id })).map_err(internal_error)
    }

    /// `totem_landscape` — the ARRIVE landscape view: systems, components,
    /// and advances for an enrolled repo, in one round trip
    /// (docs/solution-intent.md §2.3, G2). Populated by
    /// `totem-arrive-sync`'s ingestion (ADV-ARRIVE-SYNC-001); a repo that has
    /// never been synced answers with an empty landscape (`repo: null`)
    /// rather than an error, since "not yet enrolled" is a normal state, not
    /// a fault.
    #[tool(
        description = "The ARRIVE landscape for an enrolled repo: systems, components, and advances (planned/in-progress/done), plus each component's current owners. `repo` is the ARRIVE registry id (registry.yaml's `repo_id`, e.g. `058-totem`), not the `owner/name` scope form. A repo that has not been synced yet returns an empty landscape rather than an error."
    )]
    async fn totem_landscape(
        &self,
        Parameters(params): Parameters<LandscapeParams>,
    ) -> Result<String, ErrorData> {
        let Some(repo) = params.repo else {
            return Err(invalid_params(
                "totem_landscape requires `repo`: the ARRIVE registry id (registry.yaml's \
                 `repo_id`, e.g. `058-totem`)",
            ));
        };
        let view = self
            .state
            .store
            .landscape()
            .view(&repo)
            .await
            .map_err(GatewayError::from)
            .map_err(gateway_error)?;
        serde_json::to_string(&view).map_err(internal_error)
    }

    /// `totem_feedback` — an explicit value signal (`used` / `wrong` /
    /// `stale`) about an existing memory: the input side of the value loop
    /// ADV-CORE-002's automatic citation boost and usage reinforcement feed
    /// alongside. Returns the record's economics after the signal applied,
    /// as JSON.
    #[tool(
        description = "Signal explicit feedback about an existing memory: `used` (it held up, raises value_score), `wrong` (it was incorrect, lowers value_score), or `stale` (out of date, resets currency). Refused if the memory is not visible to the caller's scope chain, or is an append-only (episodic) record. Returns the updated record as JSON."
    )]
    async fn totem_feedback(
        &self,
        Parameters(params): Parameters<FeedbackParams>,
    ) -> Result<String, ErrorData> {
        let input = parse_feedback_input(params)?;
        let record = ops::feedback(&self.state, input, "mcp:totem_feedback")
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&record).map_err(internal_error)
    }

    /// `totem_contest` — file an Uncertainty record against an existing
    /// memory instead of overwriting it. Both claims survive: the contested
    /// record is never revised, and the new claim lands as its own record,
    /// linked back to it. Returns the new record's id as JSON.
    #[tool(
        description = "File a contradiction as an Uncertainty record instead of overwriting the memory it disagrees with. The contested memory is left untouched (both claims are preserved); the new record links back to it. Refused if the contested memory is not visible to the caller's scope chain. Returns the new record's id as JSON."
    )]
    async fn totem_contest(
        &self,
        Parameters(params): Parameters<ContestParams>,
    ) -> Result<String, ErrorData> {
        let input = parse_contest_input(params)?;
        let id = ops::contest(&self.state, input, "mcp:totem_contest")
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&serde_json::json!({ "id": id })).map_err(internal_error)
    }

    /// `totem_advance_log` — append a process-attuned log entry about an
    /// advance. Writes to Totem's own mirror/memory only; `/arrive/` files in
    /// the repo stay authoritative. Returns the new record's id as JSON.
    #[tool(
        description = "Append a log entry about an ARRIVE advance to Totem's memory, making a session process-attuned to the advance it is working. This writes to Totem's own mirror only — the advance's own `## Changes Made` in `/arrive/` stays authoritative. Returns the new record's id as JSON."
    )]
    async fn totem_advance_log(
        &self,
        Parameters(params): Parameters<AdvanceLogParams>,
    ) -> Result<String, ErrorData> {
        let input = parse_advance_log_input(params)?;
        let id = ops::advance_log(&self.state, input, "mcp:totem_advance_log")
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&serde_json::json!({ "id": id })).map_err(internal_error)
    }

    /// `totem_advance_status` — one advance's current status, read from the
    /// landscape mirror populated by `totem-arrive-sync`'s ingestion. Returns
    /// `{"advance": null}` rather than an error when the id has never been
    /// synced, the same "not yet enrolled is a normal state" convention
    /// `totem_landscape` already establishes.
    #[tool(
        description = "The current status of one ARRIVE advance (ADV-<COMPONENT>-<SEQ>), read from the landscape mirror: title, status (planned/in_progress/complete/cancelled), and the components it impacts. Returns `{\"advance\": null}` if the id has never been synced, not an error."
    )]
    async fn totem_advance_status(
        &self,
        Parameters(params): Parameters<AdvanceStatusParams>,
    ) -> Result<String, ErrorData> {
        let advance = ops::advance_status(&self.state, &params.advance_id)
            .await
            .map_err(gateway_error)?;
        serde_json::to_string(&serde_json::json!({ "advance": advance })).map_err(internal_error)
    }
}
