//! Bearer credentials for cloud agents, and the bounds they carry
//! (docs/solution-intent.md §3.1, §3.3; ADV-GATEWAY-003).
//!
//! `gateway.yaml`'s invariant is "Cloud credentials are least-privilege:
//! tokens bound to repo + scope". ADV-CLI-001 issued the *shape* of such a
//! credential and said plainly that nothing verified it; this module is the
//! verification, and the authorization that follows it.
//!
//! # Where the boundary is, and where it is not
//!
//! Scope isolation itself stays in `totem-store` (ADV-STORE-001) — this module
//! does not filter a single result. What it does is decide *which identity the
//! store resolves a [`ScopeChain`](totem_core::ScopeChain) for*: a caller may
//! assert an actor, a project, and team memberships on every request, and a
//! token-bound caller's assertions must match the credential it presented. A
//! request that names another actor or another repo is refused before it
//! reaches the store, so the store still sees exactly one honest chain and
//! remains the only thing deciding what that chain can read.
//!
//! The one place this module refuses something the store would have allowed is
//! [`TokenGrant::authorize_scope`]: `platform` is in *every* resolved chain by
//! `ScopeChain::resolve`'s own construction, so without a credential check any
//! token could write memories every enrolled actor sees. Least-privilege means
//! only a `platform`-bound credential may do that.
//!
//! # Two callers, one enforcement point
//!
//! [`Caller`] is passed into every [`crate::ops`] function, so a new surface
//! cannot forget to authorize — it cannot call the operation without saying
//! who is calling. [`Caller::Trusted`] is the local, single-user path (stdio
//! MCP, in-process tests): identity is caller-asserted, exactly as it was
//! before this advance. [`Caller::Bound`] is the remote path, and it is the
//! only one the authenticated HTTP application ever constructs.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, RwLock};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use totem_core::{AccessLogEntry, ActorId, RefusalReason, RepoId, Scope, ScopeParseError, TeamId};
use totem_store::CredentialGrantRow;
use uuid::Uuid;

use crate::error::GatewayError;
use crate::state::AppState;

/// A credential's fingerprint: SHA-256 of the token text.
type Fingerprint = [u8; 32];

/// The same fingerprint [`fingerprint`] computes, hex-encoded for the access
/// log (ADV-CORE-006) — never the token text, and never the raw digest bytes
/// either, so a log entry is printable and greppable.
fn fingerprint_hex(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("writing to a String never fails");
            hex
        })
}

/// The bounds one credential carries: exactly one repo, one scope, one actor.
///
/// Deliberately the same three fields `totem_cli::credential::Credential`
/// records at issue time, so a credential issued on a developer machine and
/// one issued by the gateway describe the same grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGrant {
    /// The repo this credential may name.
    pub repo: RepoId,
    /// The scope this credential is bound to — its **ceiling**, not the only
    /// scope it may touch. See [`TokenGrant::authorize_scope`] for exactly
    /// what a given binding reaches; the short version is that a credential
    /// may write its own actor scope and (unless it is `actor:`-bound) its own
    /// repo's project scope, plus the bound scope itself.
    pub scope: Scope,
    /// The actor this credential authenticates as.
    pub actor: ActorId,
    /// When this credential stops being valid. `None` never expires.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Why a request was refused before it reached the store.
