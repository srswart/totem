//! Resolving the credential the CLI presents to a gateway (ADV-CLI-002).
//!
//! Before this, `enroll.rs` sent no `Authorization` header at all. That
//! worked for as long as the gateway was an unauthenticated loopback
//! composition and failed the moment one was deployed — the third client in
//! this project built against a server that never refused anything.
//!
//! # Resolution order, and why it fails rather than degrades
//!
//! `--token`, then `TOTEM_TOKEN`, then the local store ADV-CLI-001 already
//! writes. A CLI that quietly proceeds unauthenticated when it finds nothing
//! would keep working against a loopback gateway and fail confusingly against
//! the real one, so absence is an error that names the whole order.

use std::path::Path;

use crate::credential::{self, CredentialError};

/// Where a resolved credential came from — reported to the user so a
/// surprising identity is traceable to its source, never the token itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// `--token` on the command line.
    Flag,
    /// The `TOTEM_TOKEN` environment variable.
    Environment,
    /// The local credential store (`~/.totem/credentials.json`).
    Store,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialSource::Flag => write!(f, "--token"),
            CredentialSource::Environment => write!(f, "TOTEM_TOKEN"),
            CredentialSource::Store => write!(f, "the local credential store"),
        }
    }
}

/// A credential and its provenance.
///
/// `Debug` is hand-written: the derived one would print the token, and this
/// value reaches error paths, logs and CI transcripts.
#[derive(Clone)]
pub struct ResolvedCredential {
    /// The bearer token to present.
    pub token: String,
    /// Where it came from.
    pub source: CredentialSource,
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("source", &self.source)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Why no credential could be resolved.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// Nothing supplied a credential for this repo.
    ///
    /// The message names the whole resolution order deliberately: a
    /// first-time user meeting a bare "401" has no way to know what to do,
    /// and this is the moment to tell them.
    #[error(
        "no credential for repo {repo}. Supply one with --token, or set \
         TOTEM_TOKEN, or add one for this repo to {store} (credentials.json). \
         A gateway's bootstrap token works for the first call on a new machine."
    )]
    NoCredential {
        /// The repo a credential was needed for.
        repo: String,
        /// Where the local store was looked for.
        store: String,
    },
    /// The local store exists but could not be read.
    #[error(transparent)]
    Store(#[from] CredentialError),
}

/// Resolve the credential to present for `repo`.
///
/// `flag` and `env` are passed in rather than read here so the precedence is
/// testable without mutating process state.
pub fn resolve_token(
    flag: Option<&str>,
    env: Option<&str>,
    store_path: &Path,
    repo: &str,
) -> Result<ResolvedCredential, ResolveError> {
    if let Some(token) = flag.filter(|value| !value.is_empty()) {
        return Ok(ResolvedCredential {
            token: token.to_string(),
            source: CredentialSource::Flag,
        });
    }
    if let Some(token) = env.filter(|value| !value.is_empty()) {
        return Ok(ResolvedCredential {
            token: token.to_string(),
            source: CredentialSource::Environment,
        });
    }

    // Only a credential bound to this repo will do. Presenting one issued for
    // another repo would earn a confusing 403 from the gateway in place of an
    // actionable local error.
    let stored = credential::load(store_path)?;
    stored
        .into_iter()
        .find(|candidate| candidate.repo == repo)
        .map(|candidate| ResolvedCredential {
            token: candidate.token,
            source: CredentialSource::Store,
        })
        .ok_or_else(|| ResolveError::NoCredential {
            repo: repo.to_string(),
            store: store_path.display().to_string(),
        })
}
