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
    ActorId, Author, Harness, MemoryCategory, RepoId, Scope, SessionId, SubjectRef, TeamId,
};

use crate::error::GatewayError;
use crate::ops::{self, RecallInput, SaveInput};
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
    /// The enrolled repository to describe, as `owner/name`. Accepted but
    /// currently unused — see `totem_landscape`'s own doc comment.
    pub repo: Option<String>,
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
    /// and advances for an enrolled repo (docs/solution-intent.md §2.3).
    ///
    /// **Not yet populated.** `ADV-ARRIVE-SYNC-001` — the advance that
    /// ingests `/arrive/` into the landscape graph — has not landed, and
    /// `totem-store` has no landscape repository yet. This tool is real and
    /// callable (the surface this advance commits to exposing), but always
    /// answers with an empty landscape and an explanatory `note` rather than
    /// fabricating graph data that does not exist.
    #[tool(
        description = "The ARRIVE landscape for an enrolled repo: systems, components, and advances (planned/in-progress/done). Landscape ingestion (ADV-ARRIVE-SYNC-001) has not landed yet, so this always returns an empty landscape with an explanatory note."
    )]
    async fn totem_landscape(
        &self,
        Parameters(params): Parameters<LandscapeParams>,
    ) -> Result<String, ErrorData> {
        let response = serde_json::json!({
            "repo": params.repo,
            "systems": [],
            "components": [],
            "advances": [],
            "note": "landscape ingestion has not run yet (ADV-ARRIVE-SYNC-001 is still \
                     `planned`); totem-store has no landscape repository to query",
        });
        serde_json::to_string(&response).map_err(internal_error)
    }
}
