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
use axum::body::{Body, Bytes};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use totem_core::{MemoryId, PromotionId, RepoId};

use crate::auth::{AuthError, Caller, log_refusal};
use crate::dto::{
    AdvanceLogRequest, AdvanceLogResponse, AdvanceStatusResponse, AuditRequest, AuditTrailResponse,
    ContestRequest, ContestResponse, EnrollRequest, EnrollResponse, FeedbackRequest,
    FeedbackResponse, LandscapeEventsQuery, LandscapeView, PromotionDecisionRequest,
    PromotionDecisionResponse, PromotionQueueRequest, PromotionQueueResponse,
    ProposePromotionRequest, ProposePromotionResponse, ProposedRecordRequest,
    ProposedRecordResponse, RecallRequest, RecallResponse, ResolveUncertaintyRequest,
    ResolveUncertaintyResponse, SaveRequest, SaveResponse, UncertaintyQueueRequest,
    UncertaintyQueueResponse,
};
use crate::error::GatewayError;
use crate::ops::{
    self, AdvanceLogInput, AuditInput, ContestInput, FeedbackInput, LandscapeEventsInput,
    PromotionDecisionInput, ProposePromotionInput, ProposedRecordInput, QueueReadInput,
    RecallInput, ResolveUncertaintyInput, SaveInput,
};
use crate::sse;
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

/// `GET /health`: liveness only, no credential required.
///
/// Deliberately returns a constant. A health endpoint that reports build
/// metadata, store paths, or record counts hands an unauthenticated caller
/// free reconnaissance; platform health checks need none of it. See
/// `crate::unauthenticated_routes` for why this route is outside the auth
/// layer at all.
pub(crate) async fn health() -> &'static str {
    "ok"
}

/// `GET /admin/embedding`: which model wrote the vectors in this store.
///
/// The operator's answer to "is this index in one space?". More than one entry
/// means recall is ranking across geometries, and its ordering is not
/// meaningful — a state that is otherwise invisible, because a mixed index
/// still returns results in a confident order.
pub(crate) async fn embedding_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = state
        .store
        .memories()
        .embedding_models()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let uniform = rows.len() <= 1;
    Ok(Json(serde_json::json!({
        "running": state.embedder.model_name(),
        "rows_by_model": rows
            .iter()
            .map(|(model, count)| serde_json::json!({
                "model": if model.is_empty() { "(unlabelled)" } else { model },
                "rows": count,
            }))
            .collect::<Vec<_>>(),
        "uniform": uniform,
    })))
}

/// `POST /admin/reembed`: rewrite every stale vector into the running model's
/// space (ADV-STORE-008).
///
/// Explicit rather than automatic at start-up: DEP-001 makes this process the
/// store's sole owner so the pass must run here, but doing it at boot would
/// hold the health check open for its whole duration on a single-machine
/// deployment, and would re-run on every restart with nobody deciding it
/// should. **Back up first** — the runbook step precedes this call.
pub(crate) async fn reembed(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state
        .store
        .memories()
        .reembed_all(state.embedder.as_ref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "model": state.embedder.model_name(),
        "examined": summary.examined,
        "reembedded": summary.reembedded,
        "skipped": summary.skipped,
    })))
}

/// `GET /console/config`: what the console needs to start a sign-in
/// (ADV-GATEWAY-010).
///
/// Unauthenticated by necessity — a signed-out browser has no credential and
/// this is what tells it how to get one — and by content: an issuer, a
/// **public** OAuth client id, a redirect URI and a resource identifier.
/// Nothing here is secret; the client *secret* is never sent to a browser,
/// which is the entire reason the console uses PKCE.
///
/// 404 when the deployment has no OAuth configured, so an API-only gateway
/// says "no sign-in here" rather than offering a broken one.
pub(crate) async fn console_config(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (Some(verifier), Some(client_id), Some(redirect_uri)) = (
        state.oauth.as_ref(),
        state.console_client_id.as_ref(),
        state.console_redirect_uri.as_ref(),
    ) else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(serde_json::json!({
        "issuer": verifier.issuer(),
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "resource": verifier.resource(),
    })))
}