///
/// Split by [`AuthError::is_authentication_failure`] into "we do not know who
/// you are" (401) and "we know who you are, and this is outside your grant"
/// (403). Every message names a rule the caller can act on, so all of them are
/// safe to return verbatim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// No `Authorization: Bearer ...` header was presented.
    #[error("no bearer credential was presented")]
    MissingCredential,
    /// The presented credential is not registered with this gateway. Covers a
    /// forged token and a revoked one alike — deliberately, since telling the
    /// two apart tells a caller which guesses were once real.
    #[error("the presented credential is not valid")]
    UnknownCredential,
    /// An OAuth access token failed validation: bad signature, wrong issuer,
    /// wrong audience, or expired (ADV-GATEWAY-013).
    ///
    /// The MCP authorization spec requires 401 for these — a client that
    /// receives 403 believes its *permissions* are wrong and never refreshes
    /// the token. The detail is returned verbatim because a legitimate client
    /// debugging a connector needs it, and it tells an attacker nothing about
    /// this server they could not determine by reading the metadata document.
    #[error("access token rejected: {0}")]
    InvalidToken(String),
    /// The presented credential has expired.
    #[error("the presented credential expired at {0}")]
    Expired(DateTime<Utc>),
    /// The request acts as an actor the credential is not bound to.
    #[error("this credential is bound to actor {bound}, so it cannot act as {requested}")]
    ActorNotBound {
        /// The actor the credential authenticates as.
        bound: ActorId,
        /// The actor the request tried to act as.
        requested: ActorId,
    },
    /// The request names a repo the credential is not bound to.
    #[error("this credential is bound to repo {bound}, so it cannot name repo {requested}")]
    RepoNotBound {
        /// The repo the credential is bound to.
        bound: RepoId,
        /// The repo the request named.
        requested: RepoId,
    },
    /// The request reaches a scope outside the credential's binding.
    #[error("this credential is bound to scope {bound}, so it cannot reach {requested}")]
    ScopeNotBound {
        /// The scope the credential is bound to.
        bound: Scope,
        /// The scope the request tried to reach.
        requested: Scope,
    },
    /// A credential was requested whose scope names a different repo or actor
    /// than its own binding — a broader grant than the caller stated.
    #[error("scope {scope} does not belong to repo {repo} / actor {actor}")]
    IncoherentBinding {
        /// The scope that was requested.
        scope: Scope,
        /// The repo the credential was requested for.
        repo: RepoId,
        /// The actor the credential was requested for.
        actor: ActorId,
    },
    /// A credential was requested with a repo, scope, or actor that does not
    /// validate under `totem-core`'s own rules.
    #[error("invalid credential binding: {0}")]
    InvalidBinding(String),
}

impl AuthError {
    /// Whether this refusal means "we do not know who you are" (401) rather
    /// than "you are outside your grant" (403).
    pub fn is_authentication_failure(&self) -> bool {
        matches!(
            self,
            AuthError::MissingCredential
                | AuthError::UnknownCredential
                | AuthError::Expired(_)
                | AuthError::InvalidToken(_)
        )
    }

    /// The [`RefusalReason`] a refused *request* logs (ADV-CORE-006).
    ///
    /// `None` for [`AuthError::IncoherentBinding`] and
    /// [`AuthError::InvalidBinding`]: both are refused at credential *issue*
    /// time ([`TokenRegistry::issue`]), never while authenticating or
    /// authorizing a request, so no request-refusal call site can produce
    /// them — `totem_cli`/console issuance surfaces have their own error
    /// reporting for that failure, independent of the access log.
    pub fn refusal_reason(&self) -> Option<RefusalReason> {
        match self {
            AuthError::MissingCredential => Some(RefusalReason::MissingCredential),
            // An OAuth token that fails validation is, to this server, a
            // credential it does not accept — the same audit fact as an
            // unknown bearer, so it logs under the same reason rather than
            // widening totem-core's enum for a distinction no auditor needs.
            AuthError::UnknownCredential | AuthError::InvalidToken(_) => {
                Some(RefusalReason::UnknownCredential)
            }
            AuthError::Expired(_) => Some(RefusalReason::Expired),
            AuthError::ActorNotBound { .. } => Some(RefusalReason::ActorNotBound),
            AuthError::RepoNotBound { .. } => Some(RefusalReason::RepoNotBound),
            AuthError::ScopeNotBound { .. } => Some(RefusalReason::ScopeNotBound),
            AuthError::IncoherentBinding { .. } | AuthError::InvalidBinding(_) => None,
        }
    }
}

impl TokenGrant {
    /// Whether this grant reaches beyond its own actor's private scope.
    ///
    /// An `actor:`-bound credential is the narrowest thing Totem issues: it
    /// speaks for one identity and reaches nothing shared. Letting it assert a
    /// project membership would silently widen its resolved chain to every
    /// memory that repo shares.
    fn reaches_shared_scopes(&self) -> bool {
        !matches!(self.scope, Scope::Actor(_))
    }

    /// Refuse an identity the credential does not cover, before a
    /// [`ScopeChain`](totem_core::ScopeChain) is resolved from it.
    ///
    /// `project` and `teams` are memberships the *caller* asserts; a
    /// token-bound caller may only assert the ones its credential was issued
    /// for. `platform` is not checked here because
    /// `ScopeChain::resolve` appends it to every chain — see
    /// [`TokenGrant::authorize_scope`] for the write side of that.
    pub fn authorize_identity(
        &self,
        actor: &ActorId,
        project: Option<&RepoId>,
        teams: &[TeamId],
    ) -> Result<(), AuthError> {
        if actor != &self.actor {
            return Err(AuthError::ActorNotBound {
                bound: self.actor.clone(),
                requested: actor.clone(),
            });
        }

        if let Some(project) = project {
            if project != &self.repo {
                return Err(AuthError::RepoNotBound {
                    bound: self.repo.clone(),
                    requested: project.clone(),
                });
            }
            if !self.reaches_shared_scopes() {
                return Err(AuthError::ScopeNotBound {
                    bound: self.scope.clone(),
                    requested: Scope::Project(project.clone()),
                });
            }
        }

        for team in teams {
            if self.scope != Scope::Team(team.clone()) {
                return Err(AuthError::ScopeNotBound {
                    bound: self.scope.clone(),
                    requested: Scope::Team(team.clone()),
                });
            }
        }

        Ok(())
    }

