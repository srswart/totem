//! Errors surfaced by the `totem` CLI.

use std::path::PathBuf;

/// Why a `totem` command failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CliError {
    /// `path` is not inside a git worktree (`git rev-parse --show-toplevel`
    /// failed), or `git` itself could not be run.
    #[error("{path} is not inside a git repository: {source}")]
    NotAGitRepo {
        /// The path that was checked.
        path: PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },
    /// Reading or writing a file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file that could not be read or written.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The credential file's JSON did not parse, or a credential could not be
    /// serialized.
    #[error("{path}: {source}")]
    CredentialFile {
        /// The credential file involved.
        path: PathBuf,
        /// The underlying (de)serialization failure.
        #[source]
        source: serde_json::Error,
    },
    /// `totem credential revoke` was given an id with no matching credential.
    #[error("no credential with id {0}")]
    CredentialNotFound(String),
    /// `$TOTEM_HOME` was unset and `$HOME` could not be determined.
    #[error("cannot determine the totem home directory: set $TOTEM_HOME or $HOME")]
    NoHomeDir,
    /// Parsing or syncing the repo's `/arrive/` tree failed.
    #[error(transparent)]
    Ingest(#[from] totem_arrive_sync::IngestError),
    /// The store refused an operation.
    #[error(transparent)]
    Store(#[from] totem_store::StoreError),
}