/// `POST /console/token`: relay the console's PKCE code exchange
/// (ADV-GATEWAY-010).
///
/// **Why this exists.** AuthKit answers the CORS *preflight* for its token
/// endpoint with `Access-Control-Allow-Origin: *`, but omits that header from
/// the actual response — so a browser completes the request and then refuses
/// to let the page read it. The console cannot exchange its authorization
/// code directly, and no client-side change can fix a missing response
/// header.
///
/// **What it is not.** The gateway does not become an OAuth client: no client
/// secret exists, PKCE is still what proves the exchange, and no token is
/// issued here — this forwards a request to *the configured issuer* and
/// returns its answer. The destination is taken from server configuration,
/// never from the request, so this cannot be pointed at an arbitrary host.
///
/// Unauthenticated by necessity: a caller mid-sign-in has no credential yet,
/// which is the entire point of the exchange. The authorization code is
/// worthless without the verifier held by the tab that started the flow.
pub(crate) async fn console_token(
    State(state): State<AppState>,
    Json(request): Json<crate::dto::ConsoleTokenRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (Some(verifier), Some(client_id), Some(redirect_uri)) = (
        state.oauth.as_ref(),
        state.console_client_id.as_ref(),
        state.console_redirect_uri.as_ref(),
    ) else {
        return Err(StatusCode::NOT_FOUND);
    };

    // Built by hand rather than with reqwest's `form` helper: the gateway
    // enables only the `json` feature, and pulling in `urlencoded` for five
    // known-shaped fields is not worth the dependency surface.
    let encode = |value: &str| {
        percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    let form = format!(
        "grant_type=authorization_code&code={}&code_verifier={}&client_id={}&redirect_uri={}",
        encode(&request.code),
        encode(&request.code_verifier),
        encode(client_id),
        encode(redirect_uri),
    );
    let response = reqwest::Client::new()
        .post(format!(
            "{}/oauth2/token",
            verifier.issuer().trim_end_matches('/')
        ))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status = response.status();
    let body: serde_json::Value = response.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !status.is_success() {
        // The authorization server's own refusal, forwarded without the
        // token-shaped fields a caller might otherwise mistake for success.
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(body))
}

/// `GET /.well-known/oauth-protected-resource`: RFC 9728 metadata.
///
/// Unauthenticated by necessity (MCP-014) and by design: the document names
/// this server and its authorization server, and contains nothing secret. A
/// gateway with no OAuth configured answers 404 — there is no authorization
/// server to point at, and saying so is more honest than an empty document.
pub(crate) async fn protected_resource_metadata(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.oauth.as_ref() {
        Some(verifier) => Ok(Json(verifier.metadata())),
        None => Err(StatusCode::NOT_FOUND),
    }
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
    ops::authorize_repo(&state, &caller, &git_repo, "/enroll").await?;

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
    if let Caller::Bound(grant, _) = &caller {
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
                return Err(log_refusal(
                    &state,
                    &caller,
                    AuthError::RepoNotBound {
                        bound: grant.repo.clone(),
                        requested: arrive_id,
                    },
                    "/enroll",
                )
                .await);
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

/// The path names the ARRIVE registry id, not the credential's own
/// `owner/name` id space — resolve the landscape's own bound identity
/// (falling back to the raw path when the repo has never synced, or its
/// row predates ADV-GATEWAY-009) so a `Caller::Bound` credential has
/// something to check against either way. A `Caller::Trusted` caller's
/// `authorize_repo` never inspects this value. Shared by [`landscape`] and
/// [`landscape_events`] (ADV-CONSOLE-003) so the two cannot silently diverge
/// on which repo a view's binding names.
fn landscape_git_repo(view: &LandscapeView, repo: &str) -> Result<RepoId, GatewayError> {
    let git_repo = view
        .repo
        .as_ref()
        .and_then(|repo_view| repo_view.git_repo.clone())
        .unwrap_or_else(|| repo.to_string());
    RepoId::new(git_repo).map_err(|error| GatewayError::InvalidRequest(error.to_string()))
}

pub(crate) async fn landscape(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(repo): Path<String>,
) -> Result<Json<LandscapeView>, GatewayError> {
    let view = ops::landscape_view(&state, &repo).await?;
    let git_repo = landscape_git_repo(&view, &repo)?;
    ops::authorize_repo(&state, &caller, &git_repo, "/landscape/{repo}").await?;

    Ok(Json(view))
}

/// `GET /landscape/:repo/events` (ADV-CONSOLE-003): the live relay the
/// console subscribes to instead of polling [`landscape`] behind a manual
/// Refresh button.
///
/// Authorizes exactly like [`landscape`] — same pre-authorization read to
/// learn the bound `git_repo`, same [`ops::authorize_repo`] check, so a
/// caller who cannot read a repo's landscape cannot subscribe to its changes
/// either. Once authorized, every view this stream ever emits — the initial
/// one and every one a [`totem_store::LandscapeRepository::watch`] pulse
/// triggers — is a fresh, store-enforced [`ops::landscape_view`] read, and
/// every one is logged via [`ops::log_landscape_read`]: no unlogged access
/// path (`gateway.yaml`'s invariant) just because the read arrived over a
/// stream instead of a request/response round trip.
///
/// The whole relay lives inside the response body's own stream: there is no
/// task spawned to drive it, so a disconnected client (the body stream
/// dropped) tears the subscription down for free, rather than leaking a
/// background task the way a `tokio::spawn`-per-subscriber design would.
pub(crate) async fn landscape_events(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(repo): Path<String>,
    Query(query): Query<LandscapeEventsQuery>,
) -> Result<Response, GatewayError> {
    let endpoint = "/landscape/{repo}/events";
    let input = LandscapeEventsInput {
        actor: query.actor,
        session: query.session,
    };

    let view = ops::landscape_view(&state, &repo).await?;
    let git_repo = landscape_git_repo(&view, &repo)?;
    ops::authorize_repo(&state, &caller, &git_repo, endpoint).await?;
    ops::log_landscape_read(&state, &input, endpoint).await?;

    let mut changes = state.store.landscape().watch().await?;

    let body = async_stream::stream! {
        yield Ok::<Bytes, std::convert::Infallible>(sse::frame("landscape", &view));
        while let Some(pulse) = changes.next().await {
            if pulse.is_err() {
                break;
            }
            match ops::landscape_view(&state, &repo).await {
                Ok(view) => {
                    if ops::log_landscape_read(&state, &input, endpoint).await.is_err() {
                        break;
                    }
                    yield Ok(sse::frame("landscape", &view));
                }
                Err(_) => break,
            }
        }
    };

    Ok((
        [
            (CONTENT_TYPE, "text/event-stream"),
            (CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(body),
    )
        .into_response())
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