    /// Refuse a write into a scope the credential does not reach.
    ///
    /// The store already refuses a write outside the writer's resolved chain,
    /// and after [`TokenGrant::authorize_identity`] that chain is the honest
    /// one. This adds the check the chain cannot express: `platform` is in
    /// every chain, so only a `platform`-bound credential may write there.
    ///
    /// # The binding is a ceiling, not an exact match
    ///
    /// Spelled out because it is easy to read the wrong promise into "bound to
    /// repo + scope". A grant bound at scope `S`, repo `R`, actor `A` may
    /// write:
    ///
    /// | bound scope | `actor:A` | `project:R` | `team:T` | `platform` |
    /// |---|---|---|---|---|
    /// | `actor:A`   | yes | no  | no          | no  |
    /// | `project:R` | yes | yes | no          | no  |
    /// | `team:T`    | yes | yes | only `T`    | no  |
    /// | `platform`  | yes | yes | no          | yes |
    ///
    /// Never another actor's scope, never another repo's project scope, never
    /// a team the credential was not issued for.
    ///
    /// The `team:`/`platform` rows are the ones worth pausing on: both reach
    /// `project:R`. That is deliberate, and it is a *de-escalation* rather
    /// than an escalation — `project:R` is strictly narrower than the scope
    /// those credentials already hold, and `R` is their own bound repo either
    /// way. The alternative, exact-match writes, would also stop a
    /// `project:`-bound cloud agent from saving a private `actor:` note, which
    /// is ordinary work, not a privilege.
    ///
    /// The genuinely tighter design separates a read ceiling from a write set,
    /// so an admin could issue "reads the chain, writes only `platform`". That
    /// is a real improvement and a real interface change; it is not this
    /// advance's call to make unilaterally.
    pub fn authorize_scope(&self, scope: &Scope) -> Result<(), AuthError> {
        let permitted = match scope {
            Scope::Actor(actor) => actor == &self.actor,
            Scope::Project(repo) => repo == &self.repo && self.reaches_shared_scopes(),
            Scope::Team(team) => self.scope == Scope::Team(team.clone()),
            Scope::Platform => self.scope == Scope::Platform,
        };

        if permitted {
            Ok(())
        } else {
            Err(AuthError::ScopeNotBound {
                bound: self.scope.clone(),
                requested: scope.clone(),
            })
        }
    }

    /// Refuse a landscape enroll/read naming a different repo than this
    /// credential's binding (ADV-GATEWAY-009).
    ///
    /// `requested` is the landscape entity's own `owner/name` identity —
    /// `totem_store::RepoArtifact::git_repo` / `RepoView::git_repo` — the same
    /// id space [`TokenGrant::repo`] speaks once a caller has resolved it.
    /// Before that resolution existed, `/enroll` and `GET /landscape/:repo`
    /// had no comparable value to bind against at all — ADV-GATEWAY-003's
    /// disclosed residual this advance closes.
    pub fn authorize_repo(&self, requested: &RepoId) -> Result<(), AuthError> {
        if requested == &self.repo {
            Ok(())
        } else {
            Err(AuthError::RepoNotBound {
                bound: self.repo.clone(),
                requested: requested.clone(),
            })
        }
    }
}

/// Who is calling, and how far their word is taken for it.
#[derive(Debug, Clone)]
pub enum Caller {
    /// A local, single-user transport: stdio MCP, the in-process REST router,
    /// integration tests. Identity is caller-asserted — the process boundary
    /// *is* the credential, exactly as it was before ADV-GATEWAY-003.
    ///
    /// Never constructed by [`crate::authenticated_app`].
    Trusted,
    /// A remote caller holding a verified credential, and that credential's
    /// hex fingerprint (ADV-CORE-006) — carried alongside the grant so an
    /// authorization refusal can still identify *which* credential was
    /// refused without ever holding the token text itself.
    Bound(TokenGrant, String),
}

