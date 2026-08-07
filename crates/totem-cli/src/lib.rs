//! `totem-cli`: repo enrollment and actor credential issuance
//! (docs/solution-intent.md §3.3; ADV-CLI-001).

#![warn(missing_docs)]

use std::path::PathBuf;

use thiserror::Error;

pub mod auth;
pub mod credential;
pub mod enroll;
pub mod hook;

/// Why the credential store's home directory could not be determined.
#[derive(Debug, Error)]
pub enum HomeDirError {
    /// Neither `$TOTEM_HOME` nor `$HOME` is set.
    #[error("neither $TOTEM_HOME nor $HOME is set")]
    Unset,
}

/// Where `totem credential` stores issued credentials: `$TOTEM_HOME`,
/// falling back to `$HOME`. Left unresolved rather than defaulted to a
/// made-up path when neither is set.
pub fn home_dir() -> Result<PathBuf, HomeDirError> {
    if let Ok(value) = std::env::var("TOTEM_HOME") {
        return Ok(PathBuf::from(value));
    }
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| HomeDirError::Unset)
}
