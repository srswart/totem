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
//!
//! Each memory handler takes the [`Caller`] the composition attached — the
//! trusted local caller from [`crate::router`], or the credential-bound one
//! [`crate::auth::authenticate`] verified — and hands it to [`ops`], which
//! authorizes before touching the store (ADV-GATEWAY-003). A handler mounted
//! on a composition that attaches neither finds no extension and fails the
//! request, rather than defaulting to a trusted caller.

use axum::Json;
use axum::extract::{Extension, Path, State};
use totem_core::{MemoryId, PromotionId, RepoId};

use crate::auth::{AuthError, Caller};
use crate::dto::{
    AdvanceLogRequest, AdvanceLogResponse, AdvanceStatusResponse, AuditRequest, AuditTrailResponse,
    ContestRequest, ContestResponse, EnrollRequest, EnrollResponse, FeedbackRequest,
    FeedbackResponse, LandscapeView, PromotionDecisionRequest, PromotionDecisionResponse,
    PromotionQueueRequest, PromotionQueueResponse, ProposePromotionRequest,
    ProposePromotionResponse, ProposedRecordRequest, ProposedRecordResponse, RecallRequest,
    RecallResponse, ResolveUncertaintyRequest, ResolveUncertaintyResponse, SaveRequest,
    SaveResponse, UncertaintyQueueRequest, UncertaintyQueueResponse,
};
use crate::error::GatewayError;
use crate::ops::{
    self, AdvanceLogInput, AuditInput, ContestInput, FeedbackInput, PromotionDecisionInput,
    ProposePromotionInput, ProposedRecordInput, QueueReadInput, RecallInput,
    ResolveUncertaintyInput, SaveInput,
};
use crate::state::AppState;

fn parse_memory_id(id: &str) -> Result<MemoryId, GatewayError> {
    id.parse()
        .map_err(|_| GatewayError::InvalidRequest(format!("{id} is not a valid memory id")))
}

fn parse_promotion_id(id: &str) -> Result<PromotionId, GatewayError> {
    id.parse()
        .map_err(|_| GatewayError::InvalidRequest(format!("{id} is not a valid promotion id")))
}

pub(crate) async fn save(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
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

    let id = ops::save(&state, input, &caller, "/save").await?;

    Ok(Json(SaveResponse { id }))
}

pub(crate) async fn recall(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
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

    let records = ops::recall(&state, input, &caller, "/recall").await?;

    Ok(Json(RecallResponse { records }))
}

pub(crate) async fn feedback(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<FeedbackRequest>,
) -> Result<Json<FeedbackResponse>, GatewayError> {
    let input = FeedbackInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        memory_id: request.memory_id,
        signal: request.signal,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let record = ops::feedback(&state, input, &caller, "/feedback").await?;

    Ok(Json(FeedbackResponse { record }))
}