impl Caller {
    /// Refuse an identity this caller may not assert.
    pub fn authorize_identity(
        &self,
        actor: &ActorId,
        project: Option<&RepoId>,
        teams: &[TeamId],
    ) -> Result<(), AuthError> {
        match self {
            Caller::Trusted => Ok(()),
            Caller::Bound(grant, _) => grant.authorize_identity(actor, project, teams),
        }
    }

    /// Refuse a write into a scope this caller was not granted.
    pub fn authorize_scope(&self, scope: &Scope) -> Result<(), AuthError> {
        match self {
            Caller::Trusted => Ok(()),
            Caller::Bound(grant, _) => grant.authorize_scope(scope),
        }
    }

    /// Refuse a landscape enroll/read naming a different repo than this
    /// caller's credential — see [`TokenGrant::authorize_repo`].
    pub fn authorize_repo(&self, requested: &RepoId) -> Result<(), AuthError> {
        match self {
            Caller::Trusted => Ok(()),
            Caller::Bound(grant, _) => grant.authorize_repo(requested),
        }
    }

    /// This caller's credential fingerprint, for a refusal log entry
    /// (ADV-CORE-006). `None` for [`Caller::Trusted`]: there is no credential
    /// to fingerprint on the local, single-user path.
    pub fn fingerprint(&self) -> Option<&str> {
        match self {
            Caller::Trusted => None,
            Caller::Bound(_, fingerprint) => Some(fingerprint),
        }
    }
}

/// Append a refusal entry for `error` and return it unchanged, so a call site
/// can write `return Err(refuse(...).await);` — logging is best-effort and
/// never turns a refusal into a success or a success into a refusal
/// (ADV-CORE-006's stated risk): a failed log write is reported via
/// `eprintln!` (this crate's existing warning convention — see `main.rs` —
/// it has no `tracing` dependency), not propagated.
///
/// `endpoint` should name the route (`/recall`, `mcp:totem_save`) the same
/// way every successful [`crate::ops`] entry does, so a refusal is
/// distinguishable by surface in the same audit query.
pub(crate) async fn log_refusal(
    state: &AppState,
    caller: &Caller,
    error: AuthError,
    endpoint: &str,
) -> GatewayError {
    if let Some(reason) = error.refusal_reason() {
        let mut entry = AccessLogEntry::refused(reason, endpoint, Utc::now());
        if let Some(fingerprint) = caller.fingerprint() {
            entry = entry.with_fingerprint(fingerprint);
        }
        if let Err(log_error) = state.store.access_log().record(&entry).await {
            // Best-effort, per this advance's stated risk: the refusal itself
            // must stand even if the entry describing it cannot be appended.
            // `eprintln!` matches this crate's existing warning convention
            // (main.rs) — a full observability path is a later concern.
            eprintln!(
                "warning: failed to append a refusal to the access log ({endpoint}): {log_error}"
            );
        }
    }
    GatewayError::Auth(error)
}

/// The credentials this gateway will accept, keyed by fingerprint.
///
/// Cloning shares one registry (the map is behind an `Arc`), so the copy on
/// [`crate::AppState`] and the copy an admin surface holds revoke against the
/// same set.
///
/// **Process-local and non-persistent**, deliberately matching the store this
/// gateway currently runs on (`AppState::in_memory`): a restart forgets every
/// credential. Durable credential storage belongs with the durable deployment
/// (ADV-INFRA-001), not ahead of it.
#[derive(Clone, Default)]
pub struct TokenRegistry {
    entries: Arc<RwLock<HashMap<Fingerprint, TokenGrant>>>,
}

/// Fingerprints never render as their input, so the derived `Debug` cannot
/// print credential material — the property `tests/auth.rs` asserts.
impl std::fmt::Debug for TokenRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRegistry")
            .field("credentials", &self.len())
            .finish()
    }
}

/// Parse a hex fingerprint back into registry key bytes.
///
/// Durable rows (ADV-GATEWAY-012) store the hex form — it is what an operator
/// reads in a refusal log entry and types into a revoke command — while the
/// registry keys on the raw digest.
fn fingerprint_from_hex(hex: &str) -> Option<Fingerprint> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(bytes)
}

fn fingerprint(token: &str) -> Fingerprint {
    Sha256::digest(token.as_bytes()).into()
}

impl TokenRegistry {
    /// An empty registry. A gateway with no credentials refuses every remote
    /// request — the intended fail-closed default, not a broken state.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many credentials are currently registered.
    pub fn len(&self) -> usize {
        self.read().len()
    }

