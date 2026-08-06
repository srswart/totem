//! Actor credential issuance (docs/solution-intent.md §3.3; ADV-CLI-001).
//!
//! **Scope note, stated plainly:** this issues the *shape* of a
//! least-privilege credential — an opaque token bound to exactly one repo,
//! one scope, and one actor, validated with `totem-core`'s own parsers and
//! stored locally with restrictive permissions — not a verified security
//! boundary. The gateway does not check any token on any request today (every
//! `/save`/`/recall`/`/enroll` call already accepts a caller-asserted
//! identity, unchanged by this advance); server-side verification of a
//! presented credential is `gateway.yaml`'s "least-privilege: tokens bound to
//! repo + scope" invariant, and that enforcement is ADV-GATEWAY-003's job
//! (security-critical, `MODEL: claude-opus-5`), not this one's. Issuing this
//! shape now gives that advance a real credential record to design
//! verification against, rather than inventing one from scratch — see the
//! advance's Risk + Rollback for the residual this leaves open.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use totem_core::{ActorId, IdError, RepoId, Scope, ScopeParseError};
use uuid::Uuid;

/// One issued credential, as stored locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    /// The opaque bearer token.
    pub token: String,
    /// The repo this credential is bound to (`owner/name`).
    pub repo: String,
    /// The single scope this credential is bound to, in wire form (e.g.
    /// `"actor:ada"`, `"project:owner/name"`, `"team:id"`, `"platform"`).
    pub scope: String,
    /// The actor identity this credential authenticates as.
    pub actor: String,
    /// When this credential was issued.
    pub issued_at: DateTime<Utc>,
}

/// Why a credential could not be issued or stored.
#[derive(Debug, Error)]
pub enum CredentialError {
    /// `repo` failed `totem-core`'s `RepoId` validation.
    #[error("invalid repo id {value:?}: {source}")]
    InvalidRepo {
        /// The value that was rejected.
        value: String,
        /// The underlying validation failure.
        #[source]
        source: IdError,
    },
    /// `actor` failed `totem-core`'s `ActorId` validation.
    #[error("invalid actor id {value:?}: {source}")]
    InvalidActor {
        /// The value that was rejected.
        value: String,
        /// The underlying validation failure.
        #[source]
        source: IdError,
    },
    /// `scope` failed `totem-core`'s `Scope` parser.
    #[error("invalid scope {value:?}: {source}")]
    InvalidScope {
        /// The value that was rejected.
        value: String,
        /// The underlying parse failure.
        #[source]
        source: ScopeParseError,
    },
    /// The scope names a repo or actor other than the ones this credential
    /// claims — issuing it would be a broader grant than the caller stated.
    #[error("scope {scope:?} does not belong to repo {repo:?} / actor {actor:?}")]
    ScopeMismatch {
        /// The scope that was requested.
        scope: String,
        /// The repo the credential was requested for.
        repo: String,
        /// The actor the credential was requested for.
        actor: String,
    },
    /// Reading or writing the local credential store failed.
    #[error("{path}: {source}")]
    Io {
        /// The path that could not be read or written.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The credential store file did not contain a valid credential list.
    #[error("{path}: {source}")]
    Decode {
        /// The path that failed to decode.
        path: PathBuf,
        /// The underlying decode failure.
        #[source]
        source: serde_json::Error,
    },
}

/// Issue a credential bound to `repo`, `scope`, and `actor`.
///
/// Refused if `repo`/`actor`/`scope` do not each validate under
/// `totem-core`'s own rules, or if `scope` names a different repo
/// (`project:...`) or a different actor (`actor:...`) than the ones
/// requested — a credential must not claim a broader grant than its own
/// repo/actor fields state.
pub fn issue(repo: &str, scope: &str, actor: &str) -> Result<Credential, CredentialError> {
    let repo_id = RepoId::new(repo).map_err(|source| CredentialError::InvalidRepo {
        value: repo.to_string(),
        source,
    })?;
    let actor_id = ActorId::new(actor).map_err(|source| CredentialError::InvalidActor {
        value: actor.to_string(),
        source,
    })?;
    let parsed_scope: Scope = scope
        .parse()
        .map_err(|source| CredentialError::InvalidScope {
            value: scope.to_string(),
            source,
        })?;

    let mismatch = match &parsed_scope {
        Scope::Project(scoped_repo) => scoped_repo != &repo_id,
        Scope::Actor(scoped_actor) => scoped_actor != &actor_id,
        Scope::Team(_) | Scope::Platform => false,
    };
    if mismatch {
        return Err(CredentialError::ScopeMismatch {
            scope: scope.to_string(),
            repo: repo.to_string(),
            actor: actor.to_string(),
        });
    }

    // Two v4 UUIDs (256 bits of CSPRNG-backed randomness) rather than one:
    // unguessability is the only property this token needs to hold up on its
    // own today, since nothing server-side verifies it yet.
    let token = format!(
        "totem_cred_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );

    Ok(Credential {
        token,
        repo: repo_id.to_string(),
        scope: parsed_scope.to_string(),
        actor: actor_id.to_string(),
        issued_at: Utc::now(),
    })
}

/// The default local credential store path: `<home>/.totem/credentials.json`.
pub fn default_store_path(home: &Path) -> PathBuf {
    home.join(".totem").join("credentials.json")
}

/// Every credential currently stored at `path`. An absent file is an empty
/// list, not an error — no credential has been issued yet is a normal state.
pub fn load(path: &Path) -> Result<Vec<Credential>, CredentialError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| CredentialError::Decode {
            path: path.to_path_buf(),
            source,
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(CredentialError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Append `credential` to the store at `path`, creating it (and its parent
/// directory) if it does not exist yet. Restricted to owner read/write only
/// on unix (`0600`) — the advance's own "credential handling on developer
/// machines (storage, leakage)" risk.
pub fn store(path: &Path, credential: &Credential) -> Result<(), CredentialError> {
    let mut credentials = load(path)?;
    credentials.push(credential.clone());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CredentialError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_string_pretty(&credentials).expect("credentials serialise");
    fs::write(path, json).map_err(|source| CredentialError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            CredentialError::Io {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }

    Ok(())
}
