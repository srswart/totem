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
    let _ = (repo, scope, actor);
    unimplemented!("ADV-CLI-001")
}

/// The default local credential store path: `<home>/.totem/credentials.json`.
pub fn default_store_path(home: &Path) -> PathBuf {
    home.join(".totem").join("credentials.json")
}

/// Every credential currently stored at `path`. An absent file is an empty
/// list, not an error — no credential has been issued yet is a normal state.
pub fn load(path: &Path) -> Result<Vec<Credential>, CredentialError> {
    let _ = path;
    unimplemented!("ADV-CLI-001")
}

/// Append `credential` to the store at `path`, creating it (and its parent
/// directory) if it does not exist yet. Restricted to owner read/write only
/// on unix (`0600`) — the advance's own "credential handling on developer
/// machines (storage, leakage)" risk.
pub fn store(path: &Path, credential: &Credential) -> Result<(), CredentialError> {
    let _ = (path, credential);
    unimplemented!("ADV-CLI-001")
}