    /// Whether no credential is registered, so every remote request is refused.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Issue a credential bound to `repo`, `scope`, and `actor`, returning the
    /// token text — the only moment it exists in plaintext.
    ///
    /// Refused when the three do not validate under `totem-core`'s own rules,
    /// or when `scope` names a different repo or actor than the binding
    /// states. That is the same refusal `totem_cli::credential::issue` makes,
    /// repeated here rather than shared because the two crates are different
    /// components: a gateway that trusted its caller to have checked would
    /// accept an over-scoped grant from any admin surface that forgot to.
    pub fn issue(
        &self,
        repo: &str,
        scope: &str,
        actor: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<String, AuthError> {
        let repo =
            RepoId::new(repo).map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
        let actor =
            ActorId::new(actor).map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
        let scope: Scope = scope
            .parse()
            .map_err(|error: totem_core::ScopeParseError| {
                AuthError::InvalidBinding(error.to_string())
            })?;

        let incoherent = match &scope {
            Scope::Project(scoped) => scoped != &repo,
            Scope::Actor(scoped) => scoped != &actor,
            Scope::Team(_) | Scope::Platform => false,
        };
        if incoherent {
            return Err(AuthError::IncoherentBinding { scope, repo, actor });
        }

        // Two v4 UUIDs (256 bits of CSPRNG-backed randomness), the same shape
        // `totem-cli` issues, so the two sources are indistinguishable to a
        // client and to this registry.
        let token = format!(
            "totem_cred_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );

        self.register(
            &token,
            TokenGrant {
                repo,
                scope,
                actor,
                expires_at,
            },
        );

        Ok(token)
    }

    /// Accept an already-issued credential — a `totem enroll --credential`
    /// token from ADV-CLI-001, or one restored by an operator at boot.
    ///
    /// Only the fingerprint is kept; `token` is not retained.
    pub fn register(&self, token: &str, grant: TokenGrant) {
        self.write().insert(fingerprint(token), grant);
    }

    /// Revoke a credential. Reports whether it was registered at all, so an
    /// admin surface can tell "revoked" from "never existed".
    pub fn revoke(&self, token: &str) -> bool {
        self.write().remove(&fingerprint(token)).is_some()
    }

    /// Load every durable grant into this registry (ADV-GATEWAY-012).
    ///
    /// Called once at start-up. The registry stays the hot path — verification
    /// is a hash and a map lookup, never a database round trip — and the store
    /// is what makes it survive the restart a deploy performs.
    pub fn load_from(&self, rows: Vec<CredentialGrantRow>) -> Result<usize, AuthError> {
        let mut loaded = 0;
        for row in rows {
            let repo = RepoId::new(&row.repo)
                .map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
            let actor = ActorId::new(&row.actor)
                .map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
            let scope: Scope = row
                .scope
                .parse()
                .map_err(|error: ScopeParseError| AuthError::InvalidBinding(error.to_string()))?;
            let Some(key) = fingerprint_from_hex(&row.fingerprint) else {
                return Err(AuthError::InvalidBinding(format!(
                    "stored credential fingerprint is not 64 hex characters: {}",
                    row.fingerprint
                )));
            };
            self.write().insert(
                key,
                TokenGrant {
                    repo,
                    scope,
                    actor,
                    expires_at: row.expires_at,
                },
            );
            loaded += 1;
        }
        Ok(loaded)
    }

    /// The fingerprint a token hashes to, so a caller that must persist a
    /// grant can do so without the registry handing back token text.
    pub fn fingerprint_of(token: &str) -> String {
        fingerprint_hex(token)
    }

    /// Revoke by fingerprint rather than by token text — the durable path,
    /// where the token is long gone and only its hash was ever stored.
    pub fn revoke_fingerprint(&self, hex: &str) -> bool {
        fingerprint_from_hex(hex).is_some_and(|key| self.write().remove(&key).is_some())
    }

    /// The grant behind a presented token, or why it is not acceptable.
    ///
    /// `now` is a parameter rather than read from the clock so expiry is
    /// testable without sleeping.
    pub fn verify(&self, token: &str, now: DateTime<Utc>) -> Result<TokenGrant, AuthError> {
        let grant = self
            .read()
            .get(&fingerprint(token))
            .cloned()
            .ok_or(AuthError::UnknownCredential)?;

        if let Some(expires_at) = grant.expires_at
            && expires_at <= now
        {
            return Err(AuthError::Expired(expires_at));
        }

        Ok(grant)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<Fingerprint, TokenGrant>> {
        // A poisoned lock means some other request panicked mid-update. The
        // map is a plain insert/remove structure with no invariant a panic can
        // half-break, and refusing every credential from then on would be a
        // self-inflicted outage, so the guard is taken anyway.
        self.entries
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<Fingerprint, TokenGrant>> {
        self.entries
            .write()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// Extract `Authorization: Bearer <token>`, case-insensitively on the scheme
/// per RFC 7235 §2.1.
fn bearer(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("Bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

/// Verify the presented credential and attach the resulting [`Caller`] to the
/// request, or refuse it — appending exactly one refusal entry to the access
/// log when it does (ADV-CORE-006), the only store touch a refused request
/// makes.
///
/// Applied by [`crate::authenticated_app`] to the whole application — REST
/// routes and the streamable-HTTP MCP service alike — so there is no route on
/// a remotely-reachable listener that skips it, and no route that logs a
/// refusal on one surface but not the other. Handlers extract
/// `Extension<Caller>`; a handler mounted without this layer finds no
/// extension and fails rather than defaulting to a trusted caller.
pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let oauth_metadata = state.oauth.as_ref().map(|verifier| verifier.metadata_url());
    let path = request.uri().path().to_string();
    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer)
        .map(str::to_owned);

    let Some(token) = presented else {
        let error = log_refusal(
            &state,
            &Caller::Trusted,
            AuthError::MissingCredential,
            &path,
        )
        .await;
        return with_discovery(error.into_response(), oauth_metadata.as_deref());
    };

    // Static bearer first (ADV-GATEWAY-003): it is a hash and a map lookup,
    // and it is what curl, the CLI, Claude Code's own registration and the
    // Claude API connector all present. An OAuth access token is simply not
    // in that registry, so falling through to the resource-server path costs
    // an unregistered token one failed lookup and nothing else.
    let verified = match state.tokens.verify(&token, Utc::now()) {
        Ok(grant) => Ok(grant),
        Err(registry_error) => match state.oauth.as_ref() {
            Some(verifier) => verifier.verify(&token, Utc::now()).await,
            // No authorization server configured: the registry's verdict is
            // the only verdict, and its error is the accurate one.
            None => Err(registry_error),
        },
    };

    match verified {
        Ok(grant) => {
            request
                .extensions_mut()
                .insert(Caller::Bound(grant, fingerprint_hex(&token)));
            // Deeper handlers refuse too (an authorization failure is a 403 or
            // a 401 from `ops`), so discovery is attached on the way out
            // rather than only on this function's own refusals.
            with_discovery(next.run(request).await, oauth_metadata.as_deref())
        }
        Err(error) => {
            // No verified `Caller` exists yet for a failed `verify`, so the
            // refusal is logged with the *presented* token's own fingerprint
            // rather than through a `Caller` — a forged or revoked credential
            // still deserves a correlatable trace, never its plaintext.
            let entry = AccessLogEntry::refused(
                error
                    .refusal_reason()
                    .unwrap_or(RefusalReason::UnknownCredential),
                &path,
                Utc::now(),
            )
            .with_fingerprint(fingerprint_hex(&token));
            if let Err(log_error) = state.store.access_log().record(&entry).await {
                eprintln!(
                    "warning: failed to append a refusal to the access log ({path}): {log_error}"
                );
            }
            with_discovery(
                GatewayError::Auth(error).into_response(),
                oauth_metadata.as_deref(),
            )
        }
    }
}

/// Attach RFC 9728 discovery to a 401 when an authorization server is
/// configured (ADV-GATEWAY-013).
///
/// A bare `WWW-Authenticate: Bearer` tells a spec-compliant client nothing
/// about *how* to obtain a credential; the `resource_metadata` parameter
/// points at the document that does. Non-401 responses are untouched.
fn with_discovery(mut response: Response, metadata_url: Option<&str>) -> Response {
    if response.status() == StatusCode::UNAUTHORIZED
        && let Some(url) = metadata_url
        && let Ok(value) = format!("Bearer resource_metadata=\"{url}\"").parse()
    {
        response.headers_mut().insert(WWW_AUTHENTICATE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str) -> ActorId {
        ActorId::new(id).expect("valid actor id")
    }

    fn repo(id: &str) -> RepoId {
        RepoId::new(id).expect("valid repo id")
    }

    fn team(id: &str) -> TeamId {
        TeamId::new(id).expect("valid team id")
    }

    fn project_grant() -> TokenGrant {
        TokenGrant {
            repo: repo("srswart/totem"),
            scope: Scope::Project(repo("srswart/totem")),
            actor: actor("ada"),
            expires_at: None,
        }
    }

    #[test]
    fn a_grant_covers_its_own_identity_and_refuses_every_other() {
        let grant = project_grant();

        assert!(
            grant
                .authorize_identity(&actor("ada"), Some(&repo("srswart/totem")), &[])
                .is_ok()
        );
        assert_eq!(
            grant.authorize_identity(&actor("grace"), Some(&repo("srswart/totem")), &[]),
            Err(AuthError::ActorNotBound {
                bound: actor("ada"),
                requested: actor("grace"),
            })
        );
        assert_eq!(
            grant.authorize_identity(&actor("ada"), Some(&repo("srswart/other")), &[]),
            Err(AuthError::RepoNotBound {
                bound: repo("srswart/totem"),
                requested: repo("srswart/other"),
            })
        );
        assert!(
            grant
                .authorize_identity(&actor("ada"), Some(&repo("srswart/totem")), &[team("058")])
                .is_err(),
            "a project-bound credential does not carry team membership"
        );
    }

    #[test]
    fn an_actor_bound_grant_reaches_nothing_shared() {
        let grant = TokenGrant {
            repo: repo("srswart/totem"),
            scope: Scope::Actor(actor("ada")),
            actor: actor("ada"),
            expires_at: None,
        };

        assert!(grant.authorize_identity(&actor("ada"), None, &[]).is_ok());
        assert!(
            grant
                .authorize_identity(&actor("ada"), Some(&repo("srswart/totem")), &[])
                .is_err(),
            "even its own repo is a widening it was not issued for"
        );
        assert!(grant.authorize_scope(&Scope::Actor(actor("ada"))).is_ok());
        assert!(
            grant
                .authorize_scope(&Scope::Project(repo("srswart/totem")))
                .is_err()
        );
    }

    #[test]
    fn only_a_platform_grant_writes_the_scope_every_chain_contains() {
        let project = project_grant();
        assert!(project.authorize_scope(&Scope::Platform).is_err());
        assert!(
            project
                .authorize_scope(&Scope::Project(repo("srswart/totem")))
                .is_ok()
        );

        let platform = TokenGrant {
            scope: Scope::Platform,
            ..project_grant()
        };
        assert!(platform.authorize_scope(&Scope::Platform).is_ok());
    }

    #[test]
    fn a_team_grant_covers_exactly_its_own_team() {
        let grant = TokenGrant {
            scope: Scope::Team(team("058-totem")),
            ..project_grant()
        };

        assert!(
            grant
                .authorize_identity(
                    &actor("ada"),
                    Some(&repo("srswart/totem")),
                    &[team("058-totem")]
                )
                .is_ok()
        );
        assert!(
            grant
                .authorize_identity(&actor("ada"), None, &[team("059-other")])
                .is_err()
        );
        assert!(
            grant
                .authorize_scope(&Scope::Team(team("058-totem")))
                .is_ok()
        );
        assert!(
            grant
                .authorize_scope(&Scope::Team(team("059-other")))
                .is_err()
        );
    }

    /// The full write matrix from [`TokenGrant::authorize_scope`]'s doc, as an
    /// executable table rather than prose.
    ///
    /// Pinned deliberately after review asked whether `team:`/`platform`
    /// credentials should reach `project:` scope: the answer is yes, the
    /// binding is a ceiling, and that intent should fail loudly if someone
    /// later narrows or widens it by accident.
    #[test]
    fn the_write_matrix_is_a_ceiling_and_never_crosses_a_binding() {
        let bindings = [
            Scope::Actor(actor("ada")),
            Scope::Project(repo("srswart/totem")),
            Scope::Team(team("058-totem")),
            Scope::Platform,
        ];
        //                       actor:ada  project:own  team:058  platform
        let expected = [
            /* actor:ada   */ [true, false, false, false],
            /* project:own */ [true, true, false, false],
            /* team:058    */ [true, true, true, false],
            /* platform    */ [true, true, false, true],
        ];

        let targets = [
            Scope::Actor(actor("ada")),
            Scope::Project(repo("srswart/totem")),
            Scope::Team(team("058-totem")),
            Scope::Platform,
        ];

        for (bound, row) in bindings.iter().zip(expected) {
            let grant = TokenGrant {
                scope: bound.clone(),
                ..project_grant()
            };
            for (target, allowed) in targets.iter().zip(row) {
                assert_eq!(
                    grant.authorize_scope(target).is_ok(),
                    allowed,
                    "a {bound}-bound credential writing {target}"
                );
            }

            // No binding, however wide, ever crosses to another actor, another
            // repo, or a team it was not issued for.
            for forbidden in [
                Scope::Actor(actor("grace")),
                Scope::Project(repo("srswart/other")),
                Scope::Team(team("059-other")),
            ] {
                assert!(
                    grant.authorize_scope(&forbidden).is_err(),
                    "a {bound}-bound credential must not write {forbidden}"
                );
            }
        }
    }

    #[test]
    fn a_grant_permits_its_own_repo_and_refuses_another() {
        let grant = project_grant();

        assert!(grant.authorize_repo(&repo("srswart/totem")).is_ok());
        assert_eq!(
            grant.authorize_repo(&repo("srswart/other")),
            Err(AuthError::RepoNotBound {
                bound: repo("srswart/totem"),
                requested: repo("srswart/other"),
            })
        );
    }

    #[test]
    fn a_trusted_caller_has_no_repo_binding_to_check() {
        assert!(Caller::Trusted.authorize_repo(&repo("any/repo")).is_ok());
    }

    #[test]
    fn a_trusted_caller_asserts_its_own_identity() {
        assert!(
            Caller::Trusted
                .authorize_identity(&actor("anyone"), Some(&repo("any/repo")), &[team("any")])
                .is_ok()
        );
        assert!(Caller::Trusted.authorize_scope(&Scope::Platform).is_ok());
    }

    #[test]
    fn verification_reports_unknown_revoked_and_expired_credentials() {
        let registry = TokenRegistry::new();
        let now = Utc::now();

        assert_eq!(
            registry.verify("totem_cred_nope", now),
            Err(AuthError::UnknownCredential)
        );

        let token = registry
            .issue("srswart/totem", "project:srswart/totem", "ada", None)
            .expect("a coherent binding issues");
        assert_eq!(registry.verify(&token, now), Ok(project_grant()));

        assert!(registry.revoke(&token));
        assert!(
            !registry.revoke(&token),
            "revoking twice is not a live token"
        );
        assert_eq!(
            registry.verify(&token, now),
            Err(AuthError::UnknownCredential),
            "a revoked credential is indistinguishable from a forged one"
        );

        let expiry = now - chrono::Duration::seconds(1);
        let expired = registry
            .issue(
                "srswart/totem",
                "project:srswart/totem",
                "ada",
                Some(expiry),
            )
            .expect("an already-expired credential still issues");
        assert_eq!(
            registry.verify(&expired, now),
            Err(AuthError::Expired(expiry))
        );
    }

    #[test]
    fn an_over_scoped_binding_is_refused_at_issue_time() {
        let registry = TokenRegistry::new();

        assert!(
            registry
                .issue("srswart/totem", "project:srswart/other", "ada", None)
                .is_err()
        );
        assert!(
            registry
                .issue("srswart/totem", "actor:grace", "ada", None)
                .is_err()
        );
        // `RepoId`/`ActorId` reject empty and untrimmed values only — they do
        // not police an `owner/name` shape — so this is the whole of what
        // "does not validate under totem-core" means for a binding. Isolation
        // does not rest on the shape: it rests on the *equality* of the bound
        // id with the one a request asserts.
        assert!(registry.issue("", "platform", "ada", None).is_err());
        assert!(
            registry
                .issue("srswart/totem", "platform", " ada", None)
                .is_err()
        );
        assert!(
            registry.is_empty(),
            "no refused binding left a credential behind"
        );
    }

    #[test]
    fn a_registry_shares_one_credential_set_across_clones() {
        let registry = TokenRegistry::new();
        let token = registry
            .issue("srswart/totem", "platform", "ada", None)
            .expect("issues");

        let held_elsewhere = registry.clone();
        assert!(held_elsewhere.revoke(&token));
        assert_eq!(
            registry.verify(&token, Utc::now()),
            Err(AuthError::UnknownCredential),
            "revocation must be visible to the copy the gateway is serving from"
        );
    }

    #[test]
    fn the_bearer_scheme_is_matched_case_insensitively_and_nothing_else_is() {
        assert_eq!(bearer("Bearer abc"), Some("abc"));
        assert_eq!(bearer("bearer abc"), Some("abc"));
        assert_eq!(bearer("BEARER abc"), Some("abc"));
        assert_eq!(bearer("Basic abc"), None);
        assert_eq!(bearer("abc"), None);
        assert_eq!(bearer("Bearer "), None);
    }
}