pub(crate) async fn contest(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<ContestRequest>,
) -> Result<Json<ContestResponse>, GatewayError> {
    let input = ContestInput {
        project: request.project,
        teams: request.teams,
        memory_id: request.memory_id,
        scope: request.scope,
        claim: request.claim,
        tags: request.tags,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let id = ops::contest(&state, input, &caller, "/contest").await?;

    Ok(Json(ContestResponse { id }))
}

pub(crate) async fn advance_log(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<AdvanceLogRequest>,
) -> Result<Json<AdvanceLogResponse>, GatewayError> {
    let input = AdvanceLogInput {
        project: request.project,
        teams: request.teams,
        advance_id: request.advance_id,
        scope: request.scope,
        body: request.body,
        tags: request.tags,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let id = ops::advance_log(&state, input, &caller, "/advance/log").await?;

    Ok(Json(AdvanceLogResponse { id }))
}

pub(crate) async fn advance_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdvanceStatusResponse>, GatewayError> {
    let advance = ops::advance_status(&state, &id).await?;

    Ok(Json(AdvanceStatusResponse { advance }))
}

pub(crate) async fn enroll(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>, GatewayError> {
    let git_repo = RepoId::new(request.snapshot.repo.git_repo.clone()).map_err(|error| {
        GatewayError::InvalidRequest(format!("snapshot repo.git_repo: {error}"))
    })?;
    caller.authorize_repo(&git_repo)?;

    // Validated once, up front: `arrive_id` is used both as the store's
    // lookup/write key below and as the auth error's `requested` field, so a
    // malformed value (empty, or padded with whitespace) is refused here
    // rather than silently syncing an ambiguous row or, in the refusal path,
    // being papered over with the caller's own repo (Copilot review, PR #44).
    let arrive_id = RepoId::new(request.snapshot.repo.id.clone())
        .map_err(|error| GatewayError::InvalidRequest(format!("snapshot repo.id: {error}")))?;

    // The check above only proves the *submitted* snapshot names the
    // caller's own repo — `sync` upserts by `snapshot.repo.id` (the ARRIVE
    // id) and unconditionally overwrites `git_repo`, so without this second
    // check a bound credential could take over an ARRIVE id another repo
    // already owns just by asserting its own git_repo in the snapshot. A
    // `Caller::Bound` credential may only enroll an ARRIVE id that has never
    // synced (first claim) or one whose stored git_repo already matches its
    // own binding (an ordinary re-sync); an existing row with no confirmed
    // git_repo yet is refused the same way an unconfirmed landscape read is
    // — `Caller::Trusted` is exempt, matching every other authorize_* check.
    //
    // `landscape().repo(...)` rather than `view(...)`: this only needs the
    // repo row's own `git_repo`, not the full systems/components/advances a
    // landscape view materializes (Copilot review, PR #44).
    if let Caller::Bound(grant) = &caller {
        let existing = state
            .store
            .landscape()
            .repo(&arrive_id.to_string())
            .await
            .map_err(GatewayError::from)?;
        if let Some(existing) = existing {
            let owner = existing
                .git_repo
                .map(RepoId::new)
                .transpose()
                .map_err(|error| GatewayError::InvalidRequest(error.to_string()))?;
            if owner.as_ref() != Some(&grant.repo) {
                return Err(GatewayError::Auth(AuthError::RepoNotBound {
                    bound: grant.repo.clone(),
                    requested: arrive_id,
                }));
            }
        }
    }

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
    Extension(caller): Extension<Caller>,
    Path(repo): Path<String>,
) -> Result<Json<LandscapeView>, GatewayError> {
    let view = state
        .store
        .landscape()
        .view(&repo)
        .await
        .map_err(GatewayError::from)?;

    // The path names the ARRIVE registry id, not the credential's own
    // `owner/name` id space — resolve the landscape's own bound identity
    // (falling back to the raw path when the repo has never synced, or its
    // row predates ADV-GATEWAY-009) so a `Caller::Bound` credential has
    // something to check against either way. A `Caller::Trusted` caller's
    // `authorize_repo` never inspects this value.
    let git_repo = view
        .repo
        .as_ref()
        .and_then(|repo_view| repo_view.git_repo.clone())
        .unwrap_or_else(|| repo.clone());
    let git_repo =
        RepoId::new(git_repo).map_err(|error| GatewayError::InvalidRequest(error.to_string()))?;
    caller.authorize_repo(&git_repo)?;

    Ok(Json(view))
}

pub(crate) async fn propose_promotion(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<ProposePromotionRequest>,
) -> Result<Json<ProposePromotionResponse>, GatewayError> {
    let input = ProposePromotionInput {
        project: request.project,
        teams: request.teams,
        memory_id: request.memory_id,
        to: request.to,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let outcome = ops::propose_promotion(&state, input, &caller, "/promotions").await?;

    Ok(Json(match outcome {
        totem_store::PromotionOutcome::Promoted { proposal, decision } => {
            ProposePromotionResponse::Promoted { proposal, decision }
        }
        totem_store::PromotionOutcome::Pending { proposal } => {
            ProposePromotionResponse::Pending { proposal }
        }
    }))
}

pub(crate) async fn promotion_pending(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<PromotionQueueRequest>,
) -> Result<Json<PromotionQueueResponse>, GatewayError> {
    let input = QueueReadInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let pending = ops::promotion_pending(&state, input, &caller, "/promotions/pending").await?;

    Ok(Json(PromotionQueueResponse { pending }))
}

pub(crate) async fn proposed_record(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
    Json(request): Json<ProposedRecordRequest>,
) -> Result<Json<ProposedRecordResponse>, GatewayError> {
    let input = ProposedRecordInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        proposal: parse_promotion_id(&id)?,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let record = ops::proposed_record(&state, input, &caller, "/promotions/{id}/record").await?;

    Ok(Json(ProposedRecordResponse { record }))
}

pub(crate) async fn approve_promotion(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
    Json(request): Json<PromotionDecisionRequest>,
) -> Result<Json<PromotionDecisionResponse>, GatewayError> {
    let input = PromotionDecisionInput {
        project: request.project,
        teams: request.teams,
        proposal: parse_promotion_id(&id)?,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
        reason: request.reason,
    };

    let decision =
        ops::approve_promotion(&state, input, &caller, "/promotions/{id}/approve").await?;

    Ok(Json(PromotionDecisionResponse { decision }))
}

pub(crate) async fn reject_promotion(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
    Json(request): Json<PromotionDecisionRequest>,
) -> Result<Json<PromotionDecisionResponse>, GatewayError> {
    let input = PromotionDecisionInput {
        project: request.project,
        teams: request.teams,
        proposal: parse_promotion_id(&id)?,
        author: request.author,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
        reason: request.reason,
    };

    let decision = ops::reject_promotion(&state, input, &caller, "/promotions/{id}/reject").await?;

    Ok(Json(PromotionDecisionResponse { decision }))
}

pub(crate) async fn pending_uncertainty(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<UncertaintyQueueRequest>,
) -> Result<Json<UncertaintyQueueResponse>, GatewayError> {
    let input = QueueReadInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let pending = ops::pending_uncertainty(&state, input, &caller, "/uncertainty/pending").await?;

    Ok(Json(UncertaintyQueueResponse { pending }))
}

pub(crate) async fn resolve_uncertainty(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
    Json(request): Json<ResolveUncertaintyRequest>,
) -> Result<Json<ResolveUncertaintyResponse>, GatewayError> {
    let input = ResolveUncertaintyInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        memory_id: parse_memory_id(&id)?,
        decision: request.decision,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let record =
        ops::resolve_uncertainty(&state, input, &caller, "/uncertainty/{id}/resolve").await?;

    Ok(Json(ResolveUncertaintyResponse { record }))
}

pub(crate) async fn audit_trail(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
    Json(request): Json<AuditRequest>,
) -> Result<Json<AuditTrailResponse>, GatewayError> {
    let input = AuditInput {
        actor: request.actor,
        project: request.project,
        teams: request.teams,
        memory_id: parse_memory_id(&id)?,
        harness: request.harness,
        session: request.session,
        turn: request.turn,
    };

    let trail = ops::audit_trail(&state, input, &caller, "/audit/{id}").await?;

    Ok(Json(AuditTrailResponse {
        record: trail.record,
        access_log: trail.access_log,
        curation_history: trail.curation_history,
        promotion_history: trail.promotion_history,
    }))
}
